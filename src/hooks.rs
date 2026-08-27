// Hook system — pre/post tool execution pipeline
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use yoagent::types::{AgentTool, ToolError, ToolResult};
use yoagent::Content;

/// Bytes of the *head* of a hook's stderr kept by [`cap_hook_stderr`].
///
/// Deliberately the same budget `CAPTURE_HEAD_BYTES` in `src/commands_run.rs` uses for
/// `/run`, and for the same reason: a hook's stderr reaches the model's context verbatim
/// (`PostHookResult::with_feedback`), so an unbounded one can overflow the conversation
/// and wedge the session (#844). Head **and** tail rather than tail-only, because a hook's
/// *first* error line is the actionable one while its last lines are usually the summary —
/// a tail-only cut throws away the line the user actually needs.
const HOOK_STDERR_HEAD_BYTES: usize = 4096;
/// Bytes of the *tail* of a hook's stderr kept by [`cap_hook_stderr`]. See
/// [`HOOK_STDERR_HEAD_BYTES`].
const HOOK_STDERR_TAIL_BYTES: usize = 4096;

/// Bound a hook's stderr to `head` + `tail` bytes before it enters the conversation.
///
/// At or under budget the input is returned **byte-identically** — that is every hook
/// anyone has today and is the whole regression surface. Over budget the middle is
/// dropped and the cut is marked **in band**, because a silent elision is the bug (the
/// same rule `/run`, `/bg`, `!` and `/ast` already follow).
///
/// Both cuts land on a `char` boundary and are never taken by raw byte index (rule #250 —
/// `s[..n]` panics inside a multi-byte character): the head walks *back* and the tail walks
/// *forward*, so a character straddling either cut is dropped **whole** rather than split.
fn cap_hook_stderr(raw: &str, head: usize, tail: usize) -> String {
    if raw.len() <= head + tail {
        return raw.to_string();
    }

    let mut head_end = head;
    while head_end > 0 && !raw.is_char_boundary(head_end) {
        head_end -= 1;
    }

    let mut tail_start = raw.len() - tail;
    while tail_start < raw.len() && !raw.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    // A pathological `head`/`tail` pair must not produce overlapping slices.
    if tail_start < head_end {
        tail_start = head_end;
    }

    let dropped = tail_start - head_end;
    format!(
        "{}\n… [yoyo: {dropped} bytes elided — hook stderr is capped at {head} head + {tail} tail bytes]\n{}",
        &raw[..head_end],
        &raw[tail_start..],
    )
}

/// Result returned by a post-hook, carrying both the (possibly modified) output
/// and optional feedback that will be injected into the agent's context.
///
/// Feedback is additional context from the hook — e.g. linter warnings, security
/// scan results — that the agent should see on its next turn. It's separate from
/// the tool output itself so hooks can add information without modifying what the
/// tool actually returned.
#[derive(Debug, Clone, PartialEq)]
pub struct PostHookResult {
    /// The (possibly modified) tool output, threaded through the hook chain.
    pub output: String,
    /// Optional feedback to inject into the agent's context. `None` means the hook
    /// has nothing extra to say; the tool result passes through unchanged.
    pub feedback: Option<String>,
}

impl PostHookResult {
    /// Convenience: wrap output with no feedback.
    pub fn passthrough(output: &str) -> Self {
        Self {
            output: output.to_string(),
            feedback: None,
        }
    }

    /// Convenience: wrap output with feedback.
    pub fn with_feedback(output: &str, feedback: String) -> Self {
        Self {
            output: output.to_string(),
            feedback: Some(feedback),
        }
    }
}

/// Hook that runs before/after tool execution.
///
/// Hooks form a pipeline: pre-hooks run first-to-last before the tool executes,
/// post-hooks run first-to-last after execution. A pre-hook can block execution
/// (return Err) or short-circuit with a cached result (return Ok(Some(...))).
/// A post-hook can inspect or modify the tool's output, and optionally return
/// feedback that gets injected into the agent's context.
pub trait Hook: Send + Sync {
    /// Human-readable name for this hook (used in diagnostics/logging).
    fn name(&self) -> &str;

