Title: Extract provider module from cli.rs — first step in splitting the 3,816-line monolith
Files: src/cli.rs, src/providers.rs
Issue: none

## What

Extract all provider-related constants and functions from `cli.rs` (3,816 lines, the largest file) into a new `src/providers.rs` module. Re-export the public items from `cli.rs` so downstream code (`commands.rs`, `main.rs`) doesn't need to change.

## Why

The assessment flags `cli.rs` at 3,816 lines as the next candidate for splitting. Provider logic is a natural extraction boundary — it's self-contained (list of providers, their API key env vars, default models, known models) and doesn't depend on the rest of CLI parsing. This follows the "one module at a time" sizing rule.

## What to extract

From `cli.rs`, move these items into `src/providers.rs`:

1. `KNOWN_PROVIDERS` constant (array of provider name strings)
2. `provider_api_key_env(provider: &str) -> Option<&'static str>` — maps provider name to env var
3. `known_models_for_provider(provider: &str) -> &[&str]` — lists models per provider
4. `default_model_for_provider(provider: &str) -> String` — default model selection

These are all pure functions with no dependencies on the rest of cli.rs.

## Implementation steps

1. Create `src/providers.rs` with the 4 items above
2. In `cli.rs`, replace the moved items with:
   ```rust
   // Re-export provider utilities so existing imports from cli:: still work
   pub use crate::providers::{
       KNOWN_PROVIDERS, provider_api_key_env,
       known_models_for_provider, default_model_for_provider,
   };
   ```
3. Add `mod providers;` in `main.rs` (module declaration)
4. Move any associated tests from `cli.rs` to `providers.rs`

## Critical constraint

Do NOT change any `use crate::cli::` imports elsewhere. The re-exports from `cli.rs` must maintain the existing public API. The ONLY files that change are:
- `src/providers.rs` (new file)
- `src/cli.rs` (items removed, re-exports added)
- `src/main.rs` (add `mod providers;` declaration — just one line)

This is 3 files, within the task limit.

## Tests

- Move existing provider-related tests from `cli.rs` to `providers.rs`
- Add a test that `KNOWN_PROVIDERS` contains at least 10 entries (we have 12)
- Add a test that every provider in `KNOWN_PROVIDERS` has a `default_model_for_provider`
- Add a test that every provider in `KNOWN_PROVIDERS` has a `known_models_for_provider` list

## Acceptance

- `cargo build && cargo test` passes
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo fmt -- --check` clean
- `cli.rs` line count drops by ~200-300 lines
- All existing `use crate::cli::KNOWN_PROVIDERS` etc. continue to work without changes
- Update CLAUDE.md's source architecture table to include `providers.rs` and update `cli.rs` line count
