//! #988 Q3: a sub-agent child cancelled by Ctrl-C must not be re-run on the
//! fallback model, and the parent's model must not be told it "failed".
//!
//! **The exact string, read from yoagent 0.24.2's source rather than the issue**
//! <!-- yoagent-version-claim: 0.24.2 -->: a cancelled run ends as
//! `StopReason::Aborted` (`agent_loop.rs`: `ProviderError::Cancelled` *or*
//! `cancel.is_cancelled()` turns the failed message into `Aborted`), and
//! `sub_agent.rs`'s `extract_error` returns the message's `error_message` —
//! `ProviderError::Cancelled` displays as `"Cancelled"`, and an empty one falls
//! back to the literal `"Cancelled"` — which `execute` wraps as
//! `ToolError::Failed("Sub-agent '{name}' failed: Cancelled")`. A hung tool
//! source returns the payload-less `ToolError::Cancelled` instead.
//!
//! **What the spelling cannot carry.** `agent_loop.rs` marks a run `Aborted`
//! whenever the token is cancelled, *whatever* the provider error was, so a
//! cancel that races a provider failure arrives as
//! `"Sub-agent 'sub_agent' failed: API error 404: model not found"` — the
//! spelling of a model-availability error. So the decorators key on the
//! MEANING, `ctx.cancel.is_cancelled()` (the parent's token, which `sub_agent.rs`
//! clones into the child), never on the word "Cancelled".
//!
//! **Raw readings before this fix (Day 220), recorded verbatim:**
//! - `classify_sub_agent_error("Sub-agent 'sub_agent' failed: Cancelled")` →
//!   `Unclassified`; `is_model_unavailable_error` → `false`; so the plain cancel
//!   was **not** re-run on the fallback (the fallback test below was green).
//! - the same error through `DiagnosticSubAgentTool` read:
//!   `Sub-agent 'sub_agent' failed: Cancelled\n\n[yoyo: the sub-agent failed —
//!   an ordinary failure of the delegated work — a different model will not
//!   help; model: \`claude-opus-5\`]` — a user's Ctrl-C dressed as a failure.
//! - a cancel racing a 404 **was** re-run on the fallback model (secondary
//!   called once) — the worst case #988 Q3 asked about, and it was real.
//!
//! Positive controls run (serially, restored in the same command): neutering
//! the fallback's token check reddens the two racing-404 rerun tests; neutering
//! the diagnostic's reddens the three "reads as cancelled" tests.

use crate::tool_wrappers::{
    classify_sub_agent_error, is_model_unavailable_error, sub_agent_cancelled_report,
    sub_agent_failure_report, DiagnosticSubAgentTool, FallbackSubAgentTool, SubAgentErrorClass,
    SubAgentOutputMarkerTool,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use yoagent::types::{AgentTool, ToolContext, ToolError, ToolResult};

/// The exact error yoagent 0.24.2 builds for a cancelled child named `sub_agent`.
const CANCELLED: &str = "Sub-agent 'sub_agent' failed: Cancelled";
/// A cancel that raced a provider failure: `Aborted`, with the provider's text.
const CANCEL_RACED_404: &str = "Sub-agent 'sub_agent' failed: API error 404: model not found";
const LABEL: &str = "`claude-opus-5`";

enum Reply {
    Failed(&'static str),
    Cancelled,
    Ok,
}

struct Stub {
    calls: Arc<AtomicUsize>,
    reply: Reply,
}

fn stub(reply: Reply) -> (Box<dyn AgentTool>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Box::new(Stub {
            calls: calls.clone(),
            reply,
        }),
        calls,
    )
}

#[async_trait::async_trait]
impl AgentTool for Stub {
    fn name(&self) -> &str {
        "sub_agent"
    }
    fn label(&self) -> &str {
        "sub_agent"
    }
    fn description(&self) -> &str {
        "cancel stub"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.reply {
            Reply::Failed(msg) => Err(ToolError::Failed(msg.to_string())),
            Reply::Cancelled => Err(ToolError::Cancelled),
            Reply::Ok => Ok(ToolResult {
                content: vec![yoagent::Content::Text {
                    text: "fallback answered".into(),
                }],
                details: serde_json::Value::Null,
            }),
        }
    }
}

/// A context whose token the parent has cancelled — what Ctrl-C (`agent.abort()`)
/// leaves behind by the time the child's `Err` comes back.
fn cancelled_ctx() -> ToolContext {
    let ctx = ToolContext::new("t", "sub_agent");
    ctx.cancel.cancel();
    ctx
}

fn live_ctx() -> ToolContext {
    ToolContext::new("t", "sub_agent")
}

fn fallback(primary: Reply) -> (FallbackSubAgentTool, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let (p, p_calls) = stub(primary);
    let (s, s_calls) = stub(Reply::Ok);
    (
        FallbackSubAgentTool::new(p, s, "dead-model", "live-model"),
        p_calls,
        s_calls,
    )
}

fn err_text(r: Result<ToolResult, ToolError>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected an error"),
    }
}

