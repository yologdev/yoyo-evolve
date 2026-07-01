Title: Harden --model handling: reject empty/whitespace, warn on unknown model name
Files: src/cli.rs, src/providers.rs
Issue: #543

## Problem

Two robustness gaps in `--model` handling:

1. **Empty/whitespace `--model` reaches the API unguarded.** `flag_value` in `dispatch_sub.rs` returns the raw value with no empty filter. In `parse_model_config` (cli.rs:490-492), `Some(" ")` short-circuits the `.unwrap_or_else(default_model_for_provider)`, so `model = " "` flows to `.with_model(...)` → API 400. The `validate_config_value("model","")` in config.rs DOES reject empty — but it's only wired to `/config set`, not the `--model` path.

2. **No model-name validation → one typo 404s the whole fleet.** `parse_model_config` validates the *provider* against `KNOWN_PROVIDERS` and warns — but does nothing for the model name. A typo'd model (e.g. `claude-opus-4-7`) silently 404s.

## Implementation

### Fix 1: Filter empty/whitespace in `parse_model_config` (cli.rs)

In `parse_model_config`, after getting the model value (~line 490), add a trim+empty check:

```rust
let model = flag_value(args, &["--model"])
    .or_else(|| file_config.get("model").cloned())
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| default_model_for_provider(&provider));
```

This trims whitespace and filters empty strings before the `unwrap_or_else`, so empty/whitespace values fall through to the default model instead of reaching the API.

### Fix 2: Warn on unknown model name (cli.rs + providers.rs)

After setting the model value, add a warning similar to the existing provider warning:

```rust
// Warn on potentially unknown model name (typo detection)
let known_models = known_models_for_provider(&provider);
if !known_models.is_empty() && !known_models.iter().any(|m| *m == model) {
    eprintln!(
        "{YELLOW}warning:{RESET} Model '{model}' not recognized for provider '{provider}'. \
         Known models: {}. Proceeding anyway (custom models are valid).",
        known_models.iter().take(5).copied().collect::<Vec<_>>().join(", ")
    );
}
```

This is warn-only — custom models are valid, so don't block. Only warn for first-party providers where we have a known model list.

### Tests

Add tests in cli.rs (or a test module) that verify:
- Empty string model falls through to default
- Whitespace-only model falls through to default
- Valid model passes through unchanged
- Unknown model for a known provider triggers the code path (test the logic, not stderr output)

### Files touched
- `src/cli.rs` — modify `parse_model_config` to add trim+empty filter and model name warning
- `src/providers.rs` — ensure `known_models_for_provider` is pub(crate) accessible (it already is)

No CLAUDE.md or docs changes needed — this is a robustness fix, not a behavior change.
