Title: Tighten streaming latency for digit-word and dash-word patterns
Files: src/format.rs
Issue: #147

## Description

Issue #147 (streaming performance) keeps coming back. The previous attempt (#164) was reverted because tests failed. The core problem is in `needs_line_buffering()` (line ~1558 in `format.rs`): it's too conservative for patterns that start with digits or dashes but are clearly not markdown block elements.

Current behavior:
- A line starting with `2nd` buffers until 3 chars arrive because `b'0'..=b'9'` triggers `trimmed.len() < 3`
- A line starting with `-based` buffers until 3 chars because `b'-'` triggers `trimmed.len() < 3`

These are common English words, not numbered lists or horizontal rules. We can disambiguate earlier.

### What to do

1. **Write tests FIRST** — these define the streaming contract:
   - `test_streaming_digit_nonlist_flushes_early` — buffer "2n" then call `push_token("d")` — the "2n" should have flushed before the 3rd char (or flush on the 2nd char since "n" after a digit isn't "." or ")")
   - `test_streaming_dash_nonlist_flushes_early` — buffer "-b" then call next token — "-b" should flush immediately since "b" after "-" isn't space/dash
   - `test_streaming_numbered_list_still_buffers` — "1. item" should still buffer "1." until the space confirms it's a list
   - `test_streaming_dash_list_still_buffers` — "- item" should still buffer "- " correctly
   - `test_streaming_dash_hr_still_buffers` — "---" should still buffer as potential horizontal rule
   - `test_streaming_mid_line_always_immediate` — mid-line tokens never buffer (verify `line_start = false` path)
   - `test_streaming_fence_still_buffers` — "```" at line start still buffers correctly
   - `test_streaming_plain_text_immediate` — "Hello" at line start flushes on first non-ambiguous char

2. **Fix `needs_line_buffering()`** — modify the digit and dash cases:

   For the digit case (`b'0'..=b'9'`, around line 1594):
   ```
   b'0'..=b'9' => {
       // Quick disambiguation: if we see a digit followed by a non-digit
       // that isn't '.' or ')', it can't be a numbered list — flush immediately.
       // "2nd", "3rd", "100ms" → flush. "1." and "1)" → keep buffering.
       if trimmed.len() >= 2 {
           let second = trimmed.as_bytes()[1];
           if !second.is_ascii_digit() && second != b'.' && second != b')' {
               return false; // Not a numbered list pattern
           }
       }
       trimmed.len() < 2 || (trimmed.contains(". ") && ...)
   }
   ```

   For the dash case (`b'-'`, around line 1576):
   ```
   b'-' => {
       // Quick disambiguation: "-" followed by non-space, non-dash char
       // can't be a list item or horizontal rule — flush immediately.
       // "-based", "-flag" → flush. "- item", "---" → keep buffering.
       if trimmed.len() >= 2 {
           let second = trimmed.as_bytes()[1];
           if second != b' ' && second != b'-' {
               return false;
           }
       }
       trimmed.len() < 2 || trimmed.starts_with("- ") || { ... hr check ... }
   }
   ```

3. **Run all existing format.rs tests** to make sure nothing breaks. The key is that all existing tests must still pass — the reverted task failed because it broke something.

### Safety

- Don't change the behavior for actual markdown constructs (lists, HRs, fences, headers)
- Only change the "definitely not markdown" disambiguation
- Run the full test suite after changes
