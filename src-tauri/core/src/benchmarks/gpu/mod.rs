//! GPU benchmark through wgpu (Direct3D 12 / Vulkan / Metal).
//!
//! * **Compute** — an FMA kernel with an exact FLOP count per invocation
//!   (see `compute.wgsl`), reported in GFLOP/s.
//! * **Memory** — buffer-to-buffer copies and a read+write kernel, reported
//!   as bytes read plus bytes written per second (GB/s, 10^9).
//! * **Sustained** (Extended) — repeated compute dispatches in 5 s windows.
//!
//! Sizes respect the adapter's limits and a 256 MiB cap; individual
//! dispatches are calibrated to well below the OS GPU watchdog. A missing
//! adapter or feature degrades to "unsupported" without failing the suite.

pub mod context;

use std::time::{Duration, Instant};

use context::{GpuContext, Timer, TimingMethod};

use super::cpu::{DECLINE_WARN_PERCENT, SUSTAINED_WINDOW, sustained_summary};
use super::models::{
    BenchmarkKind, BenchmarkPhase, BenchmarkProfile, BenchmarkRequest, BenchmarkWarning,
    CategoryResult, CategoryStatus, Detail, GpuAdapterInfo, GpuDeviceType, MetricUnit,
    TimelinePoint, WarningCode,
};
use super::suite::{CategoryBenchmark, RunContext, metric_from_samples, metric_value};
use crate::errors::AppResult;

const COMPUTE_WGSL: &str = include_str!("compute.wgsl");
const READWRITE_WGSL: &str = include_str!("readwrite.wgsl");

const WORKGROUP_SIZE: u64 = 64;
/// FLOPs per compute-kernel iteration per invocation (4 vec4 FMAs).
pub const FLOPS_PER_ITERATION: u64 = 32;
/// Hard cap on any single benchmark buffer.
pub const MAX_BUFFER_BYTES: u64 = 256 * 1024 * 1024;
/// Smallest memory-test buffer that still gives meaningful bandwidth.
pub const MIN_MEMORY_BUFFER_BYTES: u64 = 16 * 1024 * 1024;
/// Compute buffer: 1 Mi vec4 invocations (16 MiB).
const COMPUTE_BUFFER_BYTES: u64 = 16 * 1024 * 1024;
/// Target GPU time of one measured submission.
const TARGET_SUBMISSION: Duration = Duration::from_millis(60);
/// Default sustained duration; deliberately conservative.
pub const DEFAULT_SUSTAINED_SECS: u32 = 60;

pub fn vendor_name(id: u32) -> &'static str {
    match id & 0xFFFF {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x13B5 => "Arm",
        0x5143 => "Qualcomm",
        0x106B => "Apple",
        0x1414 => "Microsoft",
        _ => "Unknown vendor",
    }
}

pub fn device_type(t: wgpu::DeviceType) -> GpuDeviceType {
    match t {
        wgpu::DeviceType::DiscreteGpu => GpuDeviceType::Discrete,
        wgpu::DeviceType::IntegratedGpu => GpuDeviceType::Integrated,
        wgpu::DeviceType::VirtualGpu => GpuDeviceType::Virtual,
        wgpu::DeviceType::Cpu => GpuDeviceType::Software,
        wgpu::DeviceType::Other => GpuDeviceType::Other,
    }
}

pub fn backend_name(b: wgpu::Backend) -> &'static str {
    match b {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Dx12 => "Direct3D 12",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Gl => "OpenGL",
        _ => "Other",
    }
}

fn instance() -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::DX12 | wgpu::Backends::VULKAN | wgpu::Backends::METAL;
    // No debug/validation layers: they would distort timings.
    desc.flags = wgpu::InstanceFlags::empty();
    wgpu::Instance::new(desc)
}

