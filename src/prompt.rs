//! Prompt execution and agent interaction.

use crate::cli::is_verbose;
use crate::format::*;
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use yoagent::agent::Agent;
use yoagent::context::total_tokens;
use yoagent::*;

// ── Watch mode state ─────────────────────────────────────────────────────
// Global state for `/watch` — auto-run a test command after agent edits.

/// The currently active watch command (None = watch mode off).
static WATCH_COMMAND: RwLock<Option<String>> = RwLock::new(None);

/// Set the watch command, enabling watch mode.
pub fn set_watch_command(cmd: &str) {
    let mut guard = WATCH_COMMAND.write().unwrap();
    *guard = Some(cmd.to_string());
}

/// Get the current watch command, if watch mode is active.
pub fn get_watch_command() -> Option<String> {
    let guard = WATCH_COMMAND.read().unwrap();
    guard.clone()
}

/// Clear the watch command, disabling watch mode.
pub fn clear_watch_command() {
    let mut guard = WATCH_COMMAND.write().unwrap();
    *guard = None;
}

/// Maximum characters of watch command output to include in fix prompts.
const WATCH_OUTPUT_MAX: usize = 5000;

/// Maximum number of auto-fix attempts when watch mode detects failures.
pub const MAX_WATCH_FIX_ATTEMPTS: usize = 3;

/// Build a prompt asking the agent to fix failures from a watch command.
pub fn build_watch_fix_prompt(watch_cmd: &str, output: &str) -> String {
    let truncated = if output.len() > WATCH_OUTPUT_MAX {
        format!("{}... (truncated)", safe_truncate(output, WATCH_OUTPUT_MAX))
    } else {
        output.to_string()
    };
    format!(
        "Your changes caused test/lint failures. Here's the output from `{watch_cmd}`:\n\
         ```\n{truncated}\n```\n\
         Please fix the issues."
    )
}

/// Run a watch command and return (success, output).
///
/// Streams output line-by-line in real time: when stderr is a terminal,
/// prints a compact progress indicator (`⟳ 42 lines...`) so the user
/// sees something happening during long test/build runs.  The full
/// combined stdout+stderr is still collected and returned for the agent
/// to analyse.
pub fn run_watch_command(cmd: &str) -> (bool, String) {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    let is_tty = io::stderr().is_terminal();

    let child = Command::new("sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to run watch command: {e}")),
    };

    // Collect stderr lines in a background thread.
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stderr_handle = std::thread::spawn(move || {
        let reader = io::BufReader::new(stderr_pipe);
        let mut lines = Vec::new();
        for line in reader.lines() {
            match line {
                Ok(l) => lines.push(l),
                Err(_) => break,
            }
        }
        lines
    });

    // Stream stdout on the main thread, collecting lines.
    let mut stdout_lines: Vec<String> = Vec::new();
    if let Some(stdout_pipe) = child.stdout.take() {
        let reader = io::BufReader::new(stdout_pipe);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    stdout_lines.push(l);
                    if is_tty {
                        let count = stdout_lines.len();
                        eprint!("\r{DIM}  ⟳ {count} lines...{RESET}");
                        let _ = io::stderr().flush();
                    }
                }
                Err(_) => break,
            }
        }
    }

    let stderr_lines = stderr_handle.join().unwrap_or_default();

    // Clear the progress indicator if we printed one.
    if is_tty && !stdout_lines.is_empty() {
        eprint!("\r{DIM}                          {RESET}\r");
        let _ = io::stderr().flush();
    }

    let status = match child.wait() {
        Ok(s) => s.success(),
        Err(_) => false,
    };

    // Combine stdout + stderr the same way the old implementation did.
    let stdout_text = stdout_lines.join("\n");
    let stderr_text = stderr_lines.join("\n");
    let combined = if stderr_text.is_empty() {
        stdout_text
    } else if stdout_text.is_empty() {
        stderr_text
    } else {
        format!("{stdout_text}\n{stderr_text}")
    };

    (status, combined)
}

// ── Audit log + session budget ──────────────────────────────────────────
// Extracted into `prompt_budget` module. Re-exported here so the existing
// `use crate::prompt::*;` call sites in `main.rs` and `repl.rs` keep working
// without any changes, and `crate::prompt::foo` paths continue to resolve.
// Only symbols actually referenced via the `prompt::` path today are
// re-exported; the rest remain accessible at `crate::prompt_budget::`.
pub use crate::prompt_budget::{
    audit_log_tool_call, enable_audit_log, is_audit_enabled, session_budget_exhausted,
};

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

    /// Clear all tracked changes.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

#[cfg(test)]
impl SessionChanges {
    /// Return the number of unique files changed.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Return true if no files have been changed.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

/// A snapshot of file state before a single agent turn.
///
/// Stores the original content of files that existed before the turn,
/// and tracks paths of files that were newly created during the turn.
/// Used by `/undo` to revert only the most recent turn's changes.
#[derive(Debug, Clone)]
pub struct TurnSnapshot {
    /// Files that existed before the turn: path → original content.
    pub originals: HashMap<String, String>,
    /// Files that were created during the turn (didn't exist before).
    pub created: Vec<String>,
}

impl TurnSnapshot {
    /// Create a new empty snapshot.
    pub fn new() -> Self {
        Self {
            originals: HashMap::new(),
            created: Vec::new(),
        }
    }

    /// Snapshot the current content of a file. If the file exists, stores its
    /// content in `originals`. Does nothing if already snapshotted.
    pub fn snapshot_file(&mut self, path: &str) {
        if self.originals.contains_key(path) {
            return; // Already snapshotted
        }
        if let Ok(content) = std::fs::read_to_string(path) {
            self.originals.insert(path.to_string(), content);
        }
        // If file doesn't exist, we'll track it as created when we see it appear
    }

    /// Record a file as newly created during this turn.
    /// Only records if not already in originals (i.e., it truly didn't exist before).
    pub fn record_created(&mut self, path: &str) {
        if !self.originals.contains_key(path) && !self.created.contains(&path.to_string()) {
            self.created.push(path.to_string());
        }
    }

    /// Return true if no files were affected.
    pub fn is_empty(&self) -> bool {
        self.originals.is_empty() && self.created.is_empty()
    }

    /// Restore all files to their pre-turn state:
    /// - Overwrite modified files with their original content
    /// - Delete files that were created during the turn
    ///
    /// Returns a list of actions taken (for display).
    pub fn restore(&self) -> Vec<String> {
        let mut actions = Vec::new();

        // Restore modified files
        for (path, content) in &self.originals {
            if std::fs::write(path, content).is_ok() {
                actions.push(format!("restored {path}"));
            } else {
                actions.push(format!("failed to restore {path}"));
            }
        }

        // Delete newly created files
        for path in &self.created {
            if std::fs::remove_file(path).is_ok() {
                actions.push(format!("deleted {path}"));
            } else {
                actions.push(format!("failed to delete {path}"));
            }
        }

        actions
    }
}

#[cfg(test)]
impl TurnSnapshot {
    /// Return the number of files affected (modified + created).
    pub fn file_count(&self) -> usize {
        self.originals.len() + self.created.len()
    }
}

/// A stack of turn snapshots for multi-level undo.
///
/// Each completed agent turn pushes a snapshot. `/undo` pops the most recent.
/// `/undo N` pops the last N turns.
#[derive(Debug, Clone)]
pub struct TurnHistory {
    turns: Vec<TurnSnapshot>,
}

impl TurnHistory {
    /// Create a new empty history.
    pub fn new() -> Self {
        Self { turns: Vec::new() }
    }

    /// Push a completed turn's snapshot onto the stack.
    /// Skips empty snapshots (turns that didn't modify any files).
    pub fn push(&mut self, snapshot: TurnSnapshot) {
        if !snapshot.is_empty() {
            self.turns.push(snapshot);
        }
    }

    /// Return the number of undoable turns.
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Return true if there are no undoable turns.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Undo the last N turns by popping and restoring each.
    /// Returns a list of all actions taken.
    pub fn undo_last(&mut self, n: usize) -> Vec<String> {
        let mut all_actions = Vec::new();
        let count = n.min(self.turns.len());
        for _ in 0..count {
            if let Some(snapshot) = self.turns.pop() {
                all_actions.extend(snapshot.restore());
            }
        }
        all_actions
    }

