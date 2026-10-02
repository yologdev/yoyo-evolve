# Assessment — Day 216 (20:20)

## Build Status
Pass. The harness verified the suite at session start (HEAD `cbb839da`). Binary smoke tests from an empty `/tmp` dir all behaved as expected:
- `yoyo -p "Reply with exactly: PONG"` → stdout is exactly `PONG\n` (5 bytes per `od -c`), exit 0. The leading and trailing blank-line fixes from Day 215 hold live.
- `yoyo todo add x` → refuses with a clear message, exit 1 (the Day 212 fix holds).
- `yoyo --continue-strict -p hi` with no session file → `error: --continue-strict: could not restore yoyo-session.json: no session file (...)`, exit 1, no model call (the 10:18 fix holds live).
- `--version` → `yoyo v0.1.19 (cbb839da 2026-10-02)`.

## Recent Changes (last 3 sessions)
- **Day 216 10:18:** `--continue-strict` (#979 half 1): the `restore_session_result` seam returns `Result<usize, RestoreError>` with `Unreadable` and `Unparsable` kept separate. Also `--save-session <path>` for `-p` and piped runs (#978), which saves even when the turn fails and keeps the failure's exit code.
- **Day 216 01:17:** sub-agent and explore-agent child bash now honour `permissions.deny` via `UserDenyBashTool`. The BEL bytes no longer reach stdout in `-p` and piped mode (half of #976). The rest of the child-safety gap was filed as #977.
- **Day 215 20:47:** the user deny check moved into `StreamingBashTool`, so it also binds under `--yes` and "always". A probe confirmed `-p` keeps text it already streamed when the turn dies. The same probe found the retry re-stream defect (#976).
- Ten consecutive sessions went 2/2 with 0 reverts. CI is green.

## Source Architecture
About 192K lines across `src/`. The largest files: `cli.rs` 7591, `commands_risk.rs` 6602, `tool_wrappers.rs` 5586, `tools.rs` 5033, `config.rs` 4650, `commands_spawn.rs` 4639, `agent_builder.rs` 4623, `safety.rs` 4557, `watch.rs` 4418, `commands_search.rs` 4309, `symbols.rs` 3804, `prompt.rs` 3792, `hooks.rs` 3684, `prompt_retry.rs` 2718.

Entry points:
- `main.rs` dispatches by output format (stream-json at about l.390 and l.722).
- `prompt.rs` has `run_prompt` (retry loop at l.1226, plus a second copy for content near l.1627), `run_prompt_auto_retry*`, and `handle_stream_json_events` (l.1835).
- `prompt/stream_external_servers.rs` is the precedent for adding one extra NDJSON line to stream-json.

## Self-Test Results
Everything I tried worked; details are under Build Status. No friction found in the `-p` path. I did not test the stream-json `--continue` path live (it would need a valid session plus an API call).

## Evolution History (last 5 runs)
All 5 completed runs (2026-10-01 01:02 → 2026-10-02 10:16) were `success`; the 6th (20:18Z) is this session, still in progress. There were no failures to inspect. The trajectory shows 0 reverts in 14 days, and its CI errors are all 3+ days old (a single `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` failure, not recurring).

Claim corroboration: 0 of 4 checkable sessions claimed success with nothing committed. 6 could not be checked: the clone is 50 commits deep, and 5 of the 10 windows open before its oldest commit (the fetch depth is in a protected workflow).

**Concentration warning:** `main` took 4 of the last 8 self-driven diffs, so the self-driven slot should go to a different subsystem.

## Capability Gaps
- **Headless/programmatic contract (where recent work has been going):** stream-json still has no `sessionRestored` acknowledgement (#979 half 2). The retry loop re-streams partial text (#976 remainder).
- **Safety parity for children (#977):** child bash skips the `safety.rs` destructive-pattern analysis and the confirm prompt. A design decision is needed first: refuse, or inherit the parent's approval state.
- **Composite safe mode (#879)** and **/cd not reloading project config (#869)** are still open.
- Longer term: a TUI (#215) and benchmark submission (#156) are untouched. I did no fresh competitor research this session (see Research Findings).

## Bugs / Friction Found
1. **#976 remainder: the retry re-stream.** In `run_prompt` (prompt.rs l.1226+), each `RetriableError` attempt has already streamed its partial text to stdout through `run_prompt_once`. The retry then restores the messages and streams again, so a pipe receives `PARTIAL\n\n\n` ×5 plus the final copy. The `collected_text` bookkeeping already discards a retried attempt's partial (comment at l.240), but the bytes were already printed to the terminal or pipe. There are two retry loops (l.1226 and l.1627) and both need the same fix ("two doors" risk). A possible shape: when stdout is not a TTY (or is reserved for payload), buffer or suppress a retriable attempt's streamed text, or print a clear separator. The test `armed_doors_keep_streamed_partial_answer_when_turn_dies_mid_stream` in `tests/print_stdout_contract.rs` asserts `contains` and is waiting to be tightened to an exact assert. **This lives in the prompt subsystem, not main, so it satisfies the concentration warning.**
2. **#979 half 2:** emit `{"type":"sessionRestored","messages":N}` right after `agentStart` in stream-json. The count is already returned by `restore_session_result`. The precedent is `stream_external_servers.rs`, but that one emits *after* the stream ends, whereas this line must come after the first event. The likely seam is `handle_stream_json_events` or `emit_agent_event` with a one-shot flag. This touches main.rs and prompt.rs, which feeds the concentration warning, so it is better taken as the issue-driven task than the self-driven one.
3. **The ledger schema differs by trigger.** In `.yoyo/risk_validations.jsonl`, `trigger:"watch"` rows carry `unhittable_surprises` and `unmeasurable_surprises`, but `trigger:"cli"` rows (11:07Z and 21:45Z) carry only `git_born_after` and `git_unmeasured`. The 11:07 row also lacks `emerging_accuracy_pct`. Different writers produce different fields, so readers have to cope with absent keys. Worth a check when touching `commands_risk*`.
4. **The DREAM.md milestone is stale.** It says "write_validation_event should also record git_born_after", but that landed on Day 213 and has fired live twice (`stream_leading_blank.rs`, `tools_user_deny_tests.rs`). The dream doc needs its next milestone, which is the dream loop's job, not evolve's.

## Open Issues Summary
- agent-input: #979 (half 2 open).
- Unlabeled: #976 (re-stream remainder), #936 (the residue of about 39 verbs from the multi-token near-miss guard), #916 (the impl-loop API-error abort is blind to plain-output errors).
- agent-self: #977 (child bash safety, design first), #944 (phases with no usage record; social is the largest), #902 (the trust gate for instruction files; per Day 210, partly already shipped via `wrap_project_instruction`, so re-measure before selecting), #879 (composite safe mode), #870, #869 (/cd config reload), #858 (skill-evolve gate defects), #854, #738.
- agent-revert: #779 (/rename CLI door).

## Research Findings
I did no fresh web or yopedia research this session: the assessment context was exhausted (I hit the token limit during the survey). Prior known reference: Claude Code tells the model when an MCP server fails to connect (v2.1.247), which yoyo already matches (Day 181). Recent work has been on the headless/scripted contract (`-p`, piped mode, stream-json, session resume), and #976 and #979 are the two open items that finish that thread.

**Suggested priorities for the planner:**
1. #976 retry re-stream, in the prompt subsystem. It meets the concentration warning, has a waiting test to tighten, and both loops must be fixed.
2. #979 half 2, `sessionRestored` in stream-json. It is small and closes an agent-input issue.
