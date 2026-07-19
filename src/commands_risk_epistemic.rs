//! Epistemic ranking for `/risk` — rank files by how little the graded
//! validation outcomes have taught the risk model about them.
//!
//! This is the dream's epistemic-appetite milestone (DREAM.md, Day 140),
//! ranking half only: a file is high-epistemic-value (the model is blind
//! about it) when it has been *predicted* (reactive `top_10` or anticipatory
//! `emerging`) but never *graded* (never appeared in any validation event's
//! outcome, neither as hit nor surprise), when the two prediction columns
//! disagree about it (an outcome touching it would settle which model is
//! right), or — at lower weight — when it went stale (last seen many
//! snapshots ago with no graded event since).
//!
//! Steering the self-driven planner slot at this ranking is a named
//! follow-up, not part of this module.

use crate::commands_risk_snapshots::{GradedEvent, ParsedSnapshot};
use crate::format::{BOLD, CYAN, DIM, RESET, YELLOW};

/// Weight for "predicted but never graded" — the strongest blindness signal:
/// the model has made claims about this file that no outcome ever tested.
pub(crate) const W_NEVER_GRADED: f64 = 2.0;

/// Weight per recent snapshot where the reactive (`top_10`) and anticipatory
/// (`emerging`) columns disagree about a file — an outcome touching it would
/// settle which model is right.
pub(crate) const W_DISAGREE: f64 = 1.0;

/// Lower weight for staleness: last seen ≥ [`STALE_SNAPSHOT_GAP`] snapshots
/// ago with no graded event since.
pub(crate) const W_STALE: f64 = 0.5;

/// How many of the most recent snapshots are checked for column disagreement.
pub(crate) const DISAGREE_WINDOW: usize = 3;

/// A file last seen this many snapshots ago (or more) counts as stale.
pub(crate) const STALE_SNAPSHOT_GAP: usize = 5;

/// Default number of entries shown in the report.
const REPORT_TOP_N: usize = 10;

/// One file the graded outcomes have taught the model little about.
pub(crate) struct EpistemicEntry {
    pub(crate) path: String,
    pub(crate) score: f64,
    /// Human-readable reasons, e.g. "predicted 5×, never graded".
    pub(crate) reasons: Vec<String>,
}

/// Per-file aggregate built while scanning snapshots.
#[derive(Default)]
struct FileStats {
    predicted_count: usize,
    emerging_count: usize,
    /// Index into `snapshots` of the last snapshot mentioning this file.
    last_seen_index: usize,
    /// `day` of that snapshot (used to check "graded since").
    last_seen_day: u64,
}

/// Rank files by epistemic value — how little the graded outcomes have
/// taught the model about them. Pure: reads only its arguments.
///
/// Files whose every signal is satisfied (graded, columns agree, fresh)
/// score 0 and are absent from the result — the model has nothing left to
/// learn about them from this view.
pub(crate) fn compute_epistemic_ranking(
    snapshots: &[ParsedSnapshot],
    events: &[GradedEvent],
) -> Vec<EpistemicEntry> {
    use std::collections::{HashMap, HashSet};

    if snapshots.is_empty() {
        return Vec::new();
    }

    // Every path that ever appeared in a graded outcome, with the latest
    // day it was graded (hits and surprises both count — each taught the
    // model something).
    let mut graded_latest_day: HashMap<&str, u64> = HashMap::new();
    for ev in events {
        for p in &ev.paths {
            let entry = graded_latest_day.entry(p.as_str()).or_insert(0);
            *entry = (*entry).max(ev.day);
        }
    }

    // Aggregate per-file stats across all snapshots.
    let mut stats: HashMap<String, FileStats> = HashMap::new();
    for (idx, snap) in snapshots.iter().enumerate() {
        let mut seen: HashSet<&str> = HashSet::new();
        for p in &snap.predicted {
            let s = stats.entry(p.clone()).or_default();
            s.predicted_count += 1;
            seen.insert(p.as_str());
        }
        for p in &snap.emerging {
            let s = stats.entry(p.clone()).or_default();
            s.emerging_count += 1;
            seen.insert(p.as_str());
        }
        for p in seen {
            let s = stats.get_mut(p).expect("just inserted");
            s.last_seen_index = idx;
            s.last_seen_day = snap.day;
        }
    }

    // Column disagreement over the most recent DISAGREE_WINDOW snapshots:
    // file appears in exactly one of (predicted, emerging).
    let window_start = snapshots.len().saturating_sub(DISAGREE_WINDOW);
    let mut disagree_count: HashMap<&str, usize> = HashMap::new();
    for snap in &snapshots[window_start..] {
        let predicted: HashSet<&str> = snap.predicted.iter().map(String::as_str).collect();
        let emerging: HashSet<&str> = snap.emerging.iter().map(String::as_str).collect();
        for p in predicted.symmetric_difference(&emerging) {
            *disagree_count.entry(p).or_insert(0) += 1;
        }
    }

    let last_index = snapshots.len() - 1;
    let mut entries: Vec<EpistemicEntry> = Vec::new();
    for (path, s) in &stats {
        let mut score = 0.0;
        let mut reasons = Vec::new();

        let appearances = s.predicted_count + s.emerging_count;
        let graded_day = graded_latest_day.get(path.as_str()).copied();

        if graded_day.is_none() {
            score += W_NEVER_GRADED;
            reasons.push(format!("predicted {appearances}×, never graded"));
        }

        if let Some(&d) = disagree_count.get(path.as_str()) {
            if d > 0 {
                score += W_DISAGREE * d as f64;
                reasons.push(format!(
                    "reactive/emerging disagree in {d} of last {} snapshots",
                    snapshots.len().min(DISAGREE_WINDOW)
                ));
            }
        }

        let snapshots_ago = last_index - s.last_seen_index;
        let graded_since = graded_day.is_some_and(|d| d >= s.last_seen_day);
        if snapshots_ago >= STALE_SNAPSHOT_GAP && !graded_since {
            score += W_STALE;
            reasons.push(format!(
                "last seen {snapshots_ago} snapshots ago, no graded event since"
            ));
        }

        if score > 0.0 {
            entries.push(EpistemicEntry {
                path: path.clone(),
                score,
                reasons,
            });
        }
    }

    // Highest epistemic value first; tie-break by path for determinism.
    entries.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    entries
}

