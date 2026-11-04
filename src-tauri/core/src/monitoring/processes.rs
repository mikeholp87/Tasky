//! Process enumeration. Static per-process fields (exe, command line, user)
//! are read once per PID; CPU, memory and I/O are refreshed every tick.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, Uid, UpdateKind, Users};

use crate::models::{ProcessRow, ProcessState};
use crate::monitoring::rates::delta_rate;
use crate::platform;

/// Re-read the account list occasionally so new logins resolve.
const USERS_REFRESH: Duration = Duration::from_secs(300);

pub fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        // On Linux, tasks would list every thread as its own process.
        .without_tasks()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet)
}

pub struct ProcessTracker {
    users: UserResolver,
}

impl ProcessTracker {
    pub fn new() -> Self {
        Self {
            users: UserResolver::new(),
        }
    }

    /// Refreshes processes in `sys` and converts them to rows. `logical_cores`
    /// normalizes per-process CPU to a share of the whole machine.
    pub fn sample(&mut self, sys: &mut System, elapsed: Duration) -> Vec<ProcessRow> {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        self.users.maybe_refresh();

        let cores = sys.cpus().len().max(1) as f32;
        let pids: Vec<u32> = sys.processes().keys().map(|p| p.as_u32()).collect();
        let extras = platform::process_extras(&pids);

        sys.processes()
            .values()
            // Linux: never show individual threads.
            .filter(|p| p.thread_kind() != Some(sysinfo::ThreadKind::Userland))
            // Exited but not yet reaped by the parent: only a process-table
            // entry remains (no memory, no code running). Showing it makes a
            // closed app look alive, so treat it as gone.
            .filter(|p| !is_gone(ProcessState::from(p.status())))
            .map(|p| {
                let pid = p.pid().as_u32();
                let cpu_percent = normalize_cpu(p.cpu_usage(), cores);
                let extra = extras.get(&pid).copied().unwrap_or_default();
                // Per-process reads fail for protected processes when unelevated
                // and sysinfo reports zeros; a handle-free snapshot still has
                // their real memory. CPU and I/O stay unknown for them.
                let restricted = p.memory() == 0 && extra.memory_bytes.is_some_and(|m| m > 0);
                let disk = p.disk_usage();
                ProcessRow {
                    pid,
                    parent_pid: p.parent().map(|pp| pp.as_u32()),
                    name: p.name().to_string_lossy().into_owned(),
                    status: effective_state(
                        ProcessState::from(p.status()),
                        cpu_percent,
                        restricted,
                    ),
                    cpu_percent,
                    memory_bytes: if restricted {
                        extra.memory_bytes.unwrap_or(0)
                    } else {
                        p.memory()
                    },
                    virtual_memory_bytes: if restricted {
                        extra.virtual_memory_bytes.unwrap_or(0)
                    } else {
                        p.virtual_memory()
                    },
                    restricted,
                    disk_read_per_sec: delta_rate(disk.read_bytes, elapsed),
                    disk_write_per_sec: delta_rate(disk.written_bytes, elapsed),
                    user: p.user_id().and_then(|uid| self.users.name(uid)),
                    start_time: p.start_time(),
                    threads: extra.threads,
                    priority: extra.priority,
                    exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
                    cmd: join_cmd(p.cmd()),
                    session_id: p.session_id().map(|s| s.as_u32()),
                    kernel_thread: p.thread_kind() == Some(sysinfo::ThreadKind::Kernel),
                    pss_bytes: None,
                }
            })
            .collect()
    }
}

/// Machine-wide CPU share (percent) at or above which a process counts as
/// having run during the last interval: about one percent of a single core on
/// a 12-thread machine.
pub const RAN_CPU_PERCENT: f32 = 0.1;

/// The state shown to the user.
///
/// The kernel reports the state at one instant. A busy process that waits for
/// a moment (a browser tab, a compiler, a game) is in the sleeping state at
/// almost every sample, so the raw value would label nearly everything
/// "Sleeping". A process that used CPU during the interval did run, so that is
/// shown as Running. Suspended, blocked-on-disk and traced states are kept as
/// reported, and when CPU is unknown (restricted processes) so is the raw state.
pub fn effective_state(state: ProcessState, cpu_percent: f32, restricted: bool) -> ProcessState {
    match state {
        ProcessState::Sleeping | ProcessState::Idle
            if !restricted && cpu_percent >= RAN_CPU_PERCENT =>
        {
            ProcessState::Running
        }
        other => other,
    }
}

