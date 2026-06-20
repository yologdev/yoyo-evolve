Title: Fix compute_file_risk_scores truncation bug — return all files, not just 15
Files: src/commands_info.rs
Issue: none (bug fix for dream infrastructure)

## Problem

`compute_file_risk_scores()` always calls `risks.truncate(15)` before returning. This means
`/risk --all` still only shows 15 files — the `--all` flag in `format_risk_report` is useless
because the data is already truncated upstream.

## Fix

Remove the `risks.truncate(15)` line from `compute_file_risk_scores()`. The function should
return ALL scored files, sorted by score descending. The display limit belongs in
`format_risk_report()` / `handle_risk()`, which already handles the `--all` flag correctly.

Verify that `format_risk_report` already limits to 15 by default (it does — `let limit = if show_all { risks.len() } else { 15 };`).

## Tests

Add a test that verifies `compute_file_risk_scores()` returns more than 15 files when the
project has more than 15 source files (which this project does — 71 source files). This
confirms the truncation was removed.

Also verify the existing tests still pass — the format_risk_report tests should work the same
since they already handle the limit internally.

## Sizing

~5 lines changed. One new test. Surgical fix.
