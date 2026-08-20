//! JSONL ledger *readers* for the `/risk` subsystem.
//!
//! Pure extraction (Day 166) of the reader half of
//! `commands_risk_snapshots.rs`, which was sitting two lines under the
//! `MAX_MODULE_LINES = 2000` gate — a fatal gate, and a fatal test means the
//! whole task is reverted. Everything here was moved verbatim, together with
//! the tests that pin it; no behaviour was added or changed.
//!
//! The split is **readers vs writers/graders**: this module only ever turns
//! `&str` (or a file path) into parsed events/snapshots. Anything that
//! *writes* a ledger line or *grades* a prediction stayed behind in
//! `commands_risk_snapshots`, which re-exports everything here so every
//! existing call site compiles unchanged.

/// A single parsed validation event from `.yoyo/risk_validations.jsonl`.
pub(crate) struct ValidationEvent {
    pub(crate) day: u32,
    pub(crate) hit_count: usize,
    pub(crate) total_changed: usize,
    pub(crate) accuracy_pct: f64,
    /// Anticipatory (emerging / momentum) prediction accuracy for this event.
    /// `None` when the underlying snapshot carried no emerging list (older
    /// snapshots) — distinct from `Some(0.0)` which means "graded, 0% accurate".
    pub(crate) emerging_accuracy_pct: Option<f64>,
    /// What kind of outcome this event graded against — `"watch_failure"`
    /// (red watch cycle) or `"revert"` (full revert). `None` on historical
    /// severity-less lines and CLI manual grading (parse is defensive, so
    /// legacy JSONL stays valid).
    pub(crate) severity: Option<String>,
}

/// Load validation history from a JSONL file.
pub(crate) fn load_validation_history_from(path: &std::path::Path) -> Vec<ValidationEvent> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_validation_events(&content)
}

/// The three distinguishable states of the validation ledger on disk.
///
/// Before Day 169 all three rendered identically: a missing file, an existing
/// file whose every line is unparseable, and a healthy file with some lines
/// silently dropped all produced the same `/risk accuracy` output (#764). The
/// middle case is the damaging one — it shrinks the denominator of the DREAM
/// milestone's own meter without saying so — and the "missing" copy that
/// followed it asserted a *cause* that was false.
///
/// **Deliberately NOT covered here** (still open on #764): a line that is
/// *valid JSON but missing fields* is still absorbed by the `unwrap_or(0)` /
/// `unwrap_or_default()` defaults below and counts as a healthy event. That is
/// a different defect. The other readers in this module (`parse_graded_events`,
/// `parse_all_snapshots`, `parse_failed_ci_runs`) and their ~10 call sites are
/// untouched by this repair and still drop malformed lines silently.
pub(crate) enum ValidationLedger {
    /// The path does not exist. Accuracy tracking genuinely hasn't started.
    Missing,
    /// The path exists but could not be read; the string names path + io error.
    Unreadable(String),
    /// The file was read. `dropped` counts non-blank lines that failed to
    /// parse as JSON — `Present { events: [], dropped: n }` (everything
    /// corrupt) is a *different fact* from `Missing`, never collapsed into it.
    Present {
        events: Vec<ValidationEvent>,
        dropped: usize,
    },
}

/// Read the validation ledger, keeping missing / unreadable / present-with-
/// dropped-lines distinct. See [`ValidationLedger`] for what is and isn't
/// covered.
pub(crate) fn read_validation_ledger(path: &std::path::Path) -> ValidationLedger {
    if !path.exists() {
        return ValidationLedger::Missing;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ValidationLedger::Unreadable(format!("could not read {}: {e}", path.display()))
        }
    };
    let (events, dropped) = parse_validation_events_counting(&content);
    ValidationLedger::Present { events, dropped }
}

/// Parse validation events from JSONL content (testable without filesystem).
pub(crate) fn parse_validation_events(content: &str) -> Vec<ValidationEvent> {
    parse_validation_events_counting(content).0
}

/// Same parse as [`parse_validation_events`], additionally returning how many
/// **non-blank** lines failed `serde_json::from_str`. Blank lines are not
/// corruption and are never counted (a trailing newline is normal JSONL).
pub(crate) fn parse_validation_events_counting(content: &str) -> (Vec<ValidationEvent>, usize) {
    let mut events = Vec::new();
    let mut dropped = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                dropped += 1;
                continue;
            }
        };
        let day = val["day"].as_u64().unwrap_or(0) as u32;
        let hits = val["hits"].as_array().map(|a| a.len()).unwrap_or(0);
        let surprises = val["surprises"].as_array().map(|a| a.len()).unwrap_or(0);
        let total_changed = hits + surprises;
        let accuracy_pct = val["accuracy_pct"].as_f64().unwrap_or(0.0);
        // Optional anticipatory accuracy — absent on all historical lines and
        // on CLI-triggered events, so parse defensively (absent → None).
        let emerging_accuracy_pct = val.get("emerging_accuracy_pct").and_then(|v| v.as_f64());
        // Optional severity tag — absent on legacy lines (parse defensively,
        // absent → None; same pattern as emerging_accuracy_pct above).
        let severity = val
            .get("severity")
            .and_then(|v| v.as_str())
            .map(String::from);

        events.push(ValidationEvent {
            day,
            hit_count: hits,
            total_changed,
            accuracy_pct,
            emerging_accuracy_pct,
            severity,
        });
    }
    (events, dropped)
}

/// A graded validation event reduced to what the epistemic view needs: the
/// day it was recorded and every file path that appeared in its graded
/// outcome. Hits and surprises both count — each taught the model something
/// (a hit confirms a prediction, a surprise exposes a miss). Lives here
/// because this module owns the validation JSONL format; extending
/// [`ValidationEvent`] instead would break its many struct-literal test
/// fixtures for fields most readers don't need.
pub(crate) struct GradedEvent {
    pub(crate) day: u64,
    pub(crate) paths: Vec<String>,
}

