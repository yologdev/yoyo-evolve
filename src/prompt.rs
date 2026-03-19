//! Prompt execution and agent interaction.

use crate::cli::is_verbose;
use crate::format::*;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use yoagent::agent::Agent;
use yoagent::*;

/// Tracks files modified during a session via write_file and edit_file tool calls.
/// Thread-safe via Arc<Mutex<...>> so it can be shared across async tasks.
#[derive(Debug, Clone)]
pub struct SessionChanges {
    inner: Arc<Mutex<Vec<FileChange>>>,
}

/// A single file modification event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

/// The kind of file modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Write,
    Edit,
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeKind::Write => write!(f, "write"),
            ChangeKind::Edit => write!(f, "edit"),
        }
    }
}

impl SessionChanges {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record a file modification.
    pub fn record(&self, path: &str, kind: ChangeKind) {
        let mut changes = self.inner.lock().unwrap();
        // Update existing entry if same path, or add new
        if let Some(existing) = changes.iter_mut().find(|c| c.path == path) {
            existing.kind = kind;
        } else {
            changes.push(FileChange {
                path: path.to_string(),
                kind,
            });
        }
    }

    /// Get a snapshot of all changes, in order of first modification.
    pub fn snapshot(&self) -> Vec<FileChange> {
        self.inner.lock().unwrap().clone()
    }

    /// Return the number of unique files changed.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Return true if no files have been changed.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Clear all tracked changes.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

/// Outcome of a prompt execution, including the text response and any tool error.
#[derive(Debug, Clone, Default)]
pub struct PromptOutcome {
    /// The collected text output from the agent.
    pub text: String,
    /// The last tool error encountered during this prompt turn, if any.
    /// Tool errors are from `ToolExecutionEnd` events where `is_error` is true.
    pub last_tool_error: Option<String>,
}

/// Build a retry prompt that includes error context from a previous failed attempt.
///
/// If `last_error` is `Some`, prepends an error context note to help the model
/// avoid repeating the same mistake. If `None`, returns the input unchanged.
pub fn build_retry_prompt(input: &str, last_error: &Option<String>) -> String {
    match last_error {
        Some(err) => {
            // Truncate very long errors to keep the prompt focused
            let summary = if err.len() > 200 {
                format!("{}…", &err[..200])
            } else {
                err.clone()
            };
            format!("[Previous attempt failed: {summary}. Try a different approach.]\n\n{input}")
        }
        None => input.to_string(),
    }
}

/// Maximum number of automatic retries for transient API errors.
const MAX_RETRIES: u32 = 3;

/// Calculate exponential backoff delay for a given retry attempt (1-indexed).
/// Returns 1s, 2s, 4s for attempts 1, 2, 3.
pub fn retry_delay(attempt: u32) -> Duration {
    Duration::from_secs(1 << (attempt.saturating_sub(1)))
}

/// Classify whether an API error message looks transient (worth retrying).
/// Retries: rate limits (429), server errors (5xx), network/connection issues, overloaded.
/// Does NOT retry: auth errors (401/403), invalid requests (400), permission denied.
pub fn is_retriable_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();

    // Don't retry auth or client errors
    let non_retriable = [
        "401",
        "403",
        "400",
        "authentication",
        "unauthorized",
        "forbidden",
        "invalid api key",
        "invalid request",
        "permission denied",
        "invalid_api_key",
        "not_found",
        "404",
    ];
    for keyword in &non_retriable {
        if lower.contains(keyword) {
            return false;
        }
    }

    // Retry on transient errors
    let retriable = [
        "429",
        "rate limit",
        "rate_limit",
        "too many requests",
        "500",
        "502",
        "503",
        "504",
        "internal server error",
        "bad gateway",
        "service unavailable",
        "gateway timeout",
        "overloaded",
        "connection",
        "timeout",
        "timed out",
        "network",
        "temporarily",
        "retry",
        "capacity",
        "server error",
    ];
    for keyword in &retriable {
        if lower.contains(keyword) {
            return true;
        }
    }

    false
}

/// Extract a preview of tool result content for display.
/// Returns an empty string if there's nothing meaningful to show.
fn tool_result_preview(result: &ToolResult, max_chars: usize) -> String {
    let text: String = result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    // Take first line only, truncated
    let first_line = text.lines().next().unwrap_or("");
    truncate_with_ellipsis(first_line, max_chars)
}

