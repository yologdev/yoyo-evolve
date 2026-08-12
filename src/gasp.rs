//! GASP recorder wiring (issue #683, steps 1–2).
//!
//! What exists here: a default-off `gasp` cargo feature, an env-gated open of
//! `yoagent::gasp::GaspRecorder`, a process-global holder installed once at
//! startup, and two tee helpers that route a prompt through
//! `Agent::prompt_with_sender` so run/tool events are actually recorded. All
//! four agent-start call sites in `src/prompt.rs` go through those helpers.
//!
//! Redaction is not optional here: recorded summaries land in a *shareable*
//! git repo, so the recorder is opened with `with_summarizer(redact_secrets)`
//! and every persisted tool-arg/output summary passes through it first.
//!
//! Two things that remain true and should not be overstated:
//! * CI does not build `--features gasp`, so nothing in this module (including
//!   `redact_secrets` and its tests) is exercised by the default test run —
//!   `cargo test --features gasp` has to be run by hand.
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
use yoagent::gasp::{GaspRecorder, GoalId, GoalRef};
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
