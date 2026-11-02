use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

/// Polling intervals the UI offers. Anything else is rejected so a buggy or
/// hostile caller cannot spin the engine at 1 ms.
pub const ALLOWED_INTERVALS_MS: [u32; 4] = [500, 1_000, 2_000, 5_000];
pub const DEFAULT_INTERVAL_MS: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MonitoringState {
    pub paused: bool,
    pub interval_ms: u32,
}

impl Default for MonitoringState {
    fn default() -> Self {
        Self {
            paused: false,
            interval_ms: DEFAULT_INTERVAL_MS,
        }
    }
}

pub fn validate_interval(ms: u32) -> AppResult<u32> {
    if ALLOWED_INTERVALS_MS.contains(&ms) {
        Ok(ms)
    } else {
        Err(AppError::InvalidArgument(format!(
            "Refresh interval {ms} ms is not supported."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_known_intervals() {
        assert_eq!(validate_interval(500).unwrap(), 500);
        assert!(validate_interval(1).is_err());
        assert!(validate_interval(0).is_err());
        assert!(validate_interval(60_000).is_err());
    }
}
