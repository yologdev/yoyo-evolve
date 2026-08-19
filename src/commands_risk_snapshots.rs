//! Snapshot/validation persistence for the `/risk` subsystem.
//!
//! Extracted from `commands_risk.rs` (Day 126) — the append-only JSONL
//! persistence layer: risk snapshots (`.yoyo/risk_snapshots.jsonl`) and
//! prediction-validation events (`.yoyo/risk_validations.jsonl`).
//! Scoring, prediction, and reporting stay in `commands_risk.rs`, which
//! re-exports everything here so call sites are unchanged.

use crate::commands_risk::{
    compute_file_risk_scores, detect_emerging_risks, learn_weights_from_history, FileRisk,
};
use crate::commands_risk_emerging::EmergingRisk;
use crate::format::{DIM, RESET};

/// Default path for risk snapshot JSONL file.
pub(crate) const RISK_SNAPSHOT_PATH: &str = ".yoyo/risk_snapshots.jsonl";

/// Pure predicate: is opt-in risk auto-snapshot enabled given this env value?
/// Accepts "1"/"true"/"yes"; anything else (including None) is off.
fn risk_autosnapshot_enabled_for(val: Option<&str>) -> bool {
    matches!(val, Some("1") | Some("true") | Some("yes"))
}

/// Returns true if opt-in risk auto-snapshot on REPL exit is enabled.
/// Off by default (product-safe). Enabled by `YOYO_RISK_AUTOSNAPSHOT=1`
/// (also accepts "true"/"yes").
pub(crate) fn risk_autosnapshot_enabled() -> bool {
    risk_autosnapshot_enabled_for(std::env::var("YOYO_RISK_AUTOSNAPSHOT").ok().as_deref())
}

/// Build the JSON string for a risk snapshot entry.
///
/// Takes already-sorted risk scores, day number, and git hash.
/// Returns a single JSON line (no trailing newline).
pub(crate) fn build_risk_snapshot_json(
    risks: &[FileRisk],
    emerging: &[EmergingRisk],
    day: u32,
    git_hash: &str,
) -> String {
    let ts = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    let top_10: Vec<serde_json::Value> = risks
        .iter()
        .take(10)
        .map(|r| {
            serde_json::json!({
                "path": r.path,
                "score": (r.score * 100.0).round() / 100.0,
                "signals": r.signals,
            })
        })
        .collect();

    let emerging_arr: Vec<serde_json::Value> = emerging
        .iter()
        .take(10)
        .map(|e| {
            serde_json::json!({
                "path": e.path,
                "momentum": (e.momentum * 100.0).round() / 100.0,
                "current_rank": e.current_rank,
                "signals": e.signals,
            })
        })
        .collect();

    let snapshot = serde_json::json!({
        "ts": ts,
        "day": day,
        "git_hash": git_hash,
        "top_10": top_10,
        "emerging": emerging_arr,
    });

    serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string())
}

/// Append a risk snapshot JSON line to the given path.
pub(crate) fn write_risk_snapshot_to(
    path: &std::path::Path,
    json_line: &str,
) -> Result<(), std::io::Error> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{json_line}")?;
    Ok(())
}

/// Returns the `git_hash` of the last snapshot line in the given JSONL content,
/// or None if the content is empty / unparseable.
fn last_snapshot_git_hash(content: &str) -> Option<String> {
    let last = content.lines().rev().find(|l| !l.trim().is_empty())?;
    let v: serde_json::Value = serde_json::from_str(last).ok()?;
    v.get("git_hash")?.as_str().map(|s| s.to_string())
}

