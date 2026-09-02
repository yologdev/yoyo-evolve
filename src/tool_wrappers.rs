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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use yoagent::types::AgentTool;

// ---------------------------------------------------------------------------
// Refusal stems — deterministic tool-refusal messages (#662)
// ---------------------------------------------------------------------------

/// Stems shared between the refusal messages built in this module and
/// `prompt_retry::is_deterministic_tool_error`. One definition, two readers:
/// each wrapper builds its error message FROM its stem and the auto-retry
/// predicate matches ON the stem, so a reword cannot desync them. These
/// refusals are deterministic — the same call refuses identically on every
/// retry — so auto-retry must stop instead of burning attempts on the same
/// answer.
/// `ReadModeGuardTool`: "<mode> is active — …" (read mode / plan mode).
pub const REFUSAL_STEM_MODE_ACTIVE: &str = " is active — ";
/// `SessionCapTool`: "<tool> session cap reached (<N> calls) …".
pub const REFUSAL_STEM_SESSION_CAP: &str = " session cap reached (";
/// `ConfirmTool` directory-restriction denial: "User denied <tool> on '<path>'".
pub const REFUSAL_STEM_PATH_DENIED: &str = "User denied ";

/// True when an error message is one of yoyo's deliberate refusals rather than
/// a tool failure. Substring match (not prefix) — wrappers may prepend context.
///
/// Thin alias over [`crate::prompt_retry::is_deterministic_tool_error`] so the
/// three stems above keep exactly one predicate reading them (#710).
fn is_deterministic_refusal(err: &str) -> bool {
    crate::prompt_retry::is_deterministic_tool_error(err)
}

// ---------------------------------------------------------------------------
// GuardedTool — directory restriction wrapper (Box-based)
// ---------------------------------------------------------------------------

/// Repair a leaked markdown auto-link in a tool call's `path` argument.
///
/// Open-weight models are post-trained to auto-link filenames and carry that prior
/// across the tool boundary, so `path` can arrive as `dir/[notes.md](http://notes.md)`
/// (#874). This runs **before** the directory fence reads `path`, because otherwise the
/// security check and the file tool both act on a string that is not the path the model
/// meant — a confidently wrong verdict, not merely a missing one.
///
/// Only the degenerate shape is touched (see `safety::unwrap_markdown_autolink_path`);
/// every other input is returned **byte-identical**, which is the whole regression
/// surface. Non-`path` arguments — `content`, `old_text`, `new_text` — are never
/// inspected: rewriting file *content* that happens to look like a link is the silent
/// corruption this repair exists to avoid.
fn repair_path_argument(mut params: serde_json::Value) -> serde_json::Value {
    if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
        if let std::borrow::Cow::Owned(fixed) = crate::safety::unwrap_markdown_autolink_path(path) {
            params["path"] = serde_json::Value::String(fixed);
        }
    }
    params
}

/// The path-argument gate: **always** repairs `path`, and fences it against the
/// configured `DirectoryRestrictions` when any are configured.
///
/// It intercepts the `"path"` parameter, runs `repair_path_argument` on it, and
/// then validates the repaired value. If the path is blocked the tool returns an
/// error without executing the inner tool. With an empty `restrictions` the fence
/// half is a pass-through (`check_path` short-circuits to `Ok`), so an
/// unrestricted wrapper changes nothing except that the repair runs — which is
/// exactly why `maybe_guard` wraps unconditionally.
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
        // Repair a leaked markdown auto-link BEFORE the fence reads the path (#874):
        // otherwise the restriction check, and then the file tool, both act on a
        // string that is not the path the model meant.
        let params = repair_path_argument(params);
        // Check the "path" parameter against directory restrictions
        if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            if let Err(reason) = self.restrictions.check_path(path) {
                return Err(yoagent::types::ToolError::Failed(reason));
            }
        }
        self.inner.execute(params, ctx).await
    }
}

/// Wrap a tool with the path-argument gate.
///
/// **This always wraps, including when no directory restrictions are configured**,
/// and that is the whole product-safety property rather than an inefficiency to
/// optimise away. The wrapper does two things and only one of them is conditional:
/// it *always* repairs the `path` argument (`repair_path_argument`), and it fences
/// the path only when restrictions exist. `DirectoryRestrictions::check_path`
/// already short-circuits to `Ok` on an empty config, so an unrestricted wrapper
/// is a byte-identical pass-through for the fence and costs one `Box` indirection.
///
/// **Superseded behaviour, recorded rather than erased:** this returned the bare
/// `tool` when `restrictions.is_empty()`. That was correct while the wrapper did
/// nothing but fence — and it silently made the auto-link repair unreachable for
/// **every default user**, since the file-tool default is unrestricted (this
/// repo's own `.yoyo.toml` configures no `[directories]` block). A repair wired
/// only onto a default-off wrapper is not a product fix; it is a fix for the
/// minority who opted into `--allow-dir`/`--deny-dir`.
pub(crate) fn maybe_guard(
    tool: Box<dyn AgentTool>,
    restrictions: &cli::DirectoryRestrictions,
) -> Box<dyn AgentTool> {
    Box::new(GuardedTool {
        inner: tool,
        restrictions: restrictions.clone(),
    })
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
        // Repair a leaked markdown auto-link BEFORE the fence reads the path (#874):
        // otherwise the restriction check, and then the file tool, both act on a
        // string that is not the path the model meant.
        let params = repair_path_argument(params);
        // Check the "path" parameter against directory restrictions
        if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            if let Err(reason) = self.restrictions.check_path(path) {
                return Err(yoagent::types::ToolError::Failed(reason));
            }
        }
        self.inner.execute(params, ctx).await
    }
}

/// Wrap an Arc-based tool with the path-argument gate. Used for sub-agent tools,
/// which require `Arc<dyn AgentTool>`.
///
/// **Always wraps**, for the same reason as `maybe_guard` above — see that doc
/// comment for the mechanism and for the superseded default-off behaviour. This
/// is the seam that carries the repair into the *sub-agent* tool chain, which
/// receives none of `with_truncation` / `with_recovery_hints` / `maybe_hook`.
pub(crate) fn maybe_guard_arc(
    tool: Arc<dyn AgentTool>,
    restrictions: &cli::DirectoryRestrictions,
) -> Arc<dyn AgentTool> {
    Arc::new(ArcGuardedTool {
        inner: tool,
        restrictions: restrictions.clone(),
    })
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

/// Result/error truncation against `--max-tool-output` lives in its own module
/// (`tool_truncate`); `TruncatingTool` below is the wiring that calls it.
pub(crate) use crate::tool_truncate::{truncate_result, truncate_tool_error};

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
        let result = self
            .inner
            .execute(params, ctx)
            .await
            .map_err(|e| truncate_tool_error(e, self.max_chars))?;
        // #665: only bash output can legitimately contain test-runner output;
        // read_file/search results that quote test-shaped lines are content.
        let allow_test_filter = matches!(self.inner.name(), "bash");
        Ok(truncate_result(result, self.max_chars, allow_test_filter))
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
                "{}{} on '{}'",
                REFUSAL_STEM_PATH_DENIED, tool_name, path
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
            let t = safe_truncate(&output, AUTO_CHECK_MAX_CHARS);
            format!("{t}...\n[auto-check output truncated at {AUTO_CHECK_MAX_CHARS} chars]",)
        } else {
            output
        };

        let check_notice = format!("\n\n⚠ Auto-check failed ({check_cmd}):\n{truncated_output}");

        // Append the check notice to the last text block only (not all blocks)
        // to avoid duplicating the same error output across multiple content blocks,
        // which wastes context window tokens and confuses the agent.
        let mut content: Vec<yoagent::Content> = result.content;
        let last_text_idx = content
            .iter()
            .rposition(|c| matches!(c, yoagent::Content::Text { .. }));
        if let Some(idx) = last_text_idx {
            if let yoagent::Content::Text { text } = &content[idx] {
                content[idx] = yoagent::Content::Text {
                    text: format!("{text}{check_notice}"),
                };
            }
        } else {
            // No text blocks — add a new one with the check notice
            content.push(yoagent::Content::Text { text: check_notice });
        }
        let new_content = content;

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
// RecoveryHintTool — appends recovery hints to tool error messages
// ---------------------------------------------------------------------------

/// Tracks consecutive failures per (tool_name, target_file) pair so recovery
/// hints can escalate per-file. When the same tool fails on the same file
/// repeatedly, the `RecoveryHintTool` wrapper uses the failure count to select
/// increasingly aggressive recovery suggestions and prepends a file-specific
/// hint. When a tool succeeds on a target, only that (tool, target) pair resets.
#[derive(Clone, Default)]
pub(crate) struct ToolFailureTracker {
    counts: Arc<Mutex<HashMap<(String, String), u32>>>,
}

impl ToolFailureTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Increment the failure count for a (tool, target) pair and return the new count.
    fn record_failure(&self, tool_name: &str, target: &str) -> u32 {
        let mut map = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        let key = (tool_name.to_string(), target.to_string());
        let count = map.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    /// Reset the failure count for a (tool, target) pair (called on success).
    fn record_success(&self, tool_name: &str, target: &str) {
        let mut map = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&(tool_name.to_string(), target.to_string()));
    }

    /// Get the current failure count for a (tool, target) pair (for testing).
    #[cfg(test)]
    fn get(&self, tool_name: &str, target: &str) -> u32 {
        let map = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&(tool_name.to_string(), target.to_string()))
            .copied()
            .unwrap_or(0)
    }
}

/// Extract the target identifier from tool input parameters.
/// For file-oriented tools, this extracts the file path.
/// For bash, it extracts the command (truncated to 60 chars).
/// For unknown tools, returns `"_"` as a global fallback.
fn extract_target(tool_name: &str, input: &serde_json::Value) -> String {
    match tool_name {
        "edit_file" | "read_file" | "write_file" | "list_files" => input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("_")
            .to_string(),
        "bash" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("_");
            if cmd.len() <= 60 {
                cmd.to_string()
            } else {
                // Safe truncation at char boundary
                let mut b = 60;
                while b > 0 && !cmd.is_char_boundary(b) {
                    b -= 1;
                }
                cmd[..b].to_string()
            }
        }
        "search" | "rename_symbol" => input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("_")
            .to_string(),
        _ => "_".to_string(),
    }
}

