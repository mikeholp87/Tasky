//! Comparison and export of stored results.
//!
//! * Comparisons are refused (rows still listed, `compatible_method =
//!   false`) across different methodology major versions or score
//!   versions, and flagged when the hardware fingerprint changed.
//! * A change is "within noise" when it is smaller than the combined
//!   run-to-run variation of the two measurements (√(cv₁² + cv₂²)), with a
//!   2 % floor for single-value metrics.
//! * Exports contain the result record only: hardware summary, raw
//!   samples, statistics, scores, versions and warnings. No process lists,
//!   file paths beyond the chosen mount point, host names or IP addresses.

use super::environment::{fingerprint_changes, hardware_changed};
use super::models::{
    BenchmarkComparison, BenchmarkKind, BenchmarkRecord, CategoryStatus, ComparisonRow, Metric,
    MetricUnit,
};
use crate::export::csv_field;

/// Change below which single-sample metrics are treated as noise.
pub const MIN_NOISE_PERCENT: f64 = 2.0;

fn method_major(version: &str) -> &str {
    version.split('.').next().unwrap_or(version)
}

pub fn comparable(a: &BenchmarkRecord, b: &BenchmarkRecord) -> bool {
    method_major(&a.method_version) == method_major(&b.method_version)
        && a.score_version == b.score_version
}

fn cv(m: &Metric) -> f64 {
    m.summary.as_ref().map_or(0.0, |s| s.cv_percent)
}

/// Every comparable value of a record: category/system scores and metrics.
fn values(r: &BenchmarkRecord) -> Vec<(String, String, MetricUnit, bool, f64, f64)> {
    let mut out = Vec::new();
    if let Some(s) = &r.system_score {
        out.push((
            "score.system".into(),
            "Overall score".into(),
            MetricUnit::Score,
            true,
            s.value,
            0.0,
        ));
    }
    for c in &r.categories {
        if c.status != CategoryStatus::Completed {
            continue;
        }
        if let Some(score) = c.score {
            out.push((
                format!("score.{}", kind_id(c.kind)),
                format!("{} score", c.kind.label()),
                MetricUnit::Score,
                true,
                score,
                0.0,
            ));
        }
        for m in &c.metrics {
            out.push((
                m.id.clone(),
                m.label.clone(),
                m.unit,
                m.higher_is_better,
                m.value,
                cv(m),
            ));
        }
    }
    out
}

const fn kind_id(kind: BenchmarkKind) -> &'static str {
    match kind {
        BenchmarkKind::Cpu => "cpu",
        BenchmarkKind::Gpu => "gpu",
        BenchmarkKind::Disk => "disk",
        BenchmarkKind::Network => "network",
    }
}

pub fn compare(baseline: &BenchmarkRecord, current: &BenchmarkRecord) -> BenchmarkComparison {
    let base = values(baseline);
    let rows = values(current)
        .into_iter()
        .filter_map(|(id, label, unit, higher, value, cv_now)| {
            let (_, _, _, _, before, cv_before) = base.iter().find(|b| b.0 == id)?;
            if *before == 0.0 || !before.is_finite() || !value.is_finite() {
                return None;
            }
            let change = (value - before) / before * 100.0;
            let noise = (cv_now.powi(2) + cv_before.powi(2))
                .sqrt()
                .max(MIN_NOISE_PERCENT);
            Some(ComparisonRow {
                id,
                label,
                unit,
                higher_is_better: higher,
                baseline: *before,
                current: value,
                change_percent: change,
                within_noise: change.abs() <= noise,
            })
        })
        .collect();
    BenchmarkComparison {
        baseline_id: baseline.id.clone(),
        current_id: current.id.clone(),
        compatible_method: comparable(baseline, current),
        hardware_changed: hardware_changed(&baseline.fingerprint, &current.fingerprint),
        hardware_changes: fingerprint_changes(&baseline.fingerprint, &current.fingerprint),
        rows,
    }
}

fn num(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        String::new()
    }
}

