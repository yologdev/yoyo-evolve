//! Emerging-risk / anticipatory detection for the `/risk` subsystem.
//!
//! Extracted from `commands_risk.rs` (Day 127) — the anticipatory layer that
//! flags files whose risk trajectory is *accelerating*: not yet in the top-N
//! by absolute score, but changing faster recently than their own baseline.
//! `commands_risk.rs` re-exports everything here so call sites are unchanged.

use crate::commands_risk::FileRisk;
use crate::format::{BOLD, DIM, RESET, YELLOW};

/// A file whose risk trajectory is accelerating — not yet in the top-N by
/// absolute score, but changing faster recently than its own baseline.
/// This is an *anticipatory* signal: the file is **about to become** fragile.
pub(crate) struct EmergingRisk {
    /// File path.
    pub path: String,
    /// Momentum: ratio of daily change rate in the last 7 days vs. last 30 days.
    /// Values > 1.0 mean the file is changing faster recently.
    pub momentum: f64,
    /// Current rank in the absolute risk list (0-indexed).
    pub current_rank: usize,
    /// Human-readable signals driving the acceleration.
    pub signals: Vec<String>,
}

/// Compute momentum for a file: ratio of its daily change rate over the last
/// 7 days vs. the last 30 days. Returns `(7d_count / 7) / (30d_count / 30)`.
///
/// - If both counts are zero → 0.0 (no activity).
/// - If 30-day count is zero but 7-day count > 0 → 3.0 (new hotspot).
fn compute_momentum(count_7d: u32, count_30d: u32) -> f64 {
    let c7 = count_7d as f64;
    let c30 = count_30d as f64;
    if c30 > 0.0 {
        (c7 / 7.0) / (c30 / 30.0)
    } else if c7 > 0.0 {
        3.0 // Appeared only in the last week — maximally accelerating
    } else {
        0.0
    }
}

