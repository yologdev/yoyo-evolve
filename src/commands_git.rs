//! Git-related command handlers: /diff, /undo, /commit, /pr, /git.

use crate::agent_builder::AgentConfig;
use crate::commands_session::auto_compact_if_needed;
use crate::format::*;
use crate::git::*;
use crate::prompt::run_prompt;
use crate::session::TurnHistory;
use crate::symbols::{self, SymbolKind};

use std::collections::HashMap;
use std::io::{self, Write};
use yoagent::agent::Agent;
use yoagent::*;

// PR subcommand handling lives in commands_git_pr.rs; re-exported here so
// all call sites (dispatch.rs, commands.rs, help completions) are unchanged.
pub use crate::commands_git_pr::{handle_pr, parse_pr_args, PrSubcommand};

// ── /diff ────────────────────────────────────────────────────────────────

/// A parsed line from `git diff --stat` output.
/// Example: " src/main.rs | 42 +++++++++-------"
#[derive(Debug, Clone, PartialEq)]
pub struct DiffStatEntry {
    pub file: String,
    pub insertions: u32,
    pub deletions: u32,
}

/// Summary totals from `git diff --stat` output.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffStatSummary {
    pub entries: Vec<DiffStatEntry>,
    pub total_insertions: u32,
    pub total_deletions: u32,
}

/// Parse `git diff --stat` output into structured entries.
///
/// Each line looks like:
///   " src/commands.rs | 42 +++++++++-------"
/// The last line is a summary like:
///   " 3 files changed, 25 insertions(+), 10 deletions(-)"
pub fn parse_diff_stat(stat_output: &str) -> DiffStatSummary {
    let mut entries = Vec::new();
    let mut total_insertions: u32 = 0;
    let mut total_deletions: u32 = 0;

    for line in stat_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try to parse summary line: "N file(s) changed, N insertion(s)(+), N deletion(s)(-)"
        if trimmed.contains("changed")
            && (trimmed.contains("insertion") || trimmed.contains("deletion"))
        {
            // Parse insertions
            if let Some(ins_part) = trimmed.split("insertion").next() {
                if let Some(num_str) = ins_part.split(',').next_back() {
                    if let Ok(n) = num_str.trim().parse::<u32>() {
                        total_insertions = n;
                    }
                }
            }
            // Parse deletions
            if let Some(del_part) = trimmed.split("deletion").next() {
                if let Some(num_str) = del_part.split(',').next_back() {
                    if let Ok(n) = num_str.trim().parse::<u32>() {
                        total_deletions = n;
                    }
                }
            }
            continue;
        }

        // Try to parse file entry: "file | N +++---" or "file | Bin 0 -> 1234 bytes"
        if let Some(pipe_pos) = trimmed.find('|') {
            let file = trimmed[..pipe_pos].trim().to_string();
            let stats_part = trimmed[pipe_pos + 1..].trim();

            if file.is_empty() {
                continue;
            }

            // Count + and - characters in the visual bar
            let insertions = stats_part.chars().filter(|&c| c == '+').count() as u32;
            let deletions = stats_part.chars().filter(|&c| c == '-').count() as u32;

            entries.push(DiffStatEntry {
                file,
                insertions,
                deletions,
            });
        }
    }

    // If no summary line was found, compute totals from entries
    if total_insertions == 0 && total_deletions == 0 {
        total_insertions = entries.iter().map(|e| e.insertions).sum();
        total_deletions = entries.iter().map(|e| e.deletions).sum();
    }

    DiffStatSummary {
        entries,
        total_insertions,
        total_deletions,
    }
}

/// Format a diff stat summary with colors for display.
pub fn format_diff_stat(summary: &DiffStatSummary) -> String {
    let mut output = String::new();

    if summary.entries.is_empty() {
        return output;
    }

    // Find max filename length for alignment
    let max_name_len = summary
        .entries
        .iter()
        .map(|e| e.file.len())
        .max()
        .unwrap_or(0);

    output.push_str(&format!("{DIM}  File summary:{RESET}\n"));
    for entry in &summary.entries {
        let total_changes = entry.insertions + entry.deletions;
        let ins_str = if entry.insertions > 0 {
            format!("{GREEN}+{}{RESET}", entry.insertions)
        } else {
            String::new()
        };
        let del_str = if entry.deletions > 0 {
            format!("{RED}-{}{RESET}", entry.deletions)
        } else {
            String::new()
        };
        let sep = if entry.insertions > 0 && entry.deletions > 0 {
            " "
        } else {
            ""
        };
        output.push_str(&format!(
            "    {:<width$}  {}{DIM}{:>4}{RESET} {ins_str}{sep}{del_str}\n",
            entry.file,
            "",
            total_changes,
            width = max_name_len,
        ));
    }

    // Summary line
    let files_count = summary.entries.len();
    output.push_str(&format!(
        "\n  {DIM}{files_count} file{s} changed{RESET}",
        s = if files_count == 1 { "" } else { "s" }
    ));
    if summary.total_insertions > 0 {
        output.push_str(&format!(", {GREEN}+{}{RESET}", summary.total_insertions));
    }
    if summary.total_deletions > 0 {
        output.push_str(&format!(", {RED}-{}{RESET}", summary.total_deletions));
    }
    output.push('\n');

    output
}

/// Parsed options for the `/diff` command.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffOptions {
    pub staged_only: bool,
    pub name_only: bool,
    pub stat_only: bool,
    pub explain: bool,
    pub functions: bool,
    pub file: Option<String>,
    /// A git ref or ref range (e.g. `main`, `main..feature`, `HEAD~3`, `v1.0..HEAD`).
    /// When set, the diff compares the specified commits instead of working tree changes.
    pub ref_range: Option<String>,
}

/// Parse `/diff` arguments into structured options.
///
/// Supports:
/// - `/diff` — all changes (default)
/// - `/diff --staged` or `/diff --cached` — staged only
/// - `/diff --name-only` — filenames only
/// - `/diff --functions` — semantic-level change summary (added/modified/removed symbols)
/// - `/diff --explain` — AI-powered explanation of changes
/// - `/diff <file>` — diff for a specific file
/// - `/diff <ref>` — diff against a git ref (branch, tag, commit, HEAD~N)
/// - `/diff <ref>..<ref>` — diff between two refs
/// - Combined: `/diff --staged --name-only src/main.rs`
/// - Combined: `/diff main..feature --stat`
pub fn parse_diff_args(input: &str) -> DiffOptions {
    let rest = input.strip_prefix("/diff").unwrap_or("").trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut staged_only = false;
    let mut name_only = false;
    let mut stat_only = false;
    let mut explain = false;
    let mut functions = false;
    let mut file = None;
    let mut ref_range = None;

    for part in parts {
        match part {
            "--staged" | "--cached" => staged_only = true,
            "--name-only" => name_only = true,
            "--stat" => stat_only = true,
            "--explain" => explain = true,
            "--functions" => functions = true,
            _ if part.starts_with('-') => {} // ignore unknown flags
            _ => {
                // Distinguish file paths from git refs:
                // 1. Contains ".." → definitely a ref range (e.g. main..feature)
                // 2. Contains "..." → definitely a ref range (e.g. main...feature)
                // 3. Exists on disk → file path
                // 4. Otherwise → git ref
                if part.contains("..") {
                    ref_range = Some(part.to_string());
                } else if std::path::Path::new(part).exists() {
                    file = Some(part.to_string());
                } else {
                    ref_range = Some(part.to_string());
                }
            }
        }
    }

    // When comparing refs, --staged is meaningless — ignore it
    if ref_range.is_some() {
        staged_only = false;
    }

    DiffOptions {
        staged_only,
        name_only,
        stat_only,
        explain,
        functions,
        file,
        ref_range,
    }
}

pub fn handle_diff(input: &str) {
    let opts = parse_diff_args(input);

    // When a ref range is specified, compare commits directly
    if let Some(ref range) = opts.ref_range {
        handle_diff_ref_range(&opts, range);
        return;
    }

    // Check if we're in a git repo
    match run_git(&["status", "--short"]) {
        Ok(status) if status.is_empty() => {
            println!("{DIM}  (no uncommitted changes){RESET}\n");
        }
        Ok(_status) => {
            // ── Name-only mode: just list changed filenames ──────────
            if opts.name_only {
                let mut args = vec!["diff", "--name-only"];
                if opts.staged_only {
                    args.push("--cached");
                }
                let file_ref;
                if let Some(ref f) = opts.file {
                    args.push("--");
                    file_ref = f.as_str();
                    args.push(file_ref);
                }
                let names = run_git(&args).unwrap_or_default();
                // If not staged-only, also grab staged names
                if !opts.staged_only {
                    let mut staged_args = vec!["diff", "--name-only", "--cached"];
                    let staged_file_ref;
                    if let Some(ref f) = opts.file {
                        staged_args.push("--");
                        staged_file_ref = f.as_str();
                        staged_args.push(staged_file_ref);
                    }
                    let staged_names = run_git(&staged_args).unwrap_or_default();
                    // Combine and deduplicate
                    let mut all_files: Vec<&str> = names
                        .lines()
                        .chain(staged_names.lines())
                        .filter(|l| !l.trim().is_empty())
                        .collect();
                    all_files.sort();
                    all_files.dedup();
                    if all_files.is_empty() {
                        println!("{DIM}  (no changed files){RESET}\n");
                    } else {
                        println!("{DIM}  Changed files:{RESET}");
                        for f in &all_files {
                            println!("    {f}");
                        }
                        println!();
                    }
                } else if names.trim().is_empty() {
                    println!("{DIM}  (no staged files){RESET}\n");
                } else {
                    println!("{DIM}  Staged files:{RESET}");
                    for f in names.lines().filter(|l| !l.trim().is_empty()) {
                        println!("    {f}");
                    }
                    println!();
                }
                return;
            }

            // --stat: show compact diffstat summary without full diff
            if opts.stat_only {
                let mut args = vec!["diff", "--stat"];
                if opts.staged_only {
                    args.push("--cached");
                }
                let file_ref;
                if let Some(ref f) = opts.file {
                    args.push("--");
                    file_ref = f.as_str();
                    args.push(file_ref);
                }
                let stat_text = run_git(&args).unwrap_or_default();

                // If not staged-only, also grab staged stat
                if !opts.staged_only {
                    let mut staged_args = vec!["diff", "--cached", "--stat"];
                    let staged_file_ref;
                    if let Some(ref f) = opts.file {
                        staged_args.push("--");
                        staged_file_ref = f.as_str();
                        staged_args.push(staged_file_ref);
                    }
                    let staged_stat = run_git(&staged_args).unwrap_or_default();
                    let combined = combine_stats(&stat_text, &staged_stat);
                    if combined.trim().is_empty() {
                        println!("{DIM}  (no changes){RESET}\n");
                    } else {
                        let summary = parse_diff_stat(&combined);
                        let formatted = format_diff_stat(&summary);
                        if !formatted.is_empty() {
                            print!("{formatted}");
                        }
                    }
                } else if stat_text.trim().is_empty() {
                    println!("{DIM}  (no staged changes){RESET}\n");
                } else {
                    let summary = parse_diff_stat(&stat_text);
                    let formatted = format_diff_stat(&summary);
                    if !formatted.is_empty() {
                        print!("{formatted}");
                    }
                }
                return;
            }

            // --functions: show semantic-level change summary
            if opts.functions {
                handle_diff_functions(&opts);
                return;
            }

            // ── Staged-only mode ────────────────────────────────────
            if opts.staged_only {
                let mut stat_args = vec!["diff", "--cached", "--stat"];
                let stat_file_ref;
                if let Some(ref f) = opts.file {
                    stat_args.push("--");
                    stat_file_ref = f.as_str();
                    stat_args.push(stat_file_ref);
                }
                let stat_text = run_git(&stat_args).unwrap_or_default();

                if stat_text.trim().is_empty() {
                    println!("{DIM}  (no staged changes){RESET}\n");
                    return;
                }

                let summary = parse_diff_stat(&stat_text);
                let formatted = format_diff_stat(&summary);
                if !formatted.is_empty() {
                    print!("{formatted}");
                }

                // Full staged diff
                let mut diff_args = vec!["diff", "--cached"];
                let diff_file_ref;
                if let Some(ref f) = opts.file {
                    diff_args.push("--");
                    diff_file_ref = f.as_str();
                    diff_args.push(diff_file_ref);
                }
                let full_diff = run_git(&diff_args).unwrap_or_default();
                if !full_diff.trim().is_empty() {
                    println!("\n{DIM}  ── Staged diff ──{RESET}");
                    print!("{}", colorize_diff(&full_diff));
                    println!();
                }
                return;
            }

            // ── File-specific mode (unstaged + staged) ──────────────
            if let Some(ref file) = opts.file {
                let stat_text =
                    run_git(&["diff", "--stat", "--", file.as_str()]).unwrap_or_default();
                let staged_stat_text =
                    run_git(&["diff", "--cached", "--stat", "--", file.as_str()])
                        .unwrap_or_default();

                let combined_stat = combine_stats(&stat_text, &staged_stat_text);
                if combined_stat.trim().is_empty() {
                    println!("{DIM}  (no changes for {file}){RESET}\n");
                    return;
                }

                let summary = parse_diff_stat(&combined_stat);
                let formatted = format_diff_stat(&summary);
                if !formatted.is_empty() {
                    print!("{formatted}");
                }

                let full_diff = run_git(&["diff", "--", file.as_str()]).unwrap_or_default();
                let staged_diff =
                    run_git(&["diff", "--cached", "--", file.as_str()]).unwrap_or_default();
                let combined_diff = combine_stats(&full_diff, &staged_diff);
                if !combined_diff.trim().is_empty() {
                    println!("\n{DIM}  ── Diff for {file} ──{RESET}");
                    print!("{}", colorize_diff(&combined_diff));
                    println!();
                }
                return;
            }

            // ── Default: show all changes (original behavior) ───────
            let stat_text = run_git(&["diff", "--stat"]).unwrap_or_default();
            let staged_stat_text = run_git(&["diff", "--cached", "--stat"]).unwrap_or_default();

            // Show file status list
            println!("{DIM}  Changes:");
            for line in _status.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let (color, rest) = if trimmed.len() >= 2 {
                    match trimmed.chars().next().unwrap_or(' ') {
                        'M' | 'A' | 'R' => (format!("{GREEN}"), trimmed),
                        'D' => (format!("{RED}"), trimmed),
                        '?' => (format!("{YELLOW}"), trimmed),
                        _ => (format!("{DIM}"), trimmed),
                    }
                } else {
                    (format!("{DIM}"), trimmed)
                };
                println!("    {color}{rest}{RESET}");
            }
            println!("{RESET}");

            let combined_stat = combine_stats(&stat_text, &staged_stat_text);
            if !combined_stat.trim().is_empty() {
                let summary = parse_diff_stat(&combined_stat);
                let formatted = format_diff_stat(&summary);
                if !formatted.is_empty() {
                    print!("{formatted}");
                }
            }

            let full_diff = run_git(&["diff"]).unwrap_or_default();
            if !full_diff.trim().is_empty() {
                println!("\n{DIM}  ── Full diff ──{RESET}");
                print!("{}", colorize_diff(&full_diff));
                println!();
            }
        }
        _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
    }
}

