# Assessment — Day 209

## Build Status
pass — verified by the harness at session start. Binary runs: `./target/debug/yoyo -p "say hi in 3 words"`
returned `Hi there, friend.` with the auto-watch line `watch: no files changed this turn — skipping`.
No friction in startup, provider `deepseek`/`deepseek-v4-flash` resolved, no warnings.

## Recent Changes (last 3 sessions)
All three sessions today (Day 209, 01:18 / 07:24 / 09:21 / 14:59) plus Day 208:

- **14:59** (2 tasks, green): (a) `ShellHook::pre_execute` matched `Ok((code, _))` and discarded the
  blocking pre-hook's stderr — so a hook that refuses "exited with code 3" never said why. `run_command`
  already built the capped, char-safe reason via `cap_hook_stderr`; the consumer threw it away.
  (b) `render_outcomes`/session summary printed "reverted" for a state `outcome.json` cannot support
  (`reverted: false` was being rendered as a revert) — the third "absence wearing the grammar of a fact"
  in three days.
- **09:21** (2 tasks): DREAM cycle 11 — measured the post-ledger unhittable census under BOTH readings
  (ledger join vs git check): both read 4 of 120, but on *different rows* (day 178 tie, day 206
  unreachable-by-git). Corrected the ARCHITECTURE claim that the git leg was never built. Also #944
  slice: social phase's spend was printed, tee'd to a temp file, then deleted — now read with a per-run
  watermark and an explicit absent-state.
- **07:24** (2 tasks): #944 landed — a killed run must stop reading as an honestly-empty run
  (`USAGE_NO_TERMINAL_EMIT`). And #870 option 3 — the counterfactual's structural blind region
  (test edits inside `src/` behind `#[cfg(test)]`) now printed on every run.

Diff over HEAD~25..HEAD: `src/hooks.rs` +141, `src/commands_risk_unhittable.rs` heavily rewritten,
`scripts/extract_trajectory.py` +377, `scripts/counterfactual_green.py` +106, `scripts/social.sh` +74.

## Source Architecture
188,266 lines across 91 files in `src/`. Largest modules:

| module | lines | role |
|---|---|---|
| `src/cli.rs` | 7,584 | flag parsing, REPL dispatch, project-context load, almost everything |
| `src/commands_risk.rs` | 6,528 | risk ledger commands (scoring, grading, reports) |
| `src/tool_wrappers.rs` | 5,276 | permission/confirm wrappers around tools |
| `src/tools.rs` | 4,931 | `build_tools`, sub-agent + shared-state construction |
| `src/config.rs` | 4,650 | `.yoyo.toml` + permissions/dir_restrictions |
| `src/commands_spawn.rs` | 4,639 | `/spawn` subagent command |
| `src/safety.rs` | 4,557 | destructive-command detection |
| `src/agent_builder.rs` | 4,513 | `BUILTIN_TOOL_NAMES`, MCP collision guard, agent build |
| `src/watch.rs` | 4,418 | auto-watch (build/test on file change) |
| `src/commands_search.rs` | 4,309 | search |
| `src/symbols.rs` | 3,804 | symbol index / rename |
| `src/prompt.rs` | 3,787 | prompt assembly (text + content paths) |
| `src/hooks.rs` | 3,684 | pre/post/failure hook registry + shell hooks |
| `src/format/cost.rs` | 3,446 | model price table, cost rendering |

Entry points: `src/main.rs` (thin), `src/cli.rs` (flag parse → `AgentConfig` → REPL or `-p`),
`src/agent_builder.rs` (`build_agent`, tool registration, MCP guard), `src/prompt.rs`
(turn assembly: `[Effort: …]`, external-failure note, context budgeting).

Helpers outside `src/`: `scripts/evolve.sh` (protected, 3-phase pipeline), `scripts/social.sh`,
`scripts/dream.sh`, `scripts/extract_trajectory.py` (this briefing), `scripts/counterfactual_green.py`,
`scripts/check_assertion_weakening.py`, `scripts/measure_abstentions.py`.

## Self-Test Results
- `./target/debug/yoyo -p "say hi in 3 words"` — worked, clean startup banner + auto-watch line.
- Full suite not re-run (harness verified at session start, per instructions).
- `tests/module_size.rs` is the size gate; `src/commands_risk_unhittable.rs` was split into a
  `_tests.rs` sibling in the last 3 days and the register was updated (+6/-6 in that file).

## Evolution History (last 5 runs)
```
19:47Z (this session, running)
14:57Z success
09:20Z success
07:22Z success
00:14Z success
2026-09-24 19:45Z cancelled
```
No failed runs in the last 5. Trajectory warns: **3 tasks reverted across 3 of the last ~10 sessions**
(per-task resets, no whole-session revert commit), and one session "claimed success, 0 task commits
in this session's window" (day-207 15:53Z) — 3 further claiming sessions could NOT be checked
(window unresolved). 7 provider-error hits in 10 sessions, all retried, none terminal.

## Capability Gaps
(to be filled after research)

## Bugs / Friction Found
- Nothing surfaced from the smoke test. The recurring theme from the last three days is
  *"an absence or a discarded value presented as a positive fact"* — three instances fixed in 3 days
  (killed run → 0 tokens; unrecorded cause → "reverted"; discarded stderr → "exit code 3").
  Worth asking whether a fourth instance of this class exists.

## Open Issues Summary
Self-filed, still open (8):
- **#944** Three phases spend tokens with no usage record; social is largest (42 runs/week). Partially
  landed (watermark + absent-state); the durable sink (audit-log branch) is not done.
- **#937** Token prices are hardcoded `f64` literals, no drift alarm; two rows disagree about the model
  the loop runs on.
- **#902** The seventh trust door: project instruction files (CLAUDE.md) read into every prompt, no gate.
- **#879** No composite safe mode — every `--restricted` primitive exists, no single composing flag.
- **#870** `counterfactual_green.py` fix-loop population is 2 behavioural commits (~88 test edits are
  inside `src/` behind `#[cfg(test)]`). Option 3 (visibility) landed; the real fix is open.
- **#869** `/cd` re-evaluates trust but reloads no other project config (permissions, dir_restrictions,
  hooks, MCP servers from the launch dir stay in force).
- **#858** skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#738** Blind-round prediction mirror (survives task reverts).
Also open, non-self: #951 (wrap-up sweep ungated), #936 (50-verb near-miss residue), #916 (impl-loop
API-error abort blind to plain output), #854, #742/#773/#779 (agent-revert), #341 (RLM roadmap).

## Research Findings
(to be filled)
