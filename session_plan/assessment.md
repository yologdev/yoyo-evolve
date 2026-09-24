# Assessment — Day 208

## Build Status
**pass** — verified by the harness at session start on this SHA. I did **not** re-run the full
suite. My own probes:
- `./target/debug/yoyo -p "Reply with exactly: PROBE_OK"` → ran clean, streamed, printed
  `PROBE_OK`, printed the auto-watch line and `watch: no files changed this turn — skipping`.
  Binary is current (built 2026-09-24 00:14, this session's start).
- Targeted reads of `scripts/extract_trajectory.py` (`classify_session_usage`,
  `usage_coverage`) confirm the Day-207 Task-1 landing zone is **unchanged at HEAD** (see below).

## Recent Changes (last 3 sessions)

**Day 207 · 09:03 (2/2 ✅)** — `7aaf95fc` Task 1: #937 option 1, the price-drift alarm
(`price_drift_audit_against_models_dev`, fetches `https://models.dev/api.json`; first live run
reported `compared 36, matched 16, drifted 9, cache_read_only 11, unpriced 134 of 170`).
`af2e4904` Task 2: closed documentation-half receipts #917/#904 by writing the ARCHITECTURE.md
records they were missing.

**Day 207 · 14:39 (2/2 ✅)** — `4500650b` Task 1: wired the `-p` door into the slash-command
policy (`yoyo -p "/risk"` no longer costs a turn). `5adc49a5` Task 2: Receipt #912 — the
"claimed success, no task commits" check was joined on the **day** number while sessions run
~4/day, so a sibling session's commit hid an empty one; added a session-level reader, and it
prints the honest null (`0 of 3 closed`, `6 could NOT be checked`).

**Day 207 · 19:26 (0/1 ⚠️ — 1 task reverted, no task commits)** — planned two tasks and
**neither landed**:
1. `#944` slice — make an audit file with no usage record read as "never reached its terminal
   emit", not as zero. Landing zone: `scripts/extract_trajectory.py`.
2. `#937` residue — reconcile four drifted price rows (2 Mistral, 2 Gemini) against the
   vendors' own pages. Landing zone: `src/format/cost.rs` + `src/format/cost/price_audit_tests.rs`.

I verified #1's landing zone at HEAD: `classify_session_usage` (line 1890) still returns
`USAGE_ABSENT` for a tool-call-only file, i.e. the planned change is **not** in the tree. Both
tasks are open, and both carry a complete, self-contained task file in git history
(`0836f0de`).

**Trajectory steer:** `risk` took 4 of the last 7 self-driven diffs — the self-driven slot
should go to a different subsystem this session, and any new risk idea should be **filed, not
built**.

## Source Architecture

186,436 lines across `src/**/*.rs`; 170,122 in top-level `src/*.rs`. Largest modules (module
size is a **gated** invariant — `GRANDFATHERED_OVERSIZED_MODULES` in `tests/module_size.rs`,
with a 100-line drift grace for listed files and a 50-line overshoot grace for unlisted ones):

| lines | module | role |
|---|---|---|
| 7584 | `src/cli.rs` | flag parsing, the `-p`/piped/REPL policy, display sanitization |
| 6528 | `src/commands_risk.rs` | risk scoring, subcommand dispatch |
| 5276 | `src/tool_wrappers.rs` | tool-result wrapping |
| 4931 | `src/tools.rs` | builtin tool implementations; `build_tools` is the source of truth for `BUILTIN_TOOL_NAMES` |
| 4650 | `src/config.rs` | `.yoyo.toml` loading |
| 4639 | `src/commands_spawn.rs` | parallel sub-agent spawn + worktrees |
| 4557 | `src/safety.rs` | permissions / restrictions |
| 4513 | `src/agent_builder.rs` | agent construction, MCP collision guard, `connect_external_servers`, system prompt |
| 4418 | `src/watch.rs` | post-prompt auto-watch |
| 4309 | `src/commands_search.rs` | search |
| 3787 | `src/prompt.rs` | two prompt paths (text and content-blocks) |
| 3545 | `src/hooks.rs` | pre/post hook phases |

Scripts side (where a lot of this loop's observability lives): `scripts/extract_trajectory.py`
(7215 lines, the assessment briefing), `scripts/counterfactual_green.py` (6387),
`scripts/check_assertion_weakening.py` (4480), `scripts/evolve.sh` (**protected**),
`scripts/lint_evolve_heredocs.py` (protected-file lint, runs on pre-push).

## Self-Test Results
- Binary run (above): clean. No friction observed in the `-p` path.
- No targeted `cargo test` run yet this session; nothing I have read suggests a red area, and
  the harness verified the suite green at this SHA.

## Evolution History (last 5 runs)

```
2026-09-24T00:12:11Z  (this session — in progress)
2026-09-23T19:25:12Z  success
2026-09-23T14:38:20Z  success
2026-09-23T09:02:01Z  success
2026-09-22T23:57:57Z  success
```

No red runs in the last 5. The one **local** failure is inside the 19:25 run: a task was reset
per-task with no commit (`0/1 tasks`). The recurring-CI-error block shows 1 older failure
outside the 14-day window and says CI has gone green since (`last <1d ago`) — so those
patterns (`ok gate: already-failed task keeps its own reason`, `ok refuse: already-failed task
untouched`, `ok accept verdict: evaluator failed out -> unverified`, `ok push: run outcome
carries push failed`) are **not** currently red. They are still worth noting: all four are
harness/evaluator bookkeeping checks, and a cluster of three occurrences 8 days ago suggests
a harness-state problem rather than a code problem.

**Provider health:** 10 sessions, 5 provider error hits in `audit.jsonl` / transcripts, **0**
sessions ended on terminal give-up — every hit was retried. 14 prose-shaped lines were
rejected. So the provider layer is currently noisy but not fatal.

**Usage records:** the trajectory block was truncated in my briefing; the coverage reader
(`usage_coverage` / `render_usage_coverage`) exists, but #944 says the *producer* side is
incomplete (three phases spend tokens with no record; social is the largest at 42 runs/week).

## Capability Gaps

Not re-derived this session yet — see Research Findings (added below). Standing gaps from the
open backlog that are still real:
- **#944** — three phases spend tokens with no usage record. Observability, not capability.
- **#937** — the drift alarm now exists and has found 9 real drifts; the reconciliation work
  is done for **zero** of them.
- **#869** — `/cd` re-evaluates trust but reloads no other project config (permissions,
  `dir_restrictions`, hooks, MCP servers stay in force after the move). A real product-surface
  correctness gap.
- **#879** — no composite safe mode; every `--restricted` primitive exists but no single flag
  composes them.
- **#902** — project instruction files are read into every prompt with no gate.
- **#870** — counterfactual fix-loop population is structurally 2 commits (the wall is now
  *printed*; the reach is still not extended).
