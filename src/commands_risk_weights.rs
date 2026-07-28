//! Adaptive weight-learning + revert-history for the `/risk` risk scorer.
//!
//! Extracted from `commands_risk.rs` (Day 139) — the history-driven
//! weight-learning chunk (learning signal weights from prediction-validation
//! history) plus the empirical revert-history failure signal. Re-exported
//! via `commands_risk` so all call sites remain unchanged.

use crate::commands_risk_snapshots::{
    parse_validation_events, RISK_SNAPSHOT_PATH, RISK_VALIDATION_PATH,
};
use crate::format::*;

/// Risk signal weights: [churn, recency, size, complexity, test_density, coupling, revert_history].
///
/// These seven weights must sum to 1.0. The revert_history weight (0.10) captures
/// empirical failure data — files that have been reverted are more likely to cause
/// future regressions. The complexity proxy (0.10) estimates cyclomatic complexity
/// via branch/match density. This is the signal set that makes the risk scorer learn
/// from its own history rather than just measuring static properties.
pub(crate) const RISK_WEIGHTS: [f64; 7] = [0.30, 0.15, 0.15, 0.10, 0.10, 0.10, 0.10];

/// Human-readable names for the 7 risk signals (parallel to `RISK_WEIGHTS`).
pub(crate) const SIGNAL_NAMES: [&str; 7] = [
    "churn",
    "recency",
    "size",
    "complexity",
    "test_density",
    "coupling",
    "revert_history",
];

/// Map a signal label from snapshot data to its weight index.
fn label_to_index(label: &str) -> Option<usize> {
    match label {
        "▲churn" => Some(0),
        "▲recent" => Some(1),
        "▲size" => Some(2),
        "▲reverts" => Some(6),
        "▲low-test" => Some(4),
        "▲coupled" => Some(5),
        _ => None,
    }
}

/// Default path for learned risk weights.
pub(crate) const LEARNED_WEIGHTS_PATH: &str = ".yoyo/risk_weights.json";

/// Minimum number of validation events required before learning weights.
pub(crate) const MIN_VALIDATION_EVENTS: usize = 5;

/// Learning rate: how much the computed weights influence the result.
/// learned = (1 - LEARNING_RATE) * default + LEARNING_RATE * computed
const LEARNING_RATE: f64 = 0.3;

/// Tolerance under which a newly-learned weight is treated as unchanged from the
/// value already on disk. A planner-fallback session that re-learns weights can
/// produce sub-`WEIGHT_WRITE_EPSILON` floating-point drift in every weight; without
/// this gate that drift rewrites `risk_weights.json`, `git` sees a diff, and the
/// fallback manufactures a fake "1/1 ✅" commit that ships no real improvement.
const WEIGHT_WRITE_EPSILON: f64 = 1e-3;

/// Return `true` iff any weight in `new` differs from the corresponding weight in
/// `old` by more than `epsilon` — i.e. the learned weights are meaningfully
/// different from what is already on disk and are worth persisting.
///
/// This is the pure decision the write path consults so that a no-op re-learn
/// produces no file change (and therefore no `git` diff / noise commit). The
/// "nothing meaningfully changed" case is an explicit `false` return, never a
/// silent fall-through (abstention discipline, Day 144).
fn weights_changed_meaningfully(old: &[f64; 7], new: &[f64; 7], epsilon: f64) -> bool {
    old.iter()
        .zip(new.iter())
        .any(|(o, n)| (o - n).abs() > epsilon)
}

/// Load learned weights from `.yoyo/risk_weights.json`, falling back to `RISK_WEIGHTS`.
///
/// Validates that the file contains exactly 7 weights that sum to approximately 1.0
/// (within 0.05 tolerance). Returns `RISK_WEIGHTS` on any error.
pub(crate) fn load_learned_weights() -> [f64; 7] {
    load_learned_weights_from(std::path::Path::new(LEARNED_WEIGHTS_PATH))
}

/// Why a *present* `risk_weights.json` could not be used.
///
/// A missing file is deliberately NOT a variant here: "no weights learned yet"
/// is the normal first-run state and must stay silent. Every variant below means
/// the file exists and something about it is wrong — the case that used to be
/// indistinguishable from the normal one (Day 139: fail-soft without a liveness
/// signal is fail-silent). Learned weights silently reverting to `RISK_WEIGHTS`
/// would leave `/risk` printing confident scores from a dead model.
#[derive(Debug, PartialEq)]
pub(crate) enum WeightsDefect {
    /// File contents are not valid JSON.
    NotJson,
    /// No `weights` key, or it is not an array.
    NoWeightsArray,
    /// `weights` is an array, but not of length 7.
    WrongLength(usize),
    /// Element at this index is not a non-negative number.
    BadValue(usize),
    /// Weights parsed but do not sum to ~1.0 (within 0.05).
    SumOutOfRange(f64),
}

