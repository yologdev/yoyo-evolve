//! REPL command handlers for yoyo.
//!
//! Each `/command` in the interactive REPL is handled by a function in this module.
//! The main loop dispatches to these handlers, keeping main.rs as a thin REPL driver.

// All handle_* functions in this module are dispatched from the REPL in main.rs.

use crate::cli::{
    self, is_verbose, AUTO_COMPACT_THRESHOLD, DEFAULT_SESSION_PATH, MAX_CONTEXT_TOKENS, VERSION,
};
use crate::docs;
use crate::format::*;
use crate::git::*;
use crate::prompt::*;

use std::collections::HashMap;
use std::io::{self, Write};
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::*;

/// Known REPL command prefixes. Used to detect unknown slash commands
/// and for tab-completion in the REPL.
pub const KNOWN_COMMANDS: &[&str] = &[
    "/help", "/quit", "/exit", "/clear", "/compact", "/commit", "/cost", "/docs", "/find", "/fix",
    "/status", "/tokens", "/save", "/load", "/diff", "/undo", "/health", "/retry", "/history",
    "/search", "/model", "/think", "/config", "/context", "/init", "/version", "/run", "/tree",
    "/pr", "/git", "/test", "/lint", "/spawn", "/review", "/mark", "/jump", "/marks",
];

/// Check if a slash-prefixed input is an unknown command.
/// Extracts the first word and checks against known commands.
pub fn is_unknown_command(input: &str) -> bool {
    let cmd = input.split_whitespace().next().unwrap_or(input);
    !KNOWN_COMMANDS.contains(&cmd)
}

/// Format a ThinkingLevel as a display string.
pub fn thinking_level_name(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Off => "off",
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
    }
}

// ── /help ────────────────────────────────────────────────────────────────

pub fn handle_help() {
    println!("{DIM}  /help              Show this help");
    println!("  /quit, /exit       Exit yoyo");
    println!("  /clear             Clear conversation history");
    println!("  /commit [msg]      Commit staged changes (AI-generates message if no msg)");
    println!("  /compact           Compact conversation to save context space");
    println!("  /config            Show all current settings");
    println!("  /context           Show loaded project context files");
    println!("  /cost              Show estimated session cost");
    println!("  /docs <crate> [item] Look up docs.rs documentation for a Rust crate");
    println!("  /find <pattern>     Fuzzy-search project files by name");
    println!("  /init              Scan project and generate a YOYO.md context file");
    println!("  /model <name>      Switch model (preserves conversation)");
    println!("  /think [level]     Show or change thinking level (off/low/medium/high)");
    println!("  /status            Show session info");
    println!("  /tokens            Show token usage and context window");
    println!("  /save [path]       Save session to file (default: yoyo-session.json)");
    println!("  /load [path]       Load session from file");
    println!("  /diff              Show file summary, change stats, and full diff");
    println!("  /fix               Auto-fix build/lint errors (runs checks, sends failures to AI)");
    println!("  /git <subcmd>      Quick git: status, log, add, diff, branch, stash");
    println!("  /undo              Revert all uncommitted changes (git checkout)");
    println!("  /pr [number]       List open PRs, view, diff, comment, or checkout a PR");
    println!("                     /pr create [--draft] | /pr <n> diff | /pr <n> comment <text>");
    println!("  /health            Run project health checks (auto-detects project type)");
    println!("  /retry             Re-send the last user input");
    println!("  /run <cmd>         Run a shell command directly (no AI, no tokens)");
    println!("  !<cmd>             Shortcut for /run");
    println!("  /test              Auto-detect and run project tests (cargo test, npm test, etc.)");
    println!(
        "  /lint              Auto-detect and run project linter (clippy, eslint, ruff, etc.)"
    );
    println!("  /history           Show summary of conversation messages");
    println!("  /search <query>    Search conversation history for matching messages");
    println!("  /mark <name>       Bookmark current conversation state");
    println!(
        "  /jump <name>       Restore conversation to a bookmark (discards messages after it)"
    );
    println!("  /marks             List all saved bookmarks");
    println!("  /spawn <task>      Spawn a subagent to handle a task (separate context)");
    println!("  /review [path]     AI code review: staged changes (default) or a specific file");
    println!("  /tree [depth]      Show project directory tree (default depth: 3)");
    println!("  /version           Show yoyo version");
    println!();
    println!("  Multi-line input:");
    println!("  End a line with \\ to continue on the next line");
    println!("  Start with ``` to enter a fenced code block{RESET}\n");
}

// ── /version ─────────────────────────────────────────────────────────────

pub fn handle_version() {
    println!("{DIM}  yoyo v{VERSION}{RESET}\n");
}

// ── /status ──────────────────────────────────────────────────────────────

pub fn handle_status(model: &str, cwd: &str, session_total: &Usage) {
    println!("{DIM}  model:   {model}");
    if let Some(branch) = git_branch() {
        println!("  git:     {branch}");
    }
    println!("  cwd:     {cwd}");
    println!(
        "  tokens:  {} in / {} out (session total){RESET}\n",
        session_total.input, session_total.output
    );
}

// ── /tokens ──────────────────────────────────────────────────────────────

pub fn handle_tokens(agent: &Agent, session_total: &Usage, model: &str) {
    let max_context = MAX_CONTEXT_TOKENS;
    let messages = agent.messages().to_vec();
    let context_used = total_tokens(&messages) as u64;
    let bar = context_bar(context_used, max_context);

    println!("{DIM}  Context window:");
    println!("    messages:    {}", messages.len());
    println!(
        "    context:     {} / {} tokens",
        format_token_count(context_used),
        format_token_count(max_context)
    );
    println!("    {bar}");
    if context_used as f64 / max_context as f64 > 0.75 {
        println!("    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}");
    }
    println!();
    println!("  Session totals:");
    println!(
        "    input:       {} tokens",
        format_token_count(session_total.input)
    );
    println!(
        "    output:      {} tokens",
        format_token_count(session_total.output)
    );
    println!(
        "    cache read:  {} tokens",
        format_token_count(session_total.cache_read)
    );
    println!(
        "    cache write: {} tokens",
        format_token_count(session_total.cache_write)
    );
    if let Some(cost) = estimate_cost(session_total, model) {
        println!("    est. cost:   {}", format_cost(cost));
    }
    println!("{RESET}");
}

// ── /cost ────────────────────────────────────────────────────────────────

pub fn handle_cost(session_total: &Usage, model: &str) {
    if let Some(cost) = estimate_cost(session_total, model) {
        println!("{DIM}  Session cost: {}", format_cost(cost));
        println!(
            "    {} in / {} out",
            format_token_count(session_total.input),
            format_token_count(session_total.output)
        );
        if session_total.cache_read > 0 || session_total.cache_write > 0 {
            println!(
                "    cache: {} read / {} write",
                format_token_count(session_total.cache_read),
                format_token_count(session_total.cache_write)
            );
        }
        if let Some((input_cost, cw_cost, cr_cost, output_cost)) =
            cost_breakdown(session_total, model)
        {
            println!();
            println!("    Breakdown:");
            println!("      input:       {}", format_cost(input_cost));
            println!("      output:      {}", format_cost(output_cost));
            if cw_cost > 0.0 {
                println!("      cache write: {}", format_cost(cw_cost));
            }
            if cr_cost > 0.0 {
                println!("      cache read:  {}", format_cost(cr_cost));
            }
        }
        println!("{RESET}");
    } else {
        println!("{DIM}  Cost estimation not available for model '{model}'.{RESET}\n");
    }
}

// ── compact ──────────────────────────────────────────────────────────────

/// Compact the agent's conversation and return (before_count, before_tokens, after_count, after_tokens).
/// Returns None if nothing changed.
pub fn compact_agent(agent: &mut Agent) -> Option<(usize, u64, usize, u64)> {
    let messages = agent.messages().to_vec();
    let before_tokens = total_tokens(&messages) as u64;
    let before_count = messages.len();
    let config = ContextConfig::default();
    let compacted = compact_messages(messages, &config);
    let after_tokens = total_tokens(&compacted) as u64;
    let after_count = compacted.len();
    agent.replace_messages(compacted);
    if before_tokens == after_tokens {
        None
    } else {
        Some((before_count, before_tokens, after_count, after_tokens))
    }
}

/// Auto-compact conversation if context window usage exceeds threshold.
pub fn auto_compact_if_needed(agent: &mut Agent) {
    let messages = agent.messages().to_vec();
    let used = total_tokens(&messages) as u64;
    let ratio = used as f64 / MAX_CONTEXT_TOKENS as f64;

    if ratio > AUTO_COMPACT_THRESHOLD {
        if let Some((before_count, before_tokens, after_count, after_tokens)) = compact_agent(agent)
        {
            println!(
                "{DIM}  ⚡ auto-compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}",
                format_token_count(before_tokens),
                format_token_count(after_tokens)
            );
        }
    }
}

pub fn handle_compact(agent: &mut Agent) {
    let messages = agent.messages();
    let before_count = messages.len();
    let before_tokens = total_tokens(messages) as u64;
    match compact_agent(agent) {
        Some((_, _, after_count, after_tokens)) => {
            println!(
                "{DIM}  compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}\n",
                format_token_count(before_tokens),
                format_token_count(after_tokens)
            );
        }
        None => {
            println!(
                "{DIM}  (nothing to compact — {before_count} messages, ~{} tokens){RESET}\n",
                format_token_count(before_tokens)
            );
        }
    }
}

// ── /save ────────────────────────────────────────────────────────────────

pub fn handle_save(agent: &Agent, input: &str) {
    let path = input.strip_prefix("/save").unwrap_or("").trim();
    let path = if path.is_empty() {
        DEFAULT_SESSION_PATH
    } else {
        path
    };
    match agent.save_messages() {
        Ok(json) => match std::fs::write(path, &json) {
            Ok(_) => println!(
                "{DIM}  (session saved to {path}, {} messages){RESET}\n",
                agent.messages().len()
            ),
            Err(e) => eprintln!("{RED}  error saving: {e}{RESET}\n"),
        },
        Err(e) => eprintln!("{RED}  error serializing: {e}{RESET}\n"),
    }
}

// ── /load ────────────────────────────────────────────────────────────────

pub fn handle_load(agent: &mut Agent, input: &str) {
    let path = input.strip_prefix("/load").unwrap_or("").trim();
    let path = if path.is_empty() {
        DEFAULT_SESSION_PATH
    } else {
        path
    };
    match std::fs::read_to_string(path) {
        Ok(json) => match agent.restore_messages(&json) {
            Ok(_) => println!(
                "{DIM}  (session loaded from {path}, {} messages){RESET}\n",
                agent.messages().len()
            ),
            Err(e) => eprintln!("{RED}  error parsing: {e}{RESET}\n"),
        },
        Err(e) => eprintln!("{RED}  error reading {path}: {e}{RESET}\n"),
    }
}

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

