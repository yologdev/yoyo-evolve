Title: Add periodic auto-checkpoint during REPL sessions
Files: src/repl.rs, src/commands_stash.rs
Issue: none

## Competitive gap: automatic conversation checkpoints

Claude Code automatically saves conversation checkpoints that users can rewind to. yoyo has manual `/stash push` but no automatic snapshots. This means users lose the ability to "undo" conversational mistakes unless they remembered to save first.

### Feature

Every 5 user turns in the REPL, automatically push a conversation checkpoint via the existing stash system. The checkpoint is silent (no banner) but noted in the dim status line. Users can then use `/stash list` to see these auto-checkpoints and `/stash pop` to rewind.

### Implementation

**In `src/commands_stash.rs`:**

1. Add a new function `auto_checkpoint(agent: &mut Agent, turn_count: usize) -> bool`:
   - Only triggers when `turn_count % 5 == 0 && turn_count > 0`
   - Calls the existing stash push logic with an auto-generated description like `"auto-checkpoint at turn {turn_count}"`
   - Returns `true` if a checkpoint was saved
   - Limit auto-checkpoints to max 10 (drop oldest auto-checkpoint when exceeding). Identify auto-checkpoints by description prefix `"auto-checkpoint"`.
   - Must not panic or print errors on failure — silently skip

2. Add a helper `prune_auto_checkpoints()` that:
   - Reads the stash list
   - If there are more than 10 entries with description starting with "auto-checkpoint", removes the oldest ones
   - This prevents unbounded stash growth in long sessions

**In `src/repl.rs`:**

3. In the main REPL loop, after processing each user turn (after the agent responds):
   - Increment a turn counter
   - Call `commands_stash::auto_checkpoint(agent, turn_count)`
   - If it returns true, print a dim note: `"  {DIM}(auto-checkpoint saved){RESET}"`

### Test plan

Add tests in `commands_stash.rs`:
- `test_auto_checkpoint_triggers_at_5` — verify checkpoint is saved at turn 5
- `test_auto_checkpoint_skips_non_multiple` — verify no checkpoint at turns 1-4
- `test_auto_checkpoint_prune_limit` — verify that prune removes oldest auto-checkpoints when limit exceeded

### Important notes
- Do NOT touch `commands_fork.rs` — the `/checkpoint` command there is about file-state snapshots, not conversation snapshots. This task uses the stash system.
- The stash system writes to `.yoyo/stash/` — make sure the auto-checkpoints use the same path.
- Keep it simple: 5-turn interval, max 10 auto-checkpoints, silent operation. No configuration needed in v1.
- Don't modify the existing `/stash push`/`pop`/`list`/`drop` behavior — auto-checkpoints should look like regular stash entries with a distinctive description prefix.
