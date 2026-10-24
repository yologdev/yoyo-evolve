# Assessment — Day 238

## Build Status
Pass, verified by the harness at session start for HEAD 41d47eaf (the `deps: semver-compatible updates (yoagent 0.25.12 -> 0.25.13)` commit). I did not re-run the suite. The binary works: `yoyo -p "Reply with exactly: ok"` printed `ok` and exited 0, and `yoyo --version` gives `v0.3.0 (41d47eaf 2026-10-24)`.

## Recent Changes (last 3 sessions)
- **Day 237 19:52:** Fixes #1074. `/doctor`'s footer no longer says "Try /fix" for rows /fix can't repair, and it now says "N warnings. See each row above" (probed, see below). Part of #1012: the mistral-large-latest, gemini-2.5-flash and -lite prices are reconciled (drift 66→62), and cost.rs got 24 lines shorter.
- **Day 237 14:40:** Part of #1073. The audit.jsonl usage record carries a separate `sub_agents` spend object. Part of #1012: the GPT-5 family's cached-read price is no longer 0 (70→66).
- **Day 237 09:11:** Part of #1072. One shared branch reader (`src/git_branch.rs`, `symbolic-ref`), so an unborn branch shows its name and a detached HEAD says "detached". Fixes #944 / filed #1073: yoagent#173 was closed and nobody read it.
- **Day 237 00:24:** Part of #1063. `/watch` and the auto-check hook wait on `sh`, then give readers a 2s grace (`src/watch_wait.rs`). The `/bg` half is still open.
- **Post-session:** yoagent 0.25.12 → 0.25.13 (deps commit). Changelog: "no API change". Two provider fixes, both Bedrock/Gemini/Vertex only: (a) an empty assistant/user message after an errored, cancelled or refused turn is dropped instead of being sent as `parts: []`; (b) consecutive same-role messages are merged, so parallel tool results go back in one message (this fixes the Gemini 400 after any turn with ≥2 tool calls). That bears on yoyo's **google** provider. Per the d230 rule, the re-read population is the upstream diff: grep yoyo for any workaround or claim about Gemini parallel tool calls, empty messages or "role alternation" (not done in this assessment, and cheap).

## Source Architecture
140 files in src/, ~215K lines including tests (`wc -l src/*.rs src/*/*.rs`). Largest: cli.rs 7769, commands_risk.rs 6602, tool_wrappers.rs 5707, tools.rs 5247, agent_builder.rs 4962, config.rs 4851, safety.rs 4720, commands_search.rs 4582, commands_spawn.rs 4570, watch.rs 4444, prompt.rs 3909. Entry points: `main.rs` → `cli::parse_args` → `agent_builder` (build_agent, connect_external_servers) → `prompt.rs` (run_prompt*) / REPL dispatch in `commands*.rs`. Module-size register: `tests/module_size.rs`. cli.rs is registered at exactly 7769, i.e. **zero headroom**. tool_wrappers.rs (5707) and tools.rs (5247) are also at their registered values. Any task touching these needs its logic in a sibling module (the pattern every recent task used).

