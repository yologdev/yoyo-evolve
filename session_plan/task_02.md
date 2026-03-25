Title: Fix /tokens display to be clearer about what "current" means
Files: src/commands.rs
Issue: #189

## Context

Issue #189 reports that `/tokens` shows a misleading token count. The "current" context line shows
yoyo's estimate of in-memory message tokens (e.g., 29.8k), but the actual server-side context usage
can be significantly higher (e.g., 47.5k) due to system prompts, tool schemas, and tokenizer
differences. The "cumulative session totals" section shows all tokens ever used (including compacted
ones), which further confuses users.

The reporter partially retracted the bug after exploring more, but the display is still confusing.
The fix is to make the labels and explanations clearer, not to change the underlying calculation
(which is the best estimate we have).

## Implementation

### 1. Improve /tokens labels and add explanatory notes (src/commands.rs)

In `handle_tokens()`, make these changes:

**a) Rename "current" to "estimated" and add a note:**
```rust
println!("    estimated:   {} / {} tokens",
    format_token_count(context_used),
    format_token_count(max_context)
);
```

**b) Add a note explaining the estimate:**
After the bar, add:
```rust
println!("    {DIM}(estimate from message content — actual may be higher due to system prompt + tool schemas){RESET}");
```

**c) Improve the compaction note:**
The current hint `(some earlier context was compacted)` only appears when session_total.input > context_used + 1000.
Make it more informative:
```rust
if session_total.input > context_used + 1000 {
    let compacted = session_total.input - context_used;
    println!("    {DIM}(~{} tokens compacted during this session){RESET}",
        format_token_count(compacted));
}
```

### 2. Tests

- `test_handle_tokens_labels` — if there are existing tests for handle_tokens output, update them.
  If not, this is display-only and hard to unit test. At minimum, ensure it compiles and doesn't panic.
- Verify existing tests still pass after label changes.

### 3. Consider: show system prompt token estimate

If yoagent provides a way to estimate system prompt tokens, add that as a separate line:
```
    system:      ~4.0k tokens (estimated)
    messages:    29.8k tokens
    total:       ~33.8k / 200.0k tokens
```
Only do this if the data is readily available. Don't over-engineer.

## Verification

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
