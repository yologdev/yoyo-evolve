//! Per-request context-tier pricing: Claude Haiku 5.5's preset carries a
//! `ContextTier` (higher rates above a prompt-size threshold), and before this
//! nothing in yoyo read it, so every turn was priced at the base rate.
//!
//! Every expected value is built from the preset's own fields, never from a
//! typed price, so a repriced preset moves the expectation with it.

use super::*;
use yoagent::{AgentMessage, Content, Message, StopReason, Usage};

const HAIKU: &str = "claude-haiku-5-5";

fn no_overrides() -> ModelPricingOverrides {
    ModelPricingOverrides::default()
}

fn haiku_cost() -> yoagent::provider::CostConfig {
    crate::agent_builder::anthropic_preset(HAIKU)
        .and_then(|p| p.cost)
        .expect("haiku 5.5 preset is priced")
}

/// The one tier, with an anti-vacuous check that it really is a different
/// price — a tier equal to the base rate would make every test below vacuous.
fn haiku_tier() -> yoagent::provider::ContextTier {
    let cost = haiku_cost();
    assert_eq!(cost.context_tiers.len(), 1, "fixture assumes one tier");
    let tier = cost.context_tiers[0].clone();
    assert!(tier.input_per_million > cost.input_per_million);
    assert!(tier.output_per_million > cost.output_per_million);
    tier
}

/// A usage whose prompt (input + cache_read + cache_write) is exactly `prompt`.
fn usage_with_prompt(prompt: u64) -> Usage {
    let cache_read = prompt / 4;
    let cache_write = prompt / 8;
    Usage {
        input: prompt - cache_read - cache_write,
        output: 2_000,
        cache_read,
        cache_write,
        total_tokens: 0,
    }
}

/// The tier price computed by hand from the tier's own fields, in the same
/// order yoagent sums them.
fn expected_tier_cost(t: &yoagent::provider::ContextTier, u: &Usage) -> f64 {
    (u.input as f64 * t.input_per_million
        + u.output as f64 * t.output_per_million
        + u.cache_read as f64 * t.cache_read_per_million
        + u.cache_write as f64 * t.cache_write_per_million)
        / 1_000_000.0
}

fn assistant(usage: Usage) -> AgentMessage {
    AgentMessage::Llm(
        Message::assistant(
            vec![Content::Text { text: "ok".into() }],
            StopReason::Stop,
            HAIKU,
            "anthropic",
            usage,
        )
        .with_timestamp(0),
    )
}

#[test]
fn haiku_turn_below_threshold_is_byte_identical_to_the_flat_price() {
    let t = haiku_tier();
    let u = usage_with_prompt(t.above_prompt_tokens / 2);
    let flat = estimate_cost_with(&no_overrides(), &u, HAIKU).unwrap();
    assert_eq!(
        estimate_request_cost_with(&no_overrides(), &u, HAIKU),
        Some((flat, false))
    );
}

#[test]
fn haiku_turn_above_threshold_uses_the_tier_rates() {
    let t = haiku_tier();
    // These rates have non-zero cache prices, so the hand formula needs no
    // zero-rate fallback.
    assert!(t.cache_read_per_million > 0.0 && t.cache_write_per_million > 0.0);
    let u = usage_with_prompt(t.above_prompt_tokens + 40_000);
    let (cost, tiered) = estimate_request_cost_with(&no_overrides(), &u, HAIKU).unwrap();
    assert!(tiered);
    assert_eq!(cost, expected_tier_cost(&t, &u));
    // And it is really more than the flat price the session line would show.
    assert!(cost > estimate_cost_with(&no_overrides(), &u, HAIKU).unwrap());
    // Router-prefixed and dated ids resolve the same preset.
    for id in ["anthropic/claude-haiku-5-5", "claude-haiku-5-5-20261001"] {
        assert_eq!(
            estimate_request_cost_with(&no_overrides(), &u, id),
            Some((cost, true)),
            "{id}"
        );
    }
}

