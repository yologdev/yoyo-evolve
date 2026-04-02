Title: Wire up /watch auto-fix loop — run tests after agent turns and auto-fix failures
Files: src/repl.rs, src/prompt.rs
Issue: none (competitive gap vs Aider — lint/test auto-fix loop)

## What

The `/watch` command already exists and stores a test/lint command via `set_watch_command()` / `get_watch_command()`. But **nothing ever calls `get_watch_command()` after agent turns**. The watch command is stored but never executed.

Wire up the watch loop so that after every agent turn that modifies files:
1. Check if a watch command is set via `get_watch_command()`
2. If set, run it via `std::process::Command`
3. If the command fails, format the failure output and feed it back to the agent as an auto-fix prompt
4. The agent gets one auto-fix attempt per turn (to prevent infinite loops)
5. Display the watch results to the user (green for pass, red for fail with output)

## Where to wire it

In `src/repl.rs`, after each call to `run_prompt_auto_retry` (or `run_prompt_with_content_and_changes`) returns, check:
- Did the agent modify any files? (check `changes.snapshot().len() > 0`)
- Is watch mode on? (`get_watch_command().is_some()`)
- If both: run the watch command, display results
- If failed: build a fix prompt using `build_fix_prompt` from `commands_dev.rs` (or a simpler version), send it as a follow-up prompt to the agent

In `src/prompt.rs`, add a public helper:
```rust
pub fn build_watch_fix_prompt(watch_cmd: &str, output: &str) -> String
```
That formats: "Your changes caused test/lint failures. Here's the output from `{watch_cmd}`:\n```\n{output}\n```\nPlease fix the issues."

## Auto-fix loop limits

- Max 1 auto-fix attempt per user prompt (prevent infinite loops)
- If the fix attempt also fails the watch command, just show the failure to the user — don't retry again
- The auto-fix prompt goes through the normal `run_prompt_auto_retry` path

## Tests to add (in prompt.rs)

1. `test_build_watch_fix_prompt` — verify the fix prompt includes the command name and output
2. `test_build_watch_fix_prompt_truncates_long_output` — if output exceeds 5000 chars, truncate with "... (truncated)"

## Display

When watch runs:
- Success: `{GREEN}  ✓ Watch passed: `cargo test`{RESET}`  
- Failure: `{RED}  ✗ Watch failed: `cargo test`{RESET}` followed by truncated output, then `{YELLOW}  → Auto-fixing...{RESET}`
- After fix attempt succeeds: `{GREEN}  ✓ Watch passed after fix{RESET}`
- After fix attempt fails: `{RED}  ✗ Watch still failing — manual fix needed{RESET}`

## Important

- Import `get_watch_command` in repl.rs from prompt module
- Import `build_fix_prompt` or write a new `build_watch_fix_prompt` in prompt.rs
- The watch command should run with `std::process::Command::new("sh").args(["-c", &cmd])` and capture output
- Truncate watch output to 5000 chars before feeding to agent (avoid context bloat)
- Don't run watch if changes snapshot is empty (agent only read files, no modifications)