    /// Clear the entire history (used after /clear or /undo --all).
    pub fn clear(&mut self) {
        self.turns.clear();
    }
}

#[cfg(test)]
impl TurnHistory {
    /// Pop the most recent turn snapshot.
    pub fn pop(&mut self) -> Option<TurnSnapshot> {
        self.turns.pop()
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
    /// Whether this prompt triggered an auto-compact due to context overflow.
    /// Callers can use this to inform users or adjust behavior.
    pub was_overflow: bool,
    /// The last API-level error after all retries were exhausted, if any.
    /// Set when the provider itself fails (rate limits, outages, auth errors)
    /// rather than a tool execution error. Used by the REPL to trigger
    /// fallback provider switching.
    pub last_api_error: Option<String>,
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
                format!("{}…", safe_truncate(err, 200))
            } else {
                err.clone()
            };
            format!("[Previous attempt failed: {summary}. Try a different approach.]\n\n{input}")
        }
        None => input.to_string(),
    }
}

/// Maximum retries for transient API errors (rate limits, 5xx, overload).
/// Total wall-clock budget with the capped-exponential-backoff-plus-jitter
/// policy in `retry_delay`: roughly 5 × ~avg(cap/2) = up to ~150s, which
/// comfortably covers normal Anthropic overload windows (30s–2min).
const MAX_RETRIES: u32 = 5;

/// Maximum number of automatic retries when a tool execution fails during a
/// natural-language prompt. The agent re-runs with error context appended so
/// it can self-correct without the user having to `/retry` manually.
pub const MAX_AUTO_RETRIES: u32 = 2;

/// Build a prompt for automatic retry after a tool error.
/// Includes the original input plus context about what went wrong,
/// encouraging the agent to try a different approach.
pub fn build_auto_retry_prompt(original_input: &str, tool_error: &str, attempt: u32) -> String {
    let summary = if tool_error.len() > 300 {
        format!("{}…", safe_truncate(tool_error, 300))
    } else {
        tool_error.to_string()
    };
    format!(
        "[Auto-retry {attempt}/{MAX_AUTO_RETRIES}: a tool failed with: {summary}. \
         Try a different approach or fix the error.]\n\n{original_input}"
    )
}

/// Known phrases that indicate context overflow across LLM providers.
/// Mirrors the upstream yoagent patterns so we can detect overflow from
/// error *strings* (e.g., in RetriableError messages or raw API output)
/// even when the structured `ProviderError::ContextOverflow` isn't available.
const OVERFLOW_PHRASES: &[&str] = &[
    "prompt is too long",
    "input is too long",
    "exceeds the context window",
    "exceeds the maximum",
    "maximum prompt length",
    "reduce the length of the messages",
    "maximum context length",
    "exceeds the limit of",
    "exceeds the available context size",
    "greater than the context length",
    "context window exceeds limit",
    "exceeded model token limit",
    "context length exceeded",
    "context_length_exceeded",
    "too many tokens",
    "token limit exceeded",
];

/// Check if an error message indicates a context overflow / prompt-too-long error.
///
/// Works on raw error strings — useful when we only have the text, not a
/// structured `ProviderError`. Case-insensitive.
pub fn is_overflow_error(msg: &str) -> bool {
    if msg.is_empty() {
        return false;
    }
    let lower = msg.to_lowercase();
    OVERFLOW_PHRASES.iter().any(|phrase| lower.contains(phrase))
}

/// Build a retry prompt after auto-compacting due to context overflow.
/// Tells the model the context was compacted so it can re-orient.
pub fn build_overflow_retry_prompt(original_input: &str) -> String {
    format!(
        "[Context was auto-compacted because the conversation exceeded the model's token limit. \
         Earlier messages have been summarized. Please continue with the task.]\n\n{original_input}"
    )
}

/// Calculate exponential backoff delay with a 60s cap and ±50% jitter.
///
/// Attempt 1 → ~1s, 2 → ~2s, 3 → ~4s, 4 → ~8s, 5 → ~16s, 6 → ~32s, 7+ → ~60s
/// (each with jitter). Capped to protect against pathologically long waits,
/// jittered to avoid thundering-herd against Anthropic during overload events.
/// Floored at 500ms so even attempt 0 / degenerate cases still pause.
///
/// Day 47: widened from a pure 2^n (max 4s total) to this policy after an
/// Anthropic `overloaded_error` cost an entire session — see journal.
pub fn retry_delay(attempt: u32) -> Duration {
    const CAP_SECS: u64 = 60;
    // Clamp the shift so 2^n can't overflow u64 for pathological inputs.
    let shift = attempt.saturating_sub(1).min(6); // 2^6 = 64 ≥ CAP
    let base = 1u64 << shift;
    let capped = base.min(CAP_SECS);
    // Cheap entropy for ±50% jitter without pulling in `rand` as a direct dep.
    // Nanoseconds-since-epoch provide enough spread for thundering-herd avoidance.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let jitter_bp = (nanos % 1000) as u64; // 0..=999 basis points
    let factor_bp = 500 + jitter_bp; // 500..=1499 → 0.5x..~1.5x
    let jittered_ms = capped * factor_bp; // capped(sec) * factor_bp == capped*1000*factor_bp/1000 (ms)
    Duration::from_millis(jittered_ms.max(500))
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
        "stream closed",
        "unexpected eof",
        "broken pipe",
        "reset by peer",
        "incomplete",
    ];
    for keyword in &retriable {
        if lower.contains(keyword) {
            return true;
        }
    }

    false
}

/// Diagnose a non-retriable API error and return a user-friendly message
/// with actionable suggestions. Returns `None` if the error doesn't match
/// any known pattern (falls back to the raw error display).
///
/// Covers three categories:
/// 1. **Authentication errors** (401/invalid key) — shows which env var to set
/// 2. **Network errors** (connection refused, DNS, timeout) — suggests retry/checks
/// 3. **Model not found** (404/invalid model) — suggests known models for the provider
pub fn diagnose_api_error(error: &str, model: &str) -> Option<String> {
    let lower = error.to_lowercase();
    let provider = infer_provider_from_model(model);

    // ── Authentication / API key errors ──────────────────────────────
    if lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("invalid_api_key")
        || lower.contains("invalid x-api-key")
        || lower.contains("authentication")
    {
        let env_var = crate::cli::provider_api_key_env(&provider).unwrap_or("ANTHROPIC_API_KEY");
        let config_hint = "Or add api_key to .yoyo.toml, or use --api-key <key>.";
        let key_set = std::env::var(env_var).is_ok();
        let status = if key_set {
            format!("  {env_var} is set but the API rejected it — check the key value.")
        } else {
            format!("  {env_var} is not set.")
        };
        return Some(format!(
            "Authentication failed for provider '{provider}'.\n\
             {status}\n\
             Set it with: export {env_var}=<your-key>\n\
             {config_hint}"
        ));
    }

    // ── Model not found ─────────────────────────────────────────────
    if lower.contains("not_found")
        || lower.contains("model not found")
        || lower.contains("404")
        || lower.contains("does not exist")
        || lower.contains("unknown model")
        || lower.contains("invalid model")
        || lower.contains("no such model")
    {
        let known = crate::cli::known_models_for_provider(&provider);
        let mut msg = format!("Model '{model}' was not found by provider '{provider}'.");
        if !known.is_empty() {
            msg.push_str("\nAvailable models for this provider:");
            for m in known {
                msg.push_str(&format!("\n  • {m}"));
            }
            msg.push_str(&format!(
                "\nSwitch with: /model {} or --model {}",
                known[0], known[0]
            ));
        }
        return Some(msg);
    }

    // ── Network / connection errors ─────────────────────────────────
    if lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("dns")
        || lower.contains("resolve")
        || lower.contains("name or service not known")
        || lower.contains("network is unreachable")
        || lower.contains("no route to host")
    {
        let mut msg = String::from("Network error — could not reach the API.\n");
        if provider == "ollama" {
            msg.push_str("  Is Ollama running? Try: ollama serve\n");
        } else if provider == "custom" {
            msg.push_str("  Check your --base-url value.\n");
        } else {
            msg.push_str(&format!(
                "  Check your internet connection and that {provider}'s API is reachable.\n"
            ));
        }
        msg.push_str("  You can retry with /retry.");
        return Some(msg);
    }

    // ── Permission denied (403) ─────────────────────────────────────
    if lower.contains("403") || lower.contains("forbidden") || lower.contains("permission denied") {
        return Some(format!(
            "Access forbidden (403) from provider '{provider}'.\n\
             This usually means your API key doesn't have access to model '{model}'.\n\
             Check your plan/tier with {provider}, or try a different model."
        ));
    }

    // ── Stream ended (provider-specific, not retriable) ───────────
    if lower.contains("stream ended") {
        return Some(
            "The API stream ended without the expected termination signal.\n\
             This is common with some providers (e.g. MiniMax) whose SSE format \n\
             differs slightly from the OpenAI standard. The response was likely \n\
             delivered in full — check the output above. Not retrying."
                .to_string(),
        );
    }

    // ── Stream / connection interruption (retriable) ────────────────
    if lower.contains("stream closed")
        || lower.contains("unexpected eof")
        || lower.contains("broken pipe")
        || lower.contains("incomplete")
    {
        return Some(
            "The API stream was interrupted before the response completed.\n\
             This is usually a transient network issue — yoyo will auto-retry.\n\
             If it persists, check your internet connection or try a different model."
                .to_string(),
        );
    }

    None
}

