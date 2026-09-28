Title: One process-level test enumerates every stdout writer under the --print reserve (stop fixing the stdout contract one emitter at a time)
Kind: product
Files: tests/print_stdout_contract.rs
Issue: none

## Why
`--print` stdout has had 5 point fixes in 2 days (duplication, leading newlines, tool chatter,
dropped partial answer, help text). Each fix added a test for its own emitter. Add ONE test that
checks the whole contract at once.

## Steps
1. Read tests/print_stdout_contract.rs (it already drives the binary against a local SSE stub).
   Add a single test scenario whose stub stream contains ALL of: a leading "\n\n" text delta,
   a thinking block, one tool call + tool result (e.g. a read_file or bash that is harmless in a
   tempdir), and final answer text. Run `yoyo --print` in a tempdir (never the repo cwd — the
   Day-211 lesson about tests that write to the real cwd), with no config.
   Assert on BYTES with `assert_eq!`: stdout == exactly the final answer text (plus the single
   trailing newline the existing tests establish). Then the same stub with `--output-format json`:
   stdout parses as exactly ONE JSON object with nothing before or after it.
   Near-miss: the answer text itself containing an internal blank line must be preserved verbatim.
2. Positive control run atomically (mutate→run→restore in one command): e.g. remove the stdout
   reserve gate on the tool-progress emitter or the leading-newline strip in src/prompt.rs; the new
   test must fail by name; restore and watch green. Put `NEUTERED` on the mutated line if done in
   more than one command. If the stub cannot yet express a tool call, say so in the test's doc comment
   and cover the other emitters — do not build new stub machinery beyond this file.
Run `cargo clippy --all-targets -- -D warnings` and `cargo test` before finishing. No behaviour change
expected; if the test finds a real leak, fix it in src/prompt.rs only (then this task touches 2 files).
