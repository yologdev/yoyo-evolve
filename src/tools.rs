//! Tool definitions for the yoyo agent.
//!
//! Contains concrete tool implementations and builder functions:
//! - `StreamingBashTool` — real-time subprocess output
//! - `RenameSymbolTool` — cross-file symbol renaming
//! - `AskUserTool` — interactive question-asking
//! - `TodoTool` — task list management
//! - `build_tools` — assembles the complete tool set
//! - `build_sub_agent_tool` — creates a sub-agent with inherited config
//!
//! Tool decorator types (GuardedTool, TruncatingTool, ConfirmTool, ArcGuardedTool)
//! live in `tool_wrappers`.

use crate::cli;
use crate::cli_config::is_auto_edit;
use crate::commands_project;
use crate::commands_todo;
use crate::commands_web;
use crate::format::*;
use crate::hooks::{self, maybe_hook, AuditHook, HookRegistry};
use crate::safety::analyze_bash_command;
use crate::smart_edit::with_smart_edit;
use crate::tool_wrappers::{
    maybe_confirm, maybe_guard, maybe_guard_arc, sub_agent_model_label, with_auto_check,
    with_lite_description, with_read_guard, with_read_guard_arc, with_read_guard_bash,
    with_read_guard_bash_arc, with_recovery_hints, with_session_cap, with_truncation,
    DiagnosticSubAgentTool, FallbackSubAgentTool, ToolFailureTracker, SESSION_TOOL_CALL_CAP,
};
use crate::AgentConfig;

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use yoagent::provider::{
    AnthropicProvider, BedrockProvider, GoogleProvider, OpenAiCompatProvider, StreamProvider,
};
use yoagent::sub_agent::SubAgentTool;
use yoagent::tools::bash::ConfirmFn;
use yoagent::tools::edit::EditFileTool;
use yoagent::tools::file::{ReadFileTool, WriteFileTool};
use yoagent::tools::list::ListFilesTool;
use yoagent::tools::search::SearchTool;
use yoagent::types::AgentTool;
use yoagent::SharedState;

use crate::rtk::maybe_prefix_rtk;

// ---------------------------------------------------------------------------
// StreamingBashTool — real-time subprocess output via on_update and on_progress callbacks
// ---------------------------------------------------------------------------

/// Execute shell commands with real-time streaming output.
///
/// Unlike the upstream `BashTool` which waits for the process to finish before
/// returning output, `StreamingBashTool` reads stdout/stderr line-by-line and
/// calls `ctx.on_update()` periodically so the UI can display partial output
/// as the command runs. This is the difference between staring at a blank screen
/// during `cargo build` and watching compilation progress live.
///
/// Additionally, each individual line is emitted in real-time via `ctx.on_progress()`
/// (when available and in interactive mode), producing `AgentEvent::ProgressMessage`
/// events that the renderer displays immediately. Stderr lines are prefixed with
/// `stderr: ` so the user can distinguish them from stdout.
///
/// Streaming updates are sent every `update_interval` or every `lines_per_update`
/// lines, whichever comes first.
pub struct StreamingBashTool {
    /// Working directory for commands
    pub cwd: Option<String>,
    /// Max execution time per command
    pub timeout: Duration,
    /// Max output bytes to capture (prevents OOM on huge outputs)
    pub max_output_bytes: usize,
    /// Commands/patterns that are always blocked (e.g., "rm -rf /")
    pub deny_patterns: Vec<String>,
    /// Optional callback for confirming dangerous commands
    pub confirm_fn: Option<ConfirmFn>,
    /// How often to emit streaming updates
    pub update_interval: Duration,
    /// Emit an update after this many new lines (even if interval hasn't elapsed)
    pub lines_per_update: usize,
    /// When true (default), real-time progress via `on_progress` is only emitted
    /// when stderr is a terminal (interactive mode). Set to false in tests to
    /// allow progress emission regardless of TTY state.
    pub progress_requires_tty: bool,
}

impl Default for StreamingBashTool {
    fn default() -> Self {
        Self {
            cwd: None,
            timeout: Duration::from_secs(120),
            max_output_bytes: 256 * 1024, // 256KB
            deny_patterns: vec![
                "rm -rf /".into(),
                "rm -rf /*".into(),
                "mkfs".into(),
                "dd if=".into(),
                ":(){:|:&};:".into(), // fork bomb
            ],
            confirm_fn: None,
            update_interval: Duration::from_millis(500),
            lines_per_update: 20,
            progress_requires_tty: true,
        }
    }
}

impl StreamingBashTool {
    pub fn with_confirm(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        self.confirm_fn = Some(Box::new(f));
        self
    }

    /// Pin the working directory for all commands run by this tool.
    ///
    /// Used by `/spawn` worktree isolation to default the worker's bash into
    /// its worktree. Defense-in-depth, not a sandbox: relative paths and bare
    /// `git` operate in `path` by default, but absolute paths or `git -C /abs`
    /// can still escape.
    pub fn with_cwd(mut self, path: impl Into<String>) -> Self {
        self.cwd = Some(path.into());
        self
    }
}

/// Emit a streaming update with the accumulated output so far.
fn emit_update(ctx: &yoagent::types::ToolContext, output: &str) {
    if let Some(ref on_update) = ctx.on_update {
        on_update(yoagent::types::ToolResult {
            content: vec![yoagent::types::Content::Text {
                text: output.to_string(),
            }],
            details: serde_json::json!({"streaming": true}),
        });
    }
}

/// Decide whether a bash exit code represents success, accounting for the
/// `pipefail` shell option we run with.
///
/// With `set -o pipefail`, a pipeline reports the exit status of the *last*
/// stage that failed. This surfaces real mid-pipeline failures (e.g.
/// `sh -c 'exit 3' | cat` now exits 3 instead of silently succeeding). But it
/// also breaks the extremely common `yes | head` idiom: `yes` receives SIGPIPE
/// when `head` closes the pipe and exits 141 (128 + SIGPIPE 13), which pipefail
/// would otherwise propagate as a "failure" even though the pipeline did exactly
/// what was asked. So exit code 141 is treated as NOT-a-failure here — the
/// numeric code is still reported to the caller, we just don't flip `success`
/// to false for it.
fn pipeline_success(exit_code: i64) -> bool {
    exit_code == 0 || exit_code == 141
}

/// Commands that use a *specific* non-zero exit as a result, not an error.
///
/// `grep` exiting 1 means "no lines matched" — a fact about the world, not a
/// failure. A model trained that non-zero means failure reads an unannotated
/// `Exit code: 1` as something to retry, which is a paid turn spent learning
/// nothing (#876).
///
/// Keyed on an **exact** code on purpose: `grep` exiting 2 is a real error
/// (bad pattern, unreadable file) and must stay unannotated. This table is
/// deliberately tiny — a wrong "not an error" is worse than a missing one.
const BENIGN_EXITS: &[(&str, i32, &str)] = &[
    ("grep", 1, "no matches found"),
    ("egrep", 1, "no matches found"),
    ("fgrep", 1, "no matches found"),
    ("rg", 1, "no matches found"),
    ("ag", 1, "no matches found"),
    ("ack", 1, "no matches found"),
    ("diff", 1, "files differ"),
    ("cmp", 1, "files differ"),
    ("test", 1, "condition was false"),
    ("[", 1, "condition was false"),
];

/// True when `command` is one simple command — no pipeline, no separator, no
/// substitution.
///
/// The exit code of a pipeline can come from *any* stage (this tool runs bash
/// with `pipefail`), so `grep x f | wc -l` exiting 1 is not evidence that grep
/// found nothing. Annotating there would be a confident wrong diagnosis, which
/// is the one failure mode worse than saying nothing.
fn is_simple_command(command: &str) -> bool {
    !command.contains(['|', '&', ';', '\n', '`', '(', ')']) && !command.contains("$(")
}

/// The note for a benign non-zero exit, or `None` when there is nothing true
/// to add. `None` is the overwhelmingly common case and leaves the exit line
/// byte-identical.
fn benign_exit_note(command: &str, exit_code: i32) -> Option<&'static str> {
    if !is_simple_command(command) {
        return None;
    }
    // Same command-word rule `safety::detect_write_command` uses: step over
    // leading `VAR=value` assignments and wrapper words.
    let word = command
        .split_whitespace()
        .find(|t| !t.contains('=') && !crate::safety::COMMAND_WRAPPERS.contains(t))?;
    // `/usr/bin/grep` is still grep.
    let word = word.rsplit('/').next().unwrap_or(word);
    BENIGN_EXITS
        .iter()
        .find(|(name, code, _)| *name == word && *code == exit_code)
        .map(|(_, _, note)| *note)
}

#[async_trait::async_trait]
impl AgentTool for StreamingBashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn label(&self) -> &str {
        "Execute Command"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return stdout/stderr. Use for running scripts, installing packages, checking system state, etc. Supports an optional timeout parameter (in seconds) for long-running commands."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Maximum seconds to wait for command (default: 120, max: 600)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        use tokio::io::AsyncBufReadExt;
        use yoagent::types::{Content, ToolError, ToolResult as TR};

