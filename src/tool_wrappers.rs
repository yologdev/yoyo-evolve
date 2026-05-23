//! Tool decorator types that add behavior around any tool.
//!
//! These are generic wrappers — they don't know about specific tool implementations,
//! only about the `AgentTool` trait. Each adds one concern:
//! - `GuardedTool` — directory restriction enforcement
//! - `TruncatingTool` — output truncation for context window savings
//! - `ConfirmTool` — user confirmation before write/edit operations
//! - `ArcGuardedTool` — directory restrictions for `Arc<dyn AgentTool>` (sub-agents)
//!
//! Helper functions (`maybe_guard`, `maybe_confirm`, `with_truncation`, `maybe_guard_arc`)
//! conditionally wrap tools based on configuration.

use crate::cli;
use crate::format::*;

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use yoagent::types::AgentTool;

// ---------------------------------------------------------------------------
// GuardedTool — directory restriction wrapper (Box-based)
// ---------------------------------------------------------------------------

/// A wrapper tool that checks directory restrictions before delegating to an inner tool.
/// Intercepts the `"path"` parameter from tool arguments and validates it against
/// the configured `DirectoryRestrictions`. If the path is blocked, the tool returns
/// an error without executing the inner tool.
pub(crate) struct GuardedTool {
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

/// Wrap a tool with directory restrictions if any are configured.
pub(crate) fn maybe_guard(
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

// ---------------------------------------------------------------------------
// ArcGuardedTool — directory restriction wrapper (Arc-based, for sub-agents)
// ---------------------------------------------------------------------------

/// A wrapper tool that checks directory restrictions before delegating to an Arc-wrapped inner tool.
/// Used by sub-agents to inherit the parent's directory restrictions without needing Box ownership.
pub(crate) struct ArcGuardedTool {
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
pub(crate) fn maybe_guard_arc(
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

// ---------------------------------------------------------------------------
// TruncatingTool — output truncation wrapper
// ---------------------------------------------------------------------------

/// A wrapper tool that truncates large tool output to save context window tokens.
/// When tool output exceeds the configured `max_chars`, preserves the first ~100 and
/// last ~50 lines with a clear truncation marker in between.
pub(crate) struct TruncatingTool {
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
pub(crate) fn with_truncation(tool: Box<dyn AgentTool>, max_chars: usize) -> Box<dyn AgentTool> {
    Box::new(TruncatingTool {
        inner: tool,
        max_chars,
    })
}

// ---------------------------------------------------------------------------
// Permission persistence for file operations
// ---------------------------------------------------------------------------

use std::collections::HashSet as PersistHashSet;

/// Generate a directory-based allow pattern from a file path.
///
/// For files in a subdirectory: extracts the directory and appends `/*`.
/// For root files: uses `*.ext` based on the file extension.
/// Examples:
///   `src/main.rs`        → `src/*`
///   `src/format/mod.rs`  → `src/format/*`
///   `README.md`          → `*.md`
///   `Cargo.toml`         → `*.toml`
///   `script`             → `script`  (no extension, no directory — use exact name)
pub fn file_path_to_allow_pattern(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "*".to_string();
    }

    // Normalise separators and strip leading ./
    let clean = path.replace('\\', "/");
    let clean = clean.strip_prefix("./").unwrap_or(&clean);

    if let Some(idx) = clean.rfind('/') {
        // Has a directory component — use `dir/*`
        let dir = &clean[..idx];
        format!("{dir}/*")
    } else {
        // Root-level file — try `*.ext`
        if let Some(dot) = clean.rfind('.') {
            let ext = &clean[dot..]; // e.g. ".rs"
            format!("*{ext}")
        } else {
            // No extension, no directory — use exact name
            clean.to_string()
        }
    }
}

/// Track which file patterns we've already offered to persist this session.
fn already_offered_file_persistence(pattern: &str) -> bool {
    static OFFERED: std::sync::LazyLock<Mutex<PersistHashSet<String>>> =
        std::sync::LazyLock::new(|| Mutex::new(PersistHashSet::new()));
    let mut set = OFFERED.lock().unwrap_or_else(|e| e.into_inner());
    !set.insert(pattern.to_string())
}

/// After the user says "always" on a file operation, offer to persist a
/// directory-based allow pattern to `.yoyo.toml`.
///
/// Returns without action if the pattern was already offered this session.
/// This is the file-operation parallel to `tools::offer_persist_pattern` for bash.
pub fn offer_persist_file_pattern(path: &str) {
    let pattern = file_path_to_allow_pattern(path);

    // Don't re-ask if we already offered this directory pattern this session
    if already_offered_file_persistence(&pattern) {
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

// ---------------------------------------------------------------------------
// ConfirmTool — user confirmation wrapper for file operations
// ---------------------------------------------------------------------------

/// A wrapper tool that prompts for user confirmation before executing write_file or edit_file.
/// Shares the same `always_approved` flag with bash confirmation so "always" applies everywhere.
/// Checks `--allow`/`--deny` patterns against file paths before prompting.
pub(crate) struct ConfirmTool {
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

/// Maximum combined lines (old_text + new_text) before the diff preview is truncated.
const EDIT_DIFF_MAX_LINES: usize = 40;

/// Generate a colored diff preview for an `edit_file` operation.
///
/// Extracts `old_text` and `new_text` from the tool params and returns a
/// formatted diff string using the LCS-based diff renderer. Returns an empty
/// string when both texts are identical or when the params are missing.
///
/// If the combined old+new text exceeds `EDIT_DIFF_MAX_LINES`, the diff is
/// truncated with a `... (N more lines)` ellipsis.
pub fn format_edit_diff_preview(params: &serde_json::Value) -> String {
    let old_text = params
        .get("old_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_text = params
        .get("new_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if old_text.is_empty() && new_text.is_empty() {
        return String::new();
    }

    let diff = crate::format::format_edit_diff(old_text, new_text);
    if diff.is_empty() {
        return diff;
    }

    // Apply additional truncation for very large diffs
    let total_input_lines = old_text.lines().count() + new_text.lines().count();
    if total_input_lines > EDIT_DIFF_MAX_LINES {
        crate::format::truncate_diff_preview(&diff, 20)
    } else {
        diff
    }
}

/// Prompt the user to confirm a file operation, optionally showing a diff preview.
///
/// When `diff_preview` is `Some(text)`, the colored diff is printed to stderr
/// before the interactive confirmation prompt. The diff is NOT shown for
/// auto-approved or permission-approved operations.
pub fn confirm_file_operation(
    description: &str,
    path: &str,
    always_approved: &Arc<AtomicBool>,
    permissions: &cli::PermissionConfig,
    diff_preview: Option<&str>,
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
    // Show the diff preview before the confirmation prompt (if available)
    if let Some(diff) = diff_preview {
        if !diff.is_empty() {
            eprintln!("{}", diff);
        }
    }
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
        // Offer to persist a directory-based allow pattern to .yoyo.toml
        offer_persist_file_pattern(path);
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

        // Generate a diff preview for edit_file operations
        let diff_preview = if tool_name == "edit_file" {
            let preview = format_edit_diff_preview(&params);
            if preview.is_empty() {
                None
            } else {
                Some(preview)
            }
        } else {
            None
        };

        if !confirm_file_operation(
            &description,
            path,
            &self.always_approved,
            &self.permissions,
            diff_preview.as_deref(),
        ) {
            return Err(yoagent::types::ToolError::Failed(format!(
                "User denied {tool_name} on '{path}'"
            )));
        }
        self.inner.execute(params, ctx).await
    }
}

/// Wrap a tool with a confirmation prompt for write/edit operations.
pub(crate) fn maybe_confirm(
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
// AutoCheckTool — runs check command after successful file edits
// ---------------------------------------------------------------------------

/// Maximum characters of auto-check output to append to tool results.
const AUTO_CHECK_MAX_CHARS: usize = 2000;

/// A tool wrapper that automatically runs a check command after file edits.
/// When a watch command is configured (via `/watch set`), it runs the first
/// watch phase (typically lint) after successful write_file or edit_file
/// operations and appends any errors to the tool result.
///
/// This gives the agent immediate compilation feedback inline with each edit,
/// catching errors before moving on to the next file — similar to how Aider
/// runs lint+test after each individual file write.
pub(crate) struct AutoCheckTool {
    inner: Box<dyn AgentTool>,
}

#[async_trait::async_trait]
impl AgentTool for AutoCheckTool {
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

        // Only run check when a watch command is active
        let commands = crate::watch::get_watch_commands();
        if commands.is_empty() {
            return Ok(result);
        }

        // Use only the first phase (typically lint/check, not the full test suite)
        let check_cmd = &commands[0];
        let (passed, output) = crate::watch::run_watch_command(check_cmd);

        if passed {
            return Ok(result);
        }

        // Append check failure output to the tool result
        let truncated_output = if output.len() > AUTO_CHECK_MAX_CHARS {
            // Find safe char boundary for truncation
            let mut b = AUTO_CHECK_MAX_CHARS;
            while b > 0 && !output.is_char_boundary(b) {
                b -= 1;
            }
            format!(
                "{}...\n[auto-check output truncated at {AUTO_CHECK_MAX_CHARS} chars]",
                &output[..b]
            )
        } else {
            output
        };

        let check_notice = format!("\n\n⚠ Auto-check failed ({check_cmd}):\n{truncated_output}");

        // Append the check notice to each text content block
        let new_content = result
            .content
            .into_iter()
            .map(|c| match c {
                yoagent::Content::Text { text } => yoagent::Content::Text {
                    text: format!("{text}{check_notice}"),
                },
                other => other,
            })
            .collect();

        Ok(yoagent::types::ToolResult {
            content: new_content,
            details: result.details,
        })
    }
}

/// Wrap a tool with auto-check: runs the watch command after successful edits
/// and appends any errors to the tool result for immediate feedback.
pub(crate) fn with_auto_check(tool: Box<dyn AgentTool>) -> Box<dyn AgentTool> {
    Box::new(AutoCheckTool { inner: tool })
}

// ---------------------------------------------------------------------------
// SmartEditTool — augments edit_file "not found" errors with line-number context
// ---------------------------------------------------------------------------

/// Maximum file size (bytes) we'll read for nearest-match searching.
const SMART_EDIT_MAX_FILE_SIZE: u64 = 100_000;

/// Number of context lines to show around the nearest match.
const SMART_EDIT_CONTEXT_LINES: usize = 5;

/// A wrapper tool specifically for `edit_file` that intercepts "not found"
/// failures and augments the error message with:
/// - The line number of the nearest match (first-line matching with whitespace normalization)
/// - A snippet of actual content at that location
/// - A hint when the mismatch is purely whitespace/indentation
pub(crate) struct SmartEditTool {
    inner: Box<dyn AgentTool>,
}

/// Search a file's content for the best match of `old_text`, returning
/// `(line_number_1indexed, is_whitespace_only_diff, snippet)`.
fn find_nearest_match(file_content: &str, old_text: &str) -> Option<(usize, bool, String)> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    if old_lines.is_empty() {
        return None;
    }

    // Find the first non-empty line in old_text to use as anchor
    let anchor = old_lines.iter().find(|l| !l.trim().is_empty())?;
    let anchor_trimmed = anchor.trim();

    let file_lines: Vec<&str> = file_content.lines().collect();

    // Search for lines whose trimmed content matches the anchor's trimmed content
    let mut best_match: Option<(usize, usize)> = None; // (line_idx, matching_lines_count)

    for (i, line) in file_lines.iter().enumerate() {
        if line.trim() == anchor_trimmed {
            // Count how many subsequent lines also match (trimmed)
            let mut match_count = 1;
            let anchor_offset = old_lines
                .iter()
                .position(|l| !l.trim().is_empty())
                .unwrap_or(0);

            for j in 1..(old_lines.len() - anchor_offset) {
                let old_idx = anchor_offset + j;
                let file_idx = i + j;
                if file_idx < file_lines.len()
                    && old_idx < old_lines.len()
                    && file_lines[file_idx].trim() == old_lines[old_idx].trim()
                {
                    match_count += 1;
                } else {
                    break;
                }
            }

            if best_match.is_none_or(|(_, prev_count)| match_count > prev_count) {
                // Adjust line number to account for leading empty lines in old_text
                let start_line = if anchor_offset > 0 && i >= anchor_offset {
                    i - anchor_offset
                } else {
                    i
                };
                best_match = Some((start_line, match_count));
            }
        }
    }

    let (match_line_idx, _match_count) = best_match?;

    // Check if the entire old_text matches but only differs in whitespace
    let is_ws_only = {
        let mut all_match_trimmed = true;
        let mut any_exact_mismatch = false;
        for (j, old_line) in old_lines.iter().enumerate() {
            let file_idx = match_line_idx + j;
            if file_idx < file_lines.len() {
                if file_lines[file_idx].trim() == old_line.trim() {
                    if file_lines[file_idx] != *old_line {
                        any_exact_mismatch = true;
                    }
                } else {
                    all_match_trimmed = false;
                    break;
                }
            } else {
                all_match_trimmed = false;
                break;
            }
        }
        all_match_trimmed && any_exact_mismatch
    };

    // Build snippet: up to SMART_EDIT_CONTEXT_LINES lines starting at the match
    let snippet_start = match_line_idx;
    let snippet_end = (match_line_idx + SMART_EDIT_CONTEXT_LINES).min(file_lines.len());
    let snippet: String = file_lines[snippet_start..snippet_end]
        .iter()
        .enumerate()
        .map(|(j, line)| format!("{:>4} │ {}", snippet_start + j + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    Some((match_line_idx + 1, is_ws_only, snippet)) // 1-indexed line number
}

/// Wrap an edit_file tool with smart error augmentation.
pub(crate) fn with_smart_edit(tool: Box<dyn AgentTool>) -> Box<dyn AgentTool> {
    Box::new(SmartEditTool { inner: tool })
}

#[async_trait::async_trait]
impl AgentTool for SmartEditTool {
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
        match self.inner.execute(params.clone(), ctx).await {
            Ok(result) => Ok(result),
            Err(yoagent::types::ToolError::Failed(msg)) if msg.contains("not found") => {
                // Try to augment the error with location information
                let augmented = self.augment_not_found_error(&msg, &params);
                Err(yoagent::types::ToolError::Failed(augmented))
            }
            Err(other) => Err(other),
        }
    }
}

impl SmartEditTool {
    fn augment_not_found_error(&self, original_msg: &str, params: &serde_json::Value) -> String {
        // Extract path and old_text from params
        let path = match params.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return original_msg.to_string(),
        };
        let old_text = match params.get("old_text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return original_msg.to_string(),
        };

        // Check file size — skip for huge files
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return original_msg.to_string(),
        };
        if metadata.len() > SMART_EDIT_MAX_FILE_SIZE {
            return original_msg.to_string();
        }

        // Read the file
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return original_msg.to_string(),
        };

        // Search for nearest match
        match find_nearest_match(&content, old_text) {
            Some((line_num, is_ws_only, snippet)) => {
                let mut augmented = original_msg.to_string();
                augmented.push_str(&format!(
                    "\n\n📍 Nearest match at line {}:\n```\n{}\n```",
                    line_num, snippet
                ));
                if is_ws_only {
                    augmented.push_str(
                        "\n\n⚠️ Hint: the text exists but indentation/whitespace differs. \
                         Use read_file to see the exact whitespace.",
                    );
                }
                augmented
            }
            None => original_msg.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// RecoveryHintTool — appends recovery hints to tool error messages
// ---------------------------------------------------------------------------

/// Tracks consecutive failures per tool name so recovery hints can escalate.
///
/// Shared across all tools in a session — when tool A fails 3 times,
/// the hint it gets is more aggressive than on its first failure.
/// When a tool succeeds, its counter resets.
#[derive(Clone, Default)]
pub(crate) struct ToolFailureTracker {
    counts: Arc<Mutex<HashMap<String, u32>>>,
}

#[allow(dead_code)] // record_failure / record_success called only via RecoveryHintTool
impl ToolFailureTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Increment the failure count for a tool and return the new count.
    fn record_failure(&self, tool_name: &str) -> u32 {
        let mut map = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        let count = map.entry(tool_name.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Reset the failure count for a tool (called on success).
    fn record_success(&self, tool_name: &str) {
        let mut map = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(tool_name);
    }

    /// Get the current failure count for a tool (for testing).
    #[cfg(test)]
    fn get(&self, tool_name: &str) -> u32 {
        let map = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        map.get(tool_name).copied().unwrap_or(0)
    }
}

/// A wrapper tool that enriches error messages with recovery hints.
///
/// On success the failure counter resets. On failure the counter increments
/// and a tool-specific recovery hint (from `prompt_retry::tool_recovery_hint`)
/// is appended to the error message.
#[allow(dead_code)] // Public API — wired in a follow-up task
pub(crate) struct RecoveryHintTool {
    inner: Box<dyn AgentTool>,
    tracker: ToolFailureTracker,
}

#[async_trait::async_trait]
impl AgentTool for RecoveryHintTool {
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
        let tool_name = self.inner.name().to_string();
        match self.inner.execute(params, ctx).await {
            Ok(result) => {
                self.tracker.record_success(&tool_name);
                Ok(result)
            }
            Err(yoagent::types::ToolError::Failed(msg)) => {
                let attempt = self.tracker.record_failure(&tool_name);
                let hint = crate::prompt_retry::tool_recovery_hint(&tool_name, attempt);
                Err(yoagent::types::ToolError::Failed(format!(
                    "{msg}\n\n💡 Recovery hint: {hint}"
                )))
            }
            Err(other) => {
                // Non-Failed errors (NotFound, InvalidArgs, Cancelled) pass through
                // but still count as failures for escalation purposes
                self.tracker.record_failure(&tool_name);
                Err(other)
            }
        }
    }
}

/// Wrap a tool with recovery hints on failure. The `tracker` is shared across
/// all tools so consecutive failures of the same tool escalate the advice.
pub(crate) fn with_recovery_hints(
    tool: Box<dyn AgentTool>,
    tracker: &ToolFailureTracker,
) -> Box<dyn AgentTool> {
    Box::new(RecoveryHintTool {
        inner: tool,
        tracker: tracker.clone(),
    })
}

// ---------------------------------------------------------------------------
// LiteDescriptionTool — augments tool descriptions with JSON format examples
// ---------------------------------------------------------------------------

/// A wrapper tool that appends a JSON input example to the tool's description.
///
/// Small/local LLMs (llama3, mistral, codellama, phi) struggle with tool-call
/// formatting because they haven't been heavily fine-tuned on Anthropic's
/// tool-use schema. Adding explicit JSON input examples to each tool's
/// description dramatically improves tool-call accuracy for these models.
pub(crate) struct LiteDescriptionTool {
    inner: Box<dyn AgentTool>,
    augmented_description: String,
}

#[async_trait::async_trait]
impl AgentTool for LiteDescriptionTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        &self.augmented_description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        self.inner.execute(params, ctx).await
    }
}

/// Return a JSON example string for a given tool name, or `None` if no
/// example is defined (unknown tools pass through without augmentation).
fn lite_example_for_tool(name: &str) -> Option<&'static str> {
    match name {
        "bash" => Some(r#"{"command": "ls -la src/"}"#),
        "read_file" => Some(r#"{"path": "src/main.rs"}"#),
        "write_file" => Some(r#"{"path": "hello.txt", "content": "Hello world"}"#),
        "edit_file" => {
            Some(r#"{"path": "src/main.rs", "old_text": "let x = 1;", "new_text": "let x = 2;"}"#)
        }
        "list_files" => Some(r#"{"path": "src/"}"#),
        "search" => Some(r#"{"pattern": "fn main", "path": "src/"}"#),
        _ => None,
    }
}

/// Wrap a tool with an augmented description that includes a JSON format
/// example. For unknown tool names, the tool is returned as-is (no wrapper).
pub(crate) fn with_lite_description(tool: Box<dyn AgentTool>) -> Box<dyn AgentTool> {
    match lite_example_for_tool(tool.name()) {
        Some(example) => {
            let augmented_description = format!("{}\n\nExample: {}", tool.description(), example);
            Box::new(LiteDescriptionTool {
                inner: tool,
                augmented_description,
            })
        }
        None => tool,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // === describe_file_operation tests ===

    #[test]
    fn test_describe_write_file_operation() {
        let params = serde_json::json!({
            "path": "src/main.rs",
            "content": "line1\nline2\nline3\n"
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("src/main.rs"));
        assert!(desc.contains("3 lines")); // Rust's .lines() strips trailing newline
    }

    #[test]
    fn test_describe_write_file_empty_content() {
        let params = serde_json::json!({
            "path": "empty.txt",
            "content": ""
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("empty.txt"));
        assert!(
            desc.contains("EMPTY content"),
            "Empty content should show warning, got: {desc}"
        );
    }

    #[test]
    fn test_describe_write_file_missing_content() {
        // When the content key is entirely absent (model bug), treat as empty
        let params = serde_json::json!({
            "path": "missing.txt"
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("missing.txt"));
        assert!(
            desc.contains("EMPTY content"),
            "Missing content should show warning, got: {desc}"
        );
    }

    #[test]
    fn test_describe_write_file_normal_content() {
        // Normal write_file should NOT show the empty warning
        let params = serde_json::json!({
            "path": "hello.txt",
            "content": "hello world\n"
        });
        let desc = describe_file_operation("write_file", &params);
        assert!(desc.contains("write:"));
        assert!(desc.contains("hello.txt"));
        assert!(desc.contains("1 line"));
        assert!(
            !desc.contains("EMPTY"),
            "Non-empty content should not show warning, got: {desc}"
        );
    }

    #[test]
    fn test_describe_edit_file_operation() {
        let params = serde_json::json!({
            "path": "src/cli.rs",
            "old_text": "old line 1\nold line 2",
            "new_text": "new line 1\nnew line 2\nnew line 3"
        });
        let desc = describe_file_operation("edit_file", &params);
        assert!(desc.contains("edit:"));
        assert!(desc.contains("src/cli.rs"));
        assert!(desc.contains("2 → 3 lines"));
    }

    #[test]
    fn test_describe_edit_file_missing_params() {
        let params = serde_json::json!({
            "path": "test.rs"
        });
        let desc = describe_file_operation("edit_file", &params);
        assert!(desc.contains("edit:"));
        assert!(desc.contains("test.rs"));
        assert!(desc.contains("0 → 0 lines"));
    }

    #[test]
    fn test_describe_unknown_tool() {
        let params = serde_json::json!({});
        let desc = describe_file_operation("unknown_tool", &params);
        assert!(desc.contains("unknown_tool"));
    }

    // === format_edit_diff_preview tests ===

    #[test]
    fn test_edit_diff_preview_basic_change() {
        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "let x = 1;",
            "new_text": "let x = 2;"
        });
        let preview = format_edit_diff_preview(&params);
        assert!(!preview.is_empty(), "Should produce a diff preview");
        assert!(
            preview.contains("- let x = 1;"),
            "Should show removed line: {preview}"
        );
        assert!(
            preview.contains("+ let x = 2;"),
            "Should show added line: {preview}"
        );
    }

    #[test]
    fn test_edit_diff_preview_multiline() {
        let params = serde_json::json!({
            "path": "src/lib.rs",
            "old_text": "fn foo() {\n    println!(\"old\");\n}",
            "new_text": "fn foo() {\n    println!(\"new\");\n    println!(\"extra\");\n}"
        });
        let preview = format_edit_diff_preview(&params);
        assert!(preview.contains("- "), "Should have removed lines");
        assert!(preview.contains("+ "), "Should have added lines");
        assert!(preview.contains("new"), "Should show new content");
        assert!(preview.contains("extra"), "Should show extra line");
    }

    #[test]
    fn test_edit_diff_preview_identical_texts() {
        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "same text",
            "new_text": "same text"
        });
        let preview = format_edit_diff_preview(&params);
        assert!(
            preview.is_empty(),
            "Identical texts should produce empty preview"
        );
    }

    #[test]
    fn test_edit_diff_preview_missing_params() {
        let params = serde_json::json!({
            "path": "src/main.rs"
        });
        let preview = format_edit_diff_preview(&params);
        assert!(
            preview.is_empty(),
            "Missing old_text/new_text should produce empty preview"
        );
    }

    #[test]
    fn test_edit_diff_preview_empty_old_text() {
        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "",
            "new_text": "new line 1\nnew line 2"
        });
        let preview = format_edit_diff_preview(&params);
        assert!(
            !preview.is_empty(),
            "Adding new content should produce preview"
        );
        assert!(
            preview.contains("+ new line 1"),
            "Should show additions: {preview}"
        );
        assert!(
            !preview.contains("- "),
            "Should have no removals for pure addition"
        );
    }

    #[test]
    fn test_edit_diff_preview_empty_new_text() {
        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "old line 1\nold line 2",
            "new_text": ""
        });
        let preview = format_edit_diff_preview(&params);
        assert!(
            !preview.is_empty(),
            "Deleting content should produce preview"
        );
        assert!(
            preview.contains("- old line 1"),
            "Should show deletions: {preview}"
        );
        assert!(
            !preview.contains("+ "),
            "Should have no additions for pure deletion"
        );
    }

    #[test]
    fn test_edit_diff_preview_truncates_large_diff() {
        // Generate old_text and new_text that together exceed EDIT_DIFF_MAX_LINES (40)
        let old_lines: Vec<String> = (0..25).map(|i| format!("old line {i}")).collect();
        let new_lines: Vec<String> = (0..25).map(|i| format!("new line {i}")).collect();
        let params = serde_json::json!({
            "path": "src/big.rs",
            "old_text": old_lines.join("\n"),
            "new_text": new_lines.join("\n")
        });
        let preview = format_edit_diff_preview(&params);
        assert!(
            !preview.is_empty(),
            "Large diff should still produce preview"
        );
        // The preview should be truncated (the combined 50 lines exceeds the 40-line threshold)
        assert!(
            preview.contains("more lines"),
            "Large diff should be truncated with ellipsis: {preview}"
        );
    }

    #[test]
    fn test_edit_diff_preview_small_diff_not_truncated() {
        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "line 1\nline 2\nline 3",
            "new_text": "line 1\nmodified\nline 3"
        });
        let preview = format_edit_diff_preview(&params);
        // 6 total input lines — well under the 40-line threshold
        assert!(!preview.is_empty());
        assert!(
            !preview.contains("more lines"),
            "Small diff should not be truncated: {preview}"
        );
    }

    // === confirm_file_operation tests ===

    #[test]
    fn test_confirm_file_operation_auto_approved_flag() {
        // When always_approved is true, confirm should return true immediately
        let flag = Arc::new(AtomicBool::new(true));
        let perms = cli::PermissionConfig::default();
        let result =
            confirm_file_operation("write: test.rs (5 lines)", "test.rs", &flag, &perms, None);
        assert!(
            result,
            "Should auto-approve when always_approved flag is set"
        );
    }

    #[test]
    fn test_confirm_file_operation_with_allow_pattern() {
        // Permission patterns should match file paths
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec!["*.md".to_string()],
            deny: vec![],
        };
        let result = confirm_file_operation(
            "write: README.md (10 lines)",
            "README.md",
            &flag,
            &perms,
            None,
        );
        assert!(result, "Should auto-approve paths matching allow pattern");
    }

    #[test]
    fn test_confirm_file_operation_with_deny_pattern() {
        // Denied patterns should block the operation
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec![],
            deny: vec!["*.key".to_string()],
        };
        let result = confirm_file_operation(
            "write: secrets.key (1 line)",
            "secrets.key",
            &flag,
            &perms,
            None,
        );
        assert!(!result, "Should deny paths matching deny pattern");
    }

    #[test]
    fn test_confirm_file_operation_deny_overrides_allow() {
        // Deny takes priority over allow
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec!["*".to_string()],
            deny: vec!["*.key".to_string()],
        };
        let result = confirm_file_operation(
            "write: secrets.key (1 line)",
            "secrets.key",
            &flag,
            &perms,
            None,
        );
        assert!(!result, "Deny should override allow");
    }

    #[test]
    fn test_confirm_file_operation_allow_src_pattern() {
        // Realistic pattern: allow all files under src/
        let flag = Arc::new(AtomicBool::new(false));
        let perms = cli::PermissionConfig {
            allow: vec!["src/*".to_string()],
            deny: vec![],
        };
        let result = confirm_file_operation(
            "edit: src/main.rs (2 → 3 lines)",
            "src/main.rs",
            &flag,
            &perms,
            None,
        );
        assert!(
            result,
            "Should auto-approve src/ files with 'src/*' pattern"
        );
    }

    // === Shared approval flag test ===

    #[test]
    fn test_always_approved_shared_between_bash_and_file_tools() {
        // Simulates: user says "always" on a bash prompt,
        // subsequent file operations should auto-approve too.
        // This test verifies the shared flag concept.
        let always_approved = Arc::new(AtomicBool::new(false));
        let bash_flag = Arc::clone(&always_approved);
        let file_flag = Arc::clone(&always_approved);

        // Initially, nothing is auto-approved
        assert!(!bash_flag.load(Ordering::Relaxed));
        assert!(!file_flag.load(Ordering::Relaxed));

        // User says "always" on a bash command
        bash_flag.store(true, Ordering::Relaxed);

        // File tool should now see the flag as true
        assert!(
            file_flag.load(Ordering::Relaxed),
            "File tool should see always_approved after bash 'always'"
        );
    }

    // === describe_file_operation: rename_symbol ===

    #[test]
    fn test_describe_rename_symbol_operation() {
        let params = serde_json::json!({
            "old_name": "FooBar",
            "new_name": "BazQux",
            "path": "src/"
        });
        let desc = describe_file_operation("rename_symbol", &params);
        assert!(desc.contains("FooBar"), "Should contain old_name: {desc}");
        assert!(desc.contains("BazQux"), "Should contain new_name: {desc}");
        assert!(desc.contains("src/"), "Should contain scope: {desc}");
    }

    #[test]
    fn test_describe_rename_symbol_no_path() {
        let params = serde_json::json!({
            "old_name": "Foo",
            "new_name": "Bar"
        });
        let desc = describe_file_operation("rename_symbol", &params);
        assert!(
            desc.contains("project"),
            "Should default to 'project': {desc}"
        );
    }

    // === truncate_result tests ===

    #[test]
    fn test_truncate_result_with_custom_limit() {
        use yoagent::types::{Content, ToolResult};
        // Create a ToolResult with text longer than 100 chars and enough lines.
        // Each line starts with a unique first word to avoid compression collapsing.
        let long_text = (0..200)
            .map(|i| format!("T{i} data"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = ToolResult {
            content: vec![Content::Text {
                text: long_text.clone(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 100);
        let text = match &truncated.content[0] {
            Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(
            text.contains("[... truncated"),
            "Result should be truncated with 100-char limit"
        );
    }

    #[test]
    fn test_truncate_result_preserves_under_limit() {
        use crate::format::TOOL_OUTPUT_MAX_CHARS;
        use yoagent::types::{Content, ToolResult};
        let short_text = "hello world".to_string();
        let result = ToolResult {
            content: vec![Content::Text {
                text: short_text.clone(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, TOOL_OUTPUT_MAX_CHARS);
        let text = match &truncated.content[0] {
            Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert_eq!(text, short_text, "Short text should be unchanged");
    }

    // === AutoCheckTool tests ===

    /// A simple mock tool that always succeeds with the given text.
    struct MockTool {
        tool_name: &'static str,
        result_text: String,
    }

    #[async_trait::async_trait]
    impl AgentTool for MockTool {
        fn name(&self) -> &str {
            self.tool_name
        }
        fn label(&self) -> &str {
            self.tool_name
        }
        fn description(&self) -> &str {
            "mock tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            Ok(yoagent::types::ToolResult {
                content: vec![yoagent::Content::Text {
                    text: self.result_text.clone(),
                }],
                details: serde_json::Value::Null,
            })
        }
    }

    fn test_tool_context() -> yoagent::types::ToolContext {
        yoagent::types::ToolContext {
            tool_call_id: "test".to_string(),
            tool_name: "test".to_string(),
            cancel: tokio_util::sync::CancellationToken::new(),
            on_update: None,
            on_progress: None,
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_auto_check_passthrough_no_watch_command() {
        // Clear any watch commands to ensure passthrough
        crate::watch::clear_watch_command();

        let tool = with_auto_check(Box::new(MockTool {
            tool_name: "write_file",
            result_text: "File written successfully.".to_string(),
        }));

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();

        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert_eq!(text, "File written successfully.");
        assert!(
            !text.contains("Auto-check"),
            "Should not contain check output when no watch command"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_auto_check_appends_failure_output() {
        // Set a watch command that always fails
        crate::watch::set_watch_command("echo 'error[E0433]: module not found' && exit 1");

        let tool = with_auto_check(Box::new(MockTool {
            tool_name: "edit_file",
            result_text: "Edit applied.".to_string(),
        }));

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();

        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };

        // Clean up
        crate::watch::clear_watch_command();

        assert!(
            text.starts_with("Edit applied."),
            "Should start with original result"
        );
        assert!(
            text.contains("⚠ Auto-check failed"),
            "Should contain check failure notice"
        );
        assert!(
            text.contains("error[E0433]"),
            "Should contain the actual error output"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_auto_check_silent_on_success() {
        // Set a watch command that succeeds
        crate::watch::set_watch_command("true");

        let tool = with_auto_check(Box::new(MockTool {
            tool_name: "write_file",
            result_text: "File written successfully.".to_string(),
        }));

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();

        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };

        // Clean up
        crate::watch::clear_watch_command();

        assert_eq!(
            text, "File written successfully.",
            "Should pass through unchanged when check passes"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_auto_check_truncates_long_output() {
        // Set a watch command that produces output longer than AUTO_CHECK_MAX_CHARS
        // Generate ~3000 chars of output
        let long_cmd = "python3 -c \"print('x' * 3000)\" && exit 1";
        crate::watch::set_watch_command(long_cmd);

        let tool = with_auto_check(Box::new(MockTool {
            tool_name: "write_file",
            result_text: "OK".to_string(),
        }));

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();

        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };

        // Clean up
        crate::watch::clear_watch_command();

        assert!(
            text.contains("auto-check output truncated"),
            "Long output should be truncated: {text}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_auto_check_uses_first_phase_only() {
        // Set multi-phase watch commands — only first phase should run
        crate::watch::set_watch_commands(&[
            "echo 'lint phase' && exit 1",
            "echo 'test phase' && exit 1",
        ]);

        let tool = with_auto_check(Box::new(MockTool {
            tool_name: "write_file",
            result_text: "OK".to_string(),
        }));

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();

        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };

        // Clean up
        crate::watch::clear_watch_command();

        assert!(
            text.contains("lint phase"),
            "Should run first phase: {text}"
        );
        assert!(
            !text.contains("test phase"),
            "Should NOT run second phase: {text}"
        );
    }

    // === RecoveryHintTool tests ===

    /// A mock tool that can be configured to succeed or fail.
    struct ConfigurableMockTool {
        tool_name: &'static str,
        /// When `Some(msg)`, execute returns `ToolError::Failed(msg)`.
        /// When `None`, execute succeeds with "ok".
        fail_msg: Option<String>,
    }

    #[async_trait::async_trait]
    impl AgentTool for ConfigurableMockTool {
        fn name(&self) -> &str {
            self.tool_name
        }
        fn label(&self) -> &str {
            self.tool_name
        }
        fn description(&self) -> &str {
            "configurable mock"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            if let Some(ref msg) = self.fail_msg {
                Err(yoagent::types::ToolError::Failed(msg.clone()))
            } else {
                Ok(yoagent::types::ToolResult {
                    content: vec![yoagent::Content::Text {
                        text: "ok".to_string(),
                    }],
                    details: serde_json::Value::Null,
                })
            }
        }
    }

    #[tokio::test]
    async fn test_recovery_hint_tool_success_resets_counter() {
        let tracker = ToolFailureTracker::new();

        // Manually seed a failure count
        assert_eq!(tracker.record_failure("bash"), 1);
        assert_eq!(tracker.record_failure("bash"), 2);
        assert_eq!(tracker.get("bash"), 2);

        // Wrap a succeeding tool
        let tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "bash",
                fail_msg: None,
            }),
            &tracker,
        );

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await;
        assert!(result.is_ok(), "Should succeed");

        // Counter should be reset after success
        assert_eq!(tracker.get("bash"), 0);
    }

    #[tokio::test]
    async fn test_recovery_hint_tool_appends_hint_on_failure() {
        let tracker = ToolFailureTracker::new();

        let tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "edit_file",
                fail_msg: Some("old_text not found".to_string()),
            }),
            &tracker,
        );

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await;
        assert!(result.is_err(), "Should fail");

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("old_text not found"),
            "Should contain original error: {err_msg}"
        );
        assert!(
            err_msg.contains("💡 Recovery hint:"),
            "Should contain recovery hint marker: {err_msg}"
        );
        // Attempt 1 for edit_file should suggest using read_file first
        assert!(
            err_msg.contains("read_file"),
            "Attempt 1 hint for edit_file should mention read_file: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_recovery_hint_tool_escalates_on_repeated_failure() {
        let tracker = ToolFailureTracker::new();

        // First failure
        let tool1 = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "edit_file",
                fail_msg: Some("mismatch".to_string()),
            }),
            &tracker,
        );

        let err1 = tool1
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap_err()
            .to_string();

        // Second failure — should escalate (attempt >= 2 suggests write_file)
        let tool2 = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "edit_file",
                fail_msg: Some("mismatch again".to_string()),
            }),
            &tracker,
        );

        let err2 = tool2
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap_err()
            .to_string();

        // Attempt 1 mentions read_file (diagnostic hint)
        assert!(
            err1.contains("read_file"),
            "Attempt 1 should suggest read_file: {err1}"
        );
        // Attempt 2 should mention write_file (escalated alternative)
        assert!(
            err2.contains("write_file"),
            "Attempt 2 should escalate to suggesting write_file: {err2}"
        );
        // The two hints should be different
        assert_ne!(err1, err2, "Hints should escalate between attempts");
    }

    #[tokio::test]
    async fn test_tool_failure_tracker_independent_per_tool() {
        let tracker = ToolFailureTracker::new();

        // Fail bash twice
        let bash_tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "bash",
                fail_msg: Some("command not found".to_string()),
            }),
            &tracker,
        );
        let _ = bash_tool
            .execute(serde_json::json!({}), test_tool_context())
            .await;
        let _ = bash_tool
            .execute(serde_json::json!({}), test_tool_context())
            .await;

