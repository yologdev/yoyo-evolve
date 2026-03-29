Title: Implement --fallback provider failover (Issue #205)
Files: src/cli.rs, src/main.rs, src/prompt.rs, src/commands.rs, src/help.rs, tests/integration.rs, docs/src/configuration/models.md, CLAUDE.md
Issue: #205

## Context

This is attempt 5 of the most-dodged task in the project. Three previous implementations were reverted. The Day 28 learning says: "After a task has been reverted, the intervention isn't a better plan — it's a smaller first step."

The approach this time: **minimal, test-first, no new abstractions.** No `FallbackProvider` wrapper. No complex trait objects. Just:
1. Parse the flag
2. Store it in AgentConfig
3. When an API error occurs in the prompt loop, rebuild the agent with fallback config and retry

## Design

### 1. CLI parsing (src/cli.rs)

Add `--fallback <provider:model>` flag to `parse_args()`. Parse it as a string, split on `:` into provider and model. Store as `Option<(String, String)>` in the config struct returned by parse_args. Also store the fallback API key by looking up `provider_api_key_env()` for the fallback provider.

Add a `fallback` field to the config struct:
```rust
pub fallback: Option<FallbackConfig>,
```

Where `FallbackConfig` is a simple struct:
```rust
pub struct FallbackConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
}
```

Validation: if fallback provider needs an API key and it's not in the environment, print a warning and continue without fallback (don't hard-error).

### 2. AgentConfig (src/main.rs)

Add `pub fallback: Option<cli::FallbackConfig>` to `AgentConfig`.

Add a method `build_fallback_agent(&self) -> Option<Agent>` that constructs a new agent using the fallback provider/model but keeping everything else (system prompt, tools, skills, thinking, etc.) the same. This reuses `configure_agent()` — just swap the provider/model/key.

### 3. Error interception (src/prompt.rs)

In `run_prompt_with_changes()` and `run_prompt_with_content_and_changes()`, where API errors are already caught and `diagnose_api_error` is called, add a fallback path:

- When an API error occurs (not context overflow, not retriable transient errors that auto-retry handles)
- If a fallback agent is available
- Print a message: `"⚡ Primary provider failed, switching to fallback ({provider}:{model})..."`
- Rebuild the agent from the fallback config
- Return a special result that tells the caller to retry with the new agent

The simplest approach: add a `fallback_config: Option<&FallbackConfig>` parameter to `run_prompt_with_changes`. When a non-retriable API error occurs and fallback is available, return a new variant or flag indicating "switch to fallback and retry." The caller in `repl.rs` then rebuilds the agent and retries the same prompt.

Actually, even simpler: make the fallback swap happen at the `repl.rs` / `main.rs` level. The `run_prompt*` functions already return errors. In the REPL loop (and in the single-prompt path), catch the error, check if fallback is configured, swap the agent, and retry. This avoids threading fallback config through the prompt functions.

### 4. REPL integration (src/repl.rs)

In `run_repl()`, the prompt result is already handled. When `run_prompt*` returns an error string containing API/provider failure indicators, and `agent_config.fallback` is Some:
- Print the fallback message
- Rebuild agent via `agent_config.build_fallback_agent()`
- Retry the same prompt
- Only attempt fallback once per prompt (don't loop)

Similarly for single-prompt mode in `main.rs`.

### 5. Status display (src/commands.rs)

Update `/status` to show the fallback provider if configured.

### 6. Help text (src/help.rs)

Add `--fallback` to the help text.

### 7. Tests

Write tests FIRST:
- `test_parse_fallback_flag` — parsing `--fallback openai:gpt-4o`
- `test_parse_fallback_flag_missing_colon` — error handling for bad format
- `test_parse_fallback_no_api_key_warning` — graceful handling when env var missing
- `test_fallback_config_struct` — FallbackConfig construction
- `test_build_fallback_agent` — verify fallback agent uses correct provider/model

### 8. Documentation

- Update docs/src/configuration/models.md with `--fallback` usage
- Update CLAUDE.md if architecture changes

## Key constraints

- Do NOT create wrapper providers or complex abstractions — that's what got reverted before
- The fallback is a one-shot retry: primary fails → try fallback once → if fallback fails too, show the error
- Preserve conversation context: the fallback agent gets the same messages
- Only failover on API errors (4xx/5xx), NOT on context overflow (which needs compaction, not a different provider) and NOT on user cancellation
