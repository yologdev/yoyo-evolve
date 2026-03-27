Title: Fix hardcoded 200K context window — derive from model config, add --context-window override
Files: src/main.rs, src/cli.rs, src/commands.rs, src/help.rs, docs/src/configuration/models.md
Issue: #195, #197

## Problem

`configure_agent()` in `main.rs:1277` hardcodes `max_context_tokens: 200_000` for ALL providers. This overrides yoagent's built-in auto-derivation from `ModelConfig.context_window`. The result:

- **Google (1M context):** compacts at 200K — wastes 80% of available context
- **MiniMax (1M context):** same — wastes 80%
- **OpenAI (128K context):** compaction threshold is 200K — never compacts, hits API limits
- **xAI (131K context):** same issue as OpenAI
- **Anthropic (200K):** happens to be correct by accident

This has been planned and dropped in **4+ consecutive sessions**. It goes first today.

## Implementation

### Step 1: Capture context_window from ModelConfig in build_agent()

In `build_agent()` (main.rs ~line 1326), the `ModelConfig` is created BEFORE being passed to `configure_agent()`. Since `model_config` is private on `Agent`, we need to capture the `context_window` value before it disappears into the agent.

Approach: Change `configure_agent()` to accept the model's context_window as a parameter.

```rust
// Change signature from:
fn configure_agent(&self, agent: Agent) -> Agent
// To:
fn configure_agent(&self, agent: Agent, model_context_window: u32) -> Agent
```

In `build_agent()`, for each path:
- **Anthropic:** `ModelConfig::anthropic(...)` has `context_window: 200_000` — capture before passing
- **Google:** `create_model_config("google", ...)` — capture `config.context_window`
- **Others:** `create_model_config(...)` — capture `config.context_window`

Pattern for each branch:
```rust
let model_config = ModelConfig::anthropic(&self.model, &self.model);
let context_window = model_config.context_window;  // capture before move
let agent = Agent::new(AnthropicProvider).with_model_config(model_config);
self.configure_agent(agent, context_window)
```

### Step 2: Use model_context_window in configure_agent()

Replace the hardcoded block (line 1277):
```rust
agent = agent.with_context_config(ContextConfig {
    max_context_tokens: 200_000,
    ...
});
```

With:
```rust
// User override takes precedence; otherwise use the model's actual context window
let effective_context_window = self.context_window.unwrap_or(model_context_window);
agent = agent.with_context_config(ContextConfig {
    max_context_tokens: (effective_context_window as usize) * 80 / 100,
    system_prompt_tokens: 4_000,
    keep_recent: 10,
    keep_first: 2,
    tool_output_max_lines: 50,
});
```

Also fix the checkpoint mode block (~line 1301) which uses `cli::MAX_CONTEXT_TOKENS`:
```rust
// Was: let max_tokens = cli::MAX_CONTEXT_TOKENS;
let max_tokens = (effective_context_window as u64) * 80 / 100;
```

### Step 3: Add --context-window CLI flag

In `src/cli.rs`:
1. Add `pub context_window: Option<u32>` to the `Config` struct
2. Add parsing in the arg loop: `"--context-window" => { context_window = args.get(i+1).and_then(|v| v.parse().ok()); i += 1; }`
3. Also support `context_window = 32000` in `.yoyo.toml` config file parsing
4. Remove or deprecate `MAX_CONTEXT_TOKENS` constant (it was 200_000, now unused for the main path)
5. Keep `MAX_CONTEXT_TOKENS` as a fallback default if somehow no model config provides a context_window — but it shouldn't be needed since every provider path creates a ModelConfig

### Step 4: Update /tokens and /status displays

In `src/commands.rs`:
- `MAX_CONTEXT_TOKENS` is used at lines 214 and 438 to show context budget. These should now show the effective context window. Since commands don't have direct access to the config's effective context window, store it in a global or pass it through.
- Simplest: add a `pub static EFFECTIVE_CONTEXT_TOKENS: AtomicU64` in cli.rs, set it in `configure_agent()`, read it in commands.rs. This follows the same pattern as other globals (VERBOSE, NO_COLOR, etc.).
- Replace references to `MAX_CONTEXT_TOKENS` in commands.rs with reads from `EFFECTIVE_CONTEXT_TOKENS`.

### Step 5: Update help text

In `src/help.rs`, add help for `--context-window`:
```
--context-window <N>    Override context window size (tokens). By default,
                        yoyo uses the correct size for each provider
                        (e.g., 200K for Anthropic, 1M for Google).
                        Use this for custom deployments or local models.
```

### Step 6: Tests

Add to `src/cli.rs` tests:
- `test_context_window_default_is_none` — verify Config has `context_window: None` by default
- `test_context_window_cli_flag` — parse `--context-window 32000` → `Some(32000)`
- `test_context_window_from_config` — config file `context_window = 500000` → `Some(500000)`

Add to `src/main.rs` tests:
- `test_effective_context_window_anthropic` — Anthropic path uses 200K
- `test_effective_context_window_google` — Google path uses 1M
- `test_effective_context_window_openai` — OpenAI path uses 128K
- `test_effective_context_window_with_override` — `--context-window 50000` overrides model default

Update existing test `test_max_context_tokens` in cli.rs (line 1662) — it currently asserts `MAX_CONTEXT_TOKENS == 200_000`. Either remove it or update to test the new behavior.

### Step 7: Update docs

Update `docs/src/configuration/models.md`:
- Document that yoyo now automatically uses the correct context window per provider
- Document the `--context-window` override flag
- Give examples: `yoyo --context-window 32000` for a small local model

Update CLAUDE.md if the architecture section mentions the 200K constant.

### Important notes

- The `ContextConfig` import already exists: `use yoagent::context::{ContextConfig, ExecutionLimits};`
- `ContextConfig::from_context_window()` exists in yoagent but gives default keep_recent=4, keep_first=1 — we want our custom values (10, 2, 50), so manually construct the struct
- The key providers and their correct context_window values (from yoagent ModelConfig factories): Anthropic=200K, OpenAI=128K, Google=1M, MiniMax=1M, xAI=131K, Groq=128K, DeepSeek=128K
- This task has build-failed in a previous session. The likely cause was complexity — keep it surgical. Don't refactor the provider paths. Just: capture context_window, pass it through, use it.
