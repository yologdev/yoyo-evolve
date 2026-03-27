Title: Fix config path — support ~/.yoyo.toml and clarify help text
Files: src/cli.rs
Issue: #201

## Problem

The welcome message (cli.rs line 1421) says "Create a .yoyo.toml file in your project or home directory" but the code never checks `~/.yoyo.toml` — it only checks:
1. `.yoyo.toml` in the current directory (project-level)
2. `~/.config/yoyo/config.toml` (XDG user-level)

A community commenter (@BenjaminBilbro) noted this follows the Claude Code / Codex spec, but the help text is still misleading. The fix: support `~/.yoyo.toml` as an additional search path AND update the text to be clear.

## Implementation

### Step 1: Add ~/.yoyo.toml as a config search path

In `load_config_file()` (cli.rs ~line 888), after checking project-level configs and BEFORE checking XDG:

```rust
// Check ~/.yoyo.toml (home directory shorthand)
if let Ok(home) = std::env::var("HOME") {
    let home_config = std::path::PathBuf::from(&home).join(".yoyo.toml");
    if let Ok(content) = std::fs::read_to_string(&home_config) {
        eprintln!("{DIM}  config: {}{RESET}", home_config.display());
        return parse_config_file(&content);
    }
}
```

Do the same for the three parallel functions that also load config:
- `load_permissions_from_config_file()` (~line 910)
- `load_directories_from_config_file()` (~line 930)

The search order becomes:
1. `.yoyo.toml` (project — current directory)
2. `~/.yoyo.toml` (home directory — what the welcome message promises)
3. `~/.config/yoyo/config.toml` (XDG — canonical location)

### Step 2: Update help text

In `print_help()` (line 453-455), update the config files section:
```
Config files (searched in order, first found wins):
  .yoyo.toml                  Project-level config (current directory)
  ~/.yoyo.toml                Home directory config
  ~/.config/yoyo/config.toml  User-level config (XDG)
```

### Step 3: Update welcome message

Line 1421 says "project or home directory" — this is now correct with the fix, so leave it as-is. But make sure it also mentions the XDG path as an alternative.

### Step 4: Tests

Add tests:
- `test_load_config_from_home_yoyo_toml` — set HOME env, create `$HOME/.yoyo.toml`, verify it's loaded. Use a temp dir as fake HOME.
- `test_config_precedence_project_over_home` — both `.yoyo.toml` (CWD) and `~/.yoyo.toml` exist, verify CWD wins.
- `test_config_precedence_home_over_xdg` — `~/.yoyo.toml` and XDG path both exist, verify home wins.

Note: These tests need to use temp directories and env var overrides. Check existing config tests for patterns — there should be precedent.

### Step 5: Close or comment on Issue #201

The issue response should explain: now all three paths work. Project-level (`.yoyo.toml` in CWD), home (`~/.yoyo.toml`), and XDG (`~/.config/yoyo/config.toml`). First found wins.
