//! Prediction-accuracy stats for the `/risk` subsystem — trend detection,
//! aggregate accuracy statistics, and the accuracy report display.
//! Extracted from commands_risk.rs (Day 127) to keep that module focused
//! on scoring and command handling; commands_risk.rs re-exports everything
//! here so call sites are unchanged.

use crate::commands_risk_snapshots::ValidationEvent;
use crate::format::*;

// ── Risk prediction accuracy tracking ──

/// Trend direction for accuracy over time.
#[derive(Debug, PartialEq)]
pub(crate) enum AccuracyTrend {
    Improving,
    Declining,
    Stable,
    Insufficient, // not enough data points
}

/// Aggregate accuracy statistics computed from validation history.
pub(crate) struct AccuracyStats {
    pub(crate) total_validations: usize,
    pub(crate) total_hits: usize,
    pub(crate) total_changed: usize,
    pub(crate) overall_hit_rate_pct: f64,
    pub(crate) trend: AccuracyTrend,
    pub(crate) best_day: Option<(u32, f64)>, // (day, accuracy_pct)
    pub(crate) worst_day: Option<(u32, f64)>, // (day, accuracy_pct)
}

/// Compute trend by comparing the average accuracy of the last N events
/// vs the first N events. Uses min(5, len/2) as window size.
fn compute_accuracy_trend(events: &[ValidationEvent]) -> AccuracyTrend {
    if events.len() < 2 {
        return AccuracyTrend::Insufficient;
    }

    let window = std::cmp::min(5, events.len() / 2).max(1);
    let first_avg: f64 =
        events[..window].iter().map(|e| e.accuracy_pct).sum::<f64>() / window as f64;
    let last_avg: f64 = events[events.len() - window..]
        .iter()
        .map(|e| e.accuracy_pct)
        .sum::<f64>()
        / window as f64;

    let diff = last_avg - first_avg;
    if diff > 5.0 {
        AccuracyTrend::Improving
    } else if diff < -5.0 {
        AccuracyTrend::Declining
    } else {
        AccuracyTrend::Stable
    }
}

/// Compute aggregate accuracy statistics from validation events.
pub(crate) fn compute_accuracy_stats(events: &[ValidationEvent]) -> AccuracyStats {
    if events.is_empty() {
        return AccuracyStats {
            total_validations: 0,
            total_hits: 0,
            total_changed: 0,
            overall_hit_rate_pct: 0.0,
            trend: AccuracyTrend::Insufficient,
            best_day: None,
            worst_day: None,
        };
    }

    let total_validations = events.len();
    let total_hits: usize = events.iter().map(|e| e.hit_count).sum();
    let total_changed: usize = events.iter().map(|e| e.total_changed).sum();
    let overall_hit_rate_pct = if total_changed > 0 {
        (total_hits as f64 / total_changed as f64) * 100.0
    } else {
        0.0
    };

    // Group by day — average accuracy per day for best/worst
    let mut day_accuracies: std::collections::BTreeMap<u32, Vec<f64>> =
        std::collections::BTreeMap::new();
    for e in events {
        day_accuracies
            .entry(e.day)
            .or_default()
            .push(e.accuracy_pct);
    }

    let mut best_day: Option<(u32, f64)> = None;
    let mut worst_day: Option<(u32, f64)> = None;
    for (&day, accs) in &day_accuracies {
        let avg = accs.iter().sum::<f64>() / accs.len() as f64;
        let avg_rounded = (avg * 10.0).round() / 10.0;
        match best_day {
            None => best_day = Some((day, avg_rounded)),
            Some((_, best_acc)) if avg_rounded > best_acc => best_day = Some((day, avg_rounded)),
            _ => {}
        }
        match worst_day {
            None => worst_day = Some((day, avg_rounded)),
            Some((_, worst_acc)) if avg_rounded < worst_acc => worst_day = Some((day, avg_rounded)),
            _ => {}
        }
    }

    let trend = compute_accuracy_trend(events);

    AccuracyStats {
        total_validations,
        total_hits,
        total_changed,
        overall_hit_rate_pct,
        trend,
        best_day,
        worst_day,
    }
}

