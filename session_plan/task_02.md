Title: Add MCP server config to .yoyo.toml and fix MiniMax to use yoagent's ModelConfig::minimax()
Files: src/cli.rs, src/main.rs
Issue: #191, #192

## Context

Two community issues addressed in one task:

1. **Issue #191** — Users want to specify MCP servers in `.yoyo.toml` instead of CLI flags. Currently `--mcp "npx open-websearch@latest"` is the only way. The config file already supports model, provider, api_key, etc. MCP should be there too. This enables project-specific and global MCP configs.

2. **Issue #192** — MiniMax provider uses `ModelConfig::openai()` with manual overrides when yoagent 0.7.3+ provides `ModelConfig::minimax()` as a first-class helper. @yuanhao already added it to yoagent. Using it gives correct base_url (`api.minimaxi.chat` not `api.minimax.io`), proper compat flags, and future-proof model handling. Also update the default model to MiniMax-M2.7 (current flagship).

## Implementation

### Part A: MCP in .yoyo.toml (Issue #191)

#### 1. Update config file parsing in `src/cli.rs`

The current `parse_config_file()` returns `HashMap<String, String>` — flat key-value pairs. MCP servers need a list. Two approaches:

**Option A (simpler, recommended):** Use a special key format:
```toml
# .yoyo.toml
mcp = ["npx open-websearch@latest", "npx @modelcontextprotocol/server-filesystem /tmp"]
```

In `parse_config_file`, detect when a value starts with `[` and parse it as a TOML array using the existing `parse_toml_array()` function. Store the result as a comma-joined or specially-formatted string that gets split later.

Actually, `parse_toml_array` already exists and handles `["a", "b"]` syntax. Use it.

**Implementation:**
- Add a new function or modify the config loading to handle the `mcp` key specially
- In the section of `parse_args` where `file_config` is used (around line 956+), look for `mcp` key and parse it as an array
- Merge with any `--mcp` CLI flags (CLI flags take precedence / are additive)

```rust
// In parse_args, after file_config is loaded:
let mut mcp_servers: Vec<String> = args.iter()...;  // existing CLI parsing
// Add MCP servers from config file
if let Some(mcp_config) = file_config.get("mcp") {
    let config_mcps = parse_toml_array(mcp_config);
    // Prepend config MCPs (CLI --mcp flags override/add to config)
    for server in config_mcps.into_iter().rev() {
        if !mcp_servers.contains(&server) {
            mcp_servers.insert(0, server);
        }
    }
}
```

#### 2. Tests for MCP config

- `test_mcp_from_config_file` — parse a config with `mcp = ["npx foo", "npx bar"]` and verify the resulting mcp_servers vec
- `test_mcp_config_merged_with_cli` — verify CLI --mcp flags are additive with config file MCPs
- `test_mcp_config_empty_array` — `mcp = []` produces empty list

#### 3. Update docs

- Add MCP config example to the `.yoyo.toml` documentation section in help output (the `--config-help` or similar)
- Update `docs/src/configuration/skills.md` or create `docs/src/configuration/mcp.md` with example

### Part B: Fix MiniMax ModelConfig (Issue #192)

#### 1. Update `create_model_config()` in `src/main.rs`

Replace the manual MiniMax config:
```rust
// BEFORE:
"minimax" => {
    let mut config = ModelConfig::openai(model, model);
    config.provider = "minimax".into();
    config.base_url = base_url.unwrap_or("https://api.minimax.io/v1").to_string();
    config
}

// AFTER:
"minimax" => {
    let mut config = ModelConfig::minimax(model, model);
    if let Some(url) = base_url {
        config.base_url = url.to_string();
    }
    insert_client_headers(&mut config);
    config
}
```

This uses yoagent's `ModelConfig::minimax()` which sets:
- Correct base URL: `https://api.minimaxi.chat/v1`  
- Correct compat flags for MiniMax's API quirks
- Proper provider name

#### 2. Update default model in `src/cli.rs`

Change the default model for minimax from M1 to M2.7:
```rust
"minimax" => "MiniMax-M2.7".into(),
```

#### 3. Update tests

- Update `test_minimax_default_model` to expect "MiniMax-M2.7"
- Update `test_create_model_config_minimax` (if exists) to verify the new base_url from `ModelConfig::minimax()`
- Verify `create_model_config("minimax", "MiniMax-M2.7", None)` produces correct base_url

## Key considerations

- The MCP config parsing uses the existing `parse_toml_array()` — no new TOML parsing needed
- MCP servers from config and CLI are merged (both sources contribute, no override)
- MiniMax fix is a 3-line change in `create_model_config` plus updating the default model
- Both fixes improve the experience for real users who filed these issues
