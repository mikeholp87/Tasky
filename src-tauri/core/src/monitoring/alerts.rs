//! Alert evaluation with independent episodes per process lifetime and mount.
//! Durations use an injectable monotonic clock; tick times are for display. Runs on the engine thread, which keeps
//! alerts working while the window is hidden.
//!
//! A rule fires once per *episode*: when its condition has held for
//! `duration_secs`. It re-arms only after the condition clears, so a
//! sustained spike produces one notification, not one per tick.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::models::{AlertEvent, AlertMetric, AlertRule, MetricsTick};

#[derive(Debug, Default, Clone, Copy)]
struct RuleState {
    /// When the condition first became true in the current episode.
    since: Option<Instant>,
    fired: bool,
}

/// The value that crossed a threshold and, for process rules, which process.
struct Observation {
    value: f64,
    subject: Subject,
    name: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum Subject {
    System,
    Process(u32, u64),
    Disk(String),
}

#[derive(Default)]
pub struct AlertEvaluator {
    state: HashMap<String, (AlertRule, HashMap<Subject, RuleState>)>,
}

impl AlertEvaluator {
    pub fn evaluate(&mut self, rules: &[AlertRule], tick: &MetricsTick) -> Vec<AlertEvent> {
        self.evaluate_at(rules, tick, Instant::now())
    }

    fn evaluate_at(
        &mut self,
        rules: &[AlertRule],
        tick: &MetricsTick,
        now: Instant,
    ) -> Vec<AlertEvent> {
        let mut events = Vec::new();
        let mut next = HashMap::with_capacity(rules.len());

        for rule in rules.iter().filter(|r| r.enabled) {
            let mut previous = self
                .state
                .remove(&rule.id)
                .filter(|(old, _)| old == rule)
                .map(|(_, episodes)| episodes)
                .unwrap_or_default();
            let mut episodes = HashMap::new();
            for obs in observe(rule, tick)
                .into_iter()
                .filter(|o| o.value > rule.threshold)
            {
                let mut st = previous.remove(&obs.subject).unwrap_or_default();
                let since = *st.since.get_or_insert(now);
                if !st.fired
                    && now.saturating_duration_since(since)
                        >= Duration::from_secs(u64::from(rule.duration_secs))
                {
                    st.fired = true;
                    events.push(AlertEvent {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        timestamp_ms: tick.timestamp_ms,
                        message: message(rule, &obs),
                        pid: match obs.subject {
                            Subject::Process(pid, _) => Some(pid),
                            _ => None,
                        },
                    });
                }
                episodes.insert(obs.subject, st);
            }
            next.insert(rule.id.clone(), (rule.clone(), episodes));
        }
        // Rules that were removed or disabled forget their state.
        self.state = next;
        events
    }
}

fn normalize(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    lower
        .strip_suffix(".exe")
        .map(str::to_owned)
        .unwrap_or(lower)
}

fn observe(rule: &AlertRule, tick: &MetricsTick) -> Vec<Observation> {
    let simple = |value: f64| {
        vec![Observation {
            value,
            subject: Subject::System,
            name: None,
        }]
    };
    match rule.metric {
        AlertMetric::SystemCpu => simple(f64::from(tick.cpu.usage_percent)),
        AlertMetric::SystemMemory => simple(tick.memory.used_percent),
        AlertMetric::DiskUsage => tick
            .disks
            .iter()
            .map(|d| Observation {
                value: d.used_percent,
                subject: Subject::Disk(d.mount_point.clone()),
                name: None,
            })
            .collect(),
        AlertMetric::ProcessCpu | AlertMetric::ProcessMemory => {
            let Some(name) = rule.process_name.as_deref() else {
                return Vec::new();
            };
            let wanted = normalize(name);
            tick.processes
                .iter()
                // Restricted processes have unknown CPU; never alert on a 0.
                .filter(|p| normalize(&p.name) == wanted)
                // An unknown start time cannot identify a process episode.
                .filter(|p| p.start_time > 0)
                .filter(|p| rule.metric == AlertMetric::ProcessMemory || !p.restricted)
                .map(|p| Observation {
                    value: if rule.metric == AlertMetric::ProcessCpu {
                        f64::from(p.cpu_percent)
                    } else {
                        p.memory_bytes as f64
                    },
                    subject: Subject::Process(p.pid, p.start_time),
                    name: Some(p.name.clone()),
                })
                .collect()
        }
    }
}

fn message(rule: &AlertRule, obs: &Observation) -> String {
    let held = if rule.duration_secs > 0 {
        format!(" for at least {} s", rule.duration_secs)
    } else {
        String::new()
    };
    match (rule.metric, &obs.subject) {
        (AlertMetric::SystemCpu, _) => format!("CPU usage is {:.0}%{held}.", obs.value),
        (AlertMetric::SystemMemory, _) => format!("Memory usage is {:.0}%{held}.", obs.value),
        (AlertMetric::DiskUsage, Subject::Disk(mount)) => {
            format!("Drive {} is {:.0}% full.", mount, obs.value)
        }
        (AlertMetric::ProcessCpu, Subject::Process(pid, _)) => {
            let name = obs.name.as_deref().unwrap_or("Process");
            format!("{name} (PID {pid}) is using {:.0}% CPU{held}.", obs.value)
        }
        (AlertMetric::ProcessMemory, Subject::Process(pid, _)) => format!(
            "{} (PID {pid}) is using {} of memory{held}.",
            obs.name.as_deref().unwrap_or("Process"),
            format_bytes(obs.value)
        ),
        _ => format!("{} triggered.", rule.name),
    }
}

/// Compact binary byte formatting for alert messages.
fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes.max(0.0);
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{v:.0} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    struct ClockedEvaluator {
        inner: AlertEvaluator,
        origin: Instant,
    }

