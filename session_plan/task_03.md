Title: Update MiniMax known model list and default model
Files: src/cli.rs
Issue: #192

## Context

Issue #192 reports that MiniMax's known model list only has `MiniMax-M1` and `MiniMax-M1-40k`,
which are no longer in MiniMax's official docs. Current models are M2.7, M2.7-highspeed, M2.5,
M2.5-highspeed, etc. Users picking "MiniMax-M2.7" (the current flagship) get a `400 Bad Request`
with no helpful error.

Note: @taschenlampe filed this and a yoagent maintainer commented they'll add MiniMax as a
first-class provider in yoagent. Our fix is independent — just update the known model list
and default model in cli.rs so the setup wizard shows current models.

## Implementation

### 1. Update `known_models_for_provider()` in src/cli.rs

Find the `"minimax"` match arm and update:

```rust
"minimax" => &[
    "MiniMax-M2.7",
    "MiniMax-M2.7-highspeed",
    "MiniMax-M2.5",
    "MiniMax-M2.5-highspeed",
    "MiniMax-M1",
    "MiniMax-M1-40k",
],
```

Keep M1/M1-40k at the end for backward compatibility (users may still have them in configs).

### 2. Update `default_model_for_provider()` in src/cli.rs

Change the minimax default from `"MiniMax-M1"` to `"MiniMax-M2.7"`:

```rust
"minimax" => "MiniMax-M2.7",
```

### 3. Update tests

Find any tests that assert on the minimax model list and update them to match the new list.

### 4. Improve error message for 400 Bad Request

Check if `diagnose_api_error()` in prompt.rs handles 400 errors. If it already has a handler,
ensure it mentions "check your model name" as a possible cause. If not, add a case:

```rust
if error.contains("400") || error.contains("Bad Request") {
    return Some(format!(
        "The API returned 400 Bad Request. This often means the model name '{}' \
         is not recognized by the provider. Run /model to see available models, \
         or check your provider's documentation.",
        model
    ));
}
```

Only add this if it doesn't already exist and doesn't conflict with existing error handling.

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