/// Every compute-capable adapter with its stable id.
pub fn enumerate() -> Vec<(wgpu::Adapter, GpuAdapterInfo)> {
    let instance = instance();
    let backends = wgpu::Backends::DX12 | wgpu::Backends::VULKAN | wgpu::Backends::METAL;
    let adapters = pollster::block_on(instance.enumerate_adapters(backends));
    let preferred = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .ok()
    .map(|a| a.get_info());
    let mut seen: Vec<String> = Vec::new();
    let mut preferred_taken = false;
    adapters
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            let backend = info.backend.to_string();
            let key = format!(
                "{}:{:04x}:{:04x}",
                backend.to_lowercase(),
                info.vendor,
                info.device
            );
            let ordinal = seen.iter().filter(|k| **k == key).count();
            seen.push(key.clone());
            let is_preferred = !preferred_taken && preferred.as_ref() == Some(&info);
            preferred_taken |= is_preferred;
            let limits = adapter.limits();
            let summary = GpuAdapterInfo {
                id: format!("{key}:{ordinal}"),
                name: info.name.clone(),
                vendor: vendor_name(info.vendor).to_owned(),
                vendor_id: format!("{:04x}", info.vendor),
                device_id: format!("{:04x}", info.device),
                backend: backend_name(info.backend).to_owned(),
                device_type: device_type(info.device_type),
                driver: info.driver.clone(),
                driver_info: info.driver_info.clone(),
                timestamp_queries: adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY),
                max_storage_buffer_mib: u32::try_from(
                    limits.max_storage_buffer_binding_size / (1024 * 1024),
                )
                .unwrap_or(u32::MAX),
                preferred: is_preferred,
            };
            (adapter, summary)
        })
        .collect()
}

/// Adapter metadata only (for the settings and launch UI).
pub fn list_adapters() -> Vec<GpuAdapterInfo> {
    enumerate().into_iter().map(|(_, info)| info).collect()
}

/// Picks the adapter to benchmark: the requested id if present, else the
/// system's high-performance adapter, else discrete → integrated → other.
/// Software (CPU) adapters are only used when explicitly requested or when
/// nothing else exists. Selection never relies on adapter names.
pub fn select(adapters: &[GpuAdapterInfo], requested: Option<&str>) -> Option<usize> {
    if let Some(id) = requested
        && let Some(i) = adapters.iter().position(|a| a.id == id)
    {
        return Some(i);
    }
    let hardware = |a: &GpuAdapterInfo| a.device_type != GpuDeviceType::Software;
    adapters
        .iter()
        .position(|a| a.preferred && hardware(a))
        .or_else(|| {
            [
                GpuDeviceType::Discrete,
                GpuDeviceType::Integrated,
                GpuDeviceType::Virtual,
                GpuDeviceType::Other,
            ]
            .into_iter()
            .find_map(|t| adapters.iter().position(|a| a.device_type == t))
        })
        .or_else(|| (!adapters.is_empty()).then_some(0))
}

/// Largest usable buffer: within both binding and buffer limits and the
/// global cap, rounded down to a 256-byte multiple.
pub fn buffer_bytes(max_binding: u64, max_buffer: u64, cap: u64) -> u64 {
    let size = max_binding.min(max_buffer).min(cap);
    size - size % 256
}

/// 2-D dispatch for `elements` invocations: `(x, y, row_stride)` with
/// `x` ≤ the per-dimension limit; the shader computes
/// `index = gid.x + gid.y × row_stride`.
pub fn dispatch_dims(elements: u64, max_per_dim: u32) -> (u32, u32, u32) {
    let groups = elements.div_ceil(WORKGROUP_SIZE).max(1);
    let max = u64::from(max_per_dim.max(1));
    let x = groups.min(max);
    let y = groups.div_ceil(x);
    let x32 = u32::try_from(x).unwrap_or(u32::MAX);
    (
        x32,
        u32::try_from(y).unwrap_or(u32::MAX),
        x32.saturating_mul(WORKGROUP_SIZE as u32),
    )
}

fn compute_params(iterations: u32, row_stride: u32) -> [u8; 48] {
    let mut bytes = [0u8; 48];
    let floats = [
        0.999_9_f32,
        0.999_8,
        0.999_7,
        0.999_6,
        0.000_1,
        0.000_2,
        0.000_3,
        0.000_4,
    ];
    for (i, f) in floats.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
    }
    bytes[32..36].copy_from_slice(&iterations.to_le_bytes());
    bytes[36..40].copy_from_slice(&row_stride.to_le_bytes());
    bytes
}

pub fn sustained_secs(request: &BenchmarkRequest) -> u32 {
    request.sustained_secs.unwrap_or(DEFAULT_SUSTAINED_SECS)
}