/// Format the epistemic ranking as a report. Honest empty states — never a
/// silent nothing.
pub(crate) fn format_epistemic_report(
    snapshots: &[ParsedSnapshot],
    entries: &[EpistemicEntry],
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n{BOLD}{CYAN}🔍 Epistemic view — where graded outcomes have taught the model least{RESET}\n\n"
    ));

    if snapshots.is_empty() {
        out.push_str(&format!(
            "  {DIM}no snapshots yet — run `yoyo risk snapshot` first{RESET}\n"
        ));
        return out;
    }

    if entries.is_empty() {
        out.push_str(&format!(
            "  {DIM}no ungraded predictions — the model has been graded on everything it predicted{RESET}\n"
        ));
        return out;
    }

    for (i, e) in entries.iter().take(REPORT_TOP_N).enumerate() {
        out.push_str(&format!(
            "  {:>2}. {YELLOW}{:<40}{RESET} {:.1}\n",
            i + 1,
            e.path,
            e.score
        ));
        for r in &e.reasons {
            out.push_str(&format!("      {DIM}• {r}{RESET}\n"));
        }
    }
    out.push_str(&format!(
        "\n  {DIM}high score = the model is blindest here; an outcome touching these files teaches the most{RESET}\n"
    ));
    out
}

/// Handle `/risk epistemic` — read the existing snapshot + validation JSONL,
/// compute the ranking, print the report.
pub(crate) fn handle_risk_epistemic() {
    use crate::commands_risk_snapshots::{
        parse_all_snapshots, parse_graded_events, RISK_SNAPSHOT_PATH, RISK_VALIDATION_PATH,
    };

    let snapshot_content =
        std::fs::read_to_string(std::path::Path::new(RISK_SNAPSHOT_PATH)).unwrap_or_default();
    let validation_content =
        std::fs::read_to_string(std::path::Path::new(RISK_VALIDATION_PATH)).unwrap_or_default();

    let snapshots = parse_all_snapshots(&snapshot_content);
    let events = parse_graded_events(&validation_content);
    let entries = compute_epistemic_ranking(&snapshots, &events);
    print!("{}", format_epistemic_report(&snapshots, &entries));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(day: u64, predicted: &[&str], emerging: &[&str]) -> ParsedSnapshot {
        ParsedSnapshot {
            day,
            git_hash: format!("hash{day}"),
            predicted: predicted.iter().map(|s| s.to_string()).collect(),
            emerging: emerging.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn graded(day: u64, paths: &[&str]) -> GradedEvent {
        GradedEvent {
            day,
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_never_graded_ranks_above_graded() {
        // a.rs predicted and graded; b.rs predicted, never graded.
        let snapshots = vec![snap(100, &["src/a.rs", "src/b.rs"], &[])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events);
        let pos_b = ranking.iter().position(|e| e.path == "src/b.rs");
        let pos_a = ranking.iter().position(|e| e.path == "src/a.rs");
        assert!(pos_b.is_some(), "never-graded file must appear");
        match pos_a {
            None => {} // graded, no other signal → absent (score 0)
            Some(pa) => assert!(pos_b.unwrap() < pa, "never-graded ranks above graded"),
        }
        let b = &ranking[pos_b.unwrap()];
        assert!(
            b.reasons.iter().any(|r| r.contains("never graded")),
            "reason names the blindness: {:?}",
            b.reasons
        );
    }

    #[test]
    fn test_column_disagreement_gets_reason() {
        // c.rs appears in emerging but not top_10 — columns disagree.
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/c.rs"])];
        let events = vec![graded(100, &["src/a.rs", "src/c.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events);
        let c = ranking
            .iter()
            .find(|e| e.path == "src/c.rs")
            .expect("disagreement file must appear even when graded");
        assert!(
            c.reasons.iter().any(|r| r.contains("disagree")),
            "reason names the disagreement: {:?}",
            c.reasons
        );
    }

    #[test]
    fn test_graded_agreeing_file_absent() {
        // a.rs in both columns (agree) and graded — nothing left to learn.
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/a.rs"])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events);
        assert!(
            !ranking.iter().any(|e| e.path == "src/a.rs"),
            "fully-graded, agreeing file must rank absent"
        );
    }

    #[test]
    fn test_empty_snapshots_honest_message() {
        let ranking = compute_epistemic_ranking(&[], &[]);
        assert!(ranking.is_empty());
        let report = format_epistemic_report(&[], &ranking);
        assert!(
            report.contains("no snapshots yet"),
            "empty state must be honest, got: {report}"
        );
    }

    #[test]
    fn test_all_graded_honest_message() {
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/a.rs"])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events);
        let report = format_epistemic_report(&snapshots, &ranking);
        assert!(
            report.contains("no ungraded predictions"),
            "all-graded state must be honest, got: {report}"
        );
    }

    #[test]
    fn test_stale_file_gets_lower_weight_reason() {
        // stale.rs seen only in the first of 7 snapshots, never graded.
        let mut snapshots = vec![snap(100, &["src/stale.rs"], &[])];
        for d in 101..107 {
            snapshots.push(snap(d, &["src/other.rs"], &[]));
        }
        let ranking = compute_epistemic_ranking(&snapshots, &[]);
        let stale = ranking
            .iter()
            .find(|e| e.path == "src/stale.rs")
            .expect("stale file must appear");
        assert!(
            stale
                .reasons
                .iter()
                .any(|r| r.contains("snapshots ago, no graded event since")),
            "stale reason present: {:?}",
            stale.reasons
        );
        // never-graded + stale > never-graded alone
        assert!(stale.score > W_NEVER_GRADED, "stale adds weight");
    }

    #[test]
    fn test_disagreement_window_only_recent() {
        // d.rs disagrees only in an old snapshot, outside the window of 3.
        let snapshots = vec![
            snap(100, &["src/d.rs"], &[]), // disagree, but old
            snap(101, &["src/x.rs"], &["src/x.rs"]),
            snap(102, &["src/x.rs"], &["src/x.rs"]),
            snap(103, &["src/x.rs"], &["src/x.rs"]),
        ];
        let events = vec![graded(104, &["src/d.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events);
        let d = ranking.iter().find(|e| e.path == "src/d.rs");
        assert!(
            d.is_none(),
            "old disagreement outside window must not score"
        );
    }

    #[test]
    fn test_score_is_sum_of_signals() {
        // e.rs: never graded (2.0) + disagrees in 1 recent snapshot (1.0)
        let snapshots = vec![snap(100, &[], &["src/e.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &[]);
        let e = ranking
            .iter()
            .find(|en| en.path == "src/e.rs")
            .expect("must appear");
        assert!(
            (e.score - (W_NEVER_GRADED + W_DISAGREE)).abs() < 1e-9,
            "score {} should be W_NEVER_GRADED + W_DISAGREE",
            e.score
        );
    }

    #[test]
    fn test_report_lists_reasons() {
        let snapshots = vec![snap(100, &["src/b.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[]);
        let report = format_epistemic_report(&snapshots, &ranking);
        assert!(report.contains("src/b.rs"));
        assert!(report.contains("never graded"));
    }
}
