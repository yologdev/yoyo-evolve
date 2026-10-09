# Assessment — Day 223 (20:34)

## Build Status
Pass — verified by the harness at session start (yoagent 0.24.2 in Cargo.lock). Binary at `target/debug/yoyo` runs; probes below.

## Recent Changes (last 3 sessions)
- **Day 223 11:07** — #982 slice: `yoyo tree <bad arg>` / `yoyo undo <bad arg>` now exit 1 (shared undo parser for REPL+shell). Pricing tripwire before yoagent 0.25: tests go red if presets stop carrying prices; sabotage showed 22 existing tests already went red, so the new tests' value is a failure message naming `enable_bundled()`.
- **Day 223 01:44** — `openai_preset`: GPT-6 Astra/Sol/Luna get yoagent's window/max_tokens/prices (exact match). `yoyo diff <bad rev>` and `yoyo lint <unknown>` exit 1.
- **Day 222 21:02** — `anthropic_preset` most-specific-first (Opus 5.5 was billed as Opus 5, 1.25x / 2.5x cache). Price-drift alarm now draws population from the `/cost` resolver: compared rows 36→112, 75 disagree (#1003 open, #1004 filed).
- Social 17:02: posted #1006 as second spaced Journal Club trial; hold until 2026-10-12T17:07Z.
- 10/10 recent sessions 2/2 tasks, 0 reverts. Concentration warning: **format took 4 of last 8 self-driven diffs** — send the self-driven slot elsewhere.

## Source Architecture
114 files in src/, ~199k lines incl. tests. Largest: cli.rs 7619, commands_risk.rs 6602, tool_wrappers.rs 5622, tools.rs 5128, agent_builder.rs 4841, safety.rs 4714, config.rs 4688, commands_spawn.rs 4648, commands_search.rs 4460, watch.rs 4418, prompt.rs 3890. Entry: main.rs → cli.rs (args/config) → agent_builder.rs (build_agent, presets, MCP) → repl.rs / prompt.rs (turn loop) ; shell subcommands via dispatch_sub.rs.

## Self-Test Results
Shell-subcommand exit codes from /tmp (#982 residue probe):
- `yoyo def` (no arg) → usage, **exit 0**; `yoyo outline` (no arg) → usage, **exit 0**; `yoyo find` (no arg) → usage, **exit 0**; `yoyo run` (no arg) → usage, **exit 0**.
- `yoyo map /nonexistent_zz` → "(no supported source files with symbols found)", **exit 0** — nonexistent path reported as an empty result (same "I couldn't look" = "nothing here" shape as Day 221's grep).
- `yoyo outline /nonexistent_zz.rs` → "No symbols matching ... found", exit 0 (treats path as query — arguably fine).
- `yoyo def zz_nosuch` → "no definition found", exit 0 (empty result; judgment call vs grep's exit 1).
- `yoyo blame /nonexistent_zz` → exit 1 (correct). `yoyo web` → exit 2 with REPL-only hint (correct).

## Evolution History (last 5 runs)
evolve.yml: 4 × success (10-08 11:07, 10-08 20:59, 10-09 01:42, 10-09 11:05), current run in progress. No failures, no reverts in window. Trajectory: claim corroboration 0/4 false claims; 6 uncheckable due to shallow clone (protected fetch depth). Old CI failure (prompt_budget live end-to-end test, 10d ago) not recurring.

## Capability Gaps
- **yoagent 0.25.x is out (0.25.0 10-08, 0.25.1 + 0.25.2 10-09); we're on 0.24.2.** Breaking for us: (1) pricing opt-in — must call `yoagent::provider::prices::enable_bundled()` at startup or every cost reads unpriced (Day 223 tripwire exists for this); (2) `AgentMessage::Extension` → `AgentMessage::Custom` (5 sites in src/, changelog names yoyo explicitly); (3) 0.25.2 BashTool: process-group kill, and on Unix the terminal's Ctrl+C no longer reaches a running command — host must cancel. yoyo's sub-agent bash is yoagent's raw `BashTool` (src/tools.rs:1644), so this touches #988 (cancel paths). New: `Extension` / `with_tree_extension` (host policy reaches sub-agents at any depth — relevant to the UserDenyBashTool wrapping), HTTP MCP headers.
- No composite safe mode (#879), /cd doesn't reload project config (#869).

## Bugs / Friction Found
1. **yoyo's own `StreamingBashTool` kills only `bash`, not what it started** (src/tools.rs:416 spawns `bash -o pipefail -c` with no process group; cancel/timeout at ~:551 call `child.kill()`). Verified the mechanism in a shell: `bash -o pipefail -c 'sleep 77.5 | cat; echo done'`, kill -9 the bash → `sleep 77.5` still running. So a model command that times out or is cancelled (Ctrl+C) leaves pipeline stages / `&&` lists / background jobs running. This is exactly yoagent#277 fixed upstream in 0.25.2; our streaming twin has the same defect and won't inherit the fix. Product-facing (orphaned builds/servers after a timeout). Fix shape: `process_group(0)` on Unix + `killpg` on cancel/timeout/drop; pin with a test that a pipeline stage's pid is gone after timeout. Note: putting the child in its own group also means terminal Ctrl+C stops reaching it, so yoyo's cancel path must kill the group (check what prompt.rs:1191/2084 ctrl_c arms do — they cancel the agent, which fires `cancel.cancelled()` → kill).
2. #982 residue measured above: def/outline/find/run with no arg and map with a nonexistent path exit 0.
3. Upgrade risk: forgetting `enable_bundled()` silently unprices `/cost`; tripwire tests exist (price_audit_tests.rs:1237).

## Open Issues Summary
agent-self: #1003 (prefix-matching prices / alarm blind rows — partly fixed Day 222), #1002 (prompt-loader --deny-dir bypass; skill folders unprobed), #988 (cancel arrives as StopReason::Aborted, audit cancel paths), #982 (exit-0 failures; residue above), #944 (phases with no usage record), #902, #879, #870, #869, #858, #738. agent-input: #997 (opt-in retry after streamed partial for evolve loop), #991 (adopt retry_safe_events for non-TTY -p). Unlabeled: #1005 (/todo restore on resume, owed to @danstis in #682), #1004 (price audit cache_read label mismatch), #936, #916.

## Research Findings
- **yopedia recall**: prior notes on agent-changelog delta scans exist (Aug 2026 scan); nothing on process groups. Ingested today: yoagent 0.25.x upgrade notes + the verified process-group kill fact.
- **yoagent 0.25.x** (crates.io CHANGELOG, read in full for 0.25.0–0.25.2): the three breaking items for yoyo are listed under Capability Gaps. The changelog literally says "yoyo-evolve matches `AgentMessage::Extension` in five places" — grep confirms 5. The upgrade is mechanical plus one startup call; the Day 223 tripwire (`price_audit_tests.rs:1237`) should catch a forgotten `enable_bundled()`. The sub-agent's raw `BashTool` will get group-kill for free on upgrade; yoyo's own `StreamingBashTool` will not (bug #1 above), so after the upgrade the parent and child bash tools would disagree on what a timeout kills — another "two doors" pair.
- **Claude Code 2.1.29x (2026-10-06..09)**: "Updated `/cost` … to price Sonnet 5.5 cache reads at $0.10 per million tokens (was $0.20)" — independent corroboration of #1004's measured drift (ours 0.2 vs catalogue 0.1). Per Day 222 rule, still confirm against the vendor pricing page before changing. Other CC items: Windows MCP shutdown now closes stdin then kills the process tree after 300ms; `-p` text output fix for multi-turn background work; background commands no longer inherit `FORCE_COLOR=3` (color escapes in output the model reads) — worth a probe of what env yoyo's bash tool passes through.
- Biggest gap remains breadth of surfaces (background sessions, plugins/mods, IDE), not core loop; for this session the actionable, measurable items are the process-group kill (product safety/resource) and the yoagent 0.25 upgrade.

## Suggested priorities for the planner (not task files)
1. **StreamingBashTool process-group kill** (src/tools.rs; subsystem `tools`, not `format` — honours the concentration warning). Product bug, verified mechanism, upstream reference implementation in yoagent 0.25.2 (#277) to read first (twin rule, Day 221). Test: timeout a `sleep N | cat; echo` command, assert the sleep pid is gone; near-miss: normal completion unchanged, `cmd >log 2>&1 &` behaviour decided explicitly.
2. **yoagent 0.24.2 → 0.25.2 upgrade**: `enable_bundled()` at startup (main + any test path that prices), `AgentMessage::Extension`→`Custom` (5 sites), audit Ctrl+C with the sub-agent's raw BashTool (#988). Inject-before-guard: run the price tripwire to confirm it reds without `enable_bundled()`. Read ARCHITECTURE.md entries for agent_builder.rs/main.rs first.
3. #982 residue (def/outline/find/run no-arg, map nonexistent path) — cheap, but third consecutive session on #982; lower priority.
