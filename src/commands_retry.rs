//! `/retry` and `/changes` REPL command handlers.
//!
//! Extracted from `commands.rs` as another slice of issue #260, which tracks
//! splitting the multi-thousand-line `commands.rs` into focused modules.
//! These two handlers are self-contained and only touch session state through
//! well-defined helpers (`build_retry_prompt`, `run_prompt`,
//! `auto_compact_if_needed`, `format_changes`), which makes them a safe,
//! mechanical slice to pull out.

use crate::commands_session::auto_compact_if_needed;
use crate::format::*;
use crate::git::{colorize_diff, run_git};
use crate::prompt::run_prompt;
use crate::prompt_retry::build_retry_prompt;
use crate::session::{format_changes, ChangeKind, FileChange, SessionChanges};
use crate::AgentConfig;

use std::time::Instant;
use yoagent::agent::Agent;
use yoagent::*;

/// Known tool names that can appear in error messages from failed tool executions.
/// Order matters: longer names first to avoid partial matches (e.g., "edit_file"
/// before "list_files" doesn't matter here, but "read_file" before "write_file"
/// avoids false positives if error text mentions both).
const KNOWN_TOOL_NAMES: &[&str] = &[
    "rename_symbol",
    "shared_state",
    "write_file",
    "read_file",
    "edit_file",
    "list_files",
    "sub_agent",
    "ask_user",
    "search",
    "bash",
    "todo",
];

/// Extract a tool name from an error message string.
///
/// Searches for known tool names that appear as whole words (bounded by
/// non-alphanumeric/underscore characters or string edges). Returns the first
/// match found. This is used by `/retry` to provide tool-specific recovery
/// hints when the tool name wasn't preserved through the error propagation path.
///
/// Returns `None` if no recognizable tool name is found.
pub fn extract_tool_name_from_error(error: &str) -> Option<&'static str> {
    if error.is_empty() {
        return None;
    }
    for &tool in KNOWN_TOOL_NAMES {
        if let Some(pos) = error.find(tool) {
            // Check word boundary before
            let before_ok = pos == 0
                || error
                    .as_bytes()
                    .get(pos - 1)
                    .is_none_or(|&b| !b.is_ascii_alphanumeric() && b != b'_');
            // Check word boundary after
            let end = pos + tool.len();
            let after_ok = end >= error.len()
                || error
                    .as_bytes()
                    .get(end)
                    .is_none_or(|&b| !b.is_ascii_alphanumeric() && b != b'_');
            if before_ok && after_ok {
                return Some(tool);
            }
        }
    }
    None
}

/// Parse the `--with` modifier from a `/retry` command input.
///
/// Accepts the full input string (e.g. `"/retry --with use async/await"`).
/// Returns `Some(modifier)` if `--with` is present with a non-empty value,
/// `None` otherwise.
pub fn parse_with_modifier(input: &str) -> Option<String> {
    let arg = input.strip_prefix("/retry").unwrap_or("").trim();
    if let Some(rest) = arg.strip_prefix("--with") {
        let modifier = rest.trim();
        // Strip surrounding quotes if present
        let modifier = modifier
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                modifier
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(modifier);
        if modifier.is_empty() {
            None
        } else {
            Some(modifier.to_string())
        }
    } else {
        None
    }
}

pub async fn handle_retry(
    agent: &mut Agent,
    input: &str,
    last_input: &Option<String>,
    last_error: &Option<String>,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    match last_input {
        Some(prev) => {
            // Parse optional --with modifier for iterative refinement
            let with_modifier = parse_with_modifier(input);

            // Try to extract the tool name from the error text for targeted recovery hints
            let tool_name = last_error.as_deref().and_then(extract_tool_name_from_error);
            let retry_input = build_retry_prompt(prev, last_error, tool_name);

            // Append the --with modifier if present
            let retry_input = if let Some(ref modifier) = with_modifier {
                format!("{retry_input}\n\nAdditional instruction: {modifier}")
            } else {
                retry_input
            };

            if let Some(ref modifier) = with_modifier {
                println!("{DIM}  (retrying with modifier: {modifier}){RESET}");
            } else if last_error.is_some() {
                if let Some(name) = tool_name {
                    println!("{DIM}  (retrying with {name} error context + recovery hint){RESET}");
                } else {
                    println!("{DIM}  (retrying with error context){RESET}");
                }
            } else {
                println!("{DIM}  (retrying last input){RESET}");
            }
            let outcome = run_prompt(agent, &retry_input, session_total, model).await;
            auto_compact_if_needed(agent);
            outcome.last_tool_error
        }
        None => {
            eprintln!("{DIM}  (nothing to retry — no previous input){RESET}\n");
            None
        }
    }
}

