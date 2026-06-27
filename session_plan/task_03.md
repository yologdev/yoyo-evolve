Title: Add /safe-mode flag to disable all customizations for troubleshooting
Files: src/cli.rs, src/cli_config.rs, src/main.rs
Issue: none (competitive gap: Claude Code has --safe-mode)

## What

Claude Code has `--safe-mode` that disables all customizations (MCP servers, custom
commands, permission configs, skills) for troubleshooting when something is broken.
yoyo has no equivalent. When an MCP server crashes or a skill causes issues, users
have no quick way to get a clean baseline session.

## Implementation

### In `src/cli_config.rs`:

1. Add a `safe_mode: bool` field to the `Config` struct (default false).
2. Add a static `AtomicBool` for `SAFE_MODE` with getter/setter like `effort_level()`.

### In `src/cli.rs`:

3. Parse `--safe-mode` flag in `parse_args`. Set `config.safe_mode = true` and
   call `set_safe_mode(true)`.
4. Add it to the help text (`cli_help_text()` in `help.rs`).

### In `src/main.rs`:

5. When `safe_mode` is true, skip:
   - MCP server connections (the `connect_external_servers` call)
   - Custom command loading from `.yoyo/commands/` and `~/.yoyo/commands/`
   - Skills loading (don't pass `--skills`)
   - `.yoyo.toml` permission config loading
   - Auto-context injection
   
   Print a clear banner: `⚠ Safe mode: MCP servers, skills, custom commands, and
   config disabled`

6. The agent should still work normally with all builtin tools — just no extensions.

### Tests:

7. Add a test in `cli.rs` that parses `["--safe-mode"]` and verifies `config.safe_mode == true`.
8. Add a test that `is_safe_mode()` defaults to false.

### Docs:

9. Add safe-mode to `docs/src/troubleshooting/common-issues.md` as a troubleshooting step.
