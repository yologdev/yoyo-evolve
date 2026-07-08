//! Snapshot/validation persistence for the `/risk` subsystem.
//!
//! Extracted from `commands_risk.rs` (Day 126) — the append-only JSONL
//! persistence layer: risk snapshots (`.yoyo/risk_snapshots.jsonl`) and
//! prediction-validation events (`.yoyo/risk_validations.jsonl`).
//! Scoring, prediction, and reporting stay in `commands_risk.rs`, which
//! re-exports everything here so call sites are unchanged.

use crate::commands_risk::{compute_file_risk_scores, learn_weights_from_history, FileRisk};
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
pub(crate) fn build_risk_snapshot_json(risks: &[FileRisk], day: u32, git_hash: &str) -> String {
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

    let snapshot = serde_json::json!({
        "ts": ts,
        "day": day,
        "git_hash": git_hash,
        "top_10": top_10,
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

    let day: u32 = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let json_line = build_risk_snapshot_json(&risks, day, &git_hash);
    if let Err(e) = write_risk_snapshot_to(std::path::Path::new(RISK_SNAPSHOT_PATH), &json_line) {
        eprintln!("  {DIM}(risk snapshot skipped: {e}){RESET}");
    }
}

/// Variant of `auto_risk_snapshot` that writes to a specific path (for testing).
#[cfg(test)]
fn auto_risk_snapshot_to(path: &std::path::Path) {
    let risks = compute_file_risk_scores();

    let git_hash = crate::git::run_git(&["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    let day: u32 = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let json_line = build_risk_snapshot_json(&risks, day, &git_hash);
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
/// `predicted_count` (always 10), and `accuracy_pct` — exactly the fields the
/// accuracy readers (`parse_validation_events`, `parse_rich_validation_events`)
/// consume.
pub(crate) fn write_validation_event(
    validation_path: &std::path::Path,
    day: u32,
    trigger: &str,
    hits: &[String],
    surprises: &[String],
    accuracy_pct: f64,
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

    let event = serde_json::json!({
        "ts": ts,
        "day": day,
        "trigger": trigger,
        "hits": hits,
        "surprises": surprises,
        "predicted_count": 10,
        "accuracy_pct": accuracy_pct,
    });

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

/// Automatically validate risk predictions against files that were changed
/// in the current session. Called after watch failures (or successes) to
/// close the prediction-validation loop.
///
/// - No-op if no snapshots exist.
/// - No-op if no `changed_files` match `src/` paths.
/// - Appends a validation event to `.yoyo/risk_validations.jsonl`.
/// - Prints a brief 2-3 line stderr summary when there are results.
pub(crate) fn auto_validate_after_failure(changed_files: &[String]) {
    auto_validate_after_failure_to(
        changed_files,
        std::path::Path::new(RISK_SNAPSHOT_PATH),
        std::path::Path::new(RISK_VALIDATION_PATH),
    );
}

/// Inner implementation with configurable paths (for testing).
fn auto_validate_after_failure_to(
    changed_files: &[String],
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

/// A single parsed validation event from `.yoyo/risk_validations.jsonl`.
pub(crate) struct ValidationEvent {
    pub(crate) day: u32,
    pub(crate) hit_count: usize,
    pub(crate) total_changed: usize,
    pub(crate) accuracy_pct: f64,
}

/// Load validation history from a JSONL file.
pub(crate) fn load_validation_history_from(path: &std::path::Path) -> Vec<ValidationEvent> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_validation_events(&content)
}

/// Parse validation events from JSONL content (testable without filesystem).
pub(crate) fn parse_validation_events(content: &str) -> Vec<ValidationEvent> {
    let mut events = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let day = val["day"].as_u64().unwrap_or(0) as u32;
        let hits = val["hits"].as_array().map(|a| a.len()).unwrap_or(0);
        let surprises = val["surprises"].as_array().map(|a| a.len()).unwrap_or(0);
        let total_changed = hits + surprises;
        let accuracy_pct = val["accuracy_pct"].as_f64().unwrap_or(0.0);

        events.push(ValidationEvent {
            day,
            hit_count: hits,
            total_changed,
            accuracy_pct,
        });
    }
    events
}

/// A parsed risk snapshot from the JSONL file.
pub(crate) struct ParsedSnapshot {
    pub(crate) day: u64,
    pub(crate) git_hash: String,
    pub(crate) predicted: Vec<String>,
}

/// Parse all snapshots from JSONL content.
pub(crate) fn parse_all_snapshots(content: &str) -> Vec<ParsedSnapshot> {
    let mut snapshots = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // skip malformed lines
        };
        let day = val["day"].as_u64().unwrap_or(0);
        let git_hash = val["git_hash"].as_str().unwrap_or("unknown").to_string();
        let predicted: Vec<String> = val["top_10"]
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
                predicted,
            });
        }
    }
    snapshots
}

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

        let json = build_risk_snapshot_json(&risks, 112, "abc123f");
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

        let json = build_risk_snapshot_json(&risks, 42, "deadbee");
        write_risk_snapshot_to(&path, &json).expect("write ok");

        // Write a second snapshot
        let json2 = build_risk_snapshot_json(&risks, 43, "cafebab");
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

        let json = build_risk_snapshot_json(&risks, 1, "1234567");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let top = parsed["top_10"].as_array().expect("top_10 array");
        assert_eq!(top.len(), 10);
        // First entry should be highest score
        assert_eq!(top[0]["path"], "src/file_0.rs");
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
        let json_line = build_risk_snapshot_json(&risks, 130, "feed123");
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

        auto_validate_after_failure_to(&changed, &snap_path, &val_path);

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
        auto_validate_after_failure_to(&changed, &snap_path, &val_path);

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
        auto_validate_after_failure_to(&changed, &snap_path, &val_path);

        // Validation file should not be created
        assert!(
            !val_path.exists(),
            "validation file should not exist when no src/ files changed"
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
    fn test_write_validation_event_cli_trigger_roundtrip() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nested").join("validations.jsonl");

        let hits = vec!["src/main.rs".to_string(), "src/cli.rs".to_string()];
        let surprises = vec!["src/prompt.rs".to_string()];
        write_validation_event(&path, 129, "cli", &hits, &surprises, 66.7)
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
        write_validation_event(&path, 100, "watch_failure", &hits, &surprises, 100.0)
            .expect("write validation event");

        let contents = std::fs::read_to_string(&path).expect("read validation file");
        let raw: serde_json::Value =
            serde_json::from_str(contents.lines().next().unwrap()).expect("valid JSON");
        assert_eq!(raw["trigger"], "watch_failure");

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
        write_validation_event(&path, 1, "cli", &hits, &surprises, 100.0).expect("first write");
        write_validation_event(&path, 2, "cli", &hits, &surprises, 100.0).expect("second write");

        let events = load_validation_history_from(&path);
        assert_eq!(events.len(), 2, "appending twice yields two lines");
        assert_eq!(events[0].day, 1);
        assert_eq!(events[1].day, 2);
    }
}
