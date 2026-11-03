//! Windows implementation using documented Win32 APIs.

mod account;
mod bench_io;
mod connections;
mod control;
mod disk_io;
mod handle;
mod memory;
mod power;
mod process_info;

pub use account::account_name;
pub use bench_io::{open_benchmark_file, read_at, write_at};
pub use connections::connections;
pub use control::{force_kill, is_critical, resume, set_priority, suspend, system_root, terminate};
pub use disk_io::disk_io_counters;
pub use handle::architecture as process_architecture;
pub use memory::memory_extras;
pub use power::power_state;
pub use process_info::process_extras;