/// Format the accuracy report as a compact box display.
pub(crate) fn format_accuracy_report(stats: &AccuracyStats) -> String {
    if stats.total_validations == 0 {
        return format!(
            "\n{BOLD}{CYAN}  No prediction accuracy data yet.{RESET}\n\n\
             {DIM}  Accuracy tracking starts automatically when watch commands\n\
             {DIM}  detect failures and validate them against risk predictions.\n\n\
             {DIM}  Run {RESET}/risk snapshot{DIM} first, then trigger a watch failure{RESET}\n\
             {DIM}  to begin collecting data.{RESET}\n"
        );
    }

    let hit_rate_rounded = (stats.overall_hit_rate_pct * 10.0).round() / 10.0;
    let trend_str = match stats.trend {
        AccuracyTrend::Improving => format!("{GREEN}↑ Improving{RESET}"),
        AccuracyTrend::Declining => format!("{RED}↓ Declining{RESET}"),
        AccuracyTrend::Stable => format!("{YELLOW}→ Stable{RESET}"),
        AccuracyTrend::Insufficient => format!("{DIM}? Too few data points{RESET}"),
    };

    let best_str = match stats.best_day {
        Some((day, pct)) => format!("Day {day} ({pct:.0}%)"),
        None => "—".to_string(),
    };
    let worst_str = match stats.worst_day {
        Some((day, pct)) => format!("Day {day} ({pct:.0}%)"),
        None => "—".to_string(),
    };

    format!(
        "\n{BOLD}  ╭─ Risk Prediction Accuracy ─╮{RESET}\n\
         {BOLD}  │{RESET} Validations:  {:<13}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Hit rate:     {:<13}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Trend:        {:<25}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Best day:     {:<13}{BOLD}│{RESET}\n\
         {BOLD}  │{RESET} Worst day:    {:<13}{BOLD}│{RESET}\n\
         {BOLD}  ╰────────────────────────────╯{RESET}\n",
        stats.total_validations,
        format!(
            "{hit_rate_rounded:.0}% ({}/{})",
            stats.total_hits, stats.total_changed
        ),
        trend_str,
        best_str,
        worst_str,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test category 7: Accuracy tracking ──

    #[test]
    fn test_compute_accuracy_stats_empty() {
        let stats = compute_accuracy_stats(&[]);
        assert_eq!(stats.total_validations, 0);
        assert_eq!(stats.trend, AccuracyTrend::Insufficient);
        assert!(stats.best_day.is_none());
        assert!(stats.worst_day.is_none());
    }

    #[test]
    fn test_compute_accuracy_stats_single_entry() {
        let events = vec![ValidationEvent {
            day: 110,
            hit_count: 3,
            total_changed: 5,
            accuracy_pct: 60.0,
        }];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.total_validations, 1);
        assert_eq!(stats.total_hits, 3);
        assert_eq!(stats.total_changed, 5);
        assert!((stats.overall_hit_rate_pct - 60.0).abs() < 0.1);
        assert_eq!(stats.trend, AccuracyTrend::Insufficient);
        assert_eq!(stats.best_day, Some((110, 60.0)));
        assert_eq!(stats.worst_day, Some((110, 60.0)));
    }

    #[test]
    fn test_compute_accuracy_trend_improving() {
        let events = vec![
            ValidationEvent {
                day: 100,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
            },
            ValidationEvent {
                day: 101,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 25.0,
            },
            ValidationEvent {
                day: 102,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
            },
            ValidationEvent {
                day: 103,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 60.0,
            },
            ValidationEvent {
                day: 104,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
            },
            ValidationEvent {
                day: 105,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
            },
        ];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Improving);
    }

    #[test]
    fn test_compute_accuracy_trend_declining() {
        let events = vec![
            ValidationEvent {
                day: 100,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
            },
            ValidationEvent {
                day: 101,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 75.0,
            },
            ValidationEvent {
                day: 102,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 60.0,
            },
            ValidationEvent {
                day: 103,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
            },
            ValidationEvent {
                day: 104,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
            },
            ValidationEvent {
                day: 105,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 15.0,
            },
        ];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Declining);
    }

    #[test]
    fn test_compute_accuracy_trend_stable() {
        let events = vec![
            ValidationEvent {
                day: 100,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 60.0,
            },
            ValidationEvent {
                day: 101,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 58.0,
            },
            ValidationEvent {
                day: 102,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 62.0,
            },
            ValidationEvent {
                day: 103,
                hit_count: 3,
                total_changed: 5,
                accuracy_pct: 59.0,
            },
        ];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Stable);
    }

    #[test]
    fn test_compute_accuracy_trend_insufficient() {
        let events = vec![ValidationEvent {
            day: 100,
            hit_count: 3,
            total_changed: 5,
            accuracy_pct: 60.0,
        }];
        let trend = compute_accuracy_trend(&events);
        assert_eq!(trend, AccuracyTrend::Insufficient);
    }

    #[test]
    fn test_compute_accuracy_stats_best_worst_day() {
        let events = vec![
            ValidationEvent {
                day: 108,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
            },
            ValidationEvent {
                day: 110,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
            },
            ValidationEvent {
                day: 115,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
            },
        ];
        let stats = compute_accuracy_stats(&events);
        assert_eq!(stats.best_day, Some((115, 80.0)));
        assert_eq!(stats.worst_day, Some((108, 20.0)));
    }

    #[test]
    fn test_compute_accuracy_stats_multiple_events_same_day() {
        let events = vec![
            ValidationEvent {
                day: 110,
                hit_count: 1,
                total_changed: 5,
                accuracy_pct: 20.0,
            },
            ValidationEvent {
                day: 110,
                hit_count: 4,
                total_changed: 5,
                accuracy_pct: 80.0,
            },
        ];
        let stats = compute_accuracy_stats(&events);
        // Average for day 110 = (20 + 80) / 2 = 50
        assert_eq!(stats.best_day, Some((110, 50.0)));
        assert_eq!(stats.worst_day, Some((110, 50.0)));
    }

    #[test]
    fn test_format_accuracy_report_empty() {
        let stats = compute_accuracy_stats(&[]);
        let report = format_accuracy_report(&stats);
        assert!(report.contains("No prediction accuracy data yet"));
        assert!(report.contains("/risk snapshot"));
    }

    #[test]
    fn test_format_accuracy_report_with_data() {
        let stats = AccuracyStats {
            total_validations: 12,
            total_hits: 7,
            total_changed: 12,
            overall_hit_rate_pct: 58.333,
            trend: AccuracyTrend::Improving,
            best_day: Some((115, 80.0)),
            worst_day: Some((108, 20.0)),
        };
        let report = format_accuracy_report(&stats);
        assert!(report.contains("Risk Prediction Accuracy"));
        assert!(report.contains("12"));
        assert!(report.contains("58%"));
        assert!(report.contains("7/12"));
        assert!(report.contains("Improving"));
        assert!(report.contains("Day 115"));
        assert!(report.contains("Day 108"));
    }
}