pub fn estimate_secs(request: &BenchmarkRequest) -> u32 {
    let p = request.profile;
    let warm = p.warmup_ms() as f64 / 1_000.0;
    let per = warm + f64::from(p.iterations()) * TARGET_SUBMISSION.as_secs_f64() * 1.5;
    let mut secs = 3.0 + per * 3.0;
    if p == BenchmarkProfile::Extended {
        secs += f64::from(sustained_secs(request));
    }
    secs.ceil() as u32
}

pub struct GpuBenchmark;

struct Compute {
    pipeline: wgpu::ComputePipeline,
    bind: wgpu::BindGroup,
    params: wgpu::Buffer,
    dims: (u32, u32, u32),
    elements: u64,
    iterations: u32,
}

impl Compute {
    fn new(ctx: &GpuContext) -> AppResult<Self> {
        let size = buffer_bytes(
            ctx.limits.max_storage_buffer_binding_size,
            ctx.limits.max_buffer_size,
            COMPUTE_BUFFER_BYTES,
        );
        let elements = size / 16;
        let dims = dispatch_dims(elements, ctx.limits.max_compute_workgroups_per_dimension);
        let data = ctx.storage_buffer("compute data", size, wgpu::BufferUsages::COPY_DST);
        // Initial values in [0, 1): deterministic, finite.
        let init: Vec<u8> = (0..elements * 4)
            .flat_map(|i| ((i % 1000) as f32 / 1000.0).to_le_bytes())
            .collect();
        ctx.queue.write_buffer(&data, 0, &init);
        let params = ctx.uniform_buffer("compute params", &compute_params(64, dims.2));
        let pipeline = ctx.compute_pipeline("compute", COMPUTE_WGSL);
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: data.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        ctx.check()?;
        Ok(Self {
            pipeline,
            bind,
            params,
            dims,
            elements,
            iterations: 64,
        })
    }

    fn set_iterations(&mut self, ctx: &GpuContext, iterations: u32) {
        self.iterations = iterations;
        ctx.queue
            .write_buffer(&self.params, 0, &compute_params(iterations, self.dims.2));
    }

    fn flops(&self) -> f64 {
        (self.elements * u64::from(self.iterations) * FLOPS_PER_ITERATION) as f64
    }

    /// One timed dispatch: (seconds, method).
    fn run(&self, ctx: &GpuContext, timer: Option<&Timer>) -> AppResult<(f64, TimingMethod)> {
        ctx.time_compute(timer, |pass| {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.dispatch_workgroups(self.dims.0, self.dims.1, 1);
        })
    }
}

struct Memory {
    src: wgpu::Buffer,
    dst: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    bind: wgpu::BindGroup,
    dims: (u32, u32, u32),
    size: u64,
}

impl Memory {
    fn new(ctx: &GpuContext) -> AppResult<Option<Self>> {
        // Two buffers share the cap.
        let size = buffer_bytes(
            ctx.limits.max_storage_buffer_binding_size,
            ctx.limits.max_buffer_size,
            MAX_BUFFER_BYTES / 2,
        );
        if size < MIN_MEMORY_BUFFER_BYTES {
            return Ok(None);
        }
        let usage = wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
        let src = ctx.storage_buffer("memory src", size, usage);
        let dst = ctx.storage_buffer("memory dst", size, usage);
        let dims = dispatch_dims(size / 16, ctx.limits.max_compute_workgroups_per_dimension);
        let mut params = [0u8; 16];
        params[..4].copy_from_slice(&dims.2.to_le_bytes());
        let params = ctx.uniform_buffer("memory params", &params);
        let pipeline = ctx.compute_pipeline("readwrite", READWRITE_WGSL);
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("readwrite"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: src.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dst.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        ctx.check()?;
        Ok(Some(Self {
            src,
            dst,
            pipeline,
            bind,
            dims,
            size,
        }))
    }

    /// `repeats` full-buffer copies in one submission; returns seconds.
    fn copy(&self, ctx: &GpuContext, repeats: u32) -> AppResult<f64> {
        ctx.time_commands(|encoder| {
            for i in 0..repeats {
                let (a, b) = if i % 2 == 0 {
                    (&self.src, &self.dst)
                } else {
                    (&self.dst, &self.src)
                };
                encoder.copy_buffer_to_buffer(a, 0, b, 0, self.size);
            }
        })
    }

    fn readwrite(
        &self,
        ctx: &GpuContext,
        timer: Option<&Timer>,
        repeats: u32,
    ) -> AppResult<(f64, TimingMethod)> {
        ctx.time_compute(timer, |pass| {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            for _ in 0..repeats {
                pass.dispatch_workgroups(self.dims.0, self.dims.1, 1);
            }
        })
    }

    /// Bytes read + written by one full pass over the buffer.
    fn bytes_per_pass(&self) -> f64 {
        2.0 * self.size as f64
    }
}

/// Repeats needed for one submission to take about [`TARGET_SUBMISSION`].
fn repeats_for(seconds_per_unit: f64) -> u32 {
    if seconds_per_unit <= 0.0 {
        return 1;
    }
    (TARGET_SUBMISSION.as_secs_f64() / seconds_per_unit)
        .round()
        .clamp(1.0, 256.0) as u32
}

fn label_for(method: TimingMethod) -> &'static str {
    match method {
        TimingMethod::GpuTimestamp => "GPU timestamp queries",
        TimingMethod::WallClockSynchronized => {
            "CPU clock from submission until the GPU reported completion"
        }
    }
}

