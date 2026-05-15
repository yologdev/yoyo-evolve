//! Provider constants and utilities — known providers, API key env vars, default models.

/// Known provider names for the --provider flag.
pub const KNOWN_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "google",
    "openrouter",
    "ollama",
    "xai",
    "groq",
    "deepseek",
    "mistral",
    "cerebras",
    "zai",
    "minimax",
    "bedrock",
    "custom",
];

/// Get the environment variable name that holds the API key for a provider.
pub fn provider_api_key_env(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("OPENAI_API_KEY"),
        "google" => Some("GOOGLE_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        "xai" => Some("XAI_API_KEY"),
        "deepseek" => Some("DEEPSEEK_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        "mistral" => Some("MISTRAL_API_KEY"),
        "cerebras" => Some("CEREBRAS_API_KEY"),
        "zai" => Some("ZAI_API_KEY"),
        "minimax" => Some("MINIMAX_API_KEY"),
        "bedrock" => Some("AWS_ACCESS_KEY_ID"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        _ => None,
    }
}

/// Get well-known model names for a provider (for diagnostic suggestions).
/// Returns a slice of commonly-used model identifiers.
pub fn known_models_for_provider(provider: &str) -> &'static [&'static str] {
    match provider {
        "anthropic" => &[
            "claude-opus-4-7",
            "claude-opus-4-6",
            "claude-sonnet-4-7",
            "claude-sonnet-4-6",
            "claude-sonnet-4-5",
            "claude-sonnet-4-20250514",
            "claude-haiku-4-5",
            "claude-haiku-4-5-20251001",
        ],
        "openai" => &[
            "gpt-5",
            "gpt-5-mini",
            "gpt-5.5",
            "gpt-5.5-mini",
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "codex-mini",
            "o3",
            "o3-mini",
            "o4-mini",
            "o4-mini-high",
        ],
        "google" => &[
            "gemini-3.0-pro",
            "gemini-3.0-flash",
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
            "gemini-2.0-flash",
        ],
        "groq" => &[
            "llama-4-maverick-17b",
            "llama-4-scout-17b",
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
            "mixtral-8x7b-32768",
        ],
        "xai" => &["grok-4", "grok-4-mini", "grok-3", "grok-3-mini", "grok-2"],
        "deepseek" => &["deepseek-chat", "deepseek-reasoner", "deepseek-r2"],
        "mistral" => &[
            "mistral-large-latest",
            "mistral-small-latest",
            "codestral-latest",
        ],
        "cerebras" => &["llama-3.3-70b"],
        "zai" => &["glm-4-plus", "glm-4-air", "glm-4-flash"],
        "minimax" => &[
            "MiniMax-M2.7",
            "MiniMax-M2.7-highspeed",
            "MiniMax-M2.5",
            "MiniMax-M2.5-highspeed",
            "MiniMax-M1",
            "MiniMax-M1-40k",
        ],
        "bedrock" => &[
            "anthropic.claude-opus-4-7",
            "anthropic.claude-sonnet-4-7",
            "anthropic.claude-sonnet-4-6",
            "anthropic.claude-sonnet-4-5-20250929-v1:0",
            "anthropic.claude-sonnet-4-20250514-v1:0",
            "anthropic.claude-haiku-4-5-20251001-v1:0",
            "amazon.nova-pro-v1:0",
            "amazon.nova-lite-v1:0",
        ],
        "ollama" => &["llama3.2", "llama3.1", "codellama", "mistral"],
        _ => &[],
    }
}

/// Get the default model for a given provider.
pub fn default_model_for_provider(provider: &str) -> String {
    match provider {
        "openai" => "gpt-5".into(),
        "google" => "gemini-2.5-flash".into(),
        "openrouter" => "anthropic/claude-sonnet-4-6".into(),
        "ollama" => "llama3.2".into(),
        "xai" => "grok-4".into(),
        "groq" => "llama-3.3-70b-versatile".into(),
        "deepseek" => "deepseek-chat".into(),
        "mistral" => "mistral-large-latest".into(),
        "cerebras" => "llama-3.3-70b".into(),
        "zai" => "glm-4-plus".into(),
        "minimax" => "MiniMax-M2.7".into(),
        "bedrock" => "anthropic.claude-sonnet-4-20250514-v1:0".into(),
        _ => "claude-opus-4-6".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_providers_has_at_least_10() {
        assert!(
            KNOWN_PROVIDERS.len() >= 10,
            "expected at least 10 providers, got {}",
            KNOWN_PROVIDERS.len()
        );
    }

    #[test]
    fn test_every_provider_has_default_model() {
        for provider in KNOWN_PROVIDERS {
            let model = default_model_for_provider(provider);
            assert!(
                !model.is_empty(),
                "provider '{}' should have a non-empty default model",
                provider
            );
        }
    }

    #[test]
    fn test_every_non_custom_provider_has_known_models() {
        for provider in KNOWN_PROVIDERS {
            if *provider == "custom" || *provider == "openrouter" {
                // custom/openrouter don't have a fixed model list
                continue;
            }
            let models = known_models_for_provider(provider);
            assert!(
                !models.is_empty(),
                "provider '{}' should have at least one known model",
                provider
            );
        }
    }

    #[test]
    fn test_minimax_provider_api_key_env() {
        assert_eq!(provider_api_key_env("minimax"), Some("MINIMAX_API_KEY"));
    }

    #[test]
    fn test_minimax_default_model() {
        assert_eq!(default_model_for_provider("minimax"), "MiniMax-M2.7");
    }

    #[test]
    fn test_minimax_known_models() {
        let models = known_models_for_provider("minimax");
        assert!(!models.is_empty(), "minimax should have known models");
        assert!(models.contains(&"MiniMax-M1"));
        assert!(models.contains(&"MiniMax-M1-40k"));
    }

    #[test]
    fn test_bedrock_in_known_providers() {
        assert!(
            KNOWN_PROVIDERS.contains(&"bedrock"),
            "bedrock should be in KNOWN_PROVIDERS"
        );
    }

    #[test]
    fn test_bedrock_provider_api_key_env() {
        assert_eq!(provider_api_key_env("bedrock"), Some("AWS_ACCESS_KEY_ID"));
    }

    #[test]
    fn test_bedrock_default_model() {
        assert_eq!(
            default_model_for_provider("bedrock"),
            "anthropic.claude-sonnet-4-20250514-v1:0"
        );
    }

    #[test]
    fn test_bedrock_known_models() {
        let models = known_models_for_provider("bedrock");
        assert!(!models.is_empty(), "bedrock should have known models");
        assert!(models.contains(&"anthropic.claude-sonnet-4-20250514-v1:0"));
        assert!(models.contains(&"amazon.nova-pro-v1:0"));
    }

    #[test]
    fn test_minimax_in_known_providers() {
        assert!(
            KNOWN_PROVIDERS.contains(&"minimax"),
            "minimax should be in KNOWN_PROVIDERS"
        );
    }

    #[test]
    fn test_openai_known_models_includes_gpt5() {
        let models = known_models_for_provider("openai");
        assert!(models.contains(&"gpt-5"), "openai should include gpt-5");
        assert!(
            models.contains(&"gpt-5-mini"),
            "openai should include gpt-5-mini"
        );
        assert!(models.contains(&"gpt-5.5"), "openai should include gpt-5.5");
        assert!(
            models.contains(&"gpt-5.5-mini"),
            "openai should include gpt-5.5-mini"
        );
    }

    #[test]
    fn test_xai_known_models_includes_grok4() {
        let models = known_models_for_provider("xai");
        assert!(models.contains(&"grok-4"), "xai should include grok-4");
    }

    #[test]
    fn test_google_known_models_includes_flash_lite() {
        let models = known_models_for_provider("google");
        assert!(
            models.contains(&"gemini-2.5-flash-lite"),
            "google should include gemini-2.5-flash-lite"
        );
    }

    #[test]
    fn test_anthropic_known_models_includes_new_variants() {
        let models = known_models_for_provider("anthropic");
        assert!(
            models.contains(&"claude-opus-4-7"),
            "anthropic should include claude-opus-4-7"
        );
        assert!(
            models.contains(&"claude-sonnet-4-6"),
            "anthropic should include claude-sonnet-4-6"
        );
        assert!(
            models.contains(&"claude-sonnet-4-5"),
            "anthropic should include claude-sonnet-4-5"
        );
        assert!(
            models.contains(&"claude-haiku-4-5"),
            "anthropic should include claude-haiku-4-5"
        );
        // Older models still present
        assert!(
            models.contains(&"claude-opus-4-6"),
            "anthropic should still include claude-opus-4-6"
        );
        assert!(
            models.contains(&"claude-sonnet-4-20250514"),
            "anthropic should still include claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_bedrock_known_models_includes_new_variants() {
        let models = known_models_for_provider("bedrock");
        assert!(
            models.contains(&"anthropic.claude-opus-4-7"),
            "bedrock should include anthropic.claude-opus-4-7"
        );
        assert!(
            models.contains(&"anthropic.claude-sonnet-4-6"),
            "bedrock should include anthropic.claude-sonnet-4-6"
        );
    }

    #[test]
    fn test_openrouter_default_uses_sonnet_4_6() {
        assert_eq!(
            default_model_for_provider("openrouter"),
            "anthropic/claude-sonnet-4-6"
        );
    }
}