/// A wrapper tool that enriches error messages with recovery hints.
///
/// On success the failure counter resets. On failure the counter increments
/// and a tool-specific recovery hint (from `prompt_retry::tool_recovery_hint`)
/// is appended to the error message.
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
        let target = extract_target(&tool_name, &params);
        match self.inner.execute(params, ctx).await {
            Ok(result) => {
                self.tracker.record_success(&tool_name, &target);
                Ok(result)
            }
            Err(other) => {
                // A deterministic refusal (read/plan mode, session cap, denied
                // path) is the system working as designed, not a flaky tool:
                // hand it back verbatim — no failure-counter bump, and no hint
                // coaching the model around yoyo's own guard (#710).
                if is_deterministic_refusal(&other.to_string()) {
                    return Err(other);
                }
                // Every other error — Failed, NotFound, InvalidArgs, Cancelled —
                // gets the same escalating advice off the same counter.
                let attempt = self.tracker.record_failure(&tool_name, &target);
                let hint = crate::prompt_retry::tool_recovery_hint(&tool_name, attempt);
                let file_prefix = if attempt >= 2 && target != "_" {
                    format!(
                        "You've failed to {verb} '{target}' {attempt} times. ",
                        verb = tool_name.replace('_', " "),
                    )
                } else {
                    String::new()
                };
                let suffix = format!("\n\n💡 Recovery hint: {file_prefix}{hint}");
                match other {
                    yoagent::types::ToolError::Failed(msg) => {
                        Err(yoagent::types::ToolError::Failed(format!("{msg}{suffix}")))
                    }
                    yoagent::types::ToolError::NotFound(msg) => Err(
                        yoagent::types::ToolError::NotFound(format!("{msg}{suffix}")),
                    ),
                    yoagent::types::ToolError::InvalidArgs(msg) => Err(
                        yoagent::types::ToolError::InvalidArgs(format!("{msg}{suffix}")),
                    ),
                    // Cancelled carries no message — convert to Failed to attach the hint
                    yoagent::types::ToolError::Cancelled => Err(yoagent::types::ToolError::Failed(
                        format!("Tool call was cancelled.{suffix}"),
                    )),
                }
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
// SessionCapTool — session-wide call-count circuit breaker
// ---------------------------------------------------------------------------

/// Session-wide cap on calls to high-leverage tools (`web_search`, `sub_agent`).
/// Matches Claude Code's 200/session runaway-loop circuit breaker. Far above
/// any legitimate interactive session's usage, so normal users never hit it.
pub(crate) const SESSION_TOOL_CALL_CAP: usize = 200;

/// A wrapper tool that enforces a session-wide cap on the number of times the
/// wrapped tool may be called. Once the cap is exceeded, every subsequent call
/// returns an honest error explaining why the tool stopped working (never a
/// silent no-op, never a panic). The counter is per-wrapper-INSTANCE, i.e.
/// per agent build: `/clear` rebuilds the agent (see `CommandRoute::Clear` in
/// dispatch.rs), which constructs fresh wrappers with zeroed counters — so the
/// budget resets on `/clear`, matching what "session-wide" means to a user.
/// Do NOT convert this to a process-wide static; that would silently turn the
/// session cap into a process-lifetime cap (pinned by
/// `test_session_cap_fresh_wrapper_resets_budget`).
pub(crate) struct SessionCapTool {
    inner: Box<dyn AgentTool>,
    counter: Arc<AtomicUsize>,
    cap: usize,
}

#[async_trait::async_trait]
impl AgentTool for SessionCapTool {
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
        let used = self.counter.fetch_add(1, Ordering::SeqCst);
        if used >= self.cap {
            return Err(yoagent::types::ToolError::Failed(format!(
                "{}{}{} calls) — this usually means a runaway \
                 loop. Use /clear or start a new session to reset.",
                self.inner.name(),
                REFUSAL_STEM_SESSION_CAP,
                self.cap
            )));
        }
        self.inner.execute(params, ctx).await
    }
}

/// Wrap a tool with a session-wide call cap. Each wrapped tool gets its own
/// independent counter.
pub(crate) fn with_session_cap(tool: Box<dyn AgentTool>, cap: usize) -> Box<dyn AgentTool> {
    Box::new(SessionCapTool {
        inner: tool,
        counter: Arc::new(AtomicUsize::new(0)),
        cap,
    })
}

// ---------------------------------------------------------------------------
// ReadModeGuardTool — mechanical /read + /plan mode enforcement
// ---------------------------------------------------------------------------

/// How the read/plan-mode guard classifies its wrapped tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ReadGuardKind {
    /// Always refused while a blocking mode is on (write_file, edit_file,
    /// rename_symbol).
    Write,
    /// Refused while a blocking mode is on when the command matches either
    /// the destructive-pattern classification (`analyze_bash_command`) or
    /// the write-command detection (`detect_write_command`: `touch`, `mv`,
    /// `sed -i`, `tee`, `>`/`>>` redirection, ...) in `safety.rs`.
    /// Read-only commands pass through.
    Bash,
}

/// A wrapper that mechanically enforces `/read` mode and `/plan` mode at the
/// tool layer.
///
/// Both modes were previously prompt-only (`READ_MODE_PROMPT` /
/// `PLAN_MODE_PROMPT` injected into the conversation) — a request, not a
/// lock: a model that ignored the prompt could still write files. This
/// wrapper checks `crate::commands_config::is_read_mode()` and
/// `crate::commands_plan::is_plan_mode()` **at call time** (both toggle at
/// runtime; snapshotting at build time would desync the guard from the mode
/// commands) and returns an honest tool error instead of executing.
///
/// Plan-mode exception: while `/plan apply` is executing
/// (`crate::commands_plan::is_plan_apply_active()`), the plan-mode block is a
/// transparent pass-through — apply needs full tool access to execute the
/// plan. When neither mode is on (the default), the wrapper is a transparent
/// pass-through.
pub(crate) struct ReadModeGuardTool {
    /// `Arc` rather than `Box` so the same guard type serves both the main
    /// agent's `Box<dyn AgentTool>` tool list and the sub-agent tool list in
    /// `tools.rs`, which is `Arc`-shaped. One type, two entry points — a
    /// second copy of the classifier would be the duplication shape that
    /// survives longest.
    inner: Arc<dyn AgentTool>,
    kind: ReadGuardKind,
}

/// Which restrictive mode, if any, currently blocks write-class work.
///
/// Read mode wins when both are somehow on (it's the stricter promise).
/// Plan mode only counts while `/plan apply` is NOT executing — apply needs
/// write access to carry out the plan.
fn active_blocking_mode() -> Option<BlockingMode> {
    if crate::commands_config::is_read_mode() {
        return Some(BlockingMode::Read);
    }
    if crate::commands_plan::is_plan_mode() && !crate::commands_plan::is_plan_apply_active() {
        return Some(BlockingMode::Plan);
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BlockingMode {
    Read,
    Plan,
}

impl BlockingMode {
    /// "<mode> is active" prefix for refusal messages.
    fn label(self) -> &'static str {
        match self {
            BlockingMode::Read => "read mode",
            BlockingMode::Plan => "plan mode",
        }
    }

    /// How the user exits the mode (named in every refusal — no silent no-ops).
    fn exit_hint(self) -> &'static str {
        match self {
            BlockingMode::Read => "Use /read off to enable writes.",
            BlockingMode::Plan => "Use /plan apply to execute the plan or /plan off to exit.",
        }
    }
}

#[async_trait::async_trait]
impl AgentTool for ReadModeGuardTool {
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
        // Check at call time — both modes are toggled at runtime.
        if let Some(mode) = active_blocking_mode() {
            match self.kind {
                ReadGuardKind::Write => {
                    return Err(yoagent::types::ToolError::Failed(format!(
                        "{}{}{} is a write tool and was blocked. {}",
                        mode.label(),
                        REFUSAL_STEM_MODE_ACTIVE,
                        self.inner.name(),
                        mode.exit_hint()
                    )));
                }
                ReadGuardKind::Bash => {
                    if let Some(cmd) = params.get("command").and_then(|v| v.as_str()) {
                        if let Some(reason) = crate::safety::analyze_bash_command(cmd) {
                            return Err(yoagent::types::ToolError::Failed(format!(
                                "{} is active — this command was blocked as \
                                 destructive ({reason}). {}",
                                mode.label(),
                                mode.exit_hint()
                            )));
                        }
                        if let Some(what) = crate::safety::detect_write_command(cmd) {
                            return Err(yoagent::types::ToolError::Failed(format!(
                                "{} is active — this bash command was blocked \
                                 because it writes: {what}. Read-only commands \
                                 are allowed. {}",
                                mode.label(),
                                mode.exit_hint()
                            )));
                        }
                    }
                }
            }
        }
        self.inner.execute(params, ctx).await
    }
}

/// Wrap a write-class tool (write_file, edit_file, rename_symbol) with
/// read/plan-mode enforcement: refused entirely while `/read` is on, or while
/// `/plan` is on and `/plan apply` is not executing.
pub(crate) fn with_read_guard(tool: Box<dyn AgentTool>) -> Box<dyn AgentTool> {
    Box::new(ReadModeGuardTool {
        inner: Arc::from(tool),
        kind: ReadGuardKind::Write,
    })
}

/// `Arc` flavour of [`with_read_guard`], for the sub-agent tool list in
/// `tools.rs` (which is `Arc<dyn AgentTool>`-shaped). Same type, same
/// call-time mode check, same refusal text — only the smart pointer differs.
pub(crate) fn with_read_guard_arc(tool: Arc<dyn AgentTool>) -> Arc<dyn AgentTool> {
    Arc::new(ReadModeGuardTool {
        inner: tool,
        kind: ReadGuardKind::Write,
    })
}

/// Wrap the bash tool with read/plan-mode enforcement: while a blocking mode
/// is on, commands flagged by the destructive-pattern classifier OR the
/// write-command detector (`safety::detect_write_command`) are refused;
/// read-only commands pass through.
pub(crate) fn with_read_guard_bash(tool: Box<dyn AgentTool>) -> Box<dyn AgentTool> {
    Box::new(ReadModeGuardTool {
        inner: Arc::from(tool),
        kind: ReadGuardKind::Bash,
    })
}

/// `Arc` flavour of [`with_read_guard_bash`], for the sub-agent tool list in
/// `tools.rs`. Same classifier, same pass-through for read-only commands.
pub(crate) fn with_read_guard_bash_arc(tool: Arc<dyn AgentTool>) -> Arc<dyn AgentTool> {
    Arc::new(ReadModeGuardTool {
        inner: tool,
        kind: ReadGuardKind::Bash,
    })
}

// ---------------------------------------------------------------------------
// FallbackSubAgentTool — one retry on the fallback model when the primary
// model is unavailable
// ---------------------------------------------------------------------------

/// True when a tool-error message names a **model-availability** failure — the
/// model does not exist, is not enabled for this key, or is otherwise not
/// servable — as opposed to any other reason a sub-agent might fail.
///
/// This is deliberately the narrowest of the error classifiers in this repo,
/// because the only action it licenses is *re-running the whole subtask on a
/// different model*. Every shape it declines is a shape where a second model
/// would fail identically, waste a second full sub-agent run, or — in the
/// refusal case — quietly defeat a guard.
///
/// **Why this is not `prompt_retry::is_retriable_error`** (checked, not
/// assumed): that predicate answers a different question — *may I retry this
/// on the SAME model after a backoff?* — and correctly answers "no" for a 404.
/// "No, not on this model" and "yes, on a different model" are opposite
/// verdicts about the same string, so reusing it would invert the decision.
/// The digit-boundary rule it needs *is* shared, though:
/// `prompt_retry::contains_status_code` is the one statement of that rule
/// (Day-174 lesson — a bare `.contains("404")` collides with any other number
/// in an error string, and `"404"` inside `"tokens: 14045"` is exactly the
/// shape that reaches an error message).
///
/// Fires on: HTTP 404 as a standalone code, plus the prose forms providers
/// actually emit — `model not found`, `does not exist`, `unknown model`,
/// `invalid model`, `unsupported model`, `model_not_found`.
///
/// Deliberately does **not** fire on:
/// - a deterministic refusal (read/plan mode, session cap, denied path) — the
///   caller short-circuits those before asking, and see `FallbackSubAgentTool`
///   for why retrying one would be a guard bypass rather than a fallback;
/// - rate limits / overload — `prompt_retry_limits.rs` owns that policy and
///   has a real reset-time rule; a second one here would fight it;
/// - auth / permission errors — a different model on the same broken key
///   fails identically, so the retry is pure waste;
/// - ordinary failures inside the sub-agent — the sub-agent ran fine and the
///   *work* failed, which a different model does not fix.
pub(crate) fn is_model_unavailable_error(err: &str) -> bool {
    matches!(
        classify_sub_agent_error(err),
        SubAgentErrorClass::ModelUnavailable
    )
}

// --- the shape lists, stated exactly once -----------------------------------
//
// These were inline arrays inside `is_model_unavailable_error` until Day 180.
// They are consts now because a *second* reader arrived — the failure-report
// composer below, which must tell the parent agent what class of failure it is
// looking at. Two copies of "what counts as an auth error" would let the
// fallback predicate and the diagnostic disagree about the same string: the
// retry would decline while the report said "model unavailable", or worse the
// reverse. One list, two readers, and the classifier is the only place that
// reads them.