/// Returns a compact 2–3 line session summary for display on REPL exit, or
/// `None` if neither files were modified nor tokens were used (i.e., no real
/// interaction happened).
///
/// Example output:
/// ```text
///   ─── session summary ────────────────────────────────
///   Duration: 4m 32s · Tokens: 12.5K in / 3.2K out · Cost: ~$0.05
///   Files changed: 3 (src/main.rs, src/lib.rs +new, tests/test.rs)
/// ```
pub fn format_exit_summary(
    changes: &SessionChanges,
    session_total: &Usage,
    model: &str,
    session_start: Instant,
) -> Option<String> {
    let snapshot = changes.snapshot();
    let has_files = !snapshot.is_empty();
    let has_tokens = session_total.input > 0 || session_total.output > 0;

    if !has_files && !has_tokens {
        return None;
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "{DIM}  ─── session summary ────────────────────────────────{RESET}"
    ));

    // Stats line: duration · tokens · cost (all on one line)
    let elapsed = session_start.elapsed();
    let mut stats = vec![format!("Duration: {}", format_duration(elapsed))];

    if has_tokens {
        stats.push(format!(
            "Tokens: {} in / {} out",
            format_token_count(session_total.input),
            format_token_count(session_total.output),
        ));
    }

    if let Some(cost) = estimate_cost(session_total, model) {
        stats.push(format!("Cost: ~{}", format_cost(cost)));
    }

    lines.push(format!("{DIM}  {}{RESET}", stats.join(" · ")));

    // Files line with names
    if has_files {
        lines.push(format!(
            "{DIM}  Files changed: {} ({}){RESET}",
            snapshot.len(),
            format_file_list(&snapshot),
        ));

        // Show a compact colored diff if we're not in quiet mode
        if !is_quiet() {
            let paths: Vec<String> = snapshot.iter().map(|fc| fc.path.clone()).collect();
            let diffs = collect_diffs(&paths);
            let trimmed = diffs.trim();
            if !trimmed.is_empty() {
                let truncated = truncate_diff_lines(trimmed, 15);
                lines.push(String::new()); // blank separator
                lines.push(truncated);
            }
        }
    }

    Some(lines.join("\n"))
}

/// Format a list of changed files for the exit summary. Files that were
/// newly written (not edited) get a `+new` suffix. If the combined list
/// would exceed 60 chars, it is truncated with `…`.
fn format_file_list(snapshot: &[FileChange]) -> String {
    let mut names: Vec<String> = snapshot
        .iter()
        .map(|fc| {
            let name = fc.path.as_str();
            if fc.kind == ChangeKind::Write {
                format!("{name} +new")
            } else {
                name.to_string()
            }
        })
        .collect();
    names.sort();
    let joined = names.join(", ");
    if joined.len() <= 60 {
        joined
    } else {
        safe_truncate_with_suffix(&joined, 57, "…")
    }
}

/// Truncate a diff string to at most `max_lines` lines. If the original has
/// more lines, appends a dim hint with the overflow count.
fn truncate_diff_lines(diff: &str, max_lines: usize) -> String {
    if diff.is_empty() {
        return String::new();
    }
    let all_lines: Vec<&str> = diff.lines().collect();
    let total = all_lines.len();
    if total <= max_lines {
        return diff.to_string();
    }
    let kept: Vec<&str> = all_lines[..max_lines].to_vec();
    let overflow = total - max_lines;
    format!(
        "{}\n{DIM}  … and {overflow} more lines (use /changes --diff to see all){RESET}",
        kept.join("\n"),
    )
}

/// Returns `true` if the raw `/changes` input contains the `--diff` flag.
fn wants_diff(input: &str) -> bool {
    input
        .split_whitespace()
        .skip(1) // skip "/changes" itself
        .any(|arg| arg == "--diff")
}

