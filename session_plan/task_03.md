Title: Add /permissions command showing full security posture
Files: src/commands.rs, src/help.rs, src/repl.rs
Issue: none

## Context

The assessment identifies "Permission model depth" as a gap vs Claude Code. While yoyo has
`--yes`, `--allow`, `--deny`, `--allow-dir`, `--deny-dir`, and confirmation prompts, users
have no way to inspect what permissions are active during a session. `/config` shows a brief
count of allow/deny patterns, but doesn't show the actual patterns or directory restrictions.

Adding a `/permissions` command that displays the full security posture makes the permission
model discoverable and helps users verify their security configuration.

## What to do

### 1. Add `handle_permissions` function in `commands.rs`

Create a new function:

```rust
pub fn handle_permissions(
    auto_approve: bool,
    permissions: &cli::PermissionConfig,
    dir_restrictions: &cli::DirectoryRestrictions,
) {
    println!("{DIM}  Security Configuration:\n");

    // Auto-approve status
    if auto_approve {
        println!("    {YELLOW}⚠ Auto-approve: ON{RESET} (--yes flag active)");
        println!("    {DIM}  All tool operations run without confirmation{RESET}");
    } else {
        println!("    {GREEN}✓ Confirmation: required{RESET}");
        println!("    {DIM}  Tools will prompt before write/edit/bash operations{RESET}");
    }
    println!();

    // Bash command permissions
    if permissions.is_empty() {
        println!("    Command patterns: none configured");
    } else {
        if !permissions.allow.is_empty() {
            println!("    {GREEN}Allow patterns:{RESET}");
            for pat in &permissions.allow {
                println!("      ✓ {pat}");
            }
        }
        if !permissions.deny.is_empty() {
            println!("    {RED}Deny patterns:{RESET}");
            for pat in &permissions.deny {
                println!("      ✗ {pat}");
            }
        }
    }
    println!();

    // Directory restrictions
    if dir_restrictions.is_empty() {
        println!("    Directory restrictions: none (full filesystem access)");
    } else {
        if !dir_restrictions.allow.is_empty() {
            println!("    {GREEN}Allowed directories:{RESET}");
            for dir in &dir_restrictions.allow {
                println!("      ✓ {dir}");
            }
        }
        if !dir_restrictions.deny.is_empty() {
            println!("    {RED}Denied directories:{RESET}");
            for dir in &dir_restrictions.deny {
                println!("      ✗ {dir}");
            }
        }
    }
    println!();

    // Quick reference
    println!("    {DIM}Configure with: --allow <pat>, --deny <pat>, --allow-dir <d>, --deny-dir <d>");
    println!("    Or in .yoyo.toml: allow = [...], deny = [...]{RESET}\n");
}
```

### 2. Add command to KNOWN_COMMANDS in `commands.rs`

Add `/permissions` to the `KNOWN_COMMANDS` array.

### 3. Wire up in `repl.rs`

Add dispatch for `/permissions` in the REPL command matching, similar to how `/config` is handled:

```rust
"/permissions" => {
    commands::handle_permissions(
        agent_config.auto_approve,
        &agent_config.permissions,
        &agent_config.dir_restrictions,
    );
    continue;
}
```

### 4. Add help text in `help.rs`

Add a description for `/permissions` in the help system:
- Short: "Show active security and permission configuration"
- Long: Explain what it shows (auto-approve status, allow/deny patterns, directory restrictions)

### 5. Add tests in `commands.rs`

- Test with no permissions configured (all defaults)
- Test with allow/deny patterns
- Test with directory restrictions
- Test with auto_approve true vs false

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
