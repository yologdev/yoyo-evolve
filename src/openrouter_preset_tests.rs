//! #1037 (Day 247): OpenRouter spells every Anthropic id with dots
//! (`anthropic/claude-sonnet-5.5`, measured against
//! `https://openrouter.ai/api/v1/models`), so the openrouter arm ran every
//! Claude id on the generic OpenAI-compatible guess (128K window,
//! max_tokens = 4096) and warned about it. These pin the mapping onto the
//! anthropic preset, the near-misses that must stay on the guess, and the
//! no-config default.

use crate::agent_builder::{anthropic_preset, create_model_config};
use crate::model_limits_note::runs_on_guessed_limits;
use crate::providers::{default_model_for_provider, openrouter_anthropic_preset_id};

#[test]
fn openrouter_anthropic_preset_id_table() {
    let rows: &[(&str, Option<&str>)] = &[
        // OpenRouter's own (dotted) spelling.
        ("anthropic/claude-sonnet-5.5", Some("claude-sonnet-5-5")),
        ("anthropic/claude-opus-4.8", Some("claude-opus-4-8")),
        ("anthropic/claude-sonnet-4.6", Some("claude-sonnet-4-6")),
        // No dot at all: passes through unchanged.
        ("anthropic/claude-opus-5", Some("claude-opus-5")),
        // The dashed spelling yoyo used to default to: existing configs keep
        // resolving to the same preset id.
        ("anthropic/claude-sonnet-5-5", Some("claude-sonnet-5-5")),
        // Mapped, but no preset exists for it (see the next test).
        ("anthropic/claude-unknown-9.9", Some("claude-unknown-9-9")),
        // A dot that is not between two digits is left alone.
        ("anthropic/claude.x-5", Some("claude.x-5")),
        // Variant suffixes (`:batch`, `:beta`) are a different price: not mapped.
        ("anthropic/claude-sonnet-5.5:batch", None),
        // Other vendors, no prefix, empty.
        ("openai/gpt-6-sol", None),
        ("claude-sonnet-5.5", None),
        ("anthropic/", None),
        ("", None),
    ];
    for (input, want) in rows {
        assert_eq!(
            openrouter_anthropic_preset_id(input).as_deref(),
            *want,
            "input {input:?}"
        );
    }
}

/// The generic guess the openrouter arm falls back to, read from yoagent.
fn guess() -> (u32, u32) {
    let g = yoagent::provider::ModelConfig::openai("x", "x");
    (g.context_window, g.max_tokens)
}

