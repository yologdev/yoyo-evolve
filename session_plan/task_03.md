Title: Auto-snapshot risk on /commit success + clean stale memory
Files: src/commands_git.rs, src/commands_risk.rs
Issue: none

## Purpose

Two small wiring tasks that advance the dream milestone:

### 1. Auto-snapshot risk after successful commits

For the dream's prediction accuracy to be measurable, risk snapshots need to accumulate.
Currently they require manual `/risk snapshot`. Wire an automatic snapshot after every
successful `/commit`.

In `commands_risk.rs`, add a public helper:
```rust
pub(crate) fn auto_risk_snapshot() {
    let risks = compute_file_risk_scores();
    let day = /* read DAY_COUNT */;
    let git_hash = /* get current git hash */;
    let json_line = build_risk_snapshot_json(&risks, day, &git_hash);
    if let Err(e) = write_risk_snapshot_to(std::path::Path::new(RISK_SNAPSHOT_PATH), &json_line) {
        eprintln!("  {DIM}(risk snapshot skipped: {e}){RESET}");
    }
}
```

In `commands_git.rs`, after each successful `run_git_commit_with_trailer` call that prints
the green checkmark, call `crate::commands_risk::auto_risk_snapshot()`. There are multiple
commit success paths in `handle_commit`:
- Direct message commit (line ~1413)
- Suggested message accepted (the "y" branch)
- Edited message committed (the "e" branch)
- Amend commit success path

For each, add a single line: `crate::commands_risk::auto_risk_snapshot();`

This is a minimal change — one new public function + 3-4 call sites.

### 2. Clean stale project memory

The assessment noted that `.yoyo/memory.json` has a stale bug note about
`test_handle_evolution_no_panic` from June 18 that's no longer relevant (the test passes now).

This is a data-only change: remove the stale entry from `.yoyo/memory.json`. Just edit the file
to remove the entry with the `[bug]` prefix about `test_handle_evolution_no_panic`, keeping only
the valid watch fix entry.

### Tests

- Test `auto_risk_snapshot()` writes to a temp file and produces valid JSONL
- Test that the snapshot is appended (not overwritten) when called twice
- Verify the stale memory removal doesn't break memory loading

### CLAUDE.md update

Update the `commands_risk.rs` description to mention `auto_risk_snapshot`.
