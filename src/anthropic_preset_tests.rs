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
/// production code. Since #1123 the preset is yoagent 0.26.7's own
/// `claude_sonnet_4_6()`, and these typed figures stay as the outside check
/// on it: 1M window and 128K max output
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
        // #1123: yoyo sends yoagent's constructor, not a hand-written copy.
        let y = ModelConfig::claude_sonnet_4_6();
        assert_eq!(
            (p.context_window, p.max_tokens, p.reasoning),
            (y.context_window, y.max_tokens, y.reasoning),
            "{id}"
        );
        assert_eq!(Some(c), y.cost, "{id}");
    }
    // Near misses: 4.5 (200K, a different model) and 4 stay unmapped.
    assert!(anthropic_preset("claude-sonnet-4-5-20250929").is_none());
    assert!(anthropic_preset("claude-sonnet-4-20250514").is_none());
}

/// #1121/#1122: /cost resolves a Bedrock id to its Anthropic model through
/// yoagent's `ModelConfig::bedrock_anthropic_id`, the same table yoagent's
/// limits use, so the price and the "limits are guesses" warning can no longer
/// disagree about one id. Stray shapes (an unknown geo prefix, a `-vN` suffix
/// on a model whose Bedrock id has none, a dateless Haiku 4.5) are not
/// bedrock-runtime ids on AWS's model cards: an id we warn we don't know gets
/// no price, rather than a confident wrong one.
#[test]
fn bedrock_ids_resolve_through_yoagents_bedrock_anthropic_id() {
    let map = ModelConfig::bedrock_anthropic_id;
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
        // Bedrock's own spelling of Haiku 4.5.
        (
            "anthropic.claude-haiku-4-5-20251001-v1:0",
            Some("claude-haiku-4-5"),
        ),
        (
            "us.anthropic.claude-haiku-4-5-20251001-v1:0",
            Some("claude-haiku-4-5"),
        ),
        // Strays the old yoyo-side mapping accepted (#1121's list): unpriced now.
        ("xx.anthropic.claude-sonnet-4-6", None),
        ("anthropic.claude-sonnet-4-6-v1:0", None),
        ("anthropic.claude-sonnet-4-6-v2", None),
        ("anthropic.claude-haiku-4-5", None),
        ("us.anthropic.claude-opus-5-5-v1:0", None),
        // A real Bedrock id with no Anthropic preset: the hand table prices it.
        ("anthropic.claude-sonnet-4-20250514-v1:0", None),
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
        assert_eq!(map(model).as_deref(), *want, "{model}");
        // The price follows the mapping exactly: priced iff mapped.
        assert_eq!(
            crate::format::bedrock_preset_cost(model).is_some(),
            want.is_some(),
            "{model}"
        );
    }
    // Real ids price at the Anthropic preset's rate x the regional premium
    // (the module's own constant, never a typed number per id).
    let m = crate::format::BEDROCK_REGIONAL_PREMIUM;
    for (id, anthropic) in [
        ("anthropic.claude-sonnet-4-6", "claude-sonnet-4-6"),
        ("us.anthropic.claude-sonnet-4-6", "claude-sonnet-4-6"),
        (
            "anthropic.claude-haiku-4-5-20251001-v1:0",
            "claude-haiku-4-5",
        ),
    ] {
        let base = anthropic_preset(anthropic).unwrap().cost.unwrap();
        let got = crate::format::bedrock_preset_cost(id).expect(id);
        for (g, b) in [
            (got.input_per_million, base.input_per_million),
            (got.output_per_million, base.output_per_million),
            (got.cache_read_per_million, base.cache_read_per_million),
            (got.cache_write_per_million, base.cache_write_per_million),
        ] {
            assert!((g - b * m).abs() < 1e-9, "{id}: {g} vs {b} x {m}");
        }
    }
}