fn is_gone(state: ProcessState) -> bool {
    matches!(state, ProcessState::Zombie | ProcessState::Dead)
}

/// sysinfo reports per-process CPU as a percentage of one core (0..N*100).
/// Divide by the core count so values sum to at most 100, matching the
/// machine-wide figure.
pub fn normalize_cpu(raw: f32, cores: f32) -> f32 {
    if !raw.is_finite() || cores <= 0.0 {
        return 0.0;
    }
    (raw / cores).clamp(0.0, 100.0)
}

fn join_cmd(parts: &[std::ffi::OsString]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    let joined = parts
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    Some(joined)
}

/// Maps user ids to names, caching both hits and misses.
struct UserResolver {
    users: Option<Users>,
    cache: HashMap<Uid, Option<String>>,
    refreshed: Instant,
}

impl UserResolver {
    fn new() -> Self {
        Self {
            users: Users::new_with_refreshed_list()
                .inspect_err(|e| tracing::debug!("user list unavailable: {e}"))
                .ok(),
            cache: HashMap::new(),
            refreshed: Instant::now(),
        }
    }

    fn maybe_refresh(&mut self) {
        if self.refreshed.elapsed() < USERS_REFRESH {
            return;
        }
        if let Some(users) = self.users.as_mut() {
            users.refresh();
        }
        // Retry previously unresolved ids against the new list.
        self.cache.retain(|_, name| name.is_some());
        self.refreshed = Instant::now();
    }

