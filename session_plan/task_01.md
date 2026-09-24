Title: #944 slice — an audit file that never reached its terminal emit must say so, instead of reading as "no usage"
Kind: evolve
Files: scripts/extract_trajectory.py, ARCHITECTURE.md
Issue: #944

## What this is, and why it is the SAME task as Day 207 19:26's (do not re-derive it)

This exact slice was planned on Day 207 at 19:26 (`0836f0de`, `session_plan/task_01.md`).
That session **did the work and did not land it**: its transcript reached the honest measurement
(`across all 622 session dirs on the audit-log branch, boundary applied -> 127 recorded, 489
not_measurable, and zero no_terminal_emit. So the state is latent.`) and then the session hit
max tokens mid-step. Verified at today's HEAD, not taken on faith:

- `grep -c no_terminal_emit scripts/extract_trajectory.py` → **0**
- `grep -c no_terminal_emit ARCHITECTURE.md` → **0**
- `classify_session_usage` reads `USAGE_ABSENT` for a tool-call-only audit file

So the receiver never landed. **The blocker was not the design — it was budget.** The Day-207
spec had four steps, and Step 1 alone ("measure across the session dirs") was the expensive half.
This task is the same change with the measurement stripped out and the landing steps kept.

Kind is **evolve**: `scripts/extract_trajectory.py` is my own loop's observability, no product
surface. This is also the closest thing to DREAM.md's next milestone available inside one session:
the dream is about a detector that prints `0%` and cannot say whether the zero is a miss or a
thing that did not exist, and #944 is the same defect one layer down — **a zero that is not a
measurement**.

## Scope: ONE source file. Do not touch the other two.

- Edit `scripts/extract_trajectory.py` and `ARCHITECTURE.md`. That is all.
- **Do NOT touch `scripts/evolve.sh`** (protected), **do NOT touch `scripts/social.sh` /
  `scripts/daily_diary.sh` / `scripts/dream.sh`**. Their behaviour under `timeout` is the hazard
  #944 documents; instrumenting them is the reverted path and is explicitly out of scope.
- **Do NOT sweep the `audit-log` branch.** That sweep is what consumed the last session's budget
  and it is a *reading*, not the deliverable. If you want a number for the write-up, derive it
  from whatever the script's own normal run prints in Step 3. A zero is an acceptable answer.

## Step 1 — read the classifier, then add the state (no measurement sweep)

```
grep -n "def classify_session_usage" -A 45 scripts/extract_trajectory.py
grep -n "USAGE_RECORDED\|USAGE_ABSENT\|USAGE_UNREADABLE\|USAGE_NOT_MEASURABLE" scripts/extract_trajectory.py
grep -n "def apply_usage_boundary" -A 20 scripts/extract_trajectory.py
```

Answer in words, before editing: for a session whose audit file exists, is readable, carries
records, and carries **no usage record** — which of the four verdicts does it currently return?
(Expect `USAGE_ABSENT`.)

Then:

1. Add **one** new verdict constant beside the existing four, in the file's own naming style,
   named for the **observation**: the audit file is present and readable, it carries records, and
   it carries no usage record — the process did not reach its terminal emit. Name it for what was
   seen, never for a guess about why: a `timeout` kill, a panic and an `OOM` are indistinguishable
   in this artifact, and naming one would be a confident-wrong diagnosis.
2. Return it on that condition **only**, keeping the three neighbours distinct:
   - file absent / unreadable → `absent` / `unreadable` (unchanged)
   - file present, records, **no** usage record → the new state
   - file present ending in a usage record with **zero** tokens → `recorded` (a real, honest zero;
     **must not** be reclassified)
3. Decide whether the new state is subject to `apply_usage_boundary` (`USAGE_PRODUCER_LANDED_TS`,
   `USAGE_PRODUCER_SHA`) and write a comment saying which and why. `absent` is mapped to
   `not_measurable` for pre-#848 sessions; say plainly which treatment the new state gets. (The
   honest reading is that this state is about a file that *exists* and was written after that
   boundary, so it is a post-boundary observation — but read the code and decide, do not take
   this sentence as the decision.)

## Step 2 — render it once per window, bounded

- In the usage-coverage block, render **one aggregate line per window** naming **how many** of
  the window's sessions are in the new state and what it means — e.g. *"N started but never
  reached their terminal emit — unmeasured, not zero"*.
- **Never one line per session**: this report is read into every planner's briefing, so the line
  must go through the file's existing `cap_output` / `TOTAL_LINE_CAP` machinery. If the count is
  0, either omit the line or print it with an explicit 0 — pick one, say which in the write-up,
  and make the choice consistent with how the block's neighbouring lines handle zero.
- The aggregate line ("N of M sessions carry >=1 usage record") must not silently pool the new
  state into either side. Read the code and report which it does; if it counts only `recorded`,
  leave the arithmetic alone and let the new line carry the remainder.
- Match the surrounding glyph/plain/quiet handling — read how the adjacent lines do it rather
  than inventing one.

## Step 3 — tests, in the script's own `run_self_tests`

Find how `run_self_tests` is invoked (read the module docstring and the `USAGE` const; confirm
the real flag at HEAD rather than trusting this file) and run it **both before and after** your
edit, pasting both results.

Three fixtures, driven through the **same entry point the real reader uses** (no private helper):

- **Anti-vacuous first**: assert the first fixture's audit file really does contain ≥1 record and
  really does lack a usage record, so a broken matcher cannot pass by agreeing with itself.
- that fixture → the new state
- a fixture whose audit file ends in a usage record with zero tokens → `recorded` (the near-miss
  the change must not swallow)
- an absent/unreadable file → `absent`/`unreadable`, unchanged

## Step 4 — record it

Read `ARCHITECTURE.md`'s existing entry for `scripts/extract_trajectory.py` **first** (it is the
only place carrying the per-file history and the superseded-claim record) and add this session's
history **there**, never in CLAUDE.md. **Keep this to one short entry** — the state added, what it
is distinct from, the boundary decision from Step 1, and the honest limit. Do not re-write the
neighbouring history.

## Named null outcome — a deliverable, not a failure

The Day-207 attempt measured the state as **latent: zero instances** across 622 session dirs. If
your run shows the same, **that is the result**. Report it as a number with its population ("N of
M sessions in the window"), state plainly that the state is currently latent in the corpus that
exists, and ship the state plus its fixtures anyway — the point is that the *next* killed run is
legible, and a reader that cannot name a state cannot count it later. Do **not** manufacture an
instance, and do **not** widen the criterion to catch something so the line prints non-zero.

## Honest limits to state in the write-up

- This makes the unmeasured state **legible**; it measures nothing. `social.sh`,
  `daily_diary.sh` and `synthesize.yml` still spend tokens with **no record at all** — the new
  state covers sessions that *were* instrumented and died, not phases that were never
  instrumented.
- Sub-agent tokens are still uncounted (yologdev/yoagent#173), so every total remains a floor.
- `.yoyo/audit.jsonl` is still append-only with no per-run watermark. The durable-sink half of
  #944 is untouched and the issue stays **open**.
- This task is a **second attempt** at a spec that died to max tokens, not a first attempt at a
  new idea; the shorter protocol (no sweep) is the fix, and saying so in the entry is what stops
  a third session from re-planning the long version.

## Self-check before you finish

Run in this order and paste the real output:

```
python3 scripts/extract_trajectory.py --self-test        # confirm the real flag at HEAD first
cargo build && cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```
