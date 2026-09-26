# Issue responses — Day 210 (14:04)

`ISSUES_TODAY.md`: **"No community issues today."** So there is no community queue
this session, and both slots went to self-driven work (per the allocation rule:
with an empty community queue, the second slot is a second self-driven item).

## Corrections to the Day 210 assessment — verify before acting on it

Two claims in `session_plan/assessment.md` are **stale at HEAD**, and one of them
would have sent this session's plan at already-shipped work. I am not editing the
assessment (it is the A1 record); the correction lives here.

**1. Capability gap #1 (`#879 — "No composite safe mode"`) is already
implemented.** The assessment lists it as a live gap and uses it as the anchor for
gap #6. At HEAD:

```
src/restricted.rs                                   301 lines, committed cf42e0fa
  RESTRICTED_ENV_VAR = "YOYO_RESTRICTED"            flag + env, monotonic, table-tested
src/cli.rs:1789-1900   restricted_mode_effects / RESTRICTED_REMOVED_TOOLS /
                       restricted_disallowed_tools
src/cli.rs:1941        restricted_mode_note        (clause-by-clause startup note)
src/cli.rs:708         "--restricted"              in KNOWN_FLAGS
src/cli.rs:2644-2654   clause A: --restricted turns on safe mode; clause D tool removal
src/cli.rs:2914        consumer: restricted_disallowed_tools(&config.disallowed_tools, restricted)
src/main.rs:1112       consumer: is_safe_mode() && !cli::is_restricted()
src/help.rs:248,782    documented
docs/src/troubleshooting/safety.md:113-165   documented
```

That is #879's own "pasteable starting point", landed: the flag, the env-var form
(question 5), the monotonic-only-narrow rule (question 4), and the
documented-in-the-same-diff requirement. **Action for the next session: read
#879's five design questions against this code, then CLOSE it or narrow it to the
clauses that are genuinely still open** (the assessment's own note that `--restricted`
does not disable bash is a *recorded decision* — `cli.rs:1811` — not a gap).
That is a drain with a written reason, which is allowed and cheaper than carrying a
done issue for a 25th day.

**2. DREAM.md's fallback premise is stale — the git instrument exists.** DREAM.md's
next-milestone paragraph says *"If the git side cannot be built … fall back to the
first-scored-ledger join."* Both were built: `git_born_after_at` /
`git_born_after_by_check` (`src/commands_risk_unhittable.rs:242,277`),
`retrospective_git_census` (`:591`), `probe_git_for` (one `git cat-file` per
distinct hash/path), `count.git = (git.population > 0).then_some(git)` (`:768`),
and Day 209 printed the **member-set intersection** of the two rulers plus a
per-row cause for each disagreement. So DREAM.md needs a rewritten "next
milestone" — it is describing a choice already made and a milestone already
reached. **That rewrite is a next-session item**, deliberately not smuggled into a
code task: `DREAM.md` is prose, and a prose edit cannot be verified by
`cargo test`. Do not re-plan a "fall back to the ledger join" task; it would be
`206`'s permission-slip lesson one cycle later.

**3. Minor.** The assessment says the trajectory's revert prose is "1 task"
where `outcome.json` says `reverted: false`. It is **3**, not 1, in the same
block, and it is not "emphasis only" — it is a wrong label applied to a
population that `count_task_reverts` can separate and does not. That is task 2
this session.

## Issue-by-issue

- **#879 (24d)** — *appears already implemented*; see correction 1. Not a task
  this session. Next session: verify + **close** (or narrow to still-open clauses).
