Title: Add terminal bell notification on long operations
Files: src/repl.rs, src/prompt.rs
Issue: #167

## Context

Issue #167 was reverted because it tried to thread config through function signatures. The bell infrastructure already exists from Day 23's session — `format.rs` has `bell_enabled()`, `maybe_ring_bell()`, `disable_bell()`, and `--no-bell` CLI flag is already parsed.

What's missing: actually *calling* `maybe_ring_bell()` after prompts complete.

## Implementation

### 1. Ring bell after REPL prompts

In `src/repl.rs`, inside the main REPL loop, after `run_prompt()` (or `run_prompt_with_changes()`) returns, call:

```rust
crate::format::maybe_ring_bell(prompt_start.elapsed());
```

Find where `run_prompt()` is awaited in the REPL loop. The elapsed time should be measured from just before the prompt is sent. There's likely already an `Instant::now()` tracking for cost display — reuse that, or add one.

Look for the pattern in `repl.rs` where `run_prompt_with_changes` is called. The `PromptOutcome` is returned with timing info. Use `outcome.elapsed` or measure it locally.

### 2. Ring bell after single-prompt mode

In `src/main.rs`, after the single-prompt path calls `run_prompt_with_changes`, also call `maybe_ring_bell()`.

Look for the `-p` / `--prompt` handling path in main where `run_prompt_with_changes` is called. Add the bell there too.

### 3. Ring bell after piped-mode prompt

In `src/main.rs`, the piped mode path also calls `run_prompt_with_changes`. Add `maybe_ring_bell()` there.

Note: In piped mode, the bell might be less useful but it doesn't hurt — the user might be in a terminal running a script that pipes to yoyo.

### 4. Tests

The bell infrastructure (flag parsing, env var, `maybe_ring_bell`, `bell_enabled`) is already tested in `format.rs`. The integration test `test_no_bell_flag_accepted` already exists.

Add:
- `test_bell_flag_default_in_config` — verify default AgentConfig has bell enabled (this may already be covered)

Actually, the main thing to test is that `maybe_ring_bell` is called — but since it's just writing `\x07` to stdout, it's hard to test in unit tests. The existing tests are sufficient. Just verify the code compiles and passes.

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
