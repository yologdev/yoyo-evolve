Title: Add risk-awareness to /status and auto-context annotations
Files: src/commands_info.rs, src/commands_project.rs, src/commands_risk.rs
Issue: none

## Dream Milestone: Surface risk predictions where decisions happen

The `/risk` infrastructure (2,189 lines, 7 weighted signals, snapshot/validate/history) exists but
is invisible during normal workflow. This task wires risk data into two places where it matters:

### 1. `/status` — show top-3 riskiest files

In `handle_status()` in `commands_info.rs`, after the "self-written" line, add a section that
calls `compute_file_risk_scores()` from `commands_risk.rs` and displays the top 3 files with
their scores. Format:

```
  risk:    src/commands_git.rs (0.87) · src/repl.rs (0.74) · src/watch.rs (0.68)
```

This makes risk awareness part of the status check developers already run.

Add a new public helper in `commands_risk.rs`:
```rust
pub(crate) fn top_risk_files(n: usize) -> Vec<(String, f64)> {
    let risks = compute_file_risk_scores();
    risks.into_iter()
        .take(n)
        .map(|r| (r.path, r.score))
        .collect()
}
```

### 2. Auto-context file annotations — show risk score

In `format_auto_context()` in `commands_project.rs`, when formatting the auto-context block
that gets injected into prompts, annotate each file with its risk score if it's in the top 25%
of risky files. Use `compute_file_risk_scores()` to get the scores, build a HashMap of
path → score, and append `(⚠ risk: 0.82)` to the file header line for high-risk files.

This means the model sees risk warnings when it's about to edit dangerous files.

### Tests

- Test `top_risk_files` returns the correct number of entries and they're sorted by score descending
- Test that `format_auto_context` output includes the risk annotation for a known high-risk file
  (mock or use the actual codebase files — the test just checks the annotation format appears)
- Test that low-risk files don't get annotated

### CLAUDE.md update

Add `commands_risk.rs` `top_risk_files` to the architecture description for that file.
