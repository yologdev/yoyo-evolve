Title: Fix hardcoded 200K context window — let yoagent auto-derive per provider, add --context-window override
Files: src/main.rs, src/cli.rs, src/help.rs, docs/src/configuration/models.md
Issue: #195

## Why this matters

`main.rs:1158` hardcodes `max_context_tokens: 200_000` for ALL providers. This means:
- Google/MiniMax users (1M context) compact at 200K — 80% wasted
- OpenAI users (128K context) have a 200K budget but only 128K actual — never compacts until too late
- Local model users can't match their `n_ctx` setting

yoagent v0.7.4 already auto-derives from `ModelConfig.context_window` when no `with_context_config()` is called, but we want to preserve our custom `keep_recent: 10`, `keep_first: 2`, `tool_output_max_lines: 50`.

## CRITICAL: Why previous attempt failed (Issue #197)

The previous implementation **failed to build**. The exact failure was not recorded in detail, but the approach tried to conditionally call `with_context_config()` which likely caused issues with variable scoping or the builder pattern. The safe approach below avoids that by ALWAYS calling `with_context_config()` but computing `max_context_tokens` dynamically.

## Implementation

### Step 1: Add `context_window: Option<u32>` to Config and AgentConfig

In `src/cli.rs`:
1. Add `pub context_window: Option<u32>` to the `Config` struct (after `context_strategy`)
2. Parse `--context-window <N>` in the arg-parsing loop. Follow the exact same pattern as `--max-turns`:
   ```rust
   "--context-window" => { ... parse next arg as u32 ... }
   ```
3. Support `context_window = 32000` in the TOML config file (same place `max_turns` is read)
4. Add to `print_help()` under the options section:
   ```
   --context-window <n>  Context window size in tokens (default: auto per provider)
   ```
5. Initialize to `None` in `Config` construction (same as `max_turns` default handling)

In `src/main.rs`:
1. Add `pub context_window: Option<u32>` to the `AgentConfig` struct
2. Pass `config.context_window` through in the `AgentConfig { ... }` construction at line ~1261

### Step 2: Compute effective context window in configure_agent()

In `configure_agent()`, replace the hardcoded block at lines 1158-1164:

```rust
// OLD:
agent = agent.with_context_config(ContextConfig {
    max_context_tokens: 200_000,
    system_prompt_tokens: 4_000,
    keep_recent: 10,
    keep_first: 2,
    tool_output_max_lines: 50,
});
```

With this approach — ALWAYS call `with_context_config()` but derive `max_context_tokens` from the right source:

```rust
// Determine context budget: CLI override > model config default
// The model_config is already set on the agent by build_agent() before configure_agent() runs.
// We need to capture the context_window from the ModelConfig at creation time.
// Since build_agent() creates the ModelConfig, we can read it there.
```

Wait — the issue is that `configure_agent()` doesn't have access to the `ModelConfig` that was already set. The `build_agent()` method creates the `ModelConfig` and calls `with_model_config()` then passes the agent to `configure_agent()`. So we need to thread the context_window value through.

**Best approach**: In `build_agent()`, capture `model_config.context_window` and store it temporarily, then use it in `configure_agent()`. Since `build_agent()` calls `self.configure_agent(agent)`, we can pass an extra parameter.

Actually, simplest approach — change `configure_agent()` to also accept the model's context_window:

```rust
fn configure_agent(&self, mut agent: Agent, model_context_window: u32) -> Agent {
    // ... existing setup ...
    
    // Derive context budget: user override > model default
    let effective_cw = self.context_window.unwrap_or(model_context_window);
    agent = agent.with_context_config(ContextConfig {
        max_context_tokens: (effective_cw as usize) * 80 / 100,
        system_prompt_tokens: 4_000,
        keep_recent: 10,
        keep_first: 2,
        tool_output_max_lines: 50,
    });
    // ... rest unchanged ...
}
```

Then update all three call sites in `build_agent()`:
```rust
// Anthropic path:
let mut model_config = ModelConfig::anthropic(&self.model, &self.model);
let cw = model_config.context_window;
// ...
self.configure_agent(agent, cw)

// Google path:
let model_config = create_model_config(&self.provider, &self.model, base_url);
let cw = model_config.context_window;
// ...
self.configure_agent(agent, cw)

// OpenAI-compat path:
let model_config = create_model_config(&self.provider, &self.model, base_url);
let cw = model_config.context_window;
// ...
self.configure_agent(agent, cw)
```

### Step 3: Update tests

**In `src/cli.rs`:**
- `test_context_window_default_is_none` — verify Config has `context_window: None` by default
- `test_context_window_parses_value` — parse `["yoyo", "--context-window", "32000"]` → `Some(32000)`
- `test_context_window_from_toml` — test TOML config parsing with `context_window = 32000`

**In `src/main.rs`:**
- Update `test_configure_agent_sets_context_config` — it currently creates an AgentConfig; add `context_window: None` field
- Add `test_context_window_override` — create AgentConfig with `context_window: Some(32000)`, verify it builds without panic
- Update ALL existing AgentConfig construction sites in tests to include `context_window: None`

IMPORTANT: Search for all `AgentConfig {` in src/main.rs tests and add the new field. Missing fields will cause build failures — this is likely what broke the previous attempt.

### Step 4: Update docs

In `docs/src/configuration/models.md`, add a section:
```markdown
## Context Window

yoyo automatically uses the correct context window for each provider:
- Anthropic: 200K tokens
- Google Gemini: 1M tokens  
- OpenAI: 128K tokens
- MiniMax: 1M tokens
- Local models: 128K default

Override with `--context-window <N>` or `context_window = N` in config:
```bash
yoyo --provider ollama --model llama3.2 --context-window 32768
```
```

### Step 5: Verify

Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`.
