//! Choosing a graphics adapter. Pure logic over what the driver reported, so
//! it is tested without a GPU. Selection uses the reported device type, never
//! a vendor name in a string.

use super::super::BenchmarkError;
use super::super::model::{GpuAdapterInfo, GpuDeviceType};

/// Lower is preferred: a discrete GPU, then an integrated one. A software
/// renderer is the last resort because its result describes the CPU.
fn preference(t: GpuDeviceType) -> u8 {
    match t {
        GpuDeviceType::Discrete => 0,
        GpuDeviceType::Integrated => 1,
        GpuDeviceType::Virtual => 2,
        GpuDeviceType::Other => 3,
        GpuDeviceType::Software => 4,
    }
}

/// Index of the adapter to use by default; the first of equals wins.
pub fn recommended_index(adapters: &[GpuAdapterInfo]) -> Option<usize> {
    adapters
        .iter()
        .enumerate()
        .min_by_key(|(i, a)| (preference(a.device_type), *i))
        .map(|(i, _)| i)
}

/// Marks exactly one adapter as the recommended default.
pub fn mark_recommended(adapters: &mut [GpuAdapterInfo]) {
    let pick = recommended_index(adapters);
    for (i, a) in adapters.iter_mut().enumerate() {
        a.recommended = Some(i) == pick;
    }
}

/// An id that distinguishes two identical cards by their enumeration order.
pub fn make_id(backend: &str, vendor: u32, device: u32, ordinal: usize) -> String {
    format!("{backend}-{vendor:04x}-{device:04x}-{ordinal}")
}

/// The adapter named by `id`, or the recommended one when `id` is `None`.
pub fn select<'a>(
    adapters: &'a [GpuAdapterInfo],
    id: Option<&str>,
) -> Result<&'a GpuAdapterInfo, BenchmarkError> {
    match id {
        Some(id) => adapters.iter().find(|a| a.id == id).ok_or_else(|| {
            BenchmarkError::Failed("That graphics adapter is no longer available.".into())
        }),
        None => recommended_index(adapters)
            .and_then(|i| adapters.get(i))
            .ok_or_else(|| {
                BenchmarkError::Failed(
                    "No graphics adapter was found. A working Vulkan driver is needed for the GPU benchmark."
                        .into(),
                )
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(id: &str, t: GpuDeviceType) -> GpuAdapterInfo {
        GpuAdapterInfo {
            id: id.into(),
            name: id.into(),
            vendor_id: 0,
            device_id: 0,
            device_type: t,
            backend: "vulkan".into(),
            driver: String::new(),
            driver_info: String::new(),
            supports_timestamps: true,
            max_buffer_bytes: 1 << 30,
            max_storage_binding_bytes: 1 << 27,
            max_workgroups_per_dimension: 65_535,
            recommended: false,
        }
    }

    #[test]
    fn prefers_discrete_then_integrated_and_software_last() {
        use GpuDeviceType::*;
        let hybrid = [
            adapter("igpu", Integrated),
            adapter("dgpu", Discrete),
            adapter("cpu", Software),
        ];
        assert_eq!(recommended_index(&hybrid), Some(1));
        let laptop = [adapter("cpu", Software), adapter("igpu", Integrated)];
        assert_eq!(recommended_index(&laptop), Some(1));
        // Only a software renderer: usable, but flagged elsewhere.
        assert_eq!(recommended_index(&[adapter("cpu", Software)]), Some(0));
        assert_eq!(recommended_index(&[]), None);
    }

    #[test]
    fn ties_go_to_the_first_in_enumeration_order() {
        use GpuDeviceType::*;
        let two = [adapter("a", Discrete), adapter("b", Discrete)];
        assert_eq!(recommended_index(&two), Some(0));
    }

    #[test]
    fn exactly_one_adapter_is_recommended() {
        use GpuDeviceType::*;
        let mut list = vec![
            adapter("x", Virtual),
            adapter("y", Integrated),
            adapter("z", Integrated),
        ];
        mark_recommended(&mut list);
        assert_eq!(list.iter().filter(|a| a.recommended).count(), 1);
        assert!(list[1].recommended);
        // Re-marking after reordering moves the flag instead of adding one.
        list.swap(0, 1);
        mark_recommended(&mut list);
        assert_eq!(list.iter().filter(|a| a.recommended).count(), 1);
        assert!(list[0].recommended);
    }

    #[test]
    fn selection_honours_an_explicit_choice_and_reports_a_missing_one() {
        use GpuDeviceType::*;
        let list = [adapter("igpu", Integrated), adapter("dgpu", Discrete)];
        assert_eq!(select(&list, Some("igpu")).unwrap().id, "igpu");
        assert_eq!(select(&list, None).unwrap().id, "dgpu");
        assert!(select(&list, Some("gone")).is_err());
        let none: [GpuAdapterInfo; 0] = [];
        assert!(select(&none, None).is_err());
    }

    #[test]
    fn identical_cards_get_distinct_ids() {
        assert_ne!(
            make_id("vulkan", 0x10de, 0x2684, 0),
            make_id("vulkan", 0x10de, 0x2684, 1)
        );
        assert_eq!(make_id("vulkan", 0x8086, 0xa7ac, 0), "vulkan-8086-a7ac-0");
    }
}
