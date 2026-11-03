//! #1003: every id `anthropic_preset` recognises must carry the price of
//! yoagent's constructor for **that exact model**, not an older sibling's.
//! Before the fix, `claude-opus-5-5` matched the `claude-opus-5` arm first and
//! billed at Opus 5's $5/$25 (vendor: $4/$20, cache hit $0.20), and
//! `claude-fable-5-1` billed Fable 5's $1.00 cache hit (vendor: $0.25).
//! Lives beside `agent_builder.rs` because that module is at its size gate.

use crate::agent_builder::anthropic_preset;
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
        // yoagent 0.25.8 constructors (#1037); yoyo's hand-built copies
        // (Day 222 / Day 224) matched them field for field and were deleted.
        // The vendor-typed tests below still pin both prices independently.
        ("claude-haiku-5-5", ModelConfig::claude_haiku_5_5()),
        ("claude-haiku-5-5-20261001", ModelConfig::claude_haiku_5_5()),
        ("claude-sonnet-5-5", ModelConfig::claude_sonnet_5_5()),
        (
            "claude-sonnet-5-5-20261001",
            ModelConfig::claude_sonnet_5_5(),
        ),
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

/// #1116: Sonnet 4.6, typed from Anthropic's pages (Day 247), not from
/// production code: 1M window and 128K max output
/// (docs.anthropic.com/en/docs/build-with-claude/context-windows); $3 / $15,
/// cache write $3.75, cache read $0.30, and the full 1M at standard pricing
/// (docs.anthropic.com/en/docs/about-claude/pricing).
#[test]
fn sonnet_4_6_matches_anthropic_published_figures() {
    for id in ["claude-sonnet-4-6", "claude-sonnet-4-6-20260217"] {
        let p = anthropic_preset(id).unwrap_or_else(|| panic!("{id} has a preset"));
        assert_eq!(p.id, id, "the requested id is sent verbatim");
        assert_eq!(p.context_window, 1_000_000, "{id}");
        // The request default: 64K of the 128K maximum, the same split every
        // yoagent 1M Claude preset makes.
        assert_eq!(
            p.max_tokens,
            ModelConfig::claude_sonnet_5_5().max_tokens,
            "{id}"
        );
        assert_eq!(p.max_tokens, 64_000, "{id}");
        let c = p.cost.expect("sonnet 4.6 is priced");
        assert_eq!(
            (
                c.input_per_million,
                c.output_per_million,
                c.cache_read_per_million,
                c.cache_write_per_million,
            ),
            (3.0, 15.0, 0.30, 3.75),
            "{id}"
        );
        assert!(c.context_tiers.is_empty(), "no long-context tier: {id}");
    }
    // Near misses: 4.5 (200K, a different model) and 4 stay unmapped.
    assert!(anthropic_preset("claude-sonnet-4-5-20250929").is_none());
    assert!(anthropic_preset("claude-sonnet-4-20250514").is_none());
}

#[test]
fn bedrock_anthropic_preset_id_table() {
    use crate::providers::bedrock_anthropic_preset_id as map;
    let rows: &[(&str, Option<&str>)] = &[
        // yoyo's no-config bedrock default.
        ("anthropic.claude-sonnet-4-6", Some("claude-sonnet-4-6")),
        // Cross-region inference profiles.
        ("us.anthropic.claude-sonnet-4-6", Some("claude-sonnet-4-6")),
        ("eu.anthropic.claude-sonnet-4-6", Some("claude-sonnet-4-6")),
        ("apac.anthropic.claude-opus-5-5", Some("claude-opus-5-5")),
        (
            "global.anthropic.claude-sonnet-4-6",
            Some("claude-sonnet-4-6"),
        ),
        // Bedrock's version suffix is dropped, the date kept.
        (
            "anthropic.claude-sonnet-4-20250514-v1:0",
            Some("claude-sonnet-4-20250514"),
        ),
        (
            "us.anthropic.claude-haiku-4-5-20251001-v1:0",
            Some("claude-haiku-4-5-20251001"),
        ),
        ("anthropic.claude-sonnet-4-6-v2", Some("claude-sonnet-4-6")),
        // Other vendors on bedrock: not Anthropic, not mapped.
        ("amazon.nova-pro-v1:0", None),
        ("meta.llama3-70b-instruct-v1:0", None),
        ("us.meta.llama3-2-90b-instruct-v1:0", None),
        // An ARN, an empty id, a bare Anthropic id, and two dots before it.
        (
            "arn:aws:bedrock:us-east-1:1:inference-profile/us.anthropic.claude-sonnet-4-6",
            None,
        ),
        ("anthropic.", None),
        ("claude-sonnet-4-6", None),
        ("a.b.anthropic.claude-sonnet-4-6", None),
        ("", None),
    ];
    for (model, want) in rows {
        assert_eq!(map(model), *want, "{model}");
    }
}