/// Returns `true` if the raw `/changes` input contains the `summary` subcommand.
pub(crate) fn wants_summary(input: &str) -> bool {
    input
        .split_whitespace()
        .skip(1) // skip "/changes" itself
        .any(|arg| arg == "summary")
}

/// Collect colorized git diffs for the given file paths.
///
/// For each file we try both unstaged (`git diff`) and staged
/// (`git diff --cached`) so we catch changes regardless of staging state.
fn collect_diffs(paths: &[String]) -> String {
    let mut out = String::new();
    for path in paths {
        // Try unstaged diff first, then staged
        let unstaged = run_git(&["diff", "--", path]).unwrap_or_default();
        let staged = run_git(&["diff", "--cached", "--", path]).unwrap_or_default();

        let combined = match (unstaged.is_empty(), staged.is_empty()) {
            (false, false) => format!("{unstaged}\n{staged}"),
            (false, true) => unstaged,
            (true, false) => staged,
            (true, true) => String::new(),
        };

        if combined.is_empty() {
            out.push_str(&format!("    {DIM}({path}: no diff available){RESET}\n"));
        } else {
            out.push_str(&colorize_diff(&combined));
            out.push('\n');
        }
    }
    out
}

/// Handle `/changes summary` — generate an AI-written summary of session changes.
///
/// Collects diffs for every file modified this session, sends them to a
/// side agent, and streams the natural-language summary to stdout.
pub(crate) async fn handle_changes_summary(changes: &SessionChanges, agent_config: &AgentConfig) {
    let snapshot = changes.snapshot();
    if snapshot.is_empty() {
        eprintln!("{DIM}  No files modified yet this session — nothing to summarize.{RESET}\n");
        return;
    }

    // Collect diffs for all changed files
    let paths: Vec<String> = snapshot.iter().map(|c| c.path.clone()).collect();
    let mut diff_sections = String::new();
    for path in &paths {
        let unstaged = run_git(&["diff", "--", path]).unwrap_or_default();
        let staged = run_git(&["diff", "--cached", "--", path]).unwrap_or_default();
        let combined = match (unstaged.is_empty(), staged.is_empty()) {
            (false, false) => format!("{unstaged}\n{staged}"),
            (false, true) => unstaged,
            (true, false) => staged,
            (true, true) => String::new(),
        };

        diff_sections.push_str(&format!("### {path}\n"));
        if combined.is_empty() {
            diff_sections.push_str("(new file or no diff available)\n\n");
        } else {
            diff_sections.push_str("```diff\n");
            diff_sections.push_str(&combined);
            diff_sections.push_str("\n```\n\n");
        }
    }

    let prompt = format!(
        "Here are the file changes made during this coding session.\n\
         Write a concise summary suitable for a PR description or commit message.\n\
         Group related changes together. Be specific about what changed and why \
         (infer the purpose from the diff content).\n\
         Use markdown with bullet points. Start with a one-line overall summary, \
         then list each logical change.\n\n\
         ## Changed files\n\n{diff_sections}"
    );

    eprintln!("{DIM}  [summary] generating...{RESET}");

    let mut side_agent = agent_config.build_side_agent();
    let mut rx = side_agent.prompt(&prompt).await;

    let mut md_renderer = MarkdownRenderer::new();
    let mut started = false;

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                if !started {
                    print!("\n{DIM}[summary]{RESET} ");
                    started = true;
                }
                let rendered = md_renderer.render_delta(&delta);
                if !rendered.is_empty() {
                    print!("{rendered}");
                }
            }
            Some(AgentEvent::MessageEnd { .. }) => {
                let tail = md_renderer.flush();
                if !tail.is_empty() {
                    print!("{tail}");
                }
            }
            Some(AgentEvent::AgentEnd { .. }) => break,
            None => break,
            _ => {}
        }
    }

    side_agent.finish().await;

    if !started {
        eprintln!("{DIM}  (no summary generated){RESET}");
    }
    println!();
}

