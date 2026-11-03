//! The `wgpu` implementation of the GPU benchmark: enumerates adapters, opens
//! one, and executes the shaders. All GPU calls live in this file.
//!
//! The benchmark thread creates and uses the device; nothing here is shared
//! with the UI. Driver errors and device loss are captured by callbacks and
//! turned into ordinary errors so a misbehaving driver cannot crash the app.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use super::super::BenchmarkError;
use super::super::model::{GpuAdapterInfo, GpuDeviceType, GpuWorkloadId, TimingMethod};
use super::GpuBackend;
use super::adapters;
use super::runner::{GpuError, GpuExecutor, GpuRunConfig};
use super::shaders::{self, WORKGROUP_SIZE};

/// Bytes read back at each end of the memory buffer for validation.
const MEMORY_CHECK_BYTES: u64 = 64 << 10;
/// Cap on iterations per thread and passes per dispatch, whatever the device.
const MAX_ITERATIONS: u32 = 2_000_000;
const MAX_PASSES: u32 = 4_096;

pub struct WgpuBackend {
    instance: OnceLock<wgpu::Instance>,
}

impl WgpuBackend {
    pub fn new() -> Self {
        Self {
            instance: OnceLock::new(),
        }
    }

    /// Created on first use: loading a graphics driver is not free and most
    /// runs of the application never need it.
    fn instance(&self) -> &wgpu::Instance {
        self.instance.get_or_init(|| {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle())
        })
    }

    fn enumerate(&self) -> Vec<(wgpu::Adapter, GpuAdapterInfo)> {
        let raw = pollster::block_on(self.instance().enumerate_adapters(wgpu::Backends::all()));
        let mut ordinals: HashMap<(String, u32, u32), usize> = HashMap::new();
        let mut list: Vec<(wgpu::Adapter, GpuAdapterInfo)> = raw
            .into_iter()
            .map(|adapter| {
                let info = describe(&adapter, &mut ordinals);
                (adapter, info)
            })
            .collect();
        let mut infos: Vec<GpuAdapterInfo> = list.iter().map(|(_, i)| i.clone()).collect();
        adapters::mark_recommended(&mut infos);
        for ((_, slot), marked) in list.iter_mut().zip(infos) {
            *slot = marked;
        }
        list
    }
}

impl Default for WgpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn device_type(t: wgpu::DeviceType) -> GpuDeviceType {
    match t {
        wgpu::DeviceType::DiscreteGpu => GpuDeviceType::Discrete,
        wgpu::DeviceType::IntegratedGpu => GpuDeviceType::Integrated,
        wgpu::DeviceType::VirtualGpu => GpuDeviceType::Virtual,
        wgpu::DeviceType::Cpu => GpuDeviceType::Software,
        wgpu::DeviceType::Other => GpuDeviceType::Other,
    }
}

fn describe(
    adapter: &wgpu::Adapter,
    ordinals: &mut HashMap<(String, u32, u32), usize>,
) -> GpuAdapterInfo {
    let info = adapter.get_info();
    let limits = adapter.limits();
    let backend = format!("{:?}", info.backend).to_lowercase();
    let n = ordinals
        .entry((backend.clone(), info.vendor, info.device))
        .or_insert(0);
    let ordinal = *n;
    *n += 1;
    GpuAdapterInfo {
        id: adapters::make_id(&backend, info.vendor, info.device, ordinal),
        name: info.name,
        vendor_id: info.vendor,
        device_id: info.device,
        device_type: device_type(info.device_type),
        backend,
        driver: info.driver,
        driver_info: info.driver_info,
        supports_timestamps: adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY),
        max_buffer_bytes: limits.max_buffer_size,
        max_storage_binding_bytes: limits.max_storage_buffer_binding_size,
        max_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
        recommended: false,
    }
}

impl GpuBackend for WgpuBackend {
    fn adapters(&self) -> Result<Vec<GpuAdapterInfo>, BenchmarkError> {
        Ok(self.enumerate().into_iter().map(|(_, info)| info).collect())
    }

