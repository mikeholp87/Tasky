//! Short per-process history so the details page can show a graph the
//! moment it opens. Bounded per process; entries for exited processes are
//! dropped on the next tick.

use std::collections::HashMap;

use crate::models::{ProcessPoint, ProcessRow};
use crate::monitoring::history::RingBuffer;

/// Five minutes at 1 s, 2.5 minutes at 500 ms.
pub const PROCESS_HISTORY_POINTS: usize = 300;
const MAX_TRACKED_PROCESSES: usize = 4_096;

/// Keyed by (pid, start time) so a reused PID never inherits another
/// process's history.
type Key = (u32, u64);

struct Tracked {
    points: RingBuffer<ProcessPoint>,
    /// The `record` call that last saw this process.
    seen: u64,
}

#[derive(Default)]
pub struct ProcessHistory {
    map: HashMap<Key, Tracked>,
    generation: u64,
}

impl ProcessHistory {
    pub fn record(&mut self, t: u64, rows: &[ProcessRow]) {
        self.generation += 1;
        let generation = self.generation;
        for row in rows.iter().take(MAX_TRACKED_PROCESSES) {
            let tracked = self
                .map
                .entry((row.pid, row.start_time))
                .or_insert_with(|| Tracked {
                    points: RingBuffer::new(PROCESS_HISTORY_POINTS),
                    seen: generation,
                });
            tracked.seen = generation;
            tracked.points.push(ProcessPoint {
                t,
                cpu: row.cpu_percent,
                memory: row.memory_bytes,
                read: row.disk_read_per_sec,
                write: row.disk_write_per_sec,
            });
        }
        // Anything not seen this tick has exited.
        self.map.retain(|_, tracked| tracked.seen == generation);
    }

    pub fn get(&self, pid: u32, start_time: u64) -> Vec<ProcessPoint> {
        self.map
            .get(&(pid, start_time))
            .map(|tracked| tracked.points.iter().copied().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProcessState;

    fn row(pid: u32, start: u64, cpu: f32) -> ProcessRow {
        ProcessRow {
            pid,
            parent_pid: None,
            name: "p".into(),
            status: ProcessState::Running,
            cpu_percent: cpu,
            memory_bytes: 1,
            virtual_memory_bytes: 1,
            disk_read_per_sec: 0.0,
            disk_write_per_sec: 0.0,
            user: None,
            start_time: start,
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
    fn accumulates_and_prunes_exited_processes() {
        let mut h = ProcessHistory::default();
        h.record(1, &[row(10, 100, 1.0), row(20, 100, 2.0)]);
        h.record(2, &[row(10, 100, 3.0)]);
        assert_eq!(
            h.get(10, 100).iter().map(|p| p.cpu).collect::<Vec<_>>(),
            [1.0, 3.0]
        );
        assert!(h.get(20, 100).is_empty(), "exited process is pruned");
    }

    #[test]
    fn total_process_history_has_a_global_memory_bound() {
        let mut h = ProcessHistory::default();
        let rows: Vec<_> = (0..MAX_TRACKED_PROCESSES + 100)
            .map(|i| row(i as u32 + 1, 100, 0.0))
            .collect();
        h.record(1, &rows);
        assert_eq!(h.map.len(), MAX_TRACKED_PROCESSES);
    }

    #[test]
    fn reused_pid_starts_fresh() {
        let mut h = ProcessHistory::default();
        h.record(1, &[row(10, 100, 50.0)]);
        h.record(2, &[row(10, 999, 1.0)]);
        assert_eq!(h.get(10, 999).len(), 1);
        assert!(h.get(10, 100).is_empty());
    }
}
