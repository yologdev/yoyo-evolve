Title: Session-end learning prompt — summarize what was learned for next session
Files: src/commands_session.rs, src/commands_retry.rs, src/memory.rs
Issue: none

## What

When a user exits yoyo after a productive session, the exit summary shows files changed and tokens used — but not what was *learned* about the project. Add a "session learnings" section to the exit summary that extracts project knowledge from the conversation and offers to remember it.

This closes the cross-session memory gap: each session's exit becomes a checkpoint where project knowledge is captured.

### Changes to `src/commands_retry.rs`:
1. In `format_exit_summary`, after the existing file/cost summary, add a new section that shows any memories that were auto-remembered during the session. The memories are already in `.yoyo/memory.json` — just read the entries whose timestamps fall within the session window.
2. Add `fn session_learnings(session_start: Instant) -> Vec<String>` that loads memories from `.yoyo/memory.json` and filters to entries whose timestamp is after the session start time. Return the notes as a list.
3. If there are session learnings, append them to the exit summary:
   ```
   📝 Learned this session:
     • [build] always run cargo fmt before committing
     • Watch fix: 'cargo test' failed, fixed on attempt 1
   ```

### Changes to `src/memory.rs`:
1. Add `pub fn memories_since(memory: &ProjectMemory, since_epoch: i64) -> Vec<&MemoryEntry>` — filter entries whose timestamp parses to an epoch ≥ `since_epoch`. This reuses `parse_timestamp_to_epoch` which already exists (currently private — make it `pub(crate)`).

### Changes to `src/commands_session.rs`:
No changes needed — the exit path already goes through `format_exit_summary` in `commands_retry.rs`.

### Tests in `src/commands_retry.rs`:
- `session_learnings` returns empty vec when no memories in window
- `session_learnings` returns entries that fall within the session window
- `format_exit_summary` includes learnings section when memories exist

### Tests in `src/memory.rs`:
- `memories_since` filters correctly by timestamp
- `memories_since` returns empty for future timestamps
- `memories_since` handles entries with unparseable timestamps gracefully (skip them)
