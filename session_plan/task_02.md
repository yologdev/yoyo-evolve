Title: Enhance /diff with --staged, file filtering, and --name-only
Files: src/commands_git.rs, src/help.rs
Issue: none

## Context

Currently `/diff` shows all changes (staged + unstaged) with no ability to filter. Claude Code and standard git workflows often need:
- See only staged changes (what would be committed)
- See changes for a specific file
- See just the list of changed files without the full patch

These are basic git-workflow operations that `/diff` should support.

## Implementation

### 1. Refactor `handle_diff` to accept arguments

Change `pub fn handle_diff()` to `pub fn handle_diff(input: &str)` in `src/commands_git.rs`.

Parse the input after `/diff`:
- `/diff` — current behavior (all changes)
- `/diff --staged` or `/diff --cached` — only staged changes
- `/diff --name-only` — list changed filenames only (no patches)
- `/diff <file>` — diff for a specific file
- `/diff --staged <file>` — staged diff for a specific file

Parser: extract flags first, then remaining args are file paths.

```rust
struct DiffOptions {
    staged_only: bool,
    name_only: bool,
    file: Option<String>,
}

fn parse_diff_args(input: &str) -> DiffOptions {
    let rest = input.strip_prefix("/diff").unwrap_or("").trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut staged_only = false;
    let mut name_only = false;
    let mut file = None;
    
    for part in parts {
        match part {
            "--staged" | "--cached" => staged_only = true,
            "--name-only" => name_only = true,
            _ => file = Some(part.to_string()),
        }
    }
    DiffOptions { staged_only, name_only, file }
}
```

### 2. Implement filtered diff display

In the refactored `handle_diff`:

**Name-only mode**: Run `git diff --name-only [--cached] [file]` and display just the file list.

**Staged-only mode**: Run `git diff --cached [--stat] [file]` instead of `git diff --stat`. Show only staged changes with the colored patch.

**File filter**: Append the file path to the git commands.

### 3. Update help text

In `src/help.rs`, update the "diff" help entry:
```
"/diff [options] [file] — Show git changes\n\n\
 Usage:\n\
 \x20 /diff                    Show all uncommitted changes\n\
 \x20 /diff --staged           Show only staged changes\n\
 \x20 /diff --name-only        List changed filenames only\n\
 \x20 /diff src/main.rs        Show changes for a specific file\n\
 \x20 /diff --staged main.rs   Staged changes for a specific file\n\n\
 Aliases: --staged, --cached"
```

### 4. Update tab completion

In `src/commands.rs` `command_arg_completions`, add completions for `/diff`:
- Flag completions: `--staged`, `--cached`, `--name-only`
- File completions: files from `git diff --name-only`

### 5. Update caller

Wherever `handle_diff()` is called in `src/repl.rs`, pass the input string so arguments are forwarded.

### 6. Tests

In `src/commands_git.rs`:
- `test_parse_diff_args_empty` — default options (no staged, no name-only, no file)
- `test_parse_diff_args_staged` — `--staged` flag
- `test_parse_diff_args_cached` — `--cached` alias
- `test_parse_diff_args_name_only` — `--name-only` flag
- `test_parse_diff_args_file` — file argument
- `test_parse_diff_args_staged_and_file` — combined `--staged src/main.rs`
- `test_parse_diff_args_all_flags` — `--staged --name-only src/main.rs`

In `src/help.rs`:
- `test_diff_help_mentions_staged` — help text mentions --staged
