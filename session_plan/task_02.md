Title: `yoyo diff`, `yoyo commit`, `yoyo blame` exit nonzero outside a git repository (#982 slice 2)
Kind: product
Files: src/commands_git.rs, src/commands_git_review.rs, src/dispatch_sub.rs
Issue: #982

## Why

Measured from /tmp (not a git repo) in the Day 217 14:33 assessment: `yoyo diff` prints "error: not in a git repository" and exits **0**, `yoyo commit` does the same, and `yoyo blame src/x.rs` prints "✗ Not in a git repository" and exits 0. A script or CI step gets a green result from a command that did nothing. Slice 1 (838b7e3c) added `exit_if_nonzero(code: i32)` in `src/dispatch_sub.rs` (around line 93) as the single exit path. Use it, and do not add a third.

Handlers (found by grep, line numbers approximate): `handle_diff(input: &str)` at commands_git.rs:252, `handle_commit(input: &str)` at commands_git.rs:1460, `handle_blame(input: &str)` at commands_git_review.rs:491. All three are also called from the REPL.

## Steps

1. **Status-returning cores.** For each handler, add a sibling that returns the status, e.g. `pub fn handle_diff_status(input: &str) -> i32`: 1 when the not-a-git-repo branch fires, and 0 otherwise. Keep the existing `handle_diff(input)` signature as a thin wrapper that calls the core and discards the code, so every REPL call site stays byte-identical and untouched. Do the same for commit and blame. The output text is unchanged. **Do not parse output strings.** The code comes from the branch that prints the error. Only the not-a-git-repo branch changes status in this slice. "Nothing to commit" and an empty diff stay 0, and the commit message must say they were deliberately left at 0.
2. **Dispatch.** In `src/dispatch_sub.rs`, the `diff`, `commit` and `blame` shell arms call the `_status` cores and pass the result to `exit_if_nonzero`. **Before editing, grep the test modules (`src/dispatch_sub.rs`, `src/main_tests.rs`, `tests/`) for in-process dispatch tests of these three arms.** An arm that now calls `process::exit` will kill the test binary if a test dispatches it from a non-git cwd. If such a test exists, change it to call the pure core and assert the returned code, rather than deleting it (never delete tests).

## Tests

- For each core, a test in a **temp dir that is not a git repo**. Pass the dir explicitly if the handler has a cwd seam. If it reads the process cwd, do NOT `set_current_dir` in a parallel test. Instead, test via the built binary: `std::process::Command::new(env!("CARGO_BIN_EXE_yoyo"))` with `.current_dir(tempdir)`, asserting `status.code() == Some(1)` for `diff`, `commit` and `blame x.rs`, and asserting stdout/stderr still contain the original error text. Put the binary tests in a new `tests/` file only if no existing contract-test file fits. A `tests/` file does not count toward the 3 source files, but say so in the commit.
- **Near miss:** `yoyo diff` inside a fresh `git init` temp repo with no changes exits **0**.
- Positive control (one atomic serial command, marker `NEUTERED`): make the diff core always return 0, watch the exit-1 test fail by name, restore, and watch it pass.

## Out of scope (write it into the #982 comment, not the code)

`lint`, `health` and `changelog` (other files: commands_lint.rs, commands_dev.rs, commands_info.rs) and bare `run` are slice 3. The find/grep no-match convention still needs a decision. Post a short comment on #982 listing what this slice covered and what remains.

## Verify

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`. Then run by hand from /tmp: `target/debug/yoyo diff; echo $?` → 1. Update `docs/src/usage/commands.md` only if it documents exit codes for these commands (grep first).