/// Emission point: what the bedrock arm builds, and what the guessed-limits
/// warning reads (`runs_on_guessed_limits`, the same predicate the warning
/// uses). The wire stays Bedrock's.
#[test]
fn bedrock_claude_ids_run_on_the_anthropic_preset() {
    use crate::agent_builder::create_model_config;
    use crate::model_limits_note::runs_on_guessed_limits;
    let default = crate::providers::default_model_for_provider("bedrock");
    assert_eq!(
        default, "anthropic.claude-sonnet-4-6",
        "the default is not moved"
    );
    let preset = anthropic_preset("claude-sonnet-4-6").unwrap();
    for id in [
        default.as_str(),
        "us.anthropic.claude-sonnet-4-6",
        "global.anthropic.claude-sonnet-4-6",
        "us.anthropic.claude-opus-5-5-v1:0",
    ] {
        let c = create_model_config("bedrock", id, None);
        let want =
            anthropic_preset(crate::providers::bedrock_anthropic_preset_id(id).unwrap()).unwrap();
        assert_eq!(
            (c.context_window, c.max_tokens),
            (want.context_window, want.max_tokens),
            "{id}"
        );
        assert_eq!(c.id, id, "the id sent to Bedrock is the user's");
        assert_eq!(c.provider, "bedrock");
        assert_eq!(format!("{}", c.api), "bedrock_converse_stream");
        assert_eq!(
            c.base_url,
            "https://bedrock-runtime.us-east-1.amazonaws.com"
        );
        assert!(!runs_on_guessed_limits("bedrock", id), "{id}");
    }
    // Anti-vacuous: the default's limits really differ from the guess.
    let guess = create_model_config("bedrock", "amazon.nova-pro-v1:0", None);
    assert_ne!(
        (preset.context_window, preset.max_tokens),
        (guess.context_window, guess.max_tokens)
    );
    // Near misses keep the guess (200K, yoagent's default) and still warn:
    // another vendor, and a Claude id with no preset (Sonnet 4 is 200K anyway).
    let generic = yoagent::provider::ModelConfig::custom(
        yoagent::provider::ApiProtocol::BedrockConverseStream,
        "bedrock",
        "u",
        "m",
        "m",
    );
    for id in [
        "amazon.nova-pro-v1:0",
        "anthropic.claude-sonnet-4-20250514-v1:0",
    ] {
        let c = create_model_config("bedrock", id, None);
        assert_eq!(
            (c.context_window, c.max_tokens, c.cost.is_none()),
            (200_000, generic.max_tokens, generic.cost.is_none()),
            "{id}"
        );
        assert!(runs_on_guessed_limits("bedrock", id), "{id}");
    }
}

/// /cost: Bedrock bills Claude 4.5+ on regional endpoints (in-region and the
/// `us.`/`eu.`/`apac.` profiles) 10% above `global.` (Anthropic's pricing
/// page, "Regional and multi-region endpoint pricing"). The selector is the
/// id's own prefix, so it is modelled, not guessed.
#[test]
fn cost_prices_bedrock_claude_ids_by_endpoint() {
    let none = crate::config::ModelPricingOverrides::default();
    let price = |id| crate::format::model_pricing_with(&none, id).expect(id);
    let close = |a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)| {
        [a.0 - b.0, a.1 - b.1, a.2 - b.2, a.3 - b.3]
            .iter()
            .all(|d| d.abs() < 1e-9)
    };
    let base = (3.0, 3.75, 0.30, 15.0);
    let regional = (3.3, 4.125, 0.33, 16.5);
    assert!(close(price("global.anthropic.claude-sonnet-4-6"), base));
    for id in [
        "anthropic.claude-sonnet-4-6",
        "us.anthropic.claude-sonnet-4-6",
        "eu.anthropic.claude-sonnet-4-6",
    ] {
        assert!(close(price(id), regional), "{id}: {:?}", price(id));
    }
    // First-party id: no premium.
    assert!(close(price("claude-sonnet-4-6"), base));
    // Near miss: Sonnet 4 predates the regional premium and has no preset, so
    // it keeps the hand table's row, byte-identical to before.
    assert_eq!(
        price("anthropic.claude-sonnet-4-20250514-v1:0"),
        (3.0, 3.75, 0.30, 15.0)
    );
}
