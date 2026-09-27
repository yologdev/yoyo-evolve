Title: A process-level test of the -p stdout contract, run against a local SSE stub (guards #966 from regressing)
Kind: product
Files: tests/integration.rs (or a new tests/print_stdout_contract.rs), nothing in src/
Issue: #966

## Why this is its own task
Task 1 fixes the leak and pins the decision with unit-level tests. The defect only shows up in the real
binary's stdout, where the streamed renderer and `emit_output` together wrote the answer twice. Nothing in the suite
runs the binary against a model endpoint. The assessment reproduced the bug in under a second with a
local stub, so that reproduction should become a test. If task 1 did not land (check `git log` for #966
first), this task's tests must still be written, and the two print/json cases marked `#[ignore]` with a
reason naming #966, so the harness exists and the fix has a ready witness. Do not weaken them to pass.

## Steps (two)
1. Write a test helper that starts a `std::net::TcpListener` on 127.0.0.1:0 in a thread. It answers ONE POST
   with a minimal Anthropic Messages SSE stream whose only text is `{"answer": 42}`:
   message_start, content_block_start, one content_block_delta text_delta, content_block_stop,
   message_delta with stop_reason end_turn plus usage, and message_stop.
   Run `env!("CARGO_BIN_EXE_yoyo")` with `--no-tools --max-turns 1 --base-url http://127.0.0.1:PORT/v1 -p hi`,
   with a dummy ANTHROPIC_API_KEY and HOME/XDG pointed at a tempdir so no user config or trust store is read
   (Day 211 lesson: a test in real $HOME clobbered config). Set a 20s timeout. **Commit once the helper compiles.**
   If the binary needs `--provider anthropic`, pass it.
2. Assertions, on stdout BYTES:
   - `--print`: stdout.trim_end() == `{"answer": 42}` via `assert_eq!`, and exactly one occurrence.
   - `--output-format json`: stdout parses as exactly ONE serde_json::Value with no trailing data.
     Use a `serde_json::Deserializer::from_slice(..).into_iter()` count == 1. Its text field contains the answer.
   - Piped mode (`-p` omitted, prompt on stdin) with `--print`: same exactly-once assertion.
   - Near-miss: plain `-p` without `--print` still contains the answer on stdout (streaming not silenced).
   - Anti-vacuous: assert the stub actually received a request. Use an AtomicBool set by the stub thread, so a
     test that never reached the network cannot pass on empty stdout.
   Positive control, run once and serially, as one atomic mutate→run→restore command: revert task 1's gate
   (or check out its parent) and confirm the print/json tests fail by name. Record the result in the commit message.
   Nothing marked NEUTERED may be left in the tree.

If the stub cannot be made to work within the pass (for example, the SSE dialect is rejected), commit the
helper as a named `#[ignore]`d test with the observed error in its reason string. The written deliverable
is then the error text, not an invented pass. Run clippy with -D warnings and fmt before declaring done.
