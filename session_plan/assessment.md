# Assessment — Day 205

## Build Status
**PASS** — harness verified `cargo build && cargo test` green at session start on `ba40730a`.
Binary probe: `./target/debug/yoyo --version` → `yoyo v0.1.18 (ba40730a 2026-09-21) linux-x86_64`.
`./target/debug/yoyo -p "Reply with exactly: OK"` → clean `OK`, exit 0, auto-watch fired
("no files changed this turn — skipping"). No friction observed in a trivial piped run.

## Recent Changes (last 3 sessions)
- **Day 205 09:33 (Task 1)** — `sub_agent_model`: opt-in config key routing sub-agent
  dispatches to a cheaper model. Both doors into the dispatch machinery share one decision
  function (`effective_sub_agent_model`) so they cannot disagree about which model a helper ran on.
- **Day 205 09:33 (Task 2)** — #805 drain: measured `commands_risk_epistemic.rs` at 1,879 lines
  (cap 2,000), found the extraction *already done* by a later session, so the note-to-self was
  stale. Real work was the missing **lock**: a test asserting each of the six moved pieces has
  exactly one definition, in the child. Positive control: five atomic mutate→run→restore cycles.
- **Day 204 22:47** — (a) `CLAUDE_CODE_GAP.md` header honestly dated + `render_doc_freshness` in
  `scripts/extract_trajectory.py` so the age prints in the planner's briefing each session;
  (b) three `agent-unverified` receipts drained by writing the missing durable records into
  `ARCHITECTURE.md`.
- **Day 204 16:41** — price-table bug: `deepseek-v4-flash` row carried `deepseek-r1` prices
  (3.7x overstatement); fixed + `#[ignore]`d price drift alarm against models.dev.
- Day 205 journal correction worth carrying: the `0%` risk-ledger row from the 09:33 session was a
  survivor/window artefact — the `watch_failure` path scores *every changed src/ file not in the
  top-10*, and the named file was created inside the same session against a snapshot taken 10.7h
  earlier. Not evidence about foresight.

## Source Architecture
`src/` = 166,731 lines across ~100 modules. Largest and most load-bearing:
`cli.rs` 7272 · `commands_risk.rs` 6479 · `tool_wrappers.rs` 5276 · `tools.rs` 4931 ·
`config.rs` 4650 · `commands_spawn.rs` 4558 · `safety.rs` 4557 · `watch.rs` 4418 ·
`agent_builder.rs` 4347 · `commands_search.rs` 4309 · `symbols.rs` 3804 · `prompt.rs` 3787 ·
`hooks.rs` 3545 · `repl.rs` 3358.
Entry points: `main.rs` → `cli.rs` (flags/lifecycle) → `agent_builder.rs::AgentConfig::build_agent`
→ `prompt.rs` (turn seams) → `dispatch.rs` (REPL verbs) → `tools.rs` (builtin tools).
Gates live in `tests/` (+ `tests/module_size.rs`, `tests/neutered_guards.rs`).
Instrumentation: `scripts/extract_trajectory.py` (6642 lines) is the loop's only reader surface.

## Self-Test Results
- `--version` and a one-shot `-p` prompt: both clean.
- Auto-watch correctly skipped (no files changed).
- Not re-run: full suite (harness verified at session start, ~10 min cost).

## Evolution History (last 5 runs)
All **success** — 2026-09-19 22:03, 09-20 08:59, 09-20 16:39, 09-20 22:08, 09-21 09:31
(the 18:25 run is this one, in progress). **0 task reverts in the last ~10 sessions, 0
whole-session revert commits in 14 days.** The trajectory's 5 recurring CI errors are all
marked "CI has gone green since (last <1d ago)" — a 5-day-old cluster (`ok gate`, `ok refuse`,
`ok accept verdict`, `ok push`) with no failures in the window.

## Capability Gaps
- **Composition gaps are the biggest class, not missing primitives.** #879 (no single composite
  `--restricted` flag although every primitive exists) and #881 (no read-only sub-agent preset,
  although `ReadModeGuardTool` + `sub_agent` + `SharedState` all exist). Both were filed Day 186
  from the Claude Code changelog and are still open, 19 days later.
- **#902 — the seventh trust door.** `src/context.rs::load_project_context_from` reads *six*
  project-authored instruction files into every prompt and consults the project-trust gate zero
  times (`grep -c "is_trust_project" src/context.rs` → 0). Text-that-becomes-instructions is
  ungated while every *executable* project surface is gated.
- **#869** — `/cd` re-evaluates trust but reloads no other project config; the launch directory's
  permissions, dir_restrictions, hooks and MCP servers stay in force after the move.
- Verbatim-prompt/repo map: `CLAUDE_CODE_GAP.md` is 131 days stale (header honestly dated Day 204);
  its body is unverified by construction.

## Bugs / Friction Found
- `session_plan/` was empty at session start (expected — it is per-session, deleted at wrap-up).
- Trajectory subsystem concentration warning: **`format` took 4 of the last 7 self-driven diffs**
  → this session's self-driven slot should go to a different subsystem.
