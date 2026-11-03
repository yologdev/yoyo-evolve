//! #1020: the bedrock and google arms of `build_agent` handed their request
//! DEFAULT (`max_tokens`: bedrock 16000, google 8192) to the #943 ceiling
//! warning as if it were the model's maximum, so `--max-tokens 32000` was told
//! to drop to 8192 on a model whose real output limit is 65,536. Same shape as
//! #1019 (OpenAI-compatible) and #964 (Anthropic). Kept out of
//! agent_builder.rs for its size gate, like `anthropic_preset_tests.rs`.

use crate::agent_builder::{
    anthropic_output_maximum, create_model_config, declared_output_ceiling,
    max_tokens_ceiling_warning,
};

/// Composed exactly as `build_agent` composes it: resolve the arm's
/// `ModelConfig`, take `declared_output_ceiling` (the one call every arm
/// makes), then the warning `configure_agent` would print. This is the
/// `Option<String>` the user receives, not a helper one layer down.
fn ceiling_warning(provider: &str, model: &str, configured: u32) -> Option<String> {
    let resolved = create_model_config(provider, model, None).max_tokens;
    declared_output_ceiling(provider, model, resolved)
        .and_then(|c| max_tokens_ceiling_warning(configured, c, model, true))
}

#[test]
fn bedrock_and_google_do_not_quote_their_request_default_as_a_ceiling() {
    // The two live repros from #1020. The bedrock default
    // (anthropic.claude-sonnet-4-6) took the anthropic preset's 64000 on Day
    // 247 (#1116), so it can no longer hold the "under 32000" premise; a
    // bedrock id still on the 16K guess stands in for it here.
    for (provider, model) in [
        ("bedrock", "amazon.nova-pro-v1:0"),
        ("google", "gemini-2.5-flash"),
    ] {
        // Anti-vacuous: the resolved default really is below 32000, so the old
        // `Some(max_tokens)` would have warned here.
        let default = create_model_config(provider, model, None).max_tokens;
        assert!(
            default < 32_000,
            "fixture premise: {provider} default {default} should be under 32000"
        );
        assert!(max_tokens_ceiling_warning(32_000, default, model, true).is_some());
        assert_eq!(
            ceiling_warning(provider, model, 32_000),
            None,
            "{provider}/{model}: no preset declares a maximum, so no warning"
        );
    }
    // The bedrock default itself: the bedrock arm still declares no ceiling.
    let model = "anthropic.claude-sonnet-4-6";
    assert_eq!(
        create_model_config("bedrock", model, None).max_tokens,
        64_000
    );
    assert_eq!(ceiling_warning("bedrock", model, 128_000), None);
}

#[test]
fn ceiling_warning_still_fires_where_a_maximum_is_declared() {
    // Anthropic (#964): declared maximum 128000; the full string is unchanged.
    let max = anthropic_output_maximum("claude-opus-5-5").expect("declared");
    assert!(131_072 > max, "fixture premise: 131072 is above {max}");
    assert_eq!(
        ceiling_warning("anthropic", "claude-opus-5-5", 131_072).as_deref(),
        Some(
            "max_tokens: configured 131072 exceeds claude-opus-5-5's declared output \
ceiling 128000. The provider may reject the request with a 400. Lower max_tokens to \
128000 or below, or pick a model that allows more."
        )
    );
    assert_eq!(
        ceiling_warning("anthropic", "claude-opus-5-5", 128_000),
        None
    );
    // OpenAI GPT-6 preset (#1019): declared maximum 128000 still warns.
    assert_eq!(
        ceiling_warning("openai", "gpt-6-sol", 200_000).as_deref(),
        Some(
            "max_tokens: configured 200000 exceeds gpt-6-sol's declared output \
ceiling 128000. The provider may reject the request with a 400. Lower max_tokens to \
128000 or below, or pick a model that allows more."
        )
    );
}

/// Weak source guard, stated as such: it proves every arm of `build_agent`
/// CALLS `declared_output_ceiling` and none passes a bare `Some(...max_tokens)`,
/// never that the call is right. A fifth arm fails the exact count, forcing
/// whoever adds it to decide what its ceiling is.
#[test]
fn every_build_agent_arm_takes_its_ceiling_from_one_function() {
    let src = include_str!("agent_builder.rs");
    let start = src
        .find(&["pub fn ", "build_agent(&self"].concat())
        .expect("build_agent");
    let end = start + src[start..].find("\n    }\n").expect("end of build_agent");
    let body = &src[start..end];
    let call = ["declared_output_ceiling", "(&self.provider"].concat();
    assert_eq!(body.matches(&call).count(), 4, "one call per provider arm");
    let bare = ["Some(model_config", ".max_tokens)"].concat();
    assert!(
        !body.contains(&bare),
        "a request default is not a maximum (#1020)"
    );
}
