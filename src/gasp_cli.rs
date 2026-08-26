//! `yoyo gasp <arm>` — the CLI door onto the four ported GASP session-graph arms
//! in [`crate::gasp`] (#827, which is #683 item (7)'s missing prerequisite).
//!
//! Why this exists: all four arms (`session_start` / `task_planned` /
//! `task_result` / `session_end`) landed on Day 177 with **zero callers**, and
//! the thing that needs to call them — `scripts/gasp_shim.sh` — is shell. So
//! there was no path from the shim to the ported code, and `tools/gasp-emit`
//! could not be retired. This builds the door and **wires nothing**: no env var
//! is set, `RecorderPlan::Disabled` stays the live path, and the sidecar stays
//! exactly where it is.
//!
//! The flag contract is copied from `tools/gasp-emit/src/main.rs` and checked
//! against the argv `scripts/gasp_shim.sh` actually passes (lines 132, 176, 185,
//! 491) — a door with different flag names is a door onto a different building.
//!
//! **The split is the deliverable's own safety net.** A plain `cargo test`
//! compiles **none** of `crate::gasp` (the `gasp` feature is default-off) and
//! would report green over a completely broken door. So [`parse_gasp_args`] —
//! the whole argument contract — is compiled and table-tested *unconditionally*,
//! and only [`run_gasp_command`], which touches the store, sits behind the
//! feature.
//!
//! **Two deliberate deviations from the sidecar, both stated rather than
//! silent:**
//!
//! 1. A **dangling flag** (`--run-id` as the last token) is an `Err` here. The
//!    sidecar's parser leaves it as an empty string, so `req` accepts it and the
//!    run is recorded under an empty id. Empty *values* are still accepted
//!    (`--kind ""` is exactly how the shim spells "unset"); it is the missing
//!    value token that is refused.
//! 2. **`--worker` is accepted and cannot be honoured.** [`crate::gasp`] opens
//!    every recorder with the fixed worker id `yoyo-inproc`
//!    (`gasp::open_recorder`), deliberately, so in-process runs stay tellable
//!    apart from sidecar-written ones. The shim passes `--worker
//!    evolve-shim-$$` precisely so that overlapping sessions cannot share a
//!    *lease identity* — a property the sidecar has and this port does not. The
//!    flag is therefore parsed (the shim's argv must not error) and its
//!    non-effect is **announced on stderr**, never silently dropped. That gap is
//!    real work for #683 step 3, not a cosmetic mismatch.

/// Flags every arm carries. The shim passes all four to all four arms.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "gasp"), allow(dead_code))]
pub(crate) struct GaspCommon {
    /// `--state-dir`: the GASP agent-repo root. Required.
    pub state_dir: String,
    /// `--run-id`: the run these events chain to. Required.
    pub run_id: String,
    /// `--goal`: the standing goal. Empty means "unset" (the shim spells it that
    /// way), so it normalises to `None` and the arm falls back to `DEFAULT_GOAL`.
    pub goal: Option<String>,
    /// `--worker`: parsed for argv compatibility, **not honoured** — see the
    /// module doc. Kept rather than dropped so the shim's invocation still
    /// parses, and announced rather than ignored.
    pub worker: Option<String>,
}

/// One variant per sidecar arm, carrying exactly the fields that arm's ported
/// function takes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "gasp"), allow(dead_code))]
pub(crate) enum GaspCommand {
    SessionStart {
        common: GaspCommon,
        /// `--day`. Display-only; empty is a legitimate value.
        day: String,
        /// `--task`: the run's human label.
        task: Option<String>,
        /// `--goal-title` / `--goal-summary`: used only when the goal node is
        /// created on first reference.
        goal_title: Option<String>,
        goal_summary: Option<String>,
    },
    Task {
        common: GaspCommon,
        num: String,
        title: String,
        /// `--kind`: `product` reroutes to the product-value goal. Empty means
        /// unset and passes through.
        kind: String,
    },
    TaskResult {
        common: GaspCommon,
        num: String,
        title: String,
        verdict: String,
        pre_sha: String,
        post_sha: String,
        repo: String,
        branch: Option<String>,
        /// `--eval-command`: the oracle actually run. Empty → `None` → the
        /// port's own default, so the record never overclaims.
        eval_command: Option<String>,
        reason: String,
        kind: String,
    },
    SessionEnd {
        common: GaspCommon,
        outcome: Option<String>,
        /// `--extra`: comma-separated repo-relative paths committed with the
        /// boundary commit.
        extra: String,
    },
}