/// Handle `/diff` when a ref range is specified (e.g. `main..feature`, `HEAD~3`, `v1.0`).
/// Runs `git diff <range>` directly — no staged/unstaged logic needed.
fn handle_diff_ref_range(opts: &DiffOptions, range: &str) {
    // Validate the ref(s) exist
    let refs_to_check: Vec<&str> = if range.contains("...") {
        range.splitn(2, "...").collect()
    } else if range.contains("..") {
        range.splitn(2, "..").collect()
    } else {
        vec![range]
    };
    for r in &refs_to_check {
        if !r.is_empty() {
            if let Err(_e) = run_git(&["rev-parse", "--verify", r]) {
                eprintln!("{RED}  error: unknown revision '{r}'{RESET}\n");
                return;
            }
        }
    }

    if opts.name_only {
        let mut args = vec!["diff", "--name-only", range];
        let file_ref;
        if let Some(ref f) = opts.file {
            args.push("--");
            file_ref = f.as_str();
            args.push(file_ref);
        }
        let names = run_git(&args).unwrap_or_default();
        if names.trim().is_empty() {
            println!("{DIM}  (no changed files in {range}){RESET}\n");
        } else {
            println!("{DIM}  Changed files ({range}):{RESET}");
            for f in names.lines().filter(|l| !l.trim().is_empty()) {
                println!("    {f}");
            }
            println!();
        }
        return;
    }

    if opts.stat_only {
        let mut args = vec!["diff", "--stat", range];
        let file_ref;
        if let Some(ref f) = opts.file {
            args.push("--");
            file_ref = f.as_str();
            args.push(file_ref);
        }
        let stat_text = run_git(&args).unwrap_or_default();
        if stat_text.trim().is_empty() {
            println!("{DIM}  (no changes in {range}){RESET}\n");
        } else {
            let summary = parse_diff_stat(&stat_text);
            let formatted = format_diff_stat(&summary);
            if !formatted.is_empty() {
                print!("{formatted}");
            }
        }
        return;
    }

    if opts.functions {
        handle_diff_functions(opts);
        return;
    }

    // Default: full diff with stat header
    let mut stat_args = vec!["diff", "--stat", range];
    let stat_file_ref;
    if let Some(ref f) = opts.file {
        stat_args.push("--");
        stat_file_ref = f.as_str();
        stat_args.push(stat_file_ref);
    }
    let stat_text = run_git(&stat_args).unwrap_or_default();
    if !stat_text.trim().is_empty() {
        let summary = parse_diff_stat(&stat_text);
        let formatted = format_diff_stat(&summary);
        if !formatted.is_empty() {
            print!("{formatted}");
        }
    }

    let mut diff_args = vec!["diff", range];
    let diff_file_ref;
    if let Some(ref f) = opts.file {
        diff_args.push("--");
        diff_file_ref = f.as_str();
        diff_args.push(diff_file_ref);
    }
    let full_diff = run_git(&diff_args).unwrap_or_default();
    if full_diff.trim().is_empty() {
        println!("{DIM}  (no changes in {range}){RESET}\n");
    } else {
        println!("\n{DIM}  ── Diff ({range}) ──{RESET}");
        print!("{}", colorize_diff(&full_diff));
        println!();
    }
}

/// Combine two stat/diff outputs, deduplicating if both are present.
fn combine_stats(a: &str, b: &str) -> String {
    if !a.trim().is_empty() && !b.trim().is_empty() {
        format!("{}\n{}", a, b)
    } else if !b.trim().is_empty() {
        b.to_string()
    } else {
        a.to_string()
    }
}

/// Maximum diff size (in bytes) to send for AI explanation.
const DIFF_EXPLAIN_MAX_BYTES: usize = 50_000;

/// Gather the current diff text based on options.
/// Returns the diff content or None if there are no changes.
fn gather_diff_text(opts: &DiffOptions) -> Option<String> {
    let mut diff_text;

    if let Some(ref range) = opts.ref_range {
        // Ref range mode: git diff <range>
        let mut args = vec!["diff", range.as_str()];
        let file_ref;
        if let Some(ref f) = opts.file {
            args.push("--");
            file_ref = f.as_str();
            args.push(file_ref);
        }
        diff_text = run_git(&args).unwrap_or_default();
    } else {
        // Working tree mode: check for changes first
        let status = run_git(&["status", "--short"]).unwrap_or_default();
        if status.trim().is_empty() {
            println!("{DIM}  (no uncommitted changes to explain){RESET}\n");
            return None;
        }

        if opts.staged_only {
            // Only staged changes
            let mut args = vec!["diff", "--cached"];
            let file_ref;
            if let Some(ref f) = opts.file {
                args.push("--");
                file_ref = f.as_str();
                args.push(file_ref);
            }
            diff_text = run_git(&args).unwrap_or_default();
        } else {
            // Both staged and unstaged
            let mut unstaged_args = vec!["diff"];
            let file_ref;
            if let Some(ref f) = opts.file {
                unstaged_args.push("--");
                file_ref = f.as_str();
                unstaged_args.push(file_ref);
            }
            let unstaged = run_git(&unstaged_args).unwrap_or_default();

            let mut staged_args = vec!["diff", "--cached"];
            let staged_file_ref;
            if let Some(ref f) = opts.file {
                staged_args.push("--");
                staged_file_ref = f.as_str();
                staged_args.push(staged_file_ref);
            }
            let staged = run_git(&staged_args).unwrap_or_default();

            if !unstaged.trim().is_empty() && !staged.trim().is_empty() {
                diff_text = format!("{unstaged}\n{staged}");
            } else if !staged.trim().is_empty() {
                diff_text = staged;
            } else {
                diff_text = unstaged;
            }
        }
    }

    if diff_text.trim().is_empty() {
        let scope = if opts.ref_range.is_some() {
            opts.ref_range.as_deref().unwrap_or("")
        } else if opts.staged_only {
            "staged "
        } else {
            ""
        };
        println!("{DIM}  (no {scope}changes to explain){RESET}\n");
        return None;
    }

    // Truncate if too large
    if diff_text.len() > DIFF_EXPLAIN_MAX_BYTES {
        let safe_len = safe_truncate(&diff_text, DIFF_EXPLAIN_MAX_BYTES).len();
        diff_text.truncate(safe_len);
        diff_text.push_str("\n\n... (diff truncated for context limit)");
    }

    Some(diff_text)
}

/// The semantic status of a symbol when comparing two versions.
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolChange {
    Added,
    Removed,
    Modified,
}

/// A single symbol-level change entry.
#[derive(Debug, Clone)]
pub struct SymbolDiff {
    pub name: String,
    pub kind: SymbolKind,
    pub change: SymbolChange,
}

/// Compare old and new symbol lists, returning semantic diffs.
///
/// Match symbols by (name, kind) pair:
/// - Present only in new → Added
/// - Present only in old → Removed
/// - Present in both but at a different line → Modified
pub fn compare_symbols(
    old_symbols: &[symbols::Symbol],
    new_symbols: &[symbols::Symbol],
) -> Vec<SymbolDiff> {
    // Build maps from (name, kind_tag) → line for old and new
    let old_map: HashMap<(&str, &SymbolKind), usize> = old_symbols
        .iter()
        .map(|s| ((s.name.as_str(), &s.kind), s.line))
        .collect();
    let new_map: HashMap<(&str, &SymbolKind), usize> = new_symbols
        .iter()
        .map(|s| ((s.name.as_str(), &s.kind), s.line))
        .collect();

    let mut diffs = Vec::new();

    // Check new symbols: added or modified
    for sym in new_symbols {
        let key = (sym.name.as_str(), &sym.kind);
        match old_map.get(&key) {
            None => diffs.push(SymbolDiff {
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                change: SymbolChange::Added,
            }),
            Some(&old_line) if old_line != sym.line => diffs.push(SymbolDiff {
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                change: SymbolChange::Modified,
            }),
            Some(_) => {} // unchanged
        }
    }

    // Check old symbols: removed
    for sym in old_symbols {
        let key = (sym.name.as_str(), &sym.kind);
        if !new_map.contains_key(&key) {
            diffs.push(SymbolDiff {
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                change: SymbolChange::Removed,
            });
        }
    }

    diffs
}

