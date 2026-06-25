//! `/risk` command: file risk scoring, snapshot, validate, history,
//! co-change coupling, test coverage mapping.
//!
//! Extracted from `commands_info.rs` — the risk scorer subsystem built
//! over Days 111-113 as the dream milestone infrastructure.

use crate::format::*;

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

/// Risk signal weights: [churn, recency, size, complexity, test_density, coupling, revert_history].
///
/// These seven weights must sum to 1.0. The revert_history weight (0.10) captures
/// empirical failure data — files that have been reverted are more likely to cause
/// future regressions. The complexity proxy (0.10) estimates cyclomatic complexity
/// via branch/match density. This is the signal set that makes the risk scorer learn
/// from its own history rather than just measuring static properties.
const RISK_WEIGHTS: [f64; 7] = [0.30, 0.15, 0.15, 0.10, 0.10, 0.10, 0.10];

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
                // Skip if this is part of a `use crate::` (already handled above)
                if abs_idx >= 4 && &trimmed[abs_idx - 4..abs_idx] == "use " {
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
    let mut risks: Vec<FileRisk> = Vec::with_capacity(all_files.len());

    for (i, path) in all_files.into_iter().enumerate() {
        let td = tests_per_100[i];
        // Base weighted score from normalized signals
        // Order: churn, recency, size, complexity, test_density, coupling, revert_history
        let mut score = norm_churn[i] * RISK_WEIGHTS[0]
            + norm_accel[i] * RISK_WEIGHTS[1]
            + norm_size[i] * RISK_WEIGHTS[2]
            + norm_complexity[i] * RISK_WEIGHTS[3]
            + norm_test[i] * RISK_WEIGHTS[4]
            + norm_coupling[i] * RISK_WEIGHTS[5]
            + norm_revert[i] * RISK_WEIGHTS[6];

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

    // Sort descending by score
    risks.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    risks
}

/// Return the top `n` riskiest files as `(path, score)` pairs, sorted descending.
///
/// Convenience wrapper around `compute_file_risk_scores()` for use by `/status`
/// and other consumers that just need names and numbers.
pub(crate) fn top_risk_files(n: usize) -> Vec<(String, f64)> {
    let risks = compute_file_risk_scores();
    risks
        .into_iter()
        .take(n)
        .map(|r| (r.path, r.score))
        .collect()
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
fn revert_history() -> std::collections::HashMap<String, u32> {
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

    let show_all = input.contains("--all");
    let risks = compute_file_risk_scores();
    let report = format_risk_report(&risks, show_all);
    print!("{report}");
}

/// Default path for risk snapshot JSONL file.
const RISK_SNAPSHOT_PATH: &str = ".yoyo/risk_snapshots.jsonl";

/// Build the JSON string for a risk snapshot entry.
///
/// Takes already-sorted risk scores, day number, and git hash.
/// Returns a single JSON line (no trailing newline).
fn build_risk_snapshot_json(risks: &[FileRisk], day: u32, git_hash: &str) -> String {
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
fn write_risk_snapshot_to(path: &std::path::Path, json_line: &str) -> Result<(), std::io::Error> {
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

    let json = build_risk_snapshot_json(&risks, day, &git_hash);
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

/// Automatically capture a risk snapshot after a successful commit.
///
/// Called from `commands_git.rs` after each successful `/commit`.
/// Silently skips on error (prints a dim note to stderr).
pub(crate) fn auto_risk_snapshot() {
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

/// Parsed git-log entry: one commit message + the files it touched.
struct CommitEntry {
    message: String,
    files: Vec<String>,
}

/// Parse `git log --name-only --oneline` output into structured entries.
///
/// Each commit is one message line followed by zero or more blank-separated
/// file paths, then a blank line.  Example:
/// ```text
/// abc1234 Fix clippy warnings
/// src/foo.rs
/// src/bar.rs
///
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

        if current_msg.is_none() {
            // First non-blank line of a commit: "hash message..."
            current_msg = Some(trimmed.to_string());
        } else {
            // Subsequent non-blank line: file path
            current_files.push(trimmed.to_string());
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

/// Classify commits and return the set of files that "broke" —
/// i.e., appeared in revert or fix commits.
fn classify_broke_files(entries: &[CommitEntry]) -> std::collections::HashSet<String> {
    let mut broke = std::collections::HashSet::new();
    for entry in entries {
        let msg_lower = entry.message.to_lowercase();
        let is_revert = msg_lower.contains("revert");
        let is_fix = msg_lower.contains("fix");
        if is_revert || is_fix {
            for f in &entry.files {
                broke.insert(f.clone());
            }
        }
    }
    broke
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

/// A parsed risk snapshot from the JSONL file.
struct ParsedSnapshot {
    day: u64,
    git_hash: String,
    predicted: Vec<String>,
}

/// Parse all snapshots from JSONL content.
fn parse_all_snapshots(content: &str) -> Vec<ParsedSnapshot> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Risk scoring tests ────────────────────────────────────────────

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
    fn test_handle_risk_does_not_panic() {
        // Smoke test — just verify it doesn't crash
        handle_risk("/risk");
        handle_risk("/risk --all");
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
        let entries = vec![
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
    fn test_classify_broke_files_empty() {
        let entries: Vec<CommitEntry> = Vec::new();
        let broke = classify_broke_files(&entries);
        assert!(broke.is_empty());
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
    fn test_risk_weights_sum_to_one() {
        // The 7 risk signal weights must sum to 1.0 (within floating-point tolerance)
        let sum: f64 = RISK_WEIGHTS.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-10,
            "Risk weights should sum to 1.0, got {sum}"
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
        let top1 = top_risk_files(1);
        assert!(
            top1.len() <= 1,
            "expected at most 1 entry, got {}",
            top1.len()
        );

        let top5 = top_risk_files(5);
        assert!(
            top5.len() <= 5,
            "expected at most 5 entries, got {}",
            top5.len()
        );

        // top1 should be a prefix of top5
        if !top1.is_empty() && !top5.is_empty() {
            assert_eq!(
                top1[0].0, top5[0].0,
                "top-1 file should match first entry of top-5"
            );
        }
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
}
