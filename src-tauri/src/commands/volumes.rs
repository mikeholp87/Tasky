//! Mount and unmount external drives. The platform layer decides which
//! volumes are eligible; these commands only run it on a blocking worker.

use super::blocking::run as blocking;
use tauri::AppHandle;
use tm_core::errors::{AppError, AppResult};
use tm_core::models::Volume;
use tm_core::platform;

/// Drives that can be mounted or unmounted (never system partitions).
#[tauri::command]
pub async fn list_volumes() -> AppResult<Vec<Volume>> {
    blocking(platform::volumes).await
}

/// Mounts a listed drive; returns where it is mounted.
#[tauri::command]
pub async fn mount_volume(app: AppHandle, device: String) -> AppResult<Option<String>> {
    blocking(move || {
        confirm(&app, &device, "Mount")?;
        platform::mount_volume(&device)
    })
    .await
}

/// Powers off the drive holding an unmounted, listed volume.
#[tauri::command]
pub async fn power_off_volume(app: AppHandle, device: String) -> AppResult<()> {
    blocking(move || {
        confirm(&app, &device, "Safely eject")?;
        platform::power_off_volume(&device)
    })
    .await
}

/// Unmounts a listed drive. Refused for system and internal partitions.
#[tauri::command]
pub async fn unmount_volume(app: AppHandle, device: String) -> AppResult<()> {
    blocking(move || {
        confirm(&app, &device, "Unmount")?;
        platform::unmount_volume(&device)
    })
    .await
}

fn confirm(app: &AppHandle, device: &str, verb: &str) -> AppResult<()> {
    let volume = platform::volumes()?
        .into_iter()
        .find(|v| v.device == device)
        .ok_or_else(|| {
            AppError::ActionUnavailable("This external drive is no longer available.".into())
        })?;
    super::confirm::confirm(
        app,
        &format!(
            "{verb} {}?\n{}",
            volume.label.as_deref().unwrap_or("external drive"),
            volume.device
        ),
    )
}
