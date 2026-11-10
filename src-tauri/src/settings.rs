//! Settings persistence: a JSON file in the app's config directory.
//!
//! Writes are atomic (temp file + rename) so a crash never leaves a
//! half-written file. A corrupt or unreadable file falls back to defaults and
//! is moved aside rather than silently overwritten.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use tm_core::errors::{AppError, AppResult};
use tm_core::models::Settings;
use tm_core::sync_util::{read, write};

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
        read(&self.current).clone()
    }

    /// Applies `change` to the current settings, then validates, persists and
    /// stores the result, all under one write lock so concurrent updates are
    /// never lost. `then` sees the stored settings before the lock is
    /// released, so runtime effects are applied in the order saves happen.
    pub fn update(
        &self,
        change: impl FnOnce(&mut Settings),
        then: impl FnOnce(&Settings),
    ) -> AppResult<Settings> {
        let mut current = write(&self.current);
        let mut next = current.clone();
        change(&mut next);
        let enabled = next.start_with_os;
        self.commit(&mut current, next, || Ok(enabled), |_| Ok(()), then)
    }

    /// [`update`](Self::update), also applying `start_with_os` to the real
    /// login entry. Rolls autostart back if persistence fails.
    pub fn update_with_autostart(
        &self,
        change: impl FnOnce(&mut Settings),
        read_autostart: impl FnOnce() -> AppResult<bool>,
        set_autostart: impl FnMut(bool) -> AppResult<()>,
        then: impl FnOnce(&Settings),
    ) -> AppResult<Settings> {
        let mut current = write(&self.current);
        let mut next = current.clone();
        change(&mut next);
        self.commit(&mut current, next, read_autostart, set_autostart, then)
    }

    fn commit(
        &self,
        current: &mut Settings,
        next: Settings,
        read_autostart: impl FnOnce() -> AppResult<bool>,
        mut set_autostart: impl FnMut(bool) -> AppResult<()>,
        then: impl FnOnce(&Settings),
    ) -> AppResult<Settings> {
        next.validate()?;
        let previous_autostart = read_autostart()?;
        let changed = previous_autostart != next.start_with_os;
        if changed {
            set_autostart(next.start_with_os)?;
        }
        if let Some(path) = &self.path
            && let Err(error) = write_atomic(path, &next)
        {
            if changed && let Err(rollback) = set_autostart(previous_autostart) {
                return Err(AppError::SystemQueryFailed(format!(
                    "{error}; restoring the previous login setting also failed: {rollback}"
                )));
            }
            return Err(error);
        }
        *current = next.clone();
        then(&next);
        Ok(next)
    }
}

fn read_or_default(path: &Path) -> Settings {
    let text = match tm_core::filesystem::read_text(path, 256 * 1024) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            // Unreadable (permissions, too large, not a regular file). Move it
            // aside like a corrupt file so the next save cannot destroy it.
            tracing::warn!("could not read settings: {e}; using defaults");
            keep_aside(path);
            return Settings::default();
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(settings) => settings.sanitized(),
        Err(e) => {
            tracing::warn!("settings file is invalid ({e}); using defaults");
            keep_aside(path);
            Settings::default()
        }
    }
}

fn keep_aside(path: &Path) {
    let _ = std::fs::rename(path, path.with_extension("json.invalid"));
}

fn write_atomic(path: &Path, settings: &Settings) -> AppResult<()> {
    let fail = |e: &dyn std::fmt::Display| {
        AppError::SystemQueryFailed(format!("could not save settings: {e}"))
    };
    let json = serde_json::to_string_pretty(settings).map_err(|e| fail(&e))?;
    tm_core::filesystem::write_private_atomic(path, json.as_bytes()).map_err(|e| fail(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_core::models::ThemePreference;

    fn save(store: &SettingsStore, next: Settings) -> AppResult<Settings> {
        let previous = store.get().start_with_os;
        store.update_with_autostart(|s| *s = next, || Ok(previous), |_| Ok(()), |_| {})
    }

    #[test]
    fn concurrent_updates_are_not_lost_and_effects_follow_save_order() {
        let store = std::sync::Arc::new(SettingsStore::load(None));
        let applied = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let (store, applied) = (store.clone(), applied.clone());
                std::thread::spawn(move || {
                    store
                        .update(
                            |s| {
                                let n = s.new_process_seconds;
                                std::thread::yield_now();
                                s.new_process_seconds = n + 1;
                                s.show_kernel_threads = i % 2 == 0;
                            },
                            |saved| applied.lock().unwrap().push(saved.new_process_seconds),
                        )
                        .unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let start = Settings::default().new_process_seconds;
        assert_eq!(store.get().new_process_seconds, start + 8);
        let applied = applied.lock().unwrap();
        assert_eq!(*applied, (start + 1..=start + 8).collect::<Vec<_>>());
    }

    #[test]
    fn a_rejected_update_changes_nothing_and_skips_effects() {
        let store = SettingsStore::load(None);
        let mut ran = false;
        let result = store.update(|s| s.refresh_interval_ms = 7, |_| ran = true);
        assert!(result.is_err());
        assert!(!ran);
        assert_eq!(store.get(), Settings::default());
    }

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
        save(&store, s).unwrap();
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
        assert!(save(&store, bad).is_err());
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn unreadable_file_is_preserved_not_overwritten() {
        let dir = temp_dir("oversized");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), vec![b' '; 300 * 1024]).unwrap();
        let store = SettingsStore::load(Some(&dir));
        assert_eq!(store.get(), Settings::default());
        save(&store, Settings::default()).unwrap();
        assert!(dir.join("settings.json.invalid").exists());
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn failed_persistence_restores_actual_autostart_and_memory() {
        let root = temp_dir("autostart-rollback");
        std::fs::create_dir_all(&root).unwrap();
        let blocker = root.join("blocker");
        std::fs::write(&blocker, "keep").unwrap();
        let store = SettingsStore::load(Some(&blocker));
        let enabled = std::cell::Cell::new(false);
        let next = Settings {
            start_with_os: true,
            ..Settings::default()
        };
        let result = store.update_with_autostart(
            |s| *s = next,
            || Ok(enabled.get()),
            |value| {
                enabled.set(value);
                Ok(())
            },
            |_| {},
        );
        assert!(result.is_err());
        assert!(!enabled.get());
        assert_eq!(store.get(), Settings::default());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn actual_autostart_is_used_even_when_the_stored_preference_disagrees() {
        let store = SettingsStore::load(None);
        let enabled = std::cell::Cell::new(true);
        store
            .update_with_autostart(
                |s| *s = Settings::default(),
                || Ok(enabled.get()),
                |value| {
                    enabled.set(value);
                    Ok(())
                },
                |_| {},
            )
            .unwrap();
        assert!(!enabled.get());
    }
}
