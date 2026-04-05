//! Tool definitions for the yoyo agent.
//!
//! Contains all agent tool structs and implementations:
//! - `GuardedTool` — directory restriction wrapper
//! - `TruncatingTool` — output truncation wrapper
//! - `ConfirmTool` — user confirmation wrapper for file operations
//! - `StreamingBashTool` — real-time subprocess output
//! - `RenameSymbolTool` — cross-file symbol renaming
//! - `AskUserTool` — interactive question-asking
//! - `TodoTool` — task list management
//! - `build_tools` — assembles the complete tool set
//! - `build_sub_agent_tool` — creates a sub-agent with inherited config

use crate::cli;
use crate::commands_project;
use crate::format::*;
use crate::hooks::{self, maybe_hook, AuditHook, HookRegistry};
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

/// A wrapper tool that checks directory restrictions before delegating to an inner tool.
/// Intercepts the `"path"` parameter from tool arguments and validates it against
/// the configured `DirectoryRestrictions`. If the path is blocked, the tool returns
/// an error without executing the inner tool.
struct GuardedTool {
    inner: Box<dyn AgentTool>,
    restrictions: cli::DirectoryRestrictions,
}

#[async_trait::async_trait]
impl AgentTool for GuardedTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        // Check the "path" parameter against directory restrictions
        if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            if let Err(reason) = self.restrictions.check_path(path) {
                return Err(yoagent::types::ToolError::Failed(reason));
            }
        }
        self.inner.execute(params, ctx).await
    }
}

/// A wrapper tool that truncates large tool output to save context window tokens.
/// When tool output exceeds the configured `max_chars`, preserves the first ~100 and
/// last ~50 lines with a clear truncation marker in between.
struct TruncatingTool {
    inner: Box<dyn AgentTool>,
    max_chars: usize,
}

/// Truncate the text content of a ToolResult if it exceeds the given char limit.
pub(crate) fn truncate_result(
    mut result: yoagent::types::ToolResult,
    max_chars: usize,
) -> yoagent::types::ToolResult {
    use yoagent::Content;
    result.content = result
        .content
        .into_iter()
        .map(|c| match c {
            Content::Text { text } => Content::Text {
                text: truncate_tool_output(&text, max_chars),
            },
            other => other,
        })
        .collect();
    result
}

#[async_trait::async_trait]
impl AgentTool for TruncatingTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        let result = self.inner.execute(params, ctx).await?;
        Ok(truncate_result(result, self.max_chars))
    }
}

/// Wrap a tool with output truncation for large results.
fn with_truncation(tool: Box<dyn AgentTool>, max_chars: usize) -> Box<dyn AgentTool> {
    Box::new(TruncatingTool {
        inner: tool,
        max_chars,
    })
}

/// Wrap a tool with directory restrictions if any are configured.
fn maybe_guard(
    tool: Box<dyn AgentTool>,
    restrictions: &cli::DirectoryRestrictions,
) -> Box<dyn AgentTool> {
    if restrictions.is_empty() {
        tool
    } else {
        Box::new(GuardedTool {
            inner: tool,
            restrictions: restrictions.clone(),
        })
    }
}

/// A wrapper tool that checks directory restrictions before delegating to an Arc-wrapped inner tool.
/// Used by sub-agents to inherit the parent's directory restrictions without needing Box ownership.
struct ArcGuardedTool {
    inner: Arc<dyn AgentTool>,
    restrictions: cli::DirectoryRestrictions,
}

#[async_trait::async_trait]
impl AgentTool for ArcGuardedTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        // Check the "path" parameter against directory restrictions
        if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            if let Err(reason) = self.restrictions.check_path(path) {
                return Err(yoagent::types::ToolError::Failed(reason));
            }
        }
        self.inner.execute(params, ctx).await
    }
}

/// Wrap an Arc-based tool with directory restrictions if any are configured.
/// Used for sub-agent tools which require `Arc<dyn AgentTool>`.
fn maybe_guard_arc(
    tool: Arc<dyn AgentTool>,
    restrictions: &cli::DirectoryRestrictions,
) -> Arc<dyn AgentTool> {
    if restrictions.is_empty() {
        tool
    } else {
        Arc::new(ArcGuardedTool {
            inner: tool,
            restrictions: restrictions.clone(),
        })
    }
}

/// A wrapper tool that prompts for user confirmation before executing write_file or edit_file.
/// Shares the same `always_approved` flag with bash confirmation so "always" applies everywhere.
/// Checks `--allow`/`--deny` patterns against file paths before prompting.
struct ConfirmTool {
    inner: Box<dyn AgentTool>,
    always_approved: Arc<AtomicBool>,
    permissions: cli::PermissionConfig,
}

