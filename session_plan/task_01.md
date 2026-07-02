Title: Fix flaky test: stabilize risk score sort with filename tiebreaker
Files: src/commands_risk.rs
Issue: none (trajectory item — recurring CI failure)

## Problem

`test_top_risk_files_respects_n` is a recurring CI failure (appears 1× in the trajectory window). The test asserts that `top_risk_files(1)[0]` matches `top_risk_files(5)[0]`, but when multiple files have equal risk scores, the sort is unstable — `partial_cmp` on equal f64 values returns `Equal`, and the input order depends on HashMap iteration (non-deterministic).

The root cause is in `compute_file_risk_scores()` around line 849:
```rust
risks.sort_by(|a, b| {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(std::cmp::Ordering::Equal)
});
```

No tiebreaker means equal-scored files get random order.

## Fix

1. Add a secondary sort key (filename, alphabetical ascending) as tiebreaker when scores are equal. This makes the sort deterministic regardless of HashMap iteration order:

```rust
risks.sort_by(|a, b| {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.path.cmp(&b.path))
});
```

2. Apply the same tiebreaker to the OTHER sort sites in the file that also sort by score:
   - Line ~964: `emerging.sort_by(...)` — add `a.file.cmp(&b.file)` tiebreaker
   - Line ~1008: `result.sort_by(...)` — add `a.0.cmp(&b.0)` tiebreaker

3. Add a focused unit test that creates two `FileRisk` entries with identical scores but different paths, sorts them, and verifies deterministic ordering (alphabetical tiebreaker).

## Verification
`cargo test test_top_risk_files` should pass deterministically. `cargo clippy --all-targets -- -D warnings` clean.
