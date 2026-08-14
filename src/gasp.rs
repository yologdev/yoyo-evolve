//! GASP recorder wiring (issue #683, steps 1–2) plus the *named half* of the
//! session-graph emission ported from the `tools/gasp-emit` sidecar (#683
//! item 5).
//!
//! What exists here: a default-off `gasp` cargo feature, an env-gated open of
//! `yoagent::gasp::GaspRecorder`, a process-global holder installed once at
//! startup, and two tee helpers that route a prompt through
//! `Agent::prompt_with_sender` so run/tool events are actually recorded. All
//! four agent-start call sites in `src/prompt.rs` go through those helpers.
//!
//! Ported from the sidecar, and **only these two**: `session-start`
//! (`session_start`) and `task` (`task_planned`), plus the `ensure_goal` helper
//! they share. `task-result` and `session-end` are **still only in
//! `tools/gasp-emit`** — they need `StatePatch` / `propose_patch` / `link` /
//! `EvalResult`. The ported pair ships **dormant**: nothing calls it yet, since
//! its consumers are #683 items (3)+(7) (the operator-lane env bridge). It
//! compiles, it unit-tests, it records nothing.
//!
//! Redaction is not optional here: recorded summaries land in a *shareable*
//! git repo, so the recorder is opened with `with_summarizer(redact_secrets)`
//! and every persisted tool-arg/output summary passes through it first.
//!
//! Two things that remain true and should not be overstated:
//! * The **default** `cargo test` run exercises nothing in this module — the
//!   feature is off, so it compiles into nothing. CI does run the featured pair
//!   (`cargo test --features gasp`, `cargo clippy --all-targets --features gasp`),
//!   but locally those have to be run by hand.
//! * Recording failures are logged by yoagent via `tracing` and the event
//!   stream keeps flowing; yoyo's UI does not surface them.
//!
//! Everything is behind `#[cfg(feature = "gasp")]` (the whole module is
//! declared under that cfg in `main.rs`), so a plain `cargo build` / `cargo
//! test` — what yoyo runs while modifying itself — never compiles
//! `yoagent-state`.

use std::path::PathBuf;
use std::sync::OnceLock;

use tokio::sync::mpsc;
use yoagent::gasp::{
    ActorRef, EventStore, GaspGoal, GaspRecorder, GoalId, GoalRef, NodeId, RunId, StateError, Task,
    TaskId, TaskStatus, YoAgentState,
};
use yoagent::{Agent, AgentEvent, AgentMessage, Content, Message};

/// Longest task label we stamp onto a recorded run. Display-only: the label is
/// a human handle for the run, not the run's content.
const TASK_LABEL_MAX_BYTES: usize = 120;

/// Env var naming the GASP agent-repo root to record into.
pub(crate) const STATE_DIR_ENV: &str = "YOYO_GASP_STATE_DIR";
/// Env var naming the existing goal id runs are chained to.
pub(crate) const GOAL_ID_ENV: &str = "YOYO_GASP_GOAL_ID";

/// The agent id stamped into recorded events.
const AGENT_ID: &str = "yoyo";
/// Worker id, deliberately distinct from the retiring sidecar's — mixed-writer
/// history is exactly what that field is for, so in-process runs must be
/// tellable apart from sidecar-written ones after the fact.
const WORKER_ID: &str = "yoyo-inproc";

/// What the two env vars ask for. Three explicit values, because "not asked
/// for" and "asked for incompletely" are different things and only one of them
/// deserves a warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecorderPlan {
    /// No state dir configured — the normal case on every machine that isn't
    /// the evolve runner. Silent: no warning, nothing created.
    Disabled,
    /// A state dir was configured but the goal id was not. The operator asked
    /// for recording and will not get it, so this one says so out loud.
    Misconfigured(String),
    /// Both present — try to open.
    Open { root: PathBuf, goal_id: String },
}

