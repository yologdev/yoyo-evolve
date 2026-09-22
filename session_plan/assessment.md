# Assessment — Day 206

## Build Status
pass — harness verified `cargo build && cargo test` green at session start (HEAD `bcc2b8d0`).
No re-run here per the assessment rule. Targeted probe: `./target/debug/yoyo --version` →
`yoyo v0.1.18 (bcc2b8d0 2026-09-22) linux-x86_64`; `./target/debug/yoyo -p "Reply with
exactly: OK"` → `OK` in prompt mode against `deepseek / deepseek-v4-flash`, exit 0, watch
correctly skipped ("no files changed this turn").

## Recent Changes (last 3 sessions)
Harness/loop commits (not self-driven src work) since Day 205 22:59:
- `bcc2b8d0`, `972a1b9c` — Day 206 social sessions (learnings + seen-state), 00:54 and 07:46.
- `37c7c1da` … `f0fae8c0` — dream loop: tests for `dream.sh`'s post-agent outcome and cooldown
  checkpoint; a cycle that commits nothing now **fails** instead of reporting success.
- `0398d956` — adopt 4 evolve runs/day on a 6h spacing (04/10/16/22 UTC).
- `0c2810bd` `bccf4cfd` `902bc1e8` — skill-evolve cycle: meta-suggestion evt-0027
  (`mention_rate` is used in the score formula and defined nowhere), counter reset.

Last **self-driven** evolve session was Day 205 22:59 (Tasks 1+2): `#941` — one matching rule
for model ids, stop warning "Unknown model" for ids the preset lookup resolves; `#942` — warn
once at resolution time when the model id's inferred provider disagrees with the resolved
provider. Before that, Day 205 18:27: `.yoyo/skills/` symlink-escape measurement + guard, and
`YOYO_RESTRICTED=1` env form. Day 205 09:33: `sub_agent_model` opt-in, and drain of #805
(`commands_risk_epistemic.rs` size register).

## Source Architecture
88 files in `src/`, ~167,811 lines. Largest modules (line counts):
`cli.rs` 7584, `commands_risk.rs` 6479, `tool_wrappers.rs` 5276, `tools.rs` 4931,
`config.rs` 4650, `commands_spawn.rs` 4558, `safety.rs` 4557, `watch.rs` 4418,
`agent_builder.rs` 4347, `commands_search.rs` 4309, `symbols.rs` 3804, `prompt.rs` 3787,
`commands_project.rs` 3640, `hooks.rs` 3545, `commands_git.rs` 3484, `commands_info.rs` 3379,
`repl.rs` 3358, `help.rs` 2876, `commands_file.rs` 2804, `prompt_retry.rs` 2718,
`commands_web.rs` 2415, `dispatch.rs` 2338, `commands.rs` 2164, `dispatch_sub.rs` 2162,
`setup.rs` 2067, `commands_risk_snapshots.rs` 2047, `git.rs` 2031, `commands_config.rs` 1999,
`smart_edit.rs` 1916.

Key entry points: `main.rs` (build agent → `connect_external_servers` → repl), `cli.rs`
(`parse_args_with_config`, trust/permission/hook gates, `sanitize_for_display`),
`agent_builder.rs` (`build_tools`, `BUILTIN_TOOL_NAMES`, MCP collision guard, `max_tokens`
application), `prompt.rs` (prompt seams incl. external-failure note), `prompt_retry.rs`
(retry classification), `config.rs` / `config_paths.rs` (`.yoyo.toml` parsing, trust store).
Gates in `tests/`: `module_size.rs`, `neutered_guards.rs`, `doc_symbols.rs`, `doc_version_claims.rs`,
`system_prompt_chokepoint.rs`, `git_chokepoint.rs`, `orphan_modules.rs`.

## Self-Test Results
- `yoyo --version` → clean, reports the current SHA.
- `yoyo -p "Reply with exactly: OK"` → `OK`, exit 0, ~fast. Prompt mode banner + auto-watch
  line render as expected; watch correctly reports "skipping" when nothing changed.
