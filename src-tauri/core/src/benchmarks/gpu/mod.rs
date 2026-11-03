//! GPU benchmark (`gpu-compute-v1`): the same WGSL shaders on every platform,
//! run through `wgpu` (Vulkan on Linux, DirectX 12 or Vulkan on Windows).

pub mod adapters;
pub mod runner;
pub mod shaders;
pub mod wgpu_backend;

use super::BenchmarkError;
use super::model::{GpuAdapterInfo, GpuDeviceType, TimingMethod};
use runner::{GpuExecutor, GpuRunConfig};

/// A source of graphics adapters and devices. The real one uses `wgpu`;
/// tests substitute a fake so CI needs no GPU.
pub trait GpuBackend: Send + Sync {
    fn adapters(&self) -> Result<Vec<GpuAdapterInfo>, BenchmarkError>;

    /// Opens `adapter_id` (or the recommended adapter) for one run. Called on
    /// the benchmark thread, which then owns the device. `config_for` builds
    /// the run settings once the adapter is known, because the number of
    /// threads depends on the kind of device.
    fn open(
        &self,
        adapter_id: Option<&str>,
        config_for: &dyn Fn(&GpuAdapterInfo) -> GpuRunConfig,
    ) -> Result<(GpuAdapterInfo, Box<dyn GpuExecutor>, GpuRunConfig), BenchmarkError>;
}

/// Things worth knowing about a result, from the adapter and timing method.
pub fn notes(adapter: &GpuAdapterInfo, timing: TimingMethod) -> Vec<String> {
    let mut notes = Vec::new();
    match adapter.device_type {
        GpuDeviceType::Integrated => notes.push(
            "Integrated GPU: the memory test measures shared system memory, not dedicated video memory, so it is not comparable with a discrete GPU's memory bandwidth."
                .into(),
        ),
        GpuDeviceType::Software => notes.push(
            "This adapter is a software renderer that runs on the CPU. The result describes the CPU, not a GPU, and is not scored."
                .into(),
        ),
        _ => {}
    }
    if timing == TimingMethod::WallClock {
        notes.push(
            "This GPU does not offer execution timestamps, so times were measured with the wall clock and include driver overhead."
                .into(),
        );
    }
    notes.push(
        "GPU utilization, clock speed and temperature are not measured during this test.".into(),
    );
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(t: GpuDeviceType) -> GpuAdapterInfo {
        GpuAdapterInfo {
            id: "x".into(),
            name: "x".into(),
            vendor_id: 0,
            device_id: 0,
            device_type: t,
            backend: "vulkan".into(),
            driver: String::new(),
            driver_info: String::new(),
            supports_timestamps: true,
            max_buffer_bytes: 0,
            max_storage_binding_bytes: 0,
            max_workgroups_per_dimension: 65_535,
            recommended: false,
        }
    }

    #[test]
    fn notes_explain_integrated_software_and_wall_clock_cases() {
        let integrated = notes(
            &info(GpuDeviceType::Integrated),
            TimingMethod::GpuTimestamps,
        );
        assert!(
            integrated
                .iter()
                .any(|n| n.contains("shared system memory"))
        );
        let software = notes(&info(GpuDeviceType::Software), TimingMethod::GpuTimestamps);
        assert!(software.iter().any(|n| n.contains("software renderer")));
        let wall = notes(&info(GpuDeviceType::Discrete), TimingMethod::WallClock);
        assert!(wall.iter().any(|n| n.contains("wall clock")));
        // A discrete GPU with timestamps has only the monitoring caveat.
        let clean = notes(&info(GpuDeviceType::Discrete), TimingMethod::GpuTimestamps);
        assert_eq!(clean.len(), 1);
        assert!(clean[0].contains("not measured"));
    }
}
