//! Prompt execution and agent interaction.

use crate::cli::is_verbose;
use crate::format::*;
use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Instant;
use yoagent::agent::Agent;
use yoagent::*;

/// Extract a preview of tool result content for display.
/// Returns an empty string if there's nothing meaningful to show.
fn tool_result_preview(result: &ToolResult, max_chars: usize) -> String {
    let text: String = result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    // Take first line only, truncated
    let first_line = text.lines().next().unwrap_or("");
    truncate_with_ellipsis(first_line, max_chars)
}

/// Write response text to a file if --output was specified.
pub fn write_output_file(path: &Option<String>, text: &str) {
    if let Some(path) = path {
        match std::fs::write(path, text) {
            Ok(_) => eprintln!("{DIM}  wrote response to {path}{RESET}"),
            Err(e) => eprintln!("{RED}  error writing to {path}: {e}{RESET}"),
        }
    }
}

/// Summarize a message for /history display.
pub fn summarize_message(msg: &AgentMessage) -> (&str, String) {
    match msg {
        AgentMessage::Llm(Message::User { content, .. }) => {
            let text = content
                .iter()
                .filter_map(|c| match c {
                    Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            ("user", truncate_with_ellipsis(&text, 80))
        }
        AgentMessage::Llm(Message::Assistant { content, .. }) => {
            let mut parts = Vec::new();
            let mut tool_calls = 0;
            for c in content {
                match c {
                    Content::Text { text } if !text.is_empty() => {
                        parts.push(truncate_with_ellipsis(text, 60));
                    }
                    Content::ToolCall { name, .. } => {
                        tool_calls += 1;
                        if tool_calls <= 3 {
                            parts.push(format!("→{name}"));
                        }
                    }
                    _ => {}
                }
            }
            if tool_calls > 3 {
                parts.push(format!("(+{} more tools)", tool_calls - 3));
            }
            let preview = if parts.is_empty() {
                "(empty)".to_string()
            } else {
                parts.join("  ")
            };
            ("assistant", preview)
        }
        AgentMessage::Llm(Message::ToolResult {
            tool_name,
            is_error,
            ..
        }) => {
            let status = if *is_error { "✗" } else { "✓" };
            ("tool", format!("{tool_name} {status}"))
        }
        AgentMessage::Extension(ext) => ("ext", truncate_with_ellipsis(&ext.role, 60)),
    }
}

pub async fn run_prompt(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) -> String {
    let prompt_start = Instant::now();
    let mut rx = agent.prompt(input).await;
    let mut last_usage = Usage::default();
    let mut in_text = false;
    let mut tool_timers: HashMap<String, Instant> = HashMap::new();
    let mut collected_text = String::new();

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    AgentEvent::ToolExecutionStart {
                        tool_call_id, tool_name, args, ..
                    } => {
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        tool_timers.insert(tool_call_id.clone(), Instant::now());
                        let summary = format_tool_summary(&tool_name, &args);
                        print!("{YELLOW}  ▶ {summary}{RESET}");
                        if is_verbose() {
                            // Show full tool args in verbose mode
                            println!();
                            let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                            for line in args_str.lines() {
                                println!("{DIM}    {line}{RESET}");
                            }
                        }
                        io::stdout().flush().ok();
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, result, .. } => {
                        let duration = tool_timers
                            .remove(&tool_call_id)
                            .map(|start| format_duration(start.elapsed()));
                        let dur_str = duration
                            .map(|d| format!(" {DIM}({d}){RESET}"))
                            .unwrap_or_default();
                        if is_error {
                            println!(" {RED}✗{RESET}{dur_str}");
                            // Show error preview so user can see what went wrong
                            let preview = tool_result_preview(&result, 200);
                            if !preview.is_empty() {
                                println!("{DIM}    {preview}{RESET}");
                            }
                        } else {
                            println!(" {GREEN}✓{RESET}{dur_str}");
                            // In verbose mode, show a preview of successful results too
                            if is_verbose() {
                                let preview = tool_result_preview(&result, 200);
                                if !preview.is_empty() {
                                    println!("{DIM}    {preview}{RESET}");
                                }
                            }
                        }
                    }
                    AgentEvent::ToolExecutionUpdate { partial_result, .. } => {
                        // Stream partial results from tools (MCP servers, sub-agents)
                        let preview = tool_result_preview(&partial_result, 500);
                        if !preview.is_empty() {
                            print!("{DIM}{preview}{RESET}");
                            io::stdout().flush().ok();
                        }
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Text { delta },
                        ..
                    } => {
                        if !in_text {
                            println!();
                            in_text = true;
                        }
                        collected_text.push_str(&delta);
                        print!("{}", delta);
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Thinking { delta },
                        ..
                    } => {
                        // Show thinking output dimmed so user can follow the reasoning
                        print!("{DIM}{delta}{RESET}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::AgentEnd { messages } => {
                        // Sum usage across ALL assistant messages in this turn
                        // (a single prompt can trigger multiple LLM calls via tool loops)
                        for msg in &messages {
                            if let AgentMessage::Llm(Message::Assistant { usage, stop_reason, error_message, .. }) = msg {
                                last_usage.input += usage.input;
                                last_usage.output += usage.output;
                                last_usage.cache_read += usage.cache_read;
                                last_usage.cache_write += usage.cache_write;

                                // Show error stop reasons to the user
                                if *stop_reason == StopReason::Error {
                                    if let Some(err_msg) = error_message {
                                        if in_text {
                                            println!();
                                            in_text = false;
                                        }
                                        eprintln!("\n{RED}  error: {err_msg}{RESET}");
                                    }
                                }
                            }
                        }
                    }
                    AgentEvent::InputRejected { reason } => {
                        eprintln!("{RED}  input rejected: {reason}{RESET}");
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        println!("{DIM}  {text}{RESET}");
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                // Cancel the agent's background work (tool execution, API calls)
                agent.abort();
                if in_text {
                    println!();
                }
                println!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}");
                break;
            }
        }
    }

    if in_text {
        println!();
    }
    session_total.input += last_usage.input;
    session_total.output += last_usage.output;
    session_total.cache_read += last_usage.cache_read;
    session_total.cache_write += last_usage.cache_write;
    print_usage(&last_usage, session_total, model, prompt_start.elapsed());
    println!();
    collected_text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_message_user() {
        let msg = AgentMessage::Llm(Message::user("hello world, this is a test"));
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "user");
        assert!(preview.contains("hello world"));
    }

    #[test]
    fn test_summarize_message_tool_result() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "output".into(),
            }],
            is_error: false,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("bash"));
        assert!(preview.contains("✓"));
    }

    #[test]
    fn test_summarize_message_tool_result_error() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_2".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "error".into(),
            }],
            is_error: true,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("✗"));
    }

    #[test]
    fn test_write_output_file_none() {
        write_output_file(&None, "test content");
        // No assertion needed — just verify it doesn't panic
    }

    #[test]
    fn test_write_output_file_some() {
        let dir = std::env::temp_dir().join("yoyo_test_output");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_output.txt");
        let path_str = path.to_string_lossy().to_string();
        write_output_file(&Some(path_str), "hello from yoyo");
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello from yoyo");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_tool_result_preview_empty() {
        let result = ToolResult {
            content: vec![],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "");
    }

    #[test]
    fn test_tool_result_preview_text() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "error: file not found".into(),
            }],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "error: file not found");
    }

    #[test]
    fn test_tool_result_preview_truncated() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "a".repeat(200),
            }],
            details: serde_json::json!(null),
        };
        let preview = tool_result_preview(&result, 50);
        assert!(preview.len() < 100);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn test_tool_result_preview_multiline() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "first line\nsecond line\nthird line".into(),
            }],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "first line");
    }
}
