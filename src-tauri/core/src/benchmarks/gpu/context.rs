//! wgpu device ownership and timing.
//!
//! * Errors that wgpu reports asynchronously (validation, out-of-memory,
//!   device loss) are captured by our own handlers instead of wgpu's
//!   default panicking handler, then surfaced as `GpuUnavailable`.
//! * Every measurement waits for GPU completion (`Device::poll` with a
//!   timeout) before reading the clock; submission time alone is never
//!   reported.
//! * All GPU objects are owned by [`GpuContext`] and released on drop,
//!   including when a run is cancelled.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::errors::{AppError, AppResult};

/// How long to wait for one submission before declaring the GPU stuck.
/// Individual dispatches are kept far below the OS GPU watchdog (~2 s).
const SUBMISSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Which clock produced a GPU measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingMethod {
    /// GPU timestamp queries around the compute pass.
    GpuTimestamp,
    /// CPU monotonic clock from submission until the GPU reported completion.
    WallClockSynchronized,
}

impl TimingMethod {
    pub const fn id(self) -> &'static str {
        match self {
            Self::GpuTimestamp => "gpu_timestamp",
            Self::WallClockSynchronized => "wall_clock_synchronized",
        }
    }
}

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub limits: wgpu::Limits,
    pub timestamps: bool,
    error: Arc<Mutex<Option<String>>>,
    lost: Arc<AtomicBool>,
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    pollster::block_on(f)
}

impl GpuContext {
    pub fn new(adapter: &wgpu::Adapter) -> AppResult<Self> {
        let available = adapter.features();
        let mut features = wgpu::Features::empty();
        if available.contains(wgpu::Features::TIMESTAMP_QUERY) {
            features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        // The adapter's own limits can always be requested; anything more
        // would make `request_device` fail (or panic).
        let limits = adapter.limits();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("benchmark"),
            required_features: features,
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .map_err(|e| AppError::GpuInitializationFailed(e.to_string()))?;

        let error = Arc::new(Mutex::new(None));
        let lost = Arc::new(AtomicBool::new(false));
        {
            let error = Arc::clone(&error);
            device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                let mut slot = error.lock().unwrap_or_else(PoisonError::into_inner);
                slot.get_or_insert_with(|| e.to_string());
            }));
        }
        {
            let lost = Arc::clone(&lost);
            device.set_device_lost_callback(move |_reason, _message| {
                lost.store(true, Ordering::SeqCst);
            });
        }
        Ok(Self {
            device,
            queue,
            limits,
            timestamps: features.contains(wgpu::Features::TIMESTAMP_QUERY),
            error,
            lost,
        })
    }

    /// Fails if the device reported an error or was lost.
    pub fn check(&self) -> AppResult<()> {
        if self.lost.load(Ordering::SeqCst) {
            return Err(AppError::GpuUnavailable(
                "The GPU device was lost during the benchmark (driver reset or removal).".into(),
            ));
        }
        if let Some(e) = self
            .error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            tracing::warn!("GPU error during benchmark: {e}");
            return Err(AppError::GpuUnavailable(
                "The GPU reported an error during the benchmark.".into(),
            ));
        }
        Ok(())
    }

    /// Blocks until `index` has finished executing on the GPU.
    pub fn wait(&self, index: wgpu::SubmissionIndex) -> AppResult<()> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: Some(SUBMISSION_TIMEOUT),
            })
            .map_err(|e| AppError::GpuUnavailable(format!("GPU did not finish its work: {e}")))?;
        self.check()
    }

    pub fn storage_buffer(
        &self,
        label: &str,
        size: u64,
        extra: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE | extra,
            mapped_at_creation: false,
        })
    }

    pub fn uniform_buffer(&self, label: &str, bytes: &[u8]) -> wgpu::Buffer {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, bytes);
        buffer
    }

    pub fn compute_pipeline(&self, label: &str, wgsl: &str) -> wgpu::ComputePipeline {
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            });
        self.device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
    }

    /// Runs `record` inside one compute pass and returns the pass duration.
    /// Uses GPU timestamps when available, otherwise the CPU clock from
    /// submission to completion.
    pub fn time_compute(
        &self,
        timer: Option<&Timer>,
        record: impl FnOnce(&mut wgpu::ComputePass<'_>),
    ) -> AppResult<(f64, TimingMethod)> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("benchmark"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("benchmark"),
                timestamp_writes: timer.map(|t| wgpu::ComputePassTimestampWrites {
                    query_set: &t.queries,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                }),
            });
            record(&mut pass);
        }
        if let Some(t) = timer {
            t.resolve(&mut encoder);
        }
        let started = Instant::now();
        let index = self.queue.submit([encoder.finish()]);
        self.wait(index)?;
        let wall = started.elapsed().as_secs_f64();
        match timer {
            Some(t) => Ok((t.read_seconds(self)?, TimingMethod::GpuTimestamp)),
            None => Ok((wall, TimingMethod::WallClockSynchronized)),
        }
    }

    /// Records arbitrary commands (e.g. buffer copies) and times them with
    /// the CPU clock from submission to completion.
    pub fn time_commands(&self, record: impl FnOnce(&mut wgpu::CommandEncoder)) -> AppResult<f64> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("benchmark"),
            });
        record(&mut encoder);
        let started = Instant::now();
        let index = self.queue.submit([encoder.finish()]);
        self.wait(index)?;
        Ok(started.elapsed().as_secs_f64())
    }
}

/// Two timestamp queries (pass begin/end) plus resolve and read-back buffers.
pub struct Timer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    period_ns: f64,
}

impl Timer {
    pub fn new(ctx: &GpuContext) -> Option<Self> {
        if !ctx.timestamps {
            return None;
        }
        let size = 2 * u64::from(wgpu::QUERY_SIZE);
        Some(Self {
            queries: ctx.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("benchmark timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }),
            resolve: ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            period_ns: f64::from(ctx.queue.get_timestamp_period()),
        })
    }

    fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, self.resolve.size());
    }

    fn read_seconds(&self, ctx: &GpuContext) -> AppResult<f64> {
        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r.is_ok());
        });
        ctx.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(SUBMISSION_TIMEOUT),
            })
            .map_err(|e| AppError::GpuUnavailable(format!("GPU timestamps unavailable: {e}")))?;
        if !rx.recv_timeout(SUBMISSION_TIMEOUT).unwrap_or(false) {
            return Err(AppError::GpuUnavailable(
                "Could not read GPU timestamps.".into(),
            ));
        }
        let ticks = {
            let view = slice
                .get_mapped_range()
                .map_err(|e| AppError::GpuUnavailable(format!("timestamp read failed: {e}")))?;
            let read = |i: usize| -> u64 {
                let mut b = [0u8; 8];
                b.copy_from_slice(&view[i * 8..i * 8 + 8]);
                u64::from_le_bytes(b)
            };
            read(1).saturating_sub(read(0))
        };
        self.readback.unmap();
        Ok(ticks as f64 * self.period_ns / 1e9)
    }
}
