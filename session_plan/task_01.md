Title: Fix /web panic on non-ASCII HTML content
Files: src/commands_file.rs
Issue: #188

## Context

This is a **CRITICAL crash bug** — `/web` causes a thread panic on pages with non-ASCII characters. The `strip_html_tags` function in `commands_file.rs` uses `bytes[i] as char` casting (lines 56, 60), which is fundamentally wrong for multi-byte UTF-8 characters. When the input HTML contains characters like `·` (2 bytes), `—` (3 bytes), or emoji (4 bytes), the byte-to-char cast produces garbage, and subsequent string slicing panics at `floor_char_boundary`.

## Root Cause

Lines 56 and 60 in `strip_html_tags`:
```rust
cleaned.push(bytes[i] as char);  // WRONG: treats each byte as a char
```
This converts individual bytes of multi-byte UTF-8 sequences into separate `char` values. For example, `·` (U+00B7, bytes `0xC2 0xB7`) becomes two chars: `Â` and `·`. The resulting string is corrupt — it's valid chars but represents garbled text, and the length is wrong.

## Fix

Replace the byte-level iteration with proper char-based or str-slice iteration. The function needs to:

1. **First pass (skip-tag removal):** Instead of iterating by bytes and pushing `bytes[i] as char`, iterate by character positions. Use `html.char_indices()` or work with string slices instead of bytes. For the skip-tag detection, use `html_lower[i..]` string slicing (which is already done for `find`), but advance by char boundaries, not byte indices.

2. **Second pass (tag conversion):** Same issue — `cleaned_bytes[j]` is used for comparison, but `cleaned.push(bytes[i] as char)` from pass 1 means `cleaned` might have corruption. If pass 1 is fixed to produce valid UTF-8, pass 2 should also work on chars/string slices.

### Recommended approach:
The simplest correct fix: work entirely with `&str` slices in both passes. For the first pass:
- Use `html.find('<')` and string slicing instead of byte indexing
- When checking skip tags, use `html_lower[pos..].starts_with(&open)` (already similar to current logic)
- When a skip tag is found, find closing tag and jump past it
- Copy non-skip content using `&html[start..end]` string slices (preserving UTF-8)

For the second pass, similar approach — find `<` in the cleaned string, extract tag content, replace with formatting.

### Tests to add:
- `strip_html_non_ascii_content` — input with `·`, `—`, `é`, `ñ`, emoji
- `strip_html_non_ascii_in_skip_tag` — `<script>alert('café')</script>` should not panic
- `strip_html_chinese_japanese` — CJK characters in content
- `strip_html_mixed_multibyte` — mixed ASCII and multi-byte throughout

### Important constraints:
- Keep the existing test suite passing — all current `strip_html_*` tests must still pass
- Keep the function signature identical: `pub fn strip_html_tags(html: &str, max_chars: usize) -> String`
- The `max_chars` truncation using `floor_char_boundary` is already correct for the final output, but the upstream corruption means it receives garbage. Fix the upstream and the truncation will work correctly.
