//! Proportional set size (PSS) per process, sampled only while the UI shows
//! the Memory page.
//!
//! RSS counts a shared page once for every process that maps it, so summing
//! RSS over an application's processes (a browser's dozens of renderers)
//! overstates its memory badly. PSS divides each shared page between its
//! users, so PSS sums to what the application really occupies.
//!
//! Reading `/proc/[pid]/smaps_rollup` makes the kernel walk the process's
//! page tables: milliseconds for large processes, and 400+ ms for a full
//! system. So reads are spread over ticks with a time budget, biggest
//! processes first, and values are reused until they go stale. Processes
//! the kernel refuses to describe (other users') are retried rarely.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::models::{ProcessRow, ProcessState};

/// Maximum time spent reading per tick.
pub const BUDGET: Duration = Duration::from_millis(25);
/// A successful reading is reused for this long.
const FRESH_FOR: Duration = Duration::from_secs(10);
/// A refused reading is not retried for this long.
const RETRY_DENIED_AFTER: Duration = Duration::from_secs(300);

struct Entry {
    pss: Option<u64>,
    read_at: Instant,
}

#[derive(Default)]
pub struct MemoryDetailTracker {
    /// Keyed by (pid, start time) so a reused PID never inherits a reading.
    cache: HashMap<(u32, u64), Entry>,
}

