Title: Clean up dead_code annotations and remove genuinely unused code
Files: src/prompt.rs, src/commands_session.rs, src/format.rs
Issue: none

## Context
There are 12+ `#[allow(dead_code)]` annotations across the codebase. Some of these mark genuinely useful API methods on internal types (like `len()`, `is_empty()`, `get()`), while others might mark truly dead code. This task audits each one and either:
- Removes the annotation if the code is used in tests or will obviously be needed
- Removes the dead code entirely if it's genuinely unused and unlikely to be needed
- Adds a test that exercises the method (removing the need for the annotation)

## Files to audit

### src/prompt.rs (5 annotations)
- `SessionChanges::len()` — likely useful for tests. Write a test that calls it.
- `SessionChanges::is_empty()` — likely useful for tests. Write a test that calls it.
- `TurnSnapshot::len()` — check if any tests use it. If not, add one.
- `TurnHistory::pop()` — check if used. If not, either add a test or remove if truly unnecessary.
- `PromptResult::auto_compacted` — check if read anywhere. If not, either use it or remove it.

### src/commands_session.rs (4 annotations)
- `SpawnTracker::get()` — check if used anywhere
- `SpawnTracker::task_count()` — check if used anywhere
- `SpawnTracker::is_empty()` — check if used anywhere
- `parse_spawn_task()` — marked as legacy compat. If nothing calls it, remove it.

### src/format.rs (3 annotations)
- `ActiveToolState` struct — check if used in the event loop
- `ActiveToolState::new()` — check usage
- `ActiveToolState::update()` — check usage

## Approach
1. For each annotation, search the codebase (excluding the definition itself) for usage
2. If used: remove the `#[allow(dead_code)]` annotation (it's not actually dead)
3. If unused but useful (standard API methods like `len`, `is_empty`): write a test that exercises it, then remove the annotation
4. If unused and not useful: remove the code entirely
5. Run `cargo test` and `cargo clippy --all-targets -- -D warnings` after changes

The goal: zero `#[allow(dead_code)]` annotations remaining, with honest code that's either used or tested.