/// Build a user-facing description for a write_file or edit_file operation.
/// Used by `ConfirmTool` to show what's about to happen before asking y/n/always.
pub fn describe_file_operation(tool_name: &str, params: &serde_json::Value) -> String {
    match tool_name {
        "write_file" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let line_count = if content.is_empty() {
                0
            } else {
                content.lines().count()
            };
            if content.is_empty() {
                format!("write: {path} (⚠ EMPTY content — creates/overwrites with empty file)")
            } else {
                let word = crate::format::pluralize(line_count, "line", "lines");
                format!("write: {path} ({line_count} {word})")
            }
        }
        "edit_file" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let old_text = params
                .get("old_text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new_text = params
                .get("new_text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let old_lines = old_text.lines().count();
            let new_lines = new_text.lines().count();
            format!("edit: {path} ({old_lines} → {new_lines} lines)")
        }
        "rename_symbol" => {
            let old_name = params
                .get("old_name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let new_name = params
                .get("new_name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let scope = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("project");
            format!("rename: {old_name} → {new_name} (in {scope})")
        }
        _ => format!("{tool_name}: file operation"),
    }
}

/// Prompt the user to confirm a file operation (write_file or edit_file).
/// Returns true if the operation should proceed, false if denied.
/// Shared with bash confirm via the same `always_approved` flag.
pub fn confirm_file_operation(
    description: &str,
    path: &str,
    always_approved: &Arc<AtomicBool>,
    permissions: &cli::PermissionConfig,
) -> bool {
    // If user previously chose "always", skip the prompt
    if always_approved.load(Ordering::Relaxed) {
        eprintln!(
            "{GREEN}  ✓ Auto-approved: {RESET}{}",
            truncate_with_ellipsis(description, 120)
        );
        return true;
    }
    // Check permission patterns against the file path
    if let Some(allowed) = permissions.check(path) {
        if allowed {
            eprintln!(
                "{GREEN}  ✓ Permitted: {RESET}{}",
                truncate_with_ellipsis(description, 120)
            );
            return true;
        } else {
            eprintln!(
                "{RED}  ✗ Denied by permission rule: {RESET}{}",
                truncate_with_ellipsis(description, 120)
            );
            return false;
        }
    }
    use std::io::BufRead;
    // Show the operation and ask for approval
    eprint!(
        "{YELLOW}  ⚠ Allow {RESET}{}{YELLOW} ? {RESET}({GREEN}y{RESET}/{RED}n{RESET}/{GREEN}a{RESET}lways) ",
        truncate_with_ellipsis(description, 120)
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
        always_approved.store(true, Ordering::Relaxed);
        eprintln!(
            "{GREEN}  ✓ All subsequent operations will be auto-approved this session.{RESET}"
        );
    }
    approved
}

#[async_trait::async_trait]
impl AgentTool for ConfirmTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        let tool_name = self.inner.name();
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let description = describe_file_operation(tool_name, &params);

        if !confirm_file_operation(&description, path, &self.always_approved, &self.permissions) {
            return Err(yoagent::types::ToolError::Failed(format!(
                "User denied {tool_name} on '{path}'"
            )));
        }
        self.inner.execute(params, ctx).await
    }
}

/// Wrap a tool with a confirmation prompt for write/edit operations.
fn maybe_confirm(
    tool: Box<dyn AgentTool>,
    always_approved: &Arc<AtomicBool>,
    permissions: &cli::PermissionConfig,
) -> Box<dyn AgentTool> {
    Box::new(ConfirmTool {
        inner: tool,
        always_approved: Arc::clone(always_approved),
        permissions: permissions.clone(),
    })
}

// ---------------------------------------------------------------------------
// StreamingBashTool — real-time subprocess output via on_update callbacks
// ---------------------------------------------------------------------------

/// Execute shell commands with real-time streaming output.
///
/// Unlike the upstream `BashTool` which waits for the process to finish before
/// returning output, `StreamingBashTool` reads stdout/stderr line-by-line and
/// calls `ctx.on_update()` periodically so the UI can display partial output
/// as the command runs. This is the difference between staring at a blank screen
/// during `cargo build` and watching compilation progress live.
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
        }
    }
}

