//! #1159 item 2: every yoyo consumer of a `--context-strategy checkpoint`
//! stop, pinned against the shape yoagent 0.27.5 gives that stop.
//!
//! Checkpoint mode stops a run by returning `false` from `on_before_turn`
//! (`agent_builder.rs`, the `ContextStrategy::Checkpoint` arm). Since 0.27.5
//! that no longer just returns: the loop appends a `Message::User` whose text
//! is `"{AGENT_STOPPED_PREFIX} on_before_turn ended the run]"`, emits it as a
//! `MessageStart`/`MessageEnd` pair, keeps it in the history, and the run's
//! outcome is `Stopped` (agent_loop.rs `push_stop_marker`, ~l.1374).
//!
//! The rows below drive yoyo's real functions (`prompt::run_prompt`,
//! `answer_payload`, `build_json_output_with`, `exit_code_for_outcome`,
//! `save_session_to` + `restore_session_result`) over a run that really ends
//! that way: a stub provider whose first turn says something and calls a
//! tool, and an `on_before_turn` that stops before turn 1, which is where the
//! checkpoint threshold trips in practice (after a tool result grew the
//! context). What is NOT driven: the inline checkpoint closure itself (it
//! lives inside `build_agent` with no seam), so the stop here is a closure of
//! the same shape. It deliberately does not set the process-global
//! `CHECKPOINT_TRIGGERED` (main_tests.rs asserts its default), and passes the
//! flag's value to `exit_code_for_outcome` instead.
//! <!-- yoagent-version-claim: 0.27 -->

use crate::prompt::{self, PromptOutcome};
use crate::session::SessionChanges;
use tokio::sync::mpsc;
use yoagent::agent_loop::AGENT_STOPPED_PREFIX;
use yoagent::provider::traits::{ProviderError, StreamConfig, StreamEvent, StreamProvider};
use yoagent::provider::{MockProvider, ModelConfig};
use yoagent::types::{AgentTool, ToolContext, ToolError, ToolResult};
use yoagent::{Agent, AgentMessage, Content, Message, StopReason, Usage};

/// What the model said in the turn the checkpoint cut off after.
const REPLY: &str = "Reading the config before I change it.";
/// What a resumed session's next prompt gets back.
const RESUMED: &str = "RESUMED_AFTER_CHECKPOINT_Q4";

/// The exact marker 0.27.5 appends, built from yoagent's exported prefix so an
/// upstream rewording is a red test here, not a silent mismatch.
fn checkpoint_marker() -> String {
    format!("{AGENT_STOPPED_PREFIX} on_before_turn ended the run]")
}

/// Turn 0: one text block plus one tool call, streamed the way a real
/// provider streams them. Any later call would mean the stop did not stop.
struct TextThenToolCall;

#[async_trait::async_trait]
impl StreamProvider for TextThenToolCall {
    async fn stream(
        &self,
        _config: StreamConfig,
        tx: mpsc::UnboundedSender<StreamEvent>,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Message, ProviderError> {
        let _ = tx.send(StreamEvent::Start);
        let _ = tx.send(StreamEvent::TextDelta {
            content_index: 0,
            delta: REPLY.into(),
        });
        let _ = tx.send(StreamEvent::ToolCallStart {
            content_index: 1,
            id: "call-1".into(),
            name: "noop".into(),
        });
        let _ = tx.send(StreamEvent::ToolCallEnd { content_index: 1 });
        let message = Message::assistant(
            vec![
                Content::Text { text: REPLY.into() },
                Content::tool_call("call-1", "noop", serde_json::json!({})),
            ],
            StopReason::ToolUse,
            "mock",
            "mock",
            Usage::default(),
        );
        let _ = tx.send(StreamEvent::Done {
            message: message.clone(),
        });
        Ok(message)
    }
}

struct Noop;

#[async_trait::async_trait]
impl AgentTool for Noop {
    fn name(&self) -> &str {
        "noop"
    }
    fn label(&self) -> &str {
        "noop"
    }
    fn description(&self) -> &str {
        "does nothing"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            content: vec![Content::Text { text: "ok".into() }],
            details: serde_json::Value::Null,
        })
    }
}

/// Run one prompt through yoyo's real prompt path and stop it the way
/// checkpoint mode does: `on_before_turn` returns false before turn 1.
async fn checkpoint_stopped_run() -> (Agent, PromptOutcome) {
    let mut agent = Agent::from_provider(TextThenToolCall, ModelConfig::mock())
        .with_api_key("not-a-real-key")
        .with_tools(vec![Box::new(Noop)])
        .on_before_turn(|_messages, turn| turn == 0);
    let mut usage = Usage::default();
    let changes = SessionChanges::new();
    let outcome =
        prompt::run_prompt_with_changes(&mut agent, "fix the config", &mut usage, "mock", &changes)
            .await;
    agent.finish().await;
    (agent, outcome)
}

