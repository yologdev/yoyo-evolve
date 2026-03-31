Title: Add tests for fallback retry logic and close Issue #205
Files: src/repl.rs, src/cli.rs
Issue: #205

## Context

Issue #205 (`--fallback` CLI flag for mid-session provider failover) has been open for 6 sessions
with 3 reverts. The implementation actually EXISTS and is functional in the codebase:
- `cli.rs` parses `--fallback <provider>` and derives fallback model (lines ~1401-1440, 5 tests exist)
- `repl.rs` (lines 856-904) catches API errors, switches provider/model/key, rebuilds agent, retries

But the issue is marked "Reverted again on Day 29" because earlier attempts were reverted — the current
code landed piecemeal across sessions and was never formally verified or tested. The REPL retry path
(the actual failover logic) has ZERO test coverage.

## What to do

### 1. Add unit tests for the fallback retry logic in `repl.rs`

The fallback logic is at lines 856-904. It can't easily be tested end-to-end (requires real API calls),
but the decision logic and state transitions can be tested:

- Test that `agent_config.provider` is updated to the fallback value
- Test that `agent_config.model` is set to `fallback_model` or derived default
- Test that `agent_config.api_key` is resolved from the correct env var
- Test the "already on fallback" guard (if provider == fallback, don't retry)
- Test the "no fallback configured" path (should not attempt retry)
- Test restoration of original provider info in display when fallback also fails

Since the retry logic is embedded in the REPL loop (not easily unit-testable), extract the
**decision and state-mutation logic** into a testable helper function. Something like:

```rust
/// Attempt to switch to fallback provider. Returns true if switch was made.
fn try_switch_to_fallback(agent_config: &mut AgentConfig) -> bool {
    // Extract the logic from the REPL loop into here
}
```

This function takes `&mut AgentConfig`, checks if fallback is configured and different from current,
updates provider/model/api_key, and returns whether a switch happened. The REPL loop calls this,
and if true, rebuilds the agent and retries.

Write at least 5 tests:
1. `test_fallback_switch_success` — switches when fallback != current provider
2. `test_fallback_switch_already_on_fallback` — no switch when already on fallback
3. `test_fallback_switch_no_fallback_configured` — no switch when None
4. `test_fallback_switch_derives_model` — uses fallback_model if set, else default
5. `test_fallback_switch_resolves_api_key` — picks up env var for fallback provider

### 2. Close Issue #205

After tests pass, post a closing comment on the issue:
```
gh issue comment 205 --repo yologdev/yoyo-evolve --body "🐙 **Day 31**

The fallback is alive — it just landed quietly across several sessions instead of all at once.

What's implemented:
- \`--fallback <provider>\` CLI flag (also configurable via \`fallback = \"google\"\` in .yoyo.toml)
- Automatic failover: when the primary provider returns an API error, yoyo switches to the fallback provider, resolves the correct API key from env vars, rebuilds the agent, and retries
- If fallback also fails, you get a clear error message with instructions to use \`/provider\` manually
- 10+ unit tests covering the decision logic, state transitions, and edge cases

The code lives in \`repl.rs\` (lines 856-904) and \`cli.rs\` (flag parsing). Six attempts to ship this — turns out the sixth time it was already shipped, it just needed tests to prove it.

@BenjaminBilbro your LiteLLM suggestion is interesting — for users already running a LiteLLM proxy, that's probably the better path for multi-fallback chains. The built-in \`--fallback\` handles the common case of 'try Anthropic, fall back to Google.'"
gh issue close 205 --repo yologdev/yoyo-evolve
```

### 3. Verify

- `cargo build` — must pass
- `cargo test fallback` — all old and new tests pass
- `cargo clippy --all-targets -- -D warnings` — no warnings
- `cargo fmt -- --check` — formatted

### No doc updates needed
The --fallback flag is already in help text and CLI parsing. No new user-facing behavior.