impl StreamingBashTool {
    pub fn with_confirm(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        self.confirm_fn = Some(Box::new(f));
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

#[async_trait::async_trait]
impl AgentTool for StreamingBashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn label(&self) -> &str {
        "Execute Command"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return stdout/stderr. Use for running scripts, installing packages, checking system state, etc."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
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

        // Check deny patterns
        for pattern in &self.deny_patterns {
            if command.contains(pattern.as_str()) {
                return Err(ToolError::Failed(format!(
                    "Command blocked by safety policy: contains '{}'. This pattern is denied for safety.",
                    pattern
                )));
            }
        }

        // Check confirmation callback
        if let Some(ref confirm) = self.confirm_fn {
            if !confirm(command) {
                return Err(ToolError::Failed(
                    "Command was not confirmed by the user.".into(),
                ));
            }
        }

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(command);

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }

        // Pipe stdout/stderr for line-by-line reading
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let timeout = self.timeout;
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

                // Read one line from whichever stream has data
                let line = tokio::select! {
                    biased;
                    result = async {
                        match stdout_lines.as_mut() {
                            Some(lines) => lines.next_line().await,
                            None => std::future::pending().await,
                        }
                    }, if !stdout_done => {
                        match result {
                            Ok(Some(line)) => Some(line),
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
                            Ok(Some(line)) => Some(line),
                            Ok(None) => { stderr_done = true; None }
                            Err(_) => { stderr_done = true; None }
                        }
                    }
                };

                if let Some(line) = line {
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

        let exit_code = exit_status.code().unwrap_or(-1);
        let output = accumulated.lock().await.clone();

        // One final update with the complete output
        emit_update(&ctx, &output);

        let formatted = format!("Exit code: {exit_code}\n{output}");

        Ok(TR {
            content: vec![Content::Text { text: formatted }],
            details: serde_json::json!({ "exit_code": exit_code, "success": exit_code == 0 }),
        })
    }
}

// ── rename_symbol agent tool ─────────────────────────────────────────────

/// An agent-invocable tool for renaming symbols across a project.
/// Wraps `commands_project::rename_in_project` so the LLM can do cross-file
/// renames in a single tool call instead of multiple edit_file invocations.
pub(crate) struct RenameSymbolTool;

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

        match commands_project::rename_in_project(old_name, new_name, scope) {
            Ok(result) => {
                let summary = format!(
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
                Ok(TR {
                    content: vec![Content::Text { text: summary }],
                    details: serde_json::json!({}),
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
                    let items = commands_project::todo_list();
                    if items.is_empty() {
                        "No tasks. Use action 'add' to create one.".to_string()
                    } else {
                        commands_project::format_todo_list(&items)
                    }
                }
                "add" => {
                    let desc = params
                        .get("description")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            ToolError::InvalidArgs("Missing 'description' for add action".into())
                        })?;
                    let id = commands_project::todo_add(desc);
                    format!("Added task #{id}: {desc}")
                }
                "done" => {
                    let id = params.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
                        ToolError::InvalidArgs("Missing 'id' for done action".into())
                    })? as usize;
                    commands_project::todo_update(id, commands_project::TodoStatus::Done)
                        .map_err(ToolError::Failed)?;
                    format!("Task #{id} marked as done ✓")
                }
                "wip" => {
                    let id = params.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
                        ToolError::InvalidArgs("Missing 'id' for wip action".into())
                    })? as usize;
                    commands_project::todo_update(id, commands_project::TodoStatus::InProgress)
                        .map_err(ToolError::Failed)?;
                    format!("Task #{id} marked as in-progress")
                }
                "remove" => {
                    let id = params.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
                        ToolError::InvalidArgs("Missing 'id' for remove action".into())
                    })? as usize;
                    let item = commands_project::todo_remove(id).map_err(ToolError::Failed)?;
                    format!("Removed task #{id}: {}", item.description)
                }
                "clear" => {
                    commands_project::todo_clear();
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

/// Build the tool set, optionally with a bash confirmation prompt.
/// When `auto_approve` is false (default), bash commands and file writes require user approval.
/// The "always" option sets a session-wide flag so subsequent operations are auto-approved.
/// The same `always_approved` flag is shared across bash, write_file, and edit_file.
/// When `permissions` has patterns, matching commands/paths are auto-approved or auto-denied.
/// When `dir_restrictions` has rules, file tools check paths before executing.
/// When `audit` is true, all tools are wrapped with the AuditHook via the hook system.
pub fn build_tools(
    auto_approve: bool,
    permissions: &cli::PermissionConfig,
    dir_restrictions: &cli::DirectoryRestrictions,
    max_tool_output: usize,
    audit: bool,
    shell_hooks: Vec<hooks::ShellHook>,
) -> Vec<Box<dyn AgentTool>> {
    // Shared flag: when any tool gets "always", all tools skip prompts
    let always_approved = Arc::new(AtomicBool::new(false));

    let bash = if auto_approve {
        StreamingBashTool::default()
    } else {
        let flag = Arc::clone(&always_approved);
        let perms = permissions.clone();
        StreamingBashTool::default().with_confirm(move |cmd: &str| {
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
            }
            approved
        })
    };

    // Build write_file and edit_file with optional confirmation prompts
    let write_tool: Box<dyn AgentTool> = if auto_approve {
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
    let edit_tool: Box<dyn AgentTool> = if auto_approve {
        maybe_guard(Box::new(EditFileTool::new()), dir_restrictions)
    } else {
        maybe_guard(
            maybe_confirm(Box::new(EditFileTool::new()), &always_approved, permissions),
            dir_restrictions,
        )
    };

    // Build rename_symbol tool with optional confirmation (it writes files)
    let rename_tool: Box<dyn AgentTool> = if auto_approve {
        Box::new(RenameSymbolTool)
    } else {
        maybe_confirm(Box::new(RenameSymbolTool), &always_approved, permissions)
    };

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
        maybe_hook(with_truncation(Box::new(bash), max_tool_output), &hooks),
        maybe_hook(
            with_truncation(
                maybe_guard(Box::new(ReadFileTool::default()), dir_restrictions),
                max_tool_output,
            ),
            &hooks,
        ),
        maybe_hook(with_truncation(write_tool, max_tool_output), &hooks),
        maybe_hook(with_truncation(edit_tool, max_tool_output), &hooks),
        maybe_hook(
            with_truncation(
                maybe_guard(Box::new(ListFilesTool::default()), dir_restrictions),
                max_tool_output,
            ),
            &hooks,
        ),
        maybe_hook(
            with_truncation(
                maybe_guard(Box::new(SearchTool::default()), dir_restrictions),
                max_tool_output,
            ),
            &hooks,
        ),
        maybe_hook(with_truncation(rename_tool, max_tool_output), &hooks),
    ];

    // Only add ask_user in interactive mode (stdin is a terminal).
    // In piped mode or test environments, this tool isn't available.
    if std::io::stdin().is_terminal() {
        tools.push(maybe_hook(Box::new(AskUserTool), &hooks));
    }

    // TodoTool is always available — it only modifies in-memory state, not filesystem
    tools.push(maybe_hook(Box::new(TodoTool), &hooks));

    tools
}

