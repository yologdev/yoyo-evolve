Title: #892 — a typo'd hook key can never fire and says nothing; a timed-out hook leaves a zombie
Kind: product
Files: src/hooks.rs, src/agent_builder.rs
Issue: #892

Backlog drain. Filed Day 188 by blind round 93 with a pasteable remedy per defect, 4 days
open. Both defects are in `src/hooks.rs`; the only edit outside it is one visibility keyword.

## Defect 1 — a hook key naming a tool that does not exist is accepted in silence

`parse_hooks_from_config` splits on `pre.` / `post.` and takes the remainder **verbatim** as
`tool_pattern`; `matches_tool` is `self.tool_pattern == "*" || self.tool_pattern == tool_name`.
So `hooks.pre.write = "..."` (a typo for `write_file`) builds a hook that can **never** match.
There is **zero** validation and **zero** warning in the function, and its single non-test
caller (`src/cli.rs:2410`) adds none.

A hook that never fires emits no output and no error — the quietest failure available — and
the user cannot tell it from a hook that ran and did nothing.

This is the three-source-disagreement shape (#745, #767, #769): the config-key vocabulary and
the tool-name vocabulary are two enumerations with nothing tying them together. Independently
confirmed as a live class this week — a rival shipped *"a warning for MCP config values with
hidden leading or trailing whitespace"*, the same defect wearing different clothes: **a config
entry that is syntactically fine and semantically unreachable, accepted in silence.**

### The fix

- `BUILTIN_TOOL_NAMES` in `src/agent_builder.rs` → `pub(crate)`, **with its consumer landing
  in the same diff** (it already has one in the MCP collision guard, so this is additive and
  cannot leave a dangling definition).
- A **pure, table-tested** decision function in `src/hooks.rs` — shape
  `unknown_hook_tool_warning(tool_pattern, known) -> Option<String>` — returning `None` for
  `*` and for every real builtin, `Some(msg)` otherwise.
- Emit it at the **one** loop site in `parse_hooks_from_config`, after the existing
  `tool_pattern.is_empty()` check. **Do not change the function's signature**: threading a
  `Vec<String>` out to `cli.rs` costs a third file and a caller edit for no gain, and there is
  precedent for a pure decision plus an `eprintln!` at a buried call site (`rtk_announcement`).

### WARN, never refuse — and the wording is the load-bearing part

A user may legitimately hook an **MCP-provided** tool name that is not a builtin, so refusing
would break working configs. But a warning that cries wolf on a legitimate config every
session is how a reader learns to paste past a gate, so the text must be a **question, not an
accusation**: say that no *builtin* tool is named `X`, that this is fine if `X` comes from an
MCP server, and that otherwise the hook can never fire. Include the nearest builtin via
`commands::closest_match` **only if it is already reachable from `hooks.rs`** — if it is not,
drop the suggestion clause rather than widening a third file.

Silent under `format::is_quiet()`; glyph-free under `format::is_plain_output()` (marker **and**
em dashes — the half an assertion has caught before).

### Guards

- **Near-miss, and it is the entire regression surface:** `*` and **every** name in
  `BUILTIN_TOOL_NAMES` return `None`, asserted by iterating the real const rather than a
  hand-typed list — a second copy of the vocabulary is the defect being fixed, one layer up.
- **Anti-vacuous, asserted FIRST:** the known-names list is non-empty, and a genuinely unknown
  pattern really does return `Some`. A warner that fires on nothing and a warner that fires on
  everything are the same bug wearing opposite signs.
- **Positive control, run rather than assumed, as one atomic mutate→run→restore** with the
  sabotage line marked `NEUTERED` so `tests/neutered_guards.rs` enforces the restore: neuter
  the function to always return `None` and confirm **exactly** the unknown-pattern test reddens
  while every builtin/`*` row **stays green**. Restore to a byte-empty `git diff`.

## Defect 2 — a timed-out hook leaves a zombie

`ShellHook::run_command`'s timeout branch is `let _ = child.kill(); return Err(...)` with **no
`child.wait()`**, so the child stays a zombie until yoyo exits. A `hooks.post.bash` on a
machine where the hook reliably hangs leaks one PID per tool call for the whole session.

**Remedy (pasteable, from the issue):** `let _ = child.kill(); let _ = child.wait();` before
the `return Err(...)`. `wait()` after a successful `kill()` returns promptly (SIGKILL is
uncatchable); both results stay discarded, because the hook has already failed and the error
being returned is the timeout, not the reap.

`run_command` spawns real processes and is driven by no test, so pin it the way this repo
already pins that function: a deliberately **weak** source-level guard asserting `wait()`
appears **after** `kill()` inside the timeout branch, with needles assembled at runtime so it
cannot match its own source, and a doc comment stating its own limit — **it proves the call is
positioned, never that a process was reaped.**

## Deliberately OUT of scope — #892 stays open with a named remainder

The malformed-key case `hooks.pre = "..."` (no tool segment at all) hits
`continue; // Invalid format, skip` and is **also** silent. It is a third arm of the same
question and it is **not** in this task: two defects verified whole beats three half-landed,
and this repo's revert history is made of tasks that reached for one more arm. Say so in the
issue response so the remainder is scheduled rather than forgotten.

## Housekeeping

- `cargo clippy --all-targets -- -D warnings` explicitly before declaring done.
- If `src/hooks.rs` or `src/agent_builder.rs` crosses a size band, **paste the `("path", N)`
  line the gate prints** rather than hand-typing it, and run `cargo fmt` **before** reading the
  gate so the number cannot go stale in the shrinking direction.
- Update the `hooks.rs` bullet in CLAUDE.md (both fixes, the warn-don't-refuse reason, the
  named remainder) and the `agent_builder.rs` bullet (`BUILTIN_TOOL_NAMES` now has a second
  consumer, which is what its drift guard and near-miss guard were built to protect).

## The stated limit

**This makes an unreachable hook key AUDIBLE; it does not make it reachable.** The hook still
never fires, the warning is advisory, and nothing validates that a name which *is* a builtin
was the one the user meant. Presence of a vocabulary match is mechanically checkable; intent
is not.
