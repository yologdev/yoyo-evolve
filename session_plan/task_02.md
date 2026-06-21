Title: Add co-change coupling signal to /risk scorer (dream milestone)
Files: src/commands_info.rs
Issue: none

## Goal
Advance the dream milestone: "predict which file will break next." The current risk
scorer uses 5 signals (churn, recency, size, reverts, test-density). Add a 6th signal:
**co-change coupling** — files that are frequently modified in the same commit as
high-risk files inherit some of that risk.

This is a genuine self-understanding signal: if file A always changes with file B,
and B breaks, A is likely to break too. This is a well-known software engineering
metric (logical coupling / co-change analysis) that directly advances the dream of
predictive self-awareness.

## Implementation

In `src/commands_info.rs`, inside `compute_file_risk_scores()`:

1. After gathering the existing raw signals, add a co-change pass:
   - Run `git log --name-only --oneline -100` (last 100 commits) — reuse or adapt
     existing `parse_git_log_name_only` which is already in this file.
   - For each commit, record which `src/**/*.rs` files were co-modified.
   - For each file, compute a "coupling score": the average risk (from the other 5
     signals) of the files it most frequently co-changes with. Weight by co-change
     frequency.
   - Normalize this coupling score to 0.0–1.0 like the other signals.

2. Add the coupling signal to the weighted blend:
   - Current weights: churn 0.30, recency 0.25, size 0.15, reverts 0.20, test-density 0.10
   - New weights: churn 0.25, recency 0.20, size 0.12, reverts 0.18, test-density 0.10,
     coupling 0.15
   - These should sum to 1.0.

3. Add a `▲coupled` signal label when the coupling score is above the 0.7 threshold
   (same pattern as other signal labels).

4. Update the `FileRisk` struct if needed — no new fields required since `signals` is
   already a `Vec<&'static str>`.

## Tests
- Test that `compute_file_risk_scores()` returns results with the new coupling signal
  (existing tests should still pass; add a test that checks `▲coupled` can appear in
  signals for a known co-changed file, or at minimum that the scorer still returns
  valid results).
- Test that weights still sum to approximately 1.0.

## Verification
- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`

## Note
This is ONE source file only (`commands_info.rs`). The git parsing helper
`parse_git_log_name_only` is already in this file. No other files need changes.