    fn open(
        &self,
        adapter_id: Option<&str>,
        config_for: &dyn Fn(&GpuAdapterInfo) -> GpuRunConfig,
    ) -> Result<(GpuAdapterInfo, Box<dyn GpuExecutor>, GpuRunConfig), BenchmarkError> {
        let list = self.enumerate();
        let infos: Vec<GpuAdapterInfo> = list.iter().map(|(_, i)| i.clone()).collect();
        let chosen = adapters::select(&infos, adapter_id)?.clone();
        let (adapter, _) = list
            .into_iter()
            .find(|(_, i)| i.id == chosen.id)
            .ok_or_else(|| BenchmarkError::Failed("That graphics adapter is gone.".into()))?;
        let cfg = config_for(&chosen);
        let exec = Executor::new(&adapter, &chosen, &cfg)?;
        Ok((chosen, Box::new(exec), cfg))
    }
}

/// What the callbacks saw: the first fault wins.
#[derive(Default)]
struct Faults(Mutex<Option<GpuError>>);

impl Faults {
    fn record(&self, e: GpuError) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(e);
    }

    fn take(&self) -> Option<GpuError> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct Timestamps {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// Nanoseconds per timestamp tick.
    period_ns: f64,
}

/// The latest dispatch, remembered so its output can be checked.
#[derive(Clone, Copy)]
struct LastRun {
    workload: GpuWorkloadId,
    size: u32,
    /// Memory only: increments already applied to buffer A before this run.
    a_offset: u32,
}

struct Executor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    faults: Arc<Faults>,
    timestamps: Option<Timestamps>,

    threads: u32,
    groups: u32,
    mem_count: u32,
    mem_groups: u32,

    params: wgpu::Buffer,
    int_out: wgpu::Buffer,
    float_out: wgpu::Buffer,
    mem_a: wgpu::Buffer,
    mem_b: wgpu::Buffer,
    staging: wgpu::Buffer,

    int_pipe: wgpu::ComputePipeline,
    float_pipe: wgpu::ComputePipeline,
    mem_pipe: wgpu::ComputePipeline,
    int_bg: wgpu::BindGroup,
    float_bg: wgpu::BindGroup,
    mem_ab: wgpu::BindGroup,
    mem_ba: wgpu::BindGroup,

    /// Increments applied to buffer A so far (it holds `init + a_offset`).
    a_offset: u32,
    last: Option<LastRun>,
}

fn map_poll(e: wgpu::PollError) -> GpuError {
    match e {
        wgpu::PollError::Timeout => GpuError::Timeout,
        other => GpuError::Other(other.to_string()),
    }
}

