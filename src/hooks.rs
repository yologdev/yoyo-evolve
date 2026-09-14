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

/// Chars of tool output handed to a hook through `TOOL_OUTPUT` by
/// [`hook_tool_output`].
///
/// A **judgment threshold, not a measurement**: nothing measured says 1000 is
/// right. It is the historical value (`output.chars().take(1000)`) and is kept
/// verbatim so no existing hook sees a different amount of text — this change
/// is about how the cut and the channel are *reported*, never about how much is
/// sent. The unit is **chars**, matching the value it replaces; it is
/// deliberately *not* switched to bytes like [`HOOK_STDERR_HEAD_BYTES`], since
/// changing the unit would silently move the budget for every hook that exists.
const HOOK_TOOL_OUTPUT_MAX_CHARS: usize = 1000;

/// Prepare tool output for the `TOOL_OUTPUT` environment variable of a shell hook.
///
/// Two things the raw output cannot do, both properties of the **channel** rather
/// than of anything a hook author controls:
///
/// 1. **A NUL byte is unrepresentable.** `execve`'s `envp` is an array of
///    NUL-terminated strings, so a value containing `\0` cannot be passed at all;
///    Rust's `Command` converts env values to `CString` at spawn time and returns
///    `InvalidInput` ("nul byte found in provided data"). Measured: a post-hook
///    whose tool output carried a NUL (a `bash` call printing binary, a
///    `read_file` on a non-text file) failed to spawn, and `post_execute` swallows
///    a spawn error (`Err(_) => Ok(passthrough)`) — so the user's hook **silently
///    never ran**, for a reason nothing anywhere reported. NUL is therefore
///    **escaped as the four visible characters `\x00`, never deleted**: a silently
///    dropped byte is the bug, because the fact that the output contained a NUL is
///    itself information the hook may want. Only NUL is touched — newlines, tabs
///    and ESC are legal in an env var, and stripping them would change what every
///    existing hook receives.
/// 2. **A silent truncation is a confident wrong answer.** The cut is marked in
///    band, in the same vocabulary [`cap_hook_stderr`] uses, so a hook grepping
///    `$TOOL_OUTPUT` can tell *truncated* from *absent*. This was the last unmarked
///    elision in the repo; `/run`'s `CappedCapture`, `/bg`, `AST_MAX_OUTPUT_LINES`,
///    `truncate_long_line` and `cap_hook_stderr` itself all mark theirs.
///
/// **Escape-then-cap is load-bearing, not stylistic.** Escaping *lengthens* the
/// string, so capping first would let an escaped tail push past the budget and
/// would make the reported dropped count wrong — a marker that lies is worse than
/// no marker. Both numbers are measured over the escaped string, which is the
/// string actually being cut.
///
/// At or under budget with no NUL the input is returned **byte-identical**, which
/// is every hook anyone has today.
fn hook_tool_output(output: &str, max_chars: usize) -> String {
    // Escape first: this is the string that will actually be cut.
    let escaped = if output.contains('\0') {
        output.replace('\0', "\\x00")
    } else {
        output.to_string()
    };

    let total = escaped.chars().count();
    if total <= max_chars {
        return escaped;
    }

    let kept: String = escaped.chars().take(max_chars).collect();
    let dropped = total - max_chars;
    format!(
        "{kept}\n… [yoyo: {dropped} chars elided — hook TOOL_OUTPUT is capped at {max_chars} chars]"
    )
}

/// Bytes of the JSON payload written to a hook's **stdin** by
/// [`hook_stdin_payload`].
///
/// This is a **judgment threshold, not a measurement**, and the reason it
/// exists at all is a deadlock rather than a taste for round numbers.
/// `ShellHook::run_command` waits with a `try_wait` poll loop, so a blocking
/// `write_all` on a full pipe hangs **before** the 5-second timeout can ever
/// fire — the timeout only runs once we are back in the loop. Linux's default
/// pipe buffer is 64 KiB, so the payload is bounded *well under* that and a
/// blocking write is impossible by construction. 32 KiB is 32× the env
/// channel's [`HOOK_TOOL_OUTPUT_MAX_CHARS`] budget and provably cannot block;
/// it is **not** a measured right answer, and nothing says a hook wanting more
/// is wrong.
const HOOK_STDIN_MAX_BYTES: usize = 32 * 1024;

