//! #988 Q-unpaired: after a Ctrl-C lands while a tool is executing, does the
//! NEXT prompt send a `tool_use` with no `tool_result`?
//!
//! The REPL's cancel seam (`run_prompt_*` in `src/prompt.rs`) calls
//! `agent.abort()` and returns without draining; the next prompt's history is
//! whatever `agent.finish()` restores. These tests drive that exact sequence
//! against yoagent's `MockProvider` — abort while the tool runs, no drain,
//! `finish()`, then a second prompt — and read the history back.
//!
//! The second prompt is itself a check: `MockProvider` validates the transcript
//! it receives and panics on an unpaired `tool_use` (the shape a real provider
//! rejects with a 400). A panic inside the spawned loop leaves `finish()` with
//! stale state, so each test also asserts that the second turn's answer
//! actually arrived — a silent panic cannot pass as a clean run.
//!
//! Three tool shapes, because "a tool mid-cancel" is not one population:
//! a tool that honours `ctx.cancel`, a tool that ignores it and finishes late,
//! and yoyo's real `StreamingBashTool` running `sleep 30`.

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use yoagent::provider::mock::{MockResponse, MockToolCall};
use yoagent::provider::{MockProvider, ModelConfig};
use yoagent::types::{AgentTool, ToolContext, ToolError, ToolResult};
use yoagent::{Agent, AgentEvent, AgentMessage, Content, Message, StopReason};

const AFTER: &str = "SECOND_TURN_ANSWER_Q7";

/// How the stub tool behaves once started.
#[derive(Clone, Copy)]
enum Shape {
    /// Waits for `ctx.cancel`, then returns `ToolError::Cancelled`.
    HonoursCancel,
    /// Ignores `ctx.cancel`, finishes after a short sleep.
    IgnoresCancel,
    /// Returns immediately (the no-cancel near-miss).
    Instant,
}

struct SlowTool(Shape);

#[async_trait::async_trait]
impl AgentTool for SlowTool {
    fn name(&self) -> &str {
        "slow"
    }
    fn label(&self) -> &str {
        "slow"
    }
    fn description(&self) -> &str {
        "a tool that takes a while"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        match self.0 {
            Shape::HonoursCancel => {
                tokio::select! {
                    _ = ctx.cancel.cancelled() => Err(ToolError::Cancelled),
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {
                        Err(ToolError::Failed("stub was never cancelled".into()))
                    }
                }
            }
            Shape::IgnoresCancel => {
                tokio::time::sleep(Duration::from_millis(300)).await;
                Ok(text_result("slow finished anyway"))
            }
            Shape::Instant => Ok(text_result("slow finished")),
        }
    }
}

fn text_result(text: &str) -> ToolResult {
    ToolResult {
        content: vec![Content::Text { text: text.into() }],
        details: serde_json::Value::Null,
    }
}

fn tool_call(name: &str, args: serde_json::Value) -> MockToolCall {
    MockToolCall {
        name: name.into(),
        arguments: args,
        provider_metadata: None,
    }
}

fn agent_with(tool: Box<dyn AgentTool>, first: MockToolCall) -> Agent {
    let provider = MockProvider::new(vec![
        MockResponse::ToolCalls(vec![first]),
        MockResponse::Text(AFTER.into()),
    ]);
    Agent::from_provider(provider, ModelConfig::mock())
        .with_api_key("not-a-real-key")
        .with_tools(vec![tool])
}

/// Every `tool_use` id in the history, and every `tool_result` id.
fn call_and_result_ids(msgs: &[AgentMessage]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut calls = BTreeSet::new();
    let mut results = BTreeSet::new();
    for m in msgs {
        match m {
            AgentMessage::Llm(Message::Assistant { content, .. }) => {
                for c in content {
                    if let Content::ToolCall { id, .. } = c {
                        calls.insert(id.clone());
                    }
                }
            }
            AgentMessage::Llm(Message::ToolResult { tool_call_id, .. }) => {
                results.insert(tool_call_id.clone());
            }
            _ => {}
        }
    }
    (calls, results)
}