/// Pure decision: given the two env values, should we try, and with what?
///
/// Takes the values as parameters rather than reading the environment so it is
/// testable without mutating process-global state.
pub(crate) fn plan_from_env_values(root: Option<&str>, goal: Option<&str>) -> RecorderPlan {
    let root = root.map(str::trim).filter(|s| !s.is_empty());
    let goal = goal.map(str::trim).filter(|s| !s.is_empty());

    match (root, goal) {
        (None, _) => RecorderPlan::Disabled,
        (Some(_), None) => RecorderPlan::Misconfigured(format!(
            "{STATE_DIR_ENV} is set but {GOAL_ID_ENV} is not — no goal to chain runs to"
        )),
        (Some(root), Some(goal)) => RecorderPlan::Open {
            root: PathBuf::from(root),
            goal_id: goal.to_string(),
        },
    }
}

/// Carry out a plan. Never propagates, never panics, never aborts startup — a
/// session must not break because instrumentation did.
pub(crate) async fn open_recorder(plan: RecorderPlan) -> Option<GaspRecorder> {
    let (root, goal_id) = match plan {
        RecorderPlan::Disabled => return None,
        RecorderPlan::Misconfigured(reason) => {
            eprintln!("gasp: recording disabled — {reason}");
            return None;
        }
        RecorderPlan::Open { root, goal_id } => (root, goal_id),
    };

    // A configured-but-absent directory degrades silently: the env may be set
    // repo-wide while a given checkout has no store yet.
    if !root.is_dir() {
        return None;
    }

    match GaspRecorder::open(
        root.clone(),
        AGENT_ID,
        WORKER_ID,
        GoalRef::Existing(GoalId::new(goal_id)),
    )
    .await
    {
        // Redaction is applied at open, not at each call site, so there is no
        // path that persists a summary without passing through it.
        Ok(recorder) => Some(recorder.with_summarizer(crate::safety::redact_secrets)),
        Err(e) => {
            eprintln!(
                "gasp: recording disabled — opening store at {} failed: {e}",
                root.display()
            );
            None
        }
    }
}

/// Read the environment and open a recorder if it asks for one.
pub(crate) async fn open_recorder_from_env() -> Option<GaspRecorder> {
    let root = std::env::var(STATE_DIR_ENV).ok();
    let goal = std::env::var(GOAL_ID_ENV).ok();
    open_recorder(plan_from_env_values(root.as_deref(), goal.as_deref())).await
}

// ---------------------------------------------------------------------------
// Process-global holder
// ---------------------------------------------------------------------------

/// The recorder installed at startup, if any. `OnceLock` because a session
/// opens at most one store: a second install is a bug, not a reconfiguration,
/// so it is ignored rather than silently swapping the store mid-session.
static RECORDER: OnceLock<GaspRecorder> = OnceLock::new();

/// Install the process recorder. Called once, from `main`, after a successful
/// open. A second call is a no-op.
pub(crate) fn install(recorder: GaspRecorder) {
    let _ = RECORDER.set(recorder);
}

/// The installed recorder, or `None` when recording was never enabled — which
/// is the normal case on every machine that isn't the evolve runner.
fn recorder() -> Option<&'static GaspRecorder> {
    RECORDER.get()
}

/// A short, human-readable handle for a run, derived from the prompt.
///
/// Char-boundary safe via `format::safe_truncate` — never byte-indexed.
fn task_label(input: &str) -> String {
    let flat = input.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::format::safe_truncate(&flat, TASK_LABEL_MAX_BYTES).to_string()
}

/// Best-effort text of a prompt built from messages, for the task label only.
/// Non-text content (images, tool calls) contributes nothing.
fn messages_label(messages: &[AgentMessage]) -> String {
    let mut out = String::new();
    for msg in messages {
        if let AgentMessage::Llm(Message::User { content, .. }) = msg {
            for c in content {
                if let Content::Text { text } = c {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(text);
                }
            }
        }
    }
    task_label(&out)
}

