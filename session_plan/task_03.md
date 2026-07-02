Title: Fix vacuous context tests that silently pass without asserting in CI
Files: src/context.rs
Issue: none (self-discovered — assessment bug #2, also in project memories)

## Problem

Two tests in `src/context.rs` have guard clauses that make them silently pass without asserting in shallow clones (the typical CI environment):

1. `test_load_project_context_includes_git_status` (line ~462): wraps assertion inside `if let Some(context) = &result { if get_git_status_context().is_some() { ... } }` — if either returns None, zero assertions run.

2. `test_load_project_context_includes_recently_changed` (line ~372): similar pattern with `if let Some(context) = &result { if context.contains("## Git Status") && has_modified_files { ... } }` — multiple conditions gate the assertion.

These tests are "green but testing nothing" — the project memories even flag this: `[bug] Test expectation in cargo clippy... thread 'context::tests::test_load_project_context_includes_git_status'`.

## Fix

Restructure both tests to ALWAYS assert something, even in shallow clones:

### test_load_project_context_includes_git_status:
We're in a git repo (the test suite runs inside the yoyo repo), so `load_project_context()` should always return `Some` and `get_git_status_context()` should always return `Some`. Make the assertions unconditional:

```rust
fn test_load_project_context_includes_git_status() {
    let result = load_project_context();
    let context = result.expect("load_project_context should return Some in a git repo");
    // Git status should always be available when running tests inside a git repo
    assert!(
        context.contains("## Git Status"),
        "Context should contain Git Status section"
    );
}
```

### test_load_project_context_includes_recently_changed:
The "recently changed files" section depends on git history depth. In shallow clones, there may be no recent changes. Make this test assert what we CAN guarantee — that the context loads and contains expected sections — while making the recently-changed assertion conditional but with an explanatory message:

```rust
fn test_load_project_context_includes_recently_changed() {
    let result = load_project_context();
    let context = result.expect("load_project_context should return Some in a git repo");
    // The context should always contain at least a Git Status section
    assert!(context.contains("## Git Status"), "Should always have Git Status");
    
    let has_modified_files = get_recently_changed_files(1).is_some();
    if has_modified_files {
        assert!(
            context.contains("## Recently Changed Files"),
            "Context should contain Recently Changed Files when modified files exist"
        );
    }
    // Always assert something ran — even if recently-changed is absent
    assert!(!context.is_empty(), "Context should never be empty in a git repo");
}
```

The key change: remove the outer `if let Some` guards. We KNOW we're in a git repo during testing. If `load_project_context()` returns `None`, that's a real bug we want to catch, not silently skip.

## Verification
`cargo test test_load_project_context -- --nocapture` — verify assertions actually fire.
`cargo test` — full suite passes.
`cargo clippy --all-targets -- -D warnings` — clean.
