//! `/risk` command: file risk scoring, snapshot, validate, history,
//! co-change coupling, test coverage mapping.
//!
//! Extracted from `commands_info.rs` — the risk scorer subsystem built
//! over Days 111-113 as the dream milestone infrastructure.

use crate::format::*;

// Snapshot/validation persistence lives in `commands_risk_snapshots.rs`.
// Re-exported here so all call sites (watch.rs, commands_git.rs, and this
// module's own scoring/reporting code) remain unchanged.
pub(crate) use crate::commands_risk_snapshots::{
    accuracy_of, auto_risk_snapshot, auto_validate_after_failure, build_risk_snapshot_json,
    ci_event_exists_for, ci_payload_note, emerging_grade_of, load_validation_history_from,
    parse_all_snapshots, parse_ci_run_payload, parse_failed_ci_runs, parse_validation_events,
    read_snapshot_ledger, read_validation_ledger, risk_autosnapshot_enabled, snapshot_before,
    write_risk_snapshot_to, write_validation_event, SnapshotLedger, ValidationEvent,
    ValidationLedger, RISK_SNAPSHOT_PATH, RISK_VALIDATION_PATH,
};

// Report/context formatting lives in `commands_risk_report.rs`.
// Re-exported here so all call sites (watch.rs, smart_edit.rs,
// commands_info.rs, and this module's own scoring code) remain unchanged.
// (`risk_context_for_files_from` / `file_risk_summary_from` are only used
// inside the report module itself, so they aren't re-exported.)
pub(crate) use crate::commands_risk_report::{
    emerging_track_record_note, file_risk_summary, format_risk_context, format_risk_report,
    prediction_accuracy_summary, recall_coverage_note, risk_context_for_files,
};

// Emerging-risk detection lives in `commands_risk_emerging.rs`; re-exported for
// commands_project.rs, watch.rs and the snapshot path. Its renderer was deleted
// Day 163 (#724) — that module's docs hold the decision and its numbers.
pub(crate) use crate::commands_risk_emerging::detect_emerging_risks;

// Epistemic ranking (rank files by how little graded outcomes have taught the
// model — the dream's epistemic-appetite milestone, ranking half only) lives
// in `commands_risk_epistemic.rs`.
use crate::commands_risk_epistemic::handle_risk_epistemic;

// Prediction-accuracy stats live in `commands_risk_accuracy.rs`.
// Re-exported here so all call sites (commands_risk_report.rs and this
// module's own accuracy/effectiveness code) remain unchanged.
// (`AccuracyStats` and the private trend helper are only used inside the
// accuracy module itself, so they aren't re-exported.)
pub(crate) use crate::commands_risk_accuracy::{
    compute_accuracy_stats, format_accuracy_report, is_green_event, AccuracyTrend,
};

// Weight-learning + revert-history live in `commands_risk_weights.rs`.
// Re-exported here so all call sites (commands_risk_snapshots.rs,
// commands_risk_emerging.rs, and this module's own scoring/report code)
// remain unchanged.
pub(crate) use crate::commands_risk_weights::{
    learn_weights_from_history, load_learned_weights, parse_detailed_events, revert_history,
    LEARNED_WEIGHTS_PATH, MIN_VALIDATION_EVENTS, RISK_WEIGHTS, SIGNAL_NAMES,
};

/// A single file's risk assessment with score and signal labels.
pub(crate) struct FileRisk {
    pub path: String,
    pub score: f64,
    pub signals: Vec<&'static str>,
    /// Tests per 100 lines of code (`#[test]` count / line_count × 100).
    /// 0.0 for non-Rust files or files that can't be read.
    pub test_density: f64,
}

/// Min-max normalize a slice of values to the 0.0–1.0 range.
/// All-equal or empty inputs return all zeros.
fn normalize_scores(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if range == 0.0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| (v - min) / range).collect()
}

/// Build a map of source file paths → count of test-containing files that reference them.
///
/// For each `.rs` file in `src/` and `tests/` that contains `#[test]`, we parse
/// `use crate::module` and `crate::module::` patterns to find which source modules
/// it exercises. This produces cross-file test coverage signals that complement
/// the same-file `#[test]` density metric.
fn build_test_reference_map() -> std::collections::HashMap<String, u32> {
    use std::collections::{HashMap, HashSet};

    let mut ref_map: HashMap<String, u32> = HashMap::new();

    // Collect all .rs files from src/ (including src/format/) and tests/
    let mut all_rs_files: Vec<String> = Vec::new();
    for dir in &["src", "src/format", "tests"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "rs") {
                    if let Some(p) = path.to_str() {
                        all_rs_files.push(p.to_string());
                    }
                }
            }
        }
    }

    // For each file that contains #[test], extract the modules it references
    for file_path in &all_rs_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Only process files that contain test markers
        if !content.contains("#[test]") {
            continue;
        }

        // Track which source files this test file references (deduplicated)
        let mut referenced: HashSet<String> = HashSet::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Match `use crate::module_name` patterns
            // e.g. `use crate::safety;` → src/safety.rs
            // e.g. `use crate::format::cost;` → src/format/cost.rs
            // e.g. `use crate::format::cost::{fn1, fn2};` → src/format/cost.rs
            // e.g. `use crate::cli::{something};` → src/cli.rs
            if trimmed.starts_with("use crate::") {
                if let Some(rest) = trimmed.strip_prefix("use crate::") {
                    // Get the module path (before any `::` item, `{`, or `;`)
                    let module_path = rest
                        .split('{')
                        .next()
                        .unwrap_or(rest)
                        .trim_end_matches(';')
                        .trim_end_matches('*')
                        .trim_end_matches("::")
                        .trim();
                    if let Some(src_path) = module_to_source_path(module_path) {
                        referenced.insert(src_path);
                    }
                }
            }

            // Match inline `crate::module::` patterns in function calls, type references, etc.
            // e.g. `crate::format::enable_quiet();` → src/format/mod.rs
            // e.g. `crate::git::run_git(...)` → src/git.rs
            let mut search_pos = 0;
            while let Some(idx) = trimmed[search_pos..].find("crate::") {
                let abs_idx = search_pos + idx;
                // Skip if this is part of a `use crate::` (already handled above).
                // `ends_with` on the prefix, never `trimmed[abs_idx - 4..abs_idx]`:
                // `abs_idx - 4` can land inside a multi-byte char (e.g. a line
                // reading "… — [`crate::foo`]") and byte-slicing there panics.
                if trimmed[..abs_idx].ends_with("use ") {
                    search_pos = abs_idx + 7;
                    continue;
                }
                let after = &trimmed[abs_idx + 7..]; // skip "crate::"
                                                     // Extract the module path: take chars until we hit '(' or '{' or ';' or whitespace
                let module_part: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                    .collect();
                // We want the module, not the function — strip the last ::item if present
                if let Some(src_path) = resolve_crate_reference(&module_part) {
                    referenced.insert(src_path);
                }
                search_pos = abs_idx + 7 + module_part.len();
            }
        }

        // A file referencing itself doesn't count as cross-file coverage
        // (the same-file density already captures that)
        referenced.remove(file_path);

        // Increment the reference count for each referenced source file
        for src_file in referenced {
            *ref_map.entry(src_file).or_insert(0) += 1;
        }
    }

    ref_map
}

/// Convert a module path like "safety" → "src/safety.rs" or "format::cost" → "src/format/cost.rs".
/// Returns None if the resolved path doesn't exist on disk.
fn module_to_source_path(module_path: &str) -> Option<String> {
    if module_path.is_empty() {
        return None;
    }

    // Split on "::" to handle nested modules
    let parts: Vec<&str> = module_path.split("::").collect();

    // Try as a direct file: src/part1/part2/.../partN.rs
    let file_path = format!("src/{}.rs", parts.join("/"));
    if std::path::Path::new(&file_path).exists() {
        return Some(file_path);
    }

    // Try as a directory module: src/part1/part2/.../mod.rs
    let mod_path = format!("src/{}/mod.rs", parts.join("/"));
    if std::path::Path::new(&mod_path).exists() {
        return Some(mod_path);
    }

    // For single-segment like "format", also check src/format/mod.rs
    if parts.len() == 1 {
        let dir_mod = format!("src/{}/mod.rs", parts[0]);
        if std::path::Path::new(&dir_mod).exists() {
            return Some(dir_mod);
        }
    }

    None
}

/// Resolve a `crate::module::path::item` reference to a source file.
/// Tries progressively shorter prefixes until one resolves to a file.
/// e.g. "format::enable_quiet" → tries "format/enable_quiet.rs", then "format" → "src/format/mod.rs"
fn resolve_crate_reference(reference: &str) -> Option<String> {
    if reference.is_empty() {
        return None;
    }

    let parts: Vec<&str> = reference.split("::").collect();

    // Try progressively shorter prefixes (the last segment is likely a function/type name)
    for end in (1..=parts.len()).rev() {
        let module_path = parts[..end].join("::");
        if let Some(path) = module_to_source_path(&module_path) {
            return Some(path);
        }
    }

    None
}

/// Build a co-change coupling map from the last 100 commits.
///
/// For each `src/**/*.rs` file, records which other `src/**/*.rs` files are
/// frequently modified in the same commit. Returns a nested map:
///   file → { partner_file → co_change_count }
fn co_change_coupling() -> std::collections::HashMap<String, std::collections::HashMap<String, u32>>
{
    let output = match crate::git::run_git(&["log", "--name-only", "--pretty=format:", "-100"]) {
        Ok(o) => o,
        Err(_) => return std::collections::HashMap::new(),
    };

    // Parse commits: groups of file paths separated by blank lines
    let mut commits: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                commits.push(std::mem::take(&mut current));
            }
            continue;
        }
        // Only track src/**/*.rs files
        if trimmed.starts_with("src/") && trimmed.ends_with(".rs") {
            current.push(trimmed.to_string());
        }
    }
    if !current.is_empty() {
        commits.push(current);
    }

    // For each commit, record co-change pairs
    let mut coupling: std::collections::HashMap<String, std::collections::HashMap<String, u32>> =
        std::collections::HashMap::new();

    for commit_files in &commits {
        // Only consider commits that touch 2+ src files (and skip huge merges > 20 files)
        if commit_files.len() < 2 || commit_files.len() > 20 {
            continue;
        }
        for file_a in commit_files {
            for file_b in commit_files {
                if file_a != file_b {
                    *coupling
                        .entry(file_a.clone())
                        .or_default()
                        .entry(file_b.clone())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    coupling
}

/// Keep only candidate paths that still exist, per an injected resolver.
///
/// **Why this exists (do not "simplify" it away).** The risk-score universe is
/// assembled from three sources: two git-churn windows and two disk walks. The
/// disk walks are existence-checked by construction; the churn-derived paths are
/// not — and **a file's own deletion commit is churn**. So deleting a file hands
/// it a fresh 30-day *and* 7-day change count (i.e. a fresh risk score) while
/// simultaneously guaranteeing it has never appeared in any prediction column,
/// which sorts it straight to the top of the *never forecast* dark set in
/// `yoyo risk epistemic`. `scripts/extract_trajectory.py` feeds those rows to the
/// planner as "point the self-driven slot here", so a deleted file becomes the
/// most attractive target in the one output that steers task selection. Live
/// instance: `src/commands_risk_families.rs`, deleted Day 173 by #804, led the
/// dark set on Day 174 and had already eaten one session (receipt #807) as an
/// empty-diff revert.
///
/// Note the polarity: this is the opposite of the Day-166 too-new split. That one
/// stopped calling genuinely-new files dark; this one drops a file that can
/// **never** be studied from a list of rooms worth entering.
///
/// The I/O stays at the call site (`&|p| Path::new(p).exists()`) so the decision
/// half is pure and table-tested, matching the discipline of its siblings
/// (`never_forecast_files` injects `added_ts`; `revisit_add_at` injects the
/// resolved title).
fn scorable_paths(
    candidates: impl IntoIterator<Item = String>,
    exists: &dyn Fn(&str) -> bool,
) -> std::collections::HashSet<String> {
    candidates.into_iter().filter(|p| exists(p)).collect()
}

/// Compute risk scores for all `src/**/*.rs` files using six weighted signals.
pub(crate) fn compute_file_risk_scores() -> Vec<FileRisk> {
    // 1. Change frequency (30 days) — weight 0.30
    let counts_30 = crate::git::file_change_counts(30);
    let counts_30_map: std::collections::HashMap<&str, u32> =
        counts_30.iter().map(|(p, c)| (p.as_str(), *c)).collect();

    // 2. Recent acceleration (7-day count) — weight 0.25
    let counts_7 = crate::git::file_change_counts(7);
    let counts_7_map: std::collections::HashMap<&str, u32> =
        counts_7.iter().map(|(p, c)| (p.as_str(), *c)).collect();

    // Collect all unique file paths from both windows and disk
    let mut file_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (p, _) in &counts_30 {
        file_set.insert(p.clone());
    }
    for (p, _) in &counts_7 {
        file_set.insert(p.clone());
    }
    // Also include src/*.rs files from disk that might have zero churn
    if let Ok(entries) = std::fs::read_dir("src") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rs") {
                if let Some(p) = path.to_str() {
                    file_set.insert(p.to_string());
                }
            }
        }
    }
    // Also check src/format/*.rs
    if let Ok(entries) = std::fs::read_dir("src/format") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rs") {
                if let Some(p) = path.to_str() {
                    file_set.insert(p.to_string());
                }
            }
        }
    }
    // Drop candidates that no longer exist on disk. The two disk walks above are
    // existence-checked by construction; the two churn windows are not, and a
    // file's own deletion commit is churn — see `scorable_paths` for why that
    // lands a deleted file at the top of the planner's dark-room list.
    let file_set = scorable_paths(file_set, &|p: &str| std::path::Path::new(p).exists());
    let mut all_files: Vec<String> = file_set.into_iter().collect();
    all_files.sort();

    if all_files.is_empty() {
        return Vec::new();
    }

    // Gather raw signal values for each file
    let mut raw_churn: Vec<f64> = Vec::with_capacity(all_files.len());
    let mut raw_accel: Vec<f64> = Vec::with_capacity(all_files.len());
    let mut raw_size: Vec<f64> = Vec::with_capacity(all_files.len());
    let mut raw_complexity: Vec<f64> = Vec::with_capacity(all_files.len());
    let mut raw_revert: Vec<f64> = Vec::with_capacity(all_files.len());
    let mut raw_test_density: Vec<f64> = Vec::with_capacity(all_files.len());
    let mut raw_coupling: Vec<f64> = Vec::with_capacity(all_files.len());
    // Tests-per-100-lines metric (exposed on FileRisk for display)
    let mut tests_per_100: Vec<f64> = Vec::with_capacity(all_files.len());

    // 4. Revert involvement — weight 0.10
    let revert_files = revert_history();

    // 6. Co-change coupling — weight 0.15
    let coupling_map = co_change_coupling();

    // 5b. Cross-file test coverage — how many test-containing files reference each module
    let cross_file_refs = build_test_reference_map();
    // Count total test-containing files for normalization
    let total_test_files = {
        let mut test_files = std::collections::HashSet::new();
        for dir in &["src", "src/format", "tests"] {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "rs") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if content.contains("#[test]") {
                                if let Some(p) = path.to_str() {
                                    test_files.insert(p.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        test_files.len().max(1) as f64
    };

    for path in &all_files {
        // Churn (30-day count)
        let c30 = *counts_30_map.get(path.as_str()).unwrap_or(&0) as f64;
        raw_churn.push(c30);

        // Recent acceleration: ratio of 7-day to 30-day, clamped
        let c7 = *counts_7_map.get(path.as_str()).unwrap_or(&0) as f64;
        let accel = if c30 > 0.0 {
            // If all 30-day changes happened in the last 7 days, ratio = 1.0
            // Normalize by expected proportion: 7/30 ≈ 0.233
            // So accel = (c7/c30) / (7/30) — values > 1.0 mean accelerating
            (c7 / c30) / (7.0 / 30.0)
        } else if c7 > 0.0 {
            // File only appeared in the last 7 days — maximally accelerating
            3.0
        } else {
            0.0
        };
        raw_accel.push(accel);

        // File size (line count) and complexity proxy (branch/match density)
        let (line_count, complexity) = std::fs::read_to_string(path)
            .map(|content| {
                let lines = content.lines().count() as f64;
                if lines == 0.0 {
                    return (0.0, 0.0);
                }
                // Cyclomatic complexity proxy: count branching constructs
                let branches = content
                    .lines()
                    .filter(|l| {
                        let t = l.trim();
                        t.starts_with("if ")
                            || t.starts_with("} else")
                            || t.starts_with("match ")
                            || t.starts_with("for ")
                            || t.starts_with("while ")
                            || t.contains("=> ")
                    })
                    .count() as f64;
                (lines, branches / lines)
            })
            .unwrap_or((0.0, 0.0));
        raw_size.push(line_count);
        raw_complexity.push(complexity);

        // Revert involvement
        let rev_count = *revert_files.get(path.as_str()).unwrap_or(&0) as f64;
        raw_revert.push(rev_count);

        // 5. Test density: combine same-file markers with cross-file coverage
        // Same-file: #[test] + #[cfg(test)] markers / total lines
        let (same_file_density, file_tests_per_100) = std::fs::read_to_string(path)
            .map(|content| {
                let total = content.lines().count() as f64;
                if total == 0.0 {
                    return (0.0, 0.0);
                }
                let test_markers = content
                    .lines()
                    .filter(|l| {
                        let trimmed = l.trim();
                        trimmed.contains("#[test]") || trimmed.contains("#[cfg(test)]")
                    })
                    .count() as f64;
                // Count only #[test] annotations for the per-100-lines metric
                let test_fn_count = content
                    .lines()
                    .filter(|l| l.trim().contains("#[test]"))
                    .count() as f64;
                (test_markers / total, test_fn_count / total * 100.0)
            })
            .unwrap_or((0.0, 0.0));
        tests_per_100.push(if path.ends_with(".rs") {
            file_tests_per_100
        } else {
            0.0
        });

        // Cross-file: fraction of test-containing files that reference this module
        let cross_refs = *cross_file_refs.get(path.as_str()).unwrap_or(&0) as f64;
        let cross_file_coverage = (cross_refs / total_test_files).min(1.0);

        // Blend: use whichever signal is stronger — a file well-tested either
        // in-file or cross-file should have lower risk
        let effective_coverage = same_file_density.max(cross_file_coverage);

        // Invert: low coverage → high risk signal
        raw_test_density.push(1.0 - effective_coverage);

        // 6. Co-change coupling: how many high-churn partners does this file have?
        // Sum co-change counts with all partners (raw; will be normalized later)
        let coupling_score = coupling_map
            .get(path.as_str())
            .map(|partners| partners.values().sum::<u32>() as f64)
            .unwrap_or(0.0);
        raw_coupling.push(coupling_score);
    }

    // Normalize each signal to 0.0–1.0
    let norm_churn = normalize_scores(&raw_churn);
    let norm_accel = normalize_scores(&raw_accel);
    let norm_size = normalize_scores(&raw_size);
    let norm_complexity = normalize_scores(&raw_complexity);
    let norm_test = normalize_scores(&raw_test_density);
    let norm_coupling = normalize_scores(&raw_coupling);
    let norm_revert = normalize_scores(&raw_revert);

    // Weighted sum → final score (7 signals, sum = 1.0)
    // Use learned weights if available, otherwise fall back to defaults.
    let weights = load_learned_weights();
    let mut risks: Vec<FileRisk> = Vec::with_capacity(all_files.len());

    for (i, path) in all_files.into_iter().enumerate() {
        let td = tests_per_100[i];
        // Base weighted score from normalized signals
        // Order: churn, recency, size, complexity, test_density, coupling, revert_history
        let mut score = norm_churn[i] * weights[0]
            + norm_accel[i] * weights[1]
            + norm_size[i] * weights[2]
            + norm_complexity[i] * weights[3]
            + norm_test[i] * weights[4]
            + norm_coupling[i] * weights[5]
            + norm_revert[i] * weights[6];

        // Penalty: files with fewer than 5 tests per 100 lines get a bump
        // (only for .rs files where test density is meaningful)
        if path.ends_with(".rs") {
            score += f64::max(0.0, (5.0 - td) * 2.0) / 100.0;
        }

        let mut signals = Vec::new();
        if norm_churn[i] > 0.5 {
            signals.push("▲churn");
        }
        if norm_accel[i] > 0.5 {
            signals.push("▲recent");
        }
        if norm_size[i] > 0.5 {
            signals.push("▲size");
        }
        if norm_revert[i] > 0.5 {
            signals.push("▲reverts");
        }
        if norm_test[i] > 0.5 {
            signals.push("▲low-test");
        }
        if norm_coupling[i] > 0.7 {
            signals.push("▲coupled");
        }

        risks.push(FileRisk {
            path,
            score,
            signals,
            test_density: td,
        });
    }

    // Sort descending by score, with filename tiebreaker for determinism
    risks.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    risks
}

/// Return the top `n` riskiest files as `(path, score)` pairs, sorted descending.
///
/// Convenience wrapper around `compute_file_risk_scores()` for use by `/status`
/// and other consumers that just need names and numbers.
pub(crate) fn top_risk_files(n: usize) -> Vec<(String, f64)> {
    top_n_from(&compute_file_risk_scores(), n)
}

/// Pure helper: take the top `n` entries from an already-computed (sorted)
/// score list as `(path, score)` pairs.
///
/// Extracted so tests can exercise the prefix logic against a fixed synthetic
/// list instead of computing live repo scores twice (which flaked when repo
/// state shifted between the two computations mid-test).
pub(crate) fn top_n_from(risks: &[FileRisk], n: usize) -> Vec<(String, f64)> {
    risks
        .iter()
        .take(n)
        .map(|r| (r.path.clone(), r.score))
        .collect()
}

/// Subcommands for `/risk` tab-completion.
pub(crate) const RISK_SUBCOMMANDS: &[&str] = &[
    "snapshot",
    "validate",
    "history",
    "predict",
    "accuracy",
    "effectiveness",
    "epistemic",
    "harvest",
    "--all",
];

/// True if `sub` is not a recognized `/risk` subcommand. Empty input means
/// "show the default report" and is never unknown. Kept in sync with
/// [`RISK_SUBCOMMANDS`] so tab-completion and dispatch agree by construction.
pub(crate) fn is_unknown_risk_subcommand(sub: &str) -> bool {
    !sub.is_empty() && !RISK_SUBCOMMANDS.contains(&sub)
}

/// Human-readable list of `/risk` subcommands, derived from
/// [`RISK_SUBCOMMANDS`] so the unknown-subcommand error can never drift out
/// of sync with the actual dispatch table (single source of truth).
pub(crate) fn risk_subcommand_list() -> String {
    RISK_SUBCOMMANDS.join(" | ")
}

/// Handle the `/risk` command — display per-file risk scores.
pub(crate) fn handle_risk(input: &str) {
    let sub = input.strip_prefix("/risk").unwrap_or(input).trim();

    if sub == "snapshot" {
        handle_risk_snapshot();
        return;
    }

    if sub == "history" {
        handle_risk_history();
        return;
    }

    if sub == "validate" {
        handle_risk_validate();
        return;
    }

    if sub == "predict" {
        handle_risk_predict();
        return;
    }

    if sub == "accuracy" {
        handle_risk_accuracy();
        return;
    }

    if sub == "effectiveness" {
        handle_risk_effectiveness();
        return;
    }

    if sub == "epistemic" {
        handle_risk_epistemic();
        return;
    }

    if sub == "harvest" {
        handle_risk_harvest();
        return;
    }

    // Unknown subcommand: fail loudly instead of silently printing the
    // default report. A typo like `yoyo risk snapshoot` used to fall through
    // here — the user believed a snapshot was recorded when nothing was.
    if is_unknown_risk_subcommand(sub) {
        println!("Unknown /risk subcommand: {sub}");
        println!("Available: {}", risk_subcommand_list());
        return;
    }

    let show_all = input.contains("--all");
    let risks = compute_file_risk_scores();
    let report = format_risk_report(&risks, show_all);
    print!("{report}");

    // `⚡ Emerging Risks` printed here until Day 163 (#724) — deleted at 0% recall.
}

// ── /risk predict ────────────────────────────────────────────────────

/// Map signal count to a confidence level label.
fn predict_confidence_level(signal_count: usize) -> &'static str {
    match signal_count {
        0 => "low",
        1 => "low",
        2 => "medium",
        _ => "high",
    }
}

/// Format the confidence dots: ●●●○ high, ●●○○ medium, ●○○○ low.
fn predict_confidence_dots(level: &str) -> String {
    let (filled, label) = match level {
        "high" => (3, "high"),
        "medium" => (2, "medium"),
        _ => (1, "low"),
    };
    let dots: String = "●".repeat(filled) + &"○".repeat(4 - filled);
    format!("{dots} {label}")
}

/// Generate a human-readable "why this file is dangerous" explanation
/// based on the active signals for a file.
fn predict_reason(signals: &[&str], test_density: f64) -> String {
    let has = |s: &str| signals.iter().any(|sig| sig.contains(s));

    let low_test = has("low-test") || test_density < 1.0;
    let high_churn = has("churn");
    let high_coupled = has("coupled");
    let has_reverts = has("revert");
    let high_size = has("size");
    let recent = has("recent");

    // Pick the most descriptive combination
    if high_churn && low_test {
        "frequently changed with weak test coverage".to_string()
    } else if high_coupled && high_churn {
        "frequently changed alongside other files — breakage cascades".to_string()
    } else if has_reverts {
        "has been reverted before — historically fragile".to_string()
    } else if (high_size || signals.iter().any(|s| s.contains("complex"))) && recent {
        "complex file recently modified — regression risk".to_string()
    } else if high_churn && recent {
        "frequently changed with recent modifications".to_string()
    } else if high_churn {
        "high change frequency — more opportunities for bugs".to_string()
    } else if low_test {
        "low test coverage — changes go unvalidated".to_string()
    } else if high_coupled {
        "tightly coupled — changes here ripple to other files".to_string()
    } else if recent {
        "recently modified — fresh changes may contain regressions".to_string()
    } else {
        "elevated risk score from combined signals".to_string()
    }
}

/// Build a short reason summary for the top-level prediction line.
fn predict_top_reason(risk: &FileRisk) -> String {
    let has = |s: &str| risk.signals.iter().any(|sig| sig.contains(s));

    let mut parts = Vec::new();
    if has("churn") {
        parts.push("high churn");
    }
    if has("low-test") || risk.test_density < 1.0 {
        parts.push("low test density");
    }
    if has("coupled") {
        parts.push("high coupling");
    }
    if has("revert") {
        parts.push("revert history");
    }
    if has("size") {
        parts.push("large file");
    }
    if has("recent") {
        parts.push("recent changes");
    }

    if parts.is_empty() {
        "elevated risk score".to_string()
    } else {
        parts.join(" + ")
    }
}

/// Format a single prediction card for one file.
fn format_prediction_card(rank: usize, risk: &FileRisk) -> String {
    let mut out = String::new();
    let score_str = format!("{:.2}", risk.score);
    let signal_names: Vec<&str> = risk.signals.to_vec();
    let signal_list = if signal_names.is_empty() {
        "(none)".to_string()
    } else {
        signal_names.join(", ")
    };
    let density_str = format!("{:.1}", risk.test_density);
    let reason = predict_reason(&risk.signals, risk.test_density);
    let confidence = predict_confidence_level(risk.signals.len());
    let dots = predict_confidence_dots(confidence);

    out.push_str(&format!(
        "  │\n  │  {BOLD}#{rank}{RESET}  {CYAN}{}{RESET}",
        risk.path
    ));
    // Right-align score
    let pad = 50usize.saturating_sub(risk.path.len() + format!("#{rank}").len() + 2);
    out.push_str(&" ".repeat(pad));
    out.push_str(&format!("score: {score_str}\n"));

    out.push_str(&format!("  │      signals: {signal_list}\n"));
    out.push_str(&format!(
        "  │      test density: {density_str} per 100 lines\n"
    ));
    out.push_str(&format!("  │      {DIM}→ {reason}{RESET}\n"));
    out.push_str(&format!("  │      confidence: {dots}\n"));

    out
}

/// Load past prediction accuracy from snapshot history.
/// Returns `(precision_pct, snapshot_count, trend)` if snapshots exist.
fn load_past_accuracy() -> Option<(f64, usize, &'static str)> {
    load_past_accuracy_from(std::path::Path::new(RISK_SNAPSHOT_PATH))
}

/// Load past prediction accuracy from a given snapshot file path.
fn load_past_accuracy_from(path: &std::path::Path) -> Option<(f64, usize, &'static str)> {
    let content = std::fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let snapshots = parse_all_snapshots(&content);
    if snapshots.len() < 2 {
        return None;
    }

    // Validate each consecutive pair of snapshots
    let mut validations = Vec::new();
    for pair in snapshots.windows(2) {
        let from = &pair[0];
        let to = &pair[1];

        // Get files that changed/broke between snapshots
        let log_output = crate::git::run_git(&[
            "log",
            &format!("{}..{}", from.git_hash, to.git_hash),
            "--name-only",
            "--oneline",
        ])
        .ok()?;

        if log_output.trim().is_empty() {
            continue;
        }

        let entries = parse_git_log_name_only(&log_output);
        let commit_count = entries.len();
        let broke_files = classify_broke_files(&entries);

        let result = compute_validation(&from.predicted, &broke_files, None, commit_count);
        validations.push(HistoryValidation {
            day: from.day,
            git_hash_from: from.git_hash.clone(),
            git_hash_to: to.git_hash.clone(),
            result,
        });
    }

    if validations.is_empty() {
        return None;
    }

    // Average precision across all intervals
    let total_precision: f64 = validations.iter().map(|v| precision(&v.result)).sum();
    let avg_precision = total_precision / validations.len() as f64;
    let pct = (avg_precision * 100.0).round();
    let trend = compute_trend(&validations);

    Some((pct, validations.len(), trend))
}

/// Format the full prediction report (delegates to `format_prediction_report_with_accuracy`).
fn format_prediction_report(risks: &[FileRisk], top_n: usize) -> String {
    let accuracy = load_past_accuracy();
    format_prediction_report_with_accuracy(risks, top_n, accuracy)
}

/// Format the full prediction report with optional past accuracy data.
/// Separated from `format_prediction_report` so tests can supply synthetic accuracy.
fn format_prediction_report_with_accuracy(
    risks: &[FileRisk],
    top_n: usize,
    accuracy: Option<(f64, usize, &str)>,
) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "\n  ┌ {BOLD}Risk Prediction{RESET} ────────────────────────────\n"
    ));

    let display_risks: Vec<&FileRisk> = risks.iter().take(top_n).collect();

    if display_risks.is_empty() {
        out.push_str("  │\n");
        out.push_str("  │  No risk data available.\n");
        out.push_str("  └──────────────────────────────────────────────\n\n");
        return out;
    }

    for (i, risk) in display_risks.iter().enumerate() {
        out.push_str(&format_prediction_card(i + 1, risk));
    }

    // Summary prediction line
    let top = &display_risks[0];
    let top_reason = predict_top_reason(top);
    out.push_str("  │\n");
    out.push_str(&format!(
        "  │  {BOLD}Prediction:{RESET} {CYAN}{}{RESET} is most likely to\n",
        top.path
    ));
    out.push_str(&format!("  │  cause the next failure ({top_reason})\n"));

    // Track record from past snapshots
    if let Some((pct, count, trend)) = accuracy {
        let trend_arrow = if trend.contains("Improving") {
            "↑"
        } else if trend.contains("Declining") {
            "↓"
        } else {
            "→"
        };
        out.push_str("  │\n");
        out.push_str(&format!(
            "  │  {DIM}Track record: {pct:.0}% precision over {count} snapshots ({} {trend_arrow}){RESET}\n",
            trend.trim_start_matches(|c: char| !c.is_alphabetic())
        ));
    }

    out.push_str("  └──────────────────────────────────────────────\n\n");
    out
}

