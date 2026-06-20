Title: Extract risk subsystem from commands_info.rs into commands_risk.rs
Files: src/commands_risk.rs (new), src/commands_info.rs, src/dispatch.rs
Issue: none

## Dream advancement: the self-prediction machinery deserves its own file

`commands_info.rs` is 3,966 lines — the largest file in the codebase. The risk subsystem (score → snapshot → validate) accounts for ~650 lines of production code + ~330 lines of tests. Extracting it into `commands_risk.rs` reduces the complexity hotspot and gives the dream's core machinery room to grow independently.

### What to extract

Everything from line ~1555 to ~2207 in `commands_info.rs`:

**Structs:**
- `FileRisk` (pub(crate))
- `CommitEntry`
- `ValidationResult`

**Functions:**
- `normalize_scores`
- `compute_file_risk_scores` (pub(crate))
- `revert_involved_files`
- `format_risk_report` (pub(crate))
- `handle_risk` (pub(crate))
- `handle_risk_snapshot`
- `build_risk_snapshot_json`
- `write_risk_snapshot_to`
- `parse_git_log_name_only`
- `classify_broke_files`
- `compute_validation`
- `format_validation_report`
- `handle_risk_validate`
- `RISK_SNAPSHOT_PATH` const

**Tests (move to `commands_risk.rs` `#[cfg(test)]` module):**
- `test_normalize_scores_basic`
- `test_normalize_scores_all_equal`
- `test_normalize_scores_empty`
- `test_normalize_scores_single`
- `test_format_risk_report_empty`
- `test_format_risk_report_shows_signals`
- `test_handle_risk_does_not_panic`
- `test_risk_snapshot_serialization`
- `test_risk_snapshot_writes_jsonl`
- `test_risk_snapshot_top_10_limit`
- `test_risk_subcommand_routing`
- `test_compute_file_risk_scores_returns_all_files`
- `test_parse_git_log_name_only_basic`
- `test_parse_git_log_name_only_no_trailing_blank`
- `test_classify_broke_files_revert`
- `test_classify_broke_files_fix`
- `test_classify_broke_files_empty`
- `test_compute_validation_perfect_prediction`
- `test_compute_validation_partial_prediction`
- `test_compute_validation_no_breakage`
- `test_format_validation_report_has_key_sections`
- `test_format_validation_report_no_surprises`
- `test_risk_validate_routing`

### Steps

1. Create `src/commands_risk.rs` with the extracted code
2. Add `mod commands_risk;` to `main.rs`
3. Add necessary imports at the top (crate::format::*, crate::git::*, std::collections::*, etc.)
4. In `commands_info.rs`, remove the extracted code and add `pub use commands_risk::*;` or update `dispatch.rs` to reference `commands_risk` directly
5. In `dispatch.rs`, update the `CommandRoute::Risk` handler to call `commands_risk::handle_risk` instead of `commands::handle_risk`
6. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings`

### Important details
- `compute_file_risk_scores` and `format_risk_report` are `pub(crate)` — they may be referenced from other modules. Check for callers before changing paths.
- The `handle_risk` function is currently routed through `commands` module in `dispatch.rs` — update the route.
- Keep the same function signatures and visibility.

### Doc updates
- Update CLAUDE.md: add `commands_risk.rs` to the architecture list with description "Risk prediction subsystem: file risk scoring, snapshot, validation pipeline"
- Update the `src/commands_info.rs` description in CLAUDE.md to note risk functions moved out
