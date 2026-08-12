//! GASP recorder wiring — **build layer only** (issue #683, step 1).
//!
//! What exists here: a default-off `gasp` cargo feature and an env-gated open
//! of `yoagent::gasp::GaspRecorder`. What does **not** exist yet: any recording.
//! yoyo's prompt path calls `agent.prompt()`, which owns the event receiver;
//! feeding the recorder needs `agent.prompt_with_sender(...)` threaded through
//! that hot path, and that is a later task. So a session with the feature on
//! and the env set opens a recorder, reports it under `--verbose`, and records
//! **zero run/tool events**. Do not read this module as instrumentation that
//! works; read it as the socket instrumentation will later plug into.
//!
//! Everything is behind `#[cfg(feature = "gasp")]` (the whole module is
//! declared under that cfg in `main.rs`), so a plain `cargo build` / `cargo
//! test` — what yoyo runs while modifying itself — never compiles
//! `yoagent-state`.

use std::path::PathBuf;

use yoagent::gasp::{GaspRecorder, GoalId, GoalRef};

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
        Ok(recorder) => Some(recorder),
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
}