// --- the classifier: pinned exactly, in both directions ---

#[test]
fn classifier_rows_are_pinned() {
    // The cancel string is not a model/auth/rate-limit shape.
    assert_eq!(
        classify_sub_agent_error(CANCELLED),
        SubAgentErrorClass::Unclassified
    );
    assert!(!is_model_unavailable_error(CANCELLED));
    // Near misses: real provider errors classify exactly as before.
    assert_eq!(
        classify_sub_agent_error("API error 529: overloaded"),
        SubAgentErrorClass::RateLimit
    );
    assert_eq!(
        classify_sub_agent_error("API error 404: model not found"),
        SubAgentErrorClass::ModelUnavailable
    );
    // The racing cancel carries a model-unavailable SPELLING; the classifier
    // reads spelling, which is why the decorators must not ask it about a cancel.
    assert_eq!(
        classify_sub_agent_error(CANCEL_RACED_404),
        SubAgentErrorClass::ModelUnavailable
    );
}

// --- FallbackSubAgentTool: a cancel never reaches the fallback model ---

#[tokio::test]
async fn cancelled_child_is_not_rerun_on_the_fallback() {
    let (tool, p, s) = fallback(Reply::Failed(CANCELLED));
    let text = err_text(tool.execute(serde_json::json!({}), cancelled_ctx()).await);
    assert_eq!(p.load(Ordering::SeqCst), 1);
    assert_eq!(
        s.load(Ordering::SeqCst),
        0,
        "Ctrl-C must not re-run the child"
    );
    assert_eq!(text, CANCELLED, "returned verbatim");
}

#[tokio::test]
async fn cancel_racing_a_404_is_not_rerun_on_the_fallback() {
    // Red at HEAD: secondary was called once.
    let (tool, _p, s) = fallback(Reply::Failed(CANCEL_RACED_404));
    let text = err_text(tool.execute(serde_json::json!({}), cancelled_ctx()).await);
    assert_eq!(
        s.load(Ordering::SeqCst),
        0,
        "Ctrl-C must not re-run the child"
    );
    assert_eq!(text, CANCEL_RACED_404);
}

