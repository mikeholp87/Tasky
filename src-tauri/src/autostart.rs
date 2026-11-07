#[cfg(target_os = "linux")]
use tauri::Manager;
use tauri::{AppHandle, Runtime};
use tm_core::errors::{AppError, AppResult};

pub fn entry_id<R: Runtime>(app: &AppHandle<R>) -> String {
    format!("{}.desktop", app.package_info().name)
}

#[cfg(not(target_os = "linux"))]
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> AppResult<bool> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| AppError::SystemQueryFailed(format!("could not read the login setting: {e}")))
}

#[cfg(not(target_os = "linux"))]
pub fn set<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> AppResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result
        .map_err(|e| AppError::PermissionDenied(format!("could not change the login setting: {e}")))
}

#[cfg(target_os = "linux")]
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> AppResult<bool> {
    let path = tm_core::platform::startup_directory()?.join(entry_id(app));
    match tm_core::filesystem::read_text(&path, 256 * 1024) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(AppError::SystemQueryFailed(format!(
                "could not read the login entry: {e}"
            )));
        }
    }
    Ok(tm_core::platform::startup_items()?
        .iter()
        .any(|item| item.id == entry_id(app) && item.enabled))
}

#[cfg(target_os = "linux")]
pub fn set<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> AppResult<()> {
    let directory = tm_core::platform::startup_directory()?;
    if tm_core::platform::startup_items()?
        .iter()
        .any(|item| item.id == entry_id(app))
    {
        return tm_core::platform::set_startup_enabled(&entry_id(app), enabled);
    }
    if !enabled {
        return Ok(());
    }
    match std::fs::symlink_metadata(directory.join(entry_id(app))) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(AppError::SystemQueryFailed(e.to_string())),
        Ok(_) => return Err(AppError::ActionUnavailable("The existing login entry could not be read. Preserve or repair it before enabling autostart.".into())),
    }
    let exe = app
        .env()
        .appimage
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| {
            AppError::SystemQueryFailed("the application executable could not be located".into())
        })?;
    let text = linux_entry(&app.package_info().name, &exe)?;
    tm_core::filesystem::write_private_atomic(&directory.join(entry_id(app)), text.as_bytes())
        .map_err(|e| AppError::SystemQueryFailed(format!("could not save the login entry: {e}")))
}

#[cfg(target_os = "linux")]
fn linux_entry(name: &str, exe: &std::path::Path) -> AppResult<String> {
    let path = exe
        .to_str()
        .filter(|p| !p.chars().any(char::is_control))
        .ok_or_else(|| {
            AppError::InvalidArgument(
                "The executable path cannot be represented in a login entry.".into(),
            )
        })?;
    let quoted = path
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    let entry = gio::glib::KeyFile::new();
    entry.set_string("Desktop Entry", "Type", "Application");
    entry.set_string("Desktop Entry", "Name", name);
    entry.set_string("Desktop Entry", "Exec", &format!("\"{quoted}\""));
    entry.set_boolean("Desktop Entry", "Terminal", false);
    Ok(entry.to_data().to_string())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn login_entry_quotes_executable_and_escapes_desktop_field_codes() {
        let text = linux_entry(
            "Task Manager",
            std::path::Path::new("/tmp/my $app%/tasky\"x"),
        )
        .unwrap();
        let entry = gio::glib::KeyFile::new();
        entry
            .load_from_data(&text, gio::glib::KeyFileFlags::NONE)
            .unwrap();
        assert_eq!(
            entry.string("Desktop Entry", "Exec").unwrap(),
            "\"/tmp/my \\$app%%/tasky\\\"x\""
        );
        assert_eq!(
            entry.string("Desktop Entry", "Name").unwrap(),
            "Task Manager"
        );
        assert!(linux_entry("Tasky", std::path::Path::new("/tmp/bad\npath")).is_err());
    }
}
