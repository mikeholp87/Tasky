//! Mount and unmount external drives. The platform layer decides which
//! volumes are eligible; these commands add the OS confirmation for the
//! disruptive actions and run everything on a blocking worker.

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
pub async fn mount_volume(device: String) -> AppResult<Option<String>> {
    blocking(move || platform::mount_volume(&device)).await
}

/// Powers off the drive holding an unmounted, listed volume. Asks first.
#[tauri::command]
pub async fn power_off_volume(app: AppHandle, device: String) -> AppResult<()> {
    blocking(move || {
        let name = listed_name(&device)?;
        super::confirm::confirm(
            &app,
            &format!(
                "Power off the drive holding {name}?\nIt can't be used again until it is unplugged and reconnected."
            ),
        )?;
        platform::power_off_volume(&device)
    })
    .await
}

/// Unmounts a listed drive. Refused for system and internal partitions. Asks first.
#[tauri::command]
pub async fn unmount_volume(app: AppHandle, device: String) -> AppResult<()> {
    blocking(move || {
        let name = listed_name(&device)?;
        super::confirm::confirm(
            &app,
            &format!("Unmount {name}?\nPrograms using files on it may lose unsaved work."),
        )?;
        platform::unmount_volume(&device)
    })
    .await
}

/// How the prompt names a listed volume: its label and device, from the
/// platform's own list rather than from the request.
fn listed_name(device: &str) -> AppResult<String> {
    let volume = platform::volumes()?
        .into_iter()
        .find(|v| v.device == device)
        .ok_or_else(|| AppError::ActionUnavailable("That drive is no longer available.".into()))?;
    Ok(display_name(&volume))
}

fn display_name(volume: &Volume) -> String {
    match &volume.label {
        Some(label) => format!("\"{}\" ({})", label.escape_debug(), volume.device),
        None => volume.device.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(label: Option<&str>) -> Volume {
        Volume {
            device: "/dev/sdb1".into(),
            label: label.map(str::to_owned),
            file_system: "vfat".into(),
            size_bytes: 1,
            mount_point: None,
            can_power_off: true,
        }
    }

    #[test]
    fn prompts_name_the_label_and_device() {
        assert_eq!(display_name(&volume(Some("USB"))), "\"USB\" (/dev/sdb1)");
        assert_eq!(display_name(&volume(None)), "/dev/sdb1");
        // A label cannot inject extra lines into the prompt.
        assert_eq!(display_name(&volume(Some("A\nB"))), "\"A\\nB\" (/dev/sdb1)");
    }
}