/// Build the JSON payload handed to a shell hook on **stdin**.
///
/// This is a *second* channel beside the `TOOL_NAME` / `TOOL_PARAMS` /
/// `TOOL_OUTPUT` environment variables, never a replacement: every one of those
/// is still set, with byte-identical values, so a hook reading only `$TOOL_OUTPUT`
/// is completely unaffected. Two properties the env channel structurally cannot
/// have:
///
///   * **A NUL byte is representable.** `execve`'s `envp` is an array of
///     NUL-terminated strings, so a NUL in a value makes `Command::spawn` fail
///     outright — which is why [`hook_tool_output`] has to escape it as the four
///     visible characters `\x00`. JSON escapes NUL as `\u0000` by construction,
///     so this channel carries the real byte and a hook gets it back verbatim
///     from its own JSON parser.
///   * **Room.** The env budget is 1000 *chars*; this one is
///     [`HOOK_STDIN_MAX_BYTES`], so a post-hook that wants more than a
///     thousand characters of tool output has somewhere to read it.
///
/// `tool_params` goes in as the **`Value`**, not as a re-stringified blob, so a
/// hook can address one field of it without parsing a string inside a string.
///
/// `output` is an `Option` and **absence is a distinct fact from emptiness**: a
/// *pre*-hook runs before the tool does, so there is no output at all, and the
/// `tool_output` key is **omitted entirely** rather than being set to `""`,
/// which would read as *the tool produced nothing*. This deliberately does not
/// widen what a pre-hook sees — it is the same two facts it already had, in the
/// other channel's vocabulary.
///
/// The emitted string is **always valid JSON**, which is the whole value of the
/// channel, so the cut can never be a raw slice of the serialized bytes. When
/// the payload is over budget the `tool_output` *value* is trimmed on a `char`
/// boundary (rule #250 — a straddling character is dropped whole) and the cut is
/// **marked in band** in [`cap_hook_stderr`]'s vocabulary, because a silent
/// elision is the bug. The kept length is chosen so the **whole re-serialized
/// object**, marker included, fits the budget — the budget is about bytes on a
/// pipe, and JSON escaping *lengthens* a string, so it has to be measured on the
/// string actually being written.
///
/// Under budget the payload is returned untouched, which is every hook anyone
/// has today.
fn hook_stdin_payload(
    tool_name: &str,
    params: &serde_json::Value,
    output: Option<&str>,
    max_bytes: usize,
) -> String {
    fn assemble(tool_name: &str, params: &serde_json::Value, output: Option<&str>) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "tool_name".to_string(),
            serde_json::Value::String(tool_name.to_string()),
        );
        obj.insert("tool_params".to_string(), params.clone());
        if let Some(out) = output {
            obj.insert(
                "tool_output".to_string(),
                serde_json::Value::String(out.to_string()),
            );
        }
        serde_json::Value::Object(obj).to_string()
    }

    let full = assemble(tool_name, params, output);
    if full.len() <= max_bytes {
        return full;
    }

    // Over budget: trim the one unbounded *string* field we own, keeping the
    // object parseable. Binary search is safe because serialized length is
    // monotone in the number of chars kept.
    if let Some(out) = output {
        let chars: Vec<char> = out.chars().collect();
        let total = chars.len();
        let mut lo = 0usize;
        let mut hi = total;
        let mut best: Option<String> = None;
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let kept: String = chars[..mid].iter().collect();
            let dropped = total - mid;
            let marked = format!(
                "{kept}\n… [yoyo: {dropped} chars elided — hook stdin payload is capped at {max_bytes} bytes]"
            );
            let candidate = assemble(tool_name, params, Some(&marked));
            if candidate.len() <= max_bytes {
                best = Some(candidate);
                lo = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                hi = mid - 1;
            }
        }
        if let Some(payload) = best {
            return payload;
        }
    }

    // Even an empty `tool_output` does not fit, so `tool_params` itself is over
    // budget — routinely true of `write_file`, whose params carry a whole file.
    // Degrade to a minimal object that still parses and still says what
    // happened, rather than writing a truncated fragment that no parser accepts.
    let name: String = tool_name.chars().take(200).collect();
    let mut obj = serde_json::Map::new();
    obj.insert("tool_name".to_string(), serde_json::Value::String(name));
    obj.insert(
        "payload_elided".to_string(),
        serde_json::Value::String(format!(
            "… [yoyo: payload elided — the hook stdin payload was {} bytes, over the {max_bytes} byte cap; read the environment variables instead]",
            full.len()
        )),
    );
    let fallback = serde_json::Value::Object(obj).to_string();
    if fallback.len() <= max_bytes {
        return fallback;
    }
    // Floor. A budget too small to hold even the explanation is absurd in
    // production — [`HOOK_STDIN_MAX_BYTES`] is 32 KiB against a ~150-byte
    // fallback — but "fits the budget" has to hold for *every* input or the
    // deadlock argument the budget exists for is not an argument at all.
    // The empty object is the smallest thing a hook's parser still accepts;
    // writing a truncated fragment instead would hand it bytes no parser reads.
    "{}".to_string()
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
    fn run_command(
        &self,
        env_vars: &[(&str, &str)],
        stdin_payload: &str,
    ) -> Result<(i32, String), String> {
        use std::io::{Read, Write};
        use std::process::Command;
        use std::time::Duration;

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&self.command);
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        // Spawn and wait with timeout.
        //
        // stdin is **piped**, and that is a fix as well as a feature. An unset
        // stdin means `Stdio::inherit()` for `spawn()`, so until this landed a
        // hook inherited *yoyo's own* stdin — measured, not inferred: a hook
        // running `head -c 20 /dev/stdin` read the parent's bytes and consumed
        // them. Under `yoyo -p` that stream is the prompt itself, so a hook
        // that so much as read stdin could eat it. Piping it makes the channel
        // ours to fill and closes that inheritance by construction.
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn hook command: {e}"))?;

        // Hand over the payload and **drop the handle**, which closes the pipe
        // and sends EOF — a hook that never reads stdin is unaffected, and one
        // that does is not left blocking on a write end nobody will close.
        // A failed write must not kill the hook: reading stdin is optional, the
        // env vars are still set, and the common case is a hook that ignores
        // this channel entirely.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(stdin_payload.as_bytes());
        }

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
                        // Reap it (#892). Without this the child stays a zombie
                        // until yoyo exits, so a hook that reliably hangs leaks
                        // one PID per tool call for the whole session. `wait()`
                        // after a successful `kill()` returns promptly because
                        // SIGKILL is uncatchable, and both results stay
                        // discarded: the error being returned is the TIMEOUT,
                        // not the reap.
                        let _ = child.wait();
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
        // `None`, never `Some("")`: a pre-hook runs *before* the tool, so there
        // is no output — and an empty string would read as "the tool produced
        // nothing", which is a different fact. The key is omitted entirely.
        let stdin_payload = hook_stdin_payload(tool_name, params, None, HOOK_STDIN_MAX_BYTES);

        match self.run_command(&env_vars, &stdin_payload) {
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
        // Cap and escape for the env-var channel: marks its own cut and makes a
        // NUL representable, so a binary-carrying tool output can no longer make
        // the spawn fail and silently skip the user's hook. See `hook_tool_output`.
        let truncated_output = hook_tool_output(output, HOOK_TOOL_OUTPUT_MAX_CHARS);
        let env_vars = vec![
            ("TOOL_NAME", tool_name),
            ("TOOL_PARAMS", params_str.as_str()),
            ("TOOL_OUTPUT", truncated_output.as_str()),
        ];

        // Post-hooks pass through original output; stderr becomes feedback.
        // The stdin payload carries the **raw** output, not the env-truncated
        // one: a 1000-char cap is a property of the env channel, and handing
        // the same truncation to a channel that can hold 32× more would throw
        // away the whole point of offering it.
        let stdin_payload =
            hook_stdin_payload(tool_name, params, Some(output), HOOK_STDIN_MAX_BYTES);
        match self.run_command(&env_vars, &stdin_payload) {
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

/// Edit-distance budget for suggesting a builtin whose name a hook key may have
/// mistyped. Deliberately **tight**: this is a judgment threshold, not a
/// measurement, and a confidently wrong suggestion in an advisory warning is
/// worse than none. Ordinary typos (`bahs` -> `bash`) sit at 1-2; anything
/// further away is more likely a genuine MCP tool name than a slip.
const NEAREST_BUILTIN_MAX_DISTANCE: usize = 2;

/// Pure: the builtin a hook's `tool_pattern` most plausibly meant, or `None`.
///
/// Two rules, in order, because neither alone covers the shapes that occur.
/// 1. A **dropped suffix** (`write` for `write_file`) is the shape #892 was
///    filed on, and edit distance cannot see it — `write` -> `write_file` is
///    five insertions, a budget loose enough to suggest nonsense for unrelated
///    names. The prefix form is unambiguous, so it is checked first.
/// 2. Otherwise a **tight** edit distance for ordinary typos, reusing
///    [`crate::commands::closest_match`] rather than growing a second distance
///    function (the `significant_braces` precedent: two copies of a rule agree
///    the day they are written and diverge forever after).
///
/// Only ever called for a pattern that is *not* in `known`, so `closest_match`'s
/// distance-0 case is unreachable here.
fn nearest_builtin<'a>(pattern: &str, known: &[&'a str]) -> Option<&'a str> {
    if pattern.is_empty() {
        return None;
    }
    if let Some(hit) = known
        .iter()
        .find(|k| k.strip_prefix(pattern).is_some_and(|r| r.starts_with('_')))
    {
        return Some(hit);
    }
    crate::commands::closest_match(pattern, known, NEAREST_BUILTIN_MAX_DISTANCE)
}

