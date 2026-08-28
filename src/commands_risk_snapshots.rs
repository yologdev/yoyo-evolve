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

/// Default path for the first-scored ledger (see [`append_first_scored_to`]).
pub(crate) const RISK_FIRST_SCORED_PATH: &str = ".yoyo/risk_first_scored.jsonl";

/// Filename of the first-scored ledger, resolved as a **sibling** of whatever
/// snapshot path a caller passes so a tempdir test writes into its tempdir.
const RISK_FIRST_SCORED_FILE: &str = "risk_first_scored.jsonl";

/// Current UTC instant in the `YYYY-MM-DDTHH:MM:SSZ` shape both JSONL ledgers
/// use. One statement of the format — the snapshot line and the first-scored
/// line are compared against each other by `forecast_opportunities`, so two
/// producers with two shapes would be two chances to drift.
fn utc_timestamp() -> String {
    std::process::Command::new("date")
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
        .unwrap_or_else(|| "unknown".to_string())
}

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
    let ts = utc_timestamp();
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
///
/// This is the **single** place a snapshot line reaches disk — both production
/// callers (`auto_risk_snapshot`, reached from `/commit` and the opt-in
/// `YOYO_RISK_AUTOSNAPSHOT=1` REPL-exit path, and `handle_risk_snapshot` behind
/// `yoyo risk snapshot`) go through it — so the first-scored ledger is appended
/// here rather than at each entry point. A per-token pass is not a per-entry-
/// point pass; wiring one caller would have been a receipt for the working half.
///
/// `scored_paths` is the **whole scored universe**, not `top_10`: the point of
/// the ledger is precisely the paths no prediction column ever names.
///
/// The ledger append is best-effort and runs *after* the snapshot line lands —
/// a failure there must never cost a snapshot, so its error is dropped.
pub(crate) fn write_risk_snapshot_to(
    path: &std::path::Path,
    json_line: &str,
    scored_paths: &[&str],
    day: u32,
    git_hash: &str,
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

    let _ = append_first_scored_to(
        &first_scored_ledger_path(path),
        scored_paths,
        &utc_timestamp(),
        day,
        git_hash,
    );
    Ok(())
}

/// The first-scored ledger that belongs beside a given snapshot path.
///
/// Derived from the snapshot path rather than taken as a parameter so every
/// caller — including tempdir tests — inherits a ledger in the same directory
/// as the snapshots it accompanies, with no second path to keep in sync.
fn first_scored_ledger_path(snapshot_path: &std::path::Path) -> std::path::PathBuf {
    match snapshot_path.parent() {
        Some(dir) => dir.join(RISK_FIRST_SCORED_FILE),
        None => std::path::PathBuf::from(RISK_FIRST_SCORED_FILE),
    }
}

/// Append one line per path that this ledger has **never seen before**.
///
/// Shape, one line per path, once ever:
/// `{"path":"src/foo.rs","ts":"...","day":175,"git_hash":"abc1234"}`
///
/// Why a separate ledger rather than embedding the scored universe in every
/// snapshot: the universe is ~200 paths and would add ~5KB to each of ~8
/// snapshots a day, to a file already past 300KB — while the fact being
/// recorded ("when did I *first* score this path") changes at most once per
/// path, ever.
///
/// Existing lines are never rewritten or back-filled. A path already present
/// is skipped, so re-running this is a no-op — the recorded instant is the
/// *first* observation, which is the only thing that carries information.
///
/// Returns the number of lines appended.
fn append_first_scored_to(
    ledger_path: &std::path::Path,
    scored_paths: &[&str],
    ts: &str,
    day: u32,
    git_hash: &str,
) -> Result<usize, std::io::Error> {
    let content = std::fs::read_to_string(ledger_path).unwrap_or_default();
    let (known, _dropped) = parse_first_scored(&content);

    let mut fresh: Vec<&str> = scored_paths
        .iter()
        .copied()
        .filter(|p| !p.trim().is_empty() && !known.contains_key(*p))
        .collect();
    fresh.sort_unstable();
    fresh.dedup();
    if fresh.is_empty() {
        return Ok(0);
    }

    if let Some(parent) = ledger_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path)?;
    for path in &fresh {
        let line = serde_json::json!({
            "path": path,
            "ts": ts,
            "day": day,
            "git_hash": git_hash,
        });
        writeln!(file, "{line}")?;
    }
    Ok(fresh.len())
}

