//! Result-consistency heuristic shown as High / Medium / Low.
//!
//! This is *not* a statistical confidence interval. It summarizes how much
//! the measurements can be trusted to be repeatable:
//!
//! | Level  | Conditions (any one demotes) |
//! | ------ | ---------------------------- |
//! | Low    | worst CV > 10 %, fewer than 3 samples, or mean background CPU > 25 % |
//! | Medium | worst CV > 3 %, any outlier, fewer than 5 samples, background CPU > 10 %, or a sustained-performance decline |
//! | High   | otherwise |

use super::models::{BenchmarkWarning, CategoryResult, CategoryStatus, Confidence, WarningCode};
use super::statistics::HIGH_VARIATION_CV_PERCENT;

/// Mean CPU use by other processes above which a warning is attached.
pub const INTERFERENCE_WARN_PERCENT: f64 = 15.0;

pub fn assess(category: &CategoryResult) -> Option<Confidence> {
    if category.status != CategoryStatus::Completed {
        return None;
    }
    let multi: Vec<_> = category
        .metrics
        .iter()
        .filter_map(|m| m.summary.as_ref().map(|s| (s, m.outliers.len())))
        .collect();
    if multi.is_empty() {
        return None;
    }
    let worst_cv = multi.iter().map(|(s, _)| s.cv_percent).fold(0.0, f64::max);
    let min_n = multi.iter().map(|(s, _)| s.n).min().unwrap_or(0);
    let outliers: usize = multi.iter().map(|(_, o)| *o).sum();
    let background = category
        .interference
        .as_ref()
        .map_or(0.0, |i| i.mean_external_cpu_percent);
    let declined = category
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::PerformanceDecline);

    if worst_cv > 10.0 || min_n < 3 || background > 25.0 {
        Some(Confidence::Low)
    } else if worst_cv > 3.0 || outliers > 0 || min_n < 5 || background > 10.0 || declined {
        Some(Confidence::Medium)
    } else {
        Some(Confidence::High)
    }
}

/// Warnings derived from the measurements themselves.
pub fn derived_warnings(category: &CategoryResult) -> Vec<BenchmarkWarning> {
    let mut warnings = Vec::new();
    let unstable: Vec<&str> = category
        .metrics
        .iter()
        .filter(|m| {
            m.summary
                .as_ref()
                .is_some_and(|s| s.n > 1 && s.cv_percent > HIGH_VARIATION_CV_PERCENT)
        })
        .map(|m| m.label.as_str())
        .collect();
    if !unstable.is_empty() {
        warnings.push(BenchmarkWarning::new(
            WarningCode::HighVariation,
            format!(
                "Result variation was higher than expected ({}). Background activity or \
                 thermal throttling may have affected this benchmark.",
                unstable.join(", ")
            ),
        ));
    }
    if let Some(i) = &category.interference
        && i.mean_external_cpu_percent > INTERFERENCE_WARN_PERCENT
    {
        warnings.push(BenchmarkWarning::new(
            WarningCode::BackgroundActivityDuringRun,
            format!(
                "Background activity detected during this benchmark (other applications used \
                 {:.0}% CPU on average). Result accuracy may be reduced.",
                i.mean_external_cpu_percent
            ),
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::models::{BenchmarkKind, InterferenceReport, Metric, MetricUnit};
    use crate::benchmarks::statistics::summarize;

    fn category(samples: &[f64]) -> CategoryResult {
        let summary = summarize(samples);
        CategoryResult {
            metrics: vec![Metric {
                id: "m".into(),
                label: "Metric".into(),
                unit: MetricUnit::Mops,
                higher_is_better: true,
                value: summary.as_ref().map_or(0.0, |s| s.median),
                summary,
                samples: samples.to_vec(),
                outliers: crate::benchmarks::statistics::outlier_indices(samples),
                score: None,
                note: None,
            }],
            ..CategoryResult::empty(BenchmarkKind::Cpu, CategoryStatus::Completed)
        }
    }

    #[test]
    fn stable_results_are_high() {
        let c = category(&[100.0, 100.5, 99.8, 100.2, 100.1, 99.9, 100.0]);
        assert_eq!(assess(&c), Some(Confidence::High));
        assert!(derived_warnings(&c).is_empty());
    }

    #[test]
    fn noisy_results_are_low_and_warned() {
        let c = category(&[100.0, 130.0, 80.0, 120.0, 90.0]);
        assert_eq!(assess(&c), Some(Confidence::Low));
        let w = derived_warnings(&c);
        assert_eq!(w[0].code, WarningCode::HighVariation);
    }

    #[test]
    fn few_samples_or_background_load_demote() {
        assert_eq!(
            assess(&category(&[100.0, 100.2, 100.1, 99.9])),
            Some(Confidence::Medium)
        );
        let mut c = category(&[100.0, 100.5, 99.8, 100.2, 100.1]);
        c.interference = Some(InterferenceReport {
            mean_external_cpu_percent: 30.0,
            max_external_cpu_percent: 60.0,
            samples: 10,
        });
        assert_eq!(assess(&c), Some(Confidence::Low));
        assert!(
            derived_warnings(&c)
                .iter()
                .any(|w| w.code == WarningCode::BackgroundActivityDuringRun)
        );
    }

    #[test]
    fn incomplete_categories_have_no_confidence() {
        let mut c = category(&[1.0, 1.0, 1.0, 1.0, 1.0]);
        c.status = CategoryStatus::Failed;
        assert_eq!(assess(&c), None);
    }
}