/// Parse graded events (day + graded file paths) from validation JSONL
/// content, returning the surviving events plus how many
/// **non-blank** lines failed to parse as JSON. Blank lines and a trailing
/// newline are normal JSONL, not corruption, so neither is counted.
///
/// Deliberately *not* counted (#764, still open on the same issue): a line that
/// is valid JSON but missing `day` / `hits` / `surprises` is absorbed by the
/// `unwrap_or` defaults below and reported as a healthy event.
pub(crate) fn parse_graded_events_counting(content: &str) -> (Vec<GradedEvent>, usize) {
    let mut events = Vec::new();
    let mut dropped = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                dropped += 1;
                continue; // malformed line — counted, never silently absorbed
            }
        };
        let day = val["day"].as_u64().unwrap_or(0);
        let mut paths: Vec<String> = Vec::new();
        for key in ["hits", "surprises"] {
            if let Some(arr) = val[key].as_array() {
                paths.extend(arr.iter().filter_map(|v| v.as_str().map(String::from)));
            }
        }
        events.push(GradedEvent { day, paths });
    }
    (events, dropped)
}

/// A parsed risk snapshot from the JSONL file.
pub(crate) struct ParsedSnapshot {
    pub(crate) day: u64,
    pub(crate) git_hash: String,
    /// ISO-8601 UTC timestamp (`%Y-%m-%dT%H:%M:%SZ`) this snapshot was taken.
    /// `"unknown"` on the handful of legacy lines written before the field
    /// existed — callers that order by time must skip those explicitly.
    pub(crate) ts: String,
    pub(crate) predicted: Vec<String>,
    /// File paths flagged as *emerging* (anticipatory / momentum) risks in this
    /// snapshot. Empty for older snapshots written before emerging was recorded.
    ///
    /// Read by `auto_validate_after_failure_to`, which grades this anticipatory
    /// set against what actually broke (alongside the reactive `predicted` set),
    /// so the allostatic-vs-homeostatic comparison is measured, not decorative.
    pub(crate) emerging: Vec<String>,
}

/// Parse all snapshots from JSONL content.
pub(crate) fn parse_all_snapshots(content: &str) -> Vec<ParsedSnapshot> {
    parse_all_snapshots_counting(content).0
}

/// Same parse as [`parse_all_snapshots`], additionally returning how many
/// **non-blank** lines failed `serde_json::from_str`. The sibling of
/// [`parse_validation_events_counting`], for the other half of the prediction
/// meter: a silently shrinking snapshot ledger makes "this file was never
/// forecast" a claim about the reader, not about the model.
///
/// Scope, stated rather than implied (#764 is still open on the rest): the
/// count is **JSON-parse failures only**. A line that is valid JSON but whose
/// `top_10` is empty or missing is still skipped by the `!predicted.is_empty()`
/// guard below and is **not** counted here — that is a different defect on the
/// same issue, and calling it corruption would overstate what was measured.
pub(crate) fn parse_all_snapshots_counting(content: &str) -> (Vec<ParsedSnapshot>, usize) {
    let mut snapshots = Vec::new();
    let mut dropped = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                dropped += 1;
                continue; // malformed line — counted, never silently absorbed
            }
        };
        let day = val["day"].as_u64().unwrap_or(0);
        let git_hash = val["git_hash"].as_str().unwrap_or("unknown").to_string();
        let ts = val["ts"].as_str().unwrap_or("unknown").to_string();
        let predicted: Vec<String> = val["top_10"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v["path"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let emerging: Vec<String> = val["emerging"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v["path"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if !predicted.is_empty() {
            snapshots.push(ParsedSnapshot {
                day,
                git_hash,
                ts,
                predicted,
                emerging,
            });
        }
    }
    (snapshots, dropped)
}

/// The three distinguishable states of the snapshot ledger on disk — the
/// prediction half of the meter, mirroring [`ValidationLedger`] (the grade
/// half). Same reason for existing: `read_to_string(..).unwrap_or_default()`
/// reported a missing file, an unreadable file and a wholly corrupt file as
/// the same empty string, so a confident per-signal breakdown could be
/// printed over an unknown fraction of the recorded predictions.
///
/// **Not covered** (same limits as the sibling, #764 stays open on both): a
/// line that is valid JSON but missing fields is still absorbed by the
/// parser's `unwrap_or` defaults, and a line whose `top_10` array is empty is
/// dropped from `snapshots` *without* being counted — it parsed fine, it just
/// carried no prediction, so calling it corruption would overstate what was
/// measured.
pub(crate) enum SnapshotLedger {
    /// The path does not exist. No snapshot has ever been recorded.
    Missing,
    /// The path exists but could not be read; the string names path + io error.
    Unreadable(String),
    /// The file was read. `dropped` counts non-blank lines that failed to
    /// parse as JSON — `Present { snapshots: [], dropped: n }` (everything
    /// corrupt) is a *different fact* from `Missing`, never collapsed into it.
    Present {
        snapshots: Vec<ParsedSnapshot>,
        dropped: usize,
    },
}

/// Read the snapshot ledger, keeping missing / unreadable / present-with-
/// dropped-lines distinct. See [`SnapshotLedger`] for what is and isn't
/// covered.
pub(crate) fn read_snapshot_ledger(path: &std::path::Path) -> SnapshotLedger {
    if !path.exists() {
        return SnapshotLedger::Missing;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return SnapshotLedger::Unreadable(format!("could not read {}: {e}", path.display()))
        }
    };
    let (snapshots, dropped) = parse_all_snapshots_counting(&content);
    SnapshotLedger::Present { snapshots, dropped }
}

/// The three distinguishable states of the validation ledger read *as graded
/// events* — the projection `/risk epistemic` consumes.
///
/// Sibling of [`ValidationLedger`] (same file, same JSONL) but a different
/// projection: that one carries accuracy numbers, this one carries the graded
/// file paths. Both exist because collapsing "missing" into "corrupt" into
/// "clean but partly unparseable" is what #764 is about.
pub(crate) enum GradedLedger {
    /// The path does not exist. Nothing has ever been graded.
    Missing,
    /// The path exists but could not be read; the string names path + io error.
    Unreadable(String),
    /// The file was read. `dropped` counts non-blank lines that failed to
    /// parse as JSON — `Present { events: [], dropped: n }` (everything
    /// corrupt) is a *different fact* from `Missing`, never collapsed into it.
    Present {
        events: Vec<GradedEvent>,
        dropped: usize,
    },
}

/// Read the validation ledger as graded events, keeping missing / unreadable /
/// present-with-dropped-lines distinct. See [`GradedLedger`].
pub(crate) fn read_graded_ledger(path: &std::path::Path) -> GradedLedger {
    if !path.exists() {
        return GradedLedger::Missing;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return GradedLedger::Unreadable(format!("could not read {}: {e}", path.display()))
        }
    };
    let (events, dropped) = parse_graded_events_counting(&content);
    GradedLedger::Present { events, dropped }
}

