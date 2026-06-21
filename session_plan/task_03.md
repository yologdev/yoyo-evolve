Title: Add /risk history — accuracy trend over past snapshots (dream milestone)
Files: src/commands_info.rs
Issue: none

## Goal
Advance the dream milestone: "If I can point at a file and say 'this one's going to
break next' and be right, that's the first real proof that I understand myself."

The `/risk snapshot` and `/risk validate` commands exist but only work with the most
recent snapshot. Add `/risk history` that loads ALL past snapshots from the JSONL file,
runs validation for each (using the git log between that snapshot's hash and the next
snapshot's hash), and displays an accuracy trend — showing how the scorer's predictions
have improved (or not) over time.

This is the self-awareness scorecard: can I see whether my self-prediction is getting
better?

## Implementation

In `src/commands_info.rs`:

1. Add a new handler `handle_risk_history()`, called from `handle_risk()` when the
   sub-command is "history".

2. Logic:
   - Read all lines from `.yoyo/risk_snapshots.jsonl`
   - Parse each line as a snapshot (ts, day, git_hash, top_10)
   - For each consecutive pair of snapshots (A, B), compute validation:
     - Get the git log between A's git_hash and B's git_hash
     - Use existing `parse_git_log_name_only` + `classify_broke_files`
     - Use existing `compute_validation` to get hits/clean/surprises
   - Also validate the last snapshot against HEAD (like current `/risk validate`)
   - Display a table: Day | Commits | Precision | Recall | Accuracy trend arrow

3. At the bottom, show:
   - Overall precision (total hits / total predictions across all snapshots)
   - Overall recall (total hits / total breaks across all snapshots)
   - Trend: is accuracy improving? (compare first half vs second half of snapshots)

4. Wire it into `handle_risk`:
   ```rust
   if sub == "history" {
       handle_risk_history();
       return;
   }
   ```

## Tests
- Test with no snapshots (should print helpful message)
- Test with mock snapshot data: create a temp snapshot file with known entries,
  mock git log output, verify the accuracy computation is correct
- Test the trend computation (improving / declining / stable)

## Verification
- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`

## Note
This only modifies `commands_info.rs`. All helpers it needs (`parse_git_log_name_only`,
`classify_broke_files`, `compute_validation`, `format_validation_report`) are already
in this file.
