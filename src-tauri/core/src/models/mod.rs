//! Data models shared with the frontend. TypeScript definitions are generated
//! from these types into `src/types/generated/` by `cargo test`.

mod alerts;
mod control;
mod details;
mod metrics;
mod monitoring;
mod process;
mod settings;
mod system;

pub use alerts::*;
pub use control::*;
pub use details::*;
pub use metrics::*;
pub use monitoring::*;
pub use process::*;
pub use settings::*;
pub use system::*;
