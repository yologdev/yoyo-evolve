Title: Tighten streaming flush for digit-word and dash-word patterns
Files: src/format.rs
Issue: #147 (streaming latency)

## Context

Issue #164 (reverted due to test failures) attempted to tighten the `needs_line_buffering()` function so sequences like "200-line" or "2nd" flush faster. The logic is sound but the tests failed — likely because the test assertions didn't match the actual rendering behavior.

This task retries the optimization with correct tests that verify behavior through the `MarkdownRenderer::render_delta()` API (not internal state).

## Current behavior

In `format.rs`, `needs_line_buffering()` checks the `line_buffer` at line start:
- If buffer starts with a digit (`b'0'..=b'9'`), it buffers until `len >= 3` to distinguish "1. list" from "100"
- If buffer starts with `-`, it buffers until `len >= 3` to distinguish "- list" from "---"

This means "2nd" buffers all 3 chars before flushing, and "-based" buffers 3 chars. Both could flush earlier.

## Implementation

### 1. Optimize digit case in `needs_line_buffering()`

Current:
```rust
b'0'..=b'9' => buf.len() < 3,
```

New logic:
```rust
b'0'..=b'9' => {
    if buf.len() < 2 {
        true // Need at least 2 chars to decide
    } else {
        // If second char is not '.' or ')', it's not a numbered list — flush
        let second = buf[1];
        second == b'.' || second == b')' || (second >= b'0' && second <= b'9' && buf.len() < 3)
    }
}
```

This means:
- "1." → buffer (could be "1. list")
- "1)" → buffer (could be "1) list")  
- "12" → buffer one more (could be "12." or "123")
- "1x" → flush immediately (not a numbered list)
- "2n" → flush immediately (not a numbered list)

### 2. Optimize dash case similarly

Current:
```rust
b'-' => buf.len() < 3,
```

New logic:
```rust
b'-' => {
    if buf.len() < 2 {
        true // Need at least 2 chars
    } else {
        let second = buf[1];
        // "- " = list item, "--" = potential HR, anything else = flush
        second == b' ' || second == b'-'
    }
}
```

This means:
- "- " → buffer (list item)
- "--" → buffer (could be HR "---")
- "-b" → flush immediately (just a word like "-based")

### 3. Tests (through render_delta API)

Write tests that feed characters one at a time through `render_delta()` and check when output appears:

- `test_streaming_digit_word_flushes_early`: Feed "2", then "n", then "d" → output should appear after "n" (2 chars, not 3)
- `test_streaming_dash_word_flushes_early`: Feed "-", then "b" → output should appear after "b" (2 chars, not 3)
- `test_streaming_numbered_list_buffers`: Feed "1", then "." → should still buffer (waiting for space after)
- `test_streaming_dash_list_buffers`: Feed "-", then " " → should still buffer (list detection)
- `test_streaming_dash_hr_buffers`: Feed "-", then "-" → should still buffer (HR detection)

**Key insight from the revert:** Test through `render_delta()` output, not internal state. Check what gets printed, not what the buffer contains.
