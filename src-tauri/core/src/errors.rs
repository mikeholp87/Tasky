//! Structured application errors. Every error crossing the IPC boundary is
//! serialized as `{ kind, message }` so the frontend can show a friendly
//! message without ever seeing a Rust backtrace.

use serde::{Serialize, Serializer, ser::SerializeStruct};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    PermissionDenied,
    ProcessNotFound,
    UnsupportedOperation,
    SystemQueryFailed,
    InvalidPid,
    InvalidArgument,
    ProcessProtected,
    ActionUnavailable,
    EngineUnavailable,
    BenchmarkAlreadyRunning,
    BenchmarkFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    PermissionDenied(String),
    #[error("Process {0} is no longer running.")]
    ProcessNotFound(u32),
    #[error("{0} is unavailable on this platform.")]
    UnsupportedOperation(String),
    #[error("System query failed: {0}")]
    SystemQueryFailed(String),
    #[error("{0} is not a valid process ID.")]
    InvalidPid(i64),
    #[error("{0}")]
    InvalidArgument(String),
    #[error("{0}")]
    ProcessProtected(String),
    /// The requested action cannot be applied to this process (e.g. a
    /// graceful close for a process without windows).
    #[error("{0}")]
    ActionUnavailable(String),
    #[error("The monitoring engine is not running.")]
    EngineUnavailable,
    #[error("Another benchmark is currently running.")]
    BenchmarkAlreadyRunning,
    #[error("The benchmark could not finish: {0}")]
    BenchmarkFailed(String),
}

impl AppError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::PermissionDenied(_) => ErrorKind::PermissionDenied,
            Self::ProcessNotFound(_) => ErrorKind::ProcessNotFound,
            Self::UnsupportedOperation(_) => ErrorKind::UnsupportedOperation,
            Self::SystemQueryFailed(_) => ErrorKind::SystemQueryFailed,
            Self::InvalidPid(_) => ErrorKind::InvalidPid,
            Self::InvalidArgument(_) => ErrorKind::InvalidArgument,
            Self::ProcessProtected(_) => ErrorKind::ProcessProtected,
            Self::ActionUnavailable(_) => ErrorKind::ActionUnavailable,
            Self::EngineUnavailable => ErrorKind::EngineUnavailable,
            Self::BenchmarkAlreadyRunning => ErrorKind::BenchmarkAlreadyRunning,
            Self::BenchmarkFailed(_) => ErrorKind::BenchmarkFailed,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", &self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Wire shape of [`AppError`], exported for the frontend.
    #[derive(ts_rs::TS)]
    #[ts(export, rename = "AppError")]
    #[allow(dead_code)]
    struct AppErrorPayload {
        kind: ErrorKind,
        message: String,
    }

    #[test]
    fn serializes_kind_and_message() {
        let json = serde_json::to_value(AppError::ProcessNotFound(42)).unwrap();
        assert_eq!(json["kind"], "processNotFound");
        assert_eq!(json["message"], "Process 42 is no longer running.");
    }

    #[test]
    fn invalid_pid_message_is_user_facing() {
        let err = AppError::InvalidPid(-3);
        assert_eq!(err.kind(), ErrorKind::InvalidPid);
        assert_eq!(err.to_string(), "-3 is not a valid process ID.");
    }
}
