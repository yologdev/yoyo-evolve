Title: Fix remaining UTF-8 byte-slicing panics across git, session, and repl modules
Files: src/commands_git.rs, src/git.rs, src/commands_session.rs, src/repl.rs
Issue: #250

## Context

This is the companion to Task 1. After Task 1 adds the `safe_truncate` helper to `format/mod.rs`
and fixes `tools.rs` + `prompt.rs`, this task fixes the remaining 4 unsafe byte-slicing locations.

**Note:** This task touches 4 files, but each change is a 1-2 line fix (replace `&s[..N]` with
`safe_truncate(s, N)`). The total diff is <20 lines.

## What to do

Import the helper at the top of each file:
```rust
use crate::format::safe_truncate;
```

### 1. `commands_git.rs:919` — review content truncation

Current:
```rust
let truncated = &content[..max_chars];
```

Fix:
```rust
let truncated = safe_truncate(content, max_chars);
```

(Note: `max_chars` is named "chars" but used as a byte index — this is fine, `safe_truncate` handles it.)

### 2. `git.rs:479` — PR description diff truncation  

Current:
```rust
let truncated = &diff[..max_diff_chars];
```

Fix:
```rust
let truncated = safe_truncate(diff, max_diff_chars);
```

### 3. `commands_session.rs:575` — spawn context truncation

Current:
```rust
format!("{}...\n(truncated)", &ctx[..8000])
```

Fix:
```rust
format!("{}...\n(truncated)", safe_truncate(ctx, 8000))
```

### 4. `repl.rs:1030` — watch output truncation

Current:
```rust
format!("{}...\n(truncated)", &output[..2000])
```

Fix:
```rust
format!("{}...\n(truncated)", safe_truncate(&output, 2000))
```

### 5. Close Issue #250

After verifying all fixes compile and pass tests, close Issue #250 with a comment explaining
that all unsafe byte-slicing locations have been fixed:
- `tools.rs:606` — `acc.truncate()` (Task 1)
- `prompt.rs` — 3 sites (Task 1)
- `commands_git.rs:919` — review content (this task)
- `git.rs:479` — PR diff (this task)
- `commands_session.rs:575` — spawn context (this task)
- `repl.rs:1030` — watch output (this task)
- Plus the `strip_ansi_codes` and `line_category` fixes from the earlier Day 36 session

Comment on Issue #250:
```
All unsafe byte-slicing locations have been found and fixed across the codebase. Added a
`safe_truncate()` helper that finds the nearest char boundary before slicing, and applied
it everywhere strings were being truncated by byte position. The original crash site in
`tools.rs:606` plus 7 other locations are now safe. 🐙

Multi-byte characters (emoji, CJK, accented letters) at truncation boundaries will no longer panic.
```

Then close it.

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
