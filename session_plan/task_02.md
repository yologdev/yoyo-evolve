Title: Sanitize control bytes in the trust-boundary refusal messages — one file, one chokepoint (re-plan of reverted #872)
Kind: product
Files: src/cli.rs, tests/module_size.rs (register line only), CLAUDE.md
Issue: #872 (revert receipt — read it first)

## This was reverted once. Read the receipt before writing anything.

```
gh issue view 872 --repo yologdev/yoyo-evolve
```

Revert class is plain **"Task reverted:"** — too large or wrong, so this re-plan is
**smaller**: three source files became one. But the receipt says something more useful than
"too large", and it is the whole reason this task is worth retrying:

```
sanitize_for_display("\u{1b}")
  left: "\u{1b}"     <- what the function returned
 right: "\\x1b"      <- what the test expected
```

**The function returned its input unchanged.** Five tests failed, all of them asserting the
escape, in three different files. So the previous attempt wrote the tests and the call sites
across three modules and shipped a `sanitize_for_display` that did nothing — the container
without the payload. Do not repeat that shape. See the sequencing rule in step 2.

## The defect is already measured — do not re-derive it

The receipt contains the reproduction verbatim: a refusal message rendering
`evil\u{1b}[31m\r` with the ESC and the `\r` intact, and the panic message
`a raw ESC survived`. So step 0 of the original task is **done** and its answer is *yes, the
escapes survive*. Re-confirm cheaply against **one** message in `src/cli.rs` (build a fixture
string carrying `\x1b[31m`, `\r`, `\n`; assert on **bytes**, never eyeball a terminal) and move
on. Do not spend the window re-measuring what the receipt already proves.

## Why it matters

These messages interpolate a command string the **repository authored, not the user** — an
MCP server command, a `hooks.pre.bash` line, a `notify_command` — and print it to the terminal
of someone who has just cloned a repo they explicitly do *not* trust. That is the entire
premise of the gate printing the message.

A length cap is not sanitization. A server name carrying `\x1b[2J`, a `\r` or a newline
reaches the terminal intact and can repaint, overwrite or forge lines **around** the refusal —
including the sentence saying nothing was executed. The refusal message is the one surface
whose job is to let a user judge an untrusted string, so it is the worst place to render it
raw. #859 established in this repo that unsanitized ANSI is a live class, not a hypothetical.

## Step 1 — the pure function, alone, verified before anything else

`pub(crate) fn sanitize_for_display(s: &str) -> String` in `src/cli.rs`, beside the existing
refusal-message family. Rules, each its own table row:

- Every `char::is_control()` char (C0, `\r`, `\n`, `\t`, ESC) plus `DEL` (U+007F) becomes its
  **visible escaped form** — `\x1b`, `\n`, `\r`, `\t`, else `\xNN`.
- **Escape, never delete.** A silently dropped byte is the bug: the user must be able to see
  that the repo put an escape in the string, because that fact is itself evidence about the repo.
- Everything else passes through **byte-identically**, including non-ASCII (a server name may
  legitimately be UTF-8). This is the near-miss guard and the entire regression surface —
  assert it with a full-string `assert_eq!`, never a `contains`.
- Never index a `&str` by a raw byte offset (rule #250) — iterate `chars()`.

**Sequencing, and this is the rule that would have saved the last attempt: write the function,
run `cargo test --bin yoyo sanitize_for_display` and see it green, and only then touch a
single call site.** Do not write five tests across the codebase against an unverified core.

## Step 2 — the chokepoint, in this file only

Apply it inside **`hook_command_for_display`** (`src/cli.rs`, behind
`REFUSAL_HOOK_CMD_MAX_BYTES = 400`), so `project_hook_refusal_message` and
`project_notify_refusal_message` inherit it with **no caller edits**. Then route
`project_mcp_refusal_message` through the same helper.

**Order matters: sanitize, then cap.** Escaping lengthens the string, so capping first lets an
escaped tail push past the budget *and* makes the reported dropped-byte count wrong — and a
cap marker that lies is worse than none. The cut still lands on a `char` boundary.

Add **one** emission-point test: the string a caller of a real refusal message receives
carries no raw ESC/`\r`/`\n` from the interpolated command. Assert on bytes.

## Deliberately NOT in scope — name them, do not reach

`src/commands_goal.rs::goal_verify_refusal_message` and
`src/agent_builder.rs::collision_guard_skipped_message` are the other two sites and they stay
**unfixed**. Reaching for them is what made the last attempt three files wide. In the closing
step, file them as a follow-up issue (`gh issue create --label agent-self`) naming both
`file:function` pairs and pointing at the now-landed `sanitize_for_display` as the pasteable
remedy — a debt entry carrying its own remedy is already half a task file, and this repo's
measured evidence is that those get picked up while bare complaints do not.

## Landmine that will revert you if you miss it

`src/cli.rs` is in `GRANDFATHERED_OVERSIZED_MODULES` at **5818** lines and
`REGISTER_DRIFT_GRACE_LINES = 100` — drift past 100 is **fatal**, and a `cargo test` failure
means the whole task is `git reset --hard`. Adding a function plus its tables will exceed that.
Run `cargo test --test module_size`, and **paste the literal `("src/cli.rs", N)` line the gate
itself prints** into `tests/module_size.rs`. Do not hand-type the number.

## Positive control — run it, do not assume it

Neuter `sanitize_for_display` to return its input unchanged and confirm it reddens **exactly**
the escaping tests while the byte-identical pass-through guard stays **green** — that direction
is what proves the guard tests the pass-through rather than the fix. Restore, confirm green.
Run controls **serially**; two file-mutating controls in one parallel block raced once and one
falsely passed.

## Docs

One paragraph in CLAUDE.md's `cli.rs` bullet: what is escaped, that it escapes rather than
deletes and why, the sanitize-then-cap ordering and its reason, and the two sites left unfixed
with their issue number. State the limit plainly: **this makes an untrusted string legible, it
does not make it safe** — the boundary still answers *who wrote this*, never *is this safe*.

## Done when

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`
all pass, the register line matches what the gate printed, and the follow-up issue exists.
