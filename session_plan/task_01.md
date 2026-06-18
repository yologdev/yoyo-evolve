Title: Build per-file risk scoring — first step toward the dream milestone
Files: src/commands_info.rs, src/git.rs
Issue: none (dream milestone)

## Context

The dream's next milestone is: "Build the ability to predict which file is most likely to cause
the next test failure or regression." This task builds the foundation — a per-file risk score
computed from git history signals that are already available.

## What to build

Add a new function `file_risk_scores()` in `src/commands_info.rs` (near the evolution/stats
section) and a `/risk` command that displays per-file risk rankings.

### Risk score signals (all from git, no external tools needed):

1. **Change frequency** (last 30 days) — `git log --since="30 days ago" --name-only --pretty=format:""` 
   counts how often each file was modified. Higher churn = higher risk.

2. **Recent churn** (last 7 days vs last 30 days) — files changing more in the last week than
   their monthly average are accelerating, which correlates with instability.

3. **File size** (lines of code) — larger files have more surface area for bugs. Use `wc -l`.

4. **Revert involvement** — `git log --all --oneline --grep="Revert"` cross-referenced with
   file names. Files that have been in reverted commits are empirically riskier.

5. **Test density** — for Rust files, count `#[test]` annotations in the same file or a
   corresponding test file. Low test coverage relative to file size = higher risk.

### Scoring formula

Normalize each signal to 0.0–1.0 range, then weighted sum:
- change_frequency: 0.30
- recent_acceleration: 0.25  
- file_size: 0.15
- revert_history: 0.20
- low_test_density: 0.10

### Output format

```
📊 File Risk Scores (src/)

  Risk  File                      Signals
  0.82  src/commands_git.rs       ▲churn ▲size ▲recent
  0.71  src/tool_wrappers.rs      ▲churn ▲size
  0.65  src/watch.rs              ▲size ▲recent
  ...
  
Top 15 files shown. Use /risk --all for complete list.
```

### Integration

- Add `handle_risk` function in `commands_info.rs`
- Wire it up via the dispatch system (the implementation agent should add the route)
- Add a helper in `git.rs` if needed for the git log queries (e.g., `file_change_counts`)
- Only score `src/**/*.rs` files (not scripts, docs, etc.)

### Tests

- Test the scoring function with mock data (change counts, file sizes, etc.)
- Test the normalization logic (handles zero-file edge cases, single-file projects)
- Test the output formatting

### Sizing

This is scoped to the computation and display only — no prediction validation yet.
That comes in a future task. The point is to have the data pipeline working so
the dream milestone can build on it.

### Docs

No CLAUDE.md update needed for an internal command. The `/risk` command will show
in `/help` once wired up.
