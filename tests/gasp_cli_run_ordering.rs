//! #831 — a four-call `yoyo gasp` session must not interrupt its own run.
//!
//! `scripts/gasp_shim.sh` emits **one event per process**: `session-start`,
//! then `task`, then `task-result`, then `session-end`, as four separate
//! short-lived invocations. Until #831 the CLI arms opened a `GaspRecorder`,
//! and yoagent 0.16.6's `GaspRecorder::with_store` closes any run a previous
//! process left open as `"interrupted"` on **every** open — so call 2 killed
//! the run call 1 started.
//!
//! # Why this test spawns processes instead of calling the arms
//!
//! The defect does not exist inside one process. A single-process test that
//! called all four arms against one handle would pass on the broken code,
//! because the interrupt happens *at open*. The shape under test is
//! four opens, so this drives the real binary four times.
//!
//! yoyo is a binary-only crate (there is no `src/lib.rs`), so an integration
//! test cannot call `crate::gasp::*` directly in any case — `CARGO_BIN_EXE_yoyo`
//! is the reachable door, and it is also the *honest* one: it exercises the
//! same argv `scripts/gasp_shim.sh` passes.
//!
//! # Why it asserts ordering and not success
//!
//! #831's measured table has both columns emitting all the same event kinds.
//! A test that checked exit codes, or merely that each kind is present,
//! **passes on the broken behaviour**. Only the order differs: broken, the
//! interrupt's `run.finished` lands third and `task.created` lands *after* it;
//! fixed, `run.finished` is last. So the assertions are on position.
//!
//! Verified against the defect rather than only in the passing direction: with
//! `GaspRecorder::with_store` reintroduced into `open_graph_session`, this
//! repo's own four-call sequence produced
//! `[goal.created, run.started, run.finished, task.created, patch.proposed,
//! eval.finished, decision.created, patch.status_changed, failure.observed]`
//! and `session-end` failed with `cannot finish <run>: no run is open`,
//! committing no boundary. With the fix, `run.finished` is last and the
//! boundary sha is printed.
//!
//! # Stated limit
//!
//! The whole file is `#![cfg(feature = "gasp")]`, so a **plain** `cargo test`
//! compiles it to zero tests — the same way `src/gasp.rs` compiles to nothing
//! there. It runs under `cargo test --features gasp`, which CI does run
//! (`.github/workflows/ci.yml`). A green plain run is therefore not evidence
//! about this file.
#![cfg(feature = "gasp")]

use std::path::Path;
use std::process::Command;

/// Events yoagent's projector appends beside every domain event. They carry no
/// information about run boundaries and would drown the assertion, so the
/// ordering is read over domain events only.
const PROJECTION_KIND: &str = "state.ops_applied";

/// The domain events a healthy four-call session must produce. Asserted as a
/// presence set *in addition to* the ordering, so a run that silently recorded
/// nothing cannot satisfy "`run.finished` is last" vacuously.
const REQUIRED_KINDS: &[&str] = &["run.started", "task.created", "run.finished"];

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} could not run: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A scratch GASP agent repo. `GitEventStore::open` refuses a root with no
/// `.git`, and `commit_run` commits into it, so this is a real repository.
fn scratch_repo(dir: &Path) {
    git(dir, &["init", "-q", "."]);
    git(dir, &["config", "user.email", "gasp-test@example.invalid"]);
    git(dir, &["config", "user.name", "gasp test"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("seed.txt"), "seed\n").expect("seed file");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-qm", "seed"]);
}

/// One `yoyo gasp <arm> ...` invocation — its own process, as the shim does.
fn gasp_call(state_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .arg("gasp")
        .args(args)
        .arg("--state-dir")
        .arg(state_dir)
        .output()
        .expect("the yoyo binary must be runnable")
}

/// The `kind` of every domain event in `state/events.jsonl`, in file order.
fn domain_event_kinds(state_dir: &Path) -> Vec<String> {
    domain_events(state_dir)
        .into_iter()
        .map(|(kind, _)| kind)
        .collect()
}

/// Every domain event as `(kind, payload)`, in file order. The payload half is
/// what lets an assertion name *which* node was created rather than only that
/// some `task.created` landed.
fn domain_events(state_dir: &Path) -> Vec<(String, serde_json::Value)> {
    let path = state_dir.join("state").join("events.jsonl");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no event log at {}: {e}", path.display()));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: serde_json::Value =
                serde_json::from_str(l).unwrap_or_else(|e| panic!("corrupt event line {l:?}: {e}"));
            let kind = v
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or_else(|| panic!("event with no kind: {l}"))
                .to_string();
            let payload = v.get("payload").cloned().unwrap_or(serde_json::Value::Null);
            (kind, payload)
        })
        .filter(|(k, _)| k != PROJECTION_KIND)
        .collect()
}

