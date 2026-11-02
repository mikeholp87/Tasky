//! Input validation shared by commands. Everything arriving over IPC is
//! untrusted until checked here.

use tm_core::errors::{AppError, AppResult};

/// PIDs arrive as JSON numbers; accept them as `i64` so negative, zero and
/// out-of-range values produce a clear `InvalidPid` error instead of a
/// generic deserialization failure.
pub fn pid(raw: i64) -> AppResult<u32> {
    match u32::try_from(raw) {
        Ok(pid) if pid > 0 => Ok(pid),
        _ => Err(AppError::InvalidPid(raw)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_positive_u32() {
        assert_eq!(pid(1).unwrap(), 1);
        assert_eq!(pid(i64::from(u32::MAX)).unwrap(), u32::MAX);
    }

    #[test]
    fn rejects_zero_negative_and_overflow() {
        for raw in [0, -1, i64::from(u32::MAX) + 1, i64::MIN] {
            assert!(matches!(pid(raw), Err(AppError::InvalidPid(r)) if r == raw));
        }
    }
}
