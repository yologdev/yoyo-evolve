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
        // yoagent has no claude-sonnet-4-6 constructor, so 4.6 is still a guess.
        "anthropic/claude-sonnet-4.6",
        // Not mapped at all.
        "anthropic/claude-sonnet-5.5:batch",
        "openai/gpt-6-sol",
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
