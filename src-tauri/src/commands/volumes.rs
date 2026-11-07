//! Mount and unmount external drives. The platform layer decides which
//! volumes are eligible; these commands only run it on a blocking worker.

use super::blocking::run as blocking;
use tm_core::errors::AppResult;
use tm_core::models::Volume;
use tm_core::platform;

/// Drives that can be mounted or unmounted (never system partitions).
#[tauri::command]
pub async fn list_volumes() -> AppResult<Vec<Volume>> {
    blocking(platform::volumes).await
}

/// Mounts a listed drive; returns where it is mounted.
#[tauri::command]
pub async fn mount_volume(device: String) -> AppResult<Option<String>> {
    blocking(move || platform::mount_volume(&device)).await
}

/// Unmounts a listed drive. Refused for system and internal partitions.
#[tauri::command]
pub async fn unmount_volume(device: String) -> AppResult<()> {
    blocking(move || platform::unmount_volume(&device)).await
}
