Title: `-p` keeps the answer it already streamed when the turn dies on a mid-stream API error
Kind: product
Files: tests/print_stdout_contract.rs, src/stream_leading_blank.rs, src/prompt.rs (only if the probe shows loss)
Issue: none (class taken from the Claude Code changelog: "Fixed `claude -p` text output dropping the answer already produced when a turn dies on a mid-stream API error")

Why: Days 212-215 rebuilt the `-p` stdout filters. `src/stream_leading_blank.rs` now HOLDS BACK
trailing whitespace and releases it only if more text follows. Nobody has checked what happens to
already-streamed text, or to the held-back tail, when the provider stream dies mid-turn. A user
piping `yoyo -p` into a file could lose a partial answer they already paid for. This is unverified for yoyo.

Steps (both must fit in one pass):
1. REPRODUCE FIRST, through the real binary. Extend the local SSE stub that
   `tests/print_stdout_contract.rs` already uses (Day 211). Add a variant that sends text deltas
   "partial answer\n" and then breaks the stream (close the connection mid-event, or send an
   error event). Run `yoyo -p` against it through the same process-level path the existing test
   uses. Record the exact stdout bytes, the stderr and the exit code. Write the test so it asserts
   (a) stdout contains the already-streamed `partial answer` bytes, (b) the exit code is NON-zero
   (a dead turn must not exit 0), and (c) the error is reported on stderr, not stdout.
2. If the test passes as written, that is a deliverable: commit it as the regression guard. In
   the commit message and in ARCHITECTURE.md's entry for `src/stream_leading_blank.rs`, write
   "probed, already correct" plus the three measured readings (stdout bytes, exit code, stderr
   line). Do NOT invent a code change. If it fails, fix the smallest seam. The likely one is
   flushing the filter's held text on the error path in `src/prompt.rs` and dropping only the
   trailing-whitespace hold, never text. Then re-run. Add a near-miss as well: the normal
   non-error stub path must stay byte-identical (`assert_eq!` on the full stdout).

Positive control (only if code changed): neuter the flush with a `NEUTERED` marker, watch the new
test fail by name, then restore it, all in ONE command. Then run
`cargo clippy --all-targets -- -D warnings && cargo test`.
Docs: if behaviour changed, add one line to docs/src/usage/piped-mode.md and to CHANGELOG.md.
