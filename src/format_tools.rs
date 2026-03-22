//! Tool output formatting, section headers, spinners, and progress display.
//!
//! Handles formatting of tool execution summaries, diffs, turn boundaries,
//! section headers/dividers, spinners for thinking animation, and live
//! tool progress display.

use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use yoagent::types::{Content, ToolResult};

use crate::format::{
    estimate_cost, format_cost, format_duration, pluralize, truncate_with_ellipsis, BOLD_CYAN,
    CYAN, DIM, GREEN, RED, RESET,
};

// --- Section headers and dividers for visual hierarchy ---

/// Get the terminal width from the COLUMNS environment variable, falling back to 80.
fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(80)
}

/// Format a summary line for a batch of tool executions within a single turn.
///
/// Example output: `  3 tools completed in 1.2s (3 ✓, 0 ✗)`
/// When all succeed: `  3 tools completed in 1.2s (3 ✓)`
/// When some fail: `  3 tools completed in 1.2s (2 ✓, 1 ✗)`
/// Single tool batches return empty (not worth summarizing).
pub fn format_tool_batch_summary(
    total: usize,
    succeeded: usize,
    failed: usize,
    total_duration: std::time::Duration,
) -> String {
    if total <= 1 {
        return String::new();
    }
    let dur = format_duration(total_duration);
    let tool_word = pluralize(total, "tool", "tools");
    let status = if failed == 0 {
        format!("{succeeded} {GREEN}✓{RESET}")
    } else {
        format!("{succeeded} {GREEN}✓{RESET}, {failed} {RED}✗{RESET}")
    };
    format!("{DIM}  {total} {tool_word} completed in {dur}{RESET} ({status})")
}

