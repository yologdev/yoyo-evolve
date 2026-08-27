//! GASP recorder wiring (issue #683, steps 1–2) plus the *named half* of the
//! session-graph emission ported from the `tools/gasp-emit` sidecar (#683
//! item 5).
//!
//! **`tools/gasp-emit` no longer exists in the tree** — #683 item (7) deleted it
//! (creator commit `b573e523`), and `scripts/gasp_shim.sh` now shells this
//! binary (`yoyo gasp <arm>`) instead. Every `tools/gasp-emit/src/main.rs:NN`
//! citation below is therefore a **historical** reference, not a missing file:
//! it says where a constant or a parse rule came from, and why some of them may
//! not be reworded (a goal id is a node identity in a shared store). Read it
//! with `git show b573e523^:tools/gasp-emit/src/main.rs`.
//!
//! What exists here: a default-off `gasp` cargo feature, an env-gated open of
//! `yoagent::gasp::GaspRecorder`, a process-global holder installed once at
//! startup, and two tee helpers that route a prompt through
//! `Agent::prompt_with_sender` so run/tool events are actually recorded. All
//! four agent-start call sites in `src/prompt.rs` go through those helpers.
//!
//! Ported from the sidecar: **all four arms** — `session-start`
//! (`session_start`), `task` (`task_planned`), `session-end` (`session_end`)
//! and, as of Day 177, `task-result` ([`task_result`]) — plus the
//! `ensure_goal` helper they share.
//!
//! **Superseded claim, recorded rather than erased** (Day-165 rule): this
//! paragraph read "`task-result` is **still only in `tools/gasp-emit`** —
//! unported, but **not blocked**" from Day 172 until Day 177. Both halves were
//! accurate when written; only the first is now false. It is kept because the
//! *earlier* version of this sentence — which said `task-result` was
//! *unreachable* — is what cost six sessions, and the correction is worth more
//! than the tidy text.
//!
//! <!-- yoagent-version-claim: 0.16.6 -->
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
//! `tests/doc_version_claims.rs`, which fails if the pin moves without this
//! analysis being re-run. That guard is the reason this correction cannot go
//! stale the way the last one did.
//!
//! **Superseded claim, corrected in the same commit that falsified it**: this
//! paragraph opened "What is still true: `task-result` is **not ported**, and
//! porting it is out of scope here", listing `patch.base_project_ref`,
//! `patch.artifacts` and `update_patch_status(…, PatchStatus::Promoted, …)`
//! (`tools/gasp-emit/src/main.rs:166-268`) as what it would need. All three
//! are named by [`task_result`] below, so that sentence is now false. It is
//! recorded rather than deleted because a stale *sibling* of it — the earlier
//! "unreachable" wording — is precisely what cost eight sessions, and the
//! lesson is that the half an agent opens to do the work is the half that has
//! to be true.
//!
//! `session_end` is the odd one of the four: it has no store-generic `*_in`
//! body, because `commit_run` / `release_lease` are `GitEventStore` *inherent*
//! methods rather than `EventStore` trait methods, so it cannot be written
//! against a generic store at all and is compile-tested only (its two pure
//! halves are table-tested).
//!
//! **Superseded claim, recorded rather than erased (#827, then #831).** This
//! doc read: *"All four ship **dormant**: nothing calls any of them, since
//! their consumer is #683 items (3)+(7) (the operator-lane env bridge). They
//! compile, their pure halves unit-test, and they record nothing."* #827 gave
//! all four a CLI door (`yoyo gasp <arm>`, routed from `dispatch_sub.rs`) and
//! #831 made that door open the store directly, so they are wired, they run,
//! and they record; the `#[allow(dead_code)]` attributes that sentence names
//! are gone from this file.
//!
//! **Superseded claim, recorded rather than erased (Day 180):** the paragraph
//! above ended *"Still true and worth keeping: `scripts/gasp_shim.sh` still
//! invokes the `tools/gasp-emit` sidecar, so the **operator lane has not been
//! swapped** — that is #683 item (7), with the env bridge item (3), both still
//! pending."* Item (7) **shipped** at `b573e523`: the sidecar is deleted and
//! `scripts/gasp_shim.sh:43,89` shells `target/gasp-yoyo/debug/yoyo gasp` — a
//! *separate target dir*, deliberately, so the featured build cannot clobber
//! the plain `target/debug/yoyo` every `CARGO_BIN_EXE_*` consumer resolves to
//! (the #832 shared-uplift hazard). **The operator lane is swapped; this file
//! is the emitter.** It is recorded rather than deleted because a stale
//! present-tense status claim *in this exact file* is what cost eight sessions
//! (#763, #765, #782, #785, #787, #789, #803, plus the docs-only #788), and
//! the lesson is that the half an agent opens to do the work is the half that
//! has to be true.
//!
//! **Item (3), the in-process env bridge, is still open — measured, not
//! assumed.** `grep -n 'YOYO_GASP_STATE_DIR' scripts/gasp_shim.sh` returns two
//! hits and **neither is an export**: `:150` is the line *"DO NOT export
//! YOYO_GASP_STATE_DIR / YOYO_GASP_GOAL_ID here"* and `:193` says an unset
//! value keeps the recorder unreachable on purpose. So [`plan_from_env_values`]
//! still lands in [`RecorderPlan::Disabled`] for every evolve-loop process, and
//! the in-process run/model/tool tier (#683 item (3)) records **nothing** in
//! the operator lane. **What changed is the *reason*, and that is the part
//! worth carrying:** it used to be "a second store would collide with the
//! sidecar's". The sidecar is gone and the hazard is not, because the shim
//! still *opens and holds a run* for the whole session — now via
//! `yoyo gasp session-start … session-end` — so exporting the bridge today
//! would put the agent processes' in-process recorder on the same
//! single-writer store *while that run is open*, reproducing the lease theft
//! measured on Day 165 verbatim (`run.finished` written against a live run,
//! then `session-end` failing with `cannot finish <run>: no run is open`, and
//! the whole session's record lost). `scripts/gasp_shim.sh:150-194` states it
//! as an **architecture** question, not a wiring one: either the session tier
//! moves inside the agent process, or the run/model/tool tier is written
//! through these same short-lived calls. The bridge is exported in *that*
//! commit.
//!
//! **Behavioural coverage, stated precisely rather than generously.**
//! `tests/gasp_cli_run_ordering.rs` drives a four-process session through the
//! real binary against a real `GitEventStore` on a tempdir, so
//! `session_start_in`, `task_planned_in` and `task_result_in` *are* now
//! exercised end-to-end — which this module's older "**Zero tests drive any
//! `*_in` body**" claim denied (it is quoted and corrected in
//! [`task_result`]'s doc too). What that does **not** license: it is one
//! happy path, under `--features gasp` only — a plain `cargo test` compiles
//! this module to nothing — and no test drives an `*_in` body against a
//! *scratch* store, because `MemoryEventStore` is still not reachable from
//! this crate's dependency set (see [`task_result`]). "Store-generic" still
//! means the right *shape*, exercised against exactly one store.
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
    ActorRef, ArtifactRef, Decision, DecisionId, DecisionStatus, EvalId, EvalResult, EvalStatus,
    EventStore, GaspGoal, GaspRecorder, GitEventStore, GoalId, GoalRef, NodeId, PatchId,
    PatchStatus, ProjectRef, RunId, StateError, StatePatch, Task, TaskId, TaskStatus, YoAgentState,
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
/// Worker id for the **in-process** run/model/tool tier only, deliberately
/// distinct from the retiring sidecar's — mixed-writer history is exactly what
/// that field is for, so in-process runs must be tellable apart from ones a
/// shim-spawned process wrote after the fact.
///
/// **Do not unify this with the graph tier's id.** Since #828 item 2 the
/// `yoyo gasp` door takes its worker id from `--worker`
/// (`gasp_cli::graph_worker_id`), because that string is the 600s *lease*
/// identity and overlapping sessions must not share one. This one is not
/// caller-supplied and must not become so: an in-process event recorded under
/// `evolve-shim-$$` would be indistinguishable from a sidecar-written one.
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
// Graph-tier handle: the store, opened directly (#831)
// ---------------------------------------------------------------------------