/// Parse the first-scored ledger into `path -> first-seen ts`.
///
/// Defensive, and the drop count is **returned, not swallowed**: a shrinking
/// denominator inside my own meter is the defect I keep finding elsewhere
/// (#764). Non-blank lines that fail to parse, or that carry no usable
/// `path`/`ts` pair, are counted. When a path appears twice (it should not —
/// the writer skips known paths), the **earliest** ts wins, since the whole
/// value of the record is that it is the first observation.
pub(crate) fn parse_first_scored(
    content: &str,
) -> (std::collections::BTreeMap<String, String>, usize) {
    let mut map: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut dropped = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            dropped += 1;
            continue;
        };
        let (Some(path), Some(ts)) = (
            v.get("path").and_then(|p| p.as_str()),
            v.get("ts").and_then(|t| t.as_str()),
        ) else {
            dropped += 1;
            continue;
        };
        if path.trim().is_empty() || ts.trim().is_empty() {
            dropped += 1;
            continue;
        }
        map.entry(path.to_string())
            .and_modify(|existing| {
                if ts < existing.as_str() {
                    *existing = ts.to_string();
                }
            })
            .or_insert_with(|| ts.to_string());
    }
    (map, dropped)
}

/// The earliest `ts` in the ledger — the "founding batch" instant.
///
/// `None` for an empty ledger. Comparison is lexicographic, which is exactly
/// right here: every line in this file is written by [`utc_timestamp`] in one
/// fixed `...Z` shape, unlike the git-sourced dates `iso8601_sort_key` exists
/// to normalise.
pub(crate) fn founding_ts(map: &std::collections::BTreeMap<String, String>) -> Option<&str> {
    map.values().map(|s| s.as_str()).min()
}

/// When did I *first* observe this path in a scored risk universe?
///
/// This is the age signal that survives a shallow clone: it is a fact about my
/// own observation history, not about git history, so a graft boundary cannot
/// erase it. Three states, none folded into a convenient neighbour:
///
/// * path absent from the ledger → `None`. **Unknown age, not newborn.**
/// * path recorded at the ledger's earliest ts → `None`. The load-bearing
///   rule: the very first write stamps every currently-scored path at the same
///   instant, so "present in the founding batch" means *at least as old as the
///   record* — an unknown, not a birthday. Reading it as a birthday would
///   relabel every long-standing dark room "too young to judge" and empty the
///   dark set, which is the exact opposite of the point.
/// * path first seen strictly after the founding batch → `Some(ts)`, a real
///   observed birthday.
pub(crate) fn first_scored_age<'a>(
    path: &str,
    map: &'a std::collections::BTreeMap<String, String>,
    founding: Option<&str>,
) -> Option<&'a str> {
    let ts = map.get(path)?.as_str();
    match founding {
        Some(f) if ts == f => None,
        _ => Some(ts),
    }
}

/// I/O half: read the first-scored ledger. A missing file is an honest empty
/// map (this ledger is forward-only — before its first write there is simply
/// nothing to know), never an error.
pub(crate) fn read_first_scored(
    path: &std::path::Path,
) -> (std::collections::BTreeMap<String, String>, usize) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    parse_first_scored(&content)
}

/// Every `git_hash` recorded **anywhere** in the given snapshot JSONL content.
///
/// Pure; no I/O. The decision half of the idempotency guard, reading the whole
/// **set** rather than the tail on purpose (#846): the previous
/// `last_snapshot_git_hash` asked "is HEAD the hash of the *last* line?", which
/// is strictly narrower than the intent stated on `auto_risk_snapshot` ("one
/// snapshot per distinct commit-state"), so a HEAD returning to a commit
/// already in the ledger but no longer at its tail sailed past it. That is the
/// routine outcome of a reverted task — `scripts/evolve.sh` reverts with
/// `git reset --hard PRE_TASK_SHA`. Measured live when this landed: 303
/// snapshots, 300 distinct hashes, 3 duplicated.
///
/// The ledger is append-only history and may hold anything, so a line that
/// fails to parse — or carries no `git_hash` — contributes nothing and must
/// never panic. Owned `String`s because `serde_json::Value` owns its strings.
/// Deliberately **not** a bounded "last N lines" scan: that reintroduces this
/// exact bug class one parameter over, and the file is already in memory.
fn snapshot_hashes(content: &str) -> std::collections::HashSet<String> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            v.get("git_hash")?.as_str().map(str::to_string)
        })
        .collect()
}

