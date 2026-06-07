Title: Remove false dead_code annotations and truly dead functions
Files: src/commands_web.rs, src/commands_fork.rs, src/commands_session.rs
Issue: #472

## Description

Address issue #472 (Bloat) by removing unnecessary `#[allow(dead_code)]` annotations and truly dead code.

### Part A: `src/commands_web.rs` — remove 9 false `#[allow(dead_code)]` annotations

These functions ARE used (called from `src/tools.rs` via `web_search_and_read`, which calls `web_search`, which calls `url_encode`, `parse_ddg_results`, etc.). The annotations were added during scaffolding but are no longer needed.

**Steps:**
1. Remove all 9 `#[allow(dead_code)]` lines from `commands_web.rs`
2. After removing each annotation, there will be an empty line between the doc comment and the item (because the annotation was on a line between them). Remove that blank line to fix the `clippy::empty_line_after_doc_comments` warning.

**Example transformation:**
```rust
// BEFORE:
/// A single web search result.
#[allow(dead_code)]
pub(crate) struct WebSearchResult {

// AFTER:
/// A single web search result.
pub(crate) struct WebSearchResult {
```

The key is: the `#[allow(dead_code)]` line sits between a doc comment and the item. Removing it leaves a blank line (the line that was after the annotation). That blank line must also be removed.

### Part B: `src/commands_fork.rs` — remove truly dead function

`current_branch_name()` (line ~75) is never called from anywhere in the codebase. Remove the function entirely (it's ~3 lines). Keep the `#[allow(dead_code)]` annotation removal clean.

### Part C: `src/commands_session.rs` — remove truly dead function

`last_session_exists()` (line ~390) is never called from anywhere. Remove it entirely (~3 lines + doc comment).

### Verification

After all changes:
```bash
cargo clippy --all-targets -- -D warnings  # must pass clean
cargo test                                   # must pass
```

The build should produce ZERO new warnings because we're removing dead code, not adding it.

### Notes
- Do NOT remove `#[allow(dead_code)]` from `src/hooks.rs` (line 37) or `src/tool_wrappers.rs` (lines 659, 692) — those have explicit comments explaining why they exist (public API for hook implementors, wired in follow-up).
- This is purely mechanical cleanup — no behavioral changes.
