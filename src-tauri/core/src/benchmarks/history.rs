//! Saved benchmark results: one JSON file in the app data directory.
//!
//! Only completed runs are stored, so a cancelled or failed run never appears
//! as a result. Writes are atomic (temp file + rename). Raw measurements are
//! kept with every result, so scores can be recomputed later. A corrupt file
//! is moved aside rather than overwritten.
//!
//! ponytail: one JSON file, rewritten on each save. Fine for 200 results of a
//! few KB; switch to SQLite if history ever needs querying or much more depth.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use super::model::BenchmarkResult;
use crate::errors::{AppError, AppResult};

const FILE_NAME: &str = "benchmarks.json";
const FORMAT_VERSION: u32 = 1;
/// Oldest results are dropped beyond this.
pub const MAX_RESULTS: usize = 200;

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    /// Newest first.
    results: Vec<BenchmarkResult>,
}

pub struct HistoryStore {
    path: Option<PathBuf>,
    results: Mutex<Vec<BenchmarkResult>>,
}

impl HistoryStore {
    pub fn load(dir: Option<&Path>) -> Self {
        let path = dir.map(|d| d.join(FILE_NAME));
        let results = path.as_deref().map(read_or_empty).unwrap_or_default();
        Self {
            path,
            results: Mutex::new(results),
        }
    }

    /// Newest first.
    pub fn list(&self) -> Vec<BenchmarkResult> {
        self.lock().clone()
    }

    pub fn get(&self, id: &str) -> Option<BenchmarkResult> {
        self.lock().iter().find(|r| r.id == id).cloned()
    }

    /// Saves to disk first, then updates memory, so memory never claims a
    /// result the disk does not have.
    pub fn add(&self, result: BenchmarkResult) -> AppResult<()> {
        let mut current = self.lock();
        let mut next = current.clone();
        next.insert(0, result);
        next.truncate(MAX_RESULTS);
        self.persist(&next)?;
        *current = next;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let mut current = self.lock();
        let mut next = current.clone();
        next.retain(|r| r.id != id);
        self.persist(&next)?;
        *current = next;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<BenchmarkResult>> {
        self.results.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn persist(&self, results: &[BenchmarkResult]) -> AppResult<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let file = File {
            version: FORMAT_VERSION,
            results: results.to_vec(),
        };
        let json = serde_json::to_string(&file)
            .map_err(|e| AppError::SystemQueryFailed(format!("cannot encode history: {e}")))?;
        write_atomic(path, &json)
    }
}

fn read_or_empty(path: &Path) -> Vec<BenchmarkResult> {
    let text = match crate::filesystem::read_text(path, 8 * 1024 * 1024) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!("could not read benchmark history: {e}");
            return Vec::new();
        }
    };
    match serde_json::from_str::<File>(&text) {
        Ok(mut f) if f.version == FORMAT_VERSION => {
            f.results.truncate(MAX_RESULTS);
            f.results
        }
        _ => {
            tracing::warn!("benchmark history is unreadable; keeping a copy and starting empty");
            let _ = std::fs::rename(path, path.with_extension("json.invalid"));
            Vec::new()
        }
    }
}

