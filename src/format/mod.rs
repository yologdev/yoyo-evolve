//! Formatting helpers: ANSI colors, cost, duration, tokens, context bar, truncation.

use std::io::{self, Write};
use std::sync::OnceLock;
use std::time::Duration;

// --- Color support with NO_COLOR and --no-color ---

/// Whether color output has been disabled (via NO_COLOR env or --no-color flag).
static COLOR_DISABLED: OnceLock<bool> = OnceLock::new();

// --- Bell notification support with YOYO_NO_BELL and --no-bell ---

/// Whether bell notification has been disabled (via --no-bell flag or YOYO_NO_BELL env).
static BELL_DISABLED: OnceLock<bool> = OnceLock::new();

/// Disable bell notifications. Call from CLI arg parsing.
pub fn disable_bell() {
    let _ = BELL_DISABLED.set(true);
}

/// Check if bell is enabled. Respects YOYO_NO_BELL env var.
pub fn bell_enabled() -> bool {
    !*BELL_DISABLED.get_or_init(|| std::env::var("YOYO_NO_BELL").is_ok())
}

/// Ring the terminal bell if enabled and elapsed time exceeds threshold.
/// The bell character (\x07) causes most terminal emulators to flash the tab
/// or play a sound, alerting multitasking developers.
pub fn maybe_ring_bell(elapsed: Duration) {
    if bell_enabled() && elapsed.as_secs() >= 3 {
        let _ = io::stdout().write_all(b"\x07");
        let _ = io::stdout().flush();
    }
}

/// Disable color output. Call before any formatting happens (e.g., from CLI arg parsing).
pub fn disable_color() {
    let _ = COLOR_DISABLED.set(true);
}

/// Check if color output is enabled. Cached after first call.
/// Respects the NO_COLOR environment variable (https://no-color.org/).
fn color_enabled() -> bool {
    !*COLOR_DISABLED.get_or_init(|| std::env::var("NO_COLOR").is_ok())
}

/// A color code that respects the NO_COLOR convention.
/// When color is disabled, formats as an empty string.
pub struct Color(pub &'static str);

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if color_enabled() {
            f.write_str(self.0)
        } else {
            Ok(())
        }
    }
}

// ANSI color helpers — respect NO_COLOR env var and --no-color flag
pub static RESET: Color = Color("\x1b[0m");
pub static BOLD: Color = Color("\x1b[1m");
pub static DIM: Color = Color("\x1b[2m");
pub static GREEN: Color = Color("\x1b[32m");
pub static YELLOW: Color = Color("\x1b[33m");
pub static CYAN: Color = Color("\x1b[36m");
pub static RED: Color = Color("\x1b[31m");
pub static MAGENTA: Color = Color("\x1b[35m");
pub static ITALIC: Color = Color("\x1b[3m");
pub static BOLD_ITALIC: Color = Color("\x1b[1;3m");
pub static BOLD_CYAN: Color = Color("\x1b[1;36m");
pub static BOLD_YELLOW: Color = Color("\x1b[1;33m");

// --- Syntax highlighting for code blocks ---

mod cost;
/// Languages recognized for syntax highlighting.
mod highlight;
mod markdown;
mod tools;

pub use cost::*;
pub use highlight::*;
pub use markdown::*;
pub use tools::*;

/// Truncate a string at a safe UTF-8 char boundary, never exceeding `max_bytes`.
/// Returns a `&str` slice. Avoids panics from slicing mid-character.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut b = max_bytes;
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    &s[..b]
}

pub fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

/// Decode HTML entities in a string.
///
/// Handles named entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&#39;`,
/// `&nbsp;`, `&#x27;`, `&mdash;`, `&ndash;`, `&hellip;`, `&copy;`, `&reg;`)
/// and numeric entities (decimal `&#NNN;` and hex `&#xHH;`).
pub fn decode_html_entities(s: &str) -> String {
    // Fast path: if there's no '&', there are no entities to decode
    if !s.contains('&') {
        return s.to_string();
    }

    // First pass: named entities
    let s = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&copy;", "©")
        .replace("&reg;", "®");

    // Second pass: remaining numeric entities (&#NNN; and &#xHH;)
    let mut decoded = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' && chars.peek() == Some(&'#') {
            let mut entity = String::from("&#");
            chars.next(); // consume '#'
            while let Some(&nc) = chars.peek() {
                if nc == ';' {
                    chars.next();
                    break;
                }
                entity.push(nc);
                chars.next();
            }
            let num_str = &entity[2..];
            let parsed = if let Some(hex) = num_str.strip_prefix('x').or(num_str.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num_str.parse::<u32>().ok()
            };
            if let Some(ch) = parsed.and_then(char::from_u32) {
                decoded.push(ch);
            } else {
                // Failed to decode — emit original
                decoded.push_str(&entity);
                decoded.push(';');
            }
        } else {
            decoded.push(c);
        }
    }

    decoded
}

/// Default character threshold for tool output truncation.
/// Outputs longer than this get the head/tail treatment.
pub const TOOL_OUTPUT_MAX_CHARS: usize = 30_000;

/// Maximum tool output size in piped/CI mode (half of interactive).
/// Reduces context growth rate during evolution sessions and CI runs
/// where the user isn't watching live output anyway.
pub const TOOL_OUTPUT_MAX_CHARS_PIPED: usize = 15_000;

/// Number of lines to keep from the start of truncated output.
const TRUNCATION_HEAD_LINES: usize = 100;

/// Number of lines to keep from the end of truncated output.
const TRUNCATION_TAIL_LINES: usize = 50;

/// Minimum number of consecutive similar lines to trigger collapsing.
const COLLAPSE_MIN_LINES: usize = 4;

/// Maximum prefix length used for line category comparison.
const CATEGORY_PREFIX_MAX: usize = 20;