        let cancel = ctx.cancel.clone();
        let command = params["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'command' parameter".into()))?;

        // Pinned-cwd confinement (spawn workers): refuse git redirection
        // outside the confinement root (`git -C <abs-outside>`, `--git-dir`,
        // `--work-tree`, `GIT_DIR=`/`GIT_WORK_TREE=`). Enforcement lives at
        // the tool layer — the branch a misbehaving worker actually travels.
        // No pinned cwd (all ordinary sessions) → zero behavior change.
        if let Some(ref cwd) = self.cwd {
            if let Some(reason) =
                crate::safety::detect_git_redirection_escape(command, std::path::Path::new(cwd))
            {
                return Err(ToolError::Failed(
                    crate::safety::git_redirection_refusal_message(
                        &reason,
                        cwd,
                        crate::format::is_plain_output(),
                    ),
                ));
            }
        }

        // Check deny patterns (hard block — always denied, no override)
        for pattern in &self.deny_patterns {
            if command.contains(pattern.as_str()) {
                return Err(ToolError::Failed(format!(
                    "Command blocked by safety policy: contains '{}'. This pattern is denied for safety.",
                    pattern
                )));
            }
        }

        // Safety analysis — soft warning that routes through confirmation
        if let Some(warning) = analyze_bash_command(command) {
            if let Some(ref confirm) = self.confirm_fn {
                if !confirm(&format!("⚠️  {warning}\nCommand: {command}")) {
                    return Err(ToolError::Failed(
                        "Command was not confirmed by the user.".into(),
                    ));
                }
                // User confirmed the dangerous command — skip the normal confirm below
                // by proceeding directly to execution
            }
            // If no confirm_fn (piped mode), log warning but allow
            // (the deny_patterns still block the truly catastrophic ones)
        } else {
            // No safety warning — check normal confirmation callback
            if let Some(ref confirm) = self.confirm_fn {
                if !confirm(command) {
                    return Err(ToolError::Failed(
                        "Command was not confirmed by the user.".into(),
                    ));
                }
            }
        }

        // Apply RTK prefix for supported commands
        let effective_command = maybe_prefix_rtk(command);

        // Enable `pipefail` so a mid-pipeline failure is surfaced instead of
        // being masked by the exit status of the last stage. Without this,
        // `sh -c 'exit 3' | cat` reports success even though the pipe failed,
        // and the agent silently believes a broken build/test pipe succeeded.
        // The `yes | head` idiom (SIGPIPE → exit 141) is handled below by the
        // SIGPIPE-141 guard so pipefail doesn't turn a normal pipeline into a
        // reported failure.
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-o")
            .arg("pipefail")
            .arg("-c")
            .arg(&effective_command);

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }

        // Pipe stdout/stderr for line-by-line reading
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let timeout = if let Some(t) = params.get("timeout").and_then(|v| v.as_u64()) {
            Duration::from_secs(t.clamp(1, 600))
        } else {
            self.timeout
        };
        let max_bytes = self.max_output_bytes;
        let update_interval = self.update_interval;
        let lines_per_update = self.lines_per_update;

        let mut child = cmd
            .spawn()
            .map_err(|e| ToolError::Failed(format!("Failed to spawn: {e}")))?;

        // Take stdout/stderr handles
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let accumulated = Arc::new(tokio::sync::Mutex::new(String::new()));
        let truncated = Arc::new(AtomicBool::new(false));

        // Spawn a task to read stdout + stderr lines and accumulate them
        let acc_clone = Arc::clone(&accumulated);
        let trunc_clone = Arc::clone(&truncated);
        let cancel_clone = cancel.clone();
        let ctx_clone = ctx.clone();
        let emit_progress = !self.progress_requires_tty || crate::format::stderr_is_terminal();

        let reader_handle = tokio::spawn(async move {
            let stdout_reader = stdout.map(tokio::io::BufReader::new);
            let stderr_reader = stderr.map(tokio::io::BufReader::new);

            let mut stdout_lines = stdout_reader.map(|r| r.lines());
            let mut stderr_lines = stderr_reader.map(|r| r.lines());

            let mut lines_since_update: usize = 0;
            let mut last_update = tokio::time::Instant::now();
            let mut stdout_done = stdout_lines.is_none();
            let mut stderr_done = stderr_lines.is_none();

            loop {
                if cancel_clone.is_cancelled() {
                    break;
                }
                if stdout_done && stderr_done {
                    break;
                }

                // Read one line from whichever stream has data, tracking its source
                let line_info: Option<(String, bool)> = tokio::select! {
                    biased;
                    result = async {
                        match stdout_lines.as_mut() {
                            Some(lines) => lines.next_line().await,
                            None => std::future::pending().await,
                        }
                    }, if !stdout_done => {
                        match result {
                            Ok(Some(line)) => Some((line, false)),
                            Ok(None) => { stdout_done = true; None }
                            Err(_) => { stdout_done = true; None }
                        }
                    }
                    result = async {
                        match stderr_lines.as_mut() {
                            Some(lines) => lines.next_line().await,
                            None => std::future::pending().await,
                        }
                    }, if !stderr_done => {
                        match result {
                            Ok(Some(line)) => Some((line, true)),
                            Ok(None) => { stderr_done = true; None }
                            Err(_) => { stderr_done = true; None }
                        }
                    }
                };

                if let Some((line, is_stderr)) = line_info {
                    let mut acc = acc_clone.lock().await;
                    if acc.len() < max_bytes {
                        if !acc.is_empty() {
                            acc.push('\n');
                        }
                        acc.push_str(&line);
                        if acc.len() > max_bytes {
                            let safe_len = crate::format::safe_truncate(&acc, max_bytes).len();
                            acc.truncate(safe_len);
                            acc.push_str("\n... (output truncated)");
                            trunc_clone.store(true, Ordering::Relaxed);
                        }
                    }
                    lines_since_update += 1;
                    drop(acc);

                    // Emit real-time progress for each line (interactive mode only)
                    if emit_progress {
                        if let Some(ref on_progress) = ctx_clone.on_progress {
                            let progress_text = if is_stderr {
                                format!("stderr: {line}")
                            } else {
                                line.clone()
                            };
                            on_progress(progress_text);
                        }
                    }

                    // Emit update if interval elapsed or enough lines accumulated
                    let elapsed = last_update.elapsed();
                    if elapsed >= update_interval || lines_since_update >= lines_per_update {
                        let snapshot = acc_clone.lock().await.clone();
                        emit_update(&ctx_clone, &snapshot);
                        lines_since_update = 0;
                        last_update = tokio::time::Instant::now();
                    }
                }
            }
        });

        // Wait for the process with timeout and cancellation
        let exit_status = tokio::select! {
            _ = cancel.cancelled() => {
                // Kill the child process on cancellation
                let _ = child.kill().await;
                reader_handle.abort();
                return Err(yoagent::types::ToolError::Cancelled);
            }
            _ = tokio::time::sleep(timeout) => {
                let _ = child.kill().await;
                reader_handle.abort();
                return Err(ToolError::Failed(format!(
                    "Command timed out after {}s",
                    timeout.as_secs()
                )));
            }
            status = child.wait() => {
                status.map_err(|e| ToolError::Failed(format!("Failed to wait: {e}")))?
            }
        };

        // Wait for the reader to finish consuming remaining buffered output
        let _ = tokio::time::timeout(Duration::from_secs(2), reader_handle).await;

        let exit_code = exit_status.code().unwrap_or_else(|| {
            // On Unix, if code() is None the process was killed by a signal.
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(sig) = exit_status.signal() {
                    return -sig; // e.g. SIGSEGV=11 → -11
                }
            }
            -1
        });
        let output = accumulated.lock().await.clone();

        // One final update with the complete output
        emit_update(&ctx, &output);

        // Include signal name when the process was killed by a signal
        let exit_line = if exit_code < 0 && exit_code != -1 {
            let sig = -exit_code;
            let sig_name = match sig {
                1 => "SIGHUP",
                2 => "SIGINT",
                4 => "SIGILL",
                6 => "SIGABRT",
                8 => "SIGFPE",
                9 => "SIGKILL",
                11 => "SIGSEGV",
                13 => "SIGPIPE",
                14 => "SIGALRM",
                15 => "SIGTERM",
                _ => "signal",
            };
            format!("Exit code: {exit_code} ({sig_name})")
        } else {
            format!("Exit code: {exit_code}")
        };
        // Some tools use a non-zero exit as a result, not an error (#876).
        // `None` — the common case — leaves the line byte-identical.
        let exit_line = match benign_exit_note(command, exit_code) {
            Some(note) => format!("{exit_line} ({note} - not an error)"),
            None => exit_line,
        };
        let formatted = format!("{exit_line}\n{output}");

        Ok(TR {
            content: vec![Content::Text { text: formatted }],
            details: serde_json::json!({ "exit_code": exit_code, "success": pipeline_success(exit_code as i64) }),
        })
    }
}

// ── rename_symbol agent tool ─────────────────────────────────────────────

/// An agent-invocable tool for renaming symbols across a project.
/// Wraps `commands_project::rename_in_project` so the LLM can do cross-file
/// renames in a single tool call instead of multiple edit_file invocations.
#[derive(Default)]
pub(crate) struct RenameSymbolTool {
    /// Directory restrictions the rename must honor (#714). Default = empty =
    /// unrestricted, which is what the test sites and product default use.
    restrictions: cli::DirectoryRestrictions,
}

impl RenameSymbolTool {
    /// Build a rename tool bound to the session's directory restrictions.
    pub(crate) fn new(restrictions: &cli::DirectoryRestrictions) -> Self {
        Self {
            restrictions: restrictions.clone(),
        }
    }
}

#[async_trait::async_trait]
impl AgentTool for RenameSymbolTool {
    fn name(&self) -> &str {
        "rename_symbol"
    }

    fn label(&self) -> &str {
        "Rename"
    }

    fn description(&self) -> &str {
        "Rename a symbol across the project. Performs word-boundary-aware find-and-replace \
         in all git-tracked files. More reliable than multiple edit_file calls for renames. \
         Returns a preview of changes and the number of files modified."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "old_name": {
                    "type": "string",
                    "description": "The current name of the symbol to rename"
                },
                "new_name": {
                    "type": "string",
                    "description": "The new name for the symbol"
                },
                "path": {
                    "type": "string",
                    "description": "Optional: limit rename to a specific file or directory (default: entire project)"
                }
            },
            "required": ["old_name", "new_name"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        use yoagent::types::{Content, ToolError, ToolResult as TR};

        let old_name = params["old_name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'old_name' parameter".into()))?;

        let new_name = params["new_name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'new_name' parameter".into()))?;

        let scope = params["path"].as_str();

        match commands_project::rename_in_project_restricted(
            old_name,
            new_name,
            scope,
            &self.restrictions,
        ) {
            Ok(result) => {
                let mut summary = format!(
                    "Renamed '{}' → '{}': {} replacement{} across {} file{}.\n\nFiles changed:\n{}\n\n{}",
                    old_name,
                    new_name,
                    result.total_replacements,
                    if result.total_replacements == 1 { "" } else { "s" },
                    result.files_changed.len(),
                    if result.files_changed.len() == 1 { "" } else { "s" },
                    result.files_changed.iter().map(|f| format!("  - {f}")).collect::<Vec<_>>().join("\n"),
                    result.preview,
                );
                // Denied files are skipped, never silently dropped (#714).
                if let Some(note) = commands_project::format_denied_note(&result.skipped_denied) {
                    summary.push_str("\n\n");
                    summary.push_str(&note);
                }
                Ok(TR {
                    content: vec![Content::Text { text: summary }],
                    // #783: the written-file list is the only place the caller
                    // can learn which paths this call actually touched — the
                    // set is unknown until after the write, so it cannot be
                    // read off the arguments the way write_file/edit_file are.
                    // `prompt::record_rename_tool_writes` consumes this key.
                    details: serde_json::json!({ "files_written": result.written }),
                })
            }
            Err(msg) => Err(ToolError::Failed(msg)),
        }
    }
}

// ── ask_user agent tool ──────────────────────────────────────────────────

/// Tool that lets the model ask the user directed questions.
/// The user types their answer, which is returned as the tool result.
/// Only registered in interactive mode (when stdin is a terminal).
pub struct AskUserTool;

#[async_trait::async_trait]
impl AgentTool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn label(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the user a question to get clarification or input. Use this when you need \
         specific information to proceed, like a preference, a decision, or context that \
         isn't available in the codebase. The user sees your question and types a response."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user. Be specific and concise."
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        use yoagent::types::{Content, ToolError, ToolResult as TR};

        let question = params
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing 'question' parameter".into()))?;

        // Display the question with visual distinction
        eprintln!("\n{YELLOW}  ❓ {question}{RESET}");
        eprint!("{GREEN}  → {RESET}");
        io::stderr().flush().ok();

        // Read the user's response
        use std::io::BufRead;
        let mut response = String::new();
        let stdin = io::stdin();
        match stdin.lock().read_line(&mut response) {
            Ok(0) | Err(_) => {
                return Ok(TR {
                    content: vec![Content::Text {
                        text: "(user provided no response)".to_string(),
                    }],
                    details: serde_json::Value::Null,
                });
            }
            _ => {}
        }

        let response = response.trim().to_string();
        if response.is_empty() {
            return Ok(TR {
                content: vec![Content::Text {
                    text: "(user provided empty response)".to_string(),
                }],
                details: serde_json::Value::Null,
            });
        }

        Ok(TR {
            content: vec![Content::Text { text: response }],
            details: serde_json::Value::Null,
        })
    }
}

// ── todo agent tool ──────────────────────────────────────────────────────

/// Agent tool for managing a task list during complex multi-step operations.
pub struct TodoTool;

#[async_trait::async_trait]
impl AgentTool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn label(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Manage a task list to track progress on complex multi-step operations. \
         Use this to plan work, check off completed steps, and see what's remaining. \
         Available actions: list, add, done, wip, remove, clear."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "add", "done", "wip", "remove", "clear"],
                    "description": "Action: list (show all), add (create task), done (mark complete), wip (mark in-progress), remove (delete task), clear (delete all)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (required for 'add')"
                },
                "id": {
                    "type": "integer",
                    "description": "Task ID number (required for 'done', 'wip', 'remove')"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        use yoagent::types::{Content, ToolError, ToolResult as TR};

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing required 'action' parameter".into()))?;

        let text =
            match action {
                "list" => {
                    let items = commands_todo::todo_list();
                    if items.is_empty() {
                        "No tasks. Use action 'add' to create one.".to_string()
                    } else {
                        commands_todo::format_todo_list(&items)
                    }
                }
                "add" => {
                    let desc = params
                        .get("description")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ToolError::InvalidArgs("Missing 'description' for add action".into())
                        })?;
                    let id = commands_todo::todo_add(desc);
                    format!("Added task #{id}: {desc}")
                }
                "done" => {
                    let id = params.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
                        ToolError::InvalidArgs("Missing 'id' for done action".into())
                    })? as usize;
                    commands_todo::todo_update(id, commands_todo::TodoStatus::Done)
                        .map_err(ToolError::Failed)?;
                    format!("Task #{id} marked as done ✓")
                }
                "wip" => {
                    let id = params.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
                        ToolError::InvalidArgs("Missing 'id' for wip action".into())
                    })? as usize;
                    commands_todo::todo_update(id, commands_todo::TodoStatus::InProgress)
                        .map_err(ToolError::Failed)?;
                    format!("Task #{id} marked as in-progress")
                }
                "remove" => {
                    let id = params.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
                        ToolError::InvalidArgs("Missing 'id' for remove action".into())
                    })? as usize;
                    let item = commands_todo::todo_remove(id).map_err(ToolError::Failed)?;
                    format!("Removed task #{id}: {}", item.description)
                }
                "clear" => {
                    commands_todo::todo_clear();
                    "All tasks cleared.".to_string()
                }
                other => {
                    return Err(ToolError::InvalidArgs(format!(
                        "Unknown action '{other}'. Use: list, add, done, wip, remove, clear"
                    )));
                }
            };

        Ok(TR {
            content: vec![Content::Text { text }],
            details: serde_json::Value::Null,
        })
    }
}

