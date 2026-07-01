Title: Add risk reflex effectiveness report to advance dream milestone
Files: src/commands_risk.rs
Issue: none

## Context (Dream milestone)

The dream says: "Measure whether the reflex works. Track this across sessions — if the reflex reduces failures on high-risk files compared to the baseline period before it existed, the self-model is genuinely allostatic."

The prediction accuracy infrastructure exists (`prediction_accuracy_summary`, `compute_accuracy_trend`, validation JSONL). What's missing: a way to compare accuracy across time periods to answer "is the reflex improving outcomes?"

## Implementation

Add a `/risk reflex` subcommand that reads the existing `.yoyo/risk_validations.jsonl` and produces a before/after comparison:

### 1. Add `handle_risk_reflex` function in `commands_risk.rs`

The function should:
1. Load all validation events from `risk_validations.jsonl` (reuse existing `load_validation_history_from`)
2. Split them into two halves (first half = baseline, second half = with-reflex)
3. Compute hit rate for each half
4. Show whether accuracy is improving, stable, or declining between halves
5. Show the total count, earliest/latest dates, and trend

Output format:
```
Risk Reflex Effectiveness
─────────────────────────
  Total validations: 24
  Period: 2026-06-20 → 2026-07-01

  Baseline (first half):  58.3% hit rate (12 validations)
  With reflex (second half): 66.7% hit rate (12 validations)
  Delta: +8.4% ↑ improving

  Verdict: Reflex shows early positive signal (or: insufficient data / no improvement detected)
```

If fewer than 4 validation events exist, print "Insufficient data — need at least 4 validation events to compare periods."

### 2. Wire into the RISK_SUBCOMMANDS dispatch

Add "reflex" to `RISK_SUBCOMMANDS` and route it in `handle_risk`.

### 3. Tests

- Test with empty validation file → "insufficient data"
- Test with 4 events, first 2 worse than second 2 → "improving"
- Test with 4 events, first 2 better than second 2 → "declining"
- Test with exactly equal halves → "stable"

### Why this matters for the dream

This is the measurement instrument the dream milestone calls for. Without it, "does the reflex work?" remains unanswerable. With it, every `/risk reflex` invocation produces an empirical answer. The dream can then evolve from "measure whether" to "act on the measurement."

### Files touched
- `src/commands_risk.rs` — add `handle_risk_reflex`, wire into dispatch, add "reflex" to RISK_SUBCOMMANDS, add tests

Single file change. All new code, no modifications to existing functions except adding a dispatch arm.
