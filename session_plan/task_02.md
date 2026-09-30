Title: Gate REPL-only verbs that own a subcommand vocabulary in their multi-token shell form (#936 slice 1)
Kind: product
Files: src/dispatch_near_miss.rs, CHANGELOG.md
Issue: #936

## Why
#936 measured 50 REPL-only verbs that fall past the multi-token guard into a billed model turn when typed at the shell. For example, `yoyo checkpoint save` starts a paid conversation with write-capable tools about the literal string. The issue says each verb needs a per-verb argument-shape judgement, and it names the answerable form: a verb → vocabulary pair (`REPL_ONLY_MULTI_TOKEN_ARG_GATED`, which `think` uses with `THINKING_LEVELS`).

This slice takes only the verbs whose argument shape the source ALREADY owns: a residue verb with a `*_SUBCOMMANDS` const. Examples, all to be verified: `CHECKPOINT_SUBCOMMANDS`, `FORK_SUBCOMMANDS`, `BG_SUBCOMMANDS`, `GOAL_SUBCOMMANDS`, `REVISIT_SUBCOMMANDS`, `WATCH_SUBCOMMANDS`, `SPAWN_SUBCOMMANDS`, `SKILL_SUBCOMMANDS`, `LINT_SUBCOMMANDS`, `PLAN_SUBCOMMANDS`, `CONFIG_SUBCOMMANDS`, `HISTORY_SUBCOMMANDS`, `COPY_SUBCOMMANDS`, `WEB_SUBCOMMANDS`, `STASH_SUBCOMMANDS`, `REFACTOR_SUBCOMMANDS`, `PR_SUBCOMMANDS`, `GIT_SUBCOMMANDS`. Prose passes through, because a word like "the" is not in any vocabulary. The other verbs, the ones with no owned vocabulary, stay out of scope. Say so in the write-up.

## Steps
0. **Commit ordering (mandatory):** after the code edit and BEFORE any `cargo` invocation, run `git add -A && git commit -m "WIP: #936 slice 1"`.

1. Read `REPL_ONLY_MULTI_TOKEN_VERBS`, `REPL_ONLY_MULTI_TOKEN_ARG_GATED` and their tests in `src/dispatch_near_miss.rs`. Derive the residue the way #936 did: verbs that are neither routed (`ROUTED_SUBCOMMANDS`) nor already guarded.
   - For each residue verb that owns a `*_SUBCOMMANDS` const, add a `(verb, &crate::…::X_SUBCOMMANDS)` entry to `REPL_ONLY_MULTI_TOKEN_ARG_GATED`. Read the vocabulary from the owning const and never re-spell it. Widen the const's visibility to `pub(crate)` only where needed.
   - **Exclude** any verb whose `verb + vocab-word` is a plausible English prompt opener. For example, `yoyo plan <word>` or `yoyo lint fix …` should pass if the words read as prose. Write each exclusion as a one-line comment naming the prose it protects.
   - Anything already routed as a real shell subcommand must not change behaviour.

2. Tests, in the existing test module:
   - A table test over every new gated entry. First, an anti-vacuous assertion that each vocabulary is non-empty. Then `verb <vocab[0]>` must be refused, and the refusal must quote the `yoyo -p "..."` escape hatch (bound 4 in #936).
   - Near-miss rows, one per new verb: `verb the …` / `verb how do I …` must reach the prompt path byte-identically. Use `assert_eq!` on the routed result, not `contains`.
   - The existing `multi_token_guard_leaves_real_prompts_untouched` and the `think` tests must stay green and unedited.
   - **Positive control, one atomic command, serially:** empty one new entry's vocabulary (marked `NEUTERED`), confirm the table test fails by name, restore, confirm green.

3. Add a CHANGELOG.md line under Unreleased. In the commit message, report how many of the 50 verbs were gated, how many were excluded as prose-risky (named), and how many remain with no owned vocabulary.

## Verify
`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`. `src/dispatch_near_miss.rs` is ~1014 lines. Check `tests/module_size.rs` before adding tests. If the gate would be crossed, move the new tests into a sibling `dispatch_near_miss_tests.rs`-style file rather than registering a ceiling.

## Honest-null clause
If the residue contains no verb with an owned vocabulary, or every candidate is prose-risky, make no behaviour change. Report the per-verb table (verb → const or "none" → decision) in the commit message and as a comment on #936.
