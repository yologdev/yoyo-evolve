//! The never-forecast section of `/risk epistemic` — the files the epistemic
//! ranking structurally *cannot* see.
//!
//! Extracted verbatim from `commands_risk_epistemic.rs` on Day 166 to make
//! room under the module-size gate; no behaviour change, and every call site
//! is unchanged via a re-export from that module.
//!
//! The ranking next door is built by iterating snapshot columns, so a file
//! that was never guessed about gets no entry at all. This module names that
//! blind spot explicitly rather than letting absence be absorbed by silence.

use crate::commands_risk_epistemic::{latest_study_state_by_path, ExperimentVisit, StudyState};
use crate::commands_risk_snapshots::ParsedSnapshot;

/// How many never-forecast files are listed as a sample. The honest *total*
/// is always printed alongside — the cap is a display budget, not a claim.
pub(crate) const NEVER_FORECAST_SAMPLE: usize = 5;

/// Display cap for a study annotation, in chars. Long grade summaries would
/// otherwise wrap the one-line-per-file shape this section depends on. The
/// cut is marked in-band by `truncate_reason`'s ellipsis (Day 162).
const STUDY_NOTE_MAX_CHARS: usize = 100;

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

/// A file no column ever forecast **but** that an experiment round has already
/// opened on purpose — the room I lit myself.
///
/// Kept apart from [`NeverForecast`] rather than silently subtracted from it
/// (Day 144: absence gets its own name). "No column ever guessed about it" and
/// "I once studied it" are two different facts; collapsing them destroys the
/// first, and dropping these files outright would hide the second. So they are
/// surfaced separately, annotated with when the visit happened and whether it
/// graded anything — never presented as unexplored (#744: round 43 was handed
/// `src/commands_lint.rs` as a dark room ~4.5h after round 40 studied it).
pub(crate) struct StudiedNeverForecast {
    pub(crate) path: String,
    pub(crate) risk_score: f64,
    /// Human-readable study annotation. Uses the ranked half's vocabulary —
    /// `studied by graded experiment (day N, …)` for a graded round,
    /// `visited by ungraded experiment (day N)` for one that scored nothing.
    /// A visit must never wear the word "graded".
    pub(crate) note: String,
}

/// The never-forecast set, split into its two honest halves.
#[derive(Default)]
pub(crate) struct NeverForecastGroups {
    /// Never forecast **and** never studied — the actually-dark set. This is
    /// what the report leads with and what the exploration budget is for.
    pub(crate) dark: Vec<NeverForecast>,
    /// Never forecast but studied/visited on some day — lit rooms that no
    /// prediction column has ever named.
    pub(crate) studied: Vec<StudiedNeverForecast>,
}

/// Every scored path that appears in **no** snapshot's `predicted` column and
/// **no** snapshot's `emerging` column, across all snapshots — the files the
/// epistemic ranking structurally cannot see — split by whether an experiment
/// round has already visited them (`experiments`, the same ledger the ranked
/// half reads).
///
/// Ordering within each group: current risk score descending, then path
/// ascending (fully deterministic). Rationale — a file that *changes* (it has
/// a risk score at all) yet has never once been forecast is the most
/// consequential blind spot, so churny-but-unforecast files come first.
///
/// Returns empty groups when `snapshots` is empty: with no predictions on
/// record there is nothing to be blind *relative to*, and claiming every
/// scored file is unforecast would be a false "everything is blind" verdict.
///
/// Honest limitation (stated in the report too): the universe here is only
/// what the risk model scores. A file with no recent churn has no risk score
/// and is invisible to both views.
pub(crate) fn never_forecast_files(
    snapshots: &[ParsedSnapshot],
    risk_scores: &[(String, f64)],
    experiments: &[ExperimentVisit],
) -> NeverForecastGroups {
    use std::collections::HashSet;

    if snapshots.is_empty() {
        return NeverForecastGroups::default();
    }

    let mut forecast: HashSet<&str> = HashSet::new();
    for snap in snapshots {
        for p in snap.predicted.iter().chain(snap.emerging.iter()) {
            forecast.insert(p.as_str());
        }
    }

    // Same precedence the ranked half uses (graded outranks a bare visit,
    // latest day wins within a state) — shared helper, never a second copy.
    let studied_states = latest_study_state_by_path(experiments);

    let mut dark: Vec<NeverForecast> = Vec::new();
    let mut studied: Vec<StudiedNeverForecast> = Vec::new();
    for (path, score) in risk_scores.iter() {
        if forecast.contains(path.as_str()) {
            continue;
        }
        match studied_states.get(path.as_str()) {
            Some((state, day)) => studied.push(StudiedNeverForecast {
                path: path.clone(),
                risk_score: *score,
                note: study_note(state, *day),
            }),
            None => dark.push(NeverForecast {
                path: path.clone(),
                risk_score: *score,
            }),
        }
    }
    dark.sort_by(|a, b| risk_then_path(a.risk_score, &a.path, b.risk_score, &b.path));
    studied.sort_by(|a, b| risk_then_path(a.risk_score, &a.path, b.risk_score, &b.path));
    NeverForecastGroups { dark, studied }
}

