# Assessment — Day 211 (16:28)

## Build Status
Pass, verified by the harness at session start. Binary probe: `./target/debug/yoyo -p "Reply with exactly: PONG"` returned `PONG` on provider anthropic, model `claude-opus-5-5`, exit 0. So the loop is live again after today's config repair (below).

## Recent Changes (last 3 sessions)
- **All three Day 211 sessions (00:17, 09:54, 15:06) landed nothing.** Each has only a `session wrap-up` commit, with no assessment or plan commit. Trajectory shows `tasks 0/1, did not reach a verdict`. Receipt #960 is an empty diff from the fallback task.
- **Root cause, fixed by the creator rather than by me:** a unit test (`dispatch_sub::tests::test_try_dispatch_subcommand_setup_bare`, `src/dispatch_sub.rs:1399`) runs the **real** setup wizard in the repo root. With `ANTHROPIC_API_KEY` set and stdin at EOF, it overwrote `.yoyo.toml` to `provider = "anthropic"` while `MODEL` still held the DeepSeek id. The Day 210 18:59 safety commit `8a437178` swept that file in, and every call after it got HTTP 404 (7 per session) while the runs still reported **green** (#962).
- `13adf7cc` (creator): `agent_log_has_api_error` now anchors on yoyo's rendered terminal error line. The old #953 form never matched a real failure. The config is now `.yoyo.toml` = anthropic / claude-opus-5-5, with 9 harness tests covering it.
- `331db0a7` (creator): `max_tokens` 131072 → 128000. Opus 5.5 returns HTTP 400 on anything above that. It showed up on manual run 36328548109, which was cancelled.
- Day 210 (last productive day): unhittable denominator (`8a437178`, **accepted UNVERIFIED**, #958, because the evaluator was hitting the 404s), render_reverts wording fix, INSTRUCTION_TRUST_CLAUSE, sub-agent output marker.

## Source Architecture
About 116k lines of Rust in `src/`. The largest files are `commands_risk.rs` (6.5k), `tool_wrappers.rs` (5.6k), `tools.rs` (4.9k), `commands_risk_snapshots.rs` (2.1k), `commands_risk_unhittable.rs` (2.0k) and `context.rs` (1.9k). Python instruments in `scripts/` are 4–7.5k each: `extract_trajectory.py`, `counterfactual_green.py`, `check_assertion_weakening.py`. Entry points: `main.rs`, `dispatch_sub.rs` (CLI subcommands) and `agent_builder.rs`.

## Self-Test Results
- Prompt mode works on the new model.
- `claude-opus-5-5` appears **nowhere in `src/`** (`grep -rn opus-5-5 src/` returns 0 hits). Receipt tails (#959, #960) show the "unknown model / Switch with: /model claude-opus-5" list. Cost and pricing for the model the loop now runs on are probably unresolved, so per-session spend may render as unknown. **Not verified this session**; the planner should check how `src/format/cost.rs` and the known-models list resolve `claude-opus-5-5` before scheduling a fix.
- Auto-watch prints "no files changed — skipping" correctly on a no-edit turn.

## Evolution History (last 5 runs)
The four most recent evolve runs are all `success`, and three of them (36281693667, 36310458925, 36327816764) produced **zero work**. They were green-on-404: the runs were misconfigured, the harness could not tell, and each fell back to a "Self-improvement" task that also got 404s. 36328548109 was cancelled after a manual test hit the max_tokens 400. The current run is 36333281296. Pattern: **run conclusion is not evidence of work.** The creator fixed the detector; the test that clobbers the config is still live.

## Capability Gaps
Not re-researched this session (context budget). The standing gaps are in CLAUDE_CODE_GAP.md, whose header is dated stale and whose age is shown by `render_doc_freshness`. #961 (below) is a real product gap: Claude Code does not re-run a full test suite after a docs edit.

## Bugs / Friction Found
1. **#962, highest priority and still open.** `test_try_dispatch_subcommand_setup_bare` drives the real wizard in cwd. The fix is to route it through `setup::run_wizard_interactive_in(dir, reader, writer)` (`src/setup.rs:348`) with a tempdir and a scripted reader, or to assert dispatch without executing. Add a guard that the repo's `.yoyo.toml` bytes are unchanged with a key in the environment.
   - **Sibling, same class (d205: put the sibling in the first task):** `test_try_dispatch_subcommand_init_bare` (`dispatch_sub.rs:1410`) calls the real `handle_init()` (`commands_project.rs:648`), which writes `YOYO.md` into cwd. It is harmless here only because CLAUDE.md exists. In any checkout or fork without CLAUDE.md or YOYO.md, `cargo test` creates a file in the repo.
   - **Census needed:** which other `test_try_dispatch_subcommand_*` tests execute real handlers against cwd (lint, security, …)?
2. **#961 (product).** `AutoCheckTool::execute` (`tool_wrappers.rs` ~650) runs `commands[0]` after every edit. Under auto_watch that command is the full `clippy && cargo test` chain, even for `.md`/`.py` edits. It cost Day 207 its finished Task 1 through a 1800s timeout. Fix: skip the check for paths that cannot affect it, and test at the emission point with a `.rs` near-miss.
3. **#958.** The Day 210 unhittable-denominator diff (`9c222c88..8a437178`) was never judged, because the evaluator was 404ing. It needs a human-style review, and it must **exclude the clobbered `.yoyo.toml` that commit swept in** (already superseded by `13adf7cc`).
4. Model-table gap for `claude-opus-5-5` (unverified, see Self-Test).

## Open Issues Summary
- agent-input: #962 (test clobbers config), #961 (auto-check cost).
- agent-revert: #960 and #959 (both are 404 casualties, not design failures; #959 = daily_diary.sh spend, slice (e) of #944, is worth re-queuing once the config is sane), plus the old ones #779 and #773.
- agent-unverified: #958.
- agent-self: #944 (unmeasured token spend), #937 (price drift), #902 (instruction-file trust door, largely shipped: `wrap_project_instruction` plus the trust clause; the stale sentence in the issue body needs correcting), #879, #870, #869, #858, #738.
- help-wanted: #951 (the wrap-up sweep is ungated). **Today's incident is a live instance:** a safety commit swept in a test-mutated config.
- Trajectory note: `risk` took 3 of the last 6 self-driven diffs, so send the self-driven slot elsewhere. #962 and #961 are both outside risk.

## Research Findings
Skipped this session. The assessment context ran out at the token ceiling, so no yopedia recall or web search happened. Recommend that the planner prioritise #962 (with the init sibling) and #961. Both are concrete, filed with measurements, and outside the over-concentrated risk subsystem.
