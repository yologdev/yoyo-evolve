Title: Fix --fallback in piped mode and --prompt mode (Issue #230)
Files: src/main.rs
Issue: #230

## Problem

The `--fallback` flag only works in REPL mode. In piped mode (line ~1684) and `--prompt` mode (line ~1637), `run_prompt` returns a `PromptOutcome` with `last_api_error` set, but neither code path checks it. The agent silently returns empty/failed output with exit code 0.

This is critical because `evolve.sh` uses piped mode with `--fallback`.

## What to do

Add fallback retry logic to BOTH the piped mode and `--prompt` mode code paths in `main.rs`. The pattern already exists in `repl.rs:856-904` — adapt it for non-interactive use.

### For piped mode (around line 1684):

After `let response = run_prompt(...)`:
1. Check `response.last_api_error.is_some()`
2. If true, call `agent_config.try_switch_to_fallback()`
3. If switch succeeds, rebuild agent with `agent_config.build_agent()`, print fallback message to stderr
4. Retry with `run_prompt()` using the new agent
5. If fallback also fails, exit with non-zero code (exit 1)
6. If no fallback configured and API failed, also exit with non-zero code

### For --prompt mode (around line 1637):

Same pattern — check `response.last_api_error`, try fallback, retry or exit non-zero.

### Exit codes:
- Exit 0: success
- Exit 1: API failure (no fallback or fallback also failed)
- Exit 2: checkpoint triggered (already exists)

### Tests to add:

Add tests to the existing `mod tests` in `main.rs`:
- Test that `try_switch_to_fallback` returns true when fallback is configured and provider differs
- Test that `try_switch_to_fallback` returns false when already on fallback provider
- (These tests likely already exist from Day 31 — verify and add any missing coverage for the piped-mode specific logic)

Since we can't easily test the full piped-mode flow in unit tests, focus on ensuring the `AgentConfig::try_switch_to_fallback` method works correctly and that the exit code logic is sound. Add a helper function like `handle_fallback_retry` to avoid duplicating the fallback logic between piped and prompt modes.

### Refactoring hint:

To avoid duplicating the fallback retry pattern in three places (REPL, piped, prompt), extract a helper function:

```rust
/// Attempt fallback retry for non-interactive modes.
/// Returns the final PromptOutcome (either from retry or original).
/// Sets should_exit_error to true if all attempts failed.
async fn try_fallback_prompt(
    agent_config: &mut AgentConfig,
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    original_response: PromptOutcome,
) -> (PromptOutcome, bool) // (outcome, should_exit_with_error)
```

This keeps the change to 1 file and avoids code duplication.

### After implementing:

- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`
- Comment on Issue #230 with `gh issue comment 230 --body "..."` explaining the fix
- Close Issue #230 with `gh issue close 230`