/// Format a `SymbolKind` as a short display label.
fn symbol_kind_label(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "fn",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Trait => "trait",
        SymbolKind::Interface => "interface",
        SymbolKind::Class => "class",
        SymbolKind::Type => "type",
        SymbolKind::Const => "const",
        SymbolKind::Impl => "impl",
        SymbolKind::Module => "mod",
        SymbolKind::Macro => "macro",
        SymbolKind::Namespace => "ns",
    }
}

/// Handle `/diff --functions`: show semantic-level change summary.
pub fn handle_diff_functions(opts: &DiffOptions) {
    // Get the list of changed files
    let all_files: Vec<String> = if let Some(ref range) = opts.ref_range {
        // Ref range mode: get files from git diff <range> --name-only
        let mut args = vec!["diff", "--name-only", range.as_str()];
        let file_ref;
        if let Some(ref f) = opts.file {
            args.push("--");
            file_ref = f.as_str();
            args.push(file_ref);
        }
        let names = run_git(&args).unwrap_or_default();
        names
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect()
    } else {
        // Working tree mode: combine unstaged + staged
        let mut args = vec!["diff", "--name-only"];
        if opts.staged_only {
            args.push("--cached");
        }
        let file_ref;
        if let Some(ref f) = opts.file {
            args.push("--");
            file_ref = f.as_str();
            args.push(file_ref);
        }
        let unstaged_names = run_git(&args).unwrap_or_default();

        if opts.staged_only {
            unstaged_names
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect()
        } else {
            let mut staged_args = vec!["diff", "--name-only", "--cached"];
            let staged_file_ref;
            if let Some(ref f) = opts.file {
                staged_args.push("--");
                staged_file_ref = f.as_str();
                staged_args.push(staged_file_ref);
            }
            let staged_names = run_git(&staged_args).unwrap_or_default();
            let mut files: Vec<String> = unstaged_names
                .lines()
                .chain(staged_names.lines())
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect();
            files.sort();
            files.dedup();
            files
        }
    };

    if all_files.is_empty() {
        println!("{DIM}  (no changed files){RESET}\n");
        return;
    }

    let mut total_added = 0usize;
    let mut total_modified = 0usize;
    let mut total_removed = 0usize;
    let mut files_with_changes = 0usize;
    let mut output = String::new();

    for file_path in &all_files {
        // Skip files with unrecognized language
        let language = match symbols::detect_language(file_path) {
            Some(lang) => lang,
            None => continue,
        };

        // Get old and new content depending on mode
        let (old_content, new_content) = if let Some(ref range) = opts.ref_range {
            // Ref range: extract left and right refs
            let (left_ref, right_ref) = if range.contains("...") {
                let parts: Vec<&str> = range.splitn(2, "...").collect();
                (parts[0].to_string(), parts[1].to_string())
            } else if range.contains("..") {
                let parts: Vec<&str> = range.splitn(2, "..").collect();
                (parts[0].to_string(), parts[1].to_string())
            } else {
                // Single ref like HEAD~3: compare <ref> to working tree
                (range.clone(), String::new())
            };

            let old_path = format!("{left_ref}:{file_path}");
            let old = run_git(&["show", &old_path]).unwrap_or_default();

            let new = if right_ref.is_empty() {
                // Compare against working tree
                std::fs::read_to_string(file_path).unwrap_or_default()
            } else {
                let new_path = format!("{right_ref}:{file_path}");
                run_git(&["show", &new_path]).unwrap_or_default()
            };
            (old, new)
        } else {
            // Working tree mode: current file vs HEAD
            let new_content = std::fs::read_to_string(file_path).unwrap_or_default();
            let git_path = format!("HEAD:{file_path}");
            let old_content = run_git(&["show", &git_path]).unwrap_or_default();
            (old_content, new_content)
        };

        let old_symbols = symbols::extract_symbols(&old_content, language);
        let new_symbols = symbols::extract_symbols(&new_content, language);
        let diffs = compare_symbols(&old_symbols, &new_symbols);

        if diffs.is_empty() {
            continue;
        }

        files_with_changes += 1;
        output.push_str(&format!("    {BOLD}{file_path}{RESET}\n"));

        for d in &diffs {
            let label = symbol_kind_label(&d.kind);
            match d.change {
                SymbolChange::Added => {
                    total_added += 1;
                    output.push_str(&format!(
                        "      {GREEN}Added:    {RESET} {label} {GREEN}{}{RESET}\n",
                        d.name
                    ));
                }
                SymbolChange::Modified => {
                    total_modified += 1;
                    output.push_str(&format!(
                        "      {YELLOW}Modified:{RESET} {label} {YELLOW}{}{RESET}\n",
                        d.name
                    ));
                }
                SymbolChange::Removed => {
                    total_removed += 1;
                    output.push_str(&format!(
                        "      {RED}Removed: {RESET} {label} {RED}{}{RESET}\n",
                        d.name
                    ));
                }
            }
        }
    }

    if files_with_changes == 0 {
        println!("{DIM}  (no semantic changes detected){RESET}\n");
        return;
    }

    println!("{DIM}  Semantic changes:{RESET}");
    print!("{output}");
    println!(
        "\n  {DIM}{} file{}, {GREEN}{} added{DIM}, {YELLOW}{} modified{DIM}, {RED}{} removed{RESET}\n",
        files_with_changes,
        if files_with_changes == 1 { "" } else { "s" },
        total_added,
        total_modified,
        total_removed,
    );
}

/// Handle `/diff --explain`: send the diff to the AI for a natural-language explanation.
/// Returns the prompt if sent, None otherwise.
pub async fn handle_diff_explain(
    input: &str,
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let opts = parse_diff_args(input);
    let diff_text = gather_diff_text(&opts)?;

    let scope = if opts.staged_only { "staged " } else { "" };
    let file_note = opts
        .file
        .as_ref()
        .map(|f| format!(" in `{f}`"))
        .unwrap_or_default();

    let prompt = format!(
        "Explain the following {scope}code changes{file_note}. \
         Describe what was changed, why it might have been changed, \
         and any potential issues. Be concise.\n\n\
         ```diff\n{diff_text}\n```"
    );

    run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);
    Some(prompt)
}

// ── /undo ────────────────────────────────────────────────────────────────

/// Build a context note describing what `/undo` reverted, for injection into
/// the agent's next turn so it knows files have changed under it.
fn build_undo_context(actions: &[String]) -> String {
    let count = actions.len();
    let file_word = crate::format::pluralize(count, "file", "files");
    let mut note =
        format!("[System note: /undo reverted {count} {file_word} from a previous turn:\n");
    for action in actions {
        note.push_str(&format!("- {action}\n"));
    }
    note.push_str(
        "⚠️ The code referenced in my previous response may no longer exist. \
         Re-read affected files before making new changes. \
         Verify current file state before continuing.]",
    );
    note
}

/// Handle `/undo` with per-turn granularity.
///
/// - `/undo` — undo the last agent turn (restore files to pre-turn state)
/// - `/undo N` — undo the last N turns
/// - `/undo --all` — nuclear option: revert ALL uncommitted changes (old behavior)
/// - `/undo --last-commit` — revert the most recent git commit via `git revert`
///
/// Returns `Some(context)` when files were actually reverted, so the REPL can
/// inject the summary into the agent's next turn for causal consistency.
pub fn handle_undo(input: &str, history: &mut TurnHistory) -> Option<String> {
    let arg = input.strip_prefix("/undo").unwrap_or("").trim();

    // Nuclear fallback: /undo --all
    if arg == "--all" {
        return handle_undo_all(history);
    }

    // Revert last git commit: /undo --last-commit
    if arg == "--last-commit" {
        return handle_undo_last_commit();
    }

    // Parse optional count: /undo N
    let count: usize = if arg.is_empty() {
        1
    } else if let Ok(n) = arg.parse::<usize>() {
        if n == 0 {
            println!("{DIM}  (nothing to undo — count is 0){RESET}\n");
            return None;
        }
        n
    } else {
        println!("{DIM}  usage: /undo [N] | --all | --last-commit{RESET}\n");
        return None;
    };

    if history.is_empty() {
        // Fallback: check if there are uncommitted changes we could undo with --all
        let has_diff = !run_git(&["diff", "--stat"])
            .unwrap_or_default()
            .trim()
            .is_empty();
        let has_untracked = !run_git(&["ls-files", "--others", "--exclude-standard"])
            .unwrap_or_default()
            .trim()
            .is_empty();

        if has_diff || has_untracked {
            println!("{DIM}  no turn history available, but there are uncommitted changes.{RESET}");
            println!("{DIM}  use /undo --all to revert everything (nuclear option){RESET}\n");
        } else {
            println!("{DIM}  (nothing to undo — no turn history){RESET}\n");
        }
        return None;
    }

    let available = history.len();
    let actual = count.min(available);
    let word = crate::format::pluralize(actual, "turn", "turns");

    // Show what will be undone
    println!("{DIM}  undoing last {actual} {word}...{RESET}");

    let actions = history.undo_last(actual);
    for action in &actions {
        println!("{DIM}    {action}{RESET}");
    }

    if actions.is_empty() {
        println!("{DIM}  (no files were modified in those turns){RESET}\n");
    } else {
        let file_word = crate::format::pluralize(actions.len(), "file", "files");
        println!(
            "{GREEN}  ✓ undid {actual} {word} ({} {file_word} affected){RESET}\n",
            actions.len()
        );
    }

    if count > available {
        println!(
            "{DIM}  (only {available} {} available, undid all){RESET}\n",
            crate::format::pluralize(available, "turn was", "turns were")
        );
    }

    // Return context for agent injection if any files were actually affected
    if !actions.is_empty() {
        Some(build_undo_context(&actions))
    } else {
        None
    }
}

/// Undo the most recent git commit using `git revert`.
///
/// Returns `Some(context)` with causality information so the agent knows
/// that earlier conversation may reference code that no longer exists.
fn handle_undo_last_commit() -> Option<String> {
    // 1. Get the last commit info
    let log = run_git(&["log", "--oneline", "-1"]).unwrap_or_default();
    if log.trim().is_empty() {
        println!("{DIM}  (no commits to undo){RESET}\n");
        return None;
    }

    // 2. Get the files changed in that commit
    let files = run_git(&["diff", "--name-only", "HEAD~1", "HEAD"]).unwrap_or_default();

    // 3. Show what will be undone
    println!("{DIM}  Reverting last commit: {}{RESET}", log.trim());

    // 4. Revert using git revert (keeps history, safer than reset)
    let result = run_git(&["revert", "HEAD", "--no-edit"]);
    match result {
        Ok(output) => {
            println!("{GREEN}  ✓ Reverted last commit{RESET}");
            if !output.trim().is_empty() {
                println!("{DIM}  {}{RESET}", output.trim());
            }
            println!();

            // Build context for agent
            let mut actions = Vec::new();
            for f in files.lines().filter(|l| !l.is_empty()) {
                actions.push(format!("reverted changes to {f} (commit undone)"));
            }

            // Enhanced context note that mentions journal/conversation inconsistency
            let mut note =
                String::from("[System note: /undo --last-commit reverted a git commit.\n");
            note.push_str(&format!("Reverted commit: {}\n", log.trim()));
            note.push_str("Files affected:\n");
            for action in &actions {
                note.push_str(&format!("- {action}\n"));
            }
            note.push_str(
                "⚠️ Earlier messages in this conversation may reference code from this commit \
                 that no longer exists. Verify current file state before continuing.\n",
            );
            note.push_str(
                "Any journal entries about this commit describe work that has been undone.]",
            );

            Some(note)
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Revert failed: {e}{RESET}");
            eprintln!("{DIM}  (the commit may have conflicts — try manual git revert){RESET}\n");
            None
        }
    }
}