#[test]
fn dotted_and_dashed_anthropic_ids_get_the_anthropic_preset_limits() {
    for (model, preset_id) in [
        ("anthropic/claude-sonnet-5.5", "claude-sonnet-5-5"),
        ("anthropic/claude-sonnet-5-5", "claude-sonnet-5-5"),
        ("anthropic/claude-opus-4.8", "claude-opus-4-8"),
        // Day 247 (#1116): sonnet-4-6 got an anthropic preset row, so the
        // dotted 4.6 id left the near-miss list below and resolves here.
        ("anthropic/claude-sonnet-4.6", "claude-sonnet-4-6"),
    ] {
        let p = anthropic_preset(preset_id).expect("premise: the anthropic preset exists");
        let c = create_model_config("openrouter", model, None);
        assert_eq!(
            (c.context_window, c.max_tokens),
            (p.context_window, p.max_tokens),
            "{model}"
        );
        // Anti-vacuous: the preset really differs from the guess it replaces.
        assert_ne!((c.context_window, c.max_tokens), guess(), "{model}");
        assert_eq!(
            c.cost.as_ref().map(|k| k.output_per_million),
            p.cost.as_ref().map(|k| k.output_per_million)
        );
        // The wire stays OpenRouter's own.
        assert_eq!(c.provider, "openrouter");
        assert_eq!(c.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(c.id, model, "the id sent to OpenRouter is the user's");
        assert!(!runs_on_guessed_limits("openrouter", model), "{model}");
    }
}

#[test]
fn near_misses_stay_on_the_guess_and_still_warn() {
    for model in [
        // Mapped id, but no anthropic preset: the guessed-limits warning fires.
        "anthropic/claude-unknown-9.9",
        // Not mapped at all.
        "anthropic/claude-sonnet-5.5:batch",
        "anthropic/claude-sonnet-4.6:batch",
        // Day 249 (#1122): `openai/gpt-6-sol` LEFT this list on purpose: it
        // now takes yoagent's openai preset (see the vendor test below).
        // These have no yoagent row (or an unknown vendor) and still guess.
        "openai/gpt-6",
        "meta-llama/llama-4",
    ] {
        let c = create_model_config("openrouter", model, None);
        assert_eq!((c.context_window, c.max_tokens), guess(), "{model}");
        assert!(runs_on_guessed_limits("openrouter", model), "{model}");
    }
}

#[test]
fn only_the_openrouter_arm_maps() {
    // The same string on another OpenAI-compatible arm keeps that arm's guess.
    let c = create_model_config("custom", "anthropic/claude-sonnet-5.5", None);
    let p = anthropic_preset("claude-sonnet-5-5").unwrap();
    assert_ne!(
        (c.context_window, c.max_tokens),
        (p.context_window, p.max_tokens)
    );
}

#[test]
fn no_config_default_is_a_listed_dotted_id_with_real_limits() {
    let d = default_model_for_provider("openrouter");
    assert_eq!(d, "anthropic/claude-sonnet-5.5");
    assert!(!runs_on_guessed_limits("openrouter", &d));
}

#[test]
fn cost_prices_a_dotted_id_like_its_anthropic_preset() {
    let usage = yoagent::Usage {
        input: 1_000_000,
        output: 1_000_000,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
    };
    for (dotted, dashed) in [
        ("anthropic/claude-opus-4.8", "claude-opus-4-8"),
        ("anthropic/claude-sonnet-5.5", "claude-sonnet-5-5"),
    ] {
        let a = crate::format::estimate_cost(&usage, dotted).expect(dotted);
        let b = crate::format::estimate_cost(&usage, dashed).expect(dashed);
        assert!((a - b).abs() < 1e-9, "{dotted}: {a} vs {dashed}: {b}");
    }
}

/// #1122 (Day 249): non-Anthropic `vendor/id`s take yoagent's routed vendor
/// preset limits instead of the 128K / 4096 guess. Expected values are READ
/// from `ModelConfig::preset("openrouter", ..)`, never typed, and checked
/// against the first-party vendor preset so the routing is what is tested.
#[test]
fn other_vendors_take_yoagents_routed_preset_limits() {
    use yoagent::provider::ModelConfig;
    for (model, vendor, bare) in [
        ("google/gemini-3.6-flash", "google", "gemini-3.6-flash"),
        ("google/gemini-3.8-flash", "google", "gemini-3.8-flash"),
        ("x-ai/grok-4.7", "xai", "grok-4.7"),
        ("openai/gpt-6-sol", "openai", "gpt-6-sol"),
    ] {
        let routed = ModelConfig::preset("openrouter", model).expect("premise: yoagent row");
        let first_party = ModelConfig::preset(vendor, bare).expect("premise: vendor row");
        assert_eq!(
            (routed.context_window, routed.max_tokens),
            (first_party.context_window, first_party.max_tokens),
            "premise: yoagent routes {model} to the {vendor} row"
        );
        let c = create_model_config("openrouter", model, None);
        assert_eq!(
            (c.context_window, c.max_tokens),
            (routed.context_window, routed.max_tokens),
            "{model}"
        );
        assert_eq!(c.reasoning, routed.reasoning, "{model}");
        // Anti-vacuous: the preset really differs from the guess it replaces.
        assert_ne!((c.context_window, c.max_tokens), guess(), "{model}");
        assert!(!runs_on_guessed_limits("openrouter", model), "{model}");
        // The wire stays yoyo's: provider, URL, the user's id.
        assert_eq!(c.provider, "openrouter");
        assert_eq!(c.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(c.id, model);
        // A user's --base-url still wins over yoagent's hardcoded one.
        let custom = create_model_config("openrouter", model, Some("http://gw.local/v1"));
        assert_eq!(custom.base_url, "http://gw.local/v1", "{model}");
        assert_eq!(custom.max_tokens, routed.max_tokens, "{model}");
    }
}

/// Order matters (#1122): yoyo's anthropic mapping runs FIRST, because yoagent
/// refuses the dashed spelling existing configs carry and has no price. The
/// dashed ids are the rows where the two resolvers disagree.
#[test]
fn dashed_anthropic_ids_still_resolve_where_yoagent_refuses_them() {
    use yoagent::provider::ModelConfig;
    for (dashed, dotted) in [
        ("anthropic/claude-sonnet-4-6", "anthropic/claude-sonnet-4.6"),
        ("anthropic/claude-opus-5-5", "anthropic/claude-opus-5.5"),
    ] {
        assert!(
            ModelConfig::preset("openrouter", dashed).is_none(),
            "premise: yoagent refuses {dashed}"
        );
        let a = create_model_config("openrouter", dashed, None);
        let b = create_model_config("openrouter", dotted, None);
        assert_eq!(
            (a.context_window, a.max_tokens),
            (b.context_window, b.max_tokens)
        );
        assert!(a.cost.is_some() && b.cost.is_some(), "{dashed}: priced");
        assert!(!runs_on_guessed_limits("openrouter", dashed), "{dashed}");
    }
}
