Title: Risk-aware edit feedback in smart_edit tool
Files: src/smart_edit.rs, src/commands_risk.rs
Issue: none (dream — action-guidance)

## Context

This is the second piece of the body schema "action-guidance" property. Task 1 wires
risk awareness into the watch fix loop (reactive — after failure). This task wires it
into the edit tool itself (proactive — before the damage is done).

Currently `smart_edit.rs` processes edit_file operations with fuzzy matching, whitespace
auto-fix, and ambiguity detection. But it has zero awareness of whether the file being
edited is historically fragile. A body that knows its arm is injured should move it more
carefully — not refuse to use it, just be more deliberate.

## What to Do

1. **Add a helper in `commands_risk.rs`** (if not already added by Task 1):
   `pub(crate) fn file_risk_summary(path: &str) -> Option<(f64, Vec<&'static str>)>`
   Returns `Some((score, signals))` if the file's risk score is above the 75th percentile,
   `None` otherwise. This is a lightweight lookup — compute all scores, find the file,
   check if it's in the top quartile.

2. **In `smart_edit.rs`**: After a successful edit (in the `execute` method of `SmartEditTool`),
   if the target file has elevated risk, append a note to the tool output:
   ```
   Note: src/watch.rs has elevated risk (score: 0.78, signals: high churn, low test density).
   Consider running tests to verify this change.
   ```
   This is purely informational — it doesn't block the edit or require confirmation.
   It's a gentle proprioceptive signal: "you just touched something fragile."

3. **Gate the lookup**: The risk computation involves scanning all files, which could be
   slow on large repos. Gate it behind a quick check: only compute if the edit succeeded
   AND the target file is in `src/` (our own code). This keeps the overhead minimal for
   typical usage where most edits are to source files.

4. **Add tests**:
   - Test `file_risk_summary` returns `None` for non-existent files
   - Test `file_risk_summary` returns `Some` for known high-risk files in this repo
   - Test that the SmartEditTool output includes the risk note when editing a high-risk file
     (this may need to mock the risk lookup or use a temp dir)

## Design Constraint

The note must NOT be added to edit failures or retries — only to successful edits.
We don't want to add noise to error messages. The risk note is a "you succeeded, and
here's something to be aware of" signal.

## Why This Matters

This extends body schema action-guidance from reactive (Task 1: after failure) to
proactive (before the next failure). The agent sees a risk note immediately after
editing a fragile file, which naturally leads it to run tests sooner. Over time,
this creates a feedback loop: fragile files get more careful treatment, which reduces
failures, which the prediction-validation system measures as improved accuracy.

## Verification

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
