//! GPT-6 Astra/Sol/Luna (Day 223): on `provider = "openai"` these ids take
//! yoagent's preset window, max_tokens, cost and reasoning instead of the bare
//! `ModelConfig::openai` defaults (128K window, 4096 max_tokens), and the two
//! readers — `/model info`'s window and `/cost`'s price table — report the
//! same values. Expected values are DERIVED from yoagent's own constructors,
//! never typed in. Lives beside `agent_builder.rs` because that module is at
//! its size gate.

use crate::agent_builder::{
    create_model_config, max_tokens_ceiling_warning, openai_compat_output_ceiling, openai_preset,
    GPT_6_OUTPUT_MAXIMUM,
};
use crate::commands_info::model_context_window;
use crate::config::ModelPricingOverrides;
use crate::format::{estimate_request_cost_with, model_pricing_with};
use yoagent::provider::{ApiProtocol, ModelConfig};
use yoagent::Usage;

/// (id, the yoagent constructor that owns that exact id).
fn presets() -> Vec<(&'static str, ModelConfig)> {
    vec![
        ("gpt-6-astra", ModelConfig::gpt_6_astra()),
        ("gpt-6-sol", ModelConfig::gpt_6_sol()),
        ("gpt-6-luna", ModelConfig::gpt_6_luna()),
    ]
}

/// Ids that must NOT resolve to a GPT-6 preset. `gpt-6.1-sol` is the one that
/// matters: a prefix match would bill it as `gpt-6-sol` (#1003 class).
const NEAR_MISSES: &[&str] = &[
    "gpt-6.1-sol",
    "gpt-6",
    "gpt-6-sol-mini",
    "GPT-6-SOL",
    "gpt-5",
    "gpt-4o",
];

#[test]
fn openai_preset_is_the_yoagent_constructor_for_each_gpt6_id() {
    for (id, ctor) in presets() {
        let p = openai_preset(id).unwrap_or_else(|| panic!("{id}: no preset"));
        assert_eq!(p.id, id);
        assert_eq!(p.context_window, ctor.context_window, "{id}");
        assert_eq!(p.max_tokens, ctor.max_tokens, "{id}");
        assert_eq!(p.cost, ctor.cost, "{id}");
        // Anti-vacuous: the constructor really carries the values the bare
        // OpenAI config lacks, so equality above is not 128K == 128K.
        assert_eq!(ctor.context_window, 1_050_000, "{id}");
        assert_eq!(ctor.max_tokens, 64_000, "{id}");
        assert!(ctor.cost.is_some(), "{id}: yoagent ships a price");
    }
}

#[test]
fn openai_arm_takes_window_max_tokens_cost_reasoning_and_keeps_the_wire() {
    for (id, ctor) in presets() {
        let bare = ModelConfig::openai(id, id);
        let got = create_model_config("openai", id, None);
        assert_eq!(
            (got.context_window, got.max_tokens),
            (1_050_000, 64_000),
            "{id}"
        );
        assert_eq!(got.cost, ctor.cost, "{id}");
        assert!(got.reasoning, "{id}");
        // Deliberately NOT taken: Responses API, compat, base_url.
        assert_eq!(got.api, ApiProtocol::OpenAiCompletions, "{id}");
        assert_ne!(ctor.api, got.api, "{id}: preset really is a different wire");
        assert_eq!(got.base_url, bare.base_url, "{id}");
        assert_eq!(
            format!("{:?}", got.compat),
            format!("{:?}", bare.compat),
            "{id}"
        );
        // A custom base URL still wins.
        let proxied = create_model_config("openai", id, Some("https://proxy.example/v1"));
        assert_eq!(proxied.base_url, "https://proxy.example/v1");
        assert_eq!(proxied.context_window, 1_050_000);
    }
}

#[test]
fn near_misses_keep_the_bare_openai_config() {
    for id in NEAR_MISSES {
        assert!(openai_preset(id).is_none(), "{id} must not match a preset");
        let bare = ModelConfig::openai(*id, *id);
        let got = create_model_config("openai", id, None);
        assert_eq!(
            (got.context_window, got.max_tokens, got.reasoning),
            (bare.context_window, bare.max_tokens, bare.reasoning),
            "{id}"
        );
        assert_eq!(got.cost, bare.cost, "{id}");
    }
    // The full pair for the provider's default model, written out.
    let gpt5 = create_model_config("openai", "gpt-5", None);
    assert_eq!((gpt5.context_window, gpt5.max_tokens), (128_000, 4096));
}

