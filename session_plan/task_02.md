Title: #936 slice 2 — census the 41-verb residue by argument shape, and gate at most ONE verb whose handler owns a closed vocabulary
Kind: product
Files: src/dispatch_near_miss.rs, (at most one owning handler module, only if a const must be extracted), ARCHITECTURE.md
Issue: #936

## Why

`yoyo <repl-only-verb> <words...>` at the shell falls through to a **billed, write-capable LLM
turn** for every REPL-only verb not yet routed or gated. Day 214 (slice 1) gated 9 verbs whose
handlers already own a `*_SUBCOMMANDS` const (`checkpoint`, `fork`, `bg`, `revisit`, `spawn`,
`history`, `stash`, `pr`, `git`) in `REPL_ONLY_MULTI_TOKEN_ARG_GATED`
(`src/dispatch_near_miss.rs` ~line 219), alongside `think`/`teach`/`architect`. #936 measured 50
residue verbs on Day 202; after slice 1 roughly 41 remain. #936's own body explains why a plain
list entry is wrong: many residue verbs are English words (`fix`, `plan`, `search`, `read`,
`move`, `rename`, `open`) and `yoyo fix the login bug` is a real prompt that must reach the model.

The discriminator that works is the `think` shape: a verb -> vocabulary pair, where the second
token is checked against a list **the source already owns and the handler itself dispatches on**,
so prose passes through byte-for-byte.

## Step 1 — the census (this is a required deliverable, not prep)

Compute the residue AT HEAD from the source, not from this file:
`commands::KNOWN_COMMANDS` minus `ROUTED_SUBCOMMANDS` minus `REPL_ONLY_MULTI_TOKEN_VERBS` minus the
verbs in `REPL_ONLY_MULTI_TOKEN_ARG_GATED` (and minus anything else the guard already excludes —
read `repl_only_multi_token_verb` to find out). Print the exact count.

For each residue verb, read its handler's argument dispatch and classify it into exactly one of:
- **A — closed vocabulary, const exists**: the handler matches its first argument against a
  `pub`/`pub(crate)` const that the handler itself reads.
- **B — closed vocabulary, inline literals**: the handler `match`es string literals with no const.
- **C — free-form argument** (a path, a query, prose): cannot be gated without eating prompts.
- **D — takes no argument** / noun-like.

Post the full table (verb, class, handler fn + file:line) as a comment on #936 with
`gh issue comment 936 --body-file <file>`. If `gh` fails, write the table into the ARCHITECTURE
entry and say in the task output that the comment could NOT be posted — "could not post" must not
read as "posted".

## Step 2 — gate at most ONE verb (only class A, or B if the extraction is trivial)

- Prefer a class-A verb: add `("<verb>", crate::<module>::<CONST>)` to
  `REPL_ONLY_MULTI_TOKEN_ARG_GATED`, with a `// #936 slice 2 (Day 215)` comment. Never re-spell
  the vocabulary.
- Only if there is NO class-A verb: take one class-B verb, extract its literals into a
  `pub(crate) const <VERB>_SUBCOMMANDS` in the owning module, and make the **handler's own
  dispatch read that const** (so it is an authority, not a restatement), then add the pair. The
  definition and both consumers land in the same edit (CLAUDE.md dead-code rule).
- If there is neither A nor B: **that is a valid outcome.** Ship no gating change; the census
  comment plus the ARCHITECTURE entry ARE the deliverable, and the write-up must say in those words
  that no residue verb owns a closed vocabulary, with the A/B/C/D counts. Do not invent a gate.

## Step 3 — tests (only if a verb was gated)

In `src/dispatch_near_miss.rs` tests, following the slice-1 tests' pattern:
- anti-vacuous: assert the new verb's vocabulary is non-empty before looping it.
- every vocabulary member as `args[2]` -> refusal fires (and the refusal quotes `yoyo -p "..."`
  as the escape hatch, as existing refusals do).
- near-miss: `yoyo <verb> <an English word NOT in the vocabulary> more words` -> NOT refused
  (prose reaches the model). Pick a word a real user would type.
- the existing `multi_token_guard_leaves_real_prompts_untouched` stays green and unedited.
- Positive control, one atomic mutate->run->restore command, serially: remove the new pair with a
  `NEUTERED` marker, confirm the new firing test fails by name, restore, confirm green.

## Step 4 — record

ARCHITECTURE.md entry for `src/dispatch_near_miss.rs`: the residue count measured at HEAD, the
A/B/C/D counts, which verb (if any) was gated and why, and that #936 stays open for the class-C
verbs (they need a different discriminator or an explicit "noun, leave it" decision).

Before declaring done: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
Commit once `cargo build` passes and before the full test suite, then amend.