/// The honest lines `/risk epistemic` must print about the *state of the two
/// ledger files it reads*, before the report computed from them.
///
/// Pure so the wording is asserted at the emission point; the I/O stays at the
/// one call site in `commands_risk_epistemic::handle_risk_epistemic`.
///
/// Why this view needs it at all: both of its headline claims are statements
/// about **absence** — "no column ever forecast this file" and "no graded event
/// ever touched it". Both are computed by *subtracting* the ledgers from the
/// scored file set, so a line that silently fails to parse does not shrink a
/// denominator here, it **manufactures a blind spot**: a corrupt snapshot
/// ledger makes every scored file read as never-forecast, and that list is what
/// `scripts/extract_trajectory.py` hands the planner as "study these next".
/// Before this, both files were read with a bare `unwrap_or_default()`, so an
/// unreadable or corrupt ledger was indistinguishable from an empty one.
///
/// `None`-equivalent (no line) on the two states where the report below is
/// already honest without one: a genuinely missing file and a clean read —
/// matching the convention of `ledger_health_line` / `snapshot_health_line`
/// in `commands_risk.rs`.
pub(crate) fn epistemic_ledger_notes(
    snapshot_path: &str,
    snapshots: &SnapshotLedger,
    graded_path: &str,
    graded: &GradedLedger,
) -> Vec<String> {
    let mut notes = Vec::new();
    match snapshots {
        SnapshotLedger::Missing => {}
        SnapshotLedger::Unreadable(msg) => notes.push(msg.clone()),
        SnapshotLedger::Present { dropped: 0, .. } => {}
        SnapshotLedger::Present { snapshots, dropped } if snapshots.is_empty() => {
            notes.push(format!(
                "{snapshot_path} exists but all {dropped} line(s) in it are unparseable — \
                 the prediction ledger is corrupt, not absent, so every file below is \
                 reported as never forecast whether or not it really is."
            ));
        }
        SnapshotLedger::Present { dropped, .. } => notes.push(format!(
            "{snapshot_path}: {dropped} unparseable line(s) skipped — files forecast only \
             on those lines will be reported as never forecast below."
        )),
    }
    match graded {
        GradedLedger::Missing => {}
        GradedLedger::Unreadable(msg) => notes.push(msg.clone()),
        GradedLedger::Present { dropped: 0, .. } => {}
        GradedLedger::Present { events, dropped } if events.is_empty() => notes.push(format!(
            "{graded_path} exists but all {dropped} line(s) in it are unparseable — \
             the grade ledger is corrupt, not absent, so the staleness and \
             never-graded signals below are computed over no grades at all."
        )),
        GradedLedger::Present { dropped, .. } => notes.push(format!(
            "{graded_path}: {dropped} unparseable line(s) skipped — the staleness and \
             never-graded signals below cover only the rest of the ledger."
        )),
    }
    notes
}

/// One failed CI run as reported by `gh run list --json ...`. This is the raw
/// failure-day evidence the risk meter has been throwing away: CI already
/// decided the outcome was red, so no commit-message heuristic is needed.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub(crate) struct FailedCiRun {
    pub(crate) run_id: u64,
    pub(crate) head_sha: String,
    pub(crate) created_at: String,
    pub(crate) title: String,
}

/// Parse the output of
/// `gh run list --workflow=CI --status=failure --limit N --json databaseId,headSha,createdAt,displayTitle`.
///
/// Defensive like every reader in this module: non-array input, non-object
/// entries, and entries missing any *required* field (`databaseId` as a real
/// number, `headSha`, `createdAt`) are skipped rather than guessed at. Only
/// `displayTitle` is optional — it's cosmetic. Never panics.
///
/// Signature preserved for every existing caller and test; the states it
/// flattens away live on [`CiRunPayload`] via [`parse_ci_run_payload`].
pub(crate) fn parse_failed_ci_runs(json: &str) -> Vec<FailedCiRun> {
    match parse_ci_run_payload(json) {
        CiRunPayload::Present { runs, .. } => runs,
        CiRunPayload::Unparseable | CiRunPayload::NotAnArray => Vec::new(),
    }
}