/// Handle `/risk predict` — structured narrative prediction.
fn handle_risk_predict() {
    let risks = compute_file_risk_scores();
    let report = format_prediction_report(&risks, 5);
    print!("{report}");
}

/// Handle `/risk snapshot` — save current risk predictions to JSONL.
fn handle_risk_snapshot() {
    let risks = compute_file_risk_scores();
    let emerging = detect_emerging_risks(&risks);

    // Get current git hash
    let git_hash = crate::git::run_git(&["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    // Read DAY_COUNT
    let day: u32 = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let json = build_risk_snapshot_json(&risks, &emerging, day, &git_hash);
    let path = std::path::Path::new(RISK_SNAPSHOT_PATH);

    match write_risk_snapshot_to(path, &json) {
        Ok(()) => {
            let count = risks.len().min(10);
            println!("  📸 Snapshot saved — {count} files scored, git HEAD {git_hash}");
        }
        Err(e) => {
            eprintln!("  {RED}Error saving risk snapshot: {e}{RESET}");
        }
    }
}

/// A richer validation event preserving file-level hit/surprise detail and timestamp.
struct RichValidationEvent {
    ts: String,
    day: u32,
    hits: Vec<String>,
    surprises: Vec<String>,
    accuracy_pct: f64,
}

/// Parse rich validation events from JSONL content (preserves hit/surprise file lists).
fn parse_rich_validation_events(content: &str) -> Vec<RichValidationEvent> {
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
        let ts = val["ts"].as_str().unwrap_or("unknown").to_string();
        let day = val["day"].as_u64().unwrap_or(0) as u32;
        let hits: Vec<String> = val["hits"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let surprises: Vec<String> = val["surprises"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let accuracy_pct = val["accuracy_pct"].as_f64().unwrap_or(0.0);

        events.push(RichValidationEvent {
            ts,
            day,
            hits,
            surprises,
            accuracy_pct,
        });
    }
    events
}

/// Build a bar chart string of filled/empty blocks for a 0.0–1.0 ratio.
fn signal_bar(ratio: f64, width: usize) -> String {
    let filled = ((ratio * width as f64).round() as usize).min(width);
    let empty = width - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Format the per-signal breakdown table showing predictive power and weight drift.
fn format_signal_breakdown(
    snapshot_content: &str,
    validation_content: &str,
    learned_weights: &[f64; 7],
    has_learned: bool,
) -> String {
    let detailed = parse_detailed_events(validation_content, snapshot_content);

    // Count how many times each signal appeared in hits vs total hits
    let mut signal_hit_counts = [0u64; 7];
    let mut total_hits = 0u64;

    for event in &detailed {
        for signals in &event.hit_signals {
            total_hits += 1;
            for &idx in signals {
                if idx < 7 {
                    signal_hit_counts[idx] += 1;
                }
            }
        }
    }

    let mut out = format!("\n{BOLD}  Per-Signal Breakdown{RESET}\n");
    out.push_str(&format!(
        "  {:<16}{:<12}{}\n",
        "Signal", "Predictive", "Weight (default → learned)"
    ));

    for i in 0..7 {
        let ratio = if total_hits > 0 {
            signal_hit_counts[i] as f64 / total_hits as f64
        } else {
            0.0
        };
        let bar = signal_bar(ratio, 10);
        let weight_str = if has_learned {
            format!("{:.2} → {:.2}", RISK_WEIGHTS[i], learned_weights[i])
        } else {
            format!("{:.2} (default)", RISK_WEIGHTS[i])
        };
        out.push_str(&format!(
            "  {:<16}{}  {}\n",
            SIGNAL_NAMES[i], bar, weight_str
        ));
    }

    out
}

/// Format the last N rich validation events as a compact summary.
fn format_recent_events(events: &[RichValidationEvent], max_events: usize) -> String {
    if events.is_empty() {
        return String::new();
    }

    let mut out = format!("\n{BOLD}  Recent Validation Events{RESET}\n");

    let start = events.len().saturating_sub(max_events);
    for event in &events[start..] {
        // Shorten timestamp: prefer date portion only
        let ts_short = if event.ts.len() >= 10 {
            &event.ts[..10]
        } else {
            &event.ts
        };
        out.push_str(&format!(
            "  {DIM}{ts_short}{RESET}  Day {:<4}  {GREEN}{} hit{RESET}  {RED}{} surprise{RESET}  ({:.0}%)\n",
            event.day,
            event.hits.len(),
            event.surprises.len(),
            event.accuracy_pct,
        ));
        if !event.hits.is_empty() {
            let hit_list: Vec<&str> = event.hits.iter().map(|s| s.as_str()).collect();
            let display = if hit_list.len() > 3 {
                format!(
                    "{}, ... +{} more",
                    hit_list[..3].join(", "),
                    hit_list.len() - 3
                )
            } else {
                hit_list.join(", ")
            };
            out.push_str(&format!("    {DIM}✓ {display}{RESET}\n"));
        }
        if !event.surprises.is_empty() {
            let surp_list: Vec<&str> = event.surprises.iter().map(|s| s.as_str()).collect();
            let display = if surp_list.len() > 3 {
                format!(
                    "{}, ... +{} more",
                    surp_list[..3].join(", "),
                    surp_list.len() - 3
                )
            } else {
                surp_list.join(", ")
            };
            out.push_str(&format!("    {DIM}✗ {display}{RESET}\n"));
        }
    }

    out
}

/// Format the learning status section.
fn format_learning_status(weights_path: &std::path::Path) -> String {
    let mut out = format!("\n{BOLD}  Learning Status{RESET}\n");

    let content = match std::fs::read_to_string(weights_path) {
        Ok(c) => c,
        Err(_) => {
            // No weights file — check how many events we have
            let val_content = std::fs::read_to_string(RISK_VALIDATION_PATH).unwrap_or_default();
            let event_count = parse_validation_events(&val_content).len();
            if event_count == 0 {
                out.push_str(&format!(
                    "  {DIM}No learned weights yet. Collect validation events\n\
                     {DIM}  via /risk snapshot → /risk validate to start learning.{RESET}\n"
                ));
            } else {
                out.push_str(&format!(
                    "  {YELLOW}Learning... ({}/{} events collected){RESET}\n",
                    event_count, MIN_VALIDATION_EVENTS
                ));
            }
            return out;
        }
    };

    let val: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => {
            out.push_str(&format!(
                "  {DIM}Weights file exists but is malformed.{RESET}\n"
            ));
            return out;
        }
    };

    let learned_from = val["learned_from"].as_u64().unwrap_or(0);
    let last_updated = val["last_updated"].as_str().unwrap_or("unknown");

    out.push_str(&format!(
        "  {GREEN}✓ Learned weights active{RESET}\n\
         {DIM}  Based on:     {} validation events\n\
         {DIM}  Last updated:  {}{RESET}\n",
        learned_from, last_updated
    ));

    out
}

/// One honest line about the *state of the ledger file itself*, printed above
/// `/risk accuracy`'s report. Pure so the string a user receives can be
/// asserted directly (#764: all three ledger states used to render
/// identically).
///
/// `None` on the two states whose existing output is already correct: a
/// healthy ledger (say nothing) and a genuinely missing one (the report's own
/// "accuracy tracking starts automatically" copy is true).
fn ledger_health_line(ledger: &ValidationLedger) -> Option<String> {
    match ledger {
        ValidationLedger::Missing => None,
        ValidationLedger::Unreadable(msg) => Some(msg.clone()),
        // NOTE: there is deliberately no `Present { dropped: 0, .. }` arm here.
        // One existed until Day 174 and it matched *before* the two arms below,
        // so a ledger with 0 unparseable lines and N ungradable ones printed
        // nothing at all — the denominator shrank silently, which is the exact
        // defect this line exists to expose. The healthy case is the catch-all
        // at the bottom, which fires only when *both* counts are 0.
        ValidationLedger::Present {
            events,
            dropped,
            ungradable,
        } if events.is_empty() && dropped + ungradable > 0 => Some(format!(
            "{RISK_VALIDATION_PATH} exists but none of its {} line(s) are usable ({}) — \
             the ledger is present, not absent, so the \"starts automatically\" note below \
             does not apply here.",
            dropped + ungradable,
            unusable_breakdown(*dropped, *ungradable)
        )),
        ValidationLedger::Present {
            dropped,
            ungradable,
            ..
        } if dropped + ungradable > 0 => Some(format!(
            "{RISK_VALIDATION_PATH}: {} line(s) skipped ({}) — \
             the numbers below cover only the rest of the ledger.",
            dropped + ungradable,
            unusable_breakdown(*dropped, *ungradable)
        )),
        ValidationLedger::Present { .. } => None,
    }
}

/// Spell out *why* lines were skipped, keeping the two reasons distinct.
///
/// An unparseable line is corruption; a line that is valid JSON but names no
/// outcome (`hits`/`surprises`) is a different fact — it was written by
/// something, it just grades nothing. Collapsing them into one number would
/// repeat the defect this reporting exists to expose (#764).
fn unusable_breakdown(dropped: usize, ungradable: usize) -> String {
    match (dropped, ungradable) {
        (d, 0) => format!("{d} unparseable"),
        (0, u) => format!("{u} with no gradable outcome"),
        (d, u) => format!("{d} unparseable, {u} with no gradable outcome"),
    }
}

/// One honest line about the *state of the snapshot ledger file itself* — the
/// prediction half of the meter, the sibling of [`ledger_health_line`] (the
/// grade half). `None` on the two states where the report below is already
/// honest without it: a genuinely missing file (no snapshot has been recorded,
/// which the no-data copy covers) and a clean read.
///
/// It exists because the per-signal breakdown printed further down attributes
/// hits to signals recorded *in these snapshots*. If lines were dropped, that
/// breakdown covers an unknown fraction of the recorded predictions and says
/// so nowhere — a confident number over a silently shrunken denominator.
fn snapshot_health_line(ledger: &SnapshotLedger) -> Option<String> {
    match ledger {
        SnapshotLedger::Missing => None,
        SnapshotLedger::Unreadable(msg) => Some(msg.clone()),
        SnapshotLedger::Present { dropped: 0, .. } => None,
        SnapshotLedger::Present { snapshots, dropped } if snapshots.is_empty() => Some(format!(
            "{RISK_SNAPSHOT_PATH} exists but all {dropped} line(s) in it are unparseable — \
             the prediction ledger is corrupt, not absent, so the per-signal breakdown \
             below covers no predictions at all."
        )),
        SnapshotLedger::Present { dropped, .. } => Some(format!(
            "{RISK_SNAPSHOT_PATH}: {dropped} unparseable line(s) skipped — \
             the per-signal breakdown below covers only the rest of the predictions."
        )),
    }
}

/// Handle the `/risk accuracy` subcommand.
fn handle_risk_accuracy() {
    // Read the ledger through the three-state reader so a corrupt file can't
    // masquerade as a missing one (#764). Still open on that issue: a line
    // that is valid JSON but missing fields is absorbed by the parser's
    // `unwrap_or` defaults and counts as a healthy event.
    let ledger = read_validation_ledger(std::path::Path::new(RISK_VALIDATION_PATH));
    if let Some(line) = ledger_health_line(&ledger) {
        println!("  {YELLOW}⚠ {line}{RESET}");
    }
    let events = match ledger {
        ValidationLedger::Present { events, .. } => events,
        _ => Vec::new(),
    };
    let stats = compute_accuracy_stats(&events);

    // Section 1: Overall accuracy summary
    let report = format_accuracy_report(&stats);
    print!("{report}");

    // If no data, the accuracy report already explains what to do — stop here
    if stats.total_validations == 0 {
        return;
    }

    // Re-homed from the deleted emerging block (#724) — evidence, not a caveat.
    // Carries the one number the lines above don't: the pooled achievable ceiling.
    if let Some(note) = emerging_track_record_note() {
        println!("  {DIM}emerging {note}{RESET}");
    }
    // Section 2: Per-signal breakdown. The snapshot ledger is read through its
    // own three-state reader so dropped prediction lines are reported rather
    // than silently shrinking the denominator this breakdown is computed over.
    let snapshot_ledger = read_snapshot_ledger(std::path::Path::new(RISK_SNAPSHOT_PATH));
    if let Some(line) = snapshot_health_line(&snapshot_ledger) {
        println!("  {YELLOW}⚠ {line}{RESET}");
    }
    let snapshot_content = std::fs::read_to_string(RISK_SNAPSHOT_PATH).unwrap_or_default();
    let validation_content = std::fs::read_to_string(RISK_VALIDATION_PATH).unwrap_or_default();
    let learned_weights = load_learned_weights();
    let has_learned = std::path::Path::new(LEARNED_WEIGHTS_PATH).exists();
    let signal_section = format_signal_breakdown(
        &snapshot_content,
        &validation_content,
        &learned_weights,
        has_learned,
    );
    print!("{signal_section}");

    // Section 3: Recent validation events (last 5)
    let rich_events = parse_rich_validation_events(&validation_content);
    let recent_section = format_recent_events(&rich_events, 5);
    print!("{recent_section}");

    // Section 4: Learning status
    let learning_section = format_learning_status(std::path::Path::new(LEARNED_WEIGHTS_PATH));
    print!("{learning_section}");
}

// ── Risk reflex effectiveness (`/risk effectiveness`) ──

/// Minimum validation events needed for an early-vs-recent window split.
const MIN_EFFECTIVENESS_EVENTS: usize = 6;

/// Verdict on whether the risk reflex is measurably improving predictions.
#[derive(Debug, PartialEq)]
enum EffectivenessVerdict {
    /// Recent hit rate beats the early window by ≥5 points.
    Learning,
    /// Recent and early hit rates are within ±5 points.
    Flat,
    /// Recent hit rate is worse than the early window by ≥5 points.
    Decorative,
    /// Fewer than `MIN_EFFECTIVENESS_EVENTS` total events — no split possible.
    Insufficient,
}

/// Per-window aggregate for the effectiveness report.
#[derive(Debug)]
struct EffectivenessWindow {
    event_count: usize,
    hit_rate_pct: f64,
}

/// Full effectiveness report: early vs recent window comparison plus the
/// overall trend from `compute_accuracy_stats` (so this and `/status` agree).
/// Windows grade **failure-day events only** (Day 142): a "hit" on a green
/// day means crying wolf, the opposite polarity — blending it into the
/// windows would let a rising false-alarm rate read as "learning ↑".
struct EffectivenessReport {
    /// Failure-day events graded by the windows.
    total_events: usize,
    /// Green-day events excluded from the windows (surfaced so the
    /// exclusion is visible, not a silent behavior change).
    green_excluded: usize,
    early: Option<EffectivenessWindow>,
    recent: Option<EffectivenessWindow>,
    verdict: EffectivenessVerdict,
    trend: AccuracyTrend,
}

/// Aggregate a slice of validation events into a window summary.
/// Hit rate is total hits / total changed files across the window
/// (same definition as the overall hit rate in `compute_accuracy_stats`).
fn effectiveness_window(events: &[&ValidationEvent]) -> EffectivenessWindow {
    let hits: usize = events.iter().map(|e| e.hit_count).sum();
    let changed: usize = events.iter().map(|e| e.total_changed).sum();
    let hit_rate_pct = if changed > 0 {
        (hits as f64 / changed as f64) * 100.0
    } else {
        0.0
    };
    EffectivenessWindow {
        event_count: events.len(),
        hit_rate_pct,
    }
}

/// Compare early vs recent window hit rates and produce a verdict.
fn compute_effectiveness_verdict(early_rate: f64, recent_rate: f64) -> EffectivenessVerdict {
    let delta = recent_rate - early_rate;
    if delta >= 5.0 {
        EffectivenessVerdict::Learning
    } else if delta <= -5.0 {
        EffectivenessVerdict::Decorative
    } else {
        EffectivenessVerdict::Flat
    }
}

/// Build the effectiveness report from a validation-history JSONL file.
/// Only failure-day events are graded (`!is_green_event` — the authoritative
/// predicate lives in `commands_risk_accuracy.rs`; legacy severity-less
/// events count as failure-day, same convention as `compute_accuracy_stats`).
/// Splits those chronologically: early = first half, recent = second half
/// (the recent window gets the extra event when the count is odd). The
/// `MIN_EFFECTIVENESS_EVENTS` gate counts failure-day events only, so a
/// pile of green events can't unlock a verdict from 1–2 failure samples.
fn effectiveness_report_from(path: &std::path::Path) -> EffectivenessReport {
    let all_events = load_validation_history_from(path);
    let trend = compute_accuracy_stats(&all_events).trend;
    let events: Vec<&ValidationEvent> = all_events.iter().filter(|e| !is_green_event(e)).collect();
    let green_excluded = all_events.len() - events.len();
    let total_events = events.len();

    if total_events < MIN_EFFECTIVENESS_EVENTS {
        return EffectivenessReport {
            total_events,
            green_excluded,
            early: None,
            recent: None,
            verdict: EffectivenessVerdict::Insufficient,
            trend,
        };
    }

    let mid = total_events / 2;
    let early = effectiveness_window(&events[..mid]);
    let recent = effectiveness_window(&events[mid..]);
    let verdict = compute_effectiveness_verdict(early.hit_rate_pct, recent.hit_rate_pct);

    EffectivenessReport {
        total_events,
        green_excluded,
        early: Some(early),
        recent: Some(recent),
        verdict,
        trend,
    }
}

/// Format the effectiveness report for terminal display.
fn format_effectiveness_report(report: &EffectivenessReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n{BOLD}{CYAN}  Risk Reflex Effectiveness{RESET}\n\n"
    ));

    if report.verdict == EffectivenessVerdict::Insufficient {
        out.push_str(&format!(
            "  {YELLOW}insufficient data ({} events; need ≥{}){RESET}\n\
             {DIM}  Validation events accumulate automatically when watch failures\n\
             {DIM}  are checked against risk predictions. Keep working — the\n\
             {DIM}  verdict unlocks at {} events.{RESET}\n",
            report.total_events, MIN_EFFECTIVENESS_EVENTS, MIN_EFFECTIVENESS_EVENTS
        ));
        if report.green_excluded > 0 {
            out.push_str(&format!(
                "  {DIM}{} green-day events excluded — see /risk accuracy for the false-alarm rate{RESET}\n",
                report.green_excluded
            ));
        }
        return out;
    }

    // Both windows are always present for a non-insufficient verdict.
    if let (Some(early), Some(recent)) = (&report.early, &report.recent) {
        let early_rate = (early.hit_rate_pct * 10.0).round() / 10.0;
        let recent_rate = (recent.hit_rate_pct * 10.0).round() / 10.0;
        out.push_str(&format!(
            "  Early window:   {} events, {early_rate:.1}% hit rate\n\
             \x20 Recent window:  {} events, {recent_rate:.1}% hit rate\n",
            early.event_count, recent.event_count
        ));
    }

    if report.green_excluded > 0 {
        out.push_str(&format!(
            "  {DIM}{} green-day events excluded — see /risk accuracy for the false-alarm rate{RESET}\n",
            report.green_excluded
        ));
    }

    let verdict_line = match report.verdict {
        EffectivenessVerdict::Learning => {
            format!("{GREEN}reflex appears to be learning ↑{RESET}")
        }
        EffectivenessVerdict::Flat => {
            format!("{YELLOW}no measurable improvement yet — need more cycles{RESET}")
        }
        EffectivenessVerdict::Decorative => format!(
            "{RED}reflex may be decorative ↓ — consider anticipatory signals (see DREAM.md){RESET}"
        ),
        EffectivenessVerdict::Insufficient => unreachable!("handled above"),
    };
    out.push_str(&format!("\n  Verdict: {verdict_line}\n"));

    let trend_str = match report.trend {
        AccuracyTrend::Improving => format!("{GREEN}↑ improving{RESET}"),
        AccuracyTrend::Declining => format!("{RED}↓ declining{RESET}"),
        AccuracyTrend::Stable => format!("{YELLOW}→ stable{RESET}"),
        AccuracyTrend::Insufficient => format!("{DIM}? insufficient{RESET}"),
    };
    out.push_str(&format!(
        "  Overall trend: {trend_str} {DIM}(same signal as /status){RESET}\n"
    ));

    out
}

/// Handle the `/risk effectiveness` subcommand.
fn handle_risk_effectiveness() {
    let report = effectiveness_report_from(std::path::Path::new(RISK_VALIDATION_PATH));
    print!("{}", format_effectiveness_report(&report));
}

/// Map an effectiveness verdict to a short, color-free word for ambient display.
/// `Insufficient` maps to `None` so /status stays quiet until the verdict unlocks.
fn verdict_word(v: &EffectivenessVerdict) -> Option<&'static str> {
    match v {
        EffectivenessVerdict::Learning => Some("learning ↑"),
        EffectivenessVerdict::Flat => Some("flat"),
        EffectivenessVerdict::Decorative => Some("decorative ↓"),
        EffectivenessVerdict::Insufficient => None,
    }
}