pub fn handle_diff() {
    // Check if we're in a git repo
    let status_output = std::process::Command::new("git")
        .args(["status", "--short"])
        .output();

    match status_output {
        Ok(output) if output.status.success() => {
            let status = String::from_utf8_lossy(&output.stdout);
            if status.trim().is_empty() {
                println!("{DIM}  (no uncommitted changes){RESET}\n");
                return;
            }

            // Get the stat summary
            let stat_text = std::process::Command::new("git")
                .args(["diff", "--stat"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            // Also include staged changes in stat
            let staged_stat_text = std::process::Command::new("git")
                .args(["diff", "--cached", "--stat"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            // Show file status list
            println!("{DIM}  Changes:");
            for line in status.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Color by status code
                let (color, rest) = if trimmed.len() >= 2 {
                    let code = &trimmed[..2];
                    match code.chars().next().unwrap_or(' ') {
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

            // Parse and display stat summary
            let combined_stat =
                if !staged_stat_text.trim().is_empty() && !stat_text.trim().is_empty() {
                    format!("{}\n{}", staged_stat_text, stat_text)
                } else if !staged_stat_text.trim().is_empty() {
                    staged_stat_text
                } else {
                    stat_text
                };

            if !combined_stat.trim().is_empty() {
                let summary = parse_diff_stat(&combined_stat);
                let formatted = format_diff_stat(&summary);
                if !formatted.is_empty() {
                    print!("{formatted}");
                }
            }

            // Show the full diff
            let full_diff = std::process::Command::new("git")
                .args(["diff"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            if !full_diff.trim().is_empty() {
                println!("\n{DIM}  ── Full diff ──{RESET}");
                for line in full_diff.lines() {
                    if line.starts_with('+') && !line.starts_with("+++") {
                        println!("{GREEN}{line}{RESET}");
                    } else if line.starts_with('-') && !line.starts_with("---") {
                        println!("{RED}{line}{RESET}");
                    } else if line.starts_with("@@") {
                        println!("{CYAN}{line}{RESET}");
                    } else if line.starts_with("diff ") || line.starts_with("index ") {
                        println!("{BOLD}{line}{RESET}");
                    } else {
                        println!("{DIM}{line}{RESET}");
                    }
                }
                println!();
            }
        }
        _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
    }
}

// ── /undo ────────────────────────────────────────────────────────────────

pub fn handle_undo() {
    let diff_output = std::process::Command::new("git")
        .args(["diff", "--stat"])
        .output();
    let untracked = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .output();

    let has_diff = diff_output
        .as_ref()
        .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);
    let untracked_files: Vec<String> = untracked
        .as_ref()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();
    let has_untracked = !untracked_files.is_empty();

    if !has_diff && !has_untracked {
        println!("{DIM}  (nothing to undo — no uncommitted changes){RESET}\n");
    } else {
        if has_diff {
            if let Ok(ref output) = diff_output {
                let diff = String::from_utf8_lossy(&output.stdout);
                println!("{DIM}{diff}{RESET}");
            }
        }
        if has_untracked {
            println!("{DIM}  untracked files:");
            for f in &untracked_files {
                println!("    {f}");
            }
            println!("{RESET}");
        }

        if has_diff {
            let _ = std::process::Command::new("git")
                .args(["checkout", "--", "."])
                .output();
        }
        if has_untracked {
            let _ = std::process::Command::new("git")
                .args(["clean", "-fd"])
                .output();
        }
        println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");
    }
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

// ── /context ─────────────────────────────────────────────────────────────

pub fn handle_context() {
    let files = cli::list_project_context_files();
    if files.is_empty() {
        println!("{DIM}  No project context files found.");
        println!("  Create a YOYO.md to give yoyo project context.");
        println!("  Also supports: CLAUDE.md (compatibility alias), .yoyo/instructions.md");
        println!("  Run /init to create a starter YOYO.md.{RESET}\n");
    } else {
        println!("{DIM}  Project context files:");
        for (name, lines) in &files {
            println!("    {name} ({lines} lines)");
        }
        println!("{RESET}");
    }
}

// ── /init ────────────────────────────────────────────────────────────────

/// Scan the project directory and find important files (README, config, CI, etc.).
/// Returns a list of file paths that exist.
pub fn scan_important_files(dir: &std::path::Path) -> Vec<String> {
    let candidates = [
        "README.md",
        "README",
        "readme.md",
        "LICENSE",
        "LICENSE.md",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
        ".gitignore",
        ".editorconfig",
        // Rust
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        // Node
        "package.json",
        "package-lock.json",
        "tsconfig.json",
        ".eslintrc.json",
        ".eslintrc.js",
        ".prettierrc",
        // Python
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "requirements.txt",
        "Pipfile",
        "tox.ini",
        // Go
        "go.mod",
        "go.sum",
        // Build/CI
        "Makefile",
        "Dockerfile",
        "docker-compose.yml",
        "docker-compose.yaml",
        ".dockerignore",
        // CI configs
        ".github/workflows",
        ".gitlab-ci.yml",
        ".circleci/config.yml",
        ".travis.yml",
        "Jenkinsfile",
    ];
    candidates
        .iter()
        .filter(|f| dir.join(f).exists())
        .map(|f| f.to_string())
        .collect()
}

/// Detect key directories in the project (src, tests, docs, etc.).
/// Returns a list of directory names that exist.
pub fn scan_important_dirs(dir: &std::path::Path) -> Vec<String> {
    let candidates = [
        "src",
        "lib",
        "tests",
        "test",
        "docs",
        "doc",
        "examples",
        "benches",
        "scripts",
        ".github",
        ".vscode",
        "config",
        "public",
        "static",
        "assets",
        "migrations",
    ];
    candidates
        .iter()
        .filter(|d| dir.join(d).is_dir())
        .map(|d| d.to_string())
        .collect()
}

/// Get build/test/lint commands for a project type.
pub fn build_commands_for_project(project_type: &ProjectType) -> Vec<(&'static str, &'static str)> {
    match project_type {
        ProjectType::Rust => vec![
            ("Build", "cargo build"),
            ("Test", "cargo test"),
            ("Lint", "cargo clippy --all-targets -- -D warnings"),
            ("Format check", "cargo fmt -- --check"),
            ("Format", "cargo fmt"),
        ],
        ProjectType::Node => vec![
            ("Install", "npm install"),
            ("Test", "npm test"),
            ("Lint", "npx eslint ."),
        ],
        ProjectType::Python => vec![
            ("Test", "python -m pytest"),
            ("Lint", "ruff check ."),
            ("Type check", "python -m mypy ."),
        ],
        ProjectType::Go => vec![
            ("Build", "go build ./..."),
            ("Test", "go test ./..."),
            ("Vet", "go vet ./..."),
        ],
        ProjectType::Make => vec![("Build", "make"), ("Test", "make test")],
        ProjectType::Unknown => vec![],
    }
}

/// Extract the project name from a README.md title line (# Title).
/// Returns None if no README or no title found.
fn extract_project_name_from_readme(dir: &std::path::Path) -> Option<String> {
    let readme_names = ["README.md", "readme.md", "README"];
    for name in &readme_names {
        if let Ok(content) = std::fs::read_to_string(dir.join(name)) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(title) = trimmed.strip_prefix("# ") {
                    let title = title.trim();
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract the project name from Cargo.toml [package] name field.
fn extract_name_from_cargo_toml(dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let val = rest.trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Extract the project name from package.json "name" field.
fn extract_name_from_package_json(dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("package.json")).ok()?;
    // Simple JSON parsing — find "name": "value"
    for line in content.lines() {
        let trimmed = line.trim().trim_end_matches(',');
        if let Some(rest) = trimmed.strip_prefix("\"name\"") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix(':') {
                let val = rest.trim().trim_matches('"');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Best-effort project name detection. Tries multiple sources.
pub fn detect_project_name(dir: &std::path::Path) -> String {
    // Try Cargo.toml name
    if let Some(name) = extract_name_from_cargo_toml(dir) {
        return name;
    }
    // Try package.json name
    if let Some(name) = extract_name_from_package_json(dir) {
        return name;
    }
    // Try README title
    if let Some(name) = extract_project_name_from_readme(dir) {
        return name;
    }
    // Fall back to directory name
    dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-project".to_string())
}

/// Generate a complete YOYO.md context file by scanning the project.
pub fn generate_init_content(dir: &std::path::Path) -> String {
    let project_type = detect_project_type(dir);
    let project_name = detect_project_name(dir);
    let important_files = scan_important_files(dir);
    let important_dirs = scan_important_dirs(dir);
    let build_commands = build_commands_for_project(&project_type);

    let mut content = String::new();

    // Header
    content.push_str("# Project Context\n\n");
    content.push_str("<!-- YOYO.md — generated by `yoyo /init`. Edit to customize. -->\n");
    content.push_str("<!-- Also works as CLAUDE.md for compatibility with other tools. -->\n\n");

    // About section
    content.push_str("## About This Project\n\n");
    content.push_str(&format!("**{project_name}**"));
    if project_type != ProjectType::Unknown {
        content.push_str(&format!(" — {project_type} project"));
    }
    content.push_str("\n\n");
    content.push_str("<!-- Add a description of what this project does. -->\n\n");

    // Build & Test section
    content.push_str("## Build & Test\n\n");
    if build_commands.is_empty() {
        content.push_str("<!-- Add build, test, and run commands for this project. -->\n\n");
    } else {
        content.push_str("```bash\n");
        for (label, cmd) in &build_commands {
            content.push_str(&format!("{cmd:<50} # {label}\n"));
        }
        content.push_str("```\n\n");
    }

    // Coding Conventions section
    content.push_str("## Coding Conventions\n\n");
    content.push_str(
        "<!-- List any coding standards, naming conventions, or patterns to follow. -->\n\n",
    );

    // Important Files section
    content.push_str("## Important Files\n\n");
    if important_files.is_empty() && important_dirs.is_empty() {
        content.push_str("<!-- List key files and directories the agent should know about. -->\n");
    } else {
        if !important_dirs.is_empty() {
            content.push_str("Key directories:\n");
            for d in &important_dirs {
                content.push_str(&format!("- `{d}/`\n"));
            }
            content.push('\n');
        }
        if !important_files.is_empty() {
            content.push_str("Key files:\n");
            for f in &important_files {
                content.push_str(&format!("- `{f}`\n"));
            }
            content.push('\n');
        }
    }

    content
}

pub fn handle_init() {
    let path = "YOYO.md";
    if std::path::Path::new(path).exists() {
        println!("{DIM}  {path} already exists — not overwriting.{RESET}\n");
    } else if std::path::Path::new("CLAUDE.md").exists() {
        println!("{DIM}  CLAUDE.md already exists — yoyo reads it as a compatibility alias.");
        println!("  Rename it to YOYO.md when you're ready: mv CLAUDE.md YOYO.md{RESET}\n");
    } else {
        let cwd = std::env::current_dir().unwrap_or_default();
        let project_type = detect_project_type(&cwd);
        println!("{DIM}  Scanning project...{RESET}");
        if project_type != ProjectType::Unknown {
            println!("{DIM}  Detected: {project_type}{RESET}");
        }
        let content = generate_init_content(&cwd);
        match std::fs::write(path, &content) {
            Ok(_) => {
                let line_count = content.lines().count();
                println!("{GREEN}  ✓ Created {path} ({line_count} lines) — edit it to add project context.{RESET}\n");
            }
            Err(e) => eprintln!("{RED}  error creating {path}: {e}{RESET}\n"),
        }
    }
}

// ── /retry ───────────────────────────────────────────────────────────────

pub async fn handle_retry(
    agent: &mut Agent,
    last_input: &Option<String>,
    session_total: &mut Usage,
    model: &str,
) {
    match last_input {
        Some(prev) => {
            println!("{DIM}  (retrying last input){RESET}");
            let retry_input = prev.clone();
            run_prompt(agent, &retry_input, session_total, model).await;
            auto_compact_if_needed(agent);
        }
        None => {
            eprintln!("{DIM}  (nothing to retry — no previous input){RESET}\n");
        }
    }
}

// ── /model ───────────────────────────────────────────────────────────────

pub fn handle_model_show(model: &str) {
    println!("{DIM}  current model: {model}");
    println!("  usage: /model <name>{RESET}\n");
}

// ── /think ───────────────────────────────────────────────────────────────

pub fn handle_think_show(thinking: ThinkingLevel) {
    let level_str = thinking_level_name(thinking);
    println!("{DIM}  thinking: {level_str}");
    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
}

// ── /history ─────────────────────────────────────────────────────────────

pub fn handle_history(agent: &Agent) {
    let messages = agent.messages();
    if messages.is_empty() {
        println!("{DIM}  (no messages in conversation){RESET}\n");
    } else {
        println!("{DIM}  Conversation ({} messages):", messages.len());
        for (i, msg) in messages.iter().enumerate() {
            let (role, preview) = summarize_message(msg);
            let idx = i + 1;
            println!("    {idx:>3}. [{role}] {preview}");
        }
        println!("{RESET}");
    }
}

// ── /search ──────────────────────────────────────────────────────────────

pub fn handle_search(agent: &Agent, input: &str) {
    if input == "/search" {
        println!("{DIM}  usage: /search <query>");
        println!("  Search conversation history for messages containing <query>.{RESET}\n");
        return;
    }
    let query = input.trim_start_matches("/search ").trim();
    if query.is_empty() {
        println!("{DIM}  usage: /search <query>{RESET}\n");
        return;
    }
    let messages = agent.messages();
    if messages.is_empty() {
        println!("{DIM}  (no messages to search){RESET}\n");
        return;
    }
    let results = search_messages(messages, query);
    if results.is_empty() {
        println!(
            "{DIM}  No matches for '{query}' in {len} messages.{RESET}\n",
            len = messages.len()
        );
    } else {
        println!(
            "{DIM}  {count} match{es} for '{query}':",
            count = results.len(),
            es = if results.len() == 1 { "" } else { "es" }
        );
        for (idx, role, preview) in &results {
            println!("    {idx:>3}. [{role}] {preview}");
        }
        println!("{RESET}");
    }
}

// ── /mark, /jump, /marks (bookmarks) ─────────────────────────────────────

/// Storage for conversation bookmarks: named snapshots of the message list.
pub type Bookmarks = HashMap<String, String>;

/// Parse the bookmark name from `/mark <name>` input.
/// Returns None if no name is provided.
pub fn parse_bookmark_name(input: &str, prefix: &str) -> Option<String> {
    let name = input.strip_prefix(prefix).unwrap_or("").trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Handle `/mark <name>`: save the current conversation state as a named bookmark.
pub fn handle_mark(agent: &Agent, input: &str, bookmarks: &mut Bookmarks) {
    let name = match parse_bookmark_name(input, "/mark") {
        Some(n) => n,
        None => {
            println!("{DIM}  usage: /mark <name>");
            println!("  Save a bookmark at the current point in the conversation.");
            println!("  Use /jump <name> to return to this point later.{RESET}\n");
            return;
        }
    };

    match agent.save_messages() {
        Ok(json) => {
            let msg_count = agent.messages().len();
            let overwriting = bookmarks.contains_key(&name);
            bookmarks.insert(name.clone(), json);
            if overwriting {
                println!("{GREEN}  ✓ bookmark '{name}' updated ({msg_count} messages){RESET}\n");
            } else {
                println!("{GREEN}  ✓ bookmark '{name}' saved ({msg_count} messages){RESET}\n");
            }
        }
        Err(e) => eprintln!("{RED}  error saving bookmark: {e}{RESET}\n"),
    }
}

/// Handle `/jump <name>`: restore conversation to a previously saved bookmark.
pub fn handle_jump(agent: &mut Agent, input: &str, bookmarks: &Bookmarks) {
    let name = match parse_bookmark_name(input, "/jump") {
        Some(n) => n,
        None => {
            println!("{DIM}  usage: /jump <name>");
            println!("  Restore the conversation to a previously saved bookmark.");
            println!("  Messages added after the bookmark will be discarded.{RESET}\n");
            return;
        }
    };

    match bookmarks.get(&name) {
        Some(json) => match agent.restore_messages(json) {
            Ok(_) => {
                let msg_count = agent.messages().len();
                println!("{GREEN}  ✓ jumped to bookmark '{name}' ({msg_count} messages){RESET}\n");
            }
            Err(e) => eprintln!("{RED}  error restoring bookmark: {e}{RESET}\n"),
        },
        None => {
            let available: Vec<&str> = bookmarks.keys().map(|k| k.as_str()).collect();
            if available.is_empty() {
                eprintln!("{RED}  bookmark '{name}' not found — no bookmarks saved yet.");
                eprintln!("  Use /mark <name> to save one.{RESET}\n");
            } else {
                eprintln!("{RED}  bookmark '{name}' not found.");
                eprintln!("{DIM}  available: {}{RESET}\n", available.join(", "));
            }
        }
    }
}

/// Handle `/marks`: list all saved bookmarks.
pub fn handle_marks(bookmarks: &Bookmarks) {
    if bookmarks.is_empty() {
        println!("{DIM}  (no bookmarks saved)");
        println!("  Use /mark <name> to save a bookmark.{RESET}\n");
    } else {
        println!("{DIM}  Saved bookmarks:");
        let mut names: Vec<&String> = bookmarks.keys().collect();
        names.sort();
        for name in names {
            println!("    • {name}");
        }
        println!("{RESET}");
    }
}

// ── /config ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn handle_config(
    provider: &str,
    model: &str,
    base_url: &Option<String>,
    thinking: ThinkingLevel,
    max_tokens: Option<u32>,
    max_turns: Option<usize>,
    temperature: Option<f32>,
    skills: &yoagent::skills::SkillSet,
    system_prompt: &str,
    mcp_count: u32,
    openapi_count: u32,
    agent: &Agent,
    continue_session: bool,
    cwd: &str,
) {
    println!("{DIM}  Configuration:");
    println!("    provider:   {provider}");
    println!("    model:      {model}");
    if let Some(ref url) = base_url {
        println!("    base_url:   {url}");
    }
    println!("    thinking:   {}", thinking_level_name(thinking));
    println!(
        "    max_tokens: {}",
        max_tokens
            .map(|m| m.to_string())
            .unwrap_or_else(|| "default (8192)".to_string())
    );
    println!(
        "    max_turns:  {}",
        max_turns
            .map(|m| m.to_string())
            .unwrap_or_else(|| "default (50)".to_string())
    );
    println!(
        "    temperature: {}",
        temperature
            .map(|t| format!("{t:.1}"))
            .unwrap_or_else(|| "default".to_string())
    );
    println!(
        "    skills:     {}",
        if skills.is_empty() {
            "none".to_string()
        } else {
            format!("{} loaded", skills.len())
        }
    );
    let system_preview =
        truncate_with_ellipsis(system_prompt.lines().next().unwrap_or("(empty)"), 60);
    println!("    system:     {system_preview}");
    if mcp_count > 0 {
        println!("    mcp:        {mcp_count} server(s)");
    }
    if openapi_count > 0 {
        println!("    openapi:    {openapi_count} spec(s)");
    }
    println!(
        "    verbose:    {}",
        if is_verbose() { "on" } else { "off" }
    );
    if let Some(branch) = git_branch() {
        println!("    git:        {branch}");
    }
    println!("    cwd:        {cwd}");
    println!(
        "    context:    {} max tokens",
        format_token_count(MAX_CONTEXT_TOKENS)
    );
    println!(
        "    auto-compact: at {:.0}%",
        AUTO_COMPACT_THRESHOLD * 100.0
    );
    println!("    messages:   {}", agent.messages().len());
    if continue_session {
        println!("    session:    auto-save on exit ({DEFAULT_SESSION_PATH})");
    }
    println!("{RESET}");
}

// ── /docs ────────────────────────────────────────────────────────────────

pub fn handle_docs(input: &str) {
    if input == "/docs" {
        println!("{DIM}  usage: /docs <crate> [item]");
        println!("  Look up docs.rs documentation for a Rust crate.");
        println!("  Examples: /docs serde, /docs tokio task{RESET}\n");
        return;
    }
    let args = input.trim_start_matches("/docs ").trim();
    if args.is_empty() {
        println!("{DIM}  usage: /docs <crate> [item]{RESET}\n");
        return;
    }
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
    let crate_name = parts[0].trim();
    let item_name = parts.get(1).map(|s| s.trim()).unwrap_or("");

    let (found, summary) = if item_name.is_empty() {
        docs::fetch_docs_summary(crate_name)
    } else {
        docs::fetch_docs_item(crate_name, item_name)
    };
    if found {
        let label = if item_name.is_empty() {
            crate_name.to_string()
        } else {
            format!("{crate_name}::{item_name}")
        };
        println!("{GREEN}  ✓ {label}{RESET}");
        println!("{DIM}{summary}{RESET}\n");
    } else {
        println!("{RED}  ✗ {summary}{RESET}\n");
    }
}

// ── /health ──────────────────────────────────────────────────────────────

/// Detected project type based on marker files in the working directory.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Make,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust (Cargo)"),
            ProjectType::Node => write!(f, "Node.js (npm)"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Make => write!(f, "Makefile"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect project type by checking for marker files in the given directory.
pub fn detect_project_type(dir: &std::path::Path) -> ProjectType {
    if dir.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if dir.join("package.json").exists() {
        ProjectType::Node
    } else if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("setup.cfg").exists()
    {
        ProjectType::Python
    } else if dir.join("go.mod").exists() {
        ProjectType::Go
    } else if dir.join("Makefile").exists() || dir.join("makefile").exists() {
        ProjectType::Make
    } else {
        ProjectType::Unknown
    }
}

/// Return health check commands for a given project type.
#[allow(clippy::vec_init_then_push, unused_mut)]
pub fn health_checks_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => {
            let mut checks = vec![("build", vec!["cargo", "build"])];
            #[cfg(not(test))]
            checks.push(("test", vec!["cargo", "test"]));
            checks.push((
                "clippy",
                vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
            ));
            checks.push(("fmt", vec!["cargo", "fmt", "--", "--check"]));
            checks
        }
        ProjectType::Node => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["npm", "test"]));
            checks.push(("lint", vec!["npx", "eslint", "."]));
            checks
        }
        ProjectType::Python => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["python", "-m", "pytest"]));
            checks.push(("lint", vec!["python", "-m", "flake8", "."]));
            checks.push(("typecheck", vec!["python", "-m", "mypy", "."]));
            checks
        }
        ProjectType::Go => {
            let mut checks = vec![("build", vec!["go", "build", "./..."])];
            #[cfg(not(test))]
            checks.push(("test", vec!["go", "test", "./..."]));
            checks.push(("vet", vec!["go", "vet", "./..."]));
            checks
        }
        ProjectType::Make => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["make", "test"]));
            checks
        }
        ProjectType::Unknown => vec![],
    }
}

/// Run health checks for a specific project type. Returns (name, passed, detail) tuples.
pub fn run_health_check_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        let elapsed = format_duration(start.elapsed());
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, format!("ok ({elapsed})")));
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let first_line = stderr.lines().next().unwrap_or("(unknown error)");
                results.push((
                    name,
                    false,
                    format!(
                        "FAIL ({elapsed}): {}",
                        truncate_with_ellipsis(first_line, 80)
                    ),
                ));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Run health checks and capture full error output for failures.
pub fn run_health_checks_full_output(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, String::new()));
            }
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                let mut full_output = String::new();
                if !stdout.is_empty() {
                    full_output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !full_output.is_empty() {
                        full_output.push('\n');
                    }
                    full_output.push_str(&stderr);
                }
                results.push((name, false, full_output));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Build a prompt describing health check failures for the AI to fix.
pub fn build_fix_prompt(failures: &[(&str, &str)]) -> String {
    if failures.is_empty() {
        return String::new();
    }
    let mut prompt = String::from(
        "Fix the following build/lint errors in this project. Read the relevant files, understand the errors, and apply fixes:\n\n",
    );
    for (name, output) in failures {
        prompt.push_str(&format!("## {name} errors:\n```\n{output}\n```\n\n"));
    }
    prompt.push_str(
        "After fixing, run the failing checks again to verify. Fix any remaining issues.",
    );
    prompt
}

pub fn handle_health() {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return;
    }
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_check_for_project(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return;
    }
    let all_passed = results.iter().all(|(_, passed, _)| *passed);
    for (name, passed, detail) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        println!("  {icon} {name}: {detail}");
    }
    if all_passed {
        println!("\n{GREEN}  All checks passed ✓{RESET}\n");
    } else {
        println!("\n{RED}  Some checks failed ✗{RESET}\n");
    }
}

