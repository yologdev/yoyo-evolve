//! yoagent 0.25.8 presets for Google, xAI and Mistral (#1021, #1037):
//! `gemini-4-argon`, `gemini-3.8-flash` (google), `grok-4.7` (xai) and
//! `mistral-large-4` (mistral). Before this, those ids ran on yoyo's generic
//! arms: google at 1M / max_tokens 8192 with reasoning off, xai and mistral at
//! the OpenAI base config's guessed 128K / 4096 with no price, and `/cost`
//! read a hand-kept substring estimate (grok-4's $3/$15 for grok-4.7).
//!
//! Every expected value is read from the yoagent constructor, never typed in.
//! Kept out of agent_builder.rs for its size gate, like `openai_preset_tests.rs`.

use crate::agent_builder::{
    create_model_config, declared_output_ceiling, max_tokens_ceiling_warning, vendor_preset,
};
use crate::commands_info::model_context_window;
use crate::config::ModelPricingOverrides;
use crate::format::{estimate_request_cost_with, model_pricing_with};
use yoagent::provider::ModelConfig;
use yoagent::Usage;

/// (provider, id, the yoagent constructor that owns that exact id).
fn presets() -> Vec<(&'static str, &'static str, ModelConfig)> {
    vec![
        ("google", "gemini-4-argon", ModelConfig::gemini_4_argon()),
        (
            "google",
            "gemini-3.8-flash",
            ModelConfig::gemini_3_8_flash(),
        ),
        ("xai", "grok-4.7", ModelConfig::grok_4_7()),
        ("mistral", "mistral-large-4", ModelConfig::mistral_large_4()),
    ]
}

/// What yoyo's arm built for `id` before this change: the generic constructor
/// each arm used, with the arm's own provider-specific fields.
fn generic(provider: &str, id: &str) -> ModelConfig {
    match provider {
        "google" => ModelConfig::google(id, id),
        "xai" | "mistral" => {
            let mut c = ModelConfig::openai(id, id);
            c.provider = provider.into();
            c
        }
        other => panic!("no generic arm for {other}"),
    }
}

/// Must NOT resolve to a preset (#1003: a prefix match would misattribute).
const NEAR_MISSES: &[(&str, &str)] = &[
    ("google", "gemini-3.8-flash-lite"),
    ("google", "gemini-4-argon-preview"),
    ("google", "gemini-2.5-flash"),
    ("xai", "grok-4.7-mini"),
    ("xai", "grok-4"),
    ("mistral", "mistral-large-latest"),
    ("mistral", "mistral-large-4-2611"),
];

#[test]
fn create_model_config_takes_the_preset_limits_price_and_reasoning() {
    for (provider, id, ctor) in presets() {
        let got = create_model_config(provider, id, None);
        assert_eq!(got.id, id);
        assert_eq!(got.context_window, ctor.context_window, "{id}");
        assert_eq!(got.max_tokens, ctor.max_tokens, "{id}");
        assert_eq!(got.reasoning, ctor.reasoning, "{id}");
        assert_eq!(got.cost, ctor.cost, "{id}");
        assert!(got.cost.is_some(), "{id}: yoagent ships a price");
        // Anti-vacuous: the preset really differs from what the arm built.
        let old = generic(provider, id);
        assert_ne!(
            (old.context_window, old.max_tokens),
            (ctor.context_window, ctor.max_tokens),
            "{id}: fixture premise"
        );
    }
}

#[test]
fn provider_specific_fields_are_unchanged_by_the_preset() {
    // Compare with a non-preset id through the SAME arm, so everything the
    // arm adds after construction (User-Agent, compat) is in both.
    for (provider, id, _) in presets() {
        let got = create_model_config(provider, id, None);
        let sibling = create_model_config(provider, "not-a-preset-id", None);
        assert_eq!(got.provider, sibling.provider, "{id}");
        assert_eq!(got.api, sibling.api, "{id}");
        assert_eq!(got.base_url, sibling.base_url, "{id}");
        assert_eq!(got.headers, sibling.headers, "{id}");
        assert_eq!(
            format!("{:?}", got.compat),
            format!("{:?}", sibling.compat),
            "{id}"
        );
        let proxied = create_model_config(provider, id, Some("https://proxy.example/v1"));
        assert_eq!(proxied.base_url, "https://proxy.example/v1", "{id}");
        assert_eq!(proxied.context_window, got.context_window, "{id}");
    }
}

#[test]
fn near_misses_keep_the_generic_arm() {
    for (provider, id) in NEAR_MISSES {
        assert!(vendor_preset(provider, id).is_none(), "{id}");
        let got = create_model_config(provider, id, None);
        let old = generic(provider, id);
        assert_eq!(
            (got.context_window, got.max_tokens, got.reasoning),
            (old.context_window, old.max_tokens, old.reasoning),
            "{provider}/{id}"
        );
        assert_eq!(got.cost, old.cost, "{provider}/{id}");
    }
}