/// One row per metric with its statistics and raw samples.
pub fn to_csv(r: &BenchmarkRecord) -> String {
    let mut out = String::from(
        "run_id,started_at_ms,method_version,score_version,category,status,metric_id,label,unit,\
         higher_is_better,value,median,mean,min,max,std_dev,cv_percent,p90,p95,p99,samples,\
         score,raw_samples\n",
    );
    for c in &r.categories {
        if c.metrics.is_empty() {
            out.push_str(&format!(
                "{},{},{},{},{:?},{:?},,,,,,,,,,,,,,,,,\n",
                csv_field(&r.id),
                r.started_at_ms,
                csv_field(&r.method_version),
                r.score_version,
                c.kind,
                c.status
            ));
        }
        for m in &c.metrics {
            let s = m.summary.clone().unwrap_or_default();
            let raw: Vec<String> = m.samples.iter().map(|v| num(*v)).collect();
            out.push_str(&format!(
                "{},{},{},{},{:?},{:?},{},{},{:?},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&r.id),
                r.started_at_ms,
                csv_field(&r.method_version),
                r.score_version,
                c.kind,
                c.status,
                csv_field(&m.id),
                csv_field(&m.label),
                m.unit,
                m.higher_is_better,
                num(m.value),
                num(s.median),
                num(s.mean),
                num(s.min),
                num(s.max),
                num(s.std_dev),
                num(s.cv_percent),
                num(s.p90),
                num(s.p95),
                num(s.p99),
                s.n,
                m.score.map(num).unwrap_or_default(),
                csv_field(&raw.join(" ")),
            ));
        }
    }
    out
}

pub fn unit_text(unit: MetricUnit) -> &'static str {
    match unit {
        MetricUnit::Mops => "Mops/s",
        MetricUnit::Mflops => "MFLOP/s",
        MetricUnit::Gflops => "GFLOP/s",
        MetricUnit::MegabytesPerSec => "MB/s",
        MetricUnit::GigabytesPerSec => "GB/s",
        MetricUnit::Mbps => "Mbps",
        MetricUnit::Iops => "IOPS",
        MetricUnit::Milliseconds => "ms",
        MetricUnit::Microseconds => "µs",
        MetricUnit::Percent => "%",
        MetricUnit::Ratio => "×",
        MetricUnit::Celsius => "°C",
        MetricUnit::Megahertz => "MHz",
        MetricUnit::Bytes => "bytes",
        MetricUnit::Score => "points",
    }
}