fn write_atomic(path: &Path, json: &str) -> AppResult<()> {
    let fail = |e: std::io::Error| {
        AppError::SystemQueryFailed(format!("cannot save benchmark history: {e}"))
    };
    crate::filesystem::write_private_atomic(path, json.as_bytes()).map_err(fail)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::benchmarks::model::*;
    use crate::benchmarks::scoring::cpu_method;
    use crate::models::Platform;

    pub fn sample(id: &str) -> BenchmarkResult {
        BenchmarkResult {
            id: id.into(),
            kind: BenchmarkKind::Cpu,
            profile: BenchmarkProfile::Quick,
            started_at_ms: 1,
            duration_ms: 2,
            method: cpu_method(),
            environment: Environment {
                platform: Platform::Linux,
                distribution: None,
                kernel: None,
                architecture: "x86_64".into(),
                cpu_model: "Test".into(),
                physical_cores: None,
                logical_cores: 4,
                total_memory_bytes: 1,
                power_source: PowerSource::Unknown,
                cpu_governor: None,
                virtualization: None,
                background_cpu_percent: None,
                development_build: false,
            },
            workloads: vec![],
            sustained: None,
            gpu: None,
            gpu_score: None,
            disk: None,
            disk_score: None,
            internet: None,
            scores: Some(CpuScores {
                single_core: 1000,
                multi_core: 2000,
            }),
            warnings: vec![],
            stability: Stability::High,
            outlier_method: "flagged, kept".into(),
            affinity: "not pinned".into(),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tm-bench-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn saves_newest_first_and_survives_a_restart() {
        let dir = temp_dir("roundtrip");
        let store = HistoryStore::load(Some(&dir));
        store.add(sample("a")).unwrap();
        store.add(sample("b")).unwrap();
        assert_eq!(
            store
                .list()
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );

        let again = HistoryStore::load(Some(&dir));
        assert_eq!(again.list(), store.list());
        assert!(again.get("a").is_some());
        again.delete("a").unwrap();
        assert_eq!(HistoryStore::load(Some(&dir)).list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_saved_before_gpu_support_still_loads() {
        let dir = temp_dir("old-format");
        std::fs::create_dir_all(&dir).unwrap();
        let mut old = serde_json::to_value(sample("old")).unwrap();
        let fields = old.as_object_mut().unwrap();
        // Results written by the CPU-only version have neither field.
        assert!(fields.remove("gpu").is_some());
        assert!(fields.remove("gpuScore").is_some());
        let file = serde_json::json!({ "version": 1, "results": [old] });
        std::fs::write(dir.join(FILE_NAME), file.to_string()).unwrap();
        let store = HistoryStore::load(Some(&dir));
        assert_eq!(store.list().len(), 1);
        assert!(store.list()[0].gpu.is_none() && store.list()[0].gpu_score.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_is_bounded() {
        let store = HistoryStore::load(None);
        for i in 0..MAX_RESULTS + 5 {
            store.add(sample(&format!("r{i}"))).unwrap();
        }
        assert_eq!(store.list().len(), MAX_RESULTS);
        assert_eq!(store.list()[0].id, format!("r{}", MAX_RESULTS + 4));
    }

    #[test]
    fn concurrent_updates_preserve_every_result_on_disk_and_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::load(Some(dir.path()));
        std::thread::scope(|scope| {
            for i in 0..8 {
                let store = &store;
                scope.spawn(move || store.add(sample(&format!("r{i}"))).unwrap());
            }
        });
        assert_eq!(store.list().len(), 8);
        assert_eq!(HistoryStore::load(Some(dir.path())).list(), store.list());
    }

    #[test]
    fn loaded_history_also_obeys_the_retention_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = File {
            version: FORMAT_VERSION,
            results: (0..MAX_RESULTS + 5)
                .map(|i| sample(&format!("r{i}")))
                .collect(),
        };
        std::fs::write(
            dir.path().join(FILE_NAME),
            serde_json::to_vec(&file).unwrap(),
        )
        .unwrap();
        assert_eq!(
            HistoryStore::load(Some(dir.path())).list().len(),
            MAX_RESULTS
        );
    }

    #[test]
    fn a_corrupt_file_is_kept_aside_and_history_starts_empty() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        let store = HistoryStore::load(Some(&dir));
        assert!(store.list().is_empty());
        assert!(dir.join("benchmarks.json.invalid").exists());
        // And it is usable afterwards.
        store.add(sample("x")).unwrap();
        assert_eq!(HistoryStore::load(Some(&dir)).list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_does_not_change_memory() {
        // The "directory" is a file, so creating it fails.
        let dir = temp_dir("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("file");
        std::fs::write(&blocker, "x").unwrap();
        let store = HistoryStore::load(Some(&blocker));
        assert!(store.add(sample("a")).is_err());
        assert!(store.list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
