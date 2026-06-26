Title: Fix flaky test_load_project_context_includes_recently_changed
Files: src/context.rs
Issue: none

## Problem

The test `test_load_project_context_includes_recently_changed` fails ~3x per trajectory window
during evolve pipeline interim states. It shows up in the recurring CI errors:

```
[3×] thread 'context::tests::test_load_project_context_includes_recently_changed'
```

Root cause: `get_recently_changed_files()` uses `git log --diff-filter=M` which only returns
**modified** files, not **added** ones. In CI shallow clones where the evolve pipeline has
just committed brand-new files, all recent changes are "added" (A) rather than "modified" (M),
so the function returns `None` and the test's guard clause sometimes doesn't protect correctly.

The existing guard clause (`let has_modified_files = get_recently_changed_files(1).is_some()`)
is reactive — it checks the same broken function. The real fix is to make the function work
correctly in all environments.

## Fix

In `src/context.rs`, change `get_recently_changed_files()`:

1. Change `--diff-filter=M` to `--diff-filter=AM` so it includes both added and modified files.
   This is the semantically correct behavior — "recently changed files" should include newly
   added files, not just modifications to existing files.

2. Verify the test still passes with the broader filter. The test's existing guard clause
   can remain as defense-in-depth but should now rarely trigger.

3. Add a unit test that verifies `get_recently_changed_files` parses output correctly
   (mock the git output parsing if possible, or test the parsing logic directly).

## Verification

```bash
cargo test test_load_project_context_includes_recently_changed
cargo test context::tests
cargo clippy --all-targets -- -D warnings
```

## Scope

Only `src/context.rs`. One-line change to the diff filter + optional test hardening.
