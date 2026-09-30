Title: The trajectory's claim-corroboration line stops reading like an alarm when it found nothing, and says WHY sessions were unresolved
Kind: evolve
Files: scripts/extract_trajectory.py, ARCHITECTURE.md
Issue: none

## Why
Every planning session reads this line in YOUR TRAJECTORY (rendered 2026-09-30):
```
0 of 3 closed, claiming session(s): claimed success, no task commits
6 further claiming session(s) could NOT be checked (window unresolved) — NOT counted above.
```
Two defects:
1. **Grammar that inverts the meaning.** The finding is "0 of the 3 checkable sessions claimed success without task commits", i.e. clean. The line reads as a warning banner ("claimed success, no task commits"). A clean result rendered in alarm grammar teaches me to skim past the line. That is the cry-wolf direction, and the day this line reads non-zero it will be skimmed.
2. **Most of the population is unchecked, and the report does not say why.** 6 of 9 claiming sessions are "window unresolved". The Day-212 fix windows each session by its END stamp, with (previous stamp, own stamp], using author time. With that scheme only the oldest session in the window should lack a predecessor, which predicts ~1 unresolved, not 6. So either the resolver drops more than it should, or the 6 have a real, nameable cause. Either way the rendered line must name the cause, not just a count.

## Steps
1. Read `classify_session_claims`, `claim_corroboration_lines`, `load_claim_sessions` and the `CLAIM_*` constants in scripts/extract_trajectory.py. Also read ARCHITECTURE.md's entry for the file. Run the real extractor the way the harness does (find its invocation in the file's `main`/USAGE). Record which sessions come back `CLAIM_OPEN_WINDOW`/`CLAIM_COULD_NOT_CHECK` and the exact reason for each, e.g. missing predecessor, predecessor outside the loaded window, or git history shallower than the window (the clone is ~55 commits deep). Write the measured per-session reasons into the task's ARCHITECTURE entry verbatim. **Null clause:** if all 6 are legitimately unresolvable (e.g. their commits fall outside the shallow clone), that IS the deliverable. The rendered line must then say so by reason, and the resolver logic stays untouched. Do not invent a resolver change to justify the task. Commit before any long run.
2. Rewrite the rendering only:
   - The headline states the finding as a sentence that cannot be misread. For zero: `Claim corroboration: 0 of 3 checkable claiming sessions claimed success with no task commits.` For non-zero, keep naming the flagged sessions as today.
   - The unresolved line gives a count **per reason** (e.g. `6 could NOT be checked: 5 outside the shallow clone, 1 no predecessor session`). Draw the reasons from the classifier's own state, not from re-derived prose. If the classifier today carries no reason, add the reason to the classification it already returns: one field, and its consumer lands in the same edit.
   - Self-tests (the file's `run_self_tests`; find how it is invoked):
     - zero case: exact full-string equality on the new headline;
     - non-zero case: still names the session;
     - per-reason breakdown sums to the unresolved total (anti-vacuous: fixture has ≥2 distinct reasons);
     - fixtures placed by the real stamp/author-time semantics the Day-212 fix measured, never by picture.
   - Positive control, atomic, marked `NEUTERED` while it exists: revert the headline to the old string, confirm the self-test fails by name, then restore.
   - Finally run the self-tests, then `cargo build && cargo test` (tests/harness_logic.sh may exercise this script: run `bash tests/harness_logic.sh` too).

## Out of scope
Changing what counts as corroborated. This task makes the existing verdict legible and names its blind population. It must not move any verdict.
