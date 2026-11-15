//! #999: the main agent's per-run execution limits, asserted on the exact
//! value `AgentConfig::configure_agent` passes to `with_execution_limits`.
//! Lives beside `agent_builder.rs` because that module is at its size gate.

use crate::agent_builder::main_agent_execution_limits;
use crate::agent_builder::AgentConfig;
use yoagent::context::ExecutionLimits;
use yoagent::ThinkingLevel;

#[test]
fn main_agent_limits_default_has_turn_cap_and_no_token_or_clock_cap() {
    let l = main_agent_execution_limits(None);
    assert_eq!(l.max_turns, 200);
    assert_eq!(l.max_total_tokens, usize::MAX);
    assert_eq!(l.max_duration, std::time::Duration::MAX);
    // Fails if someone goes back to inheriting yoagent's default clock
    // (600 s in 0.24.2) — the cap nobody in yoyo chose.
    assert_ne!(l.max_duration, ExecutionLimits::default().max_duration);
    assert_ne!(
        l.max_total_tokens,
        ExecutionLimits::default().max_total_tokens
    );
    // Loop detection (#856) is kept, not dropped with the other caps.
    assert_eq!(
        l.max_consecutive_identical_tool_calls,
        ExecutionLimits::default().max_consecutive_identical_tool_calls
    );
}

#[test]
fn main_agent_limits_user_turns_change_only_the_turn_cap() {
    let user = main_agent_execution_limits(Some(37));
    let dflt = main_agent_execution_limits(None);
    assert_eq!(user.max_turns, 37);
    assert_eq!(user.max_total_tokens, dflt.max_total_tokens);
    assert_eq!(user.max_duration, dflt.max_duration);
    assert_eq!(
        user.max_consecutive_identical_tool_calls,
        dflt.max_consecutive_identical_tool_calls
    );
}

#[test]
fn main_agent_token_cap_exceeds_the_observed_999_stop() {
    // `[Agent stopped: Max tokens reached (1061497/1000000)]`, 2026-10-05.
    let l = main_agent_execution_limits(None);
    assert!(l.max_total_tokens > 1_061_497);
}

/// #1203: `/side` and `/architect` build their own agents. Before the fix
/// neither set `max_tokens`, so a user's cap was ignored on every provider
/// and Bedrock sent no `maxTokens` at all. Both run with thinking off, so
/// they must resolve the cap the way the main agent does with thinking off.
fn limits_config(provider: &str, model: &str, max_tokens: Option<u32>) -> AgentConfig {
    AgentConfig {
        model: model.to_string(),
        api_key: "test-key".to_string(),
        provider: provider.to_string(),
        base_url: None,
        skills: yoagent::skills::SkillSet::empty(),
        system_prompt: "Test.".to_string(),
        thinking: ThinkingLevel::Off,
        max_tokens,
        temperature: None,
        max_turns: None,
        auto_approve: true,
        auto_commit: false,
        permissions: crate::cli::PermissionConfig::default(),
        dir_restrictions: crate::cli::DirectoryRestrictions::default(),
        context_strategy: crate::cli::ContextStrategy::default(),
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

#[test]
fn side_and_architect_agents_resolve_max_tokens_like_the_main_agent() {
    let bedrock_model = "anthropic.claude-sonnet-4-6";
    let bedrock_preset =
        yoagent::provider::ModelConfig::preset("bedrock", bedrock_model).map(|p| p.max_tokens);
    // Row 3's expected value is yoagent's preset, not a number typed here;
    // `Some` so the row cannot pass vacuously as None == None.
    assert!(
        bedrock_preset.is_some(),
        "{bedrock_model}: no bedrock preset"
    );
    let rows: [(&str, &str, Option<u32>, Option<u32>); 4] = [
        ("anthropic", "claude-sonnet-4-6", Some(2048), Some(2048)),
        ("anthropic", "claude-sonnet-4-6", None, None),
        ("bedrock", bedrock_model, None, bedrock_preset),
        ("openai", "gpt-4o", Some(1234), Some(1234)),
    ];
    for (provider, model, user, expected) in rows {
        let c = limits_config(provider, model, user);
        let main = c.build_agent().max_tokens;
        let side = c.build_side_agent().max_tokens;
        // The architect usually runs a different model; same model here so
        // the three agents are comparable cell by cell.
        let architect = c.build_architect_agent(model).max_tokens;
        eprintln!(
            "{provider}/{model} user={user:?}: main={main:?} side={side:?} architect={architect:?}"
        );
        assert_eq!(main, expected, "{provider} user={user:?}: main agent");
        assert_eq!(
            side, main,
            "{provider} user={user:?}: /side differs from main"
        );
        assert_eq!(
            architect, main,
            "{provider} user={user:?}: /architect differs from main"
        );
    }
}
