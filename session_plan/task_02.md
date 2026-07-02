Title: Harden --model handling: reject empty/whitespace, warn on unknown model name
Files: src/cli.rs, src/providers.rs
Issue: #543

## Problem (from issue #543)

Two robustness gaps in `--model` handling:

1. **Empty/whitespace model reaches API unguarded.** `flag_value` returns raw values, so `--model " "` or `--model ""` bypasses the `unwrap_or_else(default_model_for_provider)` fallback and flows to the API as an invalid model name → 400 error. The `validate_config_value` function in config.rs already rejects empty, but it's only wired to `/config set`, not the `--model` CLI path.

2. **No model-name validation.** Provider names are validated against `KNOWN_PROVIDERS` with a warning, but model names are not. A typo like `claude-opus-4-7` silently 404s on the first API call. Now that the harness uses one `vars.MODEL` for all cron jobs, one typo breaks the entire fleet.

## Fix

### Part 1: Empty/whitespace filter (cli.rs)

In `parse_model_config` (around line 490), after resolving the model value, filter empty/whitespace before returning:

```rust
let model = flag_value(args, &["--model"])
    .or_else(|| file_config.get("model").cloned())
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| default_model_for_provider(&provider));
```

This trims whitespace and treats empty strings as "no model specified" → falls through to the default.

### Part 2: Unknown model warning (cli.rs)

After resolving the model, add a warn-only check (mirroring the existing provider warning):

```rust
// Warn if model isn't in the known list for first-party providers
let known = known_models_for_provider(&provider);
if !known.is_empty() && !known.contains(&model.as_str()) {
    eprintln!(
        "{YELLOW}warning:{RESET} Unknown model '{model}' for provider '{provider}'. \
         Known models: {}. Proceeding anyway (custom models are valid).",
        known.iter().take(5).copied().collect::<Vec<_>>().join(", ")
    );
}
```

This warns but does NOT block — custom/new models are valid. Only warn for providers where we have a known-models list (first-party). For unknown providers, skip the check.

### Tests

Add tests in `cli.rs` or `providers.rs`:
1. Test that `known_models_for_provider("anthropic")` returns a non-empty list containing `claude-sonnet-4-20250514`
2. Test that trimming logic works: empty and whitespace-only strings fall through to default

## Verification
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
