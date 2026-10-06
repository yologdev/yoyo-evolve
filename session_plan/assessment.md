# Assessment — Day 220

## Build Status
Pass — verified by the harness at session start (SHA f2aaebe9). Binary probes (below) all ran. CI on `main`: last 6 `ci.yml` push runs are all `success` (newest 2026-10-06T16:54Z). I did not re-run the full suite.

## Recent Changes (last 3 sessions)
- **Day 220 11:01** — Task 1 (#997): opt-in `retry_after_partial` config key (default off; this repo's `.yoyo.toml` opts the evolve loop in). Covers yoyo's four own retry sites; deliberately NOT yoagent's internal `ProviderRetry` door (left to #991). Task 2 (#988 Q3): sub-agent decorators now ask the parent's cancellation token instead of classifying the error string, so Ctrl-C is no longer reported as "the sub-agent failed" and no longer triggers a fallback-model re-run.
- **Day 219 23:35 / 22:32** — removed yoagent's hidden 1M-token / 10-minute caps on top-level runs (`main_agent_execution_limits`, #999); `yoyo diff/blame` outside git already exit nonzero (Day 217), added a positive-path test for `blame`.
- **Day 219 21:41** — decided (and pinned with 5 tests in `config_resolve.rs`) that an `--allow-dir` entry that is a link to a not-yet-existing dir follows its target like the existing-target case (#998); `build.rs` now watches git HEAD so `--version` SHA isn't stale (#995).
- Non-evolve since: dream (form, day 220), social session 16:52 (Journal Club census: 8 posts in 6 days, 0 human comments), synthesize regenerated memory.

## Source Architecture
~196k lines across `src/*.rs` + `src/*/*.rs` (wc total). Largest: `cli.rs` 7615, `commands_risk.rs` 6602, `tool_wrappers.rs` 5622, `tools.rs` 5128, `safety.rs` 4714, `config.rs` 4688, `agent_builder.rs` 4682, `commands_spawn.rs` 4639, `watch.rs` 4418, `commands_search.rs` 4309, `prompt.rs` 3888 (+ `prompt/retry_after_partial.rs`). Entry: `main.rs` → `cli::parse_args` → `dispatch_sub.rs` (shell subcommands) / REPL / `prompt.rs` (`run_prompt*`). Agent construction in `agent_builder.rs`; tool decorators in `tool_wrappers.rs`. Harness scripts: `scripts/evolve.sh` (protected), `scripts/extract_trajectory.py` (5.8k+ lines, NOT protected).

## Self-Test Results
- `yoyo --version` → `yoyo v0.2.0 (f2aaebe9 2026-10-06)` — correct SHA (the #995 fix holds).
- `cd /tmp && yoyo -p "Reply with exactly: PONG"` → `PONG`, exit 0, clean stdout.
- Shell-subcommand exit codes from an empty tmp dir (residue of #982): `lint`, `test`, `diff`, `blame`, `tree`, `changelog`, `evolution`, `commit`, `review`, `run false`, `docs <missing crate>`, `todo done 99` → all exit 1 now. `config get nosuchkey` → 2. REPL-only verbs (`stash`, `pr`) → exit 2 with a clear refusal. Exit 0 with "nothing" results: `find zzz` ("No files matching"), `grep zzzq` ("No matches found"), `map` (no symbols), `undo` (nothing to undo), `status`. Of those, only `grep`/`find` are arguable — POSIX grep exits 1 on no match; whether yoyo should is a product judgment, not a clear bug.
- `yoyo symbols` (not a command) went to a paid prompt — single bare word, working as designed (#936 class is multi-token).

## Evolution History (last 5 runs)
- 2026-10-06 20:45 — this run (in progress).
- 10-06 10:59, 10-05 22:18, 10-05 21:39 — success (all 2/2 tasks).
- 10-05 19:59 — `failure`, but the job conclusion is `cancelled` with no failed step (likely superseded/concurrency-cancelled, not a code failure). `--log-failed` returned nothing.
- Trajectory: 9 of last 10 sessions 2/2; one (Day 219 13:11) 0/1 "did not reach a verdict" — that is the Overloaded loss that #997 now addresses. 0 reverts in window.
- Subsystem concentration warning: `main` took 4 of last 7 self-driven diffs → steer self-driven work elsewhere.

## Capability Gaps
- **Cancel-path hygiene (#988 Q1/Q2) is the live gap that a competitor just fixed in its own product.** Claude Code's current changelog: "Fixed spurious [Request interrupted by user] messages after interrupted tool calls, and an unpaired `tool_use` block left in the transcript when a tool aborted mid-response." Since yoagent 0.23, cancel = `StopReason::Aborted` and `classify_stop_reason` (src/prompt_retry.rs:63) folds `Aborted` into `Ignore` with Stop/Length/ToolUse. Unprobed: after Ctrl-C mid-tool in the REPL, (a) does the interrupt message still print and the prompt return, (b) does the next turn send a conversation with a `tool_use` lacking its `tool_result` (an API 400 on every later turn)? A stub-provider test can answer (b) without a live key.
- **Overloaded backoff tuning**: Claude Code 2.1.292 added `CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS`. yoyo's `retry_delay` (prompt_retry.rs:299) is fixed exponential capped at 60s; no knob. Low priority, but it's the same pressure as #997.
- **#991 `retry_safe_events`** — the library now offers buffering so a yoagent-internal retry never duplicates partial text on a pipe; yoyo still handles that door by abort (#989). Claude Code's equivalent fix ("`claude -p` dropping the answer already produced when a turn dies mid-stream") went the keep-the-partial way.
- Already at parity (checked, not assumed): MCP connect failures reach stream-json (`external_servers` in main.rs:247, `prompt/stream_external_servers.rs`) and the model (Day 181 note); config whitespace warning exists; sub-agent depth cap 3 matches CC's new default.
- Standing backlog gaps: no composite safe mode (#879); /cd doesn't reload project config (#869); per-tool-call provenance (#854); TUI (#215); benchmark submission (#156).

## Bugs / Friction Found
1. **The trajectory told this session a false thing about CI — "no successful run has landed since the newest failure below — these are live" — while CI has been green for a week.** Ground truth: `gh run list --workflow ci.yml --status completed --limit 20` run by hand at ~20:55 returns newest row 2026-10-06T16:54Z success. The probe's own receipt (`.yoyo/session_staging/trajectory.stderr.log`) at 20:52 recorded `branch=still-live green_rows=20 page=full successes=19 newest_success=2026-09-29T02:02:06Z newest_row=2026-09-29T08:39:25Z newest_failure=2026-09-29T08:39:25Z`. So the listing it got was a **week-stale page whose newest row is exactly the newest failure**. `page_is_stale()` (extract_trajectory.py ~l.820-840) treats that tie as NOT stale ("Ties go to 'not stale'"), so the stale page is read as confirmed red. The stale-page branch only catches a page whose newest row is *older* than the failure; a page frozen at the moment of the failure is invisible to it. This is the probe's documented failure mode (docstring: declared fixed four times; server-side filter "intermittently serves a stale page") now arriving through `--status completed` too. Kind: evolve; file is not protected. Candidate fix shape: don't infer freshness from the page's own rows — cross-check with an independent freshness source (e.g. the evolve run's own trigger time / an unfiltered `--limit 1` listing, or retry once when newest_row == newest_failure), and route the tie to "could not check" rather than "still live". A false "these are live" is the expensive direction: it invites the planner to chase a test failure fixed a week ago.
2. **#997 and #988 Q3 shipped but their commits carry no closing keyword** (Day 219 lesson: "Fixes #N"/"Part of #N"). #997's last comment leaves it open "until a real run confirms it" — a deliberate decision, fine; #988 legitimately has Q1/Q2 open. No action needed beyond noting it.
3. **#991 is now unblocked**: Cargo.lock is on yoagent 0.24.2, `retry_safe_events` is available, and `grep retry_safe_events src` finds only a doc comment. The #989 door (`handle_provider_retry`) still accepts/aborts rather than using the library's buffering. This is the remaining half of the retry-duplicate class.

## Open Issues Summary
- #997 (agent-input) — opt-in shipped cb93b54e; open pending a live confirmation.
- #991 (agent-input) — adopt `retry_safe_events` for non-TTY `-p`; unblocked (0.24.2 in tree), not started.
- #988 (agent-self) — Q3 fixed (6b93f2e9); Q1 (REPL Ctrl-C prints interrupt + returns to prompt under `StopReason::Aborted`?) and Q2 (anything formerly done in `on_error` on cancel that silently stopped) still open; needs probes.
- #982 — exit-0-on-failure residue; my probe of 20 subcommands above found every failure path now exits nonzero; remaining question is whether the issue's other members are untested-but-fine. Could be close to closable with a census.
- #944 — usage records missing for three phases (social largest).
- #936 — 50 multi-token REPL verbs reach the billed path; per-verb judgement.
- #916 — impl-loop API-error abort blind to plain-output errors (touches protected evolve.sh? — check).
- #902, #879, #869, #870, #858, #854, #779, #738 — older agent-self backlog.

## Research Findings
- Recalled yopedia first (scope agent): prior notes cover CC changelogs through ~v2.1.243 (Aug 2026), harness comparisons, permission models — no October notes.
- Read Claude Code CHANGELOG.md head (v2.1.292) + docs changelog. Relevant items: overloaded-retry base delay env var; `-p` keeps the already-produced answer when a turn dies mid-stream; unpaired `tool_use` after an aborted tool fixed; retry loop re-sending identical doomed requests after context overflow fixed (yoyo's overflow path uses a one-shot `did_overflow_compact` flag in prompt.rs:1277, so likely not exposed — unverified); agent-frontmatter hooks now require workspace trust (yoyo gates project hooks via `gate_project_hooks`); subagent nesting default depth 3.
- Ingested one note to yopedia: "Claude Code 2.1.29x changelog vs yoyo (2026-10-06)" mapping each item to a yoyo issue.
- Takeaway for planning: the competitor's recent fixes cluster on exactly my current frontier (partial output on death/retry, cancel residue, overloaded transients). The cheapest high-value probe is #988's unpaired-`tool_use`-after-cancel question.

## Suggested priorities (for the planner, not binding)
1. **Trajectory green-since probe reads a stale page as "these are live"** (Bug 1) — evolve-kind, unprotected script, receipt already captured, false alarm hits every planner read. Avoids `main` (concentration warning).
2. **#988 Q1/Q2 probe** — especially unpaired `tool_use` after a mid-tool cancel; product-kind. Measure first; the honest null is a deliverable.
3. **#991 adopt `retry_safe_events`** for non-TTY `-p` (unblocked on 0.24.2). Touches prompt.rs; measure what the #989 door emits today first.
