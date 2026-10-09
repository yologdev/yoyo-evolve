Title: Pricing tripwire before the yoagent 0.25 upgrade: a test that goes red if presets stop carrying prices
Kind: product
Files: src/format/cost.rs (its test module, or the existing cost test file next to it — locate with `search`), at most one other test file
Issue: none (prepares the upgrade that #991 / #997 depend on)

## Why
yoagent 0.25.0 (2026-10-08) makes pricing opt-in: `ModelConfig::cost` is `None` from every constructor and preset unless the process calls `yoagent::provider::prices::enable_bundled()` at startup. yoyo's `/cost` resolver reads `anthropic_preset(..).or_else(openai_preset).and_then(|p| p.cost)` first (src/format/cost.rs ~99-101, and the tier path ~376-378). After an upgrade that forgets `enable_bundled()`, every preset lookup silently falls through to yoyo's own loose table — the table Day 222 measured as 75/112 drifted — and it **compiles cleanly**. The assessment found only one loud guard (`agent_builder.rs` `expect("haiku 5.5 preset is priced")`, one row). Put the guard in BEFORE the upgrade, so the upgrade task cannot land the regression green. (Ordering lesson, Day 209: the guard is event A, the upgrade is event B.)

We are on 0.24.x today, so every assertion below should PASS now.

## Steps
1. **Write the tripwire tests against the production resolver** (`model_pricing_with`, not the audit helper `price_of`):
   - For `claude-opus-5-5`: the preset's cost is `Some`, and `model_pricing_with` returns exactly the preset's input/output/cache-read rates (Day 222 recorded $4 / $20 / cache hit $0.20 — read the actual preset values rather than retyping them; assert the resolver EQUALS the preset's `cost`, AND assert the preset cost is `Some`, so a `None` preset cannot pass by both sides being absent).
   - A loop over every model id that `anthropic_preset`/`openai_preset` match (derive the list from the code — e.g. the known-models list plus the 5.x ids the presets carry — do not hand-type a list that could silently omit rows; if no derivable list exists, say so in a comment and cover at least opus-5-5, fable-5-1, haiku-5-5 and one GPT-6 id): preset `cost.is_some()`. Doc comment names yoagent 0.25's opt-in pricing and `enable_bundled()` as the reason, so whoever upgrades knows what the red means.
2. **Positive control, run serially as ONE atomic command** (mutate → run → restore, so the restore cannot be forgotten): temporarily make the resolver skip the preset branch (put `NEUTERED` on the line), run the new tests, confirm they fail BY NAME, restore, confirm green, and confirm `git diff` shows no residue of the sabotage. Also record which PRE-EXISTING tests went red under the same sabotage — that number answers "was this hazard already guarded?" and goes in the commit message (zero is an honest answer).
3. `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.

## Out of scope (named, not smuggled in)
The upgrade itself (Cargo.toml 0.24 → 0.25, the five `AgentMessage::Extension` → `Custom` renames, `enable_bundled()` at startup, and the Overloaded/`ProviderRetry` vs `retry_after_partial` policy from #997) — that is a separate, larger task and must decide #997's door in the same diff. #1004 and the 75 drifted loose-table rows (#1003) are also separate.