    impl Default for ClockedEvaluator {
        fn default() -> Self {
            Self {
                inner: AlertEvaluator::default(),
                origin: Instant::now(),
            }
        }
    }

    impl ClockedEvaluator {
        fn evaluate(&mut self, rules: &[AlertRule], tick: &MetricsTick) -> Vec<AlertEvent> {
            self.inner.evaluate_at(
                rules,
                tick,
                self.origin + Duration::from_millis(tick.timestamp_ms),
            )
        }
    }

    #[test]
    fn wall_clock_changes_do_not_change_held_duration() {
        let rules = [rule(AlertMetric::SystemCpu, 80.0, 10, None)];
        let mut evaluator = AlertEvaluator::default();
        let now = Instant::now();
        assert!(
            evaluator
                .evaluate_at(&rules, &tick(100, 90.0, vec![]), now)
                .is_empty()
        );
        assert!(
            evaluator
                .evaluate_at(
                    &rules,
                    &tick(10_000, 90.0, vec![]),
                    now + Duration::from_secs(1)
                )
                .is_empty()
        );
        assert_eq!(
            evaluator
                .evaluate_at(
                    &rules,
                    &tick(0, 90.0, vec![]),
                    now + Duration::from_secs(10)
                )
                .len(),
            1
        );
    }

    #[test]
    fn reused_pid_and_changed_rules_restart_episodes() {
        let mut rules = [rule(AlertMetric::ProcessCpu, 80.0, 10, Some("worker"))];
        let mut evaluator = ClockedEvaluator::default();
        let old = proc(7, "worker", 90.0, false);
        evaluator.evaluate(&rules, &tick(0, 0.0, vec![old.clone()]));
        let mut replacement = old;
        replacement.start_time += 1;
        assert!(
            evaluator
                .evaluate(&rules, &tick(10, 0.0, vec![replacement.clone()]))
                .is_empty()
        );
        assert_eq!(
            evaluator
                .evaluate(&rules, &tick(20, 0.0, vec![replacement.clone()]))
                .len(),
            1
        );
        rules[0].threshold = 85.0;
        assert!(
            evaluator
                .evaluate(&rules, &tick(21, 0.0, vec![replacement.clone()]))
                .is_empty()
        );
        assert_eq!(
            evaluator
                .evaluate(&rules, &tick(31, 0.0, vec![replacement]))
                .len(),
            1
        );
    }

