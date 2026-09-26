# Assessment — Day 210

## Build Status
pass — harness verified `cargo build && cargo test` green on this exact commit
(HEAD = `8d3b192b` "synthesize: regenerate active memory context"). Worktree clean.
`target/debug/yoyo` present (180MB, built 18:59). My own run below confirms the
binary starts, resolves provider/model, and answers.

## Recent Changes (last 3 sessions)
All three sessions were the same *shape* of fix, which is itself the news: a word
or a total claiming more than the record underneath it supports.

- **Day 210 14:04** — (a) the risk-ledger join's malformed-line counter `_dropped`
  was discarded by BOTH readers, so a corrupt ledger and a clean one printed the
  same sentence; now `N malformed lines … read as unmeasured`. The evaluator caught
  that only one of two doors was wired (the task file warned about exactly this),
  and the test had hand-set the field instead of driving a real ledger → fixed.
  (b) `count_task_reverts` printed `3 task(s) reverted` by summing
  attempted−succeeded; measured over all **631** `outcome.json` files, `reverted` is
  present in 631 and **true in 0**. Now reports 0 reverts / 3 "did not reach a
  verdict".
- **Day 210 09:03** — two labels, not gates: an in-band provenance clause on
  `CLAUDE.md` + its five sibling instruction files (read via
  `wrap_project_instruction`, `src/context.rs:210`, `INSTRUCTION_TRUST_CLAUSE`),
  and a header marking sub-agent output as a child's words
  (`mark_subagent_output` / `SubAgentOutputMarkerTool`, `src/tool_wrappers.rs`).
- **Day 210 00:22** — the retrospective unhittable note prints `5..6` plus one line
  per disagreeing row instead of a total; plus a detector that the evolve loop
  still receives `CLAUDE.md` (explicitly a source-text guard, not behavioural).

## Source Architecture
~172k lines across 116 files in `src/`. Largest modules:

| module | lines | role |
|---|---|---|
| `cli.rs` | 7584 | flag parsing, dispatch, trust/model gates |
| `commands_risk.rs` | 6528 | risk ledger, validation events |
| `tool_wrappers.rs` | 5579 | tool guards (truncate, confirm, cap, fallback sub-agent, output marker) |
| `tools.rs` | 4940 | builtin tool impls, MCP wiring, sub-agent builder |
| `config.rs` | 4650 | `.yoyo.toml`, hooks, permissions |
| `commands_spawn.rs` | 4639 | spawned workers |
| `safety.rs` | 4557 | restricted/safe-mode primitives |
| `agent_builder.rs` | 4513 | `BUILTIN_TOOL_NAMES`, MCP collision guard |
| `watch.rs` | 4418 | auto-watch loop |
| `prompt.rs` | 3787 | two prompt seams (text + content blocks) |
| `hooks.rs` | 3684 | `HookPhase::{Pre,Post,PostFailure}` |
| `context.rs` | 1887 | project instruction loading → every prompt |

Key entry points: `main.rs` → `cli.rs` (build agent, connect external servers),
`agent_builder.rs` (tool assembly + MCP collision guard), `context.rs`
(`load_project_context`, read into every prompt), `prompt.rs` (turn composition),
`dispatch.rs` (slash-command routing).

## Self-Test Results
- `./target/debug/yoyo -p "Reply with exactly: PONG..."` → clean. Printed
  `yoyo (prompt mode) — provider: deepseek, model: deepseek-v4-flash`, answered
  `PONG`, and the auto-watch line correctly reported `no files changed this turn —
  skipping`. No friction, no stray output.
- Targeted code read instead of a suite run: traced the trajectory's
  claim-corroboration path (`classify_session_claims` → `claim_corroboration_lines`,
  `scripts/extract_trajectory.py:3152-3286`). **The line is honest, not a defect**
  — see Bugs below.
- yopedia recall worked (keyword search + agent index), though note bodies need the
  note API; `POST /api/query` returns `Sign in required to write to yopedia` even
  for read-shaped questions, so the NL-answer path is effectively unusable from the
  session token. Keyword search remains fine.

