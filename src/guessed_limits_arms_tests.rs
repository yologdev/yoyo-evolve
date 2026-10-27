//! #1021 (Day 241): the anthropic and bedrock arms of `build_agent` call the
//! guessed-limits note like every other arm. One row per measured case; the
//! note is computed exactly as the call site computes it (limits from the
//! resolved `create_model_config`, no user overrides).

use crate::agent_builder::create_model_config;
use crate::model_limits_note::unknown_model_limits_note;

fn note_for(provider: &str, model: &str) -> Option<String> {
    let c = create_model_config(provider, model, None);
    unknown_model_limits_note(
        provider,
        model,
        c.context_window,
        c.max_tokens,
        None,
        None,
        true,
    )
}

fn expected(provider: &str, model: &str, window: u32, max: u32) -> String {
    format!(
        "warning: no preset is known for {provider} model '{model}', so yoyo is using \
guessed limits: a {window}-token context window and max_tokens = {max}. Every reply (reasoning \
included) is capped at {max} output tokens. If the model allows more, set max_tokens and \
context_window in .yoyo.toml (or pass --max-tokens / --context-window)."
    )
}

/// The caller line: this repo's own loop model, and the no-config anthropic
/// default. It has a preset with real limits, so it must stay silent.
#[test]
fn anthropic_preset_id_is_silent() {
    assert_eq!(
        crate::providers::default_model_for_provider("anthropic"),
        "claude-opus-5-5"
    );
    assert_eq!(note_for("anthropic", "claude-opus-5-5"), None);
}

#[test]
fn anthropic_id_without_preset_warns_with_the_generic_guess() {
    let c = create_model_config("anthropic", "claude-made-up-9", None);
    // Premise: the generic anthropic guess, read from yoagent, not typed.
    let g = yoagent::provider::ModelConfig::anthropic("x", "x");
    assert_eq!(
        (c.context_window, c.max_tokens),
        (g.context_window, g.max_tokens)
    );
    assert_eq!(
        note_for("anthropic", "claude-made-up-9"),
        Some(expected(
            "anthropic",
            "claude-made-up-9",
            g.context_window,
            g.max_tokens
        ))
    );
}

/// Bedrock has no presets at all, so its default id runs on the same
/// 200K / yoagent-default guess as any other id, and saying so is true.
/// The next #1021 slice gives the default real limits; this row then flips.
#[test]
fn bedrock_default_and_other_ids_warn_because_they_run_on_a_guess() {
    let default = crate::providers::default_model_for_provider("bedrock");
    assert_eq!(default, "anthropic.claude-sonnet-4-6");
    for id in [default.as_str(), "amazon.nova-pro-v1:0"] {
        let c = create_model_config("bedrock", id, None);
        assert_eq!(c.context_window, 200_000, "{id}");
        assert_eq!(
            note_for("bedrock", id),
            Some(expected("bedrock", id, 200_000, c.max_tokens)),
            "{id}"
        );
    }
}

/// Emission-point guard: both arms of `build_agent` call the note (the
/// function body is sliced so other callers in the file do not count).
#[test]
fn every_build_agent_arm_calls_the_guessed_limits_note() {
    let src = include_str!("agent_builder.rs");
    let start = src
        .find(&["pub fn ", "build_agent(&self)"].concat())
        .unwrap();
    let end = start + src[start..].find("\n    }\n").unwrap();
    let body = &src[start..end];
    let needle = ["warn_once_if_guessing", "("].concat();
    // anthropic, google, bedrock, openai-compatible: four arms, four calls.
    assert_eq!(body.matches("} else if self.provider").count() + 2, 4);
    assert_eq!(body.matches(needle.as_str()).count(), 4);
}
