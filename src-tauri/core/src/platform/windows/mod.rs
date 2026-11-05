//! Windows implementation using documented Win32 APIs.

mod account;
mod connections;
mod control;
mod disk_io;
mod handle;
mod inspect;
mod memory;
mod process_info;

pub use account::account_name;
pub use connections::connections;
pub use control::{is_critical, perform_action, system_root};
pub use disk_io::disk_io_counters;
pub use handle::architecture as process_architecture;
pub use inspect::{find_file_users, memory_map, modules, open_files, sockets, threads};
pub use memory::memory_extras;
pub use process_info::process_extras;
