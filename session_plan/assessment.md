# Assessment — Day 238 (13:51)

## Build Status
Pass — verified by the harness at session start (HEAD 694cfadc). Binary `yoyo v0.3.0 (694cfadc 2026-10-24)`.
`./target/debug/yoyo -p "Reply with exactly: PONG"` → `PONG`, exit 0 (auto-watch line printed because this repo's .yoyo.toml opts in; it skipped, no files changed).

## Recent Changes (last 3 sessions)
- **Day 238 08:38** — Fixes #1063: `/bg run` waits on `sh`, not on pipe EOF, then a 2s bounded reader grace and an in-band cut note (162849da). Part of #1016: interrupt flush now drains the RetrySafeGate's released-but-unprinted queue before the held text (6b51086d). Filed #1077 (/bg kill leaves the shell running) and #1078 (journal-prompt closing ritual; protected evolve.sh line, asked @yuanhao A/B).
- **Day 238 00:16** — Part of #1070: instruction file >150,000 bytes → loud stderr warning (src/instruction_size.rs, 9b476be5). Part of #1012: exact mistral-small-latest + two gemini-2.5-flash variant price arms (a6e28da7); audit drift 62 → 58. Planner posted the #1064 decision (split default: scrub hooks + MCP stdio by default; bash/run/!/bg/watch stay opt-in) — **decided, NOT yet implemented**.
- **Day 237 19:52** — Fixes #1074 (doctor footer no longer points at /fix); Part of #1012 (mistral-large-latest, gemini-2.5-flash(-lite) prices). yoagent 0.25.12 → 0.25.13 (41d47eaf, auto patch).
- Trajectory: last 10 sessions all 2/2 tasks, 0 reverts. Concentration: format 4/10, context 3/10, prompt 3/10.
- llm-wiki (external journal): paused mid-migration; last entries are from April.

## Source Architecture
141 files in src/, ~217k lines total (incl. inline tests). Largest: cli.rs 7769, commands_risk.rs 6602, tool_wrappers.rs 5707, tools.rs 5247, agent_builder.rs 4962, config.rs 4851, safety.rs 4720, commands_search.rs 4582, commands_spawn.rs 4570, watch.rs 4444, prompt.rs 3909, commands_git.rs 3809, symbols.rs 3804, format/cost.rs 3597.
Entry: main.rs → cli.rs (parse) → dispatch_sub.rs / dispatch_near_miss.rs (shell verbs) or REPL (repl.rs, commands*.rs) or prompt mode (prompt.rs, prompt/*). Agent construction: agent_builder.rs; tools: tools.rs + tool_wrappers.rs; subprocess env scrub: subprocess_env.rs (only tools.rs calls it today).

## Self-Test Results
Ran in a fresh temp dir (`git init`):
- `yoyo doctor` → branch shows `master` (unborn-branch fix from #1072 works), footer "3 warnings. See each row above for details." exit 0. Rows still say `run /init` and `run /remember to create` to a shell user (#1075, still open, reproduced).
- `config get model` 0; `config get nosuchkey` 2 with settable-key list; `grep zzz` 1; bare `find` / `tree zz` 2 with usage; `diff` outside a repo 1. Good.
- `yoyo cost` / `context` / `bg list` / `tokens please` / `cost now` → refused with "REPL command" message, exit 2. Good.
- `yoyo log` → "did you mean: /bg" — a poor edit-distance suggestion (log→bg). Minor.
- `yoyo remember x`, `yoyo add foo.rs`, `yoyo web example.com`, `yoyo search foo` → silently become billed prompts (prompt mode). This is the documented, deliberate residue of #886 (prose-risky verbs, dispatch_near_miss.rs:206-212), not a new bug — but `remember` is exactly the verb #1075's doctor row tells shell users to run, so following doctor's advice bills a turn.
- Confirmed at HEAD: `BackgroundJobTracker::kill` (src/commands_bg.rs:136) only `abort()`s the tokio task; no `kill_on_drop`, no `start_kill` → #1077 still live.
- Confirmed at HEAD: `src/hooks.rs:741` spawns `sh` with inherited env; `grep subprocess_env::` hits only tools.rs → hooks and MCP don't honour even the opt-in scrub (#1064 next slice).

## Evolution History (last 5 runs)
`gh run list evolve.yml`: 4 success (Oct 23 14:38, 19:44; Oct 24 00:13, 08:35) + this run in progress. No failures, no reverts in window. Last CI failure class (fifo_config tests with no API key in ci.yml) is 5 days old and fixed (44540ac4). Provider health clean; usage records 10/10.

## Capability Gaps
- **Process lifetime of background work.** Claude Code force-kills background commands 1.5s after asking them to stop on exit (2.1.29x) and fixed sandbox denies lifting when a background command outlives the session (2.1.309). yoyo: `/bg kill` doesn't kill (#1077), and since jobs run as tokio tasks owning a `Child` without `kill_on_drop`, yoyo exiting with a live `/bg` job presumably orphans it too (read from code, not probed). One fix site covers both.
- **Credential hygiene for non-model subprocesses.** Decided on #1064 (hooks + MCP stdio scrubbed by default) but unshipped; hooks/MCP don't honour even the opt-in today.
- **Oversized instruction files.** CC 2.1.309 now loads only the *start* of a nested CLAUDE.md over its limit; yoyo (#1070 slice 1) warns but still sends the whole file every request. The cap/truncation half is the open decision on #1070.
- **Surface-aware advice** (#1075): doctor tells shell users to run REPL-only commands; following `run /remember` from the shell becomes a billed prompt.
- Longer-standing: no TUI (#215), no benchmark submission (#156), reconnection of surviving MCP servers after a failed connect.

## Bugs / Friction Found
1. **#1077 /bg kill** — reports killed, shell keeps running (measured in the issue: `alive_after_kill=true`). Small, local fix: keep a kill handle / `kill_on_drop(true)` + ideally process-group kill (mirror StreamingBashTool's GroupKill); exit code after kill must reflect reality. Same file just touched by #1063, context fresh.
2. **#1064 hooks scrub default** — decision posted 00:33Z today, slice spelled out on the ticket: hooks default-on scrub through subprocess_env.rs, explicit opt-out, near-miss that a non-credential var still reaches the hook. Then MCP stdio (check whether yoagent's stdio spawn accepts an env first). Security-relevant product default; decided and unshipped.
3. **#1075 doctor REPL-only advice** — reproduced; `/remember` advice also contradicts the TTY fix offer `mkdir -p .yoyo`.
4. **#1062 --api-key doesn't skip setup wizard** — open, not re-probed this session.
5. Minor: `yoyo log` suggests `/bg`.

## Open Issues Summary
agent-self backlog (open, newest first): #1078 (waiting on @yuanhao — protected evolve.sh; do NOT edit communicate skill rule 6 meanwhile), #1077, #1075, #1073 (half landed: usage record has `sub_agents`; extract_trajectory's phase-spend totals and `/cost` still omit it), #1072 (residue: `/pr create` on detached HEAD says "not in a git repository"), #1071 (help-wanted, protected synthesize.yml), #1070 (first slice landed; truncation/cap undecided), #1068 (bash guard refusals get "try simpler" recovery hint + auto-retry eligibility), #1066 (eval ssh-agent carve-out), #1064 (decided split, unshipped), #1062, #1060 (openai wire residue), #1057 (/spawn stale sweep can remove a live --bg worker's worktree), #1054 (yoagent 0.25.9 extra tool-result blocks, five wrappers, no request-level test), #1045, #1041 (MCP prefix — check whether already landed per CLAUDE.md Day 231 text), #1040, #1037, #1026, #1023, #1021, #1018, #1016, #1014, #1012 (58 drifted rows), #991.
No new community issues; the only human-filed opens are old (#341, #215, #156, #141).

## Research Findings
yopedia recall (agent scope) surfaced existing notes: "AI Coding Agent Failure-Mode Parity", "Backgrounded child process holding the tool output pipe open", "Claude Code 2.1.295–2.1.302", "Agent Changelog Delta Analysis". Built on them rather than re-reading.
Claude Code changelog, new since the last mapped range (2.1.309 Oct 22, 2.1.310 Oct 23) — items that map onto yoyo tickets:
- nested CLAUDE.md over the size limit loads only its start → evidence for #1070's cap half.
- tool output previews cut through an emoji broke jq on transcripts → our #250 class (already a rule).
- status line showed `HEAD` as branch outside a repo → #1072 class (yoyo fixed its reader Day 237).
- background command outliving the session lifted sandbox denies → #1077 / bg-exit lifetime.
- disk-full Bash cut now says so and names the folder (yoyo: not checked).
- MCP call timing out non-interactively with unanswered elicitation: model told "no answer came, don't repeat" → same model-facing-failure idea as yoyo's Day 181 external_tool_failure_note.
- retry without effort beta / mid-conversation system messages when a gateway rejects them → #1060 third-party base_url residue.
- hook fail-open on Windows when hook shell missing → checked yoyo: pre-execute spawn error returns Err (blocks, fail-closed), post-execute passes through (by design, documented).
Saved one yopedia note: "Claude Code 2.1.309–2.1.310 mapped onto yoyo" (queued).

## Planner notes (candidates, ranked by readiness)
1. **#1064 hooks slice** — decided on the ticket at 00:33Z today with the slice spelled out (default-on for hooks via subprocess_env.rs, explicit opt-out, non-credential near-miss). Title "Part of #1064" (MCP stdio remains). Placement check: hooks.rs:741 is the one `Command::new("sh")` for hooks — grep for other hook spawns before claiming "all hooks".
2. **#1077 /bg kill** — measured bug, one file, context fresh from #1063. Decide group-kill vs sh-only kill explicitly (ticket separates it from #1063's normal-exit no-kill decision); probe the yoyo-exit-with-live-job path in the same task (event census: kill, REPL exit, panic) and report it, since `kill_on_drop` would change both.
3. **#1075** doctor rows by surface — smaller; needs a surface flag into run_doctor_checks.
4. #1073 second half (extract_trajectory phase-spend totals add `sub_agents`; check `/cost`).
Do NOT: edit skills/communicate rule 6 (held on #1078, @yuanhao); touch protected files.
