Title: Fix trajectory error fingerprint false positives on test names
Files: scripts/extract_trajectory.py
Issue: none

## Context
The trajectory section (computed by `scripts/extract_trajectory.py`) currently shows false positive "recurring CI errors" like:
```
[4×] test watch::tests::test_watch_result_failed_with_error ... ok
```

This happens because `ERROR_LINE_RE = re.compile(r"(error|panicked|FAILED|fatal)", re.IGNORECASE)` matches the word "error" inside test names that contain "error" as part of their identifier (e.g., `test_watch_result_failed_with_error`). These lines end with `... ok` — they are PASSING tests, not errors. This noise obscures real CI error signals in the trajectory.

The assessment notes: "the regex picks it up because it shares a line with the word 'error'" and calls it a false positive.

## What to do

1. In `collect_failed_ci_fingerprints()` (around line 313), add a filter to skip lines that match Rust test output patterns showing a passing test. A passing Rust test line looks like:
   ```
   test some::path::test_name ... ok
   ```
   The fix should skip lines where the pattern matches `r"test\s+\S+\s+\.\.\.\s+ok"` (a test result line that ended with "ok").

2. More broadly, also skip lines matching `r"test result:.*passed"` which is a summary line that could match if it also mentions "failed" count (e.g., `test result: ok. 3823 passed; 0 failed;`). Actually, `test result: failed.` IS a real error signal, so be careful — only skip `test result: ok.` lines.

3. Add self-tests in `run_self_tests()` (around line 480) to verify:
   - A line like `test watch::tests::test_watch_result_failed_with_error ... ok` is NOT fingerprinted
   - A line like `test watch::tests::test_watch_result_failed_with_error ... FAILED` IS still fingerprinted
   - A line like `error[E0308]: mismatched types` IS still fingerprinted
   - A line like `test result: ok. 3823 passed; 0 failed;` is NOT fingerprinted
   - A line like `test result: failed. 3823 passed; 1 failed;` IS still fingerprinted

## Verification
- `python3 scripts/extract_trajectory.py --self-test` — must pass all self-tests
- Manually verify: the fix should eliminate the `[4×] test watch::tests::test_watch_result_failed_with_error ... ok` false positive from trajectory output
