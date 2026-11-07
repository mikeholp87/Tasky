//! Data models shared with the frontend. TypeScript definitions are generated
//! from these types into `src/types/generated/` by `cargo test`.

mod activity;
mod alerts;
mod control;
mod details;
mod hardware;
mod inspect;
mod metrics;
mod monitoring;
mod process;
mod settings;
mod startup;
mod system;
mod volumes;

pub use activity::*;
pub use alerts::*;
pub use control::*;
pub use details::*;
pub use hardware::*;
pub use inspect::*;
pub use metrics::*;
pub use monitoring::*;
pub use process::*;
pub use settings::*;
pub use startup::*;
pub use system::*;
pub use volumes::*;
