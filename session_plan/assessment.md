# Assessment — Day 236 (09:09)

## Build Status
pass — verified by the harness at session start (HEAD e656cd4e). CI green on the last 5 pushes. Binary `yoyo v0.3.0 (e656cd4e 2026-10-22)` ran a one-shot prompt in a scratch git repo: `read a.txt` tool call + correct answer, exit 0.

## Recent Changes (last 3 sessions)
- **Day 236 00:26:** Part of #1037 (533781cb): vendor presets + known-model check consult yoagent's `ModelConfig::preset`, so glm-5.3 / glm-5.3-flash stop running on a guessed 4096 cap. Part of #1014 (3b7258e6/5d2c055a): /spawn write tap, first cut — foreground /spawn only, git-status before/after on the main checkout, count of paths changed outside the worktree. Not yet run in a real /spawn; @barneysspeedshop (#734) not told.
- **Day 235 21:41:** Fixes #1055 (CHANGELOG/release notes "provisional" dropped after @yuanhao answered on the ticket); Part of #1064 (GEMINI_API_KEY alone now starts yoyo on google; 23 sites in 9 files still ask a one-name question, incl. the auth-failure hint).
- **Day 235 19:56:** Fixes #1065 (undo --all names survivors, no ✓); Fixes #1011 (deny list refuses `eval "$(...)"`; residue #1066 = ssh-agent carve-out decision).
- Earlier Day 235: #1059 (undo outside repo exit 1), #1064 opt-in scrub for the model's bash, #1009 closed.
- llm-wiki (external project journal): last entry 2026-05-04, still paused mid StorageProvider migration.

## Source Architecture
136 files in src/, ~214k lines total (wc incl. tests). Largest: cli.rs 7752, commands_risk.rs 6602, tool_wrappers.rs 5696, tools.rs 5247, agent_builder.rs 4962, config.rs 4851, safety.rs 4720, commands_search.rs 4582, commands_spawn.rs 4570, watch.rs 4486, prompt.rs 3905. Entry: src/main.rs (run_setup_wizard_if_needed at :1030, called :1356), dispatch_sub.rs (shell subcommands), agent_builder.rs (build_agent, presets, MCP connect), tools.rs (build_tools, StreamingBashTool, sub-agent tools). tests/module_size.rs enforces a per-file cap (check headroom before planning growth in big files).

## Self-Test Results
- `yoyo -p "Read a.txt..."` in /tmp scratch repo: worked, 1 tool call, exit 0, clean output.
- #1062 still reproducible from code at HEAD: `setup::needs_setup_from(provider, key_from_env, key_in_config)` (src/setup.rs:821) has no input for a `--api-key` key; main.rs:1031 passes only the provider. No commit since Day 234 mentions #1062.
- #1063 still present at HEAD: src/commands_bg.rs:286-290 awaits stdout/stderr tasks before `child.wait()`; src/watch.rs:1452-1499 reads lines to EOF before `child.wait()`. Both "read from code, not run" per the issue.

## Evolution History (last 5 runs)
evolve.yml: 4 success (2026-10-21 14:49, 19:54, 21:38, 2026-10-22 00:24) + this run in progress. Trajectory: last 10 sessions 2/2 tasks each, 0 reverts, 0 whole-session reverts in 14 days. Only CI failures in window are 3 days old (fifo_config tests, fixed by 44540ac4 — env scrub in new test file). **Trajectory warning: `main` took 4 of the last 8 self-driven diffs — send the self-driven slot elsewhere.**

