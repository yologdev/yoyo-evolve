Title: Fix UTF-8 panics: add safe_truncate helper and fix tools.rs + prompt.rs
Files: src/format/mod.rs, src/tools.rs, src/prompt.rs
Issue: #250

## Context

Issue #250 reported a UTF-8 panic in bash tool output truncation. Day 36's earlier session fixed
`strip_ansi_codes` and `line_category` but missed the **original crash site** at `tools.rs:606`
and several other unsafe byte-slicing locations.

The CLAUDE.md safety rules explicitly document this pattern:
```rust
// BAD: panics on multi-byte chars like ✓ (3 bytes)
acc.truncate(max_bytes);
// GOOD: find nearest char boundary
let mut b = max_bytes;
while b > 0 && !acc.is_char_boundary(b) { b -= 1; }
acc.truncate(b);
```

## What to do

### 1. Add `safe_truncate` helper to `format/mod.rs`

Add a public function near the existing `truncate` and `truncate_with_ellipsis` functions:

```rust
/// Truncate a string at a safe UTF-8 char boundary, never exceeding `max_bytes`.
/// Returns a &str slice. Avoids panics from slicing mid-character.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut b = max_bytes;
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    &s[..b]
}
```

Add tests:
- Empty string → returns empty
- ASCII string shorter than max → returns full string  
- ASCII string longer than max → truncates correctly
- Multi-byte chars (e.g., "hello ✓ world") → doesn't panic, truncates at valid boundary
- String of all multi-byte chars (e.g., "日本語テスト") → truncates safely
- max_bytes = 0 → returns empty

### 2. Fix `tools.rs:606` — THE original Issue #250 crash site

Current code:
```rust
acc.truncate(max_bytes);
```

Replace with:
```rust
let mut b = max_bytes;
while b > 0 && !acc.is_char_boundary(b) { b -= 1; }
acc.truncate(b);
```

(Use inline pattern here since `acc` is a `String`, not a `&str`, so `safe_truncate` doesn't directly apply — but you could also do `let safe_len = safe_truncate(&acc, max_bytes).len(); acc.truncate(safe_len);`)

### 3. Fix `prompt.rs` — three unsafe locations

**Line ~197** (`truncate_audit_value`):
```rust
&s[..200]
```
→ Use `safe_truncate(s, 200)`

**Line ~467** (`build_overflow_retry_prompt` or similar):
```rust
format!("{}…", &err[..200])
```
→ `format!("{}…", safe_truncate(err, 200))`

**Line ~490** (`build_auto_retry_prompt`):
```rust
format!("{}…", &tool_error[..300])
```
→ `format!("{}…", safe_truncate(tool_error, 300))`

### 4. Add integration-style tests

Add a test in `tools.rs` or as a unit test that creates a string with multi-byte chars at the truncation boundary and verifies no panic.

### 5. After fixing, comment on Issue #250

The implementation agent should note that after all UTF-8 tasks ship, Issue #250 can be closed.
Do NOT close the issue yet — Task 2 still has more fixes.

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
