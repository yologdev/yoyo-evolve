Title: Fix permission prompt hidden by spinner (Issue #224)
Files: src/prompt.rs, src/main.rs
Issue: #224

## Problem

When a bash tool needs user confirmation, the `ToolProgressTimer` is started on `ToolExecutionStart` 
at line ~1021 of `prompt.rs`. The timer writes `\r\x1b[K` + spinner frames to stderr every 500ms.
Meanwhile, the bash tool's `execute()` method calls `confirm_fn` which prints the permission prompt 
to stderr and waits for stdin input. The timer overwrites the prompt — the user can't see the options.

This is a UX-breaking bug for anyone using yoyo in default (non `--yes`) mode.

## Fix

The fix is straightforward: **don't start `ToolProgressTimer` for bash tools until after confirmation
has been resolved.** There are two approaches:

### Approach A: Signal-based (preferred)
Add an `Arc<AtomicBool>` "confirmation pending" flag to `StreamingBashTool`. When `confirm_fn` is 
about to prompt, set the flag. When done (approved or denied), clear it. In `prompt.rs`, check this
flag before starting the timer — or better yet, start the timer only on the first `ToolUpdate` event
(which signals the command is actually running), not on `ToolExecutionStart`.

### Approach B: Don't start timer on ToolExecutionStart for bash (simpler)
The simplest fix: in `prompt.rs` around line 1021, check if auto_approve is false. If it is, don't
start the timer on `ToolExecutionStart` for "bash" tools. Instead, start it on the first 
`ToolUpdate` for that tool_call_id (which only fires once the command is actually running, meaning
confirmation already passed).

**Go with Approach B** — it's simpler and doesn't require threading state between main.rs and prompt.rs.

### Implementation details:

1. In `prompt.rs`, around line 1021 where the timer starts:
   ```rust
   if tool_name == "bash" {
       let timer = ToolProgressTimer::start(tool_name.clone());
       tool_progress_timers.insert(tool_call_id.clone(), timer);
   }
   ```
   Change to: only start the timer if `auto_approve` is true. Otherwise, defer timer creation.

2. In the `ToolUpdate` event handler (which fires when streaming output arrives), check if there's 
   no timer yet for this tool_call_id and it's a bash tool — if so, start one now.

3. The `auto_approve` flag needs to be accessible in the event loop. It's already available as 
   a parameter to `run_prompt` or can be passed through. Check how it flows from `build_tools()`.

   Actually, the simplest approach: always defer bash timer to ToolUpdate. The 2-second delay in 
   `ToolProgressTimer::start()` means short commands won't show it anyway. And for commands that 
   need confirmation, the timer only starts after the command is actually executing.

4. Add a test that verifies ToolProgressTimer is not started on ToolExecutionStart for bash tools
   when auto_approve is off.

### What to verify:
- `cargo build && cargo test`
- Manual test: run yoyo without `--yes`, trigger a bash command, verify the permission prompt is 
  visible and not overwritten
- The timer should still appear for long-running commands after approval