/// A directly-opened handle onto a GASP agent repo for the **session-graph**
/// tier (`session-start` / `task` / `task-result` / `session-end`).
///
/// Deliberately **not** a [`GaspRecorder`], and that is the whole of #831. The
/// shim (`scripts/gasp_shim.sh`) emits **one event per process** — four
/// short-lived `yoyo gasp` invocations per session — while
/// `GaspRecorder::with_store` closes any run left open by a previous process
/// as `"interrupted"` on **every** open. So call 2 interrupted the run call 1
/// opened, call 3 emitted `run.finished`, every later fact landed under a
/// finished run, and `session-end` failed with `no run is open`, making **no
/// boundary commit** — with every call still exiting 0, which is why exit
/// codes could not see it.
///
/// This is what `tools/gasp-emit` always did: `GitEventStore::open` plus
/// `YoAgentState::load`, with `resume_open_run` on every arm *except*
/// `session-start`, so an open run survives the process boundary instead of
/// being closed by its own next event.
///
/// [`GaspRecorder`] remains the right handle for the **in-process** run /
/// model / tool tier (#683 item (3)) — one process, one open, so the
/// interrupt-on-open rule is correct there. That path ([`open_recorder`],
/// [`install`], [`tee_prompt`]) is untouched by this type.
pub(crate) struct GraphSession {
    state: YoAgentState<GitEventStore>,
    store: GitEventStore,
    actor: ActorRef,
    goal: String,
}