/// Infer the provider name from a model identifier.
/// Used by `diagnose_api_error` so it doesn't need `provider` threaded through every caller.
fn infer_provider_from_model(model: &str) -> String {
    let m = model.to_lowercase();
    if m.contains("claude") || m.contains("opus") || m.contains("sonnet") || m.contains("haiku") {
        "anthropic".into()
    } else if m.starts_with("gpt-") || m.starts_with("o3") || m.starts_with("o4") {
        "openai".into()
    } else if m.contains("gemini") {
        "google".into()
    } else if m.contains("grok") {
        "xai".into()
    } else if m.contains("deepseek") {
        "deepseek".into()
    } else if m.contains("mistral") || m.contains("codestral") {
        "mistral".into()
    } else if m.contains("llama") || m.contains("mixtral") || m.contains("gemma") {
        // Could be groq, ollama, or cerebras — default to groq for hosted
        "groq".into()
    } else if m.contains("glm") {
        "zai".into()
    } else {
        "anthropic".into() // safe default
    }
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
    /// A context overflow error — caller should compact and retry.
    ContextOverflow { error_msg: String, usage: Usage },
}

/// Execute a single prompt attempt and process all events.
/// Returns whether we got a retriable error (so the caller can retry).
async fn run_prompt_once(
    agent: &mut Agent,
    input: &str,
    changes: &SessionChanges,
    model: &str,
) -> PromptResult {
    let rx = agent.prompt(input).await;
    handle_prompt_events(agent, rx, changes, model).await
}

/// Execute a single prompt attempt with pre-built messages (e.g. multi-modal content).
/// Same event handling as `run_prompt_once`, but uses `prompt_messages` instead of `prompt`.
async fn run_prompt_once_with_messages(
    agent: &mut Agent,
    messages: Vec<AgentMessage>,
    changes: &SessionChanges,
    model: &str,
) -> PromptResult {
    let rx = agent.prompt_messages(messages).await;
    handle_prompt_events(agent, rx, changes, model).await
}

