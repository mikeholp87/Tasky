//! Alert evaluation. A pure state machine driven by tick timestamps, so it is
//! deterministic and unit-testable. Runs on the engine thread, which keeps
//! alerts working while the window is hidden.
//!
//! A rule fires once per *episode*: when its condition has held for
//! `duration_secs`. It re-arms only after the condition clears, so a
//! sustained spike produces one notification, not one per tick.

use std::collections::HashMap;

use crate::models::{AlertEvent, AlertMetric, AlertRule, MetricsTick};

#[derive(Debug, Default, Clone, Copy)]
struct RuleState {
    /// When the condition first became true in the current episode.
    since_ms: Option<u64>,
    fired: bool,
}

/// The value that crossed a threshold and, for process rules, which process.
struct Observation {
    value: f64,
    subject: Option<(u32, String)>,
    detail: Option<String>,
}

#[derive(Default)]
pub struct AlertEvaluator {
    state: HashMap<String, RuleState>,
}

impl AlertEvaluator {
    pub fn evaluate(&mut self, rules: &[AlertRule], tick: &MetricsTick) -> Vec<AlertEvent> {
        let now = tick.timestamp_ms;
        let mut events = Vec::new();
        let mut next = HashMap::with_capacity(rules.len());

        for rule in rules.iter().filter(|r| r.enabled) {
            let mut st = self.state.get(&rule.id).copied().unwrap_or_default();
            match observe(rule, tick).filter(|o| o.value > rule.threshold) {
                None => st = RuleState::default(),
                Some(obs) => {
                    let since = *st.since_ms.get_or_insert(now);
                    let held_ms = now.saturating_sub(since);
                    if !st.fired && held_ms >= u64::from(rule.duration_secs) * 1_000 {
                        st.fired = true;
                        events.push(AlertEvent {
                            rule_id: rule.id.clone(),
                            rule_name: rule.name.clone(),
                            timestamp_ms: now,
                            message: message(rule, &obs),
                            pid: obs.subject.as_ref().map(|s| s.0),
                        });
                    }
                }
            }
            next.insert(rule.id.clone(), st);
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

fn observe(rule: &AlertRule, tick: &MetricsTick) -> Option<Observation> {
    let simple = |value: f64| Observation {
        value,
        subject: None,
        detail: None,
    };
    match rule.metric {
        AlertMetric::SystemCpu => Some(simple(f64::from(tick.cpu.usage_percent))),
        AlertMetric::SystemMemory => Some(simple(tick.memory.used_percent)),
        AlertMetric::DiskUsage => tick
            .disks
            .iter()
            .max_by(|a, b| a.used_percent.total_cmp(&b.used_percent))
            .map(|d| Observation {
                value: d.used_percent,
                subject: None,
                detail: Some(d.mount_point.clone()),
            }),
        AlertMetric::ProcessCpu | AlertMetric::ProcessMemory => {
            let wanted = normalize(rule.process_name.as_deref()?);
            tick.processes
                .iter()
                // Restricted processes have unknown CPU; never alert on a 0.
                .filter(|p| normalize(&p.name) == wanted)
                .filter(|p| rule.metric == AlertMetric::ProcessMemory || !p.restricted)
                .map(|p| Observation {
                    value: if rule.metric == AlertMetric::ProcessCpu {
                        f64::from(p.cpu_percent)
                    } else {
                        p.memory_bytes as f64
                    },
                    subject: Some((p.pid, p.name.clone())),
                    detail: None,
                })
                .max_by(|a, b| a.value.total_cmp(&b.value))
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
        (AlertMetric::DiskUsage, _) => format!(
            "Drive {} is {:.0}% full.",
            obs.detail.as_deref().unwrap_or("?"),
            obs.value
        ),
        (AlertMetric::ProcessCpu, Some((pid, name))) => {
            format!("{name} (PID {pid}) is using {:.0}% CPU{held}.", obs.value)
        }
        (AlertMetric::ProcessMemory, Some((pid, name))) => format!(
            "{name} (PID {pid}) is using {} of memory{held}.",
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
            },
            disks: vec![],
            networks: vec![],
            processes: procs,
            alerts: vec![],
            gpus: vec![],
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
                gpu_usage: None,
                gpu_vram_percent: None,
                gpu_temp: None,
            },
            engine: EngineStats {
                poll_duration_ms: 0.0,
                interval_ms: 1_000,
                process_count: 0,
                last_payload_bytes: 0,
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
            restricted,
        }
    }

    #[test]
    fn fires_only_after_the_condition_holds_for_the_duration() {
        let rules = [rule(AlertMetric::SystemCpu, 90.0, 30, None)];
        let mut ev = AlertEvaluator::default();
        assert!(ev.evaluate(&rules, &tick(0, 95.0, vec![])).is_empty());
        assert!(ev.evaluate(&rules, &tick(29, 95.0, vec![])).is_empty());
        let fired = ev.evaluate(&rules, &tick(30, 97.0, vec![]));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].message, "CPU usage is 97% for at least 30 s.");
    }

    #[test]
    fn fires_once_per_episode_and_rearms_after_clearing() {
        let rules = [rule(AlertMetric::SystemCpu, 90.0, 0, None)];
        let mut ev = AlertEvaluator::default();
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
        let mut ev = AlertEvaluator::default();
        ev.evaluate(&rules, &tick(0, 95.0, vec![]));
        ev.evaluate(&rules, &tick(5, 50.0, vec![]));
        assert!(ev.evaluate(&rules, &tick(12, 95.0, vec![])).is_empty());
        assert_eq!(ev.evaluate(&rules, &tick(22, 95.0, vec![])).len(), 1);
    }

    #[test]
    fn process_rules_match_names_across_platforms() {
        let rules = [rule(AlertMetric::ProcessCpu, 80.0, 0, Some("Node"))];
        let mut ev = AlertEvaluator::default();
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
        let mut ev = AlertEvaluator::default();
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
        let mut ev = AlertEvaluator::default();
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
}