/// Auth / permission shapes. A different model on the same credential fails
/// identically, so these never license a fallback.
pub(crate) const AUTH_ERROR_SHAPES: &[&str] = &[
    "authentication",
    "unauthorized",
    "invalid api key",
    "invalid_api_key",
    "permission denied",
    "forbidden",
];
pub(crate) const AUTH_STATUS_CODES: &[&str] = &["401", "403"];

/// Rate-limit / capacity shapes. `prompt_retry_limits.rs` owns that policy and
/// has a real reset-time rule; a second one here would fight it.
pub(crate) const RATE_LIMIT_ERROR_SHAPES: &[&str] = &[
    "rate limit",
    "rate_limit",
    "overloaded",
    "capacity",
    "quota",
];
pub(crate) const RATE_LIMIT_STATUS_CODES: &[&str] = &["429"];

/// Model-availability shapes — the prose forms providers actually emit.
const MODEL_UNAVAILABLE_SHAPES: &[&str] = &[
    "model not found",
    "model_not_found",
    "not found: model",
    "does not exist",
    "unknown model",
    "invalid model",
    "unsupported model",
    "model is not supported",
    "no such model",
];
/// A standalone 404 is a model-availability shape on a provider endpoint whose
/// only path parameter is the model.
const MODEL_UNAVAILABLE_STATUS_CODES: &[&str] = &["404"];

/// Every HTTP status code worth reporting back to the parent agent, in the
/// order they are probed. Read through `prompt_retry::contains_status_code`,
/// never a bare `.contains("404")` — the Day-174 lesson, where
/// `prompt is too long: 402134 tokens` was diagnosed as exhausted credits
/// because `402` sat inside a token count.
const REPORTABLE_STATUS_CODES: &[&str] = &[
    "400", "401", "402", "403", "404", "408", "409", "422", "429", "500", "502", "503", "504",
];

/// What kind of failure a sub-agent error names.
///
/// `Unclassified` is a **real value and is always reported**, not a silent
/// omission: "an ordinary failure of the delegated work" tells the parent that
/// a different model will not help, which is exactly as actionable as any
/// named class. Dressing it up as one of the named classes, or saying nothing,
/// is the confident-wrong-diagnosis defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubAgentErrorClass {
    ModelUnavailable,
    Auth,
    RateLimit,
    Unclassified,
}

impl SubAgentErrorClass {
    /// The one statement of each class's wording, so the report and any future
    /// reader cannot drift.
    fn describe(self) -> &'static str {
        match self {
            Self::ModelUnavailable => {
                "the model was unavailable (not found, or not enabled for this key)"
            }
            Self::Auth => {
                "an authentication or permission failure — a different model on the same \
                 credential fails identically"
            }
            Self::RateLimit => {
                "a rate limit or capacity failure — retrying immediately fails the same way"
            }
            Self::Unclassified => {
                "an ordinary failure of the delegated work — a different model will not help"
            }
        }
    }
}

/// Classify a sub-agent tool error.
///
/// **Precedence is auth → rate limit → model-availability, and that order is
/// the safety property rather than a style choice.** A message can carry both
/// an auth word and a model name; in that case the model is not the problem,
/// so the auth check wins. Losing a genuine fallback is cheap (today's
/// behaviour); firing on an auth error burns a second full sub-agent run
/// against the same broken credential.
///
/// [`is_model_unavailable_error`] is defined *in terms of* this function, so
/// the retry decision and the diagnostic can never disagree about a string.
pub(crate) fn classify_sub_agent_error(err: &str) -> SubAgentErrorClass {
    let lower = err.to_lowercase();
    let has_code = |codes: &[&str]| {
        codes
            .iter()
            .any(|c| crate::prompt_retry::contains_status_code(&lower, c))
    };

    if AUTH_ERROR_SHAPES.iter().any(|n| lower.contains(n)) || has_code(AUTH_STATUS_CODES) {
        return SubAgentErrorClass::Auth;
    }
    if RATE_LIMIT_ERROR_SHAPES.iter().any(|n| lower.contains(n))
        || has_code(RATE_LIMIT_STATUS_CODES)
    {
        return SubAgentErrorClass::RateLimit;
    }
    if has_code(MODEL_UNAVAILABLE_STATUS_CODES)
        || MODEL_UNAVAILABLE_SHAPES.iter().any(|s| lower.contains(s))
    {
        return SubAgentErrorClass::ModelUnavailable;
    }
    SubAgentErrorClass::Unclassified
}

/// The first HTTP status code actually present in `err`, or `None`.
///
/// `None` means **no status code was observed**, and the report then says
/// nothing about status rather than emitting `status: unknown` — "could not
/// check" must never render as "checked".
fn observed_status_code(err: &str) -> Option<&'static str> {
    let lower = err.to_lowercase();
    REPORTABLE_STATUS_CODES
        .iter()
        .copied()
        .find(|c| crate::prompt_retry::contains_status_code(&lower, c))
}

/// The note prefixed to a successful fallback result. Pure so the wording is
/// pinned by test: an invisible model switch is a bug even when it is the right
/// switch (the `⚡ auto-continuing` rule), so the parent is told which model
/// answered and which one it replaced.
pub(crate) fn fallback_switch_note(primary: &str, fallback: &str) -> String {
    format!(
        "[yoyo: the sub-agent's model `{primary}` was unavailable, so this subtask \
         ran on the fallback model `{fallback}` instead.]\n\n"
    )
}

/// The note prepended to a sub-agent result that yoagent cut short, or `None`
/// when the result finished normally.
///
/// **Where the marker comes from.** yoagent's own loop appends a stop marker to
/// the text it returns when a run ends on a *bound* rather than on the model
/// finishing: `extract_final_text` folds `stopped_notice` in as
/// `"{text}\n\n{stop}"`, where the marker is `format!("{AGENT_STOPPED_PREFIX}
/// {reason}]")` and the reason comes from `ExecutionTracker::check_limits()`
/// (e.g. `"Max turns reached (25/25)"`). Hitting `max_turns` is a bound, not a
/// failure, so the call returns **`Ok`** — which is why every `Err`-branching
/// decorator beside this one is structurally blind to it.
/// <!-- yoagent-version-claim: 0.18.1 -->
///
/// The prefix is used **by reference** rather than re-spelled, so an upstream
/// rename is a compile error here instead of a silently dead match.
///
/// `None` means the caller returns the result byte-identically — every
/// sub-agent that finished normally, and the whole regression surface.
pub(crate) fn sub_agent_partial_notice(text: &str) -> Option<String> {
    if !text.contains(yoagent::agent_loop::AGENT_STOPPED_PREFIX) {
        return None;
    }
    Some(
        "[yoyo: this sub-agent result is PARTIAL — the subtask was stopped by a run bound \
         (its turn/token/time limit), not by finishing. This is not a failure of the \
         delegated work and not a provider error; the sub-agent was still mid-task. Do not \
         treat the text below as a completed answer — re-dispatch with a narrower subtask \
         if you need the rest.]\n\n"
            .to_string(),
    )
}

