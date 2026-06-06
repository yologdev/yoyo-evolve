Title: Add --auto-edit flag plumbing via global OnceLock (no AgentConfig changes)
Files: src/cli_config.rs, src/cli.rs
Issue: #466

## Problem

Issue #466 was reverted because adding `auto_edit` to `AgentConfig` requires updating ~48 construction sites across 6 files. The previous attempt missed one site at `main.rs:654`.

## New approach: global OnceLock

Instead of adding a field to `AgentConfig`, use a global `OnceLock<bool>` — the same pattern used for `VERBOSE` in `cli.rs` and `QUIET` in `format/mod.rs`. This avoids touching any `AgentConfig` construction sites.

### Changes to `src/cli_config.rs`:

1. Add a `pub auto_edit: bool` field to the `Config` struct (default `false`). This is already small and the Config struct IS properly constructed in only a few places with `..Default::default()` or equivalent.

2. Add a global `OnceLock` and accessor functions at module level:
```rust
static AUTO_EDIT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn enable_auto_edit() {
    let _ = AUTO_EDIT.set(true);
}

pub fn is_auto_edit() -> bool {
    *AUTO_EDIT.get_or_init(|| false)
}
```

### Changes to `src/cli.rs`:

1. In `parse_args`, after the `--yes` flag handling, add handling for `--auto-edit`:
```rust
"--auto-edit" => {
    config.auto_edit = true;
}
```

2. After `parse_args` returns, in the main setup (look for where `enable_verbose()` is called as the pattern), call `enable_auto_edit()` if `config.auto_edit` is true.

3. Add `--auto-edit` to the help text in `cli_help_text()` (in `help.rs` if that's where it lives, but check — the assessment says `cli.rs` has help_text). Add it near `--yes`:
```
  --auto-edit       Auto-approve file edits, still confirm shell commands
  --yes, -y         Auto-approve all operations (no confirmations)
```

4. Add a test:
   - `test_parse_auto_edit_flag`: verify parse_args sets `config.auto_edit = true`
   - `test_auto_edit_and_yes_independent`: verify both can be set, `--yes` doesn't imply `--auto-edit` and vice versa

This task is PLUMBING ONLY. The flag is parsed and stored but not yet wired into tool behavior. Task 3 wires it into `build_tools`.
