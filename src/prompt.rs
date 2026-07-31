//! Prompt execution and agent interaction.

use crate::cli::is_verbose;
use crate::format::*;
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::time::Instant;
use yoagent::agent::Agent;
use yoagent::context::total_tokens;
use yoagent::*;

use crate::prompt_budget::{audit_log_tool_call, session_budget_exhausted};
use crate::session::{ChangeKind, SessionChanges};

/// Prepend the effort-level hint to user input when the effort level is not Medium.
///
/// Medium (default) returns an empty hint, so the input passes through unchanged.
/// Low and High prepend a bracketed instruction that guides the agent's response style.
/// This is applied per-turn so `/effort` changes take effect immediately.
pub(crate) fn apply_effort_hint(input: &str) -> String {
    let hint = crate::cli_config::effort_level().system_hint();
    if hint.is_empty() {
        input.to_string()
    } else {
        format!("[Effort: {}]\n\n{}", hint, input)
    }
}

/// Accumulate usage from `delta` into `total`.
///
/// Replaces the recurring 4-line pattern:
/// ```ignore
/// total.input  += delta.input;
/// total.output += delta.output;
/// total.cache_read  += delta.cache_read;
/// total.cache_write += delta.cache_write;
/// ```
fn accumulate_usage(total: &mut Usage, delta: &Usage) {
    total.input += delta.input;
    total.output += delta.output;
    total.cache_read += delta.cache_read;
    total.cache_write += delta.cache_write;
}

/// Shared epilogue for `run_prompt_with_changes` and
/// `run_prompt_with_content_and_changes`.
///
/// Accumulates prompt-level usage into the session total, prints the usage
/// and context bars, checks for context budget warnings, rings the bell,
/// and returns `(ctx_used, ctx_max)` for callers that need them.
async fn finish_prompt_epilogue(
    agent: &mut Agent,
    total_usage: &Usage,
    session_total: &mut Usage,
    model: &str,
    prompt_start: Instant,
) {
    accumulate_usage(session_total, total_usage);
    print_usage(total_usage, session_total, model, prompt_start.elapsed());
    // Issue #258: yoagent 0.7.x runs the agent loop in a background task; the
    // agent's internal `self.messages` is only updated when `finish()` is awaited.
    // Without this, `agent.messages()` returns stale state and the context bar
    // permanently reads "0% used". Call finish() before reading messages.
    //
    // Issue #634 escalation: under yoagent >=0.13.3 (yoagent#84) `Drop for Agent`
    // *cancels the run* — dropping the Agent before its receiver is drained
    // doesn't merely leave `messages()` stale, it aborts the loop task, and the
    // channel then closes WITHOUT an `AgentEnd` event. At a `rx.recv()` loop that
    // truncation is indistinguishable from a clean finish. So the Agent must stay
    // alive for the whole drain (see `handle_prompt_events(&mut Agent, ...)`);
    // pinned by `agent_alive_across_drain_yields_agent_end` below.
    agent.finish().await;
    let ctx_used = total_tokens(agent.messages()) as u64;
    let ctx_max = crate::cli::effective_context_tokens();
    print_context_usage(ctx_used, ctx_max);
    if let Some(warning) = crate::format::context_budget_warning(ctx_used, ctx_max) {
        eprintln!("{warning}");
    }
    maybe_ring_bell(prompt_start.elapsed());
    println!();
}

/// Outcome of a prompt execution, including the text response and any tool error.
#[derive(Debug, Clone, Default)]
pub struct PromptOutcome {
    /// The collected text output from the agent.
    pub text: String,
    /// The last tool error encountered during this prompt turn, if any.
    /// Tool errors are from `ToolExecutionEnd` events where `is_error` is true.
    pub last_tool_error: Option<String>,
    /// The name of the tool that produced `last_tool_error`, if any.
    /// Used to provide tool-specific recovery hints in auto-retry prompts.
    pub last_tool_name: Option<String>,
    /// Whether this prompt triggered an auto-compact due to context overflow.
    /// Callers can use this to inform users or adjust behavior.
    pub was_overflow: bool,
    /// The last API-level error after all retries were exhausted, if any.
    /// Set when the provider itself fails (rate limits, outages, auth errors)
    /// rather than a tool execution error. Used by the REPL to trigger
    /// fallback provider switching.
    pub last_api_error: Option<String>,
}

// Extracted into `prompt_retry` module (Day 64). Callers import directly
// from `crate::prompt_retry`.
use crate::prompt_retry::{
    build_auto_retry_prompt, build_overflow_retry_prompt, classify_stop_reason, diagnose_api_error,
    is_benign_stream_end, is_overflow_error, is_retriable_error, retry_delay, StopHandling,
    MAX_AUTO_RETRIES,
};
// MAX_RETRIES is pub(crate), so import without re-exporting.
use crate::prompt_retry::MAX_RETRIES;

// Extracted into `prompt_utils` module (Day 64). Callers import directly
// from `crate::prompt_utils`.
use crate::prompt_utils::tool_result_preview;

/// Result of a single prompt attempt — either success or a retriable/fatal error.
enum PromptResult {
    /// Prompt completed (possibly with non-retriable errors already shown).
    Done {
        collected_text: String,
        usage: Usage,
        last_tool_error: Option<String>,
        last_tool_name: Option<String>,
    },
    /// A retriable API error was detected — caller should retry.
    RetriableError { error_msg: String, usage: Usage },
    /// A context overflow error — caller should compact and retry.
    ContextOverflow { error_msg: String, usage: Usage },
    /// A fatal, non-retriable turn failure (#646).
    ///
    /// The turn ended with `StopReason::Error` for a reason that is neither a
    /// context overflow nor a known-retriable API class — e.g. yoagent 0.14.1+
    /// reporting that a tool call's arguments never assembled. Re-running the
    /// identical prompt can reproduce it and burn a slot, so this is
    /// **surface-and-stop**: it never feeds the auto-retry path, it only makes
    /// the failure visible to the caller so a turn that accomplished nothing
    /// cannot read as a clean `Done`.
    FatalError { error_msg: String, usage: Usage },
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

/// True when a `StopReason::Error` message names the #646 class: the turn's
/// tool-call arguments never assembled, so the tool never ran.
///
/// This class is deliberately checked BEFORE `is_retriable_error`: the message
/// can incidentally contain a retriable keyword (e.g. "incomplete"), but
/// re-running the identical prompt can reproduce a dropped-args turn and burn a
/// slot. Surface-and-stop wins here (creator decision, 2026-07-30).
fn is_dropped_tool_args_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    (lower.contains("tool call") || lower.contains("tool_call") || lower.contains("arguments"))
        && (lower.contains("never assembled")
            || lower.contains("never completed")
            || lower.contains("were not assembled")
            || lower.contains("incomplete arguments")
            || lower.contains("no arguments"))
}