impl WeightsDefect {
    /// One-line human description, used in the stderr warning.
    pub(crate) fn describe(&self) -> String {
        match self {
            WeightsDefect::NotJson => "not valid JSON".to_string(),
            WeightsDefect::NoWeightsArray => "missing a `weights` array".to_string(),
            WeightsDefect::WrongLength(n) => {
                format!("`weights` has {n} entries, expected {}", SIGNAL_NAMES.len())
            }
            WeightsDefect::BadValue(i) => {
                let name = SIGNAL_NAMES.get(*i).copied().unwrap_or("?");
                format!("`weights[{i}]` ({name}) is not a non-negative number")
            }
            WeightsDefect::SumOutOfRange(sum) => {
                format!("weights sum to {sum:.4}, expected ~1.0")
            }
        }
    }
}

/// Pure parse+validate of `risk_weights.json` contents.
///
/// Returns the learned weights, or an explicit named defect. Keeping this pure
/// (no filesystem, no stderr) is what lets each failure mode be tested directly
/// instead of being observed only as "we got the defaults back".
pub(crate) fn parse_learned_weights(content: &str) -> Result<[f64; 7], WeightsDefect> {
    let val: serde_json::Value =
        serde_json::from_str(content).map_err(|_| WeightsDefect::NotJson)?;
    let weights_arr = val["weights"]
        .as_array()
        .ok_or(WeightsDefect::NoWeightsArray)?;
    if weights_arr.len() != 7 {
        return Err(WeightsDefect::WrongLength(weights_arr.len()));
    }
    let mut weights = [0.0f64; 7];
    for (i, v) in weights_arr.iter().enumerate() {
        match v.as_f64() {
            Some(w) if w >= 0.0 => weights[i] = w,
            _ => return Err(WeightsDefect::BadValue(i)),
        }
    }
    let sum: f64 = weights.iter().sum();
    if (sum - 1.0).abs() > 0.05 {
        return Err(WeightsDefect::SumOutOfRange(sum));
    }
    Ok(weights)
}

/// Warn at most once per process about an unusable weights file.
///
/// `load_learned_weights` is called per risk computation, so an unconditional
/// warning would spam every `/risk` run; once is enough to break the silence.
static WEIGHTS_DEFECT_WARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Inner implementation with configurable path (for testing).
fn load_learned_weights_from(path: &std::path::Path) -> [f64; 7] {
    let content = match std::fs::read_to_string(path) {
        // Absent (or unreadable) file: the normal "nothing learned yet" state.
        Err(_) => return RISK_WEIGHTS,
        Ok(c) => c,
    };
    match parse_learned_weights(&content) {
        Ok(w) => w,
        Err(defect) => {
            if !WEIGHTS_DEFECT_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "  {YELLOW}warning:{RESET} {} is unusable ({}) — falling back to built-in risk weights",
                    path.display(),
                    defect.describe()
                );
            }
            RISK_WEIGHTS
        }
    }
}

/// A parsed validation event with per-file signal detail for weight learning.
pub(crate) struct DetailedValidationEvent {
    /// File paths that were predicted (in top-10) and actually broke.
    pub(crate) hit_signals: Vec<Vec<usize>>,
    /// File paths that broke but weren't in the top-10 predictions.
    pub(crate) surprise_count: usize,
}

