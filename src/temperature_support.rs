//! Which models still accept a sampling `temperature` (#1145).
//!
//! Anthropic removed `temperature` (with `top_p` / `top_k`) for Claude 4.7
//! and later: those models answer HTTP 400 "`temperature` is deprecated for
//! this model" to every request that carries one. yoagent 0.27.2 forwards
//! `config.temperature` unconditionally and has no per-model flag for it
//! (asked upstream in #1146), so a user with `--temperature` or
//! `temperature = ...` in `.yoyo.toml` on the default model failed 100% of
//! calls. yoyo now drops the knob for those models and says so on stderr,
//! once per process.
//!
//! Scope: the `anthropic` provider (bare ids) and `bedrock` ids that
//! yoagent's own [`ModelConfig::bedrock_anthropic_id`] maps to an Anthropic
//! id. A Bedrock id that mapper doesn't know is left alone (we can't tell
//! which model it is). Other providers are untouched; OpenAI's reasoning
//! models are #1060.

use std::sync::atomic::{AtomicBool, Ordering};
use yoagent::provider::ModelConfig;

/// First Claude version whose API rejects `temperature`.
const FIRST_REJECTING_VERSION: (u32, u32) = (4, 7);

/// `(major, minor)` of a bare Anthropic Claude id, parsed from its tokens:
/// `claude-opus-5-5` → (5, 5), `claude-opus-5` → (5, 0),
/// `claude-haiku-4-5-20251001` → (4, 5), `claude-opus-4-20250514` → (4, 0),
/// `claude-3-5-haiku-20241022` → (3, 5). A date (more than two digits) is
/// never a minor version. `None` when no version token is found.
fn claude_version(model: &str) -> Option<(u32, u32)> {
    let rest = model.strip_prefix("claude-")?;
    let mut tokens = rest.split('-');
    let major = tokens.by_ref().find_map(version_number)?;
    let minor = tokens.next().and_then(version_number).unwrap_or(0);
    Some((major, minor))
}

/// The leading digits of `token` as a version number, if there are one or
/// two of them (so `5`, `7[1m]` parse; `20251001` and `opus` don't).
fn version_number(token: &str) -> Option<u32> {
    let digits: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    digits.parse().ok()
}

/// True when `model` on `provider` is a Claude model at version 4.7 or
/// later, whose API rejects a `temperature` field.
pub fn rejects_temperature(provider: &str, model: &str) -> bool {
    let anthropic_id = match provider {
        "anthropic" => model.to_string(),
        "bedrock" => match ModelConfig::bedrock_anthropic_id(model) {
            Some(id) => id,
            None => return false,
        },
        _ => return false,
    };
    claude_version(&anthropic_id).is_some_and(|v| v >= FIRST_REJECTING_VERSION)
}

/// The stderr note printed when a configured temperature is dropped.
pub fn temperature_dropped_note(model: &str) -> String {
    format!(
        "note: temperature was not sent to {model}: Anthropic removed temperature for \
         Claude 4.7 and later, and the API rejects every request that sets it. \
         Remove --temperature / `temperature` from your config to silence this note."
    )
}

static NOTE_PRINTED: AtomicBool = AtomicBool::new(false);

/// The temperature an agent for `model` on `provider` should carry: the
/// configured one, or `None` when the model rejects it, in which case the
/// note is printed to stderr (once per process, so `/model` switches and
/// side agents don't repeat it).
pub fn effective_temperature(provider: &str, model: &str, configured: Option<f32>) -> Option<f32> {
    let temp = configured?;
    if !rejects_temperature(provider, model) {
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

    #[test]
    fn rejects_temperature_table() {
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
        ];
        for (provider, model) in rejecting {
            assert!(
                rejects_temperature(provider, model),
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
            ("openai", "gpt-5"),
            ("openai", "claude-opus-5-5"),
            ("openrouter", "anthropic/claude-opus-5-5"),
        ];
        for (provider, model) in accepting {
            assert!(
                !rejects_temperature(provider, model),
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

        let unset = config("anthropic", "claude-opus-5-5", None);
        assert_eq!(unset.build_agent().temperature, None);
    }
}