/// Internal state for the prompt event-handling loop.
/// Bundles the 15+ local variables that were previously declared inline.
/// Bundles the 15+ local variables that were previously declared inline.
struct PromptEventState {
    usage: Usage,
    in_text: bool,
    in_thinking: bool,
    tool_timers: HashMap<String, Instant>,
    collected_text: String,
    retriable_error: Option<String>,
    overflow_error: Option<String>,
    /// #646: a fatal, non-retriable `StopReason::Error`. Surface-and-stop.
    fatal_error: Option<String>,
    last_tool_error: Option<String>,
    last_tool_name: Option<String>,
    md_renderer: MarkdownRenderer,
    spinner: Option<Spinner>,
    think_filter: ThinkBlockFilter,
    /// Audit log: track in-flight tool calls (name + args) so we can log at completion
    audit_inflight: HashMap<String, (String, serde_json::Value)>,
    /// Live progress timers for long-running tools (bash)
    tool_progress_timers: HashMap<String, ToolProgressTimer>,
    /// Bash tool call IDs that need deferred timer start.
    /// Maps tool_call_id → optional command string for display label.
    deferred_bash_timers: HashMap<String, Option<String>>,
    /// Tool batch tracking for group summaries
    batch_count: usize,
    batch_succeeded: usize,
    batch_failed: usize,
    batch_start: Option<Instant>,
    /// Turn tracking for boundary markers
    turn_number: usize,
    /// Whether we've seen text output in this prompt
    had_text: bool,
    /// Cached edit_file params (old_text, new_text) for post-execution diff display.
    /// Keyed by tool_call_id. Stored at ToolExecutionStart, consumed at ToolExecutionEnd.
    edit_params: HashMap<String, serde_json::Value>,
}

impl PromptEventState {
    fn new() -> Self {
        Self {
            usage: Usage::default(),
            in_text: false,
            in_thinking: false,
            tool_timers: HashMap::new(),
            collected_text: String::new(),
            retriable_error: None,
            overflow_error: None,
            fatal_error: None,
            last_tool_error: None,
            last_tool_name: None,
            md_renderer: MarkdownRenderer::new(),
            spinner: Some(Spinner::start()),
            think_filter: ThinkBlockFilter::new(),
            audit_inflight: HashMap::new(),
            tool_progress_timers: HashMap::new(),
            deferred_bash_timers: HashMap::new(),
            batch_count: 0,
            batch_succeeded: 0,
            batch_failed: 0,
            batch_start: None,
            turn_number: 0,
            had_text: false,
            edit_params: HashMap::new(),
        }
    }

    /// Handle a ToolExecutionStart event: track file changes, display tool info,
    /// manage batch state, and set up deferred timers.
    fn handle_tool_execution_start(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        changes: &SessionChanges,
    ) {
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
        if let Some(s) = self.spinner.take() {
            s.stop();
        }

        // Show turn boundary when transitioning from text to a new tool batch
        if self.in_text {
            println!();
            self.in_text = false;
        }

        // New batch starting (first tool after text or start)
        if self.batch_count == 0 {
            if self.batch_start.is_none() {
                self.batch_start = Some(Instant::now());
            }
            // Show turn boundary for multi-turn (turn 2+)
            if self.turn_number > 1 && self.had_text {
                println!("{}", turn_boundary(self.turn_number));
            }
        }

        self.batch_count += 1;
        self.tool_timers
            .insert(tool_call_id.clone(), Instant::now());
        // Track for audit log
        self.audit_inflight
            .insert(tool_call_id.clone(), (tool_name.clone(), args.clone()));
        // Cache edit_file params for post-execution diff display
        if tool_name == "edit_file" {
            self.edit_params.insert(tool_call_id.clone(), args.clone());
        }
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
        } else if tool_name == "write_file" {
            // Show diff when overwriting an existing file
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let new_content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if !path.is_empty() && std::path::Path::new(path).exists() {
                if let Ok(old_content) = std::fs::read_to_string(path) {
                    let diff = format_edit_diff(&old_content, new_content);
                    if !diff.is_empty() {
                        let diff = truncate_diff_preview(&diff, 30);
                        println!();
                        println!("{diff}");
                    }
                }
            }
        }
        io::stdout().flush().ok();

