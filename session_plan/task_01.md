Title: #941 — one matching rule for model ids: stop warning "Unknown model" for ids the preset lookup resolves
Kind: product
Files: src/cli.rs, src/providers.rs (tests in whichever file owns the helper)
Issue: #941

## Why

`--model claude-fable-5-1` resolves to the Fable preset (`src/agent_builder.rs:~759`,
`model.starts_with("claude-fable-5")`) and prices correctly (`src/format/cost.rs:~78`
delegates to the same preset), but `src/cli.rs` warns with an **exact** match
(`known.contains(&model.as_str())`), so the model works and yells about itself on every
turn. This was verified at HEAD in the Day-205 assessment, not taken on the issue's word —
but **re-verify the two lines yourself before editing** (line numbers drift).

Two rules over one id will disagree again at the next rename. So the fix is **not** to add
`claude-fable-5-1` to the list (that is #923's shape, done twice, and it drifts).

## Steps (all of them fit one pass)

1. Read both sites and confirm the disagreement:
   - `grep -n "Unknown model" src/cli.rs` (the warning site, ~:1072/:1099)
   - `grep -n "starts_with(\"claude-fable" src/agent_builder.rs` (the prefix arms — enumerate
     **all** of them, there may be more than one family)
   - `known_models_for_provider` in `src/providers.rs` (~:47)
2. Make the warning consult the **same resolution rule** the preset lookup uses, instead of
   a second rule over the same input. One pure predicate, e.g.
   `pub(crate) fn model_is_known_for_provider(provider: &str, model: &str) -> bool`, that is
   true when the provider's model list contains the id **or** the preset lookup used by the
   model-config path resolves it. Call it from the warning site. Keep
   `known_models_for_provider` as the data source; do not copy the prefix list anywhere.
   Add the definition and its call site **in the same edit** (a dangling fn fails clippy
   under `-D warnings` and the whole task gets reverted).
3. Keep the false positive from being traded for a false negative. The predicate must stay
   **provider-scoped**: an id resolved by an Anthropic preset is "known" only when the
   resolved provider is the one that preset belongs to. A Claude id under
   `provider = "deepseek"` is a *different* defect (#942, this session's task 2) and must
   still warn here. Do not silence it.
4. Tests, at the emission point (the string a caller receives), not on the helper:
   - table: every prefix-family id found in step 1 (at minimum `claude-fable-5` and
     `claude-fable-5-1`) under its own provider → **no warning**.
   - near-miss A: a genuinely unknown id of the same shape (`claude-fable-9`) → warning still
     emitted, asserting on the existing message text, which stays byte-identical.
   - near-miss B: `claude-fable-5-1` under `provider = "deepseek"` → **still warns** (this is
     the row that proves the fix did not widen into #942's territory).
   - anti-vacuous: assert each fixture id really is resolved by the prefix rule, so the test
     cannot pass by agreeing with itself.

## Constraints

- Do **not** change the wording of the warning for genuinely unknown ids — other tests and
  user habits read it.
- Max 2 source files. No new dependency. No `.yoyo.toml` edit.
- Do not touch `scripts/` or any protected file.

## Verify

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`

Report the count of prefix arms found in step 1 and the rows of the table you actually ran —
if a step-1 prefix family turned out not to be preset-backed, say so rather than folding it in.
