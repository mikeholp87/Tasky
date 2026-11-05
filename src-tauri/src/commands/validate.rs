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

/// A path typed or picked in the file finder. Only checks shape; whether it
/// exists is the platform code's business.
pub fn search_path(raw: &str) -> AppResult<&str> {
    let path = raw.trim();
    if path.is_empty() {
        return Err(AppError::InvalidArgument("Enter or pick a path.".into()));
    }
    if path.len() > 4096 || path.contains('\0') {
        return Err(AppError::InvalidArgument(
            "That is not a valid path.".into(),
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_search_path_shape() {
        assert_eq!(search_path("  /tmp/a b  ").unwrap(), "/tmp/a b");
        for bad in ["", "   ", "a\0b", &"x".repeat(5000)] {
            assert!(search_path(bad).is_err());
        }
    }

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