/// Pure: warn when a hook key names a tool that no builtin provides.
///
/// `hooks.pre.write` (a typo for `write_file`) builds a hook whose
/// `matches_tool` can **never** be true, and a hook that never fires emits no
/// output and no error — the quietest failure available, indistinguishable from
/// a hook that ran and did nothing (#892). The config-key vocabulary and the
/// tool-name vocabulary are two enumerations with nothing tying them together;
/// this is the tie.
///
/// **It warns, it never refuses**, and the reason is written here rather than
/// only in the docs: a user may legitimately hook an **MCP-provided** tool,
/// whose name is not a builtin and cannot be known at parse time, so refusing
/// would break working configs. That is also why the wording is a *question,
/// not an accusation* — a warning that cries wolf on a legitimate config every
/// session is how a reader learns to paste past a gate.
///
/// `None` for `*` and for every name in `known`; `Some(msg)` otherwise.
/// Glyph-free under `plain` (marker **and** em dash).
fn unknown_hook_tool_warning(tool_pattern: &str, known: &[&str], plain: bool) -> Option<String> {
    if tool_pattern == "*" || known.contains(&tool_pattern) {
        return None;
    }

    let marker = if plain { "warning:" } else { "⚠" };
    let joiner = if plain { ";" } else { " —" };
    let mut msg = format!(
        "{marker} hook `{tool_pattern}`: no builtin tool has that name. That is \
         fine if `{tool_pattern}` comes from an MCP server{joiner} otherwise this \
         hook can never fire."
    );
    if let Some(nearest) = nearest_builtin(tool_pattern, known) {
        msg.push_str(&format!(" Did you mean `{nearest}`?"));
    }
    Some(msg)
}

