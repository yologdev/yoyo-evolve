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

/// Maximum weight per recent snapshot where the reactive (`top_10`) and
/// anticipatory (`emerging`) columns disagree about a file — an outcome
/// touching it would settle which model is right.
///
/// Scaling (Day 144): a flat per-snapshot count made large multi-way ties
/// (every disagreeing file scored identically), so each disagreeing snapshot
/// now contributes `W_DISAGREE × magnitude` where magnitude ∈ (0, 1] is how
/// strongly the claiming column ranks the file: top of its list = 1.0 (a
/// full-throated claim the other column ignores — full weight), position
/// `i` of `n` = `(n − i) / n` (proportional). `W_DISAGREE` remains the
/// per-snapshot maximum.
pub(crate) const W_DISAGREE: f64 = 1.0;

/// Lower weight for staleness: last seen ≥ [`STALE_SNAPSHOT_GAP`] snapshots
/// ago with no graded event since.
pub(crate) const W_STALE: f64 = 0.5;

/// How many of the most recent snapshots are checked for column disagreement.
pub(crate) const DISAGREE_WINDOW: usize = 3;

/// A file last seen this many snapshots ago (or more) counts as stale.
pub(crate) const STALE_SNAPSHOT_GAP: usize = 5;

/// Two epistemic scores within this distance count as tied and fall through
/// to the tie-break: current risk score (higher first), then path.
pub(crate) const SCORE_EPSILON: f64 = 1e-6;

/// Default number of entries shown in the report.
const REPORT_TOP_N: usize = 10;

/// How many never-forecast files are listed as a sample. The honest *total*
/// is always printed alongside — the cap is a display budget, not a claim.
pub(crate) const NEVER_FORECAST_SAMPLE: usize = 5;

/// A file the risk model scores but that has **never once** been forecast —
/// absent from every snapshot's reactive (`predicted`) and anticipatory
/// (`emerging`) column alike.
///
/// This is the ranking's structural blind spot (Day 149): the epistemic score
/// is built by iterating snapshot columns, so a file that was never guessed
/// about gets no entry at all — it is invisible to the very view whose job is
/// to find where the model is blindest. A detector for *known* unknowns.
/// Rather than blend these into the score (a list where everything is equally
/// unknown ranks nothing), they are an explicit third value.
pub(crate) struct NeverForecast {
    pub(crate) path: String,
    pub(crate) risk_score: f64,
}

/// One file the graded outcomes have taught the model little about.
pub(crate) struct EpistemicEntry {
    pub(crate) path: String,
    pub(crate) score: f64,
    /// Human-readable reasons, e.g. "predicted 5×, never graded".
    pub(crate) reasons: Vec<String>,
    /// Current risk score, used only to break epistemic-score ties.
    /// `None` is the explicit abstention case: the risk model has no score
    /// for this file — it sorts after scored files within a tie.
    pub(crate) risk_score: Option<f64>,
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
/// `risk_scores` (path → current risk score, as returned by
/// `commands_risk::top_risk_files`) is used only to break ties.
///
/// Files whose every signal is satisfied (graded, columns agree, fresh)
/// score 0 and are absent from the result — the model has nothing left to
/// learn about them from this view.
pub(crate) fn compute_epistemic_ranking(
    snapshots: &[ParsedSnapshot],
    events: &[GradedEvent],
    risk_scores: &[(String, f64)],
) -> Vec<EpistemicEntry> {
    use std::collections::{HashMap, HashSet};

    if snapshots.is_empty() {
        return Vec::new();
    }

    // Tie-break lookup: path → current risk score.
    let score_by_path: HashMap<&str, f64> =
        risk_scores.iter().map(|(p, s)| (p.as_str(), *s)).collect();

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
    // file appears in exactly one of (predicted, emerging). Each disagreeing
    // snapshot contributes a magnitude in (0, 1] — how strongly the claiming
    // column ranks the file (see W_DISAGREE docs) — so stronger unresolved
    // claims outrank weaker ones instead of tying flat.
    let window_start = snapshots.len().saturating_sub(DISAGREE_WINDOW);
    let mut disagree_count: HashMap<&str, usize> = HashMap::new();
    let mut disagree_magnitude: HashMap<&str, f64> = HashMap::new();
    for snap in &snapshots[window_start..] {
        let predicted: HashSet<&str> = snap.predicted.iter().map(String::as_str).collect();
        let emerging: HashSet<&str> = snap.emerging.iter().map(String::as_str).collect();
        for p in predicted.symmetric_difference(&emerging) {
            let claiming = if predicted.contains(p) {
                &snap.predicted
            } else {
                &snap.emerging
            };
            *disagree_count.entry(p).or_insert(0) += 1;
            *disagree_magnitude.entry(p).or_insert(0.0) += rank_magnitude(claiming, p);
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
                let magnitude = disagree_magnitude
                    .get(path.as_str())
                    .copied()
                    .unwrap_or(d as f64);
                score += W_DISAGREE * magnitude;
                reasons.push(format!(
                    "reactive/emerging disagree in {d} of last {} snapshots (magnitude {magnitude:.2})",
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
                risk_score: score_by_path.get(path.as_str()).copied(),
            });
        }
    }

    // Highest epistemic value first. Scores within SCORE_EPSILON are tied
    // and fall through to the current risk score (higher first) — a flat
    // ranking steers the planner arbitrarily; risk makes the choice
    // meaningful. A file the risk model has no score for (explicit
    // abstention, `risk_score: None`) sorts after scored files within a
    // tie; final fallback is path, for full determinism.
    entries.sort_by(|a, b| {
        use std::cmp::Ordering;
        if (a.score - b.score).abs() > SCORE_EPSILON {
            return b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal);
        }
        match (a.risk_score, b.risk_score) {
            (Some(ra), Some(rb)) => rb
                .partial_cmp(&ra)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.path.cmp(&b.path),
        }
    });
    entries
}