/// Parse validation events with signal detail by cross-referencing
/// validations against snapshots.
///
/// For each validation event, looks up the corresponding snapshot to find
/// which signals were elevated for hit files.
pub(crate) fn parse_detailed_events(
    validation_content: &str,
    snapshot_content: &str,
) -> Vec<DetailedValidationEvent> {
    // Build a map from snapshot day → signal data per file
    let mut snapshot_signals: std::collections::HashMap<
        u64,
        std::collections::HashMap<String, Vec<usize>>,
    > = std::collections::HashMap::new();

    for line in snapshot_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let day = val["day"].as_u64().unwrap_or(0);
        let mut file_signals: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        if let Some(top_10) = val["top_10"].as_array() {
            for entry in top_10 {
                if let Some(path) = entry["path"].as_str() {
                    let mut indices = Vec::new();
                    if let Some(signals) = entry["signals"].as_array() {
                        for sig in signals {
                            if let Some(label) = sig.as_str() {
                                if let Some(idx) = label_to_index(label) {
                                    indices.push(idx);
                                }
                            }
                        }
                    }
                    file_signals.insert(path.to_string(), indices);
                }
            }
        }
        // Use the latest snapshot for each day
        snapshot_signals.insert(day, file_signals);
    }

    let mut events = Vec::new();
    for line in validation_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let day = val["day"].as_u64().unwrap_or(0);
        let hits: Vec<String> = val["hits"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let surprise_count = val["surprises"].as_array().map(|a| a.len()).unwrap_or(0);

        // Look up signal data from the snapshot for the same day
        let file_signals = snapshot_signals.get(&day);

        let mut hit_signals = Vec::new();
        for hit_path in &hits {
            let signals = file_signals
                .and_then(|fs| fs.get(hit_path))
                .cloned()
                .unwrap_or_default();
            hit_signals.push(signals);
        }

        events.push(DetailedValidationEvent {
            hit_signals,
            surprise_count,
        });
    }
    events
}

/// Compute adjusted weights from validation history.
///
/// For each signal, computes a "hit contribution" score based on how often
/// it appears in correctly-predicted files. Signals that consistently appear
/// in hits get boosted; the result is blended with defaults using `LEARNING_RATE`.
fn compute_adjusted_weights(events: &[DetailedValidationEvent]) -> [f64; 7] {
    // Count how many times each signal appeared in hits
    let mut signal_hit_counts = [0u64; 7];
    let mut total_hits = 0u64;
    let mut total_surprises = 0u64;

    for event in events {
        for signals in &event.hit_signals {
            total_hits += 1;
            for &idx in signals {
                if idx < 7 {
                    signal_hit_counts[idx] += 1;
                }
            }
        }
        total_surprises += event.surprise_count as u64;
    }

    // If no hits at all, can't learn anything useful
    if total_hits == 0 {
        return RISK_WEIGHTS;
    }

    // Compute a raw effectiveness score for each signal:
    // effectiveness[i] = hit_rate[i] * (1 + surprise_penalty)
    //
    // hit_rate[i] = how often signal i appeared when we correctly predicted
    // surprise_penalty = proportion of surprises (signals that were missed)
    let surprise_ratio = if total_hits + total_surprises > 0 {
        total_surprises as f64 / (total_hits + total_surprises) as f64
    } else {
        0.0
    };

    let mut raw_weights = [0.0f64; 7];
    for i in 0..7 {
        // Base: how often this signal was present in hits
        let hit_rate = signal_hit_counts[i] as f64 / total_hits as f64;
        // Boost signals that are consistently present; penalize those absent
        // when there are many surprises
        raw_weights[i] = hit_rate + RISK_WEIGHTS[i] * (1.0 + surprise_ratio);
    }

    // Normalize raw_weights to sum to 1.0
    let raw_sum: f64 = raw_weights.iter().sum();
    if raw_sum <= 0.0 {
        return RISK_WEIGHTS;
    }
    for w in &mut raw_weights {
        *w /= raw_sum;
    }

    // Blend: learned = (1 - LEARNING_RATE) * default + LEARNING_RATE * computed
    let mut blended = [0.0f64; 7];
    for i in 0..7 {
        blended[i] = (1.0 - LEARNING_RATE) * RISK_WEIGHTS[i] + LEARNING_RATE * raw_weights[i];
    }

    // Final normalization to ensure sum = 1.0
    let blended_sum: f64 = blended.iter().sum();
    if blended_sum > 0.0 {
        for w in &mut blended {
            *w /= blended_sum;
        }
    }

    blended
}

/// Learn risk weights from prediction-validation history and save to disk.
///
/// Reads `.yoyo/risk_validations.jsonl` and `.yoyo/risk_snapshots.jsonl`,
/// computes per-signal effectiveness, and writes adjusted weights to
/// `.yoyo/risk_weights.json`. Requires at least `MIN_VALIDATION_EVENTS`
/// events before producing learned weights.
///
/// All I/O is best-effort — failures are silently ignored.
pub(crate) fn learn_weights_from_history() {
    learn_weights_from_history_to(
        std::path::Path::new(RISK_VALIDATION_PATH),
        std::path::Path::new(RISK_SNAPSHOT_PATH),
        std::path::Path::new(LEARNED_WEIGHTS_PATH),
    );
}

