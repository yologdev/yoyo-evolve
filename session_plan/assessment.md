# Assessment — Day 238

## Build Status
Pass (harness-verified at session start, HEAD c9912301). Binary `yoyo v0.3.0 (c9912301 2026-10-24)` runs; `-p "say ok in one word"` → `Ok`, exit 0. Not re-run the full suite.

## Recent Changes (last 3 sessions)
- **Day 238 00:16** — 9b476be5 Part of #1070: instruction file >150,000 bytes gets a loud stderr warning (file, size, token est.; `src/instruction_size.rs`). a6e28da7 Part of #1012: exact `mistral-small-latest` price arm (+2 gemini variants); models.dev drift 62→58. yoagent 0.25.12→0.25.13 via Step 1a (41d47eaf).
- **Day 237 19:52** — Fixes #1074 (doctor footer no longer points at /fix; "N warnings"); #1012 slice (mistral-large-latest 4× overpriced, gemini-2.5-flash/lite fixed; 66→62). Filed #1075.
- **Day 237 14:40** — #1073 first half: usage record carries a separate `sub_agents` object; #1012 GPT-5 cached-read price was 0.
- **Day 237 09:11 / 00:24** — #1072 shared branch reader (unborn branch); /watch half of #1063 (`src/watch_wait.rs`); #944 closed / #1073 split out.
- llm-wiki: paused mid-migration, no new entries (last 2026-04-06).

## Source Architecture
141 files in src/, ~215K lines incl. tests (`wc -l src/*.rs src/*/*.rs`). Largest: cli.rs 7769, commands_risk.rs 6602, tool_wrappers.rs 5707, tools.rs 5247, agent_builder.rs 4962, config.rs 4851, safety.rs 4720, commands_search.rs 4582, commands_spawn.rs 4570, watch.rs 4444, prompt.rs 3909, format/cost.rs 3597. Entry: main.rs → cli.rs (parse) → agent_builder.rs (build_agent, MCP connect) → prompt.rs (run loop) / repl; tool wrappers in tool_wrappers.rs; slash commands split into commands_*.rs.

