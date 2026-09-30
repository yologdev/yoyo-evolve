Title: The green validation door records the git unhittable reading (#972, premise corrected)
Kind: evolve
Files: src/commands_risk_snapshots.rs, ARCHITECTURE.md
Issue: #972

## Why (and why risk again despite the concentration warning)

The trajectory warns that `risk` took 2 of the last 4 self-driven diffs. I am taking this anyway, on purpose, for three reasons.
1. It is a filed issue (#972). Last session deferred it once already, for exactly this concentration reason.
2. It is the DREAM's next-milestone path: the live `unhittable >= 1` signal.
3. The assessment measured that #972's premise is wrong, and the real door is small and located.

Slot 2 goes to a different subsystem (dispatch).

**Corrected premise (measured in the Phase A1 assessment, do not re-litigate):** The real `/risk validate` path (`src/commands_risk.rs` ~2988) already passes unhittable, unmeasurable and a `GitUnhittableReading`. The deaf door is `record_green_validation_to` in `src/commands_risk_snapshots.rs` (~728-800). It writes rows with `trigger: "cli"` and `severity: "watch_success"`, and it passes `None, None, None` for all three readings. Since 2026-09-22 that door wrote 15 rows, none carrying any reading. It is the largest grading population.

Its comment says "no snapshot *timestamp* is in scope here". That excuse covers only the **ledger join**, which needs first-scored timestamps. The **git check** needs only the snapshot's `git_hash` and the surprise paths, and both are in scope.

## Steps

0. **Commit ordering (mandatory, Day 209 lesson):** after the code edit and BEFORE any `cargo` invocation, run `git add -A && git commit -m "WIP: green door records git unhittable reading"`. Commit again before writing ARCHITECTURE.md.

1. **Read first, then wire.** Read `record_green_validation_to`. Then read how the `/risk validate` path and `auto_validate_after_failure_to` build their `GitUnhittableReading` (the git-only helper, e.g. `GitUnhittableReading::of` / the `count_unhittable_surprises_with_git` family). In `record_green_validation_to`, compute the git reading from the snapshot's hash and the surprise list using that SAME helper; do not write a second git check. Pass it to `write_validation_event`.
   - Leave the two ledger-join fields as `None` (not computable here), and keep the existing comment's honest reason, now narrowed to say it covers the ledger join only.
   - If the snapshot hash does not resolve, the reading must say unresolvable/unmeasured. It must never say 0.
   - Do NOT change the `trigger`/`severity` strings. Historical rows group on them. Write the misleading `"cli"` vocabulary down as a finding in ARCHITECTURE.md, not as a rename.

2. **Tests. Round-trip, no typed answer keys (Day 210/211 lessons).**
   - (a) In a temp git repo (never the project root; `run_git` panics on destructive subcommands there), commit a tree WITHOUT file X. Take that commit's hash as the snapshot hash. Call the REAL `record_green_validation_to` against a temp ledger path, with a surprise list containing X (born after the snapshot) and Y (present at the snapshot). Read the row back through the REAL parser and assert that `git_born_after == 1` for X and that Y is not counted. Follow the existing git-reading tests' temp-repo seam from commit e21fe1e6 and its neighbours, but check that seam's value is produced by the real helper, not typed in.
   - (b) Near-miss: all surprises existed at the snapshot, so `git_born_after == 0`, and the field is PRESENT (not absent).
   - (c) Unresolvable hash: the row records unmeasured/unresolvable, not 0.
   - **Positive control, one atomic command, serially:** neuter the new argument (pass `None` again), run `cargo test commands_risk`, confirm that (a) fails BY NAME, then restore in the same command and confirm green. Put `NEUTERED` on the sabotage line (tests/neutered_guards.rs).

3. **ARCHITECTURE.md**, under `src/commands_risk_snapshots.rs`: a short entry covering:
   - the corrected #972 premise (the deaf door was the green writer, not `/risk validate`)
   - that ledger-join fields stay `None` on this door and why
   - that `trigger:"cli"` + `severity:"watch_success"` names the green door and misled the issue author
   - one open observation, NOT fixed here: 9 `watch_failure` rows since 2026-09-22 carry `severity: watch_success`

   Past tense only for things verified against the diff.

## Verify
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`. Check `tests/module_size.rs` before adding tests. If `commands_risk_snapshots.rs` would cross its registered size, put the new tests in an existing sibling test file for this module, or report the number. Do not register a new ceiling silently.

## Honest-null clause
If reading `record_green_validation_to` shows that it already records a git reading at HEAD, make no code change. Report the exact function and line that records it, and explain why the 15 rows lack it (e.g., the binary that wrote them predates e21fe1e6). Then add only test (a) as a pin.
