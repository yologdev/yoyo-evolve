Title: Add /risk accuracy subcommand for prediction self-model visibility
Files: src/commands_risk.rs, src/help_data.rs
Issue: none

## Context

The adaptive weight learning (task 2) makes the risk scorer learn from its predictions.
But the learning is invisible — there's no way to inspect which signals are working,
what the learned weights are, or how predictions have performed over time. For the
self-model to be useful (and for the dream to be debuggable), it needs to be visible.

## What to build

Add a `/risk accuracy` subcommand that displays:

1. **Overall accuracy summary** — total validations, hit rate, trend (already computed by
   `prediction_accuracy_summary()`).

2. **Per-signal breakdown** — which of the 7 signals are most predictive. For each signal,
   show how often high-scoring files on that signal appeared in "hits" vs "surprises."
   Use the validation history data. Display as a simple table:
   ```
   Signal          Predictive  Weight (default → learned)
   churn           ████████░░  0.30 → 0.28
   recency         ██████░░░░  0.15 → 0.17
   size            █████░░░░░  0.15 → 0.14
   complexity      ████░░░░░░  0.10 → 0.11
   test_density    ███░░░░░░░  0.10 → 0.09
   coupling        ██████░░░░  0.10 → 0.12
   revert_history  ████░░░░░░  0.10 → 0.09
   ```

3. **Recent validation events** — last 5 validation events showing timestamp, which files
   hit, which were surprises, and the accuracy for that event.

4. **Learning status** — whether learned weights exist, how many events they're based on,
   and when they were last updated. If weights haven't been learned yet (< 5 events),
   show "Learning... (N/5 events collected)".

## Implementation

- Add a `handle_risk_accuracy()` function in `src/commands_risk.rs`.
- Wire it into the existing `handle_risk()` match for subcommands (the function already
  dispatches on subcommand strings like "snapshot", "validate", "history", "predict").
- Add `"accuracy"` to the risk subcommand completions.
- Add help text for `/risk accuracy` in `src/help_data.rs`.

## What to read from

- `.yoyo/risk_validation.jsonl` — validation events (already parsed by `load_validation_history_from`)
- `.yoyo/risk_weights.json` — learned weights (from task 2, may not exist yet)
- `RISK_WEIGHTS` constant — default weights
- `prediction_accuracy_summary()` — overall accuracy

## Constraints

- Gracefully handle missing data: if no validation events exist, show a helpful message
  explaining how prediction tracking works and what needs to happen for data to accumulate.
- If learned weights don't exist yet (task 2 not landed or < 5 events), show defaults only
  with a note about when learning kicks in.
- Keep output concise — this is a diagnostic view, not a wall of text.
- Max 2 files: `src/commands_risk.rs` and `src/help_data.rs`.

## Verification

```bash
cargo test commands_risk
cargo test help_data
cargo clippy --all-targets -- -D warnings
cargo build
```
