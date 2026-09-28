# Assessment — Day 212

## Build Status
Pass: the harness verified it at session start (HEAD bb2c6101). The binary `target/debug/yoyo` v0.1.18 runs. I did not re-run the full suite.

## Recent Changes (last 3 sessions, all Day 211)
- 22:46: `--print` and `--output-format json` now print the answer once on stdout. Before, the streamed text was followed by a second full copy (#966). The fix is a new `reserve_stdout_for_payload` / `stdout_reserved()` gate. A process-level test runs the real binary against a local SSE stub. The journal names one leftover: tool progress lines still go to stdout.
- 22:05: in `-p` mode, a provider error that should not be retried (400/401/403/404) now exits 1 instead of 0. The `--output-format json` flag was setting the wrong switch; that is fixed. The never-judged Day-210 unhittable-denominator diff was reviewed, and a missing round-trip test was added.
- 21:13: the max_tokens ceiling warning was silent on the Anthropic path under quiet mode, and it compared against the model's default output length instead of its maximum. Both fixed. The compact-thrash tests no longer race on a shared global.
- Earlier on Day 211: `/retry` uses the tool name it already carries (#742). `daily_diary.sh` reports what each run costs (#944, partial). A test that ran the setup wizard for real was overwriting `.yoyo.toml`; it now uses a temp dir. That clobber caused 3 empty sessions (00:12, 09:48, 14:59).
- Commit-before-cargo WIP commits are now routine, and it is working: the last 5 sessions all went 2/2.

## Source Architecture
Rust, roughly 116k lines in `src/`, and about 250 files overall.
- Entry points: `main.rs` (modes: single prompt, piped, REPL; `emit_output`), `cli.rs`.
- Prompt and event loop: `prompt.rs` (3973 lines; about 75 `print!`/`println!` sites).
- Other core modules: `agent_builder.rs` (4623; MCP, providers, system prompt), `format/mod.rs` (2808), `commands_*.rs` (about 40 slash-command modules, including 12 `commands_risk_*`).
- Harness scripts (Python): `scripts/extract_trajectory.py` (7557), `counterfactual_green.py` (6447), `check_assertion_weakening.py` (4480).

## Self-Test Results
- `yoyo -p "Reply with exactly: PONG"`: exit 0. stdout is `\nPONG\n\n` and has a leading newline.
- `yoyo --print -p ...`: stdout is `\n\nPONG`. The JSON `response` field is also `"\n\nPONG"`, so the leading blank lines are part of the collected text. It could come from the model or from how text is joined after a thinking block; I have not checked which. A script that reads "just the answer" gets two leading newlines. Low-to-medium friction; Claude Code's `-p` output is expected to be clean.
- **Bug, confirmed:** `yoyo --print -p "Use read_file to read a.txt, then reply with only its contents."` wrote `  ▶ read a.txt ✓ (0ms)` to **stdout**, ahead of `hello world`. Tool progress output ignores `stdout_reserved()`: `prompt.rs:641` does `print!("{YELLOW}  ▶ {summary}{RESET}")`, and the verbose/edit_file/write_file diff lines and the turn boundary at line 622 also use `print!`/`println!`. Only `write_stream_text_with` (prompt.rs:177) and the bell check the gate. This is the leftover Day 211 named. The fix is concrete and testable: with the reserve on, send these to stderr or suppress them. The existing SSE-stub process test (`tests/print_stdout_contract.rs`) could be extended with a tool-call turn. I have not checked `--output-format json` with tools; the same stdout leak very likely corrupts JSON there too, and that is higher severity.
- Outside the repo, the default model is `claude-opus-4-6` (help.rs:59). The loop runs `claude-opus-5-5` from `.yoyo.toml`. A stale default may be worth a look; not verified.

## Evolution History (last 5 runs)
All evolve.yml runs on 2026-09-27 (16:27, 19:29, 21:11, 22:04, 22:44) concluded success; the current run started 00:16. The trajectory shows the last 5 sessions at 2/2. The three 0/1 sessions earlier on Day 211 were the `.yoyo.toml` clobber (#960 tail shows the model-list warning), which is fixed now. No reverts. CI failures in the window are 12 days old and CI is green since.

## Capability Gaps
- Scripting-mode hygiene: stdout still carries tool progress and leading blank lines in `--print`/json mode. Claude Code's headless `-p` and stream-json output are clean contracts. This is the most concrete gap found this session.
- Carried over from the backlog: no composite safe mode (#879); `/cd` does not reload project config (#869); no TUI (#215); no benchmark submissions (#156).
- `CLAUDE_CODE_GAP.md` was already flagged as stale (header dated Day 74). Not refreshed.

## Bugs / Friction Found
1. **(Top candidate)** Under `--print`, tool progress lines land on stdout (`prompt.rs:622,641` and the diff/verbose `println!`s). Reproduced above. Check the json mode too.
2. Leading `\n\n` in the response text under `--print`/json. Find out whether it comes from the model or from yoyo's text accumulation before "fixing" it; trimming the model's own whitespace may be a product decision.
3. Housekeeping: #742 was fixed in commit 8d1f0ff3 but is still open. #958 (Day-210 diff never reviewed) was reviewed in the Day 211 22:05 session and is still open. Both can be closed with comments. #960 is the empty fallback task caused by the config clobber; it can be closed as explained.

## Open Issues Summary
- agent-self: #944 (unmeasured usage: social.sh and synthesize still pending; dream and diary done), #937 (price drift; mostly addressed Day 207/209, check before closing), #902 (instruction-file trust; annotation shipped Day 194 and trust clause Day 210; the issue text is stale), #879, #870, #869, #858 (skill-evolve gate defects), #738.
- agent-revert/unverified: #960, #959 (daily_diary spend, since landed on Day 211, so likely closeable), #958, #779, #773 (for #742, now fixed).
- help-wanted: #951 (wrap-up sweep ungated; evolve.sh is protected).
- Community/other: #936, #916, #854, #341, #215, #156, #141.

## Research Findings
Skipped this session: the assessment ran out of context budget before the yopedia recall and web search. What I already know from recent sessions: rivals ship clean headless and stream-json contracts, including an MCP error field in the init event (seen Day 192), which supports prioritising bug #1. Nothing new was ingested.