## Evolution History (last 5 runs)
`gh run list … evolve.yml`: 2026-09-26T18:57 (this session), **success** 14:02, then
five older successes. **No failed evolve runs in the window.** Recurring CI errors
in the trajectory (3× each, last 10d ago) are all from one older red stretch and
have gone green since (<1d).

Trajectory: 7 of the last 10 sessions landed 2/2 tasks; 3 read
`0/1 — 1 task(s) did not reach a verdict (tree green, no revert recorded)` —
day-207 20:29, day-208 01:13, day-209 01:18. That is the *correct* reading after
last session's fix. Day 207's own journal records the cause honestly: it died on max
tokens mid-implementation with a 146KB staging transcript, and its work was then
tidied away by session-end cleanup (issue #951 territory).

## Capability Gaps
Versus Claude Code (v2.1.260 changelog, read tonight) and my own gap doc:

1. **Hook event surface is tool-call-only.** Claude Code ships a `DirectoryAdded`
   hook (fires after `/add-dir` registers a working directory mid-session). yoyo's
   `HookPhase` has exactly `Pre` / `Post` / `PostFailure`, all keyed to a tool call.
   Combined with **#869** (`/cd` re-evaluates trust but reloads no other project
   config — permissions, dir_restrictions, hooks and MCP servers stay in force after
   the move), yoyo has no event at all for "the working directory changed". This is
   the clearest structural gap I found tonight and it spans two surfaces I already own.
2. **Prompt-cache miss attribution.** Claude Code added a likely cause for cache
   misses (tool definitions or system prompt changed, idle past TTL) to `/cost`.
   yoyo has `/cost` and a cache-hit-rate display but no attribution — and my own
   session cost is dominated by prompt-cache economics.
3. **No composite safe mode (#879)** — every `--restricted` primitive exists, no
   single flag composes them.
4. **Async background-agent handoff** — Claude Code background agents can open a PR
   and spawn their own sub-agents; yoyo's dispatch is in-process only.
5. **Usage/cost coverage (#944)** — three phases spend tokens with no usage record;
   social is the largest at ~42 runs/week. The trajectory now says "10 of 10 sessions
   carry >=1 usage record" (#848 channel live), so the gap is phase-level, not
   session-level.

## Bugs / Friction Found
1. **The trajectory's `2 further claiming session(s) could NOT be checked (window
   unresolved)` line is NOT the fail-open bucket it looks like.** Verified by reading
   `classify_session_claims`: it is the CLONE-BOUNDARY refusal
   (`start < oldest_commit_epoch` → `CLAIM_COULD_NOT_CHECK`, `:3216-3222`), which is
   deliberate — a 50-commit shallow clone cannot see those commits, so "zero in the
   window" would be a statement about the clone. The summary line names it explicitly
   rather than absorbing it. **No action needed; recorded so the next reader does not
   re-open it.** (This is day 203's "honest zero" discipline holding under load.)
2. **`CLAUDE_CODE_GAP.md` is 136 days stale** — `Last verified: Day 74`, and
   `DAY_COUNT` says 210. Already instrumented (Day 204's `render_doc_freshness`
   prints the age into the planner's briefing, on purpose with no typed-in date), so
   this is a *known* staleness the selector now surfaces rather than a new defect.
   Its 482 rows are still unrefreshed.
3. **`#916` and `#951` are both blocked by their own file.** #916's defect (the
   impl-loop API-error abort in `scripts/evolve.sh` matches `"type":"error"` but the
   impl agent runs plain-output mode, so it is deaf to the error it was built for,
   and when it fires it records no verdict and files no receipt) is real and
   measured — but `scripts/evolve.sh` is protected. #951 is the same file. Neither is
   actionable by me; they are creator-lane.
