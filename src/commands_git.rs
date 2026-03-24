//! Git-related command handlers: /diff, /undo, /commit, /pr, /git, /review.

use crate::commands::auto_compact_if_needed;
use crate::format::*;
use crate::git::*;
use crate::prompt::*;

use std::io::{self, Write};
use yoagent::agent::Agent;
use yoagent::*;

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
    pub file: Option<String>,
}

/// Parse `/diff` arguments into structured options.
///
/// Supports:
/// - `/diff` — all changes (default)
/// - `/diff --staged` or `/diff --cached` — staged only
/// - `/diff --name-only` — filenames only
/// - `/diff <file>` — diff for a specific file
/// - Combined: `/diff --staged --name-only src/main.rs`
pub fn parse_diff_args(input: &str) -> DiffOptions {
    let rest = input.strip_prefix("/diff").unwrap_or("").trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut staged_only = false;
    let mut name_only = false;
    let mut file = None;

    for part in parts {
        match part {
            "--staged" | "--cached" => staged_only = true,
            "--name-only" => name_only = true,
            _ => file = Some(part.to_string()),
        }
    }

    DiffOptions {
        staged_only,
        name_only,
        file,
    }
}

pub fn handle_diff(input: &str) {
    let opts = parse_diff_args(input);

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

// ── /undo ────────────────────────────────────────────────────────────────

/// Handle `/undo` with per-turn granularity.
///
/// - `/undo` — undo the last agent turn (restore files to pre-turn state)
/// - `/undo N` — undo the last N turns
/// - `/undo --all` — nuclear option: revert ALL uncommitted changes (old behavior)
pub fn handle_undo(input: &str, history: &mut crate::prompt::TurnHistory) {
    let arg = input.strip_prefix("/undo").unwrap_or("").trim();

    // Nuclear fallback: /undo --all
    if arg == "--all" {
        handle_undo_all(history);
        return;
    }

    // Parse optional count: /undo N
    let count: usize = if arg.is_empty() {
        1
    } else if let Ok(n) = arg.parse::<usize>() {
        if n == 0 {
            println!("{DIM}  (nothing to undo — count is 0){RESET}\n");
            return;
        }
        n
    } else {
        println!("{DIM}  usage: /undo [N] or /undo --all{RESET}\n");
        return;
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
        return;
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
}

/// Nuclear undo: revert ALL uncommitted changes (old behavior).
/// Clears turn history as well.
fn handle_undo_all(history: &mut crate::prompt::TurnHistory) {
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
    } else {
        if has_diff {
            println!("{DIM}{diff_stat}{RESET}");
        }
        if has_untracked {
            println!("{DIM}  untracked files:");
            for f in &untracked_files {
                println!("    {f}");
            }
            println!("{RESET}");
        }

        if has_diff {
            let _ = run_git(&["checkout", "--", "."]);
        }
        if has_untracked {
            let _ = run_git(&["clean", "-fd"]);
        }
        println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");
    }

    // Clear turn history since everything is now reverted
    history.clear();
}

// ── /commit ──────────────────────────────────────────────────────────────

pub fn handle_commit(input: &str) {
    let arg = input.strip_prefix("/commit").unwrap_or("").trim();
    if !arg.is_empty() {
        let (ok, output) = run_git_commit(arg);
        if ok {
            println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
        } else {
            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
        }
    } else {
        match get_staged_diff() {
            None => {
                eprintln!("{RED}  error: not in a git repository{RESET}\n");
            }
            Some(diff) if diff.trim().is_empty() => {
                println!("{DIM}  nothing staged — use `git add` first{RESET}\n");
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
                            let (ok, output) = run_git_commit(&suggested);
                            if ok {
                                println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
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
                                    let (ok, output) = run_git_commit(custom_msg);
                                    if ok {
                                        println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
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

// ── /pr ──────────────────────────────────────────────────────────────────

/// Represents a parsed `/pr` subcommand.
#[derive(Debug, PartialEq)]
pub enum PrSubcommand {
    List,
    View(u32),
    Diff(u32),
    Comment(u32, String),
    Checkout(u32),
    Create { draft: bool },
    Help,
}

/// Parse the argument string after `/pr` into a `PrSubcommand`.
pub fn parse_pr_args(arg: &str) -> PrSubcommand {
    let arg = arg.trim();
    if arg.is_empty() {
        return PrSubcommand::List;
    }

    let parts: Vec<&str> = arg.splitn(3, char::is_whitespace).collect();

    // Check for "create" subcommand first (before trying to parse as number)
    if parts[0].eq_ignore_ascii_case("create") {
        let draft = parts
            .get(1)
            .map(|s| s.trim_start_matches('-').eq_ignore_ascii_case("draft"))
            .unwrap_or(false);
        return PrSubcommand::Create { draft };
    }

    let number = match parts[0].parse::<u32>() {
        Ok(n) => n,
        Err(_) => return PrSubcommand::Help,
    };

    if parts.len() == 1 {
        return PrSubcommand::View(number);
    }

    match parts[1].to_lowercase().as_str() {
        "diff" => PrSubcommand::Diff(number),
        "checkout" => PrSubcommand::Checkout(number),
        "comment" => {
            let text = if parts.len() == 3 {
                parts[2].trim().to_string()
            } else {
                String::new()
            };
            if text.is_empty() {
                PrSubcommand::Help
            } else {
                PrSubcommand::Comment(number, text)
            }
        }
        _ => PrSubcommand::Help,
    }
}

pub async fn handle_pr(input: &str, agent: &mut Agent, session_total: &mut Usage, model: &str) {
    let arg = input.strip_prefix("/pr").unwrap_or("").trim();
    match parse_pr_args(arg) {
        PrSubcommand::List => {
            match std::process::Command::new("gh")
                .args(["pr", "list", "--limit", "10"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (no open pull requests){RESET}\n");
                    } else {
                        println!("{DIM}  Open pull requests:");
                        for line in text.lines() {
                            println!("    {line}");
                        }
                        println!("{RESET}");
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::View(number) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "view", &num_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    println!("{DIM}{text}{RESET}");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Diff(number) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "diff", &num_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (no diff for PR #{number}){RESET}\n");
                    } else {
                        println!("{DIM}{text}{RESET}");
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Comment(number, text) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "comment", &num_str, "--body", &text])
                .output()
            {
                Ok(output) if output.status.success() => {
                    println!("{GREEN}  ✓ comment added to PR #{number}{RESET}\n");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Checkout(number) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "checkout", &num_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    println!("{GREEN}  ✓ checked out PR #{number}{RESET}\n");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Create { draft } => {
            // 1. Detect current branch
            let branch = match git_branch() {
                Some(b) => b,
                None => {
                    eprintln!("{RED}  error: not in a git repository{RESET}\n");
                    return;
                }
            };
            let base = detect_base_branch();

            if branch == base {
                eprintln!(
                    "{RED}  error: already on {base} — switch to a feature branch first{RESET}\n"
                );
                return;
            }

            // 2. Get diff and commits
            let diff = get_branch_diff(&base).unwrap_or_default();
            let commits = get_branch_commits(&base).unwrap_or_default();

            if diff.trim().is_empty() && commits.trim().is_empty() {
                println!(
                    "{DIM}  (no changes between {branch} and {base} — nothing to create a PR for){RESET}\n"
                );
                return;
            }

            // 3. Show what we found
            let commit_count = commits.lines().filter(|l| !l.is_empty()).count();
            println!(
                "{DIM}  Branch: {branch} → {base} ({commit_count} commit{s}){RESET}",
                s = if commit_count == 1 { "" } else { "s" }
            );
            println!("{DIM}  Generating PR description with AI...{RESET}");

            // 4. Ask AI to generate title + description
            let prompt = build_pr_description_prompt(&branch, &base, &commits, &diff);
            let response = run_prompt(agent, &prompt, session_total, model).await.text;

            // 5. Parse the AI's response
            let (title, body) = match parse_pr_description(&response) {
                Some(parsed) => parsed,
                None => {
                    eprintln!(
                        "{RED}  error: could not parse AI response into PR title/description{RESET}"
                    );
                    eprintln!("{DIM}  (try again or create manually with `gh pr create`){RESET}\n");
                    return;
                }
            };

            println!("{DIM}  Title: {BOLD}{title}{RESET}");
            println!("{DIM}  Draft: {}{RESET}", if draft { "yes" } else { "no" });

            // 6. Create the PR via gh CLI
            let mut gh_args = vec![
                "pr".to_string(),
                "create".to_string(),
                "--title".to_string(),
                title.clone(),
                "--body".to_string(),
                body,
                "--base".to_string(),
                base.clone(),
            ];
            if draft {
                gh_args.push("--draft".to_string());
            }

            let gh_str_args: Vec<&str> = gh_args.iter().map(|s| s.as_str()).collect();
            match std::process::Command::new("gh").args(&gh_str_args).output() {
                Ok(output) if output.status.success() => {
                    let url = String::from_utf8_lossy(&output.stdout);
                    let url = url.trim();
                    if url.is_empty() {
                        println!("{GREEN}  ✓ PR created: {title}{RESET}\n");
                    } else {
                        println!("{GREEN}  ✓ PR created: {url}{RESET}\n");
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Help => {
            println!("{DIM}  usage: /pr                         List open pull requests");
            println!(
                "         /pr create [--draft]        Create PR with AI-generated description"
            );
            println!("         /pr <number>                View details of a specific PR");
            println!("         /pr <number> diff           Show the diff of a PR");
            println!("         /pr <number> comment <text> Add a comment to a PR");
            println!("         /pr <number> checkout       Checkout a PR locally{RESET}\n");
        }
    }
}

// ── /git ─────────────────────────────────────────────────────────────────

pub fn handle_git(input: &str) {
    let arg = input.strip_prefix("/git").unwrap_or("").trim();
    let subcmd = parse_git_args(arg);
    run_git_subcommand(&subcmd);
}

// ── /review ──────────────────────────────────────────────────────────────

/// Build a review prompt for either staged changes or a specific file.
/// Returns None if there's nothing to review, Some(prompt) otherwise.
pub fn build_review_content(arg: &str) -> Option<(String, String)> {
    let arg = arg.trim();
    if arg.is_empty() {
        // Review staged changes
        match get_staged_diff() {
            None => {
                eprintln!("{RED}  error: not in a git repository{RESET}\n");
                None
            }
            Some(diff) if diff.trim().is_empty() => {
                // Fall back to unstaged diff if nothing staged
                let unstaged = run_git(&["diff"]).unwrap_or_default();
                if unstaged.trim().is_empty() {
                    println!("{DIM}  nothing to review — no staged or unstaged changes{RESET}\n");
                    None
                } else {
                    println!("{DIM}  reviewing unstaged changes...{RESET}");
                    Some(("unstaged changes".to_string(), unstaged))
                }
            }
            Some(diff) => {
                println!("{DIM}  reviewing staged changes...{RESET}");
                Some(("staged changes".to_string(), diff))
            }
        }
    } else {
        // Review a specific file
        let path = std::path::Path::new(arg);
        if !path.exists() {
            eprintln!("{RED}  error: file not found: {arg}{RESET}\n");
            return None;
        }
        match std::fs::read_to_string(path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    println!("{DIM}  file is empty — nothing to review{RESET}\n");
                    None
                } else {
                    println!("{DIM}  reviewing {arg}...{RESET}");
                    Some((arg.to_string(), content))
                }
            }
            Err(e) => {
                eprintln!("{RED}  error reading {arg}: {e}{RESET}\n");
                None
            }
        }
    }
}

/// Build the review prompt to send to the AI.
pub fn build_review_prompt(label: &str, content: &str) -> String {
    // Truncate if very large
    let max_chars = 30_000;
    let content_preview = if content.len() > max_chars {
        let truncated = &content[..max_chars];
        format!(
            "{truncated}\n\n... (truncated, {} more chars)",
            content.len() - max_chars
        )
    } else {
        content.to_string()
    };

    format!(
        r#"Review the following code ({label}). Look for:

1. **Bugs** — logic errors, off-by-one errors, null/None handling, race conditions
2. **Security** — injection vulnerabilities, unsafe operations, credential exposure
3. **Style** — naming, idiomatic patterns, unnecessary complexity, dead code
4. **Performance** — obvious inefficiencies, unnecessary allocations, N+1 patterns
5. **Suggestions** — improvements, missing error handling, better approaches

Be specific: reference line numbers or code snippets. Be concise — skip things that look fine.
If the code looks good overall, say so briefly and note any minor suggestions.

```
{content_preview}
```"#
    )
}

/// Handle the /review command: review staged changes or a specific file.
/// Returns the review prompt if sent to AI, None otherwise.
pub async fn handle_review(
    input: &str,
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let arg = input.strip_prefix("/review").unwrap_or("").trim();

    match build_review_content(arg) {
        Some((label, content)) => {
            let prompt = build_review_prompt(&label, &content);
            run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);
            Some(prompt)
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn parse_pr_args_empty_is_list() {
        assert_eq!(parse_pr_args(""), PrSubcommand::List);
        assert_eq!(parse_pr_args("  "), PrSubcommand::List);
    }

    #[test]
    fn parse_pr_args_number_is_view() {
        assert_eq!(parse_pr_args("42"), PrSubcommand::View(42));
        assert_eq!(parse_pr_args("1"), PrSubcommand::View(1));
        assert_eq!(parse_pr_args("  99  "), PrSubcommand::View(99));
    }

    #[test]
    fn parse_pr_args_number_diff() {
        assert_eq!(parse_pr_args("42 diff"), PrSubcommand::Diff(42));
    }

    #[test]
    fn parse_pr_args_number_checkout() {
        assert_eq!(parse_pr_args("7 checkout"), PrSubcommand::Checkout(7));
    }

    #[test]
    fn parse_pr_args_number_comment() {
        assert_eq!(
            parse_pr_args("5 comment looks good!"),
            PrSubcommand::Comment(5, "looks good!".to_string())
        );
    }

    #[test]
    fn parse_pr_args_comment_without_text_is_help() {
        assert_eq!(parse_pr_args("5 comment"), PrSubcommand::Help);
    }

    #[test]
    fn parse_pr_args_create() {
        assert_eq!(
            parse_pr_args("create"),
            PrSubcommand::Create { draft: false }
        );
    }

    #[test]
    fn parse_pr_args_create_draft() {
        assert_eq!(
            parse_pr_args("create --draft"),
            PrSubcommand::Create { draft: true }
        );
    }

    #[test]
    fn parse_pr_args_create_case_insensitive() {
        assert_eq!(
            parse_pr_args("CREATE"),
            PrSubcommand::Create { draft: false }
        );
        // --Draft with capital D: trim_start_matches('-') → "Draft", eq_ignore_ascii_case("draft") → true
        assert_eq!(
            parse_pr_args("Create --Draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("create -draft"),
            PrSubcommand::Create { draft: true }
        );
    }

    #[test]
    fn parse_pr_args_invalid_is_help() {
        assert_eq!(parse_pr_args("foobar"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("abc 123"), PrSubcommand::Help);
    }

    #[test]
    fn parse_pr_args_unknown_subcommand_is_help() {
        assert_eq!(parse_pr_args("42 merge"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 close"), PrSubcommand::Help);
    }

    // ── build_review_prompt tests ───────────────────────────────────────

    #[test]
    fn build_review_prompt_contains_label() {
        let prompt = build_review_prompt("staged changes", "fn main() {}");
        assert!(
            prompt.contains("staged changes"),
            "Prompt should include the label"
        );
    }

    #[test]
    fn build_review_prompt_contains_content() {
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let prompt = build_review_prompt("test.rs", code);
        assert!(prompt.contains(code), "Prompt should include the code");
    }

    #[test]
    fn build_review_prompt_contains_review_criteria() {
        let prompt = build_review_prompt("file.rs", "let x = 1;");
        assert!(prompt.contains("Bugs"), "Should mention bugs");
        assert!(prompt.contains("Security"), "Should mention security");
        assert!(prompt.contains("Style"), "Should mention style");
        assert!(prompt.contains("Performance"), "Should mention performance");
        assert!(prompt.contains("Suggestions"), "Should mention suggestions");
    }

    #[test]
    fn build_review_prompt_truncates_large_content() {
        let large_content = "x".repeat(50_000);
        let prompt = build_review_prompt("big.rs", &large_content);
        assert!(
            prompt.contains("truncated"),
            "Large content should be truncated"
        );
        assert!(
            prompt.contains("20000 more chars"),
            "Should show remaining char count"
        );
        // The prompt should be shorter than the original content
        assert!(
            prompt.len() < large_content.len(),
            "Prompt should be shorter than 50k"
        );
    }

    #[test]
    fn build_review_prompt_does_not_truncate_small_content() {
        let small_content = "fn hello() { println!(\"hi\"); }";
        let prompt = build_review_prompt("small.rs", small_content);
        assert!(
            !prompt.contains("truncated"),
            "Small content should not be truncated"
        );
        assert!(
            prompt.contains(small_content),
            "Full content should be present"
        );
    }

    #[test]
    fn build_review_prompt_wraps_in_code_block() {
        let prompt = build_review_prompt("test.rs", "let x = 42;");
        assert!(prompt.contains("```"), "Content should be in a code block");
    }

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
        let opts = parse_diff_args("/diff --staged --name-only src/main.rs");
        assert!(opts.staged_only);
        assert!(opts.name_only);
        assert_eq!(opts.file, Some("src/main.rs".to_string()));
    }
}
