//! gasp-emit — maps yoyo's evolve-session transitions onto the GASP event
//! vocabulary (github.com/yologdev/gasp) and appends them to the yoyo-gasp
//! state repo via yoagent-state's GitEventStore.
//!
//! Called from scripts/gasp_shim.sh at the transitions evolve.sh already has:
//!
//!   gasp-emit session-start --state-dir D --run-id R --day N --task DESC
//!   gasp-emit task          --state-dir D --run-id R --num N --title T
//!   gasp-emit task-result   --state-dir D --run-id R --num N --title T \
//!       --verdict promoted|rejected --pre-sha X --post-sha Y \
//!       --repo OWNER/NAME [--reason WHY]
//!   gasp-emit session-end   --state-dir D --run-id R --outcome TEXT
//!
//! session-end appends run.finished and makes the one boundary commit
//! (Run-Id/Goal/Outcome trailers); the shim pushes it AFTER the code push so
//! state never references unpushed commits.

use std::collections::HashMap;
use yoagent_state::{
    ActorRef, ArtifactRef, Decision, DecisionId, DecisionStatus, EvalId, EvalResult, EvalStatus,
    GitEventStore, Goal, GoalId, NodeId, PatchId, PatchStatus, ProjectRef, RunId, StatePatch,
    Task, TaskId, TaskStatus, YoAgentState,
};

const DEFAULT_GOAL: &str = "goal_self_improvement";
const DEFAULT_GOAL_TITLE: &str = "Evolve: improve yoyo's own code, skills, and reliability";
const DEFAULT_GOAL_SUMMARY: &str =
    "the standing goal every evolve session serves; tasks and patches under it are the self-improvement ratchet";

fn parse_args() -> (String, HashMap<String, String>) {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let mut flags = HashMap::new();
    let mut key: Option<String> = None;
    for arg in args {
        // Every flag takes exactly one value, so a pending flag consumes the
        // next token unconditionally — LLM-authored values starting with
        // `--` (task titles, revert reasons) stay values, not flags.
        if let Some(name) = key.take() {
            flags.insert(name, arg);
        } else if let Some(name) = arg.strip_prefix("--") {
            key = Some(name.to_string());
            flags.entry(name.to_string()).or_default();
        }
    }
    (cmd, flags)
}