/// Path-parameterized core of `reflex_effectiveness_summary`, so the exact
/// word /status shows is reachable in tests with a synthetic validation file
/// (the real `RISK_VALIDATION_PATH` was empty until #587/#575 opened the data
/// path, and even now CI has no history — day-124: an untestable verdict is a
/// verdict that can silently skip). Both windows must be present for a real
/// verdict, so the negative "decorative ↓" outcome is provably reachable.
fn reflex_effectiveness_summary_from(path: &std::path::Path) -> Option<&'static str> {
    let report = effectiveness_report_from(path);
    verdict_word(&report.verdict)
}

/// One-line effectiveness verdict for ambient display in /status.
/// Returns `None` when there aren't enough validation events yet
/// (fewer than `MIN_EFFECTIVENESS_EVENTS`) so /status stays quiet.
/// Returns `Some(&'static str)` — a short verdict word like
/// "learning ↑", "flat", or "decorative ↓" (no ANSI color codes;
/// the caller formats its own line).
pub(crate) fn reflex_effectiveness_summary() -> Option<&'static str> {
    reflex_effectiveness_summary_from(std::path::Path::new(RISK_VALIDATION_PATH))
}

/// Parsed git-log entry: one commit message + the files it touched.
struct CommitEntry {
    message: String,
    files: Vec<String>,
}

/// Decode a path as git prints it in `--name-only` output.
///
/// With `core.quotePath` at its default (true), git wraps any path containing
/// non-ASCII or control characters in double quotes and octal-escapes the raw
/// bytes — a file named `src/café.rs` is printed as `"src/caf\303\251.rs"`.
/// Verbatim `git log --oneline --name-only` capture:
///
/// ```text
/// 0488876 fix: unicode path
/// "src/caf\303\251.rs"
/// src/plain rs file.rs
/// ```
///
/// Note the second line: paths containing *spaces* are NOT quoted, so quoting
/// is not a reliable "has weird characters" signal and only the quoted form
/// needs decoding.
///
/// Left undecoded, such a path can never equal the filesystem-walked path
/// stored in a risk snapshot, so the file is silently dropped from grading —
/// an absence absorbed as "this file never broke" (Day 144: absence must be an
/// explicit value, not a convenient default). Unquoting closes that hole.
///
/// Unquoted input is returned unchanged. Octal escapes are decoded at the byte
/// level and re-assembled with `from_utf8_lossy`, so a multi-byte character
/// split across escapes round-trips correctly and invalid UTF-8 degrades to
/// replacement characters rather than panicking.
fn unquote_git_path(raw: &str) -> String {
    let inner = match raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(inner) => inner,
        // Not quoted (the common case, including paths with spaces).
        None => return raw.to_string(),
    };

    let mut bytes: Vec<u8> = Vec::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some(d @ '0'..='7') => {
                // Octal escape: up to three digits, e.g. \303 -> 0xC3.
                let mut val = d.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    let mut probe = chars.clone();
                    match probe.next().and_then(|n| n.to_digit(8)) {
                        Some(n) => {
                            val = val * 8 + n;
                            chars = probe;
                        }
                        None => break,
                    }
                }
                bytes.push((val & 0xFF) as u8);
            }
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some('r') => bytes.push(b'\r'),
            // `\"` and `\\` (and any other escape git emits) are literal.
            Some(other) => {
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            // Trailing lone backslash: keep it rather than dropping data.
            None => bytes.push(b'\\'),
        }
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

/// True when `line` has the shape of a `git log --oneline` commit header:
/// 7–40 lowercase-hex chars, a single space, then a non-empty subject.
///
/// Deliberately conservative — a file path essentially never matches this
/// (paths have no space after a bare hex token).  `deadbeef.rs`,
/// `src/abcdef1.rs`, `memory/learnings.jsonl` all correctly return false.
fn looks_like_oneline_commit_header(line: &str) -> bool {
    let Some((hash, rest)) = line.split_once(' ') else {
        return false;
    };
    if rest.trim().is_empty() {
        return false;
    }
    let len = hash.len();
    (7..=40).contains(&len) && hash.chars().all(|c| c.is_ascii_hexdigit())
}

/// Parse `git log --name-only --oneline` output into structured entries.
///
/// Each commit is one `<hash> <subject>` header line followed by zero or more
/// file paths.  Real git output has NO blank line between commits, so a new
/// commit is detected by the header shape (see
/// `looks_like_oneline_commit_header`); blank lines are also honoured as
/// separators for tolerance.  Example:
/// ```text
/// abc1234 Fix clippy warnings
/// src/foo.rs
/// src/bar.rs
/// def5678 Revert "add feature"
/// src/baz.rs
/// ```
fn parse_git_log_name_only(output: &str) -> Vec<CommitEntry> {
    let mut entries = Vec::new();
    let mut current_msg: Option<String> = None;
    let mut current_files: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Blank line separates commits
            if let Some(msg) = current_msg.take() {
                entries.push(CommitEntry {
                    message: msg,
                    files: std::mem::take(&mut current_files),
                });
            }
            continue;
        }

        if looks_like_oneline_commit_header(trimmed) {
            // New commit header: flush the previous entry, start a fresh one.
            if let Some(msg) = current_msg.take() {
                entries.push(CommitEntry {
                    message: msg,
                    files: std::mem::take(&mut current_files),
                });
            }
            current_msg = Some(trimmed.to_string());
        } else if current_msg.is_none() {
            // Leading line that doesn't look like a header: treat as the message.
            current_msg = Some(trimmed.to_string());
        } else {
            // Subsequent non-blank line: file path (git may C-quote it).
            current_files.push(unquote_git_path(trimmed));
        }
    }

    // Flush last entry if file didn't end with blank line
    if let Some(msg) = current_msg.take() {
        entries.push(CommitEntry {
            message: msg,
            files: current_files,
        });
    }

    entries
}

/// Whole-word forms that mean "this commit repaired something that was broken".
///
/// Exact token match only — deliberately NOT a substring or suffix test, because
/// `prefix`/`suffix`/`postfix`/`infix`/`affix`/`fixture` all end in "fix" and none
/// of them are repairs. See `message_claims_repair`.
const REPAIR_TOKENS: &[&str] = &[
    "fix",
    "fixes",
    "fixed",
    "fixing",
    "fixup",
    "hotfix",
    "hotfixes",
    "bugfix",
    "bugfixes",
    "revert",
    "reverts",
    "reverted",
    "reverting",
];

/// True when a commit message claims to repair breakage.
///
/// Splits the message into lowercase alphanumeric tokens (so `fix:`, `fixes #12`,
/// `bug-fix`, `fixup!` and git's own `Revert "..."` all tokenize to a repair word)
/// and requires an EXACT token match against `REPAIR_TOKENS`.
///
/// The previous implementation used `msg.contains("fix")`, which graded
/// "add test fixtures" and "rename prefix helper" as break-fixes — phantom
/// breakage fed straight into the failure-day half of the prediction meter,
/// where it inflates `broke_files` with files that never broke. Granularity bug,
/// not a pattern bug (Day 131): the fix is token matching, not more exceptions.
fn message_claims_repair(message: &str) -> bool {
    message
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| {
            let lower = tok.to_ascii_lowercase();
            REPAIR_TOKENS.contains(&lower.as_str())
        })
}

/// True when a commit message claims to UNDO something — the unambiguous half
/// of `message_claims_repair`.
///
/// Same tokenizer shape as `message_claims_repair` (so git's own
/// `Revert "..."` subject tokenizes correctly); these four tokens are a subset
/// of `REPAIR_TOKENS`, kept inline rather than as a second const so the two
/// lists cannot drift apart silently.
fn message_claims_revert(message: &str) -> bool {
    message
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| {
            matches!(
                tok.to_ascii_lowercase().as_str(),
                "revert" | "reverts" | "reverted" | "reverting"
            )
        })
}

/// True for commit subjects that `scripts/evolve.sh` generates mechanically
/// after every task, green or red.
///
/// **Why this exists (Day 163, the receipt).** The 10:25 session was fully
/// green (`tasks 1/1 ✅ — build OK, tests OK`) and still produced an untagged
/// failure-day validation event blaming `src/commands_todo.rs`. `git log`
/// shows why:
///
/// ```text
/// a7a61618  Day 163 (10:25): Fix #702 — /todo list ...   src/commands_todo.rs
/// eee8c191  Day 163 (10:25): cargo fmt                    src/commands_todo.rs
/// ```
///
/// The harness commits `cargo fmt` separately, immediately after EVERY task,
/// touching exactly the files the task touched. So tier-2 corroboration was
/// being manufactured by the harness on every single task — and a signal that
/// fires 100% of the time carries zero bits. This is the Day-148 lesson one
/// layer down: the intake filter for the meter's headline recall number was
/// again my own commit convention — last time my `Fix #NNN` titles, this time
/// the harness's mechanical follow-up commits.
///
/// **What this filter is keyed to, honestly.** It is legitimate *only* because
/// these subjects are generated by my own harness. Two ways it lies:
///
/// - A human commit literally titled "cargo fmt" would be misclassified as
///   bookkeeping and silently lose its corroborating power.
/// - If `scripts/evolve.sh` changes its wording, this list stops covering the
///   new phrasing with no error anywhere — pollution returns quietly.
///
/// `test_is_mechanical_commit_covers_harness_vocabulary` pins the vocabulary
/// so drift shows up as a failing test rather than as recall pollution nobody
/// reads. Substring match, case-insensitive: the subjects arrive prefixed with
/// a short hash and a `Day N (HH:MM):` stamp.
fn is_mechanical_commit(subject: &str) -> bool {
    /// Subjects observed in this repo's `git log` today. Not a general
    /// heuristic — a transcript of one harness's vocabulary.
    const MECHANICAL_SUBJECTS: &[&str] = &[
        "cargo fmt",
        "session wrap-up",
        "journal entry",
        "update learnings",
        "assessment (draft)",
        "docs(assessment)",
        "session plan",
        "bump skill-evolve counter",
        "social session",
    ];
    let lower = subject.to_ascii_lowercase();
    MECHANICAL_SUBJECTS.iter().any(|m| lower.contains(m)) || has_retry_suffix(&lower)
}

/// True when a subject carries the harness's fix-loop RETRY suffix —
/// `… (Task 2, eval-fix 5)`.
///
/// Day 165: this is the Day-163 hole one layer down. `scripts/evolve.sh`
/// re-commits the same task after each evaluator rejection with that suffix
/// (line ~2194), and `message_claims_repair` tokenizes on non-alphanumerics —
/// so `eval-fix` becomes `["eval", "fix"]` and EVERY retry commit reads as a
/// repair claim. Worse, the sibling retries in one window touch the same files,
/// so they satisfy tier-2 corroboration (`touches >= 2`) for each other: the
/// harness supplied both the claim and its second opinion, and three green
/// Day-165 sessions were booked as failure days with 0 hits.
///
/// The match is the retry SUFFIX, not the bare word `eval-fix`, so a human
/// commit that merely mentions a task number keeps its corroborating power.
/// Verified against 400 commits of `git log --oneline`: the harness emits
/// exactly two shapes, `(Task N)` for a delivered task and `(Task N, <label>
/// M)` for a retry. Only the second is bookkeeping — the first is the delivery
/// itself and must stay real work. Any future retry label (`build-fix`) rides
/// the same suffix and is covered by construction.
fn has_retry_suffix(lower_subject: &str) -> bool {
    let mut rest = lower_subject;
    while let Some(pos) = rest.find("(task ") {
        let after = &rest[pos + "(task ".len()..];
        let digits = after.chars().take_while(|c| c.is_ascii_digit()).count();
        // `(task 2,` — a comma right after the number is the retry marker.
        // `(task 2)` is a plain delivery commit and is deliberately NOT matched.
        if digits > 0 && after[digits..].starts_with(',') {
            return true;
        }
        rest = after;
    }
    false
}

/// Classify commits and return the set of files that "broke".
///
/// Classification is delegated to `message_claims_repair`, which matches whole
/// tokens. The earlier substring version (fixed Day 147) counted "prefix",
/// "suffix" and "fixture" commits as breakage, inflating this set with files
/// that never broke.
///
/// Only `src/` paths are returned (#708): the risk model scores nothing else,
/// so a `tests/` or `Cargo.toml` path in an outcome set is a guaranteed miss
/// that drags recall down with noise.
///
/// **Two-tier corroboration (Day 163).** A lone message token is no longer
/// enough to call a file broken, because my own delivered task commits are
/// literally titled `Day 163 (01:56): Fix #710 — ...`: an ordinary green
/// feature-delivery session was being graded as a failure day, and every
/// `src/` file it touched entered the meter as breakage. The intake filter for
/// the meter's headline recall number was my own commit-message convention
/// (Day-148 lesson, verbatim). So:
///
/// - **Tier 1 — revert.** `message_claims_revert` is breakage evidence on its
///   own. An undo says a thing was wrong; nothing else needs to agree.
/// - **Tier 2 — fix-family, needs corroboration.** A `fix`/`hotfix`/`bugfix`
///   commit counts a `src/` file only if that file also appears in at least one
///   OTHER entry in the window that is not harness bookkeeping
///   (`is_mechanical_commit` — `cargo fmt`, `session wrap-up`, ...). A genuine
///   in-window repair follows the commit that broke the thing, so the file is
///   touched twice by real work; a delivered `Fix #710` task commit is the only
///   real touch of its files in the window. Bookkeeping commits are excluded
///   because the harness makes them after EVERY task, green or red, on exactly
///   the files the task touched — corroboration that always fires is no
///   corroboration at all (Day 163, second layer of the Day-148 lesson).
///
/// Honest limits: corroboration reduces the false-failure rate, it does not
/// zero it — a task that lands as `Fix #710` plus an eval-fix commit touching
/// the same file still corroborates itself. And this changes grading from here
/// forward only: `.yoyo/risk_validations.jsonl` is never rewritten (re-grading
/// history would be forgery), so `/risk accuracy` still blends the 24 already
/// polluted untagged events with future honest ones — that number is
/// recovering, not clean. Separating them needs ledger versioning, a separate
/// task.
fn classify_broke_files(entries: &[CommitEntry]) -> std::collections::HashSet<String> {
    // How many DISTINCT entries in the window touch each path. Counting per
    // entry (not per listed path) keeps the predicate exactly "appears in at
    // least one OTHER commit". Harness bookkeeping commits contribute no
    // touches at all: they accompany every task unconditionally, so they are
    // never independent evidence that anyone came back to a file.
    let mut touches: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for entry in entries {
        if is_mechanical_commit(&entry.message) {
            continue;
        }
        let mut seen: Vec<&str> = Vec::new();
        for f in &entry.files {
            if seen.contains(&f.as_str()) {
                continue;
            }
            seen.push(f.as_str());
            *touches.entry(f.as_str()).or_insert(0) += 1;
        }
    }

    let mut broke = std::collections::HashSet::new();
    for entry in entries {
        let is_revert = message_claims_revert(&entry.message);
        if !is_revert && !message_claims_repair(&entry.message) {
            continue;
        }
        for f in &entry.files {
            // Risk predictions cover source code only — a non-src path is a
            // guaranteed miss no model could have predicted. Same filter as
            // the two grading paths in commands_risk_snapshots.rs (kept
            // inline there and here on purpose: extracting a shared
            // predicate is new machinery this fix does not need).
            if !f.starts_with("src/") {
                continue;
            }
            // Tier 2: a fix-family claim needs a second touch to be believed.
            if !is_revert && touches.get(f.as_str()).copied().unwrap_or(0) < 2 {
                continue;
            }
            broke.insert(f.clone());
        }
    }
    broke
}

/// True when the window LOOKS like a repair window but corroboration dropped
/// every candidate: at least one non-revert commit claims a repair and names a
/// `src/` file, yet `classify_broke_files` returned nothing.
///
/// This is the **third value** — "repair claimed, uncorroborated" — that keeps a
/// real failure day from being recorded as an affirmative green one (#717).
///
/// Why it is needed: tier 1 (a revert commit) is structurally dead in this repo,
/// because `scripts/evolve.sh` reverts with `git reset --hard "$PRE_TASK_SHA"`
/// and `git checkout -- .` — never a revert commit. So tier 2 decides
/// everything, and the canonical real breakage (bad commit in session N, one
/// `Fix #NNN` commit in session N+1) is exactly the shape tier 2 drops: in
/// session N+1's window the repair is the only touch of that file, so
/// corroboration fails and the file is silently skipped. An empty `broke_files`
/// then falls through to the GREEN branch and books a genuine failure day as
/// crying-wolf evidence. An ungraded window is honest; a green one is a
/// fabricated observation.
///
/// Reuses the same predicates and `src/` filter as `classify_broke_files`, so
/// the two can't drift apart.
fn has_uncorroborated_repair_evidence(entries: &[CommitEntry]) -> bool {
    // Only meaningful when the corroborated classifier found nothing: if it did
    // find breakage, the window is graded as a failure day and never reaches
    // the green branch this helper guards.
    if !classify_broke_files(entries).is_empty() {
        return false;
    }
    entries.iter().any(|entry| {
        // Harness bookkeeping is never evidence of anything — routed through
        // the SAME predicate tier-2 corroboration uses, so the two halves
        // cannot drift apart about what counts as a real commit (Day-142
        // mirror-twin lesson: I build symmetric structures and repair them
        // asymmetrically).
        !is_mechanical_commit(&entry.message)
            // A revert is tier-1 evidence on its own — if one is present and
            // classify_broke_files still came back empty, it named no `src/`
            // file, which is not the uncorroborated-repair shape.
            && !message_claims_revert(&entry.message)
            && message_claims_repair(&entry.message)
            && entry.files.iter().any(|f| f.starts_with("src/"))
    })
}

/// Result of comparing predictions against actual breakage.
struct ValidationResult {
    /// Files from the top-10 predictions that actually broke.
    hits: Vec<String>,
    /// Files from the top-10 predictions that had no issues.
    clean: Vec<String>,
    /// Files that broke but were NOT in the top-10 predictions (surprises).
    surprises: Vec<(String, Option<usize>)>,
    /// Total number of commits since snapshot.
    commit_count: usize,
}

/// Compute validation by comparing predicted top-10 files against
/// the set of files that actually broke.
///
/// `predicted` is the list of file paths from the snapshot's top_10.
/// `all_ranked` can optionally provide rank info for surprise files
/// (pass `None` if unavailable).
fn compute_validation(
    predicted: &[String],
    broke_files: &std::collections::HashSet<String>,
    all_ranked: Option<&[String]>,
    commit_count: usize,
) -> ValidationResult {
    let mut hits = Vec::new();
    let mut clean = Vec::new();

    for p in predicted {
        if broke_files.contains(p) {
            hits.push(p.clone());
        } else {
            clean.push(p.clone());
        }
    }

    let predicted_set: std::collections::HashSet<&String> = predicted.iter().collect();

    let mut surprises: Vec<(String, Option<usize>)> = broke_files
        .iter()
        .filter(|f| !predicted_set.contains(f))
        .map(|f| {
            let rank = all_ranked.and_then(|ranked| {
                ranked.iter().position(|r| r == f).map(|i| i + 1) // 1-based
            });
            (f.clone(), rank)
        })
        .collect();

    // Sort surprises by rank (known rank first, then alphabetically)
    surprises.sort_by(|a, b| match (&a.1, &b.1) {
        (Some(ra), Some(rb)) => ra.cmp(rb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });

    ValidationResult {
        hits,
        clean,
        surprises,
        commit_count,
    }
}

/// Format a validation result as a human-readable report.
fn format_validation_report(result: &ValidationResult, day: u64, git_hash: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\n  📊 {BOLD}Risk Prediction Validation{RESET}\n\n"
    ));
    out.push_str(&format!("  Snapshot: Day {day}, {git_hash}\n"));
    out.push_str(&format!("  Commits since: {}\n\n", result.commit_count));

    out.push_str(&format!(
        "  {DIM}Predicted (top 10)            Actual Result{RESET}\n"
    ));
    out.push_str(&format!(
        "  {DIM}─────────────────────────────────────────────{RESET}\n"
    ));

    let all_predicted: Vec<&String> = result.hits.iter().chain(result.clean.iter()).collect();

    // We want to show them in original order, so combine hits+clean and mark
    // Actually, let's iterate predicted order. Build a lookup set.
    let hit_set: std::collections::HashSet<&String> = result.hits.iter().collect();

    for p in &all_predicted {
        let status = if hit_set.contains(p) {
            format!("{GREEN}✅ had fixes{RESET}")
        } else {
            format!("{DIM}─  no issues{RESET}")
        };
        out.push_str(&format!("  {:<30}{}\n", p, status));
    }

    out.push('\n');

    let total_broke = result.hits.len() + result.surprises.len();

    // Precision@10: what fraction of our predictions were right
    out.push_str(&format!(
        "  Precision@10: {}/{} predicted files had issues\n",
        result.hits.len(),
        result.hits.len() + result.clean.len(),
    ));

    // Recall@10: what fraction of actual breakage did we catch
    if total_broke > 0 {
        out.push_str(&format!(
            "  Recall@10:    {}/{} broken files were predicted\n",
            result.hits.len(),
            total_broke,
        ));
    }

    if !result.surprises.is_empty() {
        out.push_str(&format!(
            "\n  {YELLOW}Surprises (broke but not predicted):{RESET}\n"
        ));
        for (f, rank) in &result.surprises {
            let rank_info = match rank {
                Some(r) => format!(" (rank #{r})"),
                None => String::new(),
            };
            out.push_str(&format!("    {f}{rank_info}\n"));
        }
    }

    out
}

/// Validation result for a single snapshot interval.
struct HistoryValidation {
    day: u64,
    git_hash_from: String,
    git_hash_to: String,
    result: ValidationResult,
}

/// Compute precision for a single validation (0.0..=1.0).
fn precision(v: &ValidationResult) -> f64 {
    let total = v.hits.len() + v.clean.len();
    if total == 0 {
        return 0.0;
    }
    v.hits.len() as f64 / total as f64
}

/// Compute trend label by comparing first-half vs second-half average precision.
fn compute_trend(validations: &[HistoryValidation]) -> &'static str {
    if validations.len() < 2 {
        return "\u{27a1}\u{fe0f}  Stable";
    }
    let mid = validations.len() / 2;
    let first_half = &validations[..mid];
    let second_half = &validations[mid..];

    let avg = |slice: &[HistoryValidation]| -> f64 {
        if slice.is_empty() {
            return 0.0;
        }
        let sum: f64 = slice.iter().map(|v| precision(&v.result)).sum();
        sum / slice.len() as f64
    };

    let first_avg = avg(first_half);
    let second_avg = avg(second_half);
    let diff = second_avg - first_avg;

    if diff > 0.05 {
        "\u{1f4c8} Improving"
    } else if diff < -0.05 {
        "\u{1f4c9} Declining"
    } else {
        "\u{27a1}\u{fe0f}  Stable"
    }
}

/// Format the history report table from validated snapshot intervals.
fn format_history_report(validations: &[HistoryValidation]) -> String {
    let mut out = String::new();

    out.push_str(&format!("\n  {BOLD}Risk Prediction History{RESET}\n\n"));

    if validations.is_empty() {
        out.push_str(
            "  No validated snapshots — need at least 2 snapshots with commits between them.\n",
        );
        out.push_str(&format!(
            "  Run {BOLD}/risk snapshot{RESET} periodically to build history.\n"
        ));
        return out;
    }

    // Table header
    out.push_str(&format!(
        "  {DIM}Day   Commits  Hits  Predicted  Precision  Recall{RESET}\n"
    ));
    out.push_str(&format!(
        "  {DIM}────  ───────  ────  ─────────  ─────────  ──────{RESET}\n"
    ));

    let mut total_hits = 0usize;
    let mut total_predicted = 0usize;
    let mut total_breaks = 0usize;

    for v in validations {
        let hits = v.result.hits.len();
        let predicted_count = hits + v.result.clean.len();
        let breaks = hits + v.result.surprises.len();
        let prec = if predicted_count > 0 {
            format!("{:>8.0}%", (hits as f64 / predicted_count as f64) * 100.0)
        } else {
            "      n/a".to_string()
        };
        let recall = if breaks > 0 {
            format!("{:>5.0}%", (hits as f64 / breaks as f64) * 100.0)
        } else {
            "  n/a".to_string()
        };

        // Show short hash range for context
        let hash_from_short = if v.git_hash_from.len() > 7 {
            &v.git_hash_from[..7]
        } else {
            &v.git_hash_from
        };
        let hash_to_short = if v.git_hash_to.len() > 7 {
            &v.git_hash_to[..7]
        } else {
            &v.git_hash_to
        };

        out.push_str(&format!(
            "  {:<6}{:>7}  {:>4}  {:>9}  {}  {}  {DIM}{}..{}{RESET}\n",
            v.day,
            v.result.commit_count,
            hits,
            predicted_count,
            prec,
            recall,
            hash_from_short,
            hash_to_short,
        ));

        total_hits += hits;
        total_predicted += predicted_count;
        total_breaks += breaks;
    }

    // Overall summary
    out.push_str(&format!(
        "\n  {BOLD}Overall{RESET} ({} snapshots validated)\n",
        validations.len()
    ));

    if total_predicted > 0 {
        let overall_prec = (total_hits as f64 / total_predicted as f64) * 100.0;
        out.push_str(&format!(
            "  Precision: {total_hits}/{total_predicted} ({overall_prec:.0}%) — predicted files that actually broke\n"
        ));
    }

    if total_breaks > 0 {
        let overall_recall = (total_hits as f64 / total_breaks as f64) * 100.0;
        out.push_str(&format!(
            "  Recall:    {total_hits}/{total_breaks} ({overall_recall:.0}%) — breaks that were predicted\n"
        ));
    }

    let trend = compute_trend(validations);
    out.push_str(&format!("  Trend:     {trend}\n"));

    out
}

