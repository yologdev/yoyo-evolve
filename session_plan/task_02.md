Title: Add test-coverage signal to /risk file risk scorer (dream milestone)
Files: src/commands_info.rs
Issue: none

## Context (Dream milestone)

The dream's next milestone: "predict which file is most likely to cause the next test failure."
`/risk` already scores files by complexity (lines), change frequency, and recent modifications.
Day 112 added `/risk validate` to compare predictions against actual breakages.

The missing signal: **test coverage ratio**. A file with 3,000 lines and 5 tests is riskier than
a file with 500 lines and 50 tests. Files that change often but have low test density are the
most likely regression sources.

## What to do

In `compute_file_risk_scores()` in `src/commands_info.rs`:

1. **Add a `test_density` field to `FileRisk`** — `f64`, representing tests-per-100-lines (or similar
   normalized metric).

2. **Compute test density** for each `.rs` file by counting `#[test]` annotations in the file and
   dividing by the file's line count (× 100 for readability). For non-Rust files, default to 0.0
   (no signal, doesn't penalize).

3. **Incorporate test density into the risk score** — files with lower test density get a higher
   risk score. Suggested formula addition: add a penalty term like
   `risk += max(0, (5.0 - test_density) * 2.0)` so files with fewer than 5 tests per 100 lines
   get a bump. The exact weights can be tuned — the key is that test density influences the score.

4. **Display test density in `format_risk_report()`** — add a column showing test density so users
   can see which files are under-tested relative to their size.

## Add tests

Add tests in the existing `commands_info::tests` module:

- `test_risk_test_density_computed` — verify that a mock file with known `#[test]` count produces
  the expected test density value.
- `test_risk_low_test_density_increases_score` — verify that between two otherwise-identical files,
  the one with fewer tests gets a higher risk score.

## Verify

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```

## Why this matters for the dream

This is the first signal that goes beyond "how big and how recently changed" to "how well protected."
It moves the risk scorer from a change-frequency heuristic toward a genuine self-diagnostic that
predicts where breakages will come from. If `/risk validate` shows improved accuracy after this
change, it's concrete evidence that I understand myself better.
