//! Spinner, ToolProgressTimer, ThinkBlockFilter.

use super::*;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use yoagent::types::{Content, ToolResult};

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

/// Maximum display length for a tool progress label (command preview).
const TOOL_LABEL_MAX_CHARS: usize = 40;

/// Format a live progress line for a running tool.
///
/// Shows spinner frame, tool name, optional label (e.g. command), elapsed time,
/// and optional line count.
/// Examples:
/// - Without label: `  ⠹ bash ⏱ 12s`
/// - With label: `  ⠹ bash: ls -la src/ ⏱ 12s`
/// - With label + lines: `  ⠹ bash: cargo test ⏱ 1m 5s ─ 142 lines captured`
pub fn format_tool_progress(
    tool_name: &str,
    elapsed: Duration,
    tick: usize,
    line_count: Option<usize>,
    label: Option<&str>,
) -> String {
    let frame = spinner_frame(tick);
    let time_str = format_duration_live(elapsed);
    let lines_str = match line_count {
        Some(n) if n > 0 => {
            let word = pluralize(n, "line", "lines");
            format!(" ─ {n} {word} captured")
        }
        _ => String::new(),
    };
    let label_str = match label {
        Some(l) if !l.is_empty() => {
            let truncated = truncate_with_ellipsis(l, TOOL_LABEL_MAX_CHARS);
            format!(": {truncated}")
        }
        _ => String::new(),
    };
    format!("{DIM}  {frame} {tool_name}{label_str} ⏱ {time_str}{lines_str}{RESET}")
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
        let shown = tail.len();
        result.push_str(&format!(
            "{DIM}    │ (showing last {shown} of {total} lines){RESET}\n"
        ));
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

/// A handle to a running tool-progress timer task.
/// Shows `  ⠹ bash ⏱ 12s` on stderr, updating every second.
/// Optionally shows a label (e.g. command being run): `  ⠹ bash: ls -la ⏱ 12s`
/// Dropping or calling `stop()` cancels it and clears the line.
pub struct ToolProgressTimer {
    cancel: tokio::sync::watch::Sender<bool>,
    line_count: Arc<std::sync::atomic::AtomicUsize>,
    label: Arc<std::sync::Mutex<Option<String>>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ToolProgressTimer {
    /// Start a timer that shows elapsed time for a tool on stderr.
    /// Updates every second with the current line count.
    pub fn start(tool_name: String) -> Self {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let line_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let line_count_clone = Arc::clone(&line_count);
        let label: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
        let label_clone = Arc::clone(&label);
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
                let lbl = label_clone.lock().ok().and_then(|g| g.clone());
                let progress =
                    format_tool_progress(&tool_name, elapsed, tick, lc_opt, lbl.as_deref());
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
            label,
            handle: Some(handle),
        }
    }

    /// Update the line count shown in the timer display.
    pub fn set_line_count(&self, count: usize) {
        self.line_count
            .store(count, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set a label (e.g. command name) to display alongside the tool name.
    /// The label is truncated to ~40 chars in the display.
    pub fn set_label(&self, label: String) {
        if let Ok(mut guard) = self.label.lock() {
            *guard = Some(label);
        }
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

// ── Think block filter ───────────────────────────────────────────────────
// Filters `<think>...</think>` blocks from streamed text deltas.
// Some models emit reasoning as raw text (not the Thinking stream),
// and we don't want that XML leaking into the user-visible output.

/// State machine for filtering `<think>...</think>` blocks from streamed text.
/// Returns the text that should be displayed (everything outside think blocks).
pub struct ThinkBlockFilter {
    in_block: bool,
    buffer: String,
}

impl ThinkBlockFilter {
    pub fn new() -> Self {
        Self {
            in_block: false,
            buffer: String::new(),
        }
    }

    /// Process a text delta, returning only the visible (non-think) portion.
    pub fn filter(&mut self, delta: &str) -> String {
        let mut result = String::new();
        self.buffer.push_str(delta);

        loop {
            if self.in_block {
                // Look for </think>
                if let Some(end_pos) = self.buffer.find("</think>") {
                    // Skip everything up to and including </think>
                    self.buffer = self.buffer[end_pos + 8..].to_string();
                    self.in_block = false;
                } else if self.buffer.ends_with('<')
                    || self.buffer.ends_with("</")
                    || self.buffer.ends_with("</t")
                    || self.buffer.ends_with("</th")
                    || self.buffer.ends_with("</thi")
                    || self.buffer.ends_with("</thin")
                    || self.buffer.ends_with("</think")
                {
                    // Might be a partial </think> — keep buffering
                    break;
                } else {
                    // No closing tag possibility — discard buffer
                    self.buffer.clear();
                    break;
                }
            } else {
                // Look for <think>
                if let Some(start_pos) = self.buffer.find("<think>") {
                    // Emit everything before <think>
                    result.push_str(&self.buffer[..start_pos]);
                    self.buffer = self.buffer[start_pos + 7..].to_string();
                    self.in_block = true;
                } else if self.buffer.ends_with('<')
                    || self.buffer.ends_with("<t")
                    || self.buffer.ends_with("<th")
                    || self.buffer.ends_with("<thi")
                    || self.buffer.ends_with("<thin")
                    || self.buffer.ends_with("<think")
                {
                    // Might be a partial <think> — emit everything before the '<'
                    if let Some(lt_pos) = self.buffer.rfind('<') {
                        result.push_str(&self.buffer[..lt_pos]);
                        self.buffer = self.buffer[lt_pos..].to_string();
                    }
                    break;
                } else {
                    // No tag possibility — emit all
                    result.push_str(&self.buffer);
                    self.buffer.clear();
                    break;
                }
            }
        }
        result
    }

    /// Flush any remaining buffered text (call at end of stream).
    pub fn flush(&mut self) -> String {
        let remaining = std::mem::take(&mut self.buffer);
        if self.in_block {
            String::new() // Still inside think block — discard
        } else {
            remaining // Partial tag that never completed — emit as-is
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        let output = format_tool_progress("bash", Duration::from_secs(5), 0, None, None);
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
        let output = format_tool_progress("bash", Duration::from_secs(12), 3, Some(142), None);
        assert!(output.contains("bash"), "should contain tool name");
        assert!(output.contains("12s"), "should contain elapsed time");
        assert!(
            output.contains("─ 142 lines captured"),
            "should contain line count with dash separator"
        );
    }

    #[test]
    fn test_format_tool_progress_single_line() {
        let output = format_tool_progress("bash", Duration::from_secs(1), 0, Some(1), None);
        assert!(
            output.contains("─ 1 line captured"),
            "should use singular 'line'"
        );
        assert!(!output.contains("1 lines"), "should not use plural for 1");
    }

    #[test]
    fn test_format_tool_progress_zero_lines_hidden() {
        let output = format_tool_progress("bash", Duration::from_secs(3), 0, Some(0), None);
        assert!(!output.contains("line"), "zero lines should be hidden");
    }

    #[test]
    fn test_format_tool_progress_with_label() {
        let output = format_tool_progress(
            "bash",
            Duration::from_secs(5),
            0,
            Some(42),
            Some("ls -la src/"),
        );
        assert!(output.contains("bash"), "should contain tool name");
        assert!(
            output.contains(": ls -la src/"),
            "should contain label after colon"
        );
        assert!(output.contains("5s"), "should contain elapsed time");
        assert!(
            output.contains("─ 42 lines captured"),
            "should contain line count"
        );
    }

    #[test]
    fn test_format_tool_progress_label_truncation() {
        let long_cmd = "cargo test --release --features all-the-things -- some::very::long::test::path::that::goes::on::forever";
        let output = format_tool_progress("bash", Duration::from_secs(10), 0, None, Some(long_cmd));
        // The label should be truncated (40 char limit + ellipsis)
        assert!(output.contains("bash"), "should contain tool name");
        assert!(output.contains(": "), "should contain colon separator");
        // Should NOT contain the full command
        assert!(!output.contains(long_cmd), "should truncate long labels");
        // Should contain the ellipsis character from truncation
        assert!(
            output.contains('…'),
            "should contain ellipsis for truncation"
        );
    }

    #[test]
    fn test_format_tool_progress_empty_label_ignored() {
        let output = format_tool_progress("bash", Duration::from_secs(3), 0, None, Some(""));
        // Empty label should not produce a colon separator
        assert!(!output.contains(": "), "empty label should not show colon");
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
        assert!(
            output.contains("showing last 2 of 5 lines"),
            "should show truncation header"
        );
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
    fn test_format_partial_tail_truncation_header_with_six_lines() {
        // Simulate what the live display now shows (6 lines from a longer output)
        let input = (1..=20)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let output = format_partial_tail(&input, 6);
        assert!(
            output.contains("showing last 6 of 20 lines"),
            "should show truncation header for 20-line output with max 6"
        );
        assert!(output.contains("line15"), "should show 6th-from-last");
        assert!(output.contains("line20"), "should show last line");
        assert!(
            !output.contains("line14"),
            "should not show lines before window"
        );
    }

    #[test]
    fn test_format_partial_tail_no_header_when_all_fit() {
        let output = format_partial_tail("a\nb\nc", 6);
        assert!(
            !output.contains("showing last"),
            "no header when all lines fit"
        );
        assert!(output.contains("a"), "should show first line");
        assert!(output.contains("c"), "should show last line");
    }

    #[test]
    fn test_format_partial_tail_exact_match_no_header() {
        let output = format_partial_tail("a\nb\nc", 3);
        assert!(
            !output.contains("showing last"),
            "no header when lines == max_lines"
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

    // ── Streaming contract tests ──
    //
    // These tests document and lock in the current behavior of the streaming
    // pipeline (MarkdownRenderer::render_delta + flush). They exist to prevent
    // regressions when modifying the renderer. Each test describes a specific
    // contract about when content is buffered vs. emitted immediately.

    #[test]
    fn test_think_filter_simple_block() {
        let mut f = ThinkBlockFilter::new();
        let out = f.filter("Hello <think>reasoning</think> World");
        assert_eq!(out, "Hello  World");
    }

    #[test]
    fn test_think_filter_no_block() {
        let mut f = ThinkBlockFilter::new();
        let out = f.filter("Hello World");
        assert_eq!(out, "Hello World");
    }

    #[test]
    fn test_think_filter_streaming_split() {
        let mut f = ThinkBlockFilter::new();
        let out1 = f.filter("Hello <thi");
        assert_eq!(out1, "Hello ");
        let out2 = f.filter("nk>secret</think> World");
        assert_eq!(out2, " World");
    }

    #[test]
    fn test_think_filter_nested_or_repeated() {
        let mut f = ThinkBlockFilter::new();
        let out = f.filter("A<think>x</think>B<think>y</think>C");
        assert_eq!(out, "ABC");
    }

    #[test]
    fn test_think_filter_partial_at_end() {
        // Buffer has partial "<thi" that never completes — flush emits it as-is
        let mut f = ThinkBlockFilter::new();
        let out1 = f.filter("Hello <thi");
        assert_eq!(out1, "Hello ");
        let flushed = f.flush();
        assert_eq!(flushed, "<thi");
    }

    #[test]
    fn test_think_filter_flush_inside_block() {
        // Flush while inside a think block — discard remaining
        let mut f = ThinkBlockFilter::new();
        let out = f.filter("Hello <think>still going");
        assert_eq!(out, "Hello ");
        let flushed = f.flush();
        assert_eq!(flushed, "");
    }

    #[test]
    fn test_think_filter_empty_input() {
        let mut f = ThinkBlockFilter::new();
        let out = f.filter("");
        assert_eq!(out, "");
        let flushed = f.flush();
        assert_eq!(flushed, "");
    }

    #[test]
    fn test_think_filter_block_at_start() {
        let mut f = ThinkBlockFilter::new();
        let out = f.filter("<think>hidden</think>visible");
        assert_eq!(out, "visible");
    }

    #[test]
    fn test_think_filter_block_at_end() {
        let mut f = ThinkBlockFilter::new();
        let out = f.filter("visible<think>hidden</think>");
        assert_eq!(out, "visible");
    }

    #[test]
    fn test_think_filter_split_closing_tag() {
        // Closing tag split across deltas
        let mut f = ThinkBlockFilter::new();
        let out1 = f.filter("<think>hidden</thi");
        assert_eq!(out1, "");
        let out2 = f.filter("nk>visible");
        assert_eq!(out2, "visible");
    }

    #[test]
    fn test_think_filter_char_by_char() {
        // Simulate extreme token-by-token streaming
        let mut f = ThinkBlockFilter::new();
        let input = "Hi<think>x</think>!";
        let mut collected = String::new();
        for ch in input.chars() {
            collected.push_str(&f.filter(&ch.to_string()));
        }
        collected.push_str(&f.flush());
        assert_eq!(collected, "Hi!");
    }
}
