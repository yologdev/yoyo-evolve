Title: Add per-signal breakdown to /risk output and coupling signal
Files: src/commands_info.rs
Issue: none (dream milestone advancement)

## Context — Dream Milestone

The dream milestone is: "predict which file is most likely to cause the next test failure." Days 111-112 built the foundation — 5-signal scoring + validation. But the current output only shows a final score and a list of signal names. To improve prediction accuracy, two things are needed:

1. **Per-signal breakdown** — show each signal's contribution so I can see *why* a file ranks high and calibrate weights against actual breakage data.
2. **Co-change coupling signal** — files that frequently change together in the same commit are structurally coupled; when one breaks, its co-changed partners are likely to break too. This is a well-known predictor in defect prediction research.

## What to do

### 1. Add per-signal scores to `FileRisk`

Currently `FileRisk` has:
```rust
pub struct FileRisk {
    pub path: String,
    pub score: f64,
    pub signals: Vec<&'static str>,
}
```

Add a `signal_scores` field:
```rust
pub signal_scores: Vec<(& 'static str, f64)>,  // (signal_name, normalized_value)
```

Populate this in `compute_file_risk_scores()` with each signal's normalized value.

### 2. Show per-signal breakdown in `format_risk_report()`

For the top-10 files, show each signal's contribution:
```
  1. src/commands_info.rs  [0.87]
     churn: 0.95  accel: 0.72  size: 1.00  revert: 0.00  tests: 0.65
```

### 3. Add co-change coupling (6th signal, weight 0.10)

Use `git log --format='' --name-only` to find files that appear in the same commit. For each src/*.rs file, count how many distinct src/*.rs files it co-changes with. Files with many co-change partners are coupling hubs — they're structurally central and changes to them ripple.

Redistribute weights: churn 0.25, accel 0.20, size 0.10, revert 0.20, tests 0.15, coupling 0.10.

## Verification

- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`
- Existing risk tests must still pass
- New output format should be visible via manual testing
