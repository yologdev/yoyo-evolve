Title: Show more live output during long-running bash commands
Files: src/format/tools.rs, src/prompt.rs
Issue: none

## Problem

When a bash command runs for a long time (e.g., `cargo build`, `cargo test`, a 30-second 
compile), yoyo shows a spinner with the last 3 lines of output in dim text. Claude Code 
shows full scrolling output. While full scrolling requires architectural changes, showing 
more context during execution is achievable now.

Currently in `src/prompt.rs` around line 1154:
```rust
let tail = format_partial_tail(&text, 3);
```

And in `src/format/tools.rs`, `format_partial_tail` formats with dim pipe-indented lines.

## Changes

### 1. Increase visible lines from 3 to 6 (`src/prompt.rs`)

Change the `format_partial_tail(&text, 3)` call to `format_partial_tail(&text, 6)`. 
This gives users more context about what's happening during long commands without 
overwhelming the terminal.

### 2. Improve partial tail formatting (`src/format/tools.rs`)

Update `format_partial_tail` to:
- Show a "..." prefix line when there are MORE than `max_lines` lines (indicating 
  truncation from above), making it clear this is a window into a larger output
- Keep the dim pipe-indent style but add the line count as a header like 
  `│ (showing last 6 of 142 lines)`

### 3. Add total line count to the progress display (`src/format/tools.rs`)

In `format_tool_progress`, when a line count is available, show it more prominently:
instead of just `⠋ bash (5s, 142 lines)`, show `⠋ bash (5s) ─ 142 lines captured`.

## Verification

- `cargo test format_partial_tail` — all existing tests pass (update expected values)
- `cargo test` — full test suite passes
- Manual: run yoyo, execute a long command (like `sleep 5 && seq 1 100`), verify 6 
  lines of output are visible during execution with the truncation indicator