/// Indent multi-line tool output under its tool header.
///
/// Each line of output gets a `    │ ` prefix for visual nesting.
/// Single-line output is returned as-is with the prefix.
/// Empty input returns empty string.
pub fn indent_tool_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }
    output
        .lines()
        .map(|line| format!("{DIM}    │ {RESET}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a turn boundary marker between agent turns.
///
/// Shows a subtle visual separator so users can distinguish
/// when the agent starts a new reasoning/action cycle.
/// Example: `  ╭─ Turn 3 ──────────────────────────╮`
pub fn turn_boundary(turn_number: usize) -> String {
    let width = terminal_width();
    let label = format!(" Turn {turn_number} ");
    let prefix = "  ╭─";
    let suffix = "╮";
    let used = prefix.len() + label.len() + suffix.len();
    let fill = width.saturating_sub(used);
    let trail = "─".repeat(fill);
    format!("{DIM}{prefix}{label}{trail}{suffix}{RESET}")
}

/// Render a labeled section header, e.g. `── Thinking ──────────────────────────`
/// Uses DIM style and thin box-drawing characters (─).
/// The label is centered between two runs of ─ characters.
pub fn section_header(label: &str) -> String {
    let width = terminal_width();
    if label.is_empty() {
        return section_divider();
    }
    // Format: "── Label ─────────..."
    let prefix = "── ";
    let separator = " ";
    let used = prefix.len() + label.len() + separator.len();
    let remaining = width.saturating_sub(used);
    let trail = "─".repeat(remaining);
    format!("{DIM}{prefix}{label}{separator}{trail}{RESET}")
}

/// Render a plain thin divider line: `──────────────────────────────────────`
/// Uses DIM style and thin box-drawing characters (─).
pub fn section_divider() -> String {
    let width = terminal_width();
    format!("{DIM}{}{RESET}", "─".repeat(width))
}

/// Truncate a string to `max` characters (no ellipsis).
#[cfg(test)]
pub fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Maximum number of diff lines to display before truncating.
const MAX_DIFF_LINES: usize = 20;

/// Format a colored unified diff between old_text and new_text.
/// Removed lines are shown in red with `- ` prefix, added lines in green with `+ ` prefix.
/// If the diff exceeds `MAX_DIFF_LINES`, it is truncated with an ellipsis note.
pub fn format_edit_diff(old_text: &str, new_text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Show removed lines (old_text)
    if !old_text.is_empty() {
        for line in old_text.lines() {
            lines.push(format!("{RED}  - {line}{RESET}"));
        }
    }

    // Show added lines (new_text)
    if !new_text.is_empty() {
        for line in new_text.lines() {
            lines.push(format!("{GREEN}  + {line}{RESET}"));
        }
    }

    if lines.is_empty() {
        return String::new();
    }

    // Truncate if too many lines
    if lines.len() > MAX_DIFF_LINES {
        let remaining = lines.len() - MAX_DIFF_LINES;
        lines.truncate(MAX_DIFF_LINES);
        lines.push(format!("{DIM}  ... ({remaining} more lines){RESET}"));
    }

    lines.join("\n")
}

/// Format a human-readable summary for a tool execution.
///
/// Each tool gets a concise one-line description showing the key parameters:
/// - `bash` — `$ <command>` (first line + line count for multi-line scripts)
/// - `read_file` — `read <path>` with optional `:offset..end` or `(N lines)` range
/// - `write_file` — `write <path> (N lines)`
/// - `edit_file` — `edit <path> (old → new lines)`
/// - `list_files` — `ls <path> (pattern)`
/// - `search` — `search 'pattern' in <path> (include)`
pub fn format_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            let line_count = cmd.lines().count();
            let first_line = cmd.lines().next().unwrap_or("...");
            if line_count > 1 {
                format!(
                    "$ {} ({line_count} lines)",
                    truncate_with_ellipsis(first_line, 60)
                )
            } else {
                format!("$ {}", truncate_with_ellipsis(cmd, 80))
            }
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let offset = args.get("offset").and_then(|v| v.as_u64());
            let limit = args.get("limit").and_then(|v| v.as_u64());
            match (offset, limit) {
                (Some(off), Some(lim)) => {
                    format!("read {path}:{off}..{}", off + lim)
                }
                (Some(off), None) => {
                    format!("read {path}:{off}..")
                }
                (None, Some(lim)) => {
                    let word = pluralize(lim as usize, "line", "lines");
                    format!("read {path} ({lim} {word})")
                }
                (None, None) => {
                    format!("read {path}")
                }
            }
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let line_info = args
                .get("content")
                .and_then(|v| v.as_str())
                .map(|c| {
                    let count = c.lines().count();
                    let word = pluralize(count, "line", "lines");
                    format!(" ({count} {word})")
                })
                .unwrap_or_default();
            format!("write {path}{line_info}")
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let old_text = args.get("old_text").and_then(|v| v.as_str());
            let new_text = args.get("new_text").and_then(|v| v.as_str());
            match (old_text, new_text) {
                (Some(old), Some(new)) => {
                    let old_lines = old.lines().count();
                    let new_lines = new.lines().count();
                    format!("edit {path} ({old_lines} → {new_lines} lines)")
                }
                _ => format!("edit {path}"),
            }
        }
        "list_files" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let pattern = args.get("pattern").and_then(|v| v.as_str());
            match pattern {
                Some(pat) => format!("ls {path} ({pat})"),
                None => format!("ls {path}"),
            }
        }
        "search" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            let search_path = args.get("path").and_then(|v| v.as_str());
            let include = args.get("include").and_then(|v| v.as_str());
            let mut summary = format!("search '{}'", truncate_with_ellipsis(pat, 60));
            if let Some(p) = search_path {
                summary.push_str(&format!(" in {p}"));
            }
            if let Some(inc) = include {
                summary.push_str(&format!(" ({inc})"));
            }
            summary
        }
        _ => tool_name.to_string(),
    }
}

/// Print usage stats after a prompt response.
pub fn print_usage(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) {
    if usage.input > 0 || usage.output > 0 {
        let cache_info = if usage.cache_read > 0 || usage.cache_write > 0 {
            format!(
                "  [cache: {} read, {} write]",
                usage.cache_read, usage.cache_write
            )
        } else {
            String::new()
        };
        let cost_info = estimate_cost(usage, model)
            .map(|c| format!("  cost: {}", format_cost(c)))
            .unwrap_or_default();
        let total_cost_info = estimate_cost(total, model)
            .map(|c| format!("  total: {}", format_cost(c)))
            .unwrap_or_default();
        let elapsed_str = format_duration(elapsed);
        println!(
            "\n{DIM}  tokens: {} in / {} out{cache_info}  (session: {} in / {} out){cost_info}{total_cost_info}  ⏱ {elapsed_str}{RESET}",
            usage.input, usage.output, total.input, total.output
        );
    }
}


// --- Waiting spinner for AI responses ---

