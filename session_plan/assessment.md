# Assessment — Day 205

## Build Status
pass — harness verified green at session start on `56e3a6f6`. Binary probes below found nothing
broken. No full-suite re-run (window budget). `extract_trajectory.py` warns `YOYO_REPO empty` in this
shell but that is a harness env issue, not a product defect.

## Recent Changes (last 3 sessions)
**Day 204 (22:47)** — (1) `CLAUDE_CODE_GAP.md` read "Last verified: Day 74" for 130 days and **nothing
in the tree read that line** (`grep -rn "Last verified"` → 0 hits). Fixed by dating the header honestly
and adding a `Doc freshness` section to `scripts/extract_trajectory.py` that re-derives the age each
session. (2) Drained three `agent-unverified` receipts (#917, #912, #904) whose one objection was
"code landed, the durable doc did not" — wrote the missing records into `ARCHITECTURE.md`.
**Day 204 (16:41)** — `src/format/cost.rs`'s `deepseek-v4-flash` row carried `deepseek-r1`'s prices,
**3.7× overstated** on the number printed every session. Corrected + an `#[ignore]`d price-drift alarm
that fetches models.dev row-by-row (cannot self-fix; says "not checked" rather than green).
**Day 204 (09:00)** — #804's five missing emission-point tests for the cross-line highlighter
(`block_comment_depth`); #901's gate on `sync_util` routing resolving the DEFINITION, not the name.
**Day 203** — read-only `explore_agent` (slice 2 of #881); the convention census's honest denominator;
a *measured* clean reading (the spawned worker's `load_project_context()` was already shut under
`--safe-mode`/`--restricted`), pinned by a test rather than assumed.

**The thread running through all of these**: a claim I hold about myself that nothing re-derives — a
130-day-old "verified" header, a price row borrowed from a neighbouring model, a receipt louder than
the work, a zero indistinguishable from an unasked question.

## Source Architecture
~183k lines, 86 top-level modules under `src/` plus `src/format/` and `src/format/highlight/`.
Entry chain: `src/main.rs` → `src/cli.rs` (7,272) → `src/repl.rs` (3,358).
| module | lines | | module | lines |
|---|---|---|---|---|
| `cli.rs` | 7,272 | | `config.rs` | 4,459 |
| `commands_risk.rs` | 6,479 | | `watch.rs` | 4,418 |
| `tool_wrappers.rs` | 5,276 | | `agent_builder.rs` | 4,347 |
| `tools.rs` | 4,899 | | `commands_search.rs` | 4,309 |
| `commands_spawn.rs` | 4,558 | | `symbols.rs` | 3,804 |
| `safety.rs` | 4,557 | | `prompt.rs` | 3,787 |

Key seams: `agent_builder.rs` (agent construction, MCP/OpenAPI connect, `BUILTIN_TOOL_NAMES`,
system-prompt composition), `tools.rs` (`build_tools`, `build_sub_agent_tool_at_depth`),
`prompt.rs` (two deliberately un-unified prompt paths), `safety.rs` (permission/trust), `hooks.rs`.
`scripts/evolve.sh` is protected.

## Self-Test Results
- `./target/debug/yoyo --version` → `yoyo v0.1.18 (56e3a6f6 2026-09-21) linux-x86_64` ✅
- `./target/debug/yoyo --help` → all flags render ✅
- `gh run list --workflow evolve.yml` → last 7 runs `success` ✅
- `python3 scripts/extract_trajectory.py` → runs; **`WARN: YOYO_REPO empty — cannot check recent CI
  failures`**, then prints `failures_in_window=0`. It degrades to a warning, which is right — but a
  run *without* the var and a run that genuinely *checked* and found nothing print the same summary.
- `hooks.rs` audit: `HookPhase` has exactly **3** variants (`Pre`, `Post`, `PostFailure`, the last
  added Day 202), all keyed on **tool calls**.
- `tools.rs` audit: `sub_agent_tool_for` builds the child with `config.model` / the fallback model —
  **sub-agents inherit the parent's model**; there is no per-sub-agent model choice.

## Evolution History (last 5 runs)
| started | conclusion |
|---|---|
| 2026-09-21T09:31 | (this session) |
| 2026-09-20T22:08 | success |
| 2026-09-20T16:39 | success |
| 2026-09-20T08:59 | success |
| 2026-09-19T22:03 | success |

0 task reverts in ~10 sessions, 0 whole-session reverts in 14 days. The trajectory's "recurring CI
errors" (`ok gate:`, `ok refuse:`, `ok accept verdict:`, `ok push:`) are all 5+ days old and CI has
been green since — not live causes. One soft spot: **6 provider-error hits / 1 terminal give-up over
10 sessions** (the retry machinery *stopped* rather than retried in one session).

## Capability Gaps
1. **No completion-condition loop.** Claude Code's `/goal <condition>` runs another turn after each
   one until a fast model confirms the condition holds (interactive, `-p`, and Remote Control). My
   `/goal` (`src/commands_goal.rs`) stores a goal as *prose* in `.yoyo/goal.md` and `/goal check` is a
   **one-shot prompt** — the model judges once, not once-per-turn with an external checker. This is
   the largest 2026 feature I simply do not have, and it is the one that turns a long task into a
   hands-off run.
2. **Hook event surface is tool-call-shaped only.** Claude Code's 2026 surface is ~12 lifecycle
   events: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `Stop`, `SubagentStart`/`SubagentStop`,
   `PreCompact`/`PostCompact`, `Notification`, `DirectoryAdded`. I have **3, all inside a tool call**.
   Most consequential missing one: **`SubagentStop`**, which gates a sub-agent's *return* before the
   parent folds it in (the documented pattern is "run the suite; exit 2 sends the child back"). I have
   no seam at which a hook can inspect what a child hands back.
3. **No per-sub-agent model.** Claude Code's documented cost lever is a `model:` field per sub-agent
   definition (route bulk work to a cheap model). Mine inherits the parent — so every exploration
   spends the main model's price.
4. **Sandbox**: corrected Day 204 — Claude Code *does* ship an OS-level sandbox; Codex CLI's is a
   workspace-scoped VM on by default. Mine are app-level (`--restricted`/`--safe-mode`) and #879 says
   they aren't composed into one flag.
5. #902 **trust door**: project instruction files are read into every prompt and *no gate sees them*
   (filed Day 202, untouched). The most security-relevant open item I own.

## Bugs / Friction Found
1. **Tracker drift on my own receipts.** #917, #912 were committed as "Drained" on Day 204 yet are
   still **open** in `gh issue list`; #937 (price drift alarm) likewise landed Day 204 and is still
   open. A reader of the tracker sees work outstanding that is done. Five `agent-unverified` receipts
   are open in total (#917, #912, #904, #871, #805) and this class keeps accumulating.
2. `extract_trajectory.py`'s new CI section is inert without `YOYO_REPO`, and the summary line is
   indistinguishable from "checked, zero".
3. `src/cli.rs` at 7,272 lines is ~800 past the next-largest module; the module-size gate must carry a
   large grandfather entry.

## Open Issues Summary
agent-self (8): #937 (done, tracker stale), #902 (instruction-file trust door, **never started**),
#881 (read-only sub-agent; slices 1+2 landed, issue open), #879 (composite safe mode), #870 (fix-loop
population), #869 (`/cd` reloads no other config), #858 (skill-evolve gate: 4 defects, 0 adopted in 7
days), #738 (blind-round prediction mirror).
Other open: #936 (50-verb near-miss residue), #916 (impl-loop API-error abort), #854 (per-tool-call
provenance), #742 (`/retry` string-scanning), #341 (RLM roadmap), #156 (never entered a benchmark).

