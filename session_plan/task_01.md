Title: Stop dispatch tests from running the real setup wizard / init handler in the repo root (#962, plus the init sibling)
Kind: evolve
Files: src/dispatch_sub.rs, src/setup.rs (only if a seam is missing), ARCHITECTURE.md
Issue: #962

## Why this is first
This test cost three whole sessions today. `dispatch_sub::tests::test_try_dispatch_subcommand_setup_bare`
(`src/dispatch_sub.rs` ~1399) calls `try_dispatch_subcommand(["yoyo","setup"])`, and that runs the REAL
`setup::run_setup_wizard` against the process cwd, which is the repo root. With `ANTHROPIC_API_KEY` set
and stdin at EOF (both true in the evolve job), it overwrites `.yoyo.toml` and writes `.yoyo.toml.bak`.
The creator reproduced it with `ANTHROPIC_API_KEY=dummy cargo test --bin yoyo test_try_dispatch_subcommand_setup_bare < /dev/null`.
Commit `8a437178` swept the clobbered file into main, and every call after that got a 404 while the runs showed green.

Sibling in the same class. Per the Day-205 rule, the sibling goes in the SAME task:
`test_try_dispatch_subcommand_init_bare` (~1410) calls the real `handle_init()` (`commands_project.rs` ~648),
which writes `YOYO.md` into cwd. It is harmless here only because CLAUDE.md already exists.

## ORDER OF EVENTS: follow it literally
1. **Commit before any cargo run.** Put the first edit in, then `git add src/dispatch_sub.rs && git commit -m "wip: #962"`.
   Then run cargo. Commit again at every green checkpoint. The Day-207 sessions lost finished work by skipping this.
2. **Never run the reproduction against the real checkout.** If you want the positive control (the old test
   clobbering the config), run it in a throwaway worktree:
   `git worktree add /tmp/wt962 HEAD~N && cd /tmp/wt962 && ANTHROPIC_API_KEY=dummy cargo test --bin yoyo <name> < /dev/null; git -C /tmp/wt962 diff --stat`
   then `git worktree remove --force /tmp/wt962`. Running it in the repo root would repeat the exact incident.
   Before every commit, check `git status` and make sure `.yoyo.toml` is NOT modified. If it is, `git checkout -- .yoyo.toml`
   and do not commit it. `.yoyo.toml.bak` is gitignored.

## Steps
1. Read `try_dispatch_subcommand` and the two tests. Pick the smallest honest fix, in this order of preference:
   (a) If dispatch can be asserted without executing (a pure classifier that recognises `setup` / `init`, or a
       seam that takes a dir), have the tests assert the classification and execute nothing.
   (b) Otherwise, drive the wizard through the existing dir-taking seam `setup::run_wizard_interactive_in(dir, reader, writer)`
       (`src/setup.rs` ~348) with a `tempfile` dir and a scripted reader. Do the same for init if a dir-taking
       variant exists. If none exists, add a minimal `handle_init_in(dir)` and have `handle_init()` call it with cwd.
       Keep `handle_init()`'s behaviour byte-identical.
   Do not weaken what the tests prove about dispatch. Each must still fail if `setup` / `init` stopped routing.
2. Add assertions that make the regression visible. The tempdir must receive the written file, which proves the
   write happened there. Then add a check that the repo-root `.yoyo.toml` bytes are the same before and after,
   captured with `std::fs::read` in the test. **Do NOT set env vars in the test** (`std::env::set_var` races other
   tests; see `tests/global_state_races.rs`). The tempdir routing is the fix, and the byte comparison is a cheap belt.
3. Census, read-only: grep the rest of `dispatch_sub.rs`'s tests for other `test_try_dispatch_subcommand_*` that
   run a real handler that WRITES to cwd (lint, security, etc.). Do not fix them in this task unless they are
   one-line. List each by name, with which file it writes, in the ARCHITECTURE.md entry for `src/dispatch_sub.rs`.
   If the list is empty, write "census: 0 other writers" and name the grep you ran. That null is a deliverable.
4. `tests/module_size.rs` registers `("src/dispatch_sub.rs", 2162)`. If the file grows, follow that file's documented
   rule for grandfathered modules (re-paste the line only if the rule allows growth). Otherwise move helpers so the
   line count does not rise.
5. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`. Confirm with
   `git status` that `.yoyo.toml` is unmodified, then commit.

## Tests (required; an unwritten test is green by construction, so the evaluator checks these by name)
- The setup-dispatch test runs in a tempdir (or executes nothing) and asserts the repo `.yoyo.toml` is unchanged.
- The init-dispatch test likewise, asserting that no `YOYO.md` appears in the repo root.
- Existing dispatch coverage is not weakened.
