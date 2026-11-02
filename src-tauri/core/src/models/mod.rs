//! Data models shared with the frontend. TypeScript definitions are generated
//! from these types into `src/types/generated/` by `cargo test`.

mod details;
mod metrics;
mod monitoring;
mod process;
mod system;

pub use details::*;
pub use metrics::*;
pub use monitoring::*;
pub use process::*;
pub use system::*;