/// The four arm names, in the order the usage line lists them.
pub(crate) const GASP_ARMS: &[&str] = &["session-start", "task", "task-result", "session-end"];

/// The values `--kind` accepts. Empty is also accepted and means "unset" — the
/// shim passes `--kind ""` whenever `GASP_TASK_KIND` is not set.
const GASP_KINDS: &[&str] = &["product", "evolve"];

/// Split `args` into flag/value pairs, mirroring the sidecar's parser: **every
/// flag takes exactly one value, and a pending flag consumes the next token
/// unconditionally**, so an LLM-authored value that starts with `--` (a task
/// title, a revert reason) stays a value rather than becoming a flag.
///
/// The one deviation: a flag left dangling at the end is an `Err` naming it,
/// where the sidecar silently gives it an empty string.
fn split_flags(args: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut pending: Option<String> = None;
    for arg in args {
        if let Some(name) = pending.take() {
            out.push((name, arg.clone()));
        } else if let Some(name) = arg.strip_prefix("--") {
            if name.is_empty() {
                return Err("`--` is not a flag name".to_string());
            }
            pending = Some(name.to_string());
        } else {
            return Err(format!(
                "unexpected positional argument `{arg}` — every gasp value is passed as `--flag <value>`"
            ));
        }
    }
    match pending {
        Some(name) => Err(format!("--{name} was given with no value")),
        None => Ok(out),
    }
}

/// Last-wins lookup, matching the sidecar's `HashMap` insert.
fn flag<'a>(flags: &'a [(String, String)], name: &str) -> Option<&'a str> {
    flags
        .iter()
        .rev()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// A required flag: present, with a value. An empty value is refused here
/// because every required field names an identity (a run, a task, a sha) and an
/// empty identity is a silently wrong record, not a default.
fn required(flags: &[(String, String)], name: &str) -> Result<String, String> {
    match flag(flags, name) {
        Some(v) if !v.trim().is_empty() => Ok(v.to_string()),
        Some(_) => Err(format!("--{name} is required and was empty")),
        None => Err(format!("--{name} is required")),
    }
}

/// An optional flag where **empty means absent** — the shim passes every flag
/// unconditionally and spells "unset" as `""`, so collapsing the two here is the
/// honest reading of that contract rather than a convenience.
fn optional(flags: &[(String, String)], name: &str) -> Option<String> {
    flag(flags, name)
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
}

/// An optional flag whose absence is the empty string (the arm takes `&str`).
fn optional_str(flags: &[(String, String)], name: &str) -> String {
    flag(flags, name).unwrap_or("").to_string()
}

/// Validate `--kind` rather than guessing: an unrecognised kind would silently
/// pass through the `product` reroute and file the work under the wrong goal.
fn kind_of(flags: &[(String, String)]) -> Result<String, String> {
    let kind = optional_str(flags, "kind");
    if kind.is_empty() || GASP_KINDS.contains(&kind.as_str()) {
        Ok(kind)
    } else {
        Err(format!(
            "--kind `{kind}` is not one of: {} (or empty for unset)",
            GASP_KINDS.join(", ")
        ))
    }
}

