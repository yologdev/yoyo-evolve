//! #1003: every id `anthropic_preset` recognises must carry the price of
//! yoagent's constructor for **that exact model**, not an older sibling's.
//! Before the fix, `claude-opus-5-5` matched the `claude-opus-5` arm first and
//! billed at Opus 5's $5/$25 (vendor: $4/$20, cache hit $0.20), and
//! `claude-fable-5-1` billed Fable 5's $1.00 cache hit (vendor: $0.25).
//! Lives beside `agent_builder.rs` because that module is at its size gate.

use crate::agent_builder::{anthropic_preset, claude_haiku_5_5, claude_sonnet_5_5};
use yoagent::provider::{CostConfig, ModelConfig};

/// (id, the constructor that owns that exact model). The expected value is
/// DERIVED from the constructor, never typed, except the one vendor-anchored
/// row in the test below. Dated/suffixed ids are near-misses for the
/// most-specific-first ordering: each must still land on its own arm.
fn preset_table() -> Vec<(&'static str, ModelConfig)> {
    vec![
        ("claude-fable-5", ModelConfig::claude_fable_5()),
        ("claude-fable-5-20260801", ModelConfig::claude_fable_5()),
        ("claude-fable-5-1", ModelConfig::claude_fable_5_1()),
        ("claude-fable-5-1-20261001", ModelConfig::claude_fable_5_1()),
        ("claude-opus-5", ModelConfig::claude_opus_5()),
        ("claude-opus-5-20260724", ModelConfig::claude_opus_5()),
        ("claude-opus-5-5", ModelConfig::claude_opus_5_5()),
        ("claude-opus-5-5-20261001", ModelConfig::claude_opus_5_5()),
        ("claude-opus-4-8", ModelConfig::claude_opus_4_8()),
        ("claude-sonnet-5", ModelConfig::claude_sonnet_5()),
        ("claude-sonnet-5-20260101", ModelConfig::claude_sonnet_5()),
        ("claude-haiku-4-5", ModelConfig::claude_haiku_4_5()),
        ("claude-haiku-4-5-20251001", ModelConfig::claude_haiku_4_5()),
        // yoyo-side arm (Day 222): keeps its own hand-assigned cost.
        ("claude-haiku-5-5", claude_haiku_5_5()),
        ("claude-haiku-5-5-20261001", claude_haiku_5_5()),
        // yoyo-side arm (Day 224): cache reads $0.10, not Sonnet 5's $0.20.
        ("claude-sonnet-5-5", claude_sonnet_5_5()),
        ("claude-sonnet-5-5-20261001", claude_sonnet_5_5()),
    ]
}

#[test]
fn every_preset_id_is_priced_by_its_own_constructor() {
    let mut wrong = Vec::new();
    for (id, owner) in preset_table() {
        let preset = anthropic_preset(id).unwrap_or_else(|| panic!("{id} must have a preset"));
        // Anti-vacuous: a `None == None` pass would prove nothing.
        assert!(owner.cost.is_some(), "{id}: owning constructor is unpriced");
        if preset.cost != owner.cost {
            wrong.push(format!(
                "{id}: preset {:?} != {} {:?}",
                preset.cost, owner.id, owner.cost
            ));
        }
        // The requested id is kept verbatim (the existing rename behaviour).
        assert_eq!(preset.id, id);
        assert_eq!(preset.context_window, owner.context_window, "{id}");
        assert_eq!(preset.max_tokens, owner.max_tokens, "{id}");
    }
    assert!(wrong.is_empty(), "mispriced presets:\n{}", wrong.join("\n"));
}

#[test]
fn opus_5_5_matches_the_vendor_price_page() {
    // Typed from https://platform.claude.com/docs/en/about-claude/pricing
    // (Opus 5.5: $4 input, $20 output, $0.20 cache hit, $5 5-minute cache
    // write per MTok), not from production code — so a wrong constructor
    // upstream cannot make this row agree with itself.
    let cost = anthropic_preset("claude-opus-5-5")
        .and_then(|c| c.cost)
        .expect("opus 5.5 is priced");
    let vendor = CostConfig::new(4.0, 20.0)
        .with_cache_read(0.20)
        .with_cache_write(5.0);
    assert_eq!(
        (
            cost.input_per_million,
            cost.output_per_million,
            cost.cache_read_per_million,
            cost.cache_write_per_million,
        ),
        (
            vendor.input_per_million,
            vendor.output_per_million,
            vendor.cache_read_per_million,
            vendor.cache_write_per_million,
        )
    );
    // Near miss: Opus 5 keeps its own, older price.
    let opus5 = anthropic_preset("claude-opus-5")
        .and_then(|c| c.cost)
        .unwrap();
    assert_eq!(
        (opus5.input_per_million, opus5.output_per_million),
        (5.0, 25.0)
    );
}

#[test]
fn sonnet_5_5_matches_the_catalogue_and_keeps_sonnet_5_limits() {
    // Typed from models.dev's `anthropic.claude-sonnet-5-5` row, fetched Day
    // 224: input 2, output 10, cache_read 0.1, cache_write 2.5. Cache read is
    // corroborated by Claude Code 2.1.293-296 ("price Sonnet 5.5 cache reads
    // at $0.10 per million tokens (was $0.20)"). Not from production code.
    let preset = anthropic_preset("claude-sonnet-5-5").expect("sonnet 5.5 has a preset");
    let cost = preset.cost.clone().expect("sonnet 5.5 is priced");
    assert_eq!(
        (
            cost.input_per_million,
            cost.output_per_million,
            cost.cache_read_per_million,
            cost.cache_write_per_million,
        ),
        (2.0, 10.0, 0.10, 2.5)
    );
    assert!(cost.context_tiers.is_empty(), "no long-context tier");
    // The arm changes the price only: with id, name and cost set back to
    // Sonnet 5's, every other serialized field (window, max_tokens, compat,
    // api, base_url, ...) must be byte-identical.
    let sonnet5 = ModelConfig::claude_sonnet_5();
    let mut rest = preset.clone();
    rest.id = sonnet5.id.clone();
    rest.name = sonnet5.name.clone();
    rest.cost = sonnet5.cost.clone();
    assert_eq!(
        serde_json::to_value(&rest).unwrap(),
        serde_json::to_value(&sonnet5).unwrap()
    );
    assert_eq!(preset.context_window, 1_000_000);
    // Near miss: Sonnet 5 (bare and dated) keeps its own $0.20 cache read.
    for id in ["claude-sonnet-5", "claude-sonnet-5-20260101"] {
        let c = anthropic_preset(id).and_then(|c| c.cost).unwrap();
        assert_eq!(c, sonnet5.cost.clone().unwrap(), "{id}");
        assert_eq!(c.cache_read_per_million, 0.20, "{id}");
    }
    // Emission point: the /cost resolver (input, cache_write, cache_read,
    // output), bare and behind the OpenRouter prefix; Sonnet 5 unchanged.
    let none = crate::config::ModelPricingOverrides::default();
    for id in ["claude-sonnet-5-5", "anthropic/claude-sonnet-5-5"] {
        assert_eq!(
            crate::format::model_pricing_with(&none, id),
            Some((2.0, 2.5, 0.10, 10.0)),
            "{id}"
        );
    }
    assert_eq!(
        crate::format::model_pricing_with(&none, "claude-sonnet-5"),
        Some((2.0, 2.5, 0.20, 10.0))
    );
}
