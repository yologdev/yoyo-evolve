Title: Add system_prompt and system_file config keys to .yoyo.toml
Files: src/cli.rs
Issue: none

## Description

Currently, the system prompt can only be customized via CLI flags (`--system "text"` or `--system-file path`). The config file parser (`parse_config_file`) reads `provider`, `base_url`, `api_key`, `model`, and `thinking` — but **not** `system_prompt` or `system_file`. This means teams can't set a per-project system prompt in `.yoyo.toml` without every developer passing `--system-file` on the command line.

### What to do

1. **Write tests first** in the `#[cfg(test)]` module of `src/cli.rs`:
   - `test_config_system_prompt_key` — config with `system_prompt = "You are a Go expert"` should be used when no `--system` CLI flag is passed
   - `test_config_system_file_key` — config with `system_file = "prompt.txt"` should read from that file path
   - `test_config_system_file_overrides_system_prompt` — when both are present, `system_file` wins (same precedence as CLI)
   - `test_cli_system_overrides_config` — CLI `--system` flag should override config file `system_prompt`
   - `test_cli_system_file_overrides_config` — CLI `--system-file` should override config file `system_file`

2. **Implementation** in `parse_args()` (around line 1043-1064 in `src/cli.rs`):
   - After determining `custom_system` and `system_from_file` from CLI args, check if neither was provided
   - If no CLI system prompt, check `file_config.get("system_file")` first — if present, read the file contents
   - If no CLI system prompt and no config `system_file`, check `file_config.get("system_prompt")` — if present, use it directly
   - The precedence chain is: CLI `--system-file` > CLI `--system` > config `system_file` > config `system_prompt` > default `SYSTEM_PROMPT`
   - Handle the error case where `system_file` in config points to a non-existent file (eprintln + exit 1, same pattern as CLI)

3. **Update help text** in `print_help()` to mention that `system_prompt` and `system_file` are supported in `.yoyo.toml`.

4. **Update `/config` display** to show the active system prompt source (default, CLI flag, or config file).

### Testing approach

The tests should exercise the parsing logic directly. Since `parse_args` reads from actual files for `--system-file`, the config file system_file tests should create temp files. The config system_prompt test can use the `parse_config_file` function to verify the key is parsed, then verify it flows through to the final `AgentConfig.system_prompt`.
