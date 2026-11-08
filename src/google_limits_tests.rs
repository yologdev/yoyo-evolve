//! #1157: offered Gemini ids with no yoagent preset run on the limits
//! Google publishes per id ([`GOOGLE_PUBLISHED_LIMITS`]), and `/model` info
//! reads the same rows the run path does.

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

/// Every id yoyo offers for google has real limits: a yoagent preset or a
/// Google-page row, never both (a row beside a preset would shadow it).
#[test]
fn every_offered_gemini_id_has_a_preset_or_a_published_limits_row() {
    for id in known_models_for_provider("google") {
        let preset = ModelConfig::preset("google", id).is_some();
        let row = google_published_limits(id).is_some();
        assert!(
            preset || row,
            "google offers {id}: no yoagent preset and no GOOGLE_PUBLISHED_LIMITS row"
        );
        assert!(
            !(preset && row),
            "{id} is a yoagent preset now; drop its GOOGLE_PUBLISHED_LIMITS row"
        );
    }
    for (id, _, _, url) in GOOGLE_PUBLISHED_LIMITS {
        assert!(
            known_models_for_provider("google").contains(id),
            "{id} row is unused"
        );
        assert!(
            url.starts_with("https://ai.google.dev/gemini-api/docs/models/") && url.ends_with(id),
            "{id}: the source must be the id's own Google page, got {url}"
        );
    }
}

/// The run path, the #943 ceiling and `/model` info all read the row.
#[test]
fn published_rows_reach_the_run_path_and_model_info() {
    let guess = ModelConfig::google("zz", "zz");
    for &(id, window, out, _) in GOOGLE_PUBLISHED_LIMITS {
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
            "{id} has published limits; the guessed-limits warning must stay quiet"
        );
    }
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

/// The row sets the config's default; a user's `max_tokens` still goes on
/// the agent (the only value GoogleProvider sends as maxOutputTokens).
#[test]
fn user_max_tokens_still_wins_over_a_published_row() {
    let agent = google_agent_config("gemini-2.5-pro", Some(1234)).build_agent();
    assert_eq!(agent.max_tokens, Some(1234));
    let agent = google_agent_config("gemini-2.5-pro", None).build_agent();
    assert_eq!(agent.max_tokens, None, "no user value, nothing overrides");
}
