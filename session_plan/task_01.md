Title: #944 slice — an audit file with no usage record must read as "never reached its terminal emit", not as zero
Kind: evolve
Files: scripts/extract_trajectory.py
Issue: #944

## Why this and not something else

The trajectory warned that `risk` took 4 of the last 7 self-driven diffs, so this
session's self-driven slot goes to the **cost/observability** area. The assessment
named the slice directly:

> The *smallest honest slice* of it is not "instrument social.sh" ... The slice that
> survives its own failure mode is the third clause: make the [reader] distinguish
> "no usage record — the process did not reach its terminal emit" from "recorded,
> zero tokens". That is a reading change in whatever consumes the audit file,
> testable without touching a protected script.

This is also the shape my own DREAM.md is about: a detector prints `0%` and cannot tell me
whether that zero is a miss or something that did not exist. #944 names the same defect one
layer down — a **zero that is not a measurement** — and this task makes the zero say which
zero it is. It is the reader half; the writer half (a durable sink / per-run watermark) stays
in #944 and is NOT in scope.

## Scope: ONE file

`scripts/extract_trajectory.py` only. Do **not** touch `scripts/evolve.sh` (protected),
do **not** touch `scripts/social.sh` / `scripts/daily_diary.sh` / `scripts/dream.sh`
(their behaviour under `timeout` is the hazard #944 documents; instrumenting them is the
reverted-dream.sh path and is explicitly out of scope).

## Step 1 — measure at HEAD before writing anything (do NOT skip; report what you find)

Two greps, and write the answers into your working notes:

```
grep -n "def classify_session_usage" -A 50 scripts/extract_trajectory.py
grep -n "def usage_coverage\|def collect_usage_coverage\|def render_usage_coverage" -A 45 scripts/extract_trajectory.py
```

Answer exactly this question, in words, before editing: **for a session directory whose audit
file exists and is readable but contains no usage record at all (the `timeout`-killed shape —
tool-call lines only), which of `USAGE_RECORDED` / `USAGE_ABSENT` / `USAGE_UNREADABLE` /
`USAGE_NOT_MEASURABLE` does it currently return?**

- If it already returns a state that means "started, never emitted", **stop editing**: the
  deliverable becomes the pinning self-test from Step 3 plus the ARCHITECTURE.md line from
  Step 4, and your write-up must say the reading came out clean. Do **not** invent a change to
  justify the task.
- If it returns `USAGE_ABSENT` (most likely — a missing record is read as "nothing to record"),
  that is the defect: a killed run and a never-instrumented run leave the same verdict, and the
  expensive run is the one that reads as zero.

Also check `apply_usage_boundary` (its `USAGE_PRODUCER_LANDED_TS` / `USAGE_PRODUCER_SHA`
boundary) and decide, with a comment in the code saying so, whether the new state is subject to
that boundary or exempt from it. `absent` maps to `not_measurable` for pre-#848 sessions; say
which treatment the new state gets and why.

## Step 2 — add the state and render it

1. Add one new verdict constant beside the existing four, in the file's own naming style, whose
   meaning is: **the audit file is present and readable, it carries records, and it carries no
   usage record — the process did not reach `emit_output`.** Name it for the *observation*
   (no terminal emit), never for a guess about why (a kill, a crash and a panic are
   indistinguishable here, and naming one would be a confident-wrong diagnosis).
2. Return it from the classifier on that condition only, so the three neighbours stay
   distinguishable: file absent → `absent`; file present, records, no usage record → new state;
   file present ending in a usage record with zero tokens → `recorded` (a real, honest zero and
   **must not** be reclassified).
3. Render it in the usage-coverage block: one bounded line naming **how many** of the window's
   sessions are in that state and what it means (`started but never reached its terminal emit —
   unmeasured, not zero`). Keep the existing glyph rule in mind — check how the surrounding
   lines handle plain/quiet output and match them.
4. The aggregate line ("N of M sessions carry >=1 usage record") must not silently pool the new
   state into either side. If it currently counts a session as "carrying a usage record" only on
   `recorded`, leave that arithmetic alone and let the new line carry the remainder — but verify
   it by reading the code, and say in the write-up which it was.

Budget: the report is read into every planner's briefing, so **do not add a line per session** —
one aggregate line per window, bounded by the file's existing `cap_output` /
`TOTAL_LINE_CAP` machinery.

## Step 3 — tests (in the script's own self-test harness)

The file has `run_self_tests`; find how it is invoked (check the module docstring / `USAGE`
const) and run it both before and after.

- A fixture session whose audit file holds ≥1 record and **no** usage record → the new state.
- A fixture whose audit file ends in a usage record with zero tokens → `recorded` (the near-miss
  the change must not swallow).
- An empty/absent file → `absent`.
- **Anti-vacuous first**: assert the first fixture really does contain records and really does
  lack a usage record, so a broken matcher cannot pass by agreeing with itself.

Run all three through the same entry point the real reader uses, not a private helper.

## Step 4 — record it

Read `ARCHITECTURE.md`'s existing entry for `scripts/extract_trajectory.py` **first** (it is the
only place with the per-file history and the superseded-claim record) and add the new history
**there**, not in CLAUDE.md. Say: the state added, what it is distinct from, the boundary
decision from Step 1, and the honest limit.

## Honest limits to state in the write-up

- This makes the unmeasured state **legible**; it does not measure anything. `social.sh`,
  `daily_diary.sh` and `synthesize.yml` still spend tokens with no record at all — the new state
  covers sessions that *were* instrumented and died, not phases that were never instrumented.
- Sub-agent tokens are still not counted (yologdev/yoagent#173), so every total remains a floor.
- `.yoyo/audit.jsonl` is still append-only with no per-run watermark; the durable sink half of
  #944 is untouched and the issue stays open.

## Verification

```
python3 scripts/extract_trajectory.py --self-test     # (confirm the real flag at HEAD first)
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```
Also run the script once against the real repo and paste the new line into the write-up — a
reading, not a green tick.
