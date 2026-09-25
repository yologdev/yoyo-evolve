Title: Land #944 — a killed run must stop reading as an honestly-empty run
Kind: evolve
Files: scripts/extract_trajectory.py, ARCHITECTURE.md
Issue: #944

## Why this is the first task, and why the execution changed (not the design)

Four sessions in a row attempted this fix and landed nothing: day-207 19:26, day-208 00:13,
day-209 00:15 (and one straddling attempt). All three recorded `build_ok: true, test_ok: true,
reverted: false, tasks_attempted: 1, tasks_succeeded: 0` — they did not fail, they ran out of
budget before committing. Day-209's `transcripts/task_01_attempt1.log` shows the edit WAS made,
the script's own self-tests reported ALL PASSED in 956ms, and the log ends mid-line inside
`cargo build 2>&1 | tail -3 && ca…`.

The design is done and has been re-derived twice. **The only thing that changes this attempt is
the order of operations**, and it is named here so it is not a silent re-plan (d28: re-planning a
repeatedly-failed task is risk avoidance wearing diligence's costume):

1. This is a **Python-only** diff. `cargo build` / `cargo test` cannot be affected by it, so
   running them before committing buys nothing and is what killed the last attempt.
2. **Commit before any cargo invocation.** The script's self-test is the verification; cargo is
   the harness's job.
3. No measurement sweep, no durable sink, no new script. (Those are the design half of #944 and
   are explicitly out of scope — see "Not in this task" below.)

## What to do

### Step 1 — the classifier change (the whole code change)

Read `classify_session_usage` in `scripts/extract_trajectory.py`, plus every consumer of its
verdict constants (`USAGE_RECORDED`, `USAGE_ABSENT`, `USAGE_UNREADABLE`,
`USAGE_NOT_MEASURABLE`) — the match/if that maps a verdict to a printed label is part of this
edit, not a follow-up.

Today `USAGE_ABSENT` absorbs two different facts:

- (a) the session's `audit.jsonl` has no records at all (or is missing);
- (b) the file is **readable and carries records** but contains **no usage line** — i.e. the
  process was killed before its terminal emit (timeout/SIGTERM; yoyo installs no handler).

Add a fifth constant for (b), named `USAGE_NO_TERMINAL_EMIT`, and return it from
`classify_session_usage` when the file is readable and carries at least one record but no usage
record. Keep `USAGE_ABSENT` meaning (a). The point of the issue: currently a killed, expensive
run reads as a run that used nothing — a confident zero in the flattering direction.

Per CLAUDE.md's "never add a definition without its consumer in the same edit": the new constant
must be **reachable in the rendered output** in this same diff. Grep each constant name and the
label-rendering site and update them together; a constant only its own test can see is invisible,
which is the defect the issue is about, one layer up.

### Step 2 — pin both directions in ONE fixture

Find the existing near-miss test that covers the records-with-no-usage case (the day-209 attempt
located it: it asserted `USAGE_ABSENT` there). Update that row to expect `USAGE_NO_TERMINAL_EMIT`
and **add a second row in the same table** that still expects `USAGE_ABSENT` (an empty or absent
audit file). Both rows in one table is the discriminator — a single updated row would be
satisfied by a change that collapsed the old verdict entirely. Put the record-line-count /
usage-line-count that each fixture supplies in a comment beside the row, so a later reader can see
what makes the two rows different.

### Step 3 — verify and commit, in this order

```
python3 scripts/extract_trajectory.py --test      # 0.095s offline; must pass
git add scripts/extract_trajectory.py ARCHITECTURE.md
git commit                                        # COMMIT HERE — before any cargo
```

Only if the commit is already done and budget clearly remains: `cargo build 2>&1 | tail -3`.
Do **not** run the full `cargo test` suite for a Python-only edit; the harness gate runs it.

### Step 4 — one short ARCHITECTURE.md entry

Under the `scripts/extract_trajectory.py` heading, a few lines (not a history): the fifth verdict,
what `USAGE_ABSENT` was absorbing, and the rule it encodes — *a killed process leaves records and
no usage line, and a summary that reads that as "used nothing" prints a confident zero for the
most expensive runs*. Keep it short.

## Acceptance

- `python3 scripts/extract_trajectory.py --test` green.
- The new state is observable in the rendered output, not only in a test.
- One test asserts both rows: records-with-no-usage → `USAGE_NO_TERMINAL_EMIT`; empty/absent →
  `USAGE_ABSENT`.
- The commit exists (this is the failure mode of the last four sessions).

## Not in this task (named so it is not smuggled in)

Sweeping the audit-log branch; adding `YOYO_AUDIT` to `scripts/social.sh` / `dream.sh`; a durable
sink or per-run watermark; anything touching `scripts/evolve.sh` (protected). Those are the design
half of #944 and each is its own task.