#[tokio::test]
async fn payloadless_cancel_is_not_rerun_on_the_fallback() {
    let (tool, _p, s) = fallback(Reply::Cancelled);
    let r = tool.execute(serde_json::json!({}), cancelled_ctx()).await;
    assert!(matches!(r, Err(ToolError::Cancelled)));
    assert_eq!(s.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn near_miss_live_404_still_reaches_the_fallback_once() {
    // The whole reason the fallback exists: an uncancelled 404 still switches.
    let (tool, _p, s) = fallback(Reply::Failed(CANCEL_RACED_404));
    let r = tool.execute(serde_json::json!({}), live_ctx()).await;
    assert!(r.is_ok());
    assert_eq!(s.load(Ordering::SeqCst), 1);
}

// --- DiagnosticSubAgentTool: the model is told "cancelled", not "failed" ---

#[test]
fn cancelled_report_wording_is_pinned() {
    assert_eq!(
        sub_agent_cancelled_report(LABEL, CANCELLED),
        "Sub-agent 'sub_agent' failed: Cancelled\n\n[yoyo: the sub-agent was cancelled \
         before it finished — the run was aborted (usually by the user pressing Ctrl-C). \
         This is not a failure of the delegated work and not a model problem; do not \
         re-run it unless the user asks; model: `claude-opus-5`]"
    );
}

#[tokio::test]
async fn cancelled_child_reads_as_cancelled_not_failed() {
    // Red at HEAD: read "[yoyo: the sub-agent failed — an ordinary failure ...]".
    let (inner, _) = stub(Reply::Failed(CANCELLED));
    let tool = DiagnosticSubAgentTool::new(inner, LABEL);
    let text = err_text(tool.execute(serde_json::json!({}), cancelled_ctx()).await);
    assert_eq!(text, sub_agent_cancelled_report(LABEL, CANCELLED));
    assert!(!text.contains("the sub-agent failed"), "{text}");
}

#[tokio::test]
async fn cancel_racing_a_404_reads_as_cancelled_not_model_unavailable() {
    let (inner, _) = stub(Reply::Failed(CANCEL_RACED_404));
    let tool = DiagnosticSubAgentTool::new(inner, LABEL);
    let text = err_text(tool.execute(serde_json::json!({}), cancelled_ctx()).await);
    assert_eq!(text, sub_agent_cancelled_report(LABEL, CANCEL_RACED_404));
    assert!(!text.contains("unavailable"), "{text}");
}

#[tokio::test]
async fn payloadless_cancel_passes_through_verbatim() {
    let (inner, _) = stub(Reply::Cancelled);
    let tool = DiagnosticSubAgentTool::new(inner, LABEL);
    let r = tool.execute(serde_json::json!({}), cancelled_ctx()).await;
    assert!(matches!(r, Err(ToolError::Cancelled)));
}

#[tokio::test]
async fn near_miss_live_errors_keep_todays_report_byte_identically() {
    // Uncancelled token: every row reads exactly what `sub_agent_failure_report`
    // said before #988 — including the inverse probe, a live error whose text
    // merely SAYS "Cancelled" (the predicate is the token, not the word).
    for msg in [
        "Sub-agent 'sub_agent' failed: API error 529: overloaded",
        "Sub-agent 'sub_agent' failed: API error 404: model not found",
        "Sub-agent 'sub_agent' failed: cargo build cancelled by timeout",
        CANCELLED,
    ] {
        let (inner, _) = stub(Reply::Failed(msg));
        let tool = DiagnosticSubAgentTool::new(inner, LABEL);
        let text = err_text(tool.execute(serde_json::json!({}), live_ctx()).await);
        assert_eq!(text, sub_agent_failure_report(LABEL, msg), "{msg}");
    }
}

// --- the stack as tools.rs builds it, at the outermost seam ---

#[tokio::test]
async fn full_stack_cancel_is_neither_rerun_nor_called_a_failure() {
    let (p, _) = stub(Reply::Failed(CANCEL_RACED_404));
    let (s, s_calls) = stub(Reply::Ok);
    let tool = SubAgentOutputMarkerTool::new(Box::new(DiagnosticSubAgentTool::new(
        Box::new(FallbackSubAgentTool::new(p, s, "dead-model", "live-model")),
        LABEL,
    )));
    let text = err_text(tool.execute(serde_json::json!({}), cancelled_ctx()).await);
    assert_eq!(s_calls.load(Ordering::SeqCst), 0);
    assert_eq!(text, sub_agent_cancelled_report(LABEL, CANCEL_RACED_404));
}
