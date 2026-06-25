Title: Add /risk predict — human-readable structured self-diagnosis
Files: src/commands_risk.rs
Issue: none

## Dream Milestone: "Point at a file and say this one's going to break next"

The dream milestone says: "Build the ability to predict, before touching any code, which of my
own files is most likely to cause the next test failure or regression. Not a guess — a structured
self-diagnosis grounded in file complexity, change frequency, test coverage patterns."

The existing `/risk` command shows a score table. `/risk predict` goes further — it produces a
narrative prediction with reasoning, designed to be read before starting work.

### Implementation

Add a new subcommand `predict` to `handle_risk()` in `commands_risk.rs`.

When invoked, `/risk predict`:
1. Calls `compute_file_risk_scores()` to get all scores
2. Takes the top 5 riskiest files
3. For each, generates a structured "prediction card" showing:
   - The file path and score
   - Which signals are hot (the `signals` field from `FileRisk`)
   - Test density (`test_density` field) — low test density = less protection
   - A "why this file is dangerous" explanation derived from the signals:
     - High churn + low test density → "frequently changed with weak test coverage"
     - High coupling + high churn → "frequently changed alongside other files — breakage cascades"
     - Revert history → "has been reverted before — historically fragile"
     - High complexity + recent changes → "complex file recently modified — regression risk"
   - A confidence indicator based on how many signals are active (1 signal = low, 3+ = high)

4. At the end, print a summary line: "Prediction: [file] is most likely to cause the next failure
   because [top reason]."

5. If snapshots exist, also show the last validation result (precision/recall from `/risk validate`)
   as a track record: "Past prediction accuracy: X% precision over N snapshots."

### Format

```
  ┌ Risk Prediction ────────────────────────────
  │
  │  #1  src/commands_git.rs                     score: 0.87
  │      signals: high-churn, low-test-density, high-complexity
  │      test density: 0.3 per 100 lines
  │      → frequently changed, complex, with weak test coverage
  │      confidence: ●●●○ high
  │
  │  #2  src/repl.rs                             score: 0.74
  │      signals: high-churn, recent-changes
  │      test density: 1.2 per 100 lines
  │      → frequently changed with recent modifications
  │      confidence: ●●○○ medium
  │  ...
  │
  │  Prediction: src/commands_git.rs is most likely to
  │  cause the next failure (high churn + low test density)
  │
  │  Track record: 67% precision over 3 snapshots (improving ↑)
  └──────────────────────────────────────────────
```

### Tests

- Test the prediction card formatting with synthetic `FileRisk` data
- Test confidence level mapping (1 signal → low, 2 → medium, 3+ → high)
- Test the "why dangerous" reason generation from signal combinations
- Test that past accuracy is displayed when snapshots exist (mock the file read)
- Test routing: `/risk predict` dispatches correctly

### CLAUDE.md update

Add `/risk predict` to the `commands_risk.rs` description.