/// Braille spinner frames used for the "thinking" animation.
pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Get the spinner frame for a given tick index (wraps around).
pub fn spinner_frame(tick: usize) -> char {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

/// A handle to a running spinner task. Dropping or calling `stop()` cancels it.
pub struct Spinner {
    cancel: tokio::sync::watch::Sender<bool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Spinner {
    /// Start a spinner that prints frames to stderr every 100ms.
    /// The spinner shows `⠋ thinking...` cycling through braille characters.
    pub fn start() -> Self {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(async move {
            let mut tick: usize = 0;
            loop {
                // Check cancellation before printing
                if *cancel_rx.borrow() {
                    // Clear the spinner line
                    eprint!("\r\x1b[K");
                    break;
                }
                let frame = spinner_frame(tick);
                eprint!("\r{DIM}  {frame} thinking...{RESET}");
                tick = tick.wrapping_add(1);

                // Wait 100ms or until cancelled
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                    _ = cancel_rx.changed() => {
                        // Clear the spinner line
                        eprint!("\r\x1b[K");
                        break;
                    }
                }
            }
        });
        Self {
            cancel: cancel_tx,
            handle: Some(handle),
        }
    }

    /// Stop the spinner and clear its output.
    /// Clears the spinner line directly (don't rely on the async task to clear,
    /// since abort() can race with the clear sequence).
    ///
    /// render_latency_budget: This is the first-token cost (~0.1ms).
    /// The synchronous eprint + flush ensures the spinner line is cleared
    /// before any stdout text appears. The async handle abort is deferred
    /// to Drop to minimize latency on the critical path.
    pub fn stop(self) {
        let _ = self.cancel.send(true);
        // Clear the spinner line from the calling thread — this is synchronous
        // and guaranteed to complete before any subsequent stdout writes.
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
        // Defer handle.abort() to Drop — it interacts with the tokio runtime
        // and doesn't need to complete before the first text token is printed.
        // The cancel signal already ensures the spinner task won't write again.
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        // Clear the spinner line synchronously on drop too
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

// --- Live tool progress display ---

/// Format a live progress line for a running tool.
///
/// Shows spinner frame, tool name, elapsed time, and optional line count.
/// Example: `  ⠹ bash ⏱ 12s` or `  ⠹ bash ⏱ 1m 5s (142 lines)`
pub fn format_tool_progress(
    tool_name: &str,
    elapsed: Duration,
    tick: usize,
    line_count: Option<usize>,
) -> String {
    let frame = spinner_frame(tick);
    let time_str = format_duration_live(elapsed);
    let lines_str = match line_count {
        Some(n) if n > 0 => {
            let word = pluralize(n, "line", "lines");
            format!(" ({n} {word})")
        }
        _ => String::new(),
    };
    format!("{DIM}  {frame} {tool_name} ⏱ {time_str}{lines_str}{RESET}")
}

/// Format elapsed duration for live display (compact, human-friendly).
///
/// - Under 60s: `5s`
/// - 60s+: `1m 5s`
/// - 60m+: `1h 2m`
pub fn format_duration_live(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m {s}s")
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    }
}

/// Format the last N lines of partial output for live display.
///
/// Returns dimmed, indented lines showing the tail of tool output.
/// Used to give users a preview of what a running command is producing.
/// Empty input returns empty string.
pub fn format_partial_tail(output: &str, max_lines: usize) -> String {
    if output.is_empty() || max_lines == 0 {
        return String::new();
    }
    let lines: Vec<&str> = output.lines().collect();
    let total = lines.len();
    let start = total.saturating_sub(max_lines);
    let tail: Vec<&str> = lines[start..].to_vec();

    let mut result = String::new();
    if start > 0 {
        let skipped = start;
        let word = pluralize(skipped, "line", "lines");
        result.push_str(&format!("{DIM}    ┆ ... {skipped} {word} above{RESET}\n"));
    }
    for line in tail {
        let truncated = truncate_with_ellipsis(line, 120);
        result.push_str(&format!("{DIM}    ┆ {truncated}{RESET}\n"));
    }
    // Remove trailing newline
    if result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Count the number of lines in a tool result's text content.
pub fn count_result_lines(result: &ToolResult) -> usize {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.lines().count()),
            _ => None,
        })
        .sum()
}

