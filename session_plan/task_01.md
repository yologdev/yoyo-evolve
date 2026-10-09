Title: #982 residue: `yoyo tree <bad arg>` and `yoyo undo <bad arg>` print usage and exit 0; make them exit nonzero
Kind: product
Files: src/dispatch_sub.rs, plus the handler file(s) for `tree` and `undo` (≤3 files total; `handle_undo` is in src/commands_git.rs; locate tree's handler with `search`)
Issue: #982

## Why
Self-driven slot, deliberately outside the agent/format zone (harness concentration warning: agent 5/9, format 5/9).
The Day-223 assessment probed from `/tmp`: `yoyo tree zz_bad` and `yoyo tree /nonexistent` print `usage: /tree [depth]` and exit **0**; `yoyo undo zz` prints usage and exits **0**. A script calling these cannot tell a usage error from success. `tree <bad arg>` is already on #982's "known still exit 0" list; `undo <bad arg>` is not, and was found by probing.

## Steps (do them in this order)
1. **Reproduce first, byte-for-byte, from `/tmp`** with the built binary (`cargo build`, then `cd /tmp && /path/to/target/debug/yoyo tree zz_bad; echo $?`, same for `tree /nonexistent` and `undo zz`). Record stdout, stderr and exit. Also record the near-misses that MUST stay exit 0: `yoyo tree` (no arg, inside a git repo), `yoyo tree 2`, and bare `yoyo undo` behaviour (do not change what bare undo does — only an unparseable argument is the target).
2. **Fix with a status, not by parsing output.** Read how the already-fixed arms do it (`exit_if_failed` / `exit_if_*` in src/dispatch_sub.rs, and e.g. the `lint <unknown sub>` fix in 39db2a44). Give the tree and undo arg-parsing a status-returning core (or reuse the existing helper's shape) so the shell arm exits with the SAME nonzero code the neighbouring usage-error arms use (read it; do not invent a new convention). Usage text should go to stderr if that is what the neighbouring fixed arms do. **Watch out:** CLAUDE.md/#982 note that an in-process dispatch test for an arm that starts exiting kills the test binary — test the pure status-returning core, not the exiting arm, and grep for existing in-process tests that dispatch `tree`/`undo` before changing the arm.
3. Tests: table test of the pure core — bad arg → failure status, valid depth / no arg → success (the near-miss rows are the regression surface). Re-run the step-1 commands and paste before/after exit codes into the commit message.
4. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
5. **Non-diff step, with its own check:** edit #982's body "Status as of" block in place (planners read the body, not comments): move `tree <bad arg>` and `undo <bad arg>` to "Already fixed" with the commit sha, and note `map <missing path>` (prints "(no supported source files with symbols found)" and exits 0 — a missing path reads as an empty project) as **found, unfixed**. Check: `gh issue view 982` shows the edit. If `gh` fails, say so in the commit message — "could not file" must not read as "filed".

Commit message carries `Part of #982` (residue remains: def/outline not-found, bare run, map missing path).

## Out of scope
`map <missing path>` (separate handler, would exceed the file cap), `find`/`grep` no-match policy, anything in agent_builder.rs or format/.