// ---------------------------------------------------------------------------
// WebSearchTool — agent-callable web search via DuckDuckGo
// ---------------------------------------------------------------------------

/// Search the web and return results the agent can use during problem-solving.
pub(crate) struct WebSearchTool;

#[async_trait::async_trait]
impl AgentTool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn label(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        "Search the web using DuckDuckGo. Returns a list of search results with titles, \
         URLs, and snippets. Use this when you need to look up documentation, find solutions \
         to errors, or research unfamiliar topics. Optional 'depth' parameter: 'auto' (default) \
         for quick lookups, 'deep' for thorough research on complex/comparison queries."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, max: 20)"
                },
                "depth": {
                    "type": "string",
                    "description": "Search depth: 'auto' (default) for quick lookups, 'deep' for thorough research on complex/comparison queries",
                    "enum": ["auto", "deep"]
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        use yoagent::types::{Content, ToolError, ToolResult as TR};

        let query = params["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'query' parameter".into()))?;

        if query.trim().is_empty() {
            return Err(ToolError::InvalidArgs(
                "'query' parameter must not be empty".into(),
            ));
        }

        let max_results = params["max_results"]
            .as_u64()
            .map(|n| n.min(20) as usize)
            .unwrap_or(5);

        let depth = params["depth"].as_str().unwrap_or("auto");
        let search_type = match depth {
            "deep" => "deep",
            _ => "auto",
        };

        let result = commands_web::web_search_and_read(query, max_results, search_type);
        Ok(TR {
            content: vec![Content::Text { text: result }],
            details: serde_json::json!({}),
        })
    }
}

// ---------------------------------------------------------------------------
// Permission persistence — offer to save "always" approvals to .yoyo.toml
// ---------------------------------------------------------------------------

use std::collections::HashSet;
use std::sync::Mutex;

/// Simplify a bash command into a glob pattern suitable for the allow list.
///
/// Heuristic: keep the first 2 tokens (base command + subcommand), append `*`.
/// This produces patterns like `cargo test*`, `npm run*`, `git commit*`.
pub fn simplify_command_pattern(cmd: &str) -> String {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    let base = match tokens.len() {
        0 => return "*".to_string(),
        1 => tokens[0].to_string(),
        _ => format!("{} {}", tokens[0], tokens[1]),
    };
    format!("{base}*")
}

/// Track which patterns we've already offered to save this session,
/// so we don't repeatedly ask for the same base pattern.
fn already_offered_persistence(pattern: &str) -> bool {
    static OFFERED: std::sync::LazyLock<Mutex<HashSet<String>>> =
        std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));
    let mut set = OFFERED.lock().unwrap_or_else(|e| e.into_inner());
    !set.insert(pattern.to_string())
}

/// After the user says "always", offer to persist the pattern to .yoyo.toml.
/// Returns without action if the pattern was already offered this session.
fn offer_persist_pattern(cmd: &str) {
    let pattern = simplify_command_pattern(cmd);

    // Don't re-ask if we already offered this pattern this session
    if already_offered_persistence(&pattern) {
        return;
    }

    eprint!(
        "{DIM}  Save '{pattern}' to .yoyo.toml allow list? ({GREEN}y{RESET}{DIM}/{RED}n{RESET}{DIM}) {RESET}"
    );
    io::stderr().flush().ok();

    let mut response = String::new();
    let stdin = io::stdin();
    use std::io::BufRead;
    if stdin.lock().read_line(&mut response).is_err() {
        return;
    }
    let response = response.trim().to_lowercase();
    if matches!(response.as_str(), "y" | "yes") {
        match crate::config::append_allow_pattern(&pattern) {
            Ok(path) => {
                eprintln!("{GREEN}  ✓ Saved to {}{RESET}", path.display());
            }
            Err(e) => {
                eprintln!("{RED}  ✗ Could not save: {e}{RESET}");
            }
        }
    }
}

/// Build the tool set, optionally with a bash confirmation prompt.
/// When `auto_approve` is false (default), bash commands and file writes require user approval.
/// The "always" option sets a session-wide flag so subsequent operations are auto-approved.
/// The same `always_approved` flag is shared across bash, write_file, and edit_file.
/// When `permissions` has patterns, matching commands/paths are auto-approved or auto-denied.
/// When `dir_restrictions` has rules, file tools check paths before executing.
/// When `audit` is true, all tools are wrapped with the AuditHook via the hook system.
/// When `bash_cwd` is `Some(path)`, the bash tool runs every command with that
/// working directory (used by `/spawn` worktree isolation); `None` keeps the
/// process cwd — the default for interactive and normal-agent use.
pub fn build_tools(
    auto_approve: bool,
    permissions: &cli::PermissionConfig,
    dir_restrictions: &cli::DirectoryRestrictions,
    max_tool_output: usize,
    audit: bool,
    shell_hooks: Vec<hooks::ShellHook>,
    bash_cwd: Option<String>,
) -> Vec<Box<dyn AgentTool>> {
    // Shared flag: when any tool gets "always", all tools skip prompts
    let always_approved = Arc::new(AtomicBool::new(false));

    let base_bash = match bash_cwd {
        Some(cwd) => StreamingBashTool::default().with_cwd(cwd),
        None => StreamingBashTool::default(),
    };
    let bash = if auto_approve {
        base_bash
    } else {
        let flag = Arc::clone(&always_approved);
        let perms = permissions.clone();
        base_bash.with_confirm(move |cmd: &str| {
            // If user previously chose "always", skip the prompt
            if flag.load(Ordering::Relaxed) {
                eprintln!(
                    "{GREEN}  ✓ Auto-approved: {RESET}{}",
                    truncate_with_ellipsis(cmd, 120)
                );
                return true;
            }
            // Check permission patterns before prompting
            if let Some(allowed) = perms.check(cmd) {
                if allowed {
                    eprintln!(
                        "{GREEN}  ✓ Permitted: {RESET}{}",
                        truncate_with_ellipsis(cmd, 120)
                    );
                    return true;
                } else {
                    eprintln!(
                        "{RED}  ✗ Denied by permission rule: {RESET}{}",
                        truncate_with_ellipsis(cmd, 120)
                    );
                    return false;
                }
            }
            use std::io::BufRead;
            // Show the command and ask for approval
            eprint!(
                "{YELLOW}  ⚠ Allow: {RESET}{}{YELLOW} ? {RESET}({GREEN}y{RESET}/{RED}n{RESET}/{GREEN}a{RESET}lways) ",
                truncate_with_ellipsis(cmd, 120)
            );
            io::stderr().flush().ok();
            let mut response = String::new();
            let stdin = io::stdin();
            if stdin.lock().read_line(&mut response).is_err() {
                return false;
            }
            let response = response.trim().to_lowercase();
            let approved = matches!(response.as_str(), "y" | "yes" | "a" | "always");
            if matches!(response.as_str(), "a" | "always") {
                flag.store(true, Ordering::Relaxed);
                eprintln!(
                    "{GREEN}  ✓ All subsequent operations will be auto-approved this session.{RESET}"
                );
                // Offer to persist this pattern to .yoyo.toml
                offer_persist_pattern(cmd);
            }
            approved
        })
    };

    // Build write_file and edit_file with optional confirmation prompts.
    // In auto_edit mode, file operations are auto-approved but bash still confirms.
    let auto_edit = is_auto_edit();
    let write_tool: Box<dyn AgentTool> = if auto_approve || auto_edit {
        maybe_guard(Box::new(WriteFileTool::new()), dir_restrictions)
    } else {
        maybe_guard(
            maybe_confirm(
                Box::new(WriteFileTool::new()),
                &always_approved,
                permissions,
            ),
            dir_restrictions,
        )
    };
    let edit_tool: Box<dyn AgentTool> = if auto_approve || auto_edit {
        maybe_guard(Box::new(EditFileTool::new()), dir_restrictions)
    } else {
        maybe_guard(
            maybe_confirm(Box::new(EditFileTool::new()), &always_approved, permissions),
            dir_restrictions,
        )
    };

    // Build rename_symbol tool with optional confirmation (it writes files)
    let rename_tool: Box<dyn AgentTool> = if auto_approve || auto_edit {
        Box::new(RenameSymbolTool::new(dir_restrictions))
    } else {
        maybe_confirm(
            Box::new(RenameSymbolTool::new(dir_restrictions)),
            &always_approved,
            permissions,
        )
    };

    // Shared failure tracker for recovery hints — counts per-tool failures
    // so hints escalate from diagnostic to alternative suggestions.
    let failure_tracker = ToolFailureTracker::new();

    // Build hook registry — AuditHook when audit mode is on, plus user-configured shell hooks.
    let hooks = {
        let mut registry = HookRegistry::new();
        if audit {
            registry.register(Box::new(AuditHook));
        }
        for hook in shell_hooks {
            registry.register(Box::new(hook));
        }
        Arc::new(registry)
    };

    let mut tools = vec![
        maybe_hook(
            with_recovery_hints(
                with_truncation(with_read_guard_bash(Box::new(bash)), max_tool_output),
                &failure_tracker,
            ),
            &hooks,
        ),
        maybe_hook(
            with_recovery_hints(
                with_truncation(
                    maybe_guard(Box::new(ReadFileTool::default()), dir_restrictions),
                    max_tool_output,
                ),
                &failure_tracker,
            ),
            &hooks,
        ),
        maybe_hook(
            with_recovery_hints(
                with_truncation(
                    with_read_guard(with_auto_check(write_tool)),
                    max_tool_output,
                ),
                &failure_tracker,
            ),
            &hooks,
        ),
        maybe_hook(
            with_recovery_hints(
                with_truncation(
                    with_read_guard(with_smart_edit(with_auto_check(edit_tool))),
                    max_tool_output,
                ),
                &failure_tracker,
            ),
            &hooks,
        ),
        maybe_hook(
            with_recovery_hints(
                with_truncation(
                    maybe_guard(Box::new(ListFilesTool::default()), dir_restrictions),
                    max_tool_output,
                ),
                &failure_tracker,
            ),
            &hooks,
        ),
        maybe_hook(
            with_recovery_hints(
                with_truncation(
                    maybe_guard(Box::new(SearchTool::default()), dir_restrictions),
                    max_tool_output,
                ),
                &failure_tracker,
            ),
            &hooks,
        ),
        maybe_hook(
            with_recovery_hints(
                with_truncation(with_read_guard(rename_tool), max_tool_output),
                &failure_tracker,
            ),
            &hooks,
        ),
    ];

    // Only add ask_user in interactive mode (stdin is a terminal).
    // In piped mode or test environments, this tool isn't available.
    if std::io::stdin().is_terminal() {
        tools.push(maybe_hook(Box::new(AskUserTool), &hooks));
    }

    // TodoTool is always available — it only modifies in-memory state, not filesystem
    tools.push(maybe_hook(Box::new(TodoTool), &hooks));

    // WebSearchTool — agent-callable web search (always available), with a
    // session-wide call cap as a runaway-loop circuit breaker.
    tools.push(maybe_hook(
        with_recovery_hints(
            with_session_cap(
                with_truncation(Box::new(WebSearchTool), max_tool_output),
                SESSION_TOOL_CALL_CAP,
            ),
            &failure_tracker,
        ),
        &hooks,
    ));

    // In lite mode (small context window), augment tool descriptions with
    // JSON format examples so small/local LLMs can produce valid tool calls.
    if crate::cli_config::effective_context_tokens() <= 16_000 {
        tools = tools.into_iter().map(with_lite_description).collect();
    }

    tools
}

/// Build a SubAgentTool that inherits the parent's provider/model/key.
/// The sub-agent gets basic tools with inherited directory restrictions
/// (no permission prompts, no sub-agent recursion).
///
/// Returns `(SubAgentTool, SharedState)` — the `SharedState` handle lets the
/// parent agent pre-populate or read shared variables. The sub-agent
/// automatically receives a `shared_state` tool (via yoagent's
/// `SharedStateTool`) so it can read/write the same store.
/// Hard cap on sub-agent nesting depth. `depth 0` = the top-level parent's
/// direct child; each nested `sub_agent` tool increments the depth by one.
/// Matches the depth-3 cap documented in CLAUDE.md's RLM substrate section.
/// The tool tree is finite by construction: the leaf level
/// (`depth == MAX_SUB_AGENT_DEPTH - 1`) omits the nested `sub_agent` tool, so
/// recursion cannot continue past the cap.
const MAX_SUB_AGENT_DEPTH: usize = 3;

