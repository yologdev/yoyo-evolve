//! The honest half of an OpenAI-compatible model id with no preset (Day 224).
//!
//! Day 223 gave `gpt-6-astra/sol/luna` exact-match presets on provider
//! `openai`. Exact matching was right, and it routes every OTHER id into
//! yoagent's base config: a 128K window and `max_tokens = 4096`, which caps
//! reasoning plus answer on every turn and used to say nothing. This module
//! does not invent limits for unknown ids (there is no source for them); it
//! makes the fallback say what it is assuming. `AgentConfig::build_agent`'s
//! OpenAI-compatible branch is the one consumer.

use crate::agent_builder::openai_preset;

/// Provider arms of `create_model_config` where an id with no preset inherits
/// yoagent's base guess: a 128K window and `max_tokens = 4096`. Most are built
/// on `ModelConfig::openai`; `zai`'s own constructor sets the same two numbers
/// for every id (found by the population guard below, not by hand).
/// Deliberately NOT here, named rather than overlooked: `ollama`, `custom` and
/// unknown providers (`ModelConfig::local`, same numbers, but the user runs
/// that server and sets its limits there); `minimax` (a different guess, 1M
/// window with `max_tokens = 4096`). `deepseek`, `google`, `bedrock` and
/// `anthropic` have their own presets/defaults. The test
/// `openai_base_fallback_providers_match_create_model_config` fails if a
/// provider lands on the base guess without being listed here.
pub(crate) const OPENAI_BASE_FALLBACK_PROVIDERS: &[&str] = &[
    "openai",
    "openrouter",
    "xai",
    "groq",
    "mistral",
    "cerebras",
    "github",
    "zai",
];

/// Bytes of a model id shown in [`unknown_model_limits_note`] before the cut.
const UNKNOWN_MODEL_ID_MAX_BYTES: usize = 200;

/// The one-line warning for an OpenAI-compatible model id that has no preset
/// and therefore runs on yoagent's guessed limits, or `None` when there is
/// nothing to say.
///
/// `window` and `max_tokens` are the **resolved** config's values (whatever
/// the base config actually set), never typed copies. Returns `None` when:
/// the user set `max_tokens` themselves (`user_max_tokens`: they chose, do not
/// nag); the provider is not one of [`OPENAI_BASE_FALLBACK_PROVIDERS`]; or the
/// id has a preset (`openai_preset` on provider `openai`). When the user set
/// `context_window` but not `max_tokens`, only the output cap is reported,
/// because claiming a guessed window would be false.
///
/// The id is user/config-authored, so it is sanitized and capped on a char
/// boundary with an ASCII marker. Glyph-free (pure ASCII apart from the id's
/// own escaped text) under `plain`.
pub(crate) fn unknown_model_limits_note(
    provider: &str,
    model: &str,
    window: u32,
    max_tokens: u32,
    user_max_tokens: Option<u32>,
    user_context_window: Option<u32>,
    plain: bool,
) -> Option<String> {
    if user_max_tokens.is_some() || !OPENAI_BASE_FALLBACK_PROVIDERS.contains(&provider) {
        return None;
    }
    if provider == "openai" && openai_preset(model).is_some() {
        return None;
    }
    let shown = crate::cli::sanitize_for_display(model);
    let shown = if shown.len() > UNKNOWN_MODEL_ID_MAX_BYTES {
        let head = crate::format::safe_truncate(&shown, UNKNOWN_MODEL_ID_MAX_BYTES);
        format!("{head}...[{} bytes elided]", shown.len() - head.len())
    } else {
        shown
    };
    let marker = if plain { "warning: " } else { "⚠ " };
    let assumed = match user_context_window {
        Some(_) => format!("max_tokens = {max_tokens}"),
        None => format!("a {window}-token context window and max_tokens = {max_tokens}"),
    };
    Some(format!(
        "{marker}no preset is known for {provider} model '{shown}', so yoyo is using guessed \
limits: {assumed}. Every reply (reasoning included) is capped at {max_tokens} output tokens. \
If the model allows more, set max_tokens and context_window in .yoyo.toml \
(or pass --max-tokens / --context-window)."
    ))
}

/// Keys `(provider, model)` already warned about by
/// [`unknown_model_limits_note`], so a rebuild (`/model`, `/think`, fallback)
/// does not repeat the line. Once per pair per process.
pub(crate) static WARNED_UNKNOWN_MODEL_LIMITS: std::sync::Mutex<Vec<(String, String)>> =
    std::sync::Mutex::new(Vec::new());