fn value_text(v: f64, unit: MetricUnit) -> String {
    let n = if v.abs() >= 100.0 {
        format!("{v:.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    };
    format!("{n} {}", unit_text(unit))
}

fn score_text(v: Option<f64>) -> String {
    v.map_or_else(|| "Not tested".into(), |s| format!("{s:.0}"))
}

/// Human-readable Markdown report.
pub fn to_markdown(r: &BenchmarkRecord) -> String {
    let e = &r.environment_before;
    let mut md = format!(
        "# Task Manager Benchmark\n\n\
         - Run: `{}`\n- Started (Unix ms): {}\n- Profile: {:?}\n- Duration: {:.0} s\n\
         - Methodology {} · score version {}\n- App version {}\n{}\n",
        r.id,
        r.started_at_ms,
        r.profile,
        r.duration_ms as f64 / 1_000.0,
        r.method_version,
        r.score_version,
        r.app_version,
        if r.simulated {
            "- **Simulated run — not a measurement**\n"
        } else {
            ""
        }
    );
    md.push_str(&format!(
        "## System\n\n- OS: {} {}\n- CPU: {} ({} logical processors)\n- Memory: {} GiB\n",
        e.os_name.as_deref().unwrap_or("Unknown"),
        e.os_version.as_deref().unwrap_or(""),
        e.cpu_brand.trim(),
        e.logical_cores,
        r.fingerprint.memory_gib
    ));
    if let Some(gpu) = &r.fingerprint.gpu {
        md.push_str(&format!("- GPU: {gpu}\n"));
    }
    if let Some(disk) = &r.fingerprint.disk {
        md.push_str(&format!("- Storage: {disk}\n"));
    }
    md.push_str(&format!(
        "\n## Overall score: {}{}\n\n",
        score_text(r.system_score.as_ref().map(|s| s.value)),
        if r.system_score.as_ref().is_some_and(|s| s.partial) {
            " (partial benchmark)"
        } else {
            ""
        }
    ));
    for c in &r.categories {
        md.push_str(&format!(
            "### {} — {:?}{}\n\n",
            c.kind.label(),
            c.status,
            c.score
                .map(|s| format!(" — score {s:.0}"))
                .unwrap_or_default()
        ));
        if !c.metrics.is_empty() {
            md.push_str("| Measurement | Value | Variation |\n| --- | --- | --- |\n");
            for m in &c.metrics {
                let cv = m.summary.as_ref().filter(|s| s.n > 1).map_or_else(
                    || "—".into(),
                    |s| format!("{:.1}% (n={})", s.cv_percent, s.n),
                );
                md.push_str(&format!(
                    "| {} | {} | {} |\n",
                    m.label.replace('|', "/"),
                    value_text(m.value, m.unit),
                    cv
                ));
            }
            md.push('\n');
        }
        for d in &c.details {
            md.push_str(&format!("- {}: {}\n", d.label, d.value));
        }
        for w in &c.warnings {
            md.push_str(&format!("- ⚠ {}\n", w.message));
        }
        if let Some(err) = &c.error {
            md.push_str(&format!("- Error: {}\n", err.message));
        }
        md.push('\n');
    }
    for w in &r.warnings {
        md.push_str(&format!("- ⚠ {}\n", w.message));
    }
    md.push_str(
        "\nThis is Task Manager's own benchmark suite. Its scores are not comparable with \
         other benchmark products.\n",
    );
    md
}

/// Short plain-text summary for the clipboard.
pub fn summary_text(r: &BenchmarkRecord) -> String {
    let mut lines = vec!["System Benchmark".to_owned()];
    if let Some(s) = &r.system_score {
        lines.push(format!(
            "Overall: {:.0}{}",
            s.value,
            if s.partial { " (partial)" } else { "" }
        ));
    }
    let metric = |kind, id: &str| {
        r.category(kind)
            .and_then(|c| c.metric(id))
            .map(|m| value_text(m.value, m.unit))
    };
    let push = |lines: &mut Vec<String>, title: &str, items: Vec<(&str, Option<String>)>| {
        let items: Vec<String> = items
            .into_iter()
            .filter_map(|(l, v)| v.map(|v| format!("  {l}: {v}")))
            .collect();
        if !items.is_empty() {
            lines.push(String::new());
            lines.push(format!("{title}:"));
            lines.extend(items);
        }
    };
    push(
        &mut lines,
        "CPU",
        vec![
            (
                "Single-core integer",
                metric(BenchmarkKind::Cpu, "cpu.single.integer"),
            ),
            (
                "Multi-core integer",
                metric(BenchmarkKind::Cpu, "cpu.multi.integer"),
            ),
            (
                "Score",
                r.category(BenchmarkKind::Cpu)
                    .and_then(|c| c.score)
                    .map(|s| format!("{s:.0}")),
            ),
        ],
    );
    push(
        &mut lines,
        "GPU",
        vec![
            ("Compute", metric(BenchmarkKind::Gpu, "gpu.compute.fma")),
            ("Memory", metric(BenchmarkKind::Gpu, "gpu.memory.readwrite")),
        ],
    );
    push(
        &mut lines,
        "Storage",
        vec![
            ("Read", metric(BenchmarkKind::Disk, "disk.seq.read")),
            ("Write", metric(BenchmarkKind::Disk, "disk.seq.write")),
            (
                "Random read QD1",
                metric(BenchmarkKind::Disk, "disk.rand.read.qd1"),
            ),
        ],
    );
    push(
        &mut lines,
        "Internet",
        vec![
            ("Download", metric(BenchmarkKind::Network, "net.download")),
            ("Upload", metric(BenchmarkKind::Network, "net.upload")),
            (
                "Latency",
                metric(BenchmarkKind::Network, "net.latency.idle"),
            ),
        ],
    );
    lines.push(String::new());
    lines.push(format!(
        "Task Manager Benchmark v{}{}",
        r.method_version,
        if r.simulated { " (simulated)" } else { "" }
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::history::tests::record;
    use crate::benchmarks::models::{CategoryResult, SystemScore};
    use crate::benchmarks::statistics::summarize;

    fn with_metric(id: &str, samples: &[f64], fp_cpu: &str) -> BenchmarkRecord {
        let mut r = record(1_000);
        r.fingerprint.cpu_brand = fp_cpu.into();
        let summary = summarize(samples);
        r.categories = vec![CategoryResult {
            metrics: vec![Metric {
                id: id.into(),
                label: "Multi-core integer".into(),
                unit: MetricUnit::Mops,
                higher_is_better: true,
                value: summary.as_ref().unwrap().median,
                summary,
                samples: samples.to_vec(),
                outliers: Vec::new(),
                score: None,
                note: None,
            }],
            score: Some(1_000.0),
            ..CategoryResult::empty(BenchmarkKind::Cpu, CategoryStatus::Completed)
        }];
        r.system_score = Some(SystemScore {
            value: 900.0,
            partial: true,
            categories: vec![BenchmarkKind::Cpu],
        });
        r
    }

    #[test]
    fn compares_with_noise_and_hardware_checks() {
        let a = with_metric(
            "cpu.multi.integer",
            &[100.0, 101.0, 99.0, 100.0, 100.5],
            "CPU A",
        );
        let b = with_metric(
            "cpu.multi.integer",
            &[110.0, 111.0, 109.0, 110.0, 110.5],
            "CPU A",
        );
        let c = compare(&a, &b);
        assert!(c.compatible_method);
        assert!(!c.hardware_changed);
        let row = c.rows.iter().find(|r| r.id == "cpu.multi.integer").unwrap();
        assert!((row.change_percent - 10.0).abs() < 1e-9);
        assert!(!row.within_noise);
        assert!(c.rows.iter().any(|r| r.id == "score.system"));

        let small = with_metric(
            "cpu.multi.integer",
            &[101.0, 101.5, 100.5, 101.0, 101.2],
            "CPU A",
        );
        let row = compare(&a, &small)
            .rows
            .into_iter()
            .find(|r| r.id == "cpu.multi.integer")
            .unwrap();
        assert!(row.within_noise, "1 % change is within noise");

        let other = with_metric("cpu.multi.integer", &[200.0; 5], "CPU B");
        let c = compare(&a, &other);
        assert!(c.hardware_changed);
        assert!(c.hardware_changes[0].contains("CPU A → CPU B"));
    }

    #[test]
    fn refuses_cross_version_comparison() {
        let a = with_metric("m", &[1.0; 5], "X");
        let mut b = a.clone();
        b.method_version = "2.0.0".into();
        assert!(!compare(&a, &b).compatible_method);
        let mut c = a.clone();
        c.method_version = "1.3.0".into();
        assert!(compare(&a, &c).compatible_method, "minor versions compare");
        let mut d = a.clone();
        d.score_version += 1;
        assert!(!compare(&a, &d).compatible_method);
    }

    #[test]
    fn csv_has_raw_samples_and_guards_formulas() {
        let mut r = with_metric("cpu.multi.integer", &[100.0, 101.0], "X");
        r.categories[0].metrics[0].label = "=HYPERLINK(\"x\")".into();
        let csv = to_csv(&r);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("100 101"));
        assert!(lines[1].contains("\"'=HYPERLINK(\"\"x\"\")\""));
        // Same number of columns in header and row (no unquoted commas leak).
        assert_eq!(lines[0].matches(',').count(), lines[1].matches(',').count());
    }

    #[test]
    fn markdown_and_summary_are_honest_about_scope() {
        let r = with_metric("cpu.multi.integer", &[100.0, 101.0], "X");
        let md = to_markdown(&r);
        assert!(md.contains("not comparable with other benchmark products"));
        assert!(md.contains("partial benchmark"));
        let s = summary_text(&r);
        assert!(s.starts_with("System Benchmark"));
        assert!(s.contains("Multi-core integer: 100 Mops/s"));
        assert!(s.contains("Task Manager Benchmark v1.0.0"));
        assert!(!s.contains("Internet"), "untested categories are omitted");
    }
}
