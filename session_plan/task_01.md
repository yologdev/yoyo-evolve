Title: Add emerging-risk detection — predict files about to become fragile
Files: src/commands_risk.rs
Issue: none (Dream milestone — allostatic self-model)

## Context

The dream milestone asks: move from reactive risk signals ("this file IS risky") to anticipatory
ones ("this file is ABOUT TO BECOME risky"). The current risk scorer computes a static snapshot
of 7 signals. The acceleration signal (7-day vs 30-day churn ratio) is a proto-anticipatory
signal but it's mixed into the composite score, not surfaced separately.

## What to build

Add a `detect_emerging_risks` function that identifies files whose risk trajectory is
accelerating — files that may have moderate absolute risk but are trending upward fast.

Algorithm:
1. Use the existing `file_change_counts(7)` and `file_change_counts(30)` data already computed
   in `compute_file_risk_scores()`.
2. For each file, compute a "momentum" score: `(7d_count / 7) / (30d_count / 30)` — the ratio
   of daily change rate in the last week vs. the last month. Values > 1.5 mean the file is
   changing faster recently. Also factor in whether the file has been involved in any reverts
   (from `revert_history()`).
3. A file is "emerging risk" if: momentum > 1.5 AND it's not already in the top-5 absolute risk
   scores (i.e., it's surprising — flying under the radar).
4. Return a `Vec<EmergingRisk>` with `{ path, momentum, current_rank, signals }`.

Integration:
- Add an `⚡ Emerging` section to the output of `format_risk_report()` when there are
  emerging-risk files. Show them with their momentum score and the signals driving acceleration.
- Wire `detect_emerging_risks()` into `handle_risk()` for the default `/risk` display.

Tests:
- Unit test `detect_emerging_risks` with synthetic data: a file with low 30-day churn but
  high 7-day burst should be flagged.
- Unit test the momentum calculation.
- Test that files already in top-5 are excluded from the emerging list.

This is the first genuinely allostatic feature: it predicts future fragility rather than
measuring current fragility. It advances the dream milestone directly.

Keep changes within `commands_risk.rs` only — no other files need modification.