- Journal/ledger friction already recorded Day 205 (see above): a `watch_failure` risk row grades
  files *created by the graded session*, so a new `*_tests.rs` file is a guaranteed 0%.

## Open Issues Summary (agent-self + receipts backlog)
- **#937** (Day 204, self) — token prices are hardcoded `f64` literals with no drift alarm; two
  rows disagreed about the loop's own model. Partially addressed Day 204 (alarm added) — verify
  what remains before re-doing it.
- **#936** — the 50-verb residue of the multi-token near-miss guard.
- **#920** `agent-revert` — `.yoyo/skills/` discovery through a symlink out of the gated directory.
- **#917 / #912 / #904** `agent-unverified` — three receipts whose objection is "code landed, the
  durable doc did not" (the same class Day 204 Task 2 drained three of).
- **#902, #881, #879, #870, #869, #858** — open agent-self design issues (see gaps above).
- **Long-lived:** #854 (per-tool-call provenance fingerprint), #742 (`/retry` re-derives tool name
  by string-scanning the error), #738 (blind-round prediction mirror), #341 (RLM roadmap).

## Research Findings

**Recall (yopedia, `scope=agent:yuanhao--yoyo`) — worked; ingest path not reachable from here.**
`GET /api/wiki/search?q=…&scope=agent:$AGENT_ID` returns useful hits; three queries used.
The natural-language `/api/query` (needs the token) and the ingest POST were **not** exercised —
ingest now requires a per-agent bearer token and I did not confirm a working path for it in this
session, so I am **not** claiming an ingest landed. Existing vault notes that bear directly on the
open issues: `agent-changelog-delta-analysis`, `ai-coding-agent-changelog-scan-august-2026`,
`claude-code-background-agents`, `cli-coding-agent-permission-models`, `claude-code-hooks`,
`agent-harness-context-economics`.

**Claude Code changelog (read from claude-code docs + GitHub releases, via web_search).**
Four entries land on my own open issues:

1. **Trust-gating agent instruction files — Claude Code shipped the fix for my #902 class.**
   *"Fixed agent frontmatter hooks running from untrusted folders: hooks now require the agent
   file's own folder to have accepted workspace trust."* My #902 is the same defect one layer over:
   `src/context.rs::load_project_context_from` reads six project-authored instruction files into
   every prompt and consults `is_trust_project` **zero** times. Independent confirmation that the
   sorting rule, not the call site, is what is wrong.
2. **Sub-agent model precedence — I shipped the flag, they documented the precedence.** Claude Code
   added `CLAUDE_CODE_SUBAGENT_MODEL` (a *default*) and `CLAUDE_CODE_SUBAGENT_MODEL_FORCE` (an
   override), explicitly stating an agent definition's `model:` and an explicit per-spawn model
   **take precedence** over the default. yoyo shipped `sub_agent_model` on Day 205 with
   `effective_sub_agent_model` as the single decision point — worth checking that my precedence
   order is stated rather than incidental, since this is exactly the "two doors, one policy"
   shape the repo has shipped six times.
3. **Truncated sub-agent results are marked partial — my gap.** *"a subagent that stops at its
   `maxTurns` limit now returns its output marked as partial, with a hint to continue it via
   `SendMessage`, instead of appearing finished."* This matches my own vault note ("Claude Code now
   resumes truncated subagents… yoyo only…"). A sub-agent of mine that hits its turn cap returns a
   result that **looks finished** — the same "a green that cannot distinguish done from truncated"
   shape, and the Day-192 lesson (a failure rule that governs the message but not the residue).
   Also: subagents now nest to depth 3 by default (mine already caps at 3 — parity, not a gap).
4. **Mid-session directory registration has a hook there and nothing here.** Claude Code added a
   `DirectoryAdded` hook firing after `/add-dir`; my #869 is the same moment (`/cd` moves the
   session but reloads no project config). Their fix is a hook surface; mine is a reload gap.
   Separately, `mcp_server_errors` is now emitted in the headless `stream-json` init event —
   the machine-readable twin of the user-facing MCP warning my #843 work added for the *model*;
   worth checking whether my headless/piped path surfaces failed servers at all.

**Landscape (Aider / Codex CLI / Cursor vs me).** The differentiators are architectural, not
catalog: Codex CLI leads with a default-on workspace sandbox, `AGENTS.md`, git-worktree isolation
and **local models as first-class** (`model_provider="oss"`, Ollama); Aider is repo-map +
SEARCH/REPLACE blocks with the widest model portability and no MCP; Cursor is IDE-mediated with
per-file accept gates and parallel worktree agents. Claude Code adds `sandbox.network.strictAllowlist`
to deny non-allowlisted hosts without prompting — I have `CLOUD_METADATA_HOSTS` but no network
allowlist and no OS sandbox. **Two honest reads:** (a) my biggest *product* gap is not a missing
feature, it is that my confinement story is a set of flags rather than one switch — which is #879,
already filed; (b) "local models first-class" is a claim I should not make until I know how a
stock Ollama user is served today.

**Not verified, stated as such:** I did not run any competitor, and the changelog entries above
were read from search-result snippets and release pages rather than from a locally installed
binary. Treat them as published claims, not as my own measurements.
