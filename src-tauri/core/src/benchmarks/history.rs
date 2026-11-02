//! Local benchmark history: one JSON file per completed run.
//!
//! * Files are written to a temp name and renamed, so a crash never leaves
//!   a half-written record that looks valid.
//! * Only ids this module generated (UUIDs) are accepted as file names, so
//!   an id from the frontend can never escape the history directory.
//! * Unreadable or foreign files are skipped (and logged), not deleted.
//! * At most [`MAX_RECORDS`] runs are kept; the oldest are pruned.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::models::{BenchmarkRecord, BenchmarkSummary};
use crate::errors::{AppError, AppResult};

pub const MAX_RECORDS: usize = 500;
/// Records larger than this are refused when reading (corruption guard).
const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;

pub struct HistoryStore {
    dir: Option<PathBuf>,
    /// Summaries, newest first; loaded lazily on first use.
    cache: Mutex<Option<Vec<BenchmarkSummary>>>,
}

/// `8-4-4-4-12` lowercase hex, as produced by `uuid::Uuid::new_v4()`.
pub fn is_valid_id(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, len)| {
            g.len() == len && g.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

pub fn validate_id(id: &str) -> AppResult<&str> {
    if is_valid_id(id) {
        Ok(id)
    } else {
        Err(AppError::InvalidArgument(
            "Unknown benchmark result.".into(),
        ))
    }
}

impl HistoryStore {
    /// `dir = None` keeps history disabled (nothing is read or written).
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            cache: Mutex::new(None),
        }
    }

    fn path(dir: &Path, id: &str) -> PathBuf {
        dir.join(format!("{id}.json"))
    }

    fn with_cache<T>(&self, f: impl FnOnce(&mut Vec<BenchmarkSummary>) -> T) -> T {
        let mut guard = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let cache = guard.get_or_insert_with(|| self.load_summaries());
        f(cache)
    }

    fn load_summaries(&self) -> Vec<BenchmarkSummary> {
        let Some(dir) = &self.dir else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut out: Vec<BenchmarkSummary> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let id = name.strip_suffix(".json")?;
                if !is_valid_id(id) {
                    return None;
                }
                match read_record(&e.path()) {
                    Ok(r) if r.id == id => Some(BenchmarkSummary::from(&r)),
                    Ok(_) => None,
                    Err(err) => {
                        tracing::warn!("skipping unreadable benchmark record {id}: {err}");
                        None
                    }
                }
            })
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.started_at_ms));
        out
    }

    pub fn list(&self) -> Vec<BenchmarkSummary> {
        self.with_cache(|c| c.clone())
    }

    pub fn get(&self, id: &str) -> AppResult<BenchmarkRecord> {
        let id = validate_id(id)?;
        let dir = self.dir.as_ref().ok_or_else(not_found)?;
        read_record(&Self::path(dir, id)).map_err(|_| not_found())
    }

    pub fn save(&self, record: &BenchmarkRecord) -> AppResult<()> {
        let id = validate_id(&record.id)?;
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        fs::create_dir_all(dir).map_err(io_err)?;
        let json = serde_json::to_vec(record)
            .map_err(|e| AppError::SystemQueryFailed(format!("cannot serialize result: {e}")))?;
        let tmp = dir.join(format!("{id}.json.tmp"));
        fs::write(&tmp, json).map_err(io_err)?;
        if let Err(e) = fs::rename(&tmp, Self::path(dir, id)) {
            let _ = fs::remove_file(&tmp);
            return Err(io_err(e));
        }
        let pruned = self.with_cache(|c| {
            c.retain(|s| s.id != record.id);
            c.insert(0, BenchmarkSummary::from(record));
            c.sort_by_key(|s| std::cmp::Reverse(s.started_at_ms));
            if c.len() > MAX_RECORDS {
                c.split_off(MAX_RECORDS)
            } else {
                Vec::new()
            }
        });
        for old in pruned {
            let _ = fs::remove_file(Self::path(dir, &old.id));
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let id = validate_id(id)?;
        let dir = self.dir.as_ref().ok_or_else(not_found)?;
        match fs::remove_file(Self::path(dir, id)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(e)),
        }
        self.with_cache(|c| c.retain(|s| s.id != id));
        Ok(())
    }
}

fn read_record(path: &Path) -> Result<BenchmarkRecord, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_RECORD_BYTES {
        return Err("record too large".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn not_found() -> AppError {
    AppError::InvalidArgument("That benchmark result no longer exists.".into())
}

fn io_err(e: std::io::Error) -> AppError {
    AppError::SystemQueryFailed(format!("cannot access benchmark history: {e}"))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::benchmarks::models::*;
    use crate::benchmarks::{METHOD_VERSION, SCHEMA_VERSION, SCORE_VERSION};

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tm-bench-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn record(started_at_ms: u64) -> BenchmarkRecord {
        BenchmarkRecord {
            schema_version: SCHEMA_VERSION,
            method_version: METHOD_VERSION.into(),
            score_version: SCORE_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            started_at_ms,
            duration_ms: 1_000,
            app_version: "0.1.0".into(),
            profile: BenchmarkProfile::Quick,
            kinds: vec![BenchmarkKind::Cpu],
            simulated: true,
            environment_before: EnvironmentSnapshot::default(),
            environment_after: None,
            fingerprint: HardwareFingerprint::default(),
            categories: vec![CategoryResult::empty(
                BenchmarkKind::Cpu,
                CategoryStatus::Completed,
            )],
            system_score: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn validates_ids_strictly() {
        assert!(is_valid_id(&uuid::Uuid::new_v4().to_string()));
        for bad in [
            "",
            "../../etc/passwd",
            "ABCDEF01-0000-0000-0000-000000000000",
            "00000000-0000-0000-0000-00000000000",
            "00000000-0000-0000-0000-000000000000/..",
            "00000000_0000_0000_0000_000000000000",
        ] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn save_list_get_delete_round_trip() {
        let dir = temp_dir("roundtrip");
        let store = HistoryStore::new(Some(dir.clone()));
        let old = record(1_000);
        let new = record(2_000);
        store.save(&old).unwrap();
        store.save(&new).unwrap();
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, new.id, "newest first");
        assert_eq!(store.get(&old.id).unwrap(), old);

        // A fresh store reads the same data from disk.
        let reopened = HistoryStore::new(Some(dir.clone()));
        assert_eq!(reopened.list().len(), 2);

        store.delete(&old.id).unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(store.get(&old.id).is_err());
        assert!(!dir.join(format!("{}.json.tmp", new.id)).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn skips_corrupt_and_foreign_files_without_deleting_them() {
        let dir = temp_dir("corrupt");
        let id = uuid::Uuid::new_v4().to_string();
        fs::write(dir.join(format!("{id}.json")), b"{ not json").unwrap();
        fs::write(dir.join("notes.txt"), b"user file").unwrap();
        let store = HistoryStore::new(Some(dir.clone()));
        assert!(store.list().is_empty());
        assert!(dir.join("notes.txt").exists());
        assert!(dir.join(format!("{id}.json")).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let store = HistoryStore::new(Some(temp_dir("traversal")));
        assert!(store.get("../settings").is_err());
        assert!(store.delete("..\\x").is_err());
    }

    #[test]
    fn disabled_store_is_a_no_op() {
        let store = HistoryStore::new(None);
        store.save(&record(1)).unwrap();
        assert!(store.list().is_empty());
    }
}
