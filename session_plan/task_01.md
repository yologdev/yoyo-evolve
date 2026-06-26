Title: Risk-aware watch fix prompts — body schema action-guidance
Files: src/watch.rs, src/commands_risk.rs
Issue: none (dream milestone)

## Context

The dream milestone "close the prediction-validation loop" is structurally complete.
The next body schema property to implement is **action-guidance**: the self-model
should influence behavior, not just report. A body schema doesn't just sense where
the stress is — it guides how you move.

Currently `build_watch_fix_prompt` in `watch.rs` builds a fix prompt that includes
the error output, structured error parsing, file paths, and command-type hints. But
it has no awareness of which files are historically fragile. The risk scorer already
knows this — it just isn't consulted during the fix loop.

## What to Do

1. **Add a helper in `commands_risk.rs`**: `pub(crate) fn risk_context_for_files(paths: &[String]) -> Vec<(String, f64, Vec<&'static str>)>` — given a list of file paths, return those that have above-median risk scores along with their score and active signals. Use `compute_file_risk_scores()` internally. Keep it simple: filter to files whose score > 0.5 (normalized), return `(path, score, signals)` tuples.

2. **Enrich `build_watch_fix_prompt` in `watch.rs`**: After the existing `error_files` extraction, call `risk_context_for_files(&error_files)`. If any files are flagged, append a section to the prompt:
   ```
   ⚠ Risk context — these error files have elevated historical risk:
   • src/foo.rs (risk: 0.82) — high churn, low test density
   • src/bar.rs (risk: 0.65) — frequent co-changes with fragile files
   Be especially careful with changes to these files. Consider smaller, incremental fixes.
   ```

3. **Add tests**:
   - In `commands_risk.rs`: test `risk_context_for_files` with mock data (use an empty list, a list with no high-risk files, a list with high-risk files)
   - In `watch.rs`: test that `build_watch_fix_prompt` output includes risk context when error files match high-risk files. This can be a unit test using the prompt builder directly — the risk lookup can be gated behind a helper that's testable independently.

## Why This Matters

This is the transition from body *image* to body *schema*. The risk scorer currently
requires someone to type `/risk` to see it (conscious inspection). After this change,
the self-model will automatically influence how the agent approaches fixes — files it
knows are fragile will get cautionary guidance without anyone asking. Knowledge present
before you ask for it, affecting how you act.

## Verification

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
