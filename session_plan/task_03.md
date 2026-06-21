Title: Extract risk scoring into src/commands_risk.rs (reduce commands_info.rs size)
Files: src/commands_info.rs, src/commands_risk.rs, src/commands.rs
Issue: none (maintenance + dream infrastructure)

## Context

`commands_info.rs` is 4,273 lines — the largest file in the codebase and a maintenance hotspot. The `/risk` command subsystem (structs, scoring, formatting, snapshot, validate) is ~600 lines and is the most actively evolving part of the file (built Days 111-112, extended in task_02 this session). Extracting it into its own module:

1. Makes the risk code easier to iterate on for dream milestone work
2. Reduces `commands_info.rs` to ~3,670 lines
3. Follows the established pattern (commands were already split into 28 `commands_*.rs` files)

## What to do

### 1. Create `src/commands_risk.rs`

Move these items from `commands_info.rs`:
- `struct FileRisk`
- `fn normalize_scores`
- `fn build_test_reference_map`
- `fn revert_involved_files`
- `fn compute_file_risk_scores`
- `fn format_risk_report`
- `fn handle_risk`
- `fn handle_risk_snapshot`
- `struct ValidationResult`
- `fn validate_predictions`
- `fn handle_risk_validate`
- All related `#[cfg(test)]` tests

Add `mod commands_risk;` to `main.rs` and update the dispatch path in `dispatch.rs` (search for where `/risk` is routed — it likely calls `commands_info::handle_risk`).

### 2. Update `commands_info.rs`

Remove the moved functions. Add `pub(crate) use commands_risk::*;` if any other code references `commands_info::FileRisk` or `commands_info::compute_file_risk_scores`.

### 3. Update `commands.rs`

If there are completions or command metadata referencing risk in commands_info, update to point to commands_risk.

## Verification

- `cargo build && cargo test`
- `cargo clippy --all-targets -- -D warnings`
- All existing risk-related tests must still pass
- The `/risk`, `/risk validate`, `/risk snapshot` commands must still work (same dispatch path)