#[test]
fn a_preset_id_on_another_provider_does_not_take_it() {
    // Same spelling, different endpoint: the preset is scoped to its vendor.
    for (provider, id) in [
        ("openrouter", "grok-4.7"),
        ("groq", "mistral-large-4"),
        ("xai", "gemini-4-argon"),
    ] {
        assert!(vendor_preset(provider, id).is_none(), "{provider}/{id}");
    }
}

#[test]
fn model_info_window_reads_the_preset() {
    for (_, id, ctor) in presets() {
        assert_eq!(
            model_context_window(id),
            Some(u64::from(ctor.context_window)),
            "{id}"
        );
    }
    // Near-misses keep the hand table's reading.
    assert_eq!(model_context_window("grok-4.7-mini"), Some(131_072));
    assert_eq!(model_context_window("mistral-large-latest"), Some(128_000));
}

#[test]
fn cost_table_reads_the_preset_price() {
    let none = ModelPricingOverrides::default();
    for (_, id, ctor) in presets() {
        let c = ctor.cost.unwrap();
        let want = Some((
            c.input_per_million,
            c.cache_write_per_million,
            c.cache_read_per_million,
            c.output_per_million,
        ));
        // After Google's introductory windows end (#1012, Day 242): before then the
        // two google ids bill the promo rate, pinned in `format/cost/promo_tests.rs`.
        let got =
            crate::format::promo::with_now(crate::format::promo::AFTER_ALL_PROMOS_UNIX, || {
                model_pricing_with(&none, id)
            });
        assert_eq!(got, want, "{id}");
    }
    // Near-miss keeps the hand-kept estimate it had before.
    assert_eq!(
        model_pricing_with(&none, "grok-4.7-mini"),
        model_pricing_with(&none, "grok-4-mini")
    );
}

#[test]
fn grok_47_request_cost_uses_its_200k_tier() {
    let none = ModelPricingOverrides::default();
    let tiers = ModelConfig::grok_4_7().cost.unwrap().context_tiers;
    assert!(!tiers.is_empty(), "fixture premise: yoagent ships a tier");
    let usage = |input| Usage {
        input,
        output: 2_000,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
    };
    let (_, tiered) = estimate_request_cost_with(&none, &usage(250_000), "grok-4.7").unwrap();
    assert!(tiered, "250K prompt is above the 200K tier");
    let (_, flat) = estimate_request_cost_with(&none, &usage(10_000), "grok-4.7").unwrap();
    assert!(!flat);
}

/// Composed exactly as `build_agent` composes it.
fn ceiling_warning(provider: &str, model: &str, configured: u32) -> Option<String> {
    let resolved = create_model_config(provider, model, None).max_tokens;
    declared_output_ceiling(provider, model, resolved)
        .and_then(|c| max_tokens_ceiling_warning(configured, c, model, true))
}

#[test]
fn output_ceiling_is_declared_only_where_the_vendor_publishes_one() {
    // Gemini: 65,536 is the published streaming output cap.
    for (provider, id) in [("google", "gemini-4-argon"), ("google", "gemini-3.8-flash")] {
        let cap = vendor_preset(provider, id).unwrap().max_tokens;
        assert!(ceiling_warning(provider, id, cap + 1).is_some(), "{id}");
        assert_eq!(ceiling_warning(provider, id, cap), None, "{id}");
    }
    // xAI and Mistral publish no output limit: the preset's max_tokens is a
    // request default, not a maximum (#964/#1019), so no ceiling claim.
    for (provider, id) in [("xai", "grok-4.7"), ("mistral", "mistral-large-4")] {
        assert_eq!(ceiling_warning(provider, id, 200_000), None, "{id}");
    }
    // Generic google ids still declare nothing (#1020).
    assert_eq!(ceiling_warning("google", "gemini-2.5-flash", 200_000), None);
}

#[test]
fn preset_ids_are_known_and_do_not_get_the_guessed_limits_note() {
    use crate::model_limits_note::unknown_model_limits_note;
    use crate::providers::model_is_known_for_provider;
    for (provider, id, _) in presets() {
        assert!(model_is_known_for_provider(provider, id), "{id}");
        let c = create_model_config(provider, id, None);
        assert_eq!(
            unknown_model_limits_note(
                provider,
                id,
                c.context_window,
                c.max_tokens,
                None,
                None,
                true
            ),
            None,
            "{id}"
        );
    }
    // Near-miss on an OpenAI-base provider still warns.
    let c = create_model_config("xai", "grok-4.7-mini", None);
    assert!(unknown_model_limits_note(
        "xai",
        "grok-4.7-mini",
        c.context_window,
        c.max_tokens,
        None,
        None,
        true
    )
    .is_some());
}