/// Handle the /fix command. Returns Some(fix_prompt) if failures were sent to AI, None otherwise.
pub async fn handle_fix(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }
    println!("{DIM}  Detected project: {project_type}{RESET}");
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_checks_full_output(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return None;
    }
    for (name, passed, _) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        let status = if *passed { "ok" } else { "FAIL" };
        println!("  {icon} {name}: {status}");
    }
    let failures: Vec<(&str, &str)> = results
        .iter()
        .filter(|(_, passed, _)| !passed)
        .map(|(name, _, output)| (*name, output.as_str()))
        .collect();
    if failures.is_empty() {
        println!("\n{GREEN}  All checks passed — nothing to fix ✓{RESET}\n");
        return None;
    }
    let fail_count = failures.len();
    println!("\n{YELLOW}  Sending {fail_count} failure(s) to AI for fixing...{RESET}\n");
    let fix_prompt = build_fix_prompt(&failures);
    run_prompt(agent, &fix_prompt, session_total, model).await;
    auto_compact_if_needed(agent);
    Some(fix_prompt)
}

// ── /test ─────────────────────────────────────────────────────────────

/// Return the test command for a given project type.
pub fn test_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some(("cargo test", vec!["cargo", "test"])),
        ProjectType::Node => Some(("npm test", vec!["npm", "test"])),
        ProjectType::Python => Some(("python -m pytest", vec!["python", "-m", "pytest"])),
        ProjectType::Go => Some(("go test ./...", vec!["go", "test", "./..."])),
        ProjectType::Make => Some(("make test", vec!["make", "test"])),
        ProjectType::Unknown => None,
    }
}

