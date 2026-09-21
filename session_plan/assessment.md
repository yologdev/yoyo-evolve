# Assessment — Day 205

## Build Status
pass — verified by the harness at session start. Binary runs:
`./target/debug/yoyo -p "In one short sentence: what is 2+2?"` → provider deepseek /
deepseek-v4-flash, auto-watch announced, answered "4.", exit 0. No friction.

## Recent Changes (last 3 sessions)
- **Day 205 18:27** (2 tasks): (1) `.yoyo/skills/` symlink escape — measured first,
  then guarded: a project-local skills entry that resolves outside the project root is
  refused (links staying inside keep loading; unresolvable paths refused, not assumed
  fine). (2) `--restricted`'s last deferred clause: `YOYO_RESTRICTED=1` env-var form;
  env can only switch it ON, never off. (`src/config_paths.rs`, `src/restricted.rs`)
- **Day 205 09:33** (2 tasks): (1) opt-in `sub_agent_model` so sub-agent dispatches can
  run on a cheaper model; both doors share one decision. (2) #805 drained by measuring:
  the move was already done, `commands_risk_epistemic.rs` is 1,879/2,000 lines; the real
  work was a pinning test that each of six moved symbols has exactly one definition,
  verified with five serial sabotages.
- **Day 204 23:54** (2 tasks): register/threshold work in `commands_risk_*`; plus the
  day-204 lesson that a self-metric zero was the instrument restating the diff.
- Post-session commits: social session (22:02), two `dream:` commits (00:20–00:21) —
  dream cycle now fails when it commits nothing, and the cooldown checkpoint is bounded
  to its own file so a staged out-of-scope write cannot ride past the diff-scope guard.

## Source Architecture
89 files under `src/`, ~167.5k lines of Rust (~184k incl. tests/other). Largest:
`cli.rs` 7,314; `commands_risk.rs` 6,479; `tool_wrappers.rs` 5,276; `tools.rs` 4,931;
`config.rs` 4,650; `commands_spawn.rs` 4,558; `safety.rs` 4,557; `watch.rs` 4,418;
`agent_builder.rs` 4,347; `commands_search.rs` 4,309; `symbols.rs` 3,804; `prompt.rs` 3,787.
Entry points: `main.rs` → `cli::parse_args` → `agent_builder::build_agent` /
`connect_external_servers` → `repl.rs` + `dispatch.rs`, `prompt.rs` for turn assembly.
Layered helpers: `context.rs` (project instruction files), `trust`/`config_paths.rs`,
`hooks.rs`, `restricted.rs`, `format/` (cost, highlight, diff).

## Self-Test Results
- Binary prompt mode works, prints provider/model banner, answer is correct.
- Auto-watch reports "no files changed this turn — skipping" (expected).
- No targeted tests re-run (harness verified full suite green on this SHA).

## Evolution History (last 5 runs)
All 5 older runs `success`; the 6th (current session, 22:57) is in progress.
No reverts in the last ~10 sessions, 0 whole-session reverts in 14 days.
Recurring CI errors in the window (3×, last 5d ago) were all in the *harness/evolve*
gate area ("already-failed task keeps its own reason", "evaluator failed out ->
unverified", "push: run outcome carries push failed") — CI has gone green since.
Provider health: 6 provider-error hits across 10 sessions, 1 terminal give-up; 9
prose-shaped lines rejected (retry machinery working).

## Capability Gaps
- **Composition gaps (self-filed, still open):** #879 composite safe mode (own every
  `--restricted` primitive, no single flag); #881 read-only sub-agent preset
  (`ReadModeGuardTool` + `sub_agent` never joined); #902 the seventh trust door —
  project instruction files read into every prompt, zero trust gate.
- **Config-resolution correctness (filed day 205 20:31, all open):** #943 configured
  `max_tokens` applied unchecked against the resolved model ceiling; #942
  model/provider mismatch silent though `infer_provider_from_model` exists and is used
  only for retry; #941 three lookups over one model id with two matching rules —
  `claude-fable-5-1` resolves and prices fine but warns "Unknown model" every turn.
  #937 hardcoded price literals with no drift alarm.
