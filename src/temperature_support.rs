//! Which models still accept a sampling `temperature` (#1145, #1122).
//!
//! Anthropic removed `temperature` (with `top_p` / `top_k`) for Claude 4.7
//! and later: Anthropic's own API answers HTTP 400 "`temperature` is
//! deprecated for this model" to every request that carries one (measured
//! with a real key, #1145). Bedrock's answer is uncaptured; OpenRouter's API
//! reference says it ignores parameters a model doesn't support (#1122). Since yoagent 0.27.3 the
//! rule lives in yoagent: [`ModelConfig::accepts_sampling_params`] is
//! inferred off for those ids by its `anthropic` constructor and presets, and
//! the loop then leaves the temperature out. yoyo no longer parses Claude
//! versions itself (#1145's copy covered only `anthropic` and `bedrock`); it
//! reads the flag off the config each agent is built from, and adds what
//! yoagent doesn't: a note on stderr, once per process, when the user set a
//! temperature that won't be sent (yoagent only logs a `tracing` warning).
//!
//! Where the flag comes from in yoyo (`create_model_config`): the
//! `anthropic` arm takes yoagent's own config; the `bedrock` and
//! `openrouter` arms are built from `custom()` / `openai()`, which leave the
//! flag on, so they copy it from yoagent's config for the Anthropic id behind
//! the id. `ollama`, `custom` and unknown OpenAI-compatible providers keep
//! yoagent's default (on): yoagent infers nothing for them and exports no
//! id predicate (`takes_sampling_params` is `pub(crate)`), so a Claude id
//! behind such a gateway still gets the temperature (asked upstream on #1122).

use std::sync::atomic::{AtomicBool, Ordering};

/// The stderr note printed when a configured temperature is dropped.
pub fn temperature_dropped_note(model: &str) -> String {
    format!(
        "note: temperature was not sent to {model}: Anthropic removed temperature for \
         Claude 4.7 and later, and Anthropic's API rejects every request that sets it. \
         Remove --temperature / `temperature` from your config to silence this note."
    )
}

static NOTE_PRINTED: AtomicBool = AtomicBool::new(false);