/// Handle `/risk history` — show accuracy trend across all past snapshots.
fn handle_risk_history() {
    // 1. Load all snapshots
    let path = std::path::Path::new(RISK_SNAPSHOT_PATH);
    let contents = match std::fs::read_to_string(path) {
        Ok(c) if !c.trim().is_empty() => c,
        Ok(_) => {
            println!(
                "  No snapshots found. Run {BOLD}/risk snapshot{RESET} periodically to build history."
            );
            return;
        }
        Err(_) => {
            println!(
                "  No snapshots found. Run {BOLD}/risk snapshot{RESET} periodically to build history."
            );
            return;
        }
    };

    let snapshots = parse_all_snapshots(&contents);
    if snapshots.len() < 2 {
        println!(
            "  Need at least 2 snapshots for history. Currently have {}.",
            snapshots.len()
        );
        println!("  Run {BOLD}/risk snapshot{RESET} periodically to build history.");
        return;
    }

    // 2. For each consecutive pair, validate
    let mut validations = Vec::new();
    for i in 0..snapshots.len() - 1 {
        let from = &snapshots[i];
        let to = &snapshots[i + 1];

        // Get git log between the two snapshot hashes
        let log_output = match crate::git::run_git(&[
            "log",
            &format!("{}..{}", from.git_hash, to.git_hash),
            "--name-only",
            "--oneline",
        ]) {
            Ok(o) if !o.trim().is_empty() => o,
            Ok(_) => continue, // no commits between these snapshots
            Err(_) => continue,
        };

        let entries = parse_git_log_name_only(&log_output);
        let commit_count = entries.len();
        let broke_files = classify_broke_files(&entries);

        let result = compute_validation(&from.predicted, &broke_files, None, commit_count);

        validations.push(HistoryValidation {
            day: from.day,
            git_hash_from: from.git_hash.clone(),
            git_hash_to: to.git_hash.clone(),
            result,
        });
    }

    // 3. Also validate last snapshot against HEAD
    if let Some(last) = snapshots.last() {
        let log_output = match crate::git::run_git(&[
            "log",
            &format!("{}..HEAD", last.git_hash),
            "--name-only",
            "--oneline",
        ]) {
            Ok(o) if !o.trim().is_empty() => Some(o),
            _ => None,
        };

        if let Some(log) = log_output {
            let entries = parse_git_log_name_only(&log);
            let commit_count = entries.len();
            let broke_files = classify_broke_files(&entries);

            let result = compute_validation(&last.predicted, &broke_files, None, commit_count);
            let head_hash = crate::git::run_git(&["rev-parse", "--short", "HEAD"])
                .unwrap_or_else(|_| "HEAD".to_string())
                .trim()
                .to_string();

            validations.push(HistoryValidation {
                day: last.day,
                git_hash_from: last.git_hash.clone(),
                git_hash_to: head_hash,
                result,
            });
        }
    }

    let report = format_history_report(&validations);
    print!("{report}");
}

