//! Windows Run entries and Startup folders. StartupApproved is the Explorer
//! policy used by Windows 11 to enable/disable these entries without deleting
//! commands or moving shortcuts. Its binary format is Windows-specific; unknown
//! states are read-only instead of being overwritten.

use std::path::PathBuf;

use sha2::{Digest, Sha256};
use windows::Win32::System::Registry::*;

use super::registry;
use crate::errors::{AppError, AppResult};
use crate::models::{StartupItem, StartupScope};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

struct Entry {
    item: StartupItem,
    root: HKEY,
    approval: String,
    name: String,
}

fn id(source: &str, name: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(source.as_bytes());
    hash.update([0]);
    hash.update(name.to_lowercase().as_bytes());
    format!("win:{:x}", hash.finalize())
}

fn approved_state(data: &[u8]) -> Option<bool> {
    if data.len() != 12 {
        return None;
    }
    match u32::from_le_bytes(data[..4].try_into().ok()?) {
        2 | 6 => Some(true),
        3 | 7 => Some(false),
        _ => None,
    }
}

fn approval(root: HKEY, path: &str, name: &str) -> AppResult<Option<Vec<u8>>> {
    let Some(key) = registry::open(root, path, KEY_WOW64_64KEY)? else {
        return Ok(None);
    };
    match registry::value(key.0, name)? {
        None => Ok(None),
        Some((REG_BINARY, data)) => Ok(Some(data)),
        Some(_) => Err(AppError::ActionUnavailable(
            "Windows uses an unrecognized startup policy for this entry.".into(),
        )),
    }
}

fn make_entry(
    root: HKEY,
    scope: StartupScope,
    source: &str,
    section: &str,
    name: String,
    command: String,
) -> AppResult<Entry> {
    let approval_path = format!(r"{APPROVED}\{section}");
    let state = approval(root, &approval_path, &name)?;
    let enabled = state.as_deref().map(approved_state).unwrap_or(Some(true));
    Ok(Entry {
        item: StartupItem {
            id: id(source, &name),
            name: name.strip_suffix(".lnk").unwrap_or(&name).to_owned(),
            comment: Some(source.into()),
            command,
            scope,
            enabled: enabled.unwrap_or(false),
            only_in: None,
            runs_here: enabled.is_some(),
        },
        root,
        approval: approval_path,
        name,
    })
}