/// Start a text prompt through the recorder, returning the *forwarded* event
/// stream so the caller sees exactly what it would have seen from
/// `agent.prompt`.
///
/// Returns `None` when no recorder is installed — the caller then falls back to
/// the plain `agent.prompt` path.
///
/// The recording `JoinHandle` is deliberately dropped (detached): the recording
/// task ends when yoagent drops the sender at loop end, and yoagent already
/// logs recording failures via `tracing` while continuing to forward events. A
/// recorder failure must never break the session, so there is nothing here to
/// await or propagate.
pub(crate) async fn tee_prompt(
    agent: &mut Agent,
    input: String,
) -> Option<mpsc::UnboundedReceiver<AgentEvent>> {
    let recorder = recorder()?;
    let (forward_tx, forward_rx) = mpsc::unbounded_channel();
    let (tx, _handle) = recorder.recording_sender(task_label(&input), Some(forward_tx));
    agent.prompt_with_sender(input, tx).await;
    Some(forward_rx)
}

/// Messages sibling of [`tee_prompt`] — same contract, same detached handle.
pub(crate) async fn tee_prompt_messages(
    agent: &mut Agent,
    messages: Vec<AgentMessage>,
) -> Option<mpsc::UnboundedReceiver<AgentEvent>> {
    let recorder = recorder()?;
    let (forward_tx, forward_rx) = mpsc::unbounded_channel();
    let (tx, _handle) = recorder.recording_sender(messages_label(&messages), Some(forward_tx));
    agent.prompt_messages_with_sender(messages, tx).await;
    Some(forward_rx)
}

// ---------------------------------------------------------------------------
// Session-graph emission — the ported half of `tools/gasp-emit` (#683 item 5)
//
// LANDED HERE: `session-start` and `task` (plus the `ensure_goal` helper they
// share). STILL ONLY IN `tools/gasp-emit`: `task-result` and `session-end` —
// those need `StatePatch` / `propose_patch` / `link` / `EvalResult`, which is
// where the rest of that surface's complexity lives. This half is a prefix of
// the sequence, not a replacement for the sidecar.
//
// Everything below ships DORMANT: nothing calls `session_start` /
// `task_planned` yet. Their consumers are #683 items (3)+(7), the operator-lane
// env bridge, which is deliberately not wired here — wiring it early would
// destroy the sidecar's session record.
// ---------------------------------------------------------------------------

/// The standing goal an evolve session serves when none is named. Copied
/// unchanged from `tools/gasp-emit/src/main.rs` — a goal id is a node identity
/// in a shared store, so a "nicer" string is a *different graph*.
const DEFAULT_GOAL: &str = "goal_self_improvement";
/// Title/summary for a session goal created on first reference.
const DEFAULT_GOAL_TITLE: &str = "Evolve: improve yoyo's own code, skills, and reliability";
const DEFAULT_GOAL_SUMMARY: &str = "the standing goal every evolve session serves; tasks and patches under it are the self-improvement ratchet";

/// The goal `--kind product` work is rerouted to, so the graph separates value
/// shipped to users from self-investment.
const PRODUCT_GOAL: &str = "goal_product_value";

/// The `--kind product` reroute, as a pure decision.
///
/// Any other kind (including the empty string) keeps the goal it was given.
fn goal_for_kind(kind: &str, goal: &str) -> String {
    if kind == "product" {
        PRODUCT_GOAL.to_string()
    } else {
        goal.to_string()
    }
}

/// Title used when a standing goal is created on first reference.
///
/// Byte-identical to the sidecar's strings on purpose — see [`DEFAULT_GOAL`].
fn standing_goal_title(goal: &str) -> &str {
    if goal == PRODUCT_GOAL {
        "Ship value to yoyo's users"
    } else {
        goal
    }
}

/// Summary sibling of [`standing_goal_title`]. Also byte-identical.
fn standing_goal_summary(goal: &str) -> &str {
    if goal == PRODUCT_GOAL {
        "value shipped to yoyo's product users — features, UX, and fixes they experience directly, independent of any single session"
    } else {
        "standing goal (created on first reference)"
    }
}

/// Node id of a planned task: `task_{run_id}_{num}`.
fn task_node_id(run_id: &str, num: &str) -> String {
    format!("task_{run_id}_{num}")
}