/// What the dedup-guarded snapshot append actually did.
#[derive(Debug, PartialEq, Eq)]
enum SnapshotWrite {
    Recorded,
    SkippedDuplicate,
}

/// Append a snapshot for `git_hash` unless that hash is already somewhere in
/// the ledger at `path`. The **single** statement of the idempotency rule.
///
/// `build` is called only when the guard lets the write through, preserving the
/// pre-#846 property that a deduped snapshot never pays for
/// `compute_file_risk_scores` (which shells out to git repeatedly). Injecting it
/// as a closure — the discipline `never_forecast_files` uses for `added_ts` —
/// keeps guard and write in one path-taking function whose emission point is the
/// file's own bytes, instead of a decision helper whose caller would have to
/// re-state the rule to be testable.
///
/// `"unknown"` is exempt and must stay exempt: `run_git` falls back to that
/// literal when it cannot resolve HEAD, and two unknown states may be different
/// commits, so deduping `"unknown"` against `"unknown"` would be *more* wrong
/// than the tail read ever was.
fn write_snapshot_unless_recorded<F>(
    path: &std::path::Path,
    git_hash: &str,
    build: F,
) -> Result<SnapshotWrite, std::io::Error>
where
    F: FnOnce() -> (String, Vec<String>, u32),
{
    if git_hash != "unknown" {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        if snapshot_hashes(&content).contains(git_hash) {
            return Ok(SnapshotWrite::SkippedDuplicate);
        }
    }

    let (json_line, scored, day) = build();
    let scored_refs: Vec<&str> = scored.iter().map(String::as_str).collect();
    write_risk_snapshot_to(path, &json_line, &scored_refs, day, git_hash)?;
    Ok(SnapshotWrite::Recorded)
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

    // Dedup by git hash: skip if *any* snapshot already recorded this HEAD
    // (#846 — the guard used to compare only against the ledger's last line,
    // so a `git reset --hard` back to an earlier recorded commit re-snapshotted
    // it). Never dedup on "unknown" — two unknown states may genuinely differ.
    match write_snapshot_unless_recorded(
        std::path::Path::new(RISK_SNAPSHOT_PATH),
        &git_hash,
        || {
            let risks = compute_file_risk_scores();
            let emerging = detect_emerging_risks(&risks);

            let day: u32 = std::fs::read_to_string("DAY_COUNT")
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);

            let json_line = build_risk_snapshot_json(&risks, &emerging, day, &git_hash);
            let scored: Vec<String> = risks.iter().map(|r| r.path.clone()).collect();
            (json_line, scored, day)
        },
    ) {
        Ok(SnapshotWrite::Recorded) => {}
        Ok(SnapshotWrite::SkippedDuplicate) => {
            eprintln!("  {DIM}(risk snapshot skipped: already recorded for {git_hash}){RESET}");
        }
        Err(e) => {
            eprintln!("  {DIM}(risk snapshot skipped: {e}){RESET}");
        }
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
    let scored: Vec<&str> = risks.iter().map(|r| r.path.as_str()).collect();
    write_risk_snapshot_to(path, &json_line, &scored, day, &git_hash)
        .expect("test snapshot write should succeed");
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
    let ts = utc_timestamp();

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
    ci_event_exists_for, ci_payload_note, epistemic_ledger_notes, load_validation_history_from,
    parse_all_snapshots, parse_ci_run_payload, parse_failed_ci_runs, parse_validation_events,
    read_graded_ledger, read_snapshot_ledger, read_validation_ledger, snapshot_before, GradedEvent,
    GradedLedger, ParsedSnapshot, SnapshotLedger, ValidationEvent, ValidationLedger,
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

    /// The decision half, pure. The ledger is append-only history and may hold
    /// anything, so unreadable lines contribute nothing and must not panic.
    #[test]
    fn snapshot_hashes_reads_every_readable_line_and_drops_the_rest() {
        let jsonl = concat!(
            "{\"day\":100,\"git_hash\":\"aaa111\",\"top_10\":[]}\n\n",
            "garbage not json\n{\"day\":101,\"top_10\":[]}\n   \n",
            "{\"day\":102,\"git_hash\":\"bbb222\",\"top_10\":[]}\n"
        );
        let hashes = snapshot_hashes(jsonl);
        assert_eq!(hashes.len(), 2, "only the two lines carrying a git_hash");
        assert!(hashes.contains("aaa111") && hashes.contains("bbb222"));
        assert!(snapshot_hashes("").is_empty(), "empty content → empty set");
    }

    fn snapshot_line(day: u32, git_hash: &str) -> String {
        format!("{{\"day\":{day},\"git_hash\":\"{git_hash}\",\"top_10\":[],\"emerging\":[]}}")
    }

    /// Seed a tempdir ledger with one line per hash, in order.
    fn seed_ledger(dir: &tempfile::TempDir, hashes: &[&str]) -> std::path::PathBuf {
        let path = dir.path().join("risk_snapshots.jsonl");
        let body: String = hashes
            .iter()
            .enumerate()
            .map(|(i, h)| format!("{}\n", snapshot_line(100 + i as u32, h)))
            .collect();
        std::fs::write(&path, body).expect("seed ledger");
        path
    }

    /// Drive the guard at its emission point — the ledger's own bytes — and
    /// report how many times the (expensive) build closure was invoked.
    fn attempt(path: &std::path::Path, git_hash: &str) -> (SnapshotWrite, usize, String) {
        let calls = std::cell::Cell::new(0usize);
        let outcome = write_snapshot_unless_recorded(path, git_hash, || {
            calls.set(calls.get() + 1);
            (
                snapshot_line(200, git_hash),
                vec!["src/x.rs".to_string()],
                200,
            )
        })
        .expect("tempdir write should succeed");
        let content = std::fs::read_to_string(path).unwrap_or_default();
        (outcome, calls.get(), content)
    }

    /// #846, the regression guard: HEAD returns to a hash the ledger already
    /// holds but which is no longer at its tail — the routine outcome of a
    /// `git reset --hard PRE_TASK_SHA` revert. Fails against the old tail read.
    #[test]
    fn a_hash_recorded_earlier_in_the_ledger_is_not_re_snapshotted() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = seed_ledger(&dir, &["aaa111", "bbb222"]);
        let before = std::fs::read_to_string(&path).expect("read");

        let (outcome, build_calls, after) = attempt(&path, "aaa111");

        assert_eq!(outcome, SnapshotWrite::SkippedDuplicate);
        assert_eq!(
            after, before,
            "nothing appended for an already-recorded hash"
        );
        assert_eq!(
            build_calls, 0,
            "a skipped snapshot must not pay for scoring"
        );
    }

    /// The near-miss guard: a dedup that blocks everything would silently
    /// freeze the meter — worse than the duplicate it fixes.
    #[test]
    fn a_genuinely_new_hash_is_still_recorded() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = seed_ledger(&dir, &["aaa111", "bbb222"]);

        let (outcome, build_calls, after) = attempt(&path, "ccc333");

        assert_eq!(outcome, SnapshotWrite::Recorded);
        assert_eq!(build_calls, 1);
        assert!(after.contains("ccc333"), "new hash appended: {after}");
        assert_eq!(after.lines().count(), 3, "exactly one line added");
    }

    /// Pre-existing behaviour, unchanged — the case the old tail read got right.
    #[test]
    fn an_immediate_repeat_of_the_tail_hash_still_skips() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = seed_ledger(&dir, &["bbb222"]);
        let before = std::fs::read_to_string(&path).expect("read");

        let (outcome, _, after) = attempt(&path, "bbb222");

        assert_eq!(outcome, SnapshotWrite::SkippedDuplicate);
        assert_eq!(after, before, "tail-hash repeat still skipped");
    }

    /// The exemption, and the one case where set membership would be *more*
    /// wrong than the tail read: two "unknown" states may be different commits.
    #[test]
    fn unknown_never_dedups_against_unknown() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = seed_ledger(&dir, &["unknown"]);

        let (outcome, _, after) = attempt(&path, "unknown");

        assert_eq!(outcome, SnapshotWrite::Recorded);
        assert_eq!(after.lines().count(), 2, "both unknowns recorded: {after}");
    }

    /// A missing ledger is an honest empty set, not an error.
    #[test]
    fn a_missing_ledger_records_rather_than_skipping() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("risk_snapshots.jsonl");
        assert!(!path.exists());

        let (outcome, _, after) = attempt(&path, "aaa111");

        assert_eq!(outcome, SnapshotWrite::Recorded);
        assert!(after.contains("aaa111"));
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
        write_risk_snapshot_to(&path, &json, &[], 0, "abc1234").expect("write ok");

        // Write a second snapshot
        let json2 = build_risk_snapshot_json(&risks, &[], 43, "cafebab");
        write_risk_snapshot_to(&path, &json2, &[], 0, "abc1234").expect("write ok");

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
        write_risk_snapshot_to(&path, &json_line, &[], 0, "abc1234")
            .expect("feed write must succeed");

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

