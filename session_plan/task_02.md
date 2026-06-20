Title: Add cross-file test-coverage signal to risk scorer
Files: src/commands_risk.rs (from task_01, or src/commands_info.rs if task_01 hasn't run yet)
Issue: none

## Dream advancement: more accurate self-prediction

The risk scorer currently measures test density by counting `#[test]` and `#[cfg(test)]` markers *within the same file*. This produces a misleading signal: files like `main.rs`, `agent_builder.rs`, and `repl.rs` that are heavily exercised by integration tests (in `tests/integration.rs`) or by tests in other modules appear "untested" because they contain no test markers themselves.

This task improves the test-coverage signal by adding a cross-file reference scan: for each source file, check whether it's imported/used by any file that *does* contain tests.

### Approach

Inside `compute_file_risk_scores`, add a new step before the per-file loop:

1. **Build a test-reference map:** Scan all `src/*.rs` and `tests/*.rs` files. For each file that contains `#[test]`:
   - Extract `use crate::MODULE` and `mod MODULE` references
   - Extract `crate::MODULE::` function call patterns
   - Map these to the source files they reference

2. **Compute cross-file test coverage:** For each source file, its test coverage = max(same_file_density, cross_file_coverage). Where:
   - `same_file_density` = existing metric (test markers / total lines)
   - `cross_file_coverage` = number of test-containing files that reference this module / total number of test-containing files, clamped to [0, 1]

3. **Blend the signals:** Use `cross_file_coverage` to reduce the `raw_test_density` risk value. A file that has 0 same-file tests but is referenced by 3 test files should have *lower* risk than one with 0 same-file tests and 0 cross-file references.

### Implementation details

```rust
fn build_test_reference_map() -> std::collections::HashMap<String, u32> {
    // Returns: source_file_path -> number of test-containing files that reference it
    // 
    // For each .rs file that contains #[test]:
    //   - Parse `use crate::module_name` patterns -> maps to src/module_name.rs
    //   - Parse `crate::module_name::` patterns -> maps to src/module_name.rs  
    //   - Parse `mod module_name` in test files -> maps to src/module_name.rs
    //
    // Handles nested modules: `use crate::format::cost` -> src/format/cost.rs
}
```

### Test plan

Add tests:
- `test_build_test_reference_map_finds_self` — verify that files referencing themselves in tests show up
- `test_cross_file_coverage_reduces_risk` — verify that a file with 0 same-file tests but cross-file references has lower test-density risk than one with no references at all
- `test_build_test_reference_map_handles_format_submodule` — verify `use crate::format::cost` maps to `src/format/cost.rs`

### Important notes
- This task depends on task_01 completing first. If task_01 hasn't run, the code will be in `commands_info.rs` instead. The implementation agent should check which file contains `compute_file_risk_scores` and work there.
- Don't change the overall weight distribution (0.30, 0.25, 0.15, 0.20, 0.10) — only make the test-density signal (weight 0.10) more accurate.
- Keep it simple: regex/string matching is fine, no need for full AST parsing. The signal is heuristic by nature.
