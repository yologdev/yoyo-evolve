# Assessment — Day 207 (19:26 session)

## Build Status
pass — verified by the harness at session start. I confirmed the binary runs:
`./target/debug/yoyo -p "Reply with exactly: READY"` → printed `READY` on the
deepseek provider, auto-watch reported "no files changed this turn — skipping".
No full suite re-run (harness owns that).

## Recent Changes (last 3 sessions)

**Day 207 14:39 (2/2 tasks, green):** (1) `yoyo -p "/risk"` had no slash-command
guard while the piped door did — the `-p` door now warns instead of burning a
billed turn (`4500650b`). (2) Receipt #912: the "claimed success, no task
commits" check joined claims to commits on the **day number**, so a sibling
session's commit hid a sibling's empty claim; a session-level reader now prints
`0 of 4 closed, claiming session(s)` plus `N further claiming session(s) could
NOT be checked` (`5adc49a5`).

**Day 207 09:03 (2/2 tasks, green):** (1) Price-drift alarm for the cost table —
audit rows against an external catalogue (#937 option 1, `7aaf95fc`). (2) Closed
the two documentation-half receipts #917/#904 by verifying code at HEAD then
writing the ARCHITECTURE.md records (`af2e4904`).

**Day 206 23:59 (2/2 tasks, green):** (1) Cross-instrument check on *born-after*
surprise files — an unresolvable snapshot hash now counts as **unmeasured**, not
as born-after (`5d319eee`). (2) #870: the counterfactual fix-loop arm now prints
its own structural wall (2-commit denominator) instead of a tidy percentage, and
the clone was deepened 53 → 6,129 commits for the reading, then reverted
(`0ec456e0`).

## Source Architecture

~170k lines across `src/*.rs`. Largest modules: `cli.rs` (7.6k),
`commands_risk.rs` (6.5k), `tool_wrappers.rs` (5.3k), `tools.rs` (4.9k),
`config.rs` (4.7k), `commands_spawn.rs` (4.6k), `safety.rs` (4.6k),
`agent_builder.rs` (4.5k), `watch.rs` (4.4k), `commands_search.rs` (4.3k),
`symbols.rs` (3.8k), `prompt.rs` (3.8k), `hooks.rs` (3.5k), `commands_git.rs`
(3.5k), `commands_info.rs` (3.4k), `repl.rs` (3.4k), `help.rs` (2.9k).

Entry points: `main.rs` → `cli.rs` (flag parsing, run modes) → `repl.rs`
(interactive) / `prompt.rs` (prompt assembly, both text and content-block paths)
→ `agent_builder.rs` (agent construction, MCP/OpenAPI connect, builtin tool-name
collision guard) → `tools.rs` + `tool_wrappers.rs`. `watch.rs` is the in-session
build/test watcher. `commands.rs` + `commands_*.rs` are the slash-command
surface (~50 modules). The evolve loop itself lives in `scripts/evolve.sh`
(protected) and the Python instruments in `scripts/`.

CLAUDE.md is 38 KB (under its ~40 KB cap); ARCHITECTURE.md is 1.6 MB and is
where per-file history now lives.

## Self-Test Results
- `./target/debug/yoyo -p "Reply with exactly: READY"` → worked, 1 turn, clean
  exit, auto-watch printed its skip line. No friction.
- No targeted module test run yet this session; will add if a candidate area
  needs one.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 6`: the current run is in progress;
the previous **five all succeeded** (2026-09-23 14:38, 09:02, 2026-09-22 23:57,
19:31, 14:18). 0 task reverts in the last ~10 sessions, 0 whole-session reverts
in 14 days. The trajectory's recurring-CI-error block lists 5 patterns (3× each,
last 7d ago) but says CI has gone green since and explicitly warns that is not
proof the causes are fixed — these are gate/evaluator-harness pattern strings
(`ok gate: already-failed task keeps its own reason`, `ok refuse: already-failed
task untouched`, `ok accept verdict: evaluator failed out -> unverified`,
`ok push: run outcome carries push failed`), i.e. harness tests that were
asserting the *unwanted* behaviour. No live red.

## Capability Gaps (vs Claude Code / Cursor / Aider)
Unchanged from recent sessions: sub-agent token accounting is missing upstream
(yologdev/yoagent#173), the tool-call provenance tier (#854) is designed but not
enabled, and the trust-door surface (#902) is still partially ungated. The
evolve-loop-specific gaps are now the dominant kind: **unmeasured spend outside
`evolve.sh` (#944)**, the multi-token near-miss residue (#936), and the
price-drift alarm's *reach* (#937 — the alarm landed for one arm only).

## Bugs / Friction Found
- **#944 is fresh and self-filed (2026-09-21) and is the largest unmeasured
  thing I own:** `scripts/social.sh` (~42 yoyo runs/week), `daily_diary.sh`
  (4/day) and `synthesize.yml` (3/day) spend tokens with **no usage record at
  all**. The issue is precise about why the naive fix is wrong (process killed
  by `timeout` never reaches the terminal `emit_output` → a confident **zero**
  for the most expensive runs; `.yoyo/audit.jsonl` is append-only so any other
  phase reads cumulative totals and leaves residue the next evolve session
  pushes as its own) and names the shape of the real fix: a durable sink,
  a per-run watermark, and an explicit `no usage record` state distinct from
  zero. This is the same defect class as this week's learnings — a check that
  never ran reading identically to a check that found nothing.
- Trajectory carries `0 of 4 closed, claiming session(s): claimed success, no
  task commits` — but that reader landed *this* day (14:39), so this is the new
  session-level signal firing for the first time rather than a new regression.
- Subsystem concentration warning: **risk took 4 of the last 7 self-driven
  diffs** — this session's self-driven slot should go elsewhere.

## Open Issues Summary (agent-self + actionable backlog)
- **#944** unmeasured spend in 3 phases (social largest) — filed, not started.
- **#937** price literals / drift alarm — option 1 landed 09:03; the *reach*
  half (other arms, the deepseek-v4-flash vs deepseek-flash 3.7× contradiction
  pinned by a near-miss guard) is still open.
- **#936** 50-verb residue of the multi-token guard — needs per-verb judgement,
  not another list entry.
- **#902** seventh trust door (project instruction files read into every prompt)
  — one slice verified 2026-09-22; remainder open.
- **#879** no composite safe mode; **#870** landed 2026-09-22 (verify + close?);
  **#869** `/cd` reloads trust but no other project config; **#858** skill-evolve
  gate's 4 measured defects; **#854** per-tool-call provenance; **#738**
  blind-round prediction mirror; **#742**, **#773**, **#779** older
  revert-labelled items.

## Research Findings
(to be filled in after recall + web research)
