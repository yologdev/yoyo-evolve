Title: Add unit tests for help.rs — worst test coverage ratio at 96 lines/test
Files: src/help.rs
Issue: none

`help.rs` has 2,511 lines and only 25 tests — the worst lines-per-test ratio (96 lines/test) in the codebase. Adding focused tests improves reliability and catches regressions when help text is updated.

**What to test:**

1. **`cli_help_text()`** — Verify it returns a non-empty string, contains key sections (USAGE, FLAGS, COMMANDS), mentions important flags (--model, --provider, --print, --thinking, --system-prompt, --skills, --disallowed-tools). Test that every flag documented in `cli.rs` `parse_args` appears in the help text (cross-reference completeness).

2. **`help_text()`** — The REPL `/help` output. Verify it contains command categories, lists known commands, includes the new commands added recently. Check that it mentions `/spawn`, `/retry`, `/bg`, `/review`, `/map`, `/grep`, etc.

3. **`command_help(cmd)`** — Test that each major command returns detailed help text:
   - `command_help("diff")` should mention flags like `--staged`, `--stat`
   - `command_help("grep")` should mention pattern arguments
   - `command_help("spawn")` should describe task delegation
   - `command_help("map")` should mention repo map
   - `command_help("review")` should describe code review
   - Test that `command_help("nonexistent")` returns None or empty

4. **`command_short_description(cmd)`** — Test that known commands return a short description. Test that unknown commands return a sensible fallback (empty string or None).

5. **`help_command_completions()`** — Test that it returns a non-empty list, contains expected commands, doesn't contain duplicates.

6. **`handle_help_command(cmd, ...)`** — Test edge cases: empty string, unknown command, command with extra whitespace.

**Guidelines:**
- Each test should be focused and fast (no I/O, no network).
- Use descriptive test names like `test_cli_help_contains_print_flag`, `test_command_help_diff_mentions_staged`.
- Add tests to the existing `#[cfg(test)] mod tests` block at the bottom of `help.rs`.
- Target adding ~15-20 tests to bring the ratio down to ~50 lines/test.
- Don't modify any non-test code in `help.rs`.
