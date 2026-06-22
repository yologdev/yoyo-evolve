Title: Extract risk scorer into commands_risk.rs
Files: src/commands_risk.rs (new), src/commands_info.rs, src/commands.rs
Issue: none

## Context
`commands_info.rs` is the largest file in the codebase at 5,108 lines — the assessment's #1 self-discovered issue and, ironically, the risk scorer's own top prediction for "most likely to cause the next regression." The risk scorer subsystem (~2,135 lines including tests) is a self-contained feature that was built over Days 111-113 as the dream milestone infrastructure. It has minimal external coupling (only 2 references from other files).

## What to do

1. Create `src/commands_risk.rs` containing ALL risk-related code currently in `commands_info.rs`:
   - `struct FileRisk` and all its fields (line ~1555)
   - `normalize_scores` (line ~1566)
   - `build_test_reference_map` (line ~1585)
   - `module_to_source_path` (line ~1685)
   - `resolve_crate_reference` (line ~1719)
   - `co_change_coupling` (line ~1742)
   - `compute_file_risk_scores` (line ~1795)
   - `revert_involved_files` (line ~2029)
   - `format_risk_report` (line ~2056)
   - `handle_risk` (line ~2101)
   - `build_risk_snapshot_json` (line ~2132)
   - `write_risk_snapshot_to` (line ~2170)
   - `handle_risk_snapshot` (line ~2186)
   - `parse_git_log_name_only` and `CommitEntry` if it's a local type (line ~2233)
   - `classify_broke_files` (line ~2273)
   - `compute_validation`, `ValidationResult`, `ParsedSnapshot`, `HistoryValidation` (lines ~2306-2507)
   - `format_validation_report` (line ~2353)
   - `parse_all_snapshots` (line ~2427)
   - `precision` (line ~2468)
   - `compute_trend` (line ~2477)
   - `format_history_report` (line ~2507)
   - `handle_risk_history` (line ~2605)
   - `handle_risk_validate` (line ~2703)
   - ALL corresponding `#[test]` functions (lines ~4216-5108) — every test whose name contains `risk`, `snapshot`, `validate`, `coupling`, `normalize`, `module_to_source`, `resolve_crate`, `parse_git_log`, `classify_broke`, `compute_validation`, `format_validation`, `parse_all_snapshots`, `precision`, `compute_trend`, `format_history`, `test_density`, `reference_map`

2. Update `src/commands_info.rs`:
   - Remove all the functions and tests listed above
   - This should reduce the file from ~5,108 lines to ~2,970 lines

3. Update `src/commands.rs`:
   - Change `pub(crate) use crate::commands_info::handle_risk;` to `pub(crate) use crate::commands_risk::handle_risk;`

4. Add `mod commands_risk;` to `main.rs` (check if it's needed — look at how other `commands_*.rs` are declared)

5. The new file needs the same `use` imports that the risk code currently relies on from `commands_info.rs`. Check what's needed: likely `crate::git::*`, `std::collections::HashMap/HashSet`, `std::io`, `std::path::Path`, etc.

## Verification
- `cargo build` — must compile
- `cargo test` — all existing tests must pass (the risk tests now live in commands_risk.rs)
- `cargo clippy --all-targets -- -D warnings` — clean

## CLAUDE.md update
Add `commands_risk.rs` to the Architecture section's file list with description: `commands_risk.rs — /risk command: file risk scoring, snapshot, validate, history, co-change coupling, test coverage mapping`