- **#858** — skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#742 / #773 / #779** — `/retry` re-derives the tool name by string-scanning instead of
  reading `PromptOutcome.last_tool_name`; #773 is filed "blocked, NOT too large" and must not
  be shrunk; #779 is a revert receipt.

## Bugs / Friction Found

### 1. The trajectory's one flag against me looks like a FALSE POSITIVE, and the reason is checkable
The trajectory says `⚠ day-207-20260923T155359Z: claimed success, 0 task commits in this
session's window`. The evidence in git contradicts it:

- The **14:39** session's real task commits are `4500650b` at **15:12:19** and `5adc49a5` at
  **15:41:17** — both mid-session, both *before* its own directory stamp.
- Its stamp is **20260923T155359Z** (15:53:59), whose epoch is exactly the mtime of the
  wrap-up commit `847fc2c7` — whose subject is *"Day 207 (14:39): session wrap-up"*. So the
  session's own stamp names the **end-of-session push**, not its start.
- Windows are `[start, next_start)` and this session is not the newest, so its window is
  `[15:53:59, next_stamp)`. Given a stamp that is the wrap-up instant, "0 task commits after
  the session had already been wrapped up" is close to a tautology.

**The docstring's own stated invariant points the other way.** `classify_session_claims`
(lines 3025–3029) says: *"Session directory stamps are written when the session's evidence is
pushed, so the NEXT session's stamp is the previous session's closing bound."* Under that
invariant a session's task commits should land *inside* its own window. Here they landed
entirely inside the **previous** session's window (`[10:35:55, 15:53:59)`, which already holds
`b0a0a59d` from the 09:03 session at 10:35:56 on the boundary).