/// Emission point: what the bedrock arm builds, and what the guessed-limits
/// warning reads (`runs_on_guessed_limits`, the same predicate the warning
/// uses). The wire stays Bedrock's. #1124: the limits authority is yoagent's
/// `ModelConfig::preset("bedrock", id)` (named geo prefixes, a fixed id
/// table), no longer yoyo's own id mapping.
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
        "jp.anthropic.claude-sonnet-4-6",
        "us.anthropic.claude-opus-5-5",
        "us.anthropic.claude-haiku-4-5-20251001-v1:0",
    ] {
        let c = create_model_config("bedrock", id, None);
        let want = ModelConfig::preset("bedrock", id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(
            (c.context_window, c.max_tokens, c.reasoning),
            (want.context_window, want.max_tokens, want.reasoning),
            "{id}"
        );
        // Priced by /cost's own Bedrock rule, never yoagent's (unpriced) preset.
        assert!(want.cost.is_none(), "{id}");
        assert_eq!(c.cost, crate::format::bedrock_preset_cost(id), "{id}");
        assert!(c.cost.is_some(), "{id}");
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
    // #1124: shapes the old mapping accepted and yoagent refuses (none is a
    // bedrock-runtime id on AWS's model cards): an unknown geo prefix, and a
    // `-vN` suffix on a model whose Bedrock id has none.
    for id in [
        "amazon.nova-pro-v1:0",
        "anthropic.claude-sonnet-4-20250514-v1:0",
        "xx.anthropic.claude-sonnet-4-6",
        "us.anthropic.claude-opus-5-5-v1:0",
        "anthropic.claude-sonnet-4-6-v2",
    ] {
        let c = create_model_config("bedrock", id, None);
        assert!(ModelConfig::preset("bedrock", id).is_none(), "{id}");
        assert_eq!(
            (c.context_window, c.max_tokens),
            (200_000, generic.max_tokens),
            "{id}"
        );
        // #1121: an id we warn we don't know gets no price, rather than a
        // confident wrong one (the Day-248 mapping priced the strays as Claude).
        assert_eq!(c.cost, None, "{id}");
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

/// #1121 item 1: `--provider bedrock --model us.anthropic.claude-sonnet-4-6`
/// (a cross-region inference profile) must not print "Unknown model", and a
/// stray geo prefix still does. Already true at yoagent 0.27.2 with no yoyo
/// change: `ModelConfig::preset("bedrock", id)` (read by
/// `model_is_known_for_provider`) resolves the profile through
/// `bedrock_anthropic_id`. Checked: this test passed before any edit here.
#[test]
fn bedrock_inference_profile_ids_are_known_models() {
    use crate::providers::model_is_known_for_provider as known;
    for id in [
        "us.anthropic.claude-sonnet-4-6",
        "eu.anthropic.claude-sonnet-4-6",
        "global.anthropic.claude-opus-5-5",
        "us.anthropic.claude-haiku-4-5-20251001-v1:0",
    ] {
        assert!(known("bedrock", id), "{id}");
    }
    for id in [
        "xx.anthropic.claude-sonnet-4-6",
        "anthropic.claude-sonnet-4-6-v1:0",
    ] {
        assert!(!known("bedrock", id), "{id}");
    }
    // Provider-scoped: a Bedrock id under another provider stays unknown.
    assert!(!known("anthropic", "us.anthropic.claude-sonnet-4-6"));
}

/// #1144 item 3, #1121: Claude ids yoyo must not offer. `claude-sonnet-4-5`
/// retires 2026-11-30 and yoagent ships no row for it; `claude-sonnet-4-7`
/// never existed (`--model claude-sonnet-4-7` is a 404); the 2025-05-14
/// Sonnet 4 snapshot retired 2026-06-15. Pricing rows are a different
/// question (a user still on 4.5 is billed until it retires) and stay.
const DEAD_CLAUDE_STEMS: [&str; 3] = [
    "claude-sonnet-4-5",
    "claude-sonnet-4-7",
    "claude-sonnet-4-20250514",
];

/// Every list yoyo offers Claude ids from: the anthropic and bedrock known
/// lists (the 404 hint's "Available models", the setup wizard, the unknown-model
/// warning) and the `/model <Tab>` completions.
fn offered_claude_ids() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for provider in ["anthropic", "bedrock"] {
        for id in crate::providers::known_models_for_provider(provider) {
            // Bedrock's list also carries Amazon Nova; only Claude ids are
            // yoagent's to vouch for here.
            if id.contains("claude") {
                out.push((provider, *id));
            }
        }
    }
    for id in crate::commands::KNOWN_MODELS {
        if id.contains("claude") {
            out.push(("/model completion", *id));
        }
    }
    out
}

#[test]
fn no_offered_list_carries_a_retired_or_nonexistent_claude_id() {
    let offered = offered_claude_ids();
    assert!(
        offered.len() > 10,
        "anti-vacuous: the walk must reach the real lists"
    );
    for (source, id) in &offered {
        for dead in DEAD_CLAUDE_STEMS {
            assert!(!id.contains(dead), "{source} still offers dead id {id}");
        }
    }
    // Near miss: claude-sonnet-4-6 shares the dead ids' stem and is current.
    assert!(offered
        .iter()
        .any(|(_, id)| id.contains("claude-sonnet-4-6")));
}

/// The Anthropic id behind an offered id: drop Bedrock's `anthropic.` and its
/// `-v1:0` revision, then an 8-digit snapshot date (`-20251001`).
fn anthropic_stem(id: &str) -> &str {
    let id = id.strip_prefix("anthropic.").unwrap_or(id);
    let id = id.strip_suffix("-v1:0").unwrap_or(id);
    match id.rsplit_once('-') {
        Some((stem, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => stem,
        _ => id,
    }
}

/// yoagent is the authority for which Claude ids are real: an offered id must
/// be a yoagent preset (`ModelConfig::preset`) or a row of yoagent's bundled
/// price table under `anthropic`. A made-up id like `claude-sonnet-4-7` is
/// neither, so the next one fails here instead of being offered.
fn yoagent_knows(provider: &str, id: &str) -> bool {
    let stem = anthropic_stem(id);
    let priced = yoagent::provider::PriceTable::builtin()
        .iter()
        .any(|(p, model, _)| p == "anthropic" && model == stem);
    ModelConfig::preset(provider, id).is_some()
        || ModelConfig::preset("anthropic", stem).is_some()
        || priced
}

#[test]
fn every_offered_claude_id_is_known_to_yoagent() {
    for (source, id) in offered_claude_ids() {
        let provider = if source == "bedrock" {
            "bedrock"
        } else {
            "anthropic"
        };
        assert!(
            yoagent_knows(provider, id),
            "{source} offers {id}, which has no yoagent preset or price row"
        );
    }
    // Recall control: the three dead ids really fail the authority, so this
    // test would have caught them.
    for dead in DEAD_CLAUDE_STEMS {
        assert!(!yoagent_knows("anthropic", dead), "{dead} must fail");
    }
    assert!(!yoagent_knows("bedrock", "anthropic.claude-sonnet-4-7"));
}

#[test]
fn anthropic_default_leads_the_list_the_404_hint_suggests_from() {
    let known = crate::providers::known_models_for_provider("anthropic");
    let default = crate::providers::default_model_for_provider("anthropic");
    assert_eq!(known[0], default, "the hint's switch line names known[0]");
    for current in ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"] {
        assert!(known.contains(&current), "anthropic list lacks {current}");
    }
    let bedrock = crate::providers::known_models_for_provider("bedrock");
    assert_eq!(
        bedrock[0],
        crate::providers::default_model_for_provider("bedrock")
    );
}

/// #1144 item 2: every Claude id yoyo offers reports the context window the
/// agent is really built with, and every output maximum yoagent's table
/// states is the one the ceiling warning uses. Expected values come from
/// `create_model_config` and yoagent's own table, never typed.
#[test]
fn offered_claude_ids_report_yoagents_limits_not_a_yoyo_copy() {
    use crate::agent_builder::{anthropic_output_maximum, create_model_config};
    use crate::commands_info::model_context_window;
    let table = yoagent::provider::prices::PriceTable::builtin();
    for provider in ["anthropic", "bedrock"] {
        for id in crate::providers::known_models_for_provider(provider) {
            if !id.contains("claude") {
                continue;
            }
            let built = create_model_config(provider, id, None).context_window;
            assert_eq!(
                model_context_window(id),
                Some(u64::from(built)),
                "{provider}/{id}"
            );
        }
    }
    let mut stated = 0;
    for (provider, id, entry) in table.iter() {
        if provider == "anthropic" && entry.max_output.is_some() {
            stated += 1;
            assert_eq!(
                anthropic_output_maximum(id),
                entry.max_output,
                "{id}: output max"
            );
            assert_eq!(
                model_context_window(id),
                entry.context_window.map(u64::from),
                "{id}: window"
            );
        }
    }
    // Anti-vacuous: the table path is exercised by an id with no named preset.
    assert!(
        stated >= 3,
        "yoagent states limits for {stated} anthropic rows"
    );
    assert!(anthropic_preset("claude-opus-4-7").is_none());
    assert!(table
        .entry("anthropic", "claude-opus-4-7")
        .and_then(|e| e.max_output)
        .is_some());
}

/// Near-misses for #1144: ids yoagent supplies nothing for get yoagent's
/// generic default (what the agent runs at), not the old table's confident 1M.
#[test]
fn claude_ids_yoagent_does_not_know_get_the_default_window_not_1m() {
    use crate::agent_builder::{anthropic_output_maximum, create_model_config};
    use crate::commands_info::model_context_window;
    for id in [
        "claude-zzz-9",
        "claude-sonnet-4-5",
        "claude-sonnet-4-20250514",
    ] {
        let built = u64::from(create_model_config("anthropic", id, None).context_window);
        assert_eq!(model_context_window(id), Some(built), "{id}");
        assert_ne!(
            model_context_window(id),
            Some(1_000_000),
            "{id}: the old sonnet-4* guess"
        );
        assert_eq!(
            anthropic_output_maximum(id),
            None,
            "{id}: unknown stays unknown"
        );
    }
    // Bedrock opus-4-7 has no yoagent Bedrock mapping in 0.27.2 (#1121): it
    // runs at the arm's 200K, and /model info now says so instead of 1M.
    let bedrock = "anthropic.claude-opus-4-7";
    let built = u64::from(create_model_config("bedrock", bedrock, None).context_window);
    assert_eq!(model_context_window(bedrock), Some(built));
}
