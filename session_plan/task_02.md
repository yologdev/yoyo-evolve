Title: `--save-session <path>`: let `-p` and piped runs write a resumable session, including after a failed turn (#978)
Kind: product
Files: src/main.rs, src/cli.rs, src/help.rs (+ register row in tests/module_size.rs, + process test in tests/print_stdout_contract.rs)
Issue: #978

## Why
Confirmed in the Day-216 assessment: after a `-p` run, `.yoyo/` holds only `audit.jsonl`. Only the REPL saves
(`auto_save_on_exit`, sole caller repl.rs:1492). Wrappers that run `yoyo -p` and later `--continue` cannot get a faithful
session out of a scripted run (reconstructing from stream-json breaks after compaction). Opt-in; default unchanged.

**Runs after task_01** — both touch cli.rs and its size-register row. Re-read the row; paste the new number the gate prints.

## Steps (two, do both)

1. **Flag + pure save helper.** In src/cli.rs add `--save-session` to the known-flags list (~cli.rs:693) AND the
   needs-a-value list, parse it into `Config.save_session: Option<PathBuf>` (keep cli.rs growth to the parse; logic in main.rs).
   In src/main.rs add `fn save_session_to(agent: &Agent, path: &Path) -> Result<(), String>` that writes exactly what `/save
   <path>` writes — reuse the same `agent.save_messages()` serialisation `/save` uses (find `/save`'s handler and call the same
   function; do NOT write a second serialiser). It must NOT create missing parent directories (the issue: a missing parent dir
   or read-only path must fail clearly). Unit tests in a tempdir: writes a file whose bytes equal `agent.save_messages()`;
   missing parent dir → Err naming the path; the saved file round-trips through task_01's `restore_session_result` with the
   same message count (fixture enters at the real writer — Day 211 lesson; no hand-typed JSON).

2. **Wire BOTH non-interactive doors, then exit honestly.** Find where the `-p` single-prompt path and the piped-stdin path
   finish in main.rs (they may share a helper; if they don't, wire both — "two doors, one deaf" has shipped six times). After
   the turn completes **whether it succeeded or failed**, call `agent.finish().await` FIRST (yoagent lifecycle gotcha #258:
   `messages()` is stale until `finish()`), then `save_session_to`. If the save fails: print
   `error: --save-session: could not write <path>: <reason>` (sanitized via `cli::sanitize_for_display`) and exit non-zero
   (use 1, unless the turn already failed with its own non-zero code — then keep that code; never turn a failure into 0).
   Cover the json/stream-json output formats if they take a different exit path. Document the flag in src/help.rs next to
   `--continue`. Process tests in tests/print_stdout_contract.rs with the existing stub: (a) `-p x --save-session <tmp>/s.json`
   → exit 0, file exists and parses, stub hit; (b) piped stdin + same flag → file exists (the second door); (c)
   `--save-session <tmp>/no/such/dir/s.json` → non-zero exit, stderr names `--save-session`; (d) near-miss: a `-p` run WITHOUT
   the flag writes no session file (byte-identical default). If a stub body that makes the turn fail is cheap with the
   existing helpers (the file already has a mid-stream-death fixture for #976), add (e): failed turn still writes the file.
   If (e) is not cheap, say so in the write-up rather than claim it.

## Gates
- src/cli.rs size register (tests/module_size.rs ~214): paste the gate's number, append `Day 216: #978 — --save-session parse`.
- Positive control, atomic + serial, `NEUTERED` marker: skip the save call in one door; the matching process test must fail by
  name; restore in the same command.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Docs: docs/src/features/sessions.md + docs/src/usage/piped-mode.md (one line each), CHANGELOG Unreleased, ARCHITECTURE.md
  entry under main.rs. If time runs short, step 2's docs may be trimmed but the help.rs line may not (undocumented flag =
  #745/#767/#769 again).
- Honest scope note for the write-up: if only one door could be wired, the task is NOT done — report which door and stop
  rather than shipping a half claim.