        assert_eq!(tracker.get("bash"), 2, "bash should have 2 failures");

        // Fail edit_file once
        let edit_tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "edit_file",
                fail_msg: Some("not found".to_string()),
            }),
            &tracker,
        );
        let _ = edit_tool
            .execute(serde_json::json!({}), test_tool_context())
            .await;

        assert_eq!(
            tracker.get("edit_file"),
            1,
            "edit_file should have 1 failure"
        );
        assert_eq!(tracker.get("bash"), 2, "bash should still have 2 failures");

        // Succeed on bash — resets only bash
        let bash_ok = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "bash",
                fail_msg: None,
            }),
            &tracker,
        );
        let _ = bash_ok
            .execute(serde_json::json!({}), test_tool_context())
            .await;

        assert_eq!(tracker.get("bash"), 0, "bash should be reset after success");
        assert_eq!(
            tracker.get("edit_file"),
            1,
            "edit_file should be unaffected"
        );
    }

    // === ToolFailureTracker unit tests (pure logic, no async) ===

    #[test]
    fn test_tracker_new_is_empty() {
        let tracker = ToolFailureTracker::new();
        assert_eq!(tracker.get("bash"), 0);
        assert_eq!(tracker.get("edit_file"), 0);
        assert_eq!(tracker.get("nonexistent"), 0);
    }

    #[test]
    fn test_tracker_record_failure_increments() {
        let tracker = ToolFailureTracker::new();
        assert_eq!(tracker.record_failure("bash"), 1);
        assert_eq!(tracker.record_failure("bash"), 2);
        assert_eq!(tracker.record_failure("bash"), 3);
        assert_eq!(tracker.get("bash"), 3);
    }

    #[test]
    fn test_tracker_record_success_resets() {
        let tracker = ToolFailureTracker::new();
        tracker.record_failure("bash");
        tracker.record_failure("bash");
        tracker.record_failure("bash");
        assert_eq!(tracker.get("bash"), 3);

        tracker.record_success("bash");
        assert_eq!(tracker.get("bash"), 0);
    }

    #[test]
    fn test_tracker_independent_tools() {
        let tracker = ToolFailureTracker::new();
        tracker.record_failure("bash");
        tracker.record_failure("bash");
        tracker.record_failure("edit_file");

        assert_eq!(tracker.get("bash"), 2);
        assert_eq!(tracker.get("edit_file"), 1);

        // Resetting one doesn't affect the other
        tracker.record_success("bash");
        assert_eq!(tracker.get("bash"), 0);
        assert_eq!(tracker.get("edit_file"), 1);
    }

    #[test]
    fn test_tracker_clone_shares_state() {
        let tracker = ToolFailureTracker::new();
        let cloned = tracker.clone();

        tracker.record_failure("bash");
        assert_eq!(cloned.get("bash"), 1, "Clone should share the same state");

        cloned.record_failure("bash");
        assert_eq!(
            tracker.get("bash"),
            2,
            "Original should see clone's mutation"
        );
    }

    // === truncate_result tests ===

    #[test]
    fn test_truncate_result_short_text_unchanged() {
        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text {
                text: "short output".to_string(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 1000);
        match &truncated.content[0] {
            yoagent::Content::Text { text } => {
                assert_eq!(text, "short output");
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_truncate_result_long_text_truncated() {
        // Generate 200 distinct lines that compression won't collapse.
        // Each line is unique enough to avoid the "similar line" collapsing.
        let lines: Vec<String> = (0..200)
            .map(|i| format!("unique_{i:04}_data: val={} extra={}", i * 7, i * 13))
            .collect();
        let long_text = lines.join("\n");
        let original_len = long_text.len();

        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text { text: long_text }],
            details: serde_json::Value::Null,
        };
        // Use max_chars smaller than text to force truncation
        let truncated = truncate_result(result, 2000);
        match &truncated.content[0] {
            yoagent::Content::Text { text } => {
                assert!(
                    text.len() < original_len,
                    "Truncated text ({}) should be shorter than original ({})",
                    text.len(),
                    original_len
                );
                assert!(
                    text.contains("truncated"),
                    "Should contain truncation marker: {}",
                    &text[..text.len().min(200)]
                );
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_truncate_result_non_text_content_unchanged() {
        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Image {
                data: "base64data".to_string(),
                mime_type: "image/png".to_string(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 10); // Very small limit
        match &truncated.content[0] {
            yoagent::Content::Image { data, mime_type } => {
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            }
            _ => panic!("Expected Image content"),
        }
    }

    #[test]
    fn test_truncate_result_empty_content() {
        let result = yoagent::types::ToolResult {
            content: vec![],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 100);
        assert!(truncated.content.is_empty());
    }

    // === describe_file_operation edge cases ===

    #[test]
    fn test_describe_read_file_operation() {
        let params = serde_json::json!({
            "path": "src/main.rs"
        });
        let desc = describe_file_operation("read_file", &params);
        assert!(
            desc.contains("read_file"),
            "Should contain tool name: {desc}"
        );
    }

    #[test]
    fn test_describe_bash_operation() {
        let params = serde_json::json!({
            "command": "cargo test"
        });
        let desc = describe_file_operation("bash", &params);
        assert!(desc.contains("bash"), "Should contain tool name: {desc}");
    }

    // =========================================================================
    // TruncatingTool / truncate_result — additional coverage
    // =========================================================================

    #[test]
    fn test_truncate_result_exact_limit_unchanged() {
        // Text exactly at limit should pass through unchanged
        let text = "abcdefghij"; // 10 chars
        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text {
                text: text.to_string(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 10);
        match &truncated.content[0] {
            yoagent::Content::Text { text: t } => {
                assert_eq!(t, text, "Text at exact limit should pass through unchanged");
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_truncate_result_multibyte_utf8_no_panic() {
        // Multi-byte UTF-8 characters near the boundary must not panic.
        // ✓ is 3 bytes, 日本語 is 3 bytes each char.
        let text = "✓日本語✓日本語✓日本語✓日本語✓日本語".repeat(50);
        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text { text }],
            details: serde_json::Value::Null,
        };
        // This should not panic even with a limit that falls mid-character
        let truncated = truncate_result(result, 100);
        match &truncated.content[0] {
            yoagent::Content::Text { text: t } => {
                // Should be valid UTF-8 (Rust strings guarantee this)
                assert!(!t.is_empty(), "Should produce some output");
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_truncate_result_emoji_boundary() {
        // Emoji are 4 bytes each. Truncation must respect char boundaries.
        let text = "🦑🐙🐠🐟🦈🐳🐋🦭🐡".repeat(30);
        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text { text }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 50);
        match &truncated.content[0] {
            yoagent::Content::Text { text: t } => {
                assert!(t.is_char_boundary(t.len()), "Output must be valid UTF-8");
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_truncate_result_empty_text() {
        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text {
                text: String::new(),
            }],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 100);
        match &truncated.content[0] {
            yoagent::Content::Text { text } => {
                assert_eq!(text, "", "Empty text should remain empty");
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_truncate_result_multiple_content_blocks() {
        // Multiple text blocks should each be independently truncated
        let short = "short".to_string();
        let long: String = (0..200)
            .map(|i| format!("line_{i:04}_unique_content"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = yoagent::types::ToolResult {
            content: vec![
                yoagent::Content::Text {
                    text: short.clone(),
                },
                yoagent::Content::Text { text: long },
            ],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 500);
        // First block should be unchanged
        match &truncated.content[0] {
            yoagent::Content::Text { text } => {
                assert_eq!(text, &short, "Short block should be unchanged");
            }
            _ => panic!("Expected Text content"),
        }
        // Second block should be truncated
        match &truncated.content[1] {
            yoagent::Content::Text { text } => {
                assert!(
                    text.contains("truncated") || text.len() < 5000,
                    "Long block should be truncated or compressed"
                );
            }
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_truncate_result_preserves_details() {
        let details = serde_json::json!({"key": "value", "count": 42});
        let result = yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text {
                text: "hello".to_string(),
            }],
            details: details.clone(),
        };
        let truncated = truncate_result(result, 1000);
        assert_eq!(
            truncated.details, details,
            "Details field should be preserved through truncation"
        );
    }

    // =========================================================================
    // with_truncation — wrapping preserves identity
    // =========================================================================

    #[tokio::test]
    async fn test_with_truncation_preserves_name_description() {
        let tool = with_truncation(
            Box::new(MockTool {
                tool_name: "my_tool",
                result_text: "result".to_string(),
            }),
            1000,
        );
        assert_eq!(tool.name(), "my_tool", "Wrapped tool should preserve name");
        assert_eq!(
            tool.description(),
            "mock tool",
            "Wrapped tool should preserve description"
        );
        assert_eq!(
            tool.label(),
            "my_tool",
            "Wrapped tool should preserve label"
        );
    }

    #[tokio::test]
    async fn test_with_truncation_truncates_large_output() {
        let long_text = (0..500)
            .map(|i| format!("uniq_{i:05}_row"))
            .collect::<Vec<_>>()
            .join("\n");
        let tool = with_truncation(
            Box::new(MockTool {
                tool_name: "bash",
                result_text: long_text,
            }),
            200,
        );
        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();
        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert!(
            text.contains("truncated"),
            "Output exceeding limit should be truncated: {}...",
            &text[..text.len().min(100)]
        );
    }

    #[tokio::test]
    async fn test_with_truncation_passes_small_output() {
        let tool = with_truncation(
            Box::new(MockTool {
                tool_name: "bash",
                result_text: "small output".to_string(),
            }),
            10000,
        );
        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();
        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert_eq!(
            text, "small output",
            "Small output should pass through unchanged"
        );
    }

    // =========================================================================
    // AutoCheckTool — wrapping preserves identity
    // =========================================================================

    #[test]
    fn test_with_auto_check_preserves_name_description() {
        let tool = with_auto_check(Box::new(MockTool {
            tool_name: "write_file",
            result_text: "ok".to_string(),
        }));
        assert_eq!(tool.name(), "write_file");
        assert_eq!(tool.description(), "mock tool");
        assert_eq!(tool.label(), "write_file");
    }

    #[test]
    fn test_with_auto_check_preserves_schema() {
        let tool = with_auto_check(Box::new(MockTool {
            tool_name: "edit_file",
            result_text: "ok".to_string(),
        }));
        let schema = tool.parameters_schema();
        assert_eq!(
            schema,
            serde_json::json!({}),
            "Schema should pass through from inner tool"
        );
    }

    // =========================================================================
    // RecoveryHintTool — additional scenarios
    // =========================================================================

    #[test]
    fn test_with_recovery_hints_preserves_name_description() {
        let tracker = ToolFailureTracker::new();
        let tool = with_recovery_hints(
            Box::new(MockTool {
                tool_name: "search",
                result_text: "ok".to_string(),
            }),
            &tracker,
        );
        assert_eq!(tool.name(), "search");
        assert_eq!(tool.description(), "mock tool");
        assert_eq!(tool.label(), "search");
    }

    #[tokio::test]
    async fn test_recovery_hint_non_failed_error_still_tracks() {
        // Non-Failed errors (e.g., NotFound) should still increment the counter
        // but pass through without recovery hint decoration
        struct NotFoundTool;

        #[async_trait::async_trait]
        impl AgentTool for NotFoundTool {
            fn name(&self) -> &str {
                "test_tool"
            }
            fn label(&self) -> &str {
                "test_tool"
            }
            fn description(&self) -> &str {
                "test"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(
                &self,
                _params: serde_json::Value,
                _ctx: yoagent::types::ToolContext,
            ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
                Err(yoagent::types::ToolError::NotFound("missing".to_string()))
            }
        }

        let tracker = ToolFailureTracker::new();
        let tool = with_recovery_hints(Box::new(NotFoundTool), &tracker);

        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await;
        assert!(result.is_err());

        // Counter should still increment even for NotFound errors
        assert_eq!(
            tracker.get("test_tool"),
            1,
            "NotFound errors should still be tracked"
        );
    }

    #[tokio::test]
    async fn test_recovery_hint_success_after_failures_resets() {
        let tracker = ToolFailureTracker::new();

        // Fail three times
        for _ in 0..3 {
            let tool = with_recovery_hints(
                Box::new(ConfigurableMockTool {
                    tool_name: "bash",
                    fail_msg: Some("error".to_string()),
                }),
                &tracker,
            );
            let _ = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await;
        }
        assert_eq!(tracker.get("bash"), 3);

        // Succeed once — should reset to 0
        let tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "bash",
                fail_msg: None,
            }),
            &tracker,
        );
        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await;
        assert!(result.is_ok());
        assert_eq!(
            tracker.get("bash"),
            0,
            "Success should reset counter from any value"
        );
    }

    #[tokio::test]
    async fn test_recovery_hint_different_tools_different_hints() {
        // Different tool names should produce different recovery hints
        let tracker = ToolFailureTracker::new();

        let bash_tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "bash",
                fail_msg: Some("command not found".to_string()),
            }),
            &tracker,
        );
        let bash_err = bash_tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap_err()
            .to_string();

        let search_tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "search",
                fail_msg: Some("pattern error".to_string()),
            }),
            &tracker,
        );
        let search_err = search_tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap_err()
            .to_string();

        // Both should have hints
        assert!(bash_err.contains("💡 Recovery hint:"));
        assert!(search_err.contains("💡 Recovery hint:"));

        // The hints should be different since the tools are different
        let bash_hint = bash_err.split("💡 Recovery hint:").nth(1).unwrap();
        let search_hint = search_err.split("💡 Recovery hint:").nth(1).unwrap();
        assert_ne!(
            bash_hint, search_hint,
            "Different tools should get different recovery hints"
        );
    }

    #[tokio::test]
    async fn test_recovery_hint_unknown_tool_gets_generic_hint() {
        let tracker = ToolFailureTracker::new();
        let tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "some_random_tool",
                fail_msg: Some("broken".to_string()),
            }),
            &tracker,
        );
        let err = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("💡 Recovery hint:"),
            "Unknown tools should still get a hint: {err}"
        );
        assert!(
            err.contains("different approach"),
            "Unknown tool hint should suggest a different approach: {err}"
        );
    }

    // =========================================================================
    // ToolFailureTracker — additional unit tests
    // =========================================================================

    #[test]
    fn test_tracker_record_success_on_nonexistent_tool_is_noop() {
        let tracker = ToolFailureTracker::new();
        // Recording success for a tool that was never recorded should not panic
        tracker.record_success("never_used");
        assert_eq!(tracker.get("never_used"), 0);
    }

    #[test]
    fn test_tracker_many_tools() {
        let tracker = ToolFailureTracker::new();
        let tool_names = [
            "bash",
            "read_file",
            "write_file",
            "edit_file",
            "search",
            "list_files",
            "rename_symbol",
        ];
        for (i, name) in tool_names.iter().enumerate() {
            for _ in 0..=i {
                tracker.record_failure(name);
            }
        }
        for (i, name) in tool_names.iter().enumerate() {
            assert_eq!(
                tracker.get(name),
                (i + 1) as u32,
                "{name} should have {} failures",
                i + 1
            );
        }
    }

    #[test]
    fn test_tracker_thread_safety() {
        // ToolFailureTracker uses Arc<Mutex<...>>, so it should be safely
        // shareable across threads.
        let tracker = ToolFailureTracker::new();
        let tracker_clone = tracker.clone();

        let handle = std::thread::spawn(move || {
            for _ in 0..100 {
                tracker_clone.record_failure("bash");
            }
        });

        for _ in 0..100 {
            tracker.record_failure("bash");
        }

        handle.join().unwrap();
        assert_eq!(
            tracker.get("bash"),
            200,
            "Concurrent failures should all be recorded"
        );
    }

    // =========================================================================
    // GuardedTool / maybe_guard — restriction logic
    // =========================================================================

    #[test]
    fn test_maybe_guard_empty_restrictions_no_wrap() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec![],
        };
        let tool: Box<dyn AgentTool> = Box::new(MockTool {
            tool_name: "read_file",
            result_text: "ok".to_string(),
        });
        let wrapped = maybe_guard(tool, &restrictions);
        // With empty restrictions, the tool should not be wrapped —
        // it should still have the same name and behavior.
        assert_eq!(wrapped.name(), "read_file");
    }

    #[test]
    fn test_maybe_guard_with_deny_wraps_tool() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/etc".to_string()],
        };
        let tool: Box<dyn AgentTool> = Box::new(MockTool {
            tool_name: "write_file",
            result_text: "ok".to_string(),
        });
        let wrapped = maybe_guard(tool, &restrictions);
        // Should still preserve the name
        assert_eq!(wrapped.name(), "write_file");
        assert_eq!(wrapped.description(), "mock tool");
    }

    #[tokio::test]
    async fn test_guarded_tool_blocks_denied_path() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/tmp/secret".to_string()],
        };
        let tool = maybe_guard(
            Box::new(MockTool {
                tool_name: "read_file",
                result_text: "should not see this".to_string(),
            }),
            &restrictions,
        );
        let params = serde_json::json!({ "path": "/tmp/secret/data.txt" });
        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err(), "Should block access to denied path");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("denied") || err.contains("restricted"),
            "Error should mention access denial: {err}"
        );
    }

    #[tokio::test]
    async fn test_guarded_tool_allows_non_denied_path() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/tmp/secret".to_string()],
        };
        let tool = maybe_guard(
            Box::new(MockTool {
                tool_name: "read_file",
                result_text: "file contents".to_string(),
            }),
            &restrictions,
        );
        // A path that is NOT under the denied directory
        let params = serde_json::json!({ "path": "/tmp/public/data.txt" });
        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_ok(), "Should allow access to non-denied path");
    }

    #[tokio::test]
    async fn test_guarded_tool_no_path_param_passes_through() {
        // If the tool params don't include "path", the guard should not block
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/forbidden".to_string()],
        };
        let tool = maybe_guard(
            Box::new(MockTool {
                tool_name: "bash",
                result_text: "command output".to_string(),
            }),
            &restrictions,
        );
        let params = serde_json::json!({ "command": "echo hello" });
        let result = tool.execute(params, test_tool_context()).await;
        assert!(
            result.is_ok(),
            "Tool without path param should pass through guard"
        );
    }

    // =========================================================================
    // ArcGuardedTool / maybe_guard_arc — restriction logic
    // =========================================================================

    #[test]
    fn test_maybe_guard_arc_empty_restrictions_no_wrap() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec![],
        };
        let tool: Arc<dyn AgentTool> = Arc::new(MockTool {
            tool_name: "search",
            result_text: "ok".to_string(),
        });
        let wrapped = maybe_guard_arc(tool, &restrictions);
        assert_eq!(wrapped.name(), "search");
    }

    #[test]
    fn test_maybe_guard_arc_with_restrictions_wraps() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec!["src/".to_string()],
            deny: vec![],
        };
        let tool: Arc<dyn AgentTool> = Arc::new(MockTool {
            tool_name: "read_file",
            result_text: "ok".to_string(),
        });
        let wrapped = maybe_guard_arc(tool, &restrictions);
        assert_eq!(wrapped.name(), "read_file");
        assert_eq!(wrapped.description(), "mock tool");
    }

    #[tokio::test]
    async fn test_arc_guarded_tool_blocks_denied_path() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/root".to_string()],
        };
        let tool: Arc<dyn AgentTool> = Arc::new(MockTool {
            tool_name: "write_file",
            result_text: "should not see this".to_string(),
        });
        let wrapped = maybe_guard_arc(tool, &restrictions);
        let params = serde_json::json!({ "path": "/root/.bashrc" });
        let result = wrapped.execute(params, test_tool_context()).await;
        assert!(result.is_err(), "ArcGuardedTool should block denied path");
    }

    #[tokio::test]
    async fn test_arc_guarded_tool_allows_valid_path() {
        let restrictions = cli::DirectoryRestrictions {
            allow: vec![],
            deny: vec!["/root".to_string()],
        };
        let tool: Arc<dyn AgentTool> = Arc::new(MockTool {
            tool_name: "read_file",
            result_text: "contents".to_string(),
        });
        let wrapped = maybe_guard_arc(tool, &restrictions);
        let params = serde_json::json!({ "path": "/home/user/file.txt" });
        let result = wrapped.execute(params, test_tool_context()).await;
        assert!(
            result.is_ok(),
            "ArcGuardedTool should allow non-denied path"
        );
    }

    // === file_path_to_allow_pattern tests ===

    #[test]
    fn test_file_pattern_subdirectory() {
        assert_eq!(file_path_to_allow_pattern("src/main.rs"), "src/*");
        assert_eq!(
            file_path_to_allow_pattern("src/format/mod.rs"),
            "src/format/*"
        );
        assert_eq!(
            file_path_to_allow_pattern("tests/integration.rs"),
            "tests/*"
        );
    }

    #[test]
    fn test_file_pattern_root_files() {
        assert_eq!(file_path_to_allow_pattern("README.md"), "*.md");
        assert_eq!(file_path_to_allow_pattern("Cargo.toml"), "*.toml");
        assert_eq!(file_path_to_allow_pattern("build.rs"), "*.rs");
    }

    #[test]
    fn test_file_pattern_no_extension() {
        // Root file without extension — use exact name
        assert_eq!(file_path_to_allow_pattern("Makefile"), "Makefile");
        assert_eq!(file_path_to_allow_pattern("Dockerfile"), "Dockerfile");
    }

    #[test]
    fn test_file_pattern_leading_dot_slash() {
        // ./src/main.rs should be treated same as src/main.rs
        assert_eq!(file_path_to_allow_pattern("./src/main.rs"), "src/*");
        assert_eq!(file_path_to_allow_pattern("./README.md"), "*.md");
    }

    #[test]
    fn test_file_pattern_empty() {
        assert_eq!(file_path_to_allow_pattern(""), "*");
        assert_eq!(file_path_to_allow_pattern("  "), "*");
    }

    #[test]
    fn test_file_pattern_deeply_nested() {
        assert_eq!(
            file_path_to_allow_pattern("src/format/highlight.rs"),
            "src/format/*"
        );
        assert_eq!(file_path_to_allow_pattern("a/b/c/d/file.txt"), "a/b/c/d/*");
    }

    #[test]
    fn test_file_pattern_backslash_normalisation() {
        // Windows-style paths should be normalised
        assert_eq!(file_path_to_allow_pattern("src\\main.rs"), "src/*");
        assert_eq!(
            file_path_to_allow_pattern("src\\format\\mod.rs"),
            "src/format/*"
        );
    }

    // === already_offered_file_persistence dedup test ===
    //
    // Note: already_offered_file_persistence uses a global static, so we test
    // the dedup logic indirectly via the pattern — each test uses unique patterns
    // to avoid cross-test pollution.

    #[test]
    fn test_file_persistence_dedup() {
        // Use a unique pattern that won't collide with other tests
        let unique = "__test_dedup_unique_1__/*";
        // First call: returns false (not already offered → was freshly inserted)
        assert!(
            !already_offered_file_persistence(unique),
            "First call for a new pattern should return false (not a duplicate)"
        );
        // Second call: returns true (already offered)
        assert!(
            already_offered_file_persistence(unique),
            "Second call for same pattern should return true (duplicate)"
        );
    }

    // === SmartEditTool / find_nearest_match tests ===

    #[test]
    fn test_find_nearest_match_exact_line() {
        let file_content = "line one\nline two\nfn hello() {\n    world()\n}\nline six\n";
        let old_text = "fn hello() {\n    world()\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should find a match");
        let (line, is_ws, snippet) = result.unwrap();
        assert_eq!(line, 3, "Match should be at line 3");
        assert!(!is_ws, "Should not be whitespace-only diff");
        assert!(
            snippet.contains("fn hello()"),
            "Snippet should contain the match"
        );
    }

    #[test]
    fn test_find_nearest_match_whitespace_only_diff() {
        // File has 4-space indent, old_text has 2-space indent
        let file_content = "fn main() {\n    let x = 1;\n    let y = 2;\n}\n";
        let old_text = "fn main() {\n  let x = 1;\n  let y = 2;\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_some(), "Should find a match");
        let (line, is_ws, _snippet) = result.unwrap();
        assert_eq!(line, 1, "Match should be at line 1");
        assert!(is_ws, "Should detect whitespace-only diff");
    }

    #[test]
    fn test_find_nearest_match_no_match() {
        let file_content = "fn main() {\n    println!(\"hello\");\n}\n";
        let old_text = "fn totally_different() {\n    nothing();\n}";
        let result = find_nearest_match(file_content, old_text);
        assert!(result.is_none(), "Should not find a match");
    }

    #[test]
    fn test_find_nearest_match_empty_old_text() {
        let file_content = "fn main() {}\n";
        let result = find_nearest_match(file_content, "");
        assert!(result.is_none(), "Empty old_text should return None");
    }

    #[test]
    fn test_find_nearest_match_only_whitespace_lines() {
        let file_content = "fn main() {}\n";
        let result = find_nearest_match(file_content, "   \n   \n");
        assert!(
            result.is_none(),
            "All-whitespace old_text should return None"
        );
    }

    #[test]
    fn test_find_nearest_match_snippet_limited_to_5_lines() {
        let file_content = (1..=20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let old_text = "line 5";
        let result = find_nearest_match(&file_content, old_text);
        assert!(result.is_some());
        let (line, _, snippet) = result.unwrap();
        assert_eq!(line, 5);
        // Should show exactly 5 lines of context
        let snippet_lines: Vec<&str> = snippet.lines().collect();
        assert_eq!(
            snippet_lines.len(),
            5,
            "Snippet should be 5 lines: {:?}",
            snippet_lines
        );
    }

    /// A mock tool for SmartEditTool tests — returns a configurable error or success.
    struct SmartEditMockTool {
        fail_msg: Option<String>,
        result_text: Option<String>,
    }

    #[async_trait::async_trait]
    impl AgentTool for SmartEditMockTool {
        fn name(&self) -> &str {
            "edit_file"
        }
        fn label(&self) -> &str {
            "edit_file"
        }
        fn description(&self) -> &str {
            "mock edit_file"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            if let Some(ref msg) = self.fail_msg {
                Err(yoagent::types::ToolError::Failed(msg.clone()))
            } else {
                Ok(yoagent::types::ToolResult {
                    content: vec![yoagent::Content::Text {
                        text: self.result_text.clone().unwrap_or_else(|| "ok".into()),
                    }],
                    details: serde_json::Value::Null,
                })
            }
        }
    }

    #[tokio::test]
    async fn test_smart_edit_passes_through_success() {
        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: None,
            result_text: Some("edited successfully".into()),
        }));

        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "fn main()",
            "new_text": "fn main2()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_ok(), "Success should pass through");
    }

    #[tokio::test]
    async fn test_smart_edit_passes_through_non_not_found_error() {
        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("permission denied".into()),
            result_text: None,
        }));

        let params = serde_json::json!({
            "path": "src/main.rs",
            "old_text": "fn main()",
            "new_text": "fn main2()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert_eq!(
            err, "permission denied",
            "Non-'not found' errors pass through unchanged"
        );
    }

    #[tokio::test]
    async fn test_smart_edit_augments_not_found_with_line_number() {
        // Create a temp file with known content
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(
            &file_path,
            "line one\nline two\nfn hello() {\n    world()\n}\nline six\n",
        )
        .unwrap();

        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("old_text not found in file".into()),
            result_text: None,
        }));

        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn hello() {\n  world()\n}",
            "new_text": "fn goodbye()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("line 3"), "Should mention line number: {err}");
        assert!(
            err.contains("fn hello()"),
            "Should show snippet with actual content: {err}"
        );
        assert!(
            err.contains("📍 Nearest match"),
            "Should have nearest match marker: {err}"
        );
    }

    #[tokio::test]
    async fn test_smart_edit_detects_whitespace_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("ws.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
        )
        .unwrap();

        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("old_text not found in file".into()),
            result_text: None,
        }));

        // old_text with 2-space indent instead of 4-space
        let params = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "fn main() {\n  let x = 1;\n  let y = 2;\n}",
            "new_text": "fn main() {\n  let x = 42;\n}"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("indentation") || err.contains("whitespace"),
            "Should hint about whitespace difference: {err}"
        );
        assert!(err.contains("line 1"), "Should report line number: {err}");
    }

    #[tokio::test]
    async fn test_smart_edit_handles_missing_file_gracefully() {
        let tool = with_smart_edit(Box::new(SmartEditMockTool {
            fail_msg: Some("old_text not found in file".into()),
            result_text: None,
        }));

        let params = serde_json::json!({
            "path": "/nonexistent/file.rs",
            "old_text": "fn hello()",
            "new_text": "fn goodbye()"
        });

        let result = tool.execute(params, test_tool_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // Should gracefully fall back to original message without panic
        assert!(
            err.contains("old_text not found"),
            "Should contain original error: {err}"
        );
    }

    // === LiteDescriptionTool tests ===

    #[test]
    fn test_lite_description_bash_has_example() {
        let tool = with_lite_description(Box::new(MockTool {
            tool_name: "bash",
            result_text: "ok".to_string(),
        }));
        let desc = tool.description();
        assert!(
            desc.contains(r#"{"command": "ls -la src/"}"#),
            "bash description should include JSON example, got: {desc}"
        );
        assert!(
            desc.contains("Example:"),
            "Should have 'Example:' label, got: {desc}"
        );
        // Original description should still be present
        assert!(
            desc.contains("mock tool"),
            "Should preserve original description, got: {desc}"
        );
    }

    #[test]
    fn test_lite_description_unknown_tool_passthrough() {
        let tool = with_lite_description(Box::new(MockTool {
            tool_name: "unknown_tool_xyz",
            result_text: "ok".to_string(),
        }));
        // Unknown tools should pass through without modification
        assert_eq!(tool.description(), "mock tool");
        assert_eq!(tool.name(), "unknown_tool_xyz");
    }

    #[tokio::test]
    async fn test_lite_description_delegates_call() {
        let tool = with_lite_description(Box::new(MockTool {
            tool_name: "bash",
            result_text: "hello from bash".to_string(),
        }));
        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap();
        let text = match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("Expected text content"),
        };
        assert_eq!(text, "hello from bash");
    }

    #[test]
    fn test_lite_description_delegates_name() {
        let tool = with_lite_description(Box::new(MockTool {
            tool_name: "read_file",
            result_text: "content".to_string(),
        }));
        assert_eq!(tool.name(), "read_file");
    }

    #[test]
    fn test_lite_description_all_known_tools() {
        // Verify examples exist for all the essential lite tools
        for tool_name in &[
            "bash",
            "read_file",
            "write_file",
            "edit_file",
            "list_files",
            "search",
        ] {
            let tool = with_lite_description(Box::new(MockTool {
                tool_name,
                result_text: "ok".to_string(),
            }));
            assert!(
                tool.description().contains("Example:"),
                "{tool_name} should have an example in lite mode"
            );
        }
    }
}