/// The payload of every `patch.proposed` event, in file order.
///
/// `propose_patch` records `serde_json::to_value(&patch)` verbatim, and
/// `StatePatch` carries no `skip_serializing_if` attributes (checked against
/// the resolved yoagent-state 0.5.2, not an older copy in the registry), so
/// `artifacts` is always present — `[]` when empty rather than absent. That is
/// what lets the #851 assertion below distinguish "recorded no artifact" from
/// "recorded no patch".
fn proposed_patches(state_dir: &Path) -> Vec<serde_json::Value> {
    domain_events(state_dir)
        .into_iter()
        .filter(|(k, _)| k == "patch.proposed")
        .map(|(_, p)| p)
        .collect()
}

/// The `artifacts` array of the one patch this session proposed.
///
/// Panics unless exactly one patch landed — the anti-vacuous half of the #851
/// assertion, so a session that recorded *no* patch can never satisfy "carries
/// no artifact" by having nothing to carry.
fn sole_patch_artifacts(state_dir: &Path) -> Vec<serde_json::Value> {
    let patches = proposed_patches(state_dir);
    assert_eq!(
        patches.len(),
        1,
        "expected exactly one `patch.proposed` — no patch means the artifact \
         assertion below would pass vacuously: {patches:?}"
    );
    patches[0]
        .get("artifacts")
        .and_then(|a| a.as_array())
        .unwrap_or_else(|| panic!("patch.proposed has no `artifacts` array: {}", patches[0]))
        .clone()
}

/// `session-start` then one `task-result` with the given verdict, against a
/// fresh scratch repo. Returns the state dir's guard so the caller can read the
/// log before it is cleaned up.
fn run_task_result(verdict: &str, run_id: &str) -> tempfile::TempDir {
    run_task_result_with_reason(verdict, run_id, "verdict artifact test")
}

