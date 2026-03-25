Title: Use yoagent's built-in context management instead of manual compaction
Files: src/main.rs, src/commands_session.rs, src/cli.rs, src/repl.rs, src/commands.rs
Issue: #183

## Context

yoyo manually reimplements context management that yoagent 0.7 already provides built-in:
- `compact_agent()`, `auto_compact_if_needed()`, `proactive_compact_if_needed()` in commands_session.rs
- `MAX_CONTEXT_TOKENS`, `AUTO_COMPACT_THRESHOLD`, `PROACTIVE_COMPACT_THRESHOLD` in cli.rs
- `configure_agent()` in main.rs never calls `with_context_config()`, so yoagent's automatic compaction doesn't know the context budget

yoagent 0.7 provides:
- `with_context_config(ContextConfig)` — sets max_context_tokens, keep_recent, keep_first, tool_output_max_lines
- Built-in compaction that runs automatically *before each LLM turn* (in `run_loop`)
- `ExecutionLimits` with `max_total_tokens` for clean budget-based stopping

## Implementation

### 1. Wire `ContextConfig` in `configure_agent()` (src/main.rs)

In the `configure_agent()` method of `AgentConfig`, add:
```rust
use yoagent::context::ContextConfig;

let context_config = ContextConfig {
    max_context_tokens: 200_000,
    system_prompt_tokens: 4_000,
    keep_recent: 10,
    keep_first: 2,
    tool_output_max_lines: 50,
};
agent = agent.with_context_config(context_config);
```

Also wire `max_total_tokens` in `ExecutionLimits`:
```rust
agent = agent.with_execution_limits(ExecutionLimits {
    max_turns: self.max_turns.unwrap_or(200),
    max_total_tokens: 1_000_000,
    ..ExecutionLimits::default()
});
```

### 2. Simplify manual compaction (src/commands_session.rs)

- **Remove** `auto_compact_if_needed()` — yoagent now handles this automatically before each turn
- **Remove** `proactive_compact_if_needed()` — same reason, yoagent's compaction runs before each turn
- **Keep** `compact_agent()` and `handle_compact()` — the `/compact` command is a useful manual override
- **Keep** the `total_tokens()` import — still needed for `/tokens` and `/compact` display

### 3. Remove auto-compact calls from callers

- In `src/commands.rs`: Remove `auto_compact_if_needed` from imports and any calls to it
- In `src/commands_git.rs`: Remove `auto_compact_if_needed` from imports and any calls to it
- In `src/repl.rs`: Remove `auto_compact_if_needed` from imports and calls. Remove `proactive_compact_if_needed` calls.

### 4. Clean up constants (src/cli.rs)

- Remove `AUTO_COMPACT_THRESHOLD` and `PROACTIVE_COMPACT_THRESHOLD` constants (no longer used)
- Keep `MAX_CONTEXT_TOKENS` for now — it's still used by `/tokens` display

### 5. Update tests

- Remove any tests that specifically test `auto_compact_if_needed` or `proactive_compact_if_needed`
- Add a test verifying that `configure_agent` produces an agent with context_config set (if testable)
- Keep tests for `compact_agent()` and `handle_compact()` since those remain

### 6. Verify

Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings` to ensure everything compiles and tests pass. The key behavioral change: compaction now happens automatically before each LLM turn (handled by yoagent), not after each turn (handled by yoyo). This is strictly better timing.