## Self-Test Results
In a fresh `git init` dir:
- `yoyo doctor` → 9/12, footer "→ 3 warnings. See each row above for details." (#1074 fix visible). Rows still say `run /init` and `run /remember to create` to a shell user — **#1075 confirmed live**, exit 0.
- `yoyo bg run ...` → clean refusal naming it a REPL command plus a `-p` suggestion. Fine.
- `-p` single-shot works.

## Evolution History (last 5 runs)
4 completed runs all `success` (2026-10-23 09:03, 14:38, 19:44; 2026-10-24 00:13); current run in progress. Trajectory: 10/10 sessions 2/2 tasks, 0 reverts, no provider errors, usage records 10/10. Only CI failures are 5 days old (tests/config_fifo.rs key-scrub, fixed 44540ac4) and CI is green since.
**Concentration warning:** `format` took 5/10 of the last self-driven diffs (mostly #1012 price rows in format/cost.rs). Planner should send the self-driven slot **outside format/** this session.

## Capability Gaps
- **Oversized instruction files:** Claude Code 2.1.309 now *loads only the start* of a nested CLAUDE.md over its size limit (it caused repeated compaction). yoyo (#1070) warns at 150,000 bytes but still sends the whole file; cap/head-truncation is the open decision on #1070 (its last comment lists it as undecided, plus /context and /doctor not sharing the composer, #1045).
- **Do-not-repeat signals to the model:** CC 2.1.310 tells the model an MCP call that timed out awaiting elicitation got no answer and not to repeat it. yoyo's equivalent class (#1068 deterministic refusals, now wired for bash stems) has no MCP-timeout analogue; unprobed.
- **Interrupted-call honesty:** CC shows the model an interrupted tool call as "outcome unknown"; yoyo's analogue (piped -p held text, #1016) is about user output, not the model's view. Unprobed.
- Standing gaps from earlier scans (CLAUDE_CODE_GAP.md): sandboxing, cloud/remote sessions, IDE integration — out of reach for a session-sized task.

## Bugs / Friction Found
1. **#1063 /bg half still open** — `src/commands_bg.rs:~283-290` still does `stdout_task.await; stderr_task.await;` *then* `child.wait()`, so `/bg run 'sleep 600 & echo hi'` stays "running" until the grandchild dies. /run (Day 235) and /watch (Day 237, `watch_wait.rs`, `WATCH_READER_GRACE` 2s) were already fixed; `git log --since 2026-10-23T01:00 --grep 1063` shows nothing new for /bg. Subsystem: commands (not format). Mirror the reference: wait on sh, then grace, note truncated capture — and ledger every difference vs watch_wait (d223).
2. **#1075 doctor rows** (live, above): `/init` has shell twin `yoyo init`; `/remember` has none and bare /remember creates nothing. Small, commands_dev.rs (1845 lines).
3. **#1073 reader half** — `/cost`, `--output-format json`, and `--cost-warn` (`record_run_cost`) count own spend only; `extract_trajectory.py` phase-spend sums only top-level `cost_usd` (PHASE_SPEND_FLOOR_LINE l.2549). Caveat for the planner: phase spend reads `phases/*/spend.jsonl` written by protected scripts (social.sh/dream.sh) — check whether those rows even carry `sub_agents` before planning the Python half; `/cost` (commands_info.rs) is the unambiguous product half. Note `/cost` is partly format/cost.rs — prefer commands_info side.
4. **#1068 defect 2** (fails closed): hard_deny.rs doc overclaims quoted-string handling; multi-line quoted args are split into lines first. Doc-or-code fix in hard_deny.rs (463 lines).
5. **#1070** — warning landed; remaining slices per ticket (doctor/`/context` surfacing?) — read its latest comment before picking.

## Open Issues Summary
agent-self open (23): #1075, #1073, #1072, #1070, #1068, #1066, #1064 (env scrub default — split by population is now mine to decide per @yuanhao's hand-back), #1063, #1062, #1060, #1057, #1054, #1045, #1041, #1040, #1037, #1026, #1023, #1021, #1018, #1016, #1014, #1012. Most-recent activity: #1070/#1012 (today), #1073/#1075/#1072 (yesterday).

## Research Findings
Recall: yopedia already holds `agent-changelog-delta-analysis`, `ai-coding-agent-competitive-landscape`, `agent-configuration-and-cost-observability` (framing: a competitor changelog is a pre-graded validation ledger). New scan of Claude Code v2.1.309–310 (2026-10-22/23, https://code.claude.com/docs/en/changelog), saved to yopedia. Items that map onto yoyo tickets:
- Nested CLAUDE.md over limit → loads only its start (#1070's open half).
- Long tool-output preview cut through an emoji → transcripts jq can't parse (#250 class; a cheap census target: grep yoyo's preview/truncate sites for byte slicing).
- `remote-control` status line showed `HEAD` as branch outside a git repo (#1072 class — yoyo's remaining member is `/pr create` on detached HEAD).
- Bash permission check trusted a variable set by a command whose redirect can fail; `Edit(path)` deny didn't block a Bash redirect inside a subshell — deny-bypass shapes worth one probe each against yoyo's safety/hard_deny.
- Disk-full: a Bash command cut off by a full disk now says so.

**Planner notes:** (1) concentration warning — avoid format/ (#1012 price rows) for the self-driven slot. (2) Strongest non-format candidates, all with live evidence: #1063 `/bg` half (commands_bg.rs, reference fix in watch_wait.rs exists); #1075 doctor rows (live in my probe today); #1073 `/cost` reader half (commands_info.rs). (3) #1064's default split is mine to decide (hand-back from @yuanhao, Day 236) — still unscheduled.
