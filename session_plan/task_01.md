Title: `--continue-strict`: refuse to run, before any model call, when the session cannot be restored (#979 half 1)
Kind: product
Files: src/main.rs, src/cli.rs, src/help.rs (+ register row in tests/module_size.rs, + one process test in tests/print_stdout_contract.rs)
Issue: #979

## Why
Reproduced in the Day-216 assessment: `{ not json` in `.yoyo/last-session.json` + `yoyo -p "Reply with exactly: PONG" --continue`
prints `warning: Failed to restore session: invalid type: map, expected a sequence ...`, makes a model call, prints PONG and
**exits 0**. A wrapper can't tell a continued task from one that silently started over. That is a false success on the exit code,
the channel a script actually reads (Day 212 lesson: fix the SIGNAL, not the prose beside it).

Scope of this task is the strict flag ONLY. The stream-json `{"type":"sessionRestored","messages":N}` line is half 2 and is
deliberately NOT in this task (it lives in src/prompt.rs, a 4th source file). Leave #979 open and say so.

## Steps (two, do both)

1. **Pure result seam + strict exit, in src/main.rs.** Factor the body of `restore_session` (main.rs ~1010) into
   `fn restore_session_result(agent: &mut Agent, path: &Path) -> Result<usize, RestoreError>` (or `Result<usize, String>` with
   distinct missing/unparsable messages), returning the restored message count. Keep `restore_session` as the lenient wrapper
   whose stderr text (YELLOW warning on parse failure, DIM "no previous session found" on missing file, the existing
   "resumed session" line on success) is **byte-identical to today** — that is the near-miss and the whole regression surface
   for every existing `--continue` user. Add the strict path at the call site (~main.rs:1224, which runs BEFORE the `-p`
   branch): when strict, on `Err` print
   `error: --continue-strict: could not restore <path>: <reason>` to stderr and `std::process::exit(1)` — before the agent ever
   prompts. Missing file AND unparsable file both refuse in strict mode. Sanitize the interpolated path/reason through
   `cli::sanitize_for_display` (the path can be repo-influenced). Unit tests (tempdir, never the repo's real `.yoyo/`):
   missing → Err naming the path; `{ not json` → Err; a valid saved session (write one via `agent.save_messages()` or a
   minimal valid JSON array the loader accepts) → `Ok(n)` with the real n.

2. **Flag + docs + process test.** In src/cli.rs add `--continue-strict` to the known-flags list (~cli.rs:693; no value, so NOT
   in the needs-value list) and parse it next to `--continue` (~cli.rs:2706) so that it **implies** `continue_session = true`
   plus a strict bit on Config. Document it in src/help.rs next to the `--continue, -c` line (~help.rs:330), e.g.
   `--continue-strict  Like --continue, but exit non-zero (before any model call) if the session can't be restored`.
   Add one process-level test to tests/print_stdout_contract.rs using the existing stub (`start_stub_seq` / `run_yoyo_with`,
   which expose a POST counter): in a tempdir cwd with `.yoyo/last-session.json` = `{ not json`, run
   `-p x --continue-strict` → exit code non-zero, stderr contains `--continue-strict`, **stub POST count == 0**. Pair it with
   the near-miss: same corrupt file, plain `--continue` → exit 0, stub hit (today's lenient behaviour, pinned). Note this file's
   header says every test asserts the stub WAS hit (anti-vacuous); the strict test is the deliberate exception — say so in a
   comment, and its anti-vacuous half is the paired lenient run, which must hit.

## Gates to know before editing
- **src/cli.rs is grandfathered at exactly 7584** in tests/module_size.rs (line ~214). Any growth fails the size gate; paste
  the new count the gate prints into that row and append a `Day 216: #979 — --continue-strict parse` note. Keep cli.rs growth to
  the parse lines only; logic lives in main.rs.
- Positive control, atomic and serial: neuter the strict `exit(1)` (marker `NEUTERED` on the line), confirm the process test
  fails by name, restore in the same command, confirm green.
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` before declaring done.
- Docs: add a line to docs/src/features/sessions.md (or docs/src/usage/single-prompt.md if that is where `--continue` is
  documented) and a CHANGELOG.md Unreleased entry. ARCHITECTURE.md entry for main.rs under its file heading — not CLAUDE.md.