/// Decide what `(provider, model)` the sub-agent's one fallback attempt should
/// use, or `None` when there is no usable fallback.
///
/// Pure so the gate is table-tested: the **`None` branch is the whole
/// regression surface**, because it is every user who has not configured a
/// fallback and it must leave the `sub_agent` tool byte-identical to before.
///
/// Rules, in order:
/// - no `fallback_model` (or an empty one) → `None`. A fallback *provider*
///   alone is not enough here: `try_switch_to_fallback` can default the model
///   for a whole session, but a sub-agent tool is built once at startup and a
///   guessed model is not worth a silent second API bill.
/// - the fallback provider is `fallback_provider` when set, else the primary's.
/// - the result must actually **differ** from the primary in provider or
///   model, otherwise the retry is a second identical call → `None`.
pub(crate) fn sub_agent_fallback_target(
    provider: &str,
    model: &str,
    fallback_provider: Option<&str>,
    fallback_model: Option<&str>,
) -> Option<(String, String)> {
    let fb_model = fallback_model.map(str::trim).filter(|m| !m.is_empty())?;
    let fb_provider = fallback_provider
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(provider);
    if fb_provider == provider && fb_model == model {
        return None;
    }
    Some((fb_provider.to_string(), fb_model.to_string()))
}

/// Resolve the API key for a sub-agent fallback attempt.
///
/// Same provider → the key already in hand. Different provider → its own env
/// var, and an unset/empty one **refuses the fallback** (`None`) rather than
/// retrying with the primary's credential, which would surface as a baffling
/// 401. Same rule, same reason as `AgentConfig::try_switch_to_fallback`.
fn sub_agent_fallback_key(
    primary_provider: &str,
    primary_key: &str,
    fallback_provider: &str,
) -> Option<String> {
    if fallback_provider == primary_provider {
        return Some(primary_key.to_string());
    }
    match cli::provider_api_key_env(fallback_provider) {
        Some(env_var) => match std::env::var(env_var) {
            Ok(key) if !key.is_empty() => Some(key),
            _ => None,
        },
        // Keyless/local provider (e.g. ollama) — no key required.
        None => Some(String::new()),
    }
}

pub(crate) fn build_sub_agent_tool(config: &AgentConfig) -> (Box<dyn AgentTool>, SharedState) {
    let shared_state = SharedState::new();
    let tool = build_sub_agent_tool_at_depth(config, 0, &shared_state);
    (tool, shared_state)
}

/// Build a `sub_agent` tool at a given nesting `depth`, all levels sharing the
/// single `shared_state` store passed in (RLM substrate invariant: every
/// sub-agent reads/writes the same key-value store). When
/// `depth + 1 < MAX_SUB_AGENT_DEPTH`, the child gets its own `sub_agent` tool
/// (built at `depth + 1`) so it can delegate further; at the leaf level the
/// nested tool is omitted, guaranteeing termination.
fn build_sub_agent_tool_at_depth(
    config: &AgentConfig,
    depth: usize,
    shared_state: &SharedState,
) -> Box<dyn AgentTool> {
    // Sub-agent gets standard yoagent tools — no permission guards needed
    // since the parent already authorized the delegation.
    //
    // Two boundaries ARE inherited, because a child must not be a way around a
    // promise the parent made to the user:
    //   1. Directory restrictions (`maybe_guard_arc`) — path-based security.
    //   2. `/read` and `/plan` mode (`with_read_guard_arc` /
    //      `with_read_guard_bash_arc`) — the same `ReadModeGuardTool` the main
    //      agent uses, checked at call time, transparent when no mode is on
    //      and during `/plan apply`.
    //
    // Known remaining gap (#709): the child's bash is yoagent's raw `BashTool`,
    // not yoyo's `StreamingBashTool`, so when NO mode is active a child's bash
    // command does not pass through `safety.rs` (no destructive-pattern check,
    // no `detect_write_command`, no `detect_git_redirection_escape`). Modes are
    // enforced; the always-on bash safety layer is not.
    let restrictions = &config.dir_restrictions;
    let mut child_tools: Vec<Arc<dyn AgentTool>> = vec![
        with_read_guard_bash_arc(Arc::new(yoagent::tools::bash::BashTool::default())),
        maybe_guard_arc(Arc::new(ReadFileTool::default()), restrictions),
        with_read_guard_arc(maybe_guard_arc(
            Arc::new(WriteFileTool::new()),
            restrictions,
        )),
        with_read_guard_arc(maybe_guard_arc(Arc::new(EditFileTool::new()), restrictions)),
        maybe_guard_arc(Arc::new(ListFilesTool::default()), restrictions),
        maybe_guard_arc(Arc::new(SearchTool::default()), restrictions),
        Arc::new(WebSearchTool),
    ];

    // Allow exactly one more level of nesting, bounded by MAX_SUB_AGENT_DEPTH.
    // The nested tool shares the SAME store (not a fresh one) so artifacts set
    // at any level are visible at every level. The leaf level omits this tool —
    // that omission is the termination guarantee.
    if depth + 1 < MAX_SUB_AGENT_DEPTH {
        let nested = build_sub_agent_tool_at_depth(config, depth + 1, shared_state);
        child_tools.push(Arc::from(nested));
    }

    // The primary attempt, on the session's configured model.
    let primary = sub_agent_tool_for(
        config,
        &config.provider,
        &config.model,
        &config.api_key,
        child_tools.clone(),
        shared_state,
    );

    // One fallback attempt, and only when a fallback model is actually
    // configured and differs. With none configured — every user who has not
    // set one, and the whole regression surface — the un-decorated tool is
    // returned byte-identically.
    let fallback = sub_agent_fallback_target(
        &config.provider,
        &config.model,
        config.fallback_provider.as_deref(),
        config.fallback_model.as_deref(),
    )
    .and_then(|(fb_provider, fb_model)| {
        sub_agent_fallback_key(&config.provider, &config.api_key, &fb_provider)
            .map(|fb_key| (fb_provider, fb_model, fb_key))
    });

    let inner: Box<dyn AgentTool> = match fallback.as_ref() {
        None => Box::new(primary),
        Some((fb_provider, fb_model, fb_key)) => {
            let secondary = sub_agent_tool_for(
                config,
                fb_provider,
                fb_model,
                fb_key,
                child_tools,
                shared_state,
            );
            Box::new(FallbackSubAgentTool::new(
                Box::new(primary),
                Box::new(secondary),
                &config.model,
                fb_model,
            ))
        }
    };

    // Failure diagnostics are the OUTERMOST wrapper and are applied at this
    // single site — unconditionally, whether or not a fallback exists. Putting
    // them inside `FallbackSubAgentTool` would reach only users who configured
    // a fallback model and leave everyone else with yoagent's opaque summary
    // string: the "two doors, one policy, one deaf" shape this repo has already
    // shipped six times. Outermost also means it annotates the error that
    // actually *survives* the fallback attempt rather than an intermediate one.
    Box::new(DiagnosticSubAgentTool::new(
        inner,
        sub_agent_model_label(&config.model, fallback.as_ref().map(|(_, m, _)| m.as_str())),
    ))
}

/// Build one `SubAgentTool` bound to an explicit `(provider, model, api_key)`.
///
/// Parameterised rather than reading `config.provider`/`config.model` directly
/// so the primary and the fallback attempt are built by the *same* code —
/// identical tools, prompt, thinking level, turn cap, skills and shared state,
/// differing only in which model answers. A second builder would be a second
/// place for the sub-agent's contract to drift.
fn sub_agent_tool_for(
    config: &AgentConfig,
    provider_name: &str,
    model: &str,
    api_key: &str,
    child_tools: Vec<Arc<dyn AgentTool>>,
    shared_state: &SharedState,
) -> SubAgentTool {
    // Select the right provider
    let provider: Arc<dyn StreamProvider> = match provider_name {
        "anthropic" => Arc::new(AnthropicProvider),
        "google" => Arc::new(GoogleProvider),
        "bedrock" => Arc::new(BedrockProvider),
        _ => Arc::new(OpenAiCompatProvider),
    };

    let model_config =
        crate::agent_builder::create_model_config(provider_name, model, config.base_url.as_deref());
    SubAgentTool::from_provider("sub_agent", provider, model_config)
        .with_description(
            "Delegate a subtask to a fresh sub-agent with its own context window. \
             Use for complex, self-contained subtasks like: researching a codebase, \
             running a series of tests, or implementing a well-scoped change. \
             The sub-agent has bash, file read/write/edit, list, and search tools. \
             The sub-agent also has its own sub_agent tool and may delegate further, \
             bounded to a hard nesting cap (recursion is available and finite). \
             It starts with a clean context and returns a summary of what it did.",
        )
        .with_system_prompt(sub_agent_system_prompt(provider_name, model))
        .with_api_key(api_key)
        .with_tools(child_tools)
        .with_thinking(config.thinking)
        .with_max_turns(25)
        .with_shared_state(shared_state.clone())
        .with_skills(config.skills.clone())
}

/// Base behavioral prompt for a sub-agent, before the runtime grounding note is
/// appended. Kept as a named constant so `sub_agent_system_prompt` is the only
/// place that decides what a sub-agent is told.
const SUB_AGENT_BASE_PROMPT: &str = "You are a focused sub-agent. Complete the given task \
     efficiently using the tools available. Be thorough but concise in your final \
     response — summarize what you did, what you found, and any issues.";

/// Compose the sub-agent's system prompt: its base behavioral prompt plus the
/// same factual provider/model grounding note every top-level agent gets
/// (`agent_builder::compose_system_prompt`).
///
/// Without this, a sub-agent asked what model it is would answer from training
/// priors — the exact confabulation #664 fixed on the main path. The main path
/// composes at a single choke point; this is the sub-agent's choke point, so a
/// sub-agent no longer stands outside that guarantee (#671).
///
/// Pure so the composed string is assertable in tests (the prompt cannot be read
/// back off a built `SubAgentTool`).
fn sub_agent_system_prompt(provider: &str, model: &str) -> String {
    crate::agent_builder::compose_system_prompt(SUB_AGENT_BASE_PROMPT, provider, model)
}

