Title: #810 — give measure_abstentions.py a real arg parser and a `--since-sha` / `--since` eligibility boundary
Kind: evolve
Files: scripts/measure_abstentions.py
Issue: #810

## Why

Two separate defects on the same surface, both confirmed this session.

**(a) There is no argument parser.** `main()` special-cases `--test` and then treats every
remaining argv entry as a path. `python3 scripts/measure_abstentions.py --help` dies with an
uncaught `FileNotFoundError: '--help'`. A tool whose whole job is honest measurement should
not answer `--help` with a traceback.

**(b) @yuanhao asked for one specific thing on #810** (2026-08-21T21:25Z), quoted verbatim
so the implementation agent does not have to re-fetch it:

> **Suggested instrument change:** teach it the boundary — a `--since-sha` or `--since`
> argument that marks sessions whose head predates the fix as *ineligible* rather than
> *gradeable*. Otherwise the verdict line will keep combining "could not fire" with
> "did not fire", and the number gets worse-looking as more pre-fix history accumulates
> in the window. The rest of the tool is right and its `--test` self-check is what caught
> my own contaminated measurement, so this is one gap, not a rewrite.

He is right, and the current output proves it: the verdict line today reads
`of 16 gradeable sessions, the gate fired in 0` where **all 16 sessions predate the #808 fix**
and a firing was impossible by construction. That is not a 0% hit rate, it is a category
error — "could not fire" being reported as "did not fire". It is the same defect the tool
already fixed once for a different reason (a shrinking or contaminated denominator inside my
own meter), which is exactly why it must not stand in the denominator half.

**Scope discipline: he said "one gap, not a rewrite" and he means it.** Do not restructure
`classify_line`, `classify`, `is_quoted_prose` or the anchored-line matching. Those are
correct and were built to solve the contamination problem. This task changes *which sessions
are graded*, plus the CLI surface, and nothing else.

## What to do

Read `scripts/measure_abstentions.py` first — the names below come from this session's
assessment and the repo symbol map, so verify them against the actual file before editing.
Existing shape: consts `A1_ABSTENTION`, `PLANNER_ZERO_TASKS`, `PLANNER_FALLBACK`,
`AUTO_CONTINUE_FIRING`, `MIN_GRADEABLE_SESSIONS`; functions `strip_ansi`, `is_quoted_prose`,
`classify_line`, `classify`, `count_lines`, `grade`, `read_lines`, `session_from_path`,
`main`, `run_self_tests`; dataclass `SessionCounts`.

### Step 1 — real argument parsing

Replace the hand-rolled argv walk in `main()` with `argparse`. Keep every existing
invocation working byte-identically:

- `python3 scripts/measure_abstentions.py --test` → runs `run_self_tests()` and exits.
  This must keep working exactly as it does today, because CI/agents call it.
- Positional paths (session dirs / log files) keep their current meaning and current output.
- `--help` must print usage and exit 0 instead of raising.

Do not change any output text for an invocation that passes no new flags. That is the
regression risk here, and it is the whole reason to check it deliberately.

### Step 2 — the eligibility boundary

Add two mutually-exclusive flags:

- `--since-sha <sha>` — resolve the commit's timestamp with
  `git show -s --format=%cI <sha>` (subprocess, at the I/O call site, never inside a pure
  decision function — the file's existing style splits these and must keep doing so).
- `--since <ISO8601>` — take the timestamp directly, no git needed.

Then classify each session into **three** states, not two. Absence and ineligibility each get
their own name; do not fold either into the other:

1. `INELIGIBLE` — the session's timestamp is **strictly before** the boundary. It could not
   have exercised the change, so it is excluded from **both** numerator and denominator.
2. `GRADEABLE` — at or after the boundary, and it recorded ≥1 abstention (the existing rule:
   a session with zero abstentions is already excluded from both halves — keep that rule
   exactly as it is, this task does not touch it).
3. everything else — unchanged from today.

The session timestamp source: `session_from_path` already derives a session id from the
`sessions/day-N-<ts>/` directory name, which carries an ISO-ish UTC stamp. Prefer parsing
that (it needs no git, no network, and works on a bare `audit-log` checkout). If the input
is a plain log file with no parseable stamp, that session's eligibility is **unknown** — and
unknown is NOT eligible and NOT ineligible: count it in its own bucket and report the count,
the same discipline `UngradedScan.unkeyed_excluded` and `NeverForecast`'s unknown-age branch
already use elsewhere in this repo. A silently-absorbed unknown inside a meter is the exact
defect this tool exists to avoid.

With no `--since*` flag passed, every session is eligible and the output must be
byte-identical to today's. Verify that.

### Step 3 — say it in the verdict

The verdict line must report the boundary and the exclusions, not just the surviving number.
Something in the shape of (wording is yours, the *facts* are the requirement):

```
boundary: <sha-or-ts> (2026-08-21T...)
excluded: 16 session(s) predate the boundary — could not fire, not graded
unknown age: 0 session(s)
of 2 gradeable sessions, the gate fired in 0
```

And keep the existing `MIN_GRADEABLE_SESSIONS = 4` refusal: below the floor it must still
**refuse a verdict** and print the wait. With a boundary applied this will now be the common
case, and that is correct — "not yet gradeable" is its own state and is not "the gate failed".

### Step 4 — self-tests

Extend `run_self_tests()` with cases that pin the new behaviour:

- a session strictly before the boundary is excluded from both numerator and denominator
- a session exactly **at** the boundary is eligible (pin the boundary inclusivity explicitly,
  in both directions — one test on each side, so the discriminator is not tested only on the
  side that blocks)
- an unparseable/absent timestamp lands in the unknown bucket and is counted, never silently
  dropped and never quietly promoted to eligible
- with no boundary flag, grading is identical to the pre-change result

`python3 scripts/measure_abstentions.py --test` must print `all self-tests passed`.

## Done when

- `--help` exits 0 with usage; `--test` still passes; no-flag invocation output unchanged
- `--since-sha` and `--since` both work; the verdict names the boundary and the excluded count
- the three eligibility states are distinct and all three are pinned by self-tests
- `cargo build && cargo test` still green (no Rust touched, but run it anyway)

## Docs

Update the `scripts/measure_abstentions.py` bullet in CLAUDE.md: name the two new flags, state
the three-state eligibility rule, and record **why** the boundary exists (a pre-fix session
reads as "did not fire" when it means "could not fire"). Keep the existing "what it cannot do"
sentence — it checks that a marker was *emitted*, never why the model abstained, and never
whether a firing was the right continuation. That limitation is unchanged by this task and
must not quietly disappear from the docs.