    /// Pre-execute: return Err to block, Ok(None) to proceed, Ok(Some(result)) to short-circuit.
    fn pre_execute(
        &self,
        _tool_name: &str,
        _params: &serde_json::Value,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// Post-execute: can inspect/modify the result and optionally return feedback.
    ///
    /// The `output` field in [`PostHookResult`] threads through the hook chain (each
    /// hook sees the previous hook's output). The `feedback` field is collected across
    /// all hooks and concatenated — it's injected into the agent's context as an
    /// additional `Content::Text` block after the tool result.
    fn post_execute(
        &self,
        _tool_name: &str,
        _params: &serde_json::Value,
        output: &str,
    ) -> Result<PostHookResult, String> {
        Ok(PostHookResult::passthrough(output))
    }
}

/// Registry that collects hooks and runs them in order.
///
/// Pre-hooks run first-to-last: the first hook to block (Err) or short-circuit
/// (Ok(Some)) wins. Post-hooks run first-to-last, each receiving the output
/// from the previous hook (or the tool itself for the first hook).
pub struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: vec![] }
    }

    pub fn register(&mut self, hook: Box<dyn Hook>) {
        if crate::cli::is_verbose() {
            eprintln!("[hooks] registered: {}", hook.name());
        }
        self.hooks.push(hook);
    }

    /// Run all pre-hooks in order. Returns:
    /// - `Ok(None)` — all hooks passed, proceed with tool execution
    /// - `Ok(Some(result))` — a hook short-circuited with a cached result
    /// - `Err(reason)` — a hook blocked execution
    pub fn run_pre_hooks(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> Result<Option<String>, String> {
        for hook in &self.hooks {
            match hook.pre_execute(tool_name, params)? {
                Some(result) => return Ok(Some(result)),
                None => continue,
            }
        }
        Ok(None)
    }

    /// Run all post-hooks in order, threading output through each.
    /// Collects feedback from all hooks. Returns the final (possibly modified) output
    /// plus any concatenated feedback, or Err if a hook fails.
    pub fn run_post_hooks(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
        output: &str,
    ) -> Result<PostHookResult, String> {
        let mut current = output.to_string();
        let mut all_feedback: Vec<String> = Vec::new();
        for hook in &self.hooks {
            let result = hook.post_execute(tool_name, params, &current)?;
            current = result.output;
            if let Some(fb) = result.feedback {
                if !fb.is_empty() {
                    all_feedback.push(fb);
                }
            }
        }
        let feedback = if all_feedback.is_empty() {
            None
        } else {
            Some(all_feedback.join("\n"))
        };
        Ok(PostHookResult {
            output: current,
            feedback,
        })
    }

    /// Number of registered hooks.
    pub fn len(&self) -> usize {
        self.hooks.len()
    }

    /// Whether the registry has no hooks.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// AuditHook — logs every tool execution to `.yoyo/audit.jsonl`.
///
/// This is the audit logging that was previously done ad-hoc in the event handler.
/// Now it's a proper hook in the tool execution pipeline. Only logs when audit
/// mode is enabled (via `--audit` flag, `YOYO_AUDIT=1`, or config).
pub struct AuditHook;

impl Hook for AuditHook {
    fn name(&self) -> &str {
        "audit"
    }

    // AuditHook doesn't block or modify — it only observes.
    // pre_execute: default (Ok(None)) — always proceed.

    fn post_execute(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
        output: &str,
    ) -> Result<PostHookResult, String> {
        // #751: this hook deliberately writes NOTHING. `.yoyo/audit.jsonl` has exactly
        // one writer — the prompt event-stream handlers in `src/prompt.rs`, which key
        // in-flight calls by `tool_call_id` and so know the real duration and the real
        // `is_error` flag. This hook knows neither: it is handed only the output string,
        // so it cannot tell a failure from a success and used to hardcode
        // `duration_ms = 0, success = true`, producing a second entry per tool call.
        // AuditHook stays registered and observe-only — no write, no feedback.
        let _ = (tool_name, params);
        Ok(PostHookResult::passthrough(output))
    }
}

/// Phase at which a shell hook fires.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HookPhase {
    Pre,
    Post,
}

/// A user-configurable shell command hook loaded from `.yoyo.toml`.
///
/// Shell hooks run a shell command before or after a tool executes.
/// The tool_pattern can be a specific tool name (e.g. "bash") or "*" for all tools.
///
/// Environment variables available to the shell command:
/// - `TOOL_NAME` — the tool being executed
/// - `TOOL_PARAMS` — JSON string of tool parameters
/// - `TOOL_OUTPUT` — (post-hooks only) tool output, truncated to 1000 chars
///
/// Pre-hooks that exit non-zero block the tool. Post-hooks always pass through.
/// All shell commands have a 5-second timeout to prevent hanging.
#[derive(Clone)]
pub struct ShellHook {
    pub name: String,
    pub phase: HookPhase,
    pub tool_pattern: String,
    pub command: String,
}

impl ShellHook {
    /// Check if this hook should fire for the given tool name.
    fn matches_tool(&self, tool_name: &str) -> bool {
        self.tool_pattern == "*" || self.tool_pattern == tool_name
    }

    /// Run the shell command with the given environment variables.
    /// Returns Ok((exit_code, stderr_output)) or Err on timeout/spawn failure.
    /// Stderr is captured so post-hooks can use it as feedback to the agent.
    fn run_command(&self, env_vars: &[(&str, &str)]) -> Result<(i32, String), String> {
        use std::io::Read;
        use std::process::Command;
        use std::time::Duration;

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&self.command);
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        // Spawn and wait with timeout
        let mut child = cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn hook command: {e}"))?;

        let timeout = Duration::from_secs(5);
        let start = std::time::Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Capture stderr output for feedback
                    let mut stderr_buf = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        let _ = stderr.read_to_string(&mut stderr_buf);
                    }
                    // Bound at *capture* time, not at each reader, so every consumer
                    // of the returned pair inherits the cap (#844). This string reaches
                    // the model's context verbatim via `PostHookResult::with_feedback`,
                    // and a hook printing megabytes could otherwise overflow the
                    // conversation and wedge the session.
                    let stderr_buf = cap_hook_stderr(
                        &stderr_buf,
                        HOOK_STDERR_HEAD_BYTES,
                        HOOK_STDERR_TAIL_BYTES,
                    );
                    return Ok((status.code().unwrap_or(1), stderr_buf));
                }
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        return Err(format!("Hook '{}' timed out after 5 seconds", self.name));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(format!("Hook wait error: {e}")),
            }
        }
    }
}

