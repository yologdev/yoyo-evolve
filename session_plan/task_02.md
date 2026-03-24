Title: Add /apply command for applying diffs and patches
Files: src/commands_project.rs, src/commands.rs, src/help.rs
Issue: none

## Context

One of the biggest practical gaps between yoyo and Claude Code is in multi-file editing workflows. When users have a diff or patch (from `git diff`, from a code review, from another tool), they currently have to manually apply it. Claude Code can take a diff and apply it because the model understands diffs natively — but having a first-class `/apply` command makes this a one-step operation.

This is also useful for yoyo's own evolution: when a task generates a partial fix that gets reverted, the diff could be saved and re-applied cleanly.

## Implementation

### 1. Add `/apply` command handler in `src/commands_project.rs`

```rust
pub fn handle_apply(input: &str) -> String
```

Parse the input to determine the source:
- `/apply` with no args — apply from clipboard/stdin (read from `git diff` output piped in, or prompt user)
- `/apply <file>` — read a patch file and apply it with `git apply`
- `/apply --check <file>` — dry-run, show what would change without applying

The implementation should:
1. Parse the argument (file path or `--check` flag)
2. If a file is given, verify it exists and read its contents
3. Run `git apply [--check] [--stat] <file>` to apply the patch
4. Show a summary of affected files and results
5. If `--check` is used, show the stat without applying
6. Handle errors gracefully (not a git repo, malformed patch, conflicts)

For the "no args" case:
- If stdin is not a terminal (piped mode), read the patch from stdin
- Otherwise, look for unstaged changes and offer to show/stage them

### 2. Add patch application helpers

```rust
/// Apply a patch file using git apply. Returns (success, output_message).
pub fn apply_patch(path: &str, check_only: bool) -> (bool, String) {
    let mut args = vec!["apply"];
    if check_only {
        args.push("--check");
    }
    args.push("--stat");  // show stat alongside
    args.push(path);
    // Run git apply and capture result
}

/// Apply a patch from string content (writes to temp file, then applies).
pub fn apply_patch_from_string(patch: &str, check_only: bool) -> (bool, String) {
    // Write to temp file, call apply_patch, clean up
}
```

### 3. Add to KNOWN_COMMANDS in `src/commands.rs`

Add `/apply` to the KNOWN_COMMANDS array.

### 4. Add help text in `src/help.rs`

Add detailed help for `/apply`:
```
/apply [file] — Apply a diff or patch file

Usage:
  /apply patch.diff       Apply a patch file
  /apply --check file     Dry-run: show what would change
  echo "..." | /apply     Apply patch from stdin (piped mode)

Uses git apply under the hood. Supports unified diff format.
```

### 5. Wire into REPL dispatch in `src/commands.rs`

Add the `/apply` case to the main command dispatch (where other `/` commands are handled).

### 6. Tests

- `test_apply_in_known_commands` — verify /apply is in KNOWN_COMMANDS
- `test_apply_in_help_text` — verify /apply appears in help
- `test_apply_parse_args_file` — parse `/apply patch.diff`
- `test_apply_parse_args_check` — parse `/apply --check patch.diff`
- `test_apply_parse_args_empty` — parse `/apply` returns None for file
- `test_apply_patch_nonexistent_file` — graceful error for missing file
- `test_apply_patch_from_string_empty` — empty string returns error
- `test_apply_help_text_exists` — command_help("apply") returns Some
