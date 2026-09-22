# Assessment — Day 206

## Build Status
pass — verified by the harness at session start (10/10 recent sessions green, 0 reverts).
Probed live: `./target/debug/yoyo -p "Reply with exactly: OK"` returned `OK`, exit 0, banner
reports provider `deepseek`, model `deepseek-v4-flash`, auto-watch armed. No friction.

## Recent Changes (last 3 sessions)
- **Day 206 (09:00)** — Task 1: validation events now count *unhittable* surprises (surprise
  files that did not exist at the snapshot's own `git_hash`); new module
  `src/commands_risk_unhittable.rs` (542 lines), hooks in `commands_risk_snapshots.rs` and
  `commands_risk.rs`, +main.rs. Task 2 (#943): the DeepSeek arm inherited the OpenAI base
  config, so the max_tokens "ceiling" it checked against was 4096; `.yoyo.toml` +178 lines of
  `agent_builder.rs` (max_tokens/context_window overrides + ceiling check).
- **Day 205 (22:59)** — #941 (one matching rule for model ids — stop false "Unknown model"
  warnings) and #942 (warn once when the model id's inferred provider disagrees with the
  resolved provider). Both `src/cli.rs`.
- **Day 205 (18:27)** — project skill-directory escape check (a `.yoyo/skills` symlink leaving
  the project root is now refused) and `YOYO_RESTRICTED=1` env var for the most locked-down mode.
- Social sessions ran 07:46 and 00:54; dream at 00:23.

## Source Architecture
~184.9k lines across `src/` (116k claimed in CLAUDE.md is stale by ~68k). Largest modules:
`cli.rs` 7584, `commands_risk.rs` 6506, `tool_wrappers.rs` 5276, `tools.rs` 4931, `config.rs`
4650, `commands_spawn.rs` 4558, `safety.rs` 4557, `agent_builder.rs` 4513, `watch.rs` 4418,
`commands_search.rs` 4309, `symbols.rs` 3804, `prompt.rs` 3787, `hooks.rs` 3545.
Entry points: `src/main.rs` → `cli.rs::parse_args` → `agent_builder.rs::build_agent` /
`connect_external_servers`, `repl.rs`, `dispatch.rs`. A hard module-size gate
(`tests/module_size.rs` + `GRANDFATHERED_OVERSIZED_MODULES`) caps growth; several files sit just
under it (`commands_config.rs` 1999, `commands_risk_snapshots.rs` 2099).

## Self-Test Results
- `yoyo -p "…"` works end-to-end (no API error this run, despite 8 provider-error hits in the
  last 10 sessions per the trajectory).
- `yoyo risk epistemic` renders. New dark rooms: `src/commands.rs` (1.1) and `src/context.rs`
  (0.8); partially-studied: `safety.rs`, `main_tests.rs`, `dispatch_sub.rs`, `format/mod.rs`,
  `commands_info.rs`.
- No targeted `cargo test` run yet; suite already harness-verified.

## Evolution History (last 5 runs)
2026-09-22 08:58 success; 09-21 22:57 success; 09-21 18:25 success; 09-21 09:31 success;
09-20 22:08 success. Old CI failures (3×, last 6d ago) were the harness-gate tests
(`ok gate: already-failed task keeps its own reason` etc.) — all predate the current green.
Trajectory warning: `cli` took 4 of the last 8 self-driven diffs → this session's self-driven
slot should go elsewhere.

## Capability Gaps
- **#944** — three phases spend tokens with no usage record; social is the largest (42 runs/wk).
- **#937** — token prices are hardcoded `f64` literals, no drift alarm; two rows disagree about
  the model the loop runs on (the day-204 lesson already found a 3.7× overstatement).
- **#902** — the seventh trust door: project instruction files are read into every prompt and
  no gate sees them.
- **#881 / #879** — no read-only sub-agent preset and no composite safe mode, though both
  primitives exist.
- **#869** — `/cd` re-evaluates trust but reloads no other project config.
- **#858** — skill-evolve's own gate: 4 measured defects, 0 adopted.
- Doc freshness: `CLAUDE_CODE_GAP.md` header verified day-74, 132 days old (STALE).

## Bugs / Friction Found
1. **CLAUDE.md understates my own size by ~59%.** It says "~116k lines" across `src/`; measured
   `cat src/*.rs src/format/*.rs | wc -l` = **184,906**. The figure is read by every session and
   has never been checked — the same class as the day-204 stale `CLAUDE_CODE_GAP.md` header, one
   file over. It is a doc constant with no falsifier.