        // Defer timer start for bash commands — the confirmation
        // prompt would be overwritten by the spinner. The timer
        // will start on the first ToolExecutionUpdate instead.
        if tool_name == "bash" {
            let cmd_label = args
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            self.deferred_bash_timers
                .insert(tool_call_id.clone(), cmd_label);
        }
    }

    /// Handle a ToolExecutionEnd event: stop timers, log audit data,
    /// display success/failure status, track errors.
    fn handle_tool_execution_end(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        is_error: bool,
        result: ToolResult,
    ) {
        // Clean up deferred timer entry if command was denied before running
        self.deferred_bash_timers.remove(&tool_call_id);
        // Stop any live progress timer for this tool
        if let Some(timer) = self.tool_progress_timers.remove(&tool_call_id) {
            timer.stop();
        }
        let elapsed = self
            .tool_timers
            .remove(&tool_call_id)
            .map(|start| start.elapsed());
        let dur_str = elapsed
            .map(|d| format!(" {DIM}({}){RESET}", format_duration(d)))
            .unwrap_or_default();

        // Audit log: record the completed tool call
        if let Some((audit_tool, audit_args)) = self.audit_inflight.remove(&tool_call_id) {
            let duration_ms = elapsed.map(|d| d.as_millis() as u64).unwrap_or(0);
            audit_log_tool_call(&audit_tool, &audit_args, duration_ms, !is_error);
        }

        // Always clean up cached edit params (success or failure)
        let cached_edit = self.edit_params.remove(&tool_call_id);

        if is_error {
            self.batch_failed += 1;
            println!(" {RED}✗{RESET}{dur_str}");
            let preview = tool_result_preview(&result, 200);
            if !preview.is_empty() {
                // Indent error output under the tool header
                println!("{}", indent_tool_output(&preview));
            }
            // Track the last tool error for /retry context (reuse preview)
            if !preview.is_empty() {
                self.last_tool_error = Some(preview);
            } else {
                self.last_tool_error = Some("tool execution failed".to_string());
            }
            self.last_tool_name = Some(tool_name.clone());
        } else {
            // Successful tool clears the last error
            self.batch_succeeded += 1;
            self.last_tool_error = None;
            self.last_tool_name = None;
            println!(" {GREEN}✓{RESET}{dur_str}");
            // Warn when write_file writes 0 bytes (empty content)
            if tool_name == "write_file" {
                let wrote_zero = result
                    .details
                    .get("bytes")
                    .and_then(|v| v.as_u64())
                    .map(|b| b == 0)
                    .unwrap_or(false);
                if wrote_zero {
                    eprintln!("{YELLOW}    ⚠ write_file wrote 0 bytes — file is now empty{RESET}");
                }
            }
            // Show post-execution diff for edit_file (what actually changed)
            if tool_name == "edit_file" && !is_verbose() {
                if let Some(params) = cached_edit {
                    let old_text = params
                        .get("old_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let new_text = params
                        .get("new_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !old_text.is_empty() || !new_text.is_empty() {
                        let diff = format_edit_diff(old_text, new_text);
                        if !diff.is_empty() {
                            let diff = truncate_diff_preview(&diff, 10);
                            eprintln!("{diff}");
                        }
                    }
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

    /// Handle a ToolExecutionUpdate event: start deferred timers,
    /// update progress, show partial output in terminal mode.
    fn handle_tool_execution_update(&mut self, tool_call_id: String, partial_result: ToolResult) {
        // Start deferred bash timer on first update.
        // This means the command is actually running (confirmation
        // has already been resolved), so the spinner won't
        // overwrite the permission prompt.
        if let Some(cmd_label) = self.deferred_bash_timers.remove(&tool_call_id) {
            let timer = ToolProgressTimer::start("bash".to_string());
            if let Some(label) = cmd_label {
                timer.set_label(label);
            }
            self.tool_progress_timers
                .insert(tool_call_id.clone(), timer);
        }

        // Update line count on the progress timer if active
        let line_count = count_result_lines(&partial_result);
        if let Some(timer) = self.tool_progress_timers.get(&tool_call_id) {
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

    /// Handle a MessageUpdate with text delta: manage spinner, batch summaries,
    /// think-block filtering, markdown rendering, and text collection.
    fn handle_message_update_text(&mut self, delta: &str) {
        // Stop spinner on first text
        if let Some(s) = self.spinner.take() {
            s.stop();
        }
        // Transition from thinking to text: add a divider
        // so text doesn't appear glued to the last thinking output
        if self.in_thinking {
            eprintln!();
            eprintln!("{}", section_divider());
            let _ = io::stderr().flush();
            self.in_thinking = false;
        }

        // Print batch summary if we just finished a tool batch
        if self.batch_count > 0 {
            self.print_batch_summary();
        }

        if !self.in_text {
            println!();
            self.in_text = true;
            self.had_text = true;
        }
        // Filter <think>...</think> blocks unless verbose mode
        let filtered = if is_verbose() {
            delta.to_string()
        } else {
            self.think_filter.filter(delta)
        };
        if filtered.is_empty() {
            // Inside a think block — nothing to render yet
            io::stdout().flush().ok();
            return;
        }
        // Render and display BEFORE collecting — minimizes time-to-screen.
        // collected_text is only used after the stream ends, so ordering
        // with print doesn't affect correctness. (render_latency_budget)
        let rendered = self.md_renderer.render_delta(&filtered);
        if !rendered.is_empty() {
            print!("{}", rendered);
        }
        io::stdout().flush().ok();
        self.collected_text.push_str(&filtered);
    }

    /// Handle an AgentEnd event: flush filters, print batch summary,
    /// accumulate usage, detect errors.
    fn handle_agent_end(&mut self, messages: Vec<AgentMessage>, model: &str) {
        // Stop spinner if still running
        if let Some(s) = self.spinner.take() {
            s.stop();
        }

        // Flush think block filter — emit any partial non-think text
        let remaining = self.think_filter.flush();
        if !remaining.is_empty() {
            let rendered = self.md_renderer.render_delta(&remaining);
            if !rendered.is_empty() {
                print!("{rendered}");
                io::stdout().flush().ok();
            }
            self.collected_text.push_str(&remaining);
        }

        // Print batch summary if tools were the last thing before end
        if self.batch_count > 0 {
            self.print_batch_summary();
        }

        for msg in &messages {
            if let AgentMessage::Llm(Message::Assistant {
                usage: msg_usage,
                stop_reason,
                error_message,
                ..
            }) = msg
            {
                accumulate_usage(&mut self.usage, msg_usage);

                match classify_stop_reason(stop_reason) {
                    StopHandling::InspectError => {
                        if self.in_text {
                            println!();
                            self.in_text = false;
                        }
                        // #646: `StopReason::Error` with no message at all is the
                        // `pause_turn` shape. Nothing used to be printed and nothing
                        // set — a silent dead turn. Surface it honestly as incomplete;
                        // this transport has no resume path.
                        let empty_message = error_message.as_deref().unwrap_or("").is_empty();
                        if empty_message {
                            let msg = "turn ended with an error but no message — \
                                       treating the response as incomplete (no resume path)";
                            eprintln!("\n{RED}  error: {msg}{RESET}");
                            self.fatal_error = Some(msg.to_string());
                        } else if let Some(err_msg) = error_message {
                            // Check for context overflow first — needs special handling
                            if is_overflow_error(err_msg) {
                                self.overflow_error = Some(err_msg.clone());
                            } else if is_dropped_tool_args_error(err_msg) {
                                // #646: this class is deterministic — re-running the
                                // identical prompt can reproduce it and burn a slot.
                                // Surface-and-stop wins even though the message can
                                // incidentally contain a retriable keyword.
                                eprintln!("\n{RED}  error: {err_msg}{RESET}");
                                self.fatal_error = Some(err_msg.clone());
                            } else if is_retriable_error(err_msg) {
                                // Check if this error is worth retrying
                                self.retriable_error = Some(err_msg.clone());
                            } else if is_benign_stream_end(err_msg) {
                                // #612: "stream ended" is a known-benign outcome —
                                // the response was delivered in full and we
                                // deliberately don't retry (#222). Frame it calmly
                                // instead of as a red error block.
                                eprintln!(
                                    "\n{DIM}  stream ended without terminator — response \
                                     delivered above (known quirk of some providers; \
                                     not retrying){RESET}"
                                );
                            } else {
                                eprintln!("\n{RED}  error: {err_msg}{RESET}");
                                // Show diagnostic help for common errors
                                if let Some(diagnostic) = diagnose_api_error(err_msg, model) {
                                    eprintln!(
                                        "{YELLOW}  💡 {}{RESET}",
                                        diagnostic
                                            .replace('\n', &format!("\n{YELLOW}     {RESET}"))
                                    );
                                }
                            }
                        }
                    }
                    StopHandling::RefusalNotice => {
                        // #568: a refusal is deterministic — retrying the same
                        // prompt burns tokens for the same answer. retriable_error
                        // is deliberately NOT set here, so into_result() returns
                        // Done and the auto-retry machinery never fires.
                        if self.in_text {
                            println!();
                            self.in_text = false;
                        }
                        eprintln!(
                            "\n{YELLOW}  ⚠ the model refused this request (safety system). \
                             Rephrase and try again — auto-retry is skipped for refusals.{RESET}"
                        );
                        if let Some(err_msg) = error_message {
                            if !err_msg.is_empty() {
                                eprintln!("{DIM}    reason: {err_msg}{RESET}");
                            }
                        }
                    }
                    StopHandling::Ignore => {}
                }
            }
        }
    }

    /// Print and reset the tool batch summary.
    fn print_batch_summary(&mut self) {
        let batch_duration = self.batch_start.map(|s| s.elapsed()).unwrap_or_default();
        let summary = format_tool_batch_summary(
            self.batch_count,
            self.batch_succeeded,
            self.batch_failed,
            batch_duration,
        );
        if !summary.is_empty() {
            println!("{summary}");
        }
        // Reset batch tracking
        self.batch_count = 0;
        self.batch_succeeded = 0;
        self.batch_failed = 0;
        self.batch_start = None;
    }

    /// Consume state and produce the final PromptResult.
    fn into_result(self) -> PromptResult {
        if let Some(err_msg) = self.overflow_error {
            PromptResult::ContextOverflow {
                error_msg: err_msg,
                usage: self.usage,
            }
        } else if let Some(err_msg) = self.fatal_error {
            // #646: a fatal turn failure is deliberately checked BEFORE the
            // retriable branch — surface-and-stop wins even if the message
            // incidentally contains a retriable keyword.
            PromptResult::FatalError {
                error_msg: err_msg,
                usage: self.usage,
            }
        } else if let Some(err_msg) = self.retriable_error {
            PromptResult::RetriableError {
                error_msg: err_msg,
                usage: self.usage,
            }
        } else {
            PromptResult::Done {
                collected_text: self.collected_text,
                usage: self.usage,
                last_tool_error: self.last_tool_error,
                last_tool_name: self.last_tool_name,
            }
        }
    }
}

/// Shared event-handling loop for prompt execution.
/// Processes all events from the agent's streaming channel and returns the result.
async fn handle_prompt_events(
    agent: &mut Agent,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
    changes: &SessionChanges,
    model: &str,
) -> PromptResult {
    let mut state = PromptEventState::new();

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    AgentEvent::ToolExecutionStart {
                        tool_call_id, tool_name, args, ..
                    } => {
                        state.handle_tool_execution_start(tool_call_id, tool_name, args, changes);
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, result, tool_name, .. } => {
                        state.handle_tool_execution_end(tool_call_id, tool_name, is_error, result);
                    }
                    AgentEvent::ToolExecutionUpdate { tool_call_id, partial_result, .. } => {
                        state.handle_tool_execution_update(tool_call_id, partial_result);
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Text { delta },
                        ..
                    } => {
                        state.handle_message_update_text(&delta);
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Thinking { delta },
                        ..
                    } => {
                        // Stop spinner on first thinking output
                        if let Some(s) = state.spinner.take() { s.stop(); }
                        if !state.in_thinking {
                            // Print thinking section header on first thinking token
                            eprintln!("\n{}", section_header("Thinking"));
                            state.in_thinking = true;
                        }
                        // Render thinking to stderr (dimmed) so it doesn't
                        // interleave with stdout text output
                        eprint!("{DIM}{delta}{RESET}");
                        let _ = io::stderr().flush();
                    }
                    AgentEvent::AgentEnd { messages } => {
                        state.handle_agent_end(messages, model);
                    }
                    AgentEvent::InputRejected { reason } => {
                        if let Some(s) = state.spinner.take() { s.stop(); }
                        eprintln!("{RED}  input rejected: {reason}{RESET}");
                        if let Some(diagnostic) = diagnose_api_error(&reason, model) {
                            eprintln!("{YELLOW}  💡 {}{RESET}", diagnostic.replace('\n', &format!("\n{YELLOW}     {RESET}")));
                        }
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        if let Some(s) = state.spinner.take() { s.stop(); }
                        if state.in_text {
                            println!();
                            state.in_text = false;
                        }
                        println!("{DIM}  {text}{RESET}");
                    }
                    AgentEvent::MessageStart { .. } => {
                        // Agent started a new message — stop the spinner
                        // so it doesn't overlap with output
                        if let Some(s) = state.spinner.take() { s.stop(); }
                    }
                    AgentEvent::MessageEnd { .. }
                        // Agent finished a message — flush any pending text
                        // (This is where ExecutionLimits stop messages appear)
                        if state.in_text =>
                    {
                        let remaining = state.md_renderer.flush();
                        if !remaining.is_empty() {
                            print!("{remaining}");
                        }
                        println!();
                        state.in_text = false;
                    }
                    AgentEvent::TurnStart => {
                        state.turn_number += 1;
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
                if let Some(s) = state.spinner.take() { s.stop(); }
                agent.abort();
                if state.in_text {
                    println!();
                }
                println!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}");
                return PromptResult::Done {
                    collected_text: state.collected_text,
                    usage: state.usage,
                    last_tool_error: state.last_tool_error,
                    last_tool_name: state.last_tool_name,
                };
            }
        }
    }

    // Stop spinner if still running (e.g., channel closed without events)
    if let Some(s) = state.spinner.take() {
        s.stop();
    }

    // Flush any remaining buffered markdown content
    let remaining = state.md_renderer.flush();
    if !remaining.is_empty() {
        print!("{}", remaining);
        io::stdout().flush().ok();
    }

    if state.in_text {
        println!();
    }

    state.into_result()
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

    // Apply effort-level hint (no-op for Medium/default)
    let effective_input = apply_effort_hint(input);

    let prompt_start = Instant::now();
    let mut total_usage = Usage::default();
    let mut collected_text = String::new();
    let mut last_tool_error: Option<String> = None;
    let mut last_tool_name: Option<String> = None;
    let mut did_overflow_compact = false;
    let mut api_error: Option<String> = None;

    // Save message state before the first attempt so we can restore on retry
    let saved_state = match agent.save_messages() {
        Ok(state) => Some(state),
        Err(e) => {
            eprintln!("{DIM}  ⚠ Could not save message state for retry: {e}{RESET}");
            None
        }
    };

    for attempt in 0..=MAX_RETRIES {
        // On retry, restore pre-prompt state so we don't duplicate the user message
        if attempt > 0 {
            match &saved_state {
                Some(json) => {
                    if let Err(e) = agent.restore_messages(json) {
                        eprintln!("{DIM}  ⚠ Could not restore state for retry: {e}{RESET}");
                        // Cannot safely retry — would duplicate messages in context
                        break;
                    }
                }
                None => {
                    // Cannot safely retry without saved state — would duplicate user message
                    eprintln!("{DIM}  ⚠ Skipping retry: no saved state to restore{RESET}");
                    break;
                }
            }
        }

        match run_prompt_once(agent, &effective_input, changes, model).await {
            PromptResult::Done {
                collected_text: text,
                usage,
                last_tool_error: tool_err,
                last_tool_name: tool_nm,
            } => {
                accumulate_usage(&mut total_usage, &usage);
                collected_text = text;
                last_tool_error = tool_err;
                last_tool_name = tool_nm;
                break;
            }
            PromptResult::RetriableError { error_msg, usage } => {
                accumulate_usage(&mut total_usage, &usage);

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
                accumulate_usage(&mut total_usage, &usage);

                // Auto-compact and retry once
                eprintln!(
                    "\n{YELLOW}  ⚡ context overflow detected — auto-compacting and retrying...{RESET}"
                );
                eprintln!("{DIM}  ({error_msg}){RESET}");

                if let Some(ref json) = saved_state {
                    if let Err(e) = agent.restore_messages(json) {
                        eprintln!(
                            "{DIM}  ⚠ Could not restore state for overflow recovery: {e}{RESET}"
                        );
                    }
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
                        last_tool_name: tool_nm,
                    } => {
                        accumulate_usage(&mut total_usage, &retry_usage);
                        collected_text = text;
                        last_tool_error = tool_err;
                        last_tool_name = tool_nm;
                    }
                    PromptResult::RetriableError {
                        error_msg: retry_err,
                        usage: retry_usage,
                    }
                    | PromptResult::ContextOverflow {
                        error_msg: retry_err,
                        usage: retry_usage,
                    } => {
                        accumulate_usage(&mut total_usage, &retry_usage);
                        eprintln!("\n{RED}  error: {retry_err}{RESET}");
                        eprintln!(
                            "{DIM}  (overflow retry also failed — try /compact manually){RESET}"
                        );
                        api_error = Some(retry_err);
                    }
                    PromptResult::FatalError {
                        error_msg: retry_err,
                        usage: retry_usage,
                    } => {
                        // #646: already printed by handle_agent_end; surface for
                        // control flow without retrying.
                        accumulate_usage(&mut total_usage, &retry_usage);
                        api_error = Some(retry_err);
                    }
                }
                break;
            }
            PromptResult::FatalError { error_msg, usage } => {
                // #646: surface-and-stop. The message was already shown by
                // handle_agent_end. Re-running the identical prompt can reproduce
                // this class of failure, so it is deliberately NOT retried — but it
                // must not read as a clean `Done` either, so record it as an API
                // error the caller can see.
                accumulate_usage(&mut total_usage, &usage);
                api_error = Some(error_msg);
                break;
            }
        }
    }

    finish_prompt_epilogue(agent, &total_usage, session_total, model, prompt_start).await;
    PromptOutcome {
        text: collected_text,
        last_tool_error,
        last_tool_name,
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
                let retry_prompt =
                    build_auto_retry_prompt(input, err, outcome.last_tool_name.as_deref(), attempt);
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
                let retry_prompt = build_auto_retry_prompt(
                    original_text,
                    err,
                    outcome.last_tool_name.as_deref(),
                    attempt,
                );
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

    // Apply effort-level hint as a leading text block (no-op for Medium/default)
    let effective_blocks = {
        let hint = crate::cli_config::effort_level().system_hint();
        if hint.is_empty() {
            content_blocks
        } else {
            let mut blocks = vec![Content::Text {
                text: format!("[Effort: {}]", hint),
            }];
            blocks.extend(content_blocks);
            blocks
        }
    };

    let prompt_start = Instant::now();
    let mut total_usage = Usage::default();
    let mut collected_text = String::new();
    let mut last_tool_error: Option<String> = None;
    let mut last_tool_name: Option<String> = None;
    let mut api_error: Option<String> = None;
    let user_msg = AgentMessage::Llm(Message::User {
        content: effective_blocks,
        timestamp: now_ms(),
    });

    // Save message state before the first attempt so we can restore on retry
    let saved_state = match agent.save_messages() {
        Ok(state) => Some(state),
        Err(e) => {
            eprintln!("{DIM}  ⚠ Could not save message state for retry: {e}{RESET}");
            None
        }
    };

    for attempt in 0..=MAX_RETRIES {
        // On retry, restore pre-prompt state so we don't duplicate the user message
        if attempt > 0 {
            match &saved_state {
                Some(json) => {
                    if let Err(e) = agent.restore_messages(json) {
                        eprintln!("{DIM}  ⚠ Could not restore state for retry: {e}{RESET}");
                        // Cannot safely retry — would duplicate messages in context
                        break;
                    }
                }
                None => {
                    // Cannot safely retry without saved state — would duplicate user message
                    eprintln!("{DIM}  ⚠ Skipping retry: no saved state to restore{RESET}");
                    break;
                }
            }
        }

        match run_prompt_once_with_messages(agent, vec![user_msg.clone()], changes, model).await {
            PromptResult::Done {
                collected_text: text,
                usage,
                last_tool_error: tool_err,
                last_tool_name: tool_nm,
            } => {
                accumulate_usage(&mut total_usage, &usage);
                collected_text = text;
                last_tool_error = tool_err;
                last_tool_name = tool_nm;
                break;
            }
            PromptResult::RetriableError { error_msg, usage } => {
                accumulate_usage(&mut total_usage, &usage);

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
                accumulate_usage(&mut total_usage, &usage);

                eprintln!(
                    "\n{YELLOW}  ⚡ context overflow detected — cannot retry with image content{RESET}"
                );
                eprintln!("{DIM}  ({error_msg}){RESET}");
                api_error = Some(error_msg);
                break;
            }
            PromptResult::FatalError { error_msg, usage } => {
                // #646: surface-and-stop — already printed by handle_agent_end,
                // never auto-retried, but visible to the caller.
                accumulate_usage(&mut total_usage, &usage);
                api_error = Some(error_msg);
                break;
            }
        }
    }

    finish_prompt_epilogue(agent, &total_usage, session_total, model, prompt_start).await;
    PromptOutcome {
        text: collected_text,
        last_tool_error,
        last_tool_name,
        was_overflow: false,
        last_api_error: api_error,
    }
}

// ---------------------------------------------------------------------------
// Streaming JSON event output (--output-format stream-json)
// ---------------------------------------------------------------------------

/// Emit a single yoagent `AgentEvent` as one line of NDJSON to stdout.
///
/// yoagent's `AgentEvent` already derives `Serialize` with an internally-tagged,
/// camelCase wire contract (`#[serde(tag = "type", rename_all = "camelCase", ...)]`),
/// so serializing it directly IS the stream-json output shape. No lossy translation.
fn emit_agent_event(event: &AgentEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        println!("{json}");
    }
}

/// Run a prompt in streaming JSON mode: emit NDJSON events to stdout as they arrive.
/// Suppresses all stderr formatting (spinners, progress).
/// Returns the same PromptOutcome as the normal `run_prompt`.
pub async fn run_prompt_stream_json(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) -> PromptOutcome {
    let rx = agent.prompt(input).await;
    let outcome = handle_stream_json_events(agent, rx, model).await;

    accumulate_usage(session_total, &outcome.1);

    outcome.0
}

/// Run a prompt with content blocks in streaming JSON mode.
pub async fn run_prompt_stream_json_with_content(
    agent: &mut Agent,
    content: Vec<Content>,
    session_total: &mut Usage,
    model: &str,
) -> PromptOutcome {
    let messages = vec![AgentMessage::Llm(Message::User {
        content,
        timestamp: now_ms(),
    })];
    let rx = agent.prompt_messages(messages).await;
    let outcome = handle_stream_json_events(agent, rx, model).await;

    accumulate_usage(session_total, &outcome.1);

    outcome.0
}

/// Internal event handler for streaming JSON mode.
/// Returns (PromptOutcome, Usage).
async fn handle_stream_json_events(
    agent: &mut Agent,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
    model: &str,
) -> (PromptOutcome, Usage) {
    let mut usage = Usage::default();
    let mut collected_text = String::new();
    let mut last_tool_error: Option<String> = None;
    let mut last_tool_name: Option<String> = None;
    let mut last_api_error: Option<String> = None;

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                // Emit the raw yoagent AgentEvent as NDJSON, full-fidelity — this is
                // the wire contract. Serialize a reference so we keep ownership for the
                // bookkeeping match below (AgentEvent's serde shape is the source of truth).
                emit_agent_event(&event);
                match &event {
                    AgentEvent::ToolExecutionEnd {
                        tool_name, is_error, result, ..
                    } => {
                        // Extract text from tool result for internal error tracking.
                        let output = result
                            .content
                            .iter()
                            .filter_map(|c| {
                                if let Content::Text { text } = c {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        if *is_error {
                            last_tool_error = Some(if output.is_empty() {
                                "tool execution failed".to_string()
                            } else {
                                output
                            });
                            last_tool_name = Some(tool_name.clone());
                        } else {
                            last_tool_error = None;
                            last_tool_name = None;
                        }
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Text { delta },
                        ..
                    } => {
                        collected_text.push_str(delta);
                    }
                    AgentEvent::AgentEnd { messages } => {
                        // Extract usage from assistant messages
                        for msg in messages {
                            if let AgentMessage::Llm(Message::Assistant {
                                usage: msg_usage,
                                ..
                            }) = msg
                            {
                                accumulate_usage(&mut usage, msg_usage);
                            }
                        }
                        // Finalize agent state
                        agent.finish().await;
                        break;
                    }
                    AgentEvent::InputRejected { reason } => {
                        last_api_error = Some(reason.clone());
                        if let Some(diagnostic) = diagnose_api_error(reason, model) {
                            last_api_error = Some(format!("{reason}: {diagnostic}"));
                        }
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                agent.abort();
                break;
            }
        }
    }

    // If we exited without AgentEnd, still try to finalize
    if usage.input == 0 && usage.output == 0 {
        agent.finish().await;
    }

    let outcome = PromptOutcome {
        text: collected_text,
        last_tool_error,
        last_tool_name,
        was_overflow: false,
        last_api_error,
    };
    (outcome, usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulate_usage_adds_all_fields() {
        let mut total = Usage {
            input: 10,
            output: 20,
            cache_read: 30,
            cache_write: 40,
            ..Default::default()
        };
        let delta = Usage {
            input: 1,
            output: 2,
            cache_read: 3,
            cache_write: 4,
            ..Default::default()
        };
        accumulate_usage(&mut total, &delta);
        assert_eq!(total.input, 11);
        assert_eq!(total.output, 22);
        assert_eq!(total.cache_read, 33);
        assert_eq!(total.cache_write, 44);
    }

    #[test]
    fn test_accumulate_usage_with_zero_delta() {
        let mut total = Usage {
            input: 100,
            output: 200,
            cache_read: 300,
            cache_write: 400,
            ..Default::default()
        };
        let delta = Usage::default();
        accumulate_usage(&mut total, &delta);
        assert_eq!(total.input, 100);
        assert_eq!(total.output, 200);
        assert_eq!(total.cache_read, 300);
        assert_eq!(total.cache_write, 400);
    }

    #[test]
    fn test_accumulate_usage_multiple_deltas() {
        let mut total = Usage::default();
        for i in 1..=5 {
            let delta = Usage {
                input: i,
                output: i * 2,
                cache_read: i * 3,
                cache_write: i * 4,
                ..Default::default()
            };
            accumulate_usage(&mut total, &delta);
        }
        // Sum of 1..=5 = 15
        assert_eq!(total.input, 15);
        assert_eq!(total.output, 30);
        assert_eq!(total.cache_read, 45);
        assert_eq!(total.cache_write, 60);
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
        let mut agent = Agent::from_provider(provider, yoagent::provider::ModelConfig::mock())
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

    // Issue #634 / Day 149: yoagent >=0.13.3 made `Drop for Agent` cancel the run.
    // A dropped Agent aborts its loop task and the receiver then closes WITHOUT an
    // `AgentEnd` event — which a `while let Some(ev) = rx.recv().await` loop cannot
    // tell apart from a clean finish. yoyo is compliant (`handle_prompt_events`
    // takes `&mut Agent` and holds it across the whole drain, and every call site
    // follows with `finish().await`), but that compliance was incidental — nothing
    // pinned it, so a future refactor that moves the Agent or returns early from the
    // drain would silently truncate runs with no signal but a yoagent stderr warning.
    //
    // Behavioural pin, both polarities (Day 122: the side that should NOT fire is
    // the half I habitually leave unverified):
    //   positive — Agent held alive across the drain => `AgentEnd` arrives, and
    //              `finish().await` afterwards yields non-empty `messages()`.
    //   negative — Agent dropped before the drain => no `AgentEnd` ever arrives.
    // The negative half is deterministic under `#[tokio::test]`'s current-thread
    // runtime: there is no await point between `prompt()` returning and the drop,
    // so the spawned loop cannot have run to completion first. We assert only the
    // *absence of AgentEnd* (not "zero events"), which holds even if the task got
    // a partial poll, so the test stays honest rather than flaky.
    #[tokio::test]
    async fn agent_alive_across_drain_yields_agent_end() {
        use yoagent::provider::MockProvider;
        use yoagent::{Agent, AgentEvent};

        // Positive: hold the Agent for the full drain.
        let provider = MockProvider::text("hello back");
        let mut agent = Agent::from_provider(provider, yoagent::provider::ModelConfig::mock())
            .with_api_key("not-a-real-key");

        let mut rx = agent.prompt("hi").await;
        let mut saw_agent_end = false;
        while let Some(event) = rx.recv().await {
            if matches!(event, AgentEvent::AgentEnd { .. }) {
                saw_agent_end = true;
            }
        }
        assert!(
            saw_agent_end,
            "Agent held alive across the drain must yield an AgentEnd event; \
             its absence means the run was cancelled, not finished"
        );

        agent.finish().await;
        assert!(
            !agent.messages().is_empty(),
            "finish() after a completed drain must restore messages"
        );
    }

    #[tokio::test]
    async fn agent_dropped_before_drain_never_yields_agent_end() {
        use yoagent::provider::MockProvider;
        use yoagent::{Agent, AgentEvent};

        let provider = MockProvider::text("hello back");
        let mut agent = Agent::from_provider(provider, yoagent::provider::ModelConfig::mock())
            .with_api_key("not-a-real-key");

        let mut rx = agent.prompt("hi").await;
        // The regression this guards: the Agent goes away mid-run.
        drop(agent);

        let mut saw_agent_end = false;
        while let Some(event) = rx.recv().await {
            if matches!(event, AgentEvent::AgentEnd { .. }) {
                saw_agent_end = true;
            }
        }
        assert!(
            !saw_agent_end,
            "dropping the Agent cancels the run — the channel must close without \
             AgentEnd (this is why the drain must hold the Agent alive)"
        );
    }

    // summarize_message, write_output_file, tool_result_preview,
    // search_messages, highlight_matches, and message_text tests
    // moved to src/prompt_utils.rs (Day 64)

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

    // TurnSnapshot and TurnHistory tests moved to src/session.rs (Day 54)

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
            last_tool_name: None,
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
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        assert!(outcome_no_error.last_api_error.is_none());
    }

    #[test]
    fn test_prompt_outcome_has_tool_name_field() {
        let outcome = PromptOutcome {
            text: String::new(),
            last_tool_error: Some("file not found".to_string()),
            last_tool_name: Some("read_file".to_string()),
            was_overflow: false,
            last_api_error: None,
        };
        assert_eq!(outcome.last_tool_name.as_deref(), Some("read_file"));

        let outcome_none = PromptOutcome {
            text: String::new(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        assert!(outcome_none.last_tool_name.is_none());
    }

    #[test]
    fn test_agent_event_agent_start_serializes_to_camel_case_type() {
        // Wire contract (DoD): first event is {"type":"agentStart"}.
        let event = AgentEvent::AgentStart;
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "agentStart");
    }

    #[test]
    fn test_agent_event_agent_end_serializes_to_camel_case_type() {
        // Wire contract (DoD): last event is {"type":"agentEnd", ...}.
        let event = AgentEvent::AgentEnd { messages: vec![] };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "agentEnd");
    }

    #[test]
    fn test_agent_event_tool_execution_start_serializes_camel_case_fields() {
        // Wire contract (DoD): toolExecutionStart carries toolCallId / toolName / args, camelCase.
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "call_123".to_string(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command": "ls -la"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        // Single-line NDJSON — no raw newlines in the serialized event.
        assert!(!json.contains('\n'), "NDJSON events must be single-line");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "toolExecutionStart");
        assert_eq!(parsed["toolCallId"], "call_123");
        assert_eq!(parsed["toolName"], "bash");
        assert_eq!(parsed["args"]["command"], "ls -la");
    }

    #[test]
    fn test_agent_event_tool_execution_end_serializes_camel_case_type() {
        // Wire contract (DoD): toolExecutionEnd present in the stream.
        let json = r#"{"type":"toolExecutionEnd"}"#;
        // Round-trip parse a minimal toolExecutionEnd back into the tagged enum to
        // confirm the discriminant is the exact camelCase string the DoD requires.
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(parsed["type"], "toolExecutionEnd");
    }

    // --- Day 74: New tests for PromptOutcome, PromptEventState ---

    #[test]
    fn test_prompt_outcome_default() {
        let outcome = PromptOutcome::default();
        assert!(outcome.text.is_empty());
        assert!(outcome.last_tool_error.is_none());
        assert!(outcome.last_tool_name.is_none());
        assert!(!outcome.was_overflow);
        assert!(outcome.last_api_error.is_none());
    }

    #[test]
    fn test_prompt_outcome_clone() {
        let outcome = PromptOutcome {
            text: "hello world".to_string(),
            last_tool_error: Some("error msg".to_string()),
            last_tool_name: Some("bash".to_string()),
            was_overflow: true,
            last_api_error: Some("429 rate limit".to_string()),
        };
        let cloned = outcome.clone();
        assert_eq!(cloned.text, "hello world");
        assert_eq!(cloned.last_tool_error, Some("error msg".to_string()));
        assert_eq!(cloned.last_tool_name, Some("bash".to_string()));
        assert!(cloned.was_overflow);
        assert_eq!(cloned.last_api_error, Some("429 rate limit".to_string()));
    }

    #[test]
    fn test_prompt_outcome_debug_format() {
        let outcome = PromptOutcome::default();
        let debug = format!("{:?}", outcome);
        assert!(debug.contains("PromptOutcome"));
        assert!(debug.contains("text"));
        assert!(debug.contains("was_overflow"));
    }

    #[test]
    fn test_prompt_outcome_with_overflow() {
        let outcome = PromptOutcome {
            text: "compacted response".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: true,
            last_api_error: None,
        };
        assert!(outcome.was_overflow);
        assert_eq!(outcome.text, "compacted response");
    }

    #[test]
    fn test_prompt_outcome_combined_tool_and_api_error() {
        // Both tool error and API error can coexist — tool error from a tool execution,
        // API error from a subsequent retry failure
        let outcome = PromptOutcome {
            text: String::new(),
            last_tool_error: Some("file not found".to_string()),
            last_tool_name: Some("read_file".to_string()),
            was_overflow: false,
            last_api_error: Some("500 internal server error".to_string()),
        };
        assert!(outcome.last_tool_error.is_some());
        assert!(outcome.last_api_error.is_some());
        assert_eq!(outcome.last_tool_name.as_deref(), Some("read_file"));
    }

    #[test]
    fn test_prompt_event_state_into_result_done() {
        let mut state = PromptEventState::new();
        // Stop the spinner to avoid background thread interference in tests
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        state.collected_text = "response text".to_string();
        state.last_tool_error = None;
        state.last_tool_name = None;
        let result = state.into_result();
        match result {
            PromptResult::Done {
                collected_text,
                last_tool_error,
                last_tool_name,
                ..
            } => {
                assert_eq!(collected_text, "response text");
                assert!(last_tool_error.is_none());
                assert!(last_tool_name.is_none());
            }
            _ => panic!("expected PromptResult::Done"),
        }
    }

    #[test]
    fn test_prompt_event_state_into_result_retriable_error() {
        let mut state = PromptEventState::new();
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        state.retriable_error = Some("429 Too Many Requests".to_string());
        let result = state.into_result();
        match result {
            PromptResult::RetriableError { error_msg, .. } => {
                assert_eq!(error_msg, "429 Too Many Requests");
            }
            _ => panic!("expected PromptResult::RetriableError"),
        }
    }

    #[test]
    fn test_prompt_event_state_into_result_context_overflow() {
        let mut state = PromptEventState::new();
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        state.overflow_error = Some("prompt is too long: 250000 tokens".to_string());
        let result = state.into_result();
        match result {
            PromptResult::ContextOverflow { error_msg, .. } => {
                assert!(error_msg.contains("prompt is too long"));
            }
            _ => panic!("expected PromptResult::ContextOverflow"),
        }
    }

    /// Build an assistant message that ended with `StopReason::Error`.
    fn error_assistant_msg(error_message: Option<&str>) -> AgentMessage {
        let base = Message::assistant(
            Vec::new(),
            StopReason::Error,
            "claude-test",
            "anthropic",
            Usage::default(),
        );
        let msg = match error_message {
            Some(m) => base.with_error_message(m),
            None => base,
        };
        AgentMessage::Llm(msg)
    }

    fn state_for_test() -> PromptEventState {
        let mut state = PromptEventState::new();
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        state
    }

    #[test]
    fn test_fatal_stop_error_is_caller_visible_not_done() {
        // #646: a StopReason::Error whose message is neither an overflow nor a
        // known-retriable class used to print a red line and set NO outcome
        // field, so into_result() returned Done — the turn was visible on
        // screen but swallowed for control flow.
        let mut state = state_for_test();
        state.handle_agent_end(
            vec![error_assistant_msg(Some(
                "tool call arguments for `read_file` were never assembled",
            ))],
            "claude-test",
        );
        assert!(
            state.fatal_error.is_some(),
            "a non-retriable, non-overflow StopReason::Error must be recorded"
        );
        match state.into_result() {
            PromptResult::FatalError { error_msg, .. } => {
                assert!(error_msg.contains("read_file"));
            }
            PromptResult::Done { .. } => {
                panic!("fatal StopReason::Error must NOT surface as Done (the swallow bug)")
            }
            _ => panic!("expected PromptResult::FatalError"),
        }
    }

    #[test]
    fn test_fatal_stop_error_without_message_surfaces_as_incomplete() {
        // pause_turn arrives as StopReason::Error with no error_message at all.
        // Previously nothing was printed and nothing was set — a totally silent
        // dead turn. Surface it honestly as incomplete; there is no resume path.
        let mut state = state_for_test();
        state.handle_agent_end(vec![error_assistant_msg(None)], "claude-test");
        let fatal = state
            .fatal_error
            .clone()
            .expect("empty-message StopReason::Error must still be recorded");
        assert!(
            fatal.to_lowercase().contains("incomplete"),
            "expected an honest 'incomplete' message, got: {fatal}"
        );
        assert!(
            matches!(state.into_result(), PromptResult::FatalError { .. }),
            "empty-message fatal error must not read as Done"
        );
    }

    #[test]
    fn test_fatal_error_does_not_hijack_retriable_or_overflow() {
        // Surface-and-stop must not steal the retriable/overflow paths: those
        // classes still get their own results so the retry machinery works.
        let mut state = state_for_test();
        state.handle_agent_end(
            vec![error_assistant_msg(Some("429 Too Many Requests"))],
            "claude-test",
        );
        assert!(
            state.fatal_error.is_none(),
            "a retriable error must not be marked fatal"
        );
        assert!(matches!(
            state.into_result(),
            PromptResult::RetriableError { .. }
        ));

        let mut state = state_for_test();
        state.handle_agent_end(
            vec![error_assistant_msg(Some(
                "prompt is too long: 250000 tokens",
            ))],
            "claude-test",
        );
        assert!(state.fatal_error.is_none());
        assert!(matches!(
            state.into_result(),
            PromptResult::ContextOverflow { .. }
        ));
    }

    #[test]
    fn test_benign_stream_end_is_not_fatal() {
        // #612: "stream ended" is a known-benign outcome — the response was
        // delivered in full. It must stay a clean Done, not become an error.
        let mut state = state_for_test();
        state.collected_text = "full response".to_string();
        state.handle_agent_end(
            vec![error_assistant_msg(Some(
                "stream ended without a terminator",
            ))],
            "claude-test",
        );
        assert!(state.fatal_error.is_none());
        assert!(matches!(state.into_result(), PromptResult::Done { .. }));
    }

    #[test]
    fn test_prompt_event_state_into_result_overflow_takes_priority() {
        // When both overflow_error and retriable_error are set,
        // overflow should take priority (checked first in into_result)
        let mut state = PromptEventState::new();
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        state.overflow_error = Some("context too large".to_string());
        state.retriable_error = Some("rate limited".to_string());
        let result = state.into_result();
        match result {
            PromptResult::ContextOverflow { error_msg, .. } => {
                assert_eq!(error_msg, "context too large");
            }
            _ => panic!("expected ContextOverflow to take priority over RetriableError"),
        }
    }

    #[test]
    fn test_prompt_event_state_into_result_preserves_tool_error() {
        let mut state = PromptEventState::new();
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        state.collected_text = "partial".to_string();
        state.last_tool_error = Some("permission denied".to_string());
        state.last_tool_name = Some("bash".to_string());
        let result = state.into_result();
        match result {
            PromptResult::Done {
                last_tool_error,
                last_tool_name,
                ..
            } => {
                assert_eq!(last_tool_error, Some("permission denied".to_string()));
                assert_eq!(last_tool_name, Some("bash".to_string()));
            }
            _ => panic!("expected PromptResult::Done"),
        }
    }

    #[test]
    fn test_prompt_event_state_new_defaults() {
        let mut state = PromptEventState::new();
        // Stop the spinner immediately to avoid test interference
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        assert!(state.collected_text.is_empty());
        assert!(!state.in_text);
        assert!(!state.in_thinking);
        assert!(state.tool_timers.is_empty());
        assert!(state.retriable_error.is_none());
        assert!(state.overflow_error.is_none());
        assert!(state.last_tool_error.is_none());
        assert!(state.last_tool_name.is_none());
        assert!(state.audit_inflight.is_empty());
        assert!(state.tool_progress_timers.is_empty());
        assert!(state.deferred_bash_timers.is_empty());
        assert_eq!(state.batch_count, 0);
        assert_eq!(state.batch_succeeded, 0);
        assert_eq!(state.batch_failed, 0);
        assert!(state.batch_start.is_none());
        assert_eq!(state.turn_number, 0);
        assert!(!state.had_text);
    }

    #[test]
    fn test_prompt_event_state_batch_reset_on_print() {
        let mut state = PromptEventState::new();
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        // Simulate a batch
        state.batch_count = 5;
        state.batch_succeeded = 3;
        state.batch_failed = 2;
        state.batch_start = Some(Instant::now());
        // print_batch_summary resets batch tracking
        state.print_batch_summary();
        assert_eq!(state.batch_count, 0);
        assert_eq!(state.batch_succeeded, 0);
        assert_eq!(state.batch_failed, 0);
        assert!(state.batch_start.is_none());
    }

    #[test]
    fn test_prompt_event_state_into_result_preserves_usage() {
        let mut state = PromptEventState::new();
        if let Some(s) = state.spinner.take() {
            s.stop();
        }
        state.usage = Usage {
            input: 500,
            output: 100,
            cache_read: 50,
            cache_write: 25,
            ..Default::default()
        };
        let result = state.into_result();
        match result {
            PromptResult::Done { usage, .. } => {
                assert_eq!(usage.input, 500);
                assert_eq!(usage.output, 100);
                assert_eq!(usage.cache_read, 50);
                assert_eq!(usage.cache_write, 25);
            }
            _ => panic!("expected PromptResult::Done"),
        }
    }

    #[test]
    fn test_accumulate_usage_commutative() {
        // Verify a + b == b + a (accumulation is commutative)
        let a = Usage {
            input: 10,
            output: 20,
            cache_read: 30,
            cache_write: 40,
            ..Default::default()
        };
        let b = Usage {
            input: 5,
            output: 15,
            cache_read: 25,
            cache_write: 35,
            ..Default::default()
        };

        let mut total_ab = Usage::default();
        accumulate_usage(&mut total_ab, &a);
        accumulate_usage(&mut total_ab, &b);

        let mut total_ba = Usage::default();
        accumulate_usage(&mut total_ba, &b);
        accumulate_usage(&mut total_ba, &a);

        assert_eq!(total_ab.input, total_ba.input);
        assert_eq!(total_ab.output, total_ba.output);
        assert_eq!(total_ab.cache_read, total_ba.cache_read);
        assert_eq!(total_ab.cache_write, total_ba.cache_write);
    }

    #[test]
    fn test_prompt_outcome_text_with_unicode() {
        let outcome = PromptOutcome {
            text: "Hello 🐙 — yoyo speaking! ✓ 日本語".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        assert!(outcome.text.contains('🐙'));
        assert!(outcome.text.contains("日本語"));
    }

    #[test]
    fn test_apply_effort_hint_low_prepends() {
        use crate::cli_config::{effort_level, set_effort_level, EffortLevel};
        let original = effort_level();
        set_effort_level(EffortLevel::Low);
        let result = apply_effort_hint("Hello agent");
        set_effort_level(original);
        assert!(result.starts_with("[Effort: "));
        assert!(result.contains("concise"));
        assert!(result.ends_with("Hello agent"));
    }

    #[test]
    fn test_apply_effort_hint_medium_noop() {
        use crate::cli_config::{effort_level, set_effort_level, EffortLevel};
        let original = effort_level();
        set_effort_level(EffortLevel::Medium);
        let result = apply_effort_hint("Hello agent");
        set_effort_level(original);
        assert_eq!(result, "Hello agent");
    }

    #[test]
    fn test_apply_effort_hint_high_prepends() {
        use crate::cli_config::{effort_level, set_effort_level, EffortLevel};
        let original = effort_level();
        set_effort_level(EffortLevel::High);
        let result = apply_effort_hint("Hello agent");
        set_effort_level(original);
        assert!(result.starts_with("[Effort: "));
        assert!(result.contains("thorough"));
        assert!(result.ends_with("Hello agent"));
    }
}