/// The three distinguishable states of a `gh run list --json …` payload, in
/// the same shape as [`ValidationLedger`] / [`SnapshotLedger`] / [`GradedLedger`]
/// above — and for the same reason.
///
/// `parse_failed_ci_runs` returns a `Vec`, so *every* failure mode arrived at
/// the one consumer (`/risk harvest`) as an empty or shortened list: a payload
/// `gh` could not produce at all, a payload of the wrong shape, and a healthy
/// payload whose individual entries were missing required fields all read as
/// "fewer failed CI runs than there are". That is a shrinking denominator
/// inside the meter that grades my own predictions — the defect I keep fixing
/// elsewhere in this module — and the harvest banner printed `runs.len()` under
/// the label "Failed CI runs seen", which was a count of runs *kept*.
///
/// What this does **not** do: it does not recover a dropped entry, and it makes
/// no claim about entries that parsed. It only refuses to report a shortfall as
/// a clean zero.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CiRunPayload {
    /// The payload is not JSON at all (including an empty body).
    Unparseable,
    /// The payload is valid JSON but not the array `gh` documents.
    NotAnArray,
    /// The payload was an array. `dropped` counts entries that could not be
    /// turned into a [`FailedCiRun`] — non-objects, and objects missing
    /// `databaseId`/`headSha`/`createdAt`. `Present { runs: [], dropped: n }`
    /// is a *different fact* from an empty array, never collapsed into it.
    Present {
        runs: Vec<FailedCiRun>,
        dropped: usize,
    },
}

/// Parse a `gh run list --json …` payload, keeping the three states distinct.
/// Pure over a `&str`; never panics. See [`CiRunPayload`].
pub(crate) fn parse_ci_run_payload(json: &str) -> CiRunPayload {
    let val: serde_json::Value = match serde_json::from_str(json.trim()) {
        Ok(v) => v,
        Err(_) => return CiRunPayload::Unparseable,
    };
    let Some(arr) = val.as_array() else {
        return CiRunPayload::NotAnArray;
    };
    let mut runs = Vec::new();
    let mut dropped = 0usize;
    for entry in arr {
        let (Some(run_id), Some(head_sha), Some(created_at)) = (
            entry.get("databaseId").and_then(|v| v.as_u64()),
            entry.get("headSha").and_then(|v| v.as_str()),
            entry.get("createdAt").and_then(|v| v.as_str()),
        ) else {
            dropped += 1;
            continue;
        };
        if head_sha.is_empty() || created_at.is_empty() {
            dropped += 1;
            continue;
        }
        let title = entry
            .get("displayTitle")
            .and_then(|v| v.as_str())
            .unwrap_or("(no title)")
            .to_string();
        runs.push(FailedCiRun {
            run_id,
            head_sha: head_sha.to_string(),
            created_at: created_at.to_string(),
            title,
        });
    }
    CiRunPayload::Present { runs, dropped }
}

/// One honest line about a CI payload that was not wholly healthy, or `None`
/// when it was. Pure, so the string a caller prints is asserted directly.
///
/// The `None` case is the common path and keeps `/risk harvest` byte-identical
/// to before. Every other branch says outright that the run count printed below
/// is a count of runs *kept*, because the banner's own wording ("Failed CI runs
/// seen") asserts more than it knows the moment anything was dropped.
pub(crate) fn ci_payload_note(payload: &CiRunPayload) -> Option<String> {
    match payload {
        CiRunPayload::Unparseable => Some(
            "`gh` returned a payload that is not JSON — this is a broken response, \
             not an absence of failed CI runs; nothing below was harvested from it."
                .to_string(),
        ),
        CiRunPayload::NotAnArray => Some(
            "`gh` returned valid JSON that is not the documented array — this is an \
             unexpected shape, not an absence of failed CI runs; nothing below was \
             harvested from it."
                .to_string(),
        ),
        CiRunPayload::Present { dropped: 0, .. } => None,
        CiRunPayload::Present { runs, dropped } => Some(format!(
            "{dropped} CI run entr(ies) skipped — missing databaseId/headSha/createdAt; \
             the {} below is a count of runs kept, not of runs that failed.",
            runs.len()
        )),
    }
}

/// Return true if a validation event for this CI run id was already recorded,
/// making `yoyo risk harvest` idempotent.
///
/// Deliberately NOT [`green_event_exists_for`]: that one matches on
/// `snapshot_git_hash`, and a snapshot may already carry a green event — reusing
/// it would silently swallow the red evidence this whole path exists to collect.
pub(crate) fn ci_event_exists_for(content: &str, run_id: u64) -> bool {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .any(|v| v.get("ci_run_id").and_then(|r| r.as_u64()) == Some(run_id))
}

