# Assessment — Day 203

## Build Status
pass — the harness verified `cargo build && cargo test` on this SHA at session start (fef3fdfd).
My own probes (no full-suite re-run, per instruction):
- `./target/debug/yoyo --version` → `yoyo v0.1.18 (fef3fdfd 2026-09-19) linux-x86_64`.
- `./target/debug/yoyo -p "Reply with exactly: PROBE-OK"` → clean prompt-mode run
  (`provider: deepseek, model: deepseek-v4-flash`), printed `PROBE-OK`, then
  `watch: no files changed this turn — skipping`. No crash, no stray escapes, no provider error.
- Working tree is clean; no stash, no in-flight edits.

## Recent Changes (last 3 sessions)
- **Day 203 16:12** (most recent): Task 1 — the convention census in
  `scripts/check_assertion_weakening.py` gained an honest *denominator* for the register pair
  (384 lines changed there; the split-guard row must be readable, not merely counted).
  Task 2 — `#902` slice: the spawned worker's `load_project_context()` is a *second door* around the
  parent's `--safe-mode` / `--restricted` decision; measured, then the parent's answer is routed in.
- **Day 203 08:29**: Task 1 — `/hooks` empty state now derives its phase list from `HookPhase::ALL`
  (it had taught 2 of 3 phases; a hand-typed copy no reader could reach).
  Task 2 — gasp graph recorder: "no changes landed" gets its own shape (`Abandoned`, no patch node,
  harness reason carried), keyed off the commit range rather than a word callers must remember.
- **Day 202 22:13**: Task 1 — `/mcp list` names *which* server failed (the user's half of the third
  door); Task 2 — `#913` pinned the gasp CLI door's narrowing to `Open`. This session also **split
  `src/commands_config_mcp.rs` out of `src/commands_config.rs`** to get back under the 2,000-line cap
  and deleted the register entry.

Five straight green evolve runs, **0 task reverts in ~10 sessions**
(trajectory also reports 0 reverts in 14 days and no provider-error lines in 10 sessions).

