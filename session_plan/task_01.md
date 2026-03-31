Title: --fallback provider failback (Issue #205) — minimal REPL-level retry
Files: src/cli.rs, src/repl.rs
Issue: #205

## Context

This is attempt SIX. Three previous implementations were reverted. Two planning-only sessions
produced blueprints that were never executed. The scope was always too ambitious.

This time: the SMALLEST possible implementation. No FallbackProvider wrapper. No new trait.
No changes to build_agent. Just:

1. Parse `--fallback <provider>` in cli.rs (add a `fallback_provider` field to Config)
2. When the agent hits an API error in the REPL loop (run_repl in repl.rs), check if a fallback
   is configured and hasn't been tried yet. If so, rebuild AgentConfig with the fallback provider
   and model, rebuild the agent, print a message, and retry the same prompt.

## Detailed steps

### Step 1: Add fallback config to cli.rs (src/cli.rs)

Add to Config struct:
```rust
pub fallback_provider: Option<String>,
pub fallback_model: Option<String>,
```

In parse_args(), parse `--fallback <provider>` (single flag, provider name):
```rust
let fallback_provider = args.iter().position(|a| a == "--fallback")
    .and_then(|i| args.get(i + 1).cloned());
```

If fallback_provider is set, derive fallback_model using `default_model_for_provider()`.

Add to the help text in print_help():
```
  --fallback <prov>   Fallback provider if primary fails (e.g. --fallback google)
```

Add `"--fallback"` to the known flags list so it doesn't trigger unknown-flag warnings.

### Step 2: Wire fallback into the REPL loop (src/repl.rs)

In run_repl(), the function already receives `agent_config: &mut AgentConfig`.

After a prompt fails with an API error (the existing `last_error` / `outcome.last_tool_error`
path), add fallback logic:

```rust
// After getting an error from run_prompt_auto_retry:
if let Some(ref err) = last_error {
    if is_retriable_error(err) || diagnose_api_error(err, &agent_config.model).is_some() {
        if let Some(ref fallback) = agent_config.fallback_provider {
            if agent_config.provider != *fallback {
                eprintln!("\n⚡ Primary provider '{}' failed. Switching to fallback '{}'...",
                    agent_config.provider, fallback);
                agent_config.provider = fallback.clone();
                agent_config.model = agent_config.fallback_model
                    .clone()
                    .unwrap_or_else(|| default_model_for_provider(fallback));
                // Resolve API key for fallback provider
                if let Some(env_var) = provider_api_key_env(fallback) {
                    if let Ok(key) = std::env::var(env_var) {
                        agent_config.api_key = key;
                    }
                }
                agent = agent_config.build_agent();
                // Retry the same prompt
                // ... re-run the prompt with the new agent
            }
        }
    }
}
```

The key insight: we don't need to intercept deep in the agent. We intercept at the REPL level
where the error surfaces, switch the config, rebuild, and retry. The conversation history is
on the `messages` Vec which is separate from the agent.

### Step 3: Add fallback_provider/fallback_model to AgentConfig (src/main.rs... wait, no)

Actually — AgentConfig is in main.rs but we said max 3 files. The fallback fields only need
to be on Config (cli.rs) and used in run_repl (repl.rs). AgentConfig doesn't need them because
the REPL modifies the AgentConfig directly before calling build_agent().

BUT: run_repl receives `agent_config: &mut AgentConfig`. So we need to add the fallback fields
to AgentConfig too. That's in main.rs.

Files needed: src/cli.rs, src/repl.rs, src/main.rs — exactly 3.

### Step 3 (revised): Add fields to AgentConfig in main.rs

Add to AgentConfig struct:
```rust
pub fallback_provider: Option<String>,
pub fallback_model: Option<String>,
```

In main() where AgentConfig is constructed from Config, pass through:
```rust
fallback_provider: config.fallback_provider,
fallback_model: config.fallback_model,
```

### Tests

Add to cli.rs tests:
- `test_parse_fallback_flag` — verify `--fallback google` sets fallback_provider
- `test_parse_fallback_missing` — verify no --fallback means None

Add to repl.rs or as integration test:
- Hard to unit-test the actual failover (needs API calls), but we can test config propagation

### What NOT to do

- Do NOT create a FallbackProvider wrapper type
- Do NOT modify build_agent() internals
- Do NOT touch prompt.rs or the streaming code
- Do NOT add retry loops inside the agent — this is REPL-level only
- Do NOT support multiple fallbacks or fallback chains — just one --fallback

### Docs

Update help text in print_help() (cli.rs) — that's it. No CLAUDE.md or README changes needed
for an initial ship. Those can come in a follow-up.
