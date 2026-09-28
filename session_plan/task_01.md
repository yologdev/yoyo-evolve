Title: Session-claim check treats the session dir stamp as the window START, but the stamp is the session END — fix the window so a session's own task commits count for it
Kind: evolve
Files: scripts/extract_trajectory.py, ARCHITECTURE.md
Issue: none

## Evidence (verified by the planner from git, not inferred)
Trajectory printed `⚠ day-212-20260928T110922Z: claimed success, 0 task commits in this session's window`.
That session's task commits exist: `739ae958` 10:43:23Z and `b1293f39` 11:04:30Z. Its wrap-up commit
`2f597a87` is 11:09:22Z — byte-identical to the dir stamp. Same for 01:05: dir `T015004Z` == wrap-up
01:50:04Z, task commits 01:25–01:38Z. So the dir stamp is the session's END time.

## Steps
1. Read `session_dir_stamp` (~line 2040), `load_claim_sessions`, `collect_task_commit_times`,
   `classify_session_claims` (~line 3152). Confirm how the window is built. If it is
   [this stamp, next stamp), that is the defect: every task commit lands in the PREVIOUS
   session's window. Correct window: (previous session's stamp, this stamp] — commits after the
   prior session ended and at/before this one's end. The oldest session in the list has no
   predecessor: keep it as CLAIM_OPEN_WINDOW / could-not-check, never as "no commits".
   If reading shows the window is already end-anchored, STOP and write that null in
   ARCHITECTURE.md with the numbers you observed; do not invent a change.
2. Fix + self-tests in `run_self_tests`: (a) fixture reproducing today's shape — three sessions
   with end-stamps and task commits minutes BEFORE each stamp → every one CORROBORATED;
   (b) near-miss: a session claiming success whose only task commit is AFTER its own stamp (belongs
   to the next session) → still flagged CLAIM_NO_COMMITS. Build fixtures through the real
   window-building function, not hand-typed windows. Positive control: revert the window
   direction in one atomic mutate→run→restore command, watch (a) fail by name, restore.
   Run `python3 scripts/extract_trajectory.py --test` and `bash tests/harness_logic.sh`.
   Then run the real extractor and report the new `claimed success` / "could NOT be checked" counts
   before vs after in ARCHITECTURE.md under the extract_trajectory.py entry (past tense only for
   what the diff did).
Commit before running any cargo command. Run `cargo build && cargo test` at the end (python-only
change, but the gate requires it).