/// Strip ANSI escape codes and collapse runs of similar lines.
///
/// This reduces token usage when tool output is fed back to the LLM:
/// - **ANSI stripping**: removes `\x1b[...X` sequences (SGR, cursor, erase)
/// - **Repetitive line collapsing**: when 4+ consecutive lines share a category
///   prefix (first word(s) up to 20 chars), replaces with first line,
///   `"... (N more similar lines)"`, and last line.
///
/// Called before head/tail truncation so the truncation operates on
/// already-compressed output.
pub fn compress_tool_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }

    // Phase 1: strip ANSI escape codes
    let stripped = strip_ansi_codes(output);

    // Phase 2: filter test framework output (more specific, runs first)
    let filtered = filter_test_output(&stripped);

    // Phase 3: collapse repetitive line sequences
    collapse_repetitive_lines(&filtered)
}

/// Remove ANSI escape sequences from a string.
///
/// Matches `ESC [ <params> <final byte>` where params are digits/semicolons
/// and final byte is an ASCII letter.
///
/// Uses char-based iteration to correctly handle multi-byte UTF-8 content.
/// ANSI escape sequences are purely ASCII, so we can safely detect them
/// by checking for ESC (\x1b) and then consuming ASCII parameter/final bytes.
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Check for CSI sequence: ESC [
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Skip parameter bytes (digits, semicolons)
                while let Some(&p) = chars.peek() {
                    if p.is_ascii_digit() || p == ';' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Skip final byte (ASCII letter)
                if let Some(&f) = chars.peek() {
                    if f.is_ascii_alphabetic() {
                        chars.next();
                    }
                }
            }
            // Non-CSI escape sequences: just skip the ESC
        } else {
            result.push(c);
        }
    }

    result
}

/// Extract a "category" from a line for grouping similar lines.
///
/// Takes the leading whitespace + first word (up to CATEGORY_PREFIX_MAX chars).
/// Lines with the same category are considered similar.
fn line_category(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return "";
    }

    // Find end of first word in the trimmed content
    let first_word_end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());

    // Include leading whitespace length + first word
    let prefix_len = (line.len() - trimmed.len()) + first_word_end;
    let mut end = prefix_len.min(CATEGORY_PREFIX_MAX).min(line.len());

    // Ensure we don't slice inside a multi-byte UTF-8 character
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }

    &line[..end]
}

/// Collapse runs of 4+ consecutive lines that share a category prefix.
fn collapse_repetitive_lines(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() < COLLAPSE_MIN_LINES {
        return s.to_string();
    }

    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        let cat = line_category(lines[i]);

        // Count consecutive lines with the same non-empty category
        if !cat.is_empty() {
            let mut run_end = i + 1;
            while run_end < lines.len() && line_category(lines[run_end]) == cat {
                run_end += 1;
            }
            let run_len = run_end - i;

            if run_len >= COLLAPSE_MIN_LINES {
                // Collapse: first line, marker, last line
                result.push(lines[i].to_string());
                let collapsed = run_len - 2; // exclude first and last
                result.push(format!("... ({collapsed} more similar lines)"));
                result.push(lines[run_end - 1].to_string());
                i = run_end;
                continue;
            }
        }

        result.push(lines[i].to_string());
        i += 1;
    }

    result.join("\n")
}

/// Minimum number of test-pass lines required to activate the test filter.
const TEST_FILTER_MIN_PASS_LINES: usize = 5;

/// Detect and filter test framework output, keeping only failures + summary.
///
/// Supports:
/// - **cargo test**: `test ... ok` / `test ... FAILED`, `test result:` summary
/// - **pytest**: `PASSED` / `FAILED` lines, summary with pass/fail counts
/// - **jest/vitest**: `✓` (pass) / `✕`/`✗` (fail) markers, `Tests:` summary
/// - **go test**: `--- PASS:` / `--- FAIL:`, `ok`/`FAIL` summary
/// - **rspec**: lines with `examples` and `failures` count
///
/// When ≥5 test-pass lines are detected, replaces them with a count marker.
/// Failure lines, their context, error sections, and summaries are preserved.
/// Non-test output passes through unchanged.
pub fn filter_test_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = output.lines().collect();

    // Phase 1: classify each line
    let mut classifications: Vec<TestLineKind> = Vec::with_capacity(lines.len());
    for line in &lines {
        classifications.push(classify_test_line(line));
    }

    // Count pass lines to decide if we should filter
    let pass_count = classifications
        .iter()
        .filter(|k| matches!(k, TestLineKind::Pass))
        .count();

    if pass_count < TEST_FILTER_MIN_PASS_LINES {
        return output.to_string();
    }

    // Phase 2: mark lines in failure sections as kept
    // Once we see a "failures:" header, everything until the summary is a failure section
    let mut in_failure_section = false;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "failures:"
            || trimmed.starts_with("---- ") && trimmed.ends_with(" stdout ----")
        {
            in_failure_section = true;
        }
        if in_failure_section {
            if matches!(classifications[i], TestLineKind::Pass) {
                // Don't reclassify pass lines even in failure sections
            } else if matches!(classifications[i], TestLineKind::Other) {
                classifications[i] = TestLineKind::FailureDetail;
            }
        }
        // Summary lines end the failure section
        if matches!(classifications[i], TestLineKind::Summary) {
            in_failure_section = false;
        }
    }

    // Phase 3: build filtered output
    let mut result_lines: Vec<String> = Vec::new();
    let mut omitted_pass_count: usize = 0;

    for (i, line) in lines.iter().enumerate() {
        match classifications[i] {
            TestLineKind::Pass => {
                omitted_pass_count += 1;
            }
            TestLineKind::Fail | TestLineKind::FailureDetail | TestLineKind::Summary => {
                // Flush any accumulated pass count before this line
                if omitted_pass_count > 0 {
                    result_lines.push(format!("... ({omitted_pass_count} passing tests omitted)"));
                    omitted_pass_count = 0;
                }
                result_lines.push(line.to_string());
            }
            TestLineKind::Other => {
                // Flush any accumulated pass count before non-test content
                if omitted_pass_count > 0 {
                    result_lines.push(format!("... ({omitted_pass_count} passing tests omitted)"));
                    omitted_pass_count = 0;
                }
                result_lines.push(line.to_string());
            }
        }
    }

    // Flush trailing pass count
    if omitted_pass_count > 0 {
        result_lines.push(format!("... ({omitted_pass_count} passing tests omitted)"));
    }

    result_lines.join("\n")
}