impl Executor {
    fn new(
        adapter: &wgpu::Adapter,
        info: &GpuAdapterInfo,
        cfg: &GpuRunConfig,
    ) -> Result<Self, BenchmarkError> {
        let want_ts = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("task-manager-benchmark"),
            required_features: if want_ts {
                wgpu::Features::TIMESTAMP_QUERY
            } else {
                wgpu::Features::empty()
            },
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .map_err(|e| {
            BenchmarkError::Failed(format!("The graphics adapter could not be opened: {e}"))
        })?;

        let faults = Arc::new(Faults::default());
        {
            let f = Arc::clone(&faults);
            device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                f.record(match e {
                    wgpu::Error::OutOfMemory { .. } => GpuError::OutOfMemory,
                    other => GpuError::Other(other.to_string()),
                });
            }));
            let f = Arc::clone(&faults);
            device.set_device_lost_callback(move |reason, message| {
                // Dropping the device at the end of a run reports "destroyed".
                if reason != wgpu::DeviceLostReason::Destroyed {
                    f.record(GpuError::DeviceLost(message));
                }
            });
        }

        // Sizes follow the adapter's real limits, never assumptions.
        let threads = cfg.threads.max(WORKGROUP_SIZE);
        let groups = (threads / WORKGROUP_SIZE).min(limits.max_compute_workgroups_per_dimension);
        let threads = groups * WORKGROUP_SIZE;
        let mem_bytes = cfg
            .memory_bytes
            .min(limits.max_storage_buffer_binding_size)
            .min(limits.max_buffer_size)
            / 16
            * 16;
        let mem_count = u32::try_from(mem_bytes / 16)
            .map_err(|_| BenchmarkError::Failed("The memory test buffer is too large.".into()))?;
        let mem_groups = 4_096.min(limits.max_compute_workgroups_per_dimension);

        let buf = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        use wgpu::BufferUsages as U;
        let params = buf("params", 16, U::UNIFORM | U::COPY_DST);
        let int_out = buf(
            "int-out",
            u64::from(threads) * 4,
            // COPY_DST lets a test corrupt the output to prove validation works.
            U::STORAGE | U::COPY_SRC | U::COPY_DST,
        );
        let float_out = buf(
            "float-out",
            u64::from(threads) * 16,
            U::STORAGE | U::COPY_SRC,
        );
        let mem_a = buf("mem-a", mem_bytes, U::STORAGE | U::COPY_DST | U::COPY_SRC);
        let mem_b = buf("mem-b", mem_bytes, U::STORAGE | U::COPY_DST | U::COPY_SRC);
        let staging_size = (u64::from(threads) * 16).max(2 * MEMORY_CHECK_BYTES);
        let staging = buf("staging", staging_size, U::MAP_READ | U::COPY_DST);

        // Deterministic initial contents for the memory test.
        let init: Vec<u8> = (0..mem_count * 4)
            .flat_map(|lane| shaders::memory_init(lane).to_le_bytes())
            .collect();
        queue.write_buffer(&mem_a, 0, &init);

        let pipeline = |label: &str, source: &str| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(source.to_owned())),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let int_pipe = pipeline("integer", shaders::INTEGER_WGSL);
        let float_pipe = pipeline("float", shaders::FLOAT_WGSL);
        let mem_pipe = pipeline("memory", shaders::MEMORY_WGSL);

        let bind = |pipe: &wgpu::ComputePipeline, entries: &[(u32, &wgpu::Buffer)]| {
            let entries: Vec<wgpu::BindGroupEntry> = entries
                .iter()
                .map(|(binding, b)| wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: b.as_entire_binding(),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipe.get_bind_group_layout(0),
                entries: &entries,
            })
        };
        let int_bg = bind(&int_pipe, &[(0, &params), (1, &int_out)]);
        let float_bg = bind(&float_pipe, &[(0, &params), (1, &float_out)]);
        let mem_ab = bind(&mem_pipe, &[(0, &params), (1, &mem_a), (2, &mem_b)]);
        let mem_ba = bind(&mem_pipe, &[(0, &params), (1, &mem_b), (2, &mem_a)]);

        let timestamps = (want_ts && info.supports_timestamps).then(|| Timestamps {
            set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }),
            resolve: buf("ts-resolve", 16, U::QUERY_RESOLVE | U::COPY_SRC),
            readback: buf("ts-readback", 16, U::MAP_READ | U::COPY_DST),
            period_ns: f64::from(queue.get_timestamp_period()),
        });

        let exec = Self {
            device,
            queue,
            faults,
            timestamps,
            threads,
            groups,
            mem_count,
            mem_groups,
            params,
            int_out,
            float_out,
            mem_a,
            mem_b,
            staging,
            int_pipe,
            float_pipe,
            mem_pipe,
            int_bg,
            float_bg,
            mem_ab,
            mem_ba,
            a_offset: 0,
            last: None,
        };
        // Surface setup problems (out of memory, shader rejected) now.
        exec.wait_idle(Duration::from_secs(10))
            .map_err(BenchmarkError::from)?;
        exec.check_faults().map_err(BenchmarkError::from)?;
        Ok(exec)
    }

    fn check_faults(&self) -> Result<(), GpuError> {
        self.faults.take().map_or(Ok(()), Err)
    }

    fn wait_idle(&self, timeout: Duration) -> Result<(), GpuError> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(timeout),
            })
            .map(|_| ())
            .map_err(map_poll)
    }

    /// Copies `ranges` of `src` into the staging buffer, waits, and returns
    /// the bytes in order.
    fn read(&self, src: &wgpu::Buffer, ranges: &[(u64, u64)]) -> Result<Vec<u8>, GpuError> {
        let total: u64 = ranges.iter().map(|r| r.1).sum();
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let mut at = 0;
        for (offset, len) in ranges {
            enc.copy_buffer_to_buffer(src, *offset, &self.staging, at, *len);
            at += len;
        }
        let idx = self.queue.submit([enc.finish()]);
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: Some(Duration::from_secs(10)),
            })
            .map_err(map_poll)?;
        map_and_copy(&self.device, &self.staging, total)
    }

    fn dispatch_params(&self, workload: GpuWorkloadId, size: u32) -> [u8; 16] {
        let words: [u32; 4] = match workload {
            GpuWorkloadId::Integer | GpuWorkloadId::FloatingPoint => {
                [size, shaders::INT_SEED, self.threads, 0]
            }
            GpuWorkloadId::Memory => [0, 0, self.mem_count, 0],
        };
        let mut bytes = [0u8; 16];
        for (chunk, w) in bytes.chunks_exact_mut(4).zip(words) {
            chunk.copy_from_slice(&w.to_le_bytes());
        }
        bytes
    }
}

