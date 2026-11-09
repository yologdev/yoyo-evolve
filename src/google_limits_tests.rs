//! #1157 -> #1168: every offered Gemini id runs on its yoagent preset's
//! limits, and `/model` info reads the same preset the run path does.
//! Until Day 254 the last id (gemini-2.5-flash-lite) came from yoyo's own
//! Google-page table; yoagent 0.27.7 ships its preset, so the table is gone
//! and these tests pin the preset instead (#1164 upstream).

use super::*;
use crate::agent_builder::{create_model_config, declared_output_ceiling, AgentConfig};
use crate::cli;
use yoagent::provider::ModelConfig;
use yoagent::ThinkingLevel;

fn google_agent_config(model: &str, max_tokens: Option<u32>) -> AgentConfig {
    AgentConfig {
        model: model.to_string(),
        api_key: "test-key".to_string(),
        provider: "google".to_string(),
        base_url: None,
        skills: yoagent::skills::SkillSet::empty(),
        system_prompt: "Test.".to_string(),
        thinking: ThinkingLevel::Off,
        max_tokens,
        temperature: None,
        max_turns: None,
        auto_approve: true,
        auto_commit: false,
        permissions: cli::PermissionConfig::default(),
        dir_restrictions: cli::DirectoryRestrictions::default(),
        context_strategy: cli::ContextStrategy::default(),
        context_window: None,
        shell_hooks: vec![],
        fallback_provider: None,
        fallback_model: None,
        auto_watch: false,
        allowed_tools: vec![],
        disallowed_tools: vec![],
        no_tools: true,
        lite: false,
        bash_cwd: None,
    }
}

/// Every id yoyo offers for google has real limits from a yoagent preset:
/// `ModelConfig::preset` resolves it, and its window / output differ from
/// `ModelConfig::google`'s 1,000,000 / 8192 guess.
#[test]
fn every_offered_gemini_id_has_a_yoagent_preset_with_real_limits() {
    let guess = ModelConfig::google("zz", "zz");
    for id in known_models_for_provider("google") {
        let preset = ModelConfig::preset("google", id)
            .unwrap_or_else(|| panic!("google offers {id}: no yoagent preset"));
        assert_ne!(
            (preset.context_window, preset.max_tokens),
            (guess.context_window, guess.max_tokens),
            "{id}: the preset is ModelConfig::google's guess, not real limits"
        );
    }
}

/// The run path, the #943 ceiling and `/model` info all read the preset.
#[test]
fn presets_reach_the_run_path_and_model_info() {
    let guess = ModelConfig::google("zz", "zz");
    for id in known_models_for_provider("google") {
        let preset = ModelConfig::preset("google", id).expect(id);
        let (window, out) = (preset.context_window, preset.max_tokens);
        let c = create_model_config("google", id, None);
        assert_eq!((c.context_window, c.max_tokens), (window, out), "{id}");
        assert_ne!(
            (c.context_window, c.max_tokens),
            (guess.context_window, guess.max_tokens),
            "{id} still runs on ModelConfig::google's guess"
        );
        assert_eq!(declared_output_ceiling("google", id, 0), Some(out), "{id}");
        assert_eq!(
            crate::commands_info::model_context_window(id),
            Some(u64::from(window)),
            "/model info and the run path disagree on {id}"
        );
        assert!(
            !crate::model_limits_note::runs_on_guessed_limits("google", id),
            "{id} has a preset; the guessed-limits warning must stay quiet"
        );
    }
    // The id the deleted table carried: Google's page says 1,048,576 / 65,536.
    let lite = create_model_config("google", "gemini-2.5-flash-lite", None);
    assert_eq!((lite.context_window, lite.max_tokens), (1_048_576, 65_536));
    // Near miss: an id Google never listed still guesses, and says so.
    let zzz = create_model_config("google", "gemini-zzz", None);
    assert_eq!(
        (zzz.context_window, zzz.max_tokens),
        (guess.context_window, guess.max_tokens)
    );
    assert!(crate::model_limits_note::runs_on_guessed_limits(
        "google",
        "gemini-zzz"
    ));
    assert_eq!(
        crate::commands_info::model_context_window("gemini-zzz"),
        None
    );
}

/// #1168: `/cost` prices gemini-2.5-flash-lite through yoagent's bundled
/// price row (the deleted table never carried a price).
#[test]
fn flash_lite_has_a_bundled_price() {
    assert!(
        yoagent::provider::PriceTable::builtin()
            .iter()
            .any(|(p, m, _)| p == "google" && m == "gemini-2.5-flash-lite"),
        "no bundled google price row for gemini-2.5-flash-lite"
    );
    assert!(
        crate::format::estimate_cost(
            &yoagent::Usage {
                input: 1_000_000,
                output: 1_000_000,
                ..Default::default()
            },
            "gemini-2.5-flash-lite"
        )
        .is_some_and(|c| c > 0.0),
        "/cost has no price for gemini-2.5-flash-lite"
    );
}

/// The preset sets the config's default; a user's `max_tokens` still goes on
/// the agent (the only value GoogleProvider sends as maxOutputTokens).
#[test]
fn user_max_tokens_still_wins_over_the_preset() {
    let agent = google_agent_config("gemini-2.5-flash-lite", Some(1234)).build_agent();
    assert_eq!(agent.max_tokens, Some(1234));
    let agent = google_agent_config("gemini-2.5-flash-lite", None).build_agent();
    assert_eq!(agent.max_tokens, None, "no user value, nothing overrides");
}
