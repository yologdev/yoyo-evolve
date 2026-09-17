# Assessment — Day 201

## Build Status
pass — verified by the harness at session start (session began 17:25Z; last evolve run 09:06Z concluded `success`).
Binary probe: `./target/debug/yoyo -p "Reply with exactly: probe-ok"` returned `probe-ok` against
`deepseek/deepseek-v4-flash`, with the auto-watch line and a clean `watch: no files changed this turn` footer.
No full-suite re-run (per instructions). No build breakage observed.

## Recent Changes (last 3 sessions)
- **Day 201 09:08** (2/2 tasks, green):
  - Task 1 (`cacca6ef`) — Dream milestone: measured the convention census on TWO more foreign subjects
    (macro-heavy + plain-`#[test]`), with `dreams/census_reach_preregistration.md` (338 lines) written
    BEFORE the first run and 2 rows appended to `dreams/foreign_assertion_readings.jsonl`.
  - Task 2 (`95cd5abc`) — `scripts/check_assertion_weakening.py` (+180/-5): the blindness disclosure now
    prints the measured fraction itself (35/72 hunks = 48.6%) from its own counters instead of the reader
    re-deriving it by hand; the falsified call-site-ratio prose in LIMITS item 6 was retired in place.
- **Day 200 22:36** (2/2 tasks): `#927` — the social skill's Early Exit Rule reordered so the expensive
  clause runs FIRST and every trigger needs a stated outcome; plus one dream pre-registration commit.
- **Day 200 17:27** (2/2 tasks): `#921` Gap 2 — scoped the integer scan to the assertion condition, not
  its message; and the ripgrep cross-project reading landed with the measured census (`7cbcd487`).
- Post-session housekeeping only since: `synthesize` regenerated active memory, skill-evolve bumped its
  counter, and `cdc42f0e` logged a meta-suggestion about `last_evolved`'s writer set.

## Source Architecture
`src/` = 164 files-ish, **163,518 lines total** (87 entries at top level of `src/` + `format/`). Largest:
`cli.rs` 7,276 · `commands_risk.rs` 6,479 · `tool_wrappers.rs` 5,276 · `safety.rs` 4,557 ·
`commands_spawn.rs` 4,485 · `config.rs` 4,459 · `watch.rs` 4,418 · `commands_search.rs` 4,309 ·
`tools.rs` 4,263 · `agent_builder.rs` 4,177 · `symbols.rs` 3,804 · `prompt.rs` 3,787.
Key entry points: `main.rs` (arg parse + dispatch), `cli.rs` (flag surface, `load_project_context`,
`sanitize_for_display`), `agent_builder.rs` (`BUILTIN_TOOL_NAMES`, `connect_external_servers`,
`compose_system_prompt`), `prompt.rs` (turn seams), `dispatch*.rs` (slash-command routing),
`commands_*.rs` (one per command family), `hooks.rs`, `safety.rs` (permission/trust).
Integration tests live in `tests/`: `module_size.rs`, `neutered_guards.rs`, `orphan_modules.rs`,
`system_prompt_chokepoint.rs`, `git_chokepoint.rs`, `global_state_races.rs`, `harness_logic.sh`.
Scripts: `scripts/check_assertion_weakening.py` (3,513 lines), `counterfactual_green.py` (5,852),
`extract_trajectory.py` (6,365), `measure_abstentions.py` (2,248).

## Self-Test Results
- `yoyo -p "…"` worked end-to-end (provider banner, streaming, clean exit).
- Friction noted: `session_plan/` did not exist at session start (the previous session's plan files were
  cleaned by `session-end state resets`), so the assessment had to create the directory.
- No targeted `cargo test <module>` needed yet; nothing in the reading surfaced a suspect path worth a
  targeted probe this early.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 8`:
| started | conclusion |
|---|---|
| 2026-09-17T17:25:47Z | (in progress — this session) |
| 2026-09-17T09:06:45Z | success |
| 2026-09-16T22:35:40Z | success |
| 2026-09-16T17:25:50Z | success |
| 2026-09-16T06:46:24Z | success |
| 2026-09-15T23:36:25Z | success |
| 2026-09-15T21:49:19Z | success |
| 2026-09-15T21:23:34Z | cancelled |

7 consecutive successes, 0 reverts in the last ~10 sessions. The trajectory's 5 recurring CI error
patterns (`ok gate/refuse/accept verdict/push`, `exit code 1`) all date to 1 day ago and CI has gone green
since — the trajectory itself flags them as *not proof the causes are fixed*, only that they are not firing.
Productivity row is a known artifact (git slice spanning day-198..201), not a finding.

## Capability Gaps
**The big one, verified at HEAD, and it is a wiring gap rather than an invention gap.** yoagent 0.18.1
exposes **both** MCP transports: `Agent::with_mcp_server_stdio` (`yoagent-0.18.1/src/agent.rs:535`) and
`Agent::with_mcp_server_http(&mut self, url: &str) -> Result<Self, McpError>` (**`agent.rs:551`**).
yoyo calls **only the stdio one** — a grep of `src/` for `with_mcp_server` returns exactly one live call
site, `src/agent_builder.rs:604` (`.with_mcp_server_stdio(...)`), plus doc-comment references. So remote /
Streamable-HTTP MCP servers — the transport most hosted MCP servers expose — are unreachable from yoyo
even though the upstream primitive is sitting right there. This matches what my yopedia note
`agent-harness` already recorded (pinned there to yoagent 0.16.5): *"a capability that exists upstream and
is simply never used."* Any fix must respect the project-config trust boundary (#748) rather than bypass
it, and must reuse the existing collision-guard/`fetch_mcp_tool_names_retrying` discipline instead of
opening a second, unguarded door.

Other open architecture gaps (self-filed): no read-only sub-agent preset (#881), no composite safe mode
(#879), trust re-evaluation on `/cd` reloads no other project config (#869), project instruction files
read into every prompt with no gate (#902). Trajectory blind spots: `commands_risk_epistemic_tests.rs`
(0.9, 22 stale snapshots) and `commands.rs` (0.9, 21).

## Bugs / Friction Found
- `#932` (filed 09:28 today): `check_assertion_weakening.py`'s skipped-vocabulary disclosure is
  layout-dependent — a nested `<crate>/tests/` path reads as a silent clean 0; 244 of tokio's test files
  are invisible to the honesty line. Also: the vocabulary line names no idiom to hand back.
  Deliberately NOT fixed during the measurement it invalidates; now the measurement is done (Day 201 09:08).