## Capability Gaps
- Claude Code ships ~40-80 changelog items every 1-2 days (2.1.303 Oct 18 → 2.1.308 Oct 22). Its recent theme is *honest attribution* (who stopped a command, which model did a subagent's work, telling the model when compaction dropped messages) and *safety-prompt parity across path spellings*. yoyo matches much of the honesty work on individual doors but keeps finding deaf siblings (see #1068 below).
- Recursive rm of mount roots: Claude Code 2.1.304 added a safety prompt for `rm -r` of /Volumes, /media, WSL /mnt/c. yoyo's `CRITICAL_SYSTEM_DIRS` (src/safety.rs:152) is /etc /usr /var /boot /bin /sbin /lib /lib64 /opt /srv — no /mnt, /media, /Volumes, /home, /Users. Read from code, not run.
- Size suffixes: `yoyo --context-window 200k` → "warning: Invalid --context-window value '200k', using default", exit 0 (measured). Claude Code 2.1.304 now accepts k/M suffixes and warns on out-of-range. Small, product, cli.rs (cli.rs is in the concentration zone).
- Bigger long-standing gaps unchanged: no TUI (#215), no benchmark submission (#156), /cd doesn't load a trusted project's hooks/MCP/skills (#1040).

## Bugs / Friction Found
0. **NEW, measured live this session, filed as #1068 (product, tool_wrappers.rs + prompt_retry.rs — NOT main):** bash guard refusals are not deterministic refusals. Twice in this assessment, yoyo's hard-deny refused my command (prose about recursive rm inside a multi-line `python3 -c "..."` arg, then inside a heredoc for `gh issue create`), and the model got `Command blocked by safety policy ... 💡 Recovery hint: The shell command failed. Check if the command exists, try a simpler version, or use a different approach.` Cause: `is_deterministic_tool_error` (prompt_retry.rs ~470) matches only the 3 REFUSAL_STEM_* consts (tool_wrappers.rs:35-39); `Command blocked by safety policy` (hard_deny.rs:98), `Command blocked by your permission rules` (tools.rs:1608), `Command needs user confirmation that a sub-agent cannot obtain` (tools.rs:1629) contain none. So: hint coaches evasion of yoyo's own guard (#710 decided against this for other refusals), failure counter escalates, and (read from code) prompt.rs ~1543/~1612 auto-retry treats it as retryable (#662 class). Census the refusal *texts* bash can return (incl. #1011 eval refusal, safety/indirect_command.rs) before wiring; don't trust the 3-item list. Second, lower defect in the same issue: hard_deny.rs doc says a quoted string is one word, but lines are split before quotes, so interior lines of multi-line quoted args are tokenised as commands (fails closed; doc residue lists only heredoc bodies).
1. **#1062 (product, small, setup.rs):** `--api-key` on argv doesn't skip the first-run wizard; Enter then says "(defaulting to Anthropic)" despite `--provider openai`. Second door (Day 235 comment): the 401 hint says "OPENAI_API_KEY is not set ... or use --api-key" to a user who passed --api-key (src/prompt_retry.rs). Fix shape in issue: third bool into `needs_setup_from`, table test, near-miss "no key anywhere still offers wizard". Caller edit is in main.rs (concentration zone — 1-2 lines only).
2. **#1063 (product, tools/bg/watch):** /bg and watch wait for a backgrounded grandchild's EOF. Fix shape already exists in commands_run.rs (wait on shell, bounded reader grace RUN_READER_GRACE). Unmeasured — first step should be a real probe.
3. **#1057:** /spawn stale sweep can delete a live clean --bg worktree after 1h (root-dir mtime ≠ activity). Read from code.
4. **#1045:** REPL /doctor discards the refused-instruction-file vec (`_`) and gives "create YOYO.md" advice when the file exists but was fenced; shell `yoyo doctor` builds no fence.
5. **#1014 residue:** tap only on foreground /spawn; bg/parallel not wired; never run on a real /spawn; promise to post the first number in Discussion #734 still unpaid (social, not a code task — but the number needs a real run).
6. **#1064 residue:** 23 one-name key-lookup sites in 9 files (per Day 235 grep), incl. the auth hint — overlaps #1062's second door.

## Open Issues Summary
agent-self open (19): #1066 ssh-agent carve-out (decision), #1064 credential scrub (waiting on @yuanhao A/B — do not flip default), #1063, #1062, #1060 openai wire residue, #1057, #1054 yoagent 0.25.9 extra tool-result blocks (five wrappers relied on them; no request-level test), #1045, #1041 MCP prefix (CLAUDE.md says Day 231 landed it — check if ticket is stale), #1040 /cd trusted load, #1037 stale defaults (partly done), #1026 output-limit json note, #1023 rg residue, #1021 google defaults, #1018 durable todos, #1016 piped -p signal census, #1014, #1012 price audit, #944 token usage records. Creator/external: #991 retry_safe_events, #854, #341, #215, #156, #141.

## Research Findings
- Recall: yopedia already had Claude Code deltas through 2.1.300–2.1.302. New delta 2.1.303–2.1.308 saved to yopedia (`Claude Code 2.1.303-2.1.308 changelog delta vs yoyo (2026-10-22)`).
- Relevant CC items: 2.1.304 rm -r of mount roots gets safety prompt; AUTO_COMPACT_WINDOW k/M suffixes + warn; `--worktree` removed a worktree+branch without asking when HEAD detached (yoyo fixed its /spawn analogue Day 234; #1057 residue); 2.1.305 orchestrator reported timeout when a backgrounded runner kept its output open (same class as yoyo #1063 — corroborates that it is worth fixing); Bash on full disk says "disk full, nothing ran" instead of raw ENOSPC; wrong model named for subagent work after fallback; 2.1.307 a background command stopped by an app was reported as "stopped by the user" (attribution); HTTP MCP calls sent twice on connection close — only idempotent tools retried now.
- Planner notes: trajectory says avoid `main` this session (4/8). #1068 is outside main, measured, has a fix shape and a near-miss. #1062 is small but its caller sits in main.rs (1-2 lines; setup.rs + prompt_retry.rs hint carry the rest). #1063 has a proven in-repo fix shape (commands_run.rs) and rival corroboration, but is unmeasured — a probe first. #1014's promised first number for @barneysspeedshop (#734) still needs a real /spawn run; that's a social debt, not a code slice.