## Self-Test Results
- `yoyo -p` → `ok`, exit 0. Auto-watch fired and skipped ("no files changed").
- `yoyo doctor` in a fresh `git init` dir: 9/12 passed, footer "→ 3 warnings. See each row above for details." (#1074 fix confirmed). **Still live, #1075:** rows say "create YOYO.md or run /init" and "run /remember to create" to a shell user. The first is REPL-only; `/remember` doesn't create the dir.
- **#1070 confirmed still live:** a temp repo with a 2,160,001-byte CLAUDE.md → `--print-system-prompt` is 2,162,856 bytes, and **stderr is empty**. A ~540K-token instruction file goes into every request silently. No commit has touched #1070, and it has no comments.
- `--api-key sk-test -p hi` (no env key, empty HOME): the 401 hint correctly names `--api-key` as the source (the #1062 slice works). It exits 1, and the wizard isn't reached under -p.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 5`: 4 success (10-23 00:22, 09:03, 14:38, 19:44) plus this run in progress. In the trajectory: 10/10 sessions 2/2 tasks, 0 reverts in 14 days, no provider-error lines. The only CI failures in the window are the Day-233 config_fifo env-key leak (fixed 44540ac4, 4d ago). It's a quiet, healthy loop. Subsystem concentration is mild (format 4/9, prompt 3/9). The risk is the opposite of failure: 20 green sessions of small slices with the same few tickets (#1012, #1062, #1063, #1072, #1073) being chipped at.

## Capability Gaps
(research section below). The standing ones from CLAUDE_CODE_GAP.md-type comparisons: no rich TUI (#215), no official benchmark submission (#156), MCP over stdio only (no HTTP/SSE MCP), and MCP colliding servers are now prefixed (#1041 is still open as a tracker, check whether it should close: the Day 231 CLAUDE.md text says prefixing landed).

## Bugs / Friction Found
1. **#1070 (live, measured today):** an oversized instruction file is loaded whole and silently. The existing `system_prompt_token_budget` warns only past the full window. Fix shape: a warning (bytes + est. tokens + share of window) at a threshold well below the window, emitted where the file is loaded (`context.rs`/`agent_builder.rs`). Decide whether to cap. Product-kind. Note: our own CLAUDE.md is ~40 KB by its own rule, so the loop won't trip a sane threshold. Check `wc -c CLAUDE.md` before choosing it.
2. **#1064, decision owed by ME since Day 235 22:45Z (>2 days):** @yuanhao handed the scrub default back. The choice is to split (scrub MCP stdio servers + repo hooks by default, keep bash/run/bg/watch opt-in) or keep all opt-in with a reason that answers his MCP/hooks point by name. `git log --grep '#1064'` since then: nothing. This is the d235 "self-sealing hold" lesson. The hold is released, and the decision is unmade.
3. **#1073 second half:** `/cost`, `--output-format json`, the `--cost-warn` tally (`record_run_cost`) and `extract_trajectory.py`'s phase-spend still sum own spend only. The data is now on disk (`sub_agents` in the usage record). The trajectory line "Totals are a floor ... (#1073)" stays true until a reader adds it.
4. **#1075:** doctor rows' REPL-only advice (`/init`, `/remember`) printed to shell users. It's small, and the sibling of #1074 just fixed.
5. **#1063 /bg half:** `commands_bg.rs:283-290` still awaits reader EOF before `child.wait()`. Read from code, not run.
6. **#1072 residue:** `/pr create` on a detached checkout says "not in a git repository".
7. **yoagent 0.25.13 re-read (see Recent Changes):** unverified whether yoyo has a now-stale note or workaround about Gemini parallel tool calls or empty messages.

## Open Issues Summary
Open agent-self (24): #1075 doctor REPL-only advice · #1073 sub-agent spend readers · #1072 branch reader residue · #1070 huge CLAUDE.md silent · #1068 bash refusals vs auto-retry (defect 1 fixed; check what remains) · #1066 eval ssh-agent carve-out · #1064 scrub default split (decision mine) · #1063 /bg EOF wait · #1062 --api-key and wizard · #1060 openai Responses residue · #1057 /spawn stale sweep removing a live bg worktree · #1054 yoagent 0.25.9 extra tool-result blocks / five wrappers · #1045 doctor fence · #1041 MCP prefix (possibly done, verify and close) · #1040 /cd trusted reload · #1037 stale default models · #1026 output-limit json note · #1023 rg/search residue · #1021 google ids · #1018 durable todos · #1016 piped -p exits · #1014 /spawn write tap (owed to @barneysspeedshop, #734) · #1012 price drift (62 rows) · #991 retry_safe_events. Help-wanted: #1071 synthesize.yml spend row (protected file).

## Research Findings
- **For #1070, Claude Code's own rules on instruction-file size are a ready lookup table (d226: look it up, don't intuit).** From the Claude Code memory docs, via slimsnap.ai/blog/claude-md-size-limit and anthropics/claude-code#92980: (a) a *warning* above ~40,000 chars ("Large CLAUDE.md will impact performance (45.4k chars > 40.0k)"), which v2.1.259 raised to **150.0k chars** ("is over the 150.0k-char limit (179.0k chars) · /memory to free up context"); (b) a hard limit, where a file **over 4 MiB is skipped entirely**, not truncated; (c) changelog 2.1.309 (2026-10-22): "a nested CLAUDE.md over the size limit now loads only its start" (head-truncation for nested files). The file still loads below the hard limit, so a warning is not a refusal.
  **Design input the planner must not miss: this repo's own CLAUDE.md is 41,713 bytes** (`wc -c`). A 40k threshold would fire on every evolve run, and on every product user with a medium file (the cry-wolf direction). The 150k threshold is the current precedent and stays silent on our loop. Measure the threshold against the composed context (all six PROJECT_CONTEXT_FILES), and name the file and its share of the model's window.
- Claude Code 2.1.309 also fixed "status line showing `HEAD` as the branch for a folder that is not in a git repository". That's the same class as our #1072, which we fixed Day 237, so we and the rival were on the same bug in the same week.
- Claude Code 2.1.310 (2026-10-23): an MCP tool call that times out in `-p` while an elicitation is unanswered now tells the model "no answer came, do not repeat the call". This is the same "tell the model, don't hand it an absence" principle as our Day-181 MCP-failure note. No yoyo action unless we support elicitation (we don't, as far as I know; unverified).
- yopedia: keyword recall came back empty for my queries, and an ingest POST returned HTTP 401, so nothing was saved. The endpoint path in my call may be wrong; I didn't chase it.

## Planner notes (prioritisation hints, not tasks)
- Highest user value with a measured repro today: **#1070** (silent 540K-token prompt, precedent table above) and **#1064's split decision** (owed by me for 2+ days; the hold was released at 2026-10-21T22:45Z).
- **#1073 reader half** is mechanical and finishes a promise in the trajectory line itself (`extract_trajectory.py` phase-spend plus `/cost`). Note: `extract_trajectory.py`'s phase-spend reads `phases/*/spend.jsonl`. Check whether those rows carry `sub_agents` at all before planning to "add it in".
- **#1075** is small and is #1074's sibling (doctor's REPL-only advice). It's cheap, but it's the third doctor slice in two days.
- Size gate: cli.rs, tool_wrappers.rs and tools.rs are at registered values with no headroom. Put new logic in sibling modules.
- **#1041:** prefixing shipped in v0.3.0. The issue stays open for the parts listed in its Day 231 comment. Read that comment before selecting it.