/// Inner implementation with configurable paths (for testing).
fn learn_weights_from_history_to(
    validation_path: &std::path::Path,
    snapshot_path: &std::path::Path,
    weights_path: &std::path::Path,
) {
    let validation_content = match std::fs::read_to_string(validation_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let snapshot_content = match std::fs::read_to_string(snapshot_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Count total validation events (using the simple parser)
    let event_count = parse_validation_events(&validation_content).len();
    if event_count < MIN_VALIDATION_EVENTS {
        return;
    }

    let detailed = parse_detailed_events(&validation_content, &snapshot_content);
    if detailed.is_empty() {
        return;
    }

    let weights = compute_adjusted_weights(&detailed);

    // Idempotency gate: if the freshly-learned weights are within
    // WEIGHT_WRITE_EPSILON of what's already on disk, skip the write entirely.
    // Rewriting sub-epsilon drift produces a `git` diff and a fake "success"
    // commit in planner-fallback sessions — an explicit no-op is the honest path.
    let existing = load_learned_weights_from(weights_path);
    if !weights_changed_meaningfully(&existing, &weights, WEIGHT_WRITE_EPSILON) {
        return;
    }

    // Build output JSON
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

    let signal_names: Vec<&str> = SIGNAL_NAMES.to_vec();
    let output = serde_json::json!({
        "weights": weights.to_vec(),
        "learned_from": event_count,
        "last_updated": ts,
        "signal_names": signal_names,
    });

    // Write atomically-ish: write to file directly (best-effort)
    if let Some(parent) = weights_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("  {DIM}(warning: could not create risk weights dir: {e}){RESET}");
            return;
        }
    }
    if let Ok(json_str) = serde_json::to_string_pretty(&output) {
        if let Err(e) = std::fs::write(weights_path, json_str) {
            eprintln!("  {DIM}(warning: could not save risk weights: {e}){RESET}");
        }
    }
}