/// Detect files whose risk trajectory is accelerating — moderate absolute risk
/// but changing faster recently than their own baseline. These are files that
/// are **about to become** fragile, the first genuinely allostatic signal.
///
/// A file qualifies as "emerging risk" if:
/// 1. Its momentum (7d vs 30d daily change rate ratio) exceeds `threshold` (default 1.5).
/// 2. It is NOT already in the top `exclude_top_n` absolute risk scores (default 5).
///
/// This is the inner, testable version. The public wrapper uses live data.
fn detect_emerging_risks_from(
    risks: &[FileRisk],
    counts_7: &[(String, u32)],
    counts_30: &[(String, u32)],
    revert_counts: &std::collections::HashMap<String, u32>,
    threshold: f64,
    exclude_top_n: usize,
) -> Vec<EmergingRisk> {
    let c7_map: std::collections::HashMap<&str, u32> =
        counts_7.iter().map(|(p, c)| (p.as_str(), *c)).collect();
    let c30_map: std::collections::HashMap<&str, u32> =
        counts_30.iter().map(|(p, c)| (p.as_str(), *c)).collect();

    // Build a set of top-N paths by absolute risk (already sorted descending)
    let top_n_paths: std::collections::HashSet<&str> = risks
        .iter()
        .take(exclude_top_n)
        .map(|r| r.path.as_str())
        .collect();

    let mut emerging: Vec<EmergingRisk> = Vec::new();

    for (rank, risk) in risks.iter().enumerate() {
        // Skip files already in the top-N — they're known risks, not emerging
        if top_n_paths.contains(risk.path.as_str()) {
            continue;
        }

        let c7 = *c7_map.get(risk.path.as_str()).unwrap_or(&0);
        let c30 = *c30_map.get(risk.path.as_str()).unwrap_or(&0);
        let momentum = compute_momentum(c7, c30);

        if momentum < threshold {
            continue;
        }

        // Must have at least 2 changes in the last 7 days to avoid noise
        // from single-touch files
        if c7 < 2 {
            continue;
        }

        // Build signal descriptions
        let mut signals = Vec::new();
        signals.push(format!("{c7} changes in 7d vs {} in 30d", c30));

        let rev = *revert_counts.get(risk.path.as_str()).unwrap_or(&0);
        if rev > 0 {
            signals.push(format!(
                "{rev} past revert{}",
                if rev > 1 { "s" } else { "" }
            ));
        }

        if !risk.signals.is_empty() {
            signals.push(format!("current: {}", risk.signals.join(" ")));
        }

        emerging.push(EmergingRisk {
            path: risk.path.clone(),
            momentum,
            current_rank: rank,
            signals,
        });
    }

    // Sort by momentum descending, with filename tiebreaker for determinism
    emerging.sort_by(|a, b| {
        b.momentum
            .partial_cmp(&a.momentum)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    emerging
}

/// Detect emerging-risk files using live git data.
/// Returns files with momentum > 1.5 that aren't in the top 5 by absolute risk.
pub(crate) fn detect_emerging_risks(risks: &[FileRisk]) -> Vec<EmergingRisk> {
    let counts_7 = crate::git::file_change_counts(7);
    let counts_30 = crate::git::file_change_counts(30);
    let revert_counts = crate::commands_risk::revert_history();
    detect_emerging_risks_from(risks, &counts_7, &counts_30, &revert_counts, 1.5, 5)
}

/// Format emerging-risk files into a report section.
/// Returns an empty string if there are no emerging risks.
pub(crate) fn format_emerging_risks(emerging: &[EmergingRisk]) -> String {
    if emerging.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(&format!(
        "  ⚡ {BOLD}Emerging Risks{RESET} {DIM}(accelerating — not yet top-5){RESET}\n\n"
    ));

    for er in emerging.iter().take(10) {
        let path_display = &er.path;
        let padded_path = if path_display.len() < 34 {
            format!("{path_display:<34}")
        } else {
            path_display.to_string()
        };
        out.push_str(&format!(
            "  {YELLOW}{:.1}x{RESET}  #{:<4} {padded_path}{DIM}{}{RESET}\n",
            er.momentum,
            er.current_rank + 1,
            er.signals.join(" · "),
        ));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_momentum_normal() {
        // 4 changes in 7d, 8 in 30d → (4/7) / (8/30) = 0.571 / 0.267 ≈ 2.14
        let m = compute_momentum(4, 8);
        assert!((m - 2.14).abs() < 0.1, "expected ~2.14, got {m}");
    }

    #[test]
    fn test_compute_momentum_zero_both() {
        assert_eq!(compute_momentum(0, 0), 0.0);
    }

    #[test]
    fn test_compute_momentum_only_7d() {
        // 30-day is zero, 7-day is positive → maximally accelerating (3.0)
        assert_eq!(compute_momentum(3, 0), 3.0);
    }

    #[test]
    fn test_compute_momentum_uniform() {
        // 7 changes in 7d, 30 in 30d → (7/7)/(30/30) = 1.0 — stable
        let m = compute_momentum(7, 30);
        assert!((m - 1.0).abs() < 0.01, "expected ~1.0, got {m}");
    }

    #[test]
    fn test_detect_emerging_risks_flags_accelerating_file() {
        // Create a scenario: file_a is top-1 (high risk), file_b is lower rank
        // but has high momentum (many 7d changes, few 30d changes)
        let risks = vec![
            FileRisk {
                path: "src/top_risk.rs".into(),
                score: 0.9,
                signals: vec!["▲churn", "▲size"],
                test_density: 1.0,
            },
            FileRisk {
                path: "src/stable.rs".into(),
                score: 0.5,
                signals: vec![],
                test_density: 3.0,
            },
            FileRisk {
                path: "src/emerging.rs".into(),
                score: 0.3,
                signals: vec!["▲recent"],
                test_density: 2.0,
            },
        ];

        let counts_7 = vec![
            ("src/top_risk.rs".into(), 5u32),
            ("src/stable.rs".into(), 1),
            ("src/emerging.rs".into(), 4), // 4 changes in 7 days — burst
        ];
        let counts_30 = vec![
            ("src/top_risk.rs".into(), 15u32),
            ("src/stable.rs".into(), 10),
            ("src/emerging.rs".into(), 5), // only 5 in 30 days
        ];
        let revert_counts = std::collections::HashMap::new();

        let emerging =
            detect_emerging_risks_from(&risks, &counts_7, &counts_30, &revert_counts, 1.5, 1);

        // emerging.rs should be flagged: momentum = (4/7)/(5/30) = 0.571/0.167 ≈ 3.43
        assert!(!emerging.is_empty(), "expected at least one emerging risk");
        assert_eq!(emerging[0].path, "src/emerging.rs");
        assert!(
            emerging[0].momentum > 1.5,
            "momentum should exceed threshold"
        );
    }

    #[test]
    fn test_detect_emerging_risks_excludes_top_n() {
        // file_a is rank 0 (top-1) and has high momentum — should be excluded
        let risks = vec![
            FileRisk {
                path: "src/already_top.rs".into(),
                score: 0.9,
                signals: vec!["▲churn"],
                test_density: 1.0,
            },
            FileRisk {
                path: "src/other.rs".into(),
                score: 0.2,
                signals: vec![],
                test_density: 5.0,
            },
        ];

        let counts_7 = vec![
            ("src/already_top.rs".into(), 6u32),
            ("src/other.rs".into(), 0),
        ];
        let counts_30 = vec![
            ("src/already_top.rs".into(), 7u32),
            ("src/other.rs".into(), 2),
        ];
        let revert_counts = std::collections::HashMap::new();

        // exclude_top_n = 1 means already_top is excluded
        let emerging =
            detect_emerging_risks_from(&risks, &counts_7, &counts_30, &revert_counts, 1.5, 1);

        // already_top has high momentum but is top-1 → excluded
        // other has low momentum → not flagged
        assert!(
            emerging.is_empty(),
            "top-N files should be excluded from emerging"
        );
    }

    #[test]
    fn test_detect_emerging_risks_minimum_changes() {
        // A file with only 1 change in 7d should not be flagged even with high momentum
        let risks = vec![FileRisk {
            path: "src/single_touch.rs".into(),
            score: 0.2,
            signals: vec![],
            test_density: 0.0,
        }];

        let counts_7 = vec![("src/single_touch.rs".into(), 1u32)]; // only 1 change
        let counts_30 = vec![]; // 0 in 30d → momentum = 3.0
        let revert_counts = std::collections::HashMap::new();

        let emerging =
            detect_emerging_risks_from(&risks, &counts_7, &counts_30, &revert_counts, 1.5, 0);

        assert!(
            emerging.is_empty(),
            "single-touch files should be filtered out"
        );
    }

    #[test]
    fn test_detect_emerging_risks_includes_revert_signal() {
        let risks = vec![
            FileRisk {
                path: "src/top.rs".into(),
                score: 0.9,
                signals: vec![],
                test_density: 0.0,
            },
            FileRisk {
                path: "src/reverted.rs".into(),
                score: 0.3,
                signals: vec![],
                test_density: 0.0,
            },
        ];

        let counts_7 = vec![("src/top.rs".into(), 2u32), ("src/reverted.rs".into(), 3)];
        let counts_30 = vec![("src/top.rs".into(), 10u32), ("src/reverted.rs".into(), 4)];
        let mut revert_counts = std::collections::HashMap::new();
        revert_counts.insert("src/reverted.rs".to_string(), 2u32);

        let emerging =
            detect_emerging_risks_from(&risks, &counts_7, &counts_30, &revert_counts, 1.5, 1);

        assert!(!emerging.is_empty());
        let rev = &emerging[0];
        assert_eq!(rev.path, "src/reverted.rs");
        // Should mention reverts in signals
        let joined = rev.signals.join(" | ");
        assert!(
            joined.contains("revert"),
            "expected revert signal, got: {joined}"
        );
    }

    #[test]
    fn test_detect_emerging_risks_sorted_by_momentum() {
        let risks = vec![
            FileRisk {
                path: "src/top.rs".into(),
                score: 0.9,
                signals: vec![],
                test_density: 0.0,
            },
            FileRisk {
                path: "src/fast.rs".into(),
                score: 0.4,
                signals: vec![],
                test_density: 0.0,
            },
            FileRisk {
                path: "src/faster.rs".into(),
                score: 0.3,
                signals: vec![],
                test_density: 0.0,
            },
        ];

        let counts_7 = vec![
            ("src/top.rs".into(), 3u32),
            ("src/fast.rs".into(), 3),   // momentum = (3/7)/(4/30) ≈ 3.21
            ("src/faster.rs".into(), 5), // momentum = (5/7)/(6/30) ≈ 3.57
        ];
        let counts_30 = vec![
            ("src/top.rs".into(), 10u32),
            ("src/fast.rs".into(), 4),
            ("src/faster.rs".into(), 6),
        ];
        let revert_counts = std::collections::HashMap::new();

        let emerging =
            detect_emerging_risks_from(&risks, &counts_7, &counts_30, &revert_counts, 1.5, 1);

        assert_eq!(emerging.len(), 2);
        // faster.rs should be first (higher momentum)
        assert_eq!(emerging[0].path, "src/faster.rs");
        assert_eq!(emerging[1].path, "src/fast.rs");
        assert!(emerging[0].momentum > emerging[1].momentum);
    }

    #[test]
    fn test_format_emerging_risks_empty() {
        let result = format_emerging_risks(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_emerging_risks_shows_content() {
        let emerging = vec![EmergingRisk {
            path: "src/hot.rs".into(),
            momentum: 2.5,
            current_rank: 7,
            signals: vec!["4 changes in 7d vs 5 in 30d".into()],
        }];

        let result = format_emerging_risks(&emerging);
        assert!(result.contains("Emerging Risks"), "should have header");
        assert!(result.contains("src/hot.rs"), "should show file path");
        assert!(result.contains("2.5x"), "should show momentum");
    }
}