fn common_of(flags: &[(String, String)]) -> Result<GaspCommon, String> {
    Ok(GaspCommon {
        state_dir: required(flags, "state-dir")?,
        run_id: required(flags, "run-id")?,
        goal: optional(flags, "goal"),
        worker: optional(flags, "worker"),
    })
}

/// The pure half: `args` is everything after `yoyo gasp`, so `args[0]` is the
/// arm name. No I/O, no store, no async — table-tested by a plain `cargo test`,
/// which is the only gate that compiles anything in this file by default.
pub(crate) fn parse_gasp_args(args: &[String]) -> Result<GaspCommand, String> {
    let arm = args.first().map(String::as_str).unwrap_or("");
    if arm.is_empty() {
        return Err(format!(
            "missing arm — expected one of: {}",
            GASP_ARMS.join(", ")
        ));
    }
    let flags = split_flags(&args[1..])?;

    match arm {
        "session-start" => Ok(GaspCommand::SessionStart {
            common: common_of(&flags)?,
            day: optional_str(&flags, "day"),
            task: optional(&flags, "task"),
            goal_title: optional(&flags, "goal-title"),
            goal_summary: optional(&flags, "goal-summary"),
        }),
        "task" => Ok(GaspCommand::Task {
            common: common_of(&flags)?,
            num: required(&flags, "num")?,
            title: required(&flags, "title")?,
            kind: kind_of(&flags)?,
        }),
        "task-result" => Ok(GaspCommand::TaskResult {
            common: common_of(&flags)?,
            num: required(&flags, "num")?,
            title: required(&flags, "title")?,
            verdict: required(&flags, "verdict")?,
            pre_sha: required(&flags, "pre-sha")?,
            post_sha: required(&flags, "post-sha")?,
            repo: required(&flags, "repo")?,
            branch: optional(&flags, "branch"),
            eval_command: optional(&flags, "eval-command"),
            reason: optional_str(&flags, "reason"),
            kind: kind_of(&flags)?,
        }),
        "session-end" => Ok(GaspCommand::SessionEnd {
            common: common_of(&flags)?,
            outcome: optional(&flags, "outcome"),
            extra: optional_str(&flags, "extra"),
        }),
        other => Err(format!(
            "unknown gasp arm `{other}` — expected one of: {}",
            GASP_ARMS.join(", ")
        )),
    }
}

/// The one-line usage, printed on any parse error. Compiled unconditionally so
/// the refusal path in a default build can print it too.
pub(crate) fn gasp_usage() -> String {
    format!(
        "usage: yoyo gasp <{}> --state-dir <dir> --run-id <id> [...]",
        GASP_ARMS.join("|")
    )
}

/// Does this arm chain to a run some **earlier process** opened?
///
/// `session-start` opens the run, so there is nothing to resume; every other
/// arm records into a run a previous `yoyo gasp` invocation started, and the
/// open-run marker is in-memory only — so it has to be restored from the
/// store's own event log or the fact lands uncorrelated. Byte-identical to the
/// sidecar's `if cmd != "session-start" { state.resume_open_run().await? }`
/// (`tools/gasp-emit/src/main.rs:115`).
///
/// Pure, so it is table-tested in a **plain** build even though the arm that
/// consumes it only compiles under `--features gasp`.
#[cfg_attr(not(feature = "gasp"), allow(dead_code))]
pub(crate) fn needs_open_run(cmd: &GaspCommand) -> bool {
    !matches!(cmd, GaspCommand::SessionStart { .. })
}