/// Nuclear undo: revert ALL uncommitted changes (old behavior).
/// Clears turn history as well.
///
/// Returns `Some(context)` when changes were actually reverted.
fn handle_undo_all(history: &mut TurnHistory) -> Option<String> {
    let diff_stat = run_git(&["diff", "--stat"]).unwrap_or_default();
    let untracked_text =
        run_git(&["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();

    let has_diff = !diff_stat.is_empty();
    let untracked_files: Vec<String> = untracked_text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    let has_untracked = !untracked_files.is_empty();

    if !has_diff && !has_untracked {
        println!("{DIM}  (nothing to undo — no uncommitted changes){RESET}\n");
        history.clear();
        return None;
    }

    // Collect action descriptions for the context note
    let mut actions = Vec::new();

    if has_diff {
        println!("{DIM}{diff_stat}{RESET}");
        // Parse which files were modified from the diff stat
        let stat = parse_diff_stat(&diff_stat);
        for entry in &stat.entries {
            actions.push(format!("restored {} (to last committed state)", entry.file));
        }
    }
    if has_untracked {
        println!("{DIM}  untracked files:");
        for f in &untracked_files {
            println!("    {f}");
            actions.push(format!("deleted {f} (was untracked)"));
        }
        println!("{RESET}");
    }

    if has_diff {
        if let Err(e) = run_git(&["checkout", "--", "."]) {
            eprintln!("{RED}  ✗ failed to revert tracked changes: {e}{RESET}\n");
            return None;
        }
    }
    if has_untracked {
        if let Err(e) = run_git(&["clean", "-fd"]) {
            eprintln!("{RED}  ✗ failed to clean untracked files: {e}{RESET}\n");
            // Tracked changes were already reverted, so continue
        }
    }
    println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");

    // Clear turn history since everything is now reverted
    history.clear();

    if !actions.is_empty() {
        Some(build_undo_context(&actions))
    } else {
        None
    }
}

// ── /commit ──────────────────────────────────────────────────────────────

/// Parsed commit arguments: flags (`-a`/`--all`, `--ai`/`--generate`) and the remaining message.
#[derive(Debug, PartialEq)]
pub(crate) struct CommitArgs {
    /// Auto-stage tracked modified files before committing (`-a` / `--all`).
    pub auto_stage: bool,
    /// Use AI to generate the commit message (`--ai` / `--generate`).
    pub ai: bool,
    /// Amend the last commit instead of creating a new one (`--amend`).
    pub amend: bool,
    /// Preview what would be committed without actually committing (`--dry-run`).
    pub dry_run: bool,
    /// The remaining commit message (if any) after stripping flags.
    pub message: String,
}

/// Parse the raw argument string after `/commit` into structured [`CommitArgs`].
///
/// Recognises `-a`, `--all`, `--ai`, and `--generate` in any position.
pub(crate) fn parse_commit_args(arg: &str) -> CommitArgs {
    let mut auto_stage = false;
    let mut ai = false;
    let mut amend = false;
    let mut dry_run = false;
    let mut message_parts: Vec<&str> = Vec::new();

    for token in arg.split_whitespace() {
        match token {
            "-a" | "--all" => auto_stage = true,
            "--ai" | "--generate" => ai = true,
            "--amend" => amend = true,
            "--dry-run" => dry_run = true,
            other => message_parts.push(other),
        }
    }

    CommitArgs {
        auto_stage,
        ai,
        amend,
        dry_run,
        message: message_parts.join(" "),
    }
}

/// Run `git add -u` to stage all tracked modified/deleted files.
///
/// Returns `true` if staging succeeded and there is something staged afterward,
/// `false` if there are no tracked changes to stage.
fn auto_stage_tracked() -> bool {
    // Run `git add -u` — stages modifications and deletions of tracked files
    if let Err(e) = run_git(&["add", "-u"]) {
        eprintln!("{RED}  error staging files: {e}{RESET}\n");
        return false;
    }
    // Check whether anything is actually staged now
    match get_staged_diff() {
        Some(diff) if !diff.trim().is_empty() => true,
        _ => {
            println!("{DIM}  nothing to commit — no tracked files have changes{RESET}\n");
            false
        }
    }
}

/// Run `git commit --amend` with a new message, including the co-authored trailer.
fn run_git_amend_with_message(message: &str) -> (bool, String) {
    let with_trailer = append_co_authored_trailer(message);
    match std::process::Command::new("git")
        .args(["commit", "--amend", "-m", &with_trailer])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let text = if stdout.is_empty() { stderr } else { stdout };
            (output.status.success(), text)
        }
        Err(e) => (false, format!("error: {e}")),
    }
}

/// Run `git commit --amend --no-edit` to amend without changing the message.
fn run_git_amend_no_edit() -> (bool, String) {
    match std::process::Command::new("git")
        .args(["commit", "--amend", "--no-edit"])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let text = if stdout.is_empty() { stderr } else { stdout };
            (output.status.success(), text)
        }
        Err(e) => (false, format!("error: {e}")),
    }
}

/// Get the message of the last commit.
fn get_last_commit_message() -> Option<String> {
    run_git(&["log", "-1", "--format=%B"])
        .ok()
        .map(|s| s.trim().to_string())
}

/// Display a dry-run preview of what would be committed.
///
/// Shows the diff stat (staged if available, otherwise unstaged + untracked)
/// and an optional commit message. Returns without running `git commit`.
fn print_dry_run_preview(message: Option<&str>) {
    // Prefer staged diff stat; fall back to unstaged + untracked
    let stat_output = run_git(&["diff", "--cached", "--stat"])
        .ok()
        .filter(|s| !s.trim().is_empty());

    let stat = if let Some(s) = stat_output {
        s
    } else {
        let mut combined = String::new();
        if let Ok(unstaged) = run_git(&["diff", "--stat"]) {
            if !unstaged.trim().is_empty() {
                combined.push_str(&unstaged);
            }
        }
        if let Ok(untracked) = run_git(&["ls-files", "--others", "--exclude-standard"]) {
            for f in untracked.lines() {
                let f = f.trim();
                if !f.is_empty() {
                    combined.push_str(&format!("  {f} (new)\n"));
                }
            }
        }
        combined
    };

    println!("\n{BOLD}  Dry run — would commit:{RESET}");
    if stat.trim().is_empty() {
        println!("{DIM}  (no changes detected){RESET}");
    } else {
        for line in stat.lines() {
            println!("  {DIM}{line}{RESET}");
        }
    }

    if let Some(msg) = message {
        if !msg.is_empty() {
            println!("\n  {DIM}Message:{RESET} \"{BOLD}{msg}{RESET}\"");
        }
    }

    println!("\n  {DIM}(use /commit without --dry-run to actually commit){RESET}\n");
}

pub fn handle_commit(input: &str) {
    let arg = input.strip_prefix("/commit").unwrap_or("").trim();
    let parsed = parse_commit_args(arg);

    // Auto-stage tracked files when `-a`/`--all` is present
    if parsed.auto_stage && !auto_stage_tracked() {
        return;
    }

    if parsed.dry_run {
        let msg = if parsed.message.is_empty() {
            // Generate a heuristic message from the staged diff for preview
            get_staged_diff()
                .filter(|d| !d.trim().is_empty())
                .map(|d| generate_commit_message(&d))
        } else {
            Some(parsed.message.clone())
        };
        print_dry_run_preview(msg.as_deref());
        return;
    }

    if parsed.amend {
        handle_commit_amend(&parsed);
    } else if !parsed.message.is_empty() {
        let (ok, output) = run_git_commit_with_trailer(&parsed.message);
        if ok {
            println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
            crate::commands_risk::auto_risk_snapshot();
        } else {
            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
        }
    } else {
        match get_staged_diff() {
            None => {
                eprintln!("{RED}  error: not in a git repository{RESET}\n");
            }
            Some(diff) if diff.trim().is_empty() => {
                println!("{DIM}  nothing staged — use `git add` first{RESET}");
                println!("{DIM}  tip: use /commit -a to auto-stage tracked files{RESET}\n");
            }
            Some(diff) => {
                let suggested = generate_commit_message(&diff);
                println!("{DIM}  Suggested commit message:{RESET}");
                println!("    {BOLD}{suggested}{RESET}");
                eprint!(
                    "\n  {DIM}({GREEN}y{RESET}{DIM})es / ({RED}n{RESET}{DIM})o / ({CYAN}e{RESET}{DIM})dit: {RESET}"
                );
                io::stderr().flush().ok();
                let mut response = String::new();
                if io::stdin().read_line(&mut response).is_ok() {
                    let response = response.trim().to_lowercase();
                    match response.as_str() {
                        "y" | "yes" | "" => {
                            let (ok, output) = run_git_commit_with_trailer(&suggested);
                            if ok {
                                println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                                crate::commands_risk::auto_risk_snapshot();
                            } else {
                                eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                            }
                        }
                        "e" | "edit" => {
                            println!("{DIM}  Enter your commit message:{RESET}");
                            eprint!("  > ");
                            io::stderr().flush().ok();
                            let mut custom_msg = String::new();
                            if io::stdin().read_line(&mut custom_msg).is_ok() {
                                let custom_msg = custom_msg.trim();
                                if custom_msg.is_empty() {
                                    println!("{DIM}  (commit cancelled — empty message){RESET}\n");
                                } else {
                                    let (ok, output) = run_git_commit_with_trailer(custom_msg);
                                    if ok {
                                        println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                                        crate::commands_risk::auto_risk_snapshot();
                                    } else {
                                        eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                                    }
                                }
                            }
                        }
                        _ => {
                            println!("{DIM}  (commit cancelled){RESET}\n");
                        }
                    }
                }
            }
        }
    }
}

/// Handle `--amend` variant of `/commit`.
///
/// Behaviour:
/// - With a message: amend and replace the commit message.
/// - Without a message but with staged changes: show current message, ask
///   whether to keep/edit it, then amend.
/// - Without a message and no staged changes: amend with `--no-edit` (useful
///   after `git add` of a forgotten file).
fn handle_commit_amend(parsed: &CommitArgs) {
    if !parsed.message.is_empty() {
        // Amend with a new message
        let (ok, output) = run_git_amend_with_message(&parsed.message);
        if ok {
            println!("{GREEN}  ✓ (amended) {}{RESET}\n", output.trim());
            crate::commands_risk::auto_risk_snapshot();
        } else {
            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
        }
        return;
    }

    // No explicit message — check for staged changes
    let has_staged = matches!(get_staged_diff(), Some(diff) if !diff.trim().is_empty());

    if has_staged {
        // Show current commit message and ask whether to keep or edit
        let current_msg = get_last_commit_message().unwrap_or_default();
        println!("{DIM}  Current commit message:{RESET}");
        println!("    {BOLD}{current_msg}{RESET}");
        eprint!(
            "\n  {DIM}({GREEN}k{RESET}{DIM})eep / ({CYAN}e{RESET}{DIM})dit / ({RED}c{RESET}{DIM})ancel: {RESET}"
        );
        io::stderr().flush().ok();
        let mut response = String::new();
        if io::stdin().read_line(&mut response).is_ok() {
            let response = response.trim().to_lowercase();
            match response.as_str() {
                "k" | "keep" | "" => {
                    let (ok, output) = run_git_amend_no_edit();
                    if ok {
                        println!("{GREEN}  ✓ (amended) {}{RESET}\n", output.trim());
                        crate::commands_risk::auto_risk_snapshot();
                    } else {
                        eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                    }
                }
                "e" | "edit" => {
                    println!("{DIM}  Enter new commit message:{RESET}");
                    eprint!("  > ");
                    io::stderr().flush().ok();
                    let mut custom_msg = String::new();
                    if io::stdin().read_line(&mut custom_msg).is_ok() {
                        let custom_msg = custom_msg.trim();
                        if custom_msg.is_empty() {
                            println!("{DIM}  (amend cancelled — empty message){RESET}\n");
                        } else {
                            let (ok, output) = run_git_amend_with_message(custom_msg);
                            if ok {
                                println!("{GREEN}  ✓ (amended) {}{RESET}\n", output.trim());
                                crate::commands_risk::auto_risk_snapshot();
                            } else {
                                eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                            }
                        }
                    }
                }
                _ => {
                    println!("{DIM}  (amend cancelled){RESET}\n");
                }
            }
        }
    } else {
        // No staged changes — amend with --no-edit (e.g. after manually staging)
        let (ok, output) = run_git_amend_no_edit();
        if ok {
            println!("{GREEN}  ✓ (amended) {}{RESET}\n", output.trim());
            crate::commands_risk::auto_risk_snapshot();
        } else {
            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
        }
    }
}