pub fn handle_changes(changes: &SessionChanges, input: &str) {
    let output = format_changes(changes);
    if output.is_empty() {
        println!("{DIM}  No files modified yet this session.");
        println!(
            "  Files touched by write_file or edit_file tool calls will appear here.{RESET}\n"
        );
        return;
    }

    println!("{DIM}{output}{RESET}");

    if wants_diff(input) {
        let snapshot = changes.snapshot();
        let paths: Vec<String> = snapshot.iter().map(|c| c.path.clone()).collect();
        let diffs = collect_diffs(&paths);
        if !diffs.is_empty() {
            println!("{diffs}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a Usage with given input/output token counts.
    fn make_usage(input: u64, output: u64) -> Usage {
        Usage {
            input,
            output,
            ..Usage::default()
        }
    }

    // ── parse_with_modifier tests ──────────────────────────────────────

    #[test]
    fn test_parse_with_modifier_plain_retry() {
        assert_eq!(parse_with_modifier("/retry"), None);
    }

    #[test]
    fn test_parse_with_modifier_unquoted() {
        assert_eq!(
            parse_with_modifier("/retry --with use async"),
            Some("use async".to_string())
        );
    }

    #[test]
    fn test_parse_with_modifier_double_quoted() {
        assert_eq!(
            parse_with_modifier("/retry --with \"quoted text\""),
            Some("quoted text".to_string())
        );
    }

    #[test]
    fn test_parse_with_modifier_single_quoted() {
        assert_eq!(
            parse_with_modifier("/retry --with 'single quoted'"),
            Some("single quoted".to_string())
        );
    }

    #[test]
    fn test_parse_with_modifier_empty_value() {
        assert_eq!(parse_with_modifier("/retry --with"), None);
        assert_eq!(parse_with_modifier("/retry --with  "), None);
    }

    #[test]
    fn test_parse_with_modifier_empty_quoted() {
        assert_eq!(parse_with_modifier("/retry --with \"\""), None);
        assert_eq!(parse_with_modifier("/retry --with ''"), None);
    }

    #[test]
    fn test_parse_with_modifier_preserves_inner_quotes() {
        // If only outer quotes are stripped, inner content stays intact
        assert_eq!(
            parse_with_modifier("/retry --with \"don't break\""),
            Some("don't break".to_string())
        );
    }

    #[test]
    fn test_parse_with_modifier_no_flag() {
        // Other flags should not match
        assert_eq!(parse_with_modifier("/retry --verbose"), None);
    }

    #[test]
    fn test_handle_changes_empty_does_not_panic() {
        let changes = SessionChanges::new();
        // Should not panic -- just prints a message
        handle_changes(&changes, "/changes");
    }

    #[test]
    fn test_handle_changes_with_entries_does_not_panic() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        // Should not panic
        handle_changes(&changes, "/changes");
    }

    #[test]
    fn test_handle_changes_diff_flag_does_not_panic() {
        let changes = SessionChanges::new();
        // Empty session with --diff should not panic
        handle_changes(&changes, "/changes --diff");
    }

    #[test]
    fn test_handle_changes_diff_flag_with_entries_does_not_panic() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        // With files and --diff -- may not produce real diffs in test env, but shouldn't panic
        handle_changes(&changes, "/changes --diff");
    }

    #[test]
    fn test_wants_diff_flag_parsing() {
        assert!(!wants_diff("/changes"));
        assert!(wants_diff("/changes --diff"));
        assert!(wants_diff("/changes   --diff"));
        assert!(!wants_diff("/changes --dif"));
        assert!(!wants_diff("/changes --verbose"));
    }

    #[test]
    fn test_wants_summary_parsing() {
        assert!(!wants_summary("/changes"));
        assert!(!wants_summary("/changes --diff"));
        assert!(wants_summary("/changes summary"));
        assert!(wants_summary("/changes   summary"));
        // summary is a subcommand, not a flag — "summari" shouldn't match
        assert!(!wants_summary("/changes summari"));
        assert!(!wants_summary("/changes --summary"));
    }

    #[test]
    fn test_format_exit_summary_empty_returns_none() {
        let changes = SessionChanges::new();
        let usage = Usage::default();
        assert!(format_exit_summary(&changes, &usage, "unknown-model", Instant::now()).is_none());
    }

    #[test]
    fn test_format_exit_summary_single_write() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        let usage = make_usage(1000, 200);
        let summary =
            format_exit_summary(&changes, &usage, "unknown-model", Instant::now()).unwrap();
        assert!(summary.contains("session summary"), "header missing");
        assert!(summary.contains("Duration:"), "duration missing");
        assert!(summary.contains("Tokens:"), "tokens missing");
        // File name should appear with +new marker
        assert!(summary.contains("Files changed: 1"), "file count missing");
        assert!(summary.contains("src/main.rs +new"), "+new marker missing");
    }

    #[test]
    fn test_format_exit_summary_single_edit() {
        let changes = SessionChanges::new();
        changes.record("src/cli.rs", ChangeKind::Edit);
        let usage = make_usage(500, 100);
        let summary =
            format_exit_summary(&changes, &usage, "unknown-model", Instant::now()).unwrap();
        assert!(summary.contains("Files changed: 1"), "file count missing");
        // Edited files should appear without +new
        assert!(summary.contains("src/cli.rs"), "file name missing");
        assert!(
            !summary.contains("src/cli.rs +new"),
            "edit should not have +new"
        );
    }

    #[test]
    fn test_format_exit_summary_mixed() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        changes.record("src/tools.rs", ChangeKind::Edit);
        let usage = make_usage(5000, 1500);
        let summary =
            format_exit_summary(&changes, &usage, "unknown-model", Instant::now()).unwrap();
        assert!(summary.contains("Files changed: 3"), "file count missing");
        assert!(
            summary.contains("src/main.rs +new"),
            "written file missing +new"
        );
        assert!(summary.contains("src/cli.rs"), "edited file missing");
        assert!(
            !summary.contains("src/cli.rs +new"),
            "edited file should not have +new"
        );
        assert!(summary.contains("src/tools.rs"), "edited file missing");
    }

    #[test]
    fn test_format_exit_summary_all_writes() {
        let changes = SessionChanges::new();
        changes.record("a.rs", ChangeKind::Write);
        changes.record("b.rs", ChangeKind::Write);
        let usage = make_usage(100, 50);
        let summary =
            format_exit_summary(&changes, &usage, "unknown-model", Instant::now()).unwrap();
        assert!(summary.contains("Files changed: 2"), "file count missing");
        assert!(summary.contains("a.rs +new"), "first file missing +new");
        assert!(summary.contains("b.rs +new"), "second file missing +new");
    }

    #[test]
    fn test_exit_summary_with_tokens_no_files() {
        // Pure Q&A session: tokens used but no file changes -- should still
        // produce a summary showing duration/tokens/cost.
        let changes = SessionChanges::new();
        let usage = make_usage(12_450, 3_200);
        let summary =
            format_exit_summary(&changes, &usage, "claude-sonnet-4-20250514", Instant::now())
                .unwrap();
        assert!(summary.contains("session summary"), "header missing");
        assert!(summary.contains("Duration:"), "duration missing");
        assert!(summary.contains("Tokens:"), "tokens missing");
        // Should NOT contain a Files line
        assert!(
            !summary.contains("Files changed:"),
            "should have no files line"
        );
        // Known model should produce a cost segment
        assert!(summary.contains("Cost:"), "cost missing for known model");
    }

    #[test]
    fn test_exit_summary_with_files_and_cost() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        let usage = make_usage(50_000, 10_000);
        let summary =
            format_exit_summary(&changes, &usage, "claude-sonnet-4-20250514", Instant::now())
                .unwrap();
        assert!(summary.contains("session summary"), "header missing");
        assert!(summary.contains("Duration:"), "duration missing");
        assert!(summary.contains("Tokens:"), "tokens missing");
        assert!(summary.contains("Cost:"), "cost missing");
        assert!(summary.contains("Files changed: 2"), "file count missing");
        assert!(summary.contains("src/main.rs +new"), "written file missing");
        assert!(summary.contains("src/cli.rs"), "edited file missing");
    }

    #[test]
    fn test_exit_summary_unknown_model_omits_cost() {
        let changes = SessionChanges::new();
        let usage = make_usage(1000, 500);
        let summary =
            format_exit_summary(&changes, &usage, "totally-unknown-model", Instant::now()).unwrap();
        assert!(summary.contains("Tokens:"));
        // Unknown model has no pricing -- cost should be absent
        assert!(!summary.contains("Cost:"));
    }

    #[test]
    fn test_exit_summary_compact_format() {
        // Verify the summary is compact: duration, tokens, and cost on one line
        // separated by " · "
        let changes = SessionChanges::new();
        let usage = make_usage(1000, 200);
        let summary =
            format_exit_summary(&changes, &usage, "claude-sonnet-4-20250514", Instant::now())
                .unwrap();
        // Duration and Tokens should be on the same line, joined by " · "
        let plain = strip_ansi(&summary);
        let stats_line = plain.lines().find(|l| l.contains("Duration:")).unwrap();
        assert!(
            stats_line.contains("Tokens:"),
            "Duration and Tokens should be on the same line, got: {stats_line}"
        );
    }

    #[test]
    fn test_format_file_list_sorted() {
        // File list should be sorted alphabetically
        let snapshot = vec![
            FileChange {
                path: "z.rs".to_string(),
                kind: ChangeKind::Edit,
            },
            FileChange {
                path: "a.rs".to_string(),
                kind: ChangeKind::Write,
            },
            FileChange {
                path: "m.rs".to_string(),
                kind: ChangeKind::Edit,
            },
        ];
        let list = format_file_list(&snapshot);
        assert_eq!(list, "a.rs +new, m.rs, z.rs");
    }

    #[test]
    fn test_format_file_list_truncation() {
        // A very long file list should be truncated with an ellipsis
        let snapshot: Vec<FileChange> = (0..20)
            .map(|i| FileChange {
                path: format!("src/very_long_module_name_{i}.rs"),
                kind: ChangeKind::Edit,
            })
            .collect();
        let list = format_file_list(&snapshot);
        assert!(list.len() <= 61, "list too long: {} chars", list.len());
        assert!(
            list.contains('\u{2026}'),
            "should contain ellipsis character"
        );
    }

    /// Strip ANSI escape sequences for test assertions.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut in_escape = false;
        for ch in s.chars() {
            if ch == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if ch == 'm' {
                    in_escape = false;
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn test_changes_command_recognized() {
        use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
        assert!(!is_unknown_command("/changes"));
        assert!(
            KNOWN_COMMANDS.contains(&"/changes"),
            "/changes should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_changes_command_not_confused_with_other_commands() {
        use crate::commands::is_unknown_command;
        // /changes should match exactly, unrelated words should be unknown
        assert!(is_unknown_command("/changed"));
        // /changelog is now a valid command (Issue #226)
        assert!(!is_unknown_command("/changelog"));
    }

    // ---------------------------------------------------------------
    // extract_tool_name_from_error tests
    // ---------------------------------------------------------------

    #[test]
    fn test_extract_tool_name_edit_file() {
        assert_eq!(
            extract_tool_name_from_error("edit_file: old_text not found in file"),
            Some("edit_file")
        );
    }

    #[test]
    fn test_extract_tool_name_bash() {
        assert_eq!(
            extract_tool_name_from_error("bash: command not found: foobar"),
            Some("bash")
        );
    }

    #[test]
    fn test_extract_tool_name_read_file() {
        assert_eq!(
            extract_tool_name_from_error("read_file failed: no such file or directory"),
            Some("read_file")
        );
    }

    #[test]
    fn test_extract_tool_name_write_file() {
        assert_eq!(
            extract_tool_name_from_error("write_file: permission denied"),
            Some("write_file")
        );
    }

    #[test]
    fn test_extract_tool_name_search() {
        assert_eq!(
            extract_tool_name_from_error("search returned no results"),
            Some("search")
        );
    }

    #[test]
    fn test_extract_tool_name_rename_symbol() {
        assert_eq!(
            extract_tool_name_from_error("rename_symbol: symbol not found"),
            Some("rename_symbol")
        );
    }

    #[test]
    fn test_extract_tool_name_in_quoted_context() {
        // Tool name might appear in quotes like "Tool 'edit_file' failed"
        assert_eq!(
            extract_tool_name_from_error("Tool 'edit_file' failed with error"),
            Some("edit_file")
        );
    }

    #[test]
    fn test_extract_tool_name_none_for_unknown() {
        assert_eq!(
            extract_tool_name_from_error("something went wrong with the compilation"),
            None
        );
    }

    #[test]
    fn test_extract_tool_name_empty_input() {
        assert_eq!(extract_tool_name_from_error(""), None);
    }

    #[test]
    fn test_extract_tool_name_word_boundary() {
        // "searching" contains "search" but shouldn't match (no word boundary after)
        assert_eq!(
            extract_tool_name_from_error("still searching for the answer"),
            None
        );
    }

    #[test]
    fn test_extract_tool_name_prefers_first_match() {
        // If error mentions multiple tools, returns the first recognized one
        // (by KNOWN_TOOL_NAMES order, which is longest-first)
        let result = extract_tool_name_from_error("tried edit_file then bash, both failed");
        assert!(
            result == Some("edit_file") || result == Some("bash"),
            "should match one of the mentioned tools: {result:?}"
        );
    }

    // ---------------------------------------------------------------
    // build_retry_prompt with tool name tests
    // ---------------------------------------------------------------

    #[test]
    fn test_retry_prompt_with_tool_name() {
        use crate::prompt_retry::build_retry_prompt;
        let err = Some("old_text not found in file".to_string());
        let result = build_retry_prompt("fix the bug", &err, Some("edit_file"));
        assert!(
            result.contains("edit_file error:"),
            "should mention tool name: {result}"
        );
        assert!(
            result.contains("old_text not found"),
            "should include error: {result}"
        );
        // Should include some recovery guidance (resilient to hint text changes)
        let has_recovery_hint = result.contains("read_file")
            || result.contains("current contents")
            || result.contains("verify")
            || result.contains("mismatch")
            || result.contains("retry");
        assert!(
            has_recovery_hint,
            "should include recovery guidance for edit_file: {result}"
        );
        assert!(
            result.contains("fix the bug"),
            "should include original input: {result}"
        );
    }

    #[test]
    fn test_retry_prompt_with_unknown_tool_name() {
        use crate::prompt_retry::build_retry_prompt;
        let err = Some("something failed".to_string());
        let result = build_retry_prompt("do stuff", &err, Some("unknown_tool"));
        // Should still work with a generic hint
        assert!(
            result.contains("unknown_tool error:"),
            "should mention tool name: {result}"
        );
        assert!(
            result.contains("something failed"),
            "should include error: {result}"
        );
        assert!(
            result.contains("do stuff"),
            "should include original input: {result}"
        );
    }

    #[test]
    fn test_retry_prompt_no_tool_no_error() {
        use crate::prompt_retry::build_retry_prompt;
        let result = build_retry_prompt("hello", &None, Some("bash"));
        // No error means no enrichment, even if tool name provided
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_diff_lines_empty() {
        let result = truncate_diff_lines("", 15);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_diff_lines_shorter_than_max() {
        let input = "line 1\nline 2\nline 3";
        let result = truncate_diff_lines(input, 15);
        assert_eq!(result, input);
    }

    #[test]
    fn test_truncate_diff_lines_exact_max() {
        let input = (1..=5)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_diff_lines(&input, 5);
        assert_eq!(result, input);
    }

    #[test]
    fn test_truncate_diff_lines_over_max() {
        let input = (1..=20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_diff_lines(&input, 5);
        // Should contain first 5 lines
        assert!(result.contains("line 1"));
        assert!(result.contains("line 5"));
        // Should NOT contain line 6+
        assert!(!result.contains("line 6\n"));
        // Should have overflow note
        let plain = strip_ansi(&result);
        assert!(
            plain.contains("15 more lines"),
            "overflow hint missing: {plain}"
        );
        assert!(plain.contains("/changes --diff"), "hint missing: {plain}");
    }

    #[test]
    fn test_truncate_diff_lines_preserves_ansi() {
        let input = format!(
            "{GREEN}+added line{RESET}\n{RED}-removed line{RESET}\nplain line 1\nplain line 2"
        );
        let result = truncate_diff_lines(&input, 2);
        // Should keep the ANSI codes in the kept lines
        assert!(result.contains(GREEN.0));
        let plain = strip_ansi(&result);
        assert!(plain.contains("2 more lines"));
    }
}