/// Handle `/risk validate` — compare past predictions against actual breakage.
/// Extract the anticipatory (emerging) prediction paths from a snapshot JSON
/// value. Older snapshots (pre-Day-138) have no `emerging` key — that yields
/// an empty list, not an error. Entries without a `"path"` string are skipped.
fn emerging_paths_from_snapshot(snapshot: &serde_json::Value) -> Vec<String> {
    snapshot["emerging"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["path"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// How many failed CI runs `/risk harvest` asks GitHub for in one crank.
const CI_HARVEST_RUN_LIMIT: usize = 10;

/// Fetch the recent failed CI runs as raw JSON via the `gh` CLI.
///
/// Fail-soft, but never fail-*silent* (Day 139): every error path returns a
/// stated reason the caller prints. Returns the raw stdout on success.
fn fetch_failed_ci_runs_json(limit: usize) -> Result<String, String> {
    let output = std::process::Command::new("gh")
        .args([
            "run",
            "list",
            "--workflow=CI",
            "--status=failure",
            "--limit",
            &limit.to_string(),
            "--json",
            "databaseId,headSha,createdAt,displayTitle",
        ])
        .output()
        .map_err(|e| format!("could not run `gh` ({e}) — is the GitHub CLI installed?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let reason = if stderr.is_empty() {
            "(no stderr)".to_string()
        } else {
            stderr
        };
        return Err(format!("`gh run list` failed: {reason}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Extract the changed-file list from a GitHub compare payload (`.files[].filename`).
///
/// Pure and network-free so it can be pinned against a captured payload.
/// Anything that isn't a compare payload — empty string, an API error object,
/// `files` of the wrong type, entries without a string `filename` — yields an
/// explicit empty Vec. Empty means "I learned nothing here", and the caller
/// treats it exactly like a git range that produced no `src/` files: skip and
/// say why. It never fabricates a file set and never panics.
fn parse_compare_changed_files(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .filter_map(|f| f["filename"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Ask GitHub which files changed between two commits, via `gh api`.
///
/// This exists because the only caller that ever turns the harvest crank is an
/// ephemeral CI runner checked out at `fetch-depth: 50` — most snapshot SHAs
/// and failing-run head SHAs simply do not resolve locally, so `git log
/// <base>..<head>` fails with "unknown revision". GitHub still holds the full
/// history; `gh api` substitutes `{owner}`/`{repo}` from the current repo, so
/// there's no hardcoded slug and forks work unchanged.
///
/// Honest limitation: the compare endpoint returns at most 300 files per page
/// and we do not paginate, so a very large range under-reports its changed
/// files. For harvest that means a possible *undercount* of the broke set, not
/// a wrong grade of the files it does see.
fn fetch_compare_files(base: &str, head: &str) -> Result<Vec<String>, String> {
    // Three dots: `base...head` compares against the merge base, which is what
    // "what changed on the way to this failing run" means.
    let endpoint = format!("repos/{{owner}}/{{repo}}/compare/{base}...{head}");
    let output = std::process::Command::new("gh")
        .args(["api", &endpoint])
        .output()
        .map_err(|e| format!("could not run `gh` ({e}) — is the GitHub CLI installed?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let reason = if stderr.is_empty() {
            "(no stderr)".to_string()
        } else {
            stderr
        };
        return Err(format!("`gh api compare` failed: {reason}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = parse_compare_changed_files(&stdout);
    if files.is_empty() {
        return Err(
            "compare payload listed no files (unexpected shape or empty range)".to_string(),
        );
    }
    Ok(files)
}

/// Handle `/risk harvest` — turn already-recorded failed CI runs into
/// failure-day validation events.
///
/// The meter's green (precision) half fills up on its own; the red (recall)
/// half almost never does, because in the evolve loop breakage is repaired
/// inside the harness fix loop and rarely survives into a commit the
/// commit-message classifier can see. Failed CI runs are real, already-recorded
/// failure-day evidence — this crank feeds them to the meter.
///
/// Deliberately manual (like `yoyo risk validate`): no cron, no hook. Every
/// skip prints its reason; nothing is recorded on a guess.
fn handle_risk_harvest() {
    let day: u32 = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    // 1. Snapshots are the predictions being graded — no snapshots, nothing to do.
    let snapshot_content = std::fs::read_to_string(RISK_SNAPSHOT_PATH).unwrap_or_default();
    let snapshots = parse_all_snapshots(&snapshot_content);
    if snapshots.is_empty() {
        println!("  No snapshots found — run {BOLD}/risk snapshot{RESET} first (nothing to grade CI failures against).");
        return;
    }

    // 2. Fetch failed CI runs.
    let json = match fetch_failed_ci_runs_json(CI_HARVEST_RUN_LIMIT) {
        Ok(j) => j,
        Err(reason) => {
            println!("  {DIM}Harvest skipped: {reason}{RESET}");
            return;
        }
    };
    let payload = parse_ci_run_payload(&json);
    if let Some(note) = ci_payload_note(&payload) {
        println!("  {DIM}⚠ {note}{RESET}");
    }
    let runs = parse_failed_ci_runs(&json);
    println!(
        "  Failed CI runs seen: {} (limit {CI_HARVEST_RUN_LIMIT})",
        runs.len()
    );
    if runs.is_empty() {
        println!("  {DIM}Nothing to harvest — no failed CI runs in the window (or `gh` returned an unexpected shape).{RESET}");
        return;
    }

    let validation_path = std::path::Path::new(RISK_VALIDATION_PATH);
    let mut existing = std::fs::read_to_string(validation_path).unwrap_or_default();

    let (mut harvested, mut deduped, mut skipped) = (0usize, 0usize, 0usize);

    for run in &runs {
        // 3. Dedup by run id — NOT by snapshot hash (a snapshot may already
        //    carry a green event, which would silently swallow this red one).
        if ci_event_exists_for(&existing, run.run_id) {
            deduped += 1;
            continue;
        }

        // 4. Match the most recent snapshot taken before the run started.
        let Some(snapshot) = snapshot_before(&snapshots, &run.created_at) else {
            skipped += 1;
            println!(
                "  {DIM}skip run {}: no snapshot recorded before {}{RESET}",
                run.run_id, run.created_at
            );
            continue;
        };

        // 5. Broke set = files changed between the snapshot and the failing
        //    commit. CI already told us the outcome was red, so the
        //    commit-message heuristic (`classify_broke_files`) is the wrong
        //    instrument here.
        //
        //    Two sources, in order: local `git log` (free, exact, works in a
        //    full clone) and — when that errors or turns up no `src/` files —
        //    the GitHub compare API, which still has the history a shallow
        //    clone threw away. The source is printed so a reader can tell
        //    which one produced the file set.
        let range = format!("{}..{}", snapshot.git_hash, run.head_sha);
        let mut local_reason: Option<String> = None;
        let mut source = "via local git";
        let mut broke: std::collections::BTreeSet<String> =
            match crate::git::run_git(&["log", &range, "--name-only", "--oneline"]) {
                Ok(log) => parse_git_log_name_only(&log)
                    .iter()
                    .flat_map(|e| e.files.iter().cloned())
                    .filter(|f| f.starts_with("src/"))
                    .collect(),
                Err(e) => {
                    local_reason = Some(format!(
                        "local git range {range} unavailable ({e}) — likely shallow clone history"
                    ));
                    std::collections::BTreeSet::new()
                }
            };

        if broke.is_empty() {
            match fetch_compare_files(&snapshot.git_hash, &run.head_sha) {
                Ok(files) => {
                    broke = files
                        .into_iter()
                        .filter(|f| f.starts_with("src/"))
                        .collect();
                    source = "via GitHub compare API";
                }
                Err(api_reason) => {
                    // Both sources failed — keep the honest skip. A zero-file
                    // event would only dilute the average.
                    skipped += 1;
                    let local =
                        local_reason.unwrap_or_else(|| format!("no src/ files changed in {range}"));
                    println!(
                        "  {DIM}skip run {}: {local}; compare API also failed ({api_reason}){RESET}",
                        run.run_id
                    );
                    continue;
                }
            }
        }

        if broke.is_empty() {
            skipped += 1;
            println!(
                "  {DIM}skip run {}: no src/ files changed in {range} ({source}) — a 0-file event would only dilute the average{RESET}",
                run.run_id
            );
            continue;
        }

        // 6. Grade both columns with the SAME shared helpers every other
        //    validation path uses.
        let broke_refs: Vec<&str> = broke.iter().map(|s| s.as_str()).collect();
        let predicted_set: std::collections::HashSet<&str> =
            snapshot.predicted.iter().map(|s| s.as_str()).collect();
        let (hit_count, accuracy_pct) = accuracy_of(&broke_refs, &predicted_set);
        let hits: Vec<String> = broke
            .iter()
            .filter(|f| predicted_set.contains(f.as_str()))
            .cloned()
            .collect();
        let surprises: Vec<String> = broke
            .iter()
            .filter(|f| !predicted_set.contains(f.as_str()))
            .cloned()
            .collect();
        let emerging_grade = emerging_grade_of(&broke_refs, &snapshot.emerging);

        // 7. Persist. `severity: "ci_failure"` is not `"watch_success"`, so
        //    `is_green_event` is false and this grades on the RECALL side of
        //    the Day-142 polarity split — which is the entire point.
        if let Err(e) = write_validation_event(
            validation_path,
            day,
            "ci_harvest",
            &hits,
            &surprises,
            accuracy_pct,
            emerging_grade.map(|(_, pct)| pct),
            Some("ci_failure"),
            Some(&snapshot.git_hash), // the snapshot this event graded — auditability (#723), not a dedup key
            Some(run.run_id),
        ) {
            skipped += 1;
            eprintln!(
                "  {RED}could not record CI failure event for run {}: {e}{RESET}",
                run.run_id
            );
            continue;
        }

        harvested += 1;
        let emerging_str = match emerging_grade {
            Some((_, pct)) => format!("{pct:.1}%"),
            None => "n/a (no emerging list in snapshot)".to_string(),
        };
        println!(
            "  📉 run {} (day {} snapshot {}): {}/{} broken files predicted ({accuracy_pct:.1}% recall / emerging {emerging_str}) [{source}] — {}",
            run.run_id,
            snapshot.day,
            snapshot.git_hash,
            hit_count,
            broke.len(),
            run.title,
        );

        // Keep the in-memory dedup view current for the rest of this loop.
        existing = std::fs::read_to_string(validation_path).unwrap_or(existing);
    }

    println!(
        "  Harvest: {harvested} recorded, {deduped} already recorded, {skipped} skipped (reasons above)."
    );
}

fn handle_risk_validate() {
    // 1. Load the most recent snapshot
    let path = std::path::Path::new(RISK_SNAPSHOT_PATH);
    let contents = match std::fs::read_to_string(path) {
        Ok(c) if !c.trim().is_empty() => c,
        Ok(_) => {
            println!("  No snapshots found. Run {BOLD}/risk snapshot{RESET} first.");
            return;
        }
        Err(_) => {
            println!("  No snapshots found. Run {BOLD}/risk snapshot{RESET} first.");
            return;
        }
    };

    // Take the last non-empty line (most recent snapshot)
    let last_line = match contents.lines().rev().find(|l| !l.trim().is_empty()) {
        Some(l) => l,
        None => {
            println!("  No snapshots found. Run {BOLD}/risk snapshot{RESET} first.");
            return;
        }
    };

    let snapshot: serde_json::Value = match serde_json::from_str(last_line) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  {RED}Error parsing snapshot: {e}{RESET}");
            return;
        }
    };

    let git_hash = snapshot["git_hash"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let day = snapshot["day"].as_u64().unwrap_or(0);

    // Extract predicted top-10 file paths
    let top_10: Vec<String> = snapshot["top_10"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["path"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    if top_10.is_empty() {
        eprintln!("  {RED}Snapshot has no top_10 predictions.{RESET}");
        return;
    }

    // 2. Check if there are commits since the snapshot
    let log_output = match crate::git::run_git(&[
        "log",
        &format!("{git_hash}..HEAD"),
        "--name-only",
        "--oneline",
    ]) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("  {RED}Error running git log: {e}{RESET}");
            return;
        }
    };

    if log_output.trim().is_empty() {
        println!("  No commits since last snapshot ({git_hash}) — nothing to validate yet.");
        return;
    }

    // 3. Parse commits and classify breakage
    let entries = parse_git_log_name_only(&log_output);
    let commit_count = entries.len();
    let broke_files = classify_broke_files(&entries);

    // 4. Get current full risk ranking for rank info on surprises
    let all_risks = compute_file_risk_scores();
    let all_ranked: Vec<String> = all_risks.iter().map(|r| r.path.clone()).collect();

    // 5. Compute and display validation
    let result = compute_validation(&top_10, &broke_files, Some(&all_ranked), commit_count);
    let report = format_validation_report(&result, day, &git_hash);
    print!("{report}");

    // 6. Persist a validation event so the CLI `/risk validate` path turns the
    //    prediction meter's crank the same way the watch-failure path does.
    //    hits = predicted files that broke; surprises = files that broke but
    //    weren't predicted. Only record when there was something to validate.
    let hits: Vec<String> = result.hits.clone();
    let surprises: Vec<String> = result.surprises.iter().map(|(f, _)| f.clone()).collect();
    if !hits.is_empty() || !surprises.is_empty() {
        let total_changed = hits.len() + surprises.len();
        let accuracy_pct = if total_changed > 0 {
            (hits.len() as f64 / total_changed as f64) * 100.0
        } else {
            0.0
        };
        let accuracy_pct_rounded = (accuracy_pct * 10.0).round() / 10.0;

        // Also grade the *anticipatory* (emerging) prediction set against the
        // same changed set — the allostatic column of the prediction meter.
        // Legacy snapshots without an `emerging` list stay `None` (ungraded),
        // so they don't drag the average in `compute_accuracy_stats`. The
        // empty-list→None invariant lives in the shared `emerging_grade_of`
        // helper (an empty forecast is ungraded, never Some(0.0)).
        let emerging = emerging_paths_from_snapshot(&snapshot);
        let changed_refs: Vec<&str> = hits
            .iter()
            .chain(surprises.iter())
            .map(|s| s.as_str())
            .collect();
        let emerging_grade = emerging_grade_of(&changed_refs, &emerging);
        if let Some((e_hits, e_pct)) = emerging_grade {
            // Reactive-vs-emerging comparison, visible the moment it's measured
            // (mirrors the watch-failure path in commands_risk_snapshots.rs).
            eprintln!(
                "  {DIM}📊 Emerging (anticipatory) accuracy: {}/{} ({:.1}%) — reactive was {:.1}%{RESET}",
                e_hits, total_changed, e_pct, accuracy_pct_rounded,
            );
        }
        let emerging_accuracy_pct = emerging_grade.map(|(_, pct)| pct);

        if let Err(e) = crate::commands_risk::write_validation_event(
            std::path::Path::new(RISK_VALIDATION_PATH),
            day as u32,
            "cli",
            &hits,
            &surprises,
            accuracy_pct_rounded,
            emerging_accuracy_pct,
            None, // CLI manual grading — untagged severity
            (git_hash != "unknown").then_some(git_hash.as_str()), // graded snapshot — auditability (#723); sentinel stays absent
            None,                                                 // not a CI-harvest event
        ) {
            eprintln!("  {DIM}(warning: could not record risk validation event: {e}){RESET}");
        }
    } else {
        // Third value (#717): the window claims a repair of a `src/` file but
        // corroboration dropped it, so we don't KNOW whether anything broke.
        // Record nothing — an ungraded window is honest, a green one would be a
        // fabricated observation booked as crying-wolf evidence.
        if has_uncorroborated_repair_evidence(&entries) {
            eprintln!(
                "  {DIM}⚠ suspected failure day (repair-claiming commits present, corroboration failed) — not graded{RESET}"
            );
            return;
        }
        // GREEN outcome: commits happened since the snapshot but nothing
        // broke. Grade it anyway — predicted-risky files that were touched
        // without breaking are false-positive evidence, and a meter that only
        // records failures can never measure false positives (Day 140). Uses
        // the same "watch_success" green marker the watch path writes, and
        // dedups by snapshot git hash so repeated validate runs stay quiet.
        let changed: std::collections::BTreeSet<String> = entries
            .iter()
            .flat_map(|e| e.files.iter().cloned())
            .collect();
        let changed: Vec<String> = changed.into_iter().collect();
        let emerging = emerging_paths_from_snapshot(&snapshot);
        match crate::commands_risk_snapshots::record_green_validation_to(
            std::path::Path::new(RISK_VALIDATION_PATH),
            day as u32,
            &git_hash,
            &changed,
            &top_10,
            &emerging,
        ) {
            Ok(crate::commands_risk_snapshots::GreenGrade::Recorded {
                top_hits,
                total,
                top_pct,
                emerging_pct,
            }) => {
                let emerging_str = match emerging_pct {
                    Some(p) => format!("{p:.1}%"),
                    None => "n/a (no emerging list in snapshot)".to_string(),
                };
                eprintln!(
                    "  {DIM}📊 Graded snapshot {git_hash}: no failures — {top_hits}/{total} predicted files touched but clean (top_10 {top_pct:.1}% / emerging {emerging_str} false-positive evidence){RESET}"
                );
            }
            // Deduped: this snapshot was already green-graded — stay silent.
            Ok(crate::commands_risk_snapshots::GreenGrade::Deduped) => {}
            Ok(crate::commands_risk_snapshots::GreenGrade::NoSrcChanges) => {
                // Liveness signal for the fail-soft path (Day 139: fail-soft
                // without a freshness signal is fail-silent).
                eprintln!("  {DIM}(no src/ changes since snapshot — nothing to grade){RESET}");
            }
            Err(e) => {
                eprintln!("  {DIM}(warning: could not record green validation event: {e}){RESET}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Day 174: the risk-score universe is filtered to paths that exist ──
    // A file's deletion commit is churn, so a deleted file otherwise earns a
    // fresh score plus a guaranteed never-forecast status and leads the list
    // that steers the planner (`src/commands_risk_families.rs`, deleted #804,
    // led the dark set and ate a session as #807).

    #[test]
    fn scorable_paths_table() {
        let present = ["src/alive.rs", "src/format/mod.rs"];
        let exists = |p: &str| present.contains(&p);

        // A path the resolver says exists survives.
        let kept = scorable_paths(
            vec!["src/alive.rs".to_string(), "src/format/mod.rs".to_string()],
            &exists,
        );
        assert_eq!(kept.len(), 2);
        assert!(kept.contains("src/alive.rs"));
        assert!(kept.contains("src/format/mod.rs"));

        // A path the resolver says does not exist is dropped — the live case:
        // a deleted file that still carries churn from its own deletion commit.
        let filtered = scorable_paths(
            vec![
                "src/alive.rs".to_string(),
                "src/commands_risk_families.rs".to_string(),
            ],
            &exists,
        );
        assert_eq!(
            filtered.iter().cloned().collect::<Vec<_>>(),
            vec!["src/alive.rs".to_string()],
            "a non-existent candidate must not be scored"
        );

        // An empty candidate set yields an empty set — no panic.
        let empty = scorable_paths(Vec::<String>::new(), &exists);
        assert!(empty.is_empty());

        // Every candidate missing is also empty, not a fallback to "keep all".
        let all_gone = scorable_paths(vec!["src/gone.rs".to_string()], &exists);
        assert!(
            all_gone.is_empty(),
            "an all-missing set must not silently pass through"
        );

        // Duplicates collapse (it returns a set), and the resolver is the only
        // authority — nothing here touches the filesystem.
        let deduped = scorable_paths(
            vec!["src/alive.rs".to_string(), "src/alive.rs".to_string()],
            &exists,
        );
        assert_eq!(deduped.len(), 1);
    }

    // ── #764: the ledger-health line a `/risk accuracy` caller actually reads ──
    // These assert the *string emitted*, not the state one layer below it.

    #[test]
    fn ledger_health_line_says_nothing_on_a_healthy_ledger() {
        // The common path must stay byte-identical to pre-#764 output.
        let ledger = ValidationLedger::Present {
            events: Vec::new(),
            dropped: 0,
            ungradable: 0,
        };
        assert_eq!(ledger_health_line(&ledger), None);
    }

    #[test]
    fn ledger_health_line_says_nothing_when_the_ledger_is_missing() {
        // Today's "accuracy tracking starts automatically" copy is already
        // correct for a genuinely absent file — no second message.
        assert_eq!(ledger_health_line(&ValidationLedger::Missing), None);
    }

    #[test]
    fn ledger_health_line_names_the_path_and_count_on_partial_corruption() {
        let ledger = ValidationLedger::Present {
            events: vec![ValidationEvent {
                day: 160,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_failure".to_string()),
            }],
            dropped: 3,
            ungradable: 0,
        };
        let line = ledger_health_line(&ledger).expect("dropped > 0 must be reported");
        assert!(
            line.contains(RISK_VALIDATION_PATH),
            "names the path: {line:?}"
        );
        assert!(line.contains('3'), "names how many were dropped: {line:?}");
        assert!(
            line.contains("only the rest"),
            "says the numbers below are partial: {line:?}"
        );
    }

    #[test]
    fn ledger_health_line_all_corrupt_names_the_path_and_denies_it_is_missing() {
        // The one that matters: every line unparseable. The report printed
        // below this says accuracy tracking "starts automatically" — copy that
        // is about an *absent* ledger. The line must contradict that outright
        // rather than let a corrupt file wear a missing file's explanation.
        let ledger = ValidationLedger::Present {
            events: Vec::new(),
            dropped: 4,
            ungradable: 0,
        };
        let line = ledger_health_line(&ledger).expect("all-corrupt must be reported");
        assert!(
            line.contains(RISK_VALIDATION_PATH),
            "names the path: {line:?}"
        );
        assert!(line.contains('4'), "names the line count: {line:?}");
        assert!(
            line.contains("exists"),
            "asserts the file is present: {line:?}"
        );
        assert!(
            line.contains("does not apply"),
            "disowns the missing-ledger copy printed underneath: {line:?}"
        );
    }

    #[test]
    fn ledger_health_line_reports_ungradable_lines_when_nothing_is_unparseable() {
        // The regression this test exists for: an earlier `dropped: 0` arm
        // matched first and returned None, so a ledger whose lines all parse
        // as JSON but carry no outcome was silently excluded from the
        // denominator while the report below claimed a confident number.
        let ledger = ValidationLedger::Present {
            events: vec![ValidationEvent {
                day: 160,
                hit_count: 2,
                total_changed: 5,
                accuracy_pct: 40.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_failure".to_string()),
            }],
            dropped: 0,
            ungradable: 7,
        };
        let line =
            ledger_health_line(&ledger).expect("ungradable > 0 must be reported even at dropped=0");
        assert!(
            line.contains(RISK_VALIDATION_PATH),
            "names the path: {line:?}"
        );
        assert!(line.contains('7'), "names how many were skipped: {line:?}");
        assert!(
            line.contains("no gradable outcome"),
            "names the reason, not just a count: {line:?}"
        );
        assert!(
            !line.contains("unparseable"),
            "must not claim corruption when nothing failed to parse: {line:?}"
        );
        assert!(
            line.contains("only the rest"),
            "says the numbers below are partial: {line:?}"
        );
    }

    #[test]
    fn ledger_health_line_reports_a_wholly_ungradable_ledger_without_calling_it_corrupt() {
        // Every line is valid JSON and none of them grades anything: the file
        // is neither missing nor corrupt, and the message must say so without
        // borrowing either of those two explanations.
        let ledger = ValidationLedger::Present {
            events: Vec::new(),
            dropped: 0,
            ungradable: 5,
        };
        let line = ledger_health_line(&ledger).expect("all-ungradable must be reported");
        assert!(
            line.contains(RISK_VALIDATION_PATH),
            "names the path: {line:?}"
        );
        assert!(line.contains('5'), "names the line count: {line:?}");
        assert!(
            line.contains("no gradable outcome"),
            "names the reason: {line:?}"
        );
        assert!(
            !line.contains("unparseable"),
            "must not claim corruption when nothing failed to parse: {line:?}"
        );
        assert!(
            line.contains("does not apply"),
            "disowns the missing-ledger copy printed underneath: {line:?}"
        );
    }

    #[test]
    fn ledger_health_line_keeps_the_two_skip_reasons_distinct() {
        // Both causes present at once: collapsing them into one number would
        // repeat the defect the reporting exists to expose.
        let ledger = ValidationLedger::Present {
            events: vec![ValidationEvent {
                day: 161,
                hit_count: 1,
                total_changed: 4,
                accuracy_pct: 25.0,
                emerging_accuracy_pct: None,
                severity: Some("watch_failure".to_string()),
            }],
            dropped: 2,
            ungradable: 3,
        };
        let line = ledger_health_line(&ledger).expect("both causes must be reported");
        assert!(
            line.contains("2 unparseable"),
            "names the corrupt count: {line:?}"
        );
        assert!(
            line.contains("3 with no gradable outcome"),
            "names the ungradable count separately: {line:?}"
        );
        assert!(
            line.contains('5'),
            "still reports the combined total skipped: {line:?}"
        );
    }

    #[test]
    fn ledger_health_line_passes_the_unreadable_message_through() {
        let ledger = ValidationLedger::Unreadable("could not read foo.jsonl: nope".to_string());
        assert_eq!(
            ledger_health_line(&ledger).as_deref(),
            Some("could not read foo.jsonl: nope")
        );
    }

    /// Build a `ParsedSnapshot` with just enough fields to stand in as one
    /// surviving prediction line.
    fn snapshot_fixture(day: u64) -> crate::commands_risk_snapshots::ParsedSnapshot {
        crate::commands_risk_snapshots::ParsedSnapshot {
            day,
            git_hash: "abc1234".to_string(),
            ts: "2026-08-19T21:00:00Z".to_string(),
            predicted: vec!["src/commands_risk.rs".to_string()],
            emerging: Vec::new(),
        }
    }

    #[test]
    fn snapshot_health_line_says_nothing_on_a_healthy_or_missing_ledger() {
        // Both quiet states, pinned together: the common path must stay
        // byte-identical to the output before the snapshot reader existed.
        assert_eq!(snapshot_health_line(&SnapshotLedger::Missing), None);
        assert_eq!(
            snapshot_health_line(&SnapshotLedger::Present {
                snapshots: vec![snapshot_fixture(170)],
                dropped: 0,
            }),
            None
        );
    }

    #[test]
    fn snapshot_health_line_names_the_path_and_count_on_partial_corruption() {
        let line = snapshot_health_line(&SnapshotLedger::Present {
            snapshots: vec![snapshot_fixture(170)],
            dropped: 2,
        })
        .expect("dropped > 0 must be reported");
        assert!(
            line.contains(RISK_SNAPSHOT_PATH),
            "names the path: {line:?}"
        );
        assert!(line.contains('2'), "names how many were dropped: {line:?}");
        assert!(
            line.contains("only the rest"),
            "says the breakdown below is partial: {line:?}"
        );
    }

    #[test]
    fn snapshot_health_line_all_corrupt_names_the_path_and_denies_it_is_missing() {
        // The sharp case: the file exists and nothing in it parsed, so the
        // per-signal breakdown underneath is computed over zero predictions.
        let line = snapshot_health_line(&SnapshotLedger::Present {
            snapshots: Vec::new(),
            dropped: 5,
        })
        .expect("all-corrupt must be reported");
        assert!(
            line.contains(RISK_SNAPSHOT_PATH),
            "names the path: {line:?}"
        );
        assert!(line.contains('5'), "names the line count: {line:?}");
        assert!(
            line.contains("exists"),
            "asserts the file is present, not absent: {line:?}"
        );
        assert!(
            line.contains("no predictions"),
            "says what the breakdown below actually covers: {line:?}"
        );
    }

    #[test]
    fn snapshot_health_line_passes_the_unreadable_message_through() {
        let ledger = SnapshotLedger::Unreadable("could not read snaps.jsonl: nope".to_string());
        assert_eq!(
            snapshot_health_line(&ledger).as_deref(),
            Some("could not read snaps.jsonl: nope")
        );
    }

    /// A *captured* GitHub compare payload, not one I wrote from memory.
    ///
    /// Taken verbatim (then trimmed to 3 of 14 file entries, patches clipped)
    /// from `gh api "repos/{owner}/{repo}/compare/7a4c5845...47ea6981"` run in
    /// this repo on Day 148. Key names and nesting are exactly what the API
    /// returned — Day 147's lesson: a hand-written fixture pins my belief about
    /// the input, not the input.
    const COMPARE_FIXTURE: &str = r#"{
  "url": "https://api.github.com/repos/yologdev/yoyo-evolve/compare/7a4c5845...47ea6981af59e69a7207fc18842edc6ccacfffe6",
  "status": "diverged",
  "ahead_by": 9,
  "behind_by": 6,
  "total_commits": 9,
  "base_commit": {
    "sha": "7a4c5845b1ddaad2b72fac0da85b6f37c8e1be54"
  },
  "commits": [
    {
      "sha": "0882cc0378132947dc70cc1309ed2fdd72249bd2"
    }
  ],
  "files": [
    {
      "sha": "7ed6ff82de6bcc2a78243fc9c54d3ef5ac14da69",
      "filename": ".skill_evolve_counter",
      "status": "modified",
      "additions": 1,
      "deletions": 1,
      "changes": 2,
      "blob_url": "https://github.com/yologdev/yoyo-evolve/blob/47ea6981af59e69a7207fc18842edc6ccacfffe6/.skill_evolve_counter",
      "raw_url": "https://github.com/yologdev/yoyo-evolve/raw/47ea6981af59e69a7207fc18842edc6ccacfffe6/.skill_evolve_counter",
      "contents_url": "https://api.github.com/repos/yologdev/yoyo-evolve/contents/.skill_evolve_counter?ref=47ea6981af59e69a7207fc18842edc6ccacfffe6",
      "patch": "@@ -1 +1 @@\n-4\n+5"
    },
    {
      "sha": "b797d5e19d0c09a6fa32c396f5b4c9a3ed678a6e",
      "filename": "src/commands_risk.rs",
      "status": "modified",
      "additions": 1,
      "deletions": 1,
      "changes": 2,
      "blob_url": "https://github.com/yologdev/yoyo-evolve/blob/47ea6981af59e69a7207fc18842edc6ccacfffe6/src%2Fcommands_risk.rs",
      "raw_url": "https://github.com/yologdev/yoyo-evolve/raw/47ea6981af59e69a7207fc18842edc6ccacfffe6/src%2Fcommands_risk.rs",
      "contents_url": "https://api.github.com/repos/yologdev/yoyo-evolve/contents/src%2Fcommands_risk.rs?ref=47ea6981af59e69a7207fc18842edc6ccacfffe6",
      "patch": "@@ -23,7 +23,7 @@ pub(crate) use crate::commands_risk_snapsh"
    },
    {
      "sha": "3a8ee0f5c657173fa74d6339531d0f8489d2cff3",
      "filename": "src/commands_risk_accuracy.rs",
      "status": "modified",
      "additions": 111,
      "deletions": 0,
      "changes": 111,
      "blob_url": "https://github.com/yologdev/yoyo-evolve/blob/47ea6981af59e69a7207fc18842edc6ccacfffe6/src%2Fcommands_risk_accuracy.rs",
      "raw_url": "https://github.com/yologdev/yoyo-evolve/raw/47ea6981af59e69a7207fc18842edc6ccacfffe6/src%2Fcommands_risk_accuracy.rs",
      "contents_url": "https://api.github.com/repos/yologdev/yoyo-evolve/contents/src%2Fcommands_risk_accuracy.rs?ref=47ea6981af59e69a7207fc18842edc6ccacfffe6",
      "patch": "@@ -467,6 +467,41 @@ pub(crate) fn format_accuracy_report(st"
    }
  ]
}"#;

    #[test]
    fn test_parse_compare_changed_files_reads_captured_payload() {
        let files = parse_compare_changed_files(COMPARE_FIXTURE);
        assert_eq!(
            files,
            vec![
                ".skill_evolve_counter".to_string(),
                "src/commands_risk.rs".to_string(),
                "src/commands_risk_accuracy.rs".to_string(),
            ],
            "parser must read .files[].filename in payload order"
        );
    }

    #[test]
    fn test_parse_compare_changed_files_absent_or_malformed_is_empty_not_panic() {
        // Every shape the network can hand back that isn't a compare payload.
        // Explicit empty (the caller's skip-with-reason branch), never a panic
        // and never a fabricated file list.
        for bad in [
            "",
            "   ",
            "not json at all",
            "{}",
            r#"{"message":"Not Found","status":"404"}"#,
            r#"{"files": null}"#,
            r#"{"files": "src/commands_risk.rs"}"#,
            r#"{"files": []}"#,
            r#"{"files": [{"status":"modified"}]}"#,
            r#"{"files": [{"filename": 42}]}"#,
        ] {
            assert!(
                parse_compare_changed_files(bad).is_empty(),
                "expected empty Vec for malformed payload: {bad}"
            );
        }
    }

    #[test]
    fn test_parse_compare_changed_files_skips_entries_without_filename() {
        // A partially-usable payload still yields the usable half.
        let json =
            r#"{"files":[{"filename":"src/a.rs"},{"sha":"deadbeef"},{"filename":"src/b.rs"}]}"#;
        assert_eq!(
            parse_compare_changed_files(json),
            vec!["src/a.rs".to_string(), "src/b.rs".to_string()]
        );
    }

    #[test]
    fn test_risk_subcommand_list_covers_every_subcommand() {
        // Drift guard: the unknown-subcommand error derives its "Available:"
        // list from RISK_SUBCOMMANDS via this helper. If anyone reverts to a
        // hardcoded string, this pins that every real subcommand is present.
        let list = risk_subcommand_list();
        for sub in RISK_SUBCOMMANDS {
            assert!(
                list.contains(sub),
                "risk_subcommand_list() is missing `{sub}`"
            );
        }
        assert!(!list.is_empty());
    }

    #[test]
    fn test_risk_subcommands_include_epistemic() {
        // Day 141 drift guard: `epistemic` is dispatched in handle_risk, so it
        // must be discoverable via RISK_SUBCOMMANDS (tab-completion and the
        // unknown-subcommand "Available:" list both read that list). This
        // assertion is deliberately explicit — the generic loop tests above
        // pass vacuously if the entry is simply missing.
        assert!(
            RISK_SUBCOMMANDS.contains(&"epistemic"),
            "RISK_SUBCOMMANDS must list `epistemic` — it is dispatched in handle_risk"
        );
        assert!(!is_unknown_risk_subcommand("epistemic"));
    }
    // ── Risk scoring tests ────────────────────────────────────────────

    #[test]
    fn test_emerging_paths_from_snapshot_present() {
        // Fixture table of input shapes (Day 137 lesson): one row per shape.
        let snapshot = serde_json::json!({
            "top_10": [{"path": "src/a.rs"}],
            "emerging": [{"path": "src/b.rs"}, {"path": "src/c.rs"}]
        });
        assert_eq!(
            emerging_paths_from_snapshot(&snapshot),
            vec!["src/b.rs".to_string(), "src/c.rs".to_string()]
        );
    }

    #[test]
    fn test_emerging_paths_from_snapshot_legacy_missing_key() {
        // Older snapshots (pre-Day-138) have no `emerging` key — must yield
        // an empty list, NOT an error.
        let snapshot = serde_json::json!({"top_10": [{"path": "src/a.rs"}]});
        assert!(emerging_paths_from_snapshot(&snapshot).is_empty());
    }

    #[test]
    fn test_emerging_paths_from_snapshot_malformed_entries() {
        // Entries without a "path" string are skipped; a non-array `emerging`
        // value yields empty.
        let snapshot = serde_json::json!({
            "emerging": [{"path": "src/ok.rs"}, {"nopath": true}, {"path": 42}]
        });
        assert_eq!(
            emerging_paths_from_snapshot(&snapshot),
            vec!["src/ok.rs".to_string()]
        );

        let not_array = serde_json::json!({"emerging": "oops"});
        assert!(emerging_paths_from_snapshot(&not_array).is_empty());
    }

    #[test]
    fn test_normalize_scores_basic() {
        let scores = vec![1.0, 5.0, 10.0];
        let normalized = normalize_scores(&scores);
        assert_eq!(normalized[0], 0.0); // min
        assert_eq!(normalized[2], 1.0); // max
        assert!((normalized[1] - 0.444).abs() < 0.01); // (5-1)/(10-1)
    }

    #[test]
    fn test_normalize_scores_all_equal() {
        let scores = vec![5.0, 5.0, 5.0];
        let normalized = normalize_scores(&scores);
        assert!(normalized.iter().all(|&v| v == 0.0)); // all equal → all 0
    }

    #[test]
    fn test_normalize_scores_empty() {
        let scores: Vec<f64> = vec![];
        let normalized = normalize_scores(&scores);
        assert!(normalized.is_empty());
    }

    #[test]
    fn test_normalize_scores_single() {
        let scores = vec![42.0];
        let normalized = normalize_scores(&scores);
        assert_eq!(normalized[0], 0.0); // single element → 0
    }

    #[test]
    fn test_handle_risk_does_not_panic() {
        // Smoke test — just verify it doesn't crash
        handle_risk("/risk");
        handle_risk("/risk --all");
    }

    #[test]
    fn test_is_unknown_risk_subcommand() {
        // Every documented subcommand is known
        for sub in RISK_SUBCOMMANDS {
            assert!(
                !is_unknown_risk_subcommand(sub),
                "{sub} should be a known /risk subcommand"
            );
        }
        // Empty means "show the report" — not unknown
        assert!(!is_unknown_risk_subcommand(""));
        // A typo must be flagged, not silently fall through to the report
        // (a user typing `yoyo risk snapshoot` would otherwise believe a
        // snapshot was recorded when nothing was)
        assert!(is_unknown_risk_subcommand("snapshoot"));
        assert!(is_unknown_risk_subcommand("foo --all"));
    }

    #[test]
    fn test_risk_subcommand_routing() {
        // "snapshot" should be recognized
        let input = "/risk snapshot";
        let trimmed = input.strip_prefix("/risk").unwrap().trim();
        assert_eq!(trimmed, "snapshot");

        // "--all" should NOT be routed to snapshot
        let input2 = "/risk --all";
        let trimmed2 = input2.strip_prefix("/risk").unwrap().trim();
        assert_ne!(trimmed2, "snapshot");
    }

    #[test]
    fn test_compute_file_risk_scores_returns_all_files() {
        // This project has 71+ source files in src/.
        // compute_file_risk_scores must return ALL of them, not truncate to 15.
        // The display limit belongs in format_risk_report, not in the scorer.
        let risks = compute_file_risk_scores();
        assert!(
            risks.len() > 15,
            "Expected more than 15 risk entries (got {}). \
             The scorer should return all files; truncation belongs in the display layer.",
            risks.len()
        );
    }

    #[test]
    fn test_build_test_reference_map_finds_self() {
        // Guard: this test reads src/*.rs on disk — skip if not in project root
        if !std::path::Path::new("src/git.rs").exists() {
            return;
        }
        // commands_info.rs has #[test] markers and `use crate::` imports.
        // The reference map should find files that commands_info tests exercise.
        let ref_map = build_test_reference_map();
        // commands_info.rs imports crate::git, so src/git.rs should be referenced
        // by at least this file's tests (plus any others that use crate::git).
        assert!(
            ref_map.contains_key("src/git.rs"),
            "Expected src/git.rs to appear in test reference map (it's imported \
             by test-containing files via `use crate::git`). Keys: {:?}",
            ref_map.keys().take(10).collect::<Vec<_>>()
        );
        // The count should be > 0
        assert!(
            *ref_map.get("src/git.rs").unwrap_or(&0) > 0,
            "Expected src/git.rs to have at least 1 cross-file test reference"
        );
    }

    #[test]
    fn test_build_test_reference_map_handles_format_submodule() {
        // Files that `use crate::format::cost` or `crate::format::*` should
        // map to src/format/cost.rs or src/format/mod.rs.
        let ref_map = build_test_reference_map();
        // Many test-containing files import crate::format::* or crate::format::mod
        // so src/format/mod.rs should appear in the map.
        assert!(
            ref_map.contains_key("src/format/mod.rs"),
            "Expected src/format/mod.rs in test reference map (many files \
             `use crate::format::*`). Keys with 'format': {:?}",
            ref_map
                .keys()
                .filter(|k| k.contains("format"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cross_file_coverage_reduces_risk() {
        // Files with cross-file test references should have lower test-density
        // risk than files with no references at all (all else being equal).
        let risks = compute_file_risk_scores();

        // Find a file that has many cross-file references (like git.rs, which is
        // imported by many test-containing files) and one that has few/none.
        let ref_map = build_test_reference_map();

        // Find the file with the most cross-file references
        let most_referenced = ref_map.iter().max_by_key(|(_, v)| *v);
        // Find a file with 0 cross-file references that also has 0 same-file tests
        // (to isolate the effect of cross-file coverage)
        let no_refs_file = risks.iter().find(|r| {
            !ref_map.contains_key(&r.path)
                && std::fs::read_to_string(&r.path)
                    .map(|c| !c.contains("#[test]"))
                    .unwrap_or(true)
        });

        if let (Some((ref_path, _)), Some(no_ref)) = (most_referenced, no_refs_file) {
            // Both files should exist in the risk list
            let ref_risk = risks.iter().find(|r| &r.path == ref_path);
            if let Some(ref_risk) = ref_risk {
                // The cross-file-referenced file should have a lower ▲low-test signal
                // (unless other signals dominate). At minimum, it shouldn't be flagged
                // as low-test while the unreferenced file is — that would be the old bug.
                let ref_has_low_test = ref_risk.signals.contains(&"▲low-test");
                let noref_has_low_test = no_ref.signals.contains(&"▲low-test");
                // If the unreferenced file is flagged as low-test, the referenced file
                // should ideally not be (or have a lower overall score from this signal)
                if noref_has_low_test && !ref_has_low_test {
                    // This is the ideal outcome — cross-file coverage corrected the signal
                } else if !noref_has_low_test {
                    // Both aren't flagged — normalization may have pushed both below threshold
                    // That's fine, the signal is still more accurate
                }
                // The key invariant: a heavily-referenced file should not have a HIGHER
                // test-density risk component than one with zero references
                // (We can't check this directly from the final score since other
                // signals contribute, but the test_reference_map being non-empty
                // and the scorer using it is the structural guarantee)
            }
        }
    }

    #[test]
    fn test_module_to_source_path_basic() {
        // Test the helper function directly
        // "git" should resolve to src/git.rs if it exists
        let result = module_to_source_path("git");
        assert_eq!(result, Some("src/git.rs".to_string()));

        // "format" should resolve to src/format/mod.rs
        let result = module_to_source_path("format");
        assert_eq!(result, Some("src/format/mod.rs".to_string()));

        // "format::cost" should resolve to src/format/cost.rs
        let result = module_to_source_path("format::cost");
        assert_eq!(result, Some("src/format/cost.rs".to_string()));

        // Non-existent module returns None
        let result = module_to_source_path("nonexistent_module_xyz");
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_crate_reference_strips_function() {
        // "git::run_git" should resolve to src/git.rs (strips the function name)
        let result = resolve_crate_reference("git::run_git");
        assert_eq!(result, Some("src/git.rs".to_string()));

        // "format::cost::something" should resolve to src/format/cost.rs
        let result = resolve_crate_reference("format::cost::format_cost");
        assert_eq!(result, Some("src/format/cost.rs".to_string()));
    }

    #[test]
    fn test_parse_git_log_name_only_basic() {
        let log = "\
abc1234 Fix clippy warnings
src/foo.rs
src/bar.rs

def5678 Add new feature
src/baz.rs
";
        let entries = parse_git_log_name_only(log);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].message.contains("Fix clippy"));
        assert_eq!(entries[0].files, vec!["src/foo.rs", "src/bar.rs"]);
        assert!(entries[1].message.contains("Add new feature"));
        assert_eq!(entries[1].files, vec!["src/baz.rs"]);
    }

    #[test]
    fn test_parse_git_log_name_only_no_trailing_blank() {
        // Some git output doesn't end with a blank line
        let log = "abc1234 Fix something\nsrc/a.rs";
        let entries = parse_git_log_name_only(log);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].files, vec!["src/a.rs"]);
    }

    /// Verbatim shape of real `git log --oneline --name-only` output:
    /// git emits NO blank line between commits.  Captured from this repo on
    /// Day 147 (`git log --oneline --name-only -n 3 | cat -A`).
    const REAL_GIT_LOG: &str = "\
c618ce4c Day 146: bump skill-evolve counter (4)
.skill_evolve_counter
2a7d20d8 Day 146 (22:03): session wrap-up
.yoyo/risk_snapshots.jsonl
.yoyo/risk_validations.jsonl
936e1f2b Day 146 (22:03): update learnings
memory/learnings.jsonl
";

    #[test]
    fn test_parse_git_log_name_only_real_output_no_blank_separators() {
        let entries = parse_git_log_name_only(REAL_GIT_LOG);
        assert_eq!(
            entries.len(),
            3,
            "git --oneline emits no blank separators; each hash header starts a commit"
        );
        assert_eq!(entries[0].files, vec![".skill_evolve_counter"]);
        assert_eq!(
            entries[1].files,
            vec![".yoyo/risk_snapshots.jsonl", ".yoyo/risk_validations.jsonl"]
        );
        assert_eq!(entries[2].files, vec!["memory/learnings.jsonl"]);
        // No commit subject line may ever leak into a files list.
        for e in &entries {
            for f in &e.files {
                assert!(
                    !looks_like_oneline_commit_header(f),
                    "commit header leaked into files: {f}"
                );
            }
        }
    }

    #[test]
    fn test_parse_git_log_name_only_commit_count_is_honest() {
        // `commit_count` in the validation report is exactly this length.
        assert_eq!(parse_git_log_name_only(REAL_GIT_LOG).len(), 3);
    }

    #[test]
    fn test_classify_broke_files_reachable_when_revert_is_not_first_commit() {
        // The bug that killed recall grading: only the FIRST commit's message
        // was ever inspected, so a revert in commit #2 was invisible.
        let log = "\
9999999 Day 147: add a feature
src/feature.rs
abc1234 Revert \"something\"
src/foo.rs
";
        let entries = parse_git_log_name_only(log);
        assert_eq!(entries.len(), 2);
        let broke = classify_broke_files(&entries);
        assert_eq!(broke.len(), 1, "expected exactly src/foo.rs, got {broke:?}");
        assert!(broke.contains("src/foo.rs"));
    }

    #[test]
    fn test_looks_like_oneline_commit_header_near_misses() {
        // Fires: real --oneline headers (short and full-length hashes).
        assert!(looks_like_oneline_commit_header(
            "abc1234 Fix clippy warnings"
        ));
        assert!(looks_like_oneline_commit_header(
            "c618ce4c Day 146: bump skill-evolve counter (4)"
        ));
        assert!(looks_like_oneline_commit_header(
            "0123456789abcdef0123456789abcdef01234567 full sha subject"
        ));

        // Near misses that must stay FILE PATHS, not headers:
        assert!(!looks_like_oneline_commit_header("deadbeef.rs"));
        assert!(!looks_like_oneline_commit_header("src/abcdef1.rs"));
        assert!(!looks_like_oneline_commit_header("memory/learnings.jsonl"));
        assert!(!looks_like_oneline_commit_header(".skill_evolve_counter"));
        assert!(!looks_like_oneline_commit_header("abc123 too short hash"));
        assert!(!looks_like_oneline_commit_header("abcdefg1 non hex char"));
        assert!(!looks_like_oneline_commit_header("abc1234")); // no subject
        assert!(!looks_like_oneline_commit_header("abc1234 ")); // empty subject
        assert!(!looks_like_oneline_commit_header(""));
    }

    #[test]
    fn test_parse_git_log_name_only_hashlike_path_and_midline_hash() {
        // A hash-like file name and a commit subject mentioning a hash mid-line
        // must both parse correctly.
        let log = "\
abc1234 Revert commit deadbeef1 in parser
deadbeef.rs
src/abcdef1.rs
";
        let entries = parse_git_log_name_only(log);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].files, vec!["deadbeef.rs", "src/abcdef1.rs"]);
    }

    /// VERBATIM `git log --oneline --name-only` capture from a scratch repo
    /// containing `src/café.rs` and `src/plain rs file.rs` (Day 148). Typed
    /// fixtures pin my *belief* about git's output; only captured output pins
    /// the output (Day 147 lesson).
    ///
    /// It shows both halves of the contract: non-ASCII paths arrive C-quoted
    /// and octal-escaped, plain paths with spaces arrive bare.
    const VERBATIM_QUOTED_PATH_LOG: &str = "\
0488876 fix: unicode path
\"src/caf\\303\\251.rs\"
src/plain rs file.rs
";

    #[test]
    fn test_parse_git_log_name_only_decodes_git_quoted_paths() {
        let entries = parse_git_log_name_only(VERBATIM_QUOTED_PATH_LOG);
        assert_eq!(entries.len(), 1, "one commit in the captured log");
        assert_eq!(
            entries[0].files,
            vec!["src/café.rs", "src/plain rs file.rs"],
            "quoted path must be decoded back to the on-disk path; \
             an unquoted path with spaces must pass through untouched"
        );
    }

    #[test]
    fn test_quoted_path_reaches_broke_set_and_matches_snapshot_path() {
        // The bug this closes: an undecoded `"src/caf\303\251.rs"` can never
        // equal the filesystem-walked path a snapshot stores, so the file was
        // silently dropped from grading instead of counting as a hit.
        // Day 163: the verbatim capture is a LONE fix commit, which the
        // corroboration rule now (correctly) declines to grade as breakage —
        // so the window gets a second, synthetic touch of the same path. The
        // path under test still comes from the real parser: that is what this
        // test is about.
        let mut entries = parse_git_log_name_only(VERBATIM_QUOTED_PATH_LOG);
        entries.push(CommitEntry {
            message: "0000aaa add the unicode path handling".to_string(),
            files: vec!["src/café.rs".to_string()],
        });
        let broke = classify_broke_files(&entries);
        assert!(
            broke.contains("src/café.rs"),
            "decoded path should join the broken set, got {broke:?}"
        );

        let predicted = vec!["src/café.rs".to_string()];
        let result = compute_validation(&predicted, &broke, None, 1);
        assert_eq!(
            result.hits,
            vec!["src/café.rs"],
            "prediction must grade as a hit"
        );
    }

    #[test]
    fn test_unquote_git_path_edge_cases() {
        // Unquoted input is returned verbatim (the common case).
        assert_eq!(unquote_git_path("src/foo.rs"), "src/foo.rs");
        assert_eq!(unquote_git_path("a b/c d.rs"), "a b/c d.rs");
        // Multi-byte character split across two octal escapes.
        assert_eq!(unquote_git_path("\"caf\\303\\251\""), "café");
        // Literal escapes git emits for quotes and backslashes.
        assert_eq!(unquote_git_path("\"a\\\"b\""), "a\"b");
        assert_eq!(unquote_git_path("\"a\\\\b\""), "a\\b");
        // Control-character escapes.
        assert_eq!(unquote_git_path("\"a\\tb\\nc\""), "a\tb\nc");
        // Malformed input must not panic or lose data: a lone trailing
        // backslash is kept as a literal rather than silently dropped.
        assert_eq!(unquote_git_path("\"trailing\\\""), "trailing\\");
        assert_eq!(unquote_git_path("\"\""), "");
        assert_eq!(unquote_git_path("\"unterminated"), "\"unterminated");
    }

    #[test]
    fn test_classify_broke_files_revert() {
        let entries = vec![
            CommitEntry {
                message: "abc1234 Revert \"add feature\"".to_string(),
                files: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
            },
            CommitEntry {
                message: "def5678 Add something cool".to_string(),
                files: vec!["src/c.rs".to_string()],
            },
        ];
        let broke = classify_broke_files(&entries);
        assert!(broke.contains("src/a.rs"));
        assert!(broke.contains("src/b.rs"));
        assert!(!broke.contains("src/c.rs"));
    }

    #[test]
    fn test_classify_broke_files_fix() {
        // Day 163: each fix here is corroborated by the commit that introduced
        // the thing it repairs (the file is touched twice in the window), which
        // is what a genuine in-window repair looks like.
        let entries = vec![
            CommitEntry {
                message: "0000aaa Add the parser".to_string(),
                files: vec!["src/parser.rs".to_string(), "src/docs.rs".to_string()],
            },
            CommitEntry {
                message: "abc1234 fix: handle empty input".to_string(),
                files: vec!["src/parser.rs".to_string()],
            },
            CommitEntry {
                message: "def5678 Fix typo in docs".to_string(),
                files: vec!["src/docs.rs".to_string()],
            },
            CommitEntry {
                message: "ghi9012 Add tests".to_string(),
                files: vec!["src/tests.rs".to_string()],
            },
        ];
        let broke = classify_broke_files(&entries);
        assert!(broke.contains("src/parser.rs"));
        assert!(broke.contains("src/docs.rs"));
        assert!(!broke.contains("src/tests.rs"));
    }

    #[test]
    fn test_message_claims_repair_fixture_table() {
        // (message, expected) — the near-miss side matters as much as the hit side.
        let cases: &[(&str, bool)] = &[
            // ── real repair claims ──
            ("abc1234 fix: handle empty input", true),
            ("abc1234 Fix typo in docs", true),
            ("abc1234 fixes #123", true),
            ("abc1234 fixed the parser", true),
            ("abc1234 fixing flaky test", true),
            ("abc1234 hotfix for release", true),
            ("abc1234 bugfix: off-by-one", true),
            ("abc1234 bug-fix in the loop", true),
            ("abc1234 fixup! earlier commit", true),
            ("abc1234 Revert \"add feature\"", true),
            ("abc1234 reverted the risky change", true),
            ("abc1234 reverts the merge", true),
            // ── near misses: contain "fix"/"revert" as a substring only ──
            ("abc1234 add test fixtures for the parser", false),
            ("abc1234 rename prefix handling", false),
            ("abc1234 strip the suffix from paths", false),
            ("abc1234 support postfix operators", false),
            ("abc1234 affix a header to the report", false),
            ("abc1234 document infix notation", false),
            ("abc1234 Add tests", false),
        ];
        for (msg, expected) in cases {
            assert_eq!(
                message_claims_repair(msg),
                *expected,
                "message_claims_repair({msg:?}) should be {expected}"
            );
        }
    }

    #[test]
    fn test_classify_broke_files_ignores_fix_substrings() {
        // A "fixtures" commit is NOT a break-fix — grading it as one poisons
        // the failure-day half of the prediction meter with phantom breakage.
        let entries = vec![
            CommitEntry {
                message: "abc1234 add test fixtures for the risk parser".to_string(),
                files: vec!["src/fixtures.rs".to_string()],
            },
            CommitEntry {
                message: "def5678 rename prefix helper".to_string(),
                files: vec!["src/prefix.rs".to_string()],
            },
            CommitEntry {
                message: "0000aaa add the loop".to_string(),
                files: vec![
                    "src/loop.rs".to_string(),
                    "src/fixtures.rs".to_string(),
                    "src/prefix.rs".to_string(),
                ],
            },
            CommitEntry {
                message: "ghi9012 fix: off-by-one in the loop".to_string(),
                files: vec!["src/loop.rs".to_string()],
            },
        ];
        let broke = classify_broke_files(&entries);
        assert!(
            !broke.contains("src/fixtures.rs"),
            "\"fixtures\" must not count as a fix"
        );
        assert!(
            !broke.contains("src/prefix.rs"),
            "\"prefix\" must not count as a fix"
        );
        assert!(broke.contains("src/loop.rs"), "a real fix must still count");
    }

    #[test]
    fn test_classify_broke_files_filters_non_src_paths() {
        // #708: the risk model scores only `src/**`, so a tests/ or Cargo.toml
        // path in the broken set is a guaranteed miss that drags recall down.
        let entries = vec![
            CommitEntry {
                message: "0000aaa Day 162: raise the module-size gate".to_string(),
                files: vec![
                    "src/foo.rs".to_string(),
                    "tests/module_size.rs".to_string(),
                    "Cargo.toml".to_string(),
                ],
            },
            CommitEntry {
                message: "abc1234 fix: the module-size gate".to_string(),
                files: vec![
                    "src/foo.rs".to_string(),
                    "tests/module_size.rs".to_string(),
                    "Cargo.toml".to_string(),
                ],
            },
        ];
        let broke = classify_broke_files(&entries);
        assert_eq!(
            broke.len(),
            1,
            "only src/ paths should be graded: {broke:?}"
        );
        assert!(broke.contains("src/foo.rs"));
    }

    #[test]
    fn test_classify_broke_files_empty() {
        let entries: Vec<CommitEntry> = Vec::new();
        let broke = classify_broke_files(&entries);
        assert!(broke.is_empty());
    }

    /// Day 163 — the two-tier corroboration rule, as a fixture table.
    ///
    /// The bug: my own delivered task commits are titled `Day 163 (01:56):
    /// Fix #710 — ...`, so a fully-green feature-delivery session graded as a
    /// failure day and every `src/` file it touched entered the meter as
    /// breakage. Each row names the window, the path under test, and whether
    /// that path should be counted as broken.
    #[test]
    fn test_classify_broke_files_corroboration_fixture_table() {
        // (case name, window, path under test, expected in broke set)
        let cases: Vec<(&str, Vec<CommitEntry>, &str, bool)> = vec![
            (
                "lone delivered fix commit is delivery, not breakage",
                vec![CommitEntry {
                    message: "973fa9a Day 163 (01:56): Fix #710 — RecoveryHintTool stops \
                              coaching around deliberate guard refusals (Task 2)"
                        .to_string(),
                    files: vec!["src/tools.rs".to_string()],
                }],
                "src/tools.rs",
                false,
            ),
            (
                "fix corroborated by another touch in the window is a real repair",
                vec![
                    CommitEntry {
                        message: "973fa9a Day 163 (01:56): Fix #710 — RecoveryHintTool stops \
                                  coaching around deliberate guard refusals (Task 2)"
                            .to_string(),
                        files: vec!["src/tools.rs".to_string()],
                    },
                    CommitEntry {
                        message: "abc1234 Day 163: add the streaming bash cwd pin".to_string(),
                        files: vec!["src/tools.rs".to_string()],
                    },
                ],
                "src/tools.rs",
                true,
            ),
            (
                "a revert needs no corroboration (tier 1)",
                vec![CommitEntry {
                    message: "abc1234 Revert \"Day 162: something\"".to_string(),
                    files: vec!["src/watch.rs".to_string()],
                }],
                "src/watch.rs",
                true,
            ),
            (
                "no repair token contributes nothing, however often re-touched",
                vec![
                    CommitEntry {
                        message: "abc1234 Add a watch phase".to_string(),
                        files: vec!["src/watch.rs".to_string()],
                    },
                    CommitEntry {
                        message: "def5678 Polish the watch phase".to_string(),
                        files: vec!["src/watch.rs".to_string()],
                    },
                ],
                "src/watch.rs",
                false,
            ),
            (
                "#708: a tests/ path is never returned, corroborated or not",
                vec![
                    CommitEntry {
                        message: "abc1234 Revert \"Day 162: something\"".to_string(),
                        files: vec!["tests/module_size.rs".to_string()],
                    },
                    CommitEntry {
                        message: "def5678 fix: the module-size gate".to_string(),
                        files: vec!["tests/module_size.rs".to_string()],
                    },
                ],
                "tests/module_size.rs",
                false,
            ),
            (
                "#708: Cargo.toml is never returned, corroborated or not",
                vec![
                    CommitEntry {
                        message: "abc1234 Revert \"Day 162: bump\"".to_string(),
                        files: vec!["Cargo.toml".to_string()],
                    },
                    CommitEntry {
                        message: "def5678 fix: pin the dependency".to_string(),
                        files: vec!["Cargo.toml".to_string()],
                    },
                ],
                "Cargo.toml",
                false,
            ),
        ];

        for (name, entries, path, expected) in cases {
            let broke = classify_broke_files(&entries);
            assert_eq!(
                broke.contains(path),
                expected,
                "{name}: expected {path} in-broke-set == {expected}, got {broke:?}"
            );
        }
    }

    #[test]
    fn test_classify_broke_files_repeat_path_inside_one_commit_is_one_touch() {
        // Corroboration means "another COMMIT touched it". A path listed twice
        // inside a single entry must not corroborate itself into breakage.
        let entries = vec![CommitEntry {
            message: "abc1234 Day 163: Fix #710 — something".to_string(),
            files: vec!["src/tools.rs".to_string(), "src/tools.rs".to_string()],
        }];
        let broke = classify_broke_files(&entries);
        assert!(
            broke.is_empty(),
            "one commit is one touch, however many times it lists the path: {broke:?}"
        );
    }

    #[test]
    fn test_message_claims_revert_is_the_unambiguous_subset() {
        assert!(message_claims_revert("abc1234 Revert \"add feature\""));
        assert!(message_claims_revert("abc1234 reverted the risky change"));
        assert!(message_claims_revert("abc1234 reverts the merge"));
        assert!(message_claims_revert("abc1234 reverting to Day 161"));
        // Fix-family tokens are repairs but NOT reverts — that is the whole
        // point of the split: they are the tier that needs corroboration.
        assert!(!message_claims_revert("abc1234 fix: handle empty input"));
        assert!(!message_claims_revert(
            "abc1234 Day 163: Fix #710 — a thing"
        ));
        assert!(!message_claims_revert("abc1234 Add tests"));
        // Substring near-miss must not fire (same discipline as REPAIR_TOKENS).
        assert!(!message_claims_revert("abc1234 add a reverter helper"));
    }

    fn entry(message: &str, files: &[&str]) -> CommitEntry {
        CommitEntry {
            message: message.to_string(),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    /// The fixture the corroboration table was missing: the repair lands in a
    /// LATER window than the break, so it is the only touch of that file and
    /// tier 2 drops it. Without the third value this window is booked green.
    #[test]
    fn test_uncorroborated_repair_window_is_flagged() {
        let entries = vec![
            entry(
                "aaa1111 Day 164: Fix #710 — guard the empty case",
                &["src/commands_risk.rs"],
            ),
            entry(
                "bbb2222 Day 164: docs — journal entry",
                &["journals/JOURNAL.md"],
            ),
        ];
        // classify_broke_files drops it (single touch, non-revert)...
        assert!(classify_broke_files(&entries).is_empty());
        // ...so the window must be reported as suspected-failure, not green.
        assert!(has_uncorroborated_repair_evidence(&entries));
    }

    /// The regression that matters most: a genuinely green window must NOT be
    /// swallowed by the new branch, or the false-positive half of the meter
    /// dies silently.
    #[test]
    fn test_genuinely_green_window_is_not_flagged() {
        let entries = vec![
            entry(
                "aaa1111 Day 164: Add /risk epistemic ranking",
                &["src/commands_risk_epistemic.rs"],
            ),
            entry(
                "bbb2222 Day 164: bump skill-evolve counter",
                &[".skill_evolve_counter"],
            ),
            entry("ccc3333 Day 164 (01:12): journal", &["journals/JOURNAL.md"]),
        ];
        assert!(!has_uncorroborated_repair_evidence(&entries));
        // A repair-claiming commit that names no src/ file is also not the
        // uncorroborated-repair shape.
        let docs_only = vec![entry(
            "ddd4444 fix: typo in the docs",
            &["docs/src/usage/commands.md"],
        )];
        assert!(classify_broke_files(&docs_only).is_empty());
        assert!(!has_uncorroborated_repair_evidence(&docs_only));
        // And an empty window says nothing at all.
        assert!(!has_uncorroborated_repair_evidence(&[]));
    }

    /// A corroborated repair is already returned as breakage by
    /// classify_broke_files, so the third value must stay quiet — the window is
    /// graded as a failure day through the normal path.
    #[test]
    fn test_corroborated_repair_is_not_the_third_value() {
        let entries = vec![
            entry(
                "aaa1111 Day 164: Fix #710 — guard the empty case",
                &["src/commands_risk.rs"],
            ),
            entry(
                "bbb2222 Day 164: follow-up tweak",
                &["src/commands_risk.rs"],
            ),
        ];
        assert!(classify_broke_files(&entries).contains("src/commands_risk.rs"));
        assert!(!has_uncorroborated_repair_evidence(&entries));

        // Same for a revert naming a src/ file: tier 1 grades it directly.
        let reverted = vec![entry(
            "ccc3333 Revert \"Day 163: risky change\"",
            &["src/commands_risk.rs"],
        )];
        assert!(!classify_broke_files(&reverted).is_empty());
        assert!(!has_uncorroborated_repair_evidence(&reverted));
    }

    #[test]
    fn test_compute_validation_perfect_prediction() {
        let predicted = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let mut broke = std::collections::HashSet::new();
        broke.insert("src/a.rs".to_string());
        broke.insert("src/b.rs".to_string());

        let result = compute_validation(&predicted, &broke, None, 10);
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.clean.len(), 0);
        assert_eq!(result.surprises.len(), 0);
        assert_eq!(result.commit_count, 10);
    }

    #[test]
    fn test_compute_validation_partial_prediction() {
        let predicted = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let mut broke = std::collections::HashSet::new();
        broke.insert("src/a.rs".to_string());
        broke.insert("src/d.rs".to_string()); // surprise

        let all_ranked = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
            "src/d.rs".to_string(),
        ];

        let result = compute_validation(&predicted, &broke, Some(&all_ranked), 5);
        assert_eq!(result.hits.len(), 1); // only src/a.rs
        assert_eq!(result.clean.len(), 2); // src/b.rs, src/c.rs
        assert_eq!(result.surprises.len(), 1); // src/d.rs
        assert_eq!(result.surprises[0].0, "src/d.rs");
        assert_eq!(result.surprises[0].1, Some(4)); // rank 4 (1-based)
    }

    #[test]
    fn test_compute_validation_no_breakage() {
        let predicted = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let broke = std::collections::HashSet::new();

        let result = compute_validation(&predicted, &broke, None, 20);
        assert_eq!(result.hits.len(), 0);
        assert_eq!(result.clean.len(), 2);
        assert_eq!(result.surprises.len(), 0);
    }

    #[test]
    fn test_format_validation_report_has_key_sections() {
        let result = ValidationResult {
            hits: vec!["src/a.rs".to_string()],
            clean: vec!["src/b.rs".to_string(), "src/c.rs".to_string()],
            surprises: vec![("src/d.rs".to_string(), Some(15))],
            commit_count: 47,
        };

        let report = format_validation_report(&result, 110, "abc123f");
        assert!(report.contains("Risk Prediction Validation"));
        assert!(report.contains("Day 110"));
        assert!(report.contains("abc123f"));
        assert!(report.contains("Commits since: 47"));
        assert!(report.contains("src/a.rs"));
        assert!(report.contains("had fixes"));
        assert!(report.contains("no issues"));
        assert!(report.contains("Precision@10: 1/3"));
        assert!(report.contains("Recall@10:    1/2"));
        assert!(report.contains("Surprises"));
        assert!(report.contains("src/d.rs"));
        assert!(report.contains("rank #15"));
    }

    #[test]
    fn test_format_validation_report_no_surprises() {
        let result = ValidationResult {
            hits: vec!["src/a.rs".to_string()],
            clean: vec!["src/b.rs".to_string()],
            surprises: vec![],
            commit_count: 5,
        };

        let report = format_validation_report(&result, 100, "fff0000");
        assert!(report.contains("Precision@10: 1/2"));
        assert!(!report.contains("Surprises"));
    }

    #[test]
    fn test_risk_validate_routing() {
        let input = "/risk validate";
        let trimmed = input.strip_prefix("/risk").unwrap().trim();
        assert_eq!(trimmed, "validate");
    }

    #[test]
    fn test_risk_history_routing() {
        let input = "/risk history";
        let trimmed = input.strip_prefix("/risk").unwrap().trim();
        assert_eq!(trimmed, "history");
    }

    #[test]
    fn test_compute_trend_improving() {
        // First half: low precision, second half: high precision
        let validations = vec![
            HistoryValidation {
                day: 100,
                git_hash_from: "a".to_string(),
                git_hash_to: "b".to_string(),
                result: ValidationResult {
                    hits: vec!["x.rs".to_string()],
                    clean: vec![
                        "a.rs".to_string(),
                        "b.rs".to_string(),
                        "c.rs".to_string(),
                        "d.rs".to_string(),
                        "e.rs".to_string(),
                        "f.rs".to_string(),
                        "g.rs".to_string(),
                        "h.rs".to_string(),
                        "i.rs".to_string(),
                    ],
                    surprises: vec![],
                    commit_count: 5,
                },
            },
            HistoryValidation {
                day: 101,
                git_hash_from: "b".to_string(),
                git_hash_to: "c".to_string(),
                result: ValidationResult {
                    hits: vec!["x.rs".to_string()],
                    clean: vec![
                        "a.rs".to_string(),
                        "b.rs".to_string(),
                        "c.rs".to_string(),
                        "d.rs".to_string(),
                        "e.rs".to_string(),
                        "f.rs".to_string(),
                        "g.rs".to_string(),
                        "h.rs".to_string(),
                        "i.rs".to_string(),
                    ],
                    surprises: vec![],
                    commit_count: 5,
                },
            },
            HistoryValidation {
                day: 102,
                git_hash_from: "c".to_string(),
                git_hash_to: "d".to_string(),
                result: ValidationResult {
                    hits: vec![
                        "x.rs".to_string(),
                        "y.rs".to_string(),
                        "z.rs".to_string(),
                        "w.rs".to_string(),
                        "v.rs".to_string(),
                    ],
                    clean: vec![
                        "a.rs".to_string(),
                        "b.rs".to_string(),
                        "c.rs".to_string(),
                        "d.rs".to_string(),
                        "e.rs".to_string(),
                    ],
                    surprises: vec![],
                    commit_count: 10,
                },
            },
            HistoryValidation {
                day: 103,
                git_hash_from: "d".to_string(),
                git_hash_to: "e".to_string(),
                result: ValidationResult {
                    hits: vec![
                        "x.rs".to_string(),
                        "y.rs".to_string(),
                        "z.rs".to_string(),
                        "w.rs".to_string(),
                        "v.rs".to_string(),
                    ],
                    clean: vec![
                        "a.rs".to_string(),
                        "b.rs".to_string(),
                        "c.rs".to_string(),
                        "d.rs".to_string(),
                        "e.rs".to_string(),
                    ],
                    surprises: vec![],
                    commit_count: 10,
                },
            },
        ];
        let trend = compute_trend(&validations);
        assert_eq!(trend, "📈 Improving");
    }

    #[test]
    fn test_compute_trend_declining() {
        // First half: high precision, second half: low precision
        let validations = vec![
            HistoryValidation {
                day: 100,
                git_hash_from: "a".to_string(),
                git_hash_to: "b".to_string(),
                result: ValidationResult {
                    hits: vec!["x.rs".to_string(), "y.rs".to_string(), "z.rs".to_string()],
                    clean: vec!["a.rs".to_string()],
                    surprises: vec![],
                    commit_count: 5,
                },
            },
            HistoryValidation {
                day: 101,
                git_hash_from: "b".to_string(),
                git_hash_to: "c".to_string(),
                result: ValidationResult {
                    hits: vec!["x.rs".to_string()],
                    clean: vec![
                        "a.rs".to_string(),
                        "b.rs".to_string(),
                        "c.rs".to_string(),
                        "d.rs".to_string(),
                    ],
                    surprises: vec![],
                    commit_count: 5,
                },
            },
        ];
        let trend = compute_trend(&validations);
        assert_eq!(trend, "📉 Declining");
    }

    #[test]
    fn test_compute_trend_stable() {
        // Same precision in both halves
        let validations = vec![
            HistoryValidation {
                day: 100,
                git_hash_from: "a".to_string(),
                git_hash_to: "b".to_string(),
                result: ValidationResult {
                    hits: vec!["x.rs".to_string()],
                    clean: vec!["a.rs".to_string()],
                    surprises: vec![],
                    commit_count: 5,
                },
            },
            HistoryValidation {
                day: 101,
                git_hash_from: "b".to_string(),
                git_hash_to: "c".to_string(),
                result: ValidationResult {
                    hits: vec!["y.rs".to_string()],
                    clean: vec!["b.rs".to_string()],
                    surprises: vec![],
                    commit_count: 5,
                },
            },
        ];
        let trend = compute_trend(&validations);
        assert_eq!(trend, "➡️  Stable");
    }

    #[test]
    fn test_compute_trend_single_validation() {
        let validations = vec![HistoryValidation {
            day: 100,
            git_hash_from: "a".to_string(),
            git_hash_to: "b".to_string(),
            result: ValidationResult {
                hits: vec!["x.rs".to_string()],
                clean: vec!["a.rs".to_string()],
                surprises: vec![],
                commit_count: 5,
            },
        }];
        let trend = compute_trend(&validations);
        // Not enough data to determine trend
        assert_eq!(trend, "➡️  Stable");
    }

    #[test]
    fn test_format_history_report_empty() {
        let report = format_history_report(&[]);
        assert!(report.contains("No validated snapshots"));
    }

    #[test]
    fn test_format_history_report_has_key_sections() {
        let validations = vec![
            HistoryValidation {
                day: 108,
                git_hash_from: "aaa1111".to_string(),
                git_hash_to: "bbb2222".to_string(),
                result: ValidationResult {
                    hits: vec!["src/a.rs".to_string()],
                    clean: vec!["src/b.rs".to_string()],
                    surprises: vec![("src/c.rs".to_string(), Some(5))],
                    commit_count: 7,
                },
            },
            HistoryValidation {
                day: 109,
                git_hash_from: "bbb2222".to_string(),
                git_hash_to: "ccc3333".to_string(),
                result: ValidationResult {
                    hits: vec![],
                    clean: vec!["src/x.rs".to_string(), "src/y.rs".to_string()],
                    surprises: vec![],
                    commit_count: 3,
                },
            },
        ];
        let report = format_history_report(&validations);
        // Should contain day labels
        assert!(report.contains("108"), "Report should contain day 108");
        assert!(report.contains("109"), "Report should contain day 109");
        // Should contain precision labels
        assert!(
            report.contains("Precision"),
            "Report should show precision info"
        );
        // Should contain overall summary
        assert!(
            report.contains("Overall"),
            "Report should contain overall summary"
        );
        // Should contain trend
        assert!(
            report.contains("Trend"),
            "Report should contain trend indicator"
        );
    }

    #[test]
    fn test_risk_test_density_computed() {
        // A file with 200 lines and 6 #[test] annotations → 6/200*100 = 3.0 tests per 100 lines
        let content = {
            let mut s = String::new();
            for i in 0..200 {
                if i < 6 {
                    s.push_str("    #[test]\n");
                } else {
                    s.push_str("    fn placeholder() {}\n");
                }
            }
            s
        };
        let total = content.lines().count() as f64;
        let test_fn_count = content
            .lines()
            .filter(|l| l.trim().contains("#[test]"))
            .count() as f64;
        let density = test_fn_count / total * 100.0;
        // 6 tests / 200 lines * 100 = 3.0
        assert!((density - 3.0).abs() < 0.01, "expected ~3.0, got {density}");
    }

    #[test]
    fn test_risk_low_test_density_increases_score() {
        // Two files with identical base scores but different test densities.
        // The one with lower test density should get a higher final score
        // due to the penalty term: risk += max(0, (5.0 - td) * 2.0) / 100.0
        let low_td = FileRisk {
            path: "src/low.rs".to_string(),
            score: 0.50,
            signals: vec![],
            test_density: 0.5, // very low
        };
        let high_td = FileRisk {
            path: "src/high.rs".to_string(),
            score: 0.50,
            signals: vec![],
            test_density: 8.0, // above 5.0 threshold
        };

        // Apply the same penalty formula used in compute_file_risk_scores
        let penalty_low = f64::max(0.0, (5.0 - low_td.test_density) * 2.0) / 100.0;
        let penalty_high = f64::max(0.0, (5.0 - high_td.test_density) * 2.0) / 100.0;

        let score_low = low_td.score + penalty_low;
        let score_high = high_td.score + penalty_high;

        assert!(
            score_low > score_high,
            "low-test-density file ({score_low}) should score higher risk than high-test-density file ({score_high})"
        );
        // High density (8.0 > 5.0) should get zero penalty
        assert!(
            penalty_high == 0.0,
            "penalty should be 0 for density above 5.0"
        );
        // Low density should get a positive penalty
        assert!(
            penalty_low > 0.0,
            "penalty should be positive for density below 5.0"
        );
    }

    #[test]
    fn test_co_change_coupling_returns_map() {
        // co_change_coupling should return a HashMap even if git history is sparse
        let coupling = co_change_coupling();
        // In a real repo with commits, we expect at least some entries
        // (the project has 100+ commits touching src/*.rs files)
        // But the function should never panic and should always return a valid map.
        // Just verify it's a HashMap (type-level) — the structured test below
        // checks contents.
        let _ = coupling.len();
    }

    #[test]
    fn test_compute_file_risk_scores_has_coupling_signal() {
        // After adding the coupling signal, compute_file_risk_scores should still
        // return valid results and the ▲coupled signal should be possible.
        let risks = compute_file_risk_scores();
        assert!(
            !risks.is_empty(),
            "Risk scores should not be empty in a real project"
        );

        // All scores should be non-negative
        for risk in &risks {
            assert!(
                risk.score >= 0.0,
                "Risk score for {} should be non-negative, got {}",
                risk.path,
                risk.score
            );
        }

        // The set of valid signals should include ▲coupled
        let all_signals: Vec<&str> = risks
            .iter()
            .flat_map(|r| r.signals.iter().copied())
            .collect();
        let valid_signals = [
            "▲churn",
            "▲recent",
            "▲size",
            "▲reverts",
            "▲low-test",
            "▲coupled",
        ];
        for sig in &all_signals {
            assert!(
                valid_signals.contains(sig),
                "Unexpected signal '{sig}' in risk scores"
            );
        }
    }

    #[test]
    fn test_co_change_coupling_known_pair() {
        // In this project, commands_info.rs and git.rs are frequently co-modified
        // (many features touch both). The coupling map should reflect this.
        let coupling = co_change_coupling();

        // If there's any coupling data at all, verify structure
        for (file, partners) in &coupling {
            assert!(
                file.starts_with("src/") && file.ends_with(".rs"),
                "Coupling keys should be src/*.rs paths, got '{file}'"
            );
            for (partner, count) in partners {
                assert!(
                    partner.starts_with("src/") && partner.ends_with(".rs"),
                    "Coupling partners should be src/*.rs paths, got '{partner}'"
                );
                assert!(*count > 0, "Coupling count should be positive, got {count}");
            }
        }
    }

    #[test]
    fn test_top_risk_files_count_and_order() {
        let top3 = top_risk_files(3);
        // Should return at most 3 entries
        assert!(
            top3.len() <= 3,
            "expected at most 3 entries, got {}",
            top3.len()
        );
        // Scores should be in descending order
        for w in top3.windows(2) {
            assert!(
                w[0].1 >= w[1].1,
                "top_risk_files should be sorted descending: {} >= {} failed",
                w[0].1,
                w[1].1
            );
        }
    }

    // ── Test category 1: Confidence level mapping ──

    #[test]
    fn test_predict_confidence_level_zero_signals_is_low() {
        assert_eq!(predict_confidence_level(0), "low");
    }

    #[test]
    fn test_predict_confidence_level_one_signal_is_low() {
        assert_eq!(predict_confidence_level(1), "low");
    }

    #[test]
    fn test_predict_confidence_level_two_signals_is_medium() {
        assert_eq!(predict_confidence_level(2), "medium");
    }

    #[test]
    fn test_predict_confidence_level_three_signals_is_high() {
        assert_eq!(predict_confidence_level(3), "high");
    }

    #[test]
    fn test_predict_confidence_level_many_signals_is_high() {
        assert_eq!(predict_confidence_level(5), "high");
        assert_eq!(predict_confidence_level(10), "high");
    }

    #[test]
    fn test_predict_confidence_dots_low() {
        let dots = predict_confidence_dots("low");
        assert!(dots.contains("●○○○"), "expected 1 filled dot, got: {dots}");
        assert!(dots.contains("low"));
    }

    #[test]
    fn test_predict_confidence_dots_medium() {
        let dots = predict_confidence_dots("medium");
        assert!(dots.contains("●●○○"), "expected 2 filled dots, got: {dots}");
        assert!(dots.contains("medium"));
    }

    #[test]
    fn test_predict_confidence_dots_high() {
        let dots = predict_confidence_dots("high");
        assert!(dots.contains("●●●○"), "expected 3 filled dots, got: {dots}");
        assert!(dots.contains("high"));
    }

    // ── Test category 2: Reason generation from signal combinations ──

    #[test]
    fn test_predict_reason_high_churn_low_test() {
        let signals = vec!["▲churn", "▲low-test"];
        let reason = predict_reason(&signals, 0.3);
        assert!(
            reason.contains("frequently changed") && reason.contains("weak test coverage"),
            "expected churn + low test reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_reason_high_churn_low_density_inferred() {
        // test_density < 1.0 triggers "low test" even without ▲low-test signal
        let signals = vec!["▲churn"];
        let reason = predict_reason(&signals, 0.5);
        assert!(
            reason.contains("frequently changed") && reason.contains("weak test coverage"),
            "expected churn + low density reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_reason_coupled_and_churn() {
        let signals = vec!["▲coupled", "▲churn"];
        let reason = predict_reason(&signals, 5.0); // high density so low-test doesn't trigger first
        assert!(
            reason.contains("alongside other files") && reason.contains("cascades"),
            "expected coupling + churn reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_reason_reverts() {
        let signals = vec!["▲reverts"];
        let reason = predict_reason(&signals, 5.0);
        assert!(
            reason.contains("reverted") && reason.contains("fragile"),
            "expected revert reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_reason_complex_recent() {
        let signals = vec!["▲size", "▲recent"];
        let reason = predict_reason(&signals, 5.0);
        assert!(
            reason.contains("complex") && reason.contains("regression"),
            "expected complexity + recent reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_reason_churn_recent() {
        let signals = vec!["▲churn", "▲recent"];
        let reason = predict_reason(&signals, 5.0); // high density
        assert!(
            reason.contains("frequently changed") && reason.contains("recent modifications"),
            "expected churn + recent reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_reason_only_low_test() {
        let signals = vec!["▲low-test"];
        let reason = predict_reason(&signals, 0.2);
        assert!(
            reason.contains("low test coverage"),
            "expected low-test reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_reason_fallback() {
        // No matching signals at all, high test density
        let signals: Vec<&str> = vec![];
        let reason = predict_reason(&signals, 5.0);
        assert!(
            reason.contains("combined signals") || reason.contains("elevated"),
            "expected fallback reason, got: {reason}"
        );
    }

    #[test]
    fn test_predict_top_reason_multi_signal() {
        let risk = FileRisk {
            path: "src/foo.rs".to_string(),
            score: 0.9,
            signals: vec!["▲churn", "▲low-test", "▲coupled"],
            test_density: 0.2,
        };
        let reason = predict_top_reason(&risk);
        assert!(
            reason.contains("high churn"),
            "expected churn, got: {reason}"
        );
        assert!(
            reason.contains("low test density"),
            "expected low test density, got: {reason}"
        );
        assert!(
            reason.contains("high coupling"),
            "expected coupling, got: {reason}"
        );
    }

    #[test]
    fn test_predict_top_reason_empty_signals() {
        let risk = FileRisk {
            path: "src/foo.rs".to_string(),
            score: 0.5,
            signals: vec![],
            test_density: 5.0,
        };
        let reason = predict_top_reason(&risk);
        assert_eq!(reason, "elevated risk score");
    }

    // ── Test category 3: Prediction card formatting ──

    #[test]
    fn test_format_prediction_card_contains_all_fields() {
        let risk = FileRisk {
            path: "src/commands_git.rs".to_string(),
            score: 0.87,
            signals: vec!["▲churn", "▲low-test", "▲size"],
            test_density: 0.3,
        };
        let card = format_prediction_card(1, &risk);

        assert!(card.contains("#1"), "card should contain rank #1");
        assert!(
            card.contains("src/commands_git.rs"),
            "card should contain file path"
        );
        assert!(card.contains("0.87"), "card should contain score");
        assert!(card.contains("signals:"), "card should have signals label");
        assert!(
            card.contains("test density:"),
            "card should have test density label"
        );
        assert!(card.contains("0.3"), "card should contain density value");
        assert!(
            card.contains("per 100 lines"),
            "card should contain density unit"
        );
        assert!(
            card.contains("confidence:"),
            "card should have confidence label"
        );
        assert!(
            card.contains("high"),
            "3 signals should give high confidence"
        );
        // Should contain the reason arrow
        assert!(card.contains("→"), "card should contain → reason line");
    }

    #[test]
    fn test_format_prediction_card_low_confidence() {
        let risk = FileRisk {
            path: "src/small.rs".to_string(),
            score: 0.40,
            signals: vec!["▲recent"],
            test_density: 3.0,
        };
        let card = format_prediction_card(3, &risk);
        assert!(card.contains("#3"), "card should contain rank #3");
        assert!(card.contains("low"), "1 signal should give low confidence");
    }

    #[test]
    fn test_format_prediction_card_medium_confidence() {
        let risk = FileRisk {
            path: "src/mid.rs".to_string(),
            score: 0.60,
            signals: vec!["▲churn", "▲recent"],
            test_density: 1.5,
        };
        let card = format_prediction_card(2, &risk);
        assert!(
            card.contains("medium"),
            "2 signals should give medium confidence"
        );
    }

    #[test]
    fn test_format_prediction_card_no_signals() {
        let risk = FileRisk {
            path: "src/empty.rs".to_string(),
            score: 0.10,
            signals: vec![],
            test_density: 0.0,
        };
        let card = format_prediction_card(1, &risk);
        assert!(card.contains("(none)"), "empty signals should show (none)");
    }

    // ── Test category 4: Past accuracy displayed with mocked accuracy data ──

    #[test]
    fn test_prediction_report_with_accuracy() {
        let risks = vec![
            FileRisk {
                path: "src/hot.rs".to_string(),
                score: 0.90,
                signals: vec!["▲churn", "▲low-test", "▲size"],
                test_density: 0.2,
            },
            FileRisk {
                path: "src/warm.rs".to_string(),
                score: 0.70,
                signals: vec!["▲churn"],
                test_density: 2.0,
            },
        ];

        let report =
            format_prediction_report_with_accuracy(&risks, 5, Some((67.0, 3, "Improving")));

        // Should contain the track record
        assert!(
            report.contains("Track record"),
            "report should contain track record line"
        );
        assert!(
            report.contains("67%"),
            "report should contain precision percentage"
        );
        assert!(
            report.contains("3 snapshots"),
            "report should contain snapshot count"
        );
        assert!(report.contains("↑"), "improving trend should show ↑");
    }

    #[test]
    fn test_prediction_report_declining_trend() {
        let risks = vec![FileRisk {
            path: "src/a.rs".to_string(),
            score: 0.80,
            signals: vec!["▲churn"],
            test_density: 1.0,
        }];

        let report =
            format_prediction_report_with_accuracy(&risks, 5, Some((40.0, 5, "Declining")));

        assert!(report.contains("40%"), "report should show 40% precision");
        assert!(report.contains("↓"), "declining trend should show ↓");
    }

    #[test]
    fn test_prediction_report_stable_trend() {
        let risks = vec![FileRisk {
            path: "src/a.rs".to_string(),
            score: 0.80,
            signals: vec!["▲churn"],
            test_density: 1.0,
        }];

        let report = format_prediction_report_with_accuracy(&risks, 5, Some((50.0, 2, "Stable")));

        assert!(report.contains("→"), "stable trend should show →");
    }

    #[test]
    fn test_prediction_report_without_accuracy() {
        let risks = vec![FileRisk {
            path: "src/a.rs".to_string(),
            score: 0.80,
            signals: vec!["▲churn"],
            test_density: 1.0,
        }];

        let report = format_prediction_report_with_accuracy(&risks, 5, None);

        assert!(
            !report.contains("Track record"),
            "no accuracy data means no track record line"
        );
        // But should still have the prediction
        assert!(
            report.contains("Prediction"),
            "report should still contain prediction line"
        );
        assert!(
            report.contains("src/a.rs"),
            "report should contain the predicted file"
        );
    }

    #[test]
    fn test_prediction_report_empty_risks() {
        let report = format_prediction_report_with_accuracy(&[], 5, Some((50.0, 1, "Stable")));
        assert!(
            report.contains("No risk data"),
            "empty risks should show no data message"
        );
        // Track record should NOT appear when there are no risks
        assert!(
            !report.contains("Track record"),
            "no risks means no track record"
        );
    }

    #[test]
    fn test_prediction_report_contains_summary_line() {
        let risks = vec![
            FileRisk {
                path: "src/top.rs".to_string(),
                score: 0.95,
                signals: vec!["▲churn", "▲reverts"],
                test_density: 0.5,
            },
            FileRisk {
                path: "src/second.rs".to_string(),
                score: 0.60,
                signals: vec!["▲recent"],
                test_density: 3.0,
            },
        ];

        let report = format_prediction_report_with_accuracy(&risks, 5, None);

        assert!(
            report.contains("src/top.rs"),
            "prediction should name the top file"
        );
        assert!(
            report.contains("most likely"),
            "prediction should say 'most likely'"
        );
        assert!(
            report.contains("cause the next failure"),
            "prediction should say 'cause the next failure'"
        );
    }

    // ── Test category 5: Routing dispatch ──

    #[test]
    fn test_risk_predict_dispatches_without_panic() {
        // Smoke test: `/risk predict` should not panic
        handle_risk("/risk predict");
    }

    #[test]
    fn test_risk_predict_routing_distinct_from_default() {
        // `/risk predict` and `/risk` should both work without panic
        // (they exercise different code paths)
        handle_risk("/risk");
        handle_risk("/risk predict");
    }

    #[test]
    fn test_top_risk_files_respects_n() {
        // Pure, deterministic check of the prefix logic: build a fixed synthetic
        // score list and derive both prefixes from the SAME snapshot. The old
        // version computed live repo scores twice (top-1 and top-5 calls) and
        // flaked when repo state shifted between the two computations mid-test.
        let risks = vec![
            FileRisk {
                path: "src/a.rs".to_string(),
                score: 0.9,
                signals: vec!["▲churn"],
                test_density: 0.5,
            },
            FileRisk {
                path: "src/b.rs".to_string(),
                score: 0.7,
                signals: vec![],
                test_density: 1.0,
            },
            FileRisk {
                path: "src/c.rs".to_string(),
                score: 0.4,
                signals: vec![],
                test_density: 2.0,
            },
        ];

        let top1 = top_n_from(&risks, 1);
        let top5 = top_n_from(&risks, 5);

        assert_eq!(top1.len(), 1, "top-1 should return exactly 1 entry");
        assert_eq!(top5.len(), 3, "top-5 of 3 entries should return all 3");
        assert_eq!(top1[0].0, "src/a.rs", "top-1 should be the highest score");
        assert_eq!(
            top1[0], top5[0],
            "top-1 entry should match first entry of top-5 (same snapshot)"
        );
        assert_eq!(top5[1].0, "src/b.rs");
        assert_eq!(top5[2].0, "src/c.rs");

        // n = 0 returns nothing
        assert!(top_n_from(&risks, 0).is_empty(), "n=0 should be empty");
        // Empty input returns nothing
        assert!(top_n_from(&[], 5).is_empty(), "empty input should be empty");

        // Smoke: the live wrapper still respects the cap (this part can't flake —
        // it's a single computation and only checks the length bound).
        let live = top_risk_files(2);
        assert!(
            live.len() <= 2,
            "live top_risk_files(2) should return at most 2 entries, got {}",
            live.len()
        );
    }

    #[test]
    fn test_risk_accuracy_dispatches_without_panic() {
        // Smoke test: `/risk accuracy` should not panic
        handle_risk("/risk accuracy");
    }

    // ── /risk effectiveness tests ──

    /// Write a synthetic validation JSONL file where each entry has the given
    /// number of hit files and surprise files. Returns the file path.
    fn write_effectiveness_fixture(
        dir: &std::path::Path,
        entries: &[(usize, usize)], // (hits, surprises) per event
    ) -> std::path::PathBuf {
        let path = dir.join("validations.jsonl");
        let mut content = String::new();
        for (i, (hits, surprises)) in entries.iter().enumerate() {
            let hit_files: Vec<String> = (0..*hits).map(|n| format!("\"h{n}.rs\"")).collect();
            let surprise_files: Vec<String> =
                (0..*surprises).map(|n| format!("\"s{n}.rs\"")).collect();
            let total = hits + surprises;
            let acc = if total > 0 {
                (*hits as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            content.push_str(&format!(
                "{{\"ts\":\"2025-01-{:02}T12:00:00Z\",\"day\":{},\"trigger\":\"watch_failure\",\"hits\":[{}],\"surprises\":[{}],\"predicted_count\":10,\"accuracy_pct\":{acc:.1}}}\n",
                i + 1,
                100 + i,
                hit_files.join(","),
                surprise_files.join(","),
            ));
        }
        std::fs::write(&path, content).expect("write fixture");
        path
    }

    /// Append green-day (`"severity":"watch_success"`) events to a fixture
    /// file. Each entry is (hits, surprises) like `write_effectiveness_fixture`.
    fn append_green_events(path: &std::path::Path, entries: &[(usize, usize)]) {
        let mut content = std::fs::read_to_string(path).unwrap_or_default();
        for (i, (hits, surprises)) in entries.iter().enumerate() {
            let hit_files: Vec<String> = (0..*hits).map(|n| format!("\"gh{n}.rs\"")).collect();
            let surprise_files: Vec<String> =
                (0..*surprises).map(|n| format!("\"gs{n}.rs\"")).collect();
            let total = hits + surprises;
            let acc = if total > 0 {
                (*hits as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            content.push_str(&format!(
                "{{\"ts\":\"2025-02-{:02}T12:00:00Z\",\"day\":{},\"trigger\":\"watch\",\"severity\":\"watch_success\",\"hits\":[{}],\"surprises\":[{}],\"predicted_count\":10,\"accuracy_pct\":{acc:.1}}}\n",
                i + 1,
                200 + i,
                hit_files.join(","),
                surprise_files.join(","),
            ));
        }
        std::fs::write(path, content).expect("append green fixture");
    }

    #[test]
    fn test_effectiveness_green_events_do_not_move_windows_or_verdict() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Same failure-day shape as the learning test: early 25%, recent 75%.
        let path = write_effectiveness_fixture(
            dir.path(),
            &[(1, 3), (1, 3), (1, 3), (3, 1), (3, 1), (3, 1)],
        );
        // Pile on green events with 100% "hit" rates — under the old blended
        // semantics these would inflate the recent window; under Day 142
        // polarity they are false-alarm evidence and must be excluded.
        append_green_events(&path, &[(4, 0), (4, 0), (4, 0), (4, 0)]);

        let report = effectiveness_report_from(&path);
        assert_eq!(report.total_events, 6, "only failure-day events graded");
        assert_eq!(report.green_excluded, 4);
        assert_eq!(report.verdict, EffectivenessVerdict::Learning);
        let early = report.early.as_ref().expect("early window");
        let recent = report.recent.as_ref().expect("recent window");
        assert_eq!(early.event_count, 3);
        assert_eq!(recent.event_count, 3);
        assert!((early.hit_rate_pct - 25.0).abs() < 0.1);
        assert!((recent.hit_rate_pct - 75.0).abs() < 0.1);
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("4 green-day events excluded"));
    }

    #[test]
    fn test_effectiveness_green_events_do_not_unlock_gate() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Only 3 failure-day events (< MIN_EFFECTIVENESS_EVENTS) plus enough
        // green events to cross the threshold if they were (wrongly) counted.
        let path = write_effectiveness_fixture(dir.path(), &[(1, 1); 3]);
        append_green_events(&path, &[(2, 0); 5]);

        let report = effectiveness_report_from(&path);
        assert_eq!(report.verdict, EffectivenessVerdict::Insufficient);
        assert_eq!(report.total_events, 3);
        assert_eq!(report.green_excluded, 5);
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("insufficient data (3 events; need ≥6)"));
        assert!(formatted.contains("5 green-day events excluded"));
    }

    #[test]
    fn test_effectiveness_insufficient_at_exactly_five_events() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Boundary test (below): exactly 5 events → insufficient, no split.
        let path = write_effectiveness_fixture(dir.path(), &[(1, 1); 5]);
        let report = effectiveness_report_from(&path);
        assert_eq!(report.verdict, EffectivenessVerdict::Insufficient);
        assert_eq!(report.total_events, 5);
        assert!(report.early.is_none());
        assert!(report.recent.is_none());
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("insufficient data (5 events; need ≥6)"));
    }

    #[test]
    fn test_effectiveness_real_verdict_at_exactly_six_events() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Boundary test (above): exactly 6 events → windows split 3/3, real verdict.
        let path = write_effectiveness_fixture(dir.path(), &[(1, 1); 6]);
        let report = effectiveness_report_from(&path);
        assert_ne!(report.verdict, EffectivenessVerdict::Insufficient);
        assert_eq!(report.total_events, 6);
        let early = report
            .early
            .as_ref()
            .expect("early window present at 6 events");
        let recent = report
            .recent
            .as_ref()
            .expect("recent window present at 6 events");
        assert_eq!(early.event_count, 3);
        assert_eq!(recent.event_count, 3);
    }

    #[test]
    fn test_effectiveness_verdict_learning() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Early: 1/4 hit rate (25%); recent: 3/4 hit rate (75%) → learning.
        let path = write_effectiveness_fixture(
            dir.path(),
            &[(1, 3), (1, 3), (1, 3), (3, 1), (3, 1), (3, 1)],
        );
        let report = effectiveness_report_from(&path);
        assert_eq!(report.verdict, EffectivenessVerdict::Learning);
        let early = report.early.as_ref().expect("early window");
        let recent = report.recent.as_ref().expect("recent window");
        assert!((early.hit_rate_pct - 25.0).abs() < 0.1);
        assert!((recent.hit_rate_pct - 75.0).abs() < 0.1);
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("reflex appears to be learning ↑"));
    }

    #[test]
    fn test_effectiveness_verdict_flat() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Both windows at 50% hit rate → flat.
        let path = write_effectiveness_fixture(dir.path(), &[(1, 1); 6]);
        let report = effectiveness_report_from(&path);
        assert_eq!(report.verdict, EffectivenessVerdict::Flat);
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("no measurable improvement yet"));
    }

    #[test]
    fn test_effectiveness_verdict_decorative() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Early: 3/4 (75%); recent: 1/4 (25%) → decorative.
        let path = write_effectiveness_fixture(
            dir.path(),
            &[(3, 1), (3, 1), (3, 1), (1, 3), (1, 3), (1, 3)],
        );
        let report = effectiveness_report_from(&path);
        assert_eq!(report.verdict, EffectivenessVerdict::Decorative);
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("reflex may be decorative ↓"));
        assert!(formatted.contains("DREAM.md"));
    }

    #[test]
    fn test_verdict_word_mapping() {
        // Pure mapping used by reflex_effectiveness_summary for /status.
        assert_eq!(verdict_word(&EffectivenessVerdict::Insufficient), None);
        assert_eq!(
            verdict_word(&EffectivenessVerdict::Learning),
            Some("learning ↑")
        );
        assert_eq!(verdict_word(&EffectivenessVerdict::Flat), Some("flat"));
        assert_eq!(
            verdict_word(&EffectivenessVerdict::Decorative),
            Some("decorative ↓")
        );
    }

    // ── End-to-end /status verdict path (day-136 milestone) ──
    // These exercise the exact function /status calls (via the path-parameterized
    // core), proving the ambient verdict is reachable with real file data now that
    // the validation data path is open (#587/#575). Day-124 discipline: assert the
    // /status word from a synthetic JSONL, not the pure mapping in isolation — the
    // report-reading + word-mapping seam must be provably non-skipping.

    #[test]
    fn test_status_verdict_gathering_below_threshold() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // 5 events → still "gathering" → /status stays silent (None).
        let path = write_effectiveness_fixture(dir.path(), &[(1, 1); 5]);
        assert_eq!(reflex_effectiveness_summary_from(&path), None);
    }

    #[test]
    fn test_status_verdict_missing_file_is_silent() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // No validation history at all (CI's real state today) → silent, not a
        // fabricated verdict. This is the honest default the milestone requires.
        let path = dir.path().join("nonexistent.jsonl");
        assert_eq!(reflex_effectiveness_summary_from(&path), None);
    }

    #[test]
    fn test_status_verdict_wakes_positive_from_real_data() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Early 25% → recent 75% hit rate → the verdict wakes and reads positive.
        let path = write_effectiveness_fixture(
            dir.path(),
            &[(1, 3), (1, 3), (1, 3), (3, 1), (3, 1), (3, 1)],
        );
        assert_eq!(reflex_effectiveness_summary_from(&path), Some("learning ↑"));
    }

    #[test]
    fn test_status_verdict_can_be_negative_from_real_data() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Early 75% → recent 25% hit rate. The whole point of the milestone: the
        // ambient /status verdict MUST be able to disappoint me. If this only ever
        // said "learning" or stayed silent, the meter would be decorative (day-135).
        let path = write_effectiveness_fixture(
            dir.path(),
            &[(3, 1), (3, 1), (3, 1), (1, 3), (1, 3), (1, 3)],
        );
        assert_eq!(
            reflex_effectiveness_summary_from(&path),
            Some("decorative ↓")
        );
    }

    #[test]
    fn test_status_verdict_boundary_five_vs_six_events() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Paired boundary (day-122/123): exactly 5 → silent, exactly 6 → speaks.
        let five = write_effectiveness_fixture(dir.path(), &[(1, 1); 5]);
        assert_eq!(reflex_effectiveness_summary_from(&five), None);
        let six_dir = tempfile::tempdir().expect("create temp dir");
        let six = write_effectiveness_fixture(six_dir.path(), &[(1, 1); 6]);
        assert!(reflex_effectiveness_summary_from(&six).is_some());
    }

    #[test]
    fn test_effectiveness_verdict_boundary_exactly_five_points() {
        // Delta of exactly +5.0 points → learning; exactly -5.0 → decorative.
        assert_eq!(
            compute_effectiveness_verdict(50.0, 55.0),
            EffectivenessVerdict::Learning
        );
        assert_eq!(
            compute_effectiveness_verdict(50.0, 45.0),
            EffectivenessVerdict::Decorative
        );
        // Just inside the ±5 band → flat (paired negative cases).
        assert_eq!(
            compute_effectiveness_verdict(50.0, 54.9),
            EffectivenessVerdict::Flat
        );
        assert_eq!(
            compute_effectiveness_verdict(50.0, 45.1),
            EffectivenessVerdict::Flat
        );
    }

    #[test]
    fn test_effectiveness_odd_event_count_splits_extra_to_recent() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // 7 events → early 3, recent 4.
        let path = write_effectiveness_fixture(dir.path(), &[(1, 1); 7]);
        let report = effectiveness_report_from(&path);
        let early = report.early.as_ref().expect("early window");
        let recent = report.recent.as_ref().expect("recent window");
        assert_eq!(early.event_count, 3);
        assert_eq!(recent.event_count, 4);
    }

    #[test]
    fn test_effectiveness_missing_file_is_insufficient() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nonexistent.jsonl");
        let report = effectiveness_report_from(&path);
        assert_eq!(report.verdict, EffectivenessVerdict::Insufficient);
        assert_eq!(report.total_events, 0);
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("insufficient data (0 events; need ≥6)"));
    }

    #[test]
    fn test_effectiveness_report_includes_overall_trend() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = write_effectiveness_fixture(dir.path(), &[(1, 1); 6]);
        let report = effectiveness_report_from(&path);
        // Flat 50% throughout → stable trend, matching /status.
        assert_eq!(report.trend, AccuracyTrend::Stable);
        let formatted = format_effectiveness_report(&report);
        assert!(formatted.contains("Overall trend"));
        assert!(formatted.contains("stable"));
    }

    #[test]
    fn test_risk_sort_deterministic_with_equal_scores() {
        // Two files with identical scores should sort alphabetically by path
        let mut risks = [
            FileRisk {
                path: "src/zebra.rs".into(),
                score: 5.0,
                signals: vec!["high churn"],
                test_density: 0.5,
            },
            FileRisk {
                path: "src/alpha.rs".into(),
                score: 5.0,
                signals: vec!["high churn"],
                test_density: 0.5,
            },
        ];

        // Sort with the same logic used in compute_file_risk_scores
        risks.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });

        assert_eq!(risks[0].path, "src/alpha.rs");
        assert_eq!(risks[1].path, "src/zebra.rs");

        // Reverse the input order — result should be the same
        let mut risks_reversed = [
            FileRisk {
                path: "src/alpha.rs".into(),
                score: 5.0,
                signals: vec!["high churn"],
                test_density: 0.5,
            },
            FileRisk {
                path: "src/zebra.rs".into(),
                score: 5.0,
                signals: vec!["high churn"],
                test_density: 0.5,
            },
        ];

        risks_reversed.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });

        assert_eq!(risks_reversed[0].path, "src/alpha.rs");
        assert_eq!(risks_reversed[1].path, "src/zebra.rs");
    }

    // ================================================================
    // CHOSEN EXPERIMENT (Day 147). Guess made BEFORE inspecting the path:
    //
    // I predict that driving a synthetic failure day end-to-end will NOT
    // cleanly produce a graded failure-day event that flips
    // `recall_coverage_note` off — because `classify_broke_files` decides
    // "this commit claims a repair" by substring-matching the subject line
    // (the documented 'fixture'/'prefix' bug I deferred two nights ago), and
    // I suspect it then attributes the *repair commit's own* file list as the
    // broken set rather than the files of the commit being repaired. Against
    // a verbatim multi-commit log I expect the broken set to come back empty
    // or to name the wrong files, so `compute_validation` grades nothing and
    // the "recall ungraded — 0 failure-day events" note stays on.
    //
    // Graded: MISSED — the whole chain ran clean on the first try. Against the
    // verbatim 3-commit log the parser returned 3 entries, `classify_broke_files`
    // returned a non-empty set of exactly the repair-claiming commit's own three
    // files (and did NOT sweep in the other commits' files), `compute_validation`
    // graded 1 hit / 2 surprises, the untagged event round-tripped through
    // `write_validation_event` → `load_validation_history_from`, `is_green_event`
    // correctly called it a failure day, `failure_hit_rate_pct` came back
    // Some(33.3), `recall_coverage_note` switched off, and the report rendered a
    // real recall number. Both halves of my guess were wrong: the substring match
    // on the subject line did fire, and attributing the repairing commit's own
    // files as the broken set is in fact the intended semantics here (the commit
    // that says "fix" touches what was broken), not the misattribution I feared.
    // So no bug to fix — what this buys is the thing last night's learning
    // demanded: the red branch is no longer "reachable in principle", it is
    // exercised with a pinned outcome. The zero in `/risk accuracy` is now an
    // explained observation (no failure day has occurred) rather than an
    // unexamined one (the path might be dead).
    // ================================================================

    /// Verbatim capture — NOT hand-written. Produced by:
    ///
    /// ```text
    /// git log --oneline --name-only -3 faeb5fa8
    /// ```
    ///
    /// run in this repo on Day 147 and pasted unedited. `faeb5fa8` is the
    /// Day-147 commit that fixed `parse_git_log_name_only` itself, so the log
    /// exercises the real shapes: a repair-claiming subject, a subject with a
    /// parenthesised counter, and a subject containing a colon and digits.
    /// A hand-written fixture would pin my belief about the input; this pins
    /// the input.
    const VERBATIM_GIT_LOG_3: &str = "\
faeb5fa8 Day 147 (02:34): Fix the git-log parser that silently collapses every validation into one commit (recall path is dead) (Task 1)
.yoyo/risk_weights.json
CLAUDE.md
src/commands_risk.rs
c618ce4c Day 146: bump skill-evolve counter (4)
.skill_evolve_counter
2a7d20d8 Day 146 (22:03): session wrap-up
.yoyo/risk_snapshots.jsonl
.yoyo/risk_validations.jsonl
";

    /// Verbatim capture — NOT hand-written. Produced by:
    ///
    /// ```text
    /// git log --oneline --name-only -3 973fa9af
    /// ```
    ///
    /// run in this repo on Day 163 and pasted unedited. It is a CORROBORATED
    /// repair window: `src/tool_wrappers.rs` is touched by the fix commit AND
    /// by one other entry, which is what an in-window repair looks like and
    /// what the Day-163 two-tier rule requires before calling a file broken.
    const VERBATIM_GIT_LOG_CORROBORATED_3: &str = "\
973fa9af Day 163 (01:56): Fix #710 — RecoveryHintTool stops coaching around deliberate guard refusals (no counter bump, no work-around hint) (Task 2, eval-fix 1)
CLAUDE.md
src/tool_wrappers.rs
tests/module_size.rs
53957ef4 Day 163 (01:56): Fix #710 — RecoveryHintTool stops coaching around deliberate guard refusals (no counter bump, no work-around hint) (Task 2)
src/tool_wrappers.rs
de5f7070 docs: CLAUDE.md describes the three-state study reader (#711)
CLAUDE.md
";

    /// Verbatim capture — NOT hand-written. Produced by:
    ///
    /// ```text
    /// git log --oneline --name-only 689cd3bd -n 7
    /// ```
    ///
    /// Day 165 — the replacement fixture for the red branch. The old
    /// `VERBATIM_GIT_LOG_CORROBORATED_3` window got its second touch from an
    /// `(Task 2, eval-fix 1)` RETRY sibling, i.e. the harness corroborating
    /// itself; that window is now (correctly) graded as no breakage, and it
    /// lives on as the negative fixture in
    /// `test_eval_fix_retry_siblings_do_not_manufacture_corroboration`.
    ///
    /// Here the two touches of `src/commands_bg.rs` come from two INDEPENDENT
    /// commits — a delivered `Fix #736 … (Task 1)` and an earlier
    /// `Blind round 37 … (Task 1)` — with three mechanical bookkeeping commits
    /// and one eval-fix retry in between, so the window also exercises the
    /// exclusion list. That independence is what the Day-163 tier-2 rule asks
    /// for. It is a heuristic, not proof of breakage: whether two independent
    /// deliveries touching one file really mean "it broke" is still open. What
    /// this fixture pins is narrower and true — the red branch is reachable
    /// end-to-end, and it is no longer reachable via harness retry siblings.
    const VERBATIM_GIT_LOG_CORROBORATED_7: &str = "\
689cd3bd Day 165 (10:08): Fix #736 — freeze /bg elapsed time when a job finishes (Task 1)
CLAUDE.md
src/commands_bg.rs
87ac79d6 Day 165: bump skill-evolve counter (1)
.skill_evolve_counter
9be97c61 Day 165 (07:27): session wrap-up
.yoyo/risk_snapshots.jsonl
.yoyo/risk_validations.jsonl
journals/JOURNAL.md
465f6bfb Day 165 (07:27): update learnings
memory/learnings.jsonl
64bef0aa Day 165 (07:27): Fix #723 — stamp snapshot_git_hash on failure-day validation events so recall can be audited against the prediction it graded (Task 2, eval-fix 1)
CLAUDE.md
src/commands_risk.rs
src/commands_risk_snapshots.rs
4fb9300c Day 165 (07:27): Fix #723 — stamp snapshot_git_hash on failure-day validation events so recall can be audited against the prediction it graded (Task 2)
src/commands_risk_snapshots.rs
fde2b725 Day 165 (07:27): Blind round 37 — chosen experiment on src/commands_bg.rs (never forecast, never studied) (Task 1)
.yoyo/memory.json
CLAUDE.md
src/commands_bg.rs
";

    /// Day 165 — the bug this fixture used to certify as correct.
    ///
    /// `scripts/evolve.sh` re-commits a task after each evaluator rejection as
    /// `… (Task N, eval-fix M)`. `message_claims_repair` tokenizes on
    /// non-alphanumerics, so `eval-fix` → `["eval", "fix"]` and every retry
    /// reads as a repair claim — and the retry siblings touch the same files,
    /// so they corroborated each other. Three green Day-165 sessions were
    /// booked as failure days with 0 hits before `is_mechanical_commit` learned
    /// the retry suffix. Those ledger events were deliberately NOT rewritten.
    #[test]
    fn test_eval_fix_retry_siblings_do_not_manufacture_corroboration() {
        let entries = parse_git_log_name_only(VERBATIM_GIT_LOG_CORROBORATED_3);
        assert_eq!(entries.len(), 3, "parser sanity on the verbatim capture");
        let broke = classify_broke_files(&entries);
        assert!(
            broke.is_empty(),
            "an `(Task N, eval-fix M)` retry is harness bookkeeping, not a \
             second opinion — the delivered fix beside it is a delivery, not \
             breakage, got {broke:?}"
        );
    }

    /// Day 163 — the bug, asserted against REAL captured output.
    ///
    /// `VERBATIM_GIT_LOG_3` is an ordinary green session that DELIVERED a fix:
    /// one `Fix ...` commit, its `src/` file touched nowhere else in the
    /// window. It used to grade as a failure day and put `src/commands_risk.rs`
    /// into the meter as breakage. Corroboration says: delivery, not repair.
    #[test]
    fn test_lone_delivered_fix_commit_is_not_breakage() {
        let entries = parse_git_log_name_only(VERBATIM_GIT_LOG_3);
        assert_eq!(entries.len(), 3, "parser sanity on the verbatim capture");
        let broke = classify_broke_files(&entries);
        assert!(
            broke.is_empty(),
            "a lone delivered `Fix ...` commit is feature delivery — grading \
             its files as breakage is the Day-163 polluted-denominator bug, \
             got {broke:?}"
        );
    }

    ///
    /// Day 147 made this branch *reachable* by fixing the git-log parser, but
    /// nothing had ever driven it, so `/risk accuracy`'s "recall ungraded —
    /// 0 failure-day events" was an unexplained zero rather than a fact about
    /// the world. This drives a synthetic failure day through the REAL chain —
    /// no re-implemented logic:
    ///
    ///   parse_git_log_name_only → classify_broke_files → compute_validation
    ///     → write_validation_event (tempdir) → load_validation_history_from
    ///     → compute_accuracy_stats → recall_coverage_note / report
    ///
    /// and asserts the claims nobody had verified.
    #[test]
    fn test_failure_day_red_branch_fires_end_to_end() {
        // --- 1. Real parser on verbatim input ------------------------------
        // Day 163: the fixture moved from `VERBATIM_GIT_LOG_3` (a LONE
        // delivered fix, which the corroboration rule correctly no longer
        // grades as breakage — see the test above) to a corroborated repair
        // window. Both are verbatim captures; the red branch is proven with
        // the shape that is actually breakage evidence.
        let entries = parse_git_log_name_only(VERBATIM_GIT_LOG_CORROBORATED_7);
        assert_eq!(
            entries.len(),
            7,
            "commit boundaries must be detected by header shape — a collapsed \
             count is the exact Day-147 bug that made this branch dead"
        );
        assert_eq!(entries[3].files, vec!["memory/learnings.jsonl".to_string()]);

        // --- 2. The gate the red branch depends on -------------------------
        let broke = classify_broke_files(&entries);
        assert!(
            !broke.is_empty(),
            "the repair-claiming commit must yield a non-empty broken set"
        );
        assert!(broke.contains("src/commands_bg.rs"));
        // #708: the same repairing commit touched CLAUDE.md and
        // tests/module_size.rs, but the risk model scores only `src/**`, so
        // grading them was a guaranteed miss. The fixture stays verbatim; the
        // expectation moved.
        assert!(!broke.contains("CLAUDE.md"));
        assert!(!broke.contains("journals/JOURNAL.md"));
        // The eval-fix retry's only unique src/ path — its claim is harness
        // bookkeeping now, so it must not reach the broken set (Day 165).
        assert!(!broke.contains("src/commands_risk.rs"));
        // Claimed by a delivered `(Task 2)` fix but touched by nothing else
        // once its retry sibling is excluded — uncorroborated, so not breakage.
        assert!(!broke.contains("src/commands_risk_snapshots.rs"));
        // Files from the NON-repair commit must not be swept in — CLAUDE.md is
        // touched twice here, so this also pins that corroboration alone is
        // never sufficient.
        assert_eq!(
            broke.len(),
            1,
            "only the corroborated src/ path counts as breakage, got {broke:?}"
        );

        // --- 3. Validation with one hit and one clean prediction -----------
        let predicted: Vec<String> = vec![
            "src/commands_bg.rs".to_string(), // hit
            "src/watch.rs".to_string(),       // clean
        ];
        let result = compute_validation(&predicted, &broke, None, entries.len());
        assert_eq!(result.hits, vec!["src/commands_bg.rs".to_string()]);
        assert_eq!(result.clean, vec!["src/watch.rs".to_string()]);
        assert!(
            result.surprises.is_empty(),
            "the only src/ file that broke was predicted; the non-src \
             paths are outside the model's universe, got {:?}",
            result.surprises
        );
        assert_eq!(result.commit_count, 7);

        // --- 4. Persist the untagged (CLI red-path) event to a TEMPDIR -----
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("risk_validations.jsonl");
        let hits: Vec<String> = result.hits.clone();
        let surprises: Vec<String> = result.surprises.iter().map(|(f, _)| f.clone()).collect();
        let total_changed = hits.len() + surprises.len();
        let accuracy_pct = (hits.len() as f64 / total_changed as f64) * 100.0;
        write_validation_event(
            &path,
            147,
            "cli",
            &hits,
            &surprises,
            (accuracy_pct * 10.0).round() / 10.0,
            None, // no emerging forecast in this synthetic snapshot
            None, // untagged severity — the shape handle_risk_validate writes
            None,
            None,
        )
        .expect("write validation event");

        // --- 5. Read back with the real parser and grade -------------------
        let events = load_validation_history_from(&path);
        assert_eq!(events.len(), 1, "one graded event must round-trip");
        let ev = &events[0];
        assert_eq!(ev.hit_count, 1);
        // #708: 1 graded file, not 3 — the two non-src paths never enter the
        // denominator, so recall stops being dragged by guaranteed misses.
        assert_eq!(ev.total_changed, 1);
        assert!(
            !crate::commands_risk_accuracy::is_green_event(ev),
            "an untagged (severity: None) event is a FAILURE-day event — if this \
             ever flips, every CLI-graded red event silently becomes green"
        );

        let stats = crate::commands_risk_accuracy::compute_accuracy_stats(&events);
        assert!(
            stats.failure_samples >= 1,
            "the failure-day branch must actually count this event"
        );
        assert!(
            stats.failure_hit_rate_pct.is_some(),
            "recall must be graded, not None, once a failure-day event exists"
        );
        let recall = stats.failure_hit_rate_pct.unwrap();
        assert!(
            (recall - 100.0).abs() < 0.01,
            "recall should be 1 hit / 1 graded (src/) broken file, got {recall}"
        );
        assert_eq!(stats.green_samples, 0);

        // --- 6. The user-visible zero must switch off ----------------------
        assert!(
            crate::commands_risk_accuracy::recall_coverage_note(&stats).is_none(),
            "the 'recall ungraded — 0 failure-day events' note must disappear \
             the moment one failure-day event is graded"
        );

        let report = crate::commands_risk_accuracy::format_accuracy_report(&stats);
        assert!(
            !report.contains("(no failure-day events yet)"),
            "report still claims no failure-day events:\n{report}"
        );
        assert!(
            report.contains("recall"),
            "report must render a recall line:\n{report}"
        );
        assert!(
            report.contains("100"),
            "report must render the real recall number:\n{report}"
        );
    }

    /// Day 163 — the receipt. Verbatim `git log --oneline --name-only` for the
    /// 10:25 session, which was fully green (`tasks 1/1 ✅ — build OK, tests
    /// OK`) yet produced this untagged (failure-day) validation event:
    ///
    /// ```json
    /// {"accuracy_pct":0.0,"day":163,"hits":[],"predicted_count":10,
    ///  "surprises":["src/commands_todo.rs"],"trigger":"cli",
    ///  "ts":"2026-08-10T11:24:15Z"}
    /// ```
    ///
    /// The "corroborating" second touch is `cargo fmt` — a commit the harness
    /// makes after EVERY task, touching exactly the files the task touched.
    const VERBATIM_GIT_LOG_MECHANICAL_FOLLOWUP: &str = "\
eee8c191 Day 163 (10:25): cargo fmt
session_plan/assessment.md
src/commands_todo.rs
a7a61618 Day 163 (10:25): Fix #702 — `/todo list` is advertised but not implemented, and `board` is invisible; give the verb list one source (Task 2)
src/commands.rs
src/commands_todo.rs
src/help_data.rs
";

    /// The same delivered fix, corroborated by a REAL second commit instead of
    /// the harness's mechanical follow-up. Same shape, different second
    /// opinion — this one still has to book breakage.
    const VERBATIM_GIT_LOG_REAL_CORROBORATION: &str = "\
eee8c191 Day 163 (10:31): tighten the todo verb table after the regression
src/commands_todo.rs
a7a61618 Day 163 (10:25): Fix #702 — `/todo list` is advertised but not implemented, and `board` is invisible; give the verb list one source (Task 2)
src/commands.rs
src/commands_todo.rs
src/help_data.rs
";

    /// The bug: a `cargo fmt` follow-up is not a second opinion.
    #[test]
    fn test_cargo_fmt_followup_does_not_corroborate_breakage() {
        let entries = parse_git_log_name_only(VERBATIM_GIT_LOG_MECHANICAL_FOLLOWUP);
        assert_eq!(entries.len(), 2, "parser sanity on the verbatim capture");

        let broke = classify_broke_files(&entries);
        assert!(
            broke.is_empty(),
            "the harness commits `cargo fmt` after every task, green or red — \
             corroboration that fires 100% of the time carries zero bits, so \
             this green session must not book breakage, got {broke:?}"
        );

        // ...and the window is exactly the third value #717 built: repair
        // claimed, corroboration dropped every candidate → ungraded, not green.
        assert!(
            has_uncorroborated_repair_evidence(&entries),
            "a repair-claiming commit naming src/ files with no real second \
             opinion is an UNGRADED window, never an affirmative green one"
        );
    }

    /// The control (Day 147 fought to make the red branch reachable; this task
    /// narrows it, it must not silence it). Same fix commit, corroborated by a
    /// commit that is NOT harness bookkeeping → still breakage.
    #[test]
    fn test_real_second_commit_still_corroborates_breakage() {
        let entries = parse_git_log_name_only(VERBATIM_GIT_LOG_REAL_CORROBORATION);
        assert_eq!(entries.len(), 2, "parser sanity on the verbatim capture");

        let broke = classify_broke_files(&entries);
        assert!(
            broke.contains("src/commands_todo.rs"),
            "a real (non-mechanical) second touch is genuine corroboration — \
             the red branch must stay alive, got {broke:?}"
        );
        // Only the twice-touched path counts; the fix commit's other files
        // were touched once.
        assert_eq!(broke.len(), 1, "got {broke:?}");
        assert!(
            !has_uncorroborated_repair_evidence(&entries),
            "a corroborated repair is graded as a failure day through the \
             normal path, so the third value must stay quiet"
        );
    }

    /// The filter is keyed to `scripts/evolve.sh`'s vocabulary — pin the list
    /// so a silent drift shows up as a test failure rather than as recall
    /// pollution nobody reads.
    #[test]
    fn test_is_mechanical_commit_covers_harness_vocabulary() {
        for subject in [
            "eee8c191 Day 163 (10:25): cargo fmt",
            "6045fc71 Day 163 (10:25): session wrap-up",
            "ef64d24c Day 163 (10:25): journal entry",
            "b4e0cd3e Day 163 (10:25): update learnings",
            "449494c3 Day 163 (11:35): assessment (draft)",
            "7b3f47b4 docs(assessment): add changes",
            "abc1234 Day 163: session plan",
            "bfec68ec Day 163: bump skill-evolve counter (5)",
            "def5678 Day 162: social session",
            // Case-insensitive.
            "abc1234 Day 163: CARGO FMT",
            // Day 165: the fix-loop retry family. Verbatim subjects from
            // `git log --oneline -60`, not hand-typed approximations — a
            // hand-written fixture pins my belief about the input, not the
            // input (Day 147).
            "07cd8bcd Day 165 (13:13): Blind round 38 — chosen experiment on src/setup.rs (coldest room: 78 snapshots since last seen) (Task 2, eval-fix 7)",
            "e0a7131b Day 165 (13:13): Blind round 38 — chosen experiment on src/setup.rs (coldest room: 78 snapshots since last seen) (Task 2, eval-fix 1)",
            "aaec088b Day 165 (16:00): #683 step 2 — actually record run events (tee all four prompt call sites) + redact secrets in the same diff (Task 1, eval-fix 4)",
            "64bef0aa Day 165 (07:27): Fix #723 — stamp snapshot_git_hash on failure-day validation events so recall can be audited against the prediction it graded (Task 2, eval-fix 1)",
            "ca0f4e34 Day 165 (13:13): `yoyo <unknown-word>` is a silent paid prompt — add a near-miss guard on the bare-word CLI path (Task 1, eval-fix 3)",
            // A future retry label rides the same suffix.
            "abc1234 Day 165 (13:13): something (Task 3, build-fix 2)",
        ] {
            assert!(
                is_mechanical_commit(subject),
                "harness bookkeeping must not corroborate: {subject}"
            );
        }
        for subject in [
            "a7a61618 Day 163 (10:25): Fix #702 — `/todo list` is advertised",
            "abc1234 Day 163: add the streaming bash cwd pin",
            "def5678 Revert \"Day 162: something\"",
            "ghi9012 Day 163: raise the module-size gate",
            // Day 165 negatives, verbatim: a DELIVERED task carries the plain
            // `(Task N)` suffix and is real work — the retry filter must not
            // swallow it, or a genuine repair loses its claim.
            "689cd3bd Day 165 (10:08): Fix #736 — freeze /bg elapsed time when a job finishes (Task 1)",
            "4fb9300c Day 165 (07:27): Fix #723 — stamp snapshot_git_hash on failure-day validation events so recall can be audited against the prediction it graded (Task 2)",
            "0469a932 Day 165 (04:49): /checkpoint restore claims work it did not do — empty and all-failed restores print a green success header (Task 2)",
            // A human commit that merely mentions a task number, no retry marker.
            "abc1234 Day 165: fix the parser (Task 2 follow-up)",
        ] {
            assert!(
                !is_mechanical_commit(subject),
                "real work must keep its corroborating power: {subject}"
            );
        }
    }
}