#[test]
fn haiku_turn_at_exactly_the_threshold_stays_at_the_base_rate() {
    // "above_prompt_tokens": the tier applies to prompts OVER the threshold.
    let t = haiku_tier();
    let at = usage_with_prompt(t.above_prompt_tokens);
    assert_eq!(
        at.input + at.cache_read + at.cache_write,
        t.above_prompt_tokens
    );
    let flat = estimate_cost_with(&no_overrides(), &at, HAIKU).unwrap();
    assert_eq!(
        estimate_request_cost_with(&no_overrides(), &at, HAIKU),
        Some((flat, false))
    );
    // Near miss one token over: tiered.
    let over = usage_with_prompt(t.above_prompt_tokens + 1);
    assert_eq!(
        over.input + over.cache_read + over.cache_write,
        t.above_prompt_tokens + 1
    );
    assert!(
        estimate_request_cost_with(&no_overrides(), &over, HAIKU)
            .unwrap()
            .1
    );
}

#[test]
fn non_tiered_model_with_a_huge_prompt_is_unchanged() {
    let opus = crate::agent_builder::anthropic_preset("claude-opus-5")
        .and_then(|p| p.cost)
        .expect("opus 5 preset is priced");
    assert!(opus.context_tiers.is_empty(), "fixture assumes no tier");
    let u = usage_with_prompt(900_000);
    let flat = estimate_cost_with(&no_overrides(), &u, "claude-opus-5").unwrap();
    assert_eq!(
        estimate_request_cost_with(&no_overrides(), &u, "claude-opus-5"),
        Some((flat, false))
    );
    // Unknown model stays unknown.
    assert_eq!(
        estimate_request_cost_with(&no_overrides(), &u, "no-such-model-zz"),
        None
    );
}

#[test]
fn user_override_replaces_pricing_wholesale_and_no_tier_applies() {
    let overrides = crate::config::parse_model_pricing_from_config(
        "[model_pricing.\"claude-haiku-5-5\"]\ninput = 1.0\noutput = 2.0\n",
    );
    let t = haiku_tier();
    let u = usage_with_prompt(t.above_prompt_tokens * 3);
    let flat = estimate_cost_with(&overrides, &u, HAIKU).unwrap();
    assert_eq!(
        estimate_request_cost_with(&overrides, &u, HAIKU),
        Some((flat, false))
    );
}

#[test]
fn two_turns_one_under_one_over_are_priced_separately() {
    let t = haiku_tier();
    let under = usage_with_prompt(t.above_prompt_tokens - 10_000);
    let over = usage_with_prompt(t.above_prompt_tokens + 10_000);
    let messages = vec![assistant(under.clone()), assistant(over.clone())];
    let turns = extract_turn_costs_with(&no_overrides(), &messages, HAIKU);
    assert_eq!(turns.len(), 2);
    assert_eq!(
        (turns[0].cost_usd, turns[0].tiered),
        (estimate_cost_with(&no_overrides(), &under, HAIKU), false)
    );
    assert_eq!(
        (turns[1].cost_usd, turns[1].tiered),
        (Some(expected_tier_cost(&t, &over)), true)
    );
    // Merged, the two prompts would clear the threshold together; priced per
    // request, the first turn must not be dragged into the tier.
    assert!(under.input + under.cache_read + under.cache_write <= t.above_prompt_tokens);
}

#[test]
fn per_turn_table_marks_tiered_rows_and_says_why_the_totals_differ() {
    let t = haiku_tier();
    let messages = vec![
        assistant(usage_with_prompt(t.above_prompt_tokens / 2)),
        assistant(usage_with_prompt(t.above_prompt_tokens * 2)),
    ];
    let out = format_turn_costs(&extract_turn_costs_with(&no_overrides(), &messages, HAIKU));
    let lines: Vec<&str> = out.lines().collect();
    assert!(!lines[2].ends_with(" *"), "untiered row: {}", lines[2]);
    assert!(lines[3].ends_with(" *"), "tiered row: {}", lines[3]);
    assert_eq!(lines.last().copied(), Some(TIERED_TURN_FOOTNOTE));

    // Near miss: no tiered turn -> no marker, no footnote.
    let quiet = vec![assistant(usage_with_prompt(t.above_prompt_tokens / 2))];
    let out = format_turn_costs(&extract_turn_costs_with(&no_overrides(), &quiet, HAIKU));
    assert!(!out.contains('*'));
    assert!(!out.contains(TIERED_TURN_FOOTNOTE));
}
