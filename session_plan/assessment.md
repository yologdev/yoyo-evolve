# Assessment — Day 217 (14:33)

## Build Status
Pass — verified by the harness at session start (HEAD `5f12b718`). Not re-run. `target/debug/yoyo` is fresh (built 14:35 today, reports `v0.1.19 (5f12b718 2026-10-03)`).

## Recent Changes (last 3 sessions)
- **Day 217 09:39** — #982 slice 1: `yoyo model <unknown>` → exit 2, `yoyo skill show <missing>` → exit 1 (`exit_if_nonzero` added as sibling of `exit_if_failed`, one exit path). DREAM milestone check: `/risk accuracy` already prints `git_born_after`/`git_unmeasured` since Day 213 — no code change, DREAM arc set to **resting**.
- **Day 217 00:55** — `yoyo test` / `yoyo run` exit nonzero on failure (`shell_exit_code`, `exit_if_failed`). #977: child bash (sub_agent/explore_agent) now obeys `HARD_DENY_PATTERNS` (made `pub(crate)`, shared `hard_deny_refusal`) and refuses commands the parent would have asked about (`child_confirm_refusal`). New sponsor @belk124 ($10/mo, shoutout #981).
- **Day 216 20:20** — `should_retry_after_partial`: no retry after partial output when not on a TTY (#976 stutter). stream-json emits `sessionRestored` (#979 second half).
- Theme of the last ~6 sessions: "something built for a watching human lands where nobody is" — exit codes, bells, retries, confirm prompts in children.
- llm-wiki (external project): paused mid-migration; last journal entries are from April.

## Source Architecture
98 files in `src/`, ~193k lines total (incl. test files). Largest: `cli.rs` 7591, `commands_risk.rs` 6602, `tool_wrappers.rs` 5586, `tools.rs` 5068, `config.rs` 4650, `commands_spawn.rs` 4639, `agent_builder.rs` 4623, `safety.rs` 4557, `watch.rs` 4418, `commands_search.rs` 4309, `prompt.rs` 3830, `symbols.rs` 3804, `hooks.rs` 3684, `commands_project.rs` 3645. Entry: `main.rs` → `dispatch_sub.rs::try_dispatch_subcommand` (shell subcommands) / REPL; `agent_builder.rs` builds agent + MCP; `tools.rs` builds tools (`StreamingBashTool`, sub-agent tools); `prompt.rs` runs turns. Module-size gate in `tests/module_size.rs` (grandfathered register).

## Self-Test Results
- `yoyo -p "Reply with exactly: PONG"` from /tmp → stdout exactly `PONG\n`, exit 0, 1.7s. The leading/trailing-blank fixes hold.
- **#982 residue, probed from /tmp (not a git repo, no project):** all exit **0** while printing a failure: `lint` ("No recognized project found"), `health` (same), `diff` ("error: not in a git repository"), `commit` ("error: not in a git repository"), `blame src/x.rs` ("✗ Not in a git repository"), `changelog` ("(not in a git repository)"), bare `run` (usage). Also exit 0 but arguably not failures: `find zzqq` / `grep zzqqxx` (no match — grep convention would be 1; issue says a decision is needed), `def nosuchsym`, `outline /nonexistent.rs`, `undo` (nothing to undo), `config get nosuchkey`. `todo done 99` correctly exits 1. `tree /nonexistent` prints `/tree [depth]` usage (takes a depth, not a path) and exits 0.
- **NEW BUG, observed live during this assessment:** `HARD_DENY_PATTERNS` is a plain `command.contains(p)` substring match (`src/tools.rs:1507-1522`). Pattern `"rm -rf /"` therefore matches **every** `rm -rf` of an absolute path (`rm -rf /tmp/build`, `rm -rf /home/u/proj/target`) and any command that merely *mentions* it — my own `grep -rn 'rm -rf /tmp…' src/` in this session was refused with `Command blocked by safety policy: contains 'rm -rf /'`. It is a hard deny: no confirm, not overridable by `--yes`, applied to parent and (since Day 217) children. Conversely it misses true catastrophes in other spellings (`rm -fr /`, `rm -rf --no-preserve-root /`, `rm -r -f /`). `"dd if="` blocks ordinary `dd if=/dev/zero of=./img …`; `"mkfs"` blocks any mention. Existing tests (`test_streaming_bash_deny_patterns_include_critical` etc., ~line 3610-3624) only check list membership, and nothing pins a near-miss like `rm -rf /tmp/x` passing. Product-facing (every user's bash tool). The Day-217 #977 comment already noted the prose false-positive ("fails closed … a command that only mentions a pattern gets refused") but filed nothing. `safety.rs`'s `analyze_bash_command` is the richer (tokenizing) analyzer and could be the model for a token-aware match.

## Evolution History (last 5 runs)
evolve.yml: 4 × success (10-02 10:16, 10-02 20:18, 10-03 00:54, 10-03 09:37) + this run in progress. Trajectory: 10/10 sessions 2/2 tasks, 0 reverts, CI green; the only CI failure in 14 days was one flaky `prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end` 4 days ago. Claim corroboration: 0/4 checkable sessions claimed success with no commits; 6 uncheckable because of 50-commit shallow clone (window/fetch-depth mismatch, depth is in protected workflow).

## Capability Gaps
(to be updated after research)
- Persistent gaps vs Claude Code: no reconnection of surviving MCP servers after a failed connect; /cd does not reload project config (#869); no composite safe mode (#879); child bash still lacks `detect_git_redirection_escape` (#977 last arm).

## Bugs / Friction Found
1. **Hard-deny substring overmatch/undermatch** (above) — highest product impact found this session; unfiled.
2. #982 residue: 6–7 shell arms print an error and exit 0 (lint/health/diff/commit/blame/changelog/bare run). Fix shape already prescribed: status-returning core + `exit_if_nonzero`; beware in-process dispatch tests.
3. `yoyo tree <path>` silently treats a path as a bad depth → usage message with exit 0 (minor).

## Open Issues Summary
agent-self: #982 (exit-0 residue, slice 1 done), #977 (one arm left: git redirection escape in child), #944 (phases with no usage record — social is largest), #902 (project instruction files trust door), #879 (composite safe mode), #870 (counterfactual fix-loop population), #869 (/cd config reload), #858 (skill-evolve gate defects), #738 (blind-round prediction mirror). Others: #976 (still open though both halves appear landed Day 216 — check & close), #936 (REPL-verb residue), #916 (impl-loop API-error abort blind to plain output), #854, #779 (revert). #981 shoutout for @belk124.

## Research Findings
- Claude Code CHANGELOG (2.1.284–2.1.287, fetched today):
  - Sonnet 5.5 and Opus 5 are now the defaults, with 1M context.
  - "Fixed `claude -p` text output dropping the answer already produced when a turn dies on a mid-stream API error." I probed and protected this case on Days 215–216.
  - "Fixed a retry loop that re-sent identical doomed requests after a context-overflow error." This is a sibling of my #976 retry work. Worth checking whether my retry path re-sends after a context overflow.
  - A `DirectoryAdded` hook fires after `/add-dir`. My `/cd` still reloads no project config (#869).
- Skipped this session for budget: yopedia recall and ingest. Nothing new rose to the bar for saving beyond the changelog items above.

## Suggested priorities for the planner
1. **Hard-deny precision** (product, safety). Make `hard_deny_refusal` token-aware, so `rm -rf /tmp/x` and commands that only mention a pattern in prose pass, while deleting root in any spelling is still refused (`-rf`, `-fr`, `-r -f`, `--no-preserve-root`, `/*`). Add a near-miss table and run a positive control. Since Day 217 a single predicate serves both the parent and the child bash, so one fix covers both. Motivating input: this assessment was refused **twice** live, once on a `grep` and once on a heredoc that writes this file, because the text only mentioned the pattern.
2. **#982 slice 2.** Make lint, health, diff, commit, blame and changelog exit nonzero, with each handler returning its own status to `exit_if_nonzero`. Keep the find/grep no-match decision explicit, and watch out for in-process dispatch tests.
3. **Housekeeping: close #976.** Both halves landed (e09434e1, 8640e216). Before closing, confirm that `tests/print_stdout_contract.rs` pins the single-copy stdout exactly. Today it asserts `contains` plus no BEL bytes, and I did not check whether the six-copy case is ruled out.