/// One line per message: role, block kinds, stop reason, ids — the raw reading.
fn describe(msgs: &[AgentMessage]) -> String {
    let mut out = String::new();
    for m in msgs {
        let line = match m {
            AgentMessage::Llm(Message::User { content, .. }) => {
                format!("user {:?}", kinds(content))
            }
            AgentMessage::Llm(Message::Assistant {
                content,
                stop_reason,
                ..
            }) => format!("assistant {:?} stop={stop_reason:?}", kinds(content)),
            AgentMessage::Llm(Message::ToolResult {
                tool_call_id,
                is_error,
                content,
                ..
            }) => format!(
                "toolResult id={tool_call_id} is_error={is_error} {:?}",
                texts(content)
            ),
            other => format!("other {other:?}"),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn kinds(content: &[Content]) -> Vec<String> {
    content
        .iter()
        .map(|c| match c {
            Content::Text { text } => format!("text({text:?})"),
            Content::ToolCall { id, name, .. } => format!("tool_use({name},{id})"),
            other => format!("{other:?}"),
        })
        .collect()
}

fn texts(content: &[Content]) -> Vec<String> {
    content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Turn 1: prompt, wait for the tool to START, abort (the REPL's Ctrl-C arm),
/// return without draining, `finish()`. Returns the restored history.
async fn cancel_mid_tool(agent: &mut Agent) -> Vec<AgentMessage> {
    let mut rx = agent.prompt("run the slow tool").await;
    let started = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(ev) = rx.recv().await {
            if matches!(ev, AgentEvent::ToolExecutionStart { .. }) {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        started,
        "the tool never started, so nothing was cancelled mid-tool"
    );
    // Give the tool a moment to be genuinely in flight.
    tokio::time::sleep(Duration::from_millis(50)).await;
    agent.abort();
    drop(rx); // the Ctrl-C arm returns without draining
    tokio::time::timeout(Duration::from_secs(20), agent.finish())
        .await
        .expect("finish() hung after abort");
    agent.messages().to_vec()
}

/// Turn 2: a fresh prompt, drained to the end. Returns the full history.
async fn second_turn(agent: &mut Agent) -> Vec<AgentMessage> {
    let mut rx = agent.prompt("are you still there?").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while rx.recv().await.is_some() {}
    })
    .await
    .expect("second turn hung");
    agent.finish().await;
    agent.messages().to_vec()
}

fn last_assistant_text(msgs: &[AgentMessage]) -> Option<String> {
    msgs.iter().rev().find_map(|m| match m {
        AgentMessage::Llm(Message::Assistant { content, .. }) => Some(texts(content).join("")),
        _ => None,
    })
}

/// The whole probe for one tool shape: cancel mid-tool, then a second turn.
/// Asserts every `tool_use` id is paired (full set equality, both turns) and
/// that the second turn's answer really arrived.
async fn probe(label: &str, tool: Box<dyn AgentTool>, first: MockToolCall) {
    let mut agent = agent_with(tool, first);
    let after_cancel = cancel_mid_tool(&mut agent).await;
    let reading = describe(&after_cancel);
    eprintln!("[{label}] after cancel + finish:\n{reading}");

    let (calls, results) = call_and_result_ids(&after_cancel);
    assert!(
        !calls.is_empty(),
        "[{label}] anti-vacuous: the cancelled turn must contain a tool_use, got:\n{reading}"
    );
    assert_eq!(
        calls, results,
        "[{label}] H: an unpaired tool_use after a mid-tool cancel:\n{reading}"
    );

    let after_second = second_turn(&mut agent).await;
    let reading2 = describe(&after_second);
    eprintln!("[{label}] after second prompt:\n{reading2}");
    let (calls2, results2) = call_and_result_ids(&after_second);
    assert_eq!(
        calls2, results2,
        "[{label}] the second request's history has an unpaired tool_use:\n{reading2}"
    );
    assert_eq!(
        last_assistant_text(&after_second).as_deref(),
        Some(AFTER),
        "[{label}] the second turn never answered (a transcript the provider \
         rejects panics inside the loop and leaves stale history):\n{reading2}"
    );
}

#[tokio::test]
async fn cancel_mid_tool_that_honours_cancel_leaves_every_tool_use_paired() {
    probe(
        "honours-cancel",
        Box::new(SlowTool(Shape::HonoursCancel)),
        tool_call("slow", serde_json::json!({})),
    )
    .await;
}

#[tokio::test]
async fn cancel_mid_tool_that_ignores_cancel_leaves_every_tool_use_paired() {
    probe(
        "ignores-cancel",
        Box::new(SlowTool(Shape::IgnoresCancel)),
        tool_call("slow", serde_json::json!({})),
    )
    .await;
}

#[tokio::test]
async fn cancel_mid_real_bash_sleep_leaves_every_tool_use_paired() {
    let bash = crate::tools::build_bash_tool(
        true,
        &crate::cli::PermissionConfig::default(),
        &Arc::new(AtomicBool::new(false)),
        None,
    );
    probe(
        "real-bash",
        Box::new(bash),
        tool_call("bash", serde_json::json!({"command": "sleep 30"})),
    )
    .await;
}

/// Near-miss: no cancel. The tool finishes, the model answers, and the history
/// is exactly user → assistant(tool_use) → toolResult → assistant(text).
#[tokio::test]
async fn no_cancel_history_is_the_ordinary_four_message_shape() {
    let mut agent = agent_with(
        Box::new(SlowTool(Shape::Instant)),
        tool_call("slow", serde_json::json!({})),
    );
    let msgs = second_turn(&mut agent).await;
    let (calls, results) = call_and_result_ids(&msgs);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls, results);
    let shape: Vec<&str> = msgs
        .iter()
        .map(|m| match m {
            AgentMessage::Llm(Message::User { .. }) => "user",
            AgentMessage::Llm(Message::Assistant { stop_reason, .. }) => match stop_reason {
                StopReason::ToolUse => "assistant:tool_use",
                StopReason::Stop => "assistant:stop",
                _ => "assistant:other",
            },
            AgentMessage::Llm(Message::ToolResult { .. }) => "toolResult",
            _ => "other",
        })
        .collect();
    assert_eq!(
        shape,
        ["user", "assistant:tool_use", "toolResult", "assistant:stop"]
    );
    assert_eq!(last_assistant_text(&msgs).as_deref(), Some(AFTER));
}

/// Positive control for the two checks `probe` relies on: hand the agent the
/// exact shape H describes — the measured post-cancel history with its
/// `toolResult` removed — and confirm both checks see it. The id-set check must
/// report the unpaired id, and the second turn must NOT produce its answer
/// (`MockProvider` rejects the transcript the way a real provider 400s). If
/// either check were blind, every "paired" verdict above would be vacuous.
#[tokio::test]
async fn positive_control_an_unpaired_tool_use_is_seen_by_both_checks() {
    let mut agent = agent_with(
        Box::new(SlowTool(Shape::HonoursCancel)),
        tool_call("slow", serde_json::json!({})),
    );
    let measured = cancel_mid_tool(&mut agent).await;
    let unpaired: Vec<AgentMessage> = measured
        .into_iter()
        .filter(|m| !matches!(m, AgentMessage::Llm(Message::ToolResult { .. })))
        .collect();

    let (calls, results) = call_and_result_ids(&unpaired);
    assert_eq!(calls.len(), 1, "fixture must keep its tool_use");
    assert!(results.is_empty(), "fixture must have no tool_result");
    assert_ne!(calls, results, "the id-set check must see the unpaired id");

    agent.replace_messages(unpaired);
    let after = second_turn(&mut agent).await;
    assert_ne!(
        last_assistant_text(&after).as_deref(),
        Some(AFTER),
        "a transcript with an unpaired tool_use must not reach a second answer:\n{}",
        describe(&after)
    );
}

/// A provider that never sends a token: it holds the stream open until the
/// run is cancelled, like a server stalled before its first byte.
struct StallsUntilCancelled;

#[async_trait::async_trait]
impl yoagent::provider::traits::StreamProvider for StallsUntilCancelled {
    async fn stream(
        &self,
        _config: yoagent::provider::traits::StreamConfig,
        _tx: tokio::sync::mpsc::UnboundedSender<yoagent::provider::traits::StreamEvent>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Message, yoagent::provider::traits::ProviderError> {
        cancel.cancelled().await;
        Err(yoagent::provider::traits::ProviderError::Cancelled)
    }
}

/// Ctrl-C while waiting for the model's first token, DRAINED to the end the
/// way the provider-retry and loop-guard aborts drain: returns the messages
/// `AgentEvent::AgentEnd` carries, which is exactly what `handle_agent_end`
/// receives. (A cancel mid-TOOL leaves no `Aborted` message at all, only the
/// `[Agent stopped: cancelled]` marker, measured with `describe` on Day 234.)
async fn cancel_while_waiting_for_the_model() -> Vec<AgentMessage> {
    let mut agent = Agent::from_provider(StallsUntilCancelled, ModelConfig::mock())
        .with_api_key("not-a-real-key");
    let mut rx = agent.prompt("hello").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    agent.abort();
    let end = tokio::time::timeout(Duration::from_secs(10), async {
        let mut end = None;
        while let Some(ev) = rx.recv().await {
            if let AgentEvent::AgentEnd { messages, .. } = ev {
                end = Some(messages);
            }
        }
        end
    })
    .await
    .expect("the drain hung after abort");
    agent.finish().await;
    end.expect("an aborted run must still send AgentEnd")
}

/// Hand a REAL aborted run's history (not a hand-typed message) to the same
/// `handle_agent_end` the drained event loops call, and name the verdict.
fn verdict_for(msgs: Vec<AgentMessage>) -> (&'static str, Option<String>) {
    let mut state = super::PromptEventState::new();
    if let Some(s) = state.spinner.take() {
        s.stop();
    }
    state.handle_agent_end(msgs, "mock");
    let fatal = state.fatal_error.clone();
    let label = match state.into_result() {
        super::PromptResult::Done { .. } => "Done",
        super::PromptResult::FatalError { .. } => "FatalError",
        super::PromptResult::RetriableError { .. } => "RetriableError",
        super::PromptResult::ContextOverflow { .. } => "ContextOverflow",
    };
    (label, fatal)
}

/// #988 Q1, pinned at the lowest seam. yoagent ≥0.23 ends a cancelled run as
/// `StopReason::Aborted`, not `Error("Cancelled")`. The REPL's Ctrl-C arm
/// returns without draining (so it never reads this stop), but the drained
/// paths do: the provider-retry abort (`handle_provider_retry` → `abort()`),
/// the loop guard, SIGTERM racing a drain. On every one of them an `Aborted`
/// stop must read as a finished turn — never an error, a retry or an
/// "incomplete" turn, which would print `error:` over a cancel the user asked
/// for. Probed on the built binary in a pty on Day 234 (transcripts on #988):
/// both Ctrl-C while waiting for the first token and Ctrl-C mid-`bash sleep`
/// printed the interrupt note and the next prompt was answered.
#[tokio::test]
async fn aborted_stop_from_a_real_cancel_reads_as_done_not_as_an_error() {
    let msgs = cancel_while_waiting_for_the_model().await;
    let reading = describe(&msgs);
    // Anti-vacuous: the fixture really carries the stop this test is about.
    let aborted = msgs
        .iter()
        .filter(|m| {
            matches!(
                m,
                AgentMessage::Llm(Message::Assistant {
                    stop_reason: StopReason::Aborted,
                    ..
                })
            )
        })
        .count();
    assert!(
        aborted >= 1,
        "a real cancel must leave an Aborted assistant message:\n{reading}"
    );

    assert_eq!(
        verdict_for(msgs.clone()),
        ("Done", None),
        "an Aborted stop must read as a finished turn:\n{reading}"
    );

    // Near-miss: the SAME history with Aborted relabelled Error must NOT read
    // as Done, so the verdict above is decided by the stop reason's arm and
    // not by something else in the history.
    let mut relabelled = msgs;
    for m in relabelled.iter_mut() {
        if let AgentMessage::Llm(Message::Assistant { stop_reason, .. }) = m {
            if *stop_reason == StopReason::Aborted {
                *stop_reason = StopReason::Error;
            }
        }
    }
    let (label, _) = verdict_for(relabelled);
    assert_ne!(
        label, "Done",
        "the same stop spelled Error must not be a clean Done"
    );
}