/// Prepend the partial-result annotation to an `Ok` sub-agent result when
/// yoagent's stop marker is present.
///
/// Only the `Ok` path reaches here, and that is deliberate rather than an
/// oversight: `LOOP_ABORT_PREFIX` *starts with* the same `"[Agent stopped:"`
/// bytes, but yoagent's `extract_error` turns a loop abort into a
/// `ToolError::Failed`, so it never arrives as `Ok` and needs no second handler.
/// <!-- yoagent-version-claim: 0.18.1 -->
fn annotate_if_partial(result: yoagent::types::ToolResult) -> yoagent::types::ToolResult {
    let joined = result
        .content
        .iter()
        .filter_map(|block| match block {
            yoagent::types::Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Guard first, and it short-circuits — the same rule `RecoveryHintTool`
    // (#710) and both Day-180 decorators follow. A deliberate refusal dressed
    // in partial-result scaffolding reads as a malfunction.
    if is_deterministic_refusal(&joined) {
        return result;
    }

    let Some(note) = sub_agent_partial_notice(&joined) else {
        return result;
    };

    // Its own block at index 0, never a splice into an existing one:
    // `content` may carry non-text blocks and a `Vec` insert cannot mangle them.
    let mut content = result.content;
    content.insert(0, yoagent::types::Content::Text { text: note });
    yoagent::types::ToolResult {
        content,
        details: result.details,
    }
}

/// Wraps the `sub_agent` tool so a **first-call model-availability failure**
/// costs one retry on the session's fallback model instead of killing the
/// subtask outright.
///
/// Why a decorator rather than a fix inside the sub-agent: `SubAgentTool` is
/// yoagent's and yoyo does not drive its turn loop, so there is no seam to
/// intercept mid-loop. Decoration is the idiom this file already uses
/// (`GuardedTool`, `TruncatingTool`, `RecoveryHintTool`, `SessionCapTool`,
/// `ReadModeGuardTool`).
///
/// **Exactly one extra attempt, never a chain of N.** The secondary is built
/// once at construction time from the fallback model; if it also fails, its
/// error is returned as-is.
///
/// **The refusal short-circuit is the dangerous half.** A deterministic refusal
/// (read mode, plan mode, session cap, denied path) is yoyo's guard working as
/// designed. Re-running the subtask on a different model would not be a
/// fallback, it would be a **guard bypass** — the same subtask, the same
/// forbidden write, one model over. So refusals are returned verbatim before
/// the availability question is even asked, exactly as `RecoveryHintTool` does
/// (#710).
pub(crate) struct FallbackSubAgentTool {
    primary: Box<dyn AgentTool>,
    secondary: Box<dyn AgentTool>,
    primary_model: String,
    fallback_model: String,
}

impl FallbackSubAgentTool {
    pub(crate) fn new(
        primary: Box<dyn AgentTool>,
        secondary: Box<dyn AgentTool>,
        primary_model: impl Into<String>,
        fallback_model: impl Into<String>,
    ) -> Self {
        Self {
            primary,
            secondary,
            primary_model: primary_model.into(),
            fallback_model: fallback_model.into(),
        }
    }
}

#[async_trait::async_trait]
impl AgentTool for FallbackSubAgentTool {
    fn name(&self) -> &str {
        self.primary.name()
    }

    fn label(&self) -> &str {
        self.primary.label()
    }

    fn description(&self) -> &str {
        self.primary.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.primary.parameters_schema()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        let err = match self.primary.execute(params.clone(), ctx.clone()).await {
            Ok(result) => return Ok(result),
            Err(e) => e,
        };

        let text = err.to_string();

        // Guard first, availability second — see the type doc: retrying a
        // deliberate refusal on another model is a bypass, not a fallback.
        if is_deterministic_refusal(&text) || !is_model_unavailable_error(&text) {
            return Err(err);
        }

        eprintln!(
            "{DIM}  sub_agent: model {} unavailable — retrying on fallback {}{RESET}",
            self.primary_model, self.fallback_model
        );

        match self.secondary.execute(params, ctx).await {
            Ok(result) => {
                // Prepend the note as its own block rather than splicing it
                // into the first one: `content` may carry non-text blocks, and
                // a `Vec` insert cannot mangle them.
                let mut content = result.content;
                content.insert(
                    0,
                    yoagent::Content::Text {
                        text: fallback_switch_note(&self.primary_model, &self.fallback_model),
                    },
                );
                Ok(yoagent::types::ToolResult {
                    content,
                    details: result.details,
                })
            }
            // The fallback failed too. Return its own error rather than the
            // primary's: it is the more recent and more actionable fact, and
            // the stderr line above already recorded that a switch happened.
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// DiagnosticSubAgentTool — the parent gets the error's class, status and model
// ---------------------------------------------------------------------------

/// The model label the failure report names, stated once so the wording cannot
/// drift between the two shapes it has.
///
/// **The honesty constraint is the whole function.** When a fallback is
/// configured, this wrapper sits *outside* [`FallbackSubAgentTool`] and so sees
/// only the error that survived — it cannot observe which of the two attempts
/// produced it. Naming the wrong model is worse than naming both, so the label
/// names the *configuration* and says outright that the attribution is not
/// available. Threading the real answer out of the fallback decorator is a
/// larger change and is deliberately not started here.
pub(crate) fn sub_agent_model_label(primary: &str, fallback: Option<&str>) -> String {
    match fallback {
        None => format!("`{primary}`"),
        Some(fb) => format!(
            "`{primary}` with fallback `{fb}` (which of the two produced this error is not \
             observable here)"
        ),
    }
}

/// Compose the failure report a parent agent receives when a sub-agent dies.
///
/// Returns the original error **verbatim** plus one short bracketed tail. Pure,
/// so the wording is pinned by table test and every `Result` stays at the call
/// site.
///
/// **It may never invent a field.** Only what is observable is reported:
/// - the **class** (always — including `Unclassified`, spelled out, because
///   "a different model will not help" is genuinely actionable);
/// - the **model configuration**, which the wiring site always knows;
/// - the **HTTP status**, only when one is actually present, read through
///   `prompt_retry::contains_status_code` so a digit run like `402134` is not
///   mistaken for a code.
///
/// Deliberately **no request-id field**: no error string this repo has observed
/// carries one in a recognisable shape, and emitting `request_id: unknown`
/// would render "could not check" as "checked".
pub(crate) fn sub_agent_failure_report(model_label: &str, err: &str) -> String {
    let class = classify_sub_agent_error(err);
    let mut tail = format!(
        "[yoyo: the sub-agent failed — {}; model: {model_label}",
        class.describe()
    );
    if let Some(code) = observed_status_code(err) {
        tail.push_str(&format!("; http status: {code}"));
    }
    tail.push(']');
    format!("{err}\n\n{tail}")
}

/// Annotates a failed `sub_agent` call with the failure's class, the model
/// configuration it ran under, and any HTTP status actually present — so the
/// parent agent can tell a dead model from a bad key from a rate limit from
/// the delegated work simply not succeeding.
///
/// **Why this is the outermost wrapper and unconditional.**
/// [`FallbackSubAgentTool`] exists only when a fallback model is configured, so
/// putting the diagnostics inside it would reach a subset of users and leave
/// everyone else with the opaque string — the "two doors, one policy, one deaf"
/// shape this repo has already shipped six times (#745, #767, #769, #816,
/// `/config show`, and the sub-agent fallback itself). Wrapping the value
/// `tools.rs` finally returns means one wiring line rather than three failure
/// paths, and it annotates the error that actually **survives** the fallback
/// attempt rather than an intermediate one.
///
/// **The refusal short-circuit is the same one `RecoveryHintTool` (#710) and
/// `FallbackSubAgentTool` make, and for the same reason.** A read-mode,
/// plan-mode, session-cap or denied-path refusal is already a precise,
/// deliberate sentence; wrapping it in diagnostic scaffolding makes a guard
/// working as designed read as a malfunction.
pub(crate) struct DiagnosticSubAgentTool {
    inner: Box<dyn AgentTool>,
    model_label: String,
}

impl DiagnosticSubAgentTool {
    pub(crate) fn new(inner: Box<dyn AgentTool>, model_label: impl Into<String>) -> Self {
        Self {
            inner,
            model_label: model_label.into(),
        }
    }
}

#[async_trait::async_trait]
impl AgentTool for DiagnosticSubAgentTool {
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
        let err = match self.inner.execute(params, ctx).await {
            // A result that finished normally is byte-identical — the entire
            // regression surface. A result yoagent cut short on a bound comes
            // back as `Ok` too, so it is annotated here rather than in the
            // `Err` arm below, which is structurally blind to it.
            Ok(result) => return Ok(annotate_if_partial(result)),
            Err(e) => e,
        };

        // Guard first, and it short-circuits.
        if is_deterministic_refusal(&err.to_string()) {
            return Err(err);
        }

        // Only `Failed` is annotated, and the variant is preserved. `NotFound`
        // and `InvalidArgs` are yoagent's own dispatch/argument errors rather
        // than the outcome of a sub-agent run — a model class is noise there —
        // and `Cancelled` carries no payload to annotate.
        match err {
            yoagent::types::ToolError::Failed(msg) => Err(yoagent::types::ToolError::Failed(
                sub_agent_failure_report(&self.model_label, &msg),
            )),
            other => Err(other),
        }
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
        let truncated = truncate_result(result, 100, true);
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
        let truncated = truncate_result(result, TOOL_OUTPUT_MAX_CHARS, true);
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
        yoagent::types::ToolContext::new("test", "test")
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
        assert_eq!(tracker.record_failure("bash", "_"), 1);
        assert_eq!(tracker.record_failure("bash", "_"), 2);
        assert_eq!(tracker.get("bash", "_"), 2);

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
        assert_eq!(tracker.get("bash", "_"), 0);
    }

    #[test]
    fn test_is_deterministic_refusal_discriminates() {
        // One case per stem — a deliberate refusal.
        for msg in [
            "read mode is active — write_file is refused",
            "web_search session cap reached (200 calls) this session",
            "User denied bash on '/etc/passwd'",
        ] {
            assert!(is_deterministic_refusal(msg), "should be a refusal: {msg}");
        }
        // Ordinary tool failures — and the empty string — are not refusals.
        for msg in [
            "",
            "No such file or directory (os error 2)",
            "Command failed with exit code 1",
        ] {
            assert!(
                !is_deterministic_refusal(msg),
                "should not be a refusal: {msg}"
            );
        }
    }

    /// Both sides of the discriminator in one place (Day 122/124 — testing only
    /// the side that fires is vacuous green): a deterministic refusal passes
    /// through `RecoveryHintTool` verbatim — no hint appended (no coaching around
    /// the guard) and no failure-counter bump — while an ordinary error still
    /// gets its hint and still bumps the counter. #710.
    #[tokio::test]
    async fn test_recovery_hint_tool_short_circuits_only_refusals() {
        let refusal = format!("read mode{}write_file is refused", REFUSAL_STEM_MODE_ACTIVE);
        for (fail_msg, expect_hint, expect_count) in [
            (refusal.clone(), false, 0),
            (
                "No such file or directory (os error 2)".to_string(),
                true,
                1,
            ),
        ] {
            let tracker = ToolFailureTracker::new();
            let tool = with_recovery_hints(
                Box::new(ConfigurableMockTool {
                    tool_name: "write_file",
                    fail_msg: Some(fail_msg.clone()),
                }),
                &tracker,
            );
            let err = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await
                .expect_err("should still be an error");
            let yoagent::types::ToolError::Failed(msg) = err else {
                panic!("expected Failed for {fail_msg}");
            };
            if expect_hint {
                assert!(
                    msg.starts_with(&fail_msg) && msg.contains("💡 Recovery hint:"),
                    "ordinary error keeps its hint: {msg}"
                );
            } else {
                assert_eq!(msg, fail_msg, "refusal must come back byte-for-byte");
            }
            assert_eq!(
                tracker.get("write_file", "_"),
                expect_count,
                "failure counter for {fail_msg}"
            );
        }
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

        assert_eq!(tracker.get("bash", "_"), 2, "bash should have 2 failures");

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
            tracker.get("edit_file", "_"),
            1,
            "edit_file should have 1 failure"
        );
        assert_eq!(
            tracker.get("bash", "_"),
            2,
            "bash should still have 2 failures"
        );

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

        assert_eq!(
            tracker.get("bash", "_"),
            0,
            "bash should be reset after success"
        );
        assert_eq!(
            tracker.get("edit_file", "_"),
            1,
            "edit_file should be unaffected"
        );
    }

    // === ToolFailureTracker unit tests (pure logic, no async) ===

    #[test]
    fn test_tracker_new_is_empty() {
        let tracker = ToolFailureTracker::new();
        assert_eq!(tracker.get("bash", "_"), 0);
        assert_eq!(tracker.get("edit_file", "_"), 0);
        assert_eq!(tracker.get("nonexistent", "_"), 0);
    }

    #[test]
    fn test_tracker_record_failure_increments() {
        let tracker = ToolFailureTracker::new();
        assert_eq!(tracker.record_failure("bash", "_"), 1);
        assert_eq!(tracker.record_failure("bash", "_"), 2);
        assert_eq!(tracker.record_failure("bash", "_"), 3);
        assert_eq!(tracker.get("bash", "_"), 3);
    }

    #[test]
    fn test_tracker_record_success_resets() {
        let tracker = ToolFailureTracker::new();
        tracker.record_failure("bash", "_");
        tracker.record_failure("bash", "_");
        tracker.record_failure("bash", "_");
        assert_eq!(tracker.get("bash", "_"), 3);

        tracker.record_success("bash", "_");
        assert_eq!(tracker.get("bash", "_"), 0);
    }

    #[test]
    fn test_tracker_independent_tools() {
        let tracker = ToolFailureTracker::new();
        tracker.record_failure("bash", "_");
        tracker.record_failure("bash", "_");
        tracker.record_failure("edit_file", "_");

        assert_eq!(tracker.get("bash", "_"), 2);
        assert_eq!(tracker.get("edit_file", "_"), 1);

        // Resetting one doesn't affect the other
        tracker.record_success("bash", "_");
        assert_eq!(tracker.get("bash", "_"), 0);
        assert_eq!(tracker.get("edit_file", "_"), 1);
    }

    #[test]
    fn test_tracker_clone_shares_state() {
        let tracker = ToolFailureTracker::new();
        let cloned = tracker.clone();

        tracker.record_failure("bash", "_");
        assert_eq!(
            cloned.get("bash", "_"),
            1,
            "Clone should share the same state"
        );

        cloned.record_failure("bash", "_");
        assert_eq!(
            tracker.get("bash", "_"),
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
        let truncated = truncate_result(result, 1000, true);
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
        let truncated = truncate_result(result, 2000, true);
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
                    crate::format::safe_truncate(text, 200)
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
        let truncated = truncate_result(result, 10, true); // Very small limit
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
        let truncated = truncate_result(result, 100, true);
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
        let truncated = truncate_result(result, 10, true);
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
        let truncated = truncate_result(result, 100, true);
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
        let truncated = truncate_result(result, 50, true);
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
        let truncated = truncate_result(result, 100, true);
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
        let truncated = truncate_result(result, 500, true);
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
        let truncated = truncate_result(result, 1000, true);
        assert_eq!(
            truncated.details, details,
            "Details field should be preserved through truncation"
        );
    }

    // =========================================================================
    // TruncatingTool — test-output filter is gated on tool provenance (#665)
    // =========================================================================

    /// A file that merely QUOTES five cargo-style passing-test lines.
    /// Prose is interleaved so neither the noisy-pattern filter nor the
    /// repetitive-line collapser touches it — only the test-output filter
    /// could alter this payload.
    fn quoted_test_lines_payload() -> String {
        [
            "Example output from the README:",
            "test alpha ... ok",
            "prose line one.",
            "test beta ... ok",
            "prose line two.",
            "test gamma ... ok",
            "prose line three.",
            "test delta ... ok",
            "prose line four.",
            "test epsilon ... ok",
            "closing prose.",
            // Day 164: a real runner always emits a summary line; without one
            // this fixture asserted the ✓/pass-shape-only collapse that was
            // eating non-test bash output. The provenance gate needs it here.
            "test result: ok. 5 passed; 0 failed; 0 ignored",
        ]
        .join("\n")
    }

    async fn run_truncating_tool(tool_name: &'static str, payload: String) -> String {
        let tool = with_truncation(
            Box::new(MockTool {
                tool_name,
                result_text: payload,
            }),
            30_000,
        );
        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .expect("mock tool succeeds");
        match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            other => panic!("expected Text content, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_read_file_result_keeps_quoted_test_lines_verbatim() {
        // #665: a read_file result that quotes passing-test lines is file
        // CONTENT, not test output — it must survive verbatim.
        let payload = quoted_test_lines_payload();
        let text = run_truncating_tool("read_file", payload.clone()).await;
        assert_eq!(
            text, payload,
            "read_file output must not be eaten by the test-output filter"
        );
    }

    #[tokio::test]
    async fn test_bash_result_still_filters_test_lines() {
        // Pins that bash provenance keeps the old compression behavior.
        let payload = quoted_test_lines_payload();
        let text = run_truncating_tool("bash", payload).await;
        assert!(
            text.contains("passing tests omitted"),
            "bash output should still compress test-pass runs, got: {text}"
        );
        assert!(
            !text.contains("test alpha ... ok"),
            "bash pass lines should still be omitted, got: {text}"
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
            crate::format::safe_truncate(&text, 100)
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
            tracker.get("test_tool", "_"),
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
        assert_eq!(tracker.get("bash", "_"), 3);

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
            tracker.get("bash", "_"),
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
        tracker.record_success("never_used", "_");
        assert_eq!(tracker.get("never_used", "_"), 0);
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
                tracker.record_failure(name, "_");
            }
        }
        for (i, name) in tool_names.iter().enumerate() {
            assert_eq!(
                tracker.get(name, "_"),
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
                tracker_clone.record_failure("bash", "_");
            }
        });

        for _ in 0..100 {
            tracker.record_failure("bash", "_");
        }

        handle.join().unwrap();
        assert_eq!(
            tracker.get("bash", "_"),
            200,
            "Concurrent failures should all be recorded"
        );
    }

    // =========================================================================
    // File-aware tracker tests — per (tool, target) tracking
    // =========================================================================

    #[test]
    fn test_tracker_per_file_independent() {
        let tracker = ToolFailureTracker::new();
        // Failing on file A doesn't affect file B's count
        tracker.record_failure("edit_file", "src/main.rs");
        tracker.record_failure("edit_file", "src/main.rs");
        tracker.record_failure("edit_file", "src/lib.rs");

        assert_eq!(tracker.get("edit_file", "src/main.rs"), 2);
        assert_eq!(tracker.get("edit_file", "src/lib.rs"), 1);
    }

    #[test]
    fn test_tracker_success_resets_only_target_file() {
        let tracker = ToolFailureTracker::new();
        tracker.record_failure("edit_file", "src/a.rs");
        tracker.record_failure("edit_file", "src/a.rs");
        tracker.record_failure("edit_file", "src/b.rs");

        // Success on file A resets only file A
        tracker.record_success("edit_file", "src/a.rs");
        assert_eq!(tracker.get("edit_file", "src/a.rs"), 0);
        assert_eq!(tracker.get("edit_file", "src/b.rs"), 1);
    }

    #[test]
    fn test_extract_target_edit_file() {
        let input =
            serde_json::json!({"path": "src/main.rs", "old_text": "foo", "new_text": "bar"});
        assert_eq!(extract_target("edit_file", &input), "src/main.rs");
    }

    #[test]
    fn test_extract_target_read_file() {
        let input = serde_json::json!({"path": "README.md"});
        assert_eq!(extract_target("read_file", &input), "README.md");
    }

    #[test]
    fn test_extract_target_write_file() {
        let input = serde_json::json!({"path": "out.txt", "content": "hello"});
        assert_eq!(extract_target("write_file", &input), "out.txt");
    }

    #[test]
    fn test_extract_target_bash_short() {
        let input = serde_json::json!({"command": "cargo test"});
        assert_eq!(extract_target("bash", &input), "cargo test");
    }

    #[test]
    fn test_extract_target_bash_truncates_long() {
        let long_cmd = "a".repeat(100);
        let input = serde_json::json!({"command": long_cmd});
        let target = extract_target("bash", &input);
        assert!(
            target.len() <= 60,
            "bash target should be truncated to <=60 chars"
        );
    }

    #[test]
    fn test_extract_target_unknown_tool() {
        let input = serde_json::json!({"foo": "bar"});
        assert_eq!(extract_target("some_custom_tool", &input), "_");
    }

    #[test]
    fn test_extract_target_missing_path() {
        let input = serde_json::json!({"content": "hello"});
        assert_eq!(extract_target("edit_file", &input), "_");
    }

    #[tokio::test]
    async fn test_recovery_hint_file_specific_prefix() {
        // When the same file fails 2+ times, the hint should name the file
        let tracker = ToolFailureTracker::new();

        let tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "edit_file",
                fail_msg: Some("old_text not found".to_string()),
            }),
            &tracker,
        );

        // First failure — no file-specific prefix (params have path)
        let err1 = tool
            .execute(
                serde_json::json!({"path": "src/main.rs"}),
                test_tool_context(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(
            !err1.contains("You've failed to"),
            "First failure should NOT have file-specific prefix: {err1}"
        );

        // Second failure on same file — should have file-specific prefix
        let err2 = tool
            .execute(
                serde_json::json!({"path": "src/main.rs"}),
                test_tool_context(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err2.contains("You've failed to edit file 'src/main.rs' 2 times"),
            "Second failure should have file-specific prefix: {err2}"
        );
    }

    #[tokio::test]
    async fn test_recovery_hint_different_files_independent() {
        // Failing on file A then file B should NOT trigger file-specific hint for B
        let tracker = ToolFailureTracker::new();

        let tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "edit_file",
                fail_msg: Some("mismatch".to_string()),
            }),
            &tracker,
        );

        // Fail on file A
        let _ = tool
            .execute(serde_json::json!({"path": "src/a.rs"}), test_tool_context())
            .await;

        // Fail on file B — first failure for B, should NOT have file-specific prefix
        let err = tool
            .execute(serde_json::json!({"path": "src/b.rs"}), test_tool_context())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            !err.contains("You've failed to"),
            "First failure on file B should not have file-specific prefix: {err}"
        );
        // But file A should still have count 1
        assert_eq!(tracker.get("edit_file", "src/a.rs"), 1);
        assert_eq!(tracker.get("edit_file", "src/b.rs"), 1);
    }

    #[tokio::test]
    async fn test_recovery_hint_success_resets_only_target() {
        let tracker = ToolFailureTracker::new();

        // Fail on file A twice
        let fail_tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "read_file",
                fail_msg: Some("not found".to_string()),
            }),
            &tracker,
        );
        let _ = fail_tool
            .execute(serde_json::json!({"path": "src/a.rs"}), test_tool_context())
            .await;
        let _ = fail_tool
            .execute(serde_json::json!({"path": "src/a.rs"}), test_tool_context())
            .await;

        // Fail on file B once
        let _ = fail_tool
            .execute(serde_json::json!({"path": "src/b.rs"}), test_tool_context())
            .await;

        assert_eq!(tracker.get("read_file", "src/a.rs"), 2);
        assert_eq!(tracker.get("read_file", "src/b.rs"), 1);

        // Success on file A — should reset only A
        let ok_tool = with_recovery_hints(
            Box::new(ConfigurableMockTool {
                tool_name: "read_file",
                fail_msg: None,
            }),
            &tracker,
        );
        let _ = ok_tool
            .execute(serde_json::json!({"path": "src/a.rs"}), test_tool_context())
            .await;

        assert_eq!(
            tracker.get("read_file", "src/a.rs"),
            0,
            "Success should reset file A"
        );
        assert_eq!(
            tracker.get("read_file", "src/b.rs"),
            1,
            "File B should be unaffected by A's success"
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

    // === SessionCapTool tests ===

    #[tokio::test]
    async fn test_session_cap_executes_below_cap() {
        let tool = with_session_cap(
            Box::new(MockTool {
                tool_name: "web_search",
                result_text: "search results".to_string(),
            }),
            3,
        );
        for _ in 0..3 {
            let result = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await
                .unwrap();
            let text = match &result.content[0] {
                yoagent::Content::Text { text } => text.clone(),
                _ => panic!("Expected text content"),
            };
            assert_eq!(text, "search results");
        }
    }

    #[tokio::test]
    async fn test_session_cap_errors_past_cap() {
        let tool = with_session_cap(
            Box::new(MockTool {
                tool_name: "web_search",
                result_text: "search results".to_string(),
            }),
            2,
        );
        // First `cap` calls succeed.
        for _ in 0..2 {
            assert!(tool
                .execute(serde_json::json!({}), test_tool_context())
                .await
                .is_ok());
        }
        // Call cap+1 returns an error mentioning the cap and the tool name.
        let err = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("session cap reached"), "got: {msg}");
        assert!(msg.contains("2 calls"), "cap should be in message: {msg}");
        assert!(msg.contains("web_search"), "tool name in message: {msg}");
    }

    #[tokio::test]
    async fn test_session_cap_independent_counters() {
        let search = with_session_cap(
            Box::new(MockTool {
                tool_name: "web_search",
                result_text: "ok".to_string(),
            }),
            1,
        );
        let sub = with_session_cap(
            Box::new(MockTool {
                tool_name: "sub_agent",
                result_text: "ok".to_string(),
            }),
            1,
        );
        // Exhaust the search cap.
        assert!(search
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .is_ok());
        assert!(search
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .is_err());
        // sub_agent's counter is untouched.
        assert!(sub
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .is_ok());
    }

    /// Regression pin for the /clear budget-reset contract: `/clear` rebuilds
    /// the agent (dispatch.rs), which constructs brand-new `with_session_cap`
    /// wrappers — so the session budget resets because the counter is
    /// per-wrapper-INSTANCE. If anyone converts the counter to a process-wide
    /// static (e.g. keyed by tool name), a rebuilt wrapper would inherit the
    /// exhausted count and /clear would silently stop resetting the budget.
    /// This test fails loudly in that world.
    #[tokio::test]
    async fn test_session_cap_fresh_wrapper_resets_budget() {
        let make = || {
            with_session_cap(
                Box::new(MockTool {
                    tool_name: "web_search",
                    result_text: "ok".to_string(),
                }),
                2,
            )
        };

        // Exhaust the cap on the first wrapper (the pre-/clear agent).
        let old = make();
        for _ in 0..2 {
            assert!(old
                .execute(serde_json::json!({}), test_tool_context())
                .await
                .is_ok());
        }
        assert!(
            old.execute(serde_json::json!({}), test_tool_context())
                .await
                .is_err(),
            "cap should be hit on the old wrapper"
        );

        // A fresh wrapper of the SAME tool name (the post-/clear agent) must
        // start with a full budget — blocked-then-rebuilt unblocks.
        let fresh = make();
        for i in 0..2 {
            assert!(
                fresh
                    .execute(serde_json::json!({}), test_tool_context())
                    .await
                    .is_ok(),
                "fresh wrapper call {i} should be within a full budget"
            );
        }

        // Near-miss side (Day 122): a wrapper that never hit its cap also
        // stays unblocked after another wrapper is created — no cross-talk.
        let untouched = make();
        assert!(untouched
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .is_ok());
        let _another = make();
        assert!(
            untouched
                .execute(serde_json::json!({}), test_tool_context())
                .await
                .is_ok(),
            "un-blocked wrapper must keep its remaining budget when a new wrapper appears"
        );

        // And the old exhausted wrapper stays blocked — fresh instances never
        // leak a reset back into it.
        assert!(old
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .is_err());
    }

    // === ReadModeGuardTool tests ===
    //
    // Read mode is a process global (commands_config::set_read_mode), so these
    // tests are #[serial] and use an RAII guard to force read mode OFF at the
    // end of each test — even when an assertion panics — to avoid poisoning
    // other tests in the process.

    struct ReadModeReset;
    impl Drop for ReadModeReset {
        fn drop(&mut self) {
            crate::commands_config::set_read_mode(false);
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_blocks_write_tools_when_read_mode_on() {
        let _reset = ReadModeReset;
        crate::commands_config::set_read_mode(true);

        for name in ["write_file", "edit_file", "rename_symbol"] {
            let tool = with_read_guard(Box::new(MockTool {
                tool_name: name,
                result_text: "wrote".to_string(),
            }));
            let result = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await;
            let err = result.expect_err(&format!("{name} must be refused under read mode"));
            let msg = format!("{err:?}");
            assert!(
                msg.contains("read mode is active"),
                "refusal for {name} should name read mode, got: {msg}"
            );
            assert!(
                msg.contains("/read off"),
                "refusal for {name} should tell the user how to exit read mode, got: {msg}"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_passthrough_when_read_mode_off() {
        let _reset = ReadModeReset;
        crate::commands_config::set_read_mode(false);

        for name in ["write_file", "edit_file", "rename_symbol"] {
            let tool = with_read_guard(Box::new(MockTool {
                tool_name: name,
                result_text: "wrote".to_string(),
            }));
            let result = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await;
            assert!(
                result.is_ok(),
                "{name} must pass through when read mode is off (product default)"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_toggles_at_call_time_not_build_time() {
        let _reset = ReadModeReset;
        // Build the wrapper while read mode is OFF...
        crate::commands_config::set_read_mode(false);
        let tool = with_read_guard(Box::new(MockTool {
            tool_name: "write_file",
            result_text: "wrote".to_string(),
        }));
        assert!(tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .is_ok());

        // ...then flip it ON: the SAME wrapper instance must start refusing.
        crate::commands_config::set_read_mode(true);
        assert!(
            tool.execute(serde_json::json!({}), test_tool_context())
                .await
                .is_err(),
            "guard must check read mode at call time, not snapshot at build time"
        );

        // And flipping OFF again unblocks without a rebuild.
        crate::commands_config::set_read_mode(false);
        assert!(tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_bash_blocks_destructive_allows_readonly() {
        let _reset = ReadModeReset;
        crate::commands_config::set_read_mode(true);

        let tool = with_read_guard_bash(Box::new(MockTool {
            tool_name: "bash",
            result_text: "ran".to_string(),
        }));

        // Destructive command: refused with an honest error.
        let result = tool
            .execute(
                serde_json::json!({"command": "rm -rf /"}),
                test_tool_context(),
            )
            .await;
        let err = result.expect_err("destructive bash must be refused under read mode");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("read mode is active"),
            "bash refusal should name read mode, got: {msg}"
        );

        // Read-only commands: pass through even under read mode.
        for cmd in ["git status", "ls -la"] {
            let result = tool
                .execute(serde_json::json!({"command": cmd}), test_tool_context())
                .await;
            assert!(
                result.is_ok(),
                "read-only command `{cmd}` must be allowed under read mode"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_bash_blocks_write_commands_refused_in_read_mode() {
        let _reset = ReadModeReset;

        let tool = with_read_guard_bash(Box::new(MockTool {
            tool_name: "bash",
            result_text: "ran".to_string(),
        }));

        // Non-destructive write commands (the plan/read-mode write hole):
        // refused under read mode with an error naming what matched.
        crate::commands_config::set_read_mode(true);
        for cmd in ["tee /tmp/x", "touch /tmp/x"] {
            let result = tool
                .execute(serde_json::json!({"command": cmd}), test_tool_context())
                .await;
            let err = result.expect_err(&format!(
                "write command `{cmd}` must be refused in read mode"
            ));
            let msg = format!("{err:?}");
            assert!(
                msg.contains("read mode is active"),
                "refusal for `{cmd}` should name read mode, got: {msg}"
            );
            assert!(
                msg.contains("writes"),
                "refusal for `{cmd}` should say the command writes, got: {msg}"
            );
        }

        // Other side of the boundary (Day 122): the SAME commands pass when
        // no mode is active — normal bash stays unrestricted.
        crate::commands_config::set_read_mode(false);
        for cmd in ["tee /tmp/x", "touch /tmp/x"] {
            let result = tool
                .execute(serde_json::json!({"command": cmd}), test_tool_context())
                .await;
            assert!(
                result.is_ok(),
                "`{cmd}` must pass through when no blocking mode is active"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_bash_passthrough_when_read_mode_off() {
        let _reset = ReadModeReset;
        crate::commands_config::set_read_mode(false);

        let tool = with_read_guard_bash(Box::new(MockTool {
            tool_name: "bash",
            result_text: "ran".to_string(),
        }));
        // Even a destructive-looking command passes through the READ-MODE
        // guard when read mode is off (other layers, e.g. confirmation,
        // still apply in the real tool stack).
        let result = tool
            .execute(
                serde_json::json!({"command": "rm -rf /"}),
                test_tool_context(),
            )
            .await;
        assert!(
            result.is_ok(),
            "read-mode guard must be a transparent pass-through when read mode is off"
        );
    }

    // === Plan-mode enforcement tests (same guard, sibling mode) ===
    //
    // Plan mode and the plan-apply flag are process globals
    // (commands_plan::set_plan_mode / set_plan_apply_active), so these tests
    // are #[serial] and use an RAII guard to force both OFF at the end of
    // each test — even when an assertion panics.

    struct PlanModeReset;
    impl Drop for PlanModeReset {
        fn drop(&mut self) {
            crate::commands_plan::set_plan_mode(false);
            crate::commands_plan::set_plan_apply_active(false);
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_plan_guard_blocks_write_tools_when_plan_mode_on() {
        let _reset = PlanModeReset;
        crate::commands_plan::set_plan_mode(true);
        crate::commands_plan::set_plan_apply_active(false);

        for name in ["write_file", "edit_file", "rename_symbol"] {
            let tool = with_read_guard(Box::new(MockTool {
                tool_name: name,
                result_text: "wrote".to_string(),
            }));
            let result = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await;
            let err = result.expect_err(&format!("{name} must be refused under plan mode"));
            let msg = format!("{err:?}");
            assert!(
                msg.contains("plan mode is active"),
                "refusal for {name} should name plan mode, got: {msg}"
            );
            assert!(
                msg.contains("/plan apply") && msg.contains("/plan off"),
                "refusal for {name} should tell the user how to exit plan mode, got: {msg}"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_plan_guard_passthrough_when_apply_active() {
        let _reset = PlanModeReset;
        crate::commands_plan::set_plan_mode(true);
        crate::commands_plan::set_plan_apply_active(true);

        for name in ["write_file", "edit_file", "rename_symbol"] {
            let tool = with_read_guard(Box::new(MockTool {
                tool_name: name,
                result_text: "wrote".to_string(),
            }));
            let result = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await;
            assert!(
                result.is_ok(),
                "{name} must pass through during /plan apply — apply needs write access"
            );
        }

        // Bash too: apply must be able to run real commands.
        let bash = with_read_guard_bash(Box::new(MockTool {
            tool_name: "bash",
            result_text: "ran".to_string(),
        }));
        assert!(
            bash.execute(
                serde_json::json!({"command": "rm -rf ./build"}),
                test_tool_context(),
            )
            .await
            .is_ok(),
            "bash must pass through during /plan apply"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_plan_guard_passthrough_when_plan_mode_off() {
        let _reset = PlanModeReset;
        crate::commands_plan::set_plan_mode(false);
        crate::commands_plan::set_plan_apply_active(false);

        for name in ["write_file", "edit_file", "rename_symbol"] {
            let tool = with_read_guard(Box::new(MockTool {
                tool_name: name,
                result_text: "wrote".to_string(),
            }));
            let result = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await;
            assert!(
                result.is_ok(),
                "{name} must pass through when plan mode is off (product default)"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_plan_guard_bash_blocks_destructive_allows_readonly() {
        let _reset = PlanModeReset;
        crate::commands_plan::set_plan_mode(true);
        crate::commands_plan::set_plan_apply_active(false);

        let tool = with_read_guard_bash(Box::new(MockTool {
            tool_name: "bash",
            result_text: "ran".to_string(),
        }));

        // Destructive command: refused with an honest error.
        let result = tool
            .execute(
                serde_json::json!({"command": "rm -rf /"}),
                test_tool_context(),
            )
            .await;
        let err = result.expect_err("destructive bash must be refused under plan mode");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("plan mode is active"),
            "bash refusal should name plan mode, got: {msg}"
        );

        // Benign read-only command passes through.
        let result = tool
            .execute(
                serde_json::json!({"command": "git status"}),
                test_tool_context(),
            )
            .await;
        assert!(
            result.is_ok(),
            "read-only `git status` must be allowed under plan mode"
        );
    }

    // === Arc-flavour mode-guard tests (#709) ===
    //
    // The sub-agent tool list in `tools.rs` is `Arc<dyn AgentTool>`-shaped, so
    // the Box-only constructors could not reach it and children ignored the
    // mode the parent was in. These pin that the Arc entry points drive the
    // SAME guard: refuse under either mode, pass through when neither is on.
    // Mode state is process-global, hence `#[serial]` + Drop resets.

    #[tokio::test]
    #[serial]
    async fn test_read_guard_arc_blocks_write_tools_under_read_mode() {
        let _reset = ReadModeReset;
        crate::commands_config::set_read_mode(true);

        for name in ["write_file", "edit_file", "rename_symbol"] {
            let tool = with_read_guard_arc(Arc::new(MockTool {
                tool_name: name,
                result_text: "wrote".to_string(),
            }));
            let err = tool
                .execute(serde_json::json!({}), test_tool_context())
                .await
                .expect_err(&format!("{name} must be refused under read mode via Arc"));
            let msg = format!("{err:?}");
            assert!(
                msg.contains("read mode is active"),
                "Arc refusal for {name} should name read mode, got: {msg}"
            );
            // The deterministic-refusal stem is what stops prompt_retry from
            // burning attempts on an answer that will never change.
            assert!(
                msg.contains(REFUSAL_STEM_MODE_ACTIVE),
                "Arc refusal for {name} must carry the mode-active refusal stem, got: {msg}"
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_arc_blocks_under_plan_mode_and_bash_too() {
        let _reset = PlanModeReset;
        crate::commands_plan::set_plan_mode(true);
        crate::commands_plan::set_plan_apply_active(false);

        let tool = with_read_guard_arc(Arc::new(MockTool {
            tool_name: "write_file",
            result_text: "wrote".to_string(),
        }));
        let err = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .expect_err("write_file must be refused under plan mode via Arc");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("plan mode is active") && msg.contains(REFUSAL_STEM_MODE_ACTIVE),
            "Arc refusal should name plan mode and carry the stem, got: {msg}"
        );

        let bash = with_read_guard_bash_arc(Arc::new(MockTool {
            tool_name: "bash",
            result_text: "ran".to_string(),
        }));
        let err = bash
            .execute(
                serde_json::json!({"command": "touch build.log"}),
                test_tool_context(),
            )
            .await
            .expect_err("write-class bash must be refused under plan mode via Arc");
        assert!(
            format!("{err:?}").contains("plan mode is active"),
            "Arc bash refusal should name plan mode, got: {err:?}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_read_guard_arc_passthrough_when_no_mode_active() {
        let _read_reset = ReadModeReset;
        let _plan_reset = PlanModeReset;
        crate::commands_config::set_read_mode(false);
        crate::commands_plan::set_plan_mode(false);
        crate::commands_plan::set_plan_apply_active(false);

        // Write-class tool: the inner tool's real result comes back untouched.
        let tool = with_read_guard_arc(Arc::new(MockTool {
            tool_name: "write_file",
            result_text: "wrote 3 lines".to_string(),
        }));
        let result = tool
            .execute(serde_json::json!({}), test_tool_context())
            .await
            .expect("must pass through when neither mode is on (product default)");
        assert_eq!(
            result.content,
            vec![yoagent::Content::Text {
                text: "wrote 3 lines".to_string()
            }],
            "Arc guard must be byte-identical pass-through with no mode active"
        );

        // Bash, including a command the guard would refuse under a mode.
        let bash = with_read_guard_bash_arc(Arc::new(MockTool {
            tool_name: "bash",
            result_text: "ran".to_string(),
        }));
        let result = bash
            .execute(
                serde_json::json!({"command": "touch build.log"}),
                test_tool_context(),
            )
            .await
            .expect("bash must pass through when neither mode is on");
        assert_eq!(
            result.content,
            vec![yoagent::Content::Text {
                text: "ran".to_string()
            }],
            "Arc bash guard must be byte-identical pass-through with no mode active"
        );
    }
}

// ---------------------------------------------------------------------------
// FallbackSubAgentTool tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod fallback_sub_agent_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    // --- the predicate, both directions ---

    #[test]
    fn model_unavailable_fires_on_availability_shapes() {
        for msg in [
            "API error 404: not found",
            "HTTP 404",
            "status=404 model unavailable",
            "model not found: claude-opus-9",
            "model_not_found",
            "The model `gpt-5-turbo` does not exist",
            "unknown model: llama-99",
            "invalid model specified",
            "unsupported model for this endpoint",
            "no such model",
            "Not found: model gemini-4",
            "MODEL NOT FOUND",
        ] {
            assert!(
                is_model_unavailable_error(msg),
                "should be a model-availability failure: {msg}"
            );
        }
    }

    #[test]
    fn model_unavailable_does_not_fire_on_near_misses() {
        // Every one of these is a shape where a second model either fails
        // identically or must not be tried at all. A discriminator tested only
        // on the side that fires is vacuous green — and here the near-miss side
        // includes the guard-bypass case, which is the dangerous half.
        for msg in [
            // Ordinary work failure inside the sub-agent.
            "Command exited with status 1: cargo test failed",
            "File not found: src/missing.rs",
            "the sub-agent could not complete the task",
            // Auth — a different model on the same broken key fails the same.
            "401 Unauthorized",
            "authentication_error: invalid x-api-key",
            "invalid api key provided",
            "403 Forbidden",
            "permission denied",
            // Rate limits / overload — prompt_retry_limits.rs owns that policy.
            "429 Too Many Requests",
            "rate limit exceeded, retry after 60s",
            "rate_limit_error",
            "Overloaded: please try again",
            "insufficient quota for this model",
            "at capacity",
            // Day-174 digit-boundary lesson: a 404 inside a bigger number is
            // not a status code.
            "prompt is too long: 404123 tokens > 200000 maximum",
            "request id req_14045 failed",
        ] {
            assert!(
                !is_model_unavailable_error(msg),
                "must NOT be treated as a model-availability failure: {msg}"
            );
        }
    }

    #[test]
    fn model_unavailable_loses_to_auth_even_when_a_model_is_named() {
        // A message can carry both an auth word and a model name. In that case
        // the model is not the problem, so the auth check wins: losing a
        // genuine fallback is cheap, burning a second full sub-agent run
        // against a broken credential is not.
        assert!(!is_model_unavailable_error(
            "401 unauthorized: model claude-opus-5 does not exist for this key"
        ));
        assert!(!is_model_unavailable_error(
            "rate limit exceeded for model gpt-4o — unknown model tier"
        ));
    }

    #[test]
    fn deterministic_refusals_are_never_availability_failures() {
        // Belt and braces: the decorator short-circuits refusals *before*
        // asking, but the predicate must not classify one as retriable either.
        let refusal = format!("read mode{}write_file is refused", REFUSAL_STEM_MODE_ACTIVE);
        assert!(is_deterministic_refusal(&refusal));
        assert!(!is_model_unavailable_error(&refusal));
    }

    #[test]
    fn switch_note_names_both_models() {
        // An invisible model switch is a bug even when it is the right switch.
        let note = fallback_switch_note("claude-opus-5", "claude-sonnet-5");
        assert!(
            note.contains("claude-opus-5"),
            "names the dead model: {note}"
        );
        assert!(
            note.contains("claude-sonnet-5"),
            "names the model that answered: {note}"
        );
    }

    // --- the decorator, driven with counting stubs ---

    pub(super) struct CountingStub {
        calls: Arc<AtomicUsize>,
        /// `Some(msg)` → `ToolError::Failed(msg)`; `None` → success.
        fail_msg: Option<String>,
        reply: &'static str,
    }

    impl CountingStub {
        pub(super) fn failing(msg: &str) -> (Box<dyn AgentTool>, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Box::new(Self {
                    calls: calls.clone(),
                    fail_msg: Some(msg.to_string()),
                    reply: "",
                }),
                calls,
            )
        }
        pub(super) fn ok(reply: &'static str) -> (Box<dyn AgentTool>, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Box::new(Self {
                    calls: calls.clone(),
                    fail_msg: None,
                    reply,
                }),
                calls,
            )
        }
    }

    #[async_trait::async_trait]
    impl AgentTool for CountingStub {
        fn name(&self) -> &str {
            "sub_agent"
        }
        fn label(&self) -> &str {
            "sub_agent"
        }
        fn description(&self) -> &str {
            "counting stub"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match &self.fail_msg {
                Some(msg) => Err(yoagent::types::ToolError::Failed(msg.clone())),
                None => Ok(yoagent::types::ToolResult {
                    content: vec![yoagent::Content::Text {
                        text: self.reply.to_string(),
                    }],
                    details: serde_json::Value::Null,
                }),
            }
        }
    }

    pub(super) fn ctx() -> yoagent::types::ToolContext {
        yoagent::types::ToolContext::new("test", "sub_agent")
    }

    pub(super) fn first_text(result: &yoagent::types::ToolResult) -> String {
        match &result.content[0] {
            yoagent::Content::Text { text } => text.clone(),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn model_unavailable_reaches_the_fallback_exactly_once() {
        let (primary, p_calls) = CountingStub::failing("API error 404: model not found");
        let (secondary, s_calls) = CountingStub::ok("subtask done");
        let tool = FallbackSubAgentTool::new(primary, secondary, "dead-model", "live-model");

        let result = tool.execute(serde_json::json!({}), ctx()).await.unwrap();

        assert_eq!(p_calls.load(Ordering::SeqCst), 1, "primary tried once");
        assert_eq!(
            s_calls.load(Ordering::SeqCst),
            1,
            "fallback tried exactly once — never a chain of N"
        );
        // The switch is announced in-band, and the real output survives.
        let note = first_text(&result);
        assert!(
            note.contains("dead-model") && note.contains("live-model"),
            "{note}"
        );
        assert!(
            result.content.iter().any(|c| matches!(
                c,
                yoagent::Content::Text { text } if text.contains("subtask done")
            )),
            "the fallback's own output must be preserved"
        );
    }

    #[tokio::test]
    async fn ordinary_failure_never_reaches_the_fallback() {
        let (primary, p_calls) = CountingStub::failing("cargo test failed with status 1");
        let (secondary, s_calls) = CountingStub::ok("should not run");
        let tool = FallbackSubAgentTool::new(primary, secondary, "a", "b");

        let err = tool
            .execute(serde_json::json!({}), ctx())
            .await
            .unwrap_err();

        assert_eq!(p_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            s_calls.load(Ordering::SeqCst),
            0,
            "the sub-agent ran fine and the WORK failed — another model does not fix that"
        );
        assert!(
            err.to_string().contains("cargo test failed"),
            "returned verbatim: {err}"
        );
    }

    #[tokio::test]
    async fn deterministic_refusal_never_reaches_the_fallback() {
        // The dangerous half: re-running the same subtask on another model
        // would be a guard BYPASS, not a fallback.
        let refusal = format!("read mode{}write_file is refused", REFUSAL_STEM_MODE_ACTIVE);
        let (primary, p_calls) = CountingStub::failing(&refusal);
        let (secondary, s_calls) = CountingStub::ok("should not run");
        let tool = FallbackSubAgentTool::new(primary, secondary, "a", "b");

        let err = tool
            .execute(serde_json::json!({}), ctx())
            .await
            .unwrap_err();

        assert_eq!(p_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            s_calls.load(Ordering::SeqCst),
            0,
            "a deliberate refusal must never be retried on another model"
        );
        assert!(err.to_string().contains("write_file is refused"), "{err}");
    }

    #[tokio::test]
    async fn success_is_passed_through_untouched() {
        let (primary, p_calls) = CountingStub::ok("all good");
        let (secondary, s_calls) = CountingStub::ok("should not run");
        let tool = FallbackSubAgentTool::new(primary, secondary, "a", "b");

        let result = tool.execute(serde_json::json!({}), ctx()).await.unwrap();

        assert_eq!(p_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            s_calls.load(Ordering::SeqCst),
            0,
            "fallback never consulted"
        );
        // Byte-identical: exactly one block, no note prepended.
        assert_eq!(result.content.len(), 1, "no note on the success path");
        assert_eq!(first_text(&result), "all good");
    }

    #[tokio::test]
    async fn fallback_failure_returns_the_fallbacks_own_error() {
        let (primary, _) = CountingStub::failing("404 model not found");
        let (secondary, s_calls) = CountingStub::failing("fallback also exploded");
        let tool = FallbackSubAgentTool::new(primary, secondary, "a", "b");

        let err = tool
            .execute(serde_json::json!({}), ctx())
            .await
            .unwrap_err();

        assert_eq!(
            s_calls.load(Ordering::SeqCst),
            1,
            "one extra attempt, no more"
        );
        assert!(
            err.to_string().contains("fallback also exploded"),
            "the more recent and more actionable fact: {err}"
        );
    }

    #[tokio::test]
    async fn delegates_its_identity_to_the_primary() {
        let (primary, _) = CountingStub::ok("x");
        let (secondary, _) = CountingStub::ok("y");
        let tool = FallbackSubAgentTool::new(primary, secondary, "a", "b");
        assert_eq!(tool.name(), "sub_agent");
        assert_eq!(tool.label(), "sub_agent");
        assert_eq!(tool.description(), "counting stub");
    }
}

// ---------------------------------------------------------------------------
// DiagnosticSubAgentTool tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod diagnostic_sub_agent_tests {
    use super::fallback_sub_agent_tests::{ctx, first_text, CountingStub};
    use super::*;

    const LABEL: &str = "`primary-model`";

    // --- the classifier, and its agreement with the fallback predicate ---

    #[test]
    fn classifier_table() {
        for (err, want) in [
            // Model availability.
            (
                "API error 404: not found",
                SubAgentErrorClass::ModelUnavailable,
            ),
            (
                "model not found: claude-opus-9",
                SubAgentErrorClass::ModelUnavailable,
            ),
            (
                "The model `gpt-5-turbo` does not exist",
                SubAgentErrorClass::ModelUnavailable,
            ),
            // Auth wins over a named model — precedence is the safety property.
            ("401 Unauthorized", SubAgentErrorClass::Auth),
            (
                "authentication_error: invalid x-api-key",
                SubAgentErrorClass::Auth,
            ),
            ("403 Forbidden", SubAgentErrorClass::Auth),
            (
                "401 unauthorized: model claude-opus-5 does not exist for this key",
                SubAgentErrorClass::Auth,
            ),
            // Rate limit wins over a named model too.
            ("429 Too Many Requests", SubAgentErrorClass::RateLimit),
            (
                "rate limit exceeded, retry after 60s",
                SubAgentErrorClass::RateLimit,
            ),
            (
                "rate limit exceeded for model gpt-4o — unknown model tier",
                SubAgentErrorClass::RateLimit,
            ),
            (
                "Overloaded: please try again",
                SubAgentErrorClass::RateLimit,
            ),
            // The delegated work simply failed.
            (
                "Command exited with status 1: cargo test failed",
                SubAgentErrorClass::Unclassified,
            ),
            (
                "File not found: src/missing.rs",
                SubAgentErrorClass::Unclassified,
            ),
            // Day-174 digit boundary: a code inside a bigger number is not a code.
            (
                "prompt is too long: 402134 tokens > 200000 maximum",
                SubAgentErrorClass::Unclassified,
            ),
        ] {
            assert_eq!(classify_sub_agent_error(err), want, "classifying: {err}");
        }
    }

    #[test]
    fn the_predicate_and_the_classifier_can_never_disagree() {
        // `is_model_unavailable_error` is defined in terms of the classifier,
        // so one shape list serves both. This pins that, rather than trusting
        // two lists to stay in step.
        for err in [
            "404 not found",
            "401 unauthorized",
            "rate limit exceeded",
            "cargo test failed",
            "unknown model: llama-99",
        ] {
            assert_eq!(
                is_model_unavailable_error(err),
                classify_sub_agent_error(err) == SubAgentErrorClass::ModelUnavailable,
                "{err}"
            );
        }
    }

    // --- the composer, pure ---

    #[test]
    fn report_keeps_the_original_error_verbatim_and_names_the_class() {
        let report = sub_agent_failure_report(LABEL, "API error 404: model not found");
        assert!(
            report.starts_with("API error 404: model not found"),
            "the original error survives verbatim and leads: {report}"
        );
        assert!(report.contains("the model was unavailable"), "{report}");
        assert!(
            report.contains("primary-model"),
            "names the model: {report}"
        );
        assert!(report.contains("http status: 404"), "{report}");
    }

    #[test]
    fn report_always_states_an_unclassified_failure_rather_than_going_quiet() {
        // `Unclassified` is a real value: "a different model will not help" is
        // exactly as actionable as any named class. Silence here would be the
        // parent guessing again.
        let report = sub_agent_failure_report(LABEL, "cargo test failed with status 1");
        assert!(
            report.contains("an ordinary failure of the delegated work"),
            "{report}"
        );
        assert!(
            report.contains("a different model will not help"),
            "{report}"
        );
    }

    #[test]
    fn report_says_nothing_about_status_when_none_is_observable() {
        // "could not check" must never render as "checked" — no field at all,
        // never `http status: unknown`.
        let report = sub_agent_failure_report(LABEL, "the sub-agent could not complete the task");
        assert!(!report.contains("http status"), "{report}");
        assert!(!report.contains("unknown"), "{report}");
    }

    #[test]
    fn report_does_not_read_a_status_code_out_of_a_bigger_number() {
        // The Day-174 defect: `402` inside a token count diagnosed as
        // exhausted credits.
        let report =
            sub_agent_failure_report(LABEL, "prompt is too long: 402134 tokens > 200000 maximum");
        assert!(!report.contains("http status"), "{report}");
        assert!(
            report.contains("an ordinary failure of the delegated work"),
            "{report}"
        );
    }

    #[test]
    fn report_never_mentions_a_request_id() {
        // No observed provider error carries one in a recognisable shape, so
        // the field does not exist rather than existing and always being empty.
        for err in ["404 not found", "cargo test failed", "429 slow down"] {
            let report = sub_agent_failure_report(LABEL, err);
            assert!(!report.contains("request_id"), "{report}");
            assert!(!report.contains("request id"), "{report}");
        }
    }

    // --- the label, both shapes ---

    #[test]
    fn label_names_only_the_primary_when_no_fallback_is_configured() {
        let label = sub_agent_model_label("claude-opus-5", None);
        assert_eq!(label, "`claude-opus-5`");
    }

    #[test]
    fn label_names_both_models_and_refuses_to_attribute_the_failure() {
        // Naming the wrong model is worse than naming both: this wrapper sits
        // outside the fallback decorator and cannot see which attempt failed.
        let label = sub_agent_model_label("claude-opus-5", Some("claude-sonnet-5"));
        assert!(
            label.contains("claude-opus-5") && label.contains("claude-sonnet-5"),
            "{label}"
        );
        assert!(label.contains("not observable here"), "{label}");
    }

    // --- the decorator, at the emission point ---

    #[tokio::test]
    async fn success_is_passed_through_byte_identically() {
        // The entire regression surface: every user whose sub-agents succeed.
        let (inner, calls) = CountingStub::ok("subtask done");
        let tool = DiagnosticSubAgentTool::new(inner, LABEL);

        let result = tool.execute(serde_json::json!({}), ctx()).await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.content.len(), 1, "no block added on success");
        assert_eq!(first_text(&result), "subtask done");
    }

    #[tokio::test]
    async fn deterministic_refusals_are_returned_verbatim_and_unannotated() {
        // The near-miss guard, and the one that matters most: a guard working
        // as designed must not be dressed up as a malfunction.
        for stem in [
            REFUSAL_STEM_MODE_ACTIVE,
            REFUSAL_STEM_SESSION_CAP,
            REFUSAL_STEM_PATH_DENIED,
        ] {
            let refusal = format!("read mode{stem}write_file is refused");
            let (inner, _) = CountingStub::failing(&refusal);
            let tool = DiagnosticSubAgentTool::new(inner, LABEL);

            let err = tool
                .execute(serde_json::json!({}), ctx())
                .await
                .unwrap_err();

            assert_eq!(
                err.to_string(),
                refusal,
                "a deliberate refusal must survive byte-identically"
            );
        }
    }

    #[tokio::test]
    async fn each_failure_class_is_reported_and_keeps_the_original_text() {
        for (msg, want_class_phrase) in [
            (
                "API error 404: model not found",
                "the model was unavailable",
            ),
            (
                "401 Unauthorized: invalid x-api-key",
                "an authentication or permission failure",
            ),
            (
                "429 rate limit exceeded",
                "a rate limit or capacity failure",
            ),
            (
                "cargo test failed with status 1",
                "an ordinary failure of the delegated work",
            ),
        ] {
            let (inner, calls) = CountingStub::failing(msg);
            let tool = DiagnosticSubAgentTool::new(inner, LABEL);

            let err = tool
                .execute(serde_json::json!({}), ctx())
                .await
                .unwrap_err();
            let text = err.to_string();

            assert_eq!(calls.load(Ordering::SeqCst), 1, "inner called once: {msg}");
            assert!(text.contains(msg), "original error preserved: {text}");
            assert!(text.contains(want_class_phrase), "class named: {text}");
            assert!(text.contains("primary-model"), "model named: {text}");
        }
    }

    #[tokio::test]
    async fn identity_is_delegated_to_the_inner_tool() {
        let (inner, _) = CountingStub::ok("x");
        let tool = DiagnosticSubAgentTool::new(inner, LABEL);
        assert_eq!(tool.name(), "sub_agent");
        assert_eq!(tool.label(), "sub_agent");
        assert_eq!(tool.description(), "counting stub");
    }

    // --- the Ok path: a run cut short by a bound ---

    /// The marker shape is captured from yoagent's own source rather than
    /// hand-typed: `check_limits()` builds `"Max turns reached ({turns}/{max})"`
    /// and the loop wraps it as `"[Agent stopped: {reason}]"`.
    /// <!-- yoagent-version-claim: 0.18.1 -->
    const STOP_MARKER: &str = "[Agent stopped: Max turns reached (25/25)]";

    /// `CountingStub::ok` takes a `&'static str`, and every fixture here is
    /// built from `STOP_MARKER` at runtime so the marker has one spelling.
    /// Leaking a few test bytes beats a second hand-typed copy that can drift.
    fn leaked(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    #[test]
    fn partial_notice_table() {
        // Fires: the real marker, alone and folded after prose the way
        // `extract_final_text` folds it.
        for text in [
            STOP_MARKER,
            &format!("partial analysis of the parser\n\n{STOP_MARKER}"),
            "[Agent stopped: Max tokens reached (1000/1000)]",
        ] {
            assert!(
                sub_agent_partial_notice(text).is_some(),
                "should be flagged partial: {text}"
            );
        }

        // Does not fire. The prose rows are the ones that matter: a sub-agent
        // that *writes about* being stopped has not been stopped.
        for text in [
            "subtask done",
            "",
            "the agent stopped after three turns and reported success",
            "I checked whether the agent stopped early — it did not.",
            "Agent stopped: no bracket here",
        ] {
            assert_eq!(
                sub_agent_partial_notice(text),
                None,
                "must NOT be flagged partial: {text}"
            );
        }
    }

    #[test]
    fn partial_notice_states_the_three_things_the_raw_marker_does_not() {
        let note = sub_agent_partial_notice(STOP_MARKER).expect("marker fires");
        assert!(note.contains("PARTIAL"), "names the state: {note}");
        assert!(
            note.contains("not a failure") && note.contains("not a provider error"),
            "refuses the wrong diagnosis: {note}"
        );
        assert!(
            note.contains("re-dispatch"),
            "names the actionable consequence: {note}"
        );
    }

    #[tokio::test]
    async fn a_result_cut_short_by_a_bound_is_annotated_at_block_zero() {
        let body = leaked(format!("partial analysis of the parser\n\n{STOP_MARKER}"));
        let (inner, calls) = CountingStub::ok(body);
        let tool = DiagnosticSubAgentTool::new(inner, LABEL);

        let result = tool.execute(serde_json::json!({}), ctx()).await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.content.len(), 2, "one block added, none replaced");
        assert!(
            first_text(&result).contains("PARTIAL"),
            "the annotation is block 0: {:?}",
            first_text(&result)
        );

        // The original text survives unmodified, marker included: this
        // annotates, it does not rewrite.
        match &result.content[1] {
            yoagent::types::Content::Text { text } => assert_eq!(text, &body),
            other => panic!("original block must survive intact: {other:?}"),
        }
    }

    #[tokio::test]
    async fn ordinary_success_is_byte_identical_and_gains_no_block() {
        // The near-miss guard. A discriminator tested only on the side that
        // fires is vacuous green, and this is the side every user is on.
        let (inner, calls) = CountingStub::ok("subtask done");
        let tool = DiagnosticSubAgentTool::new(inner, LABEL);

        let result = tool.execute(serde_json::json!({}), ctx()).await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.content.len(), 1, "no block added");
        assert_eq!(
            first_text(&result),
            "subtask done",
            "whole-string equality, not a contains"
        );
    }

    #[tokio::test]
    async fn a_refusal_on_the_ok_path_is_returned_verbatim_even_carrying_the_marker() {
        // Guard-first, and it short-circuits: a deliberate refusal must never
        // be dressed in partial-result scaffolding, whatever else the text
        // happens to contain.
        for stem in [
            REFUSAL_STEM_MODE_ACTIVE,
            REFUSAL_STEM_SESSION_CAP,
            REFUSAL_STEM_PATH_DENIED,
        ] {
            let refusal = leaked(format!(
                "read mode{stem}write_file is refused\n\n{STOP_MARKER}"
            ));
            let (inner, _) = CountingStub::ok(refusal);
            let tool = DiagnosticSubAgentTool::new(inner, LABEL);

            let result = tool.execute(serde_json::json!({}), ctx()).await.unwrap();

            assert_eq!(result.content.len(), 1, "no block added to a refusal");
            assert_eq!(first_text(&result), refusal, "refusal survives verbatim");
        }
    }
}

#[cfg(test)]
mod path_argument_repair_tests {
    use super::repair_path_argument;
    use serde_json::json;

    /// Emission point: the params the fence and the inner tool actually receive.
    #[test]
    fn leaked_autolink_path_is_repaired_before_the_fence_sees_it() {
        let got = repair_path_argument(json!({
            "path": "src/[notes.md](http://notes.md)",
            "content": "hello"
        }));
        assert_eq!(got, json!({"path": "src/notes.md", "content": "hello"}));
    }

    /// NEAR-MISS GUARD — the entire regression surface. Every shape that is not a
    /// degenerate auto-link must come back byte-identical, asserted as a whole value.
    #[test]
    fn everything_else_is_byte_identical() {
        for v in [
            json!({"path": "src/main.rs"}),
            json!({"path": "src/main.rs", "content": "a](http://a)"}),
            json!({"path": "docs/[click](https://x.com)"}),
            json!({"path": "src/[id].tsx"}),
            json!({"command": "ls"}),
            json!({"path": 7}),
            json!({}),
        ] {
            assert_eq!(repair_path_argument(v.clone()), v, "input: {v}");
        }
    }

    /// Content is never inspected — rewriting a link inside file content would be the
    /// silent corruption the repair exists to avoid.
    #[test]
    fn content_carrying_a_degenerate_autolink_is_left_alone() {
        let v = json!({"path": "a.md", "content": "[notes.md](http://notes.md)"});
        assert_eq!(repair_path_argument(v.clone()), v);
    }
}
