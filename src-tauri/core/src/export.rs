//! Export formatting. Pure functions over snapshots; the app layer decides
//! where the result is written (always through a native save dialog).
//!
//! Command lines can contain secrets (tokens, passwords in arguments), so
//! they are omitted unless the user explicitly opts in.

use serde::Serialize;

use crate::models::{MetricsTick, ProcessRow, StaticSystemInfo};

const CSV_HEADER: &[&str] = &[
    "pid",
    "parent_pid",
    "name",
    "status",
    "cpu_percent",
    "memory_bytes",
    "virtual_memory_bytes",
    "disk_read_bytes_per_sec",
    "disk_write_bytes_per_sec",
    "user",
    "start_time_unix",
    "threads",
    "priority",
    "restricted",
    "executable",
];

/// Quotes a CSV field when needed, and neutralizes values a spreadsheet
/// would interpret as a formula (process names are attacker-controlled).
pub fn csv_field(value: &str) -> String {
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_default()
}

pub fn processes_csv(rows: &[ProcessRow], include_command_lines: bool) -> String {
    let mut out = String::new();
    let mut header: Vec<&str> = CSV_HEADER.to_vec();
    if include_command_lines {
        header.push("command_line");
    }
    out.push_str(&header.join(","));
    out.push_str("\r\n");
    for r in rows {
        let mut fields = vec![
            r.pid.to_string(),
            opt(r.parent_pid),
            csv_field(&r.name),
            format!("{:?}", r.status).to_lowercase(),
            if r.restricted {
                String::new()
            } else {
                format!("{:.2}", r.cpu_percent)
            },
            r.memory_bytes.to_string(),
            r.virtual_memory_bytes.to_string(),
            if r.restricted {
                String::new()
            } else {
                format!("{:.0}", r.disk_read_per_sec)
            },
            if r.restricted {
                String::new()
            } else {
                format!("{:.0}", r.disk_write_per_sec)
            },
            csv_field(r.user.as_deref().unwrap_or("")),
            r.start_time.to_string(),
            opt(r.threads),
            opt(r.priority),
            r.restricted.to_string(),
            csv_field(r.exe.as_deref().unwrap_or("")),
        ];
        if include_command_lines {
            fields.push(csv_field(r.cmd.as_deref().unwrap_or("")));
        }
        out.push_str(&fields.join(","));
        out.push_str("\r\n");
    }
    out
}

pub fn processes_json(
    rows: &[ProcessRow],
    include_command_lines: bool,
) -> serde_json::Result<String> {
    let rows: Vec<ProcessRow> = rows
        .iter()
        .cloned()
        .map(|mut r| {
            if !include_command_lines {
                r.cmd = None;
            }
            r
        })
        .collect();
    serde_json::to_string_pretty(&rows)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSummary<'a> {
    generated_at_ms: u64,
    system: &'a StaticSystemInfo,
    cpu: &'a crate::models::CpuSnapshot,
    memory: &'a crate::models::MemorySnapshot,
    disks: &'a [crate::models::DiskSnapshot],
    networks: &'a [crate::models::NetworkInterfaceSnapshot],
    process_count: usize,
}

/// System summary without any per-process data.
pub fn system_summary_json(
    info: &StaticSystemInfo,
    tick: &MetricsTick,
) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&SystemSummary {
        generated_at_ms: tick.timestamp_ms,
        system: info,
        cpu: &tick.cpu,
        memory: &tick.memory,
        disks: &tick.disks,
        networks: &tick.networks,
        process_count: tick.processes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProcessState;

    fn row(name: &str, cmd: &str) -> ProcessRow {
        ProcessRow {
            pid: 42,
            parent_pid: Some(1),
            name: name.into(),
            status: ProcessState::Running,
            cpu_percent: 12.345,
            memory_bytes: 2048,
            virtual_memory_bytes: 4096,
            disk_read_per_sec: 1.0,
            disk_write_per_sec: 2.0,
            user: Some("alice".into()),
            start_time: 1_700_000_000,
            threads: Some(3),
            priority: Some(8),
            exe: Some(r"C:\Program Files\app\app.exe".into()),
            cmd: Some(cmd.into()),
            session_id: None,
            restricted: false,
            kernel_thread: false,
            pss_bytes: None,
        }
    }

    #[test]
    fn escapes_csv_and_blocks_formula_injection() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("=cmd|' /C calc'!A0"), "'=cmd|' /C calc'!A0");
        assert_eq!(csv_field("-2+3"), "'-2+3");
    }

    #[test]
    fn omits_command_lines_unless_requested() {
        let rows = [row("app", "app --token=SECRET")];
        let csv = processes_csv(&rows, false);
        assert!(!csv.contains("SECRET"));
        assert!(!csv.contains("command_line"));
        assert!(processes_csv(&rows, true).contains("--token=SECRET"));

        let json = processes_json(&rows, false).unwrap();
        assert!(!json.contains("SECRET"));
        assert!(processes_json(&rows, true).unwrap().contains("SECRET"));
    }

    #[test]
    fn csv_has_one_line_per_process() {
        let csv = processes_csv(&[row("a", ""), row("b", "")], false);
        let lines: Vec<&str> = csv.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("42,1,a,running,12.35,2048,"));
        assert!(
            lines[1].contains("\"C:\\Program Files\\app\\app.exe\"")
                || lines[1].contains(r"C:\Program Files\app\app.exe")
        );
    }

    #[test]
    fn restricted_values_are_blank_not_zero() {
        let mut r = row("svc", "");
        r.restricted = true;
        let csv = processes_csv(&[r], false);
        let line = csv.split("\r\n").nth(1).unwrap();
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields[4], "", "cpu unknown");
        assert_eq!(fields[13], "true");
    }
}