/// Write response text to a file if --output was specified.
pub fn write_output_file(path: &Option<String>, text: &str) {
    if let Some(path) = path {
        match std::fs::write(path, text) {
            Ok(_) => eprintln!("{DIM}  wrote response to {path}{RESET}"),
            Err(e) => eprintln!("{RED}  error writing to {path}: {e}{RESET}"),
        }
    }
}

/// Extract all searchable text from a message (for /search).
fn message_text(msg: &AgentMessage) -> String {
    match msg {
        AgentMessage::Llm(Message::User { content, .. }) => content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        AgentMessage::Llm(Message::Assistant { content, .. }) => {
            let mut parts = Vec::new();
            for c in content {
                match c {
                    Content::Text { text } if !text.is_empty() => parts.push(text.as_str()),
                    Content::ToolCall { name, .. } => parts.push(name.as_str()),
                    _ => {}
                }
            }
            parts.join(" ")
        }
        AgentMessage::Llm(Message::ToolResult {
            tool_name, content, ..
        }) => {
            let text: String = content
                .iter()
                .filter_map(|c| match c {
                    Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("{tool_name} {text}")
        }
        AgentMessage::Extension(ext) => ext.role.clone(),
    }
}

/// Highlight all occurrences of `query` in `text` using BOLD ANSI codes (case-insensitive).
/// Returns the text with matching substrings wrapped in BOLD..RESET.
pub fn highlight_matches(text: &str, query: &str) -> String {
    if query.is_empty() {
        return text.to_string();
    }
    let lower_text = text.to_lowercase();
    let lower_query = query.to_lowercase();
    let mut result = String::with_capacity(text.len() + 32);
    let mut last_end = 0;

    for (match_start, _) in lower_text.match_indices(&lower_query) {
        let match_end = match_start + query.len();
        // Append text before this match (unmodified)
        result.push_str(&text[last_end..match_start]);
        // Append the matched portion with BOLD highlighting (preserving original case)
        result.push_str(&format!("{BOLD}{}{RESET}", &text[match_start..match_end]));
        last_end = match_end;
    }
    // Append any remaining text after the last match
    result.push_str(&text[last_end..]);
    result
}

/// Search messages for a query string (case-insensitive).
/// Returns a vec of (index, role, highlighted_preview) for matching messages.
pub fn search_messages(messages: &[AgentMessage], query: &str) -> Vec<(usize, String, String)> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        let text = message_text(msg);
        if text.to_lowercase().contains(&query_lower) {
            let (role, _) = summarize_message(msg);
            // Find match context: show text around the first match
            let lower = text.to_lowercase();
            let match_pos = lower.find(&query_lower).unwrap_or(0);
            let start = match_pos.saturating_sub(20);
            // Get byte-safe boundaries
            let start = text[..start]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            let end = text
                .char_indices()
                .map(|(idx, ch)| idx + ch.len_utf8())
                .find(|&idx| idx >= match_pos + query.len() + 20)
                .unwrap_or(text.len());
            let snippet = &text[start..end];
            let prefix = if start > 0 { "…" } else { "" };
            let suffix = if end < text.len() { "…" } else { "" };
            let preview = format!("{prefix}{snippet}{suffix}");
            let highlighted = highlight_matches(&preview, query);
            results.push((i + 1, role.to_string(), highlighted));
        }
    }

    results
}

