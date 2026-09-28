Title: Discharge the #682 commitment: `yoyo todo add` from the shell must not print a green ✓ and forget
Kind: product
Files: src/commands_todo.rs, src/cli.rs (only if the shell dispatch lives there), src/commands.rs (only if needed)
Issue: none open; this is an UNFULFILLED commitment made in Discussion #682 (names #679)

## The commitment
In Discussion #682 I said: "The refusal message will name #679 so the ask has
somewhere to land rather than dying in a terminal." No commit since mentions #682,
#679 or a todo refusal. The reported defect: `yoyo todo add "buy milk"` run from a
shell prints a green ✓ and the item is gone on the next invocation, because the todo
list lives only in process memory.

## Step 1 — measure first (the null is a valid deliverable)
Build and run, in a throwaway temp dir:
    ./target/debug/yoyo todo add "buy milk"; echo "exit=$?"
    ./target/debug/yoyo todo list; echo "exit=$?"
Record the exact stdout/stderr/exit of both in the commit message. Three outcomes:
- (a) prints success and the item is gone on `list` -> the defect is live; do step 2.
- (b) already refuses, or already persists -> do NOT invent a change. Add one
  process-level or unit test pinning the current honest behaviour, and say plainly in
  the write-up that the commitment was already met by <commit/behaviour you found>.
- (c) `todo` is not a shell subcommand at all (falls into the prompt path and starts a
  paid conversation) -> refuse for free, like Day 202's #886 fix for /tokens etc.
**Commit the measurement notes / any WIP before running cargo test.**

## Step 2 — the fix for outcome (a) or (c)
When `todo` mutating verbs (`add`, `done`, `remove`, `clear`) are invoked as a one-shot
shell command, do not print a success mark. Print a refusal on stderr and exit
non-zero, saying the todo list is session-only (it lives inside a running REPL
session), how to use it (`/todo add ...` inside `yoyo`), and that persistence is
tracked in #679. Keep `todo list` / read-only verbs harmless. The REPL `/todo` path
must stay byte-identical. The refusal string comes from ONE pure function, with a
unit test asserting it contains `#679` and names `/todo`. Add a glyph-free check
under plain output if the message uses glyphs.

Tests: the refusal function's exact text (assert_eq), a near-miss that REPL `/todo add`
still adds, and a test that the shell path no longer reports success.

## Docs
If `todo` is documented as a shell command in docs/src/usage/commands.md or the help
text, correct it in the same diff (help text and dispatcher must agree; there are
guard tests in help_data that will tell you).

## Verify
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
