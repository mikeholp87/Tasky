//! Structured application errors. Every error crossing the IPC boundary is
//! serialized as `{ kind, message }` so the frontend can show a friendly
//! message without ever seeing a Rust backtrace.

use serde::{Deserialize, Serialize, Serializer, ser::SerializeStruct};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    OperationCancelled,
    OperationBusy,
    EngineUnavailable,
    BenchmarkAlreadyRunning,
    BenchmarkCancelled,
    GpuUnavailable,
    GpuUnsupportedFeature,
    GpuInitializationFailed,
    InsufficientDiskSpace,
    DiskPermissionDenied,
    NetworkUnavailable,
    NetworkTimeout,
    ProviderUnavailable,
    UnsupportedPlatform,
    InvalidBenchmarkConfiguration,
    ThermalSensorUnavailable,
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
    #[error("Operation cancelled.")]
    OperationCancelled,
    #[error("Another operation is running. Try again shortly.")]
    OperationBusy,
    #[error("The monitoring engine is not running.")]
    EngineUnavailable,
    // Benchmarks. Messages are shown to users as-is.
    #[error("Another benchmark is already running.")]
    BenchmarkAlreadyRunning,
    #[error("The benchmark was cancelled.")]
    BenchmarkCancelled,
    #[error("{0}")]
    GpuUnavailable(String),
    #[error("{0}")]
    GpuUnsupportedFeature(String),
    #[error("GPU initialization failed: {0}")]
    GpuInitializationFailed(String),
    #[error("{0}")]
    InsufficientDiskSpace(String),
    #[error("{0}")]
    DiskPermissionDenied(String),
    #[error("{0}")]
    NetworkUnavailable(String),
    #[error("{0}")]
    NetworkTimeout(String),
    #[error("{0}")]
    ProviderUnavailable(String),
    #[error("{0}")]
    UnsupportedPlatform(String),
    #[error("{0}")]
    InvalidBenchmarkConfiguration(String),
    #[error("Temperature monitoring is unavailable on this system.")]
    ThermalSensorUnavailable,
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
            Self::OperationCancelled => ErrorKind::OperationCancelled,
            Self::OperationBusy => ErrorKind::OperationBusy,
            Self::EngineUnavailable => ErrorKind::EngineUnavailable,
            Self::BenchmarkAlreadyRunning => ErrorKind::BenchmarkAlreadyRunning,
            Self::BenchmarkCancelled => ErrorKind::BenchmarkCancelled,
            Self::GpuUnavailable(_) => ErrorKind::GpuUnavailable,
            Self::GpuUnsupportedFeature(_) => ErrorKind::GpuUnsupportedFeature,
            Self::GpuInitializationFailed(_) => ErrorKind::GpuInitializationFailed,
            Self::InsufficientDiskSpace(_) => ErrorKind::InsufficientDiskSpace,
            Self::DiskPermissionDenied(_) => ErrorKind::DiskPermissionDenied,
            Self::NetworkUnavailable(_) => ErrorKind::NetworkUnavailable,
            Self::NetworkTimeout(_) => ErrorKind::NetworkTimeout,
            Self::ProviderUnavailable(_) => ErrorKind::ProviderUnavailable,
            Self::UnsupportedPlatform(_) => ErrorKind::UnsupportedPlatform,
            Self::InvalidBenchmarkConfiguration(_) => ErrorKind::InvalidBenchmarkConfiguration,
            Self::ThermalSensorUnavailable => ErrorKind::ThermalSensorUnavailable,
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
