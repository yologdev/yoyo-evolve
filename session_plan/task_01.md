Title: Fix #730 — bring /checkpoint and /context under the subcommand-drift guard (18 tables → 20)
Kind: evolve
Files: src/help_data.rs, src/commands_fork.rs, src/commands_project.rs
Issue: #730

## What this is

#725 shipped the table-driven subcommand-drift guard in both directions over
`subcommand_tables()` in `src/help_data.rs`'s test module. That fixture holds **18**
`(cmd, prefix, table)` triples. Two subcommand tables exist and are **not** covered,
for exactly one reason: they are private to their modules, so the fixture cannot
name them.

- `CHECKPOINT_SUBCOMMANDS` — `src/commands_fork.rs:374` — `["save", "list", "restore", "diff", "delete"]`
- `CONTEXT_SUBCOMMANDS` — `src/commands_project.rs:20` — `["system", "tokens", "files", "relevant"]`

So `/checkpoint` and `/context` each carry the same three hand-maintained copies of
their verb set (dispatcher, completion table, prose help) with nothing keeping them
in agreement — the shape that produced #702 (`/todo list`) and today's `/map --depth`
phantom, both found by accident rather than by a test.

## HARD CONSTRAINT — read this before writing any code

`src/help_data.rs` is currently **1979 lines** and is **NOT** in
`GRANDFATHERED_OVERSIZED_MODULES` in `tests/module_size.rs`. The cap is **2000**.
You have **~21 lines of headroom in that file** and the size gate reverts the WHOLE
task if you cross it (that is #719, verbatim: a correct fix thrown away at end of
turn).

Budget accordingly:
- terse fixture entries, no multi-line comment blocks in `help_data.rs`
- run `wc -l src/help_data.rs` **before you finish**, not after the evaluator does

Do **not** raise `src/help_data.rs` into the grandfather list to buy room. A signed
raise is evidence the smaller design was never attempted (Day 161 lesson).

## Steps (2 steps — both must land in this pass)

**Step 1 — widen visibility and register both tables.**
- `src/commands_fork.rs`: `const CHECKPOINT_SUBCOMMANDS` → `pub(crate) const CHECKPOINT_SUBCOMMANDS`.
- `src/commands_project.rs`: `const CONTEXT_SUBCOMMANDS` → `pub(crate) const CONTEXT_SUBCOMMANDS`.
- `src/help_data.rs`, in the test module: add two triples to `subcommand_tables()`
  (`("checkpoint", "/checkpoint", crate::commands_fork::CHECKPOINT_SUBCOMMANDS)` and
  `("context", "/context", crate::commands_project::CONTEXT_SUBCOMMANDS)`), and the
  matching two arms to `dispatcher_source()` — `"checkpoint" => "src/commands_fork.rs"`,
  `"context" => "src/commands_project.rs"`. `dispatcher_source` is a total match that
  panics on an unknown command, so a missing arm fails loudly; that part is already safe.

**Step 2 — run the two guards and resolve whatever they surface, honestly.**
`cargo test` and read the failures from
`test_every_subcommand_table_is_documented_in_usage_form` (table vs prose help) and
`test_every_subcommand_table_token_appears_in_its_dispatcher` (table vs dispatcher).

Expect real misses on first run. For each one, decide which of these it is and act:
- **prose help is missing/stale for a verb that exists** → fix the help text in
  `src/help_data.rs` (`command_help`) so it documents the verb in usage form. This is
  the cheap, correct outcome.
- **the dispatcher genuinely never handles the verb** → that is a phantom, i.e. the
  exact bug this guard exists to find. Remove it from the completion table and say so
  in the commit message. Do not paper over it.
- **the verb IS handled but routed on a compound literal** (the `/config show`
  pattern) → add a `KNOWN_INDIRECT` entry, and it MUST name the `file:line` that
  actually handles it. A bare allow-list entry is how a deleted feature keeps
  breathing. Each entry is ~5 lines of `help_data.rs`, so this is where your line
  budget goes.

## Pre-decided fallback (so you never deliberate mid-task)

If, after step 2, `src/help_data.rs` would land at **2000 lines or more**, do NOT
try to make both tables fit and do NOT raise the ceiling. Instead:
- keep `/checkpoint` only (revert the `/context` triple, the `"context"`
  `dispatcher_source` arm, and the `pub(crate)` on `CONTEXT_SUBCOMMANDS`),
- verify green,
- and add one comment on #730 stating that the `/context` half is still uncovered and
  why (line budget), so the residue stays named rather than implied.

Half of #730 landed and stated beats all of #730 reverted.

## Also worth knowing (do not do it in this task)

`/checkpoint` sits in the same file as `FORK_SUBCOMMANDS`, whose dispatcher already
matches against its own table — copy count three → two, and the guard becomes
unnecessary rather than automated. That is the better long-term answer and it is a
separate task; note it in the commit message if you want, but do not attempt the
refactor here.

## Docs

Update the `help_data.rs` bullet in CLAUDE.md: the fixture count goes from ~18 to 20
`(cmd, prefix, table)` triples (state the real number you end up with — if the
fallback fires, say 19 and say why). If you add any `KNOWN_INDIRECT` entry, extend the
sentence that currently lists `/config show|edit|set|get` and `/lint fix`.

## Done means

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings` all green,
`wc -l src/help_data.rs` under 2000, and the commit message states the new fixture
count plus every miss the guard surfaced and how each was resolved.
