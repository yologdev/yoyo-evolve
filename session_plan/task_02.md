Title: Reduce tool output truncation limit in piped/CI mode to prevent context overflow
Files: src/main.rs, src/format.rs
Issue: #173

## Context

Issue #173: Evolution sessions hit 400 Bad Request from the Anthropic API because the agent accumulates tool outputs across turns, eventually exceeding the model's 200K token limit. The auto-compact between REPL turns is too late — overflow happens *within* a single prompt's agentic loop.

Previous attempt (Issue #175) was reverted because it tried to modify the PromptResult enum and add proactive compaction signals, which was too complex.

**Simpler approach:** The biggest contributor to context growth is large tool outputs (30,000 chars each via `TOOL_OUTPUT_MAX_CHARS`). In piped/CI mode (evolution sessions), we can reduce this limit significantly. Each tool output at 30K chars is ~7,500 tokens. Three tool calls and we've consumed 22K tokens of context just from outputs. Halving the limit directly reduces the growth rate.

## Implementation

### 1. Add a piped-mode truncation constant

In `src/format.rs`, add:
```rust
/// Maximum tool output size in piped/CI mode (half of interactive).
/// Reduces context growth rate during evolution sessions and CI runs
/// where the user isn't watching live output anyway.
pub const TOOL_OUTPUT_MAX_CHARS_PIPED: usize = 15_000;
```

### 2. Make TruncatingTool aware of piped mode

In `src/main.rs`, the `TruncatingTool` currently hard-codes `TOOL_OUTPUT_MAX_CHARS`:
```rust
text: truncate_tool_output(&text, TOOL_OUTPUT_MAX_CHARS),
```

Change `truncate_result` to accept a `max_chars` parameter, or better: make `TruncatingTool` store the limit:

```rust
struct TruncatingTool {
    inner: Box<dyn AgentTool>,
    max_chars: usize,
}
```

Update `truncate_result` to accept `max_chars`:
```rust
fn truncate_result(mut result: ToolResult, max_chars: usize) -> ToolResult { ... }
```

Update `with_truncation` to accept `max_chars`:
```rust
fn with_truncation(tool: Box<dyn AgentTool>, max_chars: usize) -> Box<dyn AgentTool> {
    Box::new(TruncatingTool { inner: tool, max_chars })
}
```

### 3. Wire piped-mode detection into build_tools

In `build_tools()` (or at the call site in `main()`), detect piped mode and use the appropriate limit:

```rust
let max_chars = if io::stdin().is_terminal() {
    TOOL_OUTPUT_MAX_CHARS
} else {
    TOOL_OUTPUT_MAX_CHARS_PIPED
};
```

Pass `max_chars` through `with_truncation(tool, max_chars)`.

This means `build_tools` needs a parameter for the truncation limit, or the caller wraps tools after `build_tools` returns. The cleaner approach: add a `max_tool_output_chars: usize` parameter to `build_tools()`.

### 4. Tests

- `test_tool_output_max_chars_piped_smaller` — verify `TOOL_OUTPUT_MAX_CHARS_PIPED < TOOL_OUTPUT_MAX_CHARS`
- `test_truncate_result_with_custom_limit` — truncate a result with a 100-char limit, verify it's truncated
- `test_truncate_result_respects_limit` — verify the limit parameter is actually used
- `test_truncating_tool_stores_max_chars` — verify the struct field

### 5. Update build_tools signature

`build_tools` currently takes `(is_interactive: bool, perms: &PermissionConfig, dirs: &DirectoryRestrictions)`. Add `max_tool_output: usize` parameter. Update all call sites (main.rs has 2-3 call sites).

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