/// Shared ordering: risk score descending, path ascending.
fn risk_then_path(a_score: f64, a_path: &str, b_score: f64, b_path: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    b_score
        .partial_cmp(&a_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a_path.cmp(b_path))
}

/// Render one study annotation. Deliberately the ranked half's vocabulary so
/// the two views can't drift apart, and deliberately *different* per state:
/// a round that graded nothing must never be described as graded.
fn study_note(state: &StudyState, day: u32) -> String {
    let text = match state {
        StudyState::Graded(grade) => format!("studied by graded experiment (day {day}, {grade})"),
        StudyState::VisitedUngraded => format!("visited by ungraded experiment (day {day})"),
    };
    // Char-boundary-safe, and the ellipsis marks the cut in-band.
    crate::commands_risk_epistemic::truncate_reason(&text, STUDY_NOTE_MAX_CHARS).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_risk_epistemic::{
        compute_epistemic_ranking, format_epistemic_report, ExperimentFamilies,
    };
    use crate::commands_risk_snapshots::GradedEvent;
    use crate::format::{DIM, RESET, YELLOW};

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

    // ---- never-forecast section (Day 149) ----

    fn scores(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs.iter().map(|(p, s)| (p.to_string(), *s)).collect()
    }

    #[test]
    fn test_predicted_file_is_not_never_forecast() {
        let snapshots = vec![snap(100, &["src/a.rs"], &[])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/a.rs", 5.0)]), &[]);
        assert!(
            never.dark.is_empty(),
            "a file in the reactive column has been forecast: {:?}",
            never.dark.iter().map(|n| &n.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_emerging_only_file_is_not_never_forecast() {
        let snapshots = vec![snap(100, &[], &["src/a.rs"])];
        let never = never_forecast_files(&snapshots, &scores(&[("src/a.rs", 5.0)]), &[]);
        assert!(
            never.dark.is_empty(),
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
            &[],
        );
        assert_eq!(never.dark.len(), 1, "exactly the unforecast file");
        assert_eq!(never.dark[0].path, "src/update.rs");
        assert!((never.dark[0].risk_score - 3.2).abs() < 1e-9);
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
            &[],
        );
        let paths: Vec<&str> = never.dark.iter().map(|n| n.path.as_str()).collect();
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
        let never = never_forecast_files(&[], &scores(&[("src/a.rs", 5.0)]), &[]);
        assert!(never.dark.is_empty(), "empty snapshots → no claim");
    }

    #[test]
    fn test_report_never_forecast_total_and_sample_cap() {
        let snapshots = vec![snap(100, &["src/seen.rs"], &[])];
        let mut pairs: Vec<(String, f64)> = Vec::new();
        for i in 0..12 {
            pairs.push((format!("src/f{i:02}.rs"), 12.0 - i as f64));
        }
        let never = never_forecast_files(&snapshots, &pairs, &[]);
        assert_eq!(never.dark.len(), 12);
        let ranking = compute_epistemic_ranking(&snapshots, &[], &pairs, &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
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
        let ranking = compute_epistemic_ranking(&snapshots, &events, &[], &[]);
        assert!(ranking.is_empty(), "fixture must produce an empty ranking");
        let never = never_forecast_files(&snapshots, &scores(&[("src/update.rs", 2.0)]), &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
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
        let never = never_forecast_files(&snapshots, &scores(&[("src/update.rs", 2.0)]), &[]);
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &never, &ExperimentFamilies::default());
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
        let ranking = compute_epistemic_ranking(&snapshots, &[], &[], &[]);
        let report =
            format_epistemic_report(&snapshots, &ranking, &NeverForecastGroups::default(), &ExperimentFamilies::default());
        assert!(
            !report.contains("never forecast"),
            "no unforecast files → no section, got: {report}"
        );
    }
}
