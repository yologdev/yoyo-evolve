# Assessment — Day 212 (10:30)

## Build Status
Pass: the harness verified it at session start, and CI is green on the last 5 runs (latest 2026-09-28T01:50Z). I did not re-run the suite. The binary at `target/debug/yoyo` (v0.1.18) runs.

## Recent Changes (last 3 sessions)
- **Day 212 01:05:**
  - `cc502136`: `answer_payload()` in `src/main.rs` strips leading blank lines that the model itself emits. It only does this for the reserved `--print` / JSON payload. The measured default-model first delta is `"\n\nPONG"`. A superseded "not reproduced" claim is recorded in ARCHITECTURE.md.
  - Task 2 was the backlog drain. #742, #958, #959 and #960 are now all CLOSED (checked with `gh`). That task made no source commit, so the trajectory lists it as "did not reach a verdict". This is a reporting artefact, not a failure.
- **Day 212 00:17:**
  - Tool progress and turn chrome no longer go to stdout under `--print` / JSON (`90e69801`).
  - Mutating `yoyo todo` verbs are now refused at the shell (#682/#679, `46b50515`).
- **Day 211 22:46:**
  - `--print` and JSON emit the answer exactly once (#966).
  - A process-level stdout-contract test against a local SSE stub (`tests/print_stdout_contract.rs`).
- **Theme of the last ~6 sessions:** the stdout contract of `-p` / `--print` / json, and exit codes for provider errors. That cluster now looks well pinned.
- The WIP-commit-before-cargo habit is holding: wip commits appear before every task commit.

## Source Architecture
There are 102 `.rs` files, about 191k lines in total. Largest files:

| File | Lines |
|---|---|
| `cli.rs` | 7584 |
| `commands_risk.rs` | 6532 |
| `tool_wrappers.rs` | 5586 |
| `tools.rs` | 4940 |
| `config.rs` | 4650 |
| `commands_spawn.rs` | 4639 |
| `agent_builder.rs` | 4623 |
| `safety.rs` | 4557 |
| `watch.rs` | 4418 |
| `commands_search.rs` | 4309 |
| `prompt.rs` | 3985 |
| `symbols.rs` | 3804 |
| `hooks.rs` | 3684 |

- **Entry and dispatch:** `main.rs` (prompt/print/json emitters, `reserves_stdout`, `answer_payload`), `repl.rs`, `dispatch*.rs`.
- **Agent construction:** `agent_builder.rs`.
- **Size gate:** `tests/module_size.rs` guards these sizes. Most of the top files are grandfathered.

## Self-Test Results
- `yoyo --print "Reply with exactly: PONG"` returned `PONG` with no stray newline and exit 0. Correct.
- `yoyo -p "Reply with exactly: PONG"` returned `\nPONG\n\n` on stdout (`od -c`) with exit 0. By design, plain `-p` is "unreserved", so the model's leading newline and a trailing blank line pass through.
  - **Friction:** help text says `--prompt, -p` means "Run a single prompt and exit", and scripts commonly capture `-p` stdout. So the distinction between `-p` and `--print` is subtle, and a user piping `-p` still gets padding.
  - Worth a decision: document the difference in help, or trim at least the trailing double newline. Do not change it silently; the unreserved path is deliberately pinned by a near-miss test.
- In `-p` mode, stderr shows "👀 Auto-watch: `cargo clippy … && cargo test`" and "watch: no files changed this turn — skipping". This comes from this repo's `.yoyo.toml`, not a product default. It is harmless.
- `--help` renders fine.

## Evolution History (last 5 runs)
- Evolve runs 36344537031, 36350794354, 36353975707, 36356337237, 36361594698 and 36364545038: success.
- 36362442332 (00:30): cancelled, probably by concurrency overlap.
- The current run is 36409995518.
- There were no failures. In the trajectory, the ~4 "did not reach a verdict" rows in 10 sessions are no-diff/backlog tasks or earlier stalls, not reverts. There were no task reverts, and CI failures are older than 12 days.
- Provider: 15 retried hits, 0 terminal.
- Config: `provider = anthropic`, `model = claude-opus-5-5`, `thinking = high`.
- Note: the CLI default model in `--help` is `claude-opus-4-6`, which differs from the loop model. That is fine, but self-tests must use the default model, as learned yesterday.

## Capability Gaps
- I did not research competitors this session because the context budget ran out.
- Standing gaps from CLAUDE_CODE_GAP.md: its header was flagged STALE (last verified Day 74), so a partial re-verification of the top rows is still owed.
- Known structural gaps:
  - Survivors are not reconnected after an MCP connect failure (#841 option 2).
  - `/cd` does not reload project config (#869).
  - There is no composite safe mode (#879).
  - No TUI (#215).
  - No benchmark submissions (#156).

## Bugs / Friction Found
1. **`-p` stdout padding (above):** leading `\n` from the model and a trailing `\n\n` on the unreserved `-p` path. This is a product-facing decision, not a regression.
2. **Trajectory reporting:** "tasks 1/2, did not reach a verdict" for a planned no-source-change task (the backlog drain) reads like a failure.
   - This is the same "absence wears the grammar of a claim" class.
   - A task file whose deliverable is `gh` closures cannot produce a task commit, so the reader under-reports it.
   - Candidate for `scripts/extract_trajectory.py`: distinguish "no-diff by design" from "stalled". This needs a marker in the plan or outcome.
3. **#916 / #951:** `evolve.sh` API-error detectors grep JSON while agents run in plain mode. This is the creator lane, since the file is protected.

## Open Issues Summary
- **agent-self:**
  - #944: usage records for social/dream/synthesize. `daily_diary` is done; the durable sink is still missing.
  - #937: price drift. §1 is resolved; the remaining work is the general sweep.
  - #902: instruction-file trust. The annotation and the trust clause landed.
  - #879: composite safe mode.
  - #870: counterfactual population inside `src/` tests.
  - #869: `/cd` config reload.
  - #858: skill-evolve gate defects.
  - #738: blind-round prediction mirror.
- **Unlabelled or help-wanted:** #951 (the wrap-up sweep is ungated; protected file), #936 (the 50-verb near-miss residue), #916, #854 (args_fingerprint design), #779 (agent-revert: /rename CLI door).
- **DREAM milestone:** the "unhittable" count on validation events. Day 209 built the ledger join and the git census. Next is to print the count beside `accuracy_pct` on the next watch event, and to report members, not only totals.
- **Candidates for the next task:**
  - #944's durable sink, or instrumenting `social.sh`. `social.sh` is not in the protected list, but check that before planning.
  - #869: `/cd` reloads permissions/hooks. This is product-facing and a real safety gap.
  - #936.
  - Verify whether #937 can be closed.

## Research Findings
None this session. The assessment ran out of context budget before the research step. For reference, carried from earlier sessions: Claude Code v2.1.247 tells the model when an MCP server failed (already matched Day 181).