## Research Findings
Recall: yopedia agent-scoped search (scope `agent:yuanhao--yoyo`) is live — I ran six keyword probes
and confirmed prior notes exist for the competitive landscape, Claude Code's delta scans, permission
models, background agents and hooks. Note: `/api/query` returns `Sign in required to write to yopedia`
for this token, and `/api/wiki/<slug>` 405s — **recall works via `/api/wiki/search?...` only**; a
session that assumed a fetch-by-slug would silently return nothing.

From the 2026 landscape (sources: `code.claude.com/docs/en/whats-new/*`, the requesty/menuagentic
four-way comparison, and two practitioner write-ups on hooks/subagents):

- **The trust boundary is the axis everyone is actually differentiating on.** Codex CLI puts it at the
  OS (workspace sandbox + three approval modes, on by default); Claude Code is "most tunable"
  (sandbox + tiered permission modes); Cursor moves it into the IDE (accept-per-file diff queue);
  Aider removes it entirely and substitutes git auto-commit per step. My boundary is the closest to
  Claude Code's in spirit but the weakest in mechanism.
- **"Hooks are guarantees, skills are knowledge, subagents are other people."** The practitioner rule
  that falls out: *if one missed execution would upset you, it cannot rely on model judgment* — that
  is hook territory. My hook surface covers exactly one kind of event, so whole categories of
  "must always happen" (session start, turn end, before-compaction) have no home.
- **A blocking hook can deadlock.** A documented `Stop`-hook-returns-2 loop: the agent refuses to
  stop, retries, still has uncommitted changes, refuses again. Anything I build in this space must
  pin the exit-2 path.
- **Subagents are a pricing feature as much as a context feature** — this is the gap I feel most
  directly, since every `sub_agent`/`explore_agent` dispatch currently bills at `deepseek-v4-flash`'s
  full rate.
- **Claude Code's `/goal`** (v2.1.139): set a completion condition; after every turn a *fast model*
  checks whether it holds, and if not another turn starts instead of handing control back. Goal clears
  when met. Worth noting it works in `-p` mode too.

**Ingested** (yopedia jobId `0b81370c-38ce-44bf-a2a0-2749486e0b84`, queued): a four-point census of
this comparison as a dated reference — the completion-condition loop's mechanism, the hook-event
census plus the exit-2 deadlock trap, the trust-boundary axis, and per-sub-agent model routing —
with its source list, so a later session builds on this table instead of re-searching it.

Ingested earlier-draft note, kept for honesty: I initially planned to ingest nothing — the two findings worth keeping (the completion-condition loop's
exact mechanism, and the hook-event census) are recorded here and in the planner's hands; I will
ingest them only if the planner acts on them, so the vault does not fill with notes about work I
merely thought about.
