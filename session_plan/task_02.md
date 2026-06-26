Title: Boost auto-context scoring for recently-edited files
Files: src/commands_project.rs
Issue: none (competitive UX gap)

## Context

The assessment identifies "seamless project understanding without explicit /add" as
a key remaining UX gap vs Claude Code and Cursor. The auto-context system (Day 116)
improved keyword tokenization and added function signatures. But it still misses a
crucial signal: **what the user has been working on recently**.

When a developer asks "why is this test failing?", the files they edited in the last
few commits are far more likely to be relevant than files they haven't touched in weeks.
Claude Code and Cursor both track recent activity. Our auto-context scores files purely
by keyword match against the prompt, ignoring recency entirely.

## What to Do

1. **In `auto_context_for_prompt`** (src/commands_project.rs): After computing keyword
   scores via `score_files`, apply a recency boost to files that appear in recent git
   activity. Use `git diff --name-only HEAD~5` (or fewer if fewer commits exist) to get
   the list of recently-changed files. Files in this list get their score multiplied by
   a boost factor (e.g., 1.5×). This makes recently-edited files more likely to clear
   the `AUTO_CONTEXT_MIN_SCORE` threshold and appear in auto-context.

2. **Implementation details**:
   - Run `git diff --name-only HEAD~5 2>/dev/null` via `std::process::Command` (quick,
     no network). Fall back gracefully if git isn't available or fewer commits exist.
   - Build a `HashSet<String>` of recently-changed paths.
   - After `score_files()` returns results, iterate and multiply scores for matching paths.
   - Re-sort by score after boosting (the order may change).
   - Keep the boost factor as a constant: `const RECENCY_BOOST: f64 = 1.5;`

3. **Add tests**:
   - Test that `auto_context_for_prompt` returns recently-changed files even if their
     keyword score alone would be below threshold (mock this by using a prompt that
     weakly matches a recently-changed file). This may need to be an integration-style
     test using a temp git repo if the keyword matching doesn't lend itself to unit testing.
   - At minimum: test the recency file detection helper in isolation (given git output,
     parse the file list correctly).

## Why This Matters

This closes a real UX gap: when a user asks about something they're actively working on,
the tool should know what "actively working on" means. Recency is one of the strongest
signals for relevance, and we're not using it at all. The auto-context system already
reads files and scores them — this just adds one more signal to the scoring, making the
system smarter about *which* files to pull in.

## Verification

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings`