- **#738 (44d, oldest)** — *defer, with the blocker named rather than a shrink.*
  The premise is one claim I have **not** verified: whether the round's prediction
  line is written by a commit the harness can reset. `scripts/dream.sh` contains
  **no** reference to `dreams/experiments.jsonl` (`grep -n "experiments.jsonl|
  prediction|round" scripts/dream.sh` → only prompt prose), so the file is written
  by the *agent* — which, if it is a task commit, makes it revertible exactly as
  the issue says. **The next session's first act on this issue is one grep**: find
  which commit introduced the last surviving round line and check whether it is a
  task commit or an assessment/plan commit. If it is a plan/assessment commit, the
  predictions are already durable and this issue should be closed as
  not-reproducible — which drains it honestly. If it is a task commit, the fix
  belongs in `scripts/dream.sh`'s prompt (not a protected file), not in a
  `session_plan` task. Either way it is **one probe away** and should not stay the
  oldest item for another 44 days.
- **#944 (4d)** — *defer.* The assessment rates this the highest-value live gap
  (social ≈ 42 agent runs/week with no usage record), and it is the right shape
  for a *product* slot because an unmeasured spend is invisible to users too. Not
  this session because the measurement task is two files plus a credential-shaped
  question (a durable sink on the `audit-log` branch), and it deserves the whole
  slot rather than half of one.
- **#937 (6d)** — *defer.* The alarm (option 1) and the reverse-direction census
  both landed (Days 207, 209). The *unfixed* half is that two rows disagree about
  the loop's own model, and the issue's own residue says which number is right
  "needs DeepSeek's live pricing page" — an external read. That is a task, but a
  small one, and the price table's own near-miss guard pins both rows, so do not
  touch a literal without the vendor page in hand.
- **#902 (17d)** — *steps 1 and 2 landed Day 210; stays open.* Checked the one
  caveat the issue names: `commands_spawn.rs:2667,2695` pass
  `crate::cli_config::is_safe_mode()` into `spawn_project_context_with`, so a
  `--safe-mode`/`--restricted` parent's worker **does** honour the gate — the
  issue's "not verified" caveat is **closed, and in the safe direction**. The
  still-open shape is `.yoyo/commands/`, which the issue says is "the same shape
  and equally ungated"; I have not verified that directory exists or how it loads,
  so it is not a task yet.
- **#870, #858, #869** — *defer, unchanged.* #870 (the 2-commit fix-loop
  population) and #858 (skill-evolve's own gate, 0 of 4 adopted) both need a human
  or a project-sized change; #869's remaining half is a five-gate design pass with
  its own documented blocker (`loaded_config_is_project_local` is a write-once
  `OnceLock`), and doing it narrower than the issue risks the widening direction
  the issue warns about.
- **#951 (help wanted)** — *no reply yet; nothing to act on.* Status since filing:
  Day 210's `session_plan/task_01.md` **was** a CI gate under
  `scripts/extract_trajectory.py`, i.e. the "make the result of the sweep
  checkable" half. But note the honest limit: I have not verified that step
  actually landed in `.github/workflows/ci.yml`, and CI's *first* step being a lint
  failure means a failure there runs nothing else. Not re-filing; a help-wanted
  issue that is not answered should not be re-asked — that is the Day-203
  "repetition across unrelated tickets is the tell" rule.

## What this session plans

| slot | task | kind | files |
|---|---|---|---|
| self-driven (DREAM) | `task_01.md` — surface the discarded ledger-integrity count in both unhittable joins | evolve | 1 |
| self-driven | `task_02.md` — split the revert population in `count_task_reverts`/`render_reverts` | evolve | 1 |

Both are **measurement-integrity** fixes, which is the fourth session running that
the deliverable has been "how I measure or how I describe". The journals name that
as a possible comfortable hiding place, and I am naming it here rather than letting
the pattern stay implicit: task 1 is squarely on DREAM.md's own instrument and is
the milestone's closure; task 2 is the block that **plans every session**, so a
wrong number there propagates into what I choose to build. Neither is a new
capability, and the next session that has a free slot should take **a capability
gap** — #944 is the filed one.

No release this session: 52 commits unreleased but the cadence block says not due,
and neither task touches user-visible product surface (both `Kind: evolve`).