impl MemoryDetailTracker {
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Reads PSS for the most useful processes within `budget`, then stamps
    /// every row with its cached value (or `None` when unknown).
    pub fn update(
        &mut self,
        rows: &mut [ProcessRow],
        now: Instant,
        budget: Duration,
        read: impl Fn(u32) -> Option<u64>,
    ) {
        let live: std::collections::HashSet<(u32, u64)> =
            rows.iter().map(|r| (r.pid, r.start_time)).collect();
        self.cache.retain(|key, _| live.contains(key));

        // Never-read first (largest first), then the stalest readings.
        let mut due: Vec<(&ProcessRow, Option<Instant>)> = rows
            .iter()
            .filter(|r| !r.kernel_thread && r.status != ProcessState::Zombie)
            .filter_map(|r| {
                let seen = self.cache.get(&(r.pid, r.start_time));
                let needs_read = match seen {
                    None => true,
                    Some(e) => {
                        let ttl = if e.pss.is_some() {
                            FRESH_FOR
                        } else {
                            RETRY_DENIED_AFTER
                        };
                        now.duration_since(e.read_at) >= ttl
                    }
                };
                needs_read.then_some((r, seen.map(|e| e.read_at)))
            })
            .collect();
        due.sort_by(|(a, a_seen), (b, b_seen)| match (a_seen, b_seen) {
            (None, None) => b.memory_bytes.cmp(&a.memory_bytes),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => x.cmp(y),
        });

        let started = Instant::now();
        let mut reads = Vec::new();
        for (row, _) in due {
            // Always make progress, even if one read exceeds the budget.
            if !reads.is_empty() && started.elapsed() >= budget {
                break;
            }
            reads.push(((row.pid, row.start_time), read(row.pid)));
        }
        for (key, pss) in reads {
            self.cache.insert(key, Entry { pss, read_at: now });
        }

        for row in rows.iter_mut() {
            row.pss_bytes = self
                .cache
                .get(&(row.pid, row.start_time))
                .and_then(|e| e.pss);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn row(pid: u32, memory: u64) -> ProcessRow {
        ProcessRow {
            pid,
            parent_pid: None,
            name: format!("p{pid}"),
            status: ProcessState::Sleeping,
            cpu_percent: 0.0,
            memory_bytes: memory,
            virtual_memory_bytes: 0,
            disk_read_per_sec: 0.0,
            disk_write_per_sec: 0.0,
            user: None,
            start_time: 1,
            threads: None,
            priority: None,
            exe: None,
            cmd: None,
            session_id: None,
            kernel_thread: false,
            pss_bytes: None,
            restricted: false,
        }
    }

    #[test]
    fn reads_largest_first_within_budget_and_reuses_fresh_values() {
        let mut t = MemoryDetailTracker::default();
        let mut rows = vec![row(1, 10), row(2, 900), row(3, 500)];
        let calls = RefCell::new(Vec::new());
        let read = |pid: u32| {
            calls.borrow_mut().push(pid);
            Some(u64::from(pid) * 100)
        };
        let now = Instant::now();

        // A zero budget still reads one process: the largest.
        t.update(&mut rows, now, Duration::ZERO, read);
        assert_eq!(*calls.borrow(), vec![2]);
        assert_eq!(rows[1].pss_bytes, Some(200));
        assert_eq!(rows[0].pss_bytes, None);

        // The next tick continues with the rest and does not re-read pid 2.
        calls.borrow_mut().clear();
        t.update(&mut rows, now, Duration::ZERO, read);
        assert_eq!(*calls.borrow(), vec![3]);
        t.update(&mut rows, now, Duration::from_secs(1), read);
        assert_eq!(*calls.borrow(), vec![3, 1]);
        assert!(rows.iter().all(|r| r.pss_bytes.is_some()));

        // Fresh values are not re-read.
        calls.borrow_mut().clear();
        t.update(&mut rows, now, Duration::from_secs(1), read);
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn stale_values_are_refreshed_oldest_first() {
        let mut t = MemoryDetailTracker::default();
        let mut rows = vec![row(1, 1), row(2, 1)];
        let t0 = Instant::now();
        t.update(&mut rows, t0, Duration::from_secs(1), |_| Some(1));
        let calls = RefCell::new(Vec::new());
        t.cache.get_mut(&(1, 1)).unwrap().read_at = t0; // oldest
        t.cache.get_mut(&(2, 1)).unwrap().read_at = t0 + Duration::from_secs(5);
        let later = t0 + FRESH_FOR + Duration::from_secs(6);
        t.update(&mut rows, later, Duration::from_secs(1), |pid| {
            calls.borrow_mut().push(pid);
            Some(2)
        });
        assert_eq!(*calls.borrow(), vec![1, 2]);
    }

    #[test]
    fn refused_reads_back_off_and_do_not_block_others() {
        let mut t = MemoryDetailTracker::default();
        let mut rows = vec![row(1, 100), row(2, 50)];
        let calls = RefCell::new(Vec::new());
        let read = |pid: u32| {
            calls.borrow_mut().push(pid);
            (pid == 2).then_some(7)
        };
        let now = Instant::now();
        t.update(&mut rows, now, Duration::from_secs(1), read);
        assert_eq!(rows[0].pss_bytes, None);
        assert_eq!(rows[1].pss_bytes, Some(7));

        // Fresh for 10 s, but the refused one waits five minutes.
        calls.borrow_mut().clear();
        let soon = now + FRESH_FOR + Duration::from_secs(1);
        t.update(&mut rows, soon, Duration::from_secs(1), read);
        assert_eq!(*calls.borrow(), vec![2]);
    }

    #[test]
    fn drops_exited_and_reused_pids_and_skips_kernel_threads_and_zombies() {
        let mut t = MemoryDetailTracker::default();
        let mut kthread = row(9, 1_000);
        kthread.kernel_thread = true;
        let mut zombie = row(8, 1_000);
        zombie.status = ProcessState::Zombie;
        let mut rows = vec![row(1, 10), kthread, zombie];
        let now = Instant::now();
        t.update(&mut rows, now, Duration::from_secs(1), |_| Some(5));
        assert_eq!(rows[0].pss_bytes, Some(5));
        assert_eq!(rows[1].pss_bytes, None);
        assert_eq!(rows[2].pss_bytes, None);

        // Same PID, new start time: a different process, so no inherited value.
        let mut reused = row(1, 10);
        reused.start_time = 2;
        let mut rows = vec![reused];
        t.update(&mut rows, now, Duration::ZERO, |_| None);
        assert_eq!(rows[0].pss_bytes, None);
        assert_eq!(t.cache.len(), 1);
    }
}
