Title: Fix /tokens misleading context display and clean up stale .bak file
Files: src/commands.rs, src/commands_project.rs.bak (delete)
Issue: #189

## Context

Two small cleanup items that improve accuracy and reduce repo bloat.

### 1. /tokens misleading display (#189)

The `/tokens` command shows:
```
Context window:
  messages:    10
  context:     1.8k / 200.0k tokens
```

But "context: 1.8k" only reflects the **current in-memory messages** (via `total_tokens(&messages)`), not cumulative session usage. After compaction, old messages are removed, making the "context" number drop — confusing users who think it represents how much of the context window they've consumed.

**Fix:** Add a clarification label. In `handle_tokens()` in `src/commands.rs` (lines ~219-252):

1. Change the "context" label to "current messages" to make it clear this is the live message count, not cumulative:
   ```rust
   println!(
       "    current:     {} / {} tokens",
       format_token_count(context_used),
       format_token_count(max_context)
   );
   ```

2. Add a note after the context bar if compaction has occurred (if session_total.input > context_used, that means some tokens have been compacted away):
   ```rust
   if session_total.input > context_used + 1000 {
       println!("    {DIM}(some earlier context was compacted){RESET}");
   }
   ```

3. Update the "Session totals" section header to "Cumulative session totals:" for clarity.

### 2. Delete stale commands_project.rs.bak

`src/commands_project.rs.bak` is a 7,479-line leftover from a previous refactor. It's tracked by git and bloats the repository. Simply delete it:
```bash
git rm src/commands_project.rs.bak
```

### Tests:
- Verify existing `/tokens` tests still pass (search for any tests referencing handle_tokens)
- No new tests needed for the label change — it's cosmetic
- Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