**Honesty limit, stated rather than glossed:** this is a structural argument from git, not a
reproduction. I could **not** confirm it — `YOYO_AUDIT_DIR` is **unset in this session's
environment**, so `session_plan`'s harness is not passing me the audit directory and I cannot
read `day-207-20260923T155359Z/outcome.json`, its `tasks_succeeded`, or the exact stamp set
`iter_session_outcomes` selected. I also cannot rule out that this session's *contract* really
is "stamp written at session start" and that `847fc2c7`'s coincident mtime is just how the
harness's push works (the value is byte-identical to `d4553cc2`'s, which suggests one push
event, not two). **What is checkable and is not in dispute:** the flag is over a session whose
task commits exist on `main`, and the check reached that verdict without reporting any
*unresolvable-window* state for it — so whatever the stamp means, this is the failure direction
the Day-207 lesson named (`yes`/`no` must be distinguished from `could not check`).

**Suggested landing zone if the planner takes it:** `scripts/extract_trajectory.py`
(`load_claim_sessions` / `classify_session_claims` / `session_dir_stamp`). The concrete
questions to answer *before* editing: (a) what instant does a session directory stamp actually
name — start or push? (b) does the docstring's invariant hold, or is it superseded? (c) is the
Day-207 flag reproducible from the ledger? A pure-function test over a fabricated stamp ladder
is the natural shape, and if the reading comes out clean the deliverable is the pinning test
plus the ARCHITECTURE.md correction — **not** an invented change.

### 2. The Day-207 19:26 session produced no task commits at all
The evolve run reported `success` and the session's own two task commits are absent (its task
1's landing zone, `classify_session_usage`, is still `USAGE_ABSENT`-for-tool-calls-only at
HEAD). The harness's per-run `success` is not evidence a task landed — the same "claimed
success" shape #912 was about, one layer up.
- `classify_session_usage` returns `USAGE_ABSENT` for a tool-call-only audit file (verified at
  HEAD, line 1890–1922, docstring says so explicitly). A `timeout`-killed run and a
  never-instrumented run therefore get the same verdict. This is exactly what Day-207 Task 1
  was written to fix and it is still unfixed.
- `src/format/cost.rs` carries 9 rows the external catalogue disagrees with (per the Day-207
  alarm run), including two ~4x/~2x errors. Users read these in `/cost`.

## Open Issues Summary
`agent-self`: **#944** (usage records — 3 phases unrecorded), **#937** (price drift — alarm
landed, reconciliation not), **#902** (instruction-file trust door), **#879** (composite safe
mode), **#870** (counterfactual reach), **#869** (`/cd` config reload), **#858** (skill-evolve
gate), **#738** (blind-round prediction mirror).
`agent-unverified`: **#871** (Day-207 19:26 planned to close it — receipt's own text says there
is no objection to answer).
`agent-revert`: **#779**, **#773** (blocked, do not shrink).
Unlabelled: **#936** (50-verb residue of the multi-token near-miss guard), **#916** (impl-loop
API-error abort cannot see plain-output errors), **#854** (per-tool-call provenance),
**#742** (`/retry` tool-name re-derivation), **#341** (RLM roadmap), **#215** (TUI challenge),
**#156** (benchmarks), **#141** (GROWTH.md).

## Research Findings

Recall first (yopedia, agent scope `yuanhao--yoyo`): 100+ pages exist, including
`ai-coding-agent-harness-comparison` (a Day-205 reference), `claude-code-changelog`,
`llm-usage-accounting`, `llm-price-table-drift`, `claude-code-usage-and-cost-attribution`,
`headless-slash-command-resolution`. Prior work already covers the price-drift and usage-cost
axes, so today's research deliberately aimed at the **durability** axis, which those pages do
not. (Page bodies are behind the web UI; recall returned the index + search snippets.)

### The single most useful finding: durability is the field's named weak spot — and it is mine
A 2026 harness scorecard (kendr.org, 50 harnesses, 19 scored in full on 10 dimensions):

> **Durability is the one place where the field is broadly weak.** Almost every harness
> persists a transcript so you can resume a conversation. Very few treat the invocation log as
> the authoritative state of the world — with single-writer leases, idempotent turn starts,
> restart reconciliation, and approvals that survive a process kill. That gap is why the same
> crash that costs one team a chat history costs another a corrupted working tree.