/// Handle the /test command: auto-detect project type and run tests.
/// Returns a summary string suitable for AI context.
pub fn handle_test() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match test_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No test command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Tests passed ({elapsed}){RESET}\n");
                Some(format!("Tests passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Tests failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Tests FAILED (exit {code}, {elapsed}): {label}");
                // Include a preview of the error output for AI context
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /lint ──────────────────────────────────────────────────────────────

/// Return the lint command for a given project type.
pub fn lint_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some((
            "cargo clippy --all-targets -- -D warnings",
            vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
        )),
        ProjectType::Node => Some(("npx eslint .", vec!["npx", "eslint", "."])),
        ProjectType::Python => Some(("ruff check .", vec!["ruff", "check", "."])),
        ProjectType::Go => Some(("golangci-lint run", vec!["golangci-lint", "run"])),
        ProjectType::Make | ProjectType::Unknown => None,
    }
}

/// Handle the /lint command: auto-detect project type and run linter.
/// Returns a summary string suitable for AI context.
pub fn handle_lint() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match lint_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No lint command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Lint passed ({elapsed}){RESET}\n");
                Some(format!("Lint passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Lint failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Lint FAILED (exit {code}, {elapsed}): {label}");
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /tree ────────────────────────────────────────────────────────────────

/// Build a directory tree from `git ls-files`.
pub fn build_project_tree(max_depth: usize) -> String {
    let files = match std::process::Command::new("git")
        .args(["ls-files"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut files: Vec<String> = text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            files.sort();
            files
        }
        _ => return "(not a git repository — /tree requires git)".to_string(),
    };

    if files.is_empty() {
        return "(no tracked files)".to_string();
    }

    format_tree_from_paths(&files, max_depth)
}

/// Format a sorted list of file paths into an indented tree string.
pub fn format_tree_from_paths(paths: &[String], max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut output = String::new();
    let mut printed_dirs: BTreeSet<String> = BTreeSet::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let depth = parts.len() - 1;

        for level in 0..parts.len().saturating_sub(1).min(max_depth) {
            let dir_path: String = parts[..=level].join("/");
            let dir_key = format!("{}/", dir_path);
            if printed_dirs.insert(dir_key) {
                let indent = "  ".repeat(level);
                let dir_name = parts[level];
                output.push_str(&format!("{indent}{dir_name}/\n"));
            }
        }

        if depth <= max_depth {
            let indent = "  ".repeat(depth.min(max_depth));
            let file_name = parts.last().unwrap_or(&"");
            output.push_str(&format!("{indent}{file_name}\n"));
        }
    }

    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

pub fn handle_tree(input: &str) {
    let arg = input.strip_prefix("/tree").unwrap_or("").trim();
    let max_depth = if arg.is_empty() {
        3
    } else {
        match arg.parse::<usize>() {
            Ok(d) => d,
            Err(_) => {
                println!("{DIM}  usage: /tree [depth]  (default depth: 3){RESET}\n");
                return;
            }
        }
    };
    let tree = build_project_tree(max_depth);
    println!("{DIM}{tree}{RESET}\n");
}

// ── /run ─────────────────────────────────────────────────────────────────

/// Run a shell command directly and print its output.
pub fn run_shell_command(cmd: &str) {
    let start = std::time::Instant::now();
    let output = std::process::Command::new("sh").args(["-c", cmd]).output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{RED}{stderr}{RESET}");
            }
            let code = o.status.code().unwrap_or(-1);
            if code == 0 {
                println!("{DIM}  ✓ exit {code} ({elapsed}){RESET}\n");
            } else {
                println!("{RED}  ✗ exit {code} ({elapsed}){RESET}\n");
            }
        }
        Err(e) => {
            eprintln!("{RED}  error running command: {e}{RESET}\n");
        }
    }
}

pub fn handle_run(input: &str) {
    let cmd = if input.starts_with("/run ") {
        input.trim_start_matches("/run ").trim()
    } else if input.starts_with('!') && input.len() > 1 {
        input[1..].trim()
    } else {
        ""
    };
    if cmd.is_empty() {
        println!("{DIM}  usage: /run <command>  or  !<command>{RESET}\n");
    } else {
        run_shell_command(cmd);
    }
}

