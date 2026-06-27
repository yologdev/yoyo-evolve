Title: Auto-run /goal verify after each prompt turn
Files: src/commands_goal.rs, src/repl.rs
Issue: none (competitive gap: Claude Code auto-verifies /goal after every turn)

## What

Claude Code's `/goal` runs a fast model check after every agent turn to see if the goal
has been achieved. yoyo's `/goal verify <cmd>` stores a verification command but only
runs it manually when the user types `/goal verify`. Close this gap by auto-running the
verify command after each prompt turn (mirroring how `run_watch_after_prompt` works).

## Implementation

### In `src/commands_goal.rs`:

1. Add a public function `run_goal_verify_after_prompt() -> Option<(bool, String)>`:
   - Call `load_verify_command()` — if None, return None
   - Call `run_verify_command(&cmd)` (already exists, returns `(exit_code, output)`)
   - If exit code == 0, print a green `✓ Goal verify passed` message to stderr
   - If exit code != 0, print a yellow `⚠ Goal verify failed` message with truncated output
   - Return `Some((passed, output))` so the caller knows

2. The function should truncate output to MAX_VERIFY_DISPLAY_CHARS (already defined as 2000).

### In `src/repl.rs`:

3. After the `run_watch_after_prompt` block (around line 590), add a call to
   `run_goal_verify_after_prompt()`. This should run regardless of whether files
   were modified (goal completion might happen through any agent action).

4. If the verify command fails, inject a brief note into the agent's context so it
   knows the goal isn't met yet. This can be as simple as printing the status — the
   agent sees stderr.

5. Import `run_goal_verify_after_prompt` from `commands_goal`.

### Tests:

6. Add tests in `commands_goal.rs`:
   - Test `run_goal_verify_after_prompt` returns None when no verify command is set
   - Test `run_goal_verify_after_prompt` returns Some((true, _)) when command succeeds
   - Test `run_goal_verify_after_prompt` returns Some((false, _)) when command fails
   (Use temp dirs to isolate the verify file)

### Docs:

7. Update `docs/src/usage/commands.md` if `/goal` docs exist there, mentioning auto-verify.