impl Hook for ShellHook {
    fn name(&self) -> &str {
        &self.name
    }

    fn pre_execute(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> Result<Option<String>, String> {
        if self.phase != HookPhase::Pre || !self.matches_tool(tool_name) {
            return Ok(None);
        }

        let params_str = params.to_string();
        let env_vars = vec![
            ("TOOL_NAME", tool_name),
            ("TOOL_PARAMS", params_str.as_str()),
        ];

        match self.run_command(&env_vars) {
            Ok((0, _)) => Ok(None), // Success — proceed with tool execution
            Ok((code, _)) => Err(format!("Pre-hook '{}' exited with code {code}", self.name)),
            Err(e) => Err(e),
        }
    }

    fn post_execute(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
        output: &str,
    ) -> Result<PostHookResult, String> {
        if self.phase != HookPhase::Post || !self.matches_tool(tool_name) {
            return Ok(PostHookResult::passthrough(output));
        }

        let params_str = params.to_string();
        // Truncate output to 1000 chars for the env var
        let truncated_output: String = output.chars().take(1000).collect();
        let env_vars = vec![
            ("TOOL_NAME", tool_name),
            ("TOOL_PARAMS", params_str.as_str()),
            ("TOOL_OUTPUT", truncated_output.as_str()),
        ];

        // Post-hooks pass through original output; stderr becomes feedback
        match self.run_command(&env_vars) {
            Ok((_, stderr)) => {
                if stderr.trim().is_empty() {
                    Ok(PostHookResult::passthrough(output))
                } else {
                    Ok(PostHookResult::with_feedback(
                        output,
                        stderr.trim().to_string(),
                    ))
                }
            }
            // On failure, still pass through output with no feedback
            Err(_) => Ok(PostHookResult::passthrough(output)),
        }
    }
}

/// Parse shell hook definitions from a config HashMap.
///
/// Expected key format: `hooks.pre.<tool>` or `hooks.post.<tool>`
/// where `<tool>` is a tool name or `*` for all tools.
///
/// Example config entries:
/// ```text
/// hooks.pre.bash = "echo 'running bash'"
/// hooks.post.* = "echo 'tool finished'"
/// ```
pub fn parse_hooks_from_config(config: &HashMap<String, String>) -> Vec<ShellHook> {
    let mut hooks = Vec::new();

    // Collect and sort keys for deterministic ordering
    let mut keys: Vec<&String> = config.keys().filter(|k| k.starts_with("hooks.")).collect();
    keys.sort();

    for key in keys {
        let value = &config[key];
        // Strip "hooks." prefix and split into phase + tool_pattern
        let rest = &key["hooks.".len()..];
        let (phase, tool_pattern) = if let Some(tool) = rest.strip_prefix("pre.") {
            (HookPhase::Pre, tool)
        } else if let Some(tool) = rest.strip_prefix("post.") {
            (HookPhase::Post, tool)
        } else {
            continue; // Invalid format, skip
        };

        if tool_pattern.is_empty() || value.is_empty() {
            continue; // Skip empty patterns or commands
        }

        let phase_str = match phase {
            HookPhase::Pre => "pre",
            HookPhase::Post => "post",
        };

        hooks.push(ShellHook {
            name: format!("{phase_str}:{tool_pattern}"),
            phase,
            tool_pattern: tool_pattern.to_string(),
            command: value.clone(),
        });
    }

    hooks
}

/// A wrapper tool that runs hooks before/after delegating to the inner tool.
///
/// This is the outermost wrapper in the tool pipeline — it wraps tools that may
/// already be wrapped with TruncatingTool, GuardedTool, or ConfirmTool.
struct HookedTool {
    inner: Box<dyn AgentTool>,
    hooks: Arc<HookRegistry>,
}

#[async_trait::async_trait]
impl AgentTool for HookedTool {
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
    ) -> Result<ToolResult, ToolError> {
        // Run pre-hooks
        match self.hooks.run_pre_hooks(self.inner.name(), &params) {
            Err(reason) => {
                return Err(ToolError::Failed(format!("Blocked by hook: {reason}")));
            }
            Ok(Some(cached)) => {
                // Short-circuit: return the cached result without executing the tool
                return Ok(ToolResult {
                    content: vec![Content::Text { text: cached }],
                    details: serde_json::Value::default(),
                });
            }
            Ok(None) => {
                // Proceed with normal execution
            }
        }

        // Execute the inner tool
        let result = self.inner.execute(params.clone(), ctx).await?;

        // Extract text content for post-hooks
        let output_text: String = result
            .content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Run post-hooks (they can inspect/modify the output and provide feedback)
        match self
            .hooks
            .run_post_hooks(self.inner.name(), &params, &output_text)
        {
            Ok(post_result) => {
                let mut final_result = result;
                // If any post-hook returned feedback, append it as additional context
                if let Some(feedback) = post_result.feedback {
                    final_result.content.push(Content::Text {
                        text: format!("\n[Hook feedback]\n{feedback}"),
                    });
                }
                Ok(final_result)
            }
            Err(reason) => Err(ToolError::Failed(format!("Post-hook error: {reason}"))),
        }
    }
}