impl GraphSession {
    /// The graph the arms record into.
    pub(crate) fn state(&self) -> &YoAgentState<GitEventStore> {
        &self.state
    }

    /// The underlying store — needed by [`session_end`], whose `commit_run` /
    /// `release_lease` are `GitEventStore` *inherent* methods.
    pub(crate) fn store(&self) -> &GitEventStore {
        &self.store
    }

    /// Who recorded events are attributed to.
    pub(crate) fn actor(&self) -> &ActorRef {
        &self.actor
    }

    /// The configured standing goal, used as the fallback when an arm names
    /// none. Same role `GaspRecorder::goal()` played for these arms.
    pub(crate) fn goal(&self) -> &str {
        &self.goal
    }
}

/// Open the store for one graph-tier arm.
///
/// `resume` must be `false` for `session-start` (there is no prior run to
/// chain to) and `true` for every other arm, mirroring the sidecar's
/// `if cmd != "session-start" { state.resume_open_run().await? }`. Resuming is
/// what makes a run survive four processes; note it *restores* the open-run
/// marker rather than closing the run, which is precisely the difference from
/// [`open_recorder`].
///
/// Handles the same three [`RecorderPlan`] states as [`open_recorder`] and
/// never propagates, never panics: a session must not break because
/// instrumentation did.
///
/// Unlike [`open_recorder`] this does **not** validate that the goal already
/// exists — the arms create it on first reference (`session_start_in`,
/// `ensure_goal`), exactly as the sidecar did. The recorder's redaction
/// summarizer is likewise absent because it is never consulted on this tier:
/// it wraps persisted tool-arg/output summaries in the event-stream path, and
/// the graph arms call `record_*` directly.
///
/// `worker` is the caller-supplied lease identity (#828 item 2): a GASP repo is
/// single-writer behind a 600s lease, so overlapping sessions must open under
/// distinct ids. It comes from `--worker` via `gasp_cli::graph_worker_id`,
/// which supplies the historical default when the flag names none. This is
/// **not** [`WORKER_ID`], and the two must not be merged — see that constant.
pub(crate) async fn open_graph_session(
    plan: RecorderPlan,
    resume: bool,
    worker: &str,
) -> Option<GraphSession> {
    let (root, goal_id) = match plan {
        RecorderPlan::Disabled => return None,
        RecorderPlan::Misconfigured(reason) => {
            eprintln!("gasp: recording disabled — {reason}");
            return None;
        }
        RecorderPlan::Open { root, goal_id } => (root, goal_id),
    };

    if !root.is_dir() {
        return None;
    }

    let store = match GitEventStore::open(root.clone(), worker) {
        Ok(store) => store,
        Err(e) => {
            eprintln!(
                "gasp: recording disabled — opening store at {} failed: {e}",
                root.display()
            );
            return None;
        }
    };
    let state = match YoAgentState::load(store.clone()).await {
        Ok(state) => state,
        Err(e) => {
            eprintln!(
                "gasp: recording disabled — loading state at {} failed: {e}",
                root.display()
            );
            return None;
        }
    };
    if resume {
        // Failing to resume is worth saying out loud but not worth aborting:
        // the arm still records, it just records without a correlated run.
        if let Err(e) = state.resume_open_run().await {
            eprintln!("gasp: could not resume the open run: {e}");
        }
    }

    Some(GraphSession {
        state,
        store,
        actor: ActorRef::agent(AGENT_ID),
        goal: goal_id,
    })
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
// ALL FOUR ARMS LANDED HERE: `session-start`, `task` (plus the `ensure_goal`
// helper they share), `session-end` (Day 168) and `task-result` (Day 177).
//
// **Superseded claim, recorded rather than erased (Day 180):** this block read
// "LANDED HERE: `session-start`, `task` … and, since Day 168, `session-end`.
// STILL ONLY IN `tools/gasp-emit`: `task-result` — UNPORTED, but **NOT
// BLOCKED**. Read those as two facts." It was **doubly stale**: `task-result`
// was ported on Day 177 (see [`task_result`]), *and* `tools/gasp-emit` was
// deleted by #683 item (7) (`b573e523`), so there is no "only in" left to be
// in. Recorded rather than tidied away because a stale status sentence in this
// exact file is what cost eight sessions.
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
// to `Cargo.lock` by `tests/doc_version_claims.rs`.
//
// **Superseded claim, recorded rather than erased (Day 180):** this block read
// "This half is a prefix of the sequence, not a replacement for the sidecar."
// There is nothing left to be a prefix *of*: the sidecar was deleted by #683
// item (7) and all four arms live here, so this **is** the sequence.
//
// **Superseded claim, recorded rather than erased (Day 180):** this block also
// read "Everything below ships DORMANT: nothing calls `session_start` /
// `task_planned` / `session_end` yet. Their consumers are #683 items (3)+(7),
// the operator-lane env bridge, which is deliberately not wired here — wiring
// it early would destroy the sidecar's session record." Both halves are now
// false, and they were false for *different* reasons, which is why they are
// recorded separately rather than blanket-deleted:
//
//   * NOT DORMANT. #827 gave every arm a CLI door (`yoyo gasp <arm>`, routed
//     from `dispatch_sub.rs`) and #831 made that door open the store directly;
//     since `b573e523` (#683 item 7) `scripts/gasp_shim.sh` calls that door for
//     every evolve session, so these functions are the operator lane's only
//     writer. The stated *reason* for the dormancy is gone too: there is no
//     sidecar session record left to destroy.
//   * ITEM (3) IS STILL OPEN — and the two items were never one consumer.
//     Item (7) (this operator lane) shipped; item (3) (the in-process
//     run/model/tool tier, reached by exporting `YOYO_GASP_STATE_DIR` /
//     `YOYO_GASP_GOAL_ID`) has not. Measured, not assumed:
//     `scripts/gasp_shim.sh:150` is the line "DO NOT export
//     YOYO_GASP_STATE_DIR / YOYO_GASP_GOAL_ID here", so `plan_from_env_values`
//     still returns `RecorderPlan::Disabled` in the evolve loop. The hazard
//     outlived the sidecar: this lane still opens and *holds* a run from
//     `session-start` to `session-end`, so a second in-process writer on the
//     same single-writer store would steal the lease exactly as measured on
//     Day 165. See the module doc above and `scripts/gasp_shim.sh:150-194`.
// ---------------------------------------------------------------------------

/// The standing goal an evolve session serves when none is named. Copied
/// unchanged from `tools/gasp-emit/src/main.rs` — a goal id is a node identity
/// in a shared store, so a "nicer" string is a *different graph*.
///
/// `pub(crate)` for [`crate::gasp_cli::run_gasp_command`], which must name the
/// same default when no `--goal` is passed. Widened rather than copied: a second
/// literal here would be a second graph the day one of them is edited.
pub(crate) const DEFAULT_GOAL: &str = "goal_self_improvement";
/// Title/summary for a session goal created on first reference.
const DEFAULT_GOAL_TITLE: &str = "Evolve: improve yoyo's own code, skills, and reliability";
const DEFAULT_GOAL_SUMMARY: &str = "the standing goal every evolve session serves; tasks and patches under it are the self-improvement ratchet";

/// The goal `--kind product` work is rerouted to, so the graph separates value
/// shipped to users from self-investment.
const PRODUCT_GOAL: &str = "goal_product_value";

/// The `--kind product` reroute, as a pure decision.
///
/// Any other kind (including the empty string) keeps the goal it was given.
///
/// `pub(crate)` for [`crate::gasp_cli::run_gasp_command`]'s `task-result` arm:
/// [`task_planned`] applies this reroute itself, [`task_result`] takes no `kind`
/// and so cannot, and the sidecar applies it to *every* arm
/// (`tools/gasp-emit/src/main.rs:107-110`). Calling this one is what keeps the
/// two lanes filing under the same goal; a second `if kind == "product"` at the
/// call site would be a rule with two statements.
pub(crate) fn goal_for_kind(kind: &str, goal: &str) -> String {
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
pub(crate) async fn session_start(
    session: &GraphSession,
    run_id: &str,
    goal: Option<&str>,
    goal_title: Option<&str>,
    goal_summary: Option<&str>,
    day: &str,
    task: Option<&str>,
) -> Result<(), StateError> {
    let goal = session_goal(goal, session.goal()).to_string();
    session_start_in(
        session.state(),
        session.actor(),
        run_id,
        &goal,
        goal_title,
        goal_summary,
        day,
        task,
    )
    .await
}

/// Store-generic body of [`session_start`]. **Not** currently exercised: this
/// doc used to claim the shape made the behaviour "testable against a scratch
/// store without a live recorder", and no such test exists or can exist today
/// — see [`task_result`] for the measured reason. Generic for shape, not for
/// coverage.
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
pub(crate) async fn task_planned(
    session: &GraphSession,
    run_id: &str,
    num: &str,
    title: &str,
    kind: &str,
    goal: Option<&str>,
) -> Result<(), StateError> {
    let goal = session_goal(goal, session.goal()).to_string();
    task_planned_in(
        session.state(),
        session.actor(),
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
pub(crate) async fn session_end(
    session: &GraphSession,
    run_id: &str,
    outcome: Option<&str>,
    goal: Option<&str>,
    extra: &str,
) -> Result<Option<String>, StateError> {
    let goal = session_goal(goal, session.goal()).to_string();
    let outcome = session_outcome(outcome);
    session
        .state()
        .record_run_finished(
            session.actor().clone(),
            RunId::new(run_id),
            outcome.to_string(),
        )
        .await?;
    let sha = session.store().commit_run(
        &RunId::new(run_id),
        &GoalId::new(goal.as_str()),
        outcome,
        &parse_extra_paths(extra),
    )?;
    session.store().release_lease()?;
    Ok(sha)
}

/// Record a finished task: the patch it produced, the oracle that judged it,
/// the decision that followed, and — when reverted — the failure node.
///
/// Ported from the sidecar's `"task-result"` arm
/// (`tools/gasp-emit/src/main.rs:166-268`), the last of the four session-graph
/// arms and the one that stayed unported longest. It was **never blocked by
/// upstream**: `ProjectRef`/`ArtifactRef` landed in yoagent 0.16.4 (#115) and
/// `PatchStatus` in 0.16.5 (#117), all re-exported from `yoagent::gasp`. A
/// stale "unreachable" claim in this module's own doc — true against 0.16.3,
/// false the moment the pin moved — is what actually held it up: six sessions
/// opened this file, read the claim, and exited with an empty diff (#763,
/// #765, #782, #785, #787, #789, #803). The claim was corrected on Day 172;
/// this is the port it was blocking.
///
/// Split like [`task_planned`] rather than [`session_end`]: every call here is
/// an `EventStore` trait method, so the body **is** expressible generically —
/// unlike `session_end`, whose `commit_run`/`release_lease` are `GitEventStore`
/// inherent methods.
///
/// **That generic split does not currently buy a behavioural test, and saying
/// so is the point.** This module's older doc claims the `*_in` bodies are
/// "testable against a scratch store"; they are not, today. `MemoryEventStore`
/// lives in `yoagent-state`, which is deliberately *not* a direct dependency
/// (see Cargo.toml: a default build must never pull it in), and
/// `yoagent::gasp` does not re-export it. Probed and confirmed: the name does
/// not resolve. So no test drives any `*_in` body against a **scratch** store,
/// this one included; the split is kept because it is the right shape and
/// becomes scratch-testable the moment upstream re-exports the store.
/// **Superseded claim, recorded rather than erased (#831):** this paragraph
/// used to conclude *"So zero tests drive any `*_in` body, this one included
/// — the split is kept … not because it is exercised now."* That stopped
/// being true when `tests/gasp_cli_run_ordering.rs` landed: it drives a
/// four-process session through the real binary against a real
/// `GitEventStore` on a tempdir, so this body **is** exercised end-to-end —
/// just never against a scratch store, which is the narrower claim the
/// `MemoryEventStore` fact above actually supports. What IS also tested here
/// is the pure decision half ([`eval_command_or_default`]) and the node-id
/// shapes.
///
/// That last paragraph is a claim about what an external dependency *cannot*
/// do — exactly the genre that went stale above and cost eight sessions — so
/// it carries the same freshness marker, pinned by
/// `tests/doc_version_claims.rs`. If upstream ever re-exports the store, this
/// becomes false silently and the `*_in` bodies stay untested for no reason.
///
/// <!-- yoagent-version-claim: 0.16.6 -->
///
/// `verdict` is compared against `"promoted"` exactly as the sidecar does —
/// the two graphs must agree on what a promotion is, so this is deliberately a
/// byte-identical comparison and not a looser parse.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn task_result(
    session: &GraphSession,
    run_id: &str,
    num: &str,
    title: &str,
    verdict: &str,
    pre_sha: &str,
    post_sha: &str,
    repo: &str,
    branch: Option<&str>,
    reason: &str,
    eval_command: Option<&str>,
    goal: Option<&str>,
) -> Result<(), StateError> {
    let goal = session_goal(goal, session.goal()).to_string();
    task_result_in(
        session.state(),
        session.actor(),
        run_id,
        num,
        title,
        verdict,
        pre_sha,
        post_sha,
        repo,
        branch,
        reason,
        eval_command,
        &goal,
    )
    .await
}

/// The oracle named when the caller does not name one. Byte-identical to the
/// sidecar's fallback: a differently-worded oracle is a different claim about
/// what was checked.
pub(crate) const DEFAULT_EVAL_COMMAND: &str = "cargo fmt+clippy+build+test; evaluator agent";

/// Resolve the oracle string, treating empty exactly as absent.
pub(crate) fn eval_command_or_default(eval_command: Option<&str>) -> &str {
    match eval_command {
        Some(c) if !c.is_empty() => c,
        _ => DEFAULT_EVAL_COMMAND,
    }
}

/// Store-generic body of [`task_result`].
#[allow(clippy::too_many_arguments)]
async fn task_result_in<S: EventStore>(
    state: &YoAgentState<S>,
    actor: &ActorRef,
    run_id: &str,
    num: &str,
    title: &str,
    verdict: &str,
    pre_sha: &str,
    post_sha: &str,
    repo: &str,
    branch: Option<&str>,
    reason: &str,
    eval_command: Option<&str>,
    goal: &str,
) -> Result<(), StateError> {
    let promoted = verdict == "promoted";
    let suffix = format!("{run_id}_{num}");
    ensure_goal(state, goal, actor).await?;

    let mut patch = StatePatch::new(
        PatchId::new(format!("patch_{suffix}")),
        title.to_string(),
        format!("commits {pre_sha}..{post_sha} in {repo}"),
        actor.clone(),
    );
    patch.base_project_ref = Some(ProjectRef {
        repo: repo.to_string(),
        branch: branch.map(str::to_string),
        commit: Some(pre_sha.to_string()),
        worktree: None,
    });
    patch.artifacts =
        vec![ArtifactRef::new("git-commit", format!("{repo}@{post_sha}")).with_hash(post_sha)];
    let patch_id = state.propose_patch(patch).await?;
    state
        .link(
            actor.clone(),
            NodeId::new(format!("patch_{suffix}")),
            "advances",
            NodeId::new(goal),
        )
        .await?;

    // The session's mechanical gate, folded into one eval fact — the caller
    // names its actual oracle so the record never overclaims what was checked.
    let eval_command = eval_command_or_default(eval_command);
    state
        .record_eval(
            actor.clone(),
            EvalResult {
                id: EvalId::new(format!("eval_{suffix}")),
                command: eval_command.into(),
                status: if promoted {
                    EvalStatus::Passed
                } else {
                    EvalStatus::Failed
                },
                score: Some(if promoted { 1.0 } else { 0.0 }),
                metadata: serde_json::json!({ "reason": reason }),
            },
            Some(patch_id.clone()),
        )
        .await?;

    state
        .record_decision_node(
            actor.clone(),
            Decision {
                id: DecisionId::new(format!("decision_{suffix}")),
                status: if promoted {
                    DecisionStatus::Approved
                } else {
                    DecisionStatus::Rejected
                },
                reason: if promoted {
                    format!("oracle passed ({eval_command}); kept")
                } else {
                    format!("reverted to {pre_sha}: {reason}")
                },
                decided_by: actor.clone(),
                metadata: serde_json::json!({}),
            },
            Some(NodeId::new(format!("patch_{suffix}"))),
        )
        .await?;
    state
        .update_patch_status(
            patch_id,
            if promoted {
                PatchStatus::Promoted
            } else {
                PatchStatus::Rejected
            },
            (!reason.is_empty()).then(|| reason.to_string()),
        )
        .await?;

    if !promoted {
        state
            .record_failure(
                actor.clone(),
                NodeId::new(format!("failure_{suffix}")),
                format!("task reverted: {title}"),
                reason.to_string(),
            )
            .await?;
        state
            .link(
                actor.clone(),
                NodeId::new(format!("failure_{suffix}")),
                "produced_by",
                NodeId::new(run_id),
            )
            .await?;
    }
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
    fn eval_command_defaults_only_when_absent_or_empty() {
        // Empty must behave exactly as absent: the sidecar uses
        // `.filter(|s| !s.is_empty())` before its `unwrap_or`, and an empty
        // string reaching the graph would record an oracle that names nothing.
        assert_eq!(eval_command_or_default(None), DEFAULT_EVAL_COMMAND);
        assert_eq!(eval_command_or_default(Some("")), DEFAULT_EVAL_COMMAND);
        // A caller-named oracle is passed through verbatim — the record must
        // say what was actually checked, not a generic stand-in.
        assert_eq!(
            eval_command_or_default(Some("cargo test --lib")),
            "cargo test --lib"
        );
        assert_eq!(eval_command_or_default(Some(" ")), " ");
    }

    #[test]
    fn default_eval_command_matches_the_sidecar_byte_for_byte() {
        // Same discipline as standing_goal_title/summary: this string is an
        // identity in a shared graph, so a reworded constant is a different
        // claim about what judged the patch, not a nicer sentence.
        // Source: tools/gasp-emit/src/main.rs, the "task-result" arm.
        assert_eq!(
            DEFAULT_EVAL_COMMAND,
            "cargo fmt+clippy+build+test; evaluator agent"
        );
    }

    #[test]
    fn task_result_node_ids_have_the_sidecar_shape() {
        // The sidecar builds every task-result node id off
        // `format!("{}_{num}", run_id.as_str())`. These ids are how the two
        // writers' graphs line up, so the shape is pinned rather than assumed.
        let suffix = format!("{}_{}", "run_abc", "02");
        assert_eq!(suffix, "run_abc_02");
        assert_eq!(format!("patch_{suffix}"), "patch_run_abc_02");
        assert_eq!(format!("eval_{suffix}"), "eval_run_abc_02");
        assert_eq!(format!("decision_{suffix}"), "decision_run_abc_02");
        assert_eq!(format!("failure_{suffix}"), "failure_run_abc_02");
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
