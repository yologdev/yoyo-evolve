//! #1108: `/cost`'s per-turn view of delegated (sub-agent) spend. Every value
//! under test goes through the real recording path (`record_call_with`, the
//! seam `record_sub_agent_spend` calls) on a local store, and every child's
//! stats go through yoagent's own serialisation (d210/d211).

use super::*;
use yoagent::{AgentMessage, Content, Message, StopReason, Usage};

fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input,
        output,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
    }
}

/// A sub_agent tool result as yoagent's loop leaves it.
fn delegated(input: u64, output: u64, cost: Option<f64>) -> ToolResult {
    let mut child = SessionStats::default();
    child.usage = usage(input, output);
    child.cost_usd = cost;
    let mut details = serde_json::json!({});
    details[yoagent::SUB_AGENT_STATS_KEY] = serde_json::to_value(&child).expect("serialize");
    ToolResult {
        content: vec![Content::Text { text: "ok".into() }],
        details,
    }
}

/// An assistant turn that issued the given tool calls.
fn turn(call_ids: &[&str]) -> AgentMessage {
    let mut content = vec![Content::Text { text: "t".into() }];
    for id in call_ids {
        content.push(Content::tool_call(*id, "sub_agent", serde_json::json!({})));
    }
    AgentMessage::Llm(Message::assistant(
        content,
        StopReason::Stop,
        "m",
        "p",
        usage(10, 5),
    ))
}

fn user(text: &str) -> AgentMessage {
    AgentMessage::Llm(Message::user(text))
}

/// The regression surface: nobody delegated, so `/cost` gets no block at all.
/// Also covers ordinary (non-delegating) tool results going through the
/// recorder: they must leave the store empty.
#[test]
fn no_delegation_adds_nothing_to_cost() {
    let store = Mutex::new(Vec::new());
    let plain = ToolResult {
        content: vec![],
        details: serde_json::json!({"exit_code": 0}),
    };
    record_call_with(&store, "call-1", &plain);
    let by_call = lock_or_recover(&store).clone();
    assert!(by_call.is_empty());
    let msgs = vec![user("hi"), turn(&["call-1"]), turn(&[])];
    assert_eq!(delegated_turn_lines(&msgs, &by_call), None);
    assert_eq!(delegated_turn_lines(&[], &[]), None);
}

#[test]
fn one_delegating_turn_is_named_by_its_turn_number() {
    let store = Mutex::new(Vec::new());
    record_call_with(&store, "c2", &delegated(12_300, 1_200, Some(0.042)));
    let by_call = lock_or_recover(&store).clone();
    let msgs = vec![user("go"), turn(&[]), turn(&["c2"]), turn(&[])];
    assert_eq!(
        delegated_turn_lines(&msgs, &by_call).unwrap(),
        "    Sub-agent spend by turn (not in the per-turn costs above):\n      \
         Turn 2: 1 run, 12.3k in / 1.2k out, $0.042\n      \
         Total delegated: 1 run, 12.3k in / 1.2k out, $0.042"
    );
}

#[test]
fn two_turns_and_two_calls_in_one_turn_sum_to_a_total() {
    let store = Mutex::new(Vec::new());
    record_call_with(&store, "a", &delegated(1_000, 100, Some(0.01)));
    record_call_with(&store, "b", &delegated(1_000, 100, Some(0.02)));
    record_call_with(&store, "c", &delegated(500, 50, Some(0.5)));
    let by_call = lock_or_recover(&store).clone();
    let msgs = vec![user("x"), turn(&["a", "b"]), user("y"), turn(&["c"])];
    assert_eq!(
        delegated_turn_lines(&msgs, &by_call).unwrap(),
        "    Sub-agent spend by turn (not in the per-turn costs above):\n      \
         Turn 1: 2 runs, 2.0k in / 200 out, $0.030\n      \
         Turn 2: 1 run, 500 in / 50 out, $0.500\n      \
         Total delegated: 3 runs, 2.5k in / 250 out, $0.530"
    );
}

/// An unpriced child is "unpriced", never $0, and it makes the total unpriced
/// too (a partial sum would read as the whole bill).
#[test]
fn unpriced_row_says_unpriced_never_zero() {
    let store = Mutex::new(Vec::new());
    record_call_with(&store, "p", &delegated(100, 10, Some(0.01)));
    record_call_with(&store, "u", &delegated(2_000, 300, None));
    let by_call = lock_or_recover(&store).clone();
    // Anti-vacuous: the unpriced fixture really has tokens and no price.
    assert!(by_call[1].1.usage.input > 0 && by_call[1].1.cost_usd.is_none());
    let msgs = vec![turn(&["p"]), turn(&["u"])];
    let out = delegated_turn_lines(&msgs, &by_call).unwrap();
    assert_eq!(
        out,
        "    Sub-agent spend by turn (not in the per-turn costs above):\n      \
         Turn 1: 1 run, 100 in / 10 out, $0.010\n      \
         Turn 2: 1 run, 2.0k in / 300 out, unpriced (not $0)\n      \
         Total delegated: 2 runs, 2.1k in / 310 out, unpriced (not $0)"
    );
    assert!(!out.contains("$0.0000"));
}

/// Spend whose turn is no longer in history (cleared or compacted) is still
/// shown and still in the total, never silently dropped.
#[test]
fn spend_from_turns_no_longer_in_history_is_shown_apart() {
    let store = Mutex::new(Vec::new());
    record_call_with(&store, "gone", &delegated(800, 90, Some(0.2)));
    record_call_with(&store, "here", &delegated(100, 10, Some(0.1)));
    let by_call = lock_or_recover(&store).clone();
    let msgs = vec![turn(&["here"])];
    assert_eq!(
        delegated_turn_lines(&msgs, &by_call).unwrap(),
        "    Sub-agent spend by turn (not in the per-turn costs above):\n      \
         Turn 1: 1 run, 100 in / 10 out, $0.100\n      \
         Not in the turns above (cleared or compacted): 1 run, 800 in / 90 out, $0.200\n      \
         Total delegated: 2 runs, 900 in / 100 out, $0.300"
    );
}

/// The block has no glyphs at all, so it reads the same under plain output.
#[test]
fn block_is_ascii_only() {
    let store = Mutex::new(Vec::new());
    record_call_with(&store, "a", &delegated(1, 1, None));
    record_call_with(&store, "z", &delegated(1, 1, Some(0.3)));
    let by_call = lock_or_recover(&store).clone();
    let out = delegated_turn_lines(&[turn(&["a"])], &by_call).unwrap();
    assert!(out.is_ascii(), "non-ASCII in: {out}");
}
