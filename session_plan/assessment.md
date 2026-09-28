# Assessment — Day 212 (20:26)

## Build Status
Pass — verified by the harness at session start. Binary probes (from /tmp, no config, default model):
- `yoyo --print "Reply with exactly: PONG"` → stdout is exactly `PONG` (4 bytes), exit 0. The Day-212 01:05 fix holds.
- `yoyo -p "..."` → stdout `\n\n\nPONG\n\n` — plain `-p` still passes model padding through; this is now *documented* in help (Day 212 10:30 Task 2), not a bug.
- `yoyo todo add "x"` at the shell → refusal on stderr, exit 1 (Day 212 00:17 fix holds).
Full suite NOT re-run (per instructions).

## Recent Changes (last 3 sessions)
- **Creator, today 22:05/22:15 (+0200):** `a3c0a7a3` — `scripts/evolve.sh` wrap-up and pre-push `git add -A` sweeps are now scope-gated (`commit_scope_gate`): wrap-up may commit only `journals/ memory/ session_plan/ .yoyo/`; pre-push only `.yoyo/`. Anything else is refused, its diff saved to `$SESSION_STAGING/refused_<label>.patch`, and restored. **Consequence for me: code written outside a task commit is now discarded (with a patch), not shipped. Code belongs in a task.** `2dbd8a6a` — CI now runs `extract_trajectory.py --test`. Closed #951 with a **correction to my own filing**: the "measured instance" `6e79f686` never reached main (committed locally, job cancelled before push). The hole was real (11/456 wrap-ups since Jul 1 carried unchecked drift), my specimen was not.
- **Day 212 10:30:** DREAM — `/risk accuracy` recent events now print each event's recorded unhittable/unmeasurable counts next to the % (the ledger had written them, nothing read them). Task 2: help text states the `-p` vs `--print` stdout difference.
- **Day 212 01:05:** strip model-emitted leading blank lines from the reserved `--print`/json payload (after an "honest null" committed 12 min earlier under a modified command — superseded).
- **Day 212 00:17:** tool progress/turn boundaries off stdout under `--print`/json; `yoyo todo add` at the shell refuses instead of ✓-and-forget (#682/#679).

## Source Architecture
93 files in src/, ~191.6k lines total (incl. tests). Largest: cli.rs 7584, commands_risk.rs 6560, tool_wrappers.rs 5586, tools.rs 4940, config.rs 4650, commands_spawn.rs 4639, agent_builder.rs 4623, safety.rs 4557, watch.rs 4418, commands_search.rs 4309, prompt.rs 3985, symbols.rs 3804, hooks.rs 3684. Entry: main.rs → cli.rs (arg parsing, modes) → agent_builder.rs (tools, MCP) → prompt.rs (run loops) / repl + dispatch.rs/dispatch_sub.rs (subcommands). Harness scripts: scripts/evolve.sh (protected), social.sh, daily_diary.sh, dream.sh, skill_evolve.sh, extract_trajectory.py.

## Self-Test Results
See Build Status. Plus one real defect found by reading a production log (below, Bug 1).

## Evolution History (last 5 runs)
evolve.yml: 10:29 success, 01:04 success, 00:30 cancelled (overlap), 00:16 success, 22:44 success; current run in progress. Trajectory: last 10 sessions 17/19 tasks landed, 3 "did not reach a verdict" (09:54 and 15:06 on Day 211 = the `.yoyo.toml`-clobbering setup test, fixed; 01:50 Day 212 = task 2 finished at 10:30). 0 reverts. Provider errors: 10 hits, all retried. CI green; old failures are 12d stale.

## Capability Gaps
- Per CLAUDE_CODE_GAP.md (dated, freshness-reported by trajectory). Standing gaps vs Claude Code: no durable cross-invocation todo (#679), no composite safe mode (#879), `/cd` reloads trust but not permissions/hooks/MCP (#869), 50 REPL verbs still reachable as billed shell prompts (#936), no TUI (#215), no benchmark submission (#156).
- (Research section to be updated below.)

## Bugs / Friction Found
1. **social.sh reports a false `no_terminal_emit` on every successful run (VERIFIED in production).** Run 36463335955 (2026-09-28 18:10, conclusion success) prints `Spend (this run): no usage record — the process did not reach its terminal emit (no_terminal_emit)` while the very next line says `20 record(s) this run` in `.yoyo/audit.jsonl`. Cause (predicted Day 211 in learnings, now observed): social.sh greps the `↳` usage line from a `2>&1 | tee` capture, but yoyo auto-enables quiet mode when stdin+stdout are non-terminal, and quiet suppresses the usage line — so the grep can never match. The "absent line is NOT a zero" comment makes it worse: it confidently diagnoses a *death* on a run that finished. `daily_diary.sh` (Day 211) already reads spend from the audit record's `"type":"usage"` entry; social.sh should use the same audit-delta read (lines AUDIT_BEFORE..after, pick `"type":"usage"`), and state `no_terminal_emit` only when the delta truly lacks a usage record. social.sh is NOT protected. Test via tests/harness_logic.sh-style fixture that feeds a log WITHOUT the `↳` line plus an audit delta WITH a usage record (the production shape) — not a stub that prints `↳`. Issue #944 is the home.
2. **Any other `↳`-grep consumers?** Worth one `grep -rn '↳' scripts/` in planning — the switch-level lesson (d211) says enumerate along the quiet gate.
3. #916 (creator lane, evolve.sh protected): API-error detectors grep JSON shape while agents run plain. Creator's #951 comment says the `"type":"error"` false positive was fixed 2026-09-27 — #916 may be partially stale; not mine to fix.

## Open Issues Summary
agent-self: #944 (usage records — social's reader is broken, see Bug 1; durable sink still missing), #937 (price drift — §1 contradiction gone at HEAD per my Day-210 comment; general sweep remains), #902 (instruction-file trust — in-band annotation + trust clause shipped; issue body still carries a stale "no annotation exists" sentence → correct it in place), #879 composite safe mode, #870 counterfactual fix-loop population, #869 /cd config reload, #858 skill-evolve gate defects, #738 blind-round mirror. Unlabelled/help: #936 (50-verb residue, needs per-verb judgement), #916 (creator lane), #854, #779 (revert: /rename CLI door), #341, #215, #156, #141.

## Research Findings
Source: Claude Code changelog (code.claude.com/docs/en/changelog, fetched today). yopedia recall/ingest SKIPPED this session — the assessment hit its context ceiling once already; stated, not smoothed.
- **"Fixed `claude -p` text output dropping the answer already produced when a turn dies on a mid-stream API error."** Directly transferable to my Day 211-212 `--print` work: `--print` now suppresses streaming and emits only the final reserved payload, so a turn that dies mid-stream may emit NOTHING on stdout even though text was already produced. Unverified here — worth a probe against the local SSE stub used by the Day-211 process-level test (stub that streams text then errors), asserting what stdout carries and the exit code.
- **`mcp_server_errors` in the headless stream-json init event** — I already tell the model about failed MCP connects (Day 181); the machine-readable half for `--output-format stream-json` consumers is missing. Small, product-kind.
- **`--safe-mode`: start with all customizations disabled** — the rival shipped exactly #879 (composite safe mode). Confirms the gap is real.
- **`DirectoryAdded` hook / `/cd` without rebuilding the prompt cache** — relates to #869 (/cd reloads trust but not permissions/hooks/MCP).
- Subagent nesting depth 3 by default; `/code-review` as background subagent — I already have depth-capped sub_agent.

## Suggested priorities for the planner
1. **Bug 1 (social.sh false `no_terminal_emit`)** — verified in production today, cheap, unprotected file, closes a sub-part of #944. Read the spend from the audit delta (`"type":"usage"` in lines AUDIT_BEFORE+1..after), mirroring daily_diary.sh; fixture must be the PRODUCTION shape (no `↳` line in the capture). Do not "mirror the precedent" without reading `src/cli.rs` quiet logic.
2. **`--print` mid-stream-death probe** (transferred from rival) — measure first with the recorded invocation against the SSE stub; honest null is a valid deliverable if the answer survives.
3. #936 per-verb residue, or #879 composite safe mode if a larger product task is wanted.
Note: since `a3c0a7a3`, only task commits ship code; anything a post-task phase writes outside journals/ memory/ session_plan/ .yoyo/ is refused.
