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
//! Ported from the sidecar, and **only these three arms**: `session-start`
//! (`session_start`), `task` (`task_planned`) and `session-end`
//! (`session_end`), plus the `ensure_goal` helper the first two share.
//! `task-result` is **still only in `tools/gasp-emit`** — unported, but **not
//! blocked**. Read that as two separate facts, because conflating them cost
//! five sessions.
//!
//! <!-- yoagent-version-claim: 0.16.5 -->
//!
//! This paragraph used to say `task-result` was *unreachable* from yoyo,
//! because it names `ProjectRef`, `ArtifactRef` and `PatchStatus` and "none of
//! which appear anywhere in the published yoagent 0.16.3 source". That claim
//! was **true against 0.16.3 and went stale** when the pin moved: `Cargo.lock`
//! now resolves yoagent **0.16.5**, and all three types are re-exported from
//! `yoagent::gasp` there, so they are nameable as `yoagent::gasp::ProjectRef`
//! and friends. It is left recorded rather than silently deleted because the
//! stale sentence is *why* #765, #782, #785, #787 and #789 each opened this
//! file, believed an authoritative "impossible", and exited without a diff.
//!
//! How that was re-checked this session, so the next reader can redo it in ten
//! seconds instead of trusting this sentence:
//! ```text
//! $ grep -A1 'name = "yoagent"' Cargo.lock            → version = "0.16.5"
//! $ R=~/.cargo/registry/src/index.crates.io-*/
//! $ grep -rn 'ProjectRef' $R/yoagent-0.16.5/src/      → src/gasp.rs:72 (pub use)
//! ```
//! Counts: each of the three appears in **1** file of `yoagent-0.16.5/src/` —
//! the `pub use yoagent_state::{…}` re-export block at `src/gasp.rs:68-76`,
//! which is the fact that matters — and they are *defined* in
//! `yoagent-state-0.4.1` (`ProjectRef` 3 files, `ArtifactRef` 7,
//! `PatchStatus` 4). A combined count across both crates is therefore 4/8/5;
//! both denominators are named here so a future re-check that greps only one
//! crate does not read its own result as a contradiction.
//!
//! The marker line above is pinned to `Cargo.lock` by
//! `tests/gasp_doc_version.rs`, which fails if the pin moves without this
//! analysis being re-run. That guard is the reason this correction cannot go
//! stale the way the last one did.
//!
//! What is still true: `task-result` is **not ported**, and porting it is out
//! of scope here. It needs `patch.base_project_ref`, `patch.artifacts` and
//! `update_patch_status(…, PatchStatus::Promoted, …)`
//! (`tools/gasp-emit/src/main.rs:166-268`). `session_end` is the odd one of
//! the three: it has no store-generic `*_in` body, because `commit_run` /
//! `release_lease` are `GitEventStore` *inherent* methods rather than
//! `EventStore` trait methods, so it cannot be driven against a scratch store
//! and is compile-tested only (its two pure halves are table-tested). All
//! three ship **dormant**: nothing calls them yet, since their consumers are
//! #683 items (3)+(7) (the operator-lane env bridge). They compile, they
//! unit-test, they record nothing.
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
// LANDED HERE: `session-start`, `task` (plus the `ensure_goal` helper they
// share) and, since Day 168, `session-end`. STILL ONLY IN `tools/gasp-emit`:
// `task-result` — UNPORTED, but **NOT BLOCKED**. Read those as two facts.
//
// This comment used to say `task-result` was *unreachable*, because it names
// `ProjectRef`, `ArtifactRef` and `PatchStatus` and "none of which appear
// anywhere in the published yoagent 0.16.3 source (grepped, no matches)". That
// was true against 0.16.3 and went stale when the pin moved: `Cargo.lock` now
// resolves yoagent 0.16.5, where all three are re-exported from
// `yoagent::gasp`. Re-check in ten seconds rather than trusting this sentence:
//
//   $ grep -A1 'name = "yoagent"' Cargo.lock   → version = "0.16.5"
//   $ grep -rn 'ProjectRef' ~/.cargo/registry/src/*/yoagent-0.16.5/src/
//
// (each of the three appears in the `pub use yoagent_state::{…}` re-export
// block of `yoagent-0.16.5/src/gasp.rs`; they are *defined* in
// `yoagent-state-0.4.1`, so a grep across both crates counts 4/8/5 files.)
// The stale sentence is recorded rather than deleted because it is *why*
// #765, #782, #785, #787 and #789 each opened this file, believed an
// authoritative "impossible", and exited without a diff. The corrected claim
// carries the `yoagent-version-claim` marker in the module doc above, pinned
// to `Cargo.lock` by `tests/gasp_doc_version.rs`.
//
// This half is a prefix of the sequence, not a replacement for the sidecar.
//
// Everything below ships DORMANT: nothing calls `session_start` /
// `task_planned` / `session_end` yet. Their consumers are #683 items (3)+(7),
// the operator-lane env bridge, which is deliberately not wired here — wiring
// it early would destroy the sidecar's session record.
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