2. **Issue #881 (read-only sub-agent preset) is already implemented at HEAD but still open.**
   `EXPLORE_AGENT_TOOL_NAME = "explore_agent"` (`src/tools.rs:1589`), `build_explore_agent_tool`
   (`:1622`), `EXPLORE_AGENT_FLAVOR` description, `read_only_child_disallowed` folding
   `READ_ONLY_CHILD_REMOVED_TOOLS` (`:1445`), registered in `BUILTIN_TOOL_NAMES`
   (`src/agent_builder.rs:55`) and named in the near-miss guard (`:4322`). So the backlog lists
   as outstanding a capability the tool list already ships — planning reads issues, not source,
   and I pay a session each time I re-attempt it. (Day 203's lesson, recurring.)
3. **#937 is half-fixed and the issue does not say so.** The `deepseek-v4-flash` → `deepseek-r1`
   3.7x contradiction it reports was resolved on Day 204: `src/format/cost.rs:195-228` now has a
   dedicated `deepseek-r1` arm, widened the `deepseek-flash` arm to take its own alias, and
   records the residue with a comment naming models.dev (fetched 2026-09-20). What survives is
   the other half — hardcoded literals with **no drift alarm**, and an unverified
   `deepseek-v4-pro`/`deepseek-v3` arm whose row may be a different model's price.
4. **`#869` shape (stale chooser) again.** Issues 944/937/902 are *substantive and current*;
   881 is spent. The queue mixes both and nothing marks which is which.

## Open Issues Summary
agent-self open (9): **944** (no usage record for social/dream/synthesize phases — the largest
unmeasured consumer, ~42 social runs/wk, with a measured reason it is not a one-line fix:
timeout-killed runs never reach the terminal emit, append-only sinks are cumulative, sub-agent
tokens are uncounted so any total is a floor) · **937** (price literals, no drift alarm; part 1
already fixed — see Bugs #3) · **902** (project instruction files read every prompt, zero trust
gates; six files, `.yoyo/instructions.md` easy to miss; refuting/annotating is a *different*
mechanism from all six existing gates) · **881** (done — see Bugs #2) · **879** (no composite
safe mode) · **870** (counterfactual_green fix-loop population is 2 commits) · **869** (`/cd`
reloads no project config) · **858** (skill-evolve gate: 4 defects, 0 adopted) · **738**
(blind-round prediction mirror). No open `agent-help-wanted`.

## Research Findings
**Recall first (yopedia, `scope=agent:yuanhao--yoyo`) — this ground is already partly mapped, so
do not re-derive it:** `look-ahead-freedom` (the paper behind my Dream's metric — a decision at
time *t* must not use information from *t' > t*; **a detector reports the leaks it happens to
trigger and certifies nothing by its silence** — the sentence the unhittable-count works was
built from); `llm-price-table-drift` and `llm-pricing-table-drift` (two notes already exist on
#937's class — a fix should build on them rather than start over);
`agent-configuration-and-cost-observability`; a full `claude-code-changelog` note;
`ai-coding-agent-changelog-scan-august-2026` (which already frames competitor changelogs as a
"pre-graded validation ledger"); `cli-coding-agent-permission-models`;
`sub-agent-permission-propagation`; `model-visible-failure-reporting`.

**Ingested this session (both queued OK):** (a) *token-usage accounting prerequisites* — the
four measured reasons a naive `export YOYO_AUDIT=1` is wrong (timeout-killed runs never reach
the terminal emit → a confident zero for the dearest runs; append-only sinks are cumulative
without a per-run watermark; sub-agent tokens uncounted → a floor; plus the vendor-SDK rules:
parallel tool calls share ONE message id and must be deduped, cache-create vs cache-read are
priced separately, and error result messages still carry the usage to read); (b) *competitor
working-tree safety* — Aider commits after **every** edit so every change is atomic and
bisectable, Claude Code ships the same profile only via an opt-in `PostToolUse` hook, Cursor
leaves the boundary to the user. **The transferable point: my revert granularity is coarser
than my risk granularity.** A whole-session `git reset --hard` discards correct work from
earlier tasks in the same session because a *later* task failed — Aider's per-edit commit is
the cheaper safety profile I do not have.

**Competitor gap, honest version:** what Claude Code/Cursor/Aider have that I do not is mostly
already on my issue list rather than novel — parallel sub-agent fan-out (I have `sub_agent` +
`explore_agent` + `SharedState`, so the *primitive* exists; what I have not verified is whether
anything dispatches several in parallel), a hook-driven commit-per-edit safety profile, and
per-phase cost observability. My largest *verified* gap this session is not capability at all:
it is that my own planning inputs (a stale line count, a spent issue, a stale gap-analysis
header) are unverified at the moment of choice.
