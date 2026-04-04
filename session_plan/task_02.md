Title: Smart tool output compression — strip ANSI codes and collapse repetitive lines
Files: src/format/mod.rs, src/tools.rs
Issue: #229

## What

Tool output (especially from bash commands like `cargo build`, `npm install`, `pip install`)
contains two sources of token waste:
1. **ANSI escape codes** — color/formatting sequences that the LLM can't use
2. **Repetitive lines** — long sequences of near-identical lines like "Compiling foo v1.0",
   "Downloading bar v2.0", "Installing baz"

This task adds a `compress_tool_output` function that strips ANSI codes and collapses
sequences of similar lines before the existing head/tail truncation runs. This reduces
token usage without losing semantic content — the spirit of Issue #229 (RTK integration)
without requiring an external dependency.

## Why not RTK directly?

RTK (github.com/rtk-ai/rtk) is a CLI binary, not a Rust library — it has no `lib.rs`.
It uses rusqlite, ureq, and other heavy deps. Integrating it as a library isn't possible.
Using it as a CLI subprocess would require users to install it separately.

Instead, implement the most valuable compression patterns natively.

## Implementation

In `src/format/mod.rs`, add a new function `compress_tool_output(output: &str) -> String`:

1. **Strip ANSI escape codes** — regex `\x1b\[[0-9;]*[a-zA-Z]` replaces with empty string.
   This catches SGR sequences (colors, bold) and cursor movement. Use a simple byte-scan
   or regex (the `regex` crate is already a transitive dependency).

2. **Collapse repetitive line sequences** — detect runs of 4+ lines that share a common
   prefix pattern (e.g., lines starting with "   Compiling ", "   Downloading ", "  Installing ").
   Replace with: first line, "... (N more similar lines)", last line. This preserves the
   first and last for context while removing the middle.

   Algorithm:
   - Iterate lines, extract a "category" prefix (first word + maybe second word up to ~20 chars)
   - When 4+ consecutive lines share a category, collapse them
   - Keep first and last line of each collapsed group

3. Call `compress_tool_output` inside `truncate_tool_output` BEFORE the head/tail truncation,
   so the truncation operates on already-compressed output.

In `src/tools.rs`:
- No changes needed — it already calls `truncate_tool_output` which will internally use compression.

## Tests

Add tests in `src/format/mod.rs`:
- `test_compress_strips_ansi_codes` — input with `\x1b[31m` etc, output is plain text
- `test_compress_collapses_repetitive_lines` — 10 "Compiling" lines → first + "8 more" + last
- `test_compress_preserves_non_repetitive_output` — normal mixed output unchanged
- `test_compress_short_output_unchanged` — fewer than 4 similar lines not collapsed
- `test_compress_mixed_repetitive_blocks` — two different repetitive blocks both collapsed
- `test_truncate_uses_compression` — verify truncate_tool_output on ANSI input strips codes

## What NOT to do
- Don't add new crate dependencies — use the regex crate if available, or manual byte scanning
- Don't change the truncation head/tail line counts
- Don't modify the StreamingBashTool or tool execution logic
- Keep it simple — this is v1, more sophisticated compression can come later