/// Extract all text content from a ToolResult as a single string.
pub fn extract_result_text(result: &ToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// State tracker for a currently-running tool, used by the event loop
/// to display live progress updates.
#[allow(dead_code)]
pub struct ActiveToolState {
    pub tool_name: String,
    pub start: Instant,
    pub line_count: usize,
    pub last_output: String,
}

impl ActiveToolState {
    /// Create a new state tracker for a tool.
    #[allow(dead_code)]
    pub fn new(tool_name: String) -> Self {
        Self {
            tool_name,
            start: Instant::now(),
            line_count: 0,
            last_output: String::new(),
        }
    }

    /// Update with partial output from a ToolExecutionUpdate event.
    #[allow(dead_code)]
    pub fn update_partial(&mut self, text: &str) {
        self.line_count = text.lines().count();
        self.last_output = text.to_string();
    }
}

/// A handle to a running tool-progress timer task.
/// Shows `  ⠹ bash ⏱ 12s` on stderr, updating every second.
/// Dropping or calling `stop()` cancels it and clears the line.
pub struct ToolProgressTimer {
    cancel: tokio::sync::watch::Sender<bool>,
    line_count: Arc<std::sync::atomic::AtomicUsize>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ToolProgressTimer {
    /// Start a timer that shows elapsed time for a tool on stderr.
    /// Updates every second with the current line count.
    pub fn start(tool_name: String) -> Self {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let line_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let line_count_clone = Arc::clone(&line_count);
        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let mut tick: usize = 0;
            // Wait 2 seconds before showing the timer — short commands
            // finish fast and don't need a progress display.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                _ = cancel_rx.changed() => {
                    return;
                }
            }
            loop {
                if *cancel_rx.borrow() {
                    eprint!("\r\x1b[K");
                    let _ = io::stderr().flush();
                    break;
                }
                let elapsed = start.elapsed();
                let lc = line_count_clone.load(std::sync::atomic::Ordering::Relaxed);
                let lc_opt = if lc > 0 { Some(lc) } else { None };
                let progress = format_tool_progress(&tool_name, elapsed, tick, lc_opt);
                eprint!("\r\x1b[K{progress}");
                let _ = io::stderr().flush();
                tick = tick.wrapping_add(1);

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                    _ = cancel_rx.changed() => {
                        eprint!("\r\x1b[K");
                        let _ = io::stderr().flush();
                        break;
                    }
                }
            }
        });
        Self {
            cancel: cancel_tx,
            line_count,
            handle: Some(handle),
        }
    }

    /// Update the line count shown in the timer display.
    pub fn set_line_count(&self, count: usize) {
        self.line_count
            .store(count, std::sync::atomic::Ordering::Relaxed);
    }

    /// Stop the timer and clear its output.
    pub fn stop(self) {
        let _ = self.cancel.send(true);
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
    }
}

impl Drop for ToolProgressTimer {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        eprint!("\r\x1b[K");
        let _ = io::stderr().flush();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use yoagent::types::{Content, ToolResult};

    #[test]
    fn test_format_tool_summary_bash() {
        let args = serde_json::json!({"command": "echo hello"});
        assert_eq!(format_tool_summary("bash", &args), "$ echo hello");
    }

