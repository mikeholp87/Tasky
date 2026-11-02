//! macOS implementation. Not yet a supported target: functions return
//! "unavailable" so the rest of the app compiles and degrades gracefully.

use std::collections::HashMap;

use super::{MemoryExtras, ProcessExtras};

pub fn memory_extras() -> MemoryExtras {
    MemoryExtras::default()
}

pub fn process_extras(_pids: &[u32]) -> HashMap<u32, ProcessExtras> {
    HashMap::new()
}

pub fn account_name(_uid: &sysinfo::Uid) -> Option<String> {
    None
}
