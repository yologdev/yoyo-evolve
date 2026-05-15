Title: Refresh model pricing and context window data for new models
Files: src/format/cost.rs, src/commands_info.rs
Issue: none

Complement to Task 1 (model registry refresh). The pricing data in `format/cost.rs` and context window data in `commands_info.rs` need to stay in sync with newly added models.

**In `src/format/cost.rs` — `model_pricing` function:**

1. **GPT-5 extended family** — Currently handles gpt-5, gpt-5-mini, gpt-5.5, gpt-5.5-mini. If Task 1 adds gpt-5-1 through gpt-5-5 or codex-mini, ensure `model_pricing` has branches that match. Use the existing gpt-5 pricing as reasonable estimates for sub-variants (they likely share the same pricing tier). Since these are `starts_with("gpt-5")` patterns, the existing branch may already catch them — verify and add explicit branches only if needed.

2. **Gemini 3.x** — If Task 1 adds gemini-3.0-pro/flash or gemini-3.1-pro, add pricing branches. Estimate based on gemini-2.5 pricing ($1.25/$10 for pro input/output, lower for flash). Add `model.contains("gemini-3")` branches.

3. **Grok 4 variants** — If Task 1 adds grok-4-mini or grok-4-20, ensure the existing `model.contains("grok")` catch-all covers them (it should, since it returns `Some((5.0, 0.0, 0.0, 15.0))`). If grok-4-mini should have different pricing, add a specific branch before the catch-all.

4. **New Anthropic models** — If Task 1 adds claude-sonnet-4-7 or similar, the existing `model.contains("sonnet")` branch should catch it. Verify.

**In `src/commands_info.rs` — `model_context_window` function:**

1. **Gemini 3.x** — Add `model.contains("gemini-3")` returning 1M+ context (Google's trajectory suggests equal or larger context windows).

2. **GPT-5 extended family** — The existing `model.contains("gpt-5")` branch returns 1M. Verify this catches all new variants.

3. **Grok 4 variants** — The existing `model.contains("grok")` catch-all returns 131k. Verify this catches new variants.

**Testing:**
- Add tests for new pricing entries in `format/cost.rs` tests module. Each new model branch should have at least one test verifying it returns `Some(...)`.
- Add tests for new context window entries. The existing test pattern uses `assert_eq!(model_context_window("model-name"), Some(N))`.
- Don't break existing tests.

**Important:** This task depends on Task 1 — the implementation agent should check what models were actually added in `providers.rs` and ensure pricing/context data covers them. If Task 1 hasn't run yet, use the planned model list from task_01.md as guidance.
