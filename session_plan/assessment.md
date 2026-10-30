# Assessment — Day 244 (10:44)

## Build Status
Pass — verified by the harness at session start (HEAD d9c2a4d3). Binary `yoyo v0.3.0 (d9c2a4d3)` built 10:45.
Probes: `yoyo -p "Reply with exactly: PONG"` → `PONG`, exit 0 (auto-watch line printed, skipped). `yoyo --yes doctor` → exit 2 (the 09:15 fix holds).

**Two size-gate hazards the planner MUST respect (from trajectory, confirmed with wc -l):**
- `src/cli.rs` is 7869 lines vs recorded 7774 in `tests/module_size.rs:229` (+95 drift). **5 more lines makes `cargo test` fatal → whole task reverted.** Any task touching cli.rs must either paste `("src/cli.rs", <new count>)` over the entry (with a Day-244 note), or put new code in a sibling module.
- `src/commands_risk_unhittable.rs` is 2036 lines, unlisted, 36 past the 2000 cap — **14 more lines makes it fatal.** Don't grow it; if touched, split or register it.

## Recent Changes (last 3 sessions)
- **09:15** — Task 1: a global flag before a subcommand (`yoyo --yes doctor`) no longer becomes a paid prompt; refused with exit 2, and the message says to drop the flag (not "move it", because `yoyo status --model M` ignores it and `yoyo run echo hi --model M` swallows it). Added seven missing subcommands to the list, plus a test that fails if a new one is left off. Task 2: Fixes #1104, the trajectory cap fits the whole block (about 15% headroom), and a cut names the dropped sections.
- **07:12** — shell `yoyo ast` exits 0/1/2 like `yoyo grep` (ast-grep missing → 2); bare `yoyo goal set` exits 2. Fixes #1071: synthesize counted in phase spend.
- **01:34** — Fixes #1103: unknown `--provider` with no base_url refused (exit 2, did-you-mean) instead of sending the real key to localhost:8080. Fixes #1066: `eval "$(ssh-agent)"` and similar well-known setup tools allowed under a deny list, when the inner command is a plain one-liner that passes the list.
- Day 243: hard_deny tracks quote state across lines (#1101; heredoc residue is #1102); #1086 slice for run/find/tree/undo/todo/goal; #991 stream-json keeps only the try that survived a retry; #1054 version-claim comments.
- 0 reverts in 14 days; 10/10 sessions 2/2 tasks.
- External: llm-wiki journal is still paused mid-migration (newest entry Apr 2026 era); nothing new.

## Source Architecture
156 files in src/, about 221k lines including tests (`wc -l src/*.rs src/*/*.rs`). Largest: cli.rs 7869, commands_risk.rs 6602, tool_wrappers.rs 5732, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924, commands_git.rs 3822.
Entry points: main.rs → cli::parse_args → dispatch_sub::try_dispatch_subcommand (shell subcommands) or REPL/prompt (prompt.rs); agent assembly in agent_builder.rs (build_agent, connect_external_servers, presets); tools in tools.rs and tool_wrappers.rs; guards in safety/ and hard_deny.rs.

## Self-Test Results
Shell-subcommand exit-code sweep in this repo (binary d9c2a4d3):
- `yoyo grep zzz src` → 1 "No matches found" ✓; `yoyo grep` bare → 2 but prints **`usage: /grep ...`** (REPL syntax on the shell; known #1086 residue).
- `yoyo docs` bare → 2, prints **`usage: /docs <crate> [item]`** (#1086 residue). `yoyo refs` → 2 "/refs is a REPL command" (fine, intentional).
- `yoyo help nosuchtopic` → 2; `yoyo status extra` → 2; `yoyo index --x` → 2; `yoyo tree --depth x` → 2. ✓
- `yoyo find zzzqqq` → 0 "No files matching" (same as find(1)/fd, which exit 0 on no match; defensible, not a bug).
- **NEW friction: `yoyo diff --bogus` → exit 0, prints "(no uncommitted changes)".** `parse_diff_args` in src/commands_git.rs (~line 231) has `_ if part.starts_with('-') => {} // ignore unknown flags`. So a typo like `yoyo diff --stagd` silently shows the *unstaged* diff (or "no changes") and exits 0: the user gets an answer to a question they didn't ask. Same class as #982 (closed) and the `index`/`status` refusals that already exist. No open ticket found (`gh issue list --search "diff unknown flag"`). Small and self-contained, and commands_git.rs isn't near the size gate. Caution: the REPL `/diff` shares the parser, so the near-miss rows are `--staged`, `--cached`, `--name-only`, `--stat`, `--explain`, `--functions`, plus a ref/path arg.

## Evolution History (last 5 runs)
evolve.yml: 4 success (2026-10-29 20:48, 10-30 01:31, 07:09, 09:13), plus the current run (10:41, in progress). No failures. The CI failure clusters in the trajectory (post_hook_feedback test, exit 101) are 2 days old and predate the SIGPIPE fix (Day 242 4e2b1909); CI has been green since.

## Capability Gaps
(See Research Findings.) Standing gaps vs Claude Code / Codex / Cursor: no first-class TUI (#215); MCP over HTTP/SSE missing (stdio only); no sub-agent spend in /cost (#1073); /cd into a trusted project doesn't load hooks/MCP/skills (#1040); CLAUDE_CODE_GAP.md header is 170 days stale (verified day 74), so it can't be trusted as a gap list without a re-read.

## Bugs / Friction Found
1. `yoyo diff <unknown-flag>` silently ignored, exit 0 (above). New.
2. #1086 residue still live: `yoyo grep`, `yoyo docs`, `yoyo def`, `yoyo outline` bare print `/verb` usage on the shell (12 open per the 07:22Z comment, minus whatever 09:15 touched; re-probe before selecting, and age the comment by sha per the Day 232 lesson).
3. Size-gate proximity (cli.rs 5 lines, commands_risk_unhittable.rs 14 lines): a hazard to the next task, not a bug to fix as a task (unless a split is chosen).
4. **NEW, a class (prompt mode): an explicit flag whose VALUE doesn't parse warns, then runs a PAID prompt on the default and exits 0.** Probed at d9c2a4d3 in an empty dir, each with `-p "Reply PONG"`: `--max-tokens abc` → "Invalid --max-tokens value 'abc', using default", exit 0, answered. Same shape (warning, PONG, exit 0) for `--temperature hot`, `--max-turns x`, `--context-window big`, `--thinking ultra` ("Unknown thinking level 'ultra', using 'medium'"), and `--bogusflag` ("Unknown flag ... ignored"). Who reaches this branch? Only someone who made a typo: nobody types `--max-tokens abc` meaning "use the default". So the d229 rule applies, and exit 0 plus a spend needs a positive argument, which nobody has written. Sibling doors already refuse: an unknown `--provider` (exit 2, #1103) and a global flag before a subcommand (exit 2, today 09:15). #1028 (closed) only made the CONFIG-file path warn like the flag path, so it treated the flag path as the spec (the "matches the sibling door" trap). No open ticket. The code is in `src/cli_config.rs:273` (resolve_numeric_setting pushes the warning), `cli::parse_thinking_level` and `cli::warn_unknown_flags`. **cli.rs is 5 lines from fatal**, so put logic and tests in cli_config.rs or a sibling *_tests.rs, and paste the cli.rs size entry if it grows. Product-safety question for the planner: unknown flags (`--bogusflag`) are partly a forward-compat question (a newer flag given to an older binary). Bad VALUES of known flags are not, so slice them first. Callers in my loop: `grep scripts/ .github/` for these flags found only scan_commitments.py's `--max-turns 1` (valid value), so the loop is unaffected.
5. Minor: `yoyo -p hi --base-url http://127.0.0.1:1` ends "Check your internet connection and that anthropic's API is reachable. You can retry with /retry." This names anthropic rather than the base URL the user gave, and gives REPL syntax (/retry) in -p mode. Exit 1, which is correct.
6. Subsystem concentration: dispatch took 4 of the last 8 self-driven diffs, so the self-driven slot should go elsewhere (diff/commands_git, an agent_builder item, or a never-forecast file: src/grep_status.rs, src/tools_search_flag_tests.rs).

## Open Issues Summary
agent-self backlog (open): #1102 hard_deny heredoc mention refused (carrier for #1101 residue); #1100 MCP servers outlive a panicking yoyo; #1094 write_file partial-read guard residue; #1086 shell verbs print REPL slash syntax (biggest active class); #1084 cost table reads low in two places (tiered session line, DeepSeek off-peak); #1073 sub-agent spend missing from usage/cost; #1070 multi-MB CLAUDE.md goes in silently; #1064 child processes inherit provider key (opt-in scrub; default split by population handed back to me by @yuanhao on Day 235, not decided since); #1060 openai wire residue; #1041 MCP prefix (Day 240 slice; the model-visible absence premise still unmeasured); #1040 /cd trust load; #1037/#1021 stale default models; #1026 output-limit cut in json modes exits 0 with no note; #1023 search residue; #1018 durable todos; #1014 /spawn write tap; #1012 price audit; #991 retry_safe_events.
agent-input: **#1096 (new, @JC351, 2026-10-28)**: a challenge to run weekly X outreach sessions (find posts, make concrete suggestions, track followers/stars). 0 comments, no reply from me yet. It touches social workflow and public identity, so per social learnings (#950) the owner (@yuanhao) decides whether yoyo posts on X as outreach. The right move is a reply in the issue naming that, not a code task. Also #991 (agent-input).

## Research Findings
Recall: yopedia already holds rival scans through Claude Code 2.1.312 and Codex rust-v0.165.0 (2026-10-27). New this session: Claude Code **2.1.313–2.1.314** (raw CHANGELOG.md), saved to yopedia as "Claude Code 2.1.313-2.1.314 delta vs yoyo d9c2a4d3".
- **"Fixed `claude -p` exiting with code 0 and no output when startup failed; now prints the error and exits 1."** yoyo holds this for hard startup failures (no key → 1, unreadable --system-file → 1, dead base URL → 1). The live gap is the softer sibling above: bad flag values exit 0 after a paid call. This is the strongest transferable item, and it's in a non-dispatch subsystem (cli_config), which matches the trajectory's concentration warning.
- `permissions.blockWritesOutsideWorkingDirectories`: Write/Edit refuse paths outside the working dirs in every mode. yoyo has an outside-READ fence (src/outside_read.rs, #879). Whether a write-side equivalent exists is not checked this session; verify before planning.
- PreCompact hook output capped at 10k chars. yoyo already caps hook output (`HOOK_TOOL_OUTPUT_MAX_CHARS = 1000`, src/hooks.rs:92, cut marked). No gap.
- Hooks matching Bash now also fire for shell commands another tool starts (Monitor). This is yoyo's "two doors" class: whether yoyo's bash hooks fire for /bg, /watch and /run commands is unverified.
- Blocked or not-run shell/MCP calls had been displayed as if they ran (display honesty for refusals). Parent no longer holds sub-agent tool results until turn end (memory).

## Planner notes (constraints, not tasks)
- Size gate: cli.rs +5 / commands_risk_unhittable.rs +14 to fatal. Paste the cli.rs entry in tests/module_size.rs if cli.rs grows (any growth, really, since we're at +95 of the 100-line grace).
- Concentration: avoid dispatch_sub.rs for the self-driven slot.
- #1096 (agent-input, @JC351): X outreach challenge, 0 comments. It touches public identity and outbound posting on another platform, so the honest response is a reply naming that @yuanhao owns that call (social lessons #950, d242 census). Not a code task.
- Re-probe #1086 members at HEAD before selecting (the 07:22Z comment is stamped 5ea96be3; run `git log 5ea96be3..HEAD --oneline --grep '#1086'`).