/// Classification of a line in test output.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TestLineKind {
    /// A passing test line (will be omitted)
    Pass,
    /// A failing test line (will be kept)
    Fail,
    /// Detail lines inside a failure section (stack traces, assertions)
    FailureDetail,
    /// Summary/result line (will be kept)
    Summary,
    /// Non-test output (will be kept)
    Other,
}

/// Classify a single line as test pass, fail, summary, or other.
fn classify_test_line(line: &str) -> TestLineKind {
    let trimmed = line.trim();

    // --- cargo test ---
    if trimmed.starts_with("test ") && trimmed.ends_with("... ok") {
        return TestLineKind::Pass;
    }
    if trimmed.starts_with("test ") && trimmed.ends_with("... FAILED") {
        return TestLineKind::Fail;
    }
    if trimmed.starts_with("test result:") {
        return TestLineKind::Summary;
    }

    // --- pytest ---
    if trimmed.ends_with(" PASSED") && trimmed.contains("::") {
        return TestLineKind::Pass;
    }
    if trimmed.ends_with(" FAILED") && trimmed.contains("::") {
        return TestLineKind::Fail;
    }
    // pytest summary: "N passed" or "N passed, M failed"
    if (trimmed.contains(" passed") || trimmed.contains(" failed"))
        && trimmed.starts_with('=')
        && trimmed.ends_with('=')
    {
        return TestLineKind::Summary;
    }

    // --- jest/vitest ---
    // ✓ or ✔ = pass; ✕ or ✗ = fail
    if trimmed.starts_with('✓') || trimmed.starts_with('✔') {
        return TestLineKind::Pass;
    }
    if trimmed.starts_with("✕") || trimmed.starts_with("✗") {
        return TestLineKind::Fail;
    }
    if trimmed.starts_with("Tests:") && (trimmed.contains("passed") || trimmed.contains("failed")) {
        return TestLineKind::Summary;
    }

    // --- go test ---
    if trimmed.starts_with("--- PASS:") {
        return TestLineKind::Pass;
    }
    if trimmed.starts_with("--- FAIL:") {
        return TestLineKind::Fail;
    }
    // go test summary: "ok  pkg  0.123s" or "FAIL  pkg  0.123s"
    if (trimmed.starts_with("ok ") || trimmed.starts_with("FAIL\t") || trimmed.starts_with("FAIL "))
        && trimmed.contains('s')
        && !trimmed.contains("::")
    {
        // Distinguish "FAIL" summary from pytest "FAILED" lines
        if trimmed.starts_with("ok ") {
            return TestLineKind::Summary;
        }
        if trimmed.starts_with("FAIL") && !trimmed.ends_with("FAILED") {
            return TestLineKind::Summary;
        }
    }

    // --- rspec ---
    if trimmed.contains("example")
        && trimmed.contains("failure")
        && trimmed.chars().any(|c| c.is_ascii_digit())
    {
        return TestLineKind::Summary;
    }

    // --- pytest short test summary header ---
    if trimmed.starts_with('=') && trimmed.contains("short test summary") {
        return TestLineKind::Summary;
    }

    // --- FAILED line in pytest summary (e.g., "FAILED tests/...") ---
    if trimmed.starts_with("FAILED ") && trimmed.contains("::") {
        return TestLineKind::Fail;
    }

    TestLineKind::Other
}

/// Intelligently truncate large tool output to save context window tokens.
///
/// Applies compression (ANSI stripping + repetitive line collapsing) first,
/// then when output exceeds `max_chars`, keeps the first ~100 lines and last ~50 lines
/// with a clear `[... truncated N lines ...]` marker in between. This preserves
/// the beginning of output (usually the most informative — headers, first errors)
/// and the end (summary lines, final status).
///
/// Output under the threshold is returned unchanged.
pub fn truncate_tool_output(output: &str, max_chars: usize) -> String {
    // Phase 1: compress (strip ANSI + collapse repetitive lines)
    let compressed = compress_tool_output(output);

    // Under threshold — return compressed output
    if compressed.len() <= max_chars {
        return compressed;
    }

    let lines: Vec<&str> = compressed.lines().collect();
    let total_lines = lines.len();

    // If not enough lines to meaningfully truncate, return as-is
    // (edge case: very long single lines or very few lines)
    if total_lines <= TRUNCATION_HEAD_LINES + TRUNCATION_TAIL_LINES {
        return compressed;
    }

    let head = &lines[..TRUNCATION_HEAD_LINES];
    let tail = &lines[total_lines - TRUNCATION_TAIL_LINES..];
    let omitted = total_lines - TRUNCATION_HEAD_LINES - TRUNCATION_TAIL_LINES;

    let mut result = String::with_capacity(max_chars);
    for line in head {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!(
        "\n[... truncated {omitted} {} ...]\n\n",
        pluralize(omitted, "line", "lines")
    ));
    for (i, line) in tail.iter().enumerate() {
        result.push_str(line);
        if i < tail.len() - 1 {
            result.push('\n');
        }
    }

    result
}

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

/// Format usage stats into a string (verbose or compact).
///
/// Verbose format (shown with `--verbose`):
///   `tokens: 1119 in / 47 out  [cache: ...]  (session: ...)  cost: ...  total: ...  ⏱ 1.0s`
///
/// Compact format (default):
///   `↳ 1.0s · 1119→47 tokens · $0.020`
pub fn format_usage_line(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
    verbose: bool,
) -> Option<String> {
    if usage.input == 0 && usage.output == 0 {
        return None;
    }

    let elapsed_str = format_duration(elapsed);

    if verbose {
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
        Some(format!(
            "tokens: {} in / {} out{cache_info}  (session: {} in / {} out){cost_info}{total_cost_info}  ⏱ {elapsed_str}",
            usage.input, usage.output, total.input, total.output
        ))
    } else {
        let cost_suffix = estimate_cost(usage, model)
            .map(|c| format!(" · {}", format_cost(c)))
            .unwrap_or_default();
        Some(format!(
            "↳ {elapsed_str} · {}→{} tokens{cost_suffix}",
            usage.input, usage.output
        ))
    }
}

