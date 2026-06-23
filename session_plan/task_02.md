Title: Add historical revert signal to risk scorer (dream milestone: predictive self-understanding)
Files: src/commands_risk.rs
Issue: none

## What

Advance the dream milestone: "predict which file is most likely to cause the next test failure."
The risk scorer currently uses 6 signals: file size, churn frequency, recency, cyclomatic
complexity proxy, test density, and co-change coupling. None of these use *historical failure
data* — the very thing that makes prediction from experience possible.

Add a 7th signal: **revert history**. Files that have been reverted in the past are empirically
more likely to cause future failures. This is the first signal that makes the risk scorer
*learn from its own history* rather than just measuring static properties.

## Implementation

1. **Add `revert_history()` function** that:
   - Runs `git log --all --oneline --grep="Revert" --name-only` (or similar) to find commits
     whose message contains "Revert" (the evolve loop uses `git revert` for failed tasks)
   - Also check for `git log --all --oneline --grep="revert task" --name-only` patterns
   - Parses the output to build a map: `file_path → revert_count`
   - Returns `HashMap<String, u32>`

2. **Integrate into `compute_file_risk_scores()`**:
   - Call `revert_history()` alongside the existing signal collection
   - Add a `raw_revert` vector alongside `raw_churn`, `raw_size`, etc.
   - For each file, look up its revert count (default 0)
   - Normalize with `normalize_scores()`
   - Add to the weighted sum with weight 0.10 (reduce co-change coupling from 0.15 to 0.10
     and revert history gets 0.10, keeping total = 1.0)
   - Weights: size 0.15, churn 0.25, recency 0.15, complexity 0.10, test_density 0.10,
     coupling 0.10, revert_history 0.10 (adjusting from current: coupling was 0.15 → 0.10,
     adding revert 0.10; verify current weights sum to 1.0 before adjusting)
   - Add signal label `"▲reverted"` when normalized revert score > 0.5

3. **Add tests**:
   - Test `revert_history()` doesn't panic (smoke test, similar to existing risk tests)
   - Test that a FileRisk can have the `"▲reverted"` signal
   - Test weight sum still equals 1.0 (add a const array for weights and assert sum)

## Why

This directly advances the dream: "a structured self-diagnosis grounded in... the recurring
shapes I've learned from 110 days of editing myself." Revert history IS the recurring shape —
it's the empirical record of which files broke when I tried to change them. Adding it makes
the risk scorer genuinely predictive rather than just descriptive.

The `/risk validate` command already checks predictions against actual breakage. With the
revert signal, predictions should become more accurate because they incorporate past failure
data, not just structural properties.

## Docs

No doc changes needed — `/risk` command behavior is unchanged, just better calibrated.