/// Return the tool names a sub-agent built at `depth` would expose to its child.
/// Pure helper (no agent loop needed) for testing the nesting depth cap: below
/// the cap the list contains a nested "sub_agent"; at the leaf level it does not.
#[cfg(test)]
fn sub_agent_child_tool_names(config: &AgentConfig, depth: usize) -> Vec<String> {
    let restrictions = &config.dir_restrictions;
    let mut names: Vec<String> = vec![
        yoagent::tools::bash::BashTool::default().name().to_string(),
        maybe_guard_arc(Arc::new(ReadFileTool::default()), restrictions)
            .name()
            .to_string(),
        maybe_guard_arc(Arc::new(WriteFileTool::new()), restrictions)
            .name()
            .to_string(),
        maybe_guard_arc(Arc::new(EditFileTool::new()), restrictions)
            .name()
            .to_string(),
        maybe_guard_arc(Arc::new(ListFilesTool::default()), restrictions)
            .name()
            .to_string(),
        maybe_guard_arc(Arc::new(SearchTool::default()), restrictions)
            .name()
            .to_string(),
        WebSearchTool.name().to_string(),
    ];
    if depth + 1 < MAX_SUB_AGENT_DEPTH {
        names.push("sub_agent".to_string());
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::time::Duration;
    use yoagent::ThinkingLevel;

    /// Helper to create a default AgentConfig for tests, varying only the provider.
    fn test_agent_config(provider: &str, model: &str) -> AgentConfig {
        AgentConfig {
            model: model.to_string(),
            api_key: "test-key".to_string(),
            provider: provider.to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test prompt.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        }
    }

    #[test]
    fn test_build_tools_returns_eight_tools() {
        // build_tools should return 8 tools regardless of auto_approve (in non-terminal: no ask_user)
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools_approved = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let tools_confirm = build_tools(
            false,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        assert_eq!(tools_approved.len(), 9);
        assert_eq!(tools_confirm.len(), 9);
    }

    #[test]
    fn test_build_sub_agent_tool_returns_correct_name() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (tool, _state) = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    #[test]
    fn test_nested_sub_agent_present_below_cap() {
        // At depth 0 (top-level parent's direct child), the child tool set must
        // include a nested "sub_agent" so the sub-agent can delegate further.
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let names = sub_agent_child_tool_names(&config, 0);
        assert!(
            names.iter().any(|n| n == "sub_agent"),
            "below the cap the child must have a nested sub_agent tool; got {names:?}"
        );
    }

    #[test]
    fn test_nested_sub_agent_absent_at_cap() {
        // Paired negative case (differs by one depth level): at the leaf level
        // (MAX_SUB_AGENT_DEPTH - 1) the child tool set must NOT include a nested
        // "sub_agent" — this is the termination guarantee.
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let names = sub_agent_child_tool_names(&config, MAX_SUB_AGENT_DEPTH - 1);
        assert!(
            !names.iter().any(|n| n == "sub_agent"),
            "at the cap the leaf child must NOT have a nested sub_agent tool; got {names:?}"
        );
    }

    #[test]
    fn test_sub_agent_depth_ladder_terminates() {
        // Walking the depth ladder: every level below the cap has the nested
        // tool, the leaf omits it, so the tool tree is finite by construction.
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        for depth in 0..MAX_SUB_AGENT_DEPTH {
            let has_nested = sub_agent_child_tool_names(&config, depth)
                .iter()
                .any(|n| n == "sub_agent");
            assert_eq!(
                has_nested,
                depth + 1 < MAX_SUB_AGENT_DEPTH,
                "depth {depth}: nested presence should match depth+1 < MAX"
            );
        }
    }

    #[test]
    fn test_nested_sub_agent_shares_one_store() {
        // Every level shares ONE SharedState: build_sub_agent_tool returns the
        // store threaded through all nested levels. Constructing the tool must
        // not panic and returns a usable store.
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (tool, _state) = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    #[test]
    fn test_sub_agent_description_mentions_recursion() {
        // Discoverability receipt: the description must tell the model it can
        // delegate further (recursion available and finite).
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (tool, _state) = build_sub_agent_tool(&config);
        let desc = tool.description().to_lowercase();
        assert!(
            desc.contains("nesting")
                || desc.contains("recursion")
                || desc.contains("delegate further"),
            "description should mention nesting/recursion; got: {desc}"
        );
    }

    #[test]
    fn test_build_sub_agent_tool_has_task_parameter() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (tool, _state) = build_sub_agent_tool(&config);
        let schema = tool.parameters_schema();
        assert!(
            schema["properties"]["task"].is_object(),
            "Should have 'task' parameter"
        );
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("task")));
    }

    #[test]
    fn test_build_sub_agent_tool_all_providers() {
        // All provider paths should build without panic
        let (_tool_anthropic, _) =
            build_sub_agent_tool(&test_agent_config("anthropic", "claude-sonnet-4-20250514"));
        let (_tool_google, _) =
            build_sub_agent_tool(&test_agent_config("google", "gemini-2.0-flash"));
        let (_tool_openai, _) = build_sub_agent_tool(&test_agent_config("openai", "gpt-4o"));
        let (_tool_bedrock, _) = build_sub_agent_tool(&test_agent_config(
            "bedrock",
            "anthropic.claude-sonnet-4-20250514-v1:0",
        ));
    }

    #[test]
    fn test_build_sub_agent_tool_inherits_dir_restrictions() {
        // Sub-agent should inherit directory restrictions from parent config
        let mut config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        config.dir_restrictions = cli::DirectoryRestrictions {
            allow: vec!["./src".to_string()],
            deny: vec!["/etc".to_string()],
        };
        // Should build without panic — restrictions are applied to file tools
        let (tool, _state) = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    #[test]
    fn test_build_sub_agent_tool_no_restrictions_still_works() {
        // Empty restrictions shouldn't break sub-agent building
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        assert!(config.dir_restrictions.is_empty());
        let (tool, _state) = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    #[test]
    fn test_build_sub_agent_tool_inherits_skills() {
        // Sub-agents should inherit the SkillSet from parent config.
        // We can't inspect SubAgentTool's internal skills field, but we verify
        // the builder chain compiles and runs without panic with skills wired in.
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        // Baseline: empty skills should work fine
        assert!(config.skills.is_empty());
        let (tool, _state) = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");

        // Also verify with a SkillSet loaded from a non-existent dir (still empty, no panic)
        let mut config2 = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        config2.skills = yoagent::skills::SkillSet::empty();
        let (tool2, _state2) = build_sub_agent_tool(&config2);
        assert_eq!(tool2.name(), "sub_agent");
    }

    #[test]
    fn test_build_tools_count_unchanged_with_sub_agent() {
        // Verify build_tools still returns exactly 9 — SubAgentTool is added via with_sub_agent
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        assert_eq!(
            tools.len(),
            9,
            "build_tools must stay at 9 — SubAgentTool is added via with_sub_agent"
        );
    }

    // === SharedState integration tests ===

    #[test]
    fn test_build_sub_agent_tool_returns_shared_state() {
        // The returned SharedState should be a valid, usable handle
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (_tool, shared_state) = build_sub_agent_tool(&config);
        // SharedState starts empty — verify via the async API
        let rt = tokio::runtime::Runtime::new().unwrap();
        let keys = rt.block_on(shared_state.keys());
        assert!(keys.is_empty(), "Fresh SharedState should have no keys");
    }

    #[test]
    fn test_shared_state_parent_can_prepopulate() {
        // Parent agent should be able to write into SharedState before dispatching
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (_tool, shared_state) = build_sub_agent_tool(&config);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            shared_state
                .set("context", "some analysis artifact".into())
                .await
                .unwrap();
            let val = shared_state.get("context").await;
            assert_eq!(val, Some("some analysis artifact".to_string()));
        });
    }

    #[test]
    fn test_shared_state_independent_per_build() {
        // Each call to build_sub_agent_tool should produce an independent SharedState
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (_tool1, state1) = build_sub_agent_tool(&config);
        let (_tool2, state2) = build_sub_agent_tool(&config);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            state1.set("key", "from_agent_1".into()).await.unwrap();
            // state2 should not see state1's data
            assert_eq!(state2.get("key").await, None);
        });
    }

    // === build_tools confirmation integration tests ===

    #[test]
    fn test_build_tools_auto_approve_skips_confirmation() {
        // When auto_approve is true, tools should not have ConfirmTool wrappers
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        assert_eq!(tools.len(), 9);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"bash"));
    }

    #[test]
    fn test_build_tools_no_approve_includes_confirmation() {
        // When auto_approve is false, write_file and edit_file should still have correct names
        // (ConfirmTool delegates name() to inner tool)
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            false,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        assert_eq!(tools.len(), 9);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"list_files"));
        assert!(names.contains(&"search"));
        assert!(names.contains(&"todo"));
    }

    // -----------------------------------------------------------------------
    // StreamingBashTool tests
    // -----------------------------------------------------------------------

    /// Create a ToolContext for testing, with an optional on_update callback
    /// that collects partial results.
    fn test_tool_context(
        updates: Option<Arc<tokio::sync::Mutex<Vec<yoagent::types::ToolResult>>>>,
    ) -> yoagent::types::ToolContext {
        test_tool_context_with_progress(updates, None)
    }

    /// Create a ToolContext for testing, with optional on_update and on_progress callbacks.
    fn test_tool_context_with_progress(
        updates: Option<Arc<tokio::sync::Mutex<Vec<yoagent::types::ToolResult>>>>,
        progress: Option<Arc<tokio::sync::Mutex<Vec<String>>>>,
    ) -> yoagent::types::ToolContext {
        let on_update: Option<yoagent::types::ToolUpdateFn> = updates.map(|u| {
            Arc::new(move |result: yoagent::types::ToolResult| {
                // Use try_lock to avoid blocking in sync callback
                if let Ok(mut guard) = u.try_lock() {
                    guard.push(result);
                }
            }) as yoagent::types::ToolUpdateFn
        });
        let on_progress: Option<yoagent::types::ProgressFn> = progress.map(|p| {
            Arc::new(move |text: String| {
                if let Ok(mut guard) = p.try_lock() {
                    guard.push(text);
                }
            }) as yoagent::types::ProgressFn
        });
        // `ToolContext` is `#[non_exhaustive]` as of yoagent 0.18, so it is
        // built through `new()` and the two callbacks are assigned after — the
        // fields are still `pub`, and this keeps the same values the struct
        // literal set.
        let mut ctx = yoagent::types::ToolContext::new("test-id", "bash");
        ctx.on_update = on_update;
        ctx.on_progress = on_progress;
        ctx
    }

    #[tokio::test]
    async fn test_streaming_bash_deny_patterns() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "rm -rf /"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("blocked by safety policy"),
            "Expected deny pattern error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_deny_pattern_fork_bomb() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": ":(){:|:&};:"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("blocked by safety policy"));
    }

    #[tokio::test]
    async fn test_streaming_bash_confirm_rejection() {
        let tool = StreamingBashTool::default().with_confirm(|_cmd: &str| false);
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo hello"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not confirmed"),
            "Expected confirmation rejection"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_confirm_approval() {
        let tool = StreamingBashTool::default().with_confirm(|_cmd: &str| true);
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo approved"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_ok());
        let text = &result.unwrap().content[0];
        match text {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("approved"));
                assert!(text.contains("Exit code: 0"));
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_basic_execution() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo hello world"});
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("hello world"));
                assert!(text.contains("Exit code: 0"));
            }
            _ => panic!("Expected text content"),
        }
        assert_eq!(result.details["exit_code"], 0);
        assert_eq!(result.details["success"], true);
    }

    #[tokio::test]
    async fn test_streaming_bash_captures_exit_code() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "exit 42"});
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(result.details["exit_code"], 42);
        assert_eq!(result.details["success"], false);
    }

    // --- pipefail + SIGPIPE-141 guard (#579) ---

    #[test]
    fn benign_exit_note_fires_only_on_the_exact_command_and_code() {
        // The cases the annotation exists for.
        assert_eq!(
            benign_exit_note("grep foo src/main.rs", 1),
            Some("no matches found")
        );
        assert_eq!(benign_exit_note("rg TODO", 1), Some("no matches found"));
        assert_eq!(
            benign_exit_note("diff a.txt b.txt", 1),
            Some("files differ")
        );
        assert_eq!(
            benign_exit_note("test -f nope", 1),
            Some("condition was false")
        );
        // Wrapper words and env assignments are stepped over, path stripped.
        assert_eq!(
            benign_exit_note("sudo /usr/bin/grep foo f", 1),
            Some("no matches found")
        );
        assert_eq!(
            benign_exit_note("LC_ALL=C grep foo f", 1),
            Some("no matches found")
        );

        // --- near-miss guards: everything here must stay unannotated ---
        // grep 2 is a REAL error (bad pattern / unreadable file).
        assert_eq!(benign_exit_note("grep foo f", 2), None);
        // grep 0 found matches; there is nothing to explain.
        assert_eq!(benign_exit_note("grep foo f", 0), None);
        // An ordinary command exiting 1 is an ordinary failure.
        assert_eq!(benign_exit_note("cargo test", 1), None);
        assert_eq!(benign_exit_note("false", 1), None);
        // A command merely *containing* the word grep is not grep.
        assert_eq!(benign_exit_note("./my-grep-wrapper.sh", 1), None);
        // Empty input must not panic or match.
        assert_eq!(benign_exit_note("", 1), None);
    }

    #[test]
    fn benign_exit_note_refuses_pipelines_and_substitutions() {
        // With pipefail an exit-1 pipeline may have failed at ANY stage, so
        // claiming "no matches found" would be a confident wrong diagnosis.
        assert_eq!(benign_exit_note("grep foo f | wc -l", 1), None);
        assert_eq!(benign_exit_note("grep foo f && echo hi", 1), None);
        assert_eq!(benign_exit_note("grep foo f; echo hi", 1), None);
        assert_eq!(benign_exit_note("grep $(cat pat) f", 1), None);
        assert_eq!(benign_exit_note("grep foo f || true", 1), None);
        // ...but the plain simple command still annotates.
        assert!(is_simple_command("grep foo f"));
        assert!(!is_simple_command("grep foo f | wc -l"));
    }

    #[test]
    fn test_pipeline_success_helper() {
        // Clean success and the SIGPIPE-141 case are both "not a failure".
        assert!(pipeline_success(0));
        assert!(
            pipeline_success(141),
            "yes | head (SIGPIPE 141) must not fail"
        );
        // Everything else is a failure, including near-misses.
        assert!(!pipeline_success(1));
        assert!(!pipeline_success(3));
        assert!(!pipeline_success(140));
        assert!(!pipeline_success(142));
        assert!(!pipeline_success(-1));
    }

    #[tokio::test]
    async fn test_streaming_bash_pipefail_surfaces_midpipe_failure() {
        // Without pipefail, `exit 3 | cat` reports success (exit 0) because the
        // last stage (cat) succeeds. With pipefail it must surface the failure.
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "sh -c 'exit 3' | cat"});
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(
            result.details["exit_code"], 3,
            "mid-pipeline exit 3 must propagate under pipefail"
        );
        assert_eq!(result.details["success"], false);
    }

    #[tokio::test]
    async fn test_streaming_bash_sigpipe_141_not_a_failure() {
        // `yes | head` is the canonical SIGPIPE idiom: `yes` dies with SIGPIPE
        // (exit 141) when `head` closes the pipe. pipefail would report this as
        // a failure; the SIGPIPE-141 guard must keep it a success.
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "yes | head -n1"});
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(
            result.details["success"], true,
            "yes | head must not be reported as a failure"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_plain_success_still_success() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo hi"});
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(result.details["exit_code"], 0);
        assert_eq!(result.details["success"], true);
    }

    #[tokio::test]
    async fn test_streaming_bash_plain_failure_still_failure() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "false"});
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(result.details["exit_code"], 1);
        assert_eq!(result.details["success"], false);
    }

    #[tokio::test]
    async fn test_streaming_bash_timeout() {
        let tool = StreamingBashTool {
            timeout: Duration::from_millis(200),
            ..Default::default()
        };
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "sleep 30"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("timed out"),
            "Expected timeout error"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_output_truncation() {
        let tool = StreamingBashTool {
            max_output_bytes: 100,
            ..Default::default()
        };
        let ctx = test_tool_context(None);
        // Generate output longer than 100 bytes
        let params = serde_json::json!({"command": "for i in $(seq 1 100); do echo \"line number $i of the output\"; done"});
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                // The accumulated output should have been truncated
                // Total text = "Exit code: 0\n" + accumulated (which was truncated to ~100 bytes)
                assert!(
                    text.contains("truncated") || text.len() < 500,
                    "Output should be truncated or short, got {} bytes",
                    text.len()
                );
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_emits_updates() {
        let updates = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let tool = StreamingBashTool {
            lines_per_update: 1,
            update_interval: Duration::from_millis(10),
            ..Default::default()
        };
        let ctx = test_tool_context(Some(Arc::clone(&updates)));
        // Generate multi-line output with small delays to allow update emission
        let params = serde_json::json!({
            "command": "for i in 1 2 3 4 5; do echo line$i; sleep 0.02; done"
        });
        let result = tool.execute(params, ctx).await.unwrap();
        assert!(result.details["success"] == true);

        let collected = updates.lock().await;
        // Should have emitted at least one streaming update
        assert!(
            !collected.is_empty(),
            "Expected at least one streaming update, got none"
        );
        // The final update (or a late one) should contain multiple lines
        let last = &collected[collected.len() - 1];
        match &last.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(
                    text.contains("line"),
                    "Update should contain partial output"
                );
            }
            _ => panic!("Expected text content in update"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_missing_command_param() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[tokio::test]
    async fn test_streaming_bash_captures_stderr() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let params = serde_json::json!({"command": "echo err_output >&2"});
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("err_output"), "Should capture stderr: {text}");
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_progress_emits_each_line() {
        let progress = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let tool = StreamingBashTool {
            progress_requires_tty: false,
            ..Default::default()
        };
        let ctx = test_tool_context_with_progress(None, Some(Arc::clone(&progress)));
        let params = serde_json::json!({
            "command": "echo alpha; echo beta; echo gamma"
        });
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(result.details["exit_code"], 0);

        let lines = progress.lock().await;
        // Each stdout line should appear in progress
        assert!(
            lines.iter().any(|l| l.contains("alpha")),
            "Progress should contain 'alpha', got: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("beta")),
            "Progress should contain 'beta', got: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("gamma")),
            "Progress should contain 'gamma', got: {lines:?}"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_progress_stderr_prefix() {
        let progress = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let tool = StreamingBashTool {
            progress_requires_tty: false,
            ..Default::default()
        };
        let ctx = test_tool_context_with_progress(None, Some(Arc::clone(&progress)));
        let params = serde_json::json!({
            "command": "echo normal_out; echo err_line >&2"
        });
        let result = tool.execute(params, ctx).await.unwrap();
        assert_eq!(result.details["exit_code"], 0);

        let lines = progress.lock().await;
        // stdout lines emitted as-is (no prefix)
        let stdout_line = lines.iter().find(|l| l.contains("normal_out"));
        assert!(
            stdout_line.is_some(),
            "Should have stdout progress line, got: {lines:?}"
        );
        assert!(
            !stdout_line.unwrap().starts_with("stderr: "),
            "Stdout lines should not have stderr prefix"
        );
        // stderr lines should have "stderr: " prefix
        let stderr_line = lines.iter().find(|l| l.contains("err_line"));
        assert!(
            stderr_line.is_some(),
            "Should have stderr progress line, got: {lines:?}"
        );
        assert!(
            stderr_line.unwrap().starts_with("stderr: "),
            "Stderr line should have 'stderr: ' prefix, got: {:?}",
            stderr_line.unwrap()
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_progress_complete_output_unchanged() {
        // Verify that the final ToolResult still contains the full buffered output
        // (on_progress doesn't affect the return value)
        let progress = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let tool = StreamingBashTool {
            progress_requires_tty: false,
            ..Default::default()
        };
        let ctx = test_tool_context_with_progress(None, Some(Arc::clone(&progress)));
        let params = serde_json::json!({
            "command": "echo line1; echo line2; echo line3"
        });
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("Exit code: 0"));
                assert!(text.contains("line1"));
                assert!(text.contains("line2"));
                assert!(text.contains("line3"));
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_progress_with_timeout() {
        // Verify timeout still works with on_progress set
        let progress = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let tool = StreamingBashTool {
            timeout: Duration::from_millis(200),
            progress_requires_tty: false,
            ..Default::default()
        };
        let ctx = test_tool_context_with_progress(None, Some(Arc::clone(&progress)));
        let params = serde_json::json!({"command": "sleep 30"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("timed out"),
            "Expected timeout error"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_progress_with_cancellation() {
        // Verify cancellation still works with on_progress set
        let progress = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let tool = StreamingBashTool {
            progress_requires_tty: false,
            ..Default::default()
        };
        let ctx = test_tool_context_with_progress(None, Some(Arc::clone(&progress)));
        let cancel = ctx.cancel.clone();

        // Cancel immediately
        cancel.cancel();
        let params = serde_json::json!({"command": "sleep 30"});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
    }

    // ── rename_symbol tool tests ─────────────────────────────────────

    #[test]
    fn test_rename_symbol_tool_name() {
        let tool = RenameSymbolTool::default();
        assert_eq!(tool.name(), "rename_symbol");
    }

    #[test]
    fn test_rename_symbol_tool_label() {
        let tool = RenameSymbolTool::default();
        assert_eq!(tool.label(), "Rename");
    }

    #[test]
    fn test_rename_symbol_tool_schema() {
        let tool = RenameSymbolTool::default();
        let schema = tool.parameters_schema();
        // Must have old_name, new_name, and path properties
        let props = schema["properties"].as_object().unwrap();
        assert!(
            props.contains_key("old_name"),
            "schema should have old_name"
        );
        assert!(
            props.contains_key("new_name"),
            "schema should have new_name"
        );
        assert!(props.contains_key("path"), "schema should have path");
        // old_name and new_name are required
        let required = schema["required"].as_array().unwrap();
        let required_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_strs.contains(&"old_name"));
        assert!(required_strs.contains(&"new_name"));
        // path is NOT required
        assert!(!required_strs.contains(&"path"));
    }

    #[test]
    fn test_rename_result_struct() {
        let result = crate::commands_rename::RenameResult {
            files_changed: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            total_replacements: 5,
            preview: "preview text".to_string(),
            skipped_denied: Vec::new(),
            written: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
        };
        assert_eq!(result.files_changed.len(), 2);
        assert_eq!(result.total_replacements, 5);
        assert_eq!(result.preview, "preview text");
    }

    #[test]
    fn test_rename_symbol_tool_in_build_tools() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            names.contains(&"rename_symbol"),
            "build_tools should include rename_symbol, got: {names:?}"
        );
    }

    #[test]
    fn test_build_tools_with_piped_limit() {
        // build_tools should work with the piped limit too
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS_PIPED,
            false,
            vec![],
            None,
        );
        assert_eq!(tools.len(), 9, "Should still have 9 tools with piped limit");
    }

    #[test]
    fn test_ask_user_tool_schema() {
        let tool = AskUserTool;
        assert_eq!(tool.name(), "ask_user");
        assert_eq!(tool.label(), "ask_user");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["question"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("question")));
    }

    #[test]
    fn test_ask_user_tool_not_in_non_terminal_mode() {
        // In test environment (no terminal), ask_user should NOT be included
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            !names.contains(&"ask_user"),
            "ask_user should not be in non-terminal mode"
        );
    }

    // -----------------------------------------------------------------------
    // TodoTool tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_todo_tool_schema() {
        let tool = TodoTool;
        assert_eq!(tool.name(), "todo");
        assert_eq!(tool.label(), "todo");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["description"].is_object());
        assert!(schema["properties"]["id"].is_object());
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_list_empty() {
        commands_todo::todo_clear();
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "list"}), ctx)
            .await;
        assert!(result.is_ok());
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(text.contains("No tasks"));
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_add_and_list() {
        commands_todo::todo_clear();
        let tool = TodoTool;

        let ctx = test_tool_context(None);
        let result = tool
            .execute(
                serde_json::json!({"action": "add", "description": "Write tests"}),
                ctx,
            )
            .await;
        assert!(result.is_ok());

        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "list"}), ctx)
            .await;
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(text.contains("Write tests"));
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_done() {
        commands_todo::todo_clear();
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        tool.execute(
            serde_json::json!({"action": "add", "description": "Task A"}),
            ctx,
        )
        .await
        .unwrap();

        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "done", "id": 1}), ctx)
            .await;
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(text.contains("done ✓"));
    }

    #[tokio::test]
    async fn test_todo_tool_invalid_action() {
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "explode"}), ctx)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_todo_tool_missing_description() {
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "add"}), ctx)
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_todo_tool_in_build_tools() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            names.contains(&"todo"),
            "build_tools should include todo, got: {names:?}"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_custom_timeout() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        // Pass timeout: 1 second, command sleeps 5 — should time out
        let params = serde_json::json!({"command": "sleep 5", "timeout": 1});
        let result = tool.execute(params, ctx).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("timed out"),
            "Expected timeout error with custom timeout of 1s"
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_custom_timeout_default() {
        let tool = StreamingBashTool::default();
        // Without a timeout param, the schema should use the default (120s)
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(
            props.contains_key("timeout"),
            "Schema should include timeout parameter"
        );
        // Verify the default timeout is 120s by checking the struct field
        assert_eq!(tool.timeout, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_streaming_bash_custom_timeout_clamped() {
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        // Pass timeout: 9999, which should be clamped to 600
        // We verify by running a fast command — it succeeds because the
        // clamped 600s timeout is more than enough for echo
        let params = serde_json::json!({"command": "echo clamped", "timeout": 9999});
        let result = tool.execute(params, ctx).await.unwrap();
        match &result.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("clamped"));
            }
            _ => panic!("Expected text content"),
        }

        // Also verify 0 gets clamped to 1 (minimum) — command still succeeds
        let ctx2 = test_tool_context(None);
        let params2 = serde_json::json!({"command": "echo fast", "timeout": 0});
        let result2 = tool.execute(params2, ctx2).await.unwrap();
        match &result2.content[0] {
            yoagent::types::Content::Text { text } => {
                assert!(text.contains("fast"));
            }
            _ => panic!("Expected text content"),
        }
    }

    // -----------------------------------------------------------------------
    // TodoTool — additional parameter validation tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_todo_tool_wip_missing_id() {
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "wip"}), ctx)
            .await;
        assert!(result.is_err(), "wip without id should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("id"),
            "Error should mention missing 'id', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_todo_tool_remove_missing_id() {
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "remove"}), ctx)
            .await;
        assert!(result.is_err(), "remove without id should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("id"),
            "Error should mention missing 'id', got: {err_msg}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_clear() {
        commands_todo::todo_clear();
        let tool = TodoTool;

        // Add a task first
        let ctx = test_tool_context(None);
        tool.execute(
            serde_json::json!({"action": "add", "description": "Temp task"}),
            ctx,
        )
        .await
        .unwrap();

        // Clear all tasks
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "clear"}), ctx)
            .await;
        assert!(result.is_ok());
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(
            text.contains("cleared"),
            "Clear result should mention 'cleared', got: {text}"
        );

        // Verify list is now empty
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "list"}), ctx)
            .await
            .unwrap();
        let text = match &result.content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(
            text.contains("No tasks"),
            "List after clear should show no tasks, got: {text}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_wip_marks_in_progress() {
        commands_todo::todo_clear();
        let tool = TodoTool;

        // Add then mark wip
        let ctx = test_tool_context(None);
        tool.execute(
            serde_json::json!({"action": "add", "description": "WIP task"}),
            ctx,
        )
        .await
        .unwrap();

        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "wip", "id": 1}), ctx)
            .await;
        assert!(result.is_ok());
        let text = match &result.unwrap().content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(
            text.contains("in-progress"),
            "WIP result should mention 'in-progress', got: {text}"
        );
    }

    #[test]
    fn test_todo_tool_schema_action_required() {
        let tool = TodoTool;
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        let required_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            required_strs.contains(&"action"),
            "action should be required, got: {required_strs:?}"
        );
    }

    #[test]
    fn test_todo_tool_schema_action_enum_values() {
        let tool = TodoTool;
        let schema = tool.parameters_schema();
        let action_enum = schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action should have enum");
        let values: Vec<&str> = action_enum.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(values.len(), 6, "Should have 6 action values");
        for expected in &["list", "add", "done", "wip", "remove", "clear"] {
            assert!(
                values.contains(expected),
                "Action enum should contain '{expected}', got: {values:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // RenameSymbolTool — parameter validation tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rename_symbol_tool_missing_old_name() {
        let tool = RenameSymbolTool::default();
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"new_name": "foo"}), ctx)
            .await;
        assert!(result.is_err(), "Missing old_name should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("old_name"),
            "Error should mention 'old_name', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_rename_symbol_tool_missing_new_name() {
        let tool = RenameSymbolTool::default();
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"old_name": "foo"}), ctx)
            .await;
        assert!(result.is_err(), "Missing new_name should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("new_name"),
            "Error should mention 'new_name', got: {err_msg}"
        );
    }

    #[test]
    fn test_rename_symbol_tool_schema_required_fields() {
        let tool = RenameSymbolTool::default();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        let required_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(
            required_strs.len(),
            2,
            "Should have exactly 2 required fields"
        );
        assert!(required_strs.contains(&"old_name"));
        assert!(required_strs.contains(&"new_name"));
    }

    // -----------------------------------------------------------------------
    // Tool metadata consistency tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_tool_names_unique() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            assert!(
                seen.insert(name),
                "Duplicate tool name found: '{name}' in {names:?}"
            );
        }
    }

    #[test]
    fn test_all_tools_have_descriptions() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        for tool in &tools {
            let desc = tool.description();
            assert!(
                !desc.is_empty(),
                "Tool '{}' has empty description",
                tool.name()
            );
        }
    }

    // -----------------------------------------------------------------------
    // build_tools with directory restrictions and audit
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_tools_with_dir_restrictions() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions {
            allow: vec!["./src".to_string()],
            deny: vec!["/etc".to_string(), "/tmp/secret".to_string()],
        };
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        assert_eq!(
            tools.len(),
            9,
            "Directory restrictions should not change tool count"
        );
    }

    #[test]
    fn test_build_tools_with_audit_wrapping() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            true,
            vec![],
            None,
        );
        assert_eq!(
            tools.len(),
            9,
            "Audit wrapping should not change tool count"
        );
        // Verify tool names survive wrapping
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            names.contains(&"bash"),
            "Should still have bash after audit wrap"
        );
        assert!(
            names.contains(&"todo"),
            "Should still have todo after audit wrap"
        );
    }

    // -----------------------------------------------------------------------
    // StreamingBashTool — default values and construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_streaming_bash_default_cwd_is_none() {
        let tool = StreamingBashTool::default();
        assert!(tool.cwd.is_none(), "Default cwd should be None");
    }

    #[test]
    fn test_streaming_bash_default_timeout_is_120s() {
        let tool = StreamingBashTool::default();
        assert_eq!(tool.timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_streaming_bash_default_max_output_bytes() {
        let tool = StreamingBashTool::default();
        assert_eq!(
            tool.max_output_bytes,
            256 * 1024,
            "Default max output should be 256KB"
        );
    }

    #[test]
    fn test_streaming_bash_default_deny_patterns_count() {
        let tool = StreamingBashTool::default();
        assert!(
            tool.deny_patterns.len() >= 5,
            "Should have at least 5 deny patterns, got: {}",
            tool.deny_patterns.len()
        );
    }

    #[test]
    fn test_streaming_bash_deny_patterns_include_critical() {
        let tool = StreamingBashTool::default();
        assert!(tool.deny_patterns.contains(&"rm -rf /".to_string()));
        assert!(tool.deny_patterns.contains(&"mkfs".to_string()));
        assert!(tool.deny_patterns.contains(&"dd if=".to_string()));
    }

    #[test]
    fn test_streaming_bash_default_confirm_fn_is_none() {
        let tool = StreamingBashTool::default();
        assert!(
            tool.confirm_fn.is_none(),
            "Default confirm_fn should be None"
        );
    }

    #[test]
    fn test_streaming_bash_with_confirm_sets_fn() {
        let tool = StreamingBashTool::default().with_confirm(|_cmd| true);
        assert!(
            tool.confirm_fn.is_some(),
            "with_confirm should set the confirm_fn"
        );
    }

    #[test]
    fn test_streaming_bash_cwd_can_be_set() {
        let tool = StreamingBashTool {
            cwd: Some("/tmp".to_string()),
            ..Default::default()
        };
        assert_eq!(tool.cwd.as_deref(), Some("/tmp"));
    }

    #[tokio::test]
    async fn test_streaming_bash_cwd_is_applied() {
        let tmp = std::env::temp_dir();
        let tool = StreamingBashTool {
            cwd: Some(tmp.to_string_lossy().to_string()),
            ..Default::default()
        };
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"command": "pwd"}), ctx)
            .await
            .unwrap();
        let text = match &result.content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        // pwd output should contain the temp dir path
        let canonical_tmp = std::fs::canonicalize(&tmp)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            text.contains(&canonical_tmp),
            "Expected pwd output to contain '{}', got: {}",
            canonical_tmp,
            text
        );
    }

    #[tokio::test]
    async fn test_streaming_bash_pinned_cwd_refuses_git_redirection_escape() {
        // A pinned-cwd bash (spawn worker confinement) must refuse git
        // redirection outside the confinement root, with an honest error.
        let tmp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let tool = StreamingBashTool::default().with_cwd(tmp.to_string_lossy().to_string());
        for cmd in [
            "git -C /definitely/not/inside status",
            "git --git-dir=/other/.git log",
            "GIT_DIR=/x git status",
            "GIT_WORK_TREE=/y git add .",
        ] {
            let ctx = test_tool_context(None);
            let err = tool
                .execute(serde_json::json!({ "command": cmd }), ctx)
                .await
                .expect_err(&format!("`{cmd}` must be refused under pinned cwd"));
            let msg = format!("{err:?}");
            assert!(
                msg.contains("confined"),
                "error for `{cmd}` should explain confinement, got: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_pinned_cwd_allows_confined_git_and_normal_commands() {
        let tmp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let tool = StreamingBashTool::default().with_cwd(tmp.to_string_lossy().to_string());
        // Relative -C stays confined (cwd is pinned); plain commands untouched.
        for cmd in ["echo hello", "git -C sub status 2>/dev/null || true"] {
            let ctx = test_tool_context(None);
            let result = tool
                .execute(serde_json::json!({ "command": cmd }), ctx)
                .await;
            assert!(result.is_ok(), "`{cmd}` must be allowed under pinned cwd");
        }
    }

    #[tokio::test]
    async fn test_streaming_bash_no_cwd_skips_git_redirection_guard() {
        // Product-safe default: without a pinned cwd (all ordinary sessions),
        // the guard must not fire at all.
        let tool = StreamingBashTool::default();
        let ctx = test_tool_context(None);
        let result = tool
            .execute(
                serde_json::json!({ "command": "git -C /nonexistent-dir-xyz status 2>/dev/null || true" }),
                ctx,
            )
            .await;
        assert!(
            result.is_ok(),
            "unpinned bash must not run the git-escape guard"
        );
    }

    #[test]
    fn test_streaming_bash_default_update_interval() {
        let tool = StreamingBashTool::default();
        assert_eq!(tool.update_interval, Duration::from_millis(500));
    }

    #[test]
    fn test_streaming_bash_default_lines_per_update() {
        let tool = StreamingBashTool::default();
        assert_eq!(tool.lines_per_update, 20);
    }

    #[test]
    fn test_streaming_bash_name_and_description() {
        let tool = StreamingBashTool::default();
        assert_eq!(tool.name(), "bash");
        assert_eq!(tool.label(), "Execute Command");
        let desc = tool.description();
        assert!(desc.contains("Execute a bash command"));
        assert!(desc.contains("timeout"));
    }

    #[test]
    fn test_streaming_bash_schema_properties() {
        let tool = StreamingBashTool::default();
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(
            props.contains_key("command"),
            "Schema should have 'command'"
        );
        assert!(
            props.contains_key("timeout"),
            "Schema should have 'timeout'"
        );
        // command is required, timeout is not
        let required = schema["required"].as_array().unwrap();
        let req_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(req_strs.contains(&"command"));
        assert!(
            !req_strs.contains(&"timeout"),
            "timeout should not be required"
        );
    }

    #[test]
    fn test_streaming_bash_progress_requires_tty_default() {
        let tool = StreamingBashTool::default();
        assert!(
            tool.progress_requires_tty,
            "Default should require TTY for progress"
        );
    }

    // -----------------------------------------------------------------------
    // RenameSymbolTool — description and schema details
    // -----------------------------------------------------------------------

    #[test]
    fn test_rename_symbol_tool_description_content() {
        let tool = RenameSymbolTool::default();
        let desc = tool.description();
        assert!(
            desc.contains("word-boundary"),
            "Description should mention word-boundary matching"
        );
        assert!(
            desc.contains("git-tracked"),
            "Description should mention git-tracked files"
        );
    }

    #[test]
    fn test_rename_symbol_tool_schema_path_is_optional() {
        let tool = RenameSymbolTool::default();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        let req_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(req_strs.len(), 2, "Only old_name and new_name are required");
        assert!(!req_strs.contains(&"path"), "path must NOT be required");
    }

    #[test]
    fn test_rename_symbol_tool_schema_property_types() {
        let tool = RenameSymbolTool::default();
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        // All three properties should be string type
        assert_eq!(props["old_name"]["type"], "string");
        assert_eq!(props["new_name"]["type"], "string");
        assert_eq!(props["path"]["type"], "string");
    }

    // -----------------------------------------------------------------------
    // AskUserTool — description and schema details
    // -----------------------------------------------------------------------

    #[test]
    fn test_ask_user_tool_description_content() {
        let tool = AskUserTool;
        let desc = tool.description();
        assert!(desc.contains("user"), "Description should mention user");
        assert!(
            desc.contains("question"),
            "Description should mention question"
        );
        assert!(
            desc.contains("clarification"),
            "Description should mention clarification"
        );
    }

    #[test]
    fn test_ask_user_tool_schema_question_is_string() {
        let tool = AskUserTool;
        let schema = tool.parameters_schema();
        assert_eq!(
            schema["properties"]["question"]["type"], "string",
            "question parameter should be string type"
        );
    }

    // -----------------------------------------------------------------------
    // TodoTool — edge cases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_todo_tool_missing_action_entirely() {
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        // Pass empty object — no "action" key at all
        let result = tool.execute(serde_json::json!({}), ctx).await;
        assert!(result.is_err(), "Missing action should produce an error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("action"),
            "Error should mention missing 'action', got: {err_msg}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_done_nonexistent_id() {
        commands_todo::todo_clear();
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        // Try to mark done an ID that doesn't exist
        let result = tool
            .execute(serde_json::json!({"action": "done", "id": 999}), ctx)
            .await;
        // This should either error or return a message about the task not existing
        // The implementation uses todo_done which panics or returns error on bad id
        assert!(result.is_err(), "done with non-existent id should fail");
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_remove_nonexistent_id() {
        commands_todo::todo_clear();
        let tool = TodoTool;
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "remove", "id": 999}), ctx)
            .await;
        assert!(result.is_err(), "remove with non-existent id should fail");
    }

    #[tokio::test]
    #[serial]
    async fn test_todo_tool_add_multiple_tasks() {
        commands_todo::todo_clear();
        let tool = TodoTool;

        // Add three tasks
        for desc in &["First", "Second", "Third"] {
            let ctx = test_tool_context(None);
            tool.execute(
                serde_json::json!({"action": "add", "description": desc}),
                ctx,
            )
            .await
            .unwrap();
        }

        // List should show all three
        let ctx = test_tool_context(None);
        let result = tool
            .execute(serde_json::json!({"action": "list"}), ctx)
            .await
            .unwrap();
        let text = match &result.content[0] {
            yoagent::types::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(text.contains("First"), "Should contain 'First'");
        assert!(text.contains("Second"), "Should contain 'Second'");
        assert!(text.contains("Third"), "Should contain 'Third'");
    }

    // -----------------------------------------------------------------------
    // build_tools — canonical tool names
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_tools_canonical_names() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        // In non-terminal mode (test env), there should be exactly these 9 tools
        let expected = [
            "bash",
            "read_file",
            "write_file",
            "edit_file",
            "list_files",
            "search",
            "rename_symbol",
            "todo",
            "web_search",
        ];
        for name in &expected {
            assert!(
                names.contains(name),
                "Expected tool '{name}' not found in: {names:?}"
            );
        }
        assert_eq!(
            names.len(),
            expected.len(),
            "Tool count mismatch: got {names:?}"
        );
    }

    #[test]
    fn test_build_tools_no_ask_user_in_tests() {
        // In test (non-terminal) environment, ask_user should be excluded
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            !names.contains(&"ask_user"),
            "ask_user should NOT appear in non-terminal test env"
        );
    }

    // -----------------------------------------------------------------------
    // simplify_command_pattern
    // -----------------------------------------------------------------------

    #[test]
    fn test_simplify_command_pattern_two_tokens() {
        assert_eq!(simplify_command_pattern("cargo test"), "cargo test*");
        assert_eq!(simplify_command_pattern("cargo build"), "cargo build*");
        assert_eq!(simplify_command_pattern("npm run"), "npm run*");
    }

    #[test]
    fn test_simplify_command_pattern_more_tokens() {
        assert_eq!(
            simplify_command_pattern("cargo build --release"),
            "cargo build*"
        );
        assert_eq!(
            simplify_command_pattern("git commit -m \"hello world\""),
            "git commit*"
        );
        assert_eq!(
            simplify_command_pattern("npm run test -- --watch"),
            "npm run*"
        );
    }

    #[test]
    fn test_simplify_command_pattern_single_token() {
        assert_eq!(simplify_command_pattern("ls"), "ls*");
        assert_eq!(simplify_command_pattern("make"), "make*");
    }

    #[test]
    fn test_simplify_command_pattern_empty() {
        assert_eq!(simplify_command_pattern(""), "*");
    }

    // -----------------------------------------------------------------------
    // build_sub_agent_tool — deeper property checks
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_sub_agent_tool_description_mentions_subtask() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (tool, _) = build_sub_agent_tool(&config);
        let desc = tool.description();
        assert!(
            desc.contains("subtask") || desc.contains("sub-agent"),
            "Sub-agent description should mention subtask/sub-agent, got: {desc}"
        );
    }

    #[tokio::test]
    async fn test_build_sub_agent_tool_shared_state_is_independent() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (_, state1) = build_sub_agent_tool(&config);
        let (_, state2) = build_sub_agent_tool(&config);

        // Set a value in state1, it should NOT appear in state2
        state1
            .set("test_key", "test_value".to_string())
            .await
            .unwrap();
        assert_eq!(state1.get("test_key").await, Some("test_value".to_string()));
        assert_eq!(
            state2.get("test_key").await,
            None,
            "Each build_sub_agent_tool call should produce independent shared state"
        );
    }

    #[tokio::test]
    async fn test_build_sub_agent_tool_shared_state_set_get() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (_, state) = build_sub_agent_tool(&config);

        // Initially empty
        assert_eq!(state.get("nonexistent").await, None);

        // Set and get
        state.set("key1", "value1".to_string()).await.unwrap();
        assert_eq!(state.get("key1").await, Some("value1".to_string()));

        // Overwrite
        state.set("key1", "value2".to_string()).await.unwrap();
        assert_eq!(state.get("key1").await, Some("value2".to_string()));
    }

    #[test]
    fn test_build_sub_agent_tool_schema_has_task_description() {
        let config = test_agent_config("anthropic", "claude-sonnet-4-20250514");
        let (tool, _) = build_sub_agent_tool(&config);
        let schema = tool.parameters_schema();
        // The task parameter should have a description
        let task_prop = &schema["properties"]["task"];
        assert!(task_prop.is_object(), "task should be an object in schema");
        assert!(
            task_prop.get("description").is_some() || task_prop.get("type").is_some(),
            "task property should have type or description"
        );
    }

    #[test]
    fn test_build_sub_agent_tool_openai_compatible_provider() {
        // "openai-compat", "custom", etc. should all use OpenAiCompatProvider path
        let config = test_agent_config("deepseek", "deepseek-v4-pro");
        let (tool, _) = build_sub_agent_tool(&config);
        assert_eq!(tool.name(), "sub_agent");
    }

    // -----------------------------------------------------------------------
    // WebSearchTool — schema, parameter validation, BUILTIN_TOOL_NAMES
    // -----------------------------------------------------------------------

    #[test]
    fn test_web_search_tool_name() {
        let tool = WebSearchTool;
        assert_eq!(tool.name(), "web_search");
    }

    #[test]
    fn test_web_search_tool_in_builtin_names() {
        use crate::agent_builder::BUILTIN_TOOL_NAMES;
        assert!(
            BUILTIN_TOOL_NAMES.contains(&"web_search"),
            "BUILTIN_TOOL_NAMES must include 'web_search' to guard against MCP collisions"
        );
    }

    #[test]
    fn test_web_search_tool_schema_has_query_required() {
        let tool = WebSearchTool;
        let schema = tool.parameters_schema();
        let props = &schema["properties"];
        assert!(props["query"].is_object(), "Should have 'query' property");
        assert_eq!(props["query"]["type"], "string");
        let required = schema["required"].as_array().unwrap();
        assert!(
            required.contains(&serde_json::json!("query")),
            "query should be required"
        );
    }

    #[test]
    fn test_web_search_tool_schema_has_max_results_optional() {
        let tool = WebSearchTool;
        let schema = tool.parameters_schema();
        let props = &schema["properties"];
        assert!(
            props["max_results"].is_object(),
            "Should have 'max_results' property"
        );
        assert_eq!(props["max_results"]["type"], "integer");
        let required = schema["required"].as_array().unwrap();
        assert!(
            !required.contains(&serde_json::json!("max_results")),
            "max_results should NOT be required"
        );
    }

    #[tokio::test]
    async fn test_web_search_tool_missing_query_returns_error() {
        let tool = WebSearchTool;
        let ctx = test_tool_context(None);
        let result = tool.execute(serde_json::json!({}), ctx).await;
        assert!(result.is_err(), "Missing query should return error");
    }

    #[tokio::test]
    async fn test_web_search_tool_empty_query_returns_error() {
        let tool = WebSearchTool;
        let ctx = test_tool_context(None);
        let result = tool.execute(serde_json::json!({"query": "   "}), ctx).await;
        assert!(
            result.is_err(),
            "Empty/whitespace query should return error"
        );
    }

    #[test]
    fn test_web_search_tool_in_build_tools() {
        let perms = cli::PermissionConfig::default();
        let dirs = cli::DirectoryRestrictions::default();
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(
            names.contains(&"web_search"),
            "web_search should be in build_tools output, got: {names:?}"
        );
    }

    #[test]
    fn test_web_search_tool_schema_has_depth_optional() {
        let tool = WebSearchTool;
        let schema = tool.parameters_schema();
        let props = &schema["properties"];
        assert!(props["depth"].is_object(), "Should have 'depth' property");
        assert_eq!(props["depth"]["type"], "string");
        let required = schema["required"].as_array().unwrap();
        assert!(
            !required.contains(&serde_json::json!("depth")),
            "depth should NOT be required"
        );
    }

    #[test]
    fn test_web_search_tool_description_mentions_depth() {
        let tool = WebSearchTool;
        let desc = tool.description();
        assert!(
            desc.contains("depth"),
            "Description should mention 'depth' parameter, got: {desc}"
        );
        assert!(
            desc.contains("deep"),
            "Description should mention 'deep' option, got: {desc}"
        );
    }
}