    #[test]
    fn disabled_and_removed_rules_release_episode_state() {
        let rules = [rule(AlertMetric::SystemCpu, 80.0, 10, None)];
        let mut evaluator = ClockedEvaluator::default();
        evaluator.evaluate(&rules, &tick(0, 90.0, vec![]));
        evaluator.evaluate(&[], &tick(10, 90.0, vec![]));
        assert!(evaluator.inner.state.is_empty());
        assert!(
            evaluator
                .evaluate(&rules, &tick(11, 90.0, vec![]))
                .is_empty()
        );
    }

    fn tick(t_secs: u64, cpu: f32, procs: Vec<ProcessRow>) -> MetricsTick {
        MetricsTick {
            seq: t_secs,
            timestamp_ms: t_secs * 1_000,
            elapsed_ms: 1_000.0,
            cpu: CpuSnapshot {
                usage_percent: cpu,
                per_core: vec![],
                frequency_mhz: None,
                load_average: None,
            },
            memory: MemorySnapshot {
                total_bytes: 100,
                used_bytes: 50,
                available_bytes: 50,
                used_percent: 50.0,
                cached_bytes: None,
                buffers_bytes: None,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
                commit_used_bytes: None,
                commit_limit_bytes: None,
                kernel_paged_bytes: None,
                kernel_nonpaged_bytes: None,
            },
            disks: vec![],
            networks: vec![],
            processes: procs,
            gpus: vec![],
            temperatures: vec![],
            alerts: vec![],
            activity: vec![],
            point: HistoryPoint {
                t: 0,
                cpu,
                per_core: vec![],
                mem_percent: 50.0,
                mem_used: 50,
                swap_used: 0,
                net_rx: 0.0,
                net_tx: 0.0,
                disk_read: 0.0,
                disk_write: 0.0,
            },
            engine: EngineStats {
                poll_duration_ms: 0.0,
                interval_ms: 1_000,
                process_count: 0,
                last_payload_bytes: 0,
                benchmark_active: false,
            },
        }
    }

    fn rule(
        metric: AlertMetric,
        threshold: f64,
        duration_secs: u32,
        process: Option<&str>,
    ) -> AlertRule {
        AlertRule {
            id: "r".into(),
            name: "rule".into(),
            enabled: true,
            metric,
            threshold,
            duration_secs,
            process_name: process.map(Into::into),
        }
    }

    fn proc(pid: u32, name: &str, cpu: f32, restricted: bool) -> ProcessRow {
        ProcessRow {
            pid,
            parent_pid: None,
            name: name.into(),
            status: ProcessState::Running,
            cpu_percent: cpu,
            memory_bytes: 5 * 1024 * 1024 * 1024,
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
            restricted,
        }
    }