/// Pure: warn when a `hooks.*` key names a phase but no tool, so it can never
/// fire (#892's third arm).
///
/// `rest` is the key with its `hooks.` prefix already stripped — exactly what
/// `parse_hooks_from_config`'s phase split reads. A key of `hooks.pre` matches
/// neither `pre.` nor `post.`, falls to a bare `continue`, and used to emit
/// nothing at all. That is the quietest failure in this file: the hook never
/// fires, prints nothing, and a user cannot tell it from a hook that ran and
/// did nothing.
///
/// WARN, never refuse. Refusing would break a config that starts a session
/// today, which is the same call the unreachable-tool-name arm above made and
/// it applies verbatim here.
///
/// Deliberately NARROW, and the reason is the cry-wolf direction rather than
/// tidiness: it fires only when the phase is spelled correctly (`pre` /
/// `post`) and the tool segment is missing, which is unambiguous. An
/// unrecognised segment — `hooks.preflight`, or some future non-hook
/// `hooks.timeout` — reaches the same `continue` and stays **silent**, because
/// this cannot tell a typo'd phase from a config key it does not know about,
/// and a warning that cries wolf on a legitimate config is how a reader learns
/// to paste past a gate (the same reason `tests/doc_version_claims.rs` refuses
/// a blanket wording scan).
///
/// STATED LIMIT: this makes a malformed key AUDIBLE, it does not make it WORK.
/// The hook still never fires, the warning drops nothing and retries nothing,
/// and a *well-formed* key that names the wrong tool passes in complete silence
/// — `hooks.pre.read_file` written when `write_file` was meant is invisible
/// here. Presence of a vocabulary match is mechanically checkable; intent is
/// not, which is the same limit `tests/blind_round_grades.rs` states about
/// grades. `hooks.pre.` (a trailing dot, so an *empty* tool segment) reaches a
/// different `continue` one branch down and is likewise out of scope.
///
/// Glyph-free under `plain` (marker **and** em dash).
fn malformed_hook_key_warning(rest: &str, plain: bool) -> Option<String> {
    if rest != "pre" && rest != "post" {
        return None;
    }

    let marker = if plain { "warning:" } else { "⚠" };
    let joiner = if plain { ";" } else { " —" };
    Some(format!(
        "{marker} hook key `hooks.{rest}` has no tool segment, so it names no \
         tool and can never fire{joiner} write `hooks.{rest}.<tool> = \"...\"` \
         instead, e.g. `hooks.{rest}.bash`."
    ))
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
            // #892's third arm: a key with no tool segment (`hooks.pre = "..."`)
            // matches neither prefix and used to fall to this bare `continue` in
            // complete silence. Say so. WARN, never refuse — see the function's
            // doc comment for why, and for why an unrecognised phase stays
            // quiet. Same shape as the unreachable-tool-name warning below: a
            // pure decision, with the two global reads at this call site.
            if let Some(warning) =
                malformed_hook_key_warning(rest, crate::format::is_plain_output())
            {
                if !crate::format::is_quiet() {
                    eprintln!("{warning}");
                }
            }
            continue; // Invalid format, skip
        };

        if tool_pattern.is_empty() || value.is_empty() {
            continue; // Skip empty patterns or commands
        }

        // #892: a key naming a tool that does not exist parses fine and can
        // never match, so say so. WARN, never refuse — an MCP-provided tool is
        // a legitimate target whose name cannot be known here. The decision is
        // pure and table-tested; only the two global reads live at this call
        // site (the `rtk_announcement` shape).
        if let Some(warning) = unknown_hook_tool_warning(
            tool_pattern,
            crate::agent_builder::BUILTIN_TOOL_NAMES,
            crate::format::is_plain_output(),
        ) {
            if !crate::format::is_quiet() {
                eprintln!("{warning}");
            }
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
    // --- Round 93 (#844's missing twin): the TOOL_OUTPUT channel ---------------
    //
    // Two defects with one mechanism — the value handed to a hook through the
    // `TOOL_OUTPUT` env var was never prepared for the env-var *channel*:
    //   * a NUL byte is unrepresentable in `execve` envp, so `Command::spawn`
    //     returned `InvalidInput` and `post_execute`'s `Err(_) => Ok(passthrough)`
    //     swallowed it — the user's hook silently never ran;
    //   * the truncation was silent, so a hook grepping `$TOOL_OUTPUT` could not
    //     tell *truncated* from *absent*.
    // Pinned at the emission point below (`ShellHook::post_execute`'s env value)
    // via the pure function that produces it; `run_command` spawns real processes
    // and is not driven by any test.

    /// NEAR-MISS GUARD, and the whole regression surface: ordinary output that is
    /// within budget and carries no NUL must be returned **byte-identical**.
    /// Asserted with a whole-string `assert_eq!` rather than a `contains`, because
    /// a discriminator tested only on the side that fires is vacuous green — this
    /// is what every hook anyone has today receives.
    #[test]
    fn hook_tool_output_ordinary_output_is_byte_identical() {
        for raw in [
            "",
            "ok",
            "line one\nline two\ttabbed\r\n",
            "unicode: 🐙 näme ✓",
            &"x".repeat(HOOK_TOOL_OUTPUT_MAX_CHARS),
        ] {
            assert_eq!(
                hook_tool_output(raw, HOOK_TOOL_OUTPUT_MAX_CHARS),
                raw,
                "in-budget NUL-free output must pass through untouched"
            );
        }
    }

    /// The boundary is pinned on BOTH sides: exactly at the cap is untouched, one
    /// char over is marked. Testing only the firing side would leave the
    /// pass-through unguarded.
    #[test]
    fn hook_tool_output_boundary_is_pinned_on_both_sides() {
        let at = "a".repeat(HOOK_TOOL_OUTPUT_MAX_CHARS);
        assert_eq!(hook_tool_output(&at, HOOK_TOOL_OUTPUT_MAX_CHARS), at);

        let over = "a".repeat(HOOK_TOOL_OUTPUT_MAX_CHARS + 1);
        let got = hook_tool_output(&over, HOOK_TOOL_OUTPUT_MAX_CHARS);
        assert_ne!(got, over, "one char over budget must be truncated");
        assert!(
            got.contains("1 chars elided"),
            "the cut must be marked in band, got: {got}"
        );
    }

    /// The marker must not lie: the reported dropped count has to agree with what
    /// was actually dropped, and the kept prefix must be the real prefix.
    #[test]
    fn hook_tool_output_marker_reports_the_real_dropped_count() {
        let raw = "b".repeat(HOOK_TOOL_OUTPUT_MAX_CHARS + 137);
        let got = hook_tool_output(&raw, HOOK_TOOL_OUTPUT_MAX_CHARS);
        assert!(
            got.contains("137 chars elided"),
            "dropped count must match reality, got: {got}"
        );
        assert!(
            got.starts_with(&"b".repeat(HOOK_TOOL_OUTPUT_MAX_CHARS)),
            "the kept half must be the real prefix"
        );
        assert!(
            got.contains(&format!("capped at {HOOK_TOOL_OUTPUT_MAX_CHARS} chars")),
            "the marker must name the budget it enforced"
        );
    }

    /// A NUL is ESCAPED, never deleted — and the result carries no NUL, which is
    /// the property that makes the spawn succeed at all. Verified against the real
    /// channel before this landed: `Command::env` with a NUL-carrying value returns
    /// `InvalidInput: nul byte found in provided data`, while the identical command
    /// without the NUL spawns fine.
    #[test]
    fn hook_tool_output_escapes_nul_rather_than_dropping_it() {
        let raw = "before\u{0}after";
        assert!(raw.contains('\0'), "fixture must really carry a NUL");

        let got = hook_tool_output(raw, HOOK_TOOL_OUTPUT_MAX_CHARS);
        assert_eq!(got, "before\\x00after");
        assert!(
            !got.contains('\0'),
            "no NUL may survive — that is what unblocks Command::spawn"
        );
        assert!(
            got.contains("\\x00"),
            "the NUL must stay visible: a silently dropped byte is the bug"
        );
    }

    /// ESCAPE-THEN-CAP is load-bearing: escaping lengthens the string, so capping
    /// first would let an escaped tail push past the budget AND would make the
    /// reported dropped count wrong. Both numbers are measured over the escaped
    /// string, which is the string actually being cut.
    #[test]
    fn hook_tool_output_escapes_before_capping_so_the_count_is_honest() {
        // 999 filler chars + one NUL = 1000 chars raw, but 1003 chars escaped.
        let raw = format!("{}\u{0}", "c".repeat(HOOK_TOOL_OUTPUT_MAX_CHARS - 1));
        assert_eq!(raw.chars().count(), HOOK_TOOL_OUTPUT_MAX_CHARS);

        let got = hook_tool_output(&raw, HOOK_TOOL_OUTPUT_MAX_CHARS);
        assert!(!got.contains('\0'));
        // Escaped length is 1003, so exactly 3 chars fall past the budget.
        assert!(
            got.contains("3 chars elided"),
            "the count must be measured over the ESCAPED string, got tail: {}",
            &got[got.len().saturating_sub(80)..]
        );
    }

    // --- #892 defect 1: an unreachable hook key is now audible ----------------

    /// ANTI-VACUOUS, and asserted FIRST: a warner that fires on nothing and a
    /// warner that fires on everything are the same bug wearing opposite signs.
    /// So pin that the vocabulary is non-empty and that a genuinely unknown
    /// pattern really does produce a warning, before asserting any silence.
    #[test]
    fn unknown_hook_tool_warning_is_anti_vacuous() {
        let known = crate::agent_builder::BUILTIN_TOOL_NAMES;
        assert!(
            !known.is_empty(),
            "BUILTIN_TOOL_NAMES is empty, so every silence below would be vacuous"
        );
        assert!(
            unknown_hook_tool_warning("definitely_not_a_builtin", known, false).is_some(),
            "an unknown pattern must warn, or this guard can never fire"
        );
    }

    /// NEAR-MISS GUARD, and the entire regression surface: `*` and every real
    /// builtin must stay silent. The list is ITERATED from the authority rather
    /// than hand-typed — a second copy of the tool vocabulary is the very defect
    /// being fixed, one layer up.
    #[test]
    fn unknown_hook_tool_warning_is_silent_for_the_wildcard_and_every_builtin() {
        let known = crate::agent_builder::BUILTIN_TOOL_NAMES;
        for plain in [false, true] {
            assert_eq!(
                unknown_hook_tool_warning("*", known, plain),
                None,
                "the wildcard matches every tool and must never warn"
            );
            for name in known {
                assert_eq!(
                    unknown_hook_tool_warning(name, known, plain),
                    None,
                    "`{name}` is a real builtin and must never warn"
                );
            }
        }
    }

    /// The wording is the load-bearing half: a warning that cries wolf on a
    /// legitimate MCP config every session is how a reader learns to paste past
    /// a gate. So it must be a QUESTION, not an accusation — naming the pattern,
    /// naming MCP as the legitimate case, and naming the consequence.
    #[test]
    fn unknown_hook_tool_warning_asks_rather_than_accuses() {
        let known = crate::agent_builder::BUILTIN_TOOL_NAMES;
        let msg = unknown_hook_tool_warning("write", known, false).expect("must warn");
        assert!(msg.contains("write"), "must name the pattern: {msg}");
        assert!(
            msg.contains("MCP"),
            "must name the legitimate MCP case, or it is an accusation: {msg}"
        );
        assert!(
            msg.contains("can never fire"),
            "must name the consequence: {msg}"
        );
        // The issue's own example: a dropped suffix, which edit distance alone
        // cannot see (`write` -> `write_file` is five insertions).
        assert!(
            msg.contains("Did you mean `write_file`?"),
            "must suggest the dropped-suffix builtin: {msg}"
        );
    }

    /// Glyph-free under plain output — marker AND em dash, since asserting only
    /// the marker is the half an assertion has caught before.
    #[test]
    fn unknown_hook_tool_warning_is_glyph_free_when_plain() {
        let known = crate::agent_builder::BUILTIN_TOOL_NAMES;
        let plain = unknown_hook_tool_warning("write", known, true).expect("must warn");
        assert!(!plain.contains('⚠'), "no marker glyph in plain: {plain}");
        assert!(!plain.contains('—'), "no em dash in plain: {plain}");
        // Anti-vacuous the other way: the non-plain form really does carry both,
        // so the assertions above discriminate rather than passing by accident.
        let fancy = unknown_hook_tool_warning("write", known, false).expect("must warn");
        assert!(fancy.contains('⚠') && fancy.contains('—'));
    }

    /// `nearest_builtin`'s two rules, and the case where it must stay quiet: a
    /// far-away name is far more likely a real MCP tool than a slip, and a
    /// confidently wrong suggestion is worse than none.
    #[test]
    fn nearest_builtin_table() {
        let known = crate::agent_builder::BUILTIN_TOOL_NAMES;
        for (pattern, want) in [
            ("write", Some("write_file")), // dropped suffix
            ("read", Some("read_file")),   // dropped suffix
            ("list", Some("list_files")),  // dropped suffix
            ("bahs", Some("bash")),        // ordinary typo, distance 2
            ("serch", Some("search")),     // ordinary typo, distance 1
            ("github_create_issue", None), // a real MCP name, no suggestion
            ("", None),                    // guarded: empty suggests nothing
        ] {
            assert_eq!(
                nearest_builtin(pattern, known),
                want,
                "nearest_builtin({pattern:?})"
            );
        }
    }

    // --- #892 defect 3: a malformed hook key is now audible ------------------

    /// ANTI-VACUOUS, and asserted FIRST: a warner that fires on nothing and one
    /// that fires on everything are the same bug wearing opposite signs, so
    /// pin that a genuinely malformed key really does warn and a genuinely
    /// valid one really does not, before asserting any silence below.
    #[test]
    fn malformed_hook_key_warning_is_anti_vacuous() {
        assert!(
            malformed_hook_key_warning("pre", false).is_some(),
            "`hooks.pre` names no tool and must warn, or this guard can never fire"
        );
        assert!(
            malformed_hook_key_warning("post", false).is_some(),
            "`hooks.post` names no tool and must warn"
        );
        assert!(
            malformed_hook_key_warning("pre.bash", false).is_none(),
            "a well-formed key must not warn, or every silence below is vacuous"
        );
    }

    /// NEAR-MISS GUARD, and the entire regression surface: every key shape that
    /// parses today must stay silent, for BOTH `plain` values. A discriminator
    /// tested only on the side that fires is vacuous green, and over-firing
    /// here would print a warning on a working config every session.
    ///
    /// The shapes are derived from the real builtin list rather than
    /// hand-typed, so a new builtin is covered without editing this test.
    #[test]
    fn malformed_hook_key_warning_is_silent_for_every_key_that_parses() {
        let known = crate::agent_builder::BUILTIN_TOOL_NAMES;
        assert!(
            !known.is_empty(),
            "empty builtin list would make this vacuous"
        );

        for plain in [false, true] {
            for phase in ["pre", "post"] {
                // The wildcard, and every real builtin, under both phases.
                for tool in std::iter::once(&"*").chain(known.iter()) {
                    let rest = format!("{phase}.{tool}");
                    assert_eq!(
                        malformed_hook_key_warning(&rest, plain),
                        None,
                        "`hooks.{rest}` parses today and must never warn"
                    );
                }
                // The unknown-tool case is Day 192's warning to own, not this
                // one — it has a tool segment, so it is a different question.
                let rest = format!("{phase}.github_create_issue");
                assert_eq!(
                    malformed_hook_key_warning(&rest, plain),
                    None,
                    "`hooks.{rest}` has a tool segment; the unknown-tool arm owns it"
                );
            }
            // An unrecognised phase reaches the same `continue` and stays
            // SILENT on purpose: indistinguishable from a future non-hook
            // `hooks.*` config key, so warning would cry wolf.
            for rest in ["preflight", "", "timeout", "posts", "Pre"] {
                assert_eq!(
                    malformed_hook_key_warning(rest, plain),
                    None,
                    "`hooks.{rest}` names no known phase and must stay silent"
                );
            }
        }
    }

    /// The wording is the load-bearing half: an observation, never an
    /// accusation. It must name the offending key, state the consequence, and
    /// name the shape that actually works — a warning a reader cannot act on
    /// teaches them to paste past it.
    #[test]
    fn malformed_hook_key_warning_names_the_key_and_the_shape_that_works() {
        for phase in ["pre", "post"] {
            let msg = malformed_hook_key_warning(phase, false).expect("must warn");
            assert!(
                msg.contains(&format!("`hooks.{phase}`")),
                "must name the offending key: {msg}"
            );
            assert!(
                msg.contains("no tool segment"),
                "must say what is wrong: {msg}"
            );
            assert!(
                msg.contains("can never fire"),
                "must name the consequence: {msg}"
            );
            assert!(
                msg.contains(&format!("`hooks.{phase}.<tool>")),
                "must name the shape that works: {msg}"
            );
            assert!(
                msg.contains(&format!("`hooks.{phase}.bash`")),
                "must give a concrete example: {msg}"
            );
        }
    }

    /// Glyph-free under plain output — marker AND em dash, since asserting only
    /// the marker is the half an assertion has caught before. The REVERSE
    /// anti-vacuous check is what makes those two assertions discriminate
    /// rather than pass by accident.
    #[test]
    fn malformed_hook_key_warning_is_glyph_free_when_plain() {
        let plain = malformed_hook_key_warning("pre", true).expect("must warn");
        assert!(!plain.contains('⚠'), "no marker glyph in plain: {plain}");
        assert!(!plain.contains('—'), "no em dash in plain: {plain}");

        let fancy = malformed_hook_key_warning("pre", false).expect("must warn");
        assert!(
            fancy.contains('⚠') && fancy.contains('—'),
            "the non-plain form must carry both, or the assertions above are vacuous: {fancy}"
        );
    }

    /// The two #892 warnings must not collide: they answer different questions
    /// and carry different remedies, so neither may wear the other's name.
    ///
    /// A malformed key has no tool pattern at all, so it can never reach
    /// `unknown_hook_tool_warning` — the `continue` fires first. What is
    /// asserted here is the structural half: the two sentences are distinct and
    /// each names only its own remedy.
    #[test]
    fn the_two_hook_warnings_are_distinct_sentences_with_distinct_remedies() {
        let known = crate::agent_builder::BUILTIN_TOOL_NAMES;
        let malformed = malformed_hook_key_warning("pre", false).expect("must warn");
        let unknown_tool = unknown_hook_tool_warning("write", known, false).expect("must warn");

        assert_ne!(malformed, unknown_tool, "the two warnings must differ");
        assert!(
            malformed.contains("no tool segment") && !malformed.contains("MCP"),
            "the malformed-key warning must not borrow the MCP remedy: {malformed}"
        );
        assert!(
            unknown_tool.contains("MCP") && !unknown_tool.contains("no tool segment"),
            "the unknown-tool warning must not borrow the missing-segment remedy: {unknown_tool}"
        );
    }

    /// BEHAVIOUR is unchanged, which is the other half of the regression
    /// surface: the warning is advisory and drops nothing, so a malformed key
    /// still yields no hook and every valid key beside it still parses.
    #[test]
    fn a_malformed_key_still_yields_no_hook_and_does_not_eat_its_neighbours() {
        let mut config = HashMap::new();
        config.insert("hooks.pre".to_string(), "echo malformed".to_string());
        config.insert("hooks.post".to_string(), "echo malformed too".to_string());
        config.insert("hooks.pre.bash".to_string(), "echo valid".to_string());

        let hooks = parse_hooks_from_config(&config);

        assert_eq!(
            hooks.len(),
            1,
            "the two malformed keys must yield no hook, and must not drop the valid one"
        );
        assert_eq!(hooks[0].name, "pre:bash");
        assert_eq!(hooks[0].command, "echo valid");
    }

    // --- #892 defect 2: a timed-out hook is reaped ---------------------------

    /// `run_command` spawns real processes and is driven by no test, so this is
    /// a deliberately WEAK source-level guard, and its limit is stated rather
    /// than implied: it proves the reap is POSITIONED after the kill inside the
    /// timeout branch, never that any process was actually reaped. Needles are
    /// assembled at runtime so this test's own source cannot satisfy it.
    #[test]
    fn test_run_command_reaps_the_child_it_kills_on_timeout() {
        let src = include_str!("hooks.rs");
        let body = src
            .split_once("fn run_command(")
            .expect("run_command must exist")
            .1;
        let body = body
            .split_once("\nimpl Hook for ShellHook")
            .map(|(before, _)| before)
            .unwrap_or(body);

        let kill = format!("child.{}()", "kill");
        let wait = format!("child.{}()", "wait");
        let kill_at = body
            .find(&kill)
            .unwrap_or_else(|| panic!("run_command must still call `{kill}` on timeout"));
        let wait_at = body.find(&wait).unwrap_or_else(|| {
            panic!(
                "run_command kills the child on timeout but never calls `{wait}`, so it \
                 stays a zombie until yoyo exits — a hook that reliably hangs leaks one \
                 PID per tool call for the whole session (#892)."
            )
        });
        assert!(
            kill_at < wait_at,
            "the reap must come AFTER the kill: waiting on a live hung child would \
             block for as long as the hook hangs, which is the opposite of the fix."
        );
    }

    // --- Day 197: the stdin channel ------------------------------------------
    //
    // Hook payloads reached a hook only as environment variables, and BOTH of
    // Day 188's repairs were channel properties wearing payload clothes: the
    // 1000-char cap exists because env is small, and the NUL escape exists
    // because `execve`'s envp cannot carry one. A rival moved payloads to
    // stdin; this adds that channel beside the env vars rather than instead of
    // them. Every assertion below is on the string a hook actually receives.

    /// The whole regression surface: an ordinary payload is returned untouched
    /// and parses. Every hook that exists today reads the env vars, so what has
    /// to stay true is that adding this channel changes nothing about them —
    /// and that the new one is real JSON rather than something JSON-shaped.
    #[test]
    fn hook_stdin_payload_ordinary_shapes_are_untouched_and_parse() {
        let params = serde_json::json!({"path": "src/main.rs", "n": 3});
        for (label, out) in [
            ("empty", ""),
            ("ascii", "all good"),
            ("crlf and tabs", "a\r\n\tb\r\n"),
            ("multi-byte", "caf\u{e9} \u{1F419} na\u{ef}ve"),
        ] {
            let got = hook_stdin_payload("bash", &params, Some(out), HOOK_STDIN_MAX_BYTES);

            // ANTI-VACUOUS, asserted first: the fixture is genuinely in budget,
            // so "untouched" is a real pass rather than a cap that never fired.
            assert!(
                got.len() <= HOOK_STDIN_MAX_BYTES,
                "{label}: fixture must be in budget for this to test the pass-through"
            );

            let parsed: serde_json::Value = serde_json::from_str(&got)
                .unwrap_or_else(|e| panic!("{label}: payload must be valid JSON: {e}"));
            assert_eq!(parsed["tool_name"], serde_json::json!("bash"), "{label}");
            assert_eq!(
                parsed["tool_params"], params,
                "{label}: params go in as a Value"
            );
            assert_eq!(
                parsed["tool_output"],
                serde_json::json!(out),
                "{label}: an in-budget output must arrive byte-identically"
            );
            assert!(
                !got.contains("elided"),
                "{label}: nothing was dropped, so nothing may claim it was"
            );
        }
    }

    /// A NUL round-trips. This is the property the env channel structurally
    /// cannot have: `execve`'s envp is NUL-terminated strings, so
    /// `hook_tool_output` has to escape it as the four visible characters
    /// `\x00` and a hook can never recover the byte. JSON escapes it as
    /// `\u0000`, so a hook's own parser hands back the real thing.
    #[test]
    fn hook_stdin_payload_carries_a_nul_the_env_channel_cannot() {
        let raw = "before\u{0}after";

        // ANTI-VACUOUS: the fixture really carries a NUL, so a transcription
        // slip cannot make this pass by both sides agreeing on nothing.
        assert!(raw.contains('\u{0}'), "fixture must genuinely carry a NUL");

        let got = hook_stdin_payload(
            "bash",
            &serde_json::json!({}),
            Some(raw),
            HOOK_STDIN_MAX_BYTES,
        );

        // Escaped exactly once by serde, not hand-assembled with `format!`.
        assert!(
            got.contains("\\u0000"),
            "serde must escape the NUL as \\u0000, got: {got}"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&got).expect("a NUL-carrying payload must still be valid JSON");
        assert_eq!(
            parsed["tool_output"].as_str(),
            Some(raw),
            "the NUL must round-trip as the real byte"
        );

        // And the contrast that makes the point: the env channel cannot.
        assert_eq!(
            hook_tool_output(raw, HOOK_TOOL_OUTPUT_MAX_CHARS),
            "before\\x00after",
            "the env channel escapes it to visible characters because it must"
        );
    }

    /// `None` is not `Some("")`. A pre-hook runs before the tool, so the key is
    /// absent rather than empty — absence gets its own name.
    #[test]
    fn hook_stdin_payload_omits_tool_output_for_a_pre_hook() {
        let params = serde_json::json!({"command": "ls"});
        let pre = hook_stdin_payload("bash", &params, None, HOOK_STDIN_MAX_BYTES);
        let parsed: serde_json::Value = serde_json::from_str(&pre).expect("valid JSON");

        assert!(
            parsed.get("tool_output").is_none(),
            "a pre-hook has no output, so the key must be ABSENT, not empty: {pre}"
        );
        assert_eq!(parsed["tool_name"], serde_json::json!("bash"));
        assert_eq!(parsed["tool_params"], params);

        // The near-miss that makes the distinction load-bearing: an empty
        // output is a real observation and must still emit the key.
        let post = hook_stdin_payload("bash", &params, Some(""), HOOK_STDIN_MAX_BYTES);
        let parsed_post: serde_json::Value = serde_json::from_str(&post).expect("valid JSON");
        assert_eq!(
            parsed_post["tool_output"],
            serde_json::json!(""),
            "an observed-empty output is a different fact from no output at all"
        );
    }

    /// The boundary, pinned on BOTH sides — a discriminator tested only where it
    /// fires is vacuous green.
    #[test]
    fn hook_stdin_payload_boundary_is_pinned_on_both_sides() {
        let params = serde_json::json!({});
        // Serialized overhead of `{"tool_name":"b","tool_params":{},"tool_output":""}`.
        let overhead = hook_stdin_payload("b", &params, Some(""), HOOK_STDIN_MAX_BYTES).len();
        // 400 is comfortably above the in-band marker's own length, so the
        // over-budget side exercises the TRIMMING path rather than the
        // nothing-fits floor. (A cap smaller than the marker is a real input and
        // has its own test; it is not the boundary this one is about.)
        let cap = overhead + 400;

        let exactly = "a".repeat(400);
        let at = hook_stdin_payload("b", &params, Some(&exactly), cap);
        assert_eq!(at.len(), cap, "fixture must land exactly on the budget");
        assert!(
            !at.contains("elided"),
            "exactly at the cap must be untouched: {at}"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&at).expect("valid JSON")["tool_output"],
            serde_json::json!(exactly)
        );

        let over = "a".repeat(401);
        let marked = hook_stdin_payload("b", &params, Some(&over), cap);
        assert!(
            marked.contains("elided"),
            "one byte over the cap must mark its cut: {marked}"
        );
        assert!(
            marked.len() <= cap,
            "the marked payload must itself fit the budget, got {} > {cap}",
            marked.len()
        );
    }

    /// The marker must not lie: the count it reports has to equal what was
    /// actually dropped. A marker that lies is worse than none.
    #[test]
    fn hook_stdin_payload_marker_count_agrees_with_what_was_dropped() {
        let params = serde_json::json!({});
        let overhead = hook_stdin_payload("b", &params, Some(""), HOOK_STDIN_MAX_BYTES).len();
        let cap = overhead + 200;
        let total_chars = 5_000;
        let raw = "z".repeat(total_chars);

        // ANTI-VACUOUS: the fixture genuinely exceeds the cap.
        assert!(
            hook_stdin_payload("b", &params, Some(&raw), HOOK_STDIN_MAX_BYTES).len() > cap,
            "fixture must be over the budget under test"
        );

        let got = hook_stdin_payload("b", &params, Some(&raw), cap);
        assert!(
            got.len() <= cap,
            "the result must fit the budget it was given"
        );

        let parsed: serde_json::Value = serde_json::from_str(&got).expect("valid JSON");
        let value = parsed["tool_output"].as_str().expect("a string");
        let kept = value.chars().take_while(|c| *c == 'z').count();
        let claimed = value
            .split("[yoyo: ")
            .nth(1)
            .and_then(|s| s.split(' ').next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("marker must report a count: {value}"));
        assert_eq!(
            claimed,
            total_chars - kept,
            "the marker claims {claimed} chars dropped but {} were",
            total_chars - kept
        );
    }

    /// A char straddling the cut is dropped WHOLE (rule #250), and the result
    /// still parses — the cut can never be a raw byte slice of the JSON.
    #[test]
    fn hook_stdin_payload_never_splits_a_char_and_always_parses() {
        let params = serde_json::json!({});
        let raw = "\u{1F419}".repeat(400); // 4 bytes each
        assert!(raw.len() > 1000, "fixture must be genuinely large");

        let overhead = hook_stdin_payload("b", &params, Some(""), HOOK_STDIN_MAX_BYTES).len();
        // Caps above the in-band marker's own length, so each of these
        // exercises the trimming path. The emoji is 4 bytes, so several of
        // these land mid-character by construction.
        for extra in [200usize, 201, 202, 203, 400, 401] {
            let cap = overhead + extra;
            let got = hook_stdin_payload("b", &params, Some(&raw), cap);
            let parsed: serde_json::Value = serde_json::from_str(&got)
                .unwrap_or_else(|e| panic!("cap {cap}: must stay valid JSON: {e}"));
            let value = parsed["tool_output"].as_str().expect("a string");
            let kept = value.split('\n').next().unwrap_or("");
            assert!(
                !kept.is_empty() && kept.chars().all(|c| c == '\u{1F419}'),
                "cap {cap}: the kept prefix must be WHOLE characters, got {kept:?}"
            );
            assert!(
                got.len() <= cap,
                "cap {cap}: result must fit its budget, got {}",
                got.len()
            );
        }
    }

    /// Params alone can blow the budget — `write_file` carries a whole file —
    /// so the fallback must still parse and still say what happened, rather
    /// than writing a truncated fragment no parser accepts.
    #[test]
    fn hook_stdin_payload_degrades_to_a_parseable_object_when_params_alone_are_too_big() {
        let params = serde_json::json!({"content": "x".repeat(5_000)});
        let cap = 512;

        // ANTI-VACUOUS: params really are over the cap on their own.
        assert!(
            params.to_string().len() > cap,
            "fixture params must genuinely exceed the cap"
        );

        let got = hook_stdin_payload("write_file", &params, Some("done"), cap);
        let parsed: serde_json::Value =
            serde_json::from_str(&got).expect("the fallback must still be valid JSON");
        assert_eq!(parsed["tool_name"], serde_json::json!("write_file"));
        assert!(
            parsed["payload_elided"]
                .as_str()
                .is_some_and(|s| s.contains("elided")),
            "the fallback must say what happened: {got}"
        );
        assert!(
            got.len() <= cap,
            "the fallback must fit the budget, got {} > {cap}",
            got.len()
        );
    }

    /// Deliberately WEAK source-level guard. `run_command` spawns real
    /// processes and is driven by no test, so this proves the call is
    /// POSITIONED — never that a hook read a byte. It asserts BOTH directions,
    /// because a guard checking only the pipe would pass on a half-applied edit
    /// that opened the pipe and wrote nothing. Needles are assembled at runtime
    /// so this cannot match its own source.
    #[test]
    fn run_command_pipes_stdin_and_writes_the_payload() {
        let src = include_str!("hooks.rs");
        let body = src
            .split_once("fn run_command(")
            .expect("run_command must exist")
            .1;
        let body = body
            .split_once("\nimpl Hook for ShellHook")
            .map(|(before, _)| before)
            .unwrap_or(body);

        let piped = format!("{}(std::process::Stdio::{})", ".stdin", "piped()");
        assert!(
            body.contains(&piped),
            "run_command must pipe stdin (`{piped}`). Unset means Stdio::inherit() \
             for spawn(), so a hook would inherit yoyo's own stdin — under `yoyo -p` \
             that stream is the prompt itself."
        );

        let write = format!("stdin.{}(stdin_payload.as_bytes())", "write_all");
        assert!(
            body.contains(&write),
            "run_command opens the stdin pipe but never writes the payload — a hook \
             would get an empty channel and every stdin test above would still pass, \
             because they drive the pure builder rather than the spawn."
        );
    }
}
