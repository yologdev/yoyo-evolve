//! #999: the main agent's per-run execution limits, asserted on the exact
//! value `AgentConfig::configure_agent` passes to `with_execution_limits`.
//! Lives beside `agent_builder.rs` because that module is at its size gate.

use crate::agent_builder::main_agent_execution_limits;
use yoagent::context::ExecutionLimits;

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