    #[test]
    fn fires_only_after_the_condition_holds_for_the_duration() {
        let rules = [rule(AlertMetric::SystemCpu, 90.0, 30, None)];
        let mut ev = ClockedEvaluator::default();
        assert!(ev.evaluate(&rules, &tick(0, 95.0, vec![])).is_empty());
        assert!(ev.evaluate(&rules, &tick(29, 95.0, vec![])).is_empty());
        let fired = ev.evaluate(&rules, &tick(30, 97.0, vec![]));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].message, "CPU usage is 97% for at least 30 s.");
    }

    #[test]
    fn fires_once_per_episode_and_rearms_after_clearing() {
        let rules = [rule(AlertMetric::SystemCpu, 90.0, 0, None)];
        let mut ev = ClockedEvaluator::default();
        assert_eq!(ev.evaluate(&rules, &tick(0, 95.0, vec![])).len(), 1);
        assert!(
            ev.evaluate(&rules, &tick(1, 95.0, vec![])).is_empty(),
            "no repeat"
        );
        assert!(ev.evaluate(&rules, &tick(2, 10.0, vec![])).is_empty());
        assert_eq!(
            ev.evaluate(&rules, &tick(3, 95.0, vec![])).len(),
            1,
            "re-armed"
        );
    }

    #[test]
    fn a_dip_restarts_the_duration() {
        let rules = [rule(AlertMetric::SystemCpu, 90.0, 10, None)];
        let mut ev = ClockedEvaluator::default();
        ev.evaluate(&rules, &tick(0, 95.0, vec![]));
        ev.evaluate(&rules, &tick(5, 50.0, vec![]));
        assert!(ev.evaluate(&rules, &tick(12, 95.0, vec![])).is_empty());
        assert_eq!(ev.evaluate(&rules, &tick(22, 95.0, vec![])).len(), 1);
    }

    #[test]
    fn process_rules_match_names_across_platforms() {
        let rules = [rule(AlertMetric::ProcessCpu, 80.0, 0, Some("Node"))];
        let mut ev = ClockedEvaluator::default();
        let fired = ev.evaluate(
            &rules,
            &tick(0, 10.0, vec![proc(7, "node.exe", 85.0, false)]),
        );
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].pid, Some(7));
        assert!(
            fired[0]
                .message
                .starts_with("node.exe (PID 7) is using 85% CPU")
        );
    }

    #[test]
    fn restricted_processes_never_trigger_cpu_rules() {
        let rules = [rule(AlertMetric::ProcessCpu, 0.0, 0, Some("svchost"))];
        let mut ev = ClockedEvaluator::default();
        assert!(
            ev.evaluate(
                &rules,
                &tick(0, 10.0, vec![proc(4, "svchost.exe", 0.5, true)])
            )
            .is_empty()
        );
    }

    #[test]
    fn process_memory_message_is_human_readable() {
        let rules = [rule(
            AlertMetric::ProcessMemory,
            4.0 * 1024f64.powi(3),
            0,
            Some("java"),
        )];
        let mut ev = ClockedEvaluator::default();
        let fired = ev.evaluate(&rules, &tick(0, 0.0, vec![proc(9, "java", 0.0, false)]));
        assert_eq!(fired[0].message, "java (PID 9) is using 5.0 GB of memory.");
    }

    #[test]
    fn disabled_rules_do_nothing() {
        let mut r = rule(AlertMetric::SystemCpu, 1.0, 0, None);
        r.enabled = false;
        assert!(
            AlertEvaluator::default()
                .evaluate(&[r], &tick(0, 99.0, vec![]))
                .is_empty()
        );
    }
    #[test]
    fn matching_processes_keep_independent_episodes_when_the_leader_changes() {
        let rules = [rule(AlertMetric::ProcessCpu, 80.0, 20, Some("node"))];
        let mut ev = ClockedEvaluator::default();
        ev.evaluate(
            &rules,
            &tick(
                0,
                0.0,
                vec![proc(1, "node", 99.0, false), proc(2, "node", 85.0, false)],
            ),
        );
        ev.evaluate(
            &rules,
            &tick(
                10,
                0.0,
                vec![proc(1, "node", 85.0, false), proc(2, "node", 99.0, false)],
            ),
        );
        let fired = ev.evaluate(
            &rules,
            &tick(
                20,
                0.0,
                vec![proc(1, "node", 85.0, false), proc(2, "node", 99.0, false)],
            ),
        );
        assert_eq!(fired.len(), 2);
        assert!(fired.iter().any(|e| e.pid == Some(1)));
        assert!(fired.iter().any(|e| e.pid == Some(2)));
    }
}