fn req<'a>(flags: &'a HashMap<String, String>, name: &str) -> &'a str {
    flags
        .get(name)
        .map(String::as_str)
        .unwrap_or_else(|| panic!("missing --{name}"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (cmd, flags) = parse_args();
    if cmd.is_empty() || cmd == "--help" {
        eprintln!("usage: gasp-emit <session-start|task|task-result|session-end> --state-dir <yoyo-gasp> --run-id <id> ...");
        std::process::exit(2);
    }
    let state_dir = req(&flags, "state-dir");
    let run_id = RunId::new(req(&flags, "run-id"));
    let yoyo = ActorRef::agent("yoyo");
    // Per-session worker id (the shim passes evolve-shim-$PID) so overlapping
    // sessions with a pinned GASP_STATE_DIR cannot share a lease identity.
    let worker = flags
        .get("worker")
        .map(String::as_str)
        .unwrap_or("evolve-shim");
    // Sessions of different kinds serve different standing goals (evolve ->
    // goal_self_improvement, skill-evolve -> goal_skill_quality, ...).
    let goal = flags
        .get("goal")
        .map(String::as_str)
        .unwrap_or(DEFAULT_GOAL)
        .to_string();

    let store = GitEventStore::open(state_dir, worker)?;
    let state = YoAgentState::load(store.clone()).await?;
    // Each invocation is a fresh process: recover the open run so events
    // chain and correlate to it (yoagent-state 0.4.1+).
    if cmd != "session-start" {
        state.resume_open_run().await?;
    }

    match cmd.as_str() {
        "session-start" => {
            if state.get_node(NodeId::new(goal.as_str())).await.is_none() {
                state
                    .record_goal(Goal::new(
                        GoalId::new(goal.as_str()),
                        flags
                            .get("goal-title")
                            .map(String::as_str)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(DEFAULT_GOAL_TITLE),
                        flags
                            .get("goal-summary")
                            .map(String::as_str)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(DEFAULT_GOAL_SUMMARY),
                        yoyo.clone(),
                    ))
                    .await?;
            }
            let day = flags.get("day").cloned().unwrap_or_default();
            let task = flags
                .get("task")
                .cloned()
                .unwrap_or_else(|| format!("evolve session day {day}"));
            state.record_run_started(yoyo, run_id, task).await?;
        }

        "task" => {
            let num = req(&flags, "num");
            let title = req(&flags, "title");
            state
                .record_task(Task {
                    id: TaskId::new(format!("task_{}_{num}", run_id.as_str())),
                    title: title.to_string(),
                    summary: format!("planned as task {num} of the session"),
                    status: TaskStatus::Open,
                    goal: Some(GoalId::new(goal.as_str())),
                    created_by: yoyo,
                    metadata: serde_json::json!({}),
                })
                .await?;
        }

        "task-result" => {
            let num = req(&flags, "num");
            let title = req(&flags, "title");
            let verdict = req(&flags, "verdict");
            let pre_sha = req(&flags, "pre-sha");
            let post_sha = req(&flags, "post-sha");
            let repo = req(&flags, "repo");
            let reason = flags.get("reason").cloned().unwrap_or_default();
            let promoted = verdict == "promoted";
            let suffix = format!("{}_{num}", run_id.as_str());

            let mut patch = StatePatch::new(
                PatchId::new(format!("patch_{suffix}")),
                title,
                format!("commits {pre_sha}..{post_sha} in {repo}"),
                yoyo.clone(),
            );
            patch.base_project_ref = Some(ProjectRef {
                repo: repo.to_string(),
                branch: flags.get("branch").cloned(),
                commit: Some(pre_sha.to_string()),
                worktree: None,
            });
            patch.artifacts = vec![
                ArtifactRef::new("git-commit", format!("{repo}@{post_sha}")).with_hash(post_sha),
            ];
            let patch_id = state.propose_patch(patch).await?;
            state
                .link(
                    yoyo.clone(),
                    NodeId::new(format!("patch_{suffix}")),
                    "advances",
                    NodeId::new(goal.as_str()),
                )
                .await?;

            // the session's mechanical gate, folded into one eval fact — the
            // caller names its actual oracle so the record never overclaims
            let eval_command = flags
                .get("eval-command")
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("cargo fmt+clippy+build+test; evaluator agent");
            state
                .record_eval(
                    yoyo.clone(),
                    EvalResult {
                        id: EvalId::new(format!("eval_{suffix}")),
                        command: eval_command.into(),
                        status: if promoted { EvalStatus::Passed } else { EvalStatus::Failed },
                        score: Some(if promoted { 1.0 } else { 0.0 }),
                        metadata: serde_json::json!({ "reason": reason }),
                    },
                    Some(patch_id.clone()),
                )
                .await?;

            state
                .record_decision_node(
                    yoyo.clone(),
                    Decision {
                        id: DecisionId::new(format!("decision_{suffix}")),
                        status: if promoted { DecisionStatus::Approved } else { DecisionStatus::Rejected },
                        reason: if promoted {
                            format!("oracle passed ({eval_command}); kept")
                        } else {
                            format!("reverted to {pre_sha}: {reason}")
                        },
                        decided_by: yoyo.clone(),
                        metadata: serde_json::json!({}),
                    },
                    Some(NodeId::new(format!("patch_{suffix}"))),
                )
                .await?;
            state
                .update_patch_status(
                    patch_id,
                    if promoted { PatchStatus::Promoted } else { PatchStatus::Rejected },
                    (!reason.is_empty()).then(|| reason.clone()),
                )
                .await?;

            if !promoted {
                state
                    .record_failure(
                        yoyo.clone(),
                        NodeId::new(format!("failure_{suffix}")),
                        format!("task reverted: {title}"),
                        reason,
                    )
                    .await?;
                state
                    .link(
                        yoyo,
                        NodeId::new(format!("failure_{suffix}")),
                        "produced_by",
                        NodeId::new(run_id.as_str()),
                    )
                    .await?;
            }
        }

        "session-end" => {
            let outcome = flags.get("outcome").cloned().unwrap_or_else(|| "done".into());
            state
                .record_run_finished(yoyo, run_id.clone(), outcome.clone())
                .await?;
            // --extra: comma-separated repo-relative paths committed with the
            // boundary commit (e.g. mirrored skills/)
            let extra = flags.get("extra").cloned().unwrap_or_default();
            let extra_paths: Vec<&str> = extra
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect();
            let sha = store.commit_run(&run_id, &GoalId::new(goal.as_str()), &outcome, &extra_paths)?;
            store.release_lease()?;
            println!("gasp boundary commit: {}", sha.as_deref().unwrap_or("(nothing to commit)"));
        }

        other => {
            eprintln!("gasp-emit: unknown command `{other}`");
            std::process::exit(2);
        }
    }
    Ok(())
}
