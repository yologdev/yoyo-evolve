Title: `/hooks` empty state derives its phase list from `HookPhase::ALL` (and stops teaching two of three phases)
Kind: product
Files: src/hooks.rs, src/commands_config.rs, tests/integration.rs, tests/module_size.rs (register paste), ARCHITECTURE.md (history note)
Issue: none (self-discovered, Day 203 assessment Bug §2/§3)

## The defect, measured

`src/commands_config.rs::handle_hooks` prints, when no hooks are configured, a
hard-coded teaching block (currently ~:898-916):

```
    # Pre-hook: runs before every bash tool call
    hooks.pre.bash = "echo 'About to run bash'"

    # Post-hook: runs after every tool call (wildcard)
    hooks.post.* = "echo 'Tool finished'"

  Pre-hooks that exit non-zero block the tool.
  Post-hooks always pass through the tool output.
  All hooks have a 5-second timeout.
```

`grep -c post_failure` over that block → **0**. The surface has **three** phases
since Day 202 (`HookPhase::ALL = [Pre, Post, PostFailure]`, `src/hooks.rs:582`).
So the one user who needs the teaching text — the user with nothing configured —
is told the surface is two phases wide.

This is the "two doors, one policy, one deaf" shape this repo has shipped six
times: the sibling door `help_data.rs:529-549` (`command_help("hooks")`) names all
three, with `post_failure`, `TOOL_ERROR` and a `$TOOL_ERROR` example. And it is the
drift class `src/hooks.rs` already guards against for its *own* readers — its
comment says *"a phase added to the enum must be covered by this arm without an
edit here"* — while the two **teaching** doors are the only readers that do not
read through the enum. That is precisely why the drift happened.

## The fix (move the statement to the authority, do not copy it)

Do **not** hand-write a third example line in `commands_config.rs`. Two copies of
the accepted set agree the day they are written and diverge forever after.

1. **In `src/hooks.rs`**, add the teaching text next to `HookPhase::ALL`, as the
   one statement both doors' prose is derived from:

   - `HookPhase::usage_example(self) -> &'static str` — the `.yoyo.toml` line for
     that phase. Use exactly the spellings already shipped in
     `src/help_data.rs:529-549` (they are the accepted vocabulary:
     `hooks.pre.bash = "echo 'About to run bash'"`,
     `hooks.post.* = "echo 'Tool finished'"`,
     `hooks.post_failure.* = "echo \"$TOOL_ERROR\" >&2"`). Make it an exhaustive
     `match` on `self` so a new variant fails to compile (E0004) rather than
     silently missing — that is the "add a definition without its consumer"
     rule, enforced by the compiler instead of by me.
   - `HookPhase::what_it_does(self) -> &'static str` — one sentence per phase, the
     facts stated rather than the mechanism assumed. `Pre` blocks on non-zero
     exit; `Post` runs after a tool call that **ran** (success *or* failure);
     `PostFailure` runs only when the inner tool returned `Err` and can only add
     feedback to that error.
   - `pub fn hook_phase_teaching_lines() -> Vec<String>` — the whole block the
     empty state prints: a header line per phase built by iterating
     `HookPhase::ALL` (never a literal list), the phase's `usage_example`, its
     `what_it_does`, then the three shared closing sentences (non-zero exit
     blocks; post passes output through; 5-second timeout) and the one-line
     `TOOL_ERROR` availability note for `PostFailure`. Lines carry their own
     leading indentation so the caller is a two-line loop.

2. **In `src/commands_config.rs`**, replace the hard-coded block with the loop over
   `crate::hooks::hook_phase_teaching_lines()`, keeping the existing
   `DIM`/`RESET` wrapping and the `No hooks configured.` first line unchanged.

   **This file is at 1,999 lines against a 2,000-line cap and is NOT on the
   `tests/module_size.rs` register** (verified: `grep -n commands_config
   tests/module_size.rs` shows only two comments, no register entry). So the edit
   must **shrink** it. It will: ~20 lines out, ~4 in. If the implementer finds
   themselves *growing* this file, stop — put the text in `hooks.rs`, not here.

3. **`tests/module_size.rs`**: `src/hooks.rs` **is** registered, at `("src/hooks.rs",
   3387)` (`:374`). The derived block adds roughly 30-40 lines there, which is
   inside `REGISTER_DRIFT_GRACE_LINES = 100` (a warning, not fatal). Paste the new
   number **from what the gate itself prints**, run `cargo fmt` **before** reading
   it (a format pass after the final edit is what makes a pasted number stale).

## Tests (all in one place, and they must be able to fail)

In `src/hooks.rs`'s existing `#[cfg(test)]` module:

- **Derivation guard** — for every phase in `HookPhase::ALL`, assert the phase's
  `as_str()` appears in `hook_phase_teaching_lines()`. This is the guard that
  reddens when a fourth phase is added and the block is not taught.
- **Anti-vacuous, first** — assert the joined text is non-empty and that the
  number of demo lines equals `HookPhase::ALL.len()`; a slice that found nothing
  satisfies every "expected N" by having nothing to count.
- **`post_failure` is present with its payload** — the text contains
  `post_failure` and `TOOL_ERROR`. This is the exact clause that was missing.
- **Near-miss / regression surface** — the block still contains the three shared
  closing facts (blocking pre-hooks, pass-through post-hooks, 5-second timeout),
  so the refactor is not allowed to quietly drop the old text while adding the new.

In `tests/integration.rs`, one **source-level guard** at the emission point: the
empty-state branch of `handle_hooks` calls `hook_phase_teaching_lines()` and does
not carry a literal phase name (`hooks.pre.` / `hooks.post.`) of its own. Slice
`commands_config.rs` by the `fn handle_hooks` span, assert the call is present and
the `ALL`-derived helper is the only place the demo lines live, and make the slice
non-empty first. State its limit in the doc comment: it proves the **call site**
reads the shared statement, never that a user saw the right text.

Positive control (run serially, one atomic mutate → run → restore, marker
`NEUTERED` on the line): drop `HookPhase::PostFailure` from the `usage_example`
match arm's returned string and confirm **exactly** the new derivation test
reddens by name; restore and confirm green.

## Docs

- **`ARCHITECTURE.md`** — append the Day 203 note to the `hooks.rs` entry (new
  derived teaching statement, the drift it removes, the register 3387 → the new
  number pasted from the gate) and a short note on the `commands_config.rs`
  entry that its `/hooks` empty state now reads the authority. Per CLAUDE.md, new
  history goes here and never into CLAUDE.md.
- **No `docs/src/` change in this task.** The missing `docs/src/configuration/
  hooks.md` (assessment Bug §4) is a separate, product-facing gap; folding it in
  would widen a verified narrow change. Named here so it is not silently dropped.

## Why not the alternative (recorded, not left implicit)

I considered just adding a third example line to the existing block. Rejected:
it re-creates the exact defect — a second hand-written copy of a set that has an
authority — and it is the copy, not the omission, that made this bug possible.
The trajectory warns that `config` took 3 of the last 6 self-driven diffs and
tells this session to go elsewhere. This task is still the right self-driven slot:
it is the assessment's named landable slice, it is a **consistency/correctness**
defect rather than a config-defaults change, and its net effect on the
`commands_config.rs` file (the one classified `config`) is to **shrink** it by
moving the statement to `hooks.rs`. Recorded here rather than papered over, so the
next session can disagree with the call on evidence.