fn user_text(msg: &AgentMessage) -> Option<String> {
    match msg {
        AgentMessage::Llm(Message::User { content, .. }) => Some(
            content
                .iter()
                .filter_map(|c| match c {
                    Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// Positive control for every other row: the run really ends on the 0.27.5
/// marker, last in history, right after the tool result. If yoagent stops
/// appending it, the rows below would pass vacuously, so this goes red first.
#[tokio::test]
async fn checkpoint_stop_ends_history_on_the_0_27_5_marker() {
    let (agent, _) = checkpoint_stopped_run().await;
    let msgs = agent.messages();
    let last = msgs.last().expect("a history");
    assert_eq!(user_text(last), Some(checkpoint_marker()), "{msgs:?}");
    assert!(
        matches!(
            msgs[msgs.len() - 2],
            AgentMessage::Llm(Message::ToolResult { .. })
        ),
        "the marker follows the tool result: {msgs:?}"
    );
}

/// `-p` prints `answer_payload(&response.text)` and `--output-format json`
/// puts the same value under `"response"`. Both must be the model's words.
/// The marker reaches yoyo only as a `MessageStart`/`MessageEnd` pair with no
/// text delta, and `collected_text` is built from deltas alone, so it never
/// joins the answer.
#[tokio::test]
async fn print_and_json_carry_the_models_reply_not_the_marker() {
    let (_, outcome) = checkpoint_stopped_run().await;
    assert_eq!(outcome.text, REPLY);
    assert_eq!(crate::answer_payload(&outcome.text), REPLY);

    let json = crate::build_json_output_with(
        &outcome,
        "mock",
        &Usage::default(),
        false,
        &SessionChanges::new(),
        std::time::Duration::from_millis(1),
        1,
        &crate::agent_builder::ExternalServerReport::default(),
        &yoagent::SubAgentSpend::default(),
    );
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(v["response"], REPLY, "{json}");
    assert_eq!(v["is_error"], false, "{json}");
    assert!(v["incomplete"].is_null(), "{json}");
    assert!(!json.contains(AGENT_STOPPED_PREFIX), "{json}");
}

/// The exit ladder: a checkpoint stop is exit 2. Nothing in the stopped run
/// may read as a failure or an incomplete reply, or the ladder's earlier
/// rungs would turn it into a 1. `last_api_error`, `last_tool_error` and
/// `incomplete` are what `run_single_prompt`/`run_piped_mode` feed it.
#[tokio::test]
async fn checkpoint_stop_still_exits_2() {
    let (_, outcome) = checkpoint_stopped_run().await;
    assert_eq!(outcome.last_api_error, None);
    assert_eq!(outcome.last_tool_error, None);
    assert_eq!(outcome.incomplete, None);
    let reply_incomplete = prompt::incomplete_forces_failure(outcome.incomplete);
    assert_eq!(
        crate::exit_code_for_outcome(false, false, reply_incomplete, true),
        2
    );
    // Near-miss: the same run without the checkpoint flag is a plain 0.
    assert_eq!(
        crate::exit_code_for_outcome(false, false, reply_incomplete, false),
        0
    );
}

/// `--save-session` after a checkpoint stop, then `--continue`: the marker is
/// KEPT (yoyo strips nothing), so the resumed transcript has two user messages
/// in a row (marker, next prompt). Pinned as what happens, with its evidence:
/// yoagent's `MockProvider` validates transcripts and accepts it, and its
/// Anthropic provider sends consecutive user turns as they are (its own test
/// `build_request_body` fixture has two in a row), which Anthropic documents
/// combining into one turn. A real provider round trip is NOT run here.
#[tokio::test]
async fn saved_checkpoint_session_restores_and_the_next_prompt_answers() {
    let (agent, _) = checkpoint_stopped_run().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("session.json");
    crate::save_session_to(&agent, &path).expect("save");

    let mut resumed = Agent::from_provider(MockProvider::text(RESUMED), ModelConfig::mock())
        .with_api_key("not-a-real-key")
        .with_tools(vec![Box::new(Noop)]);
    let restored = crate::restore_session_result(&mut resumed, &path).expect("restore");
    assert_eq!(restored, agent.messages().len());
    assert_eq!(
        user_text(resumed.messages().last().expect("restored history")),
        Some(checkpoint_marker()),
        "the marker survives the save/restore round trip"
    );

    let mut usage = Usage::default();
    let outcome = prompt::run_prompt(&mut resumed, "carry on", &mut usage, "mock").await;
    resumed.finish().await;
    assert_eq!(outcome.text, RESUMED);
    assert_eq!(outcome.last_api_error, None);

    let msgs = resumed.messages();
    let n = msgs.len();
    assert_eq!(user_text(&msgs[n - 3]), Some(checkpoint_marker()));
    assert_eq!(user_text(&msgs[n - 2]).as_deref(), Some("carry on"));
}

/// The REPL's one stop line is the checkpoint closure's own `⚡ Context at N%
/// — checkpoint-restart triggered` (stderr, agent_builder.rs). The marker must
/// not add a second: the prompt loop renders only text deltas, and the
/// marker's `MessageEnd` writes a newline only while text is open. What the
/// loop collected for display (the whole turn and the closing text the
/// auto-continue gate reads) is the model's reply with no marker in it.
#[tokio::test]
async fn the_marker_is_never_rendered_as_a_second_stopped_line() {
    let (_, outcome) = checkpoint_stopped_run().await;
    assert!(!outcome.text.contains(AGENT_STOPPED_PREFIX));
    assert!(!outcome.text_since_last_tool.contains(AGENT_STOPPED_PREFIX));
    assert!(!outcome.text.contains("on_before_turn"));
}

/// yoyo's only `"[Agent stopped:"` classifier is the sub-agent partial-result
/// note (`tool_wrappers::sub_agent_partial_notice`), which reads a sub-agent's
/// returned TEXT. Checkpoint mode is registered on the main agent only, and
/// the main agent's reply text never contains the marker (row above), so the
/// classifier never sees it. Pinned at the classifier: the reply is not
/// misread as partial, while the marker text itself would be (the control).
#[tokio::test]
async fn checkpoint_reply_is_not_classified_as_a_stopped_sub_agent() {
    let (_, outcome) = checkpoint_stopped_run().await;
    assert_eq!(
        crate::tool_wrappers::sub_agent_partial_notice(&outcome.text),
        None
    );
    assert!(crate::tool_wrappers::sub_agent_partial_notice(&checkpoint_marker()).is_some());
}

// ── #1161: auto-continue never undoes a checkpoint stop ──
//
// `--context-strategy checkpoint` is the user's explicit choice of
// stop-and-restart over compaction. Auto-continue (on by default in the REPL,
// opt-in for piped mode via `--continue-on-silence`) used to send "Continue
// with the remaining work" right after the stop, so the next turn ran on and
// compacted at the same threshold: the default the user opted out of.

/// The real stopped run's outcome is exactly the shape auto-continue's silence
/// branch fires on (a tool ran, nothing said after it), so without the gate it
/// WOULD be continued. That makes the blocked rows below non-vacuous.
#[tokio::test]
async fn a_real_checkpoint_stop_would_otherwise_be_auto_continued() {
    let (_, outcome) = checkpoint_stopped_run().await;
    let wants = crate::repl::should_auto_continue(
        &outcome.text,
        &outcome.text_since_last_tool,
        0,
        true,
        true,
    );
    assert!(
        wants,
        "precondition: the silence branch should want to continue this turn \
         (text={:?}, closing={:?})",
        outcome.text, outcome.text_since_last_tool
    );
    assert!(!crate::repl::auto_continue_allowed(true, wants));
}

#[test]
fn repl_checkpoint_stop_blocks_auto_continue_even_with_work_pending() {
    // Heuristic says incomplete, and the follow-up queue is non-empty.
    let wants =
        crate::repl::should_auto_continue("Now let me also update", "Now let me", 2, true, false);
    assert!(wants, "precondition: this turn looks unfinished");
    assert!(!crate::repl::auto_continue_allowed(true, wants));
}

#[test]
fn repl_without_a_checkpoint_stop_auto_continue_is_unchanged() {
    for wants in [true, false] {
        assert_eq!(crate::repl::auto_continue_allowed(false, wants), wants);
    }
    // A checkpoint stop never turns "nothing pending" into a continue.
    assert!(!crate::repl::auto_continue_allowed(true, false));
}

#[test]
fn piped_continue_on_silence_refuses_after_a_checkpoint_stop() {
    // Every other condition says continue.
    assert!(crate::piped_should_continue(
        true, 0, 5, false, false, true, false
    ));
    assert!(!crate::piped_should_continue(
        true, 0, 5, false, false, true, true
    ));
}

#[test]
fn checkpoint_skip_note_says_why() {
    let note = crate::repl::CHECKPOINT_SKIPS_AUTO_CONTINUE;
    assert!(note.contains("auto-continue skipped"), "{note}");
    assert!(note.contains("checkpoint"), "{note}");
}
