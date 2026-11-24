//! #1228: an aborted stream-json run's closing events carry the partial reply.
//!
//! When a run is cancelled mid-stream (SIGTERM, SIGHUP, SIGQUIT, Ctrl+C),
//! yoagent 0.27.9 closes the message with an empty `Text` (`agent_loop.rs`
//! builds it with `error_message`, and `ProviderError::Cancelled` carries no
//! partial; upstream ask #1237). The text the model had produced exists only in
//! the earlier `messageUpdate` deltas, so a script reading only `agentEnd` gets
//! nothing.
//!
//! This module keeps the current message's text deltas and, on the wire only,
//! fills an assistant message that is BOTH `stopReason: aborted` AND empty with
//! them: in that message's `messageEnd` and as the last assistant message of
//! `agentEnd`. Same field, same event shape. Every other event (a normal end,
//! an aborted message that already has text, any non-closing event) is written
//! unchanged. The agent's own `messages()` are not touched.

use std::borrow::Cow;
use yoagent::types::{AgentMessage, Content, Message, StopReason, StreamDelta};
use yoagent::AgentEvent;

/// The text deltas of the message currently streaming.
#[derive(Default)]
pub(crate) struct AbortedTextFill {
    partial: String,
}

impl AbortedTextFill {
    /// Track `event`: a `messageStart` (including a retry's new attempt)
    /// starts a fresh message, a text delta extends it.
    pub(crate) fn observe(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::MessageStart { .. } => self.partial.clear(),
            AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            } => self.partial.push_str(delta),
            _ => {}
        }
    }

    /// `event` as it goes on the wire: borrowed unchanged unless it closes an
    /// aborted, empty assistant message and deltas were streamed for it.
    pub(crate) fn fill<'a>(&self, event: &'a AgentEvent) -> Cow<'a, AgentEvent> {
        if self.partial.is_empty() {
            return Cow::Borrowed(event);
        }
        // Clone the whole event and edit the one message in place, so every
        // other field (these variants are non-exhaustive) stays as yoagent sent it.
        let target = match event {
            AgentEvent::MessageEnd { message, .. } if needs_fill(message) => None,
            AgentEvent::AgentEnd { messages, .. } => {
                match messages.iter().rposition(is_assistant) {
                    Some(i) if needs_fill(&messages[i]) => Some(i),
                    _ => return Cow::Borrowed(event),
                }
            }
            _ => return Cow::Borrowed(event),
        };
        let mut filled = event.clone();
        match (&mut filled, target) {
            (AgentEvent::MessageEnd { message, .. }, None) => set_text(message, &self.partial),
            (AgentEvent::AgentEnd { messages, .. }, Some(i)) => {
                set_text(&mut messages[i], &self.partial)
            }
            _ => {}
        }
        Cow::Owned(filled)
    }
}

fn is_assistant(m: &AgentMessage) -> bool {
    matches!(m, AgentMessage::Llm(Message::Assistant { .. }))
}

/// An assistant message that was aborted and has no text at all.
fn needs_fill(m: &AgentMessage) -> bool {
    match m {
        AgentMessage::Llm(Message::Assistant {
            content,
            stop_reason: StopReason::Aborted,
            ..
        }) => !content
            .iter()
            .any(|c| matches!(c, Content::Text { text } if !text.is_empty())),
        _ => false,
    }
}

/// Put `text` into the message's first (empty) text block, or add one.
fn set_text(m: &mut AgentMessage, text: &str) {
    if let AgentMessage::Llm(Message::Assistant { content, .. }) = m {
        for c in content.iter_mut() {
            if let Content::Text { text: t } = c {
                *t = text.to_string();
                return;
            }
        }
        content.insert(
            0,
            Content::Text {
                text: text.to_string(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::types::Usage;

    fn assistant(text: &str, stop: StopReason) -> AgentMessage {
        AgentMessage::Llm(
            Message::assistant(
                vec![Content::Text {
                    text: text.to_string(),
                }],
                stop,
                "m",
                "unknown",
                Usage::default(),
            )
            .with_timestamp(1),
        )
    }

    fn delta(s: &str) -> AgentEvent {
        AgentEvent::MessageUpdate {
            message: assistant("", StopReason::Stop),
            delta: StreamDelta::Text { delta: s.into() },
        }
    }

    fn streamed(fill: &mut AbortedTextFill, parts: &[&str]) {
        fill.observe(&AgentEvent::MessageStart {
            message: assistant("", StopReason::Stop),
        });
        for p in parts {
            fill.observe(&delta(p));
        }
    }

    fn wire(fill: &AbortedTextFill, e: &AgentEvent) -> String {
        serde_json::to_string(fill.fill(e).as_ref()).unwrap()
    }

    #[test]
    fn aborted_and_empty_gets_the_streamed_partial() {
        let mut fill = AbortedTextFill::default();
        streamed(&mut fill, &["NEW ", "ANSWER"]);
        let end =
            AgentEvent::agent_end(vec![assistant("", StopReason::Aborted)], Default::default());
        let want = AgentEvent::agent_end(
            vec![assistant("NEW ANSWER", StopReason::Aborted)],
            Default::default(),
        );
        assert_eq!(wire(&fill, &end), serde_json::to_string(&want).unwrap());
        let msg_end = AgentEvent::MessageEnd {
            message: assistant("", StopReason::Aborted),
        };
        let want = AgentEvent::MessageEnd {
            message: assistant("NEW ANSWER", StopReason::Aborted),
        };
        assert_eq!(wire(&fill, &msg_end), serde_json::to_string(&want).unwrap());
    }

    #[test]
    fn a_normal_end_is_byte_identical() {
        let mut fill = AbortedTextFill::default();
        streamed(&mut fill, &["hi"]);
        let end = AgentEvent::agent_end(vec![assistant("", StopReason::Stop)], Default::default());
        assert_eq!(wire(&fill, &end), serde_json::to_string(&end).unwrap());
        assert!(matches!(fill.fill(&end), Cow::Borrowed(_)));
    }

    #[test]
    fn an_aborted_message_that_has_text_is_byte_identical() {
        let mut fill = AbortedTextFill::default();
        streamed(&mut fill, &["streamed"]);
        let end = AgentEvent::agent_end(
            vec![assistant("kept", StopReason::Aborted)],
            Default::default(),
        );
        assert_eq!(wire(&fill, &end), serde_json::to_string(&end).unwrap());
        assert!(matches!(fill.fill(&end), Cow::Borrowed(_)));
    }

    #[test]
    fn a_retry_drops_the_dying_attempts_text() {
        let mut fill = AbortedTextFill::default();
        streamed(&mut fill, &["dying attempt"]);
        streamed(&mut fill, &["second"]);
        let end =
            AgentEvent::agent_end(vec![assistant("", StopReason::Aborted)], Default::default());
        assert!(wire(&fill, &end).contains(r#""text":"second""#));
    }

    #[test]
    fn nothing_streamed_leaves_the_aborted_message_empty() {
        let mut fill = AbortedTextFill::default();
        streamed(&mut fill, &[]);
        let end =
            AgentEvent::agent_end(vec![assistant("", StopReason::Aborted)], Default::default());
        assert!(matches!(fill.fill(&end), Cow::Borrowed(_)));
    }
}
