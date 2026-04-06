Title: Smart test output filtering — extract failures and summary from test framework output
Files: src/format/mod.rs
Issue: #229

## What

Add a `filter_test_output` function to `format/mod.rs` that detects common test framework output patterns and intelligently compresses them to show only failures + summary, dramatically reducing token consumption for test-heavy workflows.

## Why

Issue #229 (Rust Token Killer) suggests smarter output compression. The assessment identifies "test output → failures only" as the biggest applicable RTK idea (-90% tokens). Currently `compress_tool_output` just strips ANSI codes and collapses repetitive lines — it doesn't understand test output structure at all. When `cargo test` runs 1,000 tests, all passing lines consume context window for zero value.

This is a competitive gap: Claude Code processes tool output more intelligently. Real developers run tests constantly — every saved token here compounds across sessions.

## Implementation

Add a `filter_test_output(output: &str) -> String` function that:

1. **Detects test framework output** via line patterns:
   - `cargo test`: lines starting with `test ` and ending with `... ok` (pass) or `... FAILED` (fail), summary line `test result:`, `failures:` section
   - `pytest`: lines with `PASSED`, `FAILED`, `ERROR`, summary line with counts
   - `jest`/`vitest`: `✓` or `✗`/`✕` markers, `Tests:` summary line
   - `go test`: `--- PASS:`, `--- FAIL:`, `ok` summary, `FAIL` summary
   - `rspec`: lines with examples/failures count

2. **Filtering logic** (only when test output is detected):
   - Keep ALL failure lines and their context (the line before and after each failure)
   - Keep the summary/result lines
   - Keep error output sections (stack traces, assertion messages)
   - Replace passing test lines with a single `... (N passing tests omitted)` marker
   - If there are ZERO failures, keep just the summary line + count marker

3. **Integration**: Call `filter_test_output` from inside `compress_tool_output`, BEFORE `collapse_repetitive_lines` — the test filter is more specific and should run first.

4. **Detection heuristic**: Count lines matching test-pass patterns. If ≥5 test-pass lines detected, apply the filter. Otherwise fall through to generic compression.

## Tests

Write tests first:
- `test_filter_cargo_test_all_passing` — 20 passing tests → summary + "(20 passing tests omitted)"
- `test_filter_cargo_test_with_failures` — mix of pass/fail → failures kept with context, passes omitted
- `test_filter_cargo_test_failure_details_preserved` — failures section (assertion messages, panic info) fully preserved
- `test_filter_pytest_output` — pytest format detection and filtering
- `test_filter_jest_output` — jest format detection
- `test_filter_go_test_output` — go test format detection
- `test_filter_non_test_output_unchanged` — regular command output passes through unchanged
- `test_filter_mixed_content` — test output preceded by compilation output: compilation kept, test passes filtered
- `test_compress_tool_output_integrates_test_filter` — verify `compress_tool_output` calls the test filter

## Acceptance

- `cargo build && cargo test` passes
- `cargo clippy --all-targets -- -D warnings` clean
- Test output from `cargo test` with 100+ lines compresses to <20 lines (failures + summary)
