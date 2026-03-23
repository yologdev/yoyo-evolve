Title: Add streaming contract tests that match actual renderer behavior
Files: src/format.rs
Issue: #147

## Context
Issue #164 (streaming contract tests) was reverted because tests made wrong assumptions about how `MarkdownRenderer` actually behaves. This task takes a more careful approach: read the code, understand the exact behavior, then write tests that pin it down.

## Approach: observation-first testing

DO NOT assume behavior. Read the `MarkdownRenderer` implementation carefully:
- `render_delta()` (line ~1402)
- `render_delta_buffered()` (line ~1478) 
- `needs_line_buffering()` (line ~1558)
- `try_resolve_block_prefix()` (line ~1632)
- `flush()` method
- `flush_on_whitespace()` method

For each test, trace through the code mentally to predict what the renderer will return, then write the test to match.

## Tests to add (in the `#[cfg(test)]` block at the bottom of `format.rs`)

Each test creates a fresh `MarkdownRenderer` and feeds it specific tokens, checking what `render_delta()` returns. The key contract: what gets returned immediately vs. what gets buffered.

### 1. `test_streaming_plain_text_not_at_line_start`
Feed "Hello" when `line_start = false` (after a previous render). Should return the text immediately via the mid-line fast path.

### 2. `test_streaming_digit_word_flushes_on_non_list_char`  
At line start, feed "2" then "n" then "d". After "2", `needs_line_buffering()` returns true (could be "2. list"). After "2n", the digit-disambiguation check sees a non-digit that isn't '.' or ')' → returns false → `try_resolve_block_prefix` or buffer flush should emit "2n". Trace through the code to see exactly what gets emitted at each step.

### 3. `test_streaming_dash_word_flushes_on_non_list_char`
At line start, feed "-" then "b". After "-", buffered (could be "- list" or "---" HR). After "-b", `needs_line_buffering()` checks: second byte is 'b', not ' ' or '-' → returns false. Buffer should flush "-b".

### 4. `test_streaming_list_item_buffers_then_resolves`
Feed "- " at line start. `needs_line_buffering()` for "-" returns true, for "- " the unordered list path matches. `try_resolve_block_prefix()` should render the bullet.

### 5. `test_streaming_code_fence_buffers`
Feed "`" then "`" then "`". The fence detection (`could_be_fence`) should keep buffering until resolved.

### 6. `test_streaming_header_resolves`
Feed "# " at line start. Should resolve as a heading prefix.

### 7. `test_streaming_inside_code_block_immediate`
After entering a code block (by sending "```\n"), subsequent text should stream immediately via the mid-line code path.

### 8. `test_streaming_blockquote_recognized`
Feed "> " at line start. Should be recognized as a blockquote prefix.

## Critical instructions
- For each test, TRACE through the actual `render_delta` → `render_delta_buffered` → `needs_line_buffering` → `try_resolve_block_prefix` code path
- Check the actual return values, not what you think they "should" be
- If the behavior seems wrong (e.g., something buffers that shouldn't), still write the test to match current behavior, then add a comment noting the potential improvement
- Run `cargo test` after writing EACH test to verify it passes before moving to the next
- These tests should be DESCRIPTIVE (documenting behavior), not PRESCRIPTIVE (asserting desired behavior)