#[cfg(test)]
mod sub_agent_fallback_gate_tests {
    use super::*;

    #[test]
    fn no_fallback_model_means_no_decoration() {
        // The `None` branch is the whole regression surface: every user who has
        // not configured a fallback must get the un-decorated tool, byte-identically.
        assert_eq!(
            sub_agent_fallback_target("anthropic", "claude-opus-5", None, None),
            None
        );
        // A fallback PROVIDER alone is not enough — a guessed model is not
        // worth a silent second API bill.
        assert_eq!(
            sub_agent_fallback_target("anthropic", "claude-opus-5", Some("google"), None),
            None
        );
        // Empty / whitespace-only is absence, not a model name.
        assert_eq!(
            sub_agent_fallback_target("anthropic", "claude-opus-5", None, Some("")),
            None
        );
        assert_eq!(
            sub_agent_fallback_target("anthropic", "claude-opus-5", None, Some("   ")),
            None
        );
    }

    #[test]
    fn identical_target_means_no_decoration() {
        // Retrying the exact same (provider, model) is a second identical call.
        assert_eq!(
            sub_agent_fallback_target("anthropic", "claude-opus-5", None, Some("claude-opus-5")),
            None
        );
        assert_eq!(
            sub_agent_fallback_target(
                "anthropic",
                "claude-opus-5",
                Some("anthropic"),
                Some("claude-opus-5")
            ),
            None
        );
    }

