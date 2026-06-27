Title: Extract risk validation module from commands_risk.rs
Files: src/commands_risk.rs, src/risk_validation.rs, src/commands_info.rs
Issue: none

## Context

`commands_risk.rs` is now the largest file in the codebase at 4,897 lines — it's predicting its own riskiness. The assessment explicitly flags this as a structural problem approaching the stress level that triggered the Day 114 extraction of `commands_info.rs`.

This is also dream work: the risk module IS the proprioception system. If the proprioception system is itself fragile and hard to maintain, that undermines the dream. Self-regularization through self-modeling means the self-model should be simple enough to maintain.

## What to do

Extract the **validation and accuracy** subsystem (~lines 1466–2662, roughly 1,200 lines of implementation + their associated tests) into a new `src/risk_validation.rs` module.

### What moves to `risk_validation.rs`:

All validation/accuracy types and functions:
- `RISK_VALIDATION_PATH` constant
- `auto_validate_after_failure` and `auto_validate_after_failure_to`
- `ValidationEvent` struct and `AccuracyTrend` enum and `AccuracyStats` struct  
- `load_validation_history_from`, `parse_validation_events`
- `compute_accuracy_trend`, `compute_accuracy_stats`
- `format_accuracy_report`
- `RichValidationEvent`, `parse_rich_validation_events`
- `signal_bar`, `format_signal_breakdown`, `format_recent_events`, `format_learning_status`
- `handle_risk_accuracy`
- `prediction_accuracy_summary` and `prediction_accuracy_summary_from`
- `CommitEntry`, `parse_git_log_name_only`, `classify_broke_files`
- `ValidationResult`, `compute_validation`, `format_validation_report`
- `ParsedSnapshot`, `parse_all_snapshots`
- `HistoryValidation`, `precision`, `compute_trend`, `format_history_report`
- All tests for the above functions (move relevant `#[cfg(test)]` tests)

### What stays in `commands_risk.rs`:
- `FileRisk` struct, scoring weights, `compute_file_risk_scores`, normalization
- `top_risk_files`, `risk_context_for_files`, `format_risk_report`
- `handle_risk` (the command dispatcher) — but update it to call into `risk_validation` for accuracy/validation subcommands
- Prediction cards, snapshots (`handle_risk_predict`, `handle_risk_snapshot`, `auto_risk_snapshot`)
- `RISK_SUBCOMMANDS` constant
- Weight learning (`load_learned_weights`, `compute_adjusted_weights`, `learn_weights_from_history`)
- Test reference map, co-change coupling

### Wire-up:
- In `commands_risk.rs`: add `use crate::risk_validation::*;` or specific imports as needed
- In `risk_validation.rs`: import what it needs from `commands_risk` (e.g., `FileRisk`, `compute_file_risk_scores`, snapshot types)
- In `main.rs` (or wherever modules are declared): add `mod risk_validation;`
- In `commands_info.rs`: update any `use crate::commands_risk::prediction_accuracy_summary` to point to `crate::risk_validation::prediction_accuracy_summary` if needed
- Ensure `auto_validate_after_failure` remains `pub(crate)` accessible from `watch.rs`
- Ensure `prediction_accuracy_summary` remains `pub(crate)` accessible from `commands_info.rs`

### Verification:
- `cargo build` must pass
- `cargo test` must pass (all existing risk tests must still work)
- `cargo clippy --all-targets -- -D warnings` must pass
- The extracted module should be ~1,200 lines of impl + tests, bringing `commands_risk.rs` down to ~3,600 lines

### Update CLAUDE.md:
Add `risk_validation.rs` to the architecture section with a one-line description like:
`risk_validation.rs` — risk prediction validation, accuracy tracking, trend analysis, weight learning (extracted from `commands_risk.rs`)
