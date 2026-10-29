# Assessment — Day 243 (20:53)

## Build Status
Pass — verified by the harness at session start on HEAD 1f3dd58b. `./target/debug/yoyo --version` → `yoyo v0.3.0 (1f3dd58b 2026-10-29)`, so the binary probed below IS HEAD (not a stale build; d213/d242 trap checked). `yoyo -p "say PONG only"` from an empty temp dir → `PONG`, exit 0, model claude-opus-5-5. Last 5 CI runs on main: all success. Last 4 completed evolve runs: all success (current run in progress).

## Recent Changes (last 3 sessions)
- **Day 243 10:45** — Part of #1086 slice 1+2: `commands_dev::command_form(surface, verb)` helper; `yoyo run/find/tree/undo` print shell form (9450f16a, f09340a9). Fixes #1068: hard_deny doc stops overclaiming quote handling across lines; fail-closed multi-line case pinned (36ba2832). The decision itself filed as #1101.
- **Day 243 01:33** — Part of #991 item 3: stream-json collected text kept only the surviving attempt (ea02d62d) — then a correction on #991 admitting this went against @yuanhao's "otherwise drop it"; A/B ask posted 02:44Z, **"Nothing gets touched until you answer"** (no answer yet as of 20:53; his measured reply spread is ~2h–92h). Fixes #1054: four multi-block tool-result sites marked with yoagent version markers.
- **Day 242 (×4 sessions)** — #1099 close live MCP servers on every exit door (residue #1100: panic path); #1098 MCP servers in own process group; #1097/#1057 spawn worktree locks/sweep; #1062 --api-key skips wizard; #1045 doctor names fenced CLAUDE.md; #1088 `yoyo watch` refuses; #1083 doctor curl row; SIGPIPE hook-stdin fix (4e2b1909); Gemini promo dated pricing (#1012).
- Trajectory: 10/10 sessions 2/2 tasks, 0 reverts in 14 days. Subsystem concentration spread (main 3, cli 2, dev 2, dispatch 2, mcp 2).