/// How strongly a column claims `path`: 1.0 for the top of the list, down to
/// `1/len` for the last entry, 0.0 if absent. Both columns are ordered
/// strongest-first (top_10 by risk score, emerging by momentum), so position
/// is an honest proxy for claim strength.
fn rank_magnitude(list: &[String], path: &str) -> f64 {
    match list.iter().position(|p| p == path) {
        Some(idx) if !list.is_empty() => (list.len() - idx) as f64 / list.len() as f64,
        _ => 0.0,
    }
}

/// Every scored path that appears in **no** snapshot's `predicted` column and
/// **no** snapshot's `emerging` column, across all snapshots — the files the
/// epistemic ranking structurally cannot see.
///
/// Ordering: current risk score descending, then path ascending (fully
/// deterministic). Rationale — a file that *changes* (it has a risk score at
/// all) yet has never once been forecast is the most consequential blind spot,
/// so churny-but-unforecast files come first.
///
/// Returns empty when `snapshots` is empty: with no predictions on record
/// there is nothing to be blind *relative to*, and claiming every scored file
/// is unforecast would be a false "everything is blind" verdict.
///
/// Honest limitation (stated in the report too): the universe here is only
/// what the risk model scores. A file with no recent churn has no risk score
/// and is invisible to both views.
pub(crate) fn never_forecast_files(
    snapshots: &[ParsedSnapshot],
    risk_scores: &[(String, f64)],
) -> Vec<NeverForecast> {
    use std::collections::HashSet;

    if snapshots.is_empty() {
        return Vec::new();
    }

    let mut forecast: HashSet<&str> = HashSet::new();
    for snap in snapshots {
        for p in snap.predicted.iter().chain(snap.emerging.iter()) {
            forecast.insert(p.as_str());
        }
    }

    let mut out: Vec<NeverForecast> = risk_scores
        .iter()
        .filter(|(p, _)| !forecast.contains(p.as_str()))
        .map(|(p, s)| NeverForecast {
            path: p.clone(),
            risk_score: *s,
        })
        .collect();
    out.sort_by(|a, b| {
        use std::cmp::Ordering;
        b.risk_score
            .partial_cmp(&a.risk_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

/// Format the epistemic ranking as a report. Honest empty states — never a
/// silent nothing.
pub(crate) fn format_epistemic_report(
    snapshots: &[ParsedSnapshot],
    entries: &[EpistemicEntry],
    never: &[NeverForecast],
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
    } else {
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
        // Honest ordering note: if any displayed neighbours are tied (within
        // SCORE_EPSILON), say so — the order among them came from the tie-break,
        // not the epistemic score.
        let shown = &entries[..entries.len().min(REPORT_TOP_N)];
        let has_tie = shown
            .windows(2)
            .any(|w| (w[0].score - w[1].score).abs() <= SCORE_EPSILON);
        if has_tie {
            out.push_str(&format!(
                "\n  {DIM}note: tied scores are ordered by current risk score (higher first), then path{RESET}\n"
            ));
        }
        out.push_str(&format!(
            "\n  {DIM}high score = the model is blindest here; an outcome touching these files teaches the most{RESET}\n"
        ));
    }

    // The unknown unknowns, kept as an explicit separate section — merging
    // them into the ranked list above would flatten it (a list where
    // everything is equally unknown ranks nothing).
    //
    // Consumer note: rows deliberately do NOT take the `N. path score` shape
    // that `scripts/extract_trajectory.py::EPISTEMIC_ENTRY_RE` matches, and
    // use the `◦` glyph rather than the `•` that parser appends to the
    // previous ranked entry's reasons. The chosen guard is on the parser
    // side: it stops collecting at the "never forecast" header line (see
    // `EPISTEMIC_NEVER_HEADER_RE` there). The distinct glyph is belt-and-
    // braces for any other reader.
    if !never.is_empty() {
        out.push_str(&format!(
            "\n  {YELLOW}⚠ never forecast{RESET} {DIM}— {} scored file{} {} never appeared in any prediction{RESET}\n",
            never.len(),
            if never.len() == 1 { "" } else { "s" },
            if never.len() == 1 { "has" } else { "have" },
        ));
        for n in never.iter().take(NEVER_FORECAST_SAMPLE) {
            out.push_str(&format!(
                "  ◦ {YELLOW}{}{RESET} {DIM}(risk {:.1}){RESET}\n",
                n.path, n.risk_score
            ));
        }
        if never.len() > NEVER_FORECAST_SAMPLE {
            out.push_str(&format!(
                "    {DIM}... (+{} more){RESET}\n",
                never.len() - NEVER_FORECAST_SAMPLE
            ));
        }
        // Keep each sentence on one line: the caveat is the honest part of this
        // section, and a mid-sentence wrap makes it unmatchable for any reader
        // (test or human) scanning line by line.
        out.push_str(&format!(
            "    {DIM}the ranking above cannot see these — it is built from files I once guessed about.{RESET}\n"
        ));
        out.push_str(&format!(
            "    {DIM}Files with no recent churn have no risk score and are invisible to both views.{RESET}\n"
        ));
    }
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
    // Current risk scores, used only to break epistemic ties.
    let risk_scores = crate::commands_risk::top_risk_files(usize::MAX);
    let entries = compute_epistemic_ranking(&snapshots, &events, &risk_scores);
    let never = never_forecast_files(&snapshots, &risk_scores);
    print!("{}", format_epistemic_report(&snapshots, &entries, &never));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(day: u64, predicted: &[&str], emerging: &[&str]) -> ParsedSnapshot {
        ParsedSnapshot {
            day,
            git_hash: format!("hash{day}"),
            ts: format!("2026-07-{:02}T00:00:00Z", (day % 28) + 1),
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
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[]);
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
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[]);
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
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[]);
        assert!(
            !ranking.iter().any(|e| e.path == "src/a.rs"),
            "fully-graded, agreeing file must rank absent"
        );
    }

    #[test]
    fn test_empty_snapshots_honest_message() {
        let ranking = compute_epistemic_ranking(&[], &[], &[]);
        assert!(ranking.is_empty());
        let report = format_epistemic_report(&[], &ranking, &[]);
        assert!(
            report.contains("no snapshots yet"),
            "empty state must be honest, got: {report}"
        );
    }

    #[test]
    fn test_all_graded_honest_message() {
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/a.rs"])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[]);
        let report = format_epistemic_report(&snapshots, &ranking, &[]);
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
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
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
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[]);
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
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
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
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
        let report = format_epistemic_report(&snapshots, &ranking, &[]);
        assert!(report.contains("src/b.rs"));
        assert!(report.contains("never graded"));
    }

    #[test]
    fn test_larger_disagreement_magnitude_ranks_higher() {
        // Both files never graded and both disagree once, but top.rs sits at
        // the top of the predicted column (magnitude 1.0) while bottom.rs is
        // last (magnitude 0.5) — the stronger unresolved claim ranks higher.
        let snapshots = vec![snap(100, &["src/top.rs", "src/bottom.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
        let pos_top = ranking
            .iter()
            .position(|e| e.path == "src/top.rs")
            .expect("top must appear");
        let pos_bottom = ranking
            .iter()
            .position(|e| e.path == "src/bottom.rs")
            .expect("bottom must appear");
        assert!(
            pos_top < pos_bottom,
            "larger-magnitude disagreement must rank higher"
        );
        let top = &ranking[pos_top];
        let bottom = &ranking[pos_bottom];
        assert!(
            (top.score - (W_NEVER_GRADED + W_DISAGREE)).abs() < 1e-9,
            "top-ranked claim gets full disagree weight, got {}",
            top.score
        );
        assert!(
            (bottom.score - (W_NEVER_GRADED + W_DISAGREE * 0.5)).abs() < 1e-9,
            "bottom-ranked claim gets proportional weight, got {}",
            bottom.score
        );
    }

    #[test]
    fn test_tie_break_by_risk_score_deterministic() {
        // a.rs and z.rs have identical epistemic scores (each alone at the
        // top of one column in its own snapshot, never graded). z.rs has the
        // higher current risk score, so it must rank first despite sorting
        // after a.rs alphabetically — and the ordering is stable across runs.
        let snapshots = vec![snap(100, &["src/a.rs"], &[]), snap(101, &[], &["src/z.rs"])];
        let risk_scores = vec![("src/a.rs".to_string(), 1.0), ("src/z.rs".to_string(), 9.0)];
        let first = compute_epistemic_ranking(&snapshots, &[], &risk_scores);
        assert!(
            (first[0].score - first[1].score).abs() < SCORE_EPSILON,
            "fixture must produce an epistemic tie, got {} vs {}",
            first[0].score,
            first[1].score
        );
        assert_eq!(first[0].path, "src/z.rs", "higher risk score wins the tie");
        assert_eq!(first[1].path, "src/a.rs");
        for _ in 0..5 {
            let again = compute_epistemic_ranking(&snapshots, &[], &risk_scores);
            let paths: Vec<&str> = again.iter().map(|e| e.path.as_str()).collect();
            let expected: Vec<&str> = first.iter().map(|e| e.path.as_str()).collect();
            assert_eq!(paths, expected, "ordering must be deterministic");
        }
    }

    #[test]
    fn test_tie_break_abstention_unscored_sorts_last() {
        // Abstention case (Day 144): a tied file the risk model has NO score
        // for is not silently absorbed — it explicitly sorts after scored
        // files within the tie, then by path among fellow unscored files.
        let snapshots = vec![
            snap(100, &["src/a.rs"], &[]),
            snap(101, &[], &["src/m.rs"]),
            snap(102, &["src/z.rs"], &[]),
        ];
        // Only z.rs has a current risk score; a.rs and m.rs abstain.
        let risk_scores = vec![("src/z.rs".to_string(), 0.5)];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &risk_scores);
        let paths: Vec<&str> = ranking.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["src/z.rs", "src/a.rs", "src/m.rs"],
            "scored file first within the tie, then unscored by path"
        );
    }

    #[test]
    fn test_report_notes_tie_break() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[]), snap(101, &["src/b.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
        assert!(
            (ranking[0].score - ranking[1].score).abs() < SCORE_EPSILON,
            "fixture must tie"
        );
        let report = format_epistemic_report(&snapshots, &ranking, &[]);
        assert!(
            report.contains("ordered by current risk score"),
            "report must note the tie-break honestly, got: {report}"
        );
    }

    #[test]
    fn test_report_no_tie_note_when_scores_distinct() {
        let snapshots = vec![snap(100, &["src/a.rs", "src/b.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
        let report = format_epistemic_report(&snapshots, &ranking, &[]);
        assert!(
            !report.contains("ordered by current risk score"),
            "no tie → no tie note, got: {report}"
        );
    }

    // ---- never-forecast section (Day 149) ----

    fn scores(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs.iter().map(|(p, s)| (p.to_string(), *s)).collect()
    }

    #[test]
    fn test_predicted_file_is_not_never_forecast() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/a.rs", 5.0)]));
        assert!(
            never.is_empty(),
            "a file in the reactive column has been forecast: {:?}",
            never.iter().map(|n| &n.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_emerging_only_file_is_not_never_forecast() {
        let snapshots = vec![snap(100, &[], &["src/a.rs"])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/a.rs", 5.0)]));
        assert!(
            never.is_empty(),
            "a file in the anticipatory column has been forecast"
        );
    }

    #[test]
    fn test_scored_file_in_no_snapshot_is_never_forecast() {
        // src/update.rs is the Day 149 journal's real example: churns, scores,
        // and appears in zero of the saved predictions.
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/b.rs"])];
        let never = never_forecast_files(
            &snapshots,
            &scores(&[("src/a.rs", 5.0), ("src/update.rs", 3.2)]),
        );
        assert_eq!(never.len(), 1, "exactly the unforecast file");
        assert_eq!(never[0].path, "src/update.rs");
        assert!((never[0].risk_score - 3.2).abs() < 1e-9);
    }

    #[test]
    fn test_never_forecast_ordering_risk_desc_then_path() {
        let snapshots = vec![snap(100, &["src/seen.rs"], &[])];
        let never = never_forecast_files(
            &snapshots,
            &scores(&[
                ("src/low.rs", 1.0),
                ("src/zeta.rs", 4.0),
                ("src/alpha.rs", 4.0),
                ("src/high.rs", 9.0),
            ]),
        );
        let paths: Vec<&str> = never.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["src/high.rs", "src/alpha.rs", "src/zeta.rs", "src/low.rs"],
            "higher risk first, ties broken by path ascending"
        );
    }

    #[test]
    fn test_never_forecast_empty_snapshots_makes_no_claim() {
        // No snapshots = nothing to be blind *relative to*. Claiming every
        // scored file is unforecast here would be a false "everything is
        // blind" verdict, not an honest observation.
        let never = never_forecast_files(&[], &scores(&[("src/a.rs", 5.0)]));
        assert!(never.is_empty(), "empty snapshots → no claim");
    }

    #[test]
    fn test_report_never_forecast_total_and_sample_cap() {
        let snapshots = vec![snap(100, &["src/seen.rs"], &[])];
        let mut pairs: Vec<(String, f64)> = Vec::new();
        for i in 0..12 {
            pairs.push((format!("src/f{i:02}.rs"), 12.0 - i as f64));
        }
        let never = never_forecast_files(&snapshots, &pairs);
        assert_eq!(never.len(), 12);
        let ranking = compute_epistemic_ranking(&snapshots, &[], &pairs);
        let report = format_epistemic_report(&snapshots, &ranking, &never);
        assert!(
            report.contains("12 scored files have never appeared in any prediction"),
            "honest total must be printed, got: {report}"
        );
        // Count rows by their glyph, not by a colored prefix: ANSI codes are
        // emitted unless NO_COLOR is set, so a literal "  ◦ src/f" match would
        // silently find zero rows in a colored run.
        let sample_rows = report
            .lines()
            .filter(|line| {
                let plain = line
                    .replace(RESET.0, "")
                    .replace(DIM.0, "")
                    .replace(YELLOW.0, "");
                plain.trim_start().starts_with('◦') && plain.contains("src/f")
            })
            .count();
        assert_eq!(sample_rows, NEVER_FORECAST_SAMPLE, "sample is capped at 5");
        assert!(
            report.contains("(+7 more)"),
            "remainder must be stated, got: {report}"
        );
        assert!(
            report.contains("have no risk score and are invisible to both views"),
            "the honest limitation caveat is required, got: {report}"
        );
    }

    #[test]
    fn test_report_renders_never_forecast_even_with_no_ranked_entries() {
        // The ranked list can be empty (everything predicted was graded)
        // while the blindest files are exactly the ones never predicted —
        // the section must survive that early return.
        let snapshots = vec![snap(100, &["src/a.rs"], &["src/a.rs"])];
        let events = vec![graded(100, &["src/a.rs"])];
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[]);
        assert!(ranking.is_empty(), "fixture must produce an empty ranking");
        let never = never_forecast_files(&snapshots, &scores(&[("src/update.rs", 2.0)]));
        let report = format_epistemic_report(&snapshots, &ranking, &never);
        assert!(
            report.contains("no ungraded predictions"),
            "existing empty-ranking message stays, got: {report}"
        );
        assert!(
            report.contains("src/update.rs"),
            "never-forecast section must still render, got: {report}"
        );
    }

    #[test]
    fn test_report_never_forecast_rows_are_not_numbered_entries() {
        // The trajectory parser matches ranked entries as `N. path score`.
        // If the never-forecast rows took that shape they would be silently
        // absorbed as ranked entries and push real ones out of the top-N
        // budget (Day 141). Pin the distinct bullet shape.
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/update.rs", 2.0)]));
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
        let report = format_epistemic_report(&snapshots, &ranking, &never);
        for line in report.lines() {
            let plain: String = line.replace(RESET.0, "").replace(DIM.0, "");
            if plain.contains("src/update.rs") {
                assert!(
                    plain.trim_start().starts_with('◦'),
                    "never-forecast rows use the ◦ glyph, not `N. path score`: {line:?}"
                );
            }
        }
    }

    #[test]
    fn test_report_never_forecast_section_absent_when_empty() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[]);
        let report = format_epistemic_report(&snapshots, &ranking, &[]);
        assert!(
            !report.contains("never forecast"),
            "no unforecast files → no section, got: {report}"
        );
    }
}