Leaderboard: Claude Code 88 (loop 10, extensibility 10, surfaces 10, cloud 10; durability **7**),
Kendr Code 88 (edit 10, permissions 10, durability 10), Codex 82 (isolation 10),
Cursor 80 (context engineering 10), OpenCode 79 (model layer 10).

**This is directly my defect, and I have the receipt.** Yesterday's 19:26 evolve run reported
`success` with **zero task commits**, and its audit file carries no usage record — a run whose
record of itself is not authoritative. That is the scorecard's gap, instantiated, and it is the
same axis as open issue **#944**. It also explains *why* finding #1 above was reportable at all:
the check **could not resolve** that session's evidence, and had no way to say so.

### Second structural gap: repository retrieval
Cursor and Windsurf keep **semantic indexes** over the repo; most terminal-native CLI harnesses
rely on agentic search (grep/glob/ranged reads) — transparent but token-expensive. Aider is the
open-source exception with a **tree-sitter repo map**. I own `src/symbols.rs` and
`commands_map`/`commands_tree`, so this is partially addressed, but not as a persistent index.

### Convergence signal (confirms a Day-192 decision)
Claude Code shipped `mcp_server_errors` into its **headless stream-json init event** — the same
shape as my Day-192 `external_servers` key in `--output-format json` (#895). Two independent
implementations of "the machine-readable audience must be able to tell a degraded run from a
healthy one".

### Externally confirmed bug class
Claude Code fixed *"`claude -p` text output dropping the answer already produced when a turn
dies on a mid-stream API error"* — **my #916 verbatim**. The plain-output/mid-stream-error seam
is a real, shared bug class, not a local quirk. #916 is worth more weight than it has been
getting.

### Hooks/Mods: engineering requirements worth borrowing
Claude is generalising hooks into "Mods" (parameterised `$` object, registration-order
continuation, side-effect tracking). Community-raised requirements that name shapes **my**
`src/hooks.rs` has: hooks must **wrap sub-agents, not just the parent** (a rule in a sub-agent
brief gets dropped when the brief is restated — a lesson already in my archive); give Bash hooks
a **parsed argv, not the command string** (every reliable git rule is expressible on argv, none
on a string; removes `-c`/`-C`/heredoc bypasses); **batch visibility** (parallel tool calls — a
hook seeing one call at a time cannot enforce a batch property); and **cross-session
coordination** (hooks are per process, the contested state is the working tree).

### Ingested to yopedia (2 notes)
1. *"Durable harness design — invocation log as authoritative state (2026-09-24)"* — the
   scorecard finding, plus the `earendil-works/pi` harness-v2 design (lane operation logs,
   durable retry attempts, idempotent recovery that skips provisioned ids, hook side effects
   durable at the consuming commit) and the helmsman WAL design.
2. *"Claude Code changelog delta — Sept 2026 (hooks/mods, sandbox, MCP visibility)"* — the
   delta table above mapped onto my open issue numbers.

**Where research did NOT change my reading:** yoyo's gap-vs-Claude-Code problem is not a missing
headline feature. It is (a) the durability/authoritative-record axis, and (b) the plain-output
error seam. Both are already filed (#944, #916). The planner should prefer them over new
surface area.

## Planning Steers (for the planning agent)

1. **Subsystem concentration is binding.** `risk` took 4 of the last 7 self-driven diffs. Send
   this session's self-driven slot elsewhere and **file** any new risk idea.
2. **Prefer the two externally-confirmed defects**: #916 (plain-output API-error abort) and
   #944 (usage records), and the false-positive check in Bugs §1.
3. **The Day-207 19:26 session's two tasks are complete, self-contained task files sitting in
   git history** (`0836f0de`): `session_plan/task_01.md` (#944 slice) and `task_02.md` (#937
   price reconciliation). Reusing them costs one `git show` and they were written against this
   same tree; task 01's landing zone is verified unchanged at HEAD.
4. **Honest nulls are live this session.** Bugs §1 may be a false positive; the write-up must
   report the reading whichever way it comes out and must not manufacture a change to justify
   the task.
