//! Report/context formatting for the `/risk` subsystem — the presentation
//! layer split out of `commands_risk.rs` (which keeps scoring/learning).
//! All names are re-exported from `commands_risk` so call sites
//! (watch.rs, smart_edit.rs, commands_info.rs) remain unchanged.

use crate::commands_risk::{
    compute_accuracy_stats, compute_file_risk_scores, AccuracyTrend, FileRisk,
};
use crate::commands_risk_snapshots::{
    load_validation_history_from, parse_all_snapshots, parse_validation_events, snapshot_before,
    ParsedSnapshot, ValidationEvent, RISK_SNAPSHOT_PATH, RISK_VALIDATION_PATH,
};
use crate::format::*;

/// Given a list of file paths (e.g. from error output), return those with
/// above-median risk scores (> 0.5 normalized) along with their score and
/// active signal labels.
///
/// Used by `build_watch_fix_prompt` to inject risk-aware guidance into
/// fix prompts — the "action-guidance" property of the body schema.
pub(crate) fn risk_context_for_files(paths: &[String]) -> Vec<(String, f64, Vec<&'static str>)> {
    if paths.is_empty() {
        return Vec::new();
    }
    let risks = compute_file_risk_scores();
    risk_context_for_files_from(paths, &risks)
}

/// Inner helper that operates on pre-computed risk scores (testable without git).
pub(crate) fn risk_context_for_files_from(
    paths: &[String],
    risks: &[FileRisk],
) -> Vec<(String, f64, Vec<&'static str>)> {
    let mut result = Vec::new();
    for risk in risks {
        if risk.score > 0.5 && paths.iter().any(|p| p == &risk.path) {
            result.push((risk.path.clone(), risk.score, risk.signals.clone()));
        }
    }
    // Sort descending by score for consistent output, with filename tiebreaker
    result.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    result
}