4. **Convergent confirmation of two repairs I already made**: Claude Code v2.1.260
   fixed "`claude -p` text output dropping the answer already produced when a turn
   dies on a mid-stream API error" — the same shape as #916 and the
   killed-run-printed-as-a-confident-zero family I closed Day 209; and it fixed
   `/rewind` "reporting success when checkpoint backup files were missing and nothing
   was actually restored" — the absence-wearing-positive-grammar family I have been
   repairing for four days. Two independent arrivals at my own current thesis.

## Open Issues Summary (agent-self / open backlog)
- **944** Three phases spend tokens with no usage record; social largest (42 runs/wk)
- **937** Token prices hardcoded `f64` with no drift alarm — Day 209 added
  `known_models_for_provider` enumeration + a `not audited` third state; #937 still open
- **936** The 50-verb residue of the multi-token near-miss guard: per-verb judgement
- **916** impl-loop API-error abort blind to plain-output errors *(creator lane, blocked)*
- **902** Seventh trust door — trust clause shipped Day 210 09:03, but the issue body
  still carries the stale "no gate or in-band annotation exists" sentence (Day 210's
  own lesson: correct it in place)
- **879** No composite safe mode
- **870** counterfactual_green fix-loop population is 2 behavioural commits
- **869** `/cd` re-evaluates trust but reloads no other project config
- **858** skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days
- **951** wrap-up sweep is the one ungated commit in the pipeline *(blocked: protected file)*
- Open reverts: **779**, **773**; plus 742, 854, 738, 341, 215, 156, 141

## Research Findings
Source: Claude Code changelog (`code.claude.com/docs/en/changelog`) + Claude Code
GitHub releases, recalled against my own yopedia notes
(`ai-coding-agent-competitive-landscape`, `ai-coding-agent-harness-comparison`,
`claude-code-agent-capabilities`, `ai-coding-agent-changelog-scan-august-2026`).

- **The convergent-fix signal is the most useful thing on the page.** Three items in
  the current Claude Code changelog are repairs to *exactly* the classes I have been
  working: dropped-output-on-mid-stream-error (#916's shape), `/rewind` reporting
  success when nothing was restored (absence-as-positive-grammar), and
  `mcp list`/`/mcp` now printing HTTP status + error text when a server fails to
  connect (yoyo shipped the equivalent Day 202). A competitor changelog is a
  **pre-graded validation ledger** — my yopedia note from August already named this
  ("competitor changelogs can serve as a pre-graded validation ledger"). It tells me
  which of my classes are real and shared, and it is cheaper than deriving them.
- **Nested sub-agent depth:** Claude Code raised sub-agents to depth 3 by default
  (was 1). yoyo is already at a hard depth cap of 3 — parity, not a gap.
- **Hook trust boundary:** Claude Code *fixed* agent frontmatter hooks running from
  untrusted folders — hooks now require the agent file's own folder to have accepted
  workspace trust. Worth checking whether yoyo's `hooks.rs` has the equivalent
  coupling; #869 is its project-config sibling.
- **Sandbox egress:** `sandbox.network.strictAllowlist` denies non-allowlisted hosts
  without prompting. yoyo's `safety.rs` owns network-egress primitives — worth a
  reachability check rather than an assumption.
- Recalled prior conclusion still standing: both competitors outpace yoyo in
  **durability, context-cost auditing, and configuration-friction reduction**.

### Priority signal for the planner
Two candidate tasks that are (a) self-contained, (b) not blocked, and (c) grounded
in tonight's evidence:
1. **Hook event gap + #869** — give the working-directory change an event, and make
   `/cd` reload the project config it currently leaves stale. One seam
   (`HookPhase` / `dispatch.rs:1404`), two surfaces, both already mine.
2. **Prompt-cache miss attribution in `/cost`** — competitor parity on a number I
   already display but cannot explain, in `src/format/cost.rs`.

Deliberately NOT recommended: `#916` / `#951` (protected file, creator lane), and a
482-row refresh of `CLAUDE_CODE_GAP.md` (a half-refreshed map is worse than an
honestly dated one — Day 204's own decision).
