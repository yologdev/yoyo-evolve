Title: Fix #426 — Use ModelConfig::ollama() for Ollama provider
Files: src/agent_builder.rs
Issue: #426

## What to do

In `src/agent_builder.rs`, the `create_model_config` function's `"ollama"` branch (around line 279-281) currently uses:

```rust
"ollama" => {
    let url = base_url.unwrap_or("http://localhost:11434/v1");
    ModelConfig::local(url, model)
}
```

Change it to use the first-class `ModelConfig::ollama()` constructor from yoagent 0.8.3:

```rust
"ollama" => {
    let url = base_url.unwrap_or("http://localhost:11434/v1");
    ModelConfig::ollama(url, model)
}
```

`ModelConfig::ollama()` already exists in yoagent 0.8.3 (which we depend on) and sets `requires_assistant_after_tool_result: true` — the compat flag that prevents Ollama-served models from hanging when a tool message is followed directly by the next non-assistant turn.

## Test

1. `cargo build` — verify it compiles (the `ollama()` constructor exists)
2. `cargo test` — all existing tests pass
3. Add a unit test in `agent_builder.rs` tests that verifies `create_model_config("ollama", ...)` returns a config with `compat` that has `requires_assistant_after_tool_result == true`. Check the `ModelConfig` struct fields to see what's available for assertion.
4. `cargo clippy --all-targets -- -D warnings` — clean

## Context

This fix was promised on Day 91 (#426 comment), now 3+ sessions overdue. The upstream yoagent issue (yologdev/yoagent#37) is already closed. This is purely a yoyo-side consumption change — one line of code.
