Title: `--print` must not drop the answer already produced when a turn dies mid-stream (probe first, fix if it reproduces)
Kind: product
Files: tests/print_stdout_contract.rs, src/prompt.rs, docs/src/usage/piped-mode.md
Issue: none (transferred from Claude Code changelog: "Fixed `claude -p` text output dropping the answer already produced when a turn dies on a mid-stream API error")

## Why
Since Day 211–212, `--print` suppresses streaming and writes only the final
reserved payload to stdout. So a turn whose stream dies AFTER text deltas arrived
may write NOTHING to stdout — the model's partial answer is lost, and a script
reading stdout sees empty output. Unverified: this is a hypothesis to measure.

## Step 1 — Measure (commit the test before touching src/)
`tests/print_stdout_contract.rs` already runs the real binary against a local SSE
stub (`sse_body()`, `ANSWER`). Add a second stub stream: `message_start`,
`content_block_start`, one or two `text_delta`s carrying a distinctive string
(e.g. `PARTIAL_ANSWER_7Q`), then an SSE `event: error` (`overloaded_error` or
`api_error`) OR an abrupt connection close — no `message_stop`.
yoyo retries mid-stream failures (see "connection loss mid-stream" in
src/prompt.rs), so the stub must serve this same broken stream to EVERY request
and count requests; keep retries fast (use whatever env/flag the existing test
uses to shorten backoff, or cap max retries if a flag exists — check, don't guess).

Invoke EXACTLY as the existing test invokes `--print` (same flags, same env, same
cwd seam), changing only the stub body. Record and assert/print: stdout bytes,
exit code, stderr contains an error, request count.

Honest null is a valid deliverable: if stdout already carries
`PARTIAL_ANSWER_7Q` with a nonzero exit, pin that as a regression test and stop —
report "did not reproduce under <exact stub shape>" with the four readings above.
Commit the test (red or green) before any src/ edit.

## Step 2 — Fix only if Step 1 shows stdout empty (or missing the partial text)
In src/prompt.rs, where the reserved `--print` payload is emitted, make the
failure path emit the text already accumulated for the final assistant turn
(apply the same leading-blank-line strip the success path uses), then keep the
NONZERO exit and the stderr error — stdout carries what was produced, the exit
code carries that it failed. Do not print partial text as a success (exit 0).
Scope: `--print` text mode only; `--output-format json` is out of scope — name it
in the docs as not covered.

Tests (emission point, byte-level, `assert_eq!` not `contains` where the full
string is known):
- mid-stream death → stdout == the partial text (+ trailing newline convention
  of the success path), exit != 0.
- near-miss: the existing clean-stream test stays byte-identical (`{"answer": 42}`).
- positive control, atomic mutate→run→restore in one command: disable the new
  failure-path emission and confirm the new test fails by name.

Update docs/src/usage/piped-mode.md: one short paragraph stating what stdout and
the exit code carry when a turn dies mid-stream. Run `cargo fmt`, `cargo build`,
`cargo test`, `cargo clippy --all-targets -- -D warnings`.
