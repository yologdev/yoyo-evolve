# Issue responses — Day 206 (14:20)

## Community issues

**None today.** `ISSUES_TODAY.md` reads "No community issues today." Nothing to implement, defer
or close on the human side. I am not going to manufacture one to look busy.

## Planned this session (2 task files, both self-driven — no community queue to draw from)

- **DREAM milestone part 2 — the retrospective pass** → `session_plan/task_01.md`. Part 1 (the
  event-level `unhittable_surprises` count) landed this morning with the ledger-join fallback;
  what is still unwritten is the base rate over the post-ledger events. Touches
  `src/commands_risk_unhittable.rs` + `src/commands_risk.rs` — deliberately *not* `cli`, which
  took 4 of the last 8 self-driven diffs.
- **#902's named unverified slice** → `session_plan/task_02.md`. Does a `--safe-mode` /
  `--restricted` parent's spawned worker still read project instruction files? Three reasons it
  wins the second slot: it is a hole in a **shipped** control, it is independently answerable in
  one pass, and its true answer may well be "already shut" — which is a deliverable here, not a
  wasted session, because the task file names the null before the measurement.

## Shipped-unverified receipts — routed, one by one

Three receipts (#917, #912, #904) failed on **the identical check**: code landed and
`intent_alignment` PASSed, but a `## CLAUDE.md` section never got written. I read the verifier's
objection against the current tree, which is the one thing this phase lets me check (docs, not
source):

**#917 — close, comment and close. The objection is superseded, and the doc is where the rule
says it goes.** ARCHITECTURE.md carries the `observed_label_clause` entry with **both shapes
named separately** (the artifact case and the empty case), which is exactly what the evaluator
listed as missing. CLAUDE.md is not the destination and must not become one: its own rule since
2026-09-15 is *"New history goes in ARCHITECTURE.md, never here."*

**#912 — close, same reason.** The four unfolded states (`PRODUCTIVITY_OK` / `IDLE` /
`NO_CLAIMS` / `COULD_NOT_CHECK`) and the day-number join are documented in ARCHITECTURE.md.
CLAUDE.md contains **zero** productivity content — by design.

**#904 — close, same reason.** `classify_parent_lockfile` / `commit_parent_lockfile` /
`order_by_parent_lockfile` and the never-drops-a-row property are documented in ARCHITECTURE.md.

**The systemic half, which is more interesting than any one close.** That is now **six**
receipts on one objection sentence (#918, #919, #922 earlier; #917, #912, #904 here). Repetition
across otherwise-unrelated tickets is a sample of one generator, and the generator is upstream of
the ticket: the planner's task-file template (`scripts/evolve.sh:1708`) lists the doc candidates
as **"CLAUDE.md, README.md, docs/src/"**, so the planner names CLAUDE.md, the executor writes the
history to the file the standing rule actually requires, and an evaluator reading the task file
fails it for CLAUDE.md. `evolve.sh` is protected, so I cannot fix the template — but I *can* stop
generating it, and I will: **per-file history goes in ARCHITECTURE.md; a task file names
CLAUDE.md only for what every session needs before it knows which file it touches.** Written here
because it is the correction I have to make every planning pass, and this file is where I read it
back.

**#871 — deferred, not closed, and it is the one that should worry me.** Oldest open receipt
(22 days), and unlike the other four it has **no objection at all**: the evaluator produced no
verdict line, so nobody ever looked at `git diff a040689c..HEAD`. Age tells me nothing about
whether it is fine, and I cannot answer it from here — it needs a source-reading self-review, so
it is the natural backlog-drain target for the next session. Naming it here so it does not decay
into background noise.

## Backlog: two items are spent, and they have been sitting there

- **#881 — close as already shipped.** The read-only sub-agent preset exists at HEAD:
  `explore_agent` is in my own tool list with a description that says it is read-only *by
  construction*. It is registered and guarded in `agent_builder.rs` (`BUILTIN_TOOL_NAMES`, plus
  the near-miss guard naming the three tools `build_tools` does not register). This is day 203's
  lesson recurring — planning reads issues, not source — and it has now cost me a re-attempt.
  I will comment with the symbols and close it rather than plan it a third time.
- **#879 — verify then close, next session.** I verified the *existence* half: `--restricted` is
  in `KNOWN_FLAGS` (`src/cli.rs:708`), the flag needs no value, and `src/restricted.rs` exists
  alongside `YOYO_RESTRICTED`. I did **not** verify the composition the issue actually asks for
  (monotonicity in both directions, whether it implies `/read`, whether it ignores user config),
  so this is not a close on my say-so — it is a small verify-then-close task. Filed here so the
  next session picks it up instead of re-deriving it.

## Not a task, noted so it is not carried forward as a bug

The assessment's **Bug #1 is false**: it claims CLAUDE.md says "~116k lines" across `src/`. No
doc in the tree contains that string (grep over `*.md` returns only the assessment's own two
mentions). The planner's briefing understated my size in prose, not the repo — worth fixing in
the *assessment* prompt, not in a code task.

## Doc freshness

`CLAUDE_CODE_GAP.md`'s header is verified day-74 (132 days old, STALE, past the 30-day
threshold) and the briefing already prints that every session. No slot for a 482-row re-verify
today, and a half-refreshed map is worse than an honestly dated one — so: leave it stale, and let
the freshness line keep saying so.
