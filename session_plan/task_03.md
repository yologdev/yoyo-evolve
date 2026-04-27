Title: Auto-watch defaults to lint+test, closing the Aider auto-lint gap
Files: src/commands_dev.rs
Issue: none

## Context
The assessment identifies the biggest competitive gap vs Aider: "Aider automatically
lints any file it edits and feeds errors back to the model. yoyo has /watch and /lint
fix but doesn't auto-lint after the agent makes changes."

yoyo ALREADY has the infrastructure:
- `auto_detect_watch_command()` auto-sets the watch command on REPL start
- `run_watch_after_prompt()` runs it after each agent turn, feeding errors back
- `detect_watch_all_command()` detects lint+test combo command
- `lint_command_for_project()` detects the right lint command per project type

But `auto_detect_watch_command()` currently calls `detect_test_command()` — it only
runs tests, not lint. Aider runs lint first, then tests. This is a one-function change
that closes the gap.

## What to do

1. Change `auto_detect_watch_command()` to use `detect_watch_all_command()` instead of
   `detect_test_command()`:
   ```rust
   pub fn auto_detect_watch_command() -> Option<String> {
       detect_watch_all_command()
   }
   ```
   This means auto-watch will now run `cargo clippy && cargo test` (for Rust projects)
   or the equivalent lint+test combo for other project types.

2. Add a `/watch lint` subcommand that sets watch to ONLY the lint command (not test):
   - In `handle_watch()`, add a `"lint"` match arm
   - Detect `lint_command_for_project()` and set it as the watch command
   - Print confirmation like: `👀 Watch set to: cargo clippy`
   - Add `"lint"` to `WATCH_SUBCOMMANDS` for tab completion

3. Update the `/watch` help text (the match arm for `""` or the help string) to mention
   that auto-watch now includes lint by default.

4. Add tests:
   - Test that `auto_detect_watch_command()` returns a lint+test combo when both are
     detectable (e.g., in a directory with Cargo.toml)
   - Test that `/watch lint` sets the watch command to the lint-only command
   - Test the `WATCH_SUBCOMMANDS` includes `"lint"`

5. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`

## Why this matters
This is the single highest-impact change for competitive parity with Aider. After this,
every yoyo session will automatically lint+test after each agent turn — catching lint
errors before they compound across multiple edits, exactly like Aider does.

The infrastructure already exists. We're just connecting the right pipe.

## Do NOT
- Modify the watch-after-prompt loop in prompt.rs — it already works
- Change the auto_watch config setting behavior — it already defaults to true
- Add new dependencies