/// The impure half: open the store and call the matching ported arm.
///
/// Returns the boundary sha for `session-end` (which *returns* rather than
/// prints — that was a deliberate in-process choice, so the CLI arm is the
/// caller that decides) and `None` for the other three.
///
/// **Superseded claim, recorded rather than erased (#831).** This doc used to
/// read: *"The recorder is opened the same way `open_recorder_from_env` does —
/// by handing a `RecorderPlan` to `open_recorder` — rather than by
/// constructing a second `GaspRecorder::open` call that could drift from it."*
/// Sharing the opener with the in-process path was exactly the defect: the
/// shim emits **one event per process**, and `GaspRecorder::with_store` closes
/// any run a previous process left open as `"interrupted"` on **every** open,
/// so call 2 killed the run call 1 started and `session-end` made no boundary
/// commit — while every call exited 0. The two tiers want opposite things from
/// an open, so they no longer share one.
///
/// The store is now opened directly ([`crate::gasp::open_graph_session`]), as
/// `tools/gasp-emit` always did. What *is* still shared with the in-process
/// path, and deliberately so, is the [`crate::gasp::RecorderPlan`] decision —
/// one statement of "what do the two env values ask for".
#[cfg(feature = "gasp")]
pub(crate) async fn run_gasp_command(cmd: GaspCommand) -> Result<Option<String>, String> {
    use crate::gasp;

    let common = match &cmd {
        GaspCommand::SessionStart { common, .. }
        | GaspCommand::Task { common, .. }
        | GaspCommand::TaskResult { common, .. }
        | GaspCommand::SessionEnd { common, .. } => common.clone(),
    };

    // An invisible non-effect is a bug even when it is the right non-effect.
    if let Some(worker) = &common.worker {
        eprintln!(
            "gasp: --worker {worker} ignored — in-process runs are recorded under a \
             fixed worker id so they stay tellable apart from sidecar-written ones. \
             Per-session lease identity is not yet portable (#683 step 3)."
        );
    }

    let goal_id = common
        .goal
        .clone()
        .unwrap_or_else(|| gasp::DEFAULT_GOAL.to_string());
    let plan = gasp::plan_from_env_values(Some(&common.state_dir), Some(&goal_id));
    // Directly, not via `open_recorder`: the recorder would close this
    // session's own open run as "interrupted" (#831).
    let session = gasp::open_graph_session(plan, needs_open_run(&cmd))
        .await
        .ok_or_else(|| format!("could not open a GASP store at {}", common.state_dir))?;

    let run_id = common.run_id.as_str();
    let goal = common.goal.as_deref();

    match &cmd {
        GaspCommand::SessionStart {
            day,
            task,
            goal_title,
            goal_summary,
            ..
        } => {
            gasp::session_start(
                &session,
                run_id,
                goal,
                goal_title.as_deref(),
                goal_summary.as_deref(),
                day,
                task.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(None)
        }
        GaspCommand::Task {
            num, title, kind, ..
        } => {
            // `task_planned` applies the `--kind product` reroute itself.
            gasp::task_planned(&session, run_id, num, title, kind, goal)
                .await
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        GaspCommand::TaskResult {
            num,
            title,
            verdict,
            pre_sha,
            post_sha,
            repo,
            branch,
            eval_command,
            reason,
            kind,
            ..
        } => {
            // `task_result` takes no `kind`, so the reroute is applied here —
            // through the port's own rule, never a second copy of it. The
            // sidecar reroutes globally before dispatch, so a product-kind task
            // result that skipped this would file under the wrong goal.
            let rerouted = gasp::goal_for_kind(kind, goal.unwrap_or(gasp::DEFAULT_GOAL));
            gasp::task_result(
                &session,
                run_id,
                num,
                title,
                verdict,
                pre_sha,
                post_sha,
                repo,
                branch.as_deref(),
                reason,
                eval_command.as_deref(),
                Some(rerouted.as_str()),
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(None)
        }
        GaspCommand::SessionEnd { outcome, extra, .. } => {
            gasp::session_end(&session, run_id, outcome.as_deref(), goal, extra)
                .await
                .map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    /// The exact argv `scripts/gasp_shim.sh:132` passes, minus the binary name.
    #[test]
    fn session_start_happy_path_matches_the_shim_argv() {
        let cmd = parse_gasp_args(&argv(&[
            "session-start",
            "--state-dir",
            "yoyo-gasp",
            "--run-id",
            "run_evolve178_20260825T212400Z",
            "--worker",
            "evolve-shim-4242",
            "--day",
            "178",
            "--goal",
            "goal_self_improvement",
            "--goal-title",
            "Evolve yoyo",
            "--goal-summary",
            "the standing goal",
            "--task",
            "evolve session day 178",
        ]))
        .expect("the shim's own session-start argv must parse");

        match cmd {
            GaspCommand::SessionStart {
                common,
                day,
                task,
                goal_title,
                goal_summary,
            } => {
                assert_eq!(common.state_dir, "yoyo-gasp");
                assert_eq!(common.run_id, "run_evolve178_20260825T212400Z");
                assert_eq!(common.goal.as_deref(), Some("goal_self_improvement"));
                assert_eq!(common.worker.as_deref(), Some("evolve-shim-4242"));
                assert_eq!(day, "178");
                assert_eq!(task.as_deref(), Some("evolve session day 178"));
                assert_eq!(goal_title.as_deref(), Some("Evolve yoyo"));
                assert_eq!(goal_summary.as_deref(), Some("the standing goal"));
            }
            other => panic!("expected SessionStart, got {other:?}"),
        }
    }

    #[test]
    fn task_happy_path_matches_the_shim_argv() {
        let cmd = parse_gasp_args(&argv(&[
            "task",
            "--state-dir",
            "yoyo-gasp",
            "--run-id",
            "run_1",
            "--worker",
            "evolve-shim-1",
            "--goal",
            "goal_self_improvement",
            "--kind",
            "product",
            "--num",
            "2",
            "--title",
            "give the gasp arms a CLI door",
        ]))
        .expect("the shim's own task argv must parse");

        match cmd {
            GaspCommand::Task {
                common,
                num,
                title,
                kind,
            } => {
                assert_eq!(common.run_id, "run_1");
                assert_eq!(num, "2");
                assert_eq!(title, "give the gasp arms a CLI door");
                assert_eq!(kind, "product");
            }
            other => panic!("expected Task, got {other:?}"),
        }
    }

    #[test]
    fn task_result_happy_path_matches_the_shim_argv() {
        let cmd = parse_gasp_args(&argv(&[
            "task-result",
            "--state-dir",
            "yoyo-gasp",
            "--run-id",
            "run_1",
            "--worker",
            "evolve-shim-1",
            "--goal",
            "goal_self_improvement",
            "--kind",
            "evolve",
            "--num",
            "1",
            "--title",
            "fix the deletion parser",
            "--verdict",
            "promoted",
            "--pre-sha",
            "aaaa111",
            "--post-sha",
            "bbbb222",
            "--repo",
            "yologdev/yoyo-evolve",
            "--branch",
            "main",
            "--eval-command",
            "cargo test",
            "--reason",
            "",
        ]))
        .expect("the shim's own task-result argv must parse");

        match cmd {
            GaspCommand::TaskResult {
                common,
                num,
                title,
                verdict,
                pre_sha,
                post_sha,
                repo,
                branch,
                eval_command,
                reason,
                kind,
            } => {
                assert_eq!(common.state_dir, "yoyo-gasp");
                assert_eq!(num, "1");
                assert_eq!(title, "fix the deletion parser");
                assert_eq!(verdict, "promoted");
                assert_eq!(pre_sha, "aaaa111");
                assert_eq!(post_sha, "bbbb222");
                assert_eq!(repo, "yologdev/yoyo-evolve");
                assert_eq!(branch.as_deref(), Some("main"));
                assert_eq!(eval_command.as_deref(), Some("cargo test"));
                // `--reason ""` is how the shim spells "no reason", and the
                // ported arm takes `&str`, so it stays the empty string.
                assert_eq!(reason, "");
                assert_eq!(kind, "evolve");
            }
            other => panic!("expected TaskResult, got {other:?}"),
        }
    }

    #[test]
    fn session_end_happy_path_matches_the_shim_argv() {
        let cmd = parse_gasp_args(&argv(&[
            "session-end",
            "--state-dir",
            "yoyo-gasp",
            "--run-id",
            "run_1",
            "--worker",
            "evolve-shim-1",
            "--goal",
            "goal_self_improvement",
            "--extra",
            "skills,memory",
            "--outcome",
            "done",
        ]))
        .expect("the shim's own session-end argv must parse");

        match cmd {
            GaspCommand::SessionEnd {
                common,
                outcome,
                extra,
            } => {
                assert_eq!(common.goal.as_deref(), Some("goal_self_improvement"));
                assert_eq!(outcome.as_deref(), Some("done"));
                assert_eq!(extra, "skills,memory");
            }
            other => panic!("expected SessionEnd, got {other:?}"),
        }
    }

    /// The shim passes every optional flag unconditionally and spells "unset"
    /// as `""` — so an empty optional must read as absent, not as a value.
    #[test]
    fn empty_optional_values_read_as_absent() {
        let cmd = parse_gasp_args(&argv(&[
            "session-end",
            "--state-dir",
            "d",
            "--run-id",
            "r",
            "--goal",
            "",
            "--worker",
            "",
            "--outcome",
            "",
            "--extra",
            "",
        ]))
        .expect("empty optional values are the shim's spelling of `unset`");
        match cmd {
            GaspCommand::SessionEnd {
                common,
                outcome,
                extra,
            } => {
                assert_eq!(common.goal, None);
                assert_eq!(common.worker, None);
                assert_eq!(outcome, None);
                assert_eq!(extra, "");
            }
            other => panic!("expected SessionEnd, got {other:?}"),
        }
    }

    /// One missing-required-flag case per arm, and the error must name the flag
    /// — an error that only says "bad arguments" makes the shim's failure log
    /// useless.
    #[test]
    fn missing_required_flag_is_named_for_every_arm() {
        let cases: &[(&[&str], &str)] = &[
            (&["session-start", "--run-id", "r"], "state-dir"),
            (&["session-start", "--state-dir", "d"], "run-id"),
            (
                &["task", "--state-dir", "d", "--run-id", "r", "--title", "t"],
                "num",
            ),
            (
                &["task", "--state-dir", "d", "--run-id", "r", "--num", "1"],
                "title",
            ),
            (
                &[
                    "task-result",
                    "--state-dir",
                    "d",
                    "--run-id",
                    "r",
                    "--num",
                    "1",
                    "--title",
                    "t",
                    "--pre-sha",
                    "a",
                    "--post-sha",
                    "b",
                    "--repo",
                    "o/r",
                ],
                "verdict",
            ),
            (
                &[
                    "task-result",
                    "--state-dir",
                    "d",
                    "--run-id",
                    "r",
                    "--num",
                    "1",
                    "--title",
                    "t",
                    "--verdict",
                    "promoted",
                    "--pre-sha",
                    "a",
                    "--post-sha",
                    "b",
                ],
                "repo",
            ),
            (&["session-end", "--state-dir", "d"], "run-id"),
        ];
        for (args, flag_name) in cases {
            let err = parse_gasp_args(&argv(args))
                .expect_err(&format!("{args:?} is missing --{flag_name}"));
            assert!(
                err.contains(flag_name),
                "the error for {args:?} must name --{flag_name}, got: {err}"
            );
        }
    }

    /// A required flag present but empty is refused too: an empty run id is a
    /// silently wrong record, not a default.
    #[test]
    fn empty_required_flag_is_refused_and_named() {
        let err = parse_gasp_args(&argv(&[
            "session-start",
            "--state-dir",
            "d",
            "--run-id",
            "",
        ]))
        .expect_err("an empty --run-id must not be accepted");
        assert!(err.contains("run-id"), "got: {err}");
    }

    #[test]
    fn unknown_arm_names_the_four_valid_ones() {
        let err = parse_gasp_args(&argv(&["session-strat", "--state-dir", "d"]))
            .expect_err("an unknown arm must be refused");
        for arm in GASP_ARMS {
            assert!(err.contains(arm), "the error must name `{arm}`, got: {err}");
        }
        assert!(err.contains("session-strat"), "got: {err}");
    }

    #[test]
    fn missing_arm_names_the_four_valid_ones() {
        let err = parse_gasp_args(&[]).expect_err("no arm at all must be refused");
        for arm in GASP_ARMS {
            assert!(err.contains(arm), "the error must name `{arm}`, got: {err}");
        }
    }

    /// A trailing flag with no value is an `Err` naming it — never a panic, and
    /// never a silently swallowed neighbour. This is the one place the parser
    /// deliberately diverges from the sidecar, which records an empty value.
    #[test]
    fn dangling_flag_is_refused_rather_than_swallowing_a_neighbour() {
        let err = parse_gasp_args(&argv(&["session-end", "--state-dir", "d", "--run-id"]))
            .expect_err("a dangling --run-id must be refused");
        assert!(err.contains("run-id"), "got: {err}");
        assert!(err.contains("no value"), "got: {err}");
    }

    #[test]
    fn unrecognised_kind_is_refused_rather_than_guessed() {
        let err = parse_gasp_args(&argv(&[
            "task",
            "--state-dir",
            "d",
            "--run-id",
            "r",
            "--num",
            "1",
            "--title",
            "t",
            "--kind",
            "prodcut",
        ]))
        .expect_err("a misspelled --kind must not pass through the reroute");
        assert!(err.contains("prodcut"), "got: {err}");
        assert!(err.contains("product"), "got: {err}");
        assert!(err.contains("evolve"), "got: {err}");
    }

    /// The near-miss guard: an empty `--kind` is how the shim spells "unset" and
    /// must keep passing. A discriminator tested only on the side that blocks is
    /// vacuous green.
    #[test]
    fn empty_kind_still_passes() {
        let cmd = parse_gasp_args(&argv(&[
            "task",
            "--state-dir",
            "d",
            "--run-id",
            "r",
            "--num",
            "1",
            "--title",
            "t",
            "--kind",
            "",
        ]))
        .expect("`--kind \"\"` is the shim's spelling of unset");
        match cmd {
            GaspCommand::Task { kind, .. } => assert_eq!(kind, ""),
            other => panic!("expected Task, got {other:?}"),
        }
    }

    /// The sidecar's own rule: a pending flag consumes the next token
    /// unconditionally, so an LLM-authored value starting with `--` (a revert
    /// reason, a task title) stays a value.
    #[test]
    fn a_value_starting_with_dashes_stays_a_value() {
        let cmd = parse_gasp_args(&argv(&[
            "task",
            "--state-dir",
            "d",
            "--run-id",
            "r",
            "--num",
            "1",
            "--title",
            "--verdict is not a flag here",
        ]))
        .expect("a value beginning with `--` must not become a flag");
        match cmd {
            GaspCommand::Task { title, .. } => {
                assert_eq!(title, "--verdict is not a flag here");
            }
            other => panic!("expected Task, got {other:?}"),
        }
    }

    #[test]
    fn a_stray_positional_is_refused() {
        let err = parse_gasp_args(&argv(&["session-end", "yoyo-gasp", "--run-id", "r"]))
            .expect_err("a bare positional must not be silently ignored");
        assert!(err.contains("yoyo-gasp"), "got: {err}");
    }

    #[test]
    fn usage_names_every_arm() {
        let usage = gasp_usage();
        for arm in GASP_ARMS {
            assert!(usage.contains(arm), "usage must name `{arm}`: {usage}");
        }
    }
}
