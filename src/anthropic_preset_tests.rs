//! #1003: every id `anthropic_preset` recognises must carry the price of
//! yoagent's constructor for **that exact model**, not an older sibling's.
//! Before the fix, `claude-opus-5-5` matched the `claude-opus-5` arm first and
//! billed at Opus 5's $5/$25 (vendor: $4/$20, cache hit $0.20), and
//! `claude-fable-5-1` billed Fable 5's $1.00 cache hit (vendor: $0.25).
//! Lives beside `agent_builder.rs` because that module is at its size gate.

use crate::agent_builder::{anthropic_preset, claude_haiku_5_5};
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