/// Summarize a message for /history display.
pub fn summarize_message(msg: &AgentMessage) -> (&str, String) {
    match msg {
        AgentMessage::Llm(Message::User { content, .. }) => {
            let text = content
                .iter()
                .filter_map(|c| match c {
                    Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            ("user", truncate_with_ellipsis(&text, 80))
        }
        AgentMessage::Llm(Message::Assistant { content, .. }) => {
            let mut parts = Vec::new();
            let mut tool_calls = 0;
            for c in content {
                match c {
                    Content::Text { text } if !text.is_empty() => {
                        parts.push(truncate_with_ellipsis(text, 60));
                    }
                    Content::ToolCall { name, .. } => {
                        tool_calls += 1;
                        if tool_calls <= 3 {
                            parts.push(format!("→{name}"));
                        }
                    }
                    _ => {}
                }
            }
            if tool_calls > 3 {
                parts.push(format!("(+{} more tools)", tool_calls - 3));
            }
            let preview = if parts.is_empty() {
                "(empty)".to_string()
            } else {
                parts.join("  ")
            };
            ("assistant", preview)
        }
        AgentMessage::Llm(Message::ToolResult {
            tool_name,
            is_error,
            ..
        }) => {
            let status = if *is_error { "✗" } else { "✓" };
            ("tool", format!("{tool_name} {status}"))
        }
        AgentMessage::Extension(ext) => ("ext", truncate_with_ellipsis(&ext.role, 60)),
    }
}

/// Result of a single prompt attempt — either success or a retriable/fatal error.
enum PromptResult {
    /// Prompt completed (possibly with non-retriable errors already shown).
    Done {
        collected_text: String,
        usage: Usage,
        last_tool_error: Option<String>,
    },
    /// A retriable API error was detected — caller should retry.
    RetriableError { error_msg: String, usage: Usage },
}

/// Execute a single prompt attempt and process all events.
/// Returns whether we got a retriable error (so the caller can retry).
async fn run_prompt_once(agent: &mut Agent, input: &str, changes: &SessionChanges) -> PromptResult {
    let mut rx = agent.prompt(input).await;
    let mut usage = Usage::default();
    let mut in_text = false;
    let mut tool_timers: HashMap<String, Instant> = HashMap::new();
    let mut collected_text = String::new();
    let mut retriable_error: Option<String> = None;
    let mut last_tool_error: Option<String> = None;
    let mut md_renderer = MarkdownRenderer::new();
    let mut spinner: Option<Spinner> = Some(Spinner::start());

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    AgentEvent::ToolExecutionStart {
                        tool_call_id, tool_name, args, ..
                    } => {
                        // Track file modifications from write_file and edit_file
                        match tool_name.as_str() {
                            "write_file" => {
                                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                                    changes.record(path, ChangeKind::Write);
                                }
                            }
                            "edit_file" => {
                                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                                    changes.record(path, ChangeKind::Edit);
                                }
                            }
                            _ => {}
                        }
                        // Stop spinner on first activity
                        if let Some(s) = spinner.take() { s.stop(); }
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        tool_timers.insert(tool_call_id.clone(), Instant::now());
                        let summary = format_tool_summary(&tool_name, &args);
                        print!("{YELLOW}  ▶ {summary}{RESET}");
                        if is_verbose() {
                            println!();
                            let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                            for line in args_str.lines() {
                                println!("{DIM}    {line}{RESET}");
                            }
                        } else if tool_name == "edit_file" {
                            // Show colored diff for edit_file when not in verbose mode
                            let old_text = args.get("old_text").and_then(|v| v.as_str()).unwrap_or("");
                            let new_text = args.get("new_text").and_then(|v| v.as_str()).unwrap_or("");
                            let diff = format_edit_diff(old_text, new_text);
                            if !diff.is_empty() {
                                println!();
                                println!("{diff}");
                            }
                        }
                        io::stdout().flush().ok();
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, result, .. } => {
                        let duration = tool_timers
                            .remove(&tool_call_id)
                            .map(|start| format_duration(start.elapsed()));
                        let dur_str = duration
                            .map(|d| format!(" {DIM}({d}){RESET}"))
                            .unwrap_or_default();
                        if is_error {
                            println!(" {RED}✗{RESET}{dur_str}");
                            let preview = tool_result_preview(&result, 200);
                            if !preview.is_empty() {
                                println!("{DIM}    {preview}{RESET}");
                            }
                            // Track the last tool error for /retry context
                            let error_text = tool_result_preview(&result, 200);
                            if !error_text.is_empty() {
                                last_tool_error = Some(error_text);
                            } else {
                                last_tool_error = Some("tool execution failed".to_string());
                            }
                        } else {
                            // Successful tool clears the last error
                            last_tool_error = None;
                            println!(" {GREEN}✓{RESET}{dur_str}");
                            if is_verbose() {
                                let preview = tool_result_preview(&result, 200);
                                if !preview.is_empty() {
                                    println!("{DIM}    {preview}{RESET}");
                                }
                            }
                        }
                    }
                    AgentEvent::ToolExecutionUpdate { partial_result, .. } => {
                        let preview = tool_result_preview(&partial_result, 500);
                        if !preview.is_empty() {
                            print!("{DIM}{preview}{RESET}");
                            io::stdout().flush().ok();
                        }
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Text { delta },
                        ..
                    } => {
                        // Stop spinner on first text
                        if let Some(s) = spinner.take() { s.stop(); }
                        if !in_text {
                            println!();
                            in_text = true;
                        }
                        collected_text.push_str(&delta);
                        let rendered = md_renderer.render_delta(&delta);
                        if !rendered.is_empty() {
                            print!("{}", rendered);
                        }
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Thinking { delta },
                        ..
                    } => {
                        // Stop spinner on first thinking output
                        if let Some(s) = spinner.take() { s.stop(); }
                        print!("{DIM}{delta}{RESET}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::AgentEnd { messages } => {
                        // Stop spinner if still running
                        if let Some(s) = spinner.take() { s.stop(); }
                        for msg in &messages {
                            if let AgentMessage::Llm(Message::Assistant { usage: msg_usage, stop_reason, error_message, .. }) = msg {
                                usage.input += msg_usage.input;
                                usage.output += msg_usage.output;
                                usage.cache_read += msg_usage.cache_read;
                                usage.cache_write += msg_usage.cache_write;

                                if *stop_reason == StopReason::Error {
                                    if let Some(err_msg) = error_message {
                                        if in_text {
                                            println!();
                                            in_text = false;
                                        }
                                        // Check if this error is worth retrying
                                        if is_retriable_error(err_msg) {
                                            retriable_error = Some(err_msg.clone());
                                        } else {
                                            eprintln!("\n{RED}  error: {err_msg}{RESET}");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    AgentEvent::InputRejected { reason } => {
                        if let Some(s) = spinner.take() { s.stop(); }
                        eprintln!("{RED}  input rejected: {reason}{RESET}");
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        if let Some(s) = spinner.take() { s.stop(); }
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        println!("{DIM}  {text}{RESET}");
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                // Stop spinner if still running
                if let Some(s) = spinner.take() { s.stop(); }
                agent.abort();
                if in_text {
                    println!();
                }
                println!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}");
                return PromptResult::Done {
                    collected_text,
                    usage,
                    last_tool_error,
                };
            }
        }
    }

    // Stop spinner if still running (e.g., channel closed without events)
    if let Some(s) = spinner.take() {
        s.stop();
    }

    // Flush any remaining buffered markdown content
    let remaining = md_renderer.flush();
    if !remaining.is_empty() {
        print!("{}", remaining);
        io::stdout().flush().ok();
    }

    if in_text {
        println!();
    }

    if let Some(err_msg) = retriable_error {
        PromptResult::RetriableError {
            error_msg: err_msg,
            usage,
        }
    } else {
        PromptResult::Done {
            collected_text,
            usage,
            last_tool_error,
        }
    }
}

pub async fn run_prompt(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) -> PromptOutcome {
    // Default: create a throwaway changes tracker (for callers that don't need tracking)
    let changes = SessionChanges::new();
    run_prompt_with_changes(agent, input, session_total, model, &changes).await
}

/// Run a prompt with file change tracking.
/// Like `run_prompt`, but records write_file/edit_file calls into the given tracker.
pub async fn run_prompt_with_changes(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
    changes: &SessionChanges,
) -> PromptOutcome {
    let prompt_start = Instant::now();
    let mut total_usage = Usage::default();
    let mut collected_text = String::new();
    let mut last_tool_error: Option<String> = None;

    // Save message state before the first attempt so we can restore on retry
    let saved_state = agent.save_messages().ok();

    for attempt in 0..=MAX_RETRIES {
        // On retry, restore pre-prompt state so we don't duplicate the user message
        if attempt > 0 {
            if let Some(ref json) = saved_state {
                let _ = agent.restore_messages(json);
            }
        }

        match run_prompt_once(agent, input, changes).await {
            PromptResult::Done {
                collected_text: text,
                usage,
                last_tool_error: tool_err,
            } => {
                total_usage.input += usage.input;
                total_usage.output += usage.output;
                total_usage.cache_read += usage.cache_read;
                total_usage.cache_write += usage.cache_write;
                collected_text = text;
                last_tool_error = tool_err;
                break;
            }
            PromptResult::RetriableError { error_msg, usage } => {
                total_usage.input += usage.input;
                total_usage.output += usage.output;
                total_usage.cache_read += usage.cache_read;
                total_usage.cache_write += usage.cache_write;

                if attempt < MAX_RETRIES {
                    let delay = retry_delay(attempt + 1);
                    let delay_secs = delay.as_secs();
                    let next = attempt + 2; // human-readable attempt number
                    eprintln!(
                        "{DIM}  ⚡ retrying (attempt {next}/{}, waiting {delay_secs}s)...{RESET}",
                        MAX_RETRIES + 1
                    );
                    tokio::time::sleep(delay).await;
                } else {
                    // Exhausted all retries — show the final error
                    eprintln!("\n{RED}  error: {error_msg}{RESET}");
                    eprintln!("{DIM}  (failed after {} attempts){RESET}", MAX_RETRIES + 1);
                }
            }
        }
    }

    session_total.input += total_usage.input;
    session_total.output += total_usage.output;
    session_total.cache_read += total_usage.cache_read;
    session_total.cache_write += total_usage.cache_write;
    print_usage(&total_usage, session_total, model, prompt_start.elapsed());
    println!();
    PromptOutcome {
        text: collected_text,
        last_tool_error,
    }
}

/// Format the list of session changes for display.
/// Returns an empty string if no changes have been recorded.
pub fn format_changes(changes: &SessionChanges) -> String {
    let snapshot = changes.snapshot();
    if snapshot.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!(
        "  {} {} modified this session:\n",
        snapshot.len(),
        pluralize(snapshot.len(), "file", "files")
    ));
    for change in &snapshot {
        let icon = match change.kind {
            ChangeKind::Write => "✏",
            ChangeKind::Edit => "🔧",
        };
        out.push_str(&format!("    {icon} {} ({})\n", change.path, change.kind));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_delay_exponential_backoff() {
        // attempt 1 → 1s, attempt 2 → 2s, attempt 3 → 4s
        assert_eq!(retry_delay(1), Duration::from_secs(1));
        assert_eq!(retry_delay(2), Duration::from_secs(2));
        assert_eq!(retry_delay(3), Duration::from_secs(4));
    }

    #[test]
    fn test_retry_delay_zero_attempt() {
        // Edge case: attempt 0 should still return 1s (saturating_sub prevents underflow)
        assert_eq!(retry_delay(0), Duration::from_secs(1));
    }

    #[test]
    fn test_is_retriable_rate_limit() {
        assert!(is_retriable_error("429 Too Many Requests"));
        assert!(is_retriable_error("rate limit exceeded"));
        assert!(is_retriable_error("Rate_limit_error: too many requests"));
        assert!(is_retriable_error("too many requests, please slow down"));
    }

    #[test]
    fn test_is_retriable_server_errors() {
        assert!(is_retriable_error("500 Internal Server Error"));
        assert!(is_retriable_error("502 Bad Gateway"));
        assert!(is_retriable_error("503 Service Unavailable"));
        assert!(is_retriable_error("504 Gateway Timeout"));
        assert!(is_retriable_error("the server is overloaded"));
        assert!(is_retriable_error("Server error occurred"));
    }

    #[test]
    fn test_is_retriable_network_errors() {
        assert!(is_retriable_error("connection reset by peer"));
        assert!(is_retriable_error("network error: connection refused"));
        assert!(is_retriable_error("request timed out"));
        assert!(is_retriable_error("timeout waiting for response"));
    }

    #[test]
    fn test_is_not_retriable_auth_errors() {
        assert!(!is_retriable_error("401 Unauthorized"));
        assert!(!is_retriable_error("403 Forbidden"));
        assert!(!is_retriable_error("authentication failed"));
        assert!(!is_retriable_error("invalid api key"));
        assert!(!is_retriable_error("Invalid_api_key: check your key"));
        assert!(!is_retriable_error("permission denied"));
    }

    #[test]
    fn test_is_not_retriable_client_errors() {
        assert!(!is_retriable_error("400 Bad Request"));
        assert!(!is_retriable_error("invalid request body"));
        assert!(!is_retriable_error("404 not_found"));
    }

    #[test]
    fn test_is_not_retriable_unknown_error() {
        // Unknown errors without retriable keywords should NOT be retried
        assert!(!is_retriable_error("something went wrong"));
        assert!(!is_retriable_error("unexpected error"));
    }

    #[test]
    fn test_summarize_message_user() {
        let msg = AgentMessage::Llm(Message::user("hello world, this is a test"));
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "user");
        assert!(preview.contains("hello world"));
    }

    #[test]
    fn test_summarize_message_tool_result() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "output".into(),
            }],
            is_error: false,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("bash"));
        assert!(preview.contains("✓"));
    }

    #[test]
    fn test_summarize_message_tool_result_error() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_2".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "error".into(),
            }],
            is_error: true,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("✗"));
    }

    #[test]
    fn test_write_output_file_none() {
        write_output_file(&None, "test content");
        // No assertion needed — just verify it doesn't panic
    }

    #[test]
    fn test_write_output_file_some() {
        let dir = std::env::temp_dir().join("yoyo_test_output");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_output.txt");
        let path_str = path.to_string_lossy().to_string();
        write_output_file(&Some(path_str), "hello from yoyo");
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello from yoyo");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_tool_result_preview_empty() {
        let result = ToolResult {
            content: vec![],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "");
    }

    #[test]
    fn test_tool_result_preview_text() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "error: file not found".into(),
            }],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "error: file not found");
    }

    #[test]
    fn test_tool_result_preview_truncated() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "a".repeat(200),
            }],
            details: serde_json::json!(null),
        };
        let preview = tool_result_preview(&result, 50);
        assert!(preview.len() < 100);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn test_tool_result_preview_multiline() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "first line\nsecond line\nthird line".into(),
            }],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "first line");
    }

    #[test]
    fn test_search_messages_basic_match() {
        let messages = vec![
            AgentMessage::Llm(Message::user("hello world")),
            AgentMessage::Llm(Message::user("goodbye world")),
        ];
        let results = search_messages(&messages, "hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1); // 1-indexed
        assert_eq!(results[0].1, "user");
        assert!(results[0].2.contains("hello"));
    }

    #[test]
    fn test_search_messages_case_insensitive() {
        let messages = vec![AgentMessage::Llm(Message::user("Hello World"))];
        let results = search_messages(&messages, "hello");
        assert_eq!(results.len(), 1);
        let results2 = search_messages(&messages, "HELLO");
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn test_search_messages_no_match() {
        let messages = vec![AgentMessage::Llm(Message::user("hello world"))];
        let results = search_messages(&messages, "foobar");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_messages_empty_messages() {
        let messages: Vec<AgentMessage> = vec![];
        let results = search_messages(&messages, "anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_messages_multiple_matches() {
        let messages = vec![
            AgentMessage::Llm(Message::user("the rust language")),
            AgentMessage::Llm(Message::user("python is great")),
            AgentMessage::Llm(Message::user("rust is fast")),
        ];
        let results = search_messages(&messages, "rust");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1);
        assert_eq!(results[1].0, 3);
    }

    #[test]
    fn test_search_messages_tool_result() {
        let messages = vec![AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "cargo build succeeded".into(),
            }],
            is_error: false,
            timestamp: 0,
        })];
        let results = search_messages(&messages, "cargo");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "tool");
    }

    #[test]
    fn test_message_text_user() {
        let msg = AgentMessage::Llm(Message::user("test input"));
        let text = message_text(&msg);
        assert_eq!(text, "test input");
    }

    #[test]
    fn test_message_text_tool_result() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "output text".into(),
            }],
            is_error: false,
            timestamp: 0,
        });
        let text = message_text(&msg);
        assert!(text.contains("bash"));
        assert!(text.contains("output text"));
    }

    // --- highlight_matches tests ---

    #[test]
    fn test_highlight_matches_basic() {
        let result = highlight_matches("hello world", "world");
        assert!(result.contains(&format!("{BOLD}world{RESET}")));
        assert!(result.contains("hello "));
    }

    #[test]
    fn test_highlight_matches_case_insensitive() {
        let result = highlight_matches("Hello World", "hello");
        assert!(result.contains(&format!("{BOLD}Hello{RESET}")));
    }

    #[test]
    fn test_highlight_matches_multiple_occurrences() {
        let result = highlight_matches("rust is fast, rust is safe", "rust");
        // Should highlight both occurrences
        let bold_rust = format!("{BOLD}rust{RESET}");
        let count = result.matches(&bold_rust.to_string()).count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_highlight_matches_no_match() {
        let result = highlight_matches("hello world", "foobar");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_highlight_matches_empty_query() {
        let result = highlight_matches("hello world", "");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_highlight_matches_empty_text() {
        let result = highlight_matches("", "query");
        assert_eq!(result, "");
    }

    #[test]
    fn test_highlight_matches_preserves_original_case() {
        let result = highlight_matches("The Rust Language", "rust");
        // Should wrap "Rust" (original case), not "rust"
        assert!(result.contains(&format!("{BOLD}Rust{RESET}")));
    }

    #[test]
    fn test_highlight_matches_entire_string() {
        let result = highlight_matches("hello", "hello");
        assert_eq!(result, format!("{BOLD}hello{RESET}"));
    }

    #[test]
    fn test_search_messages_results_are_highlighted() {
        let messages = vec![AgentMessage::Llm(Message::user("hello world"))];
        let results = search_messages(&messages, "hello");
        assert_eq!(results.len(), 1);
        // The preview should contain BOLD highlighting around "hello"
        assert!(results[0].2.contains(&format!("{BOLD}hello{RESET}")));
    }

    // --- SessionChanges tests ---

    #[test]
    fn test_session_changes_new_is_empty() {
        let changes = SessionChanges::new();
        assert!(changes.is_empty());
        assert_eq!(changes.len(), 0);
        assert!(changes.snapshot().is_empty());
    }

    #[test]
    fn test_session_changes_record_write() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        assert_eq!(changes.len(), 1);
        assert!(!changes.is_empty());
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].path, "src/main.rs");
        assert_eq!(snapshot[0].kind, ChangeKind::Write);
    }

    #[test]
    fn test_session_changes_record_edit() {
        let changes = SessionChanges::new();
        changes.record("src/cli.rs", ChangeKind::Edit);
        assert_eq!(changes.len(), 1);
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].path, "src/cli.rs");
        assert_eq!(snapshot[0].kind, ChangeKind::Edit);
    }

    #[test]
    fn test_session_changes_deduplicates_same_path() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/main.rs", ChangeKind::Edit);
        // Should still be 1 entry, updated to Edit
        assert_eq!(changes.len(), 1);
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].kind, ChangeKind::Edit);
    }

    #[test]
    fn test_session_changes_multiple_files() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        changes.record("README.md", ChangeKind::Write);
        assert_eq!(changes.len(), 3);
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].path, "src/main.rs");
        assert_eq!(snapshot[1].path, "src/cli.rs");
        assert_eq!(snapshot[2].path, "README.md");
    }

    #[test]
    fn test_session_changes_clear() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        assert_eq!(changes.len(), 2);
        changes.clear();
        assert!(changes.is_empty());
        assert_eq!(changes.len(), 0);
    }

    #[test]
    fn test_session_changes_clone_is_independent() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        let cloned = changes.clone();
        // They share the same inner Arc, so they should be linked
        changes.record("src/cli.rs", ChangeKind::Edit);
        assert_eq!(cloned.len(), 2);
    }

    #[test]
    fn test_change_kind_display() {
        assert_eq!(format!("{}", ChangeKind::Write), "write");
        assert_eq!(format!("{}", ChangeKind::Edit), "edit");
    }

    #[test]
    fn test_format_changes_empty() {
        let changes = SessionChanges::new();
        let output = format_changes(&changes);
        assert!(output.is_empty());
    }

    #[test]
    fn test_format_changes_single_write() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        let output = format_changes(&changes);
        assert!(output.contains("1 file modified"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("write"));
        assert!(output.contains("✏"));
    }

    #[test]
    fn test_format_changes_multiple_files() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        let output = format_changes(&changes);
        assert!(output.contains("2 files modified"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/cli.rs"));
        assert!(output.contains("write"));
        assert!(output.contains("edit"));
        assert!(output.contains("🔧"));
    }
}