/// Print usage stats after a prompt response.
pub fn print_usage(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) {
    if let Some(line) = format_usage_line(usage, total, model, elapsed, crate::cli::is_verbose()) {
        println!("\n{DIM}  {line}{RESET}");
    }
}

/// Return the color code for a context usage percentage.
/// Green if ≤50%, yellow if 51-80%, red if >80%.
pub fn context_usage_color(pct: u32) -> &'static Color {
    if pct > 80 {
        &RED
    } else if pct > 50 {
        &YELLOW
    } else {
        &GREEN
    }
}

/// Print a context window usage indicator line.
/// Shows percentage of context consumed, color-coded by fullness.
pub fn print_context_usage(used_tokens: u64, max_tokens: u64) {
    if max_tokens == 0 {
        return;
    }
    let pct = ((used_tokens as f64 / max_tokens as f64) * 100.0).min(100.0) as u32;
    let color = context_usage_color(pct);
    println!("{DIM}  {color}⬤{RESET}{DIM} {pct}% of context window used{RESET}");
}

#[cfg(test)]
pub fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_unicode() {
        assert_eq!(truncate("héllo wörld", 5), "héllo");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn test_safe_truncate_empty_string() {
        assert_eq!(safe_truncate("", 10), "");
    }

    #[test]
    fn test_safe_truncate_ascii_shorter_than_max() {
        assert_eq!(safe_truncate("hello", 10), "hello");
    }

    #[test]
    fn test_safe_truncate_ascii_longer_than_max() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_safe_truncate_multibyte_no_panic() {
        // ✓ is 3 bytes (E2 9C 93). "hello ✓ world" = 13 chars, 15 bytes
        let s = "hello ✓ world";
        // Truncating at byte 7 would land inside ✓ — should back up to byte 6
        let result = safe_truncate(s, 7);
        assert_eq!(result, "hello ");
        // Truncating at byte 9 should include ✓ (bytes 6-8)
        let result = safe_truncate(s, 9);
        assert_eq!(result, "hello ✓");
    }

    #[test]
    fn test_safe_truncate_all_multibyte() {
        // Each CJK char is 3 bytes: "日本語テスト" = 18 bytes, 6 chars
        let s = "日本語テスト";
        // Truncating at 4 bytes should back up to 3 (one char)
        let result = safe_truncate(s, 4);
        assert_eq!(result, "日");
        // Truncating at 7 should back up to 6 (two chars)
        let result = safe_truncate(s, 7);
        assert_eq!(result, "日本");
    }

    #[test]
    fn test_safe_truncate_zero_max() {
        assert_eq!(safe_truncate("hello", 0), "");
        assert_eq!(safe_truncate("日本語", 0), "");
    }

    #[test]
    fn test_safe_truncate_exact_boundary() {
        // "ab✓" = 5 bytes. Truncating at exactly 5 should return all.
        let s = "ab✓";
        assert_eq!(safe_truncate(s, 5), "ab✓");
        // Truncating at 4 lands mid-char, should back up to 2
        assert_eq!(safe_truncate(s, 4), "ab");
        // Truncating at 2 should give "ab"
        assert_eq!(safe_truncate(s, 2), "ab");
    }

    #[test]
    fn test_truncate_adds_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
        assert_eq!(truncate_with_ellipsis("hi", 5), "hi");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

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
    fn test_color_struct_display_outputs_ansi() {
        // Color struct should produce the ANSI code when color is enabled
        let c = Color("\x1b[1m");
        let formatted = format!("{c}");
        // We can't guarantee NO_COLOR isn't set in the test environment,
        // but the type itself should compile and format correctly.
        assert!(formatted == "\x1b[1m" || formatted.is_empty());
    }

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
    fn test_truncate_tool_output_under_threshold_unchanged() {
        let short = "hello world\nsecond line\nthird line";
        let result = truncate_tool_output(short, 30_000);
        assert_eq!(result, short);
    }

    #[test]
    fn test_truncate_tool_output_empty_string() {
        let result = truncate_tool_output("", 30_000);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_tool_output_exactly_at_threshold() {
        // Create output exactly at the threshold.
        // Each line starts with a unique first word so compress won't collapse them.
        let lines: Vec<String> = (0..300)
            .map(|i| format!("L{i} {}", "x".repeat(100)))
            .collect();
        let output = lines.join("\n");
        // If it's at or under threshold length, it should be unchanged
        let result = truncate_tool_output(&output, output.len());
        assert_eq!(result, output);
    }

    #[test]
    fn test_truncate_tool_output_over_threshold_has_marker() {
        // Create output with 200 lines, each long enough to exceed 30k chars
        let line = "x".repeat(200);
        let lines: Vec<String> = (0..200).map(|i| format!("line{i}: {line}")).collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert!(result.contains("[... truncated"));
        assert!(result.contains("lines ...]"));
        // Should contain head lines
        assert!(result.contains("line0:"));
        assert!(result.contains("line99:"));
        // Should contain tail lines
        assert!(result.contains("line199:"));
        assert!(result.contains("line150:"));
        // Should NOT contain middle lines
        assert!(!result.contains("line100:"));
        assert!(!result.contains("line120:"));
    }

    #[test]
    fn test_truncate_tool_output_preserves_head_and_tail_count() {
        // 300 lines, each 200 chars → ~60k chars, well over 30k threshold.
        // Each line starts with a unique first word to avoid compression collapsing.
        let lines: Vec<String> = (0..300).map(|i| format!("U{i} {:>200}", i)).collect();
        let output = lines.join("\n");

        let result = truncate_tool_output(&output, 30_000);
        let _result_lines: Vec<&str> = result.lines().collect();

        // Head: first 100 lines should be present
        for i in 0..100 {
            let expected = format!("U{i} {:>200}", i);
            assert!(result.contains(&expected), "Missing head line {i}");
        }

        // Tail: last 50 lines should be present
        for i in 250..300 {
            let expected = format!("U{i} {:>200}", i);
            assert!(result.contains(&expected), "Missing tail line {i}");
        }

        // Middle should be omitted
        assert!(!result.contains(&format!("U150 {:>200}", 150)));

        // Marker should show correct count
        // 300 - 100 - 50 = 150 omitted lines
        assert!(result.contains("[... truncated 150 lines ...]"));

        // Result should be shorter than original
        assert!(result.len() < output.len());
    }

    #[test]
    fn test_truncate_tool_output_few_long_lines_not_truncated() {
        // Only 140 lines (< head + tail = 150), even if over char threshold
        // Should NOT be truncated because there aren't enough lines.
        // Each line starts with a unique first word to avoid compression collapsing.
        let lines: Vec<String> = (0..140)
            .map(|i| format!("L{i} {}", "x".repeat(500)))
            .collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert_eq!(
            result, output,
            "Too few lines to truncate, should be unchanged"
        );
    }

    #[test]
    fn test_truncate_tool_output_single_truncated_line_in_marker() {
        // 151 lines → head 100 + tail 50 + 1 omitted → "line" (singular).
        // Each line starts with a unique first word to avoid compression collapsing.
        let lines: Vec<String> = (0..151)
            .map(|i| format!("L{i} {}", "x".repeat(300)))
            .collect();
        let output = lines.join("\n");
        assert!(output.len() > 30_000);

        let result = truncate_tool_output(&output, 30_000);
        assert!(result.contains("[... truncated 1 line ...]"));
    }

    #[test]
    fn test_truncate_tool_output_default_threshold_constant() {
        // Verify the default constant is 30,000
        assert_eq!(TOOL_OUTPUT_MAX_CHARS, 30_000);
    }

    #[test]
    fn test_tool_output_max_chars_piped_smaller() {
        // Piped/CI mode limit should be strictly less than interactive limit
        const _: () = assert!(TOOL_OUTPUT_MAX_CHARS_PIPED < TOOL_OUTPUT_MAX_CHARS);
    }

    #[test]
    fn test_tool_output_max_chars_piped_value() {
        // Piped/CI mode limit should be 15,000
        assert_eq!(TOOL_OUTPUT_MAX_CHARS_PIPED, 15_000);
    }

    #[test]
    fn test_truncate_tool_output_with_custom_limit() {
        // Verify truncation respects a custom (small) limit.
        // Each line starts with a unique first word to avoid compression collapsing.
        let output = (0..200)
            .map(|i| format!("W{i} data"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_tool_output(&output, 100);
        // Output is well over 100 chars and has 200 lines (> head+tail),
        // so it should be truncated
        assert!(
            result.contains("[... truncated"),
            "Should be truncated with 100-char limit, got length {}",
            result.len()
        );
    }

    #[test]
    fn test_truncate_tool_output_respects_limit_parameter() {
        // Same output should NOT be truncated with a large limit but SHOULD be with a small one.
        // Each line starts with a unique first word to avoid compression collapsing.
        let output = (0..200)
            .map(|i| format!("R{i} data"))
            .collect::<Vec<_>>()
            .join("\n");
        let large_limit_result = truncate_tool_output(&output, 1_000_000);
        let small_limit_result = truncate_tool_output(&output, 100);
        assert_eq!(
            large_limit_result, output,
            "Large limit should return output unchanged"
        );
        assert_ne!(
            small_limit_result, output,
            "Small limit should truncate the output"
        );
    }

    // ── decode_html_entities tests ──────────────────────────────────

    #[test]
    fn test_decode_html_entities_named() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;"), "<");
        assert_eq!(decode_html_entities("&gt;"), ">");
        assert_eq!(decode_html_entities("&quot;"), "\"");
        assert_eq!(decode_html_entities("&apos;"), "'");
        assert_eq!(decode_html_entities("&#39;"), "'");
        assert_eq!(decode_html_entities("&nbsp;"), " ");
        assert_eq!(decode_html_entities("&#x27;"), "'");
        assert_eq!(decode_html_entities("&mdash;"), "—");
        assert_eq!(decode_html_entities("&ndash;"), "–");
        assert_eq!(decode_html_entities("&hellip;"), "…");
        assert_eq!(decode_html_entities("&copy;"), "©");
        assert_eq!(decode_html_entities("&reg;"), "®");
    }

    #[test]
    fn test_decode_html_entities_numeric_decimal() {
        // &#65; = 'A'
        assert_eq!(decode_html_entities("&#65;"), "A");
        // &#8212; = '—' (em dash)
        assert_eq!(decode_html_entities("&#8212;"), "—");
    }

    #[test]
    fn test_decode_html_entities_numeric_hex() {
        // &#x41; = 'A'
        assert_eq!(decode_html_entities("&#x41;"), "A");
        // &#x2014; = '—' (em dash)
        assert_eq!(decode_html_entities("&#x2014;"), "—");
    }

    #[test]
    fn test_decode_html_entities_mixed() {
        assert_eq!(
            decode_html_entities("hello &amp; world &lt;3 &#8212; done"),
            "hello & world <3 — done"
        );
    }

    #[test]
    fn test_decode_html_entities_no_entities() {
        assert_eq!(decode_html_entities("plain text"), "plain text");
    }

    #[test]
    fn test_decode_html_entities_invalid_numeric() {
        // Invalid numeric entity — should be preserved as-is
        assert_eq!(decode_html_entities("&#xZZZZ;"), "&#xZZZZ;");
        assert_eq!(decode_html_entities("&#abc;"), "&#abc;");
    }

    #[test]
    fn test_decode_html_entities_incomplete() {
        // Ampersand not part of an entity
        assert_eq!(decode_html_entities("a & b"), "a & b");
    }

    // --- Section header and divider tests ---

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
    fn test_bell_enabled_default() {
        // Verify bell_enabled() is callable and returns a bool without panicking.
        // Since OnceLock is global, the value depends on test ordering and env,
        // but the function itself should never panic.
        let _result = bell_enabled();
    }

    #[test]
    fn test_maybe_ring_bell_short_duration_no_bell() {
        // Durations under 3s should never ring the bell, regardless of settings.
        // This just verifies no panic or error — the bell character is harmless
        // even if it does get emitted.
        maybe_ring_bell(Duration::from_secs(0));
        maybe_ring_bell(Duration::from_secs(1));
        maybe_ring_bell(Duration::from_secs(2));
        // No assertion needed — we're testing that it doesn't panic.
    }

    #[test]
    fn test_maybe_ring_bell_long_duration_no_panic() {
        // Durations >= 3s should attempt the bell if enabled.
        // In test environment this is harmless.
        maybe_ring_bell(Duration::from_secs(3));
        maybe_ring_bell(Duration::from_secs(60));
    }

    // ── format_usage_line tests ────────────────────────────────────

    #[test]
    fn test_format_usage_compact() {
        let usage = yoagent::Usage {
            input: 1119,
            output: 47,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = yoagent::Usage {
            input: 1119,
            output: 47,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let elapsed = Duration::from_secs_f64(1.0);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, false)
            .expect("should produce output");
        // Compact: ↳ 1.0s · 1119→47 tokens · $0.006
        assert!(line.starts_with("↳ 1.0s"), "got: {line}");
        assert!(line.contains("1119→47 tokens"), "got: {line}");
        // Should NOT contain verbose markers
        assert!(!line.contains("session:"), "got: {line}");
        assert!(!line.contains("in /"), "got: {line}");
    }

    #[test]
    fn test_format_usage_verbose() {
        let usage = yoagent::Usage {
            input: 500,
            output: 100,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = yoagent::Usage {
            input: 2000,
            output: 400,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let elapsed = Duration::from_secs(3);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, true)
            .expect("should produce output");
        // Verbose: tokens: 500 in / 100 out  (session: 2000 in / 400 out) ...
        assert!(line.contains("tokens: 500 in / 100 out"), "got: {line}");
        assert!(line.contains("session: 2000 in / 400 out"), "got: {line}");
        assert!(line.contains("⏱"), "got: {line}");
    }

    #[test]
    fn test_format_usage_zero_tokens_returns_none() {
        let usage = yoagent::Usage {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_secs(1);
        assert!(
            format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, false).is_none()
        );
        assert!(
            format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, true).is_none()
        );
    }

    #[test]
    fn test_format_usage_verbose_with_cache() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 200,
            cache_read: 500,
            cache_write: 100,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_secs(2);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, true)
            .expect("should produce output");
        assert!(line.contains("[cache: 500 read, 100 write]"), "got: {line}");
    }

    #[test]
    fn test_format_usage_compact_includes_cost() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_secs(5);
        let line = format_usage_line(&usage, &total, "claude-sonnet-4-20250514", elapsed, false)
            .expect("should produce output");
        // Should have cost separator
        assert!(line.contains(" · $"), "compact should include cost: {line}");
    }

    #[test]
    fn test_format_usage_compact_unknown_model_no_cost() {
        let usage = yoagent::Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let total = usage.clone();
        let elapsed = Duration::from_millis(500);
        let line = format_usage_line(&usage, &total, "unknown-model-xyz", elapsed, false)
            .expect("should produce output");
        // No cost for unknown model
        assert!(
            !line.contains("$"),
            "unknown model should have no cost: {line}"
        );
        assert!(line.contains("100→50 tokens"), "got: {line}");
    }

    // ── ThinkBlockFilter tests ───────────────────────────────────────

    // ── context_usage_color tests ─────────────────────────────────────

    #[test]
    fn test_context_usage_color_green_at_zero() {
        let color = context_usage_color(0);
        assert_eq!(color.0, GREEN.0);
    }

    #[test]
    fn test_context_usage_color_green_at_50() {
        let color = context_usage_color(50);
        assert_eq!(color.0, GREEN.0);
    }

    #[test]
    fn test_context_usage_color_yellow_at_51() {
        let color = context_usage_color(51);
        assert_eq!(color.0, YELLOW.0);
    }

    #[test]
    fn test_context_usage_color_yellow_at_80() {
        let color = context_usage_color(80);
        assert_eq!(color.0, YELLOW.0);
    }

    #[test]
    fn test_context_usage_color_red_at_81() {
        let color = context_usage_color(81);
        assert_eq!(color.0, RED.0);
    }

    #[test]
    fn test_context_usage_color_red_at_100() {
        let color = context_usage_color(100);
        assert_eq!(color.0, RED.0);
    }

    // ── compress_tool_output tests ────────────────────────────────────

    #[test]
    fn test_compress_strips_ansi_codes() {
        let input = "\x1b[31merror\x1b[0m: something \x1b[1;33mwent\x1b[0m wrong";
        let result = compress_tool_output(input);
        assert_eq!(result, "error: something went wrong");
        assert!(!result.contains("\x1b"));
    }

    #[test]
    fn test_compress_strips_various_ansi_sequences() {
        // SGR, cursor movement, erase
        let input = "\x1b[32mgreen\x1b[0m \x1b[2Kclear \x1b[1Aup \x1b[38;5;196mcolor256\x1b[0m";
        let result = compress_tool_output(input);
        assert!(!result.contains("\x1b"), "still has ANSI: {result}");
        assert!(result.contains("green"));
        assert!(result.contains("color256"));
    }

    #[test]
    fn test_compress_collapses_repetitive_lines() {
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("   Compiling foo-{i} v1.0.{i}"));
        }
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        let result_lines: Vec<&str> = result.lines().collect();
        // Should have first line, collapse marker, last line = 3 lines
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(
            result_lines[0].contains("foo-0"),
            "first: {}",
            result_lines[0]
        );
        assert!(
            result_lines[1].contains("8 more similar"),
            "marker: {}",
            result_lines[1]
        );
        assert!(
            result_lines[2].contains("foo-9"),
            "last: {}",
            result_lines[2]
        );
    }

    #[test]
    fn test_compress_preserves_non_repetitive_output() {
        let input = "line one\nline two\nline three\nsomething different";
        let result = compress_tool_output(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_compress_short_output_unchanged() {
        // Only 3 similar lines — below the threshold of 4
        let input = "   Compiling a v1.0\n   Compiling b v1.0\n   Compiling c v1.0";
        let result = compress_tool_output(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_compress_mixed_repetitive_blocks() {
        let mut lines = Vec::new();
        for i in 0..5 {
            lines.push(format!("   Compiling crate-{i} v0.1.0"));
        }
        lines.push("warning: unused variable".to_string());
        lines.push("  --> src/main.rs:10:5".to_string());
        for i in 0..6 {
            lines.push(format!("  Downloading dep-{i} v2.0.0"));
        }
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        // Both repetitive blocks collapsed
        assert!(
            result.contains("3 more similar"),
            "compiling block: {result}"
        );
        assert!(
            result.contains("4 more similar"),
            "downloading block: {result}"
        );
        // Non-repetitive lines preserved
        assert!(result.contains("warning: unused variable"));
        assert!(result.contains("--> src/main.rs:10:5"));
    }

    #[test]
    fn test_truncate_uses_compression() {
        // Verify truncate_tool_output strips ANSI codes from output
        let input = "\x1b[32mhello\x1b[0m world";
        let result = truncate_tool_output(input, 100_000);
        assert!(!result.contains("\x1b"), "ANSI not stripped: {result}");
        assert!(result.contains("hello world"));
    }

    #[test]
    fn test_compress_exact_threshold_four_lines() {
        // Exactly 4 similar lines — should collapse
        let input = "   Compiling a v1\n   Compiling b v1\n   Compiling c v1\n   Compiling d v1";
        let result = compress_tool_output(input);
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(result_lines[1].contains("2 more similar"));
    }

    #[test]
    fn test_compress_empty_input() {
        assert_eq!(compress_tool_output(""), "");
    }

    #[test]
    fn test_compress_pip_install_pattern() {
        let mut lines = Vec::new();
        for i in 0..8 {
            lines.push(format!("Installing package-{i}==1.0.{i}"));
        }
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(result_lines[1].contains("6 more similar"));
    }

    #[test]
    fn test_strip_ansi_preserves_multibyte_utf8() {
        // ✓ is 3 bytes (0xE2 0x9C 0x93), 日本語 has 3-byte chars
        let input = "\x1b[32m✓\x1b[0m passed: 日本語テスト";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "✓ passed: 日本語テスト");
    }

    #[test]
    fn test_strip_ansi_preserves_emoji() {
        // Emoji are 4-byte UTF-8 characters
        let input = "\x1b[1m🦀 Rust\x1b[0m is 🔥";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "🦀 Rust is 🔥");
    }

    #[test]
    fn test_strip_ansi_preserves_accented_chars() {
        // é is 2 bytes (0xC3 0xA9)
        let input = "\x1b[33mcafé\x1b[0m résumé";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "café résumé");
    }

    #[test]
    fn test_compress_multibyte_content() {
        // End-to-end: compress_tool_output should handle multi-byte chars
        let input = "\x1b[32m✓\x1b[0m テスト完了";
        let result = compress_tool_output(input);
        assert_eq!(result, "✓ テスト完了");
    }

    #[test]
    fn test_line_category_multibyte_prefix() {
        // "日本語テストの結" = 8 chars × 3 bytes = 24 bytes, no spaces.
        // first_word_end = 24 (no whitespace found), prefix_len = 24,
        // min(24, CATEGORY_PREFIX_MAX=20) = 20, but byte 20 is inside
        // the 7th character (bytes 18-20). Must not panic.
        let line = "日本語テストの結";
        let _cat = line_category(line); // Should not panic
    }

    #[test]
    fn test_line_category_multibyte_short_word() {
        // "café something" — first word "café" is 5 chars but 6 bytes
        let line = "café something";
        let cat = line_category(line);
        assert_eq!(cat, "café");
    }

    #[test]
    fn test_collapse_repetitive_multibyte_lines() {
        // Lines with multi-byte content that share a category
        let mut lines = Vec::new();
        for i in 0..6 {
            lines.push(format!("コンパイル中 パッケージ-{i} v1.0"));
        }
        let input = lines.join("\n");
        let result = collapse_repetitive_lines(&input);
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 3, "got: {result}");
        assert!(result_lines[1].contains("4 more similar"));
    }

    // ── filter_test_output tests ────────────────────────────────────

    #[test]
    fn test_filter_cargo_test_all_passing() {
        let mut lines = Vec::new();
        for i in 0..20 {
            lines.push(format!("test tests::test_case_{i} ... ok"));
        }
        lines.push(String::new());
        lines.push("test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("(20 passing tests omitted)"),
            "should omit passing tests, got: {result}"
        );
        assert!(
            result.contains("test result: ok."),
            "should keep summary, got: {result}"
        );
        // Should be much shorter than input
        assert!(
            result.lines().count() < 5,
            "should be very short, got {} lines: {result}",
            result.lines().count()
        );
    }

    #[test]
    fn test_filter_cargo_test_with_failures() {
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("test tests::test_pass_{i} ... ok"));
        }
        lines.push("test tests::test_broken ... FAILED".to_string());
        for i in 10..15 {
            lines.push(format!("test tests::test_pass_{i} ... ok"));
        }
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push(String::new());
        lines.push("---- tests::test_broken stdout ----".to_string());
        lines.push("thread 'tests::test_broken' panicked at 'assertion failed'".to_string());
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push("    tests::test_broken".to_string());
        lines.push(String::new());
        lines.push("test result: FAILED. 15 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.0s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // Failures must be preserved
        assert!(
            result.contains("test tests::test_broken ... FAILED"),
            "should keep failure line, got: {result}"
        );
        // Failure details must be preserved
        assert!(
            result.contains("assertion failed"),
            "should keep failure details, got: {result}"
        );
        // Summary must be preserved
        assert!(
            result.contains("test result: FAILED."),
            "should keep summary, got: {result}"
        );
        // Passing tests should be omitted
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing tests, got: {result}"
        );
        assert!(
            !result.contains("test_pass_5 ... ok"),
            "should not contain passing test lines, got: {result}"
        );
    }

    #[test]
    fn test_filter_cargo_test_failure_details_preserved() {
        let mut lines = Vec::new();
        for i in 0..5 {
            lines.push(format!("test test_{i} ... ok"));
        }
        lines.push("test test_bad ... FAILED".to_string());
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push(String::new());
        lines.push("---- test_bad stdout ----".to_string());
        lines.push("thread 'test_bad' panicked at src/lib.rs:42:".to_string());
        lines.push("assertion `left == right` failed".to_string());
        lines.push("  left: 1".to_string());
        lines.push("  right: 2".to_string());
        lines.push("note: run with `RUST_BACKTRACE=1`".to_string());
        lines.push(String::new());
        lines.push("failures:".to_string());
        lines.push("    test_bad".to_string());
        lines.push(String::new());
        lines.push(
            "test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out"
                .to_string(),
        );
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // All failure details must be present
        assert!(
            result.contains("thread 'test_bad' panicked"),
            "got: {result}"
        );
        assert!(result.contains("left: 1"), "got: {result}");
        assert!(result.contains("right: 2"), "got: {result}");
        assert!(result.contains("RUST_BACKTRACE"), "got: {result}");
    }

    #[test]
    fn test_filter_pytest_output() {
        let mut lines = Vec::new();
        lines.push(
            "============================= test session starts ============================="
                .to_string(),
        );
        lines.push("collected 15 items".to_string());
        lines.push(String::new());
        for i in 0..12 {
            lines.push(format!("tests/test_app.py::test_case_{i} PASSED"));
        }
        lines.push("tests/test_app.py::test_broken FAILED".to_string());
        lines.push("tests/test_app.py::test_another PASSED".to_string());
        lines.push("tests/test_app.py::test_more PASSED".to_string());
        lines.push(String::new());
        lines.push(
            "=========================== short test summary info ==========================="
                .to_string(),
        );
        lines.push("FAILED tests/test_app.py::test_broken - AssertionError".to_string());
        lines.push(
            "========================= 14 passed, 1 failed =========================".to_string(),
        );
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing pytest tests, got: {result}"
        );
        assert!(
            result.contains("test_broken FAILED"),
            "should keep failures, got: {result}"
        );
        assert!(
            result.contains("14 passed, 1 failed"),
            "should keep summary, got: {result}"
        );
    }

    #[test]
    fn test_filter_jest_output() {
        let mut lines = Vec::new();
        lines.push("PASS src/app.test.js".to_string());
        lines.push("  App component".to_string());
        for i in 0..10 {
            lines.push(format!("    ✓ should render item {i} (5ms)"));
        }
        lines.push("    ✕ should handle error (10ms)".to_string());
        lines.push(String::new());
        lines.push("Tests:  1 failed, 10 passed, 11 total".to_string());
        lines.push("Time:   2.5s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing jest tests, got: {result}"
        );
        assert!(
            result.contains("should handle error"),
            "should keep failure, got: {result}"
        );
        assert!(
            result.contains("Tests:"),
            "should keep summary, got: {result}"
        );
    }

    #[test]
    fn test_filter_go_test_output() {
        let mut lines = Vec::new();
        for i in 0..8 {
            lines.push(format!("--- PASS: TestCase{i} (0.00s)"));
        }
        lines.push("--- FAIL: TestBroken (0.01s)".to_string());
        lines.push("    expected: 1, got: 2".to_string());
        lines.push("FAIL".to_string());
        lines.push("FAIL    github.com/user/repo    0.05s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing go tests, got: {result}"
        );
        assert!(
            result.contains("--- FAIL: TestBroken"),
            "should keep failure, got: {result}"
        );
        assert!(
            result.contains("expected: 1, got: 2"),
            "should keep failure details, got: {result}"
        );
    }

    #[test]
    fn test_filter_non_test_output_unchanged() {
        let input = "hello world\nthis is regular output\nnothing to see here\nfoo bar baz";
        let result = filter_test_output(input);
        assert_eq!(
            result, input,
            "non-test output should pass through unchanged"
        );
    }

    #[test]
    fn test_filter_mixed_content() {
        // Compilation output followed by test output
        let mut lines = vec![
            "   Compiling myapp v0.1.0".to_string(),
            "   Compiling dep v1.0.0".to_string(),
            "    Finished test [unoptimized + debuginfo] target(s) in 5.00s".to_string(),
            "     Running unittests src/lib.rs".to_string(),
            String::new(),
            "running 15 tests".to_string(),
        ];
        for i in 0..15 {
            lines.push(format!("test tests::test_case_{i} ... ok"));
        }
        lines.push(String::new());
        lines.push("test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s".to_string());
        let input = lines.join("\n");
        let result = filter_test_output(&input);
        // Compilation output should be preserved
        assert!(
            result.contains("Compiling myapp"),
            "should keep compilation output, got: {result}"
        );
        // Passing tests should be omitted
        assert!(
            result.contains("passing tests omitted"),
            "should omit passing tests, got: {result}"
        );
        // Summary should be preserved
        assert!(
            result.contains("test result: ok."),
            "should keep test summary, got: {result}"
        );
    }

    #[test]
    fn test_compress_tool_output_integrates_test_filter() {
        // Verify compress_tool_output calls the test filter
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("\x1b[32mtest test_{i} ... ok\x1b[0m"));
        }
        lines.push(String::new());
        lines.push("\x1b[32mtest result: ok. 10 passed; 0 failed; 0 ignored\x1b[0m".to_string());
        let input = lines.join("\n");
        let result = compress_tool_output(&input);
        // Should have stripped ANSI AND filtered test output
        assert!(!result.contains("\x1b"), "should strip ANSI, got: {result}");
        assert!(
            result.contains("passing tests omitted"),
            "should filter test output, got: {result}"
        );
    }
}
