Title: #780 — remove the CWD movers from ONE cluster: give `suggest_related_files` a dir-taking seam (10 of 22 sites)
Kind: evolve
Files: src/commands_file.rs, tests/module_size.rs, CLAUDE.md
Issue: #780

## Read this first — this task shape has died 3 times with an EMPTY DIFF

Receipts #790, #791, #797 are all `agent-revert`, and all three are the
**"no progress — likely blocked, NOT too large"** class: the agent exited 0 having
changed nothing. Making this smaller is not automatically the answer; the blocker has
to be named. Here is my reading of it, stated as a hypothesis because the receipts
record no reason:

- **#790** asked for 7 files in one pass ("the whole class, one attribute per site").
- **#797** asked for 3 files in one pass — and those 3 files need **three different
  seams** (a `try_dispatch_subcommand` route, a `handle_undo_last_commit` call, a
  `/cd` production path).
- **#791** was a drift *guard*, which died differently (its own register was wrong).

Every one of them left the *design decision* — "what seam, in which function,
with what signature" — to the implementation agent, for several unrelated functions
at once. **This task removes that decision entirely.** One function. One new
signature, written out below verbatim. If you find yourself deciding anything
architectural, you are off-script.

## The problem (from @yuanhao, #780)

`std::env::set_current_dir` is **process-global** and `cargo test` runs tests as
threads in one process. A test that chdir's corrupts any sibling test that resolves a
relative path *while it runs*. `#[serial]` does **not** fix this: it serialises a test
only against other `#[serial]` tests, so an unmarked CWD-*reading* test is still
exposed. The live victims this window are
`setup::tests::test_wizard_saves_key_when_confirmed` and
`test_wizard_declines_key_and_prints_export_instructions` — both take an explicit
tempdir, both are innocent readers, both went red in CI.

The price: `scripts/evolve.sh` reverts on any `cargo test` failure, so a flake
destroys a correct finished task at random.

**DO NOT adopt a `CurrentDirGuard` / `CWD_LOCK` mutex.** The ecosystem calls that the
idiomatic fix and it is the wrong one here — like `#[serial]`, it protects only
*participants* and leaves the unguarded readers (our actual victims) exposed. The only
fix that reaches them is **removing the mover**.

## Census — measured this session on `main`

```
$ grep -rn "env::set_current_dir" src/ | grep -v ":\s*//" | awk -F: '{print $1}' | sort | uniq -c
     17 src/commands_file.rs
      2 src/dispatch_sub.rs
      2 src/commands_git.rs
      1 src/dispatch.rs      <- production code (/cd), NOT a test. Leave it alone.
TOTAL: 22
```

(`src/context.rs`'s 12 were cleared earlier today. `src/git.rs` shows up in a naive
grep but both hits are comments.)

## Scope — exactly one cluster, 10 of the 17 sites

`src/commands_file.rs` holds two independent clusters. **Do only the second one:**

- lines ~1961–2209 — the `apply_patch` tests (7 sites). **OUT OF SCOPE.** They shell out
  to `git apply` in the process cwd; that needs production changes. Leave untouched.
- lines ~2655–2790 — the `suggest_related_files` tests (**10 sites, ~5 tests**). **THIS IS
  THE TASK.** Each is a clean pair:

```rust
let orig_dir = std::env::current_dir().unwrap();
std::env::set_current_dir(dir.path()).unwrap();
let suggestions = suggest_related_files("src/foo.rs", &[]);
assert!(suggestions.contains(&"src/foo_test.rs".to_string()), ...);
std::env::set_current_dir(orig_dir).unwrap();
```

## What to do

1. **Add the dir-taking seam.** Rename the body of `suggest_related_files` to

   ```rust
   pub(crate) fn suggest_related_files_in(root: &Path, path: &str, added: &[String]) -> Vec<String>
   ```

   resolving every filesystem probe against `root.join(...)` instead of the bare
   relative path, and return paths in the **same relative form as today** (the tests
   assert `"src/foo_test.rs"`, not an absolute path — keep that true).

   Then keep the public name as a thin wrapper so **every production call site is
   byte-identical**:

   ```rust
   pub fn suggest_related_files(path: &str, added: &[String]) -> Vec<String> {
       suggest_related_files_in(Path::new("."), path, added)
   }
   ```

   This `_in` convention already exists in this codebase — `context.rs` has
   `get_project_file_listing_from` / `load_project_context_from`, and the same technique
   already cleared `watch.rs`, `setup.rs`, `commands_goal.rs` and `context.rs`. Copy it,
   do not invent a new one.

2. **Rewrite those ~5 tests** to call `suggest_related_files_in(dir.path(), "src/foo.rs", &[])`
   and delete **both** `set_current_dir` lines from each. Assertions stay exactly as they
   are. Delete the now-unused `let orig_dir = ...` bindings (an unused binding fails
   clippy under `-D warnings` and reverts the task).

3. **Remove `#[serial]` from the tests you just cleaned** — and *only* those. A test that
   no longer touches the CWD does not need it, and leaving it there keeps a false record
   of which tests are dangerous. Do **not** touch `#[serial]` anywhere else.

## ⚠️ The trap that will revert this task if you miss it

`tests/module_size.rs` carries `("src/commands_file.rs", 2809)` in
`GRANDFATHERED_OVERSIZED_MODULES`. **Branch 3 of that gate is FATAL when a registered
file drops BELOW its recorded line count** — it is a ratchet, deliberately. This task
*deletes* lines, so the file will shrink and `cargo test` **will fail** unless you
update that entry to the new actual count in the same commit. The failure message
prints the exact `("path", N)` line to paste. Do not skip this; it is a `git reset --hard`.

## Done when

- `grep -c "env::set_current_dir" src/commands_file.rs` returns **7** (down from 17).
- `cargo build && cargo test && cargo clippy --all-targets -- -D warnings` all green.
- `tests/module_size.rs` register entry updated to the new count.
- CLAUDE.md: add one sentence to the `commands_file.rs` area (or create the bullet if
  absent) recording that `suggest_related_files_in(root, …)` is the seam and
  `suggest_related_files` is the `Path::new(".")` wrapper, with the #780 reason. Keep it
  short — one or two sentences, not an essay.

## Deliberately not in scope (say so, do not silently drop)

The 7 `apply_patch` sites in the same file, the 4 sites in `dispatch_sub.rs` /
`commands_git.rs`, and the production `/cd` site in `dispatch.rs`. If you finish early,
**stop** and report the remaining census rather than starting a second cluster — a
half-done second cluster is what reverts the first one.
