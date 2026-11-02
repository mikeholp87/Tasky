//! Settings persistence: a JSON file in the app's config directory.
//!
//! Writes are atomic (temp file + rename) so a crash never leaves a
//! half-written file. A corrupt or unreadable file falls back to defaults and
//! is moved aside rather than silently overwritten.

use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use tm_core::errors::{AppError, AppResult};
use tm_core::models::Settings;

const FILE_NAME: &str = "settings.json";

pub struct SettingsStore {
    path: Option<PathBuf>,
    current: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(dir: Option<&Path>) -> Self {
        let path = dir.map(|d| d.join(FILE_NAME));
        let current = path.as_deref().map(read_or_default).unwrap_or_default();
        Self {
            path,
            current: RwLock::new(current),
        }
    }

    pub fn get(&self) -> Settings {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Validates, persists and stores `next`. Returns the stored settings.
    pub fn save(&self, next: Settings) -> AppResult<Settings> {
        next.validate()?;
        if let Some(path) = &self.path {
            write_atomic(path, &next)?;
        }
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = next.clone();
        Ok(next)
    }
}

fn read_or_default(path: &Path) -> Settings {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            tracing::warn!("could not read settings: {e}");
            return Settings::default();
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(settings) => settings.sanitized(),
        Err(e) => {
            tracing::warn!("settings file is invalid ({e}); using defaults");
            let _ = std::fs::rename(path, path.with_extension("json.invalid"));
            Settings::default()
        }
    }
}

fn write_atomic(path: &Path, settings: &Settings) -> AppResult<()> {
    let fail = |e: &dyn std::fmt::Display| {
        AppError::SystemQueryFailed(format!("could not save settings: {e}"))
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| fail(&e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| fail(&e))?;
    std::fs::rename(&tmp, path).map_err(|e| fail(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_core::models::ThemePreference;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tm-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = temp_dir("roundtrip");
        let store = SettingsStore::load(Some(&dir));
        let mut s = store.get();
        s.theme = ThemePreference::Dark;
        store.save(s).unwrap();
        assert_eq!(
            SettingsStore::load(Some(&dir)).get().theme,
            ThemePreference::Dark
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_invalid_and_keeps_previous() {
        let store = SettingsStore::load(None);
        let bad = Settings {
            refresh_interval_ms: 7,
            ..Settings::default()
        };
        assert!(store.save(bad).is_err());
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn corrupt_file_falls_back_and_is_preserved() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        assert_eq!(SettingsStore::load(Some(&dir)).get(), Settings::default());
        assert!(dir.join("settings.json.invalid").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
