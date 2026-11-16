//! #1206: a refused turn must not poison the next request.
//!
//! Seen Day 261 01:38: the model refused (`StopReason::Refusal`) after ten
//! tool calls, and the piped auto-continue loop (`run_piped_mode`,
//! `continue_on_silence = true`) sent the next request with the refused turn
//! still in the history. Anthropic answered 400
//! `messages.19.content.0.thinking.cache_control: Extra inputs are not
//! permitted`: the refused assistant message held only a thinking block, and
//! yoagent 0.27.9 puts the conversation cache breakpoint on the LAST block of
//! the latest earlier message (`provider/anthropic.rs`, "Breakpoint 3", and
//! `bridge_breakpoint`), skipping only empty text — so it landed on thinking,
//! which Anthropic refuses to cache-mark. Any later request in that session
//! (REPL next prompt, auto-continue, a saved and restored session) failed the
//! same way.
//!
//! The real fix is upstream (yologdev/yoagent#455: never place
//! `cache_control` on a thinking block). Until that ships and yoyo's pin
//! moves, this scrubs the refused turn after `finish()`: its thinking blocks
//! go (a refusal produced no answer to reason about, and nothing later needs
//! their signatures — only a thinking block before a `tool_use` must be
//! replayed, and a refusal has no `tool_use` to continue), and if nothing
//! readable is left, one short text marker keeps user/assistant alternation.
//! Every other message is left exactly as it was.

use yoagent::{Agent, AgentMessage, Content, Message, StopReason};

/// Text that stands in for a refused turn whose only content was thinking.
/// Not the stderr notice's `refused this request` contract (#686): this goes
/// to the model, not to the evolve harness's log grep.
pub(super) const REFUSED_TURN_MARKER: &str = "[the model refused this request]";

/// `Some(new history)` when the LAST message is a refused assistant turn that
/// still carries thinking blocks; `None` when there is nothing to change.
pub(super) fn scrub_refused_turn(msgs: &[AgentMessage]) -> Option<Vec<AgentMessage>> {
    let Some(AgentMessage::Llm(Message::Assistant {
        content,
        stop_reason: StopReason::Refusal,
        ..
    })) = msgs.last()
    else {
        return None;
    };
    if !content
        .iter()
        .any(|c| matches!(c, Content::Thinking { .. }))
    {
        return None;
    }
    let mut out = msgs.to_vec();
    if let Some(AgentMessage::Llm(Message::Assistant { content, .. })) = out.last_mut() {
        content.retain(|c| !matches!(c, Content::Thinking { .. }));
        let readable = content
            .iter()
            .any(|c| !matches!(c, Content::Text { text } if text.trim().is_empty()));
        if !readable {
            // Empty text blocks are rejected by Anthropic too, so "only empty
            // text left" counts as nothing left.
            *content = vec![Content::Text {
                text: REFUSED_TURN_MARKER.to_string(),
            }];
        }
    }
    Some(out)
}

/// `agent.finish()`, then scrub a refused final turn (#1206). Used at every
/// place a prompt run is joined, so the history is current when it is read
/// (#258) and is clean before anything saves or resends it.
pub(super) async fn finish_and_scrub(agent: &mut Agent) {
    agent.finish().await;
    if let Some(scrubbed) = scrub_refused_turn(agent.messages()) {
        agent.replace_messages(scrubbed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::Usage;

    fn user(text: &str) -> AgentMessage {
        AgentMessage::Llm(Message::user(text))
    }

    fn thinking() -> Content {
        serde_json::from_value(serde_json::json!({
            "type": "thinking",
            "thinking": "considering",
            "signature": "sig-1"
        }))
        .expect("thinking block deserializes")
    }

    fn text(t: &str) -> Content {
        Content::Text {
            text: t.to_string(),
        }
    }

    /// Built through serde because `Message::Assistant` is `#[non_exhaustive]`.
    fn assistant(stop: StopReason, content: Vec<Content>) -> AgentMessage {
        let stop = serde_json::to_value(stop).unwrap();
        let content = serde_json::to_value(content).unwrap();
        let usage = serde_json::to_value(Usage::default()).unwrap();
        serde_json::from_value(serde_json::json!({
            "role": "assistant",
            "content": content,
            "stopReason": stop,
            "model": "m",
            "provider": "anthropic",
            "usage": usage,
            "timestamp": 0
        }))
        .expect("assistant message deserializes")
    }

    fn last_content(msgs: &[AgentMessage]) -> Vec<Content> {
        match msgs.last() {
            Some(AgentMessage::Llm(Message::Assistant { content, .. })) => content.clone(),
            other => panic!("last message is not an assistant turn: {other:?}"),
        }
    }

    #[test]
    fn refused_thinking_only_turn_becomes_the_text_marker() {
        let msgs = vec![user("hi"), assistant(StopReason::Refusal, vec![thinking()])];
        let out = scrub_refused_turn(&msgs).expect("a refused thinking-only turn is scrubbed");
        assert_eq!(out.len(), 2, "no message added or dropped");
        assert_eq!(last_content(&out), vec![text(REFUSED_TURN_MARKER)]);
        // The user turn is untouched.
        assert_eq!(
            serde_json::to_value(&out[0]).unwrap(),
            serde_json::to_value(&msgs[0]).unwrap()
        );
    }

    #[test]
    fn refused_turn_with_text_keeps_the_text_and_loses_the_thinking() {
        let msgs = vec![
            user("hi"),
            assistant(StopReason::Refusal, vec![thinking(), text("I can't help")]),
        ];
        let out = scrub_refused_turn(&msgs).expect("scrubbed");
        assert_eq!(last_content(&out), vec![text("I can't help")]);
    }

    #[test]
    fn refused_turn_with_only_empty_text_left_gets_the_marker() {
        let msgs = vec![
            user("hi"),
            assistant(StopReason::Refusal, vec![thinking(), text("  ")]),
        ];
        let out = scrub_refused_turn(&msgs).expect("scrubbed");
        assert_eq!(last_content(&out), vec![text(REFUSED_TURN_MARKER)]);
    }

    /// Thinking before a tool_use must be replayed with its signature, so a
    /// non-refusal turn is never touched.
    #[test]
    fn non_refusal_last_turn_with_thinking_is_unchanged() {
        for stop in [StopReason::Stop, StopReason::ToolUse, StopReason::Length] {
            let msgs = vec![user("hi"), assistant(stop.clone(), vec![thinking()])];
            assert!(scrub_refused_turn(&msgs).is_none(), "{stop:?}");
        }
    }

    #[test]
    fn empty_history_is_unchanged() {
        assert!(scrub_refused_turn(&[]).is_none());
    }

    #[test]
    fn refusal_that_is_not_the_last_message_is_unchanged() {
        let msgs = vec![
            user("hi"),
            assistant(StopReason::Refusal, vec![thinking()]),
            user("continue"),
        ];
        assert!(scrub_refused_turn(&msgs).is_none());
    }

    /// Idempotent: an already-scrubbed refusal (no thinking) is left alone.
    #[test]
    fn refusal_without_thinking_is_unchanged() {
        let msgs = vec![
            user("hi"),
            assistant(StopReason::Refusal, vec![text(REFUSED_TURN_MARKER)]),
        ];
        assert!(scrub_refused_turn(&msgs).is_none());
    }
}
