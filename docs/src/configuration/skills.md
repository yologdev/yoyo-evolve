# Skills

Skills are markdown files that provide additional context and instructions to yoyo. They're loaded at startup and added to the agent's context.

## Usage

```bash
yoyo --skills ./skills
```

You can pass multiple skill directories:

```bash
yoyo --skills ./skills --skills ./my-custom-skills
```

## What is a skill?

A skill file is a markdown file with YAML frontmatter. It contains instructions, rules, or context that the agent should follow. For example:

```markdown
---
name: rust-expert
description: Rust-specific coding guidelines
tools: [bash, read_file, edit_file]
---

# Rust Guidelines

- Always use `clippy` before committing
- Prefer `?` over `.unwrap()` in production code
- Write tests for every public function
```

## Built-in skills

yoyo's own evolution is guided by skills in the `skills/` directory of the repository:

- **evolve** — rules for safely modifying its own source code
- **communicate** — writing journal entries and issue responses
- **self-assess** — analyzing its own capabilities
- **research** — searching the web and reading docs
- **release** — evaluating readiness for publishing

## MCP servers

yoyo can connect to [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) servers, giving the agent access to external tools provided by any MCP-compatible server. Use the `--mcp` flag with a shell command that starts the server via stdio:

```bash
yoyo --mcp "npx -y @modelcontextprotocol/server-filesystem /home/user/projects"
```

The flag is repeatable — connect to multiple MCP servers in a single session:

```bash
yoyo \
  --mcp "npx -y @modelcontextprotocol/server-filesystem /tmp" \
  --mcp "npx -y @modelcontextprotocol/server-github" \
  --mcp "python my_custom_server.py"
```

### MCP in config files

You can also configure MCP servers in `.yoyo.toml`, `~/.yoyo.toml`, or `~/.config/yoyo/config.toml`, so they connect automatically without needing CLI flags:

```toml
mcp = ["npx -y @modelcontextprotocol/server-filesystem /tmp", "npx open-websearch@latest"]
```

MCP servers from the config file are merged with any `--mcp` CLI flags — both sources contribute. CLI flags are additive, not overriding.

Each `--mcp` command is launched as a child process. yoyo communicates with it over stdio using the MCP protocol, discovers the tools it offers, and makes them available to the agent alongside the built-in tools.

## OpenAPI specs

You can give yoyo access to any HTTP API by pointing it at an OpenAPI specification file. yoyo parses the spec and registers each endpoint as a callable tool:

```bash
yoyo --openapi ./petstore.yaml
```

Like `--mcp`, this flag is repeatable:

```bash
yoyo --openapi ./api-v1.yaml --openapi ./internal-api.json
```

Both YAML and JSON spec formats are supported.

## Additional configuration flags

Beyond skills, MCP, and OpenAPI, a few other flags fine-tune agent behavior:

### `--temperature <float>`

Set the sampling temperature (0.0–1.0). Lower values make output more deterministic; higher values make it more creative. Defaults to the model's own default.

```bash
yoyo --temperature 0.2   # More focused/deterministic
yoyo --temperature 0.9   # More creative/varied
```

### `--max-turns <int>`

Limit the number of agentic turns (tool-use loops) per prompt. Defaults to 50. Useful for keeping costs predictable or preventing runaway tool loops:

```bash
yoyo --max-turns 10
```

Both flags can also be set in `.yoyo.toml`:

```toml
temperature = 0.5
max_turns = 20
```

### `--no-bell`

Disable the terminal bell notification that rings after long-running prompts (≥3 seconds). By default, yoyo sends a bell character (`\x07`) when a prompt completes, which causes most terminals to flash the tab or play a sound — useful when you switch away while waiting. Disable it with the flag or environment variable:

```bash
yoyo --no-bell
YOYO_NO_BELL=1 yoyo
```

### `--no-update-check`

Skip the startup update check. On startup (interactive REPL mode only), yoyo checks GitHub for a newer release and shows a notification if one exists. The check uses a 3-second timeout and fails silently on network errors. Disable it with the flag or environment variable:

```bash
yoyo --no-update-check
YOYO_NO_UPDATE_CHECK=1 yoyo
```

The update check is automatically skipped in non-interactive modes (piped input, `--prompt` flag).

## Error handling

If the skills directory doesn't exist or can't be loaded, yoyo prints a warning and continues without skills:

```
warning: Failed to load skills: ...
```

This is intentional — skills are optional and should never prevent yoyo from starting.