/// Automatically capture a risk snapshot after a successful commit.
///
/// Called from `commands_git.rs` after each successful `/commit`.
/// Silently skips on error (prints a dim note to stderr).
///
/// Idempotency guard: a snapshot records "predictions AS OF this HEAD", so
/// re-recording the same HEAD adds no information. If the last snapshot in the
/// file already has this `git_hash` (and it's a real hash, not "unknown"), we
/// skip the write — one snapshot per distinct commit-state keeps accumulation
/// clean for the accuracy-trend math.
pub(crate) fn auto_risk_snapshot() {
    let git_hash = crate::git::run_git(&["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    // Dedup by git hash: skip if the last snapshot already recorded this HEAD.
    // Never dedup on "unknown" — two "unknown" states may genuinely differ.
    if git_hash != "unknown" {
        let content = std::fs::read_to_string(RISK_SNAPSHOT_PATH).unwrap_or_default();
        if last_snapshot_git_hash(&content).as_deref() == Some(git_hash.as_str()) {
            eprintln!("  {DIM}(risk snapshot skipped: already recorded for {git_hash}){RESET}");
            return;
        }
    }

    let risks = compute_file_risk_scores();
    let emerging = detect_emerging_risks(&risks);

    let day: u32 = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let json_line = build_risk_snapshot_json(&risks, &emerging, day, &git_hash);
    if let Err(e) = write_risk_snapshot_to(std::path::Path::new(RISK_SNAPSHOT_PATH), &json_line) {
        eprintln!("  {DIM}(risk snapshot skipped: {e}){RESET}");
    }
}

/// Variant of `auto_risk_snapshot` that writes to a specific path (for testing).
#[cfg(test)]
fn auto_risk_snapshot_to(path: &std::path::Path) {
    let risks = compute_file_risk_scores();
    let emerging = detect_emerging_risks(&risks);

    let git_hash = crate::git::run_git(&["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    let day: u32 = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let json_line = build_risk_snapshot_json(&risks, &emerging, day, &git_hash);
    write_risk_snapshot_to(path, &json_line).expect("test snapshot write should succeed");
}

/// Default path for risk validation JSONL file.
pub(crate) const RISK_VALIDATION_PATH: &str = ".yoyo/risk_validations.jsonl";

/// Append a validation event to the given JSONL path. Reused by both the
/// watch-failure auto-validate path (`trigger: "watch_failure"`) and the CLI
/// `/risk validate` path (`trigger: "cli"`), so both accumulate the validation
/// half of the prediction meter in the same shape.
///
/// The JSON line carries `ts`, `day`, `trigger`, `hits`, `surprises`,
/// `predicted_count` (always 10), `accuracy_pct`, and — when emerging data is
/// present — `emerging_accuracy_pct`. These are exactly the fields the accuracy
/// readers (`parse_validation_events`, `parse_rich_validation_events`) consume.
///
/// `severity` tags what *kind* of outcome this event graded against —
/// `"watch_failure"` (post-prompt watch went red), `"watch_success"` (green
/// outcome: watch stayed green, or CLI validate found no breakage since the
/// snapshot — one vocabulary for "graded against a green outcome"), `"revert"`
/// (full revert — reserved for higher-severity wiring). `None` omits the key
/// entirely (CLI manual grading, legacy lines), so the file stays
/// backward-compatible with severity-less entries.
///
/// `snapshot_git_hash` records which snapshot this event graded — used by the
/// green-outcome dedup (grade each snapshot at most once). `None` omits the
/// key (all pre-existing event shapes).
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_validation_event(
    validation_path: &std::path::Path,
    day: u32,
    trigger: &str,
    hits: &[String],
    surprises: &[String],
    accuracy_pct: f64,
    emerging_accuracy_pct: Option<f64>,
    severity: Option<&str>,
    snapshot_git_hash: Option<&str>,
    ci_run_id: Option<u64>,
) -> std::io::Result<()> {
    let ts = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let mut event = serde_json::json!({
        "ts": ts,
        "day": day,
        "trigger": trigger,
        "hits": hits,
        "surprises": surprises,
        "predicted_count": 10,
        "accuracy_pct": accuracy_pct,
    });

    // Only record the anticipatory accuracy when we actually have emerging
    // data to grade against — absent means "no emerging list in the snapshot"
    // (older snapshots), NOT "0% accurate".
    if let Some(ea) = emerging_accuracy_pct {
        if let Some(obj) = event.as_object_mut() {
            obj.insert("emerging_accuracy_pct".to_string(), serde_json::json!(ea));
        }
    }

    // Severity is optional so legacy readers and historical lines stay valid —
    // absent means "untagged" (pre-severity event or CLI manual grading).
    if let Some(sev) = severity {
        if let Some(obj) = event.as_object_mut() {
            obj.insert("severity".to_string(), serde_json::json!(sev));
        }
    }

    // Snapshot hash is optional. Green events use it as their dedup key
    // ("grade each snapshot at most once"); failure-day events carry it purely
    // for auditability (#723) — so recall can be traced back to the exact
    // prediction list it graded instead of paired by timestamp guess.
    if let Some(hash) = snapshot_git_hash {
        if let Some(obj) = event.as_object_mut() {
            obj.insert("snapshot_git_hash".to_string(), serde_json::json!(hash));
        }
    }

    // CI-harvest events carry the GitHub Actions run id as their dedup key.
    // Absent on every other event shape (parse defensively downstream).
    if let Some(run_id) = ci_run_id {
        if let Some(obj) = event.as_object_mut() {
            obj.insert("ci_run_id".to_string(), serde_json::json!(run_id));
        }
    }

    if let Some(parent) = validation_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json_str = serde_json::to_string(&event).map_err(std::io::Error::other)?;

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(validation_path)?;
    writeln!(file, "{json_str}")
}

/// Pure grading helper: given the list of files that actually changed and a
/// set of predicted paths, return `(hits, accuracy_pct_rounded)`.
///
/// `hits` is the count of changed files that appeared in `predicted`;
/// accuracy is `hits / changed * 100`, rounded to 1 decimal. An empty
/// `changed` list yields `(0, 0.0)`. Used to grade BOTH the reactive
/// (`top_10`) and anticipatory (`emerging`) prediction sets against the same
/// outcome, so the allostatic-vs-homeostatic comparison is measurable.
/// Shared with the CLI `/risk validate` path (`commands_risk.rs`) so both
/// validation cranks grade with identical logic.
pub(crate) fn accuracy_of(
    changed: &[&str],
    predicted: &std::collections::HashSet<&str>,
) -> (usize, f64) {
    if changed.is_empty() {
        return (0, 0.0);
    }
    let hits = changed.iter().filter(|f| predicted.contains(**f)).count();
    let pct = (hits as f64 / changed.len() as f64) * 100.0;
    let pct_rounded = (pct * 10.0).round() / 10.0;
    (hits, pct_rounded)
}

/// Grade the anticipatory (emerging) prediction list. An empty list is
/// absence of a forecast, not a 0% one — it must grade as None (ungraded),
/// never Some(0.0). Verified Day 144: the recent 0.0% green events graded
/// against POPULATED lists (quiet-because-right); the #623 hypothesis of
/// quiet-because-silent did not hold — this helper pins the invariant anyway
/// so the previously-triplicated inline logic can't drift. Returns
/// `Some((hits, pct))` so the watch-failure path can surface the hit count;
/// call sites that only need the pct take `.map(|(_, pct)| pct)`.
pub(crate) fn emerging_grade_of(changed: &[&str], emerging: &[String]) -> Option<(usize, f64)> {
    if emerging.is_empty() {
        return None;
    }
    let emerging_set: std::collections::HashSet<&str> =
        emerging.iter().map(|s| s.as_str()).collect();
    Some(accuracy_of(changed, &emerging_set))
}

/// Return true if a **green** validation event referencing this snapshot's git
/// hash already exists in the validations JSONL content. Since Day 165 (#723)
/// failure-day events carry `snapshot_git_hash` too — for *auditability*, not
/// as a dedup key — so the severity filter below is load-bearing: without it a
/// red event would silently suppress a legitimate green grade for the same
/// snapshot. This is exactly the "was this snapshot
/// already green-graded?" question — the dedup that keeps repeated
/// `yoyo risk validate` runs from spamming duplicate green events.
pub(crate) fn green_event_exists_for(content: &str, snapshot_git_hash: &str) -> bool {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(is_green_json_event)
        .any(|v| v.get("snapshot_git_hash").and_then(|h| h.as_str()) == Some(snapshot_git_hash))
}

/// Green-day predicate over a raw JSONL value. Delegates the severity test to
/// `commands_risk_accuracy::is_green_event` — the authoritative definition —
/// instead of re-typing the marker string here (Day 140: never hand-copy a
/// predicate the code already owns). Only `severity` is read, so the other
/// fields are filler.
fn is_green_json_event(v: &serde_json::Value) -> bool {
    crate::commands_risk_accuracy::is_green_event(&ValidationEvent {
        day: 0,
        hit_count: 0,
        total_changed: 0,
        accuracy_pct: 0.0,
        emerging_accuracy_pct: None,
        severity: v
            .get("severity")
            .and_then(|s| s.as_str())
            .map(str::to_string),
    })
}

/// Outcome of attempting to record a green (no-failure) validation event.
#[derive(Debug, PartialEq)]
pub(crate) enum GreenGrade {
    /// Event written: `top_hits` of `total` changed src files were predicted
    /// (touched-but-didn't-break — false-positive evidence for the reactive
    /// column), plus the anticipatory grade when the snapshot carried one.
    Recorded {
        top_hits: usize,
        total: usize,
        top_pct: f64,
        emerging_pct: Option<f64>,
    },
    /// This snapshot was already green-graded — nothing written.
    Deduped,
    /// No `src/` files changed since the snapshot — nothing to grade
    /// (a 0/0 event would read as 0% accuracy and drag the average).
    NoSrcChanges,
}

/// Record a green-outcome validation event: commits happened since the
/// snapshot but nothing broke. Grades BOTH the reactive `top_10` and the
/// anticipatory `emerging` prediction sets against the files that were
/// *touched* — under a green outcome, a predicted-risky file that changed
/// without breaking is false-positive evidence. Uses severity
/// `"watch_success"` (the same green marker the Day-139 watch path writes,
/// so readers see one vocabulary; `trigger: "cli"` still identifies the
/// crank) and stamps `snapshot_git_hash` so each snapshot is green-graded
/// at most once.
///
/// Green events COUNT toward `compute_accuracy_stats` exactly like the
/// watch-green events already do — under a green outcome the hit rate reads
/// as a false-positive rate, which is the meter's other half.
pub(crate) fn record_green_validation_to(
    validation_path: &std::path::Path,
    day: u32,
    snapshot_git_hash: &str,
    changed_files: &[String],
    top_10: &[String],
    emerging: &[String],
) -> std::io::Result<GreenGrade> {
    // Risk predictions cover source code only — mirror the watch path's filter.
    let src_files: Vec<&String> = changed_files
        .iter()
        .filter(|f| f.starts_with("src/"))
        .collect();
    if src_files.is_empty() {
        return Ok(GreenGrade::NoSrcChanges);
    }

    // Dedup: grade each snapshot at most once.
    if let Ok(existing) = std::fs::read_to_string(validation_path) {
        if green_event_exists_for(&existing, snapshot_git_hash) {
            return Ok(GreenGrade::Deduped);
        }
    }

    let changed_refs: Vec<&str> = src_files.iter().map(|s| s.as_str()).collect();
    let top_set: std::collections::HashSet<&str> = top_10.iter().map(|s| s.as_str()).collect();
    let (top_hits, top_pct) = accuracy_of(&changed_refs, &top_set);

    // Legacy snapshots without an emerging list stay None (ungraded) —
    // distinct from Some(0.0), so they don't drag the emerging average.
    let emerging_pct = emerging_grade_of(&changed_refs, emerging).map(|(_, pct)| pct);

    let mut hits: Vec<String> = Vec::new();
    let mut surprises: Vec<String> = Vec::new();
    for f in &src_files {
        if top_set.contains(f.as_str()) {
            hits.push(f.to_string());
        } else {
            surprises.push(f.to_string());
        }
    }
    let total = src_files.len();

    write_validation_event(
        validation_path,
        day,
        "cli",
        &hits,
        &surprises,
        top_pct,
        emerging_pct,
        Some("watch_success"),
        Some(snapshot_git_hash),
        None, // green event — not a CI harvest
    )?;

    Ok(GreenGrade::Recorded {
        top_hits,
        total,
        top_pct,
        emerging_pct,
    })
}

/// Automatically validate risk predictions against files that were changed
/// in the current session. Called after watch failures (or successes) to
/// close the prediction-validation loop.
///
/// - No-op if no snapshots exist.
/// - No-op if no `changed_files` match `src/` paths.
/// - Appends a validation event to `.yoyo/risk_validations.jsonl`.
/// - Prints a brief 2-3 line stderr summary when there are results.
///
/// `severity` tags what kind of outcome the predictions are graded against:
/// `"watch_failure"` for a red watch cycle (the lower-severity feed that lets
/// the meter accumulate without a catastrophe), `"watch_success"` for a clean
/// green cycle, `"revert"` reserved for full reverts.
pub(crate) fn auto_validate_after_failure(changed_files: &[String], severity: &str) {
    auto_validate_after_failure_to(
        changed_files,
        severity,
        std::path::Path::new(RISK_SNAPSHOT_PATH),
        std::path::Path::new(RISK_VALIDATION_PATH),
    );
}

/// Inner implementation with configurable paths (for testing).
fn auto_validate_after_failure_to(
    changed_files: &[String],
    severity: &str,
    snapshot_path: &std::path::Path,
    validation_path: &std::path::Path,
) {
    // Filter to only src/ files — risk predictions focus on source code
    let src_files: Vec<&String> = changed_files
        .iter()
        .filter(|f| f.starts_with("src/"))
        .collect();
    if src_files.is_empty() {
        return;
    }

    // Load the most recent snapshot
    let content = match std::fs::read_to_string(snapshot_path) {
        Ok(c) => c,
        Err(_) => return, // no snapshots exist — no-op
    };
    let snapshots = parse_all_snapshots(&content);
    let last = match snapshots.last() {
        Some(s) => s,
        None => return, // no valid snapshots — no-op
    };

    // Classify: which changed src/ files were predicted (hits) vs not (surprises)
    let predicted_set: std::collections::HashSet<&str> =
        last.predicted.iter().map(|s| s.as_str()).collect();

    let mut hits: Vec<String> = Vec::new();
    let mut surprises: Vec<String> = Vec::new();
    for f in &src_files {
        if predicted_set.contains(f.as_str()) {
            hits.push(f.to_string());
        } else {
            surprises.push(f.to_string());
        }
    }

    // Only produce output if there's something meaningful to report
    if hits.is_empty() && surprises.is_empty() {
        return;
    }

    let total_changed = hits.len() + surprises.len();
    let accuracy_pct = if total_changed > 0 {
        (hits.len() as f64 / total_changed as f64) * 100.0
    } else {
        0.0
    };
    let accuracy_pct_rounded = (accuracy_pct * 10.0).round() / 10.0;

    // Also grade the *anticipatory* (emerging) prediction set against the same
    // outcome — the allostatic signal. Only when the snapshot actually carried
    // an emerging list (older snapshots have none → None, so the reader can tell
    // "no emerging data" from "0% accurate").
    let changed_refs: Vec<&str> = src_files.iter().map(|s| s.as_str()).collect();
    let emerging_grade = emerging_grade_of(&changed_refs, &last.emerging);
    let emerging_accuracy_pct = emerging_grade.map(|(_, pct)| pct);

    let day: u32 = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    // Append the validation event via the shared writer (same shape the CLI
    // `/risk validate` path uses).
    if let Err(e) = write_validation_event(
        validation_path,
        day,
        "watch_failure",
        &hits,
        &surprises,
        accuracy_pct_rounded,
        emerging_accuracy_pct,
        Some(severity),
        Some(&last.git_hash), // the snapshot this event graded — auditability, not a dedup key
        None,                 // not a CI-harvested event — no run id
    ) {
        eprintln!("  {DIM}(warning: could not write risk validation entry: {e}){RESET}");
    }

    // Brief stderr summary (2-3 lines)
    eprintln!(
        "{DIM}  📊 Risk validation: {}/{} changed files were in top-10 predictions ({:.1}% accuracy){RESET}",
        hits.len(),
        total_changed,
        accuracy_pct_rounded,
    );
    if let Some((emerging_hits, e_pct)) = emerging_grade {
        // Allostatic-vs-homeostatic comparison, visible the moment it's measured.
        eprintln!(
            "{DIM}     📊 Emerging (anticipatory) accuracy: {}/{} ({:.1}%) — reactive was {:.1}%{RESET}",
            emerging_hits, total_changed, e_pct, accuracy_pct_rounded,
        );
    }
    if !hits.is_empty() {
        let hit_list: Vec<&str> = hits.iter().map(|s| s.as_str()).collect();
        eprintln!(
            "{DIM}     Predicted correctly: {}{RESET}",
            hit_list.join(", ")
        );
    }
    if !surprises.is_empty() {
        let surprise_list: Vec<&str> = surprises.iter().map(|s| s.as_str()).collect();
        eprintln!(
            "{DIM}     Surprises (not predicted): {}{RESET}",
            surprise_list.join(", ")
        );
    }

    // Update learned weights after every validation event
    learn_weights_from_history();
}

// The JSONL ledger *readers* live in `commands_risk_parse` (extracted Day 166:
// a pure move — const, struct, fn and every test carried over verbatim). They are
// re-exported here so every existing call site compiles unchanged. `FailedCiRun`
// is deliberately absent: no call site ever names it (it is only reached as
// `parse_failed_ci_runs`'s return type), so re-exporting it would be an unused
// import rather than a preserved seam.
pub(crate) use crate::commands_risk_parse::{
    ci_event_exists_for, epistemic_ledger_notes, load_validation_history_from, parse_all_snapshots,
    parse_failed_ci_runs, parse_validation_events, read_graded_ledger, read_snapshot_ledger,
    read_validation_ledger, snapshot_before, GradedEvent, GradedLedger, ParsedSnapshot,
    SnapshotLedger, ValidationEvent, ValidationLedger,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_autosnapshot_enabled_for_truthy() {
        assert!(risk_autosnapshot_enabled_for(Some("1")));
        assert!(risk_autosnapshot_enabled_for(Some("true")));
        assert!(risk_autosnapshot_enabled_for(Some("yes")));
    }

    #[test]
    fn test_risk_autosnapshot_enabled_for_falsy() {
        assert!(!risk_autosnapshot_enabled_for(None));
        assert!(!risk_autosnapshot_enabled_for(Some("0")));
        assert!(!risk_autosnapshot_enabled_for(Some("off")));
        assert!(!risk_autosnapshot_enabled_for(Some("")));
        assert!(!risk_autosnapshot_enabled_for(Some("junk")));
        // case-sensitive: only exact lowercase tokens count
        assert!(!risk_autosnapshot_enabled_for(Some("TRUE")));
        assert!(!risk_autosnapshot_enabled_for(Some("Yes")));
    }

    #[test]
    fn test_last_snapshot_git_hash_basic() {
        let jsonl = "{\"day\":100,\"git_hash\":\"aaa111\",\"top_10\":[]}\n{\"day\":101,\"git_hash\":\"bbb222\",\"top_10\":[]}";
        assert_eq!(
            last_snapshot_git_hash(jsonl),
            Some("bbb222".to_string()),
            "should return the last line's git_hash"
        );
    }

    #[test]
    fn test_last_snapshot_git_hash_empty() {
        assert_eq!(last_snapshot_git_hash(""), None);
    }

    #[test]
    fn test_last_snapshot_git_hash_trailing_blank() {
        let jsonl = "{\"day\":100,\"git_hash\":\"aaa111\",\"top_10\":[]}\n\n";
        assert_eq!(
            last_snapshot_git_hash(jsonl),
            Some("aaa111".to_string()),
            "trailing blank lines should be ignored"
        );
    }

    #[test]
    fn test_last_snapshot_git_hash_malformed_last_line() {
        let jsonl = "{\"day\":100,\"git_hash\":\"aaa111\",\"top_10\":[]}\ngarbage";
        assert_eq!(
            last_snapshot_git_hash(jsonl),
            None,
            "malformed last line → None"
        );
    }

    #[test]
    fn test_last_snapshot_git_hash_dedup_decision() {
        let jsonl = "{\"day\":100,\"git_hash\":\"aaa111\",\"top_10\":[]}\n{\"day\":101,\"git_hash\":\"bbb222\",\"top_10\":[]}";
        // Same hash as last → would dedup.
        assert_eq!(
            last_snapshot_git_hash(jsonl).as_deref(),
            Some("bbb222"),
            "same-hash case detected"
        );
        // A different hash would NOT match the last line's hash.
        assert_ne!(
            last_snapshot_git_hash(jsonl).as_deref(),
            Some("ccc333"),
            "different-hash case detected"
        );
    }

    #[test]
    fn test_last_snapshot_git_hash_missing_field() {
        let jsonl = "{\"day\":100,\"top_10\":[]}";
        assert_eq!(
            last_snapshot_git_hash(jsonl),
            None,
            "missing git_hash field → None"
        );
    }

    #[test]
    fn test_risk_snapshot_serialization() {
        // Verify snapshot JSON is valid JSONL
        let risks = vec![
            FileRisk {
                path: "src/foo.rs".to_string(),
                score: 0.82,
                signals: vec!["▲churn", "▲size"],
                test_density: 2.0,
            },
            FileRisk {
                path: "src/bar.rs".to_string(),
                score: 0.71,
                signals: vec!["▲churn"],
                test_density: 0.5,
            },
        ];

        let json = build_risk_snapshot_json(&risks, &[], 112, "abc123f");
        // Must be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["day"], 112);
        assert_eq!(parsed["git_hash"], "abc123f");
        let top = parsed["top_10"].as_array().expect("top_10 is array");
        assert_eq!(top.len(), 2);
        assert_eq!(top[0]["path"], "src/foo.rs");
        assert!((top[0]["score"].as_f64().unwrap() - 0.82).abs() < 0.001);
        let sigs = top[0]["signals"].as_array().expect("signals is array");
        assert_eq!(sigs.len(), 2);
        assert_eq!(sigs[0], "▲churn");
        assert_eq!(sigs[1], "▲size");
        // Must have a timestamp
        assert!(parsed["ts"].as_str().is_some());
    }

    #[test]
    fn test_risk_snapshot_writes_jsonl() {
        // Write to a temp file and verify it's valid JSONL
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("risk_snapshots.jsonl");

        let risks = vec![FileRisk {
            path: "src/main.rs".to_string(),
            score: 0.55,
            signals: vec!["▲size"],
            test_density: 0.0,
        }];

        let json = build_risk_snapshot_json(&risks, &[], 42, "deadbee");
        write_risk_snapshot_to(&path, &json).expect("write ok");

        // Write a second snapshot
        let json2 = build_risk_snapshot_json(&risks, &[], 43, "cafebab");
        write_risk_snapshot_to(&path, &json2).expect("write ok");

        // Read back and verify both lines are valid JSON
        let contents = std::fs::read_to_string(&path).expect("read");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        let p1: serde_json::Value = serde_json::from_str(lines[0]).expect("line 1 valid JSON");
        let p2: serde_json::Value = serde_json::from_str(lines[1]).expect("line 2 valid JSON");
        assert_eq!(p1["day"], 42);
        assert_eq!(p2["day"], 43);
    }

    #[test]
    fn test_risk_snapshot_top_10_limit() {
        // If there are more than 10 risks, only top 10 are saved
        let risks: Vec<FileRisk> = (0..20)
            .map(|i| FileRisk {
                path: format!("src/file_{i}.rs"),
                score: 1.0 - (i as f64 * 0.05),
                signals: vec!["▲churn"],
                test_density: 0.0,
            })
            .collect();

        let json = build_risk_snapshot_json(&risks, &[], 1, "1234567");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let top = parsed["top_10"].as_array().expect("top_10 array");
        assert_eq!(top.len(), 10);
        // First entry should be highest score
        assert_eq!(top[0]["path"], "src/file_0.rs");
    }

    #[test]
    fn test_risk_snapshot_records_emerging() {
        // The anticipatory signal must be persisted alongside top_10 so Task 2
        // can validate it. A snapshot with synthetic emerging entries round-trips.
        let risks = vec![FileRisk {
            path: "src/foo.rs".to_string(),
            score: 0.82,
            signals: vec!["▲churn"],
            test_density: 0.0,
        }];
        let emerging = vec![
            EmergingRisk {
                path: "src/rising.rs".to_string(),
                momentum: 2.146,
                current_rank: 12,
                signals: vec!["7d change-rate ×2.1 vs 30d".to_string()],
            },
            EmergingRisk {
                path: "src/climbing.rs".to_string(),
                momentum: 1.5,
                current_rank: 8,
                signals: vec!["accelerating".to_string()],
            },
        ];

        let json = build_risk_snapshot_json(&risks, &emerging, 130, "feed123");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let emerging_arr = parsed["emerging"].as_array().expect("emerging is array");
        assert_eq!(emerging_arr.len(), 2);
        assert_eq!(emerging_arr[0]["path"], "src/rising.rs");
        // momentum rounded to 2 decimals the same way score is
        assert!((emerging_arr[0]["momentum"].as_f64().unwrap() - 2.15).abs() < 0.001);
        assert_eq!(emerging_arr[0]["current_rank"], 12);
        let sigs = emerging_arr[0]["signals"]
            .as_array()
            .expect("emerging signals is array");
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0], "7d change-rate ×2.1 vs 30d");
        assert_eq!(emerging_arr[1]["path"], "src/climbing.rs");
        assert_eq!(emerging_arr[1]["current_rank"], 8);
    }

    #[test]
    fn test_risk_snapshot_empty_emerging_yields_empty_array() {
        // An empty emerging list must serialize as "emerging": [] (present, not
        // missing) so downstream readers can rely on the key existing.
        let risks = vec![FileRisk {
            path: "src/main.rs".to_string(),
            score: 0.55,
            signals: vec!["▲size"],
            test_density: 0.0,
        }];

        let json = build_risk_snapshot_json(&risks, &[], 42, "deadbee");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let emerging_arr = parsed["emerging"]
            .as_array()
            .expect("emerging key present as array even when empty");
        assert!(emerging_arr.is_empty());
    }

    #[test]
    fn test_risk_snapshot_emerging_top_10_limit() {
        // More than 10 emerging entries → only the top 10 are persisted.
        let risks = vec![FileRisk {
            path: "src/main.rs".to_string(),
            score: 0.55,
            signals: vec!["▲size"],
            test_density: 0.0,
        }];
        let emerging: Vec<EmergingRisk> = (0..15)
            .map(|i| EmergingRisk {
                path: format!("src/rising_{i}.rs"),
                momentum: 3.0 - (i as f64 * 0.1),
                current_rank: 20 + i,
                signals: vec!["accelerating".to_string()],
            })
            .collect();

        let json = build_risk_snapshot_json(&risks, &emerging, 1, "1234567");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let emerging_arr = parsed["emerging"].as_array().expect("emerging array");
        assert_eq!(emerging_arr.len(), 10);
        assert_eq!(emerging_arr[0]["path"], "src/rising_0.rs");
    }

    // CONTRACT: this is the evolve.sh-hook feed contract. The autonomous loop
    // is meant to call `yoyo risk snapshot` once per session (a human patch to
    // the protected scripts/evolve.sh — tracked in the "wire risk snapshot into
    // evolve.sh" agent-help-wanted issue). That CLI path runs
    // build_risk_snapshot_json → write_risk_snapshot_to; the accuracy/trend math
    // downstream reads the file back via parse_all_snapshots. This test pins the
    // full round-trip so the harness patch has a red/green receipt: if either the
    // writer's field layout or the parser's field expectations drift, this breaks.
    #[test]
    fn test_snapshot_feed_contract_roundtrip() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("risk_snapshots.jsonl");

        // Synthetic risk data + synthetic git hash (no real repo touched).
        let risks = vec![
            FileRisk {
                path: "src/commands_risk.rs".to_string(),
                score: 0.91,
                signals: vec!["▲churn", "▲size"],
                test_density: 0.3,
            },
            FileRisk {
                path: "src/prompt.rs".to_string(),
                score: 0.64,
                signals: vec!["▲churn"],
                test_density: 1.2,
            },
        ];

        // The exact two-step the non-interactive CLI feed performs.
        let json_line = build_risk_snapshot_json(&risks, &[], 130, "feed123");
        write_risk_snapshot_to(&path, &json_line).expect("feed write must succeed");

        // Read back exactly as the downstream accuracy math does.
        let content = std::fs::read_to_string(&path).expect("feed read");
        let parsed = parse_all_snapshots(&content);

        // The invariant the harness feed depends on: one snapshot, with the
        // day, git hash, and per-file predicted paths all surviving the round-trip.
        assert_eq!(parsed.len(), 1, "one snapshot line written → one parsed");
        assert_eq!(parsed[0].day, 130, "day must survive round-trip");
        assert_eq!(
            parsed[0].git_hash, "feed123",
            "git_hash must survive round-trip"
        );
        assert_eq!(
            parsed[0].predicted,
            vec![
                "src/commands_risk.rs".to_string(),
                "src/prompt.rs".to_string()
            ],
            "per-file predicted paths (in rank order) must survive round-trip"
        );
    }

    #[test]
    fn test_auto_risk_snapshot_writes_valid_jsonl() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("snapshots.jsonl");

        auto_risk_snapshot_to(&path);

        let contents = std::fs::read_to_string(&path).expect("read snapshot file");
        assert!(
            !contents.trim().is_empty(),
            "snapshot file should not be empty"
        );

        // Each line should be valid JSON
        for line in contents.lines() {
            let parsed: serde_json::Value =
                serde_json::from_str(line).expect("each line should be valid JSON");
            assert!(parsed.get("day").is_some(), "snapshot should have 'day'");
            assert!(
                parsed.get("git_hash").is_some(),
                "snapshot should have 'git_hash'"
            );
            assert!(
                parsed.get("top_10").is_some(),
                "snapshot should have 'top_10'"
            );
        }
    }

    #[test]
    fn test_auto_risk_snapshot_appends_not_overwrites() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("snapshots.jsonl");

        // Write two snapshots
        auto_risk_snapshot_to(&path);
        auto_risk_snapshot_to(&path);

        let contents = std::fs::read_to_string(&path).expect("read snapshot file");
        let line_count = contents.lines().count();
        assert_eq!(
            line_count, 2,
            "two calls should produce two lines, got {line_count}"
        );
    }

    #[test]
    fn test_auto_validate_with_synthetic_snapshot() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl");
        let val_path = dir.path().join("validations.jsonl");

        // Write a synthetic snapshot with known top-10 files
        let snapshot = serde_json::json!({
            "ts": "2025-01-15T12:00:00Z",
            "day": 100,
            "git_hash": "abc1234",
            "top_10": [
                {"path": "src/main.rs", "score": 0.9, "signals": ["churn"]},
                {"path": "src/cli.rs", "score": 0.8, "signals": ["size"]},
                {"path": "src/watch.rs", "score": 0.7, "signals": ["recent"]},
                {"path": "src/repl.rs", "score": 0.6, "signals": ["churn"]},
                {"path": "src/tools.rs", "score": 0.5, "signals": ["coupled"]},
            ]
        });
        std::fs::write(&snap_path, serde_json::to_string(&snapshot).unwrap())
            .expect("write snapshot");

        // Simulate changed files: 3 match predictions, 2 are surprises
        let changed = vec![
            "src/main.rs".to_string(),   // hit
            "src/cli.rs".to_string(),    // hit
            "src/prompt.rs".to_string(), // surprise
            "src/safety.rs".to_string(), // surprise
            "src/watch.rs".to_string(),  // hit
        ];

        auto_validate_after_failure_to(&changed, "watch_failure", &snap_path, &val_path);

        // Verify JSONL output
        let contents = std::fs::read_to_string(&val_path).expect("read validation file");
        assert!(
            !contents.trim().is_empty(),
            "validation file should not be empty"
        );

        let parsed: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert_eq!(parsed["trigger"], "watch_failure");
        assert_eq!(parsed["predicted_count"], 10);

        let hits = parsed["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 3, "should have 3 hits");
        assert!(hits.contains(&serde_json::json!("src/main.rs")));
        assert!(hits.contains(&serde_json::json!("src/cli.rs")));
        assert!(hits.contains(&serde_json::json!("src/watch.rs")));

        let surprises = parsed["surprises"].as_array().unwrap();
        assert_eq!(surprises.len(), 2, "should have 2 surprises");
        assert!(surprises.contains(&serde_json::json!("src/prompt.rs")));
        assert!(surprises.contains(&serde_json::json!("src/safety.rs")));

        // accuracy = 3/5 = 60%
        let accuracy = parsed["accuracy_pct"].as_f64().unwrap();
        assert!(
            (accuracy - 60.0).abs() < 0.1,
            "accuracy should be ~60%, got {accuracy}"
        );
    }

    #[test]
    fn test_auto_validate_noop_when_no_snapshots() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl"); // does not exist
        let val_path = dir.path().join("validations.jsonl");

        let changed = vec!["src/main.rs".to_string()];
        auto_validate_after_failure_to(&changed, "watch_failure", &snap_path, &val_path);

        // Validation file should not be created
        assert!(
            !val_path.exists(),
            "validation file should not exist when no snapshots"
        );
    }

    #[test]
    fn test_auto_validate_noop_when_no_src_files() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl");
        let val_path = dir.path().join("validations.jsonl");

        // Write a valid snapshot
        let snapshot = serde_json::json!({
            "ts": "2025-01-15T12:00:00Z",
            "day": 100,
            "git_hash": "abc1234",
            "top_10": [
                {"path": "src/main.rs", "score": 0.9, "signals": ["churn"]},
            ]
        });
        std::fs::write(&snap_path, serde_json::to_string(&snapshot).unwrap())
            .expect("write snapshot");

        // Changed files are all non-src/
        let changed = vec![
            "README.md".to_string(),
            "docs/guide.md".to_string(),
            "Cargo.toml".to_string(),
        ];
        auto_validate_after_failure_to(&changed, "watch_failure", &snap_path, &val_path);

        // Validation file should not be created
        assert!(
            !val_path.exists(),
            "validation file should not exist when no src/ files changed"
        );
    }

    #[test]
    fn test_write_validation_event_cli_trigger_roundtrip() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nested").join("validations.jsonl");

        let hits = vec!["src/main.rs".to_string(), "src/cli.rs".to_string()];
        let surprises = vec!["src/prompt.rs".to_string()];
        write_validation_event(
            &path, 129, "cli", &hits, &surprises, 66.7, None, None, None, None,
        )
        .expect("write validation event");

        let contents = std::fs::read_to_string(&path).expect("read validation file");
        // Raw JSON shape check: trigger and predicted_count.
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert_eq!(raw["trigger"], "cli");
        assert_eq!(raw["predicted_count"], 10);

        // Reader roundtrip.
        let events = parse_validation_events(&contents);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].day, 129);
        assert_eq!(events[0].hit_count, 2);
        assert_eq!(events[0].total_changed, 3);
        assert!((events[0].accuracy_pct - 66.7).abs() < 0.1);
    }

    #[test]
    fn test_write_validation_event_watch_failure_trigger() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");

        let hits = vec!["src/tools.rs".to_string()];
        let surprises: Vec<String> = vec![];
        write_validation_event(
            &path,
            100,
            "watch_failure",
            &hits,
            &surprises,
            100.0,
            None,
            Some("watch_failure"),
            None,
            None,
        )
        .expect("write validation event");

        let contents = std::fs::read_to_string(&path).expect("read validation file");
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert_eq!(raw["trigger"], "watch_failure");
        assert_eq!(
            raw["severity"], "watch_failure",
            "severity tag written when Some"
        );

        let events = parse_validation_events(&contents);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].hit_count, 1);
        assert_eq!(events[0].total_changed, 1);
    }

    #[test]
    fn test_write_validation_event_appends() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");

        let hits = vec!["src/main.rs".to_string()];
        let surprises: Vec<String> = vec![];
        write_validation_event(
            &path, 1, "cli", &hits, &surprises, 100.0, None, None, None, None,
        )
        .expect("first write");
        write_validation_event(
            &path, 2, "cli", &hits, &surprises, 100.0, None, None, None, None,
        )
        .expect("second write");

        let events = load_validation_history_from(&path);
        assert_eq!(events.len(), 2, "appending twice yields two lines");
        assert_eq!(events[0].day, 1);
        assert_eq!(events[1].day, 2);
    }

    #[test]
    fn test_accuracy_of_grades_reactive_and_emerging() {
        // A single outcome graded against two distinct prediction sets: the
        // reactive (top_10) set and the anticipatory (emerging) set. Both must
        // be scored by the same pure helper so the allostatic-vs-homeostatic
        // comparison is apples-to-apples.
        let changed = ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"];

        // Reactive set predicts 2 of the 4 changed files.
        let reactive: std::collections::HashSet<&str> =
            ["src/a.rs", "src/b.rs", "src/z.rs"].into_iter().collect();
        let (r_hits, r_pct) = accuracy_of(&changed, &reactive);
        assert_eq!(r_hits, 2, "reactive hits = a,b");
        assert!((r_pct - 50.0).abs() < 0.001, "2/4 = 50.0%");

        // Emerging set predicts 3 of the 4 — the anticipatory signal beats the
        // reactive baseline on this outcome.
        let emerging: std::collections::HashSet<&str> =
            ["src/a.rs", "src/b.rs", "src/c.rs"].into_iter().collect();
        let (e_hits, e_pct) = accuracy_of(&changed, &emerging);
        assert_eq!(e_hits, 3, "emerging hits = a,b,c");
        assert!((e_pct - 75.0).abs() < 0.001, "3/4 = 75.0%");

        // Rounding to 1 decimal: 1/3 = 33.333... → 33.3.
        let three = ["src/x.rs", "src/y.rs", "src/w.rs"];
        let one_hit: std::collections::HashSet<&str> = ["src/x.rs"].into_iter().collect();
        let (h, p) = accuracy_of(&three, &one_hit);
        assert_eq!(h, 1);
        assert!((p - 33.3).abs() < 0.001, "1/3 rounds to 33.3");

        // Empty changed list yields (0, 0.0), never a divide-by-zero.
        let empty: [&str; 0] = [];
        let (zh, zp) = accuracy_of(&empty, &reactive);
        assert_eq!(zh, 0);
        assert!((zp - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_emerging_grade_of_empty_list_is_ungraded_none() {
        // An empty emerging list is absence of a forecast, not a 0% one — it
        // must grade as None (ungraded), never Some(0.0). Some(0.0) would drag
        // the anticipatory average with grades that were never predictions.
        let changed = ["src/a.rs"];
        assert_eq!(emerging_grade_of(&changed, &[]), None);
    }

    #[test]
    fn test_emerging_grade_of_populated_no_overlap_is_real_zero() {
        // A populated list with no overlap on the changed files is a REAL
        // graded zero — the forecast existed and missed. Distinct from the
        // empty-list None above.
        let changed = ["src/a.rs", "src/b.rs"];
        let emerging = vec!["src/z.rs".to_string(), "src/y.rs".to_string()];
        let (hits, pct) = emerging_grade_of(&changed, &emerging).expect("populated list grades");
        assert_eq!(hits, 0);
        assert!((pct - 0.0).abs() < 0.001, "0/2 = Some(0.0), a real zero");
    }

    #[test]
    fn test_emerging_grade_of_populated_with_overlap() {
        // Overlap grades via the same accuracy_of math as the reactive column.
        let changed = ["src/a.rs", "src/b.rs", "src/c.rs"];
        let emerging = vec!["src/a.rs".to_string(), "src/x.rs".to_string()];
        let (hits, pct) = emerging_grade_of(&changed, &emerging).expect("populated list grades");
        assert_eq!(hits, 1, "only a.rs was both changed and forecast");
        assert!((pct - 33.3).abs() < 0.001, "1/3 rounds to 33.3");
    }

    #[test]
    fn test_write_validation_event_emerging_accuracy_roundtrips() {
        // When emerging data is present, write_validation_event must emit the
        // emerging_accuracy_pct field and the reader must recover it.
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");

        let hits = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let surprises = vec!["src/c.rs".to_string(), "src/d.rs".to_string()];
        write_validation_event(
            &path,
            138,
            "watch_failure",
            &hits,
            &surprises,
            50.0,
            Some(75.0),
            Some("watch_failure"),
            None,
            None,
        )
        .expect("write validation event with emerging");

        let contents = std::fs::read_to_string(&path).expect("read validation file");
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert!(
            (raw["emerging_accuracy_pct"].as_f64().unwrap() - 75.0).abs() < 0.001,
            "emerging_accuracy_pct written when Some"
        );

        let events = parse_validation_events(&contents);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].emerging_accuracy_pct, Some(75.0));
    }

    #[test]
    fn test_record_green_validation_writes_one_graded_event() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");

        let changed = vec![
            "src/main.rs".to_string(),
            "src/cli.rs".to_string(),
            "README.md".to_string(), // non-src: filtered out of grading
        ];
        let top_10 = vec!["src/main.rs".to_string(), "src/prompt.rs".to_string()];
        let emerging = vec!["src/cli.rs".to_string()];

        let grade = record_green_validation_to(&path, 140, "abc1234", &changed, &top_10, &emerging)
            .expect("record green validation");

        // 2 src files changed; 1 (main.rs) was in top_10 → 50% reactive;
        // 1 (cli.rs) was in emerging → 50% anticipatory.
        assert_eq!(
            grade,
            GreenGrade::Recorded {
                top_hits: 1,
                total: 2,
                top_pct: 50.0,
                emerging_pct: Some(50.0),
            }
        );

        let contents = std::fs::read_to_string(&path).expect("read validation file");
        assert_eq!(
            contents.lines().filter(|l| !l.trim().is_empty()).count(),
            1,
            "exactly one event written"
        );
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        // Same green marker the Day-139 watch path uses — one vocabulary.
        assert_eq!(raw["severity"], "watch_success");
        assert_eq!(raw["trigger"], "cli");
        assert_eq!(raw["snapshot_git_hash"], "abc1234");
        assert!((raw["accuracy_pct"].as_f64().unwrap() - 50.0).abs() < 0.001);
        assert!((raw["emerging_accuracy_pct"].as_f64().unwrap() - 50.0).abs() < 0.001);
        // Under a green outcome the "hits" are touched-but-didn't-break —
        // false-positive evidence for the reactive column.
        assert_eq!(raw["hits"], serde_json::json!(["src/main.rs"]));
        assert_eq!(raw["surprises"], serde_json::json!(["src/cli.rs"]));
    }

    #[test]
    fn test_record_green_validation_dedups_same_snapshot() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");

        let changed = vec!["src/main.rs".to_string()];
        let top_10 = vec!["src/main.rs".to_string()];
        let emerging: Vec<String> = vec![];

        let first = record_green_validation_to(&path, 140, "abc1234", &changed, &top_10, &emerging)
            .expect("first green validation");
        assert!(matches!(first, GreenGrade::Recorded { .. }));

        // Second run against the SAME snapshot hash → deduped, nothing written.
        let second =
            record_green_validation_to(&path, 140, "abc1234", &changed, &top_10, &emerging)
                .expect("second green validation");
        assert_eq!(second, GreenGrade::Deduped);

        let contents = std::fs::read_to_string(&path).expect("read validation file");
        assert_eq!(
            contents.lines().filter(|l| !l.trim().is_empty()).count(),
            1,
            "dedup: still exactly one event after a repeat run"
        );

        // A DIFFERENT snapshot hash is not deduped.
        let third = record_green_validation_to(&path, 141, "def5678", &changed, &top_10, &emerging)
            .expect("third green validation");
        assert!(matches!(third, GreenGrade::Recorded { .. }));
        let contents = std::fs::read_to_string(&path).expect("read validation file");
        assert_eq!(
            contents.lines().filter(|l| !l.trim().is_empty()).count(),
            2,
            "a new snapshot hash writes a new event"
        );
    }

    #[test]
    fn test_record_green_validation_no_src_changes_writes_nothing() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");

        let changed = vec!["README.md".to_string(), "docs/src/intro.md".to_string()];
        let grade = record_green_validation_to(&path, 140, "abc1234", &changed, &[], &[])
            .expect("green validation with no src changes");
        assert_eq!(grade, GreenGrade::NoSrcChanges);
        assert!(
            !path.exists(),
            "no src/ changes → no event file (a 0/0 event would drag the average)"
        );
    }

    #[test]
    fn test_accuracy_readers_count_green_event() {
        // Behavior choice (documented on record_green_validation_to): green
        // events COUNT toward compute_accuracy_stats — under a green outcome
        // the hit rate reads as a false-positive rate, the meter's other half.
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("validations.jsonl");

        let changed = vec!["src/main.rs".to_string(), "src/cli.rs".to_string()];
        let top_10 = vec!["src/main.rs".to_string()];
        let emerging = vec!["src/cli.rs".to_string()];
        record_green_validation_to(&path, 140, "abc1234", &changed, &top_10, &emerging)
            .expect("record green validation");

        let contents = std::fs::read_to_string(&path).expect("read validation file");
        let events = parse_validation_events(&contents);
        assert_eq!(events.len(), 1, "reader parses the green shape");
        assert_eq!(events[0].severity.as_deref(), Some("watch_success"));
        assert_eq!(events[0].hit_count, 1);
        assert_eq!(events[0].total_changed, 2);
        assert_eq!(events[0].emerging_accuracy_pct, Some(50.0));

        let stats = crate::commands_risk_accuracy::compute_accuracy_stats(&events);
        assert_eq!(
            stats.total_validations, 1,
            "green event counts toward stats"
        );
        assert_eq!(stats.total_hits, 1);
        assert_eq!(stats.total_changed, 2);
        assert_eq!(stats.emerging_samples, 1);
        assert_eq!(stats.emerging_avg_pct, Some(50.0));
        assert_eq!(
            stats.severity_counts.get("watch_success").copied(),
            Some(1),
            "green events are tallied under the watch_success severity"
        );
    }

    // ---- CI harvest (Day 148): turning failed CI runs into failure-day events ----

    #[test]
    fn test_ci_event_exists_for_ignores_green_event_with_same_snapshot_hash() {
        // Regression: green events dedup on `snapshot_git_hash`. If harvest
        // reused `green_event_exists_for`, an already-green-graded snapshot
        // would silently swallow the red event. The red dedup key is the run id.
        let green = concat!(
            r#"{"ts":"2026-07-25T00:00:00Z","day":147,"trigger":"cli","hits":[],"surprises":["src/a.rs"],"accuracy_pct":0.0,"severity":"watch_success","snapshot_git_hash":"deadbee"}"#,
            "\n"
        );
        assert!(
            green_event_exists_for(green, "deadbee"),
            "sanity: the green dedup key does match"
        );
        assert!(
            !ci_event_exists_for(green, 30051449447),
            "a green event must never mask a CI failure event"
        );
    }

    #[test]
    fn test_ci_failure_event_round_trip_counts_as_failure_day() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("risk_validations.jsonl");

        write_validation_event(
            &path,
            148,
            "ci_harvest",
            &["src/commands_risk.rs".to_string()],
            &["src/repl.rs".to_string()],
            50.0,
            Some(25.0),
            Some("ci_failure"),
            None,
            Some(30051449447),
        )
        .expect("write ci_failure event");

        let content = std::fs::read_to_string(&path).expect("read back");
        assert!(
            content.contains("\"ci_run_id\":30051449447"),
            "the run id is persisted so re-harvesting is idempotent: {content}"
        );
        assert!(ci_event_exists_for(&content, 30051449447));

        let events = parse_validation_events(&content);
        assert_eq!(events.len(), 1);
        assert!(
            !crate::commands_risk_accuracy::is_green_event(&events[0]),
            "ci_failure must grade on the RECALL side of the Day-142 polarity split"
        );

        let stats = crate::commands_risk_accuracy::compute_accuracy_stats(&events);
        assert_eq!(stats.failure_samples, 1);
        assert!(
            stats.failure_hit_rate_pct.is_some(),
            "harvested CI failures are exactly the failure-day evidence the meter was starving for"
        );
    }

    #[test]
    fn test_green_dedup_ignores_red_event_with_same_snapshot_hash() {
        // #723: failure-day events now carry `snapshot_git_hash` too (for
        // auditability), so green dedup must key on the SEVERITY marker — else a
        // red event would silently suppress the same snapshot's green grade.
        let red = concat!(
            r#"{"ts":"2026-08-12T00:00:00Z","day":165,"trigger":"ci_harvest","hits":[],"surprises":["src/a.rs"],"accuracy_pct":0.0,"severity":"ci_failure","snapshot_git_hash":"deadbee"}"#,
            "\n",
            r#"{"ts":"2026-08-12T01:00:00Z","day":165,"trigger":"cli","hits":[],"surprises":["src/b.rs"],"accuracy_pct":0.0,"snapshot_git_hash":"deadbee"}"#,
            "\n"
        );
        assert!(
            !green_event_exists_for(red, "deadbee"),
            "red/untagged events carrying the hash must not count as a green grade"
        );
        let green = format!(
            "{red}{}\n",
            r#"{"severity":"watch_success","snapshot_git_hash":"deadbee"}"#
        );
        assert!(
            green_event_exists_for(&green, "deadbee"),
            "a real green event still dedups"
        );
    }

    #[test]
    fn test_failure_event_records_graded_snapshot_hash() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl");
        let val_path = dir.path().join("validations.jsonl");
        let snapshot = serde_json::json!({
            "ts": "2026-08-12T00:00:00Z", "day": 165, "git_hash": "feedfac",
            "top_10": [{"path": "src/main.rs", "score": 0.9, "signals": ["churn"]}],
        });
        std::fs::write(&snap_path, serde_json::to_string(&snapshot).unwrap()).expect("write snap");
        let changed = ["src/main.rs".to_string()];
        auto_validate_after_failure_to(&changed, "watch_failure", &snap_path, &val_path);

        let contents = std::fs::read_to_string(&val_path).expect("read validation file");
        assert!(
            contents.contains(r#""snapshot_git_hash":"feedfac""#),
            "#723: a failure-day event names the snapshot it graded: {contents}"
        );
        // ...and does not thereby masquerade as a green grade.
        assert!(!green_event_exists_for(&contents, "feedfac"));
    }
}
