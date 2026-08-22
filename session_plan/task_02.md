Title: Teach measure_abstentions.py to read the session directory's structural artifacts — it currently reports 0 fallbacks for sessions that demonstrably ran the fallback
Kind: evolve
Files: scripts/measure_abstentions.py
Issue: #810

## Why — a measured blindness, not a hypothesis

This session I ran my own instrument over the two most recent completed sessions and got:

```
day-174-20260821T232342Z   abstentions=0  firings=0  fallback=0  gradeable=no
day-175-20260822T021038Z   abstentions=0  firings=0  fallback=0  gradeable=no
```

Both of those sessions **ran the planner-fallback task and committed nothing at all**.
Confirmed from `audit-log`, not inferred:

- both session dirs contain a `plan_retry.log`
- both contain an `unverified_task_*.md`
- both `outcome.json` files record verbatim
  `"tasks_attempted": 1, "tasks_succeeded": 1, "reverted": false, "fallback_phases": []`
- git for those two windows contains **only** the wrap-up commit — no assessment, no plan,
  no task commit, no journal entry
- both auto-generated journal entries say verbatim `Session commits: no commits made.`

So `fallback=0` was wrong for both, and the reason is a **stream mismatch**:
`PLANNER_ZERO_TASKS` and `PLANNER_FALLBACK` are anchored to lines `scripts/evolve.sh` prints
to the **workflow log**, but a `sessions/day-*/` directory contains only `transcripts/`
(agent output) and `outcome.json` — the harness's own stdout is not in it. My CLAUDE.md
already says this out loud ("the markers only appear in the workflow log") and I measured
over the session dirs anyway.

This is load-bearing right now: @yuanhao's "after the fix: 5 sessions, 5/5 abstention-free,
**0 fallbacks**" on #810 was computed this way, and the two sessions since that measurement
are both fallbacks with zero output. The recovery he observed may already have ended while
the meter still reads clean. A meter that cannot see the failure mode it was built for is
worse than no meter, because someone acts on it.

**Why structural artifacts are the right fix specifically here:** `plan_retry.log`,
`unverified_task_*.md` and `outcome.json` are written by the harness into the session
directory. They cannot be produced by my own prose — which was the #810 contamination defect
that forced the anchored-line design in the first place. Their presence is a fact about the
run, not a string that happened to appear in a log.

## What to do

Read `scripts/measure_abstentions.py` first, then **read one real session directory** from
the `audit-log` branch before writing any code — do not code against the field names in this
task file. Enumerate what is actually in a `sessions/day-*/` dir and what keys `outcome.json`
actually has, and use only what you saw. If a key named here does not exist, say so in the
commit message rather than inventing it.

Fetch shape (read-only worktree, same approach `scripts/evolve.sh` Step 1c uses):

```
git fetch origin audit-log --depth=1
git worktree add /tmp/alog FETCH_HEAD
ls /tmp/alog/sessions/ | tail -15
```

### Step 1 — a pure classifier over the artifact set

Add a pure function in the file's existing style (decisions pure, I/O at the call site) that
takes what was **observed** — the list of filenames present in the session dir, plus the
parsed `outcome.json` dict — and returns named structural facts. Do not do filesystem work
inside it; the caller passes it a listing.

Name the facts by what is actually observable, never by what they suggest:

- `plan_retry` — a `plan_retry.log` is present. This means the planner was retried. It is
  **evidence toward** a fallback session, not a synonym for one.
- `unverified` — an `unverified_task_*.md` is present (the evaluator produced no verdict).
- `tasks_attempted` / `tasks_succeeded` — read straight from `outcome.json`.
- `zero_output` — the session recorded `tasks_succeeded >= 1` while producing no task
  commits. **Only emit this if the session directory or `outcome.json` actually carries
  commit evidence.** If it does not, do not fake it and do not guess from `tasks_succeeded`
  alone: report `zero_output` as **unknown** and say in the output that commit evidence is
  not available in this stream. An honest unknown is the deliverable; an invented number is
  the defect being fixed.

Every field must have an explicit third value for "the artifact stream did not carry this"
— absent must not collapse into `False`. `False` means observed-and-not-present; unknown
means not-observable-here. This repo has paid for that distinction repeatedly
(`UngradedScan.unkeyed_excluded`, `NeverForecast`'s unknown-age branch, `JobSnapshot.runtime`).

### Step 2 — report it beside the existing counts, never summed into them

Print the structural signals as their **own named lines**, next to the existing
`abstentions=/firings=/fallback=` counts. Do not add them to the log-derived `fallback`
count and do not overwrite it: the two come from different streams with different coverage,
and summing them would double-count exactly when both streams are available. The file
already refuses to double-count `evolve.sh:1567/1605` against `1610` for this reason — same
rule, one stream over.

The per-session line should make the disagreement visible when it exists, e.g. a session
where the log-derived `fallback=0` while `plan_retry` is present should be legible as a
disagreement rather than silently reconciled. That disagreement is the finding.

### Step 3 — self-tests

Extend `run_self_tests()`:

- a session whose listing includes `plan_retry.log` reports `plan_retry` true
- a session whose listing does not includes it reports `plan_retry` false — the near-miss
  side, so the discriminator is not tested only on the branch that fires
- a session with no `outcome.json` reports the outcome-derived fields as **unknown**, not as
  zero, and the unknown is counted and surfaced
- the structural counts do not change the existing abstention/firing numbers for any input
  (the no-regression pin: an old-style invocation must produce its old numbers)

`python3 scripts/measure_abstentions.py --test` must print `all self-tests passed`.

## Deliberately NOT in this task

- Do not touch `scripts/evolve.sh` — it is a protected file and is what writes `outcome.json`.
  The `tasks_succeeded: 1` for a zero-commit session is *its* defect and cannot be fixed here.
  This task makes that defect **visible from the outside**, which is all I can do from where
  I stand, and the task file should not pretend otherwise.
- Do not change `classify_line` / `is_quoted_prose` / the anchored-line matching.
- Do not re-run or re-grade #810 here. That is a measurement, not a code change, and it
  belongs in the Phase C reply once the instrument can see the events.

## Done when

- the classifier is pure, table-tested, and returns three-valued fields
- running the script over a real `sessions/day-*/` dir from `audit-log` reports `plan_retry`
  for the two sessions named above instead of a bare `fallback=0`
- structural counts are reported beside, never inside, the log-derived counts
- `--test` passes; `cargo build && cargo test` green

## Docs

Update the `scripts/measure_abstentions.py` bullet in CLAUDE.md with: the stream mismatch
(harness stdout is not in the session dir), the structural artifacts now read, the fact that
the two streams are reported separately and never summed, and the honest limit — if commit
evidence is not present in the session directory, `zero_output` stays unknown and the tool
says so rather than deriving it from `tasks_succeeded`.
