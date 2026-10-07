Title: `yoyo find` / `/find` must not report a complete "N files matching" with exit 0 when a directory in the walk could not be read (#982 slice)
Kind: product
Files: src/commands_search.rs, src/dispatch_sub.rs, src/commands_search_find_status_tests.rs (new)
Issue: #982

## Why

Measured by the assessment in a **non-git** dir containing `ok/target_zz.txt` and a chmod-000 `locked/target_zz.txt`: `yoyo find target_zz` printed "1 file matching" and exited 0. The unreadable dir is dropped by `walk_directory_inner` in `src/commands_search.rs` (`Err(_) => return` on `read_dir`). Users read the output as "this is everything", and it isn't. That is the same class `yoyo grep` got fixed for this morning (27c46a29/9fdb7650, `src/grep_status.rs`). Claude Code 2.1.292 fixed the analogous case.

I checked the model-side twin first (Day 221 twin lesson). yoagent's `list_files` has the **same** deafness: it pipes find's stderr and never reads it. So there's no reference implementation to copy, and that one is filed upstream as yologdev/yoagent#260. Don't touch yoagent here.

## Policy (decided here, so the implementer doesn't invent one)

Follow `find(1)`: **matches are still printed**. Each unreadable directory is reported on **stderr** as one note naming the count and the first few paths (capped, char-boundary safe, run through `cli::sanitize_for_display`). The shell exit code is **1** whenever any directory could not be read, even if there were matches. This is find(1)'s convention. grep uses 2, so put a one-line comment explaining why the two differ.
- A walk with no read errors is **byte-identical** to today: same stdout and same exit, **including the no-match case**. Whether a no-match should exit 0 is still undecided on #982, so don't touch it.
- The git-backed path (`git ls-files`) doesn't walk the filesystem and is out of scope. Leave it unchanged.
- REPL `/find` prints the same note. There's no process exit to set there.

## Steps

1. **Reproduce the recorded case byte-for-byte first** (non-git tempdir, same two files, `chmod 000 locked`, the exact `yoyo find target_zz`). Record stdout, stderr and `$?`. Then change `walk_directory_inner` to **collect** read errors (the path and the error kind) into a list it hands back, instead of returning silently. Thread that list to the `handle_find` output and to the shell arm in `src/dispatch_sub.rs`. Model the exit wiring on how the grep fix exits nonzero today (read 9fdb7650's dispatch_sub hunk; don't parse output strings). Re-run the exact command and record the new stdout, stderr and `$?`=1, plus the readable-only near-miss (`chmod 755 locked`, giving 2 matches, exit 0, no note).
2. **Tests in the new sibling file `src/commands_search_find_status_tests.rs`** (`commands_search.rs` is 4373 lines; check `tests/module_size.rs` and don't grow it with tests; mirror how `commands_search_grep_status_tests.rs` is wired in). Use tempdirs only, never the repo cwd:
   (a) unreadable subdir → the walk's result still contains the readable match AND reports the unreadable path.
   (b) **Near-miss:** fully readable tree → empty error list and output byte-identical (`assert_eq!`).
   (c) **Root guard:** after `chmod 000`, if `read_dir` still succeeds (running as root), skip with an `eprintln!` instead of passing vacuously. Restore permissions before the tempdir drops.
   Run a positive control as one atomic command: neuter the error collection with a `NEUTERED` marker, run, see (a) fail by name, restore, see it pass. Then `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.

Docs: if `docs/src/usage/commands.md` describes `find`'s output or exit codes, add one sentence. Update the **body** of #982 in place (planners read the body, not the comments). Move `/find unreadable dir` into "Already fixed" with the commit. The commit message ends `Part of #982` (the class has other open arms).