    #[test]
    fn a_different_model_or_provider_is_a_real_fallback() {
        // Same provider, different model — the key is unchanged.
        assert_eq!(
            sub_agent_fallback_target("anthropic", "claude-opus-5", None, Some("claude-sonnet-5")),
            Some(("anthropic".to_string(), "claude-sonnet-5".to_string()))
        );
        // Different provider carries its own model.
        assert_eq!(
            sub_agent_fallback_target(
                "anthropic",
                "claude-opus-5",
                Some("google"),
                Some("gemini-2.0-flash")
            ),
            Some(("google".to_string(), "gemini-2.0-flash".to_string()))
        );
        // Same model name on a different provider is still a real switch.
        assert_eq!(
            sub_agent_fallback_target("openai", "gpt-4o", Some("openrouter"), Some("gpt-4o")),
            Some(("openrouter".to_string(), "gpt-4o".to_string()))
        );
        // Surrounding whitespace is trimmed, not treated as a distinct model.
        assert_eq!(
            sub_agent_fallback_target("anthropic", "claude-opus-5", None, Some("  haiku-4-5  ")),
            Some(("anthropic".to_string(), "haiku-4-5".to_string()))
        );
    }

    #[test]
    fn same_provider_fallback_reuses_the_key_in_hand() {
        assert_eq!(
            sub_agent_fallback_key("anthropic", "sk-primary", "anthropic"),
            Some("sk-primary".to_string())
        );
    }

    #[test]
    fn cross_provider_fallback_refuses_without_that_providers_key() {
        // Same rule, same reason as AgentConfig::try_switch_to_fallback:
        // retrying with the primary's credential surfaces as a baffling 401.
        // "google" requires a key env var; the test env does not set it.
        let env_var = cli::provider_api_key_env("google").expect("google needs a key");
        if std::env::var(env_var).map(|v| v.is_empty()).unwrap_or(true) {
            assert_eq!(
                sub_agent_fallback_key("anthropic", "sk-primary", "google"),
                None,
                "an unset fallback key must refuse the fallback, not reuse the primary's"
            );
        }
    }

    #[test]
    fn keyless_provider_needs_no_key() {
        // ollama and friends have no API-key env var at all.
        if cli::provider_api_key_env("ollama").is_none() {
            assert_eq!(
                sub_agent_fallback_key("anthropic", "sk-primary", "ollama"),
                Some(String::new())
            );
        }
    }
}
