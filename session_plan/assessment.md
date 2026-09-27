# Assessment — Day 211 (22:46)

## Build Status
Pass: the harness verified it at session start, and I did not re-run the suite. `target/debug/yoyo` exists and runs. I ran it against a local Anthropic-SSE stub and it **reproduced #966, and a sibling that #966 doesn't mention** (see Bugs).

## Recent Changes (last 3 sessions, all Day 211)
- **22:05:** Non-retriable provider errors (400/401/403/404) in `-p` mode now exit 1. The error is recorded in the InspectError fall-through branch, so the fallback provider actually gets tried. `--output-format json` was setting a different switch than the one the output code reads; that is fixed too (e980f780, 64a6de59). Task 2 reviewed the never-judged Day-210 unhittable-denominator diff (#958). One producer (the code that writes the count) had no coverage, and a write-then-read test was added (a03c41f8). Afterwards, `scan_commitments.py` was changed to read the answer from `-o` because `--print` doubled its JSON (fafe3b46, which is what prompted #966).
- **21:13:** Fixed a flaky compact-thrash test that raced on the global COMPACT_T… counter. The `max_tokens` ceiling warning was silent in captured/quiet mode on the Anthropic path; it now fires.
- **19:30:** `/retry` uses `PromptOutcome.last_tool_name` instead of string-scanning the error (#742, 8d1f0ff3). `daily_diary.sh` now reports its own per-run spend on stderr (#944 partial).
- **16:28:** A `yoyo setup` routing test was running the real wizard in `$HOME`, which clobbered config. That was the cause of the 3 empty sessions at 00:12, 09:48 and 14:59. Per-edit auto-check now skips cargo for edits that cannot affect Rust.

## Source Architecture
104 `.rs` files, about 191K lines in total. Largest files: cli.rs 7584, commands_risk.rs 6532, tool_wrappers.rs 5586, tools.rs 4940, config.rs 4650, commands_spawn.rs 4639, agent_builder.rs 4623, safety.rs 4557, watch.rs 4418, commands_search.rs 4309, prompt.rs 3913.
- Entry point: `src/main.rs`. `-p` goes to `run_single_prompt` (around line 1206), piped input to `run_piped_mode` (around line 1234), otherwise the REPL.
- `emit_output` (main.rs:294) handles the output modes: print, json and `-o`.
- Streaming renderer: `src/prompt.rs`. Every stream event goes through `print!` at lines 858 (text delta), 878 (think flush at agent end), 1120 and 1183 (markdown flush). None of these check print or json mode.

## Self-Test Results
Test setup: Python SSE stub on 127.0.0.1, `--no-tools --max-turns 1 --base-url ... -p hi`. Results:
- `--print`: stdout is `\n{"answer": 42}\n\n{"answer": 42}`, i.e. **doubled with a leading blank line**, exit 0. This confirms #966 exactly.
- `--output-format json`: stdout is `\n{"answer": 42}\n\n{json object}`. **The streamed text leaks onto stdout before the JSON envelope**, so `yoyo -p ... --output-format json | jq` fails with "Extra data". **#966 does not mention this.** It is the same defect coming out through a second output mode.
- In both cases the cause is that the streaming renderer in prompt.rs always `print!`s to stdout. In print mode, main.rs only calls `disable_color()`.

## Evolution History (last 5 runs)
The 22:04, 21:11, 19:29 and 16:27 runs succeeded. The 15:10 run was cancelled. The current run started at 22:44. Earlier on Day 211 (00:12, 09:48, 14:59) there were 3 sessions with no commits, caused by the setup-test clobber and slow per-edit checks; both were fixed at 16:28. One fallback task was reverted as an empty diff (#960). The fallback agent log shows a model-list warning ("Switch with: /model claude-opus-5"), which is worth a glance but is not blocking. No CI failures in the last 11 days.

## Capability Gaps
- **Scripting contract (product):** `--print` and `--output-format json` are not clean on stdout. This is the biggest current gap relative to Claude Code's `-p --output-format json`, which the automation world relies on. That fix is #966 plus the json sibling.
- Carried forward: no composite safe mode (#879); `/cd` doesn't reload project config (#869); no TUI (#215); no benchmark submission (#156).

## Bugs / Friction Found
1. **#966 (creator-filed, Kind: product, highest priority):** `--print` doubles the output.
   - The fix must cover **both** `print_mode` and `json_output`: suppress the streamed text on stdout in either mode, or route it to stderr. Otherwise the fix repeats the "two doors, one deaf" pattern.
   - Seam: the four `print!` sites in prompt.rs (858/878/1120/1183) plus the trailing `println!` when `state.in_text` is set. A process-global "stdout is reserved for the final payload" flag, set in main.rs next to `disable_color()`, probably reaches all of them. Check how `is_quiet()` in `format/mod.rs` is set first, because it may already be the right switch.
   - Test at the emission point: stdout is exactly the response text for `-p` and for piped input, and a single JSON object in json mode.
   - Near-miss guard: normal `-p` mode without `--print` still streams to stdout.
   - The stub-server approach above reproduces it in under a second and could become an integration test.
2. **#742 is still open, but the fix landed at 19:30 (8d1f0ff3).** Close it, along with its revert receipt #773.
3. **#958 (agent-unverified):** reviewed and fixed at 22:05 (a03c41f8). Likely closeable; verify the last comment first.

## Open Issues Summary
- **agent-input:** #966, the print duplication above.
- **agent-self:**
  - #944: usage records; social is still unmeasured, and the durable sink is missing.
  - #937: price drift; §1 was resolved at HEAD.
  - #902: instruction-file trust; step 2 landed.
  - #879: composite safe mode.
  - #870: the counterfactual population.
  - #869: `/cd` config reload.
  - #858: skill-evolve gate defects.
  - #738: prediction mirror.
- **Receipts:**
  - #958 (unverified; likely done).
  - #959 (diary spend, since done in ecf15eee, so closeable).
  - #960 (empty fallback).
  - #779 and #773 (old reverts).
- **Help wanted:** #951 (the wrap-up sweep is ungated, and it lives in protected evolve.sh).

## Research Findings
Skipped this session: the context budget ran out. No yopedia recall or ingest was done. The relevant observation was made locally instead: Claude Code's `-p --output-format json` emits exactly one JSON object on stdout, and yoyo currently does not.
