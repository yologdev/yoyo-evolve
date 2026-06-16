Title: Auto-learn project facts from watch-mode fixes
Files: src/watch.rs, src/memory.rs
Issue: none

## What

The biggest gap vs Claude Code is cross-session project memory. Currently, auto-remember only fires when watch-mode fixes a broken build. Extend it to also capture *what was learned* from the fix — not just "fixed on attempt 1" but the specific project fact discovered.

This is the first step toward automatic project learning. The watch-mode fix loop already has all the context: what command failed, what the error was, what the fix was. Extract a learning from this.

### Changes to `src/memory.rs`:
1. Add `pub fn build_learn_memory_note(watch_cmd: &str, error_hint: &str) -> String` — builds a note like "Project quirk: `cargo test` requires DATABASE_URL set" or "Build note: tests need `--features test-utils`". The note should be prefixed with the category in bracket syntax so it gets categorized when stored.
2. This function takes the watch command and the error category hint (from `error_category_hint` in watch.rs) and constructs a useful memory.

### Changes to `src/watch.rs`:
1. In `run_watch_after_prompt`, after a successful fix (attempt > 0 and final result is success), call a new helper `maybe_learn_from_fix` that:
   - Looks at the `CompilerError` list from the failed run
   - If the errors fall into a recognizable pattern (missing feature flag, environment variable needed, specific dependency issue), build a learning note via `build_learn_memory_note`
   - Call `auto_remember` with the note (dedup already handled by auto_remember)
2. Add `fn maybe_learn_from_fix(watch_cmd: &str, errors: &[CompilerError], attempt: usize)` — the logic that decides whether a fix is worth remembering. Conservative: only learn from clear patterns, not every fix.
3. Keep the existing `auto_remember` call for the generic fix note. The learning note is *additional* — it captures what was learned, not just that something was fixed.

### Tests
- `build_learn_memory_note` produces reasonable notes for known patterns
- `maybe_learn_from_fix` with no recognizable pattern doesn't auto-remember
- `maybe_learn_from_fix` with a clear pattern (e.g., missing env var) does auto-remember
- Integration: multiple fixes for the same pattern don't create duplicate memories (dedup)