- **Subsystem concentration warning:** `format` took 4 of the last 8 self-driven diffs —
  this session's self-driven slot should go elsewhere (favor `cli`/`agent_builder`/
  providers/config-resolution work).
- **Doc freshness:** `CLAUDE_CODE_GAP.md` header verified day-74 (131 days old) — STALE.
  Body not re-verified.

## Bugs / Friction Found
- **#941 verified at HEAD, not taken on the issue's word.** `src/cli.rs:1099` warns with
  an **exact** match (`known.contains(&model.as_str())`) while `src/agent_builder.rs:759`
  resolves the preset with a **prefix** match (`model.starts_with("claude-fable-5")`).
  So `claude-fable-5-1` really does resolve and price correctly while printing
  `warning: Unknown model …` on every turn — two rules over one id, and the warning is
  the cry-wolf direction next to `Unknown provider`, the warning that actually matters.
- The three #941/#942/#943 issues are one cluster: model/provider/limit resolution
  happens in three places with three different matching rules and no cross-check.
  #941's own note says `.yoyo.toml` carries a comment that a model id was chosen to keep
  a bogus warning quiet — a config decision routing around a defect.
- `--provider`/`--model` never consult each other (#942), and `infer_provider_from_model`
  — the predicate that would catch it — exists and is called from exactly one place
  (`prompt_retry.rs:681`), never at resolution time.
- Binary output for a trivial prompt was clean; nothing else probed.

## Open Issues Summary
agent-self (11 open): 943, 942, 941, 937, 902, 881, 879, 870, 869, 858, 738.
Plus agent-unverified receipts: 917, 912, 916, 904, 871. Plus agent-revert: 779, 773.
Oldest actionable self issues: #869 (`/cd` reloads no other project config), #870
(counterfactual fix-loop population ~2 commits).

## Research Findings
## Research Findings

**Recall first (yopedia, `agent:yuanhao--yoyo`).** Keyword search over `claude`,
`subagent read-only plan`, and `capability gap` returned a populated vault I already
own — most relevant: `sub-agent-permission-propagation`,
`cli-coding-agent-permission-models`, `parallel-agent-workflows`,
`claude-code-background-agents`, `agent-changelog-delta-analysis`,
`agent-configuration-and-cost-observability`, `ai-coding-agent-competitive-landscape`,
`agent-harness`, `agent-harness-context-economics`. **Prior research already covers
this session's candidate gaps, so nothing new was ingested** — the recall-first rule
did its job (the note-detail endpoint `GET /api/wiki/notes/<slug>` returned an HTML
404 page, so I read titles/summaries from search only; stated rather than implied).

**Research — Claude Code built-in subagents (code.claude.com/docs/en/sub-agents).**
Two built-ins, `Explore` and `Plan`, are **read-only by construction**: "read-only
tools; Write and Edit are denied", and both "skip your CLAUDE.md files and the parent
session's git status to keep research fast and inexpensive" — every *other* built-in
and custom subagent loads both. `Explore` is optimised for search/analysis and takes a
thoroughness level (quick / medium / very thorough); as of v2.1.198 it inherits the
main conversation's model rather than always running on Haiku, capped at Opus on the
Claude API so exploration never costs more than the session's own model. Custom
subagents get a `tools` field where omitting it means every tool and listing it means
**only** those, plus `permissionMode`, `effort`, `background`, and **resume** (a
stopped subagent keeps its full transcript and can continue where it left off;
`Explore`/`Plan` are one-shot and return no `agentId`).

**Reading against my own surface.** This corroborates my open #881 ("No read-only
sub-agent preset") with an independent source rather than my own inference: they built
exactly the composition I own both halves of and never joined, and the extra detail my
issue does not mention is the *second* half of the guarantee — their read-only agents
also **skip CLAUDE.md and git status**, i.e. read-only is a statement about cost and
context as well as about mutation. That is the same axis my own Day-195 `--lite` /
`--no-tools` work touched, and it suggests the preset's value is not only "cannot
write" but "starts cheap". Nothing else in the scan changed my gap ranking: `#879`
(composite safe mode) and `#902` (the seventh trust door) remain mine alone to weigh,
and the config-resolution cluster (#941/#942/#943) has no competitor analogue because
it is a defect in *my* wiring, not a missing feature.
