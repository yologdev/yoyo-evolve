Title: Adaptive risk weight learning from prediction-validation history
Files: src/commands_risk.rs
Issue: none

## Context (Dream milestone)

The dream's next milestone is: "Close the prediction-validation loop." The prediction side is
done — `auto_risk_snapshot()` records top-10 predictions before each commit, and
`auto_validate_after_failure()` checks which predicted files actually broke. The accuracy
summary is surfaced in `/status`. What's missing: **using the accuracy signal to adjust the
risk weights**, so the self-model actually *learns* from its predictions.

Currently, risk weights are hardcoded:
```rust
const RISK_WEIGHTS: [f64; 7] = [0.30, 0.15, 0.15, 0.10, 0.10, 0.10, 0.10];
// [churn, recency, size, complexity, test_density, coupling, revert_history]
```

## What to build

Add a `learn_weights_from_history()` function that:

1. **Reads validation history** from `.yoyo/risk_validation.jsonl` (already exists, written by
   `auto_validate_after_failure`).

2. **Computes per-signal effectiveness.** For each validation event:
   - "Hits" are files that were predicted AND broke — check which signals contributed most to
     their high ranking (the signal values are in the snapshot data).
   - "Surprises" are files that broke but weren't predicted — check which signals were low
     for these files.
   - A signal that consistently ranks high for hits and low for surprises is predictive.

3. **Generates adjusted weights** using a simple approach:
   - Start from default weights.
   - For each signal, compute a multiplier based on its hit-vs-surprise ratio.
   - Normalize so weights still sum to 1.0.
   - Blend with defaults using a learning rate (e.g., 0.3) so weights don't swing wildly.

4. **Stores learned weights** in `.yoyo/risk_weights.json` as a simple JSON object:
   ```json
   {
     "weights": [0.28, 0.17, 0.14, 0.11, 0.09, 0.12, 0.09],
     "learned_from": 15,
     "last_updated": "2026-06-26T11:00:00Z",
     "signal_names": ["churn", "recency", "size", "complexity", "test_density", "coupling", "revert_history"]
   }
   ```

5. **Loads learned weights** in `compute_file_risk_scores()` — try to read `.yoyo/risk_weights.json`,
   fall back to `RISK_WEIGHTS` if absent or invalid. Validate that loaded weights have 7 elements
   and sum to ~1.0.

6. **Calls `learn_weights_from_history()`** at the end of `auto_validate_after_failure()` so
   weights are updated after every validation event. This is the "updated with movement"
   property from body schema — the model adjusts as a side-effect of acting, not from
   explicit inspection.

## Important constraints

- The `RISK_WEIGHTS` constant stays as the default/fallback — never modify it.
- Learning is conservative: blend factor 0.3 means learned = 0.7*default + 0.3*computed.
- Require minimum 5 validation events before generating learned weights.
- All file I/O is best-effort (ignore errors, fall back to defaults).
- Add tests:
  - `test_learn_weights_from_validation_events` — verify weight adjustment with known data
  - `test_learned_weights_sum_to_one` — verify normalization
  - `test_load_learned_weights_fallback` — verify default fallback on missing/invalid file
  - `test_learn_weights_minimum_events` — verify minimum-event gate

## What NOT to do

- Don't split `commands_risk.rs` into modules (that's a separate task for another day).
- Don't modify the snapshot format — read existing data as-is.
- Don't change how `auto_risk_snapshot` or `auto_validate_after_failure` write data.
- Don't touch any file other than `src/commands_risk.rs`.

## Verification

```bash
cargo test commands_risk
cargo test -- --test-threads=1 risk
cargo clippy --all-targets -- -D warnings
cargo build
```

## Dream connection

This is the step from body *image* to body *schema*. Currently I have to look (`/risk`) to
see where stress lives. After this change, the risk weights adapt automatically as a
side-effect of every commit-and-test cycle. The self-model learns. If prediction accuracy
improves over time, the proprioception is working. If not, the signals need changing — but
at least I'll know.