/// Record `(provider, model)` in `store`; `true` only the first time. Takes
/// the store as a parameter so tests drive a local `Mutex`.
pub(crate) fn first_unknown_limits_warning(
    store: &std::sync::Mutex<Vec<(String, String)>>,
    provider: &str,
    model: &str,
) -> bool {
    let mut seen = crate::sync_util::lock_or_recover(store);
    if seen.iter().any(|(p, m)| p == provider && m == model) {
        return false;
    }
    seen.push((provider.to_string(), model.to_string()));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_builder::create_model_config;

    /// The base config's real numbers, read from yoagent, never typed copies.
    fn base() -> (u32, u32) {
        let c = yoagent::provider::ModelConfig::openai("x", "x");
        (c.context_window, c.max_tokens)
    }

    /// The note exactly as `build_agent` would compute it for a given id on a
    /// given provider with no user overrides: limits come from the resolved
    /// `create_model_config`, the same producer the call site reads.
    fn note_for(provider: &str, model: &str, plain: bool) -> Option<String> {
        let c = create_model_config(provider, model, None);
        unknown_model_limits_note(
            provider,
            model,
            c.context_window,
            c.max_tokens,
            None,
            None,
            plain,
        )
    }

    #[test]
    fn unknown_openai_id_warns_with_real_base_limits_and_override_keys() {
        let (window, max) = base();
        // Premise, so a yoagent change to the base guess is noticed here.
        assert_eq!((window, max), (128_000, 4096), "base guess changed");
        let out = note_for("openai", "gpt-6.1-sol", true).expect("gpt-6.1-sol must warn");
        assert_eq!(
            out,
            format!(
                "warning: no preset is known for openai model 'gpt-6.1-sol', so yoyo is using \
guessed limits: a {window}-token context window and max_tokens = {max}. Every reply (reasoning \
included) is capped at {max} output tokens. If the model allows more, set max_tokens and \
context_window in .yoyo.toml (or pass --max-tokens / --context-window)."
            )
        );
    }

    #[test]
    fn day_223_preset_ids_stay_silent_near_miss() {
        for id in ["gpt-6-astra", "gpt-6-sol", "gpt-6-luna"] {
            assert_eq!(note_for("openai", id, false), None, "{id} has a preset");
        }
        // One character off a preset id: no preset, so it must warn.
        assert!(note_for("openai", "gpt-6.1-sol", false).is_some());
        assert!(note_for("openai", "gpt-6-sol-2", false).is_some());
    }

    #[test]
    fn explicit_max_tokens_silences_it() {
        let (window, max) = base();
        assert_eq!(
            unknown_model_limits_note("openai", "gpt-6.1-sol", window, max, Some(4096), None, true),
            None
        );
    }

    #[test]
    fn explicit_context_window_drops_the_window_claim_only() {
        let (window, max) = base();
        let out = unknown_model_limits_note(
            "openai",
            "gpt-6.1-sol",
            window,
            max,
            None,
            Some(400_000),
            true,
        )
        .expect("output cap still applies");
        assert!(!out.contains(&window.to_string()), "{out}");
        assert!(out.contains(&format!("max_tokens = {max}")), "{out}");
    }

    #[test]
    fn providers_off_the_base_fallback_stay_silent() {
        for p in [
            "anthropic",
            "google",
            "deepseek",
            "ollama",
            "custom",
            "bedrock",
        ] {
            assert_eq!(note_for(p, "some-unknown-model", true), None, "{p}");
        }
        // Another arm built on the base guess does warn, naming its provider.
        let out = note_for("openrouter", "vendor/some-model", true).unwrap();
        assert!(
            out.contains("openrouter model 'vendor/some-model'"),
            "{out}"
        );
    }

    /// Population guard (Day 222: a hand list goes stale with the same edit
    /// that adds an arm). Every known provider whose `create_model_config`
    /// lands an unknown id on the base guess must be listed; `ollama` and
    /// `custom` reach the same numbers through `ModelConfig::local` and are
    /// excluded on purpose (see the const's doc comment).
    #[test]
    fn openai_base_fallback_providers_match_create_model_config() {
        let (window, max) = base();
        for p in crate::providers::KNOWN_PROVIDERS {
            let c = create_model_config(p, "no-such-model-xyz", None);
            let on_base_guess = c.context_window == window
                && c.max_tokens == max
                && c.api == yoagent::provider::ApiProtocol::OpenAiCompletions;
            let excluded = *p == "ollama" || *p == "custom";
            assert_eq!(
                OPENAI_BASE_FALLBACK_PROVIDERS.contains(p),
                on_base_guess && !excluded,
                "{p}: on_base_guess={on_base_guess}; update OPENAI_BASE_FALLBACK_PROVIDERS"
            );
        }
    }

    #[test]
    fn plain_mode_is_ascii_and_fancy_mode_has_marker() {
        let plain = note_for("openai", "gpt-6.1-sol", true).unwrap();
        assert!(plain.is_ascii(), "{plain}");
        let fancy = note_for("openai", "gpt-6.1-sol", false).unwrap();
        assert!(fancy.starts_with("⚠ "), "{fancy}");
    }

    #[test]
    fn hostile_id_is_sanitized_and_capped_on_a_char_boundary() {
        let hostile = "gpt-\x1b[2Jevil";
        assert!(hostile.as_bytes().contains(&0x1b), "fixture premise");
        let out = note_for("openai", hostile, true).unwrap();
        assert!(!out.as_bytes().contains(&0x1b), "{out:?}");

        let long = "é".repeat(300); // 600 bytes, 2-byte chars
        let out = note_for("openai", &long, true).unwrap();
        assert!(out.contains("bytes elided]"), "{out}");
        assert!(out.len() < long.len(), "id was capped");
    }

    #[test]
    fn warns_once_per_provider_model_pair() {
        let store = std::sync::Mutex::new(Vec::new());
        assert!(first_unknown_limits_warning(&store, "openai", "a"));
        assert!(!first_unknown_limits_warning(&store, "openai", "a"));
        assert!(first_unknown_limits_warning(&store, "openai", "b"));
        assert!(first_unknown_limits_warning(&store, "groq", "a"));
    }

    /// Weak source-level guard: the call site is not gated on `is_quiet()`.
    /// Proves the decision is present in the source, not that it fires.
    #[test]
    fn call_site_is_not_quiet_gated() {
        let src = include_str!("agent_builder.rs");
        let needle = ["model_limits_note::", "unknown_model_limits_note("].concat();
        let at = src.find(&needle).expect("call site present");
        let window = &src[src[..at].rfind("let output_ceiling").unwrap()..at + 900];
        assert!(!window.contains(&["is_", "quiet()"].concat()), "{window}");
    }
}