/// The temperature an agent for `model` should carry, given its built
/// config's `accepts_sampling_params`: the configured one, or `None` when the
/// model rejects it, in which case the note is printed to stderr (once per
/// process, so `/model` switches and side agents don't repeat it).
pub fn effective_temperature(
    model: &str,
    accepts_sampling_params: bool,
    configured: Option<f32>,
) -> Option<f32> {
    let temp = configured?;
    if accepts_sampling_params {
        return Some(temp);
    }
    if !NOTE_PRINTED.swap(true, Ordering::Relaxed) {
        eprintln!("{}", temperature_dropped_note(model));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;
    use crate::AgentConfig;
    use yoagent::ThinkingLevel;

    /// The flag on the config yoyo actually builds (`create_model_config`),
    /// per provider x id: yoagent's rule, reached through each arm.
    fn accepts(provider: &str, model: &str) -> bool {
        crate::agent_builder::create_model_config(provider, model, None).accepts_sampling_params
    }

    #[test]
    fn built_config_sampling_flag_table() {
        let rejecting = [
            ("anthropic", "claude-opus-5-5"),
            ("anthropic", "claude-haiku-5-5"),
            ("anthropic", "claude-sonnet-5-5"),
            ("anthropic", "claude-opus-4-7"),
            ("anthropic", "claude-opus-4-8"),
            ("anthropic", "claude-opus-5"),
            ("anthropic", "claude-fable-5-1"),
            ("anthropic", "claude-opus-4-7-20260101"),
            ("anthropic", "claude-opus-5-5[1m]"),
            ("bedrock", "us.anthropic.claude-opus-4-8"),
            ("bedrock", "global.anthropic.claude-sonnet-5-5"),
            ("bedrock", "anthropic.claude-haiku-5-5"),
            // #1122: these sent a temperature before. No request was made, so
            // whether OpenRouter answered 400 is unmeasured; its API reference
            // says it ignores parameters a model doesn't support, and
            // /api/v1/models omits `temperature` for these ids. Either way it
            // was never applied, and now the user gets the stderr note.
            ("openrouter", "anthropic/claude-opus-5.5"),
            ("openrouter", "anthropic/claude-opus-5-5"),
            ("openrouter", "anthropic/claude-sonnet-5"),
        ];
        for (provider, model) in rejecting {
            assert!(
                !accepts(provider, model),
                "{provider}/{model} should reject temperature"
            );
        }
        let accepting = [
            ("anthropic", "claude-haiku-4-5"),
            ("anthropic", "claude-haiku-4-5-20251001"),
            ("anthropic", "claude-sonnet-4-6"),
            ("anthropic", "claude-opus-4-6"),
            ("anthropic", "claude-opus-4-20250514"),
            ("anthropic", "claude-3-5-haiku-20241022"),
            ("anthropic", "claude-3-7-sonnet-latest"),
            ("anthropic", "not-a-claude-id"),
            ("anthropic", "claude-"),
            ("bedrock", "us.anthropic.claude-sonnet-4-6"),
            ("bedrock", "anthropic.claude-haiku-4-5-20251001-v1:0"),
            // Not in yoagent's Bedrock map: we can't tell, so we don't touch it.
            ("bedrock", "us.anthropic.claude-unknown-9-9"),
            ("openrouter", "anthropic/claude-sonnet-4.6"),
            ("openrouter", "google/gemini-3.1-pro-preview"),
            ("openrouter", "openai/gpt-5"),
            ("openai", "gpt-5"),
            ("openai", "claude-opus-5-5"),
            // Known limit (#1122): yoagent infers nothing for a gateway and
            // exports no id predicate, so these keep the flag on.
            ("custom", "claude-opus-5-5"),
            ("ollama", "llama3"),
        ];
        for (provider, model) in accepting {
            assert!(
                accepts(provider, model),
                "{provider}/{model} should keep temperature"
            );
        }
    }

    #[test]
    fn dropped_note_names_model_and_reason() {
        let note = temperature_dropped_note("claude-opus-5-5");
        assert!(note.contains("claude-opus-5-5"), "{note}");
        assert!(note.contains("temperature was not sent"), "{note}");
        assert!(note.contains("Claude 4.7 and later"), "{note}");
        // #1122: the note is also printed for Bedrock and OpenRouter, where a
        // 400 is not measured, so the 400 claim must name whose API it is.
        assert!(note.contains("Anthropic's API rejects"), "{note}");
        assert!(!note.contains("the API rejects"), "{note}");
    }

    fn config(provider: &str, model: &str, temperature: Option<f32>) -> AgentConfig {
        AgentConfig {
            model: model.to_string(),
            api_key: "test-key".to_string(),
            provider: provider.to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature,
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

    /// The decision, through every real builder that sets a temperature.
    #[test]
    fn builders_drop_temperature_only_for_rejecting_models() {
        let opus = config("anthropic", "claude-opus-5-5", Some(0.5));
        assert_eq!(opus.build_agent().temperature, None);
        assert_eq!(opus.build_side_agent().temperature, None);
        assert_eq!(
            opus.build_architect_agent("claude-sonnet-5-5").temperature,
            None
        );
        assert_eq!(
            opus.build_editor_agent("claude-haiku-5-5").temperature,
            None
        );

        let haiku = config("anthropic", "claude-haiku-4-5", Some(0.5));
        assert_eq!(haiku.build_agent().temperature, Some(0.5));
        assert_eq!(haiku.build_side_agent().temperature, Some(0.5));
        // The architect runs on its own model: judged by that one, not self.model.
        assert_eq!(
            haiku.build_architect_agent("claude-sonnet-4-6").temperature,
            Some(0.5)
        );
        assert_eq!(
            haiku.build_architect_agent("claude-opus-5-5").temperature,
            None
        );

        let bedrock = config("bedrock", "us.anthropic.claude-opus-5-5", Some(0.5));
        assert_eq!(bedrock.build_agent().temperature, None);

        let openrouter = config("openrouter", "anthropic/claude-opus-5.5", Some(0.5));
        assert_eq!(openrouter.build_agent().temperature, None);
        assert_eq!(openrouter.build_side_agent().temperature, None);
        let openrouter_gemini = config("openrouter", "google/gemini-3.1-pro-preview", Some(0.5));
        assert_eq!(openrouter_gemini.build_agent().temperature, Some(0.5));
        let bedrock_old = config("bedrock", "us.anthropic.claude-sonnet-4-6", Some(0.5));
        assert_eq!(bedrock_old.build_agent().temperature, Some(0.5));

        // The REPL's `/model` path: the flag comes from the rebuilt config.
        let mut switching = config("openrouter", "anthropic/claude-sonnet-4.6", Some(0.5));
        let mut agent = switching.build_agent();
        assert_eq!(agent.temperature, Some(0.5));
        switching.model = "anthropic/claude-opus-5.5".to_string();
        switching.rebuild_preserving_messages(&mut agent);
        assert_eq!(agent.temperature, None);

        let unset_openrouter = config("openrouter", "anthropic/claude-opus-5.5", None);
        assert_eq!(unset_openrouter.build_agent().temperature, None);

        let unset = config("anthropic", "claude-opus-5-5", None);
        assert_eq!(unset.build_agent().temperature, None);
    }
}
