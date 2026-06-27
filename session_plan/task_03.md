Title: Add selective Exa deep search for synthesis/comparison queries
Files: src/commands_web.rs, src/tools.rs
Issue: #530

## Context

Issue #530 requests selectively using Exa's `type:"deep"` mode for hard research queries (synthesis, comparison, multi-source analysis) while keeping `type:"auto"` as the default for simple lookups. The `type:"deep"` mode costs more Exa credits but returns higher-quality results for complex queries.

## What to do

### 1. Add depth parameter to WebSearchTool

In `src/tools.rs`, the `WebSearchTool` struct's `run` method currently calls `web_search(query, max_results)`. Update the tool to accept an optional `depth` parameter in the JSON input:
- `"depth": "auto"` (default if omitted) — uses current `type:"auto"` behavior
- `"depth": "deep"` — uses Exa `type:"deep"` for thorough research

Update the tool's description to mention the depth parameter:
```
"Optional 'depth' parameter: 'auto' (default) for quick lookups, 'deep' for thorough research on complex/comparison queries"
```

Parse the depth from the tool's JSON input and pass it through to the search function.

### 2. Update exa_search to accept a depth/search_type parameter

In `src/commands_web.rs`, modify `exa_search()` to accept an optional search type parameter:
```rust
pub(crate) fn exa_search(query: &str, max_results: usize) -> Result<Vec<WebSearchResult>, String>
// becomes:
pub(crate) fn exa_search(query: &str, max_results: usize, search_type: &str) -> Result<Vec<WebSearchResult>, String>
```

Update the request body format string to use the passed `search_type` instead of hardcoded `"auto"`:
```
r#"{{"query":"{}","type":"{}","numResults":{},"contents":...}}"#, escaped_query, search_type, max_results
```

### 3. Update all callers

- `web_search()` in `commands_web.rs` — pass `"auto"` by default, or accept and forward the depth parameter
- `web_search_and_read()` — same
- `handle_web_search()` — pass `"auto"`
- Any test that calls `exa_search` directly — update signature

Add `depth` parameter threading through `web_search`:
```rust
pub(crate) fn web_search(query: &str, max_results: usize, search_type: &str) -> Result<Vec<WebSearchResult>, String>
```

### 4. Add tests

- Test that WebSearchTool parses `depth` parameter from JSON input
- Test that `exa_search` formats the request body with the correct type
- Test default behavior (no depth specified → "auto")

### Verification:
- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`
