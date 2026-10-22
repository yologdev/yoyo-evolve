//! #1037: yoyo's resolver checked against every yoagent preset, driven by
//! `ModelConfig::presets()` so a preset yoagent adds is in this test's
//! population the moment Cargo.lock moves (Day 222: never let the check's
//! population be a hand list). Kept out of agent_builder.rs for its size gate.

use crate::agent_builder::{create_model_config, vendor_preset, vendor_preset_by_id};
use crate::cli::unknown_model_warning;
use crate::format::{RESET, YELLOW};
use crate::model_limits_note::unknown_model_limits_note;
use crate::providers::model_is_known_for_provider;
use yoagent::provider::ModelConfig;

/// Presets yoyo does NOT resolve to their own limits yet: (provider, id, why).
/// The test below FAILS if a listed id starts resolving to its preset, so this
/// list can only shrink. Anthropic's nine are absent on purpose: they already
/// resolve to identical numbers via `anthropic_preset` (whose move onto
/// `ModelConfig::preset` waits on the `-YYYYMMDD` normalization, #1037).
const PRESETS_NOT_YET_ROUTED: &[(&str, &str, &str)] = &[
    (
        "openai",
        "gpt-5.5",
        "its preset's `api` is the Responses wire decision, #1060 item 1",
    ),
    (
        "qwen",
        "qwen3.8-max",
        "yoyo has no `qwen` provider arm (unknown provider -> localhost)",
    ),
    (
        "qwen",
        "qwen3.8-flash",
        "yoyo has no `qwen` provider arm (unknown provider -> localhost)",
    ),
];

#[test]
fn every_yoagent_preset_resolves_to_its_own_limits_or_is_registered() {
    let mut routed = 0;
    for preset in ModelConfig::presets() {
        let (p, id) = (preset.provider.as_str(), preset.id.as_str());
        let ours = create_model_config(p, id, None);
        let matches =
            ours.context_window == preset.context_window && ours.max_tokens == preset.max_tokens;
        println!(
            "{p}/{id}: yoyo {}/{} preset {}/{}",
            ours.context_window, ours.max_tokens, preset.context_window, preset.max_tokens
        );
        match PRESETS_NOT_YET_ROUTED
            .iter()
            .find(|(lp, lid, _)| *lp == p && *lid == id)
        {
            Some((_, _, why)) => assert!(
                !matches,
                "{p}/{id} now resolves to its preset ({}/{}); remove it from \
                 PRESETS_NOT_YET_ROUTED (was: {why})",
                preset.context_window, preset.max_tokens
            ),
            None => {
                assert!(
                    matches,
                    "{p}/{id}: yoyo resolves {}/{}, yoagent's preset says {}/{}. Route it \
                     (vendor_preset / VENDOR_PRESET_PROVIDERS) or register it with a reason",
                    ours.context_window, ours.max_tokens, preset.context_window, preset.max_tokens
                );
                routed += 1;
            }
        }
    }
    // Anti-vacuous: the population is non-trivial and zai's two are in it.
    assert!(routed >= 17, "only {routed} presets routed");
    assert!(ModelConfig::preset("zai", "glm-5.3").is_some());
    // Every registered row names a preset that still exists.
    for (p, id, _) in PRESETS_NOT_YET_ROUTED {
        assert!(ModelConfig::preset(p, id).is_some(), "{p}/{id} is gone");
    }
}

#[test]
fn every_yoagent_preset_id_is_known_for_its_provider() {
    for preset in ModelConfig::presets() {
        let (p, id) = (preset.provider.as_str(), preset.id.as_str());
        assert!(model_is_known_for_provider(p, id), "{p}/{id}");
        assert_eq!(unknown_model_warning(p, id), None, "{p}/{id}");
    }
}

#[test]
fn glm_5_3_keeps_the_zai_wire_and_drops_the_guessed_limits_note() {
    for id in ["glm-5.3", "glm-5.3-flash"] {
        let c = create_model_config("zai", id, None);
        assert_eq!((c.context_window, c.max_tokens), (1_000_000, 128_000));
        assert!(c.cost.is_some() && c.reasoning, "{id}");
        assert_eq!(c.provider, "zai");
        assert_eq!(c.base_url, "https://api.z.ai/api/paas/v4");
        let w = (c.context_window, c.max_tokens);
        assert_eq!(
            unknown_model_limits_note("zai", id, w.0, w.1, None, None, true),
            None
        );
        // `/cost` and the `/model` window read the same preset by id.
        assert_eq!(
            vendor_preset_by_id(id).map(|p| p.context_window),
            Some(1_000_000)
        );
    }
    // A user base URL still wins over the preset's.
    let c = create_model_config("zai", "glm-5.3", Some("http://proxy.local/v4"));
    assert_eq!(c.base_url, "http://proxy.local/v4");
    assert_eq!(c.max_tokens, 128_000);
}

#[test]
fn near_miss_ids_and_other_providers_keep_the_generic_guess() {
    // One character off a preset: generic config AND the honest warnings.
    for id in ["glm-5.3-flashx", "glm-5.4"] {
        let c = create_model_config("zai", id, None);
        assert_eq!((c.context_window, c.max_tokens), (128_000, 4096), "{id}");
        assert!(vendor_preset("zai", id).is_none(), "{id}");
        assert!(
            unknown_model_limits_note("zai", id, 128_000, 4096, None, None, true).is_some(),
            "{id}"
        );
    }
    assert_eq!(
        unknown_model_warning("zai", "glm-5.4"),
        Some(format!(
            "{YELLOW}warning:{RESET} Unknown model 'glm-5.4' for provider 'zai'. \
             Known models: glm-4-plus, glm-4-air, glm-4-flash, glm-5.3, glm-5.3-flash. \
             Proceeding anyway (custom models are valid)."
        ))
    );
    // A preset id on a provider that is not its vendor gets no preset.
    for (p, id) in [
        ("openrouter", "glm-5.3"),
        ("groq", "glm-5.3"),
        ("xai", "glm-5.3"),
    ] {
        assert!(vendor_preset(p, id).is_none(), "{p}/{id}");
        let c = create_model_config(p, id, None);
        assert_eq!(
            (c.context_window, c.max_tokens),
            (128_000, 4096),
            "{p}/{id}"
        );
    }
    // vendor_preset never answers for the two providers with their own functions.
    assert!(vendor_preset("openai", "gpt-5.5").is_none());
    assert!(vendor_preset("anthropic", "claude-opus-5-5").is_none());
}