/// Maps `staging`, copies its first `len` bytes out and unmaps it.
fn map_and_copy(
    device: &wgpu::Device,
    staging: &wgpu::Buffer,
    len: u64,
) -> Result<Vec<u8>, GpuError> {
    let slot: Arc<Mutex<Option<Result<(), wgpu::BufferAsyncError>>>> = Arc::new(Mutex::new(None));
    let s = Arc::clone(&slot);
    staging
        .slice(..len)
        .map_async(wgpu::MapMode::Read, move |r| {
            *s.lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .map_err(map_poll)?;
    let outcome = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
    match outcome {
        Some(Ok(())) => {}
        Some(Err(e)) => return Err(GpuError::Other(format!("reading results failed: {e}"))),
        None => return Err(GpuError::Timeout),
    }
    let bytes = {
        // The view must be gone before the buffer is unmapped.
        let view = staging
            .slice(..len)
            .get_mapped_range()
            .map_err(|e| GpuError::Other(format!("reading results failed: {e}")))?;
        view.to_vec()
    };
    staging.unmap();
    Ok(bytes)
}

fn le_u32(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Thread indices to check: spread over the whole range, and few enough that
/// the CPU reference stays cheap for long chains.
fn sample_indices(threads: u32, iterations: u32) -> Vec<u32> {
    let budget = 20_000_000u64 / u64::from(iterations.max(1));
    let n = budget.clamp(16, 2_048).min(u64::from(threads)) as u32;
    (0..n)
        .map(|i| (u64::from(i) * u64::from(threads) / u64::from(n)) as u32)
        .collect()
}

impl GpuExecutor for Executor {
    fn timing_method(&self) -> TimingMethod {
        if self.timestamps.is_some() {
            TimingMethod::GpuTimestamps
        } else {
            TimingMethod::WallClock
        }
    }

    fn work(&self, workload: GpuWorkloadId, size: u32) -> f64 {
        let size = f64::from(size);
        match workload {
            GpuWorkloadId::Integer => f64::from(self.threads) * size * shaders::INT_OPS_PER_ITER,
            GpuWorkloadId::FloatingPoint => {
                f64::from(self.threads) * size * shaders::FLOAT_OPS_PER_ITER
            }
            GpuWorkloadId::Memory => {
                // Passes x elements x (bytes read + bytes written per element).
                size * f64::from(self.mem_count) * shaders::MEMORY_BYTES_PER_ELEMENT
            }
        }
    }

    fn max_size(&self, workload: GpuWorkloadId) -> u32 {
        match workload {
            GpuWorkloadId::Memory => MAX_PASSES,
            _ => MAX_ITERATIONS,
        }
    }

    fn run(
        &mut self,
        workload: GpuWorkloadId,
        size: u32,
        timeout: Duration,
    ) -> Result<f64, GpuError> {
        self.check_faults()?;
        self.queue
            .write_buffer(&self.params, 0, &self.dispatch_params(workload, size));

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let writes = self
                .timestamps
                .as_ref()
                .map(|t| wgpu::ComputePassTimestampWrites {
                    query_set: &t.set,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: writes,
            });
            match workload {
                GpuWorkloadId::Integer => {
                    pass.set_pipeline(&self.int_pipe);
                    pass.set_bind_group(0, &self.int_bg, &[]);
                    pass.dispatch_workgroups(self.groups, 1, 1);
                }
                GpuWorkloadId::FloatingPoint => {
                    pass.set_pipeline(&self.float_pipe);
                    pass.set_bind_group(0, &self.float_bg, &[]);
                    pass.dispatch_workgroups(self.groups, 1, 1);
                }
                GpuWorkloadId::Memory => {
                    pass.set_pipeline(&self.mem_pipe);
                    for k in 0..size {
                        let bg = if k % 2 == 0 {
                            &self.mem_ab
                        } else {
                            &self.mem_ba
                        };
                        pass.set_bind_group(0, bg, &[]);
                        pass.dispatch_workgroups(self.mem_groups, 1, 1);
                    }
                }
            }
        }
        if let Some(t) = &self.timestamps {
            enc.resolve_query_set(&t.set, 0..2, &t.resolve, 0);
            enc.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, 16);
        }

        let started = Instant::now();
        let idx = self.queue.submit([enc.finish()]);
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: Some(timeout),
            })
            .map_err(map_poll)?;
        let wall = started.elapsed().as_secs_f64();
        self.check_faults()?;

        let a_before = self.a_offset;
        if workload == GpuWorkloadId::Memory {
            // Pass k alternates A->B, B->A: A ends up `size` (even) or
            // `size - 1` (odd) increments ahead of where it started.
            self.a_offset = self.a_offset.wrapping_add(size - (size & 1));
        }
        self.last = Some(LastRun {
            workload,
            size,
            a_offset: a_before,
        });

        match &self.timestamps {
            Some(t) => {
                let bytes = map_and_copy(&self.device, &t.readback, 16)?;
                let (start, end) = (
                    u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8])),
                    u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0; 8])),
                );
                let ns = end.saturating_sub(start) as f64 * t.period_ns;
                // A zero or absurd span means the counter was unusable;
                // fall back to the wall clock rather than report nonsense.
                Ok(if ns > 0.0 && ns / 1e9 < wall * 4.0 {
                    ns / 1e9
                } else {
                    wall
                })
            }
            None => Ok(wall),
        }
    }

    fn validate(&mut self, workload: GpuWorkloadId, size: u32) -> Result<(), GpuError> {
        let last = self
            .last
            .filter(|l| l.workload == workload && l.size == size)
            .ok_or_else(|| GpuError::Other("nothing to validate".into()))?;
        match workload {
            GpuWorkloadId::Integer => {
                let bytes = self.read(&self.int_out, &[(0, u64::from(self.threads) * 4)])?;
                let got = le_u32(&bytes);
                for i in sample_indices(self.threads, size) {
                    let want = shaders::integer_reference(i, shaders::INT_SEED, size);
                    if got.get(i as usize) != Some(&want) {
                        return Err(GpuError::WrongOutput(format!("integer, thread {i}")));
                    }
                }
            }
            GpuWorkloadId::FloatingPoint => {
                let bytes = self.read(&self.float_out, &[(0, u64::from(self.threads) * 16)])?;
                let got = le_u32(&bytes);
                for i in sample_indices(self.threads, size) {
                    let want = shaders::float_expected(i, size);
                    for (lane, w) in want.iter().enumerate() {
                        let v =
                            f32::from_bits(got.get(i as usize * 4 + lane).copied().unwrap_or(0));
                        if !v.is_finite() || (f64::from(v) - w).abs() > shaders::FLOAT_TOLERANCE {
                            return Err(GpuError::WrongOutput(format!(
                                "floating point, thread {i} lane {lane}"
                            )));
                        }
                    }
                }
            }
            GpuWorkloadId::Memory => {
                let bytes = u64::from(self.mem_count) * 16;
                let check = MEMORY_CHECK_BYTES.min(bytes);
                let result = if size.is_multiple_of(2) {
                    &self.mem_a
                } else {
                    &self.mem_b
                };
                let data = self.read(result, &[(0, check), (bytes - check, check)])?;
                let got = le_u32(&data);
                let lanes_per_end = (check / 4) as usize;
                let total_lanes = u64::from(self.mem_count) * 4;
                let add = last.a_offset.wrapping_add(size);
                for (n, v) in got.iter().enumerate() {
                    let lane = if n < lanes_per_end {
                        n as u64
                    } else {
                        total_lanes - lanes_per_end as u64 + (n - lanes_per_end) as u64
                    } as u32;
                    if *v != shaders::memory_init(lane).wrapping_add(add) {
                        return Err(GpuError::WrongOutput(format!("memory, lane {lane}")));
                    }
                }
            }
        }
        self.check_faults()
    }
}