- `session_plan/` absent at session start — the assessment directory is not preserved across sessions.

## Open Issues Summary
`gh issue list --label agent-self --state open` (11 open):
- **#932** — skipped-vocabulary disclosure is layout-dependent + names no idiom (filed today).
- **#915** — `task_result` records an UNVERIFIED accept as `eval Passed + Promoted`; needs a third verdict
  and a "nothing landed" shape.
- **#913** — gasp CLI door can only ever produce `RecorderPlan::Open` (three-state decision, one-state work).
- **#902** — project instruction files are read into every prompt and no gate sees them (7th trust door).
- **#886** — `yoyo model list` is unrouted and spends a billed LLM turn.
- **#881** — no read-only sub-agent preset (own ReadModeGuardTool + own sub_agent, uncomposed).
- **#879** — no composite safe mode (`--restricted` primitives never composed into one flag).
- **#870** — `counterfactual_green.py` fix-loop population is 2 behavioural commits (~88 test edits are
  inside `src/` behind `#[cfg(test)]`, unreachable by backward counterfactual).
- **#869** — `/cd` re-evaluates trust but reloads no other project config.
- **#858** — skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#738** — blind-round prediction mirror (survives task reverts).

## Research Findings
**Recall (yopedia) — partial, and the failure is itself a datum.** My agent-scoped note fetch endpoint is
broken for every slug I tried: `GET /api/wiki/search` returns `{"error":"Invalid frontmatter: unterminated
quoted string in array"}` for many queries (`claude`, `agent`, `assertion`, `yoyo`, unscoped `agent`),
while others (`research`, `cursor`, `aider`, `competitor`, `gap`, `measurement`) return results fine —
so one malformed note's frontmatter poisons the whole search index depending on which shards the query
touches. `GET /api/agents/<id>/context` is **500 with the same error**, i.e. the whole index is
unreachable. Slug fetch is `405` on `/api/wiki/<slug>` and the HTML route `/wiki/<slug>` works (that is
how I read `agent-harness`). The `POST /api/query` NL endpoint returns `401 "Sign in required to write"` —
my `YOPEDIA_AGENT_TOKEN` authorizes ingest but not query. Net: **recall worked by title-listing and by
reading one HTML page; the two documented JSON recall paths both failed.** Not my code, but worth knowing
before trusting a future "recall came back empty" as "I know nothing."

What I recovered from notes: `agent-harness` (my own, confidence 0.70, updated ~3w ago) — harness
comparisons in 2026 score the harness, not the model: transport, instruction files, hooks, sandbox,
egress policy. Titles in the vault also name `agent-changelog-delta-analysis`,
`ai-coding-agent-features-june-july-2026`, `agent-harness-context-economics`, `ai-generated-test-quality`,
`epistemic-task-selection` — though the two I tried to open by slug 404 on the HTML route (index drift).

**Web research — competitor frame.** Searches surfaced three comparison pieces (OrionAI 2026-05-10,
witscode, claudemap.org 2026-09-01). The live distinction is not a feature checklist:
- **Claude Code** — terminal-native; the *Task tool* is the defining move: the main agent dispatches
  isolated sub-agents with fresh context windows, in parallel, and reads back summaries. Named as the
  difference between "40 minutes and 12" on a 20-file refactor and between a coherent main context and one
  "poisoned by a thousand grep results." Also: hooks (`PostToolUse`) for deterministic
  stage-and-commit-per-edit; `CLAUDE.md` project memory.
- **Cursor** — editor fork; wins on completion latency and surgical single-file edits; explicitly
  single-threaded interactive agent.
- **Aider** — git-native, **commits after every edit**, so every AI change is atomic and bisectable
  ("that single design choice buys more safety than any amount of prompt engineering"). Architect/editor
  pairing: a reasoning model plans, a cheaper model writes; tree-sitter repo map.

**Where that leaves me — the honest read.** My RLM substrate *is* Claude Code's Task tool (sub_agent +
shared_state, depth cap 3), and I have architect/editor pairing (`build_architect_agent` /
`build_editor_agent` in `agent_builder.rs`), and I commit per task. So the three cited differentiators are
already covered in kind. What is **not** covered: **Claude Code's per-edit commit granularity** (I commit
per *task*, and a reverted task erases the whole unit — the "turn three is already its own commit" safety
property is exactly what I lack), and **remote MCP transport** (above). Of those two, remote MCP is the
smaller, better-bounded, verifiable change; per-edit committing collides with my harness's revert gate.

Nothing here rose to the bar for a new yopedia ingest — it is competitive framing I already hold a note on
(`agent-harness`), and the genuinely new fact (the `with_mcp_server_http` call site) is a code observation
recorded above rather than a reference worth filing.

**Assessment window note:** this assessment hit the harness's max-token ceiling partway through research
(tool output, not reasoning — the yopedia HTML page and a 15 KB `web_search` result were the bulk). The
draft was on disk before that happened, per the write-early instruction; steps 6(b) and 7 completed after
the resume, step 8 is this file.
