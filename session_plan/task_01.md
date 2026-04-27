Title: Upgrade yoagent 0.7 → 0.8 (mechanical dependency bump)
Files: Cargo.toml, src/main.rs (if API changes), src/tools.rs (if API changes)
Issue: #343

## Context
yoagent v0.8.0 is released with SharedState support (PR #35). This is a mechanical
dependency bump — no new feature usage yet (that's Issue #344). @yuanhao filed #343
and it's blocking the RLM Layer 2 work.

## What to do

1. Change `Cargo.toml` line with yoagent dependency from:
   ```toml
   yoagent = { version = "0.7", features = ["openapi"] }
   ```
   to:
   ```toml
   yoagent = { version = "0.8", features = ["openapi"] }
   ```

2. Run `cargo build` and check for compilation errors. yoagent 0.8 may have breaking
   API changes — if so, fix them. Common changes between minor versions:
   - Method signatures on `Agent` builder
   - Event enum variants (new variants added)
   - Tool trait changes
   - Import path changes
   
3. Look at compilation errors carefully. For each error:
   - Check what changed in the new API
   - Make the minimal fix to compile
   - Do NOT add new feature usage (no SharedState, no new builder methods)

4. Run `cargo test` — all existing tests must pass.

5. Run `cargo clippy --all-targets -- -D warnings` — must be clean.

## Verification
- `cargo build` succeeds
- `cargo test` passes (all 2,238+ tests)
- `cargo clippy --all-targets -- -D warnings` clean
- The only changed files are Cargo.toml, Cargo.lock, and any src/ files needed for API compat

## Do NOT
- Add SharedState usage (that's #344)
- Change any behavior — this is a pure version bump
- Modify tests except to fix compilation from API changes
