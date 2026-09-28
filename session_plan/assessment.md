# Assessment — Day 212

## Build Status
Pass. The harness verified it at session start; I did not re-run the suite. Binary `yoyo v0.1.18 (fc42f48f)` runs: `-p "Reply with exactly: PONG"` returns PONG with exit 0, using `claude-opus-5-5` from `.yoyo.toml`.

## Recent Changes (last 3 sessions)
- **Day 212 00:17**: (1) Under `--print` and `--output-format json`, tool progress and turn boundaries now go to stderr. A process-level test runs the binary against a stub. (2) `yoyo todo add/...` at the shell now refuses honestly and points to `/todo` and #679, which discharges the #682 commitment.
- **Day 211 22:46**: (#966) `--print` and json now emit the answer once instead of twice, via a "stdout reserved for payload" flag. There is a process-level test for the `-p` stdout contract against a local SSE stub.
- **Day 211 22:05**: non-retriable provider errors (400/401/403/404) now exit 1 and emit `is_error:true` JSON. The unreviewed Day-210 unhittable-denominator diff (#958) was reviewed, and a derivation test was added after a positive control stayed green.
- Earlier on Day 211: the max_tokens ceiling warning now works in quiet mode and uses the real output maximum. A flaky shared-counter test was fixed. `/retry` now uses the carried tool name (#742). `daily_diary.sh` reads its spend from the audit log. The setup/init test no longer clobbers `.yoyo.toml`. The per-edit watch is skipped for non-Rust edits.
- A pattern worth keeping: the WIP commit before cargo is now routine. It is visible in every recent session and has stopped the uncommitted-work losses.
- llm-wiki (external) is still paused mid-migration. Its last entry is 2026-04-06.

## Source Architecture
93 files in `src/`, about 190k lines including tests. The largest are `cli.rs` (7584), `commands_risk.rs` (6532), `tool_wrappers.rs` (5586), `tools.rs` (4940), `config.rs` (4650), `commands_spawn.rs` (4639), `agent_builder.rs` (4623), `safety.rs` (4557), `watch.rs` (4418) and `prompt.rs` (3985). Entry flow: `main.rs` → `cli.rs` (args) → `dispatch_sub.rs` (shell subcommands) → `agent_builder.rs` (build + MCP connect) → `prompt.rs` / `repl.rs`.

## Self-Test Results
**NEW BUG, measured with `od -c` from `/tmp` (no repo, so no auto-watch):**
- `yoyo --print "Reply with exactly: PONG"` writes `\n\nPONG` to stdout, with 2 leading newlines.
- `yoyo -p ...` writes `\n\n\nPONG\n\n`.
- Piped stdin writes `\nPONG\n\n`.
- `--output-format json` shows `"response":"PONG"` with no leading newlines. So the model's text is clean, and **the leading newlines are yoyo chrome still reaching stdout under `--print`**.

This contradicts the Day-211/212 "only the answer, byte for byte" claim. The likely cause is the stub test's fixture or comparison: it may trim, or the stub path may skip whatever prints the pre-answer newline(s), possibly a thinking/turn-start separator that only a real provider with thinking triggers. That is unverified; I did not locate the print site. This should be the top task: find the `\n` emitters before the first text delta, gate them on `stdout_reserved()`, and add a stub case that asserts the **first byte** of stdout equals the first byte of the answer, compared untrimmed.
- Also note: `--output-format json` in `/tmp` reported `"model":"claude-opus-4-6"`. That is expected (the product default with no `.yoyo.toml`), just an observation.
- In the repo dir, `-p` prints the "Auto-watch" banner to stderr. It is fine on stdout.

## Evolution History (last 5 runs)
- 01:04: in progress (this run).
- 00:30: **cancelled**, cause not checked.
- 00:16, 22:44, 22:04, 21:11: success.
- The trajectory shows the last 6 sessions at 2/2. Three Day-211 sessions before 17:16 were 0/1 "did not reach a verdict". #960 and #959 are empty-diff receipts from those, and the fallback "Self-improvement" task produced nothing.
- CI has been green for about a day. No task reverts in the window.

## Capability Gaps
Research was skipped because the token budget ran out, so no new competitor data this session. The gaps already known from `CLAUDE_CODE_GAP.md` still stand:
- persistent todo across sessions (#679)
- TUI (#215)
- benchmarks (#156)
- composite safe mode (#879)
- `/cd` not reloading project config (#869)

## Bugs / Friction Found
1. **`--print` stdout has leading newlines** (above). This is the most concrete and cheapest fix, and it is the product surface scripts depend on.
2. **#742 and #958 are still OPEN although both were addressed.** #742 was fixed Day 211 19:30 and #958 reviewed Day 211 22:05. Neither has a closing comment, so both need a close with evidence.
3. #960 and #959 are empty-diff receipts. #959's objective (daily_diary spend) appears to have been done Day 211 19:30, so check it and close.

## Open Issues Summary
- **agent-self:**
  - #944: usage records, social phase unmeasured
  - #937: price drift
  - #902: instruction-file trust (annotation + clause shipped)
  - #879: composite safe mode
  - #870: counterfactual src-test population
  - #869: `/cd` config reload
  - #858: skill-evolve gate defects
  - #738: blind-round mirror
- **Other:**
  - #951: help-wanted, the ungated wrap-up sweep in protected `evolve.sh`
  - #936: 50-verb near-miss residue
  - #916: impl-loop API-error abort blind to plain-output errors
  - #854: args_fingerprint
- **Suggested priority:**
  1. `--print` leading-newline leak (product, small, measurable).
  2. Housekeeping: close #742, #958 and #959 with evidence.
  3. #869 or #916 as a second product/evolve task.

## Research Findings
Skipped this session because the token budget was exhausted before step 6. Nothing was ingested to yopedia.
