//! #991 item 3: the stream-json loop (`handle_stream_json_events`) runs
//! outside the retry-safe gate, and until Day 243 it had no `MessageStart` /
//! `ProviderRetry` arm at all, so after yoagent retried a dead attempt
//! in-loop, the outcome's `text` read `PARTIAL_PONG`: both attempts joined.
//! Measured with the stream below before the fix.
//!
//! Today no production caller reads that text (both `main.rs` stream-json
//! doors read only `last_api_error`), but `PromptOutcome` is the documented
//! "same outcome as `run_prompt`" contract and #808 pins its text on this
//! path, so it is held to the text path's #989 truncation instead of
//! being deleted. These tests assert on the outcome a caller receives.

use super::*;
use yoagent::provider::MockProvider;

fn assistant_shell() -> AgentMessage {
    Message::assistant(
        Vec::new(),
        StopReason::Stop,
        "claude-test",
        "anthropic",
        Usage::default(),
    )
    .into()
}

fn start() -> AgentEvent {
    AgentEvent::MessageStart {
        message: assistant_shell(),
    }
}

fn text(delta: &str) -> AgentEvent {
    AgentEvent::MessageUpdate {
        message: assistant_shell(),
        delta: StreamDelta::Text {
            delta: delta.to_string(),
        },
    }
}

fn retry() -> AgentEvent {
    AgentEvent::provider_retry(
        1,
        3,
        "429 rate limited",
        std::time::Duration::from_millis(0),
    )
}

async fn outcome_for(events: Vec<AgentEvent>) -> PromptOutcome {
    let provider = MockProvider::text("unused");
    let mut agent = Agent::from_provider(provider, yoagent::provider::ModelConfig::mock())
        .with_api_key("not-a-real-key");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    for event in events {
        tx.send(event).unwrap();
    }
    drop(tx); // closing the channel ends the loop
    let changes = SessionChanges::new();
    let (outcome, _usage) =
        handle_stream_json_events(&mut agent, rx, "claude-test", &changes).await;
    outcome
}

#[tokio::test]
async fn stream_json_retry_drops_dead_attempt_text_from_outcome() {
    let outcome = outcome_for(vec![
        start(),
        text("PARTIAL_"),
        retry(),
        start(),
        text("PONG"),
    ])
    .await;
    assert_eq!(outcome.text, "PONG", "dead attempt's text must not survive");
    assert_eq!(outcome.text_since_last_tool, "PONG");
}

/// The dying attempt may end inside a multi-byte char's neighbourhood; the
/// cut is at the attempt boundary, never a byte index inside a char (#250).
#[tokio::test]
async fn stream_json_retry_keeps_earlier_messages_and_multibyte_text() {
    let outcome = outcome_for(vec![
        start(),
        text("🐙 first message. "),
        start(),
        text("日本語の途中"),
        retry(),
        start(),
        text("✓ done"),
    ])
    .await;
    assert_eq!(outcome.text, "🐙 first message. ✓ done");
    assert_eq!(outcome.text_since_last_tool, "🐙 first message. ✓ done");
}

/// Near-miss: one attempt, no retry, is byte-identical to before.
#[tokio::test]
async fn stream_json_without_retry_keeps_all_text() {
    let outcome = outcome_for(vec![start(), text("PARTIAL_"), text("PONG")]).await;
    assert_eq!(outcome.text, "PARTIAL_PONG");
    assert_eq!(outcome.text_since_last_tool, "PARTIAL_PONG");
}

/// Near-miss: a retry that fires before any text streamed (the common case,
/// a 429 on the first request) drops nothing.
#[tokio::test]
async fn stream_json_retry_before_any_text_drops_nothing() {
    let outcome = outcome_for(vec![start(), retry(), start(), text("PONG")]).await;
    assert_eq!(outcome.text, "PONG");
    assert_eq!(outcome.text_since_last_tool, "PONG");
}