/// Label stamped on a session run when the caller names no task.
fn default_session_task_label(day: &str) -> String {
    format!("evolve session day {day}")
}

/// Which goal a session records under: an explicitly requested one wins, else
/// the goal the recorder was opened against, else [`DEFAULT_GOAL`].
fn session_goal<'a>(requested: Option<&'a str>, recorder_goal: &'a str) -> &'a str {
    let requested = requested.map(str::trim).filter(|s| !s.is_empty());
    let fallback = Some(recorder_goal.trim()).filter(|s| !s.is_empty());
    requested.or(fallback).unwrap_or(DEFAULT_GOAL)
}

/// Create a standing goal node if absent (used for `goal_product_value`, which
/// no session opens but product-kind tasks advance).
///
/// Ported verbatim from `tools/gasp-emit/src/main.rs:58-82`, minus its
/// `Box<dyn Error>` return: yoyo propagates the store's own error type.
async fn ensure_goal<S: EventStore>(
    state: &YoAgentState<S>,
    goal: &str,
    actor: &ActorRef,
) -> Result<(), StateError> {
    if state.get_node(NodeId::new(goal)).await.is_none() {
        state
            .record_goal(GaspGoal::new(
                GoalId::new(goal),
                standing_goal_title(goal),
                standing_goal_summary(goal),
                actor.clone(),
            ))
            .await?;
    }
    Ok(())
}

/// Record the start of an evolve session: create the session goal if it does
/// not exist yet, then open a run under it.
///
/// Takes the recorder rather than a store because a GASP repo is single-writer
/// behind a 600s lease — opening a second `GitEventStore` on the same root
/// collides with the recorder's rather than cooperating (measured Day 165, and
/// the whole reason the sidecar cannot run alongside).
// Dormant: the caller is #683 items (3)+(7) (the operator-lane env bridge),
// which cannot exist until the sidecar's session record is retired.
#[allow(dead_code)]
pub(crate) async fn session_start(
    recorder: &GaspRecorder,
    run_id: &str,
    goal: Option<&str>,
    goal_title: Option<&str>,
    goal_summary: Option<&str>,
    day: &str,
    task: Option<&str>,
) -> Result<(), StateError> {
    let goal = session_goal(goal, recorder.goal().as_str()).to_string();
    session_start_in(
        recorder.state(),
        recorder.actor(),
        run_id,
        &goal,
        goal_title,
        goal_summary,
        day,
        task,
    )
    .await
}

/// Store-generic body of [`session_start`], so the behaviour is testable
/// against a scratch store without a live recorder.
#[allow(clippy::too_many_arguments)]
async fn session_start_in<S: EventStore>(
    state: &YoAgentState<S>,
    actor: &ActorRef,
    run_id: &str,
    goal: &str,
    goal_title: Option<&str>,
    goal_summary: Option<&str>,
    day: &str,
    task: Option<&str>,
) -> Result<(), StateError> {
    // A session goal keeps its *configured* title/summary — unlike
    // `ensure_goal`, which is the on-first-reference fallback.
    if state.get_node(NodeId::new(goal)).await.is_none() {
        state
            .record_goal(GaspGoal::new(
                GoalId::new(goal),
                goal_title
                    .filter(|s| !s.is_empty())
                    .unwrap_or(DEFAULT_GOAL_TITLE),
                goal_summary
                    .filter(|s| !s.is_empty())
                    .unwrap_or(DEFAULT_GOAL_SUMMARY),
                actor.clone(),
            ))
            .await?;
    }
    let task = task
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_session_task_label(day));
    state
        .record_run_started(actor.clone(), RunId::new(run_id), task)
        .await?;
    Ok(())
}

/// Record a task planned inside a session, under the goal its `kind` selects.
// Dormant for the same reason as [`session_start`] — #683 items (3)+(7).
#[allow(dead_code)]
pub(crate) async fn task_planned(
    recorder: &GaspRecorder,
    run_id: &str,
    num: &str,
    title: &str,
    kind: &str,
    goal: Option<&str>,
) -> Result<(), StateError> {
    let goal = session_goal(goal, recorder.goal().as_str()).to_string();
    task_planned_in(
        recorder.state(),
        recorder.actor(),
        run_id,
        num,
        title,
        kind,
        &goal,
    )
    .await
}

