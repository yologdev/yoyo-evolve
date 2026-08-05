//! Truncation of tool *results* and tool *errors* against the user's
//! `--max-tool-output` ceiling.
//!
//! These two helpers are the only place that ceiling is applied to what a tool
//! hands back, so they live together rather than inside the decorator module:
//! `TruncatingTool` (in `tool_wrappers.rs`) is just the wiring that calls them.
//!
//! Both halves matter. A tool that fails can put as much text into the model's
//! context as one that succeeds — `SmartEditTool` embeds a file excerpt in its
//! near-miss `Err(ToolError::Failed(..))` (#675) — and before
//! [`truncate_tool_error`] existed the error path propagated through
//! `TruncatingTool` with `?`, untouched by the cap the user set.

use crate::format::truncate_tool_output;

/// Truncate the text content of a ToolResult if it exceeds the given char limit.
pub(crate) fn truncate_result(
    mut result: yoagent::types::ToolResult,
    max_chars: usize,
) -> yoagent::types::ToolResult {
    use yoagent::Content;
    result.content = result
        .content
        .into_iter()
        .map(|c| match c {
            Content::Text { text } => Content::Text {
                text: truncate_tool_output(&text, max_chars),
            },
            other => other,
        })
        .collect();
    result
}

/// Truncate the message carried by a `ToolError` if it exceeds `max_chars`.
///
/// Tool *errors* reach the model's context exactly like tool *results* do, but
/// `TruncatingTool::execute` used to propagate them with `?`, so the
/// user-configured `--max-tool-output` ceiling could not touch them (#675).
/// A tool that embeds a file excerpt in its error — `SmartEditTool`'s near-miss
/// hint is the known case — could therefore push far more text into context
/// than the user asked for.
///
/// The message-bearing variants are truncated through `truncate_tool_output`,
/// so the result stays char-boundary-safe and keeps head+tail context.
/// Deliberately a no-op below the ceiling: an error message that already fits
/// is returned byte-identical, because `truncate_tool_output` also strips ANSI
/// and collapses repeated lines, and error text is parsed elsewhere
/// (`commands_retry::extract_tool_name_from_error`). `Cancelled` carries no
/// message and is passed through untouched.
pub(crate) fn truncate_tool_error(
    err: yoagent::types::ToolError,
    max_chars: usize,
) -> yoagent::types::ToolError {
    use yoagent::types::ToolError as E;
    let shrink = |msg: String| {
        if msg.len() <= max_chars {
            msg
        } else {
            truncate_tool_output(&msg, max_chars)
        }
    };
    match err {
        E::Failed(msg) => E::Failed(shrink(msg)),
        E::NotFound(msg) => E::NotFound(shrink(msg)),
        E::InvalidArgs(msg) => E::InvalidArgs(shrink(msg)),
        E::Cancelled => E::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::types::ToolError;

    fn message_of(err: &ToolError) -> Option<&str> {
        match err {
            ToolError::Failed(m) | ToolError::NotFound(m) | ToolError::InvalidArgs(m) => Some(m),
            ToolError::Cancelled => None,
        }
    }

    #[test]
    fn short_error_message_is_unchanged() {
        let err = truncate_tool_error(ToolError::Failed("boom".to_string()), 1000);
        assert_eq!(message_of(&err), Some("boom"));
    }

    #[test]
    fn long_error_message_is_shrunk() {
        // A near-miss snippet shape: many long lines in one error message.
        let huge = std::iter::repeat_n("x".repeat(500), 400)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(huge.len() > 100_000);
        let err = truncate_tool_error(ToolError::Failed(huge.clone()), 5_000);
        let msg = message_of(&err).expect("Failed carries a message");
        assert!(
            msg.len() < huge.len(),
            "expected truncation, got {} bytes",
            msg.len()
        );
    }

    #[test]
    fn error_variant_is_preserved() {
        let long = "y".repeat(50_000);
        assert!(matches!(
            truncate_tool_error(ToolError::NotFound(long.clone()), 100),
            ToolError::NotFound(_)
        ));
        assert!(matches!(
            truncate_tool_error(ToolError::InvalidArgs(long), 100),
            ToolError::InvalidArgs(_)
        ));
    }

    #[test]
    fn cancelled_has_no_message_and_passes_through() {
        assert!(matches!(
            truncate_tool_error(ToolError::Cancelled, 10),
            ToolError::Cancelled
        ));
    }

    #[test]
    fn multibyte_error_message_does_not_panic() {
        let msg = "✓ 完了 ".repeat(20_000);
        let err = truncate_tool_error(ToolError::Failed(msg), 200);
        assert!(message_of(&err).is_some());
    }

    #[test]
    fn text_result_is_truncated_and_non_text_is_not() {
        let result = yoagent::types::ToolResult {
            content: vec![
                yoagent::Content::Text {
                    text: "z\n".repeat(50_000),
                },
                yoagent::Content::Image {
                    data: "base64data".to_string(),
                    mime_type: "image/png".to_string(),
                },
            ],
            details: serde_json::Value::Null,
        };
        let truncated = truncate_result(result, 1_000);
        match &truncated.content[0] {
            yoagent::Content::Text { text } => assert!(text.len() < 100_000),
            other => panic!("expected Text, got {other:?}"),
        }
        match &truncated.content[1] {
            yoagent::Content::Image { data, .. } => assert_eq!(data, "base64data"),
            other => panic!("expected Image, got {other:?}"),
        }
    }
}
