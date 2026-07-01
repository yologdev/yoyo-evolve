Title: Add GitHub Copilot as a model provider
Files: src/providers.rs, src/setup.rs
Issue: #544

## Problem

GitHub Copilot is a widely used coding subscription. Users with Copilot access can use its API endpoint (https://api.githubcopilot.com) which is OpenAI-compatible. Adding it as a known provider lowers onboarding friction for Copilot subscribers.

## Implementation

### 1. Add "github" to KNOWN_PROVIDERS in `src/providers.rs`

Add `"github"` to the `KNOWN_PROVIDERS` array (alphabetical position, after "google").

### 2. Add API key env var mapping

In `provider_api_key_env`, add:
```rust
"github" => Some("GITHUB_TOKEN"),
```

GitHub Copilot API uses the `GITHUB_TOKEN` environment variable for authentication. Users with Copilot access can generate a token via GitHub settings.

### 3. Add default base URL

In `default_base_url_for_provider` (if it exists) or in the provider base URL logic, set the default base URL for github to `https://api.githubcopilot.com`. If there's no such function, add a match arm in the relevant location.

Check: search for where base URLs are set per-provider. If there's no centralized function, the user will need to pass `--base-url https://api.githubcopilot.com` — document this in the setup wizard output.

### 4. Add known models for GitHub Copilot

In `known_models_for_provider`, add:
```rust
"github" => &[
    "gpt-4o",
    "gpt-4o-mini",
    "gpt-4.1",
    "gpt-4.1-mini",
    "claude-sonnet-4",
    "o3-mini",
],
```

These are the models available through GitHub Copilot's API.

### 5. Add default model

In `default_model_for_provider`, add:
```rust
"github" => "gpt-4o",
```

### 6. Add to setup wizard

In `src/setup.rs`, add GitHub to the `WIZARD_PROVIDERS` list so it shows up during `yoyo --setup`. Include setup instructions mentioning:
- Users need a GitHub Copilot subscription
- Authentication uses `GITHUB_TOKEN`
- Set the env var: `export GITHUB_TOKEN=ghp_...`

### 7. Tests

Add a test in providers.rs:
- Verify "github" is in KNOWN_PROVIDERS
- Verify known_models_for_provider("github") returns non-empty
- Verify provider_api_key_env("github") returns Some("GITHUB_TOKEN")

### Scope note

This task does NOT implement the OAuth device flow described in the issue (opening a browser for authentication code). That would be a significant feature requiring HTTP server infrastructure. Instead, this adds GitHub Copilot as a standard provider using token-based auth (same as every other provider). The user sets `GITHUB_TOKEN` in their environment.

### Files touched
- `src/providers.rs` — add "github" to KNOWN_PROVIDERS, API key env var, known models, default model
- `src/setup.rs` — add GitHub to WIZARD_PROVIDERS with setup instructions
