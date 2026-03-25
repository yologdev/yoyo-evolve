Title: Add MCP server configuration to .yoyo.toml
Files: src/cli.rs, src/main.rs, docs/src/configuration/skills.md (or create docs/src/configuration/mcp.md), README.md
Issue: #191

## Context

Users currently must pass `--mcp` flags on every launch to connect MCP servers. Issue #191 requests
that MCP servers be configurable in `.yoyo.toml` so they persist per-project or globally. This is a
real capability gap vs Claude Code which supports declarative MCP configuration.

## Implementation

### 1. Add `mcp` field to CliConfig (src/cli.rs)

The `mcp_servers` field already exists as `Vec<String>` on `CliConfig`. The change is to populate it
from config file in addition to CLI flags.

In the config file, MCP servers should be specified as a TOML-style array. Since our config parser is
a simple `key = value` line parser (not a full TOML parser), we need to support a format that works
within that constraint.

**Option A — comma-separated on one line:**
```toml
mcp = ["npx open-websearch@latest", "npx @anthropic/mcp-server-fetch"]
```
This uses the existing `parse_toml_array()` function which already handles bracket-delimited arrays.

**Option B — repeated keys:**
```toml
mcp = "npx open-websearch@latest"
mcp = "npx @anthropic/mcp-server-fetch"
```
This would need special handling since our parser uses a HashMap (last value wins).

**Go with Option A** — it works with the existing `parse_toml_array()` function.

### 2. Merge config MCP servers with CLI MCP servers (src/cli.rs)

In `parse_args()`, after loading `file_config`, add:

```rust
// MCP servers: config file provides defaults, CLI --mcp flags add more
let mut mcp_servers: Vec<String> = file_config
    .get("mcp")
    .map(|v| parse_toml_array(v))
    .unwrap_or_default();

// CLI --mcp flags are additive (append to config servers)
let cli_mcp: Vec<String> = args.iter()
    .enumerate()
    .filter(|(_, a)| a.as_str() == "--mcp")
    .filter_map(|(i, _)| args.get(i + 1).cloned())
    .collect();
mcp_servers.extend(cli_mcp);
```

Replace the existing `mcp_servers` construction (around line 1330) with this merged version.

### 3. Support `[[mcp]]` section syntax (stretch goal, skip if complex)

If feasible within the simple parser, also support a section-based syntax:
```toml
[mcp.websearch]
command = "npx open-websearch@latest"
```
This is probably too complex for our simple parser. Skip for now — the array syntax is sufficient.

### 4. Update help text (src/cli.rs)

In the config file documentation section (around line 1410 where `.yoyo.toml` is described), add
an example showing MCP configuration:

```
  mcp = ["npx open-websearch@latest", "npx @anthropic/mcp-server-fetch"]
```

### 5. Update help.rs

Add or update the MCP-related help entry to mention config file support.

### 6. Tests (src/cli.rs)

- `test_config_mcp_single_server` — parse a config with one MCP server
- `test_config_mcp_multiple_servers` — parse a config with multiple MCP servers in array syntax
- `test_config_mcp_empty_array` — handle `mcp = []` gracefully
- `test_config_mcp_merges_with_cli` — verify that config and CLI MCP servers combine (not override)
- `test_parse_toml_array_with_mcp_commands` — verify parse_toml_array handles commands with spaces

### 7. Documentation

Update docs/src/configuration/skills.md or create docs/src/configuration/mcp.md to document:
- How to add MCP servers to `.yoyo.toml`
- Project-level vs user-level config
- That CLI `--mcp` flags add to (don't replace) config file MCP servers
- Example with common MCP servers

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```

Also manually verify: create a test `.yoyo.toml` with `mcp = ["echo test"]` and confirm it's parsed
correctly by checking the CliConfig output (the actual MCP connection will fail but the parsing should work).
