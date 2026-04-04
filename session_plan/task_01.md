Title: Watch mode multi-attempt fix loop (up to 3 retries)
Files: src/repl.rs, src/prompt.rs
Issue: none

## What

The current `/watch` auto-fix makes exactly one attempt to fix failures, then gives up with
"manual fix needed." Claude Code and Aider both have more resilient retry patterns. This task
adds a retry loop (up to 3 attempts) where each retry includes the latest failure output.

## Current behavior (src/repl.rs around line 1005)

After the agent modifies files, the watch command runs. If it fails:
1. Show truncated output
2. Print "Auto-fixing..."
3. Run ONE fix prompt via `run_prompt_auto_retry`
4. Re-run watch command
5. If still failing: "Watch still failing — manual fix needed"

## Target behavior

Replace the single fix attempt with a loop:
1. After files are modified, run watch command
2. If it fails, enter retry loop (max 3 attempts, configurable via const):
   - Show which attempt this is: "Auto-fixing (attempt 1/3)..."
   - Build fix prompt including the LATEST failure output (not the original)
   - Run the fix prompt
   - Re-run watch command
   - If passes: "Watch passed after fix (attempt N)" → break
   - If fails: capture new output, continue to next attempt
3. After all attempts exhausted: "Watch still failing after 3 attempts — manual fix needed"

## Implementation details

In `src/prompt.rs`:
- Add `const MAX_WATCH_FIX_ATTEMPTS: usize = 3;`
- Export it for use in repl.rs

In `src/repl.rs`:
- Replace the single-attempt block (lines ~1003-1023) with a `for attempt in 1..=MAX_WATCH_FIX_ATTEMPTS` loop
- Each iteration: build fix prompt with current output, run prompt, re-run watch, break on success
- Update status messages to include attempt number

## Tests

Add tests in `src/prompt.rs` for the new constant (exists, is reasonable value).
The retry loop logic itself is in the REPL async path and is tested via the existing integration pattern.

## What NOT to do
- Don't change the watch command parsing or /watch command itself
- Don't add configuration flags — the const is enough for now
- Don't touch any other files