/// As [`run_task_result`], but with the `--reason` the harness would pass.
///
/// Split out for #915: the unverified path must carry the harness's own
/// sentence **verbatim** into the eval record, so a test has to be able to
/// choose that string. `run_task_result` delegates here with the reason it
/// always used, so every pre-existing call site is byte-identical.
fn run_task_result_with_reason(verdict: &str, run_id: &str, reason: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    scratch_repo(dir);

    let start = gasp_call(
        dir,
        &[
            "session-start",
            "--run-id",
            run_id,
            "--day",
            "181",
            "--task",
            "artifact verdict test",
        ],
    );
    assert!(
        start.status.success(),
        "session-start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );

    // `task-result` resumes the open run, so `session-start` above is required.
    let result = gasp_call(
        dir,
        &[
            "task-result",
            "--run-id",
            run_id,
            "--num",
            "1",
            "--title",
            "artifact verdict task",
            "--verdict",
            verdict,
            "--pre-sha",
            PRE_SHA,
            "--post-sha",
            POST_SHA,
            "--repo",
            "yologdev/yoyo-evolve",
            "--reason",
            reason,
        ],
    );
    assert!(
        result.status.success(),
        "task-result ({verdict}) failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    tmp
}

/// Distinct on purpose: an assertion can name *which* sha it found, so a patch
/// that recorded the base commit as its artifact is not mistaken for a pass.
const PRE_SHA: &str = "aaaaaaa1111111111111111111111111111aaaa1";
const POST_SHA: &str = "bbbbbbb2222222222222222222222222222bbbb2";

/// #851 — a rejected verdict attaches NO commit artifact.
///
/// The defect: the artifact was attached unconditionally, above the
/// `update_patch_status(… Rejected …)` that follows. `scripts/evolve.sh`
/// reverts a failed task with `git reset --hard PRE_TASK_SHA`, so HEAD at this
/// point is the *previous* task's commit — day 173 has patches `_1` and `_2`
/// both carrying `fb60556e`, i.e. a thrown-away task claiming the commit of the
/// one beside it that shipped.
#[test]
fn rejected_verdict_records_no_commit_artifact() {
    let tmp = run_task_result("rejected", "run_gasp_artifact_rejected");
    let artifacts = sole_patch_artifacts(tmp.path());

    assert!(
        artifacts.is_empty(),
        "a rejected task produced nothing, so its patch must carry no artifact \
         — got {artifacts:?}"
    );

    // The patch still says where it started from, so dropping the artifact
    // loses no information a reader had before.
    let patch = &proposed_patches(tmp.path())[0];
    let base = patch
        .get("base_project_ref")
        .unwrap_or_else(|| panic!("no base_project_ref: {patch}"));
    assert_eq!(
        base.get("commit").and_then(|c| c.as_str()),
        Some(PRE_SHA),
        "base_project_ref must still record pre_sha: {base}"
    );
}

/// The near-miss guard: a promoted verdict is byte-identical to before.
///
/// A discriminator tested only on the side that fires is vacuous green, and
/// this one has a live wrong-value case to prove against — so the *kept* half
/// is asserted down to the sha, not merely "non-empty".
#[test]
fn promoted_verdict_still_records_the_commit_artifact() {
    let tmp = run_task_result("promoted", "run_gasp_artifact_promoted");
    let artifacts = sole_patch_artifacts(tmp.path());

    assert_eq!(
        artifacts.len(),
        1,
        "a promoted task must record exactly one commit artifact: {artifacts:?}"
    );
    let artifact = &artifacts[0];
    assert_eq!(
        artifact.get("kind").and_then(|k| k.as_str()),
        Some("git-commit"),
        "artifact kind: {artifact}"
    );
    assert_eq!(
        artifact.get("hash").and_then(|h| h.as_str()),
        Some(POST_SHA),
        "the artifact must carry the post-task sha, not the base: {artifact}"
    );
    assert_eq!(
        artifact.get("uri").and_then(|u| u.as_str()),
        Some(format!("yologdev/yoyo-evolve@{POST_SHA}").as_str()),
        "artifact uri: {artifact}"
    );
}

/// The payload of the one `eval.finished` this session recorded.
///
/// **Anti-vacuous, and this is the half that matters for #915:** it panics
/// unless exactly one such event exists, so a session that recorded *no*
/// evaluation can never satisfy "the status is not `Passed`" by having no
/// status at all. A test that finds nothing and passes is this defect wearing
/// the opposite sign.
fn sole_eval_payload(state_dir: &Path) -> serde_json::Value {
    let evals: Vec<serde_json::Value> = domain_events(state_dir)
        .into_iter()
        .filter(|(k, _)| k == "eval.finished")
        .map(|(_, p)| p)
        .collect();
    assert_eq!(
        evals.len(),
        1,
        "expected exactly one `eval.finished` — zero would make every \
         assertion below pass vacuously: {evals:?}"
    );
    evals.into_iter().next().unwrap()
}

/// The payload of the one `decision.created` this session recorded.
///
/// Anti-vacuous for the same reason as [`sole_eval_payload`]: the decision
/// assertions below are about *what the reason says*, and a missing decision
/// would satisfy "does not claim the oracle passed" by saying nothing at all.
fn sole_decision_payload(state_dir: &Path) -> serde_json::Value {
    let decisions: Vec<serde_json::Value> = domain_events(state_dir)
        .into_iter()
        .filter(|(k, _)| k == "decision.created")
        .map(|(_, p)| p)
        .collect();
    assert_eq!(
        decisions.len(),
        1,
        "expected exactly one `decision.created`: {decisions:?}"
    );
    decisions.into_iter().next().unwrap()
}

/// The `status` of the one `patch.status_changed` this session recorded.
///
/// Anti-vacuous in the same shape: no status change at all would let
/// "the patch is still Promoted" pass by never having been judged.
fn sole_patch_status(state_dir: &Path) -> String {
    let changes: Vec<serde_json::Value> = domain_events(state_dir)
        .into_iter()
        .filter(|(k, _)| k == "patch.status_changed")
        .map(|(_, p)| p)
        .collect();
    assert_eq!(
        changes.len(),
        1,
        "expected exactly one `patch.status_changed`: {changes:?}"
    );
    changes[0]
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or_else(|| panic!("patch.status_changed has no string status: {}", changes[0]))
        .to_string()
}

/// The sentence `scripts/evolve.sh` emits when it accepts a task whose
/// evaluator never produced a verdict. Carried verbatim into the eval record,
/// because on this path the reason string is the only place the truth lives.
const UNVERIFIED_REASON: &str =
    "accepted UNVERIFIED: evaluator produced no verdict; build+test only";

/// #915 — an UNVERIFIED accept is recorded as `Skipped`, never `Passed`.
///
/// The defect: `verdict` was two-valued (`let promoted = verdict == "promoted"`),
/// so the harness's fail-open accept — build+test green, evaluator never ran —
/// was written into the append-only record as `eval.finished Passed`. That is
/// `"could not check"` reading as `"checked; clean"`, in the one place a
/// dashboard reads. Five Day-195 sessions rendered `5/5 promoted` with zero
/// task commits.
///
/// Asserted on the **emitted event payload**, never on `Ok`: `task_result`
/// returning success says nothing about which status reached the log.
#[test]
fn unverified_verdict_records_skipped_and_never_passed() {
    let tmp = run_task_result_with_reason(
        "unverified",
        "run_gasp_eval_unverified",
        UNVERIFIED_REASON,
    );
    // Anti-vacuous first: one eval exists to be judged.
    let eval = sole_eval_payload(tmp.path());

    let status = eval
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or_else(|| panic!("eval.finished has no string status: {eval}"));

    // The whole of #915, stated as the thing that must never be true.
    assert_ne!(
        status, "Passed",
        "an accept where NO evaluator ran must not be recorded as `Passed` — \
         that is \"could not check\" reading as \"checked; clean\" in the \
         append-only record: {eval}"
    );
    // BRANCH A: the resolved yoagent-state carries `Skipped`, so the graph can
    // say "not run" at the status level rather than only in prose.
    assert_eq!(
        status, "Skipped",
        "the unverified path must use EvalStatus::Skipped: {eval}"
    );

    // `Failed` would be the other wrong answer: the eval did not fail, it did
    // not run. Pinned explicitly so a later "simplification" cannot land it.
    assert_ne!(
        status, "Failed",
        "the evaluator did not fail, it never ran: {eval}"
    );

    // A score would assert a measurement nobody took.
    assert!(
        eval.get("score").map(|s| s.is_null()).unwrap_or(false),
        "an eval that never ran has no score: {eval}"
    );

    // The harness's own sentence, verbatim — this is what closes the
    // parsed-but-ignored `--reason` flag on the landed path.
    assert_eq!(
        eval.pointer("/metadata/reason").and_then(|r| r.as_str()),
        Some(UNVERIFIED_REASON),
        "the eval reason must carry the harness string verbatim: {eval}"
    );
}

/// #915 — the code LANDED, so the patch stays `Promoted` and the decision
/// stays `Approved`; only the oracle claim changes.
///
/// The two questions have different answers and must not be folded: *did the
/// code land and stay?* (yes — it is on main, and saying otherwise would be a
/// second lie) versus *did an oracle look at it?* (no). This test pins the
/// first half; the one above pins the second.
#[test]
fn unverified_verdict_still_promotes_the_patch_and_approves_the_decision() {
    let tmp = run_task_result_with_reason(
        "unverified",
        "run_gasp_landed_unverified",
        UNVERIFIED_REASON,
    );

    assert_eq!(
        sole_patch_status(tmp.path()),
        "Promoted",
        "the commit is on main, so the patch status must stay Promoted"
    );

    let decision = sole_decision_payload(tmp.path());
    assert_eq!(
        decision.get("status").and_then(|s| s.as_str()),
        Some("Approved"),
        "the change was kept, so the decision stays Approved: {decision}"
    );

    let reason = decision
        .get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or_else(|| panic!("decision has no string reason: {decision}"));

    // The decision reason used to hardcode `oracle passed (<cmd>); kept`, which
    // is the same false claim one node over.
    assert!(
        !reason.contains("oracle passed"),
        "the decision reason must not claim an oracle passed when none ran: \
         {reason:?}"
    );
    assert!(
        reason.contains("unverified"),
        "the decision reason must name the state, so a consumer can tell \
         \"the oracle passed\" from \"nobody looked\": {reason:?}"
    );

    // The artifact half is unchanged: the code landed, so the commit is
    // recorded exactly as on the promoted path.
    let artifacts = sole_patch_artifacts(tmp.path());
    assert_eq!(
        artifacts.len(),
        1,
        "an unverified accept still landed a commit: {artifacts:?}"
    );
    assert_eq!(
        artifacts[0].get("hash").and_then(|h| h.as_str()),
        Some(POST_SHA),
        "the artifact must carry the post-task sha: {}",
        artifacts[0]
    );
}

/// The near-miss guard, and it is the entire regression surface.
///
/// Every existing session's row is on the promoted path, so this asserts the
/// **whole** eval payload shape — status, score and reason together — rather
/// than a `contains`. A discriminator tested only on the side that fires is
/// vacuous green, and the three-way split must leave this byte-identical.
#[test]
fn promoted_verdict_still_records_passed_byte_identically() {
    let tmp = run_task_result_with_reason(
        "promoted",
        "run_gasp_eval_promoted",
        "oracle reason passthrough",
    );
    let eval = sole_eval_payload(tmp.path());

    assert_eq!(
        eval.get("status").and_then(|s| s.as_str()),
        Some("Passed"),
        "a promoted verdict must still record Passed: {eval}"
    );
    assert_eq!(
        eval.get("score").and_then(|s| s.as_f64()),
        Some(1.0),
        "a promoted verdict must still score 1.0: {eval}"
    );
    assert_eq!(
        eval.pointer("/metadata/reason").and_then(|r| r.as_str()),
        Some("oracle reason passthrough"),
        "the promoted path now consumes --reason too: {eval}"
    );

    // And the landed half is unchanged on this path as well.
    assert_eq!(sole_patch_status(tmp.path()), "Promoted");
    let decision = sole_decision_payload(tmp.path());
    assert_eq!(
        decision.get("status").and_then(|s| s.as_str()),
        Some("Approved"),
        "{decision}"
    );
    assert!(
        decision
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .contains("oracle passed"),
        "the promoted decision reason is unchanged and still names the oracle: \
         {decision}"
    );
}

/// The `id` of every task node created, in file order.
fn task_created_ids(state_dir: &Path) -> Vec<String> {
    domain_events(state_dir)
        .into_iter()
        .filter(|(k, _)| k == "task.created")
        .map(|(_, p)| {
            p.get("id")
                .and_then(|i| i.as_str())
                .unwrap_or_else(|| panic!("task.created with no string id: {p}"))
                .to_string()
        })
        .collect()
}

#[test]
fn four_call_session_finishes_its_own_run_last() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    scratch_repo(dir);

    let run_id = "run_gasp_cli_ordering_test";

    let start = gasp_call(
        dir,
        &[
            "session-start",
            "--run-id",
            run_id,
            "--day",
            "178",
            "--task",
            "ordering test session",
        ],
    );
    assert!(
        start.status.success(),
        "session-start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );

    let task = gasp_call(
        dir,
        &[
            "task", "--run-id", run_id, "--num", "1", "--title", "task one", "--kind", "evolve",
        ],
    );
    assert!(
        task.status.success(),
        "task failed: {}",
        String::from_utf8_lossy(&task.stderr)
    );

    let result = gasp_call(
        dir,
        &[
            "task-result",
            "--run-id",
            run_id,
            "--num",
            "1",
            "--title",
            "task one",
            "--verdict",
            "success",
            "--pre-sha",
            "aaaaaaa",
            "--post-sha",
            "bbbbbbb",
            "--repo",
            "yologdev/yoyo-evolve",
            "--kind",
            "evolve",
        ],
    );
    assert!(
        result.status.success(),
        "task-result failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let end = gasp_call(
        dir,
        &["session-end", "--run-id", run_id, "--outcome", "done"],
    );
    assert!(
        end.status.success(),
        "session-end failed: {}",
        String::from_utf8_lossy(&end.stderr)
    );

    let kinds = domain_event_kinds(dir);

    // Anti-vacuous: an empty or truncated log must not satisfy the ordering
    // assertions below by having nothing to order.
    for required in REQUIRED_KINDS {
        assert!(
            kinds.iter().any(|k| k == required),
            "no `{required}` in the log — the session recorded nothing to order: {kinds:?}"
        );
    }

    // The assertion #831 is about. Broken, the interrupt's `run.finished`
    // lands third and everything after it is orphaned.
    assert_eq!(
        kinds.last().map(String::as_str),
        Some("run.finished"),
        "`run.finished` must be the last domain event: {kinds:?}"
    );

    // Exactly one, and it is this session's own close — not an interrupt plus
    // a close. Two would mean a prior open was reaped mid-session.
    assert_eq!(
        kinds.iter().filter(|k| *k == "run.finished").count(),
        1,
        "exactly one `run.finished` — more means a run was interrupted: {kinds:?}"
    );

    // The inversion the defect produced, pinned directly: the task must be
    // recorded *into* an open run, never after it closed.
    let started = kinds.iter().position(|k| k == "run.started").unwrap();
    let created = kinds.iter().position(|k| k == "task.created").unwrap();
    let finished = kinds.iter().position(|k| k == "run.finished").unwrap();
    assert!(
        started < created && created < finished,
        "task.created must sit between run.started and run.finished: {kinds:?}"
    );

    // The payload, not the container: a run that closes without a boundary
    // commit never reaches the operator lane at all.
    let stdout = String::from_utf8_lossy(&end.stdout);
    assert!(
        stdout.contains("boundary commit"),
        "session-end must report a boundary commit, got stdout {stdout:?} / stderr {:?}",
        String::from_utf8_lossy(&end.stderr)
    );

    // #847: the session gets its own node, so a run is never the only evidence
    // that a session happened. Every run-tier assertion above is the regression
    // guard for the "additive" promise — this must ADD a node and change
    // nothing about `run.started` / `run.finished`.
    let task_ids = task_created_ids(dir);
    let session_node = format!("task_{run_id}_session");
    assert!(
        task_ids.contains(&session_node),
        "no `{session_node}` node — a session with no planned tasks would have \
         nothing in the graph but its run: {task_ids:?}"
    );

    // The near-miss guard: the session node is an ADDITION, not a rename of the
    // planned task's node. Both must be present, or this "fix" would have moved
    // a node rather than added one.
    let planned_node = format!("task_{run_id}_1");
    assert!(
        task_ids.contains(&planned_node),
        "planned task node `{planned_node}` must survive beside the session node: {task_ids:?}"
    );

    // Recorded inside the run, exactly as a planned task is — not before it
    // opened, and not after it closed.
    let session_created = kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| *k == "task.created")
        .map(|(i, _)| i)
        .next()
        .expect("a task.created must exist");
    assert!(
        started < session_created && session_created < finished,
        "the session node must be created inside the open run: {kinds:?}"
    );

    // Closed at session-end, and closed BEFORE the run — a status change after
    // `run.finished` would both break the ordering above and land outside the
    // boundary commit.
    let status_changed = kinds
        .iter()
        .position(|k| k == "task.status_changed")
        .unwrap_or_else(|| panic!("session-end must close the session node: {kinds:?}"));
    assert!(
        status_changed < finished,
        "the session node must be closed before the run is: {kinds:?}"
    );
}
