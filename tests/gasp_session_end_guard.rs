//! Source-level ordering guard for `gasp::session_end`'s session-node close (#849).
//!
//! **The defect this pins.** #847 taught `session_end` to close the session's own
//! task node before closing the run. That close was written fail-soft — an
//! `if let Err(e) = … update_task_status(…)` with a stderr message — on the
//! stated belief that an absent node would surface as an `Err` and be *named*.
//! It does not. In `yoagent-state` 0.5.0, `update_task_status` (`state.rs:457`)
//! records the `task.status_changed` event **first, unconditionally**, and only
//! then applies the `UpdateNode` op; it performs no node-existence check
//! anywhere. So the guard was dead twice over: the append had already happened
//! by the time any `Err` could return, and the error most likely never returned
//! at all — the corruption surfaced later, at *fold* time, in a different
//! process (`fold failed: node not found: task_<run>_session`), which is how the
//! live `yologdev/yoyo-gasp` graph went 7/7 → 5/7 conformant.
//!
//! **The transferable shape:** *a guard on the result cannot protect a side
//! effect that happens before the result.* For an append-only event store,
//! catching the error does not unwrite the event. The check has to be on the
//! **call** — does the node exist? — not on its **return value**.
//!
//! **Why this file is not feature-gated, deliberately.** `src/gasp.rs` sits
//! behind the default-off `gasp` feature, so a `#[cfg(test)]` module inside it
//! never runs under the plain `cargo test` that gates every evolve task, and
//! `tests/gasp_cli_run_ordering.rs` carries a file-level
//! `#![cfg(feature = "gasp")]`, so it compiles to **zero tests** there — exactly
//! the blindness `tests/feature_gated_tests.rs` exists to name. A text scan
//! needs no feature, so this guard runs in the loop's own gate and therefore has
//! a consumer that actually reads it. Because it is not gated it needs no entry
//! in `REGISTERED_FEATURE_GATED_TESTS`.
//!
//! **What it cannot do, stated rather than implied.** It proves the existence
//! check is *positioned* ahead of the update — never that it fires, never that
//! its result is used, and never that the emitted events are correct. The
//! behavioural half is `tests/gasp_cli_run_ordering.rs`, which drives four real
//! processes under `--features gasp`; this is the cheap ordering guard that runs
//! everywhere.

use std::path::Path;

/// Read `src/gasp.rs` off disk. Resolved from `CARGO_MANIFEST_DIR` rather than a
/// relative path, the same technique `src/help_data_guards.rs` uses, so the test
/// does not depend on the process CWD.
fn gasp_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("gasp.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {} for the ordering guard: {e}",
            path.display()
        )
    })
}

/// Slice the body of a top-level `async fn` by brace-free heuristics: start at
/// the signature, stop at the next line that is exactly `}` at column 0.
///
/// Deliberately crude — this is a text scan, not a Rust parser (the same limit
/// `tests/orphan_modules.rs` and `tests/cargo_spawning_tests.rs` state about
/// themselves). It is anti-vacuous: a signature that cannot be found, or a body
/// that never closes, panics rather than yielding an empty slice that every
/// assertion below would then pass over.
fn fn_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source.find(signature).unwrap_or_else(|| {
        panic!("could not find `{signature}` in src/gasp.rs — did it get renamed?")
    });
    let rest = &source[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("could not find the end of `{signature}` in src/gasp.rs"));
    let body = &rest[..end];
    assert!(
        body.len() > 200,
        "sliced body of `{signature}` is {} bytes — too short to be the real function, \
         so every assertion over it would be vacuously green",
        body.len()
    );
    body
}

/// The session-node close must ask whether the node **exists** before calling
/// the update, because the update appends its event before it can return an
/// error. Asserted as an *index comparison* inside `session_end`'s own body, so
/// a `get_node` call sitting elsewhere in the module (there are two, in
/// `ensure_goal` and its sibling) cannot satisfy it.
///
/// Needles are assembled at runtime so this file's own source can never match
/// them — the discipline every source-level guard in this repo uses.
#[test]
fn session_end_checks_the_node_exists_before_updating_its_status() {
    let source = gasp_source();

    let signature = format!("{}{}", "async fn session_", "end(");
    let body = fn_body(&source, &signature);

    let existence_check = format!("{}{}", "get_", "node(");
    let status_update = format!("{}{}", "update_task_", "status(");

    let check_at = body.find(&existence_check).unwrap_or_else(|| {
        panic!(
            "`session_end` never calls `{existence_check}`. #849: `update_task_status` appends \
             `task.status_changed` before it can return an error, so an `if let Err(…)` around it \
             cannot protect against a missing node — the check must be on the call, not the result."
        )
    });
    let update_at = body.find(&status_update).unwrap_or_else(|| {
        panic!("`session_end` no longer calls `{status_update}` — did the session-node close move?")
    });

    assert!(
        check_at < update_at,
        "`session_end` calls `{status_update}` (byte {update_at}) before `{existence_check}` \
         (byte {check_at}). The existence check must come first: the update writes its event \
         before it can fail, so a check placed after it guards nothing."
    );
}

/// The close must stay **above** `record_run_finished`, and both must stay above
/// `commit_run`. Position is load-bearing in two directions: `run.finished` has
/// to remain the last domain event (pinned behaviourally by
/// `tests/gasp_cli_run_ordering.rs`), and the close has to precede the boundary
/// commit or the commit would not carry it. #849 only *guards* that block — it
/// must not relocate it.
#[test]
fn session_end_closes_the_session_node_before_finishing_and_committing_the_run() {
    let source = gasp_source();

    let signature = format!("{}{}", "async fn session_", "end(");
    let body = fn_body(&source, &signature);

    let status_update = format!("{}{}", "update_task_", "status(");
    let run_finished = format!("{}{}", "record_run_", "finished(");
    let commit = format!("{}{}", "commit_", "run(");

    let update_at = body
        .find(&status_update)
        .expect("`session_end` no longer closes the session node");
    let finished_at = body
        .find(&run_finished)
        .expect("`session_end` no longer calls `record_run_finished`");
    let commit_at = body
        .find(&commit)
        .expect("`session_end` no longer calls `commit_run`");

    assert!(
        update_at < finished_at,
        "the session-node close (byte {update_at}) must precede `record_run_finished` \
         (byte {finished_at}) — `run.finished` is pinned as the last domain event"
    );
    assert!(
        update_at < commit_at,
        "the session-node close (byte {update_at}) must precede `commit_run` (byte {commit_at}) \
         — otherwise the boundary commit would not carry it"
    );
}

/// A skipped close is a real event, not nothing. The absent-node branch must
/// announce itself on stderr, and it must stay **distinct** from the pre-existing
/// error branch: "the node was absent" and "the update errored" are different
/// facts with different remedies, and folding them would be the
/// absence-absorbed-by-a-convenient-neighbour defect this repo keeps fixing.
#[test]
fn session_end_announces_a_skipped_close_separately_from_an_errored_one() {
    let source = gasp_source();

    let signature = format!("{}{}", "async fn session_", "end(");
    let body = fn_body(&source, &signature);

    let stderr_write = format!("{}{}", "eprint", "ln!(");
    let announcements = body.matches(&stderr_write).count();

    assert!(
        announcements >= 2,
        "`session_end` has {announcements} stderr announcement(s) in the session-node close; \
         expected at least 2 — one for the absent node (skipped) and one for a genuine error. \
         A silent skip is the exact defect #849 is about."
    );
}