    fn name(&mut self, uid: &Uid) -> Option<String> {
        if let Some(cached) = self.cache.get(uid) {
            return cached.clone();
        }
        let resolved = self
            .users
            .as_ref()
            .and_then(|u| u.get_user_by_id(uid))
            .map(|u| u.name().to_owned())
            .or_else(|| platform::account_name(uid));
        self.cache.insert(uid.clone(), resolved.clone());
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_is_normalized_to_machine_capacity() {
        assert_eq!(normalize_cpu(200.0, 8.0), 25.0);
        assert_eq!(normalize_cpu(1600.0, 8.0), 100.0);
        assert_eq!(normalize_cpu(f32::NAN, 8.0), 0.0);
        assert_eq!(normalize_cpu(50.0, 0.0), 0.0);
    }

    #[test]
    fn a_process_that_used_cpu_is_shown_as_running_even_when_sampled_asleep() {
        use ProcessState::*;
        assert_eq!(effective_state(Sleeping, 3.0, false), Running);
        assert_eq!(effective_state(Idle, 0.1, false), Running);
        // Waiting, with no measurable CPU.
        assert_eq!(effective_state(Sleeping, 0.0, false), Sleeping);
        assert_eq!(effective_state(Sleeping, 0.09, false), Sleeping);
        assert_eq!(effective_state(Idle, 0.0, false), Idle);
        // States that mean something else are never overridden.
        for s in [Stopped, DiskSleep, Tracing, Zombie, Dead, Unknown] {
            assert_eq!(effective_state(s, 50.0, false), s);
        }
        // CPU is unknown, not zero or high, for a restricted process.
        assert_eq!(effective_state(Sleeping, 9.0, true), Sleeping);
        assert_eq!(effective_state(Running, 0.0, false), Running);
    }

    /// A waiting child is Sleeping, a spinning one is Running, in real rows.
    #[cfg(target_os = "linux")]
    #[test]
    fn real_busy_and_waiting_processes_get_the_right_state() {
        let mut waiting = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let mut busy = std::process::Command::new("sh")
            .args(["-c", "while :; do :; done"])
            .spawn()
            .unwrap();
        let mut sys = System::new().unwrap();
        let mut tracker = ProcessTracker::new();
        tracker.sample(&mut sys, Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(1200));
        let rows = tracker.sample(&mut sys, Duration::from_millis(1200));
        let _ = (waiting.kill(), busy.kill());
        let _ = (waiting.wait(), busy.wait());
        let state = |pid: u32| rows.iter().find(|r| r.pid == pid).map(|r| r.status);
        assert_eq!(state(waiting.id()), Some(ProcessState::Sleeping));
        assert_eq!(state(busy.id()), Some(ProcessState::Running));
    }

    #[test]
    fn joins_command_line() {
        let parts = vec!["node".into(), "server.js".into(), "--port=3000".into()];
        assert_eq!(
            join_cmd(&parts).as_deref(),
            Some("node server.js --port=3000")
        );
        assert_eq!(join_cmd(&[]), None);
    }

    /// Developer measurement: `cargo test --release measure_ -- --ignored --nocapture`.
    #[test]
    #[ignore = "measurement, not a correctness test"]
    fn measure_process_sampling_cost() {
        let mut sys = System::new().unwrap();
        let mut tracker = ProcessTracker::new();
        let first = Instant::now();
        tracker.sample(&mut sys, Duration::from_secs(1));
        let first = first.elapsed();
        let mut steady = Duration::ZERO;
        let mut rows = Vec::new();
        for _ in 0..5 {
            std::thread::sleep(Duration::from_millis(500));
            let t = Instant::now();
            rows = tracker.sample(&mut sys, Duration::from_millis(500));
            steady += t.elapsed();
        }
        let json = serde_json::to_string(&rows).unwrap();
        let cmd_bytes: usize = rows
            .iter()
            .filter_map(|r| r.cmd.as_ref())
            .map(String::len)
            .sum();
        println!(
            "processes={} first_sample={:?} steady_avg={:?} json_bytes={} cmd_bytes={} unresolved_users={} restricted={} zero_memory={}",
            rows.len(),
            first,
            steady / 5,
            json.len(),
            cmd_bytes,
            rows.iter().filter(|r| r.user.is_none()).count(),
            rows.iter().filter(|r| r.restricted).count(),
            rows.iter().filter(|r| r.memory_bytes == 0).count(),
        );
    }

    /// Developer measurement of how the shown states are distributed:
    /// `cargo test measure_status -- --ignored --nocapture`.
    #[test]
    #[ignore = "measurement, not a correctness test"]
    fn measure_status_distribution() {
        let mut sys = System::new().unwrap();
        let mut tracker = ProcessTracker::new();
        tracker.sample(&mut sys, Duration::from_secs(1));
        std::thread::sleep(Duration::from_secs(2));
        let rows = tracker.sample(&mut sys, Duration::from_secs(2));
        let mut counts = std::collections::BTreeMap::new();
        let mut raw_running = 0;
        for r in &rows {
            *counts.entry(format!("{:?}", r.status)).or_insert(0) += 1;
            raw_running +=
                usize::from(r.status == ProcessState::Running && r.cpu_percent < RAN_CPU_PERCENT);
        }
        println!(
            "{} processes: {counts:?}; running without CPU: {raw_running}",
            rows.len()
        );
    }

    /// A killed child that nobody reaped (zombie) must disappear from the list.
    #[cfg(target_os = "linux")]
    #[test]
    fn exited_unreaped_child_is_not_listed() {
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut sys = System::new().unwrap();
        let mut tracker = ProcessTracker::new();
        let rows = tracker.sample(&mut sys, Duration::from_secs(1));
        assert!(rows.iter().any(|r| r.pid == pid), "live child listed");

        // SIGKILL without wait(): the child stays in the table as a zombie.
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
        std::thread::sleep(Duration::from_millis(300));
        let rows = tracker.sample(&mut sys, Duration::from_secs(1));
        assert!(
            !rows.iter().any(|r| r.pid == pid),
            "zombie must not be listed"
        );
        let _ = child.wait();
    }

    /// Integration: real enumeration finds this test process with sane data.
    #[test]
    fn enumerates_current_process() {
        let mut sys = System::new().unwrap();
        let mut tracker = ProcessTracker::new();
        tracker.sample(&mut sys, Duration::from_secs(1));
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let rows = tracker.sample(&mut sys, Duration::from_secs(1));

        assert!(rows.len() > 1, "expected several processes");
        let me = rows
            .iter()
            .find(|r| r.pid == std::process::id())
            .expect("current process listed");
        assert!(me.memory_bytes > 0);
        assert!(me.exe.is_some());
        assert!(me.threads.unwrap_or(0) >= 1);
        assert!(me.user.is_some(), "current user should resolve");
        assert!(rows.iter().all(|r| (0.0..=100.0).contains(&r.cpu_percent)));
    }
}