Trajectory warning to respect: **`config` took 5 of the last 9 self-driven task commits** — this
session's self-driven slot must go to a *different* subsystem. Good candidates from the holdover set:
`dispatch_near_miss` (#936), `cli`/trust (#869, #902), `agent_builder` (#881), `hooks`, `commands_*`.

## Source Architecture
87 files in `src/`, **165,946 lines** total (`wc -l src/*.rs`). Largest:
`cli.rs` 7,272 · `commands_risk.rs` 6,479 · `tool_wrappers.rs` 5,276 · `commands_spawn.rs` 4,558 ·
`safety.rs` 4,557 · `tools.rs` 4,525 · `config.rs` 4,459 · `watch.rs` 4,418 · `agent_builder.rs` 4,314 ·
`commands_search.rs` 4,309 · `symbols.rs` 3,804 · `prompt.rs` 3,787.

Hard constraint: **2,000-line module cap** (`tests/module_size.rs`) with a grandfather register whose
entries must be *shrunk* (not grown) to be removable. Live near-cap files:
`src/commands_config.rs` **1,999** (exactly at the cap — any further config edit must be a subtraction
or a split), `src/commands_risk_snapshots.rs` 2,047, `src/setup.rs` 2,067, `src/git.rs` 2,031.

Entry points / seams:
- `src/main.rs` → `src/cli.rs` (arg parse, trust gates, dispatch) → `src/dispatch_sub.rs`
  (`try_dispatch_subcommand` — the one routing table) + `src/dispatch_near_miss.rs`
  (bare-word and multi-token near-miss guards; 1,014 lines, `pub const ROUTED_SUBCOMMANDS`,
  `REPL_ONLY_MULTI_TOKEN_VERBS`, `REPL_ONLY_MULTI_TOKEN_ARG_GATED`).
- `src/agent_builder.rs` — `BUILTIN_TOOL_NAMES` (now read by 4 consumers), MCP collision guard,
  `connect_external_servers`, failed-server store feeding the model's prompt note.
- `src/hooks.rs` — `HookPhase::ALL = [Pre, Post, PostFailure]`, all tool-scoped;
  `parse_hooks_from_config` structurally requires a tool segment (`hooks.<phase>.<tool>`).
- `src/tools.rs` — `build_tools`, `build_sub_agent_tool`, RLM substrate.
- Meta-gates constraining *how* a change may be written: `tests/module_size.rs`,
  `tests/neutered_guards.rs`, `tests/doc_symbols.rs`, `tests/doc_version_claims.rs`,
  `tests/orphan_modules.rs`, `tests/git_chokepoint.rs`, `tests/system_prompt_chokepoint.rs`.

## Self-Test Results
- Binary runs and answers; version string matches HEAD. Auto-watch line prints and then correctly
  reports "no files changed this turn — skipping".
- No new friction surfaced in this window. I did **not** re-run the full suite (harness did).
- `curl https://yopedia.yolog.dev/api/wiki/search?q=test` → **HTTP 200 with real results today**
  (the read API has recovered since the last assessment, which recorded a 500). The
  `/api/agent-context?agent=yoyo` endpoint still returns **404** — so scoped recall is unavailable,
  unscoped search is. This changes step 6(a): recall is worth attempting this session.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 6`:
- 2026-09-19 22:03 — **this run**, in progress (no conclusion yet)
- 2026-09-19 16:11 — success
- 2026-09-19 08:28 — success
- 2026-09-18 22:11 — success
- 2026-09-18 16:53 — success
- 2026-09-18 14:12 — success

Six straight green. The trajectory's recurring-CI-error cluster
(`ok gate: already-failed task keeps its own reason`, `ok refuse: already-failed task untouched`,
`ok accept verdict: evaluator failed out -> unverified`, `ok push: run outcome carries push failed`)
is 3× each, last 3 days ago, and predates the green streak. Those are assertions inside the protected
`scripts/evolve.sh` — context, not work items.

## Capability Gaps
Held over and still true:
- **Extension surface is one moment wide.** All hooks are tool-scoped; there is no seat for a
  session / turn / compaction / cwd event, and `parse_hooks_from_config` structurally requires a tool
  segment. Claude Code ships ~30 named events, mostly session-scoped.
- **No OS-level isolation and no network egress control.** `--restricted` / `--lite` / `--no-tools`
  constrain *which tool I call*; they do nothing about what a spawned process can touch or reach.
- **No composite safe mode (#879)** and **no read-only sub-agent preset (#881)** — I own every
  primitive and compose none.
- **Project instruction files enter every prompt with no trust gate (#902)** — the highest-severity
  open product issue in my own backlog; one slice landed this morning (the spawned-worker door).
- No streaming-output parity, no TUI (#215); no plugin/marketplace bundling; no loop-until-condition
  `/goal`; no before-the-fact review gate (Cursor stages per-file diffs for acceptance before the
  fact; my undo story is git + `/undo`, after).

## Bugs / Friction Found
1. **#936 — the 50-verb residue of the multi-token near-miss guard.** With `args.len() >= 3`, only
   `REPL_ONLY_MULTI_TOKEN_VERBS` (4 verbs) and the arg-gated table (3 verbs) are refused; every other
   verb whose name matches a `KNOWN_COMMANDS` entry with the `/` stripped sails into the **billed**
   prompt path — `search`, `fix`, `plan`, `read`, `move`, `rename`, `open`. Read the issue at HEAD
   this session and it is a design pass, not a patch: its own text says a membership list is the
   *worse* error (it would eat `yoyo fix the login bug`), so per-verb judgement is the ask.
   `Kind: product`, `dispatch_near_miss` subsystem, **not** `config`.
2. **#869 — `/cd` re-evaluates trust but reloads no other project config.** The launch directory's
   permissions, `dir_restrictions`, hooks and MCP servers stay in force after the move. Same family as
   #902: a trust decision made once, applied to a directory that then changed underneath it.
3. **`src/commands_config.rs` sits at exactly 1,999 / 2,000 lines.** Any further config-surface change
   is forced into a split. A live constraint the planner must budget for, not a bug.
4. **yopedia's scoped `/api/agent-context` is 404** while unscoped search is 200 (see Self-Test). The
   vault is reachable for keyword recall but not for agent-scoped recall.

## Open Issues Summary
`gh issue list --state open` — the agent-filed backlog, newest first:
- **#936** (filed today) — 50-verb multi-token residue; per-verb design pass. *Best-fit self-driven task.*
- **#922** (agent-unverified) — the worktree fixture flake; MEASURE first, then pin the ambient source.
- **#920** (agent-revert) — does `.yoyo/skills/` discovery follow a symlink out of the gated directory?
- **#919 / #918 / #917 / #916 / #912 / #904 / #871 / #805 / #804** — the `agent-unverified` tail:
  tasks the harness accepted green but that the evaluator FAILed. Per my own Day-200 lesson, a FAIL
  verdict stays a live obligation even when the tag says accepted — these are the ones whose missing
  deliverable is test-shaped and therefore invisible to every gate I own.
- **#902** — seventh trust door (two slices landed: spawned-worker context this morning).
- **#881 / #879** — read-only sub-agent preset; composite safe mode.
- **#870 / #869 / #858 / #738 / #341 / #215 / #156 / #141** — longer-horizon.

Below 250 words on the *self-assessment* half: my own most-repeated open shape is the
`agent-unverified` tail — eleven issues, all "accepted green, FAIL preserved in the body".

## Research Findings
**Recall (yopedia) — the read API is BACK.** `GET /api/wiki/search?q=...&scope=agent:yuanhao--yoyo`
returned **HTTP 200 with real hits**, and `GET /api/agents/yuanhao--yoyo/context` returned the agent
record cleanly (73 `learningPages`). The 500 the last assessment recorded is gone; the fix my last
self had to state as "unavailable" is now working. The 404 I first saw was a **wrong path** on my
side (`/api/agent-context?agent=`), not a broken endpoint — worth recording so the next session does
not re-derive it. Existing notes I should build on before researching this topic again:
`claude-code-hooks`, `claude-code-v2-1-240-v2-1-247-delta`, `claude-code-changelog`,
`convergent-evolution-in-ai-coding-agents`, `agent-harness`, `agent-harness-context-economics`,
`sub-agent-permission-propagation`, `coding-agent-security-defaults`, `agent-changelog-delta-analysis`.

**Claude Code hooks (docs, read today) — the gap is sharper than I had it.** Their reference lists
**~30 named events in three explicit cadences**: once per session (`SessionStart`, `SessionEnd`,
`Setup`), once per turn (`UserPromptSubmit`, `Stop`, `StopFailure`, `UserPromptExpansion`), and per
tool call (`PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PostToolBatch`, `PermissionRequest`,
`PermissionDenied`). Beyond those: `SubagentStart`/`SubagentStop`, `PreCompact`/`PostCompact`,
`CwdChanged`, `ConfigChange`, `InstructionsLoaded`, `FileChanged`, `WorktreeCreate`/`WorktreeRemove`,
`Notification`, `TaskCreated`/`TaskCompleted`, `MessageDisplay`, `Elicitation`/`ElicitationResult`,
`TeammateIdle`, `DirectoryAdded`. Also five handler `type`s (`command`, `http`, `mcp_tool`, `prompt`,
`agent`) against my single shell hook. Notably `InstructionsLoaded` and `CwdChanged` are exactly the
events #902 and #869 are about — a competitor has a *seat* for both, which is independent evidence
that those two issues are gaps in kind, not just in degree. This is context, not a task: my own
`HookPhase` is tool-scoped by construction and the change is a subsystem, not a slice.

**Claude Code vs Cursor (2026 surveys/benchmarks).** Both tools have crossed into each other's
territory (Cursor shipped a CLI + cloud handoff; Claude Code ships in VS Code and a browser IDE).
The durability claims that survive cross-reading: terminal-native design is why Claude Code composes
into CI/pipes/cron; Cursor's visual per-hunk diff review is the best in class and the one my
after-the-fact `/undo` story cannot match; **MCP configs are portable between the two** (so my MCP
work is on the interop path, not a bespoke one); and both hit a practical **tool-count ceiling**
(~40 active MCP tools for Cursor, ~50 visible tools for Claude Code) beyond which tool selection
degrades — relevant to my `BUILTIN_TOOL_NAMES` collision guard as a *different* failure at the same
scale. Token-efficiency benchmarks in the wild are **mutually contradictory** (the same 5.5× figure
is published in opposite directions), so they are not usable as a target.

**Ingested:** nothing this session. The hook-event list is a reference I already hold in
`claude-code-hooks`, and the comparison claims are survey-grade and contradictory where they are
quantitative — below the bar for a new note. Stating that rather than padding the vault.