## Source Architecture
175 .rs files under src/ (~220k lines incl. sub-dirs). Largest: cli.rs 7780, commands_risk.rs 6602, tool_wrappers.rs 5732, tools.rs 5268, agent_builder.rs 4954, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924 (over cap per #991 note), commands_git.rs 3822. Entry: main.rs (1552) → cli::parse_args → agent_builder::build_agent → repl.rs / prompt.rs; shell subcommands via dispatch_sub.rs::try_dispatch_subcommand. Size gate: tests/module_size.rs (grandfathered register with grace band; cli.rs registered at 7774, now 7780 — inside grace).

## Self-Test Results
- `-p` round trip: works.
- **#1086 residue probe re-run at HEAD** (empty dir, HOME=/nonexistent, bare + `zz1 zz2 zz3`, count lines matching `/<verb>\b`): grep 10, todo 11, goal 9, map 2, index 2, blame 1, lint 1, risk 1, skill 1 → **9 verbs still print slash syntax**, identical to the 12:23Z ticket comment (so the ticket is current; no fix landed since). Plus `yoyo help doctor` line 14 still says "Curl — needed for /docs and /web" on the shell (the d242 residue; on #1086's list).
- hard_deny: read `match_at_depth` (src/hard_deny.rs:122). Lines are split via `joined.lines()` before `split_line` tracks quotes, and an unbalanced line sets `needs_desugar`, which strips ALL quotes from the whole command and rescans — that is the mechanism behind #1101's 5 recorded self-refusals.

## Evolution History (last 5 runs)
evolve.yml: 4 success (10-28 17:06, 10-28 20:44, 10-29 01:29, 10-29 10:41) + current in progress. CI: 5/5 success. Trajectory's recurring CI error (`post_hook_feedback_reaches_the_model_as_its_own_tool_result_block`, exit 101) is ~1d old and predates the SIGPIPE fix 4e2b1909; CI green since. No provider errors in audit logs. No reverts.

## Capability Gaps
See Research Findings for the fresh Claude Code delta. Carry-over vs Claude Code: no rich TUI (#215), MCP over HTTP/SSE absent (stdio only), no benchmark submission (#156). Product-hygiene gaps that ARE in my backlog and measured: shell/REPL surface confusion (#1086, 9 verbs), stale default models (#1037/#1021), credential inheritance by children (#1064, default undecided — @yuanhao handed it back Day 235, so it's mine).

## Bugs / Friction Found
1. **#1086 residue — 9 verbs** print `/verb` on the shell (counts above). The helper + table-test pattern exists (`command_form`, `shell_usage_names_the_shell_form_not_the_slash_form` in tests/integration.rs, one row per verb). Ticket comment 11:38Z documents the drafted-and-discarded shape for map/index/grep/blame. Cheapest high-count members: grep (10), todo (11), goal (9). Also `yoyo help doctor`.
2. **#1101 hard_deny cross-line quotes** — my own default is A (track quote state across the joined command; unquoted newline separates commands, quoted newline stays in the word). Self-filed, agent-self, no owner ask outstanding → decision is mine. 5 recorded refusals of my own prose in 26 days. Plan must carry the two near-miss rows the issue specifies (unquoted newline + pattern must refuse; unbalanced quote closing on next line + pattern must refuse) plus d235's "removed normaliser" rows. Security-sensitive: run inverse probes (d217).
3. **#991 item 3 is HELD** — do not touch stream-json collected_text until @yuanhao answers the 02:44Z A/B. Items 4a/4b on #991 are unaffected (verify their state before selecting).
4. **#1100** MCP servers outlive a panicking yoyo (panic hook / Drop path) — open, no comments since filing.

## Open Issues Summary (agent-self backlog)
#1101 hard_deny cross-line quotes (decide; default A) · #1100 MCP panic path · #1094 partial-read guard residue (item 2 done 813a7289; never-read overwrite + process-wide store remain) · #1086 shell slash syntax (9 verbs + help doctor) · #1084 cost rows reading low (tiered session line, DeepSeek off-peak) · #1073 sub-agent spend (/cost done aafc072e; usage-record half?) · #1071 synthesize spend row (protected file, help wanted) · #1070 oversized CLAUDE.md (first slice 150KB warning) · #1066 eval ssh-agent carve-out · #1064 credential scrub default (mine to decide) · #1060 openai wire residue · #1041 MCP prefix (landed Day 231/240; check what remains) · #1040 /cd trusted loads · #1037/#1021 stale defaults · #1026 output-limit cut in json modes · #1023 search residue · #1018 durable todos · #1014 spawn write tap · #1012 price drift (22 rows) · #991 retry (item 3 held on owner).
agent-input: #1096 outreach challenge (social/X — not an evolve-src task), #991.

## Research Findings
Recall: yopedia already holds Claude Code deltas through v2.1.310 (scan Day 238). New this session: **Claude Code v2.1.311–v2.1.312** (CHANGELOG.md read 2026-10-29; ingested to yopedia). Transferable items, each checked against my tree / yoagent-0.25.16 source:
1. **"Fixed retrying requests the API marked not to retry"** (`x-should-retry: false`). `grep -rni should.retry` over yoagent-0.25.16 → **0 hits** (it parses `retry-after` only, e.g. provider/bedrock.rs:559). yoyo src → 0 hits. This is a yoagent gap → candidate `--label upstream` issue, not a src task.
2. **json/stream-json now escape DEL, C1 controls and Unicode format chars as `\uXXXX`.** **Already at parity, which corrects my first draft.** The draft said yoyo writes them raw. It was inferred from serde_json, which does write U+007F/U+0085/U+202E raw (measured with a scratch crate: only ESC came out as `\u001b`). But yoyo post-processes its output with `prompt::escape_unsafe_json_chars` (closed #1092): main.rs:301 (`--output json`), stream_session_restored.rs:55, stream_external_servers.rs:37. **Not a task.**
3. **Bash tool description falsely claimed cwd persists between calls; env block named a shell commands don't run in.** Class = describing one's own environment falsely, which is my #1086 class. yoagent's bash description (bash.rs:124) makes no cwd claim — not affected; my system prompt's environment claims not audited this session.
4. Timeout → output-so-far + "still running"; yoagent bash already keeps captures across the timeout race (bash.rs ~226) — likely parity, unverified.
5. Interrupt not taking effect while an MCP server hasn't answered; sub-agents missing MCP tools from late-connecting servers; hook concurrency cap (24/session); permission-rule startup warnings once-per-user.

Biggest structural gaps vs Claude Code remain the ones not in this delta: HTTP/SSE MCP transport, rich TUI (#215), benchmarks (#156).

## Suggested priorities for the planner (my read, not binding)
- **#1086 next slice** — 9 verbs measured at HEAD; pattern + test table exist; grep/todo/goal are 30 of the ~38 hit lines. Pure product hygiene, low gate risk. Include `yoyo help doctor` or say why not. Title by class predicate, not a verb list (d232), unless each verb is probed at plan time (probe loop is in this file).
- **#1101 decision A** — my own default, no owner ask pending → mine to ship. Security guard: needs the issue's two must-still-refuse rows, inverse probes, and a positive control. Do NOT bundle with anything else (d230).
- Do NOT touch #991 item 3 (held on @yuanhao, 02:44Z ask). #1064 default is mine since Day 235 (he handed it back) — still undecided, aging.
- (Dropped: the json escaping candidate. Already fixed by #1092, see Research item 2.)
