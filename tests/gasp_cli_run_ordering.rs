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
    let path = state_dir.join("state").join("events.jsonl");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no event log at {}: {e}", path.display()));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("corrupt event line {l:?}: {e}"));
            v.get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or_else(|| panic!("event with no kind: {l}"))
                .to_string()
        })
        .filter(|k| k != PROJECTION_KIND)
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

    let end = gasp_call(dir, &["session-end", "--run-id", run_id, "--outcome", "done"]);
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
}
