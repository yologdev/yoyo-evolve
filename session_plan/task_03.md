Title: Add /risk validate — compare predictions against actual breakage
Files: src/commands_info.rs
Issue: none (dream milestone — prediction validation loop, step 2 of 2)

## Context

This completes the prediction validation loop for the dream milestone. Task 2 added
`/risk snapshot` which saves predictions. This task adds `/risk validate` which checks
how accurate those predictions were.

## What to build

### 1. `/risk validate` subcommand

When the user runs `/risk validate`, load the most recent snapshot from
`.yoyo/risk_snapshots.jsonl`, then check which files have actually had problems since
the snapshot was taken.

### 2. "Actually broke" signals

A file "actually broke" if, since the snapshot's `git_hash`, it appears in:
- A commit whose message contains "Revert" (the file was in a reverted change)
- A commit whose message contains "fix" or "Fix" AND the file was modified (the file needed fixing)

Use `git log <snapshot_hash>..HEAD --name-only --oneline` to get all commits and their files
since the snapshot. Classify each commit by its message. Build a set of "broke" files.

### 3. Precision@K calculation

Compare the snapshot's top_10 against the "actually broke" set:
- **Hits**: files in top_10 that actually broke
- **Misses**: files that actually broke but were NOT in top_10
- **Precision@10**: hits / min(10, total_broke_files) — what fraction of breakage did we predict?
- **Recall@10**: hits / total_broke_files — what fraction of predicted files actually broke?

### 4. Output format

```
📊 Risk Prediction Validation

  Snapshot: Day 110, abc123f (3 days ago)
  Commits since: 47
  
  Predicted (top 10)    Actual Result
  ─────────────────────────────────────
  src/commands_git.rs   ✅ had fixes
  src/tool_wrappers.rs  ─  no issues
  src/watch.rs          ─  no issues
  src/cli.rs            ✅ had fixes
  ...
  
  Precision@10: 2/10 predicted files had issues
  
  Surprises (broke but not predicted):
    src/context.rs (rank #23, score 0.31)
    src/safety.rs (rank #18, score 0.42)
```

### 5. Implementation details

- `handle_risk_validate()`:
  1. Read `.yoyo/risk_snapshots.jsonl`, take the last line (most recent snapshot)
  2. Parse the JSON to get `git_hash` and `top_10`
  3. Run `git log <hash>..HEAD --name-only --oneline` to get commits since snapshot
  4. Classify commits: "revert" or "fix" by message, collect affected files
  5. Compute hits/misses/precision
  6. Format and print the report
  7. If no snapshot exists, print helpful message: "No snapshots found. Run `/risk snapshot` first."
  8. If HEAD == snapshot hash, print: "No commits since last snapshot — nothing to validate yet."

### 6. Tests

- Test the commit classification logic (revert detection, fix detection) with mock commit messages
- Test precision calculation with known inputs (e.g., 3 of 10 predicted files broke → 30%)
- Test edge cases: empty snapshot file, snapshot with no commits since

### Sizing

~100-120 lines of new code. One main function + helper for commit parsing. All in `commands_info.rs`.