/// Opt-in tests against a real GPU:
/// `cargo test -p tm-core gpu_hardware -- --ignored --test-threads=1`
#[cfg(test)]
mod hardware_tests {
    use super::*;
    use crate::benchmarks::CancelToken;
    use crate::benchmarks::cpu::runner::Observer;
    use crate::benchmarks::gpu::runner::run_gpu;
    use crate::benchmarks::model::{BenchmarkState, LiveReading};

    struct Quiet;
    impl Observer for Quiet {
        fn stage(&self, _: BenchmarkState, _: &str, _: f32) {}
        fn live(&self, _: Option<LiveReading>) {}
    }

    fn open_executor() -> Executor {
        let backend = WgpuBackend::new();
        let cfg = GpuRunConfig::test();
        let list = backend.enumerate();
        let infos: Vec<GpuAdapterInfo> = list.iter().map(|(_, i)| i.clone()).collect();
        let chosen = adapters::select(&infos, None).expect("an adapter").clone();
        let (adapter, _) = list.into_iter().find(|(_, i)| i.id == chosen.id).unwrap();
        Executor::new(&adapter, &chosen, &cfg).expect("opens")
    }

    #[test]
    #[ignore = "needs a graphics adapter"]
    fn gpu_hardware_runs_all_workloads_and_validates() {
        let backend = WgpuBackend::new();
        let (info, mut exec, cfg) = backend
            .open(None, &|_| GpuRunConfig::test())
            .expect("opens");
        let out = run_gpu(&mut *exec, &cfg, &CancelToken::new(), &Quiet).expect("runs");
        assert_eq!(out.workloads.len(), 3, "{info:?}");
        assert!(out.workloads.iter().all(|w| w.validated && w.median > 0.0));
    }