#[test]
fn other_openai_compatible_providers_do_not_take_the_preset() {
    // An OpenRouter or Groq id that happens to spell `gpt-6-sol` is not
    // OpenAI's endpoint; the preset is scoped to the `"openai"` arm.
    for provider in ["openrouter", "groq", "custom"] {
        let got = create_model_config(provider, "gpt-6-sol", None);
        assert_ne!(got.context_window, 1_050_000, "{provider}");
    }
}

#[test]
fn model_info_window_reads_the_preset() {
    for (id, ctor) in presets() {
        assert_eq!(
            model_context_window(id),
            Some(u64::from(ctor.context_window)),
            "{id}"
        );
    }
    // Near-misses: unchanged readings (gpt-6.1-sol stays unknown — residue).
    assert_eq!(model_context_window("gpt-6.1-sol"), None);
    assert_eq!(model_context_window("gpt-5"), Some(1_048_576));
    assert_eq!(model_context_window("gpt-4o"), Some(128_000));
}

#[test]
fn cost_table_reads_the_preset_price() {
    let none = ModelPricingOverrides::default();
    for (id, ctor) in presets() {
        let c = ctor.cost.unwrap();
        let want = Some((
            c.input_per_million,
            c.cache_write_per_million,
            c.cache_read_per_million,
            c.output_per_million,
        ));
        assert_eq!(model_pricing_with(&none, id), want, "{id}");
        // Router prefix is stripped exactly as for Anthropic presets.
        assert_eq!(
            model_pricing_with(&none, &format!("openai/{id}")),
            want,
            "openai/{id}"
        );
    }
    assert_eq!(model_pricing_with(&none, "gpt-6.1-sol"), None);
    assert_eq!(model_pricing_with(&none, "gpt-6"), None);
    assert_eq!(
        model_pricing_with(&none, "gpt-5"),
        Some((2.00, 0.0, 0.0, 8.00))
    );
}

fn usage_with_prompt(prompt: u64) -> Usage {
    Usage {
        input: prompt,
        output: 2_000,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
    }
}

#[test]
fn request_above_272k_uses_the_gpt6_tier_and_below_stays_flat() {
    let none = ModelPricingOverrides::default();
    for (id, ctor) in presets() {
        let cost = ctor.cost.unwrap();
        // Anti-vacuous: yoagent ships a tier for these ids.
        let tier = cost
            .context_tiers
            .first()
            .unwrap_or_else(|| panic!("{id}: yoagent ships no context tier"));
        let above = usage_with_prompt(tier.above_prompt_tokens + 10_000);
        let (price, fired) = estimate_request_cost_with(&none, &above, id).unwrap();
        assert!(fired, "{id}: tier must fire above its threshold");
        assert_eq!(price, cost.cost_usd(&above), "{id}");

        let below = usage_with_prompt(tier.above_prompt_tokens - 10_000);
        let (_, fired) = estimate_request_cost_with(&none, &below, id).unwrap();
        assert!(!fired, "{id}: below the threshold the price is flat");
    }
}

#[test]
fn output_ceiling_is_the_model_maximum_not_the_preset_default() {
    for (id, ctor) in presets() {
        let ceiling = openai_compat_output_ceiling("openai", id, ctor.max_tokens);
        assert_eq!(ceiling, GPT_6_OUTPUT_MAXIMUM, "{id}");
        // A correct 128000 must not warn; one over must.
        assert_eq!(max_tokens_ceiling_warning(128_000, ceiling, id, true), None);
        assert!(max_tokens_ceiling_warning(128_001, ceiling, id, true).is_some());
    }
    // Near-misses keep the pre-existing rule: the config's own max_tokens.
    assert_eq!(openai_compat_output_ceiling("openai", "gpt-5", 4096), 4096);
    assert_eq!(
        openai_compat_output_ceiling("openai", "gpt-6.1-sol", 4096),
        4096
    );
    assert_eq!(
        openai_compat_output_ceiling("openrouter", "gpt-6-sol", 4096),
        4096
    );
}

#[test]
fn gpt6_ids_are_known_models_for_openai_only() {
    use crate::providers::model_is_known_for_provider;
    for (id, _) in presets() {
        assert!(model_is_known_for_provider("openai", id), "{id}");
    }
    assert!(!model_is_known_for_provider("openai", "gpt-6.1-sol"));
    assert!(!model_is_known_for_provider("openai", "gpt-6"));
}