    #[test]
    fn test_format_tool_summary_bash_long_command() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = format_tool_summary("bash", &args);
        assert!(result.starts_with("$ "));
        assert!(result.ends_with('…'));
        assert!(result.len() < 100);
    }

    #[test]
    fn test_format_tool_summary_read_file() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(format_tool_summary("read_file", &args), "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_write_file() {
        let args = serde_json::json!({"path": "out.txt"});
        assert_eq!(format_tool_summary("write_file", &args), "write out.txt");
    }

    #[test]
    fn test_format_tool_summary_edit_file() {
        let args = serde_json::json!({"path": "foo.rs"});
        assert_eq!(format_tool_summary("edit_file", &args), "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_list_files() {
        let args = serde_json::json!({"path": "src/"});
        assert_eq!(format_tool_summary("list_files", &args), "ls src/");
    }

    #[test]
    fn test_format_tool_summary_list_files_no_path() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("list_files", &args), "ls .");
    }

    #[test]
    fn test_format_tool_summary_search() {
        let args = serde_json::json!({"pattern": "TODO"});
        assert_eq!(format_tool_summary("search", &args), "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_unknown_tool() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("custom_tool", &args), "custom_tool");
    }

    #[test]
    fn test_spinner_frames_not_empty() {
        assert!(!SPINNER_FRAMES.is_empty());
    }

    #[test]
    fn test_spinner_frames_are_braille() {
        // All braille characters are in the Unicode range U+2800..U+28FF
        for &frame in SPINNER_FRAMES {
            assert!(
                ('\u{2800}'..='\u{28FF}').contains(&frame),
                "Expected braille character, got {:?}",
                frame
            );
        }
    }

    #[test]
    fn test_spinner_frame_cycling() {
        // First 10 frames should match SPINNER_FRAMES exactly
        for (i, &expected) in SPINNER_FRAMES.iter().enumerate() {
            assert_eq!(spinner_frame(i), expected);
        }
    }

    #[test]
    fn test_spinner_frame_wraps_around() {
        let len = SPINNER_FRAMES.len();
        // After one full cycle, it should repeat
        assert_eq!(spinner_frame(0), spinner_frame(len));
        assert_eq!(spinner_frame(1), spinner_frame(len + 1));
        assert_eq!(spinner_frame(2), spinner_frame(len + 2));
    }

    #[test]
    fn test_spinner_frame_large_index() {
        // Should not panic even with very large indices
        let frame = spinner_frame(999_999);
        assert!(SPINNER_FRAMES.contains(&frame));
    }

    #[test]
    fn test_spinner_frames_all_unique() {
        // Each frame in the animation should be distinct
        let mut seen = std::collections::HashSet::new();
        for &frame in SPINNER_FRAMES {
            assert!(seen.insert(frame), "Duplicate spinner frame: {:?}", frame);
        }
    }

    // --- format_edit_diff tests ---

    #[test]
    fn test_format_edit_diff_single_line_change() {
        let diff = format_edit_diff("old line", "new line");
        assert!(diff.contains("- old line"));
        assert!(diff.contains("+ new line"));
        // Should have red for removed, green for added
        assert!(diff.contains(&format!("{RED}")));
        assert!(diff.contains(&format!("{GREEN}")));
    }

    #[test]
    fn test_format_edit_diff_multi_line_change() {
        let old = "line 1\nline 2\nline 3";
        let new = "line A\nline B";
        let diff = format_edit_diff(old, new);
        assert!(diff.contains("- line 1"));
        assert!(diff.contains("- line 2"));
        assert!(diff.contains("- line 3"));
        assert!(diff.contains("+ line A"));
        assert!(diff.contains("+ line B"));
    }

    #[test]
    fn test_format_edit_diff_addition_only() {
        let diff = format_edit_diff("", "new content\nmore content");
        // No removed lines
        assert!(!diff.contains("- "));
        // Added lines present
        assert!(diff.contains("+ new content"));
        assert!(diff.contains("+ more content"));
    }

    #[test]
    fn test_format_edit_diff_deletion_only() {
        let diff = format_edit_diff("old content\nmore old", "");
        // Removed lines present
        assert!(diff.contains("- old content"));
        assert!(diff.contains("- more old"));
        // No added lines
        assert!(!diff.contains("+ "));
    }

    #[test]
    fn test_format_edit_diff_long_diff_truncation() {
        // Generate a diff with more than MAX_DIFF_LINES lines
        let old_lines: Vec<&str> = (0..15).map(|_| "old").collect();
        let new_lines: Vec<&str> = (0..15).map(|_| "new").collect();
        let old = old_lines.join("\n");
        let new = new_lines.join("\n");
        let diff = format_edit_diff(&old, &new);
        // Should be truncated — total would be 30 lines, max is 20
        assert!(diff.contains("more lines)"));
    }

    #[test]
    fn test_format_edit_diff_empty_both() {
        let diff = format_edit_diff("", "");
        assert!(diff.is_empty());
    }

    #[test]
    fn test_format_edit_diff_empty_old_text_new_file_section() {
        // Simulates adding new content to a file (old_text is empty)
        let diff = format_edit_diff("", "fn new_function() {\n    println!(\"hello\");\n}");
        assert!(!diff.contains("- "));
        assert!(diff.contains("+ fn new_function()"));
        assert!(diff.contains("+ }"));
    }

    #[test]
    fn test_format_edit_diff_short_diff_not_truncated() {
        let diff = format_edit_diff("a", "b");
        assert!(!diff.contains("more lines"));
    }

    // --- format_tool_summary write_file with line count ---

    #[test]
    fn test_format_tool_summary_write_file_with_content() {
        let args = serde_json::json!({"path": "out.txt", "content": "line1\nline2\nline3"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt (3 lines)");
    }

    #[test]
    fn test_format_tool_summary_write_file_single_line() {
        let args = serde_json::json!({"path": "out.txt", "content": "hello"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt (1 line)");
    }

    #[test]
    fn test_format_tool_summary_write_file_no_content() {
        let args = serde_json::json!({"path": "out.txt"});
        let result = format_tool_summary("write_file", &args);
        assert_eq!(result, "write out.txt");
    }

    // --- format_tool_summary enriched details ---

    #[test]
    fn test_format_tool_summary_read_file_with_offset_and_limit() {
        let args = serde_json::json!({"path": "src/main.rs", "offset": 10, "limit": 50});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs:10..60");
    }

    #[test]
    fn test_format_tool_summary_read_file_with_offset_only() {
        let args = serde_json::json!({"path": "src/main.rs", "offset": 100});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs:100..");
    }

    #[test]
    fn test_format_tool_summary_read_file_with_limit_only() {
        let args = serde_json::json!({"path": "src/main.rs", "limit": 25});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs (25 lines)");
    }

    #[test]
    fn test_format_tool_summary_read_file_no_extras() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let result = format_tool_summary("read_file", &args);
        assert_eq!(result, "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_edit_file_with_text() {
        let args = serde_json::json!({
            "path": "foo.rs",
            "old_text": "fn old() {\n}\n",
            "new_text": "fn new() {\n    // improved\n    do_stuff();\n}\n"
        });
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs (2 → 4 lines)");
    }

    #[test]
    fn test_format_tool_summary_edit_file_no_text() {
        let args = serde_json::json!({"path": "foo.rs"});
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_edit_file_same_lines() {
        let args = serde_json::json!({
            "path": "foo.rs",
            "old_text": "let x = 1;",
            "new_text": "let x = 2;"
        });
        let result = format_tool_summary("edit_file", &args);
        assert_eq!(result, "edit foo.rs (1 → 1 lines)");
    }

    #[test]
    fn test_format_tool_summary_search_with_path() {
        let args = serde_json::json!({"pattern": "TODO", "path": "src/"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'TODO' in src/");
    }

    #[test]
    fn test_format_tool_summary_search_with_include() {
        let args = serde_json::json!({"pattern": "fn main", "include": "*.rs"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'fn main' (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_search_with_path_and_include() {
        let args = serde_json::json!({"pattern": "test", "path": "src/", "include": "*.rs"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'test' in src/ (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_search_pattern_only() {
        let args = serde_json::json!({"pattern": "TODO"});
        let result = format_tool_summary("search", &args);
        assert_eq!(result, "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_list_files_with_pattern() {
        let args = serde_json::json!({"path": "src/", "pattern": "*.rs"});
        let result = format_tool_summary("list_files", &args);
        assert_eq!(result, "ls src/ (*.rs)");
    }

    #[test]
    fn test_format_tool_summary_list_files_pattern_no_path() {
        let args = serde_json::json!({"pattern": "*.toml"});
        let result = format_tool_summary("list_files", &args);
        assert_eq!(result, "ls . (*.toml)");
    }

    #[test]
    fn test_format_tool_summary_bash_multiline_shows_first_line() {
        let args = serde_json::json!({"command": "cd src\ngrep -r 'test' ."});
        let result = format_tool_summary("bash", &args);
        assert!(
            result.starts_with("$ cd src"),
            "Should show first line: {result}"
        );
        assert!(
            result.contains("(2 lines)"),
            "Should indicate line count: {result}"
        );
    }

    // --- pluralize ---

    #[test]
    fn test_section_header_contains_label_and_line_chars() {
        let header = section_header("Thinking");
        assert!(
            header.contains("Thinking"),
            "header should contain the label"
        );
        assert!(
            header.contains("─"),
            "header should contain box-drawing chars"
        );
    }

    #[test]
    fn test_section_header_empty_label_produces_divider() {
        let header = section_header("");
        // Empty label should produce the same as section_divider
        let divider = section_divider();
        assert_eq!(header, divider);
    }

    #[test]
    fn test_section_divider_nonempty_with_line_chars() {
        let divider = section_divider();
        assert!(!divider.is_empty(), "divider should not be empty");
        assert!(
            divider.contains("─"),
            "divider should contain box-drawing chars"
        );
    }

    #[test]
    fn test_section_header_no_color() {
        // When NO_COLOR is set, the output still contains the label and line chars
        // (Color codes render as empty strings, but the structural content remains)
        let header = section_header("Tools");
        assert!(header.contains("Tools"));
        assert!(header.contains("─"));
    }

    #[test]
    fn test_section_divider_no_color() {
        let divider = section_divider();
        assert!(divider.contains("─"));
    }

    #[test]
    fn test_terminal_width_default() {
        // terminal_width should return a reasonable default (80) when COLUMNS is not set
        // or it should return the value of COLUMNS if set
        let width = terminal_width();
        assert!(width > 0, "terminal width should be positive");
    }

    #[test]
    fn test_section_header_with_various_labels() {
        // Test with different labels to ensure formatting works
        for label in &[
            "Thinking",
            "Response",
            "A",
            "Very Long Section Label For Testing",
        ] {
            let header = section_header(label);
            assert!(header.contains(label), "header should contain '{}'", label);
            assert!(header.contains("──"), "header should have line prefix");
        }
    }

    // ── tool batch summary tests ──────────────────────────────────

    #[test]
    fn test_tool_batch_summary_single_tool_returns_empty() {
        let result = format_tool_batch_summary(1, 1, 0, Duration::from_millis(500));
        assert!(
            result.is_empty(),
            "single tool batch should not produce summary"
        );
    }

    #[test]
    fn test_tool_batch_summary_zero_tools_returns_empty() {
        let result = format_tool_batch_summary(0, 0, 0, Duration::from_millis(0));
        assert!(result.is_empty(), "zero tools should not produce summary");
    }

    #[test]
    fn test_tool_batch_summary_all_succeed() {
        let result = format_tool_batch_summary(3, 3, 0, Duration::from_millis(1200));
        assert!(result.contains("3 tools"), "should show tool count");
        assert!(result.contains("1.2s"), "should show duration");
        assert!(result.contains("3"), "should show success count");
        assert!(result.contains("✓"), "should show success marker");
        // When all succeed, no failure count shown
        assert!(
            !result.contains("✗"),
            "should not show failure marker when all succeed"
        );
    }

    #[test]
    fn test_tool_batch_summary_with_failures() {
        let result = format_tool_batch_summary(4, 3, 1, Duration::from_millis(2500));
        assert!(result.contains("4 tools"), "should show total count");
        assert!(result.contains("2.5s"), "should show duration");
        assert!(result.contains("3"), "should show success count");
        assert!(result.contains("✓"), "should show success marker");
        assert!(result.contains("1"), "should show failure count");
        assert!(result.contains("✗"), "should show failure marker");
    }

    #[test]
    fn test_tool_batch_summary_two_tools_plural() {
        let result = format_tool_batch_summary(2, 2, 0, Duration::from_millis(800));
        assert!(result.contains("2 tools"), "should pluralize 'tools'");
        assert!(result.contains("800ms"), "should show ms for sub-second");
    }

    // ── indent tool output tests ──────────────────────────────────

    #[test]
    fn test_indent_tool_output_empty() {
        assert_eq!(indent_tool_output(""), "");
    }

    #[test]
    fn test_indent_tool_output_single_line() {
        let result = indent_tool_output("hello world");
        assert!(result.contains("│"), "should have indent marker");
        assert!(result.contains("hello world"), "should preserve content");
    }

    #[test]
    fn test_indent_tool_output_multiline() {
        let result = indent_tool_output("line 1\nline 2\nline 3");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3, "should preserve line count");
        for line in &lines {
            assert!(line.contains("│"), "each line should have indent marker");
        }
        assert!(lines[0].contains("line 1"));
        assert!(lines[1].contains("line 2"));
        assert!(lines[2].contains("line 3"));
    }

    // ── turn boundary tests ──────────────────────────────────

    #[test]
    fn test_turn_boundary_contains_number() {
        let result = turn_boundary(1);
        assert!(result.contains("Turn 1"), "should show turn number");
        assert!(result.contains("╭"), "should have box-drawing start");
        assert!(result.contains("╮"), "should have box-drawing end");
    }

    #[test]
    fn test_turn_boundary_different_numbers() {
        for n in [1, 5, 10, 99] {
            let result = turn_boundary(n);
            assert!(
                result.contains(&format!("Turn {n}")),
                "should contain Turn {n}"
            );
        }
    }

    #[test]
    fn test_turn_boundary_has_fill_characters() {
        let result = turn_boundary(1);
        assert!(result.contains("─"), "should have fill characters");
    }

    // --- Streaming latency tests (issue #147) ---

    #[test]
    fn test_format_duration_live_seconds() {
        assert_eq!(format_duration_live(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration_live(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration_live(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn test_format_duration_live_minutes() {
        assert_eq!(format_duration_live(Duration::from_secs(60)), "1m");
        assert_eq!(format_duration_live(Duration::from_secs(65)), "1m 5s");
        assert_eq!(format_duration_live(Duration::from_secs(120)), "2m");
        assert_eq!(format_duration_live(Duration::from_secs(3599)), "59m 59s");
    }

    #[test]
    fn test_format_duration_live_hours() {
        assert_eq!(format_duration_live(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration_live(Duration::from_secs(3660)), "1h 1m");
        assert_eq!(format_duration_live(Duration::from_secs(7200)), "2h");
    }

    #[test]
    fn test_format_tool_progress_no_lines() {
        let output = format_tool_progress("bash", Duration::from_secs(5), 0, None);
        assert!(output.contains("bash"), "should contain tool name");
        assert!(output.contains("⏱"), "should contain timer emoji");
        assert!(output.contains("5s"), "should contain elapsed time");
        // Should contain spinner frame
        assert!(
            output.contains('⠋'),
            "should contain spinner frame for tick 0"
        );
    }

    #[test]
    fn test_format_tool_progress_with_lines() {
        let output = format_tool_progress("bash", Duration::from_secs(12), 3, Some(142));
        assert!(output.contains("bash"), "should contain tool name");
        assert!(output.contains("12s"), "should contain elapsed time");
        assert!(output.contains("142 lines"), "should contain line count");
    }

    #[test]
    fn test_format_tool_progress_single_line() {
        let output = format_tool_progress("bash", Duration::from_secs(1), 0, Some(1));
        assert!(output.contains("1 line"), "should use singular 'line'");
        assert!(!output.contains("1 lines"), "should not use plural for 1");
    }

    #[test]
    fn test_format_tool_progress_zero_lines_hidden() {
        let output = format_tool_progress("bash", Duration::from_secs(3), 0, Some(0));
        assert!(!output.contains("line"), "zero lines should be hidden");
    }

    #[test]
    fn test_format_partial_tail_empty() {
        assert_eq!(format_partial_tail("", 3), "");
    }

    #[test]
    fn test_format_partial_tail_zero_lines() {
        assert_eq!(format_partial_tail("hello\nworld", 0), "");
    }

    #[test]
    fn test_format_partial_tail_fewer_lines_than_max() {
        let output = format_partial_tail("line1\nline2", 5);
        assert!(output.contains("line1"), "should show all lines");
        assert!(output.contains("line2"), "should show all lines");
        assert!(
            !output.contains("above"),
            "should not show 'above' indicator"
        );
    }

    #[test]
    fn test_format_partial_tail_more_lines_than_max() {
        let output = format_partial_tail("line1\nline2\nline3\nline4\nline5", 2);
        assert!(!output.contains("line1"), "should not show early lines");
        assert!(!output.contains("line2"), "should not show early lines");
        assert!(!output.contains("line3"), "should not show line3");
        assert!(output.contains("line4"), "should show tail lines");
        assert!(output.contains("line5"), "should show tail lines");
        assert!(output.contains("3 lines above"), "should show skip count");
    }

    #[test]
    fn test_format_partial_tail_uses_pipe_indent() {
        let output = format_partial_tail("hello", 1);
        assert!(
            output.contains("┆"),
            "should use dotted pipe for indentation"
        );
    }

    #[test]
    fn test_count_result_lines() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "line1\nline2\nline3".to_string(),
            }],
            details: serde_json::Value::Null,
        };
        assert_eq!(count_result_lines(&result), 3);
    }

    #[test]
    fn test_count_result_lines_empty() {
        let result = ToolResult {
            content: vec![],
            details: serde_json::Value::Null,
        };
        assert_eq!(count_result_lines(&result), 0);
    }

    #[test]
    fn test_extract_result_text() {
        let result = ToolResult {
            content: vec![
                Content::Text {
                    text: "hello".to_string(),
                },
                Content::Text {
                    text: "world".to_string(),
                },
            ],
            details: serde_json::Value::Null,
        };
        assert_eq!(extract_result_text(&result), "hello\nworld");
    }

    #[test]
    fn test_extract_result_text_empty() {
        let result = ToolResult {
            content: vec![],
            details: serde_json::Value::Null,
        };
        assert_eq!(extract_result_text(&result), "");
    }

    #[test]
    fn test_active_tool_state_new() {
        let state = ActiveToolState::new("bash".to_string());
        assert_eq!(state.tool_name, "bash");
        assert_eq!(state.line_count, 0);
        assert!(state.last_output.is_empty());
    }

    #[test]
    fn test_active_tool_state_update_partial() {
        let mut state = ActiveToolState::new("bash".to_string());
        state.update_partial("line1\nline2\nline3");
        assert_eq!(state.line_count, 3);
        assert_eq!(state.last_output, "line1\nline2\nline3");
    }
}
