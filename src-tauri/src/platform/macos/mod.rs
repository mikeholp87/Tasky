//! macOS implementation. Not yet a supported target: functions return
//! "unavailable" so the rest of the app compiles and degrades gracefully.

use super::MemoryExtras;

pub fn memory_extras() -> MemoryExtras {
    MemoryExtras::default()
}
