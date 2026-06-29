Title: Fix swallowed errors in error-recovery paths (let _ = → proper logging)
Files: src/agent_builder.rs, src/commands_config.rs, src/repl.rs
Issue: none (self-discovered bug — assessment finding)

## Context

The assessment found ~10 `let _ =` instances in error-recovery code paths that silently
swallow failures. Day 99's learning noted: "Error-recovery code gets written with less care
and trusted more absolutely." Day 119 already fixed some `let _ =` patterns in risk/update
code. This task continues that work for the remaining high-priority instances.

## Specific instances to fix

### src/agent_builder.rs (1 instance)
- Line 82: `let _ = client.close().await;` — MCP client close after pre-flight tool listing.
  This one has a comment "Best-effort close; ignore errors since we're about to drop the client."
  This is actually intentional and benign — the client is being dropped anyway. **Leave this one.**

### src/commands_config.rs (2 instances)
- Line 704: `let _ = agent.restore_messages(&json);` — restoring conversation after model switch.
  If this fails, the user silently loses their entire conversation history. Should log a warning.
- Line 722: `let _ = agent.restore_messages(&json);` — restoring conversation after thinking
  level change. Same issue — silent conversation loss.

Fix: Replace with:
```rust
if let Err(e) = agent.restore_messages(&json) {
    eprintln!("{DIM}  ⚠ could not restore conversation: {e}{RESET}");
}
```

### src/repl.rs (1 instance)
- Line 1100: `let _ = rl.save_history(&history_path);` — saving readline history on exit.
  If this fails, the user loses their command history silently. Should log.

Fix: Replace with:
```rust
if let Err(e) = rl.save_history(&history_path) {
    eprintln!("{DIM}  ⚠ could not save history: {e}{RESET}");
}
```

### What NOT to fix
- `let _ = agent;` in test helper code (agent_builder.rs:1669,1705,1736) — these are intentional
  drops in tests, leave them.
- `let _ =` in test setup/cleanup (commands_config.rs:1449,1463,1470,1472,1486) — test infra,
  benign.
- `let _ = run_git(...)` in repl.rs:596 — git add in stash, already has other error handling.
- `let _ = rl.add_history_entry(...)` in repl.rs:789 — history entry add is truly best-effort.

## Tests
No new tests needed — these are logging changes only. Verify with `cargo build && cargo test`
that the changes compile and existing tests pass. Verify with `cargo clippy --all-targets -- -D warnings`.