/// Store-generic body of [`task_planned`].
#[allow(clippy::too_many_arguments)]
async fn task_planned_in<S: EventStore>(
    state: &YoAgentState<S>,
    actor: &ActorRef,
    run_id: &str,
    num: &str,
    title: &str,
    kind: &str,
    goal: &str,
) -> Result<(), StateError> {
    let goal = goal_for_kind(kind, goal);
    ensure_goal(state, &goal, actor).await?;
    state
        .record_task(Task {
            id: TaskId::new(task_node_id(run_id, num)),
            title: title.to_string(),
            summary: format!("planned as task {num} of the session"),
            status: TaskStatus::Open,
            goal: Some(GoalId::new(goal.as_str())),
            created_by: actor.clone(),
            metadata: serde_json::json!({ "kind": kind }),
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_env_is_disabled_silently() {
        assert_eq!(plan_from_env_values(None, None), RecorderPlan::Disabled);
        // A goal id alone is still not a request to record anywhere.
        assert_eq!(
            plan_from_env_values(None, Some("goal_abc")),
            RecorderPlan::Disabled
        );
    }

    #[test]
    fn empty_or_whitespace_root_is_disabled() {
        assert_eq!(
            plan_from_env_values(Some(""), Some("goal_abc")),
            RecorderPlan::Disabled
        );
        assert_eq!(
            plan_from_env_values(Some("   "), Some("goal_abc")),
            RecorderPlan::Disabled
        );
    }

    #[test]
    fn root_without_goal_is_misconfigured_not_disabled() {
        match plan_from_env_values(Some("/tmp/store"), None) {
            RecorderPlan::Misconfigured(reason) => {
                assert!(reason.contains(GOAL_ID_ENV), "reason names the missing var");
            }
            other => panic!("expected Misconfigured, got {other:?}"),
        }
        // Whitespace-only goal id is the same case, not a valid goal.
        assert!(matches!(
            plan_from_env_values(Some("/tmp/store"), Some("  ")),
            RecorderPlan::Misconfigured(_)
        ));
    }

    #[test]
    fn both_set_plans_an_open_with_trimmed_values() {
        assert_eq!(
            plan_from_env_values(Some(" /tmp/store "), Some(" goal_abc ")),
            RecorderPlan::Open {
                root: PathBuf::from("/tmp/store"),
                goal_id: "goal_abc".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn nonexistent_root_degrades_to_none() {
        let root = std::env::temp_dir().join("yoyo-gasp-does-not-exist-4f2a9c");
        assert!(!root.is_dir(), "test precondition: path must not exist");
        let plan = RecorderPlan::Open {
            root,
            goal_id: "goal_abc".to_string(),
        };
        assert!(open_recorder(plan).await.is_none());
    }

    #[tokio::test]
    async fn disabled_plan_opens_nothing() {
        assert!(open_recorder(RecorderPlan::Disabled).await.is_none());
    }

    #[test]
    fn task_label_truncates_on_a_char_boundary() {
        let long = "✓".repeat(500);
        let label = task_label(&long);
        assert!(label.len() <= TASK_LABEL_MAX_BYTES);
        // Would have panicked already if a byte index split the multi-byte char.
        assert!(label.chars().all(|c| c == '✓'));
        assert_eq!(task_label("  hello   world \n"), "hello world");
    }

    #[test]
    fn messages_label_reads_user_text_only() {
        let messages = vec![AgentMessage::Llm(Message::User {
            content: vec![
                Content::Image {
                    data: "ignored".into(),
                    mime_type: "image/png".into(),
                },
                Content::Text {
                    text: "fix the parser".into(),
                },
            ],
            timestamp: 0,
        })];
        assert_eq!(messages_label(&messages), "fix the parser");
        assert_eq!(messages_label(&[]), "");
    }
}