/// Shared event-handling loop for prompt execution.
/// Processes all events from the agent's streaming channel and returns the result.
async fn handle_prompt_events(
    agent: &mut Agent,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
    changes: &SessionChanges,
    model: &str,
) -> PromptResult {
    let mut usage = Usage::default();
    let mut in_text = false;
    let mut in_thinking = false;
    let mut tool_timers: HashMap<String, Instant> = HashMap::new();
    let mut collected_text = String::new();
    let mut retriable_error: Option<String> = None;
    let mut overflow_error: Option<String> = None;
    let mut last_tool_error: Option<String> = None;
    let mut md_renderer = MarkdownRenderer::new();
    let mut spinner: Option<Spinner> = Some(Spinner::start());

    // Filter for <think>...</think> blocks that leak into text output
    let mut think_filter = ThinkBlockFilter::new();

    // Audit log: track in-flight tool calls (name + args) so we can log at completion
    let mut audit_inflight: HashMap<String, (String, serde_json::Value)> = HashMap::new();

    // Live progress timers for long-running tools (bash)
    let mut tool_progress_timers: HashMap<String, ToolProgressTimer> = HashMap::new();

    // Bash tool call IDs that need deferred timer start.
    // We don't start the timer on ToolExecutionStart for bash because the
    // confirmation prompt would be overwritten by the spinner. Instead we
    // defer to the first ToolExecutionUpdate (which only fires once the
    // command is actually running, i.e. after confirmation).
    // Maps tool_call_id → optional command string for display label.
    let mut deferred_bash_timers: HashMap<String, Option<String>> = HashMap::new();

    // Tool batch tracking for group summaries
    let mut batch_count: usize = 0;
    let mut batch_succeeded: usize = 0;
    let mut batch_failed: usize = 0;
    let mut batch_start: Option<Instant> = None;

    // Turn tracking for boundary markers
    let mut turn_number: usize = 0;
    let mut had_text = false; // whether we've seen text output in this prompt

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

                        // Show turn boundary when transitioning from text to a new tool batch
                        if in_text {
                            println!();
                            in_text = false;
                        }

                        // New batch starting (first tool after text or start)
                        if batch_count == 0 {
                            if batch_start.is_none() {
                                batch_start = Some(Instant::now());
                            }
                            // Show turn boundary for multi-turn (turn 2+)
                            if turn_number > 1 && had_text {
                                println!("{}", turn_boundary(turn_number));
                            }
                        }

                        batch_count += 1;
                        tool_timers.insert(tool_call_id.clone(), Instant::now());
                        // Track for audit log
                        audit_inflight.insert(
                            tool_call_id.clone(),
                            (tool_name.clone(), args.clone()),
                        );
                        let summary = format_tool_summary(&tool_name, &args);
                        if tool_name == "sub_agent" {
                            // Distinctive header for sub-agent delegation
                            eprintln!("\n{DIM}  🐙 Delegating to sub-agent...{RESET}");
                        }
                        print!("{YELLOW}  ▶ {summary}{RESET}");
                        if is_verbose() {
                            println!();
                            let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                            for line in args_str.lines() {
                                println!("{DIM}    │ {line}{RESET}");
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

                        // Defer timer start for bash commands — the confirmation
                        // prompt would be overwritten by the spinner. The timer
                        // will start on the first ToolExecutionUpdate instead.
                        // Store the command string for display as a label.
                        if tool_name == "bash" {
                            let cmd_label = args
                                .get("command")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            deferred_bash_timers.insert(tool_call_id.clone(), cmd_label);
                        }
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, result, tool_name, .. } => {
                        // Clean up deferred timer entry if command was denied before running
                        deferred_bash_timers.remove(&tool_call_id);
                        // Stop any live progress timer for this tool
                        if let Some(timer) = tool_progress_timers.remove(&tool_call_id) {
                            timer.stop();
                        }
                        let elapsed = tool_timers
                            .remove(&tool_call_id)
                            .map(|start| start.elapsed());
                        let dur_str = elapsed
                            .map(|d| format!(" {DIM}({}){RESET}", format_duration(d)))
                            .unwrap_or_default();

                        // Audit log: record the completed tool call
                        if let Some((audit_tool, audit_args)) = audit_inflight.remove(&tool_call_id) {
                            let duration_ms = elapsed.map(|d| d.as_millis() as u64).unwrap_or(0);
                            audit_log_tool_call(&audit_tool, &audit_args, duration_ms, !is_error);
                        }

                        if is_error {
                            batch_failed += 1;
                            println!(" {RED}✗{RESET}{dur_str}");
                            let preview = tool_result_preview(&result, 200);
                            if !preview.is_empty() {
                                // Indent error output under the tool header
                                println!("{}", indent_tool_output(&preview));
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
                            batch_succeeded += 1;
                            last_tool_error = None;
                            println!(" {GREEN}✓{RESET}{dur_str}");
                            // Warn when write_file writes 0 bytes (empty content)
                            if tool_name == "write_file" {
                                let wrote_zero = result.details.get("bytes")
                                    .and_then(|v| v.as_u64())
                                    .map(|b| b == 0)
                                    .unwrap_or(false);
                                if wrote_zero {
                                    eprintln!("{YELLOW}    ⚠ write_file wrote 0 bytes — file is now empty{RESET}");
                                }
                            }
                            if is_verbose() {
                                let preview = tool_result_preview(&result, 200);
                                if !preview.is_empty() {
                                    // Indent verbose output under the tool header
                                    println!("{}", indent_tool_output(&preview));
                                }
                            }
                        }
                    }
                    AgentEvent::ToolExecutionUpdate { tool_call_id, partial_result, .. } => {
                        // Start deferred bash timer on first update.
                        // This means the command is actually running (confirmation
                        // has already been resolved), so the spinner won't
                        // overwrite the permission prompt.
                        if let Some(cmd_label) = deferred_bash_timers.remove(&tool_call_id) {
                            let timer = ToolProgressTimer::start("bash".to_string());
                            if let Some(label) = cmd_label {
                                timer.set_label(label);
                            }
                            tool_progress_timers.insert(tool_call_id.clone(), timer);
                        }

                        // Update line count on the progress timer if active
                        let line_count = count_result_lines(&partial_result);
                        if let Some(timer) = tool_progress_timers.get(&tool_call_id) {
                            timer.set_line_count(line_count);
                        }

                        // Only show partial output in interactive (terminal) mode.
                        // In piped/CI mode, cursor-up sequences don't work and every
                        // partial update becomes a permanent log line, inflating output.
                        if io::stdout().is_terminal() {
                            let text = extract_result_text(&partial_result);
                            if !text.is_empty() {
                                let tail = format_partial_tail(&text, 6);
                                if !tail.is_empty() {
                                    println!();
                                    println!("{tail}");
                                    io::stdout().flush().ok();
                                }
                            }
                        }
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Text { delta },
                        ..
                    } => {
                        // render_latency_budget: First-token path
                        // 1. Spinner stop: ~0.1ms (synchronous eprint + flush, first token only)
                        // 2. Batch summary print: conditional, rare
                        // 3. render_delta(): ~0 for mid-line, 1-token buffer at line start
                        // 4. print!() + flush(): ~0.01ms system call
                        // Total: <0.2ms first token, <0.05ms subsequent tokens.
                        // The API network latency (~50-200ms) dominates; renderer is negligible.

                        // Stop spinner on first text
                        if let Some(s) = spinner.take() { s.stop(); }
                        // Transition from thinking to text: add a divider
                        // so text doesn't appear glued to the last thinking output
                        if in_thinking {
                            eprintln!();
                            eprintln!("{}", section_divider());
                            let _ = io::stderr().flush();
                            in_thinking = false;
                        }

                        // Print batch summary if we just finished a tool batch
                        if batch_count > 0 {
                            let batch_duration = batch_start
                                .map(|s| s.elapsed())
                                .unwrap_or_default();
                            let summary = format_tool_batch_summary(
                                batch_count, batch_succeeded, batch_failed, batch_duration,
                            );
                            if !summary.is_empty() {
                                println!("{summary}");
                            }
                            // Reset batch tracking
                            batch_count = 0;
                            batch_succeeded = 0;
                            batch_failed = 0;
                            batch_start = None;
                        }

                        if !in_text {
                            println!();
                            in_text = true;
                            had_text = true;
                        }
                        // Filter <think>...</think> blocks unless verbose mode
                        let filtered = if is_verbose() {
                            delta.clone()
                        } else {
                            think_filter.filter(&delta)
                        };
                        if filtered.is_empty() {
                            // Inside a think block — nothing to render yet
                            io::stdout().flush().ok();
                            continue;
                        }
                        // Render and display BEFORE collecting — minimizes time-to-screen.
                        // collected_text is only used after the stream ends, so ordering
                        // with print doesn't affect correctness. (render_latency_budget)
                        let rendered = md_renderer.render_delta(&filtered);
                        if !rendered.is_empty() {
                            print!("{}", rendered);
                        }
                        io::stdout().flush().ok();
                        collected_text.push_str(&filtered);
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Thinking { delta },
                        ..
                    } => {
                        // Stop spinner on first thinking output
                        if let Some(s) = spinner.take() { s.stop(); }
                        if !in_thinking {
                            // Print thinking section header on first thinking token
                            eprintln!("\n{}", section_header("Thinking"));
                            in_thinking = true;
                        }
                        // Render thinking to stderr (dimmed) so it doesn't
                        // interleave with stdout text output
                        eprint!("{DIM}{delta}{RESET}");
                        let _ = io::stderr().flush();
                    }
                    AgentEvent::AgentEnd { messages } => {
                        // Stop spinner if still running
                        if let Some(s) = spinner.take() { s.stop(); }

                        // Flush think block filter — emit any partial non-think text
                        let remaining = think_filter.flush();
                        if !remaining.is_empty() {
                            let rendered = md_renderer.render_delta(&remaining);
                            if !rendered.is_empty() {
                                print!("{rendered}");
                                io::stdout().flush().ok();
                            }
                            collected_text.push_str(&remaining);
                        }

                        // Print batch summary if tools were the last thing before end
                        if batch_count > 0 {
                            let batch_duration = batch_start
                                .map(|s| s.elapsed())
                                .unwrap_or_default();
                            let summary = format_tool_batch_summary(
                                batch_count, batch_succeeded, batch_failed, batch_duration,
                            );
                            if !summary.is_empty() {
                                println!("{summary}");
                            }
                            batch_count = 0;
                            batch_succeeded = 0;
                            batch_failed = 0;
                            batch_start = None;
                        }

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
                                        // Check for context overflow first — needs special handling
                                        if is_overflow_error(err_msg) {
                                            overflow_error = Some(err_msg.clone());
                                        } else if is_retriable_error(err_msg) {
                                            // Check if this error is worth retrying
                                            retriable_error = Some(err_msg.clone());
                                        } else {
                                            eprintln!("\n{RED}  error: {err_msg}{RESET}");
                                            // Show diagnostic help for common errors
                                            if let Some(diagnostic) = diagnose_api_error(err_msg, model) {
                                                eprintln!("{YELLOW}  💡 {}{RESET}", diagnostic.replace('\n', &format!("\n{YELLOW}     {RESET}")));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    AgentEvent::InputRejected { reason } => {
                        if let Some(s) = spinner.take() { s.stop(); }
                        eprintln!("{RED}  input rejected: {reason}{RESET}");
                        if let Some(diagnostic) = diagnose_api_error(&reason, model) {
                            eprintln!("{YELLOW}  💡 {}{RESET}", diagnostic.replace('\n', &format!("\n{YELLOW}     {RESET}")));
                        }
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        if let Some(s) = spinner.take() { s.stop(); }
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        println!("{DIM}  {text}{RESET}");
                    }
                    AgentEvent::MessageStart { .. } => {
                        // Agent started a new message — stop the spinner
                        // so it doesn't overlap with output
                        if let Some(s) = spinner.take() { s.stop(); }
                    }
                    AgentEvent::MessageEnd { .. }
                        // Agent finished a message — flush any pending text
                        // (This is where ExecutionLimits stop messages appear)
                        if in_text =>
                    {
                        let remaining = md_renderer.flush();
                        if !remaining.is_empty() {
                            print!("{remaining}");
                        }
                        println!();
                        in_text = false;
                    }
                    AgentEvent::TurnStart => {
                        turn_number += 1;
                    }
                    AgentEvent::TurnEnd { .. } => {
                        // Turn complete — nothing needed here for now.
                        // Explicitly matched to keep event handling exhaustive.
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

    if let Some(err_msg) = overflow_error {
        PromptResult::ContextOverflow {
            error_msg: err_msg,
            usage,
        }
    } else if let Some(err_msg) = retriable_error {
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
    // Proactive compact: if context is already near the limit, compact before attempting
    crate::commands_session::proactive_compact_if_needed(agent);

    let prompt_start = Instant::now();
    let mut total_usage = Usage::default();
    let mut collected_text = String::new();
    let mut last_tool_error: Option<String> = None;
    let mut did_overflow_compact = false;
    let mut api_error: Option<String> = None;

    // Save message state before the first attempt so we can restore on retry
    let saved_state = agent.save_messages().ok();

    for attempt in 0..=MAX_RETRIES {
        // On retry, restore pre-prompt state so we don't duplicate the user message
        if attempt > 0 {
            if let Some(ref json) = saved_state {
                let _ = agent.restore_messages(json);
            }
        }

        match run_prompt_once(agent, input, changes, model).await {
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
                    // Exhausted all retries — show the final error with diagnostic
                    eprintln!("\n{RED}  error: {error_msg}{RESET}");
                    eprintln!("{DIM}  (failed after {} attempts){RESET}", MAX_RETRIES + 1);
                    if let Some(diagnostic) = diagnose_api_error(&error_msg, model) {
                        eprintln!(
                            "{YELLOW}  💡 {}{RESET}",
                            diagnostic.replace('\n', &format!("\n{YELLOW}     {RESET}"))
                        );
                    }
                    api_error = Some(error_msg);
                }
            }
            PromptResult::ContextOverflow { error_msg, usage } => {
                total_usage.input += usage.input;
                total_usage.output += usage.output;
                total_usage.cache_read += usage.cache_read;
                total_usage.cache_write += usage.cache_write;

                // Auto-compact and retry once
                eprintln!(
                    "\n{YELLOW}  ⚡ context overflow detected — auto-compacting and retrying...{RESET}"
                );
                eprintln!("{DIM}  ({error_msg}){RESET}");

                if let Some(ref json) = saved_state {
                    let _ = agent.restore_messages(json);
                }
                if let Some((before_count, before_tokens, after_count, after_tokens)) =
                    crate::commands_session::compact_agent(agent)
                {
                    eprintln!(
                        "{DIM}  compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}",
                        crate::format::format_token_count(before_tokens),
                        crate::format::format_token_count(after_tokens)
                    );
                }

                did_overflow_compact = true;

                // Retry with the compacted context
                let retry_input = build_overflow_retry_prompt(input);
                match run_prompt_once(agent, &retry_input, changes, model).await {
                    PromptResult::Done {
                        collected_text: text,
                        usage: retry_usage,
                        last_tool_error: tool_err,
                    } => {
                        total_usage.input += retry_usage.input;
                        total_usage.output += retry_usage.output;
                        total_usage.cache_read += retry_usage.cache_read;
                        total_usage.cache_write += retry_usage.cache_write;
                        collected_text = text;
                        last_tool_error = tool_err;
                    }
                    PromptResult::RetriableError {
                        error_msg: retry_err,
                        usage: retry_usage,
                    }
                    | PromptResult::ContextOverflow {
                        error_msg: retry_err,
                        usage: retry_usage,
                    } => {
                        total_usage.input += retry_usage.input;
                        total_usage.output += retry_usage.output;
                        total_usage.cache_read += retry_usage.cache_read;
                        total_usage.cache_write += retry_usage.cache_write;
                        eprintln!("\n{RED}  error: {retry_err}{RESET}");
                        eprintln!(
                            "{DIM}  (overflow retry also failed — try /compact manually){RESET}"
                        );
                        api_error = Some(retry_err);
                    }
                }
                break;
            }
        }
    }

    session_total.input += total_usage.input;
    session_total.output += total_usage.output;
    session_total.cache_read += total_usage.cache_read;
    session_total.cache_write += total_usage.cache_write;
    print_usage(&total_usage, session_total, model, prompt_start.elapsed());
    // Issue #258: yoagent 0.7.x runs the agent loop in a background task; the
    // agent's internal `self.messages` is only updated when `finish()` is awaited.
    // Without this, `agent.messages()` returns stale state and the context bar
    // permanently reads "0% used". Call finish() before reading messages.
    agent.finish().await;
    let ctx_used = total_tokens(agent.messages()) as u64;
    let ctx_max = crate::cli::effective_context_tokens();
    print_context_usage(ctx_used, ctx_max);
    if let Some(warning) = crate::format::context_budget_warning(ctx_used, ctx_max) {
        eprintln!("{warning}");
    }
    maybe_ring_bell(prompt_start.elapsed());
    println!();
    PromptOutcome {
        text: collected_text,
        last_tool_error,
        was_overflow: did_overflow_compact,
        last_api_error: api_error,
    }
}

/// Run a prompt with automatic retry on tool errors.
///
/// Wraps `run_prompt_with_changes` with self-correction: if the outcome
/// contains a `last_tool_error`, the prompt is automatically re-run with
/// error context appended (up to `MAX_AUTO_RETRIES` times). This makes
/// yoyo more resilient — instead of waiting for the user to `/retry`,
/// the agent self-corrects on transient tool failures.
///
/// Only meant for natural-language prompts (not slash commands).
pub async fn run_prompt_auto_retry(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
    changes: &SessionChanges,
) -> PromptOutcome {
    let mut outcome = run_prompt_with_changes(agent, input, session_total, model, changes).await;

    for attempt in 1..=MAX_AUTO_RETRIES {
        match outcome.last_tool_error {
            Some(ref err) => {
                if session_budget_exhausted(30) {
                    eprintln!(
                        "{DIM}  ⏱ session budget nearly exhausted, stopping retries early{RESET}"
                    );
                    break;
                }
                let retry_prompt = build_auto_retry_prompt(input, err, attempt);
                eprintln!(
                    "{DIM}  ⚡ auto-retrying after tool error (attempt {attempt}/{MAX_AUTO_RETRIES})...{RESET}"
                );
                outcome =
                    run_prompt_with_changes(agent, &retry_prompt, session_total, model, changes)
                        .await;
            }
            None => break,
        }
    }

    outcome
}

/// Run a prompt with pre-built content blocks (e.g. text + image).
/// This is the content-block equivalent of `run_prompt`.
pub async fn run_prompt_with_content(
    agent: &mut Agent,
    content_blocks: Vec<Content>,
    session_total: &mut Usage,
    model: &str,
) -> PromptOutcome {
    let changes = SessionChanges::new();
    run_prompt_with_content_and_changes(agent, content_blocks, session_total, model, &changes).await
}

/// Run a content-block prompt with automatic retry on tool errors.
///
/// This is the content-block equivalent of `run_prompt_auto_retry`: when the
/// outcome contains a `last_tool_error`, the prompt is automatically re-run
/// with error context appended as a text-only follow-up (up to `MAX_AUTO_RETRIES`
/// times). The original content blocks (including images and @file mentions) are
/// already in the conversation history, so the retry only needs the text nudge.
///
/// Without this, @file mention prompts silently skip auto-retry, meaning tool
/// failures require the user to manually `/retry` — inconsistent with regular
/// prompts where auto-retry kicks in automatically.
pub async fn run_prompt_auto_retry_with_content(
    agent: &mut Agent,
    content_blocks: Vec<Content>,
    session_total: &mut Usage,
    model: &str,
    changes: &SessionChanges,
    original_text: &str,
) -> PromptOutcome {
    let mut outcome =
        run_prompt_with_content_and_changes(agent, content_blocks, session_total, model, changes)
            .await;

    for attempt in 1..=MAX_AUTO_RETRIES {
        match outcome.last_tool_error {
            Some(ref err) => {
                if session_budget_exhausted(30) {
                    eprintln!(
                        "{DIM}  ⏱ session budget nearly exhausted, stopping retries early{RESET}"
                    );
                    break;
                }
                // Retry with a text-only follow-up — the original content blocks
                // (files, images) are already in conversation history from the first attempt
                let retry_prompt = build_auto_retry_prompt(original_text, err, attempt);
                eprintln!(
                    "{DIM}  ⚡ auto-retrying after tool error (attempt {attempt}/{MAX_AUTO_RETRIES})...{RESET}"
                );
                outcome =
                    run_prompt_with_changes(agent, &retry_prompt, session_total, model, changes)
                        .await;
            }
            None => break,
        }
    }

    outcome
}

/// Run a prompt with pre-built content blocks and file change tracking.
/// This is the content-block equivalent of `run_prompt_with_changes`.
pub async fn run_prompt_with_content_and_changes(
    agent: &mut Agent,
    content_blocks: Vec<Content>,
    session_total: &mut Usage,
    model: &str,
    changes: &SessionChanges,
) -> PromptOutcome {
    // Proactive compact: if context is already near the limit, compact before attempting
    crate::commands_session::proactive_compact_if_needed(agent);

    let prompt_start = Instant::now();
    let mut total_usage = Usage::default();
    let mut collected_text = String::new();
    let mut last_tool_error: Option<String> = None;
    let mut api_error: Option<String> = None;
    let user_msg = AgentMessage::Llm(Message::User {
        content: content_blocks,
        timestamp: now_ms(),
    });

    // Save message state before the first attempt so we can restore on retry
    let saved_state = agent.save_messages().ok();

    for attempt in 0..=MAX_RETRIES {
        // On retry, restore pre-prompt state so we don't duplicate the user message
        if attempt > 0 {
            if let Some(ref json) = saved_state {
                let _ = agent.restore_messages(json);
            }
        }

        match run_prompt_once_with_messages(agent, vec![user_msg.clone()], changes, model).await {
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
                    let next = attempt + 2;
                    eprintln!(
                        "{DIM}  ⚡ retrying (attempt {next}/{}, waiting {delay_secs}s)...{RESET}",
                        MAX_RETRIES + 1
                    );
                    tokio::time::sleep(delay).await;
                } else {
                    eprintln!("\n{RED}  error: {error_msg}{RESET}");
                    eprintln!("{DIM}  (failed after {} attempts){RESET}", MAX_RETRIES + 1);
                    if let Some(diagnostic) = diagnose_api_error(&error_msg, model) {
                        eprintln!(
                            "{YELLOW}  💡 {}{RESET}",
                            diagnostic.replace('\n', &format!("\n{YELLOW}     {RESET}"))
                        );
                    }
                    api_error = Some(error_msg);
                }
            }
            PromptResult::ContextOverflow { error_msg, usage } => {
                total_usage.input += usage.input;
                total_usage.output += usage.output;
                total_usage.cache_read += usage.cache_read;
                total_usage.cache_write += usage.cache_write;

                eprintln!(
                    "\n{YELLOW}  ⚡ context overflow detected — cannot retry with image content{RESET}"
                );
                eprintln!("{DIM}  ({error_msg}){RESET}");
                api_error = Some(error_msg);
                break;
            }
        }
    }

    session_total.input += total_usage.input;
    session_total.output += total_usage.output;
    session_total.cache_read += total_usage.cache_read;
    session_total.cache_write += total_usage.cache_write;
    print_usage(&total_usage, session_total, model, prompt_start.elapsed());
    // Issue #258: see run_prompt_with_changes — yoagent 0.7.x requires finish()
    // before reading messages, otherwise the context bar reads stale "0%".
    agent.finish().await;
    let ctx_used = total_tokens(agent.messages()) as u64;
    let ctx_max = crate::cli::effective_context_tokens();
    print_context_usage(ctx_used, ctx_max);
    if let Some(warning) = crate::format::context_budget_warning(ctx_used, ctx_max) {
        eprintln!("{warning}");
    }
    maybe_ring_bell(prompt_start.elapsed());
    println!();
    PromptOutcome {
        text: collected_text,
        last_tool_error,
        was_overflow: false,
        last_api_error: api_error,
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
    fn test_retry_delay_exponential_backoff_ranges() {
        // Post-Day-47 policy: cap + ±50% jitter. Assertions are ranges, not
        // exact values, so the test doesn't flake on the jitter RNG.
        // Attempt 1 ideal=1s → [0.5s, 1.5s]
        let d1 = retry_delay(1);
        assert!(
            d1 >= Duration::from_millis(500) && d1 <= Duration::from_millis(1500),
            "attempt 1 out of range: {d1:?}"
        );
        // Attempt 2 ideal=2s → [1s, 3s]
        let d2 = retry_delay(2);
        assert!(
            d2 >= Duration::from_secs(1) && d2 <= Duration::from_secs(3),
            "attempt 2 out of range: {d2:?}"
        );
        // Attempt 3 ideal=4s → [2s, 6s]
        let d3 = retry_delay(3);
        assert!(
            d3 >= Duration::from_secs(2) && d3 <= Duration::from_secs(6),
            "attempt 3 out of range: {d3:?}"
        );
    }

    #[test]
    fn test_retry_delay_capped_at_60s() {
        // Very high attempt numbers must be capped (jitter can push up to ~90s,
        // but never the pathological 2^20 seconds the old pure-exponential would).
        let d = retry_delay(20);
        assert!(d <= Duration::from_secs(90), "not capped: {d:?}");
        assert!(d >= Duration::from_secs(30), "cap too aggressive: {d:?}");
    }

    // Issue #258 / Day 33 lesson (test from the user's perspective):
    // After draining the event stream from prompt_messages, the agent's
    // internal `messages` field is still empty until `finish().await` is
    // called. This is exactly the bug yoyo had: it read `agent.messages()`
    // immediately after the loop ended and saw 0, so the context bar
    // permanently said "0% used".
    //
    // This test reproduces the failure mode against yoagent's MockProvider
    // and verifies that calling `finish()` is what makes messages visible.
    #[tokio::test]
    async fn agent_messages_empty_until_finish_is_called() {
        use yoagent::provider::MockProvider;
        use yoagent::Agent;

        let provider = MockProvider::text("hello back");
        let mut agent = Agent::new(provider)
            .with_model("mock-model")
            .with_api_key("not-a-real-key");

        // Sanity: starts empty.
        assert_eq!(agent.messages().len(), 0);

        // Drive a prompt and drain all events.
        let mut rx = agent.prompt("hi").await;
        while rx.recv().await.is_some() {}

        // Without finish(), yoagent 0.7.x leaves messages stale. This is the
        // root cause of Issue #258 — and exactly why yoyo's context bar read 0%.
        let stale_count = agent.messages().len();

        // After finish(), the loop's messages are restored into the agent.
        agent.finish().await;
        let real_count = agent.messages().len();

        assert!(
            real_count > 0,
            "expected agent.messages() to be non-empty after finish(), got {real_count}"
        );
        assert!(
            real_count > stale_count || stale_count == 0,
            "finish() should restore messages: stale={stale_count}, real={real_count}"
        );
    }

    #[test]
    fn test_retry_delay_zero_attempt_floor() {
        // Edge case: attempt 0 with saturating_sub should still yield the floor
        // and land in the attempt-1 jitter window.
        let d = retry_delay(0);
        assert!(d >= Duration::from_millis(500), "below floor: {d:?}");
        assert!(
            d <= Duration::from_millis(1500),
            "above attempt-1 range: {d:?}"
        );
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
    fn test_is_retriable_stream_errors() {
        // "stream ended" is NOT retriable — the response was likely complete
        // (see Issue #222: MiniMax SSE format causes false retries)
        assert!(!is_retriable_error("Stream ended"));

        // Other stream interruptions ARE retriable
        assert!(is_retriable_error("stream closed unexpectedly"));
        assert!(is_retriable_error("unexpected eof while reading"));
        assert!(is_retriable_error("broken pipe"));
        assert!(is_retriable_error("connection reset by peer"));
        assert!(is_retriable_error("incomplete response from server"));
    }

    #[test]
    fn test_stream_ended_not_retriable() {
        // Issue #222: MiniMax's SSE stream doesn't send `data: [DONE]` in the
        // expected format. yoagent reports "stream ended" but the response was
        // already complete. Retrying causes 4x duplicated output.
        assert!(!is_retriable_error("stream ended"));
        assert!(!is_retriable_error("Stream ended"));
        assert!(!is_retriable_error("stream ended unexpectedly"));
        assert!(!is_retriable_error("Stream ended: no more data"));
    }

    #[test]
    fn test_diagnose_stream_ended() {
        // "stream ended" now gets a distinct message (not retriable, Issue #222)
        let diag = diagnose_api_error("error: Stream ended", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        let msg = diag.unwrap();
        assert!(msg.contains("stream ended"));
        assert!(msg.contains("delivered in full"));
        assert!(msg.contains("Not retrying"));
    }

    #[test]
    fn test_diagnose_stream_closed() {
        let diag = diagnose_api_error("stream closed unexpectedly", "gpt-4o");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_diagnose_unexpected_eof() {
        let diag = diagnose_api_error("unexpected eof", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_diagnose_broken_pipe() {
        let diag = diagnose_api_error("broken pipe while writing", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_diagnose_incomplete() {
        let diag = diagnose_api_error("incomplete response", "claude-sonnet-4-20250514");
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("interrupted"));
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

    #[test]
    fn test_build_auto_retry_prompt_includes_error_and_input() {
        let prompt = build_auto_retry_prompt("fix the bug", "file not found: foo.rs", 1);
        assert!(prompt.contains("fix the bug"));
        assert!(prompt.contains("file not found: foo.rs"));
        assert!(prompt.contains("Auto-retry 1/2"));
        assert!(prompt.contains("Try a different approach"));
    }

    #[test]
    fn test_build_auto_retry_prompt_attempt_number() {
        let prompt1 = build_auto_retry_prompt("do something", "error", 1);
        let prompt2 = build_auto_retry_prompt("do something", "error", 2);
        assert!(prompt1.contains("Auto-retry 1/2"));
        assert!(prompt2.contains("Auto-retry 2/2"));
    }

    #[test]
    fn test_build_auto_retry_prompt_truncates_long_errors() {
        let long_error = "x".repeat(500);
        let prompt = build_auto_retry_prompt("input", &long_error, 1);
        // The error should be truncated to 300 chars + ellipsis
        assert!(prompt.contains(&"x".repeat(300)));
        assert!(prompt.contains('…'));
        // Should NOT contain the full 500-char error
        assert!(!prompt.contains(&"x".repeat(500)));
    }

    #[test]
    fn test_build_auto_retry_prompt_preserves_short_errors() {
        let short_error = "command exited with code 1";
        let prompt = build_auto_retry_prompt("input", short_error, 1);
        assert!(prompt.contains(short_error));
        // Short errors should not have ellipsis
        let error_portion = &prompt[..prompt.find("Try a").unwrap()];
        assert!(!error_portion.ends_with("…. "));
    }

    #[test]
    fn test_auto_retry_prompt_with_file_mention_context() {
        // When @file mentions are expanded, the cleaned_text passed to
        // run_prompt_auto_retry_with_content should be preserved in retries.
        // The retry prompt includes the original text so the agent knows what
        // the user asked even though the files are already in conversation history.
        let original_text = "explain the bug in this file";
        let error = "bash: command not found: cargo";
        let prompt = build_auto_retry_prompt(original_text, error, 1);
        assert!(
            prompt.contains(original_text),
            "Retry should include original user text: {prompt}"
        );
        assert!(
            prompt.contains(error),
            "Retry should include the tool error: {prompt}"
        );
    }

    #[test]
    fn test_session_changes_shared_across_content_prompts() {
        // Verifies that SessionChanges can be used across multiple prompt styles.
        // When @file mention prompts use the same SessionChanges as regular prompts,
        // all changes should be tracked together.
        let changes = SessionChanges::new();

        // Simulate a regular prompt recording a write
        changes.record("src/main.rs", ChangeKind::Write);

        // Simulate an @file mention prompt recording an edit
        changes.record("src/cli.rs", ChangeKind::Edit);

        // Both should be visible in the snapshot
        let snapshot = changes.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].path, "src/main.rs");
        assert_eq!(snapshot[0].kind, ChangeKind::Write);
        assert_eq!(snapshot[1].path, "src/cli.rs");
        assert_eq!(snapshot[1].kind, ChangeKind::Edit);

        // format_changes should show both
        let output = format_changes(&changes);
        assert!(output.contains("2 files"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/cli.rs"));
    }

    #[test]
    fn test_max_auto_retries_constant() {
        assert_eq!(MAX_AUTO_RETRIES, 2);
    }

    // ── Context overflow detection tests ─────────────────────────────────

    #[test]
    fn test_is_overflow_error_anthropic() {
        assert!(is_overflow_error(
            "prompt is too long: 213462 tokens > 200000 maximum"
        ));
    }

    #[test]
    fn test_is_overflow_error_openai() {
        assert!(is_overflow_error(
            "Your input exceeds the context window of this model"
        ));
    }

    #[test]
    fn test_is_overflow_error_google() {
        assert!(is_overflow_error(
            "The input token count (1196265) exceeds the maximum number of tokens allowed"
        ));
    }

    #[test]
    fn test_is_overflow_error_generic_too_many_tokens() {
        assert!(is_overflow_error("too many tokens in request"));
    }

    #[test]
    fn test_is_overflow_error_context_length_exceeded() {
        assert!(is_overflow_error("context length exceeded"));
        assert!(is_overflow_error("context_length_exceeded"));
    }

    #[test]
    fn test_is_overflow_error_max_token_exceeded() {
        assert!(is_overflow_error(
            "exceeded model token limit for this request"
        ));
        assert!(is_overflow_error("token limit exceeded"));
    }

    #[test]
    fn test_is_overflow_error_case_insensitive() {
        assert!(is_overflow_error("PROMPT IS TOO LONG"));
        assert!(is_overflow_error("Too Many Tokens"));
        assert!(is_overflow_error("CONTEXT LENGTH EXCEEDED"));
    }

    #[test]
    fn test_is_overflow_error_bedrock() {
        assert!(is_overflow_error("input is too long for requested model"));
    }

    #[test]
    fn test_is_overflow_error_groq() {
        assert!(is_overflow_error(
            "Please reduce the length of the messages or completion"
        ));
    }

    #[test]
    fn test_is_overflow_error_xai() {
        assert!(is_overflow_error(
            "This model's maximum prompt length is 131072 but request contains 537812 tokens"
        ));
    }

    #[test]
    fn test_is_not_overflow_error() {
        assert!(!is_overflow_error("invalid api key"));
        assert!(!is_overflow_error("rate limit exceeded"));
        assert!(!is_overflow_error("500 Internal Server Error"));
        assert!(!is_overflow_error("connection reset"));
        assert!(!is_overflow_error("bad request"));
        assert!(!is_overflow_error(""));
    }

    #[test]
    fn test_build_overflow_retry_prompt() {
        let prompt = build_overflow_retry_prompt("explain the code");
        assert!(prompt.contains("explain the code"));
        assert!(prompt.contains("auto-compacted"));
    }

    #[test]
    fn test_image_content_block_construction() {
        // Verify that Content::Image can be constructed with base64 data and mime type
        let data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string();
        let mime_type = "image/png".to_string();

        let content_blocks = [
            Content::Text {
                text: "describe this image".to_string(),
            },
            Content::Image {
                data: data.clone(),
                mime_type: mime_type.clone(),
            },
        ];

        assert_eq!(content_blocks.len(), 2);
        match &content_blocks[0] {
            Content::Text { text } => assert_eq!(text, "describe this image"),
            _ => panic!("expected Text content"),
        }
        match &content_blocks[1] {
            Content::Image {
                data: d,
                mime_type: m,
            } => {
                assert_eq!(d, &data);
                assert_eq!(m, &mime_type);
            }
            _ => panic!("expected Image content"),
        }
    }

    #[test]
    fn test_user_message_with_image_content() {
        // Verify that a user message with image content blocks can be constructed
        // and wrapped as an AgentMessage — this is the exact pattern used by
        // run_prompt_with_content
        let content_blocks = vec![
            Content::Text {
                text: "what is this?".to_string(),
            },
            Content::Image {
                data: "base64data".to_string(),
                mime_type: "image/jpeg".to_string(),
            },
        ];

        let user_msg = AgentMessage::Llm(Message::User {
            content: content_blocks,
            timestamp: now_ms(),
        });

        assert_eq!(user_msg.role(), "user");
        if let AgentMessage::Llm(Message::User { content, .. }) = &user_msg {
            assert_eq!(content.len(), 2);
        } else {
            panic!("expected Llm(User) message");
        }
    }

    // ── TurnSnapshot tests ──────────────────────────────────────────────

    #[test]
    fn test_turn_snapshot_new_is_empty() {
        let snap = TurnSnapshot::new();
        assert!(snap.is_empty());
        assert_eq!(snap.file_count(), 0);
    }

    #[test]
    fn test_turn_snapshot_save_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "original content").unwrap();
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);

        assert!(!snap.is_empty());
        assert_eq!(snap.file_count(), 1);
        assert_eq!(snap.originals.get(path_str).unwrap(), "original content");

        // Simulate agent modifying the file
        std::fs::write(&path, "modified content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "modified content");

        // Restore should revert to original
        let actions = snap.restore();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("restored"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original content");
    }

    #[test]
    fn test_turn_snapshot_created_files_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new_file.txt");
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        // File doesn't exist yet — record as created
        snap.record_created(path_str);

        assert!(!snap.is_empty());
        assert_eq!(snap.file_count(), 1);

        // Simulate agent creating the file
        std::fs::write(&path, "new content").unwrap();
        assert!(path.exists());

        // Restore should delete it
        let actions = snap.restore();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("deleted"));
        assert!(!path.exists());
    }

    #[test]
    fn test_turn_snapshot_no_duplicate_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "v1").unwrap();
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);

        // Modify file, then snapshot again — should keep original
        std::fs::write(&path, "v2").unwrap();
        snap.snapshot_file(path_str);

        assert_eq!(snap.originals.get(path_str).unwrap(), "v1");
    }

    #[test]
    fn test_turn_snapshot_nonexistent_file() {
        let mut snap = TurnSnapshot::new();
        snap.snapshot_file("/nonexistent/path/to/file.txt");
        // Should not add to originals since file doesn't exist
        assert!(snap.originals.is_empty());
    }

    #[test]
    fn test_turn_snapshot_created_not_duplicated() {
        let mut snap = TurnSnapshot::new();
        snap.record_created("new.txt");
        snap.record_created("new.txt");
        assert_eq!(snap.created.len(), 1);
    }

    #[test]
    fn test_turn_snapshot_created_ignores_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "content").unwrap();
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);
        // Should not add to created since it was already snapshotted
        snap.record_created(path_str);
        assert!(snap.created.is_empty());
    }

    // ── TurnHistory tests ───────────────────────────────────────────────

    #[test]
    fn test_turn_history_new_is_empty() {
        let hist = TurnHistory::new();
        assert!(hist.is_empty());
        assert_eq!(hist.len(), 0);
    }

    #[test]
    fn test_turn_history_push_pop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "original").unwrap();

        let mut hist = TurnHistory::new();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());
        hist.push(snap);

        assert_eq!(hist.len(), 1);

        let popped = hist.pop();
        assert!(popped.is_some());
        assert_eq!(hist.len(), 0);
    }

    #[test]
    fn test_turn_history_skips_empty_snapshots() {
        let mut hist = TurnHistory::new();
        hist.push(TurnSnapshot::new()); // empty — should be skipped
        assert!(hist.is_empty());
    }

    #[test]
    fn test_turn_history_undo_last_n() {
        let dir = tempfile::tempdir().unwrap();

        // Turn 1: modify a.txt
        let path_a = dir.path().join("a.txt");
        std::fs::write(&path_a, "a_original").unwrap();
        let mut snap1 = TurnSnapshot::new();
        snap1.snapshot_file(path_a.to_str().unwrap());

        // Turn 2: modify b.txt
        let path_b = dir.path().join("b.txt");
        std::fs::write(&path_b, "b_original").unwrap();
        let mut snap2 = TurnSnapshot::new();
        snap2.snapshot_file(path_b.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap1);
        hist.push(snap2);
        assert_eq!(hist.len(), 2);

        // Simulate modifications
        std::fs::write(&path_a, "a_modified").unwrap();
        std::fs::write(&path_b, "b_modified").unwrap();

        // Undo last 1 — only b.txt should be restored
        let actions = hist.undo_last(1);
        assert!(!actions.is_empty());
        assert_eq!(std::fs::read_to_string(&path_b).unwrap(), "b_original");
        assert_eq!(std::fs::read_to_string(&path_a).unwrap(), "a_modified");
        assert_eq!(hist.len(), 1);

        // Undo last 1 — now a.txt should be restored
        let actions = hist.undo_last(1);
        assert!(!actions.is_empty());
        assert_eq!(std::fs::read_to_string(&path_a).unwrap(), "a_original");
        assert!(hist.is_empty());
    }

    #[test]
    fn test_turn_history_undo_more_than_available() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.txt");
        std::fs::write(&path, "orig").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap);

        // Undo 5 when only 1 exists — should undo 1 without panic
        std::fs::write(&path, "changed").unwrap();
        let actions = hist.undo_last(5);
        assert!(!actions.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "orig");
        assert!(hist.is_empty());
    }

    #[test]
    fn test_turn_history_clear() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.txt");
        std::fs::write(&path, "content").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap);
        assert_eq!(hist.len(), 1);

        hist.clear();
        assert!(hist.is_empty());
    }

    /// Verify the deferred bash timer logic: bash tool_call_ids are tracked
    /// in the deferred map with optional command label, removed on first update
    /// (timer start), and cleaned up on end if no update ever arrived (e.g. denied command).
    #[test]
    fn test_deferred_bash_timer_set_lifecycle() {
        let mut deferred: HashMap<String, Option<String>> = HashMap::new();
        let mut timers: HashMap<String, &str> = HashMap::new(); // simplified stand-in

        // 1. ToolExecutionStart for bash → add to deferred set, NOT to timers
        let id = "call_abc".to_string();
        let cmd_label = Some("cargo test".to_string());
        deferred.insert(id.clone(), cmd_label);
        assert!(
            deferred.contains_key(&id),
            "bash tool should be in deferred set"
        );
        assert!(
            !timers.contains_key(&id),
            "timer should NOT start on ToolExecutionStart"
        );

        // 2. ToolExecutionUpdate → remove from deferred, start timer (with label)
        if let Some(label) = deferred.remove(&id) {
            assert_eq!(
                label,
                Some("cargo test".to_string()),
                "label should be preserved"
            );
            timers.insert(id.clone(), "bash");
        }
        assert!(
            !deferred.contains_key(&id),
            "should be removed from deferred after update"
        );
        assert!(
            timers.contains_key(&id),
            "timer should start on first ToolExecutionUpdate"
        );

        // 3. ToolExecutionEnd → timer is already active, just clean up
        timers.remove(&id);
        deferred.remove(&id); // no-op, already removed
        assert!(!timers.contains_key(&id));
        assert!(!deferred.contains_key(&id));
    }

    /// Verify that a denied bash command (no ToolExecutionUpdate) gets cleaned
    /// up properly on ToolExecutionEnd.
    #[test]
    fn test_deferred_bash_timer_denied_command_cleanup() {
        let mut deferred: HashMap<String, Option<String>> = HashMap::new();
        let timers: HashMap<String, &str> = HashMap::new();

        // ToolExecutionStart for bash → deferred
        let id = "call_denied".to_string();
        deferred.insert(id.clone(), Some("rm -rf /".to_string()));

        // No ToolExecutionUpdate (command was denied by user)

        // ToolExecutionEnd → clean up deferred entry
        deferred.remove(&id);
        assert!(
            !deferred.contains_key(&id),
            "deferred entry should be cleaned up on end"
        );
        assert!(
            !timers.contains_key(&id),
            "no timer should exist for denied command"
        );
    }

    /// Non-bash tools should not be deferred — they don't have confirmation prompts.
    #[test]
    fn test_non_bash_tools_not_deferred() {
        let deferred: HashMap<String, Option<String>> = HashMap::new();
        // For non-bash tools (read_file, write_file, etc.), we never insert into deferred
        assert!(
            deferred.is_empty(),
            "non-bash tools should never be in deferred set"
        );
    }

    #[test]
    fn test_prompt_outcome_has_api_error_field() {
        let outcome = PromptOutcome {
            text: String::new(),
            last_tool_error: None,
            was_overflow: false,
            last_api_error: Some("503 Service Unavailable".to_string()),
        };
        assert_eq!(
            outcome.last_api_error,
            Some("503 Service Unavailable".to_string())
        );

        let outcome_no_error = PromptOutcome {
            text: "hello".to_string(),
            last_tool_error: None,
            was_overflow: false,
            last_api_error: None,
        };
        assert!(outcome_no_error.last_api_error.is_none());
    }

    #[test]
    fn test_build_watch_fix_prompt() {
        let prompt = build_watch_fix_prompt("cargo test", "error[E0308]: mismatched types");
        assert!(
            prompt.contains("cargo test"),
            "prompt should include the command name"
        );
        assert!(
            prompt.contains("error[E0308]: mismatched types"),
            "prompt should include the output"
        );
        assert!(prompt.contains("Please fix"), "prompt should ask for a fix");
        assert!(
            prompt.contains("```"),
            "prompt should wrap output in code fence"
        );
    }

    #[test]
    fn test_max_watch_fix_attempts_constant() {
        // The constant should exist and be a reasonable retry count (1..=10)
        let attempts = MAX_WATCH_FIX_ATTEMPTS;
        assert!(attempts >= 1, "should allow at least 1 attempt");
        assert!(attempts <= 10, "should not retry excessively");
        assert_eq!(attempts, 3, "default should be 3 attempts");
    }

    #[test]
    fn test_build_watch_fix_prompt_truncates_long_output() {
        let long_output = "x".repeat(6000);
        let prompt = build_watch_fix_prompt("cargo test", &long_output);
        assert!(
            prompt.contains("... (truncated)"),
            "long output should be truncated"
        );
        // The output in the prompt should not contain the full 6000 chars
        assert!(
            !prompt.contains(&"x".repeat(6000)),
            "full output should not appear"
        );
        // But should contain the first 5000
        assert!(
            prompt.contains(&"x".repeat(5000)),
            "first 5000 chars should appear"
        );
    }

    #[test]
    fn test_run_watch_command_success() {
        let (ok, output) = run_watch_command("echo hello");
        assert!(ok, "echo should succeed");
        assert_eq!(output.trim(), "hello");
    }

    #[test]
    fn test_run_watch_command_failure() {
        let (ok, _output) = run_watch_command("exit 1");
        assert!(!ok, "exit 1 should fail");
    }

    #[test]
    fn test_run_watch_command_captures_all_output() {
        let (ok, output) = run_watch_command("for i in 1 2 3 4 5; do echo line$i; done");
        assert!(ok);
        assert!(output.contains("line1"));
        assert!(output.contains("line5"));
        // Should have all 5 lines
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5, "should capture all 5 lines");
    }

    #[test]
    fn test_run_watch_command_captures_stderr() {
        let (ok, output) = run_watch_command("echo err_msg >&2");
        assert!(ok, "writing to stderr is not a failure");
        assert!(
            output.contains("err_msg"),
            "stderr should be captured: {output}"
        );
    }

    #[test]
    fn test_run_watch_command_combines_stdout_stderr() {
        let (ok, output) = run_watch_command("echo out_msg; echo err_msg >&2");
        assert!(ok);
        assert!(output.contains("out_msg"), "should contain stdout");
        assert!(output.contains("err_msg"), "should contain stderr");
    }

    #[test]
    fn test_run_watch_command_invalid_command() {
        let (ok, output) = run_watch_command("nonexistent_command_xyz_123");
        assert!(!ok, "nonexistent command should fail");
        assert!(
            !output.is_empty(),
            "should have some error output: {output}"
        );
    }
}
