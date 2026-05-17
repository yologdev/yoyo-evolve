Title: Expand test coverage for help.rs — command help lookups and completions
Files: src/help.rs
Issue: none

## Problem

`help.rs` is 2,888 lines with only 48 tests — the lowest test density of any large source file. Help text correctness directly impacts user experience: wrong command names, missing descriptions, or broken formatting all erode trust. The existing tests cover basic structure but not the per-command help lookup paths or edge cases.

## What to test

### 1. `command_short_description` coverage
- Test that every command in `KNOWN_COMMANDS` has a non-empty short description
- Test specific important commands return expected descriptions
- Test unknown commands return a sensible fallback

### 2. `handle_help_command` per-command help
- Test that `/help compact`, `/help diff`, `/help model`, `/help spawn`, `/help config` etc. produce non-empty output containing relevant keywords
- Test `/help` with an unknown command suggests alternatives or shows general help
- Test case sensitivity (if applicable)

### 3. `help_command_completions` 
- Test that completions for "/help " include all documented commands
- Test that completions filter correctly with a partial prefix (e.g., "/help co" matches "compact", "config", "commit", "copy", "context")
- Test empty prefix returns all commands

### 4. `cli_help_text` and `help_text` 
- Test that `cli_help_text()` contains all major flag names (--model, --thinking, --no-tools, etc.)
- Test that `help_text()` (REPL help) contains all major slash commands
- Test that neither function panics

### 5. Edge cases
- Test `/help help` (recursive help)
- Test `/help` with extra whitespace
- Test that the help output doesn't contain broken formatting (unclosed bold, etc.)

## Sizing
This is purely additive — no changes to existing code, just new `#[test]` functions in the existing `mod tests` block at the bottom of `help.rs`. Target: 20-30 new tests.