// ── /pr ──────────────────────────────────────────────────────────────────

/// Maximum diff size (in bytes) sent to the AI for commit message generation.
const COMMIT_AI_MAX_BYTES: usize = 30_000;

/// Build the prompt sent to the side agent for AI commit message generation.
///
/// This is a pure function — easy to test without an actual agent.
pub(crate) fn build_commit_ai_prompt(diff: &str) -> String {
    let truncated = if diff.len() > COMMIT_AI_MAX_BYTES {
        let t = safe_truncate(diff, COMMIT_AI_MAX_BYTES);
        format!("{}\n\n... (diff truncated, {} total bytes)", t, diff.len())
    } else {
        diff.to_string()
    };

    format!(
        "Generate a concise git commit message for the following diff.\n\
         Use conventional commit format: type(scope): description\n\
         Types: feat, fix, refactor, docs, test, chore, style, perf\n\
         The message MUST be a single line, max 72 characters.\n\
         Output ONLY the commit message — no quotes, no backticks, no explanation.\n\n\
         ```diff\n{truncated}\n```"
    )
}

/// Extract a clean commit message from AI output.
///
/// Strips markdown formatting, quotes, and extra whitespace that the model
/// might include despite instructions.
fn clean_ai_commit_message(raw: &str) -> String {
    let msg = raw
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim();
    // Take only the first line in case the model returns multiple
    let first_line = msg.lines().next().unwrap_or("").trim();
    // Strip any leading "commit message:" or similar preamble
    let cleaned = first_line
        .strip_prefix("commit message:")
        .or_else(|| first_line.strip_prefix("Commit message:"))
        .or_else(|| first_line.strip_prefix("Commit Message:"))
        .unwrap_or(first_line)
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim();
    cleaned.to_string()
}

/// Handle `/commit --ai` — generate a commit message using a side agent.
///
/// Falls back to the heuristic `generate_commit_message` if the AI returns
/// an empty response.
pub async fn handle_commit_ai(input: &str, agent_config: &AgentConfig) {
    let arg = input.strip_prefix("/commit").unwrap_or("").trim();
    let parsed = parse_commit_args(arg);

    // Auto-stage tracked files when `-a`/`--all` is present
    if parsed.auto_stage && !auto_stage_tracked() {
        return;
    }

    if !parsed.message.is_empty() {
        if parsed.dry_run {
            print_dry_run_preview(Some(&parsed.message));
            return;
        }
        // User gave a message alongside --ai — just use it directly
        let (ok, output) = run_git_commit_with_trailer(&parsed.message);
        if ok {
            println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
            crate::commands_risk::auto_risk_snapshot();
        } else {
            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
        }
        return;
    }

    // Get staged diff
    let diff = match get_staged_diff() {
        None => {
            eprintln!("{RED}  error: not in a git repository{RESET}\n");
            return;
        }
        Some(d) if d.trim().is_empty() => {
            println!("{DIM}  nothing staged — use `git add` first{RESET}");
            println!("{DIM}  tip: use /commit -a to auto-stage tracked files{RESET}\n");
            return;
        }
        Some(d) => d,
    };

    eprintln!("{DIM}  generating commit message...{RESET}");

    let prompt = build_commit_ai_prompt(&diff);
    let mut side_agent = agent_config.build_side_agent();
    let mut rx = side_agent.prompt(&prompt).await;

    let mut message = String::new();
    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                message.push_str(&delta);
            }
            Some(AgentEvent::AgentEnd { .. }) | None => break,
            _ => {}
        }
    }
    side_agent.finish().await;

    let message = clean_ai_commit_message(&message);

    // Fall back to heuristic if AI returned nothing useful
    let suggested = if message.is_empty() {
        eprintln!("{DIM}  (AI returned empty — falling back to heuristic){RESET}");
        generate_commit_message(&diff)
    } else {
        message
    };

    if parsed.dry_run {
        print_dry_run_preview(Some(&suggested));
        println!("  {DIM}(dry run — use /commit ai to commit with this message){RESET}\n");
        return;
    }

    println!("{DIM}  Suggested commit message:{RESET}");
    println!("    {BOLD}{suggested}{RESET}");
    eprint!(
        "\n  {DIM}({GREEN}y{RESET}{DIM})es / ({RED}n{RESET}{DIM})o / ({CYAN}e{RESET}{DIM})dit: {RESET}"
    );
    io::stderr().flush().ok();

    let mut response = String::new();
    if io::stdin().read_line(&mut response).is_ok() {
        let response = response.trim().to_lowercase();
        match response.as_str() {
            "y" | "yes" | "" => {
                let (ok, output) = run_git_commit_with_trailer(&suggested);
                if ok {
                    println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                    crate::commands_risk::auto_risk_snapshot();
                } else {
                    eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                }
            }
            "e" | "edit" => {
                println!("{DIM}  Enter your commit message:{RESET}");
                eprint!("  > ");
                io::stderr().flush().ok();
                let mut custom_msg = String::new();
                if io::stdin().read_line(&mut custom_msg).is_ok() {
                    let custom_msg = custom_msg.trim();
                    if custom_msg.is_empty() {
                        println!("{DIM}  (commit cancelled — empty message){RESET}\n");
                    } else {
                        let (ok, output) = run_git_commit_with_trailer(custom_msg);
                        if ok {
                            println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                            crate::commands_risk::auto_risk_snapshot();
                        } else {
                            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                        }
                    }
                }
            }
            _ => {
                println!("{DIM}  (commit cancelled){RESET}\n");
            }
        }
    }
}

/// Returns `true` if the input contains `--ai` or `--generate` flags.
pub fn wants_ai_commit(input: &str) -> bool {
    let arg = input.strip_prefix("/commit").unwrap_or("").trim();
    parse_commit_args(arg).ai
}

// ── /git ─────────────────────────────────────────────────────────────────