/// Format risk context entries into a human-readable prompt section.
pub(crate) fn format_risk_context(entries: &[(String, f64, Vec<&'static str>)]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut section =
        String::from("\n\n⚠ Risk context — these error files have elevated historical risk:\n");
    for (path, score, signals) in entries {
        let signal_desc = signal_labels_to_description(signals);
        section.push_str(&format!("• {path} (risk: {score:.2}) — {signal_desc}\n"));
    }
    section.push_str(
        "Be especially careful with changes to these files. Consider smaller, incremental fixes.",
    );
    section
}

/// Check whether a single file has elevated risk (top 25th percentile).
///
/// Returns `Some((score, signals_description))` if the file is in the top quartile,
/// `None` otherwise. The description uses human-readable signal names.
/// This is the proactive counterpart to `risk_context_for_files` — intended for
/// single-file lookups after a successful edit (body-schema action-guidance).
pub(crate) fn file_risk_summary(path: &str) -> Option<(f64, Vec<&'static str>)> {
    file_risk_summary_from(path, &compute_file_risk_scores())
}

/// Inner implementation with pre-computed scores (testable without git).
pub(crate) fn file_risk_summary_from(
    path: &str,
    risks: &[FileRisk],
) -> Option<(f64, Vec<&'static str>)> {
    if risks.is_empty() {
        return None;
    }
    // Find the 75th percentile threshold (risks are sorted descending by score)
    let p75_index = risks.len() / 4; // top 25% = first quarter of sorted-desc list
    let threshold = risks.get(p75_index).map(|r| r.score).unwrap_or(0.0);

    // Look up the file
    risks.iter().find(|r| r.path == path).and_then(|r| {
        if r.score >= threshold {
            Some((r.score, r.signals.clone()))
        } else {
            None
        }
    })
}

/// Convert signal labels like `["▲churn", "▲size"]` to a readable description
/// like `"high churn, large file"`.
fn signal_labels_to_description(signals: &[&str]) -> String {
    let parts: Vec<&str> = signals
        .iter()
        .filter_map(|s| match *s {
            "▲churn" => Some("high churn"),
            "▲recent" => Some("recent changes"),
            "▲size" => Some("large file"),
            "▲reverts" => Some("revert history"),
            "▲low-test" => Some("low test density"),
            "▲coupled" => Some("frequent co-changes with fragile files"),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        "elevated risk score".to_string()
    } else {
        parts.join(", ")
    }
}

/// Format risk scores into a human-readable report.
pub(crate) fn format_risk_report(risks: &[FileRisk], show_all: bool) -> String {
    if risks.is_empty() {
        return "  No risk data — not enough git history or source files found.\n".to_string();
    }

    let mut out = String::new();
    out.push_str(&format!("\n  📊 {BOLD}File Risk Scores (src/){RESET}\n\n"));
    out.push_str(&format!(
        "  {DIM}Risk   T/100  File{:width$}Signals{RESET}\n",
        "",
        width = 26
    ));
    out.push_str(&format!("  {DIM}{}{RESET}\n", "─".repeat(78)));

    let limit = if show_all { risks.len() } else { 15 };
    for risk in risks.iter().take(limit) {
        let signals_str = risk.signals.join(" ");
        let path_display = &risk.path;
        // Pad path to 34 chars for alignment
        let padded_path = if path_display.len() < 34 {
            format!("{path_display:<34}")
        } else {
            path_display.to_string()
        };
        let td_display = if risk.test_density > 0.0 {
            format!("{:5.1}", risk.test_density)
        } else {
            "    -".to_string()
        };
        out.push_str(&format!(
            "  {YELLOW}{:.2}{RESET}   {td_display}  {padded_path}{CYAN}{signals_str}{RESET}\n",
            risk.score
        ));
    }

    if !show_all && risks.len() > 15 {
        out.push_str(&format!(
            "\n  {DIM}Top 15 files shown. Use /risk --all for complete list.{RESET}\n"
        ));
    }
    out.push('\n');
    out
}

/// Return a compact prediction accuracy summary for ambient display (e.g. `/status`).
///
/// Returns `Some((hit_rate_pct, validation_count, trend_label))` when there are
/// ≥2 validation entries in `.yoyo/risk_validations.jsonl`, or `None` if there
/// isn't enough data yet. This keeps `/status` clean when no data exists.
pub(crate) fn prediction_accuracy_summary() -> Option<(f64, usize, &'static str)> {
    prediction_accuracy_summary_from(std::path::Path::new(RISK_VALIDATION_PATH))
}

/// Inner implementation with configurable path (for testing).
fn prediction_accuracy_summary_from(path: &std::path::Path) -> Option<(f64, usize, &'static str)> {
    let events = load_validation_history_from(path);
    if events.len() < 2 {
        return None;
    }
    let stats = compute_accuracy_stats(&events);
    let trend_label = match stats.trend {
        AccuracyTrend::Improving => "↑ improving",
        AccuracyTrend::Declining => "↓ declining",
        AccuracyTrend::Stable => "→ stable",
        AccuracyTrend::Insufficient => "? insufficient",
    };
    let hit_rate = (stats.overall_hit_rate_pct * 10.0).round() / 10.0;
    Some((hit_rate, stats.total_validations, trend_label))
}

/// Honest one-line note for `/status` when the prediction meter is
/// precision-only (zero failure-day / recall-graded events). Reads the live
/// validation history; returns `None` when the meter has recall data or when
/// there are no events at all. See
/// `commands_risk_accuracy::recall_coverage_note` for the polarity rationale.
pub(crate) fn recall_coverage_note() -> Option<String> {
    recall_coverage_note_from(std::path::Path::new(RISK_VALIDATION_PATH))
}

/// Inner implementation with configurable path (for testing).
fn recall_coverage_note_from(path: &std::path::Path) -> Option<String> {
    let events = load_validation_history_from(path);
    let stats = compute_accuracy_stats(&events);
    crate::commands_risk_accuracy::recall_coverage_note(&stats)
}

/// Honest one-line track record for the anticipatory (emerging/momentum)
/// column, rendered directly under the `⚡ Emerging Risks` header so the
/// forecast can't be read as carrying the same standing as the reactive list
/// beside it (#720 — a capability is real only where something *consumes* it;
/// this column's only evidence was its own advertisement).
///
/// This is DISCLOSURE, not repair: nothing here touches the momentum formula
/// or the grading window. Three explicit states, absence never collapsed into
/// a neighbour (Day 144):
///   1. no validation events at all → `None` (nothing measured, say nothing —
///      never print `0%` for an unmeasured thing)
///   2. events exist but the column was never graded on a failure day →
///      "not yet graded on a failure day" (ungraded ≠ wrong)
///   3. graded on ≥1 failure day → the measured number, next to the reactive
///      column's number over the same days
///
/// Day 163 (#720 step 2) adds the *denominator's* honest upper bound beside the
/// numerator: see `emerging_achievable_ceiling`.
pub(crate) fn emerging_track_record_note() -> Option<String> {
    emerging_track_record_note_from(
        std::path::Path::new(RISK_VALIDATION_PATH),
        std::path::Path::new(RISK_SNAPSHOT_PATH),
    )
}

/// Best recall the emerging column *could* have scored on the graded
/// failure-day events, given how many files it actually named at grading time.
///
/// Why this exists (#720 step 2, Day 163 lesson "a zero I can blame on the
/// instrument is a zero I never have to accept"): the column reads 0% and every
/// repair since Day 138 went to the grading apparatus. If the recorded emerging
/// lists were structurally too small to cover the outcomes, the zero would be
/// the instrument's fault; if they weren't, the zero is the *forecast's*. This
/// function computes which, so the reader never has to take my word for it.
///
/// Pooled — `sum(min(list_len, outcome_len)) / sum(outcome_len)` — not an
/// average of per-event percentages, so it is commensurable with
/// `failure_hit_rate_pct` (Day 149: averaging per-event percentages silently
/// reweights tiny events upward).
///
/// Absence gets its own name (Day 144): an event with no timestamp, no
/// preceding snapshot, or an empty outcome set is *excluded* and counted, never
/// folded in as a 0. Zero surviving events → `None`, never `Some(0.0)`.
pub(crate) struct EmergingCeiling {
    /// Pooled best-possible recall percentage over the paired events.
    pub(crate) pct: f64,
    /// How many graded failure-day events were successfully paired and scored.
    pub(crate) events: usize,
    /// How many were dropped as un-pairable (see the struct docs).
    pub(crate) excluded: usize,
}

/// A validation event paired with the `ts` its JSONL line recorded.
///
/// `ValidationEvent` itself carries no timestamp, and the ceiling computation
/// needs one to find the snapshot the event graded against — so the timestamp
/// rides alongside rather than being re-parsed into a second event type.
struct TimedValidationEvent {
    /// `None` on lines with no `ts` field (legacy) — un-pairable, so excluded.
    ts: Option<String>,
    event: ValidationEvent,
}

/// Re-read validation JSONL keeping each line's `ts` next to its parsed event.
///
/// Deliberately reuses `parse_validation_events` one line at a time rather than
/// re-implementing the event parser here: the two must never disagree about
/// which lines are valid, and a duplicated parser is exactly the shape that
/// drifts (the only thing added is the timestamp the shared struct omits).
fn parse_timed_validation_events(content: &str) -> Vec<TimedValidationEvent> {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(event) = parse_validation_events(trimmed).into_iter().next() else {
            continue;
        };
        let ts = serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .and_then(|v| v.get("ts").and_then(|t| t.as_str()).map(String::from));
        out.push(TimedValidationEvent { ts, event });
    }
    out
}

/// Compute the pooled achievable ceiling (see `EmergingCeiling`).
fn emerging_achievable_ceiling(
    events: &[TimedValidationEvent],
    snapshots: &[ParsedSnapshot],
) -> Option<EmergingCeiling> {
    let mut best_hits = 0usize;
    let mut total_changed = 0usize;
    let mut scored = 0usize;
    let mut excluded = 0usize;

    for timed in events {
        // Same failure-day filter the track record uses. Do not re-derive the
        // green predicate — call the one the accuracy module owns.
        if crate::commands_risk_accuracy::is_green_event(&timed.event) {
            continue;
        }
        if timed.event.emerging_accuracy_pct.is_none() {
            continue;
        }
        let outcome = timed.event.total_changed;
        // Strict `<` is deliberate: at least one recorded event carries a `ts`
        // byte-identical to a snapshot's, and `yoyo risk validate` grades
        // against the *prior* snapshot before writing the new one — `<=` would
        // grade an outcome against a snapshot written after it.
        let list_len = timed
            .ts
            .as_deref()
            .and_then(|ts| snapshot_before(snapshots, ts))
            .map(|s| s.emerging.len());
        match (list_len, outcome) {
            (Some(len), out) if out > 0 => {
                best_hits += len.min(out);
                total_changed += out;
                scored += 1;
            }
            // No timestamp, no preceding snapshot, or nothing changed: this
            // event teaches the ceiling nothing. Named, not silently zeroed.
            _ => excluded += 1,
        }
    }

    if scored == 0 || total_changed == 0 {
        return None;
    }
    Some(EmergingCeiling {
        pct: (best_hits as f64 / total_changed as f64) * 100.0,
        events: scored,
        excluded,
    })
}

/// Render the ceiling as a clause appended to the measured track record.
///
/// Branches on direction: if the recorded lists left headroom above the
/// measured recall, the miss is the forecast's; if they didn't, say that
/// instead. The sentence must be true in both directions (the point of the
/// exercise is disconfirmation, not a better-sounding zero).
fn ceiling_clause(ceiling: &EmergingCeiling, actual_pct: f64) -> String {
    let pct = ceiling.pct;
    let verdict = if pct > actual_pct {
        "the list named enough files to have hit — the miss is the forecast, not the list size"
    } else {
        "the recorded list sizes leave no headroom above the measured recall — list size, not the forecast, bounds this column"
    };
    let excluded_clause = if ceiling.excluded > 0 {
        let plural = if ceiling.excluded == 1 { "" } else { "s" };
        format!(
            " ({n} event{plural} excluded — no snapshot recorded before them; ceiling over {m})",
            n = ceiling.excluded,
            m = ceiling.events
        )
    } else {
        String::new()
    };
    format!(
        " — achievable ceiling on those events: {pct:.0}%, {verdict}{excluded_clause} (pairing approximate: events do not record which snapshot they graded)"
    )
}

/// Inner implementation with configurable paths (for testing).
fn emerging_track_record_note_from(
    path: &std::path::Path,
    snapshot_path: &std::path::Path,
) -> Option<String> {
    let events = load_validation_history_from(path);
    let stats = compute_accuracy_stats(&events);

    // State 1: nothing measured at all.
    if stats.total_validations == 0 {
        return None;
    }

    let ungraded = stats.emerging_failure_ungraded;
    let ungraded_clause = if ungraded > 0 {
        let plural = if ungraded == 1 { "" } else { "s" };
        format!(" — {ungraded} failure-day event{plural} carried no emerging forecast")
    } else {
        String::new()
    };

    // State 2: events exist, but this column has never been graded against a
    // failure day. Ungraded is not the same as wrong — say so plainly.
    if stats.emerging_failure_samples == 0 {
        return Some(format!(
            "track record: not yet graded on a failure day{ungraded_clause}"
        ));
    }

    // State 3: measured. State the number and the sample size; put the
    // reactive column beside it so the reader can compare on the same days.
    // When the reactive side has no failure-day recall, omit the clause
    // rather than printing a `0%` nobody measured.
    let pct = stats.emerging_failure_avg_pct.unwrap_or(0.0);
    let samples = stats.emerging_failure_samples;
    let plural = if samples == 1 { "" } else { "s" };
    let reactive_clause = match stats.failure_hit_rate_pct {
        Some(r) => format!(" (reactive column: {r:.0}%)"),
        None => String::new(),
    };
    // The achievable ceiling: how well this column *could* have done given the
    // list sizes it actually recorded. `None` (un-pairable / no snapshots) adds
    // nothing — the three original states stay byte-identical without it.
    let snapshots = std::fs::read_to_string(snapshot_path)
        .map(|c| parse_all_snapshots(&c))
        .unwrap_or_default();
    let timed = std::fs::read_to_string(path)
        .map(|c| parse_timed_validation_events(&c))
        .unwrap_or_default();
    let ceiling_clause = emerging_achievable_ceiling(&timed, &snapshots)
        .map(|c| ceiling_clause(&c, pct))
        .unwrap_or_default();
    Some(format!(
        "track record: {pct:.0}% recall over {samples} graded failure day{plural}{reactive_clause}{ungraded_clause}{ceiling_clause}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failure-day event (no `watch_success` severity) with an emerging grade.
    fn failure_line(day: u32, hits: &str, surprises: &str, emerging: Option<f64>) -> String {
        let emerging_field = match emerging {
            Some(e) => format!(r#","emerging_accuracy_pct":{e}"#),
            None => String::new(),
        };
        format!(
            r#"{{"ts":"2026-01-0{d}T12:00:00Z","day":{day},"trigger":"watch_failure","severity":"watch_failure","hits":[{hits}],"surprises":[{surprises}],"predicted_count":10,"accuracy_pct":50.0{emerging_field}}}"#,
            d = (day % 9) + 1
        )
    }

    /// A snapshot path that doesn't exist. With no snapshots the ceiling is
    /// un-computable (`None`), so the note falls back to step 1's exact
    /// wording — which is what the three original state tests pin.
    fn no_snapshots(dir: &std::path::Path) -> std::path::PathBuf {
        dir.join("no_snapshots.jsonl")
    }

    #[test]
    fn test_emerging_track_record_note_missing_ledger_is_silent() {
        // State 1: nothing measured → say nothing. Never print "0%" for a
        // thing that was never graded.
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nonexistent.jsonl");
        assert!(emerging_track_record_note_from(&path, &no_snapshots(dir.path())).is_none());

        let empty = dir.path().join("empty.jsonl");
        std::fs::write(&empty, "").expect("write");
        assert!(emerging_track_record_note_from(&empty, &no_snapshots(dir.path())).is_none());
    }

    #[test]
    fn test_emerging_track_record_note_ungraded_is_not_wrong() {
        // State 2: failure-day events exist but none carried an emerging
        // forecast → say "not yet graded", never a percentage.
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        let lines = [
            failure_line(90, r#""src/a.rs""#, r#""src/b.rs""#, None),
            failure_line(91, r#""src/a.rs""#, r#""src/c.rs""#, None),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").expect("write");

        let note = emerging_track_record_note_from(&path, &no_snapshots(dir.path()))
            .expect("note expected");
        assert!(
            note.contains("not yet graded on a failure day"),
            "expected ungraded wording, got: {note}"
        );
        assert!(
            !note.contains('%'),
            "ungraded state must not print a percentage: {note}"
        );
        assert!(
            note.contains("2 failure-day events carried no emerging forecast"),
            "expected the ungraded count, got: {note}"
        );
    }

    #[test]
    fn test_emerging_track_record_note_reports_measured_number() {
        // State 3: graded on failure days → the percentage AND the sample
        // count both appear, with the reactive column beside it.
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        let lines = [
            failure_line(90, r#""src/a.rs""#, r#""src/b.rs""#, Some(0.0)),
            failure_line(91, r#""src/a.rs""#, r#""src/c.rs""#, Some(0.0)),
            // One failure-day event with no emerging forecast at all.
            failure_line(92, r#""src/a.rs""#, r#""src/d.rs""#, None),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").expect("write");

        let note = emerging_track_record_note_from(&path, &no_snapshots(dir.path()))
            .expect("note expected");
        assert!(
            note.contains("0% recall"),
            "expected the measured percentage, got: {note}"
        );
        assert!(
            note.contains("2 graded failure days"),
            "expected the sample count, got: {note}"
        );
        assert!(
            note.contains("reactive column:"),
            "expected the reactive comparison, got: {note}"
        );
        assert!(
            note.contains("1 failure-day event carried no emerging forecast"),
            "expected the ungraded tail (singular), got: {note}"
        );
        // No snapshots on disk → the ceiling is un-computable, and the note is
        // byte-identical to step 1's output (no stray clause).
        assert!(
            !note.contains("achievable ceiling"),
            "unmeasurable ceiling must add nothing: {note}"
        );
    }

    // ---- #720 step 2: achievable ceiling ------------------------------------

    /// One validation JSONL line with an explicit timestamp and outcome set.
    fn event_line(
        ts: &str,
        severity: Option<&str>,
        paths: &[&str],
        emerging: Option<f64>,
    ) -> String {
        let sev = match severity {
            Some(s) => format!(r#","severity":"{s}""#),
            None => String::new(),
        };
        let em = match emerging {
            Some(e) => format!(r#","emerging_accuracy_pct":{e}"#),
            None => String::new(),
        };
        let surprises: Vec<String> = paths.iter().map(|p| format!("\"{p}\"")).collect();
        format!(
            r#"{{"ts":"{ts}","day":163,"trigger":"cli","hits":[],"surprises":[{s}],"predicted_count":10,"accuracy_pct":0.0{em}{sev}}}"#,
            s = surprises.join(",")
        )
    }

    /// One snapshot JSONL line whose emerging list has `n` entries. Shaped
    /// like the real ledger: both columns are arrays of objects with a `path`
    /// key, and `top_10` must be non-empty or `parse_all_snapshots` drops the
    /// line entirely.
    fn snapshot_line(ts: &str, n: usize) -> String {
        let emerging: Vec<String> = (0..n).map(|i| format!(r#"{{"path":"src/e{i}.rs"}}"#)).collect();
        format!(
            r#"{{"day":163,"git_hash":"abc{n}","ts":"{ts}","top_10":[{{"path":"src/hot.rs"}}],"emerging":[{e}]}}"#,
            e = emerging.join(",")
        )
    }

    fn ceiling_of(snapshot_jsonl: &str, validation_jsonl: &str) -> Option<EmergingCeiling> {
        let snapshots = parse_all_snapshots(snapshot_jsonl);
        let events = parse_timed_validation_events(validation_jsonl);
        emerging_achievable_ceiling(&events, &snapshots)
    }

    #[test]
    fn test_ceiling_is_pooled_not_averaged() {
        // Snapshot A names 2 emerging files, snapshot B names 5.
        // Event 1 (after A): outcome 4 files → best possible 2.
        // Event 2 (after B): outcome 2 files → best possible 2.
        // Pooled = (2 + 2) / (4 + 2) = 66.7%, NOT the mean of 50% and 100%.
        let snaps = [
            snapshot_line("2026-08-01T00:00:00Z", 2),
            snapshot_line("2026-08-03T00:00:00Z", 5),
        ]
        .join("\n");
        let events = [
            event_line(
                "2026-08-02T00:00:00Z",
                Some("watch_failure"),
                &["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"],
                Some(0.0),
            ),
            event_line(
                "2026-08-04T00:00:00Z",
                Some("watch_failure"),
                &["src/a.rs", "src/b.rs"],
                Some(0.0),
            ),
        ]
        .join("\n");

        let c = ceiling_of(&snaps, &events).expect("ceiling expected");
        assert_eq!(c.events, 2);
        assert_eq!(c.excluded, 0);
        assert!(
            (c.pct - 66.666).abs() < 0.1,
            "pooled ceiling should be 4/6 = 66.7%, got {}",
            c.pct
        );
    }

    #[test]
    fn test_event_with_no_preceding_snapshot_is_excluded_not_zeroed() {
        // The only snapshot is written AFTER the event → un-pairable. It must
        // be counted as excluded, never folded in as a 0 (Day 144: absence
        // gets its own name).
        let snaps = snapshot_line("2026-08-05T00:00:00Z", 5);
        let events = [
            event_line(
                "2026-08-01T00:00:00Z",
                Some("watch_failure"),
                &["src/a.rs", "src/b.rs"],
                Some(0.0),
            ),
            event_line(
                "2026-08-06T00:00:00Z",
                Some("watch_failure"),
                &["src/a.rs", "src/b.rs"],
                Some(0.0),
            ),
        ]
        .join("\n");

        let c = ceiling_of(&snaps, &events).expect("ceiling expected");
        assert_eq!(c.events, 1, "only the later event is pairable");
        assert_eq!(c.excluded, 1, "the earlier event must be named, not zeroed");
        assert!(
            (c.pct - 100.0).abs() < 0.1,
            "the excluded event must not drag the pooled number down: {}",
            c.pct
        );
    }

    #[test]
    fn test_equal_timestamp_pairs_with_the_earlier_snapshot() {
        // Pins the strict `<`: an event whose ts equals a snapshot's grades
        // against the snapshot BEFORE it, because `yoyo risk validate` grades
        // the prior snapshot before writing the new one.
        let snaps = [
            snapshot_line("2026-08-01T00:00:00Z", 1),
            snapshot_line("2026-08-02T00:00:00Z", 9),
        ]
        .join("\n");
        let events = event_line(
            "2026-08-02T00:00:00Z",
            Some("watch_failure"),
            &["src/a.rs", "src/b.rs", "src/c.rs"],
            Some(0.0),
        );

        let c = ceiling_of(&snaps, &events).expect("ceiling expected");
        assert_eq!(c.events, 1);
        assert!(
            (c.pct - (1.0 / 3.0 * 100.0)).abs() < 0.1,
            "must use the earlier (1-file) list, got {}",
            c.pct
        );
    }

    #[test]
    fn test_green_and_ungraded_events_do_not_contribute() {
        let snaps = snapshot_line("2026-08-01T00:00:00Z", 5);
        let events = [
            // green day → other polarity, excluded from a recall ceiling
            event_line(
                "2026-08-02T00:00:00Z",
                Some("watch_success"),
                &["src/a.rs", "src/b.rs"],
                Some(0.0),
            ),
            // failure day but no emerging forecast → ungraded, not scored
            event_line(
                "2026-08-03T00:00:00Z",
                Some("watch_failure"),
                &["src/a.rs"],
                None,
            ),
        ]
        .join("\n");

        assert!(
            ceiling_of(&snaps, &events).is_none(),
            "no graded failure-day events → None, never Some(0.0)"
        );
    }

    #[test]
    fn test_ceiling_clause_renders_next_to_the_measured_number() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let vpath = dir.path().join("validations.jsonl");
        let spath = dir.path().join("snapshots.jsonl");
        std::fs::write(&spath, snapshot_line("2026-08-01T00:00:00Z", 5) + "\n").expect("write");
        std::fs::write(
            &vpath,
            event_line(
                "2026-08-02T00:00:00Z",
                Some("watch_failure"),
                &["src/a.rs", "src/b.rs"],
                Some(0.0),
            ) + "\n",
        )
        .expect("write");

        let note = emerging_track_record_note_from(&vpath, &spath).expect("note expected");
        assert!(
            note.contains("0% recall"),
            "measured number must survive: {note}"
        );
        assert!(
            note.contains("achievable ceiling on those events: 100%"),
            "ceiling must be stated: {note}"
        );
        assert!(
            note.contains("the miss is the forecast, not the list size"),
            "headroom verdict expected when ceiling > actual: {note}"
        );
        assert!(
            note.contains("pairing approximate"),
            "the approximation must be disclosed: {note}"
        );
    }

    #[test]
    fn test_ceiling_clause_does_not_claim_headroom_when_there_is_none() {
        // Ceiling == actual: the sentence must NOT say the column could have
        // hit. (Constructed: a 1-file emerging list, a 1-file outcome, and a
        // recorded 100% emerging accuracy.)
        let c = EmergingCeiling {
            pct: 20.0,
            events: 3,
            excluded: 0,
        };
        let clause = ceiling_clause(&c, 20.0);
        assert!(
            !clause.contains("could"),
            "must not claim headroom that isn't there: {clause}"
        );
        assert!(
            clause.contains("no headroom"),
            "expected the no-headroom wording: {clause}"
        );
    }

    #[test]
    fn test_ceiling_clause_reports_exclusions() {
        let c = EmergingCeiling {
            pct: 40.0,
            events: 4,
            excluded: 2,
        };
        let clause = ceiling_clause(&c, 0.0);
        assert!(
            clause.contains("2 events excluded"),
            "exclusions must be disclosed: {clause}"
        );
        assert!(
            clause.contains("ceiling over 4"),
            "the paired count must be stated: {clause}"
        );
    }

    #[test]
    fn test_format_risk_report_empty() {
        let result = format_risk_report(&[], false);
        assert!(result.contains("No risk data"));
    }

    #[test]
    fn test_format_risk_report_shows_signals() {
        let risks = vec![FileRisk {
            path: "src/foo.rs".to_string(),
            score: 0.75,
            signals: vec!["▲churn", "▲size"],
            test_density: 1.5,
        }];
        let result = format_risk_report(&risks, false);
        assert!(result.contains("0.75"));
        assert!(result.contains("src/foo.rs"));
        assert!(result.contains("▲churn"));
    }

    #[test]
    fn test_prediction_accuracy_summary_missing_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nonexistent.jsonl");
        assert!(prediction_accuracy_summary_from(&path).is_none());
    }

    #[test]
    fn test_prediction_accuracy_summary_too_few_entries() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        // Only 1 entry — should return None (need ≥2)
        let line = r#"{"ts":"2025-01-15T12:00:00Z","day":100,"trigger":"watch_failure","hits":["src/main.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}"#;
        std::fs::write(&path, format!("{line}\n")).expect("write");
        assert!(prediction_accuracy_summary_from(&path).is_none());
    }

    #[test]
    fn test_prediction_accuracy_summary_returns_correct_values() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        let line1 = r#"{"ts":"2025-01-15T12:00:00Z","day":100,"trigger":"watch_failure","hits":["src/main.rs"],"surprises":["src/cli.rs"],"predicted_count":10,"accuracy_pct":50.0}"#;
        let line2 = r#"{"ts":"2025-01-16T12:00:00Z","day":101,"trigger":"watch_failure","hits":["src/tools.rs","src/main.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}"#;
        std::fs::write(&path, format!("{line1}\n{line2}\n")).expect("write");

        let result = prediction_accuracy_summary_from(&path);
        assert!(result.is_some());
        let (hit_rate, count, _trend) = result.unwrap();
        assert_eq!(count, 2);
        // 3 hits out of 4 total changed = 75%
        assert!((hit_rate - 75.0).abs() < 0.2);
    }

    #[test]
    fn test_prediction_accuracy_summary_trend_improving() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        // First entries low accuracy, later entries high — should show improving
        let lines = [
            r#"{"ts":"2025-01-10T12:00:00Z","day":90,"trigger":"watch_failure","hits":[],"surprises":["src/a.rs","src/b.rs","src/c.rs","src/d.rs","src/e.rs"],"predicted_count":10,"accuracy_pct":0.0}"#,
            r#"{"ts":"2025-01-11T12:00:00Z","day":91,"trigger":"watch_failure","hits":[],"surprises":["src/a.rs","src/b.rs","src/c.rs"],"predicted_count":10,"accuracy_pct":0.0}"#,
            r#"{"ts":"2025-01-15T12:00:00Z","day":100,"trigger":"watch_failure","hits":["src/a.rs","src/b.rs","src/c.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}"#,
            r#"{"ts":"2025-01-16T12:00:00Z","day":101,"trigger":"watch_failure","hits":["src/a.rs","src/b.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}"#,
        ];
        std::fs::write(&path, lines.join("\n") + "\n").expect("write");

        let result = prediction_accuracy_summary_from(&path);
        assert!(result.is_some());
        let (_hit_rate, count, trend) = result.unwrap();
        assert_eq!(count, 4);
        assert!(
            trend.contains("improving"),
            "expected improving, got: {trend}"
        );
    }

    #[test]
    fn test_prediction_accuracy_summary_trend_declining() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        // First entries high accuracy, later entries low — should show declining
        let lines = [
            r#"{"ts":"2025-01-10T12:00:00Z","day":90,"trigger":"watch_failure","hits":["src/a.rs","src/b.rs","src/c.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}"#,
            r#"{"ts":"2025-01-11T12:00:00Z","day":91,"trigger":"watch_failure","hits":["src/a.rs","src/b.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}"#,
            r#"{"ts":"2025-01-15T12:00:00Z","day":100,"trigger":"watch_failure","hits":[],"surprises":["src/a.rs","src/b.rs","src/c.rs"],"predicted_count":10,"accuracy_pct":0.0}"#,
            r#"{"ts":"2025-01-16T12:00:00Z","day":101,"trigger":"watch_failure","hits":[],"surprises":["src/a.rs","src/b.rs","src/c.rs","src/d.rs"],"predicted_count":10,"accuracy_pct":0.0}"#,
        ];
        std::fs::write(&path, lines.join("\n") + "\n").expect("write");

        let result = prediction_accuracy_summary_from(&path);
        assert!(result.is_some());
        let (_hit_rate, _count, trend) = result.unwrap();
        assert!(
            trend.contains("declining"),
            "expected declining, got: {trend}"
        );
    }

    #[test]
    fn test_prediction_accuracy_summary_trend_stable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        // Similar accuracy throughout — should show stable
        let lines = [
            r#"{"ts":"2025-01-10T12:00:00Z","day":90,"trigger":"watch_failure","hits":["src/a.rs"],"surprises":["src/b.rs"],"predicted_count":10,"accuracy_pct":50.0}"#,
            r#"{"ts":"2025-01-11T12:00:00Z","day":91,"trigger":"watch_failure","hits":["src/a.rs"],"surprises":["src/b.rs"],"predicted_count":10,"accuracy_pct":50.0}"#,
        ];
        std::fs::write(&path, lines.join("\n") + "\n").expect("write");

        let result = prediction_accuracy_summary_from(&path);
        assert!(result.is_some());
        let (_hit_rate, _count, trend) = result.unwrap();
        assert!(trend.contains("stable"), "expected stable, got: {trend}");
    }

    #[test]
    fn risk_context_for_files_empty_paths() {
        let risks = vec![FileRisk {
            path: "src/foo.rs".to_string(),
            score: 0.8,
            signals: vec!["▲churn"],
            test_density: 1.0,
        }];
        let result = risk_context_for_files_from(&[], &risks);
        assert!(result.is_empty(), "empty paths should return empty result");
    }

    #[test]
    fn risk_context_for_files_no_high_risk() {
        let risks = vec![
            FileRisk {
                path: "src/foo.rs".to_string(),
                score: 0.3,
                signals: vec![],
                test_density: 5.0,
            },
            FileRisk {
                path: "src/bar.rs".to_string(),
                score: 0.1,
                signals: vec![],
                test_density: 8.0,
            },
        ];
        let paths = vec!["src/foo.rs".to_string(), "src/bar.rs".to_string()];
        let result = risk_context_for_files_from(&paths, &risks);
        assert!(
            result.is_empty(),
            "no files above 0.5 threshold should return empty"
        );
    }

    #[test]
    fn risk_context_for_files_with_high_risk() {
        let risks = vec![
            FileRisk {
                path: "src/fragile.rs".to_string(),
                score: 0.82,
                signals: vec!["▲churn", "▲low-test"],
                test_density: 0.5,
            },
            FileRisk {
                path: "src/stable.rs".to_string(),
                score: 0.2,
                signals: vec![],
                test_density: 10.0,
            },
            FileRisk {
                path: "src/coupled.rs".to_string(),
                score: 0.65,
                signals: vec!["▲coupled"],
                test_density: 3.0,
            },
        ];
        let paths = vec![
            "src/fragile.rs".to_string(),
            "src/stable.rs".to_string(),
            "src/coupled.rs".to_string(),
        ];
        let result = risk_context_for_files_from(&paths, &risks);
        assert_eq!(result.len(), 2, "should return 2 high-risk files");
        // Should be sorted descending by score
        assert_eq!(result[0].0, "src/fragile.rs");
        assert!((result[0].1 - 0.82).abs() < 0.001);
        assert_eq!(result[0].2, vec!["▲churn", "▲low-test"]);
        assert_eq!(result[1].0, "src/coupled.rs");
        assert!((result[1].1 - 0.65).abs() < 0.001);
    }

    #[test]
    fn risk_context_for_files_unmatched_paths_ignored() {
        let risks = vec![FileRisk {
            path: "src/fragile.rs".to_string(),
            score: 0.9,
            signals: vec!["▲churn"],
            test_density: 0.5,
        }];
        // Query for a path not in the risk data
        let paths = vec!["src/other.rs".to_string()];
        let result = risk_context_for_files_from(&paths, &risks);
        assert!(
            result.is_empty(),
            "paths not in risk data should not appear"
        );
    }

    #[test]
    fn format_risk_context_empty() {
        let result = format_risk_context(&[]);
        assert!(
            result.is_empty(),
            "empty entries should produce empty string"
        );
    }

    #[test]
    fn format_risk_context_with_entries() {
        let entries = vec![
            ("src/foo.rs".to_string(), 0.82, vec!["▲churn", "▲low-test"]),
            ("src/bar.rs".to_string(), 0.65, vec!["▲coupled"]),
        ];
        let result = format_risk_context(&entries);
        assert!(result.contains("⚠ Risk context"));
        assert!(result.contains("src/foo.rs (risk: 0.82)"));
        assert!(result.contains("high churn, low test density"));
        assert!(result.contains("src/bar.rs (risk: 0.65)"));
        assert!(result.contains("frequent co-changes with fragile files"));
        assert!(result.contains("Be especially careful"));
    }

    #[test]
    fn file_risk_summary_from_returns_none_for_missing_file() {
        let risks = vec![
            FileRisk {
                path: "src/a.rs".to_string(),
                score: 0.9,
                signals: vec!["▲churn"],
                test_density: 0.5,
            },
            FileRisk {
                path: "src/b.rs".to_string(),
                score: 0.5,
                signals: vec![],
                test_density: 1.0,
            },
        ];
        assert!(file_risk_summary_from("src/nonexistent.rs", &risks).is_none());
    }

    #[test]
    fn file_risk_summary_from_returns_none_for_empty_risks() {
        assert!(file_risk_summary_from("src/a.rs", &[]).is_none());
    }

    #[test]
    fn file_risk_summary_from_returns_some_for_top_quartile() {
        // 4 files: top quartile threshold is at index 1 (4/4=1), so score >= 0.70
        let risks = vec![
            FileRisk {
                path: "src/high.rs".to_string(),
                score: 0.90,
                signals: vec!["▲churn", "▲size"],
                test_density: 0.2,
            },
            FileRisk {
                path: "src/medium_high.rs".to_string(),
                score: 0.70,
                signals: vec!["▲recent"],
                test_density: 0.5,
            },
            FileRisk {
                path: "src/medium.rs".to_string(),
                score: 0.50,
                signals: vec![],
                test_density: 1.0,
            },
            FileRisk {
                path: "src/low.rs".to_string(),
                score: 0.20,
                signals: vec![],
                test_density: 2.0,
            },
        ];

        // High-risk file should return Some
        let result = file_risk_summary_from("src/high.rs", &risks);
        assert!(result.is_some());
        let (score, signals) = result.unwrap();
        assert!((score - 0.90).abs() < 0.001);
        assert_eq!(signals, vec!["▲churn", "▲size"]);

        // At-threshold file should also return Some
        let result = file_risk_summary_from("src/medium_high.rs", &risks);
        assert!(result.is_some());

        // Below-threshold file should return None
        assert!(file_risk_summary_from("src/medium.rs", &risks).is_none());
        assert!(file_risk_summary_from("src/low.rs", &risks).is_none());
    }

    // ── Emerging-risk detection tests ──────────────────────────────────
}