impl CategoryBenchmark for GpuBenchmark {
    fn kind(&self) -> BenchmarkKind {
        BenchmarkKind::Gpu
    }

    fn run(&mut self, rc: &mut RunContext<'_, '_>) -> AppResult<CategoryResult> {
        rc.reporter
            .phase(BenchmarkPhase::Preparing, "Detecting GPU adapters");
        let adapters = enumerate();
        let infos: Vec<GpuAdapterInfo> = adapters.iter().map(|(_, i)| i.clone()).collect();
        let requested = rc.request.gpu_adapter_id.as_deref();
        let Some(index) = select(&infos, requested) else {
            let mut r = CategoryResult::empty(BenchmarkKind::Gpu, CategoryStatus::Unsupported);
            r.details.push(Detail::new(
                "reason",
                "Status",
                "Unsupported by this GPU/backend: no Direct3D 12, Vulkan or Metal adapter was found.",
            ));
            return Ok(r);
        };
        let (adapter, info) = &adapters[index];
        let mut warnings = Vec::new();
        let mut details = vec![
            Detail::new(
                "hardware",
                "GPU",
                format!("{} ({})", info.name, info.backend),
            ),
            Detail::new(
                "vendor",
                "Vendor",
                format!("{} ({})", info.vendor, info.vendor_id),
            ),
            Detail::new("backend", "Backend", info.backend.clone()),
            Detail::new(
                "deviceType",
                "Device type",
                match info.device_type {
                    GpuDeviceType::Discrete => "Discrete GPU",
                    GpuDeviceType::Integrated => "Integrated GPU",
                    GpuDeviceType::Virtual => "Virtual GPU",
                    GpuDeviceType::Software => "Software renderer (CPU)",
                    GpuDeviceType::Other => "Other",
                },
            ),
        ];
        if !info.driver.is_empty() || !info.driver_info.is_empty() {
            details.push(Detail::new(
                "driver",
                "Driver",
                format!("{} {}", info.driver, info.driver_info)
                    .trim()
                    .to_owned(),
            ));
        }
        if let Some(id) = requested
            && infos[index].id != id
        {
            details.push(Detail::new(
                "selection",
                "Adapter selection",
                "The selected GPU was not found; the default GPU was used instead.",
            ));
        }
        if info.device_type == GpuDeviceType::Software {
            warnings.push(BenchmarkWarning::new(
                WarningCode::SoftwareAdapter,
                "This adapter is a software renderer running on the CPU; results do not reflect \
                 GPU hardware.",
            ));
        }
        if info.device_type == GpuDeviceType::Integrated {
            warnings.push(BenchmarkWarning::new(
                WarningCode::SharedMemoryGpu,
                "Integrated GPU: it shares system memory with the CPU, so memory throughput is \
                 not comparable with discrete graphics cards.",
            ));
        }

        let ctx = GpuContext::new(adapter)?;
        let timer = Timer::new(&ctx);
        let profile = rc.request.profile;
        let iterations = profile.iterations();
        let warmup = Duration::from_millis(profile.warmup_ms());
        let extended = profile == BenchmarkProfile::Extended;
        let total_steps = 3 + usize::from(extended);
        let mut metrics = Vec::new();
        let mut methods: Vec<(String, TimingMethod)> = Vec::new();

        // --- Compute -------------------------------------------------------
        let label = "Compute (FMA)";
        rc.reporter.subtest(label);
        rc.reporter
            .phase(BenchmarkPhase::WarmingUp, format!("Warming up: {label}"));
        let mut compute = Compute::new(&ctx)?;
        // Calibrate the loop count so one dispatch takes ~TARGET_SUBMISSION.
        let (t, _) = compute.run(&ctx, timer.as_ref())?;
        let per_iteration = t / f64::from(compute.iterations);
        let iters = if per_iteration > 0.0 {
            (TARGET_SUBMISSION.as_secs_f64() / per_iteration).clamp(16.0, 16_384.0) as u32
        } else {
            1_024
        };
        compute.set_iterations(&ctx, iters);
        let warm_until = Instant::now() + warmup;
        while Instant::now() < warm_until {
            rc.cancel.check()?;
            compute.run(&ctx, timer.as_ref())?;
        }
        rc.reporter.phase(BenchmarkPhase::Running, label);
        let mut samples = Vec::new();
        let mut method = TimingMethod::WallClockSynchronized;
        for i in 1..=iterations {
            rc.cancel.check()?;
            let (secs, m) = compute.run(&ctx, timer.as_ref())?;
            method = m;
            let gflops = compute.flops() / secs.max(1e-9) / 1e9;
            samples.push(gflops);
            rc.reporter.iteration(i, iterations);
            rc.reporter.measurement(label, gflops, MetricUnit::Gflops);
            rc.reporter
                .live("gpu.compute.fma", gflops, MetricUnit::Gflops);
            rc.reporter
                .progress(i as f32 / iterations as f32 / total_steps as f32, label);
        }
        let mut m =
            metric_from_samples("gpu.compute.fma", label, MetricUnit::Gflops, true, samples);
        m.note = Some(format!(
            "Measured synthetic throughput: {} invocations × {} iterations × {FLOPS_PER_ITERATION} \
             FLOPs (an FMA counts as 2). Not a manufacturer peak figure.",
            compute.elements, compute.iterations
        ));
        metrics.push(m);
        methods.push((label.into(), method));

        // --- Memory --------------------------------------------------------
        match Memory::new(&ctx)? {
            None => {
                details.push(Detail::new(
                    "memoryTest",
                    "Memory test",
                    "Unsupported by this GPU/backend: storage buffers are too small.",
                ));
            }
            Some(memory) => {
                details.push(Detail::new(
                    "memoryBuffer",
                    "Memory test buffer",
                    format!("2 × {} MiB", memory.size / (1024 * 1024)),
                ));
                // Copy
                let label = "Memory copy";
                rc.reporter.subtest(label);
                let probe = memory.copy(&ctx, 1)?;
                let repeats = repeats_for(probe);
                for _ in 0..2 {
                    rc.cancel.check()?;
                    memory.copy(&ctx, repeats)?;
                }
                let mut samples = Vec::new();
                for i in 1..=iterations {
                    rc.cancel.check()?;
                    let secs = memory.copy(&ctx, repeats)?;
                    let gbps = memory.bytes_per_pass() * f64::from(repeats) / secs.max(1e-9) / 1e9;
                    samples.push(gbps);
                    rc.reporter.iteration(i, iterations);
                    rc.reporter
                        .measurement(label, gbps, MetricUnit::GigabytesPerSec);
                    rc.reporter
                        .live("gpu.memory.copy", gbps, MetricUnit::GigabytesPerSec);
                    rc.reporter.progress(
                        (1.0 + i as f32 / iterations as f32) / total_steps as f32,
                        label,
                    );
                }
                let mut m = metric_from_samples(
                    "gpu.memory.copy",
                    "Memory copy (read + write)",
                    MetricUnit::GigabytesPerSec,
                    true,
                    samples,
                );
                m.note = Some("Buffer-to-buffer copies; bytes read plus bytes written.".into());
                metrics.push(m);
                methods.push((label.into(), TimingMethod::WallClockSynchronized));

                // Read/write kernel
                let label = "Memory read/write kernel";
                rc.reporter.subtest(label);
                let (probe, _) = memory.readwrite(&ctx, timer.as_ref(), 1)?;
                let repeats = repeats_for(probe);
                for _ in 0..2 {
                    rc.cancel.check()?;
                    memory.readwrite(&ctx, timer.as_ref(), repeats)?;
                }
                let mut samples = Vec::new();
                let mut method = TimingMethod::WallClockSynchronized;
                for i in 1..=iterations {
                    rc.cancel.check()?;
                    let (secs, m) = memory.readwrite(&ctx, timer.as_ref(), repeats)?;
                    method = m;
                    let gbps = memory.bytes_per_pass() * f64::from(repeats) / secs.max(1e-9) / 1e9;
                    samples.push(gbps);
                    rc.reporter.iteration(i, iterations);
                    rc.reporter
                        .measurement(label, gbps, MetricUnit::GigabytesPerSec);
                    rc.reporter
                        .live("gpu.memory.readwrite", gbps, MetricUnit::GigabytesPerSec);
                    rc.reporter.progress(
                        (2.0 + i as f32 / iterations as f32) / total_steps as f32,
                        label,
                    );
                }
                metrics.push(metric_from_samples(
                    "gpu.memory.readwrite",
                    "Memory read/write kernel",
                    MetricUnit::GigabytesPerSec,
                    true,
                    samples,
                ));
                methods.push((label.into(), method));
            }
        }

        // --- Sustained (Extended) -----------------------------------------
        let mut timeline = Vec::new();
        if extended {
            let label = "Sustained compute";
            rc.reporter.subtest(label);
            rc.reporter.phase(BenchmarkPhase::Running, label);
            let secs = sustained_secs(rc.request);
            let windows = (secs / SUSTAINED_WINDOW.as_secs() as u32).max(3);
            let mut values = Vec::new();
            let started = Instant::now();
            for w in 0..windows {
                let window_start = Instant::now();
                let mut flops = 0.0;
                while window_start.elapsed() < SUSTAINED_WINDOW {
                    rc.cancel.check()?;
                    compute.run(&ctx, None)?;
                    flops += compute.flops();
                }
                let gflops = flops / window_start.elapsed().as_secs_f64() / 1e9;
                values.push(gflops);
                timeline.push(TimelinePoint {
                    t_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(0),
                    series: "gpu.sustained".into(),
                    value: gflops,
                });
                rc.reporter.iteration(w + 1, windows);
                rc.reporter
                    .live("gpu.sustained", gflops, MetricUnit::Gflops);
                rc.reporter.progress(
                    (3.0 + (w + 1) as f32 / windows as f32) / total_steps as f32,
                    label,
                );
            }
            if let Some((peak, sustained, drop)) = sustained_summary(&values) {
                metrics.push(metric_value(
                    "gpu.sustained.peak",
                    "Peak GPU performance",
                    MetricUnit::Gflops,
                    true,
                    peak,
                ));
                metrics.push(metric_value(
                    "gpu.sustained.value",
                    "Sustained GPU performance",
                    MetricUnit::Gflops,
                    true,
                    sustained,
                ));
                metrics.push(metric_value(
                    "gpu.sustained.drop",
                    "Performance drop",
                    MetricUnit::Percent,
                    false,
                    drop,
                ));
                if drop > DECLINE_WARN_PERCENT {
                    warnings.push(BenchmarkWarning::new(
                        WarningCode::PerformanceDecline,
                        format!(
                            "Sustained GPU performance decreased by {drop:.1}% during the test. \
                             Thermal or power limits may be contributing."
                        ),
                    ));
                }
            }
            methods.push((label.into(), TimingMethod::WallClockSynchronized));
        }
        drop(compute);
        ctx.check()?;

        let timing: Vec<String> = methods
            .iter()
            .map(|(l, m)| format!("{l}: {}", label_for(*m)))
            .collect();
        details.push(Detail::new("timingMethod", "Timing", timing.join("; ")));
        details.push(Detail::new(
            "timingMethodIds",
            "Timing method ids",
            methods
                .iter()
                .map(|(_, m)| m.id())
                .collect::<Vec<_>>()
                .join(","),
        ));
        details.push(Detail::new(
            "temperature",
            "GPU temperature",
            "GPU temperature monitoring unavailable on this system.",
        ));

        let mut result = CategoryResult::empty(BenchmarkKind::Gpu, CategoryStatus::Completed);
        result.metrics = metrics;
        result.details = details;
        result.warnings = warnings;
        result.timeline = timeline;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(id: &str, t: GpuDeviceType, preferred: bool) -> GpuAdapterInfo {
        GpuAdapterInfo {
            id: id.into(),
            name: id.into(),
            vendor: "Test".into(),
            vendor_id: "0000".into(),
            device_id: "0000".into(),
            backend: "Vulkan".into(),
            device_type: t,
            driver: String::new(),
            driver_info: String::new(),
            timestamp_queries: false,
            max_storage_buffer_mib: 128,
            preferred,
        }
    }

    #[test]
    fn selection_prefers_request_then_system_preference_then_discrete() {
        let list = vec![
            adapter("sw", GpuDeviceType::Software, false),
            adapter("igpu", GpuDeviceType::Integrated, false),
            adapter("dgpu", GpuDeviceType::Discrete, false),
        ];
        assert_eq!(select(&list, Some("igpu")), Some(1));
        assert_eq!(select(&list, None), Some(2));
        assert_eq!(select(&list, Some("unplugged")), Some(2), "falls back");
        let mut preferred = list.clone();
        preferred[1].preferred = true;
        assert_eq!(select(&preferred, None), Some(1));
    }

    #[test]
    fn software_adapters_only_as_last_resort_or_on_request() {
        let only_sw = vec![adapter("sw", GpuDeviceType::Software, true)];
        assert_eq!(select(&only_sw, None), Some(0));
        let mixed = vec![
            adapter("sw", GpuDeviceType::Software, true),
            adapter("other", GpuDeviceType::Other, false),
        ];
        assert_eq!(select(&mixed, None), Some(1));
        assert_eq!(select(&mixed, Some("sw")), Some(0));
        assert_eq!(select(&[], None), None);
    }

    #[test]
    fn buffer_sizes_respect_limits() {
        let mib = 1024 * 1024;
        assert_eq!(
            buffer_bytes(128 * mib, 256 * mib, MAX_BUFFER_BYTES),
            128 * mib
        );
        assert_eq!(
            buffer_bytes(4 * 1024 * mib, 4 * 1024 * mib, MAX_BUFFER_BYTES),
            MAX_BUFFER_BYTES
        );
        assert_eq!(buffer_bytes(1000, 2000, MAX_BUFFER_BYTES), 768);
    }

    #[test]
    fn dispatch_dims_cover_all_elements_within_limits() {
        let (x, y, stride) = dispatch_dims(1 << 20, 65_535);
        assert_eq!((x, y, stride), (16_384, 1, 16_384 * 64));
        let elements = 10_000_000u64;
        let (x, y, stride) = dispatch_dims(elements, 65_535);
        assert!(x <= 65_535);
        assert!(u64::from(x) * u64::from(y) * 64 >= elements);
        assert_eq!(stride, x * 64);
    }

    #[test]
    fn compute_params_layout_matches_wgsl() {
        let p = compute_params(512, 4096);
        assert_eq!(f32::from_le_bytes(p[0..4].try_into().unwrap()), 0.999_9);
        assert_eq!(u32::from_le_bytes(p[32..36].try_into().unwrap()), 512);
        assert_eq!(u32::from_le_bytes(p[36..40].try_into().unwrap()), 4096);
    }

    #[test]
    fn shaders_declare_compute_entry_points() {
        for src in [COMPUTE_WGSL, READWRITE_WGSL] {
            assert!(src.contains("@compute"));
        }
    }

    #[test]
    fn vendor_ids_map_to_names() {
        assert_eq!(vendor_name(0x10DE), "NVIDIA");
        assert_eq!(vendor_name(0x8086), "Intel");
        assert_eq!(vendor_name(0xDEAD), "Unknown vendor");
    }
}
