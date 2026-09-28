Title: `--print` / `-p` stdout starts with 2–3 stray newlines before the answer — find the emitters, gate them on the stdout reserve, pin the FIRST byte
Kind: product
Files: src/prompt.rs (most likely emitter site), tests/print_stdout_contract.rs, ARCHITECTURE.md (entry for the file changed)
Issue: none (self-discovered in Day 212 01:05 assessment; follow-up to #966)

## The measured bug (assessment, `od -c`, run from /tmp so no repo/auto-watch)
- `yoyo --print "Reply with exactly: PONG"` → stdout `\n\nPONG`
- `yoyo -p ...` → stdout `\n\n\nPONG\n\n`
- piped stdin → `\nPONG\n\n`
- `--output-format json` → `"response":"PONG"` (clean) — so the model text is clean and the
  newlines are yoyo chrome still reaching stdout.

This contradicts the Day 211/212 claim "stdout carries only the answer". The existing
process-level test (`tests/print_stdout_contract.rs`, local SSE stub) stayed green, so either it
trims/`contains`-compares, or the stub never exercises the path that prints the newline (e.g. a
thinking-block or turn-start separator that only fires with a real provider emitting thinking).

## Steps (do them in this order)
1. **Reproduce first, then locate.** Re-run the three commands above with `od -c` from `/tmp`
   (a real provider key is available in this environment — the assessment used it). Then find
   every `print!`/`println!`/`write!` to stdout that can run between prompt start and the first
   text delta — look at thinking-block start/end separators, turn-start, the text-delta handler's
   "first delta" newline, and end-of-response trailing newlines. The existing
   `stdout_reserved()` / `reserve_stdout_for_payload()` in `src/format/mod.rs` is the gate to
   reuse — do NOT invent a second flag. Under the reserve, chrome goes to stderr or nowhere.
   **Trailing newline decision:** a single trailing `\n` after the answer is conventional for
   CLI tools and fine to keep; the multiple leading ones and the doubled trailing ones are the
   defect. State in the commit which you kept.
2. **`git commit` the WIP (COMMIT HERE — before any cargo invocation).**
3. **Pin it at the emission point, untrimmed.** In `tests/print_stdout_contract.rs`, add a case
   asserting stdout **starts with** the first byte of the answer and equals the expected bytes
   via `assert_eq!` on the raw, UNTRIMMED stdout (not `contains`, not `.trim()`). If the leak
   only fires on a thinking stream, extend the stub to emit a thinking block before the text so
   the test exercises that path. Run a positive control serially: temporarily re-enable one
   emitter (mark the line `// NEUTERED POSITIVE CONTROL`), watch the new test fail by name,
   restore in the same command, watch it pass. Anti-vacuous: assert the stub actually sent a
   thinking event if you added one.
4. Near-miss guard: interactive/REPL output (no reserve) must be byte-identical to before — the
   blank-line spacing there is intentional UX. If an existing test covers REPL spacing, it must
   stay green unedited.

## Honest-null clause
If step 1 cannot reproduce (e.g. the leak is specific to one provider's thinking stream you cannot
trigger), report the exact `od -c` output you got, do not ship a speculative gate, and write the
reading into ARCHITECTURE.md under `tests/print_stdout_contract.rs` as "not reproduced on <date>,
command X, output Y".

## Verify
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`,
then re-run the three `od -c` commands and paste their output into the commit message.
Docs: `docs/src/usage/piped-mode.md` already promises answer-only stdout — only touch it if the
trailing-newline decision needs stating.