fn entries() -> AppResult<Vec<Entry>> {
    let mut out = Vec::new();
    for (root, scope, label) in [
        (HKEY_CURRENT_USER, StartupScope::User, "Current user"),
        (HKEY_LOCAL_MACHINE, StartupScope::System, "All users"),
    ] {
        for (view, section, arch) in [
            (KEY_WOW64_64KEY, "Run", "64-bit Run"),
            (KEY_WOW64_32KEY, "Run32", "32-bit Run"),
        ] {
            // HKCU\Software is shared between registry views. Read it once.
            if root == HKEY_CURRENT_USER && view == KEY_WOW64_32KEY {
                continue;
            }
            let Some(key) = registry::open(root, RUN, view)? else {
                continue;
            };
            for name in registry::names(key.0)? {
                let Some((kind, data)) = registry::value(key.0, &name)? else {
                    continue;
                };
                let Some(command) = registry::decode_string(kind, &data).filter(|v| !v.is_empty())
                else {
                    continue;
                };
                out.push(make_entry(
                    root,
                    scope,
                    &format!("{label} · {arch}"),
                    section,
                    name,
                    command,
                )?);
            }
        }
    }
    for (env, scope, root, label) in [
        (
            "APPDATA",
            StartupScope::User,
            HKEY_CURRENT_USER,
            "Current user · Startup folder",
        ),
        (
            "PROGRAMDATA",
            StartupScope::System,
            HKEY_LOCAL_MACHINE,
            "All users · Startup folder",
        ),
    ] {
        let Some(base) = std::env::var_os(env) else {
            continue;
        };
        let folder = PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
        let dir = match std::fs::read_dir(folder) {
            Ok(dir) => dir,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(AppError::PermissionDenied(format!(
                    "Cannot read the Startup folder: {e}"
                )));
            }
        };
        for entry in dir.take(2_000) {
            let entry = entry.map_err(|e| AppError::SystemQueryFailed(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.eq_ignore_ascii_case("desktop.ini")
                || !entry.file_type().is_ok_and(|t| t.is_file())
            {
                continue;
            }
            out.push(make_entry(
                root,
                scope,
                label,
                "StartupFolder",
                name,
                entry.path().to_string_lossy().into_owned(),
            )?);
        }
    }
    out.sort_by(|a, b| {
        a.item
            .name
            .to_lowercase()
            .cmp(&b.item.name.to_lowercase())
            .then(a.item.id.cmp(&b.item.id))
    });
    Ok(out)
}

pub fn startup_items() -> AppResult<Vec<StartupItem>> {
    Ok(entries()?.into_iter().map(|e| e.item).collect())
}

pub fn set_startup_enabled(entry_id: &str, enabled: bool) -> AppResult<()> {
    // Resolve only an existing, freshly enumerated entry. An IPC caller cannot
    // supply a registry key, value name, command, or filesystem destination.
    let entry = entries()?
        .into_iter()
        .find(|e| e.item.id == entry_id)
        .ok_or_else(|| {
            AppError::InvalidArgument(
                "This startup entry no longer exists. Refresh the list.".into(),
            )
        })?;
    let previous = approval(entry.root, &entry.approval, &entry.name)?;
    if previous
        .as_deref()
        .is_some_and(|v| approved_state(v).is_none())
    {
        return Err(AppError::ActionUnavailable(
            "Windows uses an unrecognized startup policy. Manage this entry in Windows Settings."
                .into(),
        ));
    }
    let data = policy_bytes(previous.as_deref(), enabled);
    registry::set_binary(entry.root, &entry.approval, &entry.name, &data)?;
    if approval(entry.root, &entry.approval, &entry.name)?
        .as_deref()
        .and_then(approved_state)
        != Some(enabled)
    {
        return Err(AppError::SystemQueryFailed(
            "Windows did not retain the startup change. Refresh and try again.".into(),
        ));
    }
    Ok(())
}

fn policy_bytes(previous: Option<&[u8]>, enabled: bool) -> [u8; 12] {
    let mut data = [0u8; 12];
    if let Some(old) = previous.filter(|v| v.len() == 12) {
        data.copy_from_slice(old);
    }
    data[..4].copy_from_slice(&(if enabled { 2u32 } else { 3u32 }).to_le_bytes());
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_round_trip_preserves_other_bytes_and_rejects_unknown_formats() {
        let mut old = [0u8; 12];
        old[0] = 7;
        old[4..].copy_from_slice(&12345u64.to_le_bytes());
        assert_eq!(approved_state(&old), Some(false));
        let enabled = policy_bytes(Some(&old), true);
        assert_eq!(approved_state(&enabled), Some(true));
        assert_eq!(&enabled[4..], &old[4..]);
        assert_eq!(approved_state(&[3]), None);
        assert_eq!(approved_state(&[255; 12]), None);
    }

    #[test]
    fn ids_include_source_and_preserve_case_insensitive_windows_identity() {
        assert_eq!(id("user", "App"), id("user", "app"));
        assert_ne!(id("user", "app"), id("system", "app"));
        assert_eq!(id("user", &"x".repeat(20_000)).len(), 68);
    }

    #[test]
    fn real_startup_list_is_read_only_and_ids_are_unique() {
        let items = startup_items().unwrap();
        let ids: std::collections::HashSet<_> = items.iter().map(|i| &i.id).collect();
        assert_eq!(ids.len(), items.len());
        assert!(items.iter().all(|i| !i.command.is_empty()));
        assert!(set_startup_enabled("not-a-real-entry", false).is_err());
    }
}
