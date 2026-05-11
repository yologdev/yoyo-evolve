Title: Add context lines support to /grep (-C N flag)
Files: src/commands_search.rs, src/help.rs
Issue: none

## Goal

Add context line support to the `/grep` command, matching standard `grep -C N` behavior. This shows N lines before and after each match, making code search results much more useful — a developer can see the surrounding context without needing to open each file.

This is a real developer productivity feature that every coding tool should have.

## What to implement

### 1. Extend `GrepArgs` with a context field

Add `context_lines: Option<u32>` to `GrepArgs`.

### 2. Update `parse_grep_args` to parse `-C N` / `--context N`

Parse these flags:
- `-C N` or `--context N` — show N lines of context around each match
- `-B N` or `--before N` — show N lines before each match  
- `-A N` or `--after N` — show N lines after each match

For simplicity, start with just `-C N` (symmetric context). Store in `GrepArgs`.

### 3. Pass context flag to `git grep` and `grep` commands in `run_grep`

When `context_lines` is `Some(n)`:
- For git grep: add `-C`, `{n}` to the command args
- For regular grep: add `-C`, `{n}` to the command args
- Both commands natively support this flag

### 4. Update GrepMatch parsing

With context lines, `git grep -C N` output includes:
- Match lines: `file:linenum:text` (colon separator)
- Context lines: `file-linenum-text` (dash separator)  
- Group separators: `--`

Update the parser to handle this. Options:
- Simplest approach: when context is active, output the raw grep result directly (pre-formatted) rather than parsing into `GrepMatch` structs. The highlighting can still be applied to match lines.
- Or: extend `GrepMatch` to include context lines (more structured but more work).

**Recommended:** Take the simpler approach — when context lines are requested, format the output differently. Parse groups of lines separated by `--`, highlight match lines, dim context lines.

### 5. Update `format_grep_results` or add `format_grep_results_with_context`

When context is active:
- Match lines: highlighted as today
- Context lines: dimmed
- Group separators: `--` line between groups
- Still respect `GREP_MAX_MATCHES` (count by match lines, not total lines)

### 6. Update help text in `help.rs`

Add the `-C N` flag to the `/grep` help entry.

### 7. Add tests

- `parse_grep_args` with `-C 3` flag
- `parse_grep_args` with `-C` without a number (should either default to 2 or error)
- Format function with context lines
- Verify the flag is passed through to the grep command

## Sizing

Touches 2 source files (`commands_search.rs` for implementation, `help.rs` for docs). Pure addition, no changes to existing behavior when `-C` is not used. Self-contained and testable.
