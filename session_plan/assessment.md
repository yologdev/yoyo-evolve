# Assessment — Day 211 (19:30)

## Build Status
Pass — verified by the harness at session start (HEAD `1f5505dc`). Binary probes this session:
- `./target/debug/yoyo --version` → `yoyo v0.1.18 (1f5505dc 2026-09-27)`.
- `yoyo -p "Reply with exactly: PONG"` → `PONG`, model `claude-opus-5-5`, auto-watch skipped ("no files changed").
- `yoyo tokens` at the shell → correct free refusal ("/tokens is a REPL command…") — #886 fix holding.
- `.yoyo.toml` md5 identical before and after the `-p` run (bcd60552…) — the #962 clobber class did not recur on a real prompt.

## Recent Changes (last 3 sessions)
- **Day 211 16:28 (2/2 ✅)** — the session that ended three empty sessions.
  - Task 1 (#962): dispatch tests were running the real setup wizard / `init` handler in the repo root and overwrote `.yoyo.toml`; they now target a tempdir and assert the real config is byte-identical before/after. Root cause of the day's silent damage: commit `8a437178` (Day 210 18:59 Task 1) committed a clobbered `.yoyo.toml` (60 lines deleted) plus a `.yoyo.toml.bak`; the creator repaired config in `331db0a7` / `13adf7cc` (max_tokens 128000 for Opus 5.5; evolve.sh API-error detection).
  - Task 2 (#961): `AutoCheckTool` skips the per-edit `cargo …` check when the edited file cannot affect it (new `src/auto_check_scope.rs`, 161 lines); non-cargo check commands keep old behaviour.
  - Committed WIP before cargo (7 small commits) — the Day-209 ordering lesson held.
- **Day 211 00:12 / 09:48 / 14:59 — 0/1 each, no commits.** Receipts #960/#959 show the impl agent's log ending in a model list (`claude-sonnet-5 … Switch with: /model claude-opus-5`) — i.e. the model-unavailable path from the clobbered config, not a code problem. Run 36328548109 (15:10) cancelled on HTTP 400 `max_tokens: 131072 > 128000`.
- **Day 210 18:59 (1/2)** — unhittable-count denominator (`recorded_unhittable/recorded_unmeasurable` on `SurpriseRow`/`RetrospectiveCount`); shipped UNVERIFIED (#958, evaluator produced no verdict). Task 2 (#944 slice: daily_diary.sh spend) landed nothing (#959).

## Source Architecture
93 files in `src/`, ~189k lines incl. tests (`wc -l src/*.rs src/*/*.rs`). Largest: `cli.rs` 7584, `commands_risk.rs` 6532, `tool_wrappers.rs` 5586, `tools.rs` 4940, `config.rs` 4650, `commands_spawn.rs` 4639, `safety.rs` 4557, `agent_builder.rs` 4513, `watch.rs` 4418, `commands_search.rs` 4309, `symbols.rs` 3804, `prompt.rs` 3787, `hooks.rs` 3684, `commands_project.rs` 3645. Entry: `main.rs` → `cli.rs` (args/config) → `agent_builder.rs` (agent + MCP) → `prompt.rs` (turn loop) → `dispatch.rs` / `dispatch_sub.rs` (slash + shell subcommands). Tools in `tools.rs`, wrappers (guard/confirm/auto-check/recovery/sub-agent fallback) in `tool_wrappers.rs`. Risk subsystem is 12+ `commands_risk_*` files.

## Self-Test Results
- Prompt mode works end to end on Anthropic/Opus 5.5.
- Auto-watch banner still prints `cargo clippy … && cargo test` in prompt mode even for a no-tool answer (then correctly skips) — harmless noise.
- No friction found in the three probes; I did not run the suite (harness did).

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 6`: 19:29 in progress (this one); 16:27 success; 15:10 **cancelled** (max_tokens 400); 14:58 success; 09:46 success; 00:10 success. Note: three of those "success" runs produced zero commits (workflow green ≠ work landed — the trajectory's "did not reach a verdict" rows). CI failures listed in trajectory are all 11 days old and CI is green since. No task reverts in window.

**Subsystem concentration: risk 2/4 of last self-driven diffs → trajectory says send the self-driven slot elsewhere.** The DREAM's unhittable milestone (cycles 10–11 + Day 210 denominator) is effectively delivered; further risk work should wait.

## Capability Gaps
- `/cd` reloads only trust, not project config/permissions/hooks/MCP/skills (#869) — Claude Code treats project config per-directory.
- No composite `--restricted`/safe-mode flag (#879).
- `/retry` guesses the failed tool by string-scanning the error text (`extract_tool_name_from_error`, `src/commands_retry.rs:55`, called at :125) although `PromptOutcome.last_tool_name` (`src/prompt.rs:190`) already carries it (#742; reverted twice as #773/#779). Still true at HEAD.
- TUI (#215), benchmark submission (#156) — long-standing, large.

## Bugs / Friction Found
1. **#958 unverified diff needs a human-style review** — `8a437178` also carried the `.yoyo.toml` clobber + `.yoyo.toml.bak` (the .bak is gone at HEAD; config repaired). The risk-code half (≈320 lines, mostly tests) was never judged. Small review task; close or follow up.
2. **#960 / #959 receipts are artifacts of the config clobber**, not task defects. #960 (fallback "self-improvement") can be closed as caused-by-#962; #959's objective (daily_diary.sh spend, #944 slice e) is still valid and unattempted in substance.
3. **#742** — concrete product bug, small, non-risk subsystem; two prior reverts were "no progress", so a plan must say *commit before cargo* and keep the scope to wiring `last_tool_name` into `handle_retry` with the string-scan kept only as fallback.
4. Empty-commit sessions render as workflow "success" — trajectory now surfaces them, but #916 (evolve.sh API-error detector deaf to plain output) is creator lane (protected file).

## Open Issues Summary
agent-self backlog: #944 (unmetered phases: social/daily_diary/synthesize; social slice done Day 209), #937 (price drift — Days 204/207/209 built the audit; likely closable/narrow), #902 (instruction-file trust door — annotation + clause shipped Day 194/210; residue only), #879 (composite safe mode), #870 (counterfactual_green fix-loop population), #869 (/cd reload), #858 (skill-evolve gate defects), #738 (blind-round mirror). Receipts: #958 unverified, #959/#960 no-change reverts, #779/#773 old reverts on #742. Unlabelled: #936, #916 (creator lane), #854, #742.

## Research Findings
Source: code.claude.com/docs/en/changelog + GitHub releases (v2.1.246–v2.1.260, Sep 2026), one web_search this session. yopedia recall/ingest skipped this session (context budget exhausted mid-assessment); nothing below is saved to yopedia.
- **`claude -p` fixed: text output dropping the already-produced answer when a turn dies on a mid-stream API error.** Transferable probe for yoyo's prompt mode: does `-p` print partial assistant text on a mid-stream failure? Unverified here — worth one targeted read of `src/prompt.rs` before planning.
- **`DirectoryAdded` hook fires after `/add-dir` mid-session** — CC treats a working-directory change as an event that re-evaluates config/hooks; directly relevant to #869 (`/cd` reloads only trust).
- **`/rewind` fixed for reporting success when checkpoint backups were missing** — same "success wording over a no-op" class my trajectory has been repairing; candidate audit of yoyo's `/undo`/checkpoint messages.
- **MCP connect failures now show HTTP status + error text in `/mcp` and `claude mcp list`**, plus a warning for hidden leading/trailing whitespace in MCP config values. yoyo shipped the failed-server naming (Day 202) and model-side note (Day 181); the whitespace warning is a small unshipped transfer.
- Subagents nest to depth 3 by default (yoyo already has depth cap 3 via RLM substrate) — parity, not a gap.

## Suggested priorities for the planner (non-binding)
1. #742 `/retry` → use `last_tool_name` (product, small, non-risk subsystem; plan must say *commit before cargo*; two prior no-progress reverts).
2. Review #958's unverified diff and close or follow up; close #960 as a config-clobber artifact (caused by what #962 fixed).
3. #944 slice (e) `daily_diary.sh` spend (#959's objective, never really attempted — the session died on the wrong model).
Avoid the risk subsystem this session (2/4 concentration).
