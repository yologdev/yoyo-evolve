//! Bedrock Claude prices (#1116, Day 247).
//!
//! A Bedrock Claude id is the same model as the anthropic preset that
//! yoagent's `ModelConfig::bedrock_anthropic_id` names (#1121, Day 251: the
//! same table yoagent's Bedrock limits use, so an id that runs on guessed
//! limits is also unpriced, never priced as a Claude it may not be), and it
//! costs what that preset and costs what that preset costs **on the `global.`
//! endpoint**. Every other Bedrock endpoint (the bare in-region id and the
//! `us.`/`eu.`/`apac.` profiles) is regional, and Anthropic's pricing page
//! ("Regional and multi-region endpoint pricing for Claude 4.5 models and
//! beyond", docs.anthropic.com/en/docs/about-claude/pricing) says regional
//! endpoints "include a 10 percent premium over global endpoints", from
//! Sonnet 4.5 / Haiku 4.5 / Opus 4.5 on. Every anthropic preset is one of
//! those, so the premium applies to every id this module resolves. The
//! selector is the id's own prefix, which yoyo has: modelled, not guessed.
//! Claude ids with no preset (Sonnet 4, which predates the premium) return
//! `None` and keep the hand table's row.

use yoagent::provider::CostConfig;

/// Regional (non-`global.`) Bedrock endpoints: +10% on every rate.
pub(crate) const REGIONAL_PREMIUM: f64 = 1.1;

/// The price Bedrock bills `model` at, or `None` when it is not a Bedrock
/// Claude id with an anthropic preset.
pub(crate) fn bedrock_preset_cost(model: &str) -> Option<CostConfig> {
    let id = yoagent::provider::ModelConfig::bedrock_anthropic_id(model)?;
    let mut cost = crate::agent_builder::anthropic_preset(&id)?.cost?;
    if model.starts_with("global.") {
        return Some(cost);
    }
    let m = REGIONAL_PREMIUM;
    cost.input_per_million *= m;
    cost.output_per_million *= m;
    cost.cache_read_per_million *= m;
    cost.cache_write_per_million *= m;
    for tier in &mut cost.context_tiers {
        tier.input_per_million *= m;
        tier.output_per_million *= m;
        tier.cache_read_per_million *= m;
        tier.cache_write_per_million *= m;
    }
    Some(cost)
}
