Title: Add --fallback CLI flag for provider failover (Issue #205, attempt 4 — simplified)
Files: src/main.rs, src/cli.rs, tests/integration.rs, src/help.rs, docs/src/configuration/models.md
Issue: #205

## Context

This task has been attempted 3 times and reverted 3 times — twice due to test failures (#207, Day 28 04:07), once due to build failures. The previous approach tried to build a complex `FallbackProvider` wrapper. This attempt uses a SIMPLIFIED approach inspired by @BenjaminBilbro's LiteLLM suggestion and takes a test-first path.

**The key design change from previous attempts:** Don't try to transparently intercept mid-stream — instead, implement failover at the `AgentConfig::build_agent()` level using a simple `FallbackProvider` that wraps two providers and tries the fallback only on non-retryable, non-cancelled errors from `stream()`.

## WRITE TESTS FIRST

Before writing ANY implementation code, write these tests:

### 1. CLI parsing tests (in src/cli.rs tests section)

```rust
// Test: --fallback parses correctly
// Input: ["yoyo", "--fallback", "openai:gpt-4o"]
// Expected: config.fallback == Some("openai:gpt-4o".to_string())

// Test: --fallback without value gives error or is None
// Input: ["yoyo", "--fallback"]
// Expected: config.fallback == None (consumed as missing arg)

// Test: --fallback invalid format (no colon) gives warning but still sets it
// Input: ["yoyo", "--fallback", "openai"]  
// Expected: config.fallback == Some("openai".to_string()) — warn at runtime, not at parse time

// Test: fallback from config file
// Input: file_config with fallback = "google:gemini-2.0-flash"
// Expected: config.fallback == Some("google:gemini-2.0-flash".to_string())

// Test: CLI --fallback overrides config file
// Input: ["yoyo", "--fallback", "openai:gpt-4o"] with file_config fallback = "google:gemini"
// Expected: config.fallback == Some("openai:gpt-4o".to_string())
```

### 2. Integration test (in tests/integration.rs)

```rust
// Test: --fallback appears in help output
// Test: --fallback with --help shows the flag description
```

### 3. FallbackProvider unit tests (in src/main.rs tests section)

```rust
// Test: parse_fallback_spec("openai:gpt-4o") -> ("openai", "gpt-4o")
// Test: parse_fallback_spec("google:gemini-2.0-flash") -> ("google", "gemini-2.0-flash")
// Test: parse_fallback_spec("anthropic:claude-sonnet-4-20250514") -> ("anthropic", "claude-sonnet-4-20250514")
// Test: parse_fallback_spec("invalid") -> ("invalid", default_model_for_provider("invalid"))
// Test: parse_fallback_spec("openai:") -> ("openai", default_model_for_provider("openai"))
```

Run `cargo test` after writing tests — they should compile but may fail (since the implementation doesn't exist yet). That's fine — make them compile by adding stub types/functions first if needed.

## Implementation

### Step 1: Add `fallback` field to CLI config

In `src/cli.rs`, in the `CliConfig` struct (or wherever the parsed config is returned):
- Add `pub fallback: Option<String>` field
- Parse `--fallback <provider:model>` from args (look for `--fallback` in position scanning, like `--model`)
- Parse `fallback = "provider:model"` from config file
- CLI overrides config file

In the `KNOWN_FLAGS` or flag list, add `"--fallback"`.

### Step 2: Add `parse_fallback_spec` helper

In `src/main.rs`, add:
```rust
/// Parse a "provider:model" spec into (provider, model).
/// If no colon, treat the whole string as provider and use its default model.
fn parse_fallback_spec(spec: &str) -> (String, String) {
    if let Some((provider, model)) = spec.split_once(':') {
        let model = if model.is_empty() {
            cli::default_model_for_provider(provider)
        } else {
            model.to_string()
        };
        (provider.to_string(), model)
    } else {
        (spec.to_string(), cli::default_model_for_provider(spec))
    }
}
```

### Step 3: Build `FallbackProvider`

In `src/main.rs`:

```rust
use yoagent::provider::{StreamProvider, StreamConfig, StreamEvent, ProviderError};

/// A provider wrapper that tries a primary provider first,
/// falling back to a secondary on non-retryable errors.
struct FallbackProvider {
    primary: Arc<dyn StreamProvider>,
    fallback: Arc<dyn StreamProvider>,
    fallback_model: String,
    fallback_model_config: Option<ModelConfig>,
}

#[async_trait::async_trait]
impl StreamProvider for FallbackProvider {
    async fn stream(
        &self,
        config: StreamConfig,
        tx: mpsc::UnboundedSender<StreamEvent>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Message, ProviderError> {
        // Try primary
        match self.primary.stream(config.clone(), tx.clone(), cancel.clone()).await {
            Ok(msg) => Ok(msg),
            Err(e) => {
                // Only failover on non-retryable, non-cancelled errors
                // (yoagent's retry handles RateLimited and Network already)
                match &e {
                    ProviderError::Cancelled => return Err(e),
                    ProviderError::RateLimited { .. } => return Err(e),
                    ProviderError::Network(_) => return Err(e),
                    _ => {}
                }
                
                // Log the failover
                eprintln!("\n{YELLOW}⚡ Primary provider failed: {e}{RESET}");
                eprintln!("{DIM}  Trying fallback: {}...{RESET}", self.fallback_model);
                
                // Build fallback config with same messages/tools but different model
                let fallback_config = StreamConfig {
                    model: self.fallback_model.clone(),
                    model_config: self.fallback_model_config.clone(),
                    ..config
                };
                
                self.fallback.stream(fallback_config, tx, cancel).await
            }
        }
    }
}
```

**CRITICAL**: Check that `StreamConfig` derives `Clone`. If it doesn't, you'll need to reconstruct it manually. Check the yoagent source:
```bash
grep -n "derive" ~/.cargo/registry/src/*/yoagent-*/src/provider/traits.rs | head
```
The assessment shows `StreamConfig` has `#[derive(Debug, Clone)]` — so cloning is fine.

**CRITICAL**: Check that `mpsc::UnboundedSender<StreamEvent>` can be cloned. Yes, `UnboundedSender` implements `Clone`.

**CRITICAL**: Check that `CancellationToken` can be cloned. Yes, it implements `Clone`.

### Step 4: Wire into `AgentConfig`

Add `fallback: Option<String>` to `AgentConfig`. In `build_agent()`:

```rust
pub fn build_agent(&self) -> Agent {
    let base_url = self.base_url.as_deref();
    
    // Build the primary provider + model config
    let (primary_provider, primary_model_config): (Arc<dyn StreamProvider>, ModelConfig) = 
        if self.provider == "anthropic" && base_url.is_none() {
            let mut mc = ModelConfig::anthropic(&self.model, &self.model);
            insert_client_headers(&mut mc);
            (Arc::new(AnthropicProvider), mc)
        } else if self.provider == "google" {
            let mc = create_model_config(&self.provider, &self.model, base_url);
            (Arc::new(GoogleProvider), mc)
        } else {
            let mc = create_model_config(&self.provider, &self.model, base_url);
            (Arc::new(OpenAiCompatProvider), mc)
        };
    
    let context_window = primary_model_config.context_window;
    
    // If fallback is configured, wrap in FallbackProvider
    let (final_provider, final_model_config): (Arc<dyn StreamProvider>, ModelConfig) = 
        if let Some(ref fallback_spec) = self.fallback {
            let (fb_provider_name, fb_model) = parse_fallback_spec(fallback_spec);
            
            // Check if fallback provider has an API key
            let fb_has_key = cli::provider_api_key_env(&fb_provider_name)
                .and_then(|env_var| std::env::var(env_var).ok())
                .is_some();
            
            if !fb_has_key && fb_provider_name != "ollama" && fb_provider_name != "custom" {
                eprintln!("{YELLOW}warning:{RESET} No API key found for fallback provider '{fb_provider_name}', fallback disabled");
                (primary_provider, primary_model_config)
            } else {
                let fb_model_config = create_model_config(&fb_provider_name, &fb_model, None);
                let fallback_provider: Arc<dyn StreamProvider> = if fb_provider_name == "anthropic" {
                    Arc::new(AnthropicProvider)
                } else if fb_provider_name == "google" {
                    Arc::new(GoogleProvider)
                } else {
                    Arc::new(OpenAiCompatProvider)
                };
                
                let wrapped = FallbackProvider {
                    primary: primary_provider,
                    fallback: fallback_provider,
                    fallback_model: fb_model,
                    fallback_model_config: Some(fb_model_config),
                };
                (Arc::new(wrapped), primary_model_config)
            }
        } else {
            (primary_provider, primary_model_config)
        };
    
    let agent = Agent::new_with_arc(final_provider)  // Check if this method exists
        .with_model_config(final_model_config);
    self.configure_agent(agent, context_window)
}
```

**CRITICAL CHECK**: Does `Agent` have a constructor that takes `Arc<dyn StreamProvider>`? Check:
```bash
grep -n "pub fn new\|fn new_with" ~/.cargo/registry/src/*/yoagent-*/src/agent.rs | head
```

If `Agent::new()` takes `impl StreamProvider + 'static`, then `FallbackProvider` can be passed directly:
```rust
let agent = Agent::new(wrapped).with_model_config(final_model_config);
```

This is likely the cleaner path. The existing code uses `Agent::new(AnthropicProvider)` etc., which takes the provider by value. `FallbackProvider` implements `StreamProvider`, so `Agent::new(FallbackProvider { ... })` should work.

**The refactoring of build_agent()** is the risky part. The current code creates the `Agent` in three branches, each calling `Agent::new(ProviderType)`. To wrap in `FallbackProvider`, you need to abstract the provider creation. DO THIS INCREMENTALLY:

1. First, refactor `build_agent()` to extract provider creation into a helper that returns `(Box<dyn StreamProvider>, ModelConfig)` 
2. Then add the fallback wrapping around that helper
3. Test after EACH step

Actually, even simpler: keep the existing three branches but add a parallel path when fallback is set. This avoids refactoring the working code:

```rust
pub fn build_agent(&self) -> Agent {
    if let Some(ref fallback_spec) = self.fallback {
        // Build with fallback wrapping
        self.build_agent_with_fallback(fallback_spec)
    } else {
        // Existing code unchanged
        self.build_agent_primary()
    }
}
```

This is the SAFEST approach — existing behavior is completely untouched when `--fallback` is not set.

### Step 5: Update help text

In `src/help.rs`, add `--fallback` to the CLI flags section:
```
  --fallback <p:m>  Fallback provider:model on API errors (e.g. openai:gpt-4o)
```

### Step 6: Update config file parsing

In `src/cli.rs`, where `file_config` keys are read, add support for:
```toml
fallback = "openai:gpt-4o"
```

### Step 7: Add to docs

In `docs/src/configuration/models.md`, add a section on fallback configuration:
```markdown
## Fallback Provider

Configure a fallback provider for automatic failover:

```bash
yoyo --fallback openai:gpt-4o
```

Or in `.yoyo.toml`:
```toml
fallback = "openai:gpt-4o"
```
```

### Step 8: Run full test suite

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```

If ANY step fails, revert ONLY that step and try a simpler approach. If the `FallbackProvider` struct is causing issues, start with JUST the CLI parsing and config — that alone is useful progress that can be committed.

## IMPORTANT: Incremental commits

After each working step, do a build check:
```bash
cargo build && cargo test
```

If it passes, keep going. If it fails, fix immediately before moving to the next step.

## What NOT to do

- Don't modify `evolve.sh` or workflow files
- Don't try to make fallback work for sub-agents (that's a future enhancement)
- Don't implement a provider chain (multiple fallbacks) — one fallback is enough for v1
- Don't change the behavior when `--fallback` is NOT set — existing users must see zero changes
- Don't fail at startup if fallback API key is missing — warn and disable

## If all else fails

If the `FallbackProvider` wrapper approach fails again (e.g., `Agent::new()` doesn't accept `Arc<dyn StreamProvider>` or `FallbackProvider`), fall back to the SIMPLEST possible version:
- Just parse `--fallback` and store it in config
- When the agent loop encounters a non-retryable error, rebuild the agent with the fallback provider/model
- This is the "agent-level restart" approach rather than "provider-level failover" — less elegant but guaranteed to work since `build_agent()` already supports switching providers

The agent rebuild approach works like this in `main()`:
```rust
// After agent error in the REPL loop or prompt execution:
if let Some(fallback) = &agent_config.fallback {
    let (fb_provider, fb_model) = parse_fallback_spec(fallback);
    agent_config.provider = fb_provider;
    agent_config.model = fb_model;
    agent = agent_config.build_agent();
    eprintln!("⚡ Switched to fallback: {}", fallback);
    // Retry the last prompt
}
```
This doesn't require implementing `StreamProvider` at all — it reuses the existing `build_agent()` mechanism.