/// Build a map of file paths → number of times that file appeared in a revert commit.
///
/// Searches git history for multiple revert patterns:
/// - "Revert" (git's default `git revert` prefix)
/// - "revert task" (evolve loop's task-level revert messages)
/// - "revert session" (evolve loop's session-level revert messages)
///
/// This is the empirical failure signal — files that have been reverted in the past
/// are more likely to cause future failures. Part of the dream milestone: predictive
/// self-understanding grounded in historical failure data.
pub(crate) fn revert_history() -> std::collections::HashMap<String, u32> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    // Search for multiple revert patterns used by both `git revert` and the evolve loop
    let patterns = ["Revert", "revert task", "revert session"];
    for pattern in &patterns {
        let output = match crate::git::run_git(&[
            "log",
            "--all",
            "--oneline",
            &format!("--grep={pattern}"),
            "--name-only",
            "--pretty=format:",
        ]) {
            Ok(o) => o,
            Err(_) => continue,
        };

        for line in output.lines() {
            let path = line.trim();
            if path.is_empty() {
                continue;
            }
            if path.starts_with("src/") && path.ends_with(".rs") {
                *counts.entry(path.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_weights_sum_to_one() {
        // The 7 risk signal weights must sum to 1.0 (within floating-point tolerance)
        let sum: f64 = RISK_WEIGHTS.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-10,
            "Risk weights should sum to 1.0, got {sum}"
        );
    }

    // ── Adaptive weight learning tests ──

    #[test]
    fn test_learn_weights_from_validation_events() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl");
        let val_path = dir.path().join("validations.jsonl");
        let weights_path = dir.path().join("weights.json");

        // Build 6 snapshot+validation pairs where churn signal consistently predicts hits
        let mut snap_lines = Vec::new();
        let mut val_lines = Vec::new();
        for day in 100..106 {
            snap_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T12:00:00Z","day":{},"git_hash":"abc{}","top_10":[{{"path":"src/hot.rs","score":0.9,"signals":["▲churn","▲size"]}},{{"path":"src/cold.rs","score":0.5,"signals":["▲size"]}}]}}"#,
                day - 90, day, day
            ));
            val_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T13:00:00Z","day":{},"trigger":"watch_failure","hits":["src/hot.rs"],"surprises":["src/other.rs"],"predicted_count":10,"accuracy_pct":50.0}}"#,
                day - 90, day
            ));
        }
        std::fs::write(&snap_path, snap_lines.join("\n") + "\n").expect("write snapshots");
        std::fs::write(&val_path, val_lines.join("\n") + "\n").expect("write validations");

        learn_weights_from_history_to(&val_path, &snap_path, &weights_path);

        assert!(weights_path.exists(), "weights file should be created");
        let content = std::fs::read_to_string(&weights_path).expect("read weights");
        let val: serde_json::Value = serde_json::from_str(&content).expect("parse JSON");

        let weights = val["weights"].as_array().expect("weights array");
        assert_eq!(weights.len(), 7);
        assert_eq!(val["learned_from"].as_u64(), Some(6));

        // Churn (index 0) should be boosted relative to default since it was present in all hits
        let churn_weight = weights[0].as_f64().unwrap();
        // Size (index 2) was present in hits too, but also in non-hit predictions
        // Both should be > 0 and the total should sum to ~1.0
        assert!(churn_weight > 0.0, "churn weight should be positive");
    }

    // ── Idempotent-write (planner-fallback no-op) tests ──

    #[test]
    fn test_weights_unchanged_within_epsilon_skips_write() {
        // Identical maps → not meaningfully changed.
        let old = [0.30, 0.15, 0.15, 0.10, 0.10, 0.10, 0.10];
        assert!(
            !weights_changed_meaningfully(&old, &old, WEIGHT_WRITE_EPSILON),
            "identical weights must not be a meaningful change"
        );

        // Sub-epsilon floating-point drift in every weight → still no change.
        let drift = 5e-4; // < WEIGHT_WRITE_EPSILON (1e-3)
        let new = old.map(|w| w + drift);
        assert!(
            !weights_changed_meaningfully(&old, &new, WEIGHT_WRITE_EPSILON),
            "sub-epsilon drift must not count as a meaningful change"
        );
    }

    #[test]
    fn test_weights_changed_beyond_epsilon_writes() {
        let old = [0.30, 0.15, 0.15, 0.10, 0.10, 0.10, 0.10];
        // One weight differs by more than epsilon → meaningful change.
        let mut new = old;
        new[0] += 1e-2; // 0.01 > 1e-3
        assert!(
            weights_changed_meaningfully(&old, &new, WEIGHT_WRITE_EPSILON),
            "a > epsilon difference must count as a meaningful change"
        );
    }

    #[test]
    fn test_weights_boundary_at_epsilon_near_miss() {
        let epsilon = 1e-3;
        // Construct old/new so the delta is *exactly* representable as epsilon:
        // start from 0.0 so no prior rounding error accumulates.
        let old = [0.0_f64; 7];

        // Exactly at epsilon: the comparison is strictly `> epsilon`, so a delta
        // *equal to* epsilon is NOT meaningful (the near-miss side, Day 122).
        let mut at = old;
        at[0] = epsilon;
        assert!(
            (at[0] - old[0]).abs() == epsilon,
            "test precondition: delta is exactly epsilon"
        );
        assert!(
            !weights_changed_meaningfully(&old, &at, epsilon),
            "a delta exactly at epsilon must NOT count as meaningful (near-miss)"
        );

        // Just above epsilon: meaningful.
        let mut above = old;
        above[0] = epsilon + 1e-6;
        assert!(
            weights_changed_meaningfully(&old, &above, epsilon),
            "a delta just above epsilon must count as meaningful"
        );
    }

    #[test]
    fn test_learn_weights_second_run_is_noop() {
        // A second learn with the same inputs must not rewrite the file:
        // the on-disk bytes stay byte-identical, so `git` sees no diff and the
        // planner-fallback stops manufacturing noise commits.
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl");
        let val_path = dir.path().join("validations.jsonl");
        let weights_path = dir.path().join("weights.json");

        let mut snap_lines = Vec::new();
        let mut val_lines = Vec::new();
        for day in 100..106 {
            snap_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T12:00:00Z","day":{},"git_hash":"abc{}","top_10":[{{"path":"src/hot.rs","score":0.9,"signals":["▲churn","▲size"]}},{{"path":"src/cold.rs","score":0.5,"signals":["▲size"]}}]}}"#,
                day - 90, day, day
            ));
            val_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T13:00:00Z","day":{},"trigger":"watch_failure","hits":["src/hot.rs"],"surprises":["src/other.rs"],"predicted_count":10,"accuracy_pct":50.0}}"#,
                day - 90, day
            ));
        }
        std::fs::write(&snap_path, snap_lines.join("\n") + "\n").expect("write snapshots");
        std::fs::write(&val_path, val_lines.join("\n") + "\n").expect("write validations");

        // First run creates the file.
        learn_weights_from_history_to(&val_path, &snap_path, &weights_path);
        assert!(weights_path.exists(), "weights file should be created");
        let first = std::fs::read_to_string(&weights_path).expect("read weights");

        // Second run with identical inputs must leave the bytes untouched
        // (the `last_updated` timestamp would otherwise differ, so this also
        // proves the write was skipped, not just re-written to the same value).
        learn_weights_from_history_to(&val_path, &snap_path, &weights_path);
        let second = std::fs::read_to_string(&weights_path).expect("read weights again");
        assert_eq!(
            first, second,
            "a re-learn with unchanged weights must not rewrite the file"
        );
    }

    #[test]
    fn test_learned_weights_sum_to_one() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl");
        let val_path = dir.path().join("validations.jsonl");
        let weights_path = dir.path().join("weights.json");

        // Create diverse validation data (6 events, meeting the minimum)
        let mut snap_lines = Vec::new();
        let mut val_lines = Vec::new();
        for day in 100..106 {
            snap_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T12:00:00Z","day":{},"git_hash":"abc{}","top_10":[{{"path":"src/a.rs","score":0.8,"signals":["▲churn","▲recent","▲reverts"]}},{{"path":"src/b.rs","score":0.6,"signals":["▲size","▲coupled"]}}]}}"#,
                day - 90, day, day
            ));
            val_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T13:00:00Z","day":{},"trigger":"watch_failure","hits":["src/a.rs"],"surprises":["src/c.rs"],"predicted_count":10,"accuracy_pct":50.0}}"#,
                day - 90, day
            ));
        }
        std::fs::write(&snap_path, snap_lines.join("\n") + "\n").expect("write");
        std::fs::write(&val_path, val_lines.join("\n") + "\n").expect("write");

        learn_weights_from_history_to(&val_path, &snap_path, &weights_path);

        let content = std::fs::read_to_string(&weights_path).expect("read");
        let val: serde_json::Value = serde_json::from_str(&content).expect("parse");
        let weights: Vec<f64> = val["weights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let sum: f64 = weights.iter().sum();
        assert!(
            (sum - 1.0).abs() < 0.001,
            "weights should sum to ~1.0, got {sum}"
        );
        for (i, &w) in weights.iter().enumerate() {
            assert!(w >= 0.0, "weight {i} should be non-negative, got {w}");
        }
    }

    #[test]
    fn test_load_learned_weights_fallback() {
        // Missing file → defaults
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("nonexistent.json");
        let weights = load_learned_weights_from(&missing);
        assert_eq!(weights, RISK_WEIGHTS);

        // Invalid JSON → defaults
        let bad_json = dir.path().join("bad.json");
        std::fs::write(&bad_json, "not json").expect("write");
        let weights = load_learned_weights_from(&bad_json);
        assert_eq!(weights, RISK_WEIGHTS);

        // Wrong number of weights → defaults
        let wrong_count = dir.path().join("wrong.json");
        std::fs::write(&wrong_count, r#"{"weights":[0.5,0.5]}"#).expect("write");
        let weights = load_learned_weights_from(&wrong_count);
        assert_eq!(weights, RISK_WEIGHTS);

        // Weights that don't sum to ~1.0 → defaults
        let bad_sum = dir.path().join("badsum.json");
        std::fs::write(&bad_sum, r#"{"weights":[0.5,0.5,0.5,0.5,0.5,0.5,0.5]}"#).expect("write");
        let weights = load_learned_weights_from(&bad_sum);
        assert_eq!(weights, RISK_WEIGHTS);

        // Valid weights → loaded
        let good = dir.path().join("good.json");
        std::fs::write(&good, r#"{"weights":[0.25,0.15,0.15,0.10,0.10,0.15,0.10]}"#)
            .expect("write");
        let weights = load_learned_weights_from(&good);
        assert_eq!(weights, [0.25, 0.15, 0.15, 0.10, 0.10, 0.15, 0.10]);
    }

    /// Every way a *present* weights file can be unusable must surface as a
    /// distinct, named defect — not as "we silently got the defaults back".
    ///
    /// Before this, all six paths returned `RISK_WEIGHTS` with no signal, so a
    /// corrupted `risk_weights.json` would make the learned half of the risk
    /// model dead while `/risk` kept printing confident scores. Day 139:
    /// fail-soft without a liveness signal is fail-silent.
    #[test]
    fn parse_learned_weights_names_every_defect() {
        let cases: &[(&str, WeightsDefect)] = &[
            ("not json at all", WeightsDefect::NotJson),
            ("{}", WeightsDefect::NoWeightsArray),
            (r#"{"weights": "nope"}"#, WeightsDefect::NoWeightsArray),
            (r#"{"weights":[0.5,0.5]}"#, WeightsDefect::WrongLength(2)),
            (
                r#"{"weights":[0.2,0.2,0.2,0.2,0.1,0.1,"x"]}"#,
                WeightsDefect::BadValue(6),
            ),
            (
                r#"{"weights":[-0.1,0.3,0.2,0.2,0.1,0.1,0.2]}"#,
                WeightsDefect::BadValue(0),
            ),
        ];
        for (content, expected) in cases {
            let got = parse_learned_weights(content);
            assert_eq!(
                got.as_ref().err(),
                Some(expected),
                "parse_learned_weights({content:?}) should report {expected:?}"
            );
            // The warning text must actually say something about what's wrong.
            assert!(
                !expected.describe().is_empty(),
                "{expected:?} must describe itself"
            );
        }

        // Sum-out-of-range carries the offending sum so the warning is specific.
        let bad_sum = parse_learned_weights(r#"{"weights":[0.5,0.5,0.5,0.5,0.5,0.5,0.5]}"#);
        match bad_sum {
            Err(WeightsDefect::SumOutOfRange(sum)) => {
                assert!((sum - 3.5).abs() < 1e-9, "sum should be 3.5, got {sum}");
            }
            other => panic!("expected SumOutOfRange, got {other:?}"),
        }

        // A valid file still parses cleanly — the guard must not eat good input.
        let good = parse_learned_weights(r#"{"weights":[0.25,0.15,0.15,0.10,0.10,0.15,0.10]}"#);
        assert_eq!(
            good,
            Ok([0.25, 0.15, 0.15, 0.10, 0.10, 0.15, 0.10]),
            "a valid weights file must load"
        );
    }

    /// A defect description must name the signal / value at fault, so the
    /// stderr line is actionable rather than merely present.
    #[test]
    fn weights_defect_descriptions_are_specific() {
        assert!(WeightsDefect::WrongLength(2).describe().contains('2'));
        // `SIGNAL_NAMES[1]` is the second learned signal; the message names it.
        let bad = WeightsDefect::BadValue(1).describe();
        assert!(
            bad.contains(SIGNAL_NAMES[1]),
            "BadValue should name the signal, got {bad:?}"
        );
        assert!(WeightsDefect::SumOutOfRange(3.5).describe().contains("3.5"));
        assert!(WeightsDefect::NotJson.describe().contains("JSON"));
    }

    #[test]
    fn test_learn_weights_minimum_events() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let snap_path = dir.path().join("snapshots.jsonl");
        let val_path = dir.path().join("validations.jsonl");
        let weights_path = dir.path().join("weights.json");

        // Only 3 events (below MIN_VALIDATION_EVENTS of 5) — should NOT produce weights
        let mut snap_lines = Vec::new();
        let mut val_lines = Vec::new();
        for day in 100..103 {
            snap_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T12:00:00Z","day":{},"git_hash":"abc{}","top_10":[{{"path":"src/a.rs","score":0.8,"signals":["▲churn"]}}]}}"#,
                day - 90, day, day
            ));
            val_lines.push(format!(
                r#"{{"ts":"2025-01-{:02}T13:00:00Z","day":{},"trigger":"watch_failure","hits":["src/a.rs"],"surprises":[],"predicted_count":10,"accuracy_pct":100.0}}"#,
                day - 90, day
            ));
        }
        std::fs::write(&snap_path, snap_lines.join("\n") + "\n").expect("write");
        std::fs::write(&val_path, val_lines.join("\n") + "\n").expect("write");

        learn_weights_from_history_to(&val_path, &snap_path, &weights_path);

        assert!(
            !weights_path.exists(),
            "weights file should NOT be created with fewer than 5 events"
        );
    }
}