/// Build a SubAgentTool that inherits the parent's provider/model/key.
/// The sub-agent gets basic tools with inherited directory restrictions
/// (no permission prompts, no sub-agent recursion).
pub(crate) fn build_sub_agent_tool(config: &AgentConfig) -> SubAgentTool {
    // Sub-agent gets standard yoagent tools — no permission guards needed
    // since the parent already authorized the delegation.
    // Directory restrictions ARE inherited to prevent sub-agents from bypassing
    // path-based security boundaries.
    let restrictions = &config.dir_restrictions;
    let child_tools: Vec<Arc<dyn AgentTool>> = vec![
        Arc::new(yoagent::tools::bash::BashTool::default()),
        maybe_guard_arc(Arc::new(ReadFileTool::default()), restrictions),
        maybe_guard_arc(Arc::new(WriteFileTool::new()), restrictions),
        maybe_guard_arc(Arc::new(EditFileTool::new()), restrictions),
        maybe_guard_arc(Arc::new(ListFilesTool::default()), restrictions),
        maybe_guard_arc(Arc::new(SearchTool::default()), restrictions),
    ];

    // Select the right provider
    let provider: Arc<dyn StreamProvider> = match config.provider.as_str() {
        "anthropic" => Arc::new(AnthropicProvider),
        "google" => Arc::new(GoogleProvider),
        "bedrock" => Arc::new(BedrockProvider),
        _ => Arc::new(OpenAiCompatProvider),
    };

    SubAgentTool::new("sub_agent", provider)
        .with_description(
            "Delegate a subtask to a fresh sub-agent with its own context window. \
             Use for complex, self-contained subtasks like: researching a codebase, \
             running a series of tests, or implementing a well-scoped change. \
             The sub-agent has bash, file read/write/edit, list, and search tools. \
             It starts with a clean context and returns a summary of what it did.",
        )
        .with_system_prompt(
            "You are a focused sub-agent. Complete the given task efficiently \
             using the tools available. Be thorough but concise in your final \
             response — summarize what you did, what you found, and any issues.",
        )
        .with_model(&config.model)
        .with_api_key(&config.api_key)
        .with_tools(child_tools)
        .with_thinking(config.thinking)
        .with_max_turns(25)
}
