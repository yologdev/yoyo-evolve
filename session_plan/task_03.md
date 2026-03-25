Title: Add MiniMax as a named provider
Files: src/cli.rs, src/setup.rs, src/main.rs
Issue: #179

## Context

Community request to add MiniMax as a named provider in the onboarding wizard. MiniMax is an OpenAI-compatible provider. Currently users have to use the "Custom" escape hatch to configure it.

## Implementation

### 1. Add to `KNOWN_PROVIDERS` in `src/cli.rs`

Add `"minimax"` to the `KNOWN_PROVIDERS` array (before `"custom"`):
```rust
pub const KNOWN_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "google",
    "openrouter",
    "ollama",
    "xai",
    "groq",
    "deepseek",
    "mistral",
    "cerebras",
    "zai",
    "minimax",  // NEW
    "custom",
];
```

### 2. Add provider config functions in `src/cli.rs`

In `provider_api_key_env()`, add:
```rust
"minimax" => "MINIMAX_API_KEY",
```

In `default_model_for_provider()`, add:
```rust
"minimax" => "MiniMax-M1",
```

In `known_models_for_provider()`, add:
```rust
"minimax" => vec!["MiniMax-M1", "MiniMax-M1-40k"],
```

In `provider_base_url()`, add:
```rust
"minimax" => Some("https://api.minimax.io/v1/"),
```

In `cost_per_million()`, add appropriate cost entry for minimax (check if pricing is known, otherwise use a reasonable default or skip).

### 3. Add to wizard menu in `src/setup.rs`

Add to `WIZARD_PROVIDERS`:
```rust
("minimax", "MiniMax"),
```

This will make it option 11 in the wizard, shifting "Custom" to option 12.

### 4. Wire provider creation in `src/main.rs`

In the `create_provider()` function (or wherever providers are instantiated), add `"minimax"` to the OpenAI-compatible provider list. It should use `OpenAiCompatProvider` with the MiniMax base URL.

### 5. Tests

- Test that `provider_api_key_env("minimax")` returns `"MINIMAX_API_KEY"`
- Test that `default_model_for_provider("minimax")` returns a valid model
- Test that `parse_provider_choice` recognizes "minimax" by name
- Test that `WIZARD_PROVIDERS` includes the minimax entry
- Update any existing tests that count the number of providers (e.g., if a test checks `KNOWN_PROVIDERS.len()`)

### 6. Update docs

No docs changes needed — the provider list in the README is auto-generated from the code during releases.