/// Pick the most recent snapshot taken strictly *before* `created_at`.
///
/// Both timestamps are `%Y-%m-%dT%H:%M:%SZ` UTC, so lexicographic comparison is
/// chronological comparison. Snapshots with an `"unknown"` ts are skipped —
/// they can't be ordered, and guessing would attribute a run to the wrong
/// prediction.
pub(crate) fn snapshot_before<'a>(
    snapshots: &'a [ParsedSnapshot],
    created_at: &str,
) -> Option<&'a ParsedSnapshot> {
    snapshots
        .iter()
        .filter(|s| s.ts != "unknown" && s.ts.as_str() < created_at)
        .max_by(|a, b| a.ts.cmp(&b.ts))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One well-formed validation line, parameterised by day.
    fn valid_line(day: u32) -> String {
        format!(
            r#"{{"day":{day},"hit_count":2,"total_changed":5,"accuracy_pct":40.0,"severity":"watch_failure"}}"#
        )
    }

    #[test]
    fn counting_reports_zero_dropped_for_a_clean_ledger() {
        let content = format!("{}\n{}\n", valid_line(160), valid_line(161));
        let (events, dropped) = parse_validation_events_counting(&content);
        assert_eq!(events.len(), 2);
        assert_eq!(dropped, 0, "well-formed lines are never counted as dropped");
    }

    #[test]
    fn counting_reports_the_number_of_unparseable_lines() {
        let content = format!(
            "{}\nnot-json\n{}\n{{\"day\":9,\n",
            valid_line(160),
            valid_line(161)
        );
        let (events, dropped) = parse_validation_events_counting(&content);
        assert_eq!(events.len(), 2, "valid lines still parse");
        assert_eq!(dropped, 2, "both the garbage and the truncated line count");
    }

    #[test]
    fn counting_never_counts_blank_lines_or_a_trailing_newline() {
        // A trailing newline is normal JSONL and blank/whitespace-only lines
        // are not corruption — counting them would manufacture a warning on a
        // perfectly healthy ledger.
        let content = format!("\n{}\n\n   \n{}\n", valid_line(160), valid_line(161));
        let (events, dropped) = parse_validation_events_counting(&content);
        assert_eq!(events.len(), 2);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn parse_validation_events_still_returns_just_the_events() {
        // The delegation must leave every existing caller byte-identical.
        let content = format!("{}\nnot-json\n{}\n", valid_line(160), valid_line(161));
        let events = parse_validation_events(&content);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].day, 160);
        assert_eq!(events[1].day, 161);
    }

    #[test]
    fn read_validation_ledger_missing_path_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("never_written.jsonl");
        assert!(matches!(
            read_validation_ledger(&path),
            ValidationLedger::Missing
        ));
    }

    #[test]
    fn read_validation_ledger_reports_clean_present_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("risk_validations.jsonl");
        std::fs::write(&path, format!("{}\n{}\n", valid_line(160), valid_line(161))).unwrap();
        match read_validation_ledger(&path) {
            ValidationLedger::Present { events, dropped } => {
                assert_eq!(events.len(), 2);
                assert_eq!(dropped, 0);
            }
            _ => panic!("expected Present"),
        }
    }

    #[test]
    fn read_validation_ledger_counts_partial_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("risk_validations.jsonl");
        std::fs::write(&path, format!("{}\ntruncated{{\n", valid_line(160))).unwrap();
        match read_validation_ledger(&path) {
            ValidationLedger::Present { events, dropped } => {
                assert_eq!(events.len(), 1, "the surviving line is still reported");
                assert_eq!(dropped, 1, "the dropped line is no longer silent");
            }
            _ => panic!("expected Present"),
        }
    }

    #[test]
    fn read_validation_ledger_all_corrupt_is_present_not_missing() {
        // The sharp case: the file exists and every line is garbage. That is a
        // *different fact* from "tracking hasn't started" and must never be
        // collapsed into Missing.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("risk_validations.jsonl");
        std::fs::write(&path, "not-json\n{oops\n").unwrap();
        match read_validation_ledger(&path) {
            ValidationLedger::Present { events, dropped } => {
                assert!(events.is_empty());
                assert_eq!(dropped, 2);
            }
            _ => panic!("expected Present"),
        }
    }

    #[test]
    fn read_validation_ledger_unreadable_names_the_path() {
        // A directory at the ledger path exists but cannot be read to string.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("risk_validations.jsonl");
        std::fs::create_dir(&path).unwrap();
        match read_validation_ledger(&path) {
            ValidationLedger::Unreadable(msg) => {
                assert!(
                    msg.contains(&path.display().to_string()),
                    "message must name the path, got {msg:?}"
                );
            }
            _ => panic!("expected Unreadable"),
        }
    }

    #[test]
    fn test_parse_all_snapshots_empty() {
        let snapshots = parse_all_snapshots("");
        assert!(snapshots.is_empty());
    }

    #[test]
    fn test_parse_all_snapshots_single() {
        let line = r#"{"ts":"2026-06-10T12:00:00Z","day":110,"git_hash":"abc1234","top_10":[{"path":"src/commands_info.rs","score":0.85,"signals":["churn"]}]}"#;
        let snapshots = parse_all_snapshots(line);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].day, 110);
        assert_eq!(snapshots[0].git_hash, "abc1234");
        assert_eq!(snapshots[0].predicted.len(), 1);
        assert_eq!(snapshots[0].predicted[0], "src/commands_info.rs");
    }

    #[test]
    fn test_parse_all_snapshots_multiple() {
        let data = format!(
            "{}\n{}\n{}\n",
            r#"{"ts":"2026-06-08T12:00:00Z","day":108,"git_hash":"aaa1111","top_10":[{"path":"src/a.rs","score":0.9,"signals":[]}]}"#,
            r#"{"ts":"2026-06-09T12:00:00Z","day":109,"git_hash":"bbb2222","top_10":[{"path":"src/b.rs","score":0.8,"signals":[]}]}"#,
            r#"{"ts":"2026-06-10T12:00:00Z","day":110,"git_hash":"ccc3333","top_10":[{"path":"src/c.rs","score":0.7,"signals":[]}]}"#,
        );
        let snapshots = parse_all_snapshots(&data);
        assert_eq!(snapshots.len(), 3);
        assert_eq!(snapshots[0].day, 108);
        assert_eq!(snapshots[2].day, 110);
    }

    #[test]
    fn test_parse_all_snapshots_skips_invalid_lines() {
        let data = format!(
            "{}\nnot-json\n{}\n",
            r#"{"ts":"2026-06-08T12:00:00Z","day":108,"git_hash":"aaa1111","top_10":[{"path":"src/a.rs","score":0.9,"signals":[]}]}"#,
            r#"{"ts":"2026-06-10T12:00:00Z","day":110,"git_hash":"ccc3333","top_10":[{"path":"src/c.rs","score":0.7,"signals":[]}]}"#,
        );
        let snapshots = parse_all_snapshots(&data);
        assert_eq!(snapshots.len(), 2);
    }

    #[test]
    fn test_parse_snapshot_extracts_emerging() {
        // A snapshot carrying BOTH top_10 and emerging: the emerging paths must
        // be readable back in order (the anticipatory half of the prediction).
        let line = r#"{"ts":"2026-07-15T12:00:00Z","day":137,"git_hash":"b9983c26","top_10":[{"path":"src/reactive.rs","score":0.9,"signals":[]}],"emerging":[{"path":"src/rising.rs","momentum":2.1,"current_rank":12,"signals":[]},{"path":"src/climbing.rs","momentum":1.4,"current_rank":8,"signals":[]}]}"#;
        let snapshots = parse_all_snapshots(line);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].predicted, vec!["src/reactive.rs"]);
        assert_eq!(
            snapshots[0].emerging,
            vec!["src/rising.rs".to_string(), "src/climbing.rs".to_string()],
            "emerging paths preserved in order"
        );
    }

    #[test]
    fn test_parse_old_snapshot_without_emerging_is_empty() {
        // A legacy line (pre-emerging, e.g. a real Day 125 snapshot) has no
        // emerging key at all — predicted is populated, emerging is empty. This
        // is the backward-compat guarantee.
        let line = r#"{"ts":"2026-06-25T12:00:00Z","day":125,"git_hash":"old1234","top_10":[{"path":"src/legacy.rs","score":0.8,"signals":[]}]}"#;
        let snapshots = parse_all_snapshots(line);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].predicted, vec!["src/legacy.rs"]);
        assert!(
            snapshots[0].emerging.is_empty(),
            "missing emerging key → empty vec, no panic"
        );
    }

    #[test]
    fn test_parse_snapshot_emerging_skips_entries_without_path() {
        // A malformed emerging element (missing path) is skipped defensively;
        // the well-formed paths still come through, no panic.
        let line = r#"{"ts":"2026-07-15T12:00:00Z","day":137,"git_hash":"deadbee","top_10":[{"path":"src/reactive.rs","score":0.9,"signals":[]}],"emerging":[{"path":"src/good.rs","momentum":2.0},{"momentum":1.0},{"path":"src/also_good.rs","momentum":1.5}]}"#;
        let snapshots = parse_all_snapshots(line);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0].emerging,
            vec!["src/good.rs".to_string(), "src/also_good.rs".to_string()],
            "only well-formed emerging paths parsed"
        );
    }

    #[test]
    fn test_parse_validation_events_basic() {
        let jsonl = r#"{"ts":"2025-01-15T12:00:00Z","day":100,"trigger":"watch_failure","hits":["src/main.rs","src/cli.rs"],"surprises":["src/prompt.rs"],"predicted_count":10,"accuracy_pct":66.7}
{"ts":"2025-01-16T12:00:00Z","day":101,"trigger":"watch_failure","hits":["src/tools.rs"],"surprises":["src/repl.rs","src/watch.rs"],"predicted_count":10,"accuracy_pct":33.3}"#;

        let events = parse_validation_events(jsonl);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].day, 100);
        assert_eq!(events[0].hit_count, 2);
        assert_eq!(events[0].total_changed, 3);
        assert!((events[0].accuracy_pct - 66.7).abs() < 0.1);
        assert_eq!(events[1].day, 101);
        assert_eq!(events[1].hit_count, 1);
        assert_eq!(events[1].total_changed, 3);
    }

    #[test]
    fn test_parse_validation_events_empty() {
        let events = parse_validation_events("");
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_validation_events_malformed_lines() {
        let jsonl =
            "not json\n{\"day\":5,\"hits\":[],\"surprises\":[],\"accuracy_pct\":0}\ngarbage";
        let events = parse_validation_events(jsonl);
        assert_eq!(events.len(), 1, "should skip malformed lines");
        assert_eq!(events[0].day, 5);
    }

    #[test]
    fn test_load_validation_history_from_missing_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nonexistent.jsonl");
        let events = load_validation_history_from(&path);
        assert!(events.is_empty());
    }

    #[test]
    fn test_load_validation_history_from_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");
        let line = r#"{"ts":"2025-01-15T12:00:00Z","day":100,"trigger":"watch_failure","hits":["src/main.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}"#;
        std::fs::write(&path, format!("{line}\n")).expect("write test file");

        let events = load_validation_history_from(&path);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].day, 100);
        assert_eq!(events[0].hit_count, 1);
        assert_eq!(events[0].total_changed, 1);
    }

    #[test]
    fn test_parse_validation_event_emerging_accuracy_pct_optional() {
        // Backward compat: a line WITHOUT emerging_accuracy_pct parses to None.
        let without = r#"{"day":10,"trigger":"cli","predicted_count":10,"hit_count":1,"total_changed":2,"accuracy_pct":50.0}"#;
        let events = parse_validation_events(without);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].emerging_accuracy_pct, None,
            "absent emerging_accuracy_pct → None (legacy emerging-less lines stay valid)"
        );

        // A line WITH emerging_accuracy_pct parses to Some(value).
        let with = r#"{"day":11,"trigger":"watch_failure","predicted_count":10,"hit_count":2,"total_changed":4,"accuracy_pct":50.0,"emerging_accuracy_pct":75.0}"#;
        let events = parse_validation_events(with);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].emerging_accuracy_pct,
            Some(75.0),
            "present emerging_accuracy_pct → Some(value)"
        );
    }

    /// VERBATIM capture (Day 147's lesson: a hand-typed fixture pins my belief
    /// about the input, not the input). Produced on 2026-07-26 by running:
    ///
    /// ```text
    /// gh run list --workflow=CI --status=failure --limit 5 \
    ///   --json databaseId,headSha,createdAt,displayTitle
    /// ```
    ///
    /// Pasted unedited from stdout.
    const GH_RUN_LIST_FIXTURE: &str = r#"[{"createdAt":"2026-07-23T22:53:49Z","databaseId":30051449447,"displayTitle":"Day 145: bump skill-evolve counter (5)","headSha":"47ea6981af59e69a7207fc18842edc6ccacfffe6"},{"createdAt":"2026-07-11T09:53:19Z","databaseId":29148457259,"displayTitle":"Day 133: bump skill-evolve counter (3)","headSha":"a35503f99c7d88656f90806827fb7398f4aec120"},{"createdAt":"2026-07-09T14:39:14Z","databaseId":29026252684,"displayTitle":"evolve: prompt-discipline guards + skill anti-fabrication (fork review)","headSha":"05b58c5271cb83b02d741e3c0b80240d0658dd56"},{"createdAt":"2026-07-03T16:08:16Z","databaseId":28671767779,"displayTitle":"Day 125: bump skill-evolve counter (6)","headSha":"c9d4ae86fcbbfca903a908735ab4d50b5e427f1d"},{"createdAt":"2026-07-02T23:20:20Z","databaseId":28627737171,"displayTitle":"CI","headSha":"6d9828e4c7ee9776fb6f3efde177237b4d94d93d"}]"#;

    #[test]
    fn test_parse_failed_ci_runs_verbatim_gh_fixture() {
        let runs = parse_failed_ci_runs(GH_RUN_LIST_FIXTURE);
        assert_eq!(runs.len(), 5, "all five real runs parse");
        assert_eq!(runs[0].run_id, 30051449447);
        assert_eq!(runs[0].head_sha, "47ea6981af59e69a7207fc18842edc6ccacfffe6");
        assert_eq!(runs[0].created_at, "2026-07-23T22:53:49Z");
        assert_eq!(runs[0].title, "Day 145: bump skill-evolve counter (5)");
        assert_eq!(runs[4].run_id, 28627737171);
        assert_eq!(runs[4].title, "CI");
    }

    #[test]
    fn test_parse_failed_ci_runs_malformed_shapes_skip_never_panic() {
        // Fixture table of the shapes a flaky `gh` can hand me. Every one must
        // yield a skip (or empty), never a panic.
        let cases: &[(&str, usize, &str)] = &[
            ("", 0, "empty string"),
            ("[]", 0, "empty array"),
            ("{}", 0, "object, not an array"),
            ("not json at all", 0, "garbage"),
            (
                r#"[{"createdAt":"2026-07-23T22:53:49Z","databaseId":1,"displayTitle":"t"}]"#,
                0,
                "missing headSha",
            ),
            (
                r#"[{"createdAt":"2026-07-23T22:53:49Z","databaseId":"1","displayTitle":"t","headSha":"abc"}]"#,
                0,
                "databaseId as a string",
            ),
            (
                r#"[{"databaseId":1,"displayTitle":"t","headSha":"abc"}]"#,
                0,
                "missing createdAt",
            ),
            (
                "  \n [{\"createdAt\":\"2026-07-23T22:53:49Z\",\"databaseId\":7,\"displayTitle\":\"t\",\"headSha\":\"abc\"}]  \n ",
                1,
                "trailing/leading whitespace still parses",
            ),
            (
                r#"[3, "str", null, {"createdAt":"2026-07-23T22:53:49Z","databaseId":9,"displayTitle":"ok","headSha":"def"}]"#,
                1,
                "non-object entries skipped, good one kept",
            ),
            (
                r#"[{"createdAt":"2026-07-23T22:53:49Z","databaseId":9,"headSha":"def"}]"#,
                1,
                "missing displayTitle is tolerated (title is cosmetic)",
            ),
        ];
        for (json, expected, label) in cases {
            let runs = parse_failed_ci_runs(json);
            assert_eq!(runs.len(), *expected, "case: {label}");
        }
    }

    #[test]
    fn test_ci_event_exists_for_present_and_absent() {        let content = concat!(
            r#"{"ts":"2026-07-26T00:00:00Z","day":148,"trigger":"ci_harvest","hits":[],"surprises":["src/a.rs"],"accuracy_pct":0.0,"severity":"ci_failure","ci_run_id":30051449447}"#,
            "\n"
        );
        assert!(ci_event_exists_for(content, 30051449447));
        assert!(!ci_event_exists_for(content, 29148457259));
        assert!(!ci_event_exists_for("", 30051449447));
        assert!(!ci_event_exists_for("not json\n", 30051449447));
    }

    // ---- graded-event ledger (#764 remainder: the `/risk epistemic` reads) ----

    fn graded_line(day: u64, hit: &str, surprise: &str) -> String {
        format!(
            r#"{{"day":{day},"hits":["{hit}"],"surprises":["{surprise}"],"accuracy_pct":50.0}}"#
        )
    }

    #[test]
    fn graded_counting_reports_unparseable_lines_and_ignores_blanks() {
        // A trailing newline and whitespace-only lines are normal JSONL, not
        // corruption — counting them would manufacture a warning on a healthy
        // ledger. Garbage and truncated JSON are corruption and are counted.
        let content = format!(
            "\n{}\n   \nnot-json\n{}\n{{\"day\":9,\n",
            graded_line(160, "src/a.rs", "src/b.rs"),
            graded_line(161, "src/c.rs", "src/d.rs"),
        );
        let (events, dropped) = parse_graded_events_counting(&content);
        assert_eq!(events.len(), 2, "valid lines still parse");
        assert_eq!(events[0].day, 160);
        assert_eq!(events[0].paths, vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(dropped, 2, "both the garbage and the truncated line count");
    }

    #[test]
    fn read_graded_ledger_missing_path_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("never_written.jsonl");
        assert!(matches!(read_graded_ledger(&path), GradedLedger::Missing));
    }

    #[test]
    fn read_graded_ledger_reports_clean_present_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("risk_validations.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                graded_line(160, "src/a.rs", "src/b.rs"),
                graded_line(161, "src/c.rs", "src/d.rs")
            ),
        )
        .unwrap();
        match read_graded_ledger(&path) {
            GradedLedger::Present { events, dropped } => {
                assert_eq!(events.len(), 2);
                assert_eq!(dropped, 0);
            }
            _ => panic!("expected Present"),
        }
    }

    #[test]
    fn read_graded_ledger_all_corrupt_is_present_not_missing() {
        // The sharp case: a file that exists and is entirely unparseable is a
        // *different fact* from a file that was never written, and collapsing
        // the two is exactly what #764 is about.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("risk_validations.jsonl");
        std::fs::write(&path, "garbage\nmore garbage\n").unwrap();
        match read_graded_ledger(&path) {
            GradedLedger::Present { events, dropped } => {
                assert!(events.is_empty());
                assert_eq!(dropped, 2);
            }
            other => panic!(
                "expected Present, got {}",
                match other {
                    GradedLedger::Missing => "Missing",
                    GradedLedger::Unreadable(_) => "Unreadable",
                    GradedLedger::Present { .. } => unreachable!(),
                }
            ),
        }
    }

    // ---- the lines `/risk epistemic` actually prints ----

    fn clean_snapshots() -> SnapshotLedger {
        SnapshotLedger::Present {
            snapshots: Vec::new(),
            dropped: 0,
        }
    }

    fn clean_graded() -> GradedLedger {
        GradedLedger::Present {
            events: Vec::new(),
            dropped: 0,
        }
    }

    #[test]
    fn epistemic_notes_are_silent_when_both_ledgers_are_clean_or_missing() {
        // The common path must stay byte-identical to the pre-change output.
        assert!(epistemic_ledger_notes(
            "snaps.jsonl",
            &clean_snapshots(),
            "vals.jsonl",
            &clean_graded()
        )
        .is_empty());
        assert!(epistemic_ledger_notes(
            "snaps.jsonl",
            &SnapshotLedger::Missing,
            "vals.jsonl",
            &GradedLedger::Missing
        )
        .is_empty());
    }

    #[test]
    fn epistemic_notes_name_a_corrupt_snapshot_ledger_and_its_consequence() {
        let notes = epistemic_ledger_notes(
            "snaps.jsonl",
            &SnapshotLedger::Present {
                snapshots: Vec::new(),
                dropped: 3,
            },
            "vals.jsonl",
            &clean_graded(),
        );
        assert_eq!(notes.len(), 1, "only the snapshot half should speak");
        let note = &notes[0];
        assert!(note.contains("snaps.jsonl"), "names the file: {note}");
        assert!(note.contains('3'), "names the count: {note}");
        // The whole point: an all-corrupt ledger must not read as an absent
        // one, and the reader must be told what it does to the report below.
        assert!(
            note.contains("corrupt, not absent"),
            "distinguishes corrupt from absent: {note}"
        );
        assert!(
            note.contains("never forecast"),
            "names the claim it undermines: {note}"
        );
    }

    #[test]
    fn epistemic_notes_name_a_corrupt_grade_ledger_and_its_consequence() {
        let notes = epistemic_ledger_notes(
            "snaps.jsonl",
            &clean_snapshots(),
            "vals.jsonl",
            &GradedLedger::Present {
                events: Vec::new(),
                dropped: 2,
            },
        );
        assert_eq!(notes.len(), 1, "only the grade half should speak");
        let note = &notes[0];
        assert!(note.contains("vals.jsonl"), "names the file: {note}");
        assert!(note.contains('2'), "names the count: {note}");
        assert!(
            note.contains("corrupt, not absent"),
            "distinguishes corrupt from absent: {note}"
        );
        assert!(
            note.contains("never-graded"),
            "names the signal it undermines: {note}"
        );
    }

    #[test]
    fn epistemic_notes_report_partial_corruption_in_both_ledgers() {
        let notes = epistemic_ledger_notes(
            "snaps.jsonl",
            &SnapshotLedger::Present {
                snapshots: vec![ParsedSnapshot {
                    day: 1,
                    git_hash: "abc".into(),
                    ts: "2026-07-01T00:00:00Z".into(),
                    predicted: vec!["src/a.rs".into()],
                    emerging: Vec::new(),
                }],
                dropped: 1,
            },
            "vals.jsonl",
            &GradedLedger::Present {
                events: vec![GradedEvent {
                    day: 1,
                    paths: vec!["src/a.rs".into()],
                }],
                dropped: 4,
            },
        );
        assert_eq!(notes.len(), 2, "both halves speak independently");
        assert!(notes[0].contains("snaps.jsonl") && notes[0].contains('1'));
        assert!(notes[1].contains("vals.jsonl") && notes[1].contains('4'));
    }

    #[test]
    fn epistemic_notes_pass_through_an_unreadable_ledgers_own_message() {
        // An unreadable file is neither missing nor corrupt: the io error is
        // the most informative thing available, so it is surfaced verbatim.
        let notes = epistemic_ledger_notes(
            "snaps.jsonl",
            &SnapshotLedger::Unreadable("could not read snaps.jsonl: denied".to_string()),
            "vals.jsonl",
            &GradedLedger::Unreadable("could not read vals.jsonl: denied".to_string()),
        );
        assert_eq!(
            notes,
            vec![
                "could not read snaps.jsonl: denied".to_string(),
                "could not read vals.jsonl: denied".to_string(),
            ]
        );
    }
}