/// Wrap a tool with the hook registry. If the registry is empty, returns the tool unwrapped.
pub fn maybe_hook(tool: Box<dyn AgentTool>, hooks: &Arc<HookRegistry>) -> Box<dyn AgentTool> {
    if hooks.is_empty() {
        tool
    } else {
        Box::new(HookedTool {
            inner: tool,
            hooks: Arc::clone(hooks),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::TOOL_OUTPUT_MAX_CHARS;
    use crate::tools::build_tools;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_hook_registry_new_is_empty() {
        let registry = HookRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_hook_registry_default_is_empty() {
        let registry = HookRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_pre_hooks_with_no_hooks_returns_none() {
        let registry = HookRegistry::new();
        let params = serde_json::json!({"command": "ls"});
        let result = registry.run_pre_hooks("bash", &params);
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_post_hooks_with_no_hooks_passes_through() {
        let registry = HookRegistry::new();
        let params = serde_json::json!({});
        let result = registry.run_post_hooks("bash", &params, "hello world");
        assert_eq!(result, Ok(PostHookResult::passthrough("hello world")));
    }

    /// A test hook that blocks all tool execution.
    struct BlockingHook;
    impl Hook for BlockingHook {
        fn name(&self) -> &str {
            "blocker"
        }
        fn pre_execute(
            &self,
            _tool_name: &str,
            _params: &serde_json::Value,
        ) -> Result<Option<String>, String> {
            Err("blocked by test".to_string())
        }
    }

    #[test]
    fn test_blocking_pre_hook_returns_err() {
        let mut registry = HookRegistry::new();
        registry.register(Box::new(BlockingHook));
        let params = serde_json::json!({});
        let result = registry.run_pre_hooks("bash", &params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "blocked by test");
    }

    /// A test hook that short-circuits with a cached result.
    struct CachingHook {
        cached: String,
    }
    impl Hook for CachingHook {
        fn name(&self) -> &str {
            "cache"
        }
        fn pre_execute(
            &self,
            _tool_name: &str,
            _params: &serde_json::Value,
        ) -> Result<Option<String>, String> {
            Ok(Some(self.cached.clone()))
        }
    }

    #[test]
    fn test_short_circuit_pre_hook_returns_cached_result() {
        let mut registry = HookRegistry::new();
        registry.register(Box::new(CachingHook {
            cached: "cached output".to_string(),
        }));
        let params = serde_json::json!({});
        let result = registry.run_pre_hooks("read_file", &params);
        assert_eq!(result, Ok(Some("cached output".to_string())));
    }

    /// A test hook that modifies output in post_execute.
    struct UppercaseHook;
    impl Hook for UppercaseHook {
        fn name(&self) -> &str {
            "uppercase"
        }
        fn post_execute(
            &self,
            _tool_name: &str,
            _params: &serde_json::Value,
            output: &str,
        ) -> Result<PostHookResult, String> {
            Ok(PostHookResult::passthrough(&output.to_uppercase()))
        }
    }

    #[test]
    fn test_post_hook_can_modify_output() {
        let mut registry = HookRegistry::new();
        registry.register(Box::new(UppercaseHook));
        let params = serde_json::json!({});
        let result = registry.run_post_hooks("bash", &params, "hello");
        assert_eq!(result, Ok(PostHookResult::passthrough("HELLO")));
    }

    /// A test hook that appends a tag to output.
    struct TagHook {
        tag: String,
    }
    impl Hook for TagHook {
        fn name(&self) -> &str {
            "tag"
        }
        fn post_execute(
            &self,
            _tool_name: &str,
            _params: &serde_json::Value,
            output: &str,
        ) -> Result<PostHookResult, String> {
            Ok(PostHookResult::passthrough(&format!(
                "{output}:{}",
                self.tag
            )))
        }
    }

    #[test]
    fn test_hook_ordering_post_hooks_chain_first_to_last() {
        let mut registry = HookRegistry::new();
        registry.register(Box::new(TagHook {
            tag: "first".to_string(),
        }));
        registry.register(Box::new(TagHook {
            tag: "second".to_string(),
        }));
        registry.register(Box::new(TagHook {
            tag: "third".to_string(),
        }));
        let params = serde_json::json!({});
        let result = registry.run_post_hooks("bash", &params, "start");
        // Each hook appends its tag in order
        assert_eq!(
            result,
            Ok(PostHookResult::passthrough("start:first:second:third"))
        );
    }

    /// A pass-through hook that increments a counter.
    struct CountingHook {
        count: std::sync::atomic::AtomicUsize,
    }
    impl Hook for CountingHook {
        fn name(&self) -> &str {
            "counter"
        }
        fn pre_execute(
            &self,
            _tool_name: &str,
            _params: &serde_json::Value,
        ) -> Result<Option<String>, String> {
            self.count.fetch_add(1, Ordering::Relaxed);
            Ok(None)
        }
    }

    #[test]
    fn test_hook_ordering_pre_hooks_run_first_to_last() {
        // Register a pass-through hook, then a blocking hook.
        // The pass-through should run (incrementing count), then the blocker fires.
        let mut registry = HookRegistry::new();
        let counter = Arc::new(CountingHook {
            count: std::sync::atomic::AtomicUsize::new(0),
        });
        // We can't share Arc<CountingHook> directly via register(Box<dyn Hook>),
        // so we test ordering by putting a blocker second and checking that Err is returned.
        // A pass-through + blocker = first runs, second blocks.
        struct PassThroughHook;
        impl Hook for PassThroughHook {
            fn name(&self) -> &str {
                "pass"
            }
        }
        registry.register(Box::new(PassThroughHook));
        registry.register(Box::new(BlockingHook));
        let params = serde_json::json!({});
        // Blocker is second, so result should be Err (first hook passed through)
        let result = registry.run_pre_hooks("bash", &params);
        assert!(
            result.is_err(),
            "Second hook (blocker) should fire after first"
        );
        // Count that registry has 2 hooks
        assert_eq!(registry.len(), 2);
        drop(counter);
    }

    #[test]
    fn test_short_circuit_pre_hook_stops_later_hooks() {
        // A caching hook followed by a blocking hook: the cache should win, blocker never runs.
        let mut registry = HookRegistry::new();
        registry.register(Box::new(CachingHook {
            cached: "early exit".to_string(),
        }));
        registry.register(Box::new(BlockingHook));
        let params = serde_json::json!({});
        let result = registry.run_pre_hooks("bash", &params);
        assert_eq!(
            result,
            Ok(Some("early exit".to_string())),
            "Caching hook should short-circuit before blocker"
        );
    }

    #[test]
    fn test_audit_hook_implements_trait() {
        let hook = AuditHook;
        assert_eq!(hook.name(), "audit");

        // pre_execute should always return Ok(None) — never blocks
        let params = serde_json::json!({"command": "ls"});
        let pre = hook.pre_execute("bash", &params);
        assert_eq!(pre, Ok(None));

        // post_execute should pass through output unchanged
        // (#751: the hook never writes to the audit log at all — the prompt
        //  event-stream handlers are the single writer)
        let post = hook.post_execute("bash", &params, "file1.rs\nfile2.rs");
        assert_eq!(post, Ok(PostHookResult::passthrough("file1.rs\nfile2.rs")));
    }

    /// #751: `.yoyo/audit.jsonl` must have exactly ONE writer — the prompt event-stream
    /// handlers in `src/prompt.rs`, which know the real duration and the real error flag.
    /// `AuditHook::post_execute` used to write a second entry per tool call with a
    /// hardcoded `0` duration and a hardcoded `success: true` (it only receives the output
    /// string, so it cannot know whether the tool errored). That doubled every count read
    /// off the log, halved every duration average, and would have reported a
    /// 100%-failure session as 50% success.
    ///
    /// This is a source-level check because the defect is "a call site came back", not a
    /// value a unit test can observe without racing on the process-global cwd. The needle
    /// is assembled at runtime so this test's own source cannot satisfy it.
    #[test]
    fn test_hooks_never_writes_to_the_audit_log() {
        let src = include_str!("hooks.rs");
        let needle = format!("{}{}", "audit_log_tool", "_call(");
        assert!(
            !src.contains(&needle),
            "src/hooks.rs calls `{needle})`, reintroducing a second writer for \
             .yoyo/audit.jsonl. The single writer is the event-stream handler in \
             src/prompt.rs, which has the real duration and the real `!is_error`; a hook \
             only sees the output string and would have to fabricate both. The audit log \
             is pushed to the public audit-log branch and mined by skill-evolve and \
             scripts/extract_trajectory.py, so a duplicate row is a wrong number, not \
             noise."
        );
    }

    #[test]
    fn test_hook_registry_register_increases_len() {
        let mut registry = HookRegistry::new();
        assert_eq!(registry.len(), 0);
        registry.register(Box::new(AuditHook));
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        registry.register(Box::new(UppercaseHook));
        assert_eq!(registry.len(), 2);
    }

    // --- ShellHook tests ---

    #[test]
    fn test_parse_hooks_from_config_empty() {
        let config = HashMap::new();
        let hooks = parse_hooks_from_config(&config);
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_parse_hooks_from_config_pre_bash() {
        let mut config = HashMap::new();
        config.insert(
            "hooks.pre.bash".to_string(),
            "echo 'running bash'".to_string(),
        );
        let hooks = parse_hooks_from_config(&config);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].name, "pre:bash");
        assert_eq!(hooks[0].phase, HookPhase::Pre);
        assert_eq!(hooks[0].tool_pattern, "bash");
        assert_eq!(hooks[0].command, "echo 'running bash'");
    }

    #[test]
    fn test_parse_hooks_from_config_post_wildcard() {
        let mut config = HashMap::new();
        config.insert("hooks.post.*".to_string(), "echo 'tool done'".to_string());
        let hooks = parse_hooks_from_config(&config);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].name, "post:*");
        assert_eq!(hooks[0].phase, HookPhase::Post);
        assert_eq!(hooks[0].tool_pattern, "*");
        assert_eq!(hooks[0].command, "echo 'tool done'");
    }

    #[test]
    fn test_parse_hooks_from_config_multiple() {
        let mut config = HashMap::new();
        config.insert("hooks.pre.bash".to_string(), "echo 'pre bash'".to_string());
        config.insert(
            "hooks.post.write_file".to_string(),
            "echo 'wrote file'".to_string(),
        );
        config.insert("hooks.post.*".to_string(), "echo 'any tool'".to_string());
        // Non-hook key should be ignored
        config.insert("model".to_string(), "claude-opus-4-6".to_string());
        let hooks = parse_hooks_from_config(&config);
        assert_eq!(hooks.len(), 3);
        // Should be sorted by key: hooks.post.* < hooks.post.write_file < hooks.pre.bash
        assert_eq!(hooks[0].name, "post:*");
        assert_eq!(hooks[1].name, "post:write_file");
        assert_eq!(hooks[2].name, "pre:bash");
    }

    #[test]
    fn test_parse_hooks_from_config_ignores_invalid() {
        let mut config = HashMap::new();
        // Invalid: no phase
        config.insert("hooks.bash".to_string(), "echo test".to_string());
        // Invalid: empty tool pattern
        config.insert("hooks.pre.".to_string(), "echo test".to_string());
        // Invalid: empty command
        config.insert("hooks.post.bash".to_string(), "".to_string());
        let hooks = parse_hooks_from_config(&config);
        assert!(hooks.is_empty(), "Invalid entries should be skipped");
    }

    #[test]
    fn test_shell_hook_pre_matching() {
        // A pre-hook for "bash" should only fire for bash, not for read_file
        let hook = ShellHook {
            name: "pre:bash".to_string(),
            phase: HookPhase::Pre,
            tool_pattern: "bash".to_string(),
            command: "true".to_string(), // exits 0
        };

        let params = serde_json::json!({"command": "ls"});

        // Should fire for bash (exits 0 → Ok(None))
        let result = hook.pre_execute("bash", &params);
        assert_eq!(result, Ok(None));

        // Should NOT fire for read_file (returns Ok(None) without running)
        let result = hook.pre_execute("read_file", &params);
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_shell_hook_pre_blocking() {
        // A pre-hook that exits non-zero should block the tool
        let hook = ShellHook {
            name: "pre:bash".to_string(),
            phase: HookPhase::Pre,
            tool_pattern: "bash".to_string(),
            command: "exit 1".to_string(),
        };

        let params = serde_json::json!({"command": "rm -rf /"});
        let result = hook.pre_execute("bash", &params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("pre:bash"));
    }

    #[test]
    fn test_shell_hook_post_passthrough() {
        // A post-hook should return the original output unchanged
        let hook = ShellHook {
            name: "post:bash".to_string(),
            phase: HookPhase::Post,
            tool_pattern: "bash".to_string(),
            command: "echo 'notified'".to_string(),
        };

        let params = serde_json::json!({"command": "ls"});
        let result = hook.post_execute("bash", &params, "file1.rs\nfile2.rs");
        assert_eq!(
            result,
            Ok(PostHookResult::passthrough("file1.rs\nfile2.rs"))
        );
    }

    #[test]
    fn test_shell_hook_wildcard_matches_all() {
        // A wildcard hook should fire for any tool
        let hook = ShellHook {
            name: "pre:*".to_string(),
            phase: HookPhase::Pre,
            tool_pattern: "*".to_string(),
            command: "true".to_string(),
        };

        let params = serde_json::json!({});
        assert_eq!(hook.pre_execute("bash", &params), Ok(None));
        assert_eq!(hook.pre_execute("read_file", &params), Ok(None));
        assert_eq!(hook.pre_execute("write_file", &params), Ok(None));
    }

    #[test]
    fn test_shell_hook_post_non_matching_passes_through() {
        // A post-hook for "bash" should not run for "read_file" — just pass through
        let hook = ShellHook {
            name: "post:bash".to_string(),
            phase: HookPhase::Post,
            tool_pattern: "bash".to_string(),
            command: "exit 1".to_string(), // Would fail if it ran
        };

        let params = serde_json::json!({});
        let result = hook.post_execute("read_file", &params, "content");
        assert_eq!(result, Ok(PostHookResult::passthrough("content")));
    }

    #[test]
    fn test_shell_hook_pre_phase_skips_post_tool() {
        // A Pre-phase hook should not fire in post_execute
        let hook = ShellHook {
            name: "pre:bash".to_string(),
            phase: HookPhase::Pre,
            tool_pattern: "bash".to_string(),
            command: "exit 1".to_string(), // Would fail if it ran
        };

        let params = serde_json::json!({});
        // post_execute should pass through because phase is Pre
        let result = hook.post_execute("bash", &params, "output");
        assert_eq!(result, Ok(PostHookResult::passthrough("output")));
    }

    #[test]
    fn test_shell_hook_env_vars_available() {
        // Verify that TOOL_NAME and TOOL_PARAMS env vars are set
        let hook = ShellHook {
            name: "pre:bash".to_string(),
            phase: HookPhase::Pre,
            tool_pattern: "bash".to_string(),
            // This command checks that the env vars exist
            command: "test -n \"$TOOL_NAME\" && test -n \"$TOOL_PARAMS\"".to_string(),
        };

        let params = serde_json::json!({"command": "ls -la"});
        let result = hook.pre_execute("bash", &params);
        assert_eq!(result, Ok(None), "Env vars should be set and non-empty");
    }

    // ── Tests relocated from main.rs ──────────────────────────────────

    #[test]
    fn test_maybe_hook_skips_wrap_when_empty() {
        // With an empty registry, maybe_hook should return the tool as-is (no HookedTool wrapper)
        let perms = crate::config::PermissionConfig::default();
        let dirs = crate::config::DirectoryRestrictions::default();
        // Build with audit=false => hooks is empty => tools are NOT wrapped
        let tools = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        assert_eq!(tools.len(), 9, "Tool count should be 9 without audit hooks");
    }

    #[test]
    fn test_build_tools_with_audit_preserves_tool_count() {
        // With audit=true, tool count stays the same (tools are wrapped, not added)
        let perms = crate::config::PermissionConfig::default();
        let dirs = crate::config::DirectoryRestrictions::default();
        let tools_no_audit = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let tools_with_audit = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            true,
            vec![],
            None,
        );
        assert_eq!(
            tools_no_audit.len(),
            tools_with_audit.len(),
            "Audit hooks should wrap tools, not add new ones"
        );
    }

    #[test]
    fn test_build_tools_with_audit_preserves_tool_names() {
        // Tool names should be identical with or without audit
        let perms = crate::config::PermissionConfig::default();
        let dirs = crate::config::DirectoryRestrictions::default();
        let tools_no_audit = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            false,
            vec![],
            None,
        );
        let tools_with_audit = build_tools(
            true,
            &perms,
            &dirs,
            TOOL_OUTPUT_MAX_CHARS,
            true,
            vec![],
            None,
        );
        let names_no: Vec<&str> = tools_no_audit.iter().map(|t| t.name()).collect();
        let names_yes: Vec<&str> = tools_with_audit.iter().map(|t| t.name()).collect();
        assert_eq!(
            names_no, names_yes,
            "Tool names should be identical with/without audit"
        );
    }

    // --- PostHookResult feedback tests ---

    /// A test hook that returns feedback alongside the output.
    struct FeedbackHook {
        feedback: String,
    }
    impl Hook for FeedbackHook {
        fn name(&self) -> &str {
            "feedback"
        }
        fn post_execute(
            &self,
            _tool_name: &str,
            _params: &serde_json::Value,
            output: &str,
        ) -> Result<PostHookResult, String> {
            Ok(PostHookResult::with_feedback(output, self.feedback.clone()))
        }
    }

    #[test]
    fn test_hook_with_feedback_returns_it_in_result() {
        let mut registry = HookRegistry::new();
        registry.register(Box::new(FeedbackHook {
            feedback: "lint: 2 warnings".to_string(),
        }));
        let params = serde_json::json!({});
        let result = registry.run_post_hooks("bash", &params, "ok");
        let expected = PostHookResult {
            output: "ok".to_string(),
            feedback: Some("lint: 2 warnings".to_string()),
        };
        assert_eq!(result, Ok(expected));
    }

    #[test]
    fn test_hook_without_feedback_no_extra_content() {
        let mut registry = HookRegistry::new();
        registry.register(Box::new(UppercaseHook)); // returns passthrough (no feedback)
        let params = serde_json::json!({});
        let result = registry.run_post_hooks("bash", &params, "hello");
        assert_eq!(result.unwrap().feedback, None);
    }

    #[test]
    fn test_multiple_hooks_feedback_concatenated() {
        let mut registry = HookRegistry::new();
        registry.register(Box::new(FeedbackHook {
            feedback: "hook-a says hi".to_string(),
        }));
        registry.register(Box::new(FeedbackHook {
            feedback: "hook-b says hi".to_string(),
        }));
        let params = serde_json::json!({});
        let result = registry.run_post_hooks("bash", &params, "output").unwrap();
        assert_eq!(
            result.feedback,
            Some("hook-a says hi\nhook-b says hi".to_string())
        );
    }

    #[test]
    fn test_shell_hook_stderr_becomes_feedback() {
        // A post-hook that writes to stderr — stderr should appear as feedback
        let hook = ShellHook {
            name: "post:bash".to_string(),
            phase: HookPhase::Post,
            tool_pattern: "bash".to_string(),
            command: "echo 'lint warning: unused var' >&2".to_string(),
        };

        let params = serde_json::json!({"command": "ls"});
        let result = hook.post_execute("bash", &params, "file1.rs").unwrap();
        // Output should be unchanged
        assert_eq!(result.output, "file1.rs");
        // Feedback should contain the stderr content
        assert!(result.feedback.is_some());
        assert!(result
            .feedback
            .as_ref()
            .unwrap()
            .contains("lint warning: unused var"));
    }

    // -----------------------------------------------------------------------
    // #844 — hook stderr is bounded before it enters the conversation.
    //
    // `run_command` already capped its *input* (`TOOL_OUTPUT`, 1000 chars) and left
    // its *output* unbounded — so the bounded direction was the one yoyo controls
    // and the unbounded one was arbitrary user output flowing into the model's
    // context. The existing input cap is what made the missing half look handled.
    // -----------------------------------------------------------------------

    /// Under budget is byte-identical. This is every hook anyone has today and is the
    /// whole regression surface, so it is asserted as a full-string equality rather
    /// than a `contains` — a fragment assertion would quietly certify the rest.
    #[test]
    fn cap_hook_stderr_under_budget_is_byte_identical() {
        let raw = "error: something broke\n  at line 3\n";
        assert_eq!(cap_hook_stderr(raw, 4096, 4096), raw);
        assert_eq!(cap_hook_stderr("", 4096, 4096), "");
    }

    /// The boundary, pinned on BOTH sides: a discriminator tested only on the side
    /// that fires is vacuous green.
    #[test]
    fn cap_hook_stderr_boundary_is_pinned_on_both_sides() {
        let exactly = "x".repeat(20);
        assert_eq!(
            cap_hook_stderr(&exactly, 10, 10),
            exactly,
            "exactly at budget must pass through untouched"
        );

        let one_over = "x".repeat(21);
        let capped = cap_hook_stderr(&one_over, 10, 10);
        assert_ne!(capped, one_over, "one byte over budget must be truncated");
        assert!(capped.contains("1 bytes elided"));
    }

    /// Head AND tail, not tail-only: the first error line is the actionable one and
    /// the last lines are the summary. Tail-only would throw away the line the user
    /// needs, which is why `/run` made the same choice.
    #[test]
    fn cap_hook_stderr_keeps_head_and_tail_and_marks_the_cut() {
        let raw = format!("FIRST-LINE\n{}\nLAST-LINE", "m".repeat(50_000));
        let capped = cap_hook_stderr(&raw, 4096, 4096);

        assert!(
            capped.starts_with("FIRST-LINE"),
            "the head must survive: {capped:.60}"
        );
        assert!(capped.ends_with("LAST-LINE"), "the tail must survive");
        assert!(
            capped.len() < raw.len(),
            "a 50KB stderr must not reach the conversation whole"
        );
        assert!(
            capped.len() <= 4096 + 4096 + 200,
            "capped length {} exceeds budget plus marker",
            capped.len()
        );
        // The cut is marked in band — a silent elision is the bug.
        assert!(
            capped.contains("bytes elided — hook stderr is capped at"),
            "elision must be marked in band: {capped:.200}"
        );
        // ... and the marker sits on its own line between the two halves.
        assert!(capped.contains("\n… [yoyo: "));
    }

    /// Rule #250: never slice on a raw byte offset. Both cuts land mid-character
    /// here, and a straddling char must be dropped WHOLE rather than split.
    #[test]
    fn cap_hook_stderr_never_splits_a_multibyte_char() {
        // 🐙 is FOUR bytes, so a 10-byte cut lands inside a character on both ends
        // (boundaries sit at 0, 4, 8, 12 …). Asserted as a property rather than a
        // hand-counted char count, so the test does not silently encode my own
        // arithmetic about the fixture.
        let raw = "🐙".repeat(4000);
        assert!(raw.len() > 8192);

        let capped = cap_hook_stderr(&raw, 10, 10);
        // Did not panic, and both halves are made of WHOLE octopuses.
        let (head_part, rest) = capped
            .split_once('\n')
            .expect("marker sits on its own line");
        let tail_part = rest
            .rsplit_once('\n')
            .expect("marker sits on its own line")
            .1;

        assert!(!head_part.is_empty() && !tail_part.is_empty());
        assert_eq!(
            head_part,
            "🐙".repeat(head_part.chars().count()),
            "head must be whole chars, not a split one"
        );
        assert_eq!(
            tail_part,
            "🐙".repeat(tail_part.chars().count()),
            "tail must be whole chars, not a split one"
        );
        // The straddling char is dropped WHOLE: the kept head is the largest multiple
        // of 4 at or below the 10-byte budget, i.e. 8 — never a 10-byte split.
        assert_eq!(
            head_part.len(),
            8,
            "head cut walked back to a char boundary"
        );
        assert_eq!(
            tail_part.len(),
            8,
            "tail cut walked forward to a char boundary"
        );
        assert!(capped.contains("bytes elided"));

        // And at the real budget it still holds.
        let capped = cap_hook_stderr(&raw, HOOK_STDERR_HEAD_BYTES, HOOK_STDERR_TAIL_BYTES);
        assert!(capped.contains("bytes elided"));
        assert!(capped.len() < raw.len());
    }

    /// The dropped-byte count in the marker must agree with what was actually dropped,
    /// or the in-band marker is itself a lie.
    #[test]
    fn cap_hook_stderr_marker_byte_count_is_honest() {
        let raw = "a".repeat(100);
        let capped = cap_hook_stderr(&raw, 10, 10);
        // 100 bytes in, 10 head + 10 tail kept, so 80 dropped.
        assert!(
            capped.contains("80 bytes elided"),
            "marker must state the real count: {capped}"
        );
        assert!(capped.starts_with("aaaaaaaaaa\n"));
        assert!(capped.ends_with("\naaaaaaaaaa"));
    }

    /// Source-level guard that `run_command` still routes its stderr through the cap.
    ///
    /// Deliberately WEAK, and this doc comment is the disclosure: it proves the call is
    /// *present* in that body, never that its result is used. `run_command` spawns real
    /// processes, so the real assertions live on the pure function above; this only
    /// guards against the call site being silently deleted. The needle is assembled at
    /// runtime so this test's own source cannot satisfy it.
    #[test]
    fn test_run_command_routes_stderr_through_the_cap() {
        let src = include_str!("hooks.rs");
        let body = src
            .split_once("fn run_command(")
            .expect("run_command must exist")
            .1;
        let body = body
            .split_once("\nimpl Hook for ShellHook")
            .map(|(before, _)| before)
            .unwrap_or(body);
        let needle = format!("{}{}", "cap_hook", "_stderr(");
        assert!(
            body.contains(&needle),
            "ShellHook::run_command no longer calls `{needle})`. A hook's stderr is \
             returned verbatim into the agent's context via \
             PostHookResult::with_feedback, so an unbounded one can overflow the \
             conversation and wedge the session (#844). The cap belongs at capture \
             time so every consumer of the returned pair inherits it."
        );
    }
}
