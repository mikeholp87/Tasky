//! Windows implementation using documented Win32 APIs.

mod account;
mod bench_io;
mod connections;
mod control;
mod disk_io;
mod handle;
mod hardware;
mod inspect;
mod memory;
mod net;
mod power;
mod process_info;
mod registry;
mod startup;

pub use account::account_name;
pub use bench_io::{open_benchmark_file, read_at, write_at};
pub use connections::connections;
pub use control::{is_critical, pin_process, system_root};
pub use disk_io::disk_io_counters;
pub use handle::architecture as process_architecture;
pub use hardware::hardware_info;
pub use inspect::{find_file_users, memory_map, modules, open_files, sockets, threads};
pub use memory::memory_extras;
pub use net::interface_kind;
pub use power::power_state;
pub use process_info::process_extras;
pub use startup::{set_startup_enabled, startup_items};