/// Tests for the first-scored ledger — the age signal that survives a shallow
/// clone. Kept in their own module so the discriminator (founding batch vs. a
/// genuinely later first sighting) is covered in **both** directions: a rule
/// tested only where it blocks is vacuous green.
#[cfg(test)]
mod first_scored_tests {
    use super::*;
    use crate::commands_risk_neverforecast::forecast_opportunities;

    fn ledger(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join(RISK_FIRST_SCORED_FILE)
    }

    #[test]
    fn founding_batch_paths_have_unknown_age_not_a_birthday() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = ledger(&dir);
        let n = append_first_scored_to(
            &path,
            &["src/a.rs", "src/b.rs", "src/c.rs"],
            "2026-08-22T15:40:00Z",
            175,
            "abc1234",
        )
        .expect("append");
        assert_eq!(n, 3, "three fresh paths appended");

        let (map, dropped) = read_first_scored(&path);
        assert_eq!(dropped, 0, "well-formed ledger drops nothing");
        assert_eq!(map.len(), 3);
        let founding = founding_ts(&map);
        assert_eq!(founding, Some("2026-08-22T15:40:00Z"));

        // The load-bearing rule: everything stamped in the founding batch is
        // "at least as old as the record", i.e. unknown — never young.
        for p in ["src/a.rs", "src/b.rs", "src/c.rs"] {
            assert_eq!(
                first_scored_age(p, &map, founding),
                None,
                "{p} is in the founding batch, so its age is unknown, not new"
            );
        }
        // ...and so it does not reach the too-young branch at all.
        assert_eq!(
            forecast_opportunities(
                first_scored_age("src/a.rs", &map, founding),
                &["2026-08-22T16:00:00Z".to_string()]
            ),
            None,
            "unknown age must stay unknown all the way through the consumer"
        );
    }

    #[test]
    fn a_path_first_seen_after_the_founding_batch_has_a_real_birthday() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = ledger(&dir);
        append_first_scored_to(
            &path,
            &["src/old.rs"],
            "2026-08-22T15:40:00Z",
            175,
            "abc1234",
        )
        .expect("founding");
        let n = append_first_scored_to(
            &path,
            &["src/old.rs", "src/new.rs"],
            "2026-08-24T09:00:00Z",
            177,
            "def5678",
        )
        .expect("second batch");
        assert_eq!(n, 1, "only the genuinely new path is appended");

        let (map, _) = read_first_scored(&path);
        let founding = founding_ts(&map);
        // Near-miss guard: the founding path must NOT be called young.
        assert_eq!(first_scored_age("src/old.rs", &map, founding), None);
        // The discriminating side.
        assert_eq!(
            first_scored_age("src/new.rs", &map, founding),
            Some("2026-08-24T09:00:00Z")
        );
        // And it reaches the consumer as a real, countable age.
        assert_eq!(
            forecast_opportunities(
                first_scored_age("src/new.rs", &map, founding),
                &[
                    "2026-08-23T09:00:00Z".to_string(), // before  → not counted
                    "2026-08-25T09:00:00Z".to_string(), // after   → counted
                    "2026-08-26T09:00:00Z".to_string(), // after   → counted
                ]
            ),
            Some(2)
        );
    }

    #[test]
    fn a_path_absent_from_the_ledger_has_unknown_age_not_zero() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = ledger(&dir);
        append_first_scored_to(&path, &["src/a.rs"], "2026-08-22T15:40:00Z", 175, "abc1234")
            .expect("append");
        let (map, _) = read_first_scored(&path);
        let founding = founding_ts(&map);
        assert_eq!(
            first_scored_age("src/never_seen.rs", &map, founding),
            None,
            "absence is unknown age — never a birthday, never a zero"
        );
    }

    #[test]
    fn a_missing_ledger_is_an_honest_empty_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (map, dropped) = read_first_scored(&dir.path().join("nope.jsonl"));
        assert!(map.is_empty());
        assert_eq!(dropped, 0);
        assert_eq!(founding_ts(&map), None);
        assert_eq!(first_scored_age("src/a.rs", &map, None), None);
    }

    #[test]
    fn unparseable_lines_are_counted_not_silently_dropped() {
        let content = concat!(
            "{\"path\":\"src/a.rs\",\"ts\":\"2026-08-22T15:40:00Z\",\"day\":175}\n",
            "\n",
            "not json at all\n",
            "{\"ts\":\"2026-08-22T15:40:00Z\"}\n",
            "{\"path\":\"\",\"ts\":\"2026-08-22T15:40:00Z\"}\n",
            "{\"path\":\"src/b.rs\",\"ts\":\"2026-08-22T15:40:00Z\"}\n",
        );
        let (map, dropped) = parse_first_scored(content);
        assert_eq!(map.len(), 2, "the two well-formed lines survive");
        assert_eq!(
            dropped, 3,
            "unparseable, path-less and empty-path lines are counted; a blank line is not"
        );
    }

    #[test]
    fn a_duplicate_path_keeps_the_earliest_ts() {
        let content = concat!(
            "{\"path\":\"src/a.rs\",\"ts\":\"2026-08-24T09:00:00Z\"}\n",
            "{\"path\":\"src/a.rs\",\"ts\":\"2026-08-22T15:40:00Z\"}\n",
        );
        let (map, dropped) = parse_first_scored(content);
        assert_eq!(dropped, 0);
        assert_eq!(
            map.get("src/a.rs").map(|s| s.as_str()),
            Some("2026-08-22T15:40:00Z")
        );
    }

    #[test]
    fn writing_a_snapshot_also_lands_the_first_scored_ledger_beside_it() {
        // The wiring test: every entry point inherits the ledger because the
        // append lives inside the one function that writes the snapshot line.
        let dir = tempfile::tempdir().expect("tempdir");
        let snap = dir.path().join("risk_snapshots.jsonl");
        write_risk_snapshot_to(
            &snap,
            "{\"ts\":\"x\"}",
            &["src/a.rs", "src/b.rs"],
            175,
            "abc1234",
        )
        .expect("snapshot write");

        let (map, dropped) = read_first_scored(&ledger(&dir));
        assert_eq!(dropped, 0);
        assert_eq!(map.len(), 2, "the whole scored universe is recorded once");
        assert!(map.contains_key("src/a.rs") && map.contains_key("src/b.rs"));

        // Second snapshot, same universe → no new lines (never back-filled,
        // never rewritten).
        write_risk_snapshot_to(
            &snap,
            "{\"ts\":\"y\"}",
            &["src/a.rs", "src/b.rs"],
            175,
            "abc1234",
        )
        .expect("snapshot write");
        let content = std::fs::read_to_string(ledger(&dir)).expect("read");
        assert_eq!(
            content.lines().filter(|l| !l.trim().is_empty()).count(),
            2,
            "a path is recorded once, ever"
        );
    }

    #[test]
    fn an_empty_scored_universe_writes_no_ledger_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let snap = dir.path().join("risk_snapshots.jsonl");
        write_risk_snapshot_to(&snap, "{\"ts\":\"x\"}", &[], 175, "abc1234").expect("write");
        assert!(snap.exists(), "the snapshot still lands");
        assert!(
            !ledger(&dir).exists(),
            "nothing to record means no file, not an empty one"
        );
    }
}
