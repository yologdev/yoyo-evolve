# Issue Responses — Day 132 (17:47)

## #582 — Track promises made in Discussions + check your own past actions before filing/replying
**Action: implement (both parts, split across two tasks).**
- **Part 1 → Task 1:** Make `scan_commitments.py` source-aware so discussion
  items triage through the same call and render as `### Discussion #N — title`.
  This is the self-editable half. The actual feeding of discussion data into
  the scanner's stdin lives in `scripts/evolve.sh` (protected do-not-modify) —
  filed as a separate help-wanted follow-up with the exact `gh api graphql`
  approach (Day-58 pattern: ship the editable half + contract test, file the
  harness half). Follow-up filed: **#589** (agent-help-wanted) with the exact
  `gh api graphql` + `jq` feed patch.
- **Part 2 → Task 2:** Add a concrete "check your own footprint before
  filing/replying" step to the social/communicate skill guidance — the #401
  double-reply incident this addresses is exactly the failure mode. Trigger-
  shaped, names the actual `gh` commands.

Reply in voice: acknowledge the diagnosis is precise (both halves of a real
blind spot — my promise-tracker literally can't see half the places I make
promises), and that the @danstis release-tag promise rotting for weeks is the
receipt. Note the split: Python + skill this session, evolve.sh wiring behind a
help-wanted issue because that file is protected.

## #587 — wire risk validate into evolve.sh (agent-help-wanted, already filed)
**Action: no new work; blocked on human.** Patch + contract test already
shipped Day 132. DREAM milestone is accumulation-blocked, not implementation-
blocked (assessment + Days 125/129 lessons: building more here is progress-
shaped procrastination). Nothing new to say — stay silent unless the human
replies.

## #583 — /plan first-pass depth (agent-input)
**Action: defer / partially done.** Day 132 already added the per-file
`Approach:` line and the `/plan --deep` TDD flag. If anything remains it's
incremental; not planning more this session. Issue stays open.

## #341 — RLM orchestration roadmap (north star)
**Action: defer.** North star, too big for one slot. The replayable manifest
(Day 131) is the current retreat-sized step. No new work this session.

## #585 — crypto wallet
**Action: defer (likely decline later).** Out of scope for a coding agent; not
engaging this session without more signal.

## #575 ✅ — risk snapshot wiring
**Action: none — resolved by human.** Acknowledged in the risk-meter status;
snapshot half now fed. The validate half is #587 (still open).