    #[test]
    #[ignore = "needs a graphics adapter"]
    fn gpu_hardware_validation_catches_corrupted_output() {
        let mut exec = open_executor();
        // Healthy first.
        exec.run(GpuWorkloadId::Integer, 50, Duration::from_secs(5))
            .expect("runs");
        exec.validate(GpuWorkloadId::Integer, 50)
            .expect("healthy output validates");
        // Corrupt the device's result and check it is noticed.
        let zeros = vec![0u8; exec.threads as usize * 4];
        exec.queue.write_buffer(&exec.int_out, 0, &zeros);
        let err = exec
            .validate(GpuWorkloadId::Integer, 50)
            .expect_err("corruption is caught");
        assert!(matches!(err, GpuError::WrongOutput(_)), "{err:?}");
    }

    #[test]
    #[ignore = "needs a graphics adapter"]
    fn gpu_hardware_memory_validation_tracks_repeated_runs() {
        let mut exec = open_executor();
        // Odd and even pass counts, back to back: the expected contents must
        // follow the cumulative increments across runs.
        for passes in [1u32, 2, 3, 4, 3] {
            exec.run(GpuWorkloadId::Memory, passes, Duration::from_secs(5))
                .expect("runs");
            exec.validate(GpuWorkloadId::Memory, passes)
                .unwrap_or_else(|e| panic!("{passes} passes: {e:?}"));
        }
    }

    #[test]
    #[ignore = "needs a graphics adapter"]
    fn gpu_hardware_a_tiny_timeout_is_reported_not_hung() {
        let mut exec = open_executor();
        // A far too large job with a near-zero timeout must return Timeout.
        let r = exec.run(
            GpuWorkloadId::FloatingPoint,
            1_500_000,
            Duration::from_millis(1),
        );
        assert!(matches!(r, Err(GpuError::Timeout)), "{r:?}");
        // And the device is still usable afterwards.
        exec.wait_idle(Duration::from_secs(60)).expect("drains");
        exec.run(GpuWorkloadId::Integer, 10, Duration::from_secs(5))
            .expect("still works");
    }
}