pub fn handle_git(input: &str) {
    let arg = input.strip_prefix("/git").unwrap_or("").trim();
    let subcmd = parse_git_args(arg);
    run_git_subcommand(&subcmd);
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
    use serial_test::serial;

    // ── parse_diff_stat tests ───────────────────────────────────────────

    #[test]
    fn parse_diff_stat_single_file() {
        let input =
            " src/main.rs | 10 +++++++---\n 1 file changed, 7 insertions(+), 3 deletions(-)\n";
        let summary = parse_diff_stat(input);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "src/main.rs");
        assert_eq!(summary.entries[0].insertions, 7);
        assert_eq!(summary.entries[0].deletions, 3);
        assert_eq!(summary.total_insertions, 7);
        assert_eq!(summary.total_deletions, 3);
    }

    #[test]
    fn parse_diff_stat_multiple_files() {
        let input = "\
 src/commands.rs | 42 +++++++++++++++++++++---------------------
 src/main.rs     |  5 ++---
 src/cli.rs      | 12 ++++++++++++
 3 files changed, 25 insertions(+), 10 deletions(-)
";
        let summary = parse_diff_stat(input);
        assert_eq!(summary.entries.len(), 3);

        assert_eq!(summary.entries[0].file, "src/commands.rs");
        assert_eq!(summary.entries[1].file, "src/main.rs");
        assert_eq!(summary.entries[2].file, "src/cli.rs");

        // The visual bar has + and - characters, so counts come from those
        assert!(summary.entries[0].insertions > 0);
        assert!(summary.entries[0].deletions > 0);
        assert!(
            summary.entries[2].deletions == 0,
            "cli.rs is insertions only"
        );

        // Summary line totals
        assert_eq!(summary.total_insertions, 25);
        assert_eq!(summary.total_deletions, 10);
    }

    #[test]
    fn parse_diff_stat_insertions_only() {
        let input = " new_file.rs | 20 ++++++++++++++++++++\n 1 file changed, 20 insertions(+)\n";
        let summary = parse_diff_stat(input);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "new_file.rs");
        assert_eq!(summary.entries[0].insertions, 20);
        assert_eq!(summary.entries[0].deletions, 0);
        assert_eq!(summary.total_insertions, 20);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn parse_diff_stat_deletions_only() {
        let input = " old_file.rs | 8 --------\n 1 file changed, 8 deletions(-)\n";
        let summary = parse_diff_stat(input);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "old_file.rs");
        assert_eq!(summary.entries[0].insertions, 0);
        assert_eq!(summary.entries[0].deletions, 8);
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 8);
    }

    #[test]
    fn parse_diff_stat_empty_input() {
        let summary = parse_diff_stat("");
        assert_eq!(summary.entries.len(), 0);
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn parse_diff_stat_whitespace_only() {
        let summary = parse_diff_stat("   \n  \n\n");
        assert_eq!(summary.entries.len(), 0);
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn parse_diff_stat_no_summary_line() {
        // Sometimes git output might not include the summary line
        let input = " src/lib.rs | 3 +++\n";
        let summary = parse_diff_stat(input);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].insertions, 3);
        assert_eq!(summary.entries[0].deletions, 0);
        // Without a summary line, totals are computed from entries
        assert_eq!(summary.total_insertions, 3);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn parse_diff_stat_binary_file() {
        let input = " assets/logo.png | Bin 0 -> 1234 bytes\n 1 file changed, 0 insertions(+), 0 deletions(-)\n";
        let summary = parse_diff_stat(input);
        // Binary file lines still have a pipe, so they're parsed as entries
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "assets/logo.png");
        // "Bin 0 -> 1234 bytes" — the parser counts literal + and - chars
        // The "->" contains one '-', so deletions=1
        assert_eq!(summary.entries[0].insertions, 0);
        assert_eq!(summary.entries[0].deletions, 1);
        // Summary line says 0/0, but the fallback path recomputes from entries
        // when both summary totals are zero, so total_deletions picks up the entry's 1
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 1);
    }

    // ── format_diff_stat tests ──────────────────────────────────────────

    #[test]
    fn format_diff_stat_empty_entries() {
        let summary = DiffStatSummary {
            entries: vec![],
            total_insertions: 0,
            total_deletions: 0,
        };
        let output = format_diff_stat(&summary);
        assert!(
            output.is_empty(),
            "Empty entries should produce empty output"
        );
    }

    #[test]
    fn format_diff_stat_single_entry_insertions_only() {
        let summary = DiffStatSummary {
            entries: vec![DiffStatEntry {
                file: "src/main.rs".to_string(),
                insertions: 10,
                deletions: 0,
            }],
            total_insertions: 10,
            total_deletions: 0,
        };
        let output = format_diff_stat(&summary);
        assert!(output.contains("src/main.rs"), "Should contain filename");
        assert!(output.contains("+10"), "Should show insertions count");
        assert!(!output.contains("-0"), "Should not show zero deletions");
        assert!(output.contains("1 file changed"), "Should show summary");
        assert!(output.contains("+10"), "Summary should show insertions");
    }

    #[test]
    fn format_diff_stat_single_entry_deletions_only() {
        let summary = DiffStatSummary {
            entries: vec![DiffStatEntry {
                file: "old.rs".to_string(),
                insertions: 0,
                deletions: 5,
            }],
            total_insertions: 0,
            total_deletions: 5,
        };
        let output = format_diff_stat(&summary);
        assert!(output.contains("old.rs"), "Should contain filename");
        assert!(output.contains("-5"), "Should show deletions count");
        assert!(!output.contains("+0"), "Should not show zero insertions");
    }

    #[test]
    fn format_diff_stat_mixed_changes() {
        let summary = DiffStatSummary {
            entries: vec![
                DiffStatEntry {
                    file: "src/a.rs".to_string(),
                    insertions: 20,
                    deletions: 5,
                },
                DiffStatEntry {
                    file: "src/b.rs".to_string(),
                    insertions: 3,
                    deletions: 0,
                },
            ],
            total_insertions: 23,
            total_deletions: 5,
        };
        let output = format_diff_stat(&summary);
        assert!(output.contains("src/a.rs"), "Should contain first file");
        assert!(output.contains("src/b.rs"), "Should contain second file");
        assert!(
            output.contains("2 files changed"),
            "Should pluralize 'files'"
        );
        assert!(
            output.contains("+23"),
            "Summary should show total insertions"
        );
        assert!(output.contains("-5"), "Summary should show total deletions");
    }

    #[test]
    fn format_diff_stat_singular_file() {
        let summary = DiffStatSummary {
            entries: vec![DiffStatEntry {
                file: "f.rs".to_string(),
                insertions: 1,
                deletions: 1,
            }],
            total_insertions: 1,
            total_deletions: 1,
        };
        let output = format_diff_stat(&summary);
        assert!(
            output.contains("1 file changed"),
            "Should use singular 'file' not 'files'"
        );
    }

    // ── parse_pr_args tests ─────────────────────────────────────────────

    // ── DiffStatEntry / DiffStatSummary equality ────────────────────────

    #[test]
    fn diff_stat_entry_equality() {
        let a = DiffStatEntry {
            file: "a.rs".to_string(),
            insertions: 5,
            deletions: 3,
        };
        let b = DiffStatEntry {
            file: "a.rs".to_string(),
            insertions: 5,
            deletions: 3,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn diff_stat_summary_round_trip() {
        // Parse real git output, format it, verify structure
        let input = "\
 src/main.rs | 15 +++++++++------
 Cargo.toml  |  2 +-
 2 files changed, 10 insertions(+), 5 deletions(-)
";
        let summary = parse_diff_stat(input);
        let formatted = format_diff_stat(&summary);

        // Formatted output should contain both filenames
        assert!(formatted.contains("src/main.rs"));
        assert!(formatted.contains("Cargo.toml"));
        // Should contain "2 files changed"
        assert!(formatted.contains("2 files changed"));
    }

    // ── parse_diff_args tests ────────────────────────────────────────────

    #[test]
    fn test_parse_diff_args_empty() {
        let opts = parse_diff_args("/diff");
        assert!(!opts.staged_only);
        assert!(!opts.name_only);
        assert!(!opts.stat_only);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_staged() {
        let opts = parse_diff_args("/diff --staged");
        assert!(opts.staged_only);
        assert!(!opts.name_only);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_cached() {
        let opts = parse_diff_args("/diff --cached");
        assert!(opts.staged_only, "--cached should be an alias for --staged");
        assert!(!opts.name_only);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_name_only() {
        let opts = parse_diff_args("/diff --name-only");
        assert!(!opts.staged_only);
        assert!(opts.name_only);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_file() {
        let opts = parse_diff_args("/diff src/main.rs");
        assert!(!opts.staged_only);
        assert!(!opts.name_only);
        assert_eq!(opts.file, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_parse_diff_args_staged_and_file() {
        let opts = parse_diff_args("/diff --staged src/main.rs");
        assert!(opts.staged_only);
        assert!(!opts.name_only);
        assert_eq!(opts.file, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_parse_diff_args_all_flags() {
        let opts = parse_diff_args("/diff --staged --name-only --stat src/main.rs");
        assert!(opts.staged_only);
        assert!(opts.name_only);
        assert!(opts.stat_only);
        assert_eq!(opts.file, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_parse_diff_args_stat() {
        let opts = parse_diff_args("/diff --stat");
        assert!(!opts.staged_only);
        assert!(!opts.name_only);
        assert!(opts.stat_only);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_staged_stat() {
        let opts = parse_diff_args("/diff --staged --stat");
        assert!(opts.staged_only);
        assert!(!opts.name_only);
        assert!(opts.stat_only);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_stat_with_file() {
        let opts = parse_diff_args("/diff --stat src/tools.rs");
        assert!(!opts.staged_only);
        assert!(opts.stat_only);
        assert_eq!(opts.file, Some("src/tools.rs".to_string()));
    }

    #[test]
    fn test_parse_diff_args_explain() {
        let opts = parse_diff_args("/diff --explain");
        assert!(!opts.staged_only);
        assert!(!opts.name_only);
        assert!(!opts.stat_only);
        assert!(opts.explain);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_staged_explain() {
        let opts = parse_diff_args("/diff --staged --explain");
        assert!(opts.staged_only);
        assert!(opts.explain);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_explain_with_file() {
        let opts = parse_diff_args("/diff --explain src/main.rs");
        assert!(opts.explain);
        assert!(!opts.staged_only);
        assert_eq!(opts.file, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_parse_diff_args_functions() {
        let opts = parse_diff_args("/diff --functions");
        assert!(opts.functions);
        assert!(!opts.staged_only);
        assert!(!opts.name_only);
        assert!(!opts.explain);
        assert_eq!(opts.file, None);
    }

    #[test]
    fn test_parse_diff_args_functions_staged() {
        let opts = parse_diff_args("/diff --functions --staged");
        assert!(opts.functions);
        assert!(opts.staged_only);
    }

    #[test]
    fn test_parse_diff_args_functions_with_file() {
        let opts = parse_diff_args("/diff --functions src/main.rs");
        assert!(opts.functions);
        assert_eq!(opts.file, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_compare_symbols_added() {
        use crate::symbols::{Symbol, SymbolKind};
        let old = vec![];
        let new = vec![Symbol {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            is_public: true,
            line: 1,
        }];
        let diffs = compare_symbols(&old, &new);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].name, "foo");
        assert_eq!(diffs[0].change, SymbolChange::Added);
    }

    #[test]
    fn test_compare_symbols_removed() {
        use crate::symbols::{Symbol, SymbolKind};
        let old = vec![Symbol {
            name: "bar".to_string(),
            kind: SymbolKind::Struct,
            is_public: true,
            line: 5,
        }];
        let new = vec![];
        let diffs = compare_symbols(&old, &new);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].name, "bar");
        assert_eq!(diffs[0].change, SymbolChange::Removed);
    }

    #[test]
    fn test_compare_symbols_modified() {
        use crate::symbols::{Symbol, SymbolKind};
        let old = vec![Symbol {
            name: "baz".to_string(),
            kind: SymbolKind::Function,
            is_public: true,
            line: 10,
        }];
        let new = vec![Symbol {
            name: "baz".to_string(),
            kind: SymbolKind::Function,
            is_public: true,
            line: 20,
        }];
        let diffs = compare_symbols(&old, &new);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].name, "baz");
        assert_eq!(diffs[0].change, SymbolChange::Modified);
    }

    #[test]
    fn test_compare_symbols_unchanged() {
        use crate::symbols::{Symbol, SymbolKind};
        let old = vec![Symbol {
            name: "unchanged".to_string(),
            kind: SymbolKind::Function,
            is_public: true,
            line: 5,
        }];
        let new = vec![Symbol {
            name: "unchanged".to_string(),
            kind: SymbolKind::Function,
            is_public: true,
            line: 5,
        }];
        let diffs = compare_symbols(&old, &new);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_compare_symbols_mixed() {
        use crate::symbols::{Symbol, SymbolKind};
        let old = vec![
            Symbol {
                name: "kept".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            },
            Symbol {
                name: "removed_fn".to_string(),
                kind: SymbolKind::Function,
                is_public: false,
                line: 10,
            },
            Symbol {
                name: "moved_fn".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 20,
            },
        ];
        let new = vec![
            Symbol {
                name: "kept".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 1,
            },
            Symbol {
                name: "moved_fn".to_string(),
                kind: SymbolKind::Function,
                is_public: true,
                line: 30,
            },
            Symbol {
                name: "new_struct".to_string(),
                kind: SymbolKind::Struct,
                is_public: true,
                line: 40,
            },
        ];
        let diffs = compare_symbols(&old, &new);
        // moved_fn changed line: Modified, new_struct: Added, removed_fn: Removed
        assert_eq!(diffs.len(), 3);
        let modified: Vec<_> = diffs
            .iter()
            .filter(|d| d.change == SymbolChange::Modified)
            .collect();
        let added: Vec<_> = diffs
            .iter()
            .filter(|d| d.change == SymbolChange::Added)
            .collect();
        let removed: Vec<_> = diffs
            .iter()
            .filter(|d| d.change == SymbolChange::Removed)
            .collect();
        assert_eq!(modified.len(), 1);
        assert_eq!(modified[0].name, "moved_fn");
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].name, "new_struct");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].name, "removed_fn");
    }

    // ── PR tests (moved from commands.rs) ───────────────────────────────

    #[test]
    fn test_init_command_recognized() {
        assert!(!is_unknown_command("/init"));
        assert!(
            KNOWN_COMMANDS.contains(&"/init"),
            "/init should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_parse_diff_stat_basic() {
        let stat_output = " src/commands.rs | 42 ++++++++++++++++++++++++++++--------------
 src/main.rs     |  8 +++++---
 2 files changed, 30 insertions(+), 20 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 2);
        assert_eq!(summary.entries[0].file, "src/commands.rs");
        assert_eq!(summary.entries[1].file, "src/main.rs");
        assert_eq!(summary.total_insertions, 30);
        assert_eq!(summary.total_deletions, 20);
    }

    #[test]
    fn test_parse_diff_stat_single_file() {
        let stat_output = " src/format.rs | 10 +++++++---
 1 file changed, 7 insertions(+), 3 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "src/format.rs");
        assert_eq!(summary.total_insertions, 7);
        assert_eq!(summary.total_deletions, 3);
    }

    #[test]
    fn test_parse_diff_stat_insertions_only() {
        let stat_output = " new_file.rs | 25 +++++++++++++++++++++++++
 1 file changed, 25 insertions(+)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "new_file.rs");
        assert!(summary.entries[0].insertions > 0);
        assert_eq!(summary.entries[0].deletions, 0);
        assert_eq!(summary.total_insertions, 25);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn test_parse_diff_stat_deletions_only() {
        let stat_output = " old_file.rs | 15 ---------------
 1 file changed, 15 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "old_file.rs");
        assert_eq!(summary.entries[0].insertions, 0);
        assert!(summary.entries[0].deletions > 0);
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 15);
    }

    #[test]
    fn test_parse_diff_stat_empty() {
        let summary = parse_diff_stat("");
        assert!(summary.entries.is_empty());
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn test_parse_diff_stat_no_summary_line() {
        // Sometimes stat output has no summary — compute from entries
        let stat_output = " src/main.rs | 5 +++--
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        // Totals computed from entry counts
        assert_eq!(summary.total_insertions, summary.entries[0].insertions);
        assert_eq!(summary.total_deletions, summary.entries[0].deletions);
    }

    #[test]
    fn test_parse_diff_stat_multiple_files() {
        let stat_output = " Cargo.toml       |  2 +-
 src/cli.rs       | 15 ++++++++-------
 src/commands.rs  | 88 +++++++++++++++++++++++++++++++++++++++++++++++++++++---
 src/format.rs    |  3 ++-
 4 files changed, 78 insertions(+), 30 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 4);
        assert_eq!(summary.entries[0].file, "Cargo.toml");
        assert_eq!(summary.entries[2].file, "src/commands.rs");
        assert_eq!(summary.total_insertions, 78);
        assert_eq!(summary.total_deletions, 30);
    }

    #[test]
    fn test_format_diff_stat_empty() {
        let summary = DiffStatSummary {
            entries: vec![],
            total_insertions: 0,
            total_deletions: 0,
        };
        let formatted = format_diff_stat(&summary);
        assert!(
            formatted.is_empty(),
            "Empty summary should produce empty output"
        );
    }

    #[test]
    fn test_format_diff_stat_single_entry() {
        let summary = DiffStatSummary {
            entries: vec![DiffStatEntry {
                file: "src/main.rs".to_string(),
                insertions: 5,
                deletions: 2,
            }],
            total_insertions: 5,
            total_deletions: 2,
        };
        let formatted = format_diff_stat(&summary);
        assert!(formatted.contains("src/main.rs"), "Should contain filename");
        assert!(
            formatted.contains("1 file changed"),
            "Should show file count"
        );
        assert!(formatted.contains("+5"), "Should show insertions");
        assert!(formatted.contains("-2"), "Should show deletions");
    }

    #[test]
    fn test_format_diff_stat_multiple_entries() {
        let summary = DiffStatSummary {
            entries: vec![
                DiffStatEntry {
                    file: "src/a.rs".to_string(),
                    insertions: 10,
                    deletions: 0,
                },
                DiffStatEntry {
                    file: "src/b.rs".to_string(),
                    insertions: 0,
                    deletions: 5,
                },
            ],
            total_insertions: 10,
            total_deletions: 5,
        };
        let formatted = format_diff_stat(&summary);
        assert!(formatted.contains("src/a.rs"));
        assert!(formatted.contains("src/b.rs"));
        assert!(formatted.contains("2 files changed"));
    }

    #[test]
    fn test_format_diff_stat_insertions_only_no_deletions_shown() {
        let summary = DiffStatSummary {
            entries: vec![DiffStatEntry {
                file: "new.rs".to_string(),
                insertions: 10,
                deletions: 0,
            }],
            total_insertions: 10,
            total_deletions: 0,
        };
        let formatted = format_diff_stat(&summary);
        assert!(formatted.contains("+10"), "Should show insertions");
        // "-0" should not appear
        assert!(!formatted.contains("-0"), "Should not show zero deletions");
    }

    // ── build_undo_context tests ────────────────────────────────────────

    #[test]
    fn build_undo_context_includes_all_actions() {
        let actions = vec![
            "restored src/main.rs".to_string(),
            "deleted src/new_file.rs".to_string(),
        ];
        let ctx = build_undo_context(&actions);
        assert!(ctx.contains("restored src/main.rs"));
        assert!(ctx.contains("deleted src/new_file.rs"));
        assert!(ctx.contains("[System note:"));
        assert!(ctx.contains("may no longer exist"));
        // File count included
        assert!(ctx.contains("2 files"), "Context should include file count");
    }

    #[test]
    fn build_undo_context_single_action() {
        let actions = vec!["restored src/foo.rs".to_string()];
        let ctx = build_undo_context(&actions);
        assert!(ctx.contains("- restored src/foo.rs"));
        assert!(ctx.contains("Verify current file state"));
        // Singular "file" for count of 1
        assert!(
            ctx.contains("1 file"),
            "Context should use singular 'file' for single action"
        );
    }

    #[test]
    fn build_undo_context_warns_about_stale_references() {
        let actions = vec!["restored src/lib.rs".to_string()];
        let ctx = build_undo_context(&actions);
        assert!(
            ctx.contains("⚠️"),
            "Context should contain ⚠️ warning about stale references"
        );
        assert!(
            ctx.contains("may no longer exist"),
            "Context should warn that referenced code may no longer exist"
        );
    }

    #[test]
    fn build_undo_context_recommends_rereading_files() {
        let actions = vec![
            "restored src/a.rs".to_string(),
            "restored src/b.rs".to_string(),
        ];
        let ctx = build_undo_context(&actions);
        assert!(
            ctx.contains("Re-read affected files"),
            "Context should recommend re-reading affected files before new changes"
        );
    }

    // ── handle_undo return value tests ──────────────────────────────────

    #[test]
    fn handle_undo_returns_none_on_empty_history() {
        let mut history = TurnHistory::new();
        let result = handle_undo("/undo", &mut history);
        assert!(result.is_none(), "Should return None when history is empty");
    }

    #[test]
    fn handle_undo_returns_some_when_files_reverted() {
        use crate::session::TurnSnapshot;
        use std::fs;

        // Create a temp file to snapshot
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_undo.txt");
        fs::write(&file_path, "original content").unwrap();
        let path_str = file_path.to_str().unwrap();

        // Build a snapshot with the original file
        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);

        // Modify the file (simulating agent changes)
        fs::write(&file_path, "modified content").unwrap();

        // Push the snapshot into history
        let mut history = TurnHistory::new();
        history.push(snap);

        let result = handle_undo("/undo", &mut history);
        assert!(
            result.is_some(),
            "Should return Some when files were reverted"
        );

        let ctx = result.unwrap();
        assert!(
            ctx.contains(path_str),
            "Context should mention the reverted file path"
        );
        assert!(ctx.contains("[System note:"));
        // Verify causality harness content
        assert!(
            ctx.contains("⚠️"),
            "Context should contain ⚠️ stale-reference warning"
        );
        assert!(
            ctx.contains("1 file"),
            "Context should include the affected file count"
        );
        assert!(
            ctx.contains("Re-read affected files"),
            "Context should recommend re-reading files"
        );

        // Verify the file was actually restored
        let restored = fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored, "original content");
    }

    #[test]
    fn handle_undo_returns_none_on_zero_count() {
        let mut history = TurnHistory::new();
        let result = handle_undo("/undo 0", &mut history);
        assert!(result.is_none());
    }

    #[test]
    fn handle_undo_returns_none_on_bad_arg() {
        let mut history = TurnHistory::new();
        let result = handle_undo("/undo xyz", &mut history);
        assert!(result.is_none());
    }

    // ── handle_undo --last-commit tests ─────────────────────────────────

    #[test]
    fn handle_undo_dispatches_last_commit() {
        // Verify that "--last-commit" is recognized as a valid argument
        // (not rejected as a bad arg). We only test the parse/dispatch logic
        // here — NOT the actual git revert, because run_git() inherits the
        // process CWD, and `cargo test` runs in the real project directory.
        // Calling handle_undo_last_commit() here would run `git revert HEAD`
        // against real project commits, creating revert commits every time
        // the test suite runs. The actual revert logic is tested in
        // undo_last_commit_in_real_repo() which uses a temp dir.
        let arg = "/undo --last-commit";
        let trimmed = arg.trim_start_matches("/undo").trim();
        assert_eq!(trimmed, "--last-commit", "should parse --last-commit arg");
    }

    #[test]
    fn undo_last_commit_context_format() {
        // Test the context note format that handle_undo_last_commit builds.
        // We replicate the context-building logic to verify the format
        // without needing a real git repo (avoids cwd races).
        let log_line = "abc1234 fix: something important";
        let files = "src/main.rs\nsrc/tools.rs\n";

        let mut actions = Vec::new();
        for f in files.lines().filter(|l| !l.is_empty()) {
            actions.push(format!("reverted changes to {f} (commit undone)"));
        }

        let mut note = String::from("[System note: /undo --last-commit reverted a git commit.\n");
        note.push_str(&format!("Reverted commit: {}\n", log_line.trim()));
        note.push_str("Files affected:\n");
        for action in &actions {
            note.push_str(&format!("- {action}\n"));
        }
        note.push_str(
            "⚠️ Earlier messages in this conversation may reference code from this commit \
             that no longer exists. Verify current file state before continuing.\n",
        );
        note.push_str("Any journal entries about this commit describe work that has been undone.]");

        assert!(note.contains("abc1234 fix: something important"));
        assert!(note.contains("reverted changes to src/main.rs"));
        assert!(note.contains("reverted changes to src/tools.rs"));
        assert!(note.contains("⚠️"));
        assert!(note.contains("journal entries"));
        assert!(note.contains("[System note: /undo --last-commit"));
        assert!(note.contains("has been undone.]"));
    }

    #[test]
    #[serial]
    fn undo_last_commit_in_real_repo() {
        use std::fs;

        // Create a temp dir with a git repo
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();

        // Initialize git repo
        let init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(init.status.success(), "git init failed");

        // Configure git user for the test repo
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(repo)
            .output();
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(repo)
            .output();

        // Create initial commit
        let file_path = repo.join("hello.txt");
        fs::write(&file_path, "initial").unwrap();
        let _ = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo)
            .output();
        let _ = std::process::Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(repo)
            .output();

        // Create a second commit to revert
        fs::write(&file_path, "changed").unwrap();
        let _ = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo)
            .output();
        let _ = std::process::Command::new("git")
            .args(["commit", "-m", "change hello"])
            .current_dir(repo)
            .output();

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "changed");

        // Capture the commit hash before reverting so we can verify it in context
        let hash_output = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let commit_hash = String::from_utf8_lossy(&hash_output.stdout)
            .trim()
            .to_string();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(repo).unwrap();

        let result = handle_undo_last_commit();

        std::env::set_current_dir(&original_dir).unwrap();

        // The revert should succeed
        assert!(
            result.is_some(),
            "handle_undo_last_commit should return Some"
        );
        let ctx = result.unwrap();
        assert!(
            ctx.contains("hello.txt"),
            "Context should mention the reverted file"
        );
        assert!(ctx.contains("⚠️"), "Context should contain the warning");
        assert!(
            ctx.contains("journal entries"),
            "Context should mention journal entries"
        );
        assert!(
            ctx.contains("Reverted commit:"),
            "Context should show the reverted commit"
        );
        // Verify the context includes the actual commit hash
        assert!(
            ctx.contains(&commit_hash),
            "Context should include the commit hash '{commit_hash}'"
        );
        // Verify the context mentions the commit message
        assert!(
            ctx.contains("change hello"),
            "Context should include the commit message"
        );
        // Verify the --last-commit specific system note format
        assert!(
            ctx.contains("[System note: /undo --last-commit"),
            "Context should use --last-commit specific system note"
        );

        // Verify file was reverted to initial content
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, "initial",
            "File should be reverted to initial content"
        );
    }

    // --- AI commit message tests ---

    #[test]
    fn build_commit_ai_prompt_includes_diff() {
        let diff = "+++ b/src/main.rs\n+fn hello() {}\n";
        let prompt = build_commit_ai_prompt(diff);
        assert!(prompt.contains("conventional commit format"));
        assert!(prompt.contains("+fn hello() {}"));
        assert!(prompt.contains("```diff"));
    }

    #[test]
    fn build_commit_ai_prompt_truncates_large_diff() {
        // Create a diff larger than COMMIT_AI_MAX_BYTES
        let big_diff = "a".repeat(40_000);
        let prompt = build_commit_ai_prompt(&big_diff);
        assert!(prompt.contains("(diff truncated"));
        assert!(prompt.contains("40000 total bytes"));
        // Should not contain the full 40k chars
        assert!(prompt.len() < 35_000);
    }

    #[test]
    fn build_commit_ai_prompt_truncates_safely_on_multibyte() {
        // Build a diff with multi-byte chars right around the boundary
        let prefix = "x".repeat(COMMIT_AI_MAX_BYTES - 2);
        let diff = format!("{prefix}✓✓✓"); // ✓ is 3 bytes
        let prompt = build_commit_ai_prompt(&diff);
        // Should not panic and should contain truncation notice
        assert!(prompt.contains("(diff truncated"));
    }

    #[test]
    fn clean_ai_commit_message_strips_quotes() {
        assert_eq!(
            clean_ai_commit_message("\"feat: add login\""),
            "feat: add login"
        );
        assert_eq!(clean_ai_commit_message("`fix: typo`"), "fix: typo");
    }

    #[test]
    fn clean_ai_commit_message_takes_first_line() {
        let msg = "feat: add login\n\nThis is a longer description.";
        assert_eq!(clean_ai_commit_message(msg), "feat: add login");
    }

    #[test]
    fn clean_ai_commit_message_strips_preamble() {
        assert_eq!(
            clean_ai_commit_message("Commit message: feat: add login"),
            "feat: add login"
        );
        assert_eq!(
            clean_ai_commit_message("commit message: fix: typo"),
            "fix: typo"
        );
    }

    #[test]
    fn clean_ai_commit_message_handles_empty() {
        assert_eq!(clean_ai_commit_message(""), "");
        assert_eq!(clean_ai_commit_message("   "), "");
    }

    #[test]
    fn wants_ai_commit_detects_flags() {
        assert!(wants_ai_commit("/commit --ai"));
        assert!(wants_ai_commit("/commit --generate"));
        assert!(wants_ai_commit("/commit --ai some msg"));
        assert!(wants_ai_commit("/commit --ai -a"));
        assert!(!wants_ai_commit("/commit"));
        assert!(!wants_ai_commit("/commit fix: typo"));
        assert!(!wants_ai_commit("/commit -a fix: typo"));
    }

    #[test]
    fn parse_commit_args_no_flags() {
        let args = parse_commit_args("");
        assert!(!args.auto_stage);
        assert!(!args.ai);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_message_only() {
        let args = parse_commit_args("fix the bug");
        assert!(!args.auto_stage);
        assert!(!args.ai);
        assert_eq!(args.message, "fix the bug");
    }

    #[test]
    fn parse_commit_args_auto_stage_short() {
        let args = parse_commit_args("-a");
        assert!(args.auto_stage);
        assert!(!args.ai);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_auto_stage_long() {
        let args = parse_commit_args("--all");
        assert!(args.auto_stage);
        assert!(!args.ai);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_auto_stage_with_message() {
        let args = parse_commit_args("-a fix the bug");
        assert!(args.auto_stage);
        assert!(!args.ai);
        assert_eq!(args.message, "fix the bug");
    }

    #[test]
    fn parse_commit_args_auto_stage_message_first() {
        // Flag can appear after message tokens
        let args = parse_commit_args("fix the bug --all");
        assert!(args.auto_stage);
        assert!(!args.ai);
        assert_eq!(args.message, "fix the bug");
    }

    #[test]
    fn parse_commit_args_all_flags() {
        let args = parse_commit_args("--ai -a fix it");
        assert!(args.auto_stage);
        assert!(args.ai);
        assert_eq!(args.message, "fix it");
    }

    #[test]
    fn parse_commit_args_ai_and_all_no_message() {
        let args = parse_commit_args("-a --generate");
        assert!(args.auto_stage);
        assert!(args.ai);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_amend_only() {
        let args = parse_commit_args("--amend");
        assert!(args.amend);
        assert!(!args.auto_stage);
        assert!(!args.ai);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_amend_with_message() {
        let args = parse_commit_args("--amend fix typo");
        assert!(args.amend);
        assert!(!args.auto_stage);
        assert_eq!(args.message, "fix typo");
    }

    #[test]
    fn parse_commit_args_amend_auto_stage() {
        let args = parse_commit_args("-a --amend");
        assert!(args.amend);
        assert!(args.auto_stage);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_amend_auto_stage_reversed() {
        let args = parse_commit_args("--amend -a");
        assert!(args.amend);
        assert!(args.auto_stage);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_amend_auto_stage_with_message() {
        let args = parse_commit_args("-a --amend new message");
        assert!(args.amend);
        assert!(args.auto_stage);
        assert_eq!(args.message, "new message");
    }

    #[test]
    fn parse_commit_args_amend_message_first() {
        let args = parse_commit_args("fix the bug --amend");
        assert!(args.amend);
        assert!(!args.auto_stage);
        assert_eq!(args.message, "fix the bug");
    }

    #[test]
    fn parse_commit_args_dry_run() {
        let args = parse_commit_args("--dry-run");
        assert!(args.dry_run);
        assert!(!args.auto_stage);
        assert!(!args.ai);
        assert!(!args.amend);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_ai_dry_run() {
        let args = parse_commit_args("--ai --dry-run");
        assert!(args.dry_run);
        assert!(args.ai);
        assert!(!args.auto_stage);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_dry_run_with_message() {
        let args = parse_commit_args("--dry-run fix the bug");
        assert!(args.dry_run);
        assert!(!args.ai);
        assert_eq!(args.message, "fix the bug");
    }

    #[test]
    fn parse_commit_args_dry_run_auto_stage() {
        let args = parse_commit_args("-a --dry-run");
        assert!(args.dry_run);
        assert!(args.auto_stage);
        assert_eq!(args.message, "");
    }

    #[test]
    fn parse_commit_args_no_dry_run() {
        let args = parse_commit_args("fix the bug");
        assert!(!args.dry_run);
        assert_eq!(args.message, "fix the bug");
    }

    #[test]
    fn parse_commit_args_all_flags_with_dry_run() {
        let args = parse_commit_args("--ai -a --dry-run fix it");
        assert!(args.dry_run);
        assert!(args.auto_stage);
        assert!(args.ai);
        assert_eq!(args.message, "fix it");
    }

    // --- ref_range parsing tests ---

    #[test]
    fn parse_diff_args_ref_range_with_dots() {
        let opts = parse_diff_args("/diff main..feature");
        assert_eq!(opts.ref_range, Some("main..feature".to_string()));
        assert!(!opts.staged_only);
        assert!(!opts.stat_only);
        assert!(!opts.name_only);
        assert!(opts.file.is_none());
    }

    #[test]
    fn parse_diff_args_ref_range_triple_dot() {
        let opts = parse_diff_args("/diff main...feature");
        assert_eq!(opts.ref_range, Some("main...feature".to_string()));
    }

    #[test]
    fn parse_diff_args_ref_range_head_tilde() {
        let opts = parse_diff_args("/diff HEAD~3");
        assert_eq!(opts.ref_range, Some("HEAD~3".to_string()));
        assert!(opts.file.is_none());
    }

    #[test]
    fn parse_diff_args_ref_range_tag_to_head() {
        let opts = parse_diff_args("/diff v1.0..HEAD");
        assert_eq!(opts.ref_range, Some("v1.0..HEAD".to_string()));
    }

    #[test]
    fn parse_diff_args_ref_range_with_stat() {
        let opts = parse_diff_args("/diff main..feature --stat");
        assert_eq!(opts.ref_range, Some("main..feature".to_string()));
        assert!(opts.stat_only);
        assert!(!opts.name_only);
    }

    #[test]
    fn parse_diff_args_ref_range_with_name_only() {
        let opts = parse_diff_args("/diff main..feature --name-only");
        assert_eq!(opts.ref_range, Some("main..feature".to_string()));
        assert!(opts.name_only);
    }

    #[test]
    fn parse_diff_args_ref_range_with_functions() {
        let opts = parse_diff_args("/diff main..feature --functions");
        assert_eq!(opts.ref_range, Some("main..feature".to_string()));
        assert!(opts.functions);
    }

    #[test]
    fn parse_diff_args_ref_range_with_explain() {
        let opts = parse_diff_args("/diff HEAD~5 --explain");
        assert_eq!(opts.ref_range, Some("HEAD~5".to_string()));
        assert!(opts.explain);
    }

    #[test]
    fn parse_diff_args_ref_range_with_multiple_flags() {
        let opts = parse_diff_args("/diff main..feature --stat --name-only");
        assert_eq!(opts.ref_range, Some("main..feature".to_string()));
        assert!(opts.stat_only);
        assert!(opts.name_only);
    }

    #[test]
    fn parse_diff_args_ref_range_ignores_staged() {
        // When ref_range is set, --staged is meaningless and should be cleared
        let opts = parse_diff_args("/diff main..feature --staged");
        assert_eq!(opts.ref_range, Some("main..feature".to_string()));
        assert!(
            !opts.staged_only,
            "--staged should be ignored with ref range"
        );
    }

    #[test]
    fn parse_diff_args_flags_after_ref() {
        let opts = parse_diff_args("/diff v1.0..HEAD --stat --explain");
        assert_eq!(opts.ref_range, Some("v1.0..HEAD".to_string()));
        assert!(opts.stat_only);
        assert!(opts.explain);
    }

    #[test]
    fn parse_diff_args_existing_file_takes_precedence() {
        // If a positional arg matches an existing file path, it should be
        // treated as a file, not a ref. Cargo.toml exists in the repo root.
        let opts = parse_diff_args("/diff Cargo.toml");
        assert!(
            opts.file.is_some(),
            "existing file should be parsed as file, not ref"
        );
        assert_eq!(opts.file, Some("Cargo.toml".to_string()));
        assert!(
            opts.ref_range.is_none(),
            "should not be treated as a ref range"
        );
    }

    #[test]
    fn parse_diff_args_dotdot_always_ref_even_if_path_exists() {
        // A `..` in the argument always means ref range, even if parts
        // could look like paths
        let opts = parse_diff_args("/diff Cargo.toml..HEAD");
        assert_eq!(opts.ref_range, Some("Cargo.toml..HEAD".to_string()));
        assert!(opts.file.is_none());
    }

    #[test]
    fn parse_diff_args_nonexistent_path_becomes_ref() {
        // A positional arg that doesn't exist on disk and has no `..`
        // is treated as a ref (branch name, tag, etc.)
        let opts = parse_diff_args("/diff some-branch-that-does-not-exist");
        assert_eq!(
            opts.ref_range,
            Some("some-branch-that-does-not-exist".to_string())
        );
        assert!(opts.file.is_none());
    }

    #[test]
    fn parse_diff_args_no_ref_range_plain() {
        let opts = parse_diff_args("/diff");
        assert!(opts.ref_range.is_none());
        assert!(!opts.staged_only);
    }

    #[test]
    fn parse_diff_args_no_ref_range_staged_only() {
        let opts = parse_diff_args("/diff --staged");
        assert!(opts.ref_range.is_none());
        assert!(opts.staged_only);
    }

    #[test]
    fn diff_options_stores_ref_range() {
        let opts = DiffOptions {
            staged_only: false,
            name_only: false,
            stat_only: false,
            explain: false,
            functions: false,
            file: None,
            ref_range: Some("main..dev".to_string()),
        };
        assert_eq!(opts.ref_range, Some("main..dev".to_string()));
    }
}