/// Outcome recorded for a session that names none. The sidecar's
/// `unwrap_or_else(|| "done".into())`, named rather than inlined.
const DEFAULT_SESSION_OUTCOME: &str = "done";

/// Split the comma-separated `--extra` list of repo-relative projection paths
/// committed alongside the run boundary.
///
/// Byte-identical to the sidecar's parse (`tools/gasp-emit/src/main.rs:277-282`):
/// same three operations, same order. Not "improved" on purpose — a
/// differently-parsed path list writes a *different boundary commit*.
fn parse_extra_paths(extra: &str) -> Vec<&str> {
    extra
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// Which outcome a finished session records: an explicitly requested one wins,
/// else [`DEFAULT_SESSION_OUTCOME`]. Same shape as [`session_goal`] — absence
/// gets its own name instead of being absorbed by an inline `unwrap_or`.
fn session_outcome(requested: Option<&str>) -> &str {
    requested
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_SESSION_OUTCOME)
}

/// Record the end of an evolve session: finish the open run, write the
/// run-boundary commit, and release the repo lease.
///
/// Ported from the sidecar's `"session-end"` arm
/// (`tools/gasp-emit/src/main.rs:269-288`). Two deliberate differences from
/// the sidecar:
///
/// 1. It returns the boundary sha instead of printing it. The sidecar printed
///    because it was a CLI; in-process the caller decides what to say.
/// 2. Unlike [`session_start`] and [`task_planned`] there is **no**
///    store-generic `*_in` body: `commit_run` and `release_lease` are
///    `GitEventStore` inherent methods, not `EventStore` trait methods, so a
///    generic body is not expressible here. That is a property of the upstream
///    API, not an oversight — which is also why this function is
///    compile-and-unit-tested only: exercising it needs a live
///    `GitEventStore` and takes a 600s repo lease. Its two decision halves
///    ([`parse_extra_paths`], [`session_outcome`]) are pure and table-tested.
///
/// Takes the recorder for the same reason its siblings do: a GASP repo is
/// single-writer behind that lease, so opening a second `GitEventStore` on the
/// same root collides with the recorder's rather than cooperating.
// Dormant: the caller is #683 items (3)+(7) (the operator-lane env bridge),
// which cannot exist until the sidecar's session record is retired.
#[allow(dead_code)]
pub(crate) async fn session_end(
    recorder: &GaspRecorder,
    run_id: &str,
    outcome: Option<&str>,
    goal: Option<&str>,
    extra: &str,
) -> Result<Option<String>, StateError> {
    let goal = session_goal(goal, recorder.goal().as_str()).to_string();
    let outcome = session_outcome(outcome);
    recorder
        .state()
        .record_run_finished(
            recorder.actor().clone(),
            RunId::new(run_id),
            outcome.to_string(),
        )
        .await?;
    let sha = recorder.store().commit_run(
        &RunId::new(run_id),
        &GoalId::new(goal.as_str()),
        outcome,
        &parse_extra_paths(extra),
    )?;
    recorder.store().release_lease()?;
    Ok(sha)
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

    // -----------------------------------------------------------------------
    // Pure decision helpers of the ported half (#683 item 5).
    //
    // These assert *verbatim literals*, never the module's own consts: the
    // point is that a node identity in a shared store still matches the
    // sidecar's (`tools/gasp-emit/src/main.rs:66-79,155`). Comparing a helper
    // against the const it returns would pass through any rewording and pin
    // nothing.
    // -----------------------------------------------------------------------

    #[test]
    fn goal_for_kind_reroutes_only_product() {
        let cases: &[(&str, &str, &str)] = &[
            // (kind, passed goal, expected goal)
            ("product", "goal_self_improvement", "goal_product_value"),
            ("product", "goal_skill_quality", "goal_product_value"),
            ("evolve", "goal_self_improvement", "goal_self_improvement"),
            ("", "goal_self_improvement", "goal_self_improvement"),
            // Near-misses keep their goal: the sidecar compares for equality.
            ("Product", "goal_self_improvement", "goal_self_improvement"),
            ("product ", "goal_self_improvement", "goal_self_improvement"),
            ("products", "goal_self_improvement", "goal_self_improvement"),
            // An already-product goal survives a non-product kind unchanged.
            ("evolve", "goal_product_value", "goal_product_value"),
        ];
        for (kind, goal, expected) in cases {
            assert_eq!(
                goal_for_kind(kind, goal),
                *expected,
                "goal_for_kind({kind:?}, {goal:?})"
            );
        }
    }

    #[test]
    fn standing_goal_title_matches_the_sidecar_byte_for_byte() {
        let cases: &[(&str, &str)] = &[
            ("goal_product_value", "Ship value to yoyo's users"),
            // Every other goal is titled with its own id.
            ("goal_self_improvement", "goal_self_improvement"),
            ("goal_skill_quality", "goal_skill_quality"),
            ("", ""),
        ];
        for (goal, expected) in cases {
            assert_eq!(standing_goal_title(goal), *expected, "title of {goal:?}");
        }
    }

    #[test]
    fn standing_goal_summary_matches_the_sidecar_byte_for_byte() {
        let cases: &[(&str, &str)] = &[
            (
                "goal_product_value",
                "value shipped to yoyo's product users — features, UX, and fixes they experience directly, independent of any single session",
            ),
            (
                "goal_self_improvement",
                "standing goal (created on first reference)",
            ),
            ("", "standing goal (created on first reference)"),
        ];
        for (goal, expected) in cases {
            assert_eq!(
                standing_goal_summary(goal),
                *expected,
                "summary of {goal:?}"
            );
        }
    }

    #[test]
    fn task_node_id_has_the_sidecar_shape() {
        let cases: &[(&str, &str, &str)] = &[
            ("run_42", "01", "task_run_42_01"),
            ("run_42", "2", "task_run_42_2"),
            // Neither half is normalised — the sidecar interpolates verbatim.
            ("", "", "task__"),
            ("day-167-0252", "10", "task_day-167-0252_10"),
        ];
        for (run_id, num, expected) in cases {
            assert_eq!(
                task_node_id(run_id, num),
                *expected,
                "task_node_id({run_id:?}, {num:?})"
            );
        }
    }

    #[test]
    fn default_session_task_label_matches_the_sidecar() {
        let cases: &[(&str, &str)] = &[
            ("167", "evolve session day 167"),
            // The sidecar falls back to an empty --day, and so do we.
            ("", "evolve session day "),
        ];
        for (day, expected) in cases {
            assert_eq!(
                default_session_task_label(day),
                *expected,
                "label for day {day:?}"
            );
        }
    }

    #[test]
    fn session_goal_prefers_request_then_recorder_then_default() {
        let cases: &[(Option<&str>, &str, &str)] = &[
            // (requested, recorder's goal, expected)
            (Some("goal_skill_quality"), "goal_x", "goal_skill_quality"),
            (
                Some("  goal_skill_quality  "),
                "goal_x",
                "goal_skill_quality",
            ),
            // A blank request is absence, not a goal named "".
            (Some(""), "goal_x", "goal_x"),
            (Some("   "), "goal_x", "goal_x"),
            (None, "goal_x", "goal_x"),
            (None, "  goal_x  ", "goal_x"),
            // Both blank: the standing default, never an empty node id.
            (None, "", "goal_self_improvement"),
            (Some("  "), "   ", "goal_self_improvement"),
        ];
        for (requested, recorder_goal, expected) in cases {
            assert_eq!(
                session_goal(*requested, recorder_goal),
                *expected,
                "session_goal({requested:?}, {recorder_goal:?})"
            );
        }
    }

    #[test]
    fn parse_extra_paths_matches_the_sidecar_parse() {
        let cases: &[(&str, &[&str])] = &[
            // No paths at all: an empty list, never a vec holding "".
            ("", &[]),
            (",,", &[]),
            ("  ", &[]),
            // The ordinary shapes.
            ("skills", &["skills"]),
            ("a,b", &["a", "b"]),
            // Whitespace around each element is trimmed, like the sidecar's
            // `map(str::trim)`.
            (" a , b ", &["a", "b"]),
            ("\ta\n,b", &["a", "b"]),
            // Empty elements between real ones are dropped, not kept as "".
            ("a,,b", &["a", "b"]),
            (",a,", &["a"]),
        ];
        for (input, expected) in cases {
            assert_eq!(
                parse_extra_paths(input),
                expected.to_vec(),
                "parse_extra_paths({input:?})"
            );
        }
    }

    #[test]
    fn session_outcome_names_absence_rather_than_absorbing_it() {
        // Absence, and every shape that is absence wearing a string.
        assert_eq!(session_outcome(None), "done");
        assert_eq!(session_outcome(Some("")), "done");
        assert_eq!(session_outcome(Some("   ")), "done");
        assert_eq!(session_outcome(None), DEFAULT_SESSION_OUTCOME);
        // A real outcome survives, trimmed.
        assert_eq!(session_outcome(Some("reverted")), "reverted");
        assert_eq!(session_outcome(Some(" reverted ")), "reverted");
        assert_eq!(session_outcome(Some("done")), "done");
    }
}