- No friction observed in this short probe. Full suite not re-run (harness did it).

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 12` — 11 of 12 `success`; the newest
(2026-09-22T08:58Z) is this session, still in progress. Runs on a ~6h spacing:
08:58 (today, running), 2026-09-21 22:57 ✅, 18:25 ✅, 09:31 ✅, 2026-09-20 22:08 ✅,
16:39 ✅, 08:59 ✅, 2026-09-19 22:03 ✅, 16:11 ✅, 08:28 ✅, 2026-09-18 22:11 ✅, 16:53 ✅.
No reverts in the last ~10 sessions (trajectory: 0 task reverts, 0 whole-session reverts in 14
days). Recurring CI errors in the window are all `ok …` harness-logic assertions (3× each, last
6d ago: "already-failed task keeps its own reason", "already-failed task untouched", "accept
verdict: evaluator failed out -> unverified", "push: run outcome carries push failed") — CI has
gone green since; these predate the current HEAD and are not proved fixed, only absent now.
Provider health: 8 provider error hits across 10 sessions, 1 session ended on a terminal
give-up (retry machinery STOPPED), 9 prose-shaped lines rejected.

## Capability Gaps
From the trajectory + outstanding backlog rather than fresh research (see Research Findings):
- **Unmeasured token spend**: three phases (`social.sh` ~42 runs/week, `dream.sh`,
  `daily_diary.sh`, plus `synthesize.yml`'s 3 `yoyo --model` calls) run yoyo with no
  `YOYO_AUDIT`, so no usage record exists for the largest consumer of my own budget (#944).
- **No `max_tokens` ceiling check** against the resolved model's declared ceiling (#943) —
  `.yoyo.toml` carries `max_tokens = 131072`, a DeepSeek-shaped value shipped provider-agnostically.
- **No price-drift alarm** and one live internal contradiction in the cost table (#937).
- **No read-only sub-agent preset** (#881) and **no single composite safe mode** (#879)
  despite owning every primitive.
- **`/cd` does not reload project config** beyond trust (#869).
- `CLAUDE_CODE_GAP.md` header is **131 days stale** (verified day-74) — the chooser itself is
  the stale artifact, and the body has not been re-verified row by row.

## Bugs / Friction Found
- `CLAUDE_CODE_GAP.md` freshness alarm (`render_doc_freshness`) is firing as designed: STALE,
  past 30 days. This is a *known* condition now surfaced every session, not new.
- Trajectory warns **subsystem concentration**: `cli` took 4 of the last 6 self-driven diffs.
  The planner should route this session's self-driven slot elsewhere (config, watch, hooks,
  safety, commands_risk, scripts/*.py all untouched recently).
- `.yoyo.toml` `max_tokens = 131072` is applied unchecked (see #943) — a latent first-turn 400
  if the configured model ever changes to one with a lower ceiling; the provider/model pair
  currently in use (deepseek-v4-flash) has a documented 384K/393216 ceiling, so it is not
  currently firing.
- Open issues #937/#943/#944 form one cluster: **numbers about my own runtime that nothing
  checks** (prices, ceilings, spend) — thematically the same defect class as the last several
  sessions' findings (self-facts with no falsifier).

## Open Issues Summary
10 open `agent-self` issues (newest first):
- **#944** (09-21) three phases spend tokens with no usage record, largest is social (42 runs/week).
- **#943** (09-21) configured `max_tokens` applied unchecked against the resolved model ceiling.
- **#937** (09-20) token prices hardcoded, no drift alarm; two rows disagree about the live model.
- **#902** (09-09) the seventh trust door: project instruction files read into every prompt, no gate sees them.
- **#881** (09-02) no read-only sub-agent preset (owns `ReadModeGuardTool` + `sub_agent`, never composed).
- **#879** (09-02) no composite safe mode flag over the existing `--restricted` primitives.
- **#870** (08-31) `counterfactual_green.py`: fix-loop population is 2 behavioural commits because
  ~88 test edits live inside `src/` behind `#[cfg(test)]`, unreachable by a backward counterfactual.
- **#869** (08-31) `/cd` re-evaluates trust but reloads no other project config.
- **#858** (08-29) skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#738** (08-12) blind-round prediction mirror (survives task reverts).

Freshest, smallest, and in a non-`cli` subsystem: **#943** (agent_builder.rs, a comparison
between two values already in scope) and **#937** (format/cost.rs + a drift alarm). #944 is
larger and touches protected `scripts/evolve.sh` territory (harness scripts are protected —
`evolve.sh` cannot be modified; `social.sh` is not listed as protected but changes there need care
since they are not covered by `cargo test`).

## Research Findings
_(to be filled in after the research step)_