pub fn handle_run_usage() {
    println!("{DIM}  usage: /run <command>  or  !<command>");
    println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
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
            let response = run_prompt(agent, &prompt, session_total, model).await;

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

// ── /spawn ────────────────────────────────────────────────────────────────

/// Parse the task from a `/spawn <task>` input.
/// Returns None if no task is provided.
pub fn parse_spawn_task(input: &str) -> Option<String> {
    let task = input
        .strip_prefix("/spawn")
        .unwrap_or("")
        .trim()
        .to_string();
    if task.is_empty() {
        None
    } else {
        Some(task)
    }
}

/// Handle the /spawn command: create a fresh subagent, run a task, and return the result.
/// The subagent gets its own independent context window so complex tasks don't pollute
/// the main conversation.
/// Returns Some(context_msg) to be injected back into the main conversation, or None.
pub async fn handle_spawn(
    input: &str,
    agent_config: &crate::AgentConfig,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let task = match parse_spawn_task(input) {
        Some(t) => t,
        None => {
            println!("{DIM}  usage: /spawn <task>");
            println!("  Spawn a subagent with a fresh context to handle a task.");
            println!("  The result is summarized back into your main conversation.");
            println!("  Example: /spawn read src/main.rs and summarize the architecture{RESET}\n");
            return None;
        }
    };

    println!("{CYAN}  🐙 spawning subagent...{RESET}");
    println!(
        "{DIM}  task: {}{RESET}",
        crate::format::truncate_with_ellipsis(&task, 100)
    );

    // Build a fresh agent with the same config but independent context
    let mut sub_agent = agent_config.build_agent();

    // Run the task as a single prompt on the subagent
    let response = run_prompt(&mut sub_agent, &task, session_total, model).await;

    println!("\n{GREEN}  ✓ subagent completed{RESET}");
    println!("{DIM}  injecting result into main conversation...{RESET}\n");

    // Build a context message for the main agent summarizing what the subagent did
    let result_text = if response.trim().is_empty() {
        "(no output)".to_string()
    } else {
        response.trim().to_string()
    };

    let context_msg = format!(
        "A subagent just completed a task. Here is its result:\n\n**Task:** {task}\n\n**Result:**\n{result_text}"
    );

    Some(context_msg)
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
                let unstaged = std::process::Command::new("git")
                    .args(["diff"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_default();
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

// ── /find ────────────────────────────────────────────────────────────────

/// Result of a fuzzy file match: (file_path, score, match_ranges).
/// Higher score = better match. match_ranges are byte offsets into the lowercased path.
#[derive(Debug, Clone, PartialEq)]
pub struct FindMatch {
    pub path: String,
    pub score: i32,
}

/// Score a file path against a fuzzy pattern (case-insensitive substring match).
/// Returns None if the pattern doesn't match.
/// Scoring:
///   - Base score for containing the pattern as a substring
///   - Bonus for matching the filename (last component) vs directory
///   - Bonus for exact filename match
///   - Bonus for match at the start of the filename
///   - Shorter paths score higher (less noise)
pub fn fuzzy_score(path: &str, pattern: &str) -> Option<i32> {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if !path_lower.contains(&pattern_lower) {
        return None;
    }

    let mut score: i32 = 100; // base score for matching

    // Extract filename (last path component)
    let filename = path.rsplit('/').next().unwrap_or(path);
    let filename_lower = filename.to_lowercase();

    // Big bonus if the pattern matches within the filename itself
    if filename_lower.contains(&pattern_lower) {
        score += 50;

        // Bonus for matching at the start of filename
        if filename_lower.starts_with(&pattern_lower) {
            score += 30;
        }

        // Bonus for exact filename match (without extension)
        let stem = filename_lower.split('.').next().unwrap_or(&filename_lower);
        if stem == pattern_lower {
            score += 20;
        }
    }

    // Shorter paths are slightly preferred (less deeply nested = more relevant)
    let depth = path.matches('/').count();
    score -= depth as i32 * 2;

    Some(score)
}

/// Find files matching a fuzzy pattern. Uses `git ls-files` if in a git repo,
/// otherwise falls back to a recursive directory listing.
pub fn find_files(pattern: &str) -> Vec<FindMatch> {
    let files = list_project_files();
    let mut matches: Vec<FindMatch> = files
        .iter()
        .filter_map(|path| {
            fuzzy_score(path, pattern).map(|score| FindMatch {
                path: path.clone(),
                score,
            })
        })
        .collect();

    // Sort by score descending, then alphabetically for ties
    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
    matches
}

/// List all project files. Prefers `git ls-files`, falls back to walkdir-style listing.
fn list_project_files() -> Vec<String> {
    if let Ok(output) = std::process::Command::new("git")
        .args(["ls-files"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            return text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
        }
    }

    // Fallback: recursive listing of current directory (respecting common ignores)
    walk_directory(".", 8)
}

/// Simple recursive directory walk (fallback when not in a git repo).
fn walk_directory(dir: &str, max_depth: usize) -> Vec<String> {
    let mut files = Vec::new();
    walk_directory_inner(dir, max_depth, 0, &mut files);
    files
}

fn walk_directory_inner(dir: &str, max_depth: usize, depth: usize, files: &mut Vec<String>) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs and common ignore patterns
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let path = if dir == "." {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            walk_directory_inner(&path, max_depth, depth + 1, files);
        } else {
            files.push(path);
        }
    }
}

/// Highlight the matching pattern within a file path for display.
/// Returns the path with ANSI bold/color around the matched portion.
pub fn highlight_match(path: &str, pattern: &str) -> String {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if let Some(pos) = path_lower.rfind(&pattern_lower) {
        // Prefer highlighting in the filename portion
        let end = pos + pattern.len();
        format!(
            "{}{BOLD}{GREEN}{}{RESET}{}",
            &path[..pos],
            &path[pos..end],
            &path[end..]
        )
    } else {
        path.to_string()
    }
}

pub fn handle_find(input: &str) {
    let arg = input.strip_prefix("/find").unwrap_or("").trim();
    if arg.is_empty() {
        println!("{DIM}  usage: /find <pattern>");
        println!("  Fuzzy-search project files by name.");
        println!("  Examples: /find main, /find .toml, /find test{RESET}\n");
        return;
    }

    let matches = find_files(arg);
    if matches.is_empty() {
        println!("{DIM}  No files matching '{arg}'.{RESET}\n");
    } else {
        let count = matches.len();
        let shown = matches.iter().take(20);
        println!(
            "{DIM}  {count} file{s} matching '{arg}':",
            s = if count == 1 { "" } else { "s" }
        );
        for m in shown {
            let highlighted = highlight_match(&m.path, arg);
            println!("    {highlighted}");
        }
        if count > 20 {
            println!("    {DIM}... and {} more{RESET}", count - 20);
        }
        println!("{RESET}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::ThinkingLevel;

    #[test]
    fn test_command_parsing_quit() {
        let quit_commands = ["/quit", "/exit"];
        for cmd in &quit_commands {
            assert!(
                *cmd == "/quit" || *cmd == "/exit",
                "Unrecognized quit command: {cmd}"
            );
        }
    }

    #[test]
    fn test_command_parsing_model() {
        let input = "/model claude-opus-4-6";
        assert!(input.starts_with("/model "));
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_parsing_model_whitespace() {
        let input = "/model   claude-opus-4-6  ";
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_help_recognized() {
        let commands = [
            "/help", "/quit", "/exit", "/clear", "/compact", "/commit", "/config", "/context",
            "/cost", "/docs", "/find", "/fix", "/init", "/status", "/tokens", "/save", "/load",
            "/diff", "/undo", "/health", "/retry", "/run", "/history", "/search", "/model",
            "/think", "/version", "/tree", "/pr", "/git", "/test", "/lint", "/spawn", "/review",
            "/mark", "/jump", "/marks",
        ];
        for cmd in &commands {
            assert!(
                KNOWN_COMMANDS.contains(cmd),
                "Command not in KNOWN_COMMANDS: {cmd}"
            );
        }
    }

    #[test]
    fn test_model_switch_updates_variable() {
        let original = "claude-opus-4-6";
        let input = "/model claude-haiku-35";
        let new_model = input.trim_start_matches("/model ").trim();
        assert_ne!(new_model, original);
        assert_eq!(new_model, "claude-haiku-35");
    }

    #[test]
    fn test_bare_model_command_is_recognized() {
        let input = "/model";
        assert_eq!(input, "/model");
        assert!(!input.starts_with("/model "));
    }

    #[test]
    fn test_unknown_slash_command_detection() {
        assert!(is_unknown_command("/foo"));
        assert!(is_unknown_command("/foo bar baz"));
        assert!(is_unknown_command("/unknown argument"));
        // Verify typo-like commands are caught as unknown
        assert!(is_unknown_command("/savefile"));
        assert!(is_unknown_command("/loadfile"));

        assert!(!is_unknown_command("/help"));
        assert!(!is_unknown_command("/quit"));
        assert!(!is_unknown_command("/model"));
        assert!(!is_unknown_command("/model claude-opus-4-6"));
        assert!(!is_unknown_command("/save"));
        assert!(!is_unknown_command("/save myfile.json"));
        assert!(!is_unknown_command("/load"));
        assert!(!is_unknown_command("/load myfile.json"));
        assert!(!is_unknown_command("/config"));
        assert!(!is_unknown_command("/context"));
        assert!(!is_unknown_command("/version"));
    }

    #[test]
    fn test_thinking_level_name() {
        assert_eq!(thinking_level_name(ThinkingLevel::Off), "off");
        assert_eq!(thinking_level_name(ThinkingLevel::Minimal), "minimal");
        assert_eq!(thinking_level_name(ThinkingLevel::Low), "low");
        assert_eq!(thinking_level_name(ThinkingLevel::Medium), "medium");
        assert_eq!(thinking_level_name(ThinkingLevel::High), "high");
    }

    #[test]
    fn test_health_check_function() {
        // run_health_check_for_project skips "cargo test" under #[cfg(test)] to avoid recursion
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_check_for_project(&project_type);
        assert!(
            !results.is_empty(),
            "Health check should return at least one result"
        );
        for (name, passed, _) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
        // "test" check should be excluded under cfg(test)
        assert!(
            !results.iter().any(|(name, _, _)| *name == "test"),
            "cargo test check should be skipped to avoid recursion"
        );
    }

    #[test]
    fn test_detect_project_type_rust() {
        // Current directory has Cargo.toml, so should detect Rust
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(detect_project_type(&cwd), ProjectType::Rust);
    }

    #[test]
    fn test_detect_project_type_node() {
        let tmp = std::env::temp_dir().join("yoyo_test_node");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Node);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_pyproject() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_pyproject");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_setup_py() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_setup");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("setup.py"), "").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_go() {
        let tmp = std::env::temp_dir().join("yoyo_test_go");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("go.mod"), "module example.com/test").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Go);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_makefile() {
        let tmp = std::env::temp_dir().join("yoyo_test_make");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Makefile"), "test:\n\techo ok").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Make);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_unknown() {
        let tmp = std::env::temp_dir().join("yoyo_test_unknown");
        let _ = std::fs::create_dir_all(&tmp);
        // Empty dir — no marker files
        assert_eq!(detect_project_type(&tmp), ProjectType::Unknown);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_priority_rust_over_makefile() {
        // If both Cargo.toml and Makefile exist, Rust wins
        let tmp = std::env::temp_dir().join("yoyo_test_priority");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(tmp.join("Makefile"), "test:").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Rust);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_health_checks_for_rust_project() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Rust should have build check");
        assert!(names.contains(&"clippy"), "Rust should have clippy check");
        assert!(names.contains(&"fmt"), "Rust should have fmt check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    #[test]
    fn test_health_checks_for_node_project() {
        let checks = health_checks_for_project(&ProjectType::Node);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Node should have lint check");
    }

    #[test]
    fn test_health_checks_for_go_project() {
        let checks = health_checks_for_project(&ProjectType::Go);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Go should have build check");
        assert!(names.contains(&"vet"), "Go should have vet check");
    }

    #[test]
    fn test_health_checks_for_python_project() {
        let checks = health_checks_for_project(&ProjectType::Python);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Python should have lint check");
        assert!(names.contains(&"typecheck"), "Python should have typecheck");
    }

    #[test]
    fn test_health_checks_for_unknown_returns_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty(), "Unknown project should return no checks");
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(format!("{}", ProjectType::Rust), "Rust (Cargo)");
        assert_eq!(format!("{}", ProjectType::Node), "Node.js (npm)");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
        assert_eq!(format!("{}", ProjectType::Go), "Go");
        assert_eq!(format!("{}", ProjectType::Make), "Makefile");
        assert_eq!(format!("{}", ProjectType::Unknown), "Unknown");
    }

    #[test]
    fn test_run_command_recognized() {
        assert!(!is_unknown_command("/run"));
        assert!(!is_unknown_command("/run echo hello"));
        assert!(!is_unknown_command("/run ls -la"));
    }

    #[test]
    fn test_run_shell_command_basic() {
        // Verify run_shell_command doesn't panic on basic commands
        // (output goes to stdout/stderr, we just check it doesn't crash)
        run_shell_command("echo hello");
    }

    #[test]
    fn test_run_shell_command_failing() {
        // Non-zero exit should not panic
        run_shell_command("false");
    }

    #[test]
    fn test_bang_shortcut_matching() {
        // ! prefix should match for /run shortcut
        let bang_matches = |s: &str| s.starts_with('!') && s.len() > 1;
        assert!(bang_matches("!ls"));
        assert!(bang_matches("!echo hello"));
        assert!(bang_matches("! ls")); // space after bang is fine
        assert!(!bang_matches("!")); // bare bang alone should not match
    }

    #[test]
    fn test_run_command_matching() {
        // /run should only match /run or /run <cmd>, not /running
        let run_matches = |s: &str| s == "/run" || s.starts_with("/run ");
        assert!(run_matches("/run"));
        assert!(run_matches("/run echo hello"));
        assert!(!run_matches("/running"));
        assert!(!run_matches("/runaway"));
    }

    #[test]
    fn test_format_tree_from_paths_basic() {
        let paths = vec![
            "Cargo.toml".to_string(),
            "README.md".to_string(),
            "src/cli.rs".to_string(),
            "src/format.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("  main.rs"));
        assert!(tree.contains("  cli.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_nested() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/utils/helpers.rs".to_string(),
            "src/utils/format.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("  utils/"));
        assert!(tree.contains("    helpers.rs"));
        assert!(tree.contains("    format.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_limit() {
        let paths = vec![
            "a/b/c/d/deep.txt".to_string(),
            "a/shallow.txt".to_string(),
            "top.txt".to_string(),
        ];
        // depth 1: show dirs at level 0 ('a/'), files at depth ≤ 1
        let tree = format_tree_from_paths(&paths, 1);
        assert!(tree.contains("top.txt"));
        assert!(tree.contains("a/"));
        assert!(tree.contains("  shallow.txt"));
        // Files deeper than max_depth should not appear
        assert!(!tree.contains("deep.txt"));
        // Directory 'b/' is at level 1, beyond max_depth=1 for dirs
        assert!(!tree.contains("b/"));
    }

    #[test]
    fn test_format_tree_from_paths_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_format_tree_from_paths_root_files_only() {
        let paths = vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // No directories, just root files
        assert!(!tree.contains('/'));
        assert!(tree.contains("Cargo.lock"));
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_zero() {
        let paths = vec!["README.md".to_string(), "src/main.rs".to_string()];
        let tree = format_tree_from_paths(&paths, 0);
        // Depth 0: only root-level files shown
        assert!(tree.contains("README.md"));
        // main.rs is at depth 1, should not show at depth 0
        assert!(!tree.contains("main.rs"));
    }

    #[test]
    fn test_format_tree_dir_printed_once() {
        let paths = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // "src/" should appear exactly once
        assert_eq!(tree.matches("src/").count(), 1);
    }

    #[test]
    fn test_build_project_tree_runs() {
        // build_project_tree should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
        // In a git repo, should contain Cargo.toml; outside one (e.g. cargo-mutants
        // temp dir) the tree still works but uses filesystem walk instead of git ls-files
    }

    #[test]
    fn test_tree_command_recognized() {
        assert!(!is_unknown_command("/tree"));
        assert!(!is_unknown_command("/tree 2"));
        assert!(!is_unknown_command("/tree 5"));
    }

    #[test]
    fn test_pr_command_recognized() {
        assert!(!is_unknown_command("/pr"));
        assert!(!is_unknown_command("/pr 42"));
        assert!(!is_unknown_command("/pr 123"));
    }

    #[test]
    fn test_pr_command_matching() {
        // /pr should match exact or with space separator, not /print etc.
        let pr_matches = |s: &str| s == "/pr" || s.starts_with("/pr ");
        assert!(pr_matches("/pr"));
        assert!(pr_matches("/pr 42"));
        assert!(pr_matches("/pr 123"));
        assert!(!pr_matches("/print"));
        assert!(!pr_matches("/process"));
    }

    #[test]
    fn test_pr_number_parsing() {
        // Verify we can parse a PR number from /pr <number>
        let input = "/pr 42";
        let arg = input.strip_prefix("/pr").unwrap_or("").trim();
        assert_eq!(arg, "42");
        assert!(arg.parse::<u32>().is_ok());
        assert_eq!(arg.parse::<u32>().unwrap(), 42);

        // Bare /pr has empty arg
        let input_bare = "/pr";
        let arg_bare = input_bare.strip_prefix("/pr").unwrap_or("").trim();
        assert!(arg_bare.is_empty());
    }

    #[test]
    fn test_pr_subcommand_list() {
        assert_eq!(parse_pr_args(""), PrSubcommand::List);
        assert_eq!(parse_pr_args("  "), PrSubcommand::List);
    }

    #[test]
    fn test_pr_subcommand_view() {
        assert_eq!(parse_pr_args("42"), PrSubcommand::View(42));
        assert_eq!(parse_pr_args("123"), PrSubcommand::View(123));
        assert_eq!(parse_pr_args("1"), PrSubcommand::View(1));
    }

    #[test]
    fn test_pr_subcommand_diff() {
        assert_eq!(parse_pr_args("42 diff"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("7 diff"), PrSubcommand::Diff(7));
    }

    #[test]
    fn test_pr_subcommand_checkout() {
        assert_eq!(parse_pr_args("42 checkout"), PrSubcommand::Checkout(42));
        assert_eq!(parse_pr_args("99 checkout"), PrSubcommand::Checkout(99));
    }

    #[test]
    fn test_pr_subcommand_comment() {
        assert_eq!(
            parse_pr_args("42 comment looks good!"),
            PrSubcommand::Comment(42, "looks good!".to_string())
        );
        assert_eq!(
            parse_pr_args("10 comment LGTM, merging now"),
            PrSubcommand::Comment(10, "LGTM, merging now".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_comment_requires_text() {
        // comment without text should show help
        assert_eq!(parse_pr_args("42 comment"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 comment  "), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_invalid() {
        assert_eq!(parse_pr_args("abc"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 unknown"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 merge"), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_case_insensitive() {
        assert_eq!(parse_pr_args("42 DIFF"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("42 Checkout"), PrSubcommand::Checkout(42));
        assert_eq!(
            parse_pr_args("42 Comment nice work"),
            PrSubcommand::Comment(42, "nice work".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_create() {
        assert_eq!(
            parse_pr_args("create"),
            PrSubcommand::Create { draft: false }
        );
        assert_eq!(
            parse_pr_args("CREATE"),
            PrSubcommand::Create { draft: false }
        );
        assert_eq!(
            parse_pr_args("Create"),
            PrSubcommand::Create { draft: false }
        );
    }

    #[test]
    fn test_pr_subcommand_create_draft() {
        assert_eq!(
            parse_pr_args("create --draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("create draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("CREATE --DRAFT"),
            PrSubcommand::Create { draft: true }
        );
    }

    #[test]
    fn test_pr_subcommand_create_no_flag() {
        // "create somethingelse" should still create but not be draft
        assert_eq!(
            parse_pr_args("create --nodraft"),
            PrSubcommand::Create { draft: false }
        );
    }

    #[test]
    fn test_pr_subcommand_recognized() {
        // Subcommands should not be flagged as unknown commands
        assert!(!is_unknown_command("/pr 42 diff"));
        assert!(!is_unknown_command("/pr 42 comment hello"));
        assert!(!is_unknown_command("/pr 42 checkout"));
    }

    #[test]
    fn test_save_load_command_matching() {
        // /save and /load should only match exact word or with space separator
        // This tests the fix for /savefile being treated as /save
        let save_matches = |s: &str| s == "/save" || s.starts_with("/save ");
        let load_matches = |s: &str| s == "/load" || s.starts_with("/load ");

        assert!(save_matches("/save"));
        assert!(save_matches("/save myfile.json"));
        assert!(!save_matches("/savefile"));
        assert!(!save_matches("/saveXYZ"));

        assert!(load_matches("/load"));
        assert!(load_matches("/load myfile.json"));
        assert!(!load_matches("/loadfile"));
        assert!(!load_matches("/loadXYZ"));
    }

    #[test]
    fn test_fix_command_recognized() {
        assert!(!is_unknown_command("/fix"));
        assert!(
            KNOWN_COMMANDS.contains(&"/fix"),
            "/fix should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_run_health_checks_full_output_returns_results() {
        // In a Rust project, should return results with full error output
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_checks_full_output(&project_type);
        assert!(
            !results.is_empty(),
            "Should return at least one check result"
        );
        for (name, passed, _output) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
    }

    #[test]
    fn test_build_fix_prompt_with_failures() {
        let failures = vec![
            (
                "build",
                "error[E0308]: mismatched types\n  --> src/main.rs:42",
            ),
            (
                "clippy",
                "warning: unused variable `x`\n  --> src/lib.rs:10",
            ),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build"), "Prompt should mention build");
        assert!(prompt.contains("clippy"), "Prompt should mention clippy");
        assert!(
            prompt.contains("error[E0308]"),
            "Prompt should include build error"
        );
        assert!(
            prompt.contains("unused variable"),
            "Prompt should include clippy warning"
        );
    }

    #[test]
    fn test_build_fix_prompt_empty_failures() {
        let failures: Vec<(&str, &str)> = vec![];
        let prompt = build_fix_prompt(&failures);
        assert!(
            prompt.is_empty() || prompt.contains("Fix"),
            "Empty failures should produce empty or minimal prompt"
        );
    }

    #[test]
    fn test_test_command_recognized() {
        assert!(!is_unknown_command("/test"));
        assert!(
            KNOWN_COMMANDS.contains(&"/test"),
            "/test should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_test_command_for_rust_project() {
        let cmd = test_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("cargo"),
            "Rust test label should mention cargo"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_node_project() {
        let cmd = test_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("npm"), "Node test label should mention npm");
        assert_eq!(args[0], "npm");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_python_project() {
        let cmd = test_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a test command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("pytest"),
            "Python test label should mention pytest"
        );
    }

    #[test]
    fn test_test_command_for_go_project() {
        let cmd = test_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("go"), "Go test label should mention go");
        assert_eq!(args[0], "go");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_make_project() {
        let cmd = test_command_for_project(&ProjectType::Make);
        assert!(cmd.is_some(), "Make project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("make"),
            "Make test label should mention make"
        );
        assert_eq!(args[0], "make");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_unknown_project() {
        let cmd = test_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a test command"
        );
    }

    #[test]
    fn test_lint_command_recognized() {
        assert!(!is_unknown_command("/lint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/lint"),
            "/lint should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_lint_command_for_rust_project() {
        let cmd = lint_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("clippy"),
            "Rust lint label should mention clippy"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"clippy"));
    }

    #[test]
    fn test_lint_command_for_node_project() {
        let cmd = lint_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("eslint"),
            "Node lint label should mention eslint"
        );
        assert_eq!(args[0], "npx");
        assert!(args.contains(&"eslint"));
    }

    #[test]
    fn test_lint_command_for_python_project() {
        let cmd = lint_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a lint command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("ruff"),
            "Python lint label should mention ruff"
        );
    }

    #[test]
    fn test_lint_command_for_go_project() {
        let cmd = lint_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("golangci-lint"),
            "Go lint label should mention golangci-lint"
        );
        assert_eq!(args[0], "golangci-lint");
    }

    #[test]
    fn test_lint_command_for_make_project() {
        let cmd = lint_command_for_project(&ProjectType::Make);
        assert!(cmd.is_none(), "Make project should not have a lint command");
    }

    #[test]
    fn test_lint_command_for_unknown_project() {
        let cmd = lint_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a lint command"
        );
    }

    #[test]
    fn test_docs_command_recognized() {
        assert!(!is_unknown_command("/docs"));
        assert!(!is_unknown_command("/docs serde"));
        assert!(!is_unknown_command("/docs tokio"));
        assert!(
            KNOWN_COMMANDS.contains(&"/docs"),
            "/docs should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_docs_command_matching() {
        // /docs should match exact or with space, not /docstring etc.
        let docs_matches = |s: &str| s == "/docs" || s.starts_with("/docs ");
        assert!(docs_matches("/docs"));
        assert!(docs_matches("/docs serde"));
        assert!(docs_matches("/docs tokio-runtime"));
        assert!(!docs_matches("/docstring"));
        assert!(!docs_matches("/docsify"));
    }

    #[test]
    fn test_docs_crate_arg_extraction() {
        let input = "/docs serde";
        let crate_name = input.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name, "serde");

        let input2 = "/docs tokio-runtime";
        let crate_name2 = input2.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name2, "tokio-runtime");

        // Bare /docs has empty after stripping
        let input_bare = "/docs";
        assert_eq!(input_bare, "/docs");
        assert!(!input_bare.starts_with("/docs "));
    }

    #[test]
    fn test_spawn_command_recognized() {
        assert!(!is_unknown_command("/spawn"));
        assert!(!is_unknown_command("/spawn read src/main.rs and summarize"));
        assert!(
            KNOWN_COMMANDS.contains(&"/spawn"),
            "/spawn should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_spawn_command_matching() {
        // /spawn should match exact or with space separator, not /spawning
        let spawn_matches = |s: &str| s == "/spawn" || s.starts_with("/spawn ");
        assert!(spawn_matches("/spawn"));
        assert!(spawn_matches("/spawn read file"));
        assert!(spawn_matches("/spawn analyze the codebase"));
        assert!(!spawn_matches("/spawning"));
        assert!(!spawn_matches("/spawnpoint"));
    }

    #[test]
    fn test_parse_spawn_task_with_task() {
        let task = parse_spawn_task("/spawn read src/main.rs and summarize");
        assert_eq!(task, Some("read src/main.rs and summarize".to_string()));
    }

    #[test]
    fn test_parse_spawn_task_empty() {
        let task = parse_spawn_task("/spawn");
        assert_eq!(task, None);
    }

    #[test]
    fn test_parse_spawn_task_whitespace_only() {
        let task = parse_spawn_task("/spawn   ");
        assert_eq!(task, None);
    }

    #[test]
    fn test_parse_spawn_task_preserves_full_task() {
        let task = parse_spawn_task("/spawn analyze src/ and list all public functions");
        assert_eq!(
            task,
            Some("analyze src/ and list all public functions".to_string())
        );
    }

    #[test]
    fn test_find_command_recognized() {
        assert!(!is_unknown_command("/find"));
        assert!(!is_unknown_command("/find main"));
        assert!(!is_unknown_command("/find .toml"));
        assert!(
            KNOWN_COMMANDS.contains(&"/find"),
            "/find should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_fuzzy_score_basic_match() {
        // Pattern found in path → Some score
        let score = fuzzy_score("src/main.rs", "main");
        assert!(score.is_some(), "should match 'main' in 'src/main.rs'");
        assert!(score.unwrap() > 0, "score should be positive");
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        let score = fuzzy_score("src/main.rs", "zzznotfound");
        assert!(score.is_none(), "should not match 'zzznotfound'");
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        let score_lower = fuzzy_score("src/main.rs", "main");
        let score_upper = fuzzy_score("src/main.rs", "MAIN");
        assert!(score_lower.is_some());
        assert!(score_upper.is_some());
        // Both should match with same score
        assert_eq!(score_lower, score_upper);
    }

    #[test]
    fn test_fuzzy_score_filename_match_higher() {
        // "main" matches in filename for "src/main.rs" but only in dir for "main/other.rs"
        let filename_score = fuzzy_score("src/main.rs", "main");
        let dir_score = fuzzy_score("main_stuff/other.rs", "main");
        assert!(filename_score.is_some());
        assert!(dir_score.is_some());
        // Filename match should score higher because it gets the filename bonus
        assert!(
            filename_score.unwrap() > dir_score.unwrap(),
            "filename match should score higher: {} vs {}",
            filename_score.unwrap(),
            dir_score.unwrap()
        );
    }

    #[test]
    fn test_fuzzy_score_start_of_filename_bonus() {
        // "cli" at start of filename should score higher than "cli" embedded elsewhere
        let start_score = fuzzy_score("src/cli.rs", "cli");
        let mid_score = fuzzy_score("src/public_client.rs", "cli");
        assert!(start_score.is_some());
        assert!(mid_score.is_some());
        assert!(
            start_score.unwrap() > mid_score.unwrap(),
            "start-of-filename match should score higher: {} vs {}",
            start_score.unwrap(),
            mid_score.unwrap()
        );
    }

    #[test]
    fn test_find_files_returns_sorted() {
        // Search for a common pattern in this project
        let matches = find_files("main");
        assert!(!matches.is_empty(), "should find files matching 'main'");
        // Results should be sorted by score descending
        for window in matches.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "results should be sorted by score descending: {} >= {}",
                window[0].score,
                window[1].score
            );
        }
    }

    #[test]
    fn test_find_files_no_results() {
        let matches = find_files("xyzzy_nonexistent_pattern_12345");
        assert!(
            matches.is_empty(),
            "should find no files for nonsense pattern"
        );
    }

    #[test]
    fn test_find_command_matching() {
        // /find should match exact or with space separator, not /finding
        let find_matches = |s: &str| s == "/find" || s.starts_with("/find ");
        assert!(find_matches("/find"));
        assert!(find_matches("/find main"));
        assert!(find_matches("/find .toml"));
        assert!(!find_matches("/finding"));
        assert!(!find_matches("/findall"));
    }

    #[test]
    fn test_highlight_match_basic() {
        let result = highlight_match("src/main.rs", "main");
        // Should contain the original path text
        assert!(result.contains("main"));
        assert!(result.contains("src/"));
        assert!(result.contains(".rs"));
    }

    // ── /review tests ──────────────────────────────────────────────────────

    #[test]
    fn test_review_command_recognized() {
        assert!(!is_unknown_command("/review"));
        assert!(!is_unknown_command("/review src/main.rs"));
        assert!(
            KNOWN_COMMANDS.contains(&"/review"),
            "/review should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_review_command_matching() {
        // /review should match exact or with space separator, not /reviewing
        let review_matches = |s: &str| s == "/review" || s.starts_with("/review ");
        assert!(review_matches("/review"));
        assert!(review_matches("/review src/main.rs"));
        assert!(review_matches("/review Cargo.toml"));
        assert!(!review_matches("/reviewing"));
        assert!(!review_matches("/reviewer"));
    }

    #[test]
    fn test_build_review_prompt_contains_content() {
        let prompt =
            build_review_prompt("staged changes", "fn main() {\n    println!(\"hello\");\n}");
        assert!(
            prompt.contains("staged changes"),
            "Should mention the label"
        );
        assert!(prompt.contains("fn main()"), "Should contain the code");
        assert!(prompt.contains("Bugs"), "Should ask for bug review");
        assert!(
            prompt.contains("Security"),
            "Should ask for security review"
        );
        assert!(prompt.contains("Style"), "Should ask for style review");
        assert!(
            prompt.contains("Performance"),
            "Should ask for performance review"
        );
        assert!(prompt.contains("Suggestions"), "Should ask for suggestions");
    }

    #[test]
    fn test_build_review_prompt_truncates_large_content() {
        let large_content = "x".repeat(40_000);
        let prompt = build_review_prompt("big file", &large_content);
        assert!(
            prompt.contains("truncated"),
            "Large content should be truncated"
        );
        assert!(
            prompt.len() < 40_000,
            "Prompt should be truncated, got {} chars",
            prompt.len()
        );
    }

    #[test]
    fn test_build_review_content_nonexistent_file() {
        let result = build_review_content("nonexistent_file_xyz_12345.rs");
        assert!(result.is_none(), "Nonexistent file should return None");
    }

    #[test]
    fn test_build_review_content_existing_file() {
        // Cargo.toml exists in the project root
        let result = build_review_content("Cargo.toml");
        assert!(result.is_some(), "Existing file should return Some");
        let (label, content) = result.unwrap();
        assert_eq!(label, "Cargo.toml");
        assert!(!content.is_empty(), "Content should not be empty");
    }

    #[test]
    fn test_build_review_content_empty_arg_in_git_repo() {
        // Empty arg reviews staged/unstaged changes
        // In CI, this may or may not have changes — just verify it doesn't panic
        let result = build_review_content("");
        // Result depends on git state — either Some or None is valid
        if let Some((label, _content)) = result {
            assert!(
                label.contains("changes"),
                "Label should describe what's being reviewed: {label}"
            );
        }
    }

    #[test]
    fn test_review_help_text_present() {
        // Verify /review appears in the help output by checking the handle_help function output
        // We can't easily capture stdout, but we can verify the command is in KNOWN_COMMANDS
        // and that the help text format is correct
        assert!(KNOWN_COMMANDS.contains(&"/review"));
    }

    // ── /init scanning tests ─────────────────────────────────────────────

    #[test]
    fn test_init_command_recognized() {
        assert!(!is_unknown_command("/init"));
        assert!(
            KNOWN_COMMANDS.contains(&"/init"),
            "/init should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_scan_important_files_in_current_project() {
        let cwd = std::env::current_dir().unwrap();
        let files = scan_important_files(&cwd);
        // This is a Rust project, so Cargo.toml should be found
        assert!(
            files.contains(&"Cargo.toml".to_string()),
            "Should find Cargo.toml: {files:?}"
        );
    }

    #[test]
    fn test_scan_important_files_empty_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let files = scan_important_files(&tmp);
        assert!(files.is_empty(), "Empty dir should have no important files");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_files_with_readme() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_readme");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("README.md"), "# Hello").unwrap();
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        let files = scan_important_files(&tmp);
        assert!(
            files.contains(&"README.md".to_string()),
            "Should find README.md"
        );
        assert!(
            files.contains(&"package.json".to_string()),
            "Should find package.json"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_dirs_in_current_project() {
        let cwd = std::env::current_dir().unwrap();
        let dirs = scan_important_dirs(&cwd);
        // This project has src/
        assert!(
            dirs.contains(&"src".to_string()),
            "Should find src/ dir: {dirs:?}"
        );
    }

    #[test]
    fn test_scan_important_dirs_empty_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_dirs_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.is_empty(), "Empty dir should have no important dirs");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_dirs_with_subdirs() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_subdirs");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        let _ = std::fs::create_dir_all(tmp.join("tests"));
        let _ = std::fs::create_dir_all(tmp.join("docs"));
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.contains(&"src".to_string()), "Should find src/");
        assert!(dirs.contains(&"tests".to_string()), "Should find tests/");
        assert!(dirs.contains(&"docs".to_string()), "Should find docs/");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_build_commands_for_rust() {
        let cmds = build_commands_for_project(&ProjectType::Rust);
        assert!(!cmds.is_empty(), "Rust should have build commands");
        let labels: Vec<&str> = cmds.iter().map(|(l, _)| *l).collect();
        assert!(labels.contains(&"Build"), "Should have Build command");
        assert!(labels.contains(&"Test"), "Should have Test command");
        assert!(labels.contains(&"Lint"), "Should have Lint command");
    }

    #[test]
    fn test_build_commands_for_node() {
        let cmds = build_commands_for_project(&ProjectType::Node);
        assert!(!cmds.is_empty(), "Node should have build commands");
        let labels: Vec<&str> = cmds.iter().map(|(l, _)| *l).collect();
        assert!(labels.contains(&"Test"), "Should have Test command");
    }

    #[test]
    fn test_build_commands_for_unknown() {
        let cmds = build_commands_for_project(&ProjectType::Unknown);
        assert!(
            cmds.is_empty(),
            "Unknown project should have no build commands"
        );
    }

    #[test]
    fn test_detect_project_name_rust() {
        // Current project has Cargo.toml with a name field
        let cwd = std::env::current_dir().unwrap();
        let name = detect_project_name(&cwd);
        assert_eq!(
            name, "yoyo",
            "Should detect project name 'yoyo' from Cargo.toml"
        );
    }

    #[test]
    fn test_detect_project_name_fallback_to_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_fallback");
        let _ = std::fs::create_dir_all(&tmp);
        let name = detect_project_name(&tmp);
        assert_eq!(
            name, "yoyo_test_name_fallback",
            "Should fall back to directory name"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_name_from_readme() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_readme");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("README.md"), "# My Awesome Project\n\nSome text.").unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(
            name, "My Awesome Project",
            "Should extract name from README title"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_name_from_package_json() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_pkg");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("package.json"),
            "{\n  \"name\": \"cool-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(name, "cool-app", "Should extract name from package.json");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_generate_init_content_rust_project() {
        let cwd = std::env::current_dir().unwrap();
        let content = generate_init_content(&cwd);
        // Should contain project name
        assert!(
            content.contains("yoyo"),
            "Should contain project name: {}",
            &content[..200.min(content.len())]
        );
        // Should detect Rust
        assert!(content.contains("Rust"), "Should mention Rust project type");
        // Should have build commands
        assert!(
            content.contains("cargo build"),
            "Should include cargo build command"
        );
        assert!(
            content.contains("cargo test"),
            "Should include cargo test command"
        );
        // Should have sections
        assert!(
            content.contains("## Build & Test"),
            "Should have Build & Test section"
        );
        assert!(
            content.contains("## Important Files"),
            "Should have Important Files section"
        );
        assert!(
            content.contains("## Coding Conventions"),
            "Should have Coding Conventions section"
        );
        // Should list Cargo.toml as important file
        assert!(
            content.contains("Cargo.toml"),
            "Should list Cargo.toml as important"
        );
        // Should list src/ as important dir
        assert!(
            content.contains("`src/`"),
            "Should list src/ as important dir"
        );
    }

    #[test]
    fn test_generate_init_content_empty_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_gen_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let content = generate_init_content(&tmp);
        // Should still have sections even for empty/unknown project
        assert!(content.contains("# Project Context"));
        assert!(content.contains("## About This Project"));
        assert!(content.contains("## Build & Test"));
        assert!(content.contains("## Coding Conventions"));
        assert!(content.contains("## Important Files"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_generate_init_content_node_project() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_gen_node");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("package.json"),
            "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        let _ = std::fs::create_dir_all(tmp.join("src"));
        let content = generate_init_content(&tmp);
        assert!(
            content.contains("my-app"),
            "Should detect project name from package.json"
        );
        assert!(content.contains("Node"), "Should detect Node project type");
        assert!(content.contains("npm"), "Should include npm commands");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── /diff stat parsing tests ────────────────────────────────────────

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

    // ── bookmark (/mark, /jump, /marks) tests ────────────────────────────

    #[test]
    fn test_mark_command_recognized() {
        assert!(!is_unknown_command("/mark"));
        assert!(!is_unknown_command("/mark checkpoint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/mark"),
            "/mark should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_jump_command_recognized() {
        assert!(!is_unknown_command("/jump"));
        assert!(!is_unknown_command("/jump checkpoint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/jump"),
            "/jump should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_marks_command_recognized() {
        assert!(!is_unknown_command("/marks"));
        assert!(
            KNOWN_COMMANDS.contains(&"/marks"),
            "/marks should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_parse_bookmark_name_with_name() {
        let name = parse_bookmark_name("/mark checkpoint", "/mark");
        assert_eq!(name, Some("checkpoint".to_string()));
    }

    #[test]
    fn test_parse_bookmark_name_with_spaces() {
        let name = parse_bookmark_name("/mark  my bookmark  ", "/mark");
        assert_eq!(name, Some("my bookmark".to_string()));
    }

    #[test]
    fn test_parse_bookmark_name_empty() {
        let name = parse_bookmark_name("/mark", "/mark");
        assert_eq!(name, None);
    }

    #[test]
    fn test_parse_bookmark_name_whitespace_only() {
        let name = parse_bookmark_name("/mark   ", "/mark");
        assert_eq!(name, None);
    }

    #[test]
    fn test_parse_bookmark_name_for_jump() {
        let name = parse_bookmark_name("/jump start", "/jump");
        assert_eq!(name, Some("start".to_string()));
    }

    #[test]
    fn test_bookmarks_create_and_list() {
        let mut bookmarks = Bookmarks::new();
        assert!(bookmarks.is_empty());

        bookmarks.insert("start".to_string(), "[]".to_string());
        assert_eq!(bookmarks.len(), 1);
        assert!(bookmarks.contains_key("start"));
    }

    #[test]
    fn test_bookmarks_overwrite_same_name() {
        let mut bookmarks = Bookmarks::new();
        bookmarks.insert("checkpoint".to_string(), "[1]".to_string());
        bookmarks.insert("checkpoint".to_string(), "[1,2]".to_string());
        // Should still have just one entry
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks.get("checkpoint").unwrap(), "[1,2]");
    }

    #[test]
    fn test_bookmarks_nonexistent_returns_none() {
        let bookmarks = Bookmarks::new();
        assert!(!bookmarks.contains_key("nonexistent"));
    }

    #[test]
    fn test_bookmarks_multiple_entries() {
        let mut bookmarks = Bookmarks::new();
        bookmarks.insert("start".to_string(), "[]".to_string());
        bookmarks.insert("middle".to_string(), "[1]".to_string());
        bookmarks.insert("end".to_string(), "[1,2,3]".to_string());
        assert_eq!(bookmarks.len(), 3);
        assert!(bookmarks.contains_key("start"));
        assert!(bookmarks.contains_key("middle"));
        assert!(bookmarks.contains_key("end"));
    }

    #[test]
    fn test_handle_marks_empty_does_not_panic() {
        let bookmarks = Bookmarks::new();
        // Should not panic — just prints a message
        handle_marks(&bookmarks);
    }

    #[test]
    fn test_handle_marks_with_entries_does_not_panic() {
        let mut bookmarks = Bookmarks::new();
        bookmarks.insert("alpha".to_string(), "[]".to_string());
        bookmarks.insert("beta".to_string(), "[]".to_string());
        // Should not panic
        handle_marks(&bookmarks);
    }

    #[test]
    fn test_mark_command_matching() {
        // /mark should match exact or with space, not /marker
        let mark_matches = |s: &str| s == "/mark" || s.starts_with("/mark ");
        assert!(mark_matches("/mark"));
        assert!(mark_matches("/mark checkpoint"));
        assert!(!mark_matches("/marker"));
        assert!(!mark_matches("/marking"));
    }

    #[test]
    fn test_jump_command_matching() {
        // /jump should match exact or with space, not /jumper
        let jump_matches = |s: &str| s == "/jump" || s.starts_with("/jump ");
        assert!(jump_matches("/jump"));
        assert!(jump_matches("/jump checkpoint"));
        assert!(!jump_matches("/jumper"));
        assert!(!jump_matches("/jumping"));
    }
}
