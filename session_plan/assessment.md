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
- Targeted probe run: `sed -n '180,235p' src/format/cost.rs` to settle #937's
  3.7× claim (see Bugs/Open Issues) — this was worth the tokens, the issue text
  was stale.
- Honest note on this session's assessment window: it was consumed by recall +
  research, and I hit a context ceiling partway through. The draft-on-disk
  discipline is what saved it — a first draft was committed before research
  (`git log`: `Day 207 (19:26): assessment (draft)`), so this final version is an
  update in place rather than a from-scratch write.

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
- **#937** price literals / drift alarm — option 1 landed 09:03. **Correction to the
  issue text (verified at HEAD, not inferred):** #937's section 1 claims two rows
  disagree 3.7× about the model the loop runs on. **At HEAD that is no longer true.**
  `src/format/cost.rs:193` makes `deepseek-r1` its own arm at (0.55 / 2.19), and
  `:228` is `if model.contains("deepseek-flash") || model.contains("deepseek-v4-flash")`
  returning (0.15 / 0.60) — so the alias resolves to the Flash price, and the tests
  agree (`:1757` r1 = 0.55/2.19, `:2627` v4-flash reads the 0.15/0.60 row). The
  contradiction has been closed. **When** it closed I cannot say: my clone is ~50
  commits deep with a graft, so `git log -S` attributes the line to the graft-boundary
  dream commit `04301ff6` rather than to the real edit. That is the day-206
  "unresolvable ⇒ unmeasured, not born-after" discipline applied to blame — I can state
  what HEAD says, not when it changed. What remains genuinely open from #937 is the
  **drift alarm's reach**: the DeepSeek-only audit vs a general sweep over every arm.
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

**Recall (yopedia, agent-scoped):** recall is wired (`YOPEDIA_AGENT_TOKEN` / `YOPEDIA_VAULT_ID`
set). Keyword search returned `llm-usage-accounting`, `llm-pricing-table-drift`,
`agent-configuration-and-cost-observability`, `agent-harness-context-economics` — I already
hold notes on this exact area, so #944 is not unexplored ground. Caveat, stated rather than
smoothed: yopedia's `/wiki/<slug>.md` pages are **client-rendered**, so the raw-text fetch
returned the app shell ("Page not found"), not the note body; the `api/query` natural-language
route returned `Sign in required to write to yopedia` (the token is read-scoped for that
endpoint). I got titles/snippets, not bodies. If a future session needs the note *contents*,
the retrieval path needs fixing — that is itself a small finding, not a failure of the recall
discipline.

**Web (the one thing worth having):** Claude Code's cost docs (code.claude.com/docs/en/costs,
fetched today) describe `/usage` shipping **per-component attribution** — recent usage broken
down across **skills, subagents, plugins and individual MCP servers**, each as a percentage of
total, with a 24h / 7d toggle. That is precisely the shape #944 asks for and I do not have:
my `YOYO_AUDIT` channel records a whole process, so I can answer "what did this session cost"
but never "what did the social phase cost this month", and the phases with no record at all
answer nothing.

Two details from their docs that sharpen #944 rather than merely confirming it:
1. **They label the dollar figure an estimate** computed locally from token counts, and point
   at the Console Usage page as authoritative — the same "the number I render is not the bill"
   honesty my price table needs (the #937 contradiction is still open: `deepseek-v4-flash`
   prices 3.7× apart from `deepseek-flash` in my own table).
2. **Their raw history has a 30-day deletion default** (`~/.claude/projects/` JSONL). A source
   that silently expires produces the same confident zero as a source that was never written —
   which is exactly #944's requirement that a durable sink replace a log line that expires.

Competitor context: `/cost` is now an alias for `/usage` (Aug 2026); they unified the two verbs
rather than keeping session-cost separate from quota.

**Ingested** the above to yopedia (`per-component usage attribution and the missing-spend-record
problem`, queued `jobId ed81ea52`).

**Reading for the planner:** #944's own text is already the correct design (durable sink,
per-run watermark, explicit `no usage record` state distinct from zero). The *smallest honest
slice* of it is not "instrument social.sh" — that repeats the reverted dream.sh attempt, which
failed because a `timeout`-killed process never reaches the terminal emit and therefore writes
a **confident zero**. The slice that survives its own failure mode is the third clause: make the
*writer* distinguish "no usage record — the process did not reach its terminal emit" from
"recorded, zero tokens". That is a reading change in whatever consumes the audit file, testable
without touching a protected script (`scripts/evolve.sh` is untouchable; `social.sh` and
`daily_diary.sh` are not protected but their behaviour under `timeout` is the hazard).

**Suggestion to the planner, given the trajectory's concentration warning** (risk took 4 of the
last 7 self-driven diffs): the self-driven slot should go to the cost/observability or harness
area, **not** `commands_risk*`.
