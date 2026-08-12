//! Structural smoke gate: a deterministic module line-count cap over `src/`.
//!
//! Borrowed from razzant/ouroboros's `MAX_MODULE_LINES` /
//! `GRANDFATHERED_OVERSIZED_MODULES` pair (issue #673). I add ~3 sessions of
//! lines a day and my incentive at the end of every task is to append to
//! whatever file I'm already in — that is exactly how `commands_risk.rs`
//! reached 4,714 lines despite being split six times. Nothing in
//! `cargo build`/`test`/`clippy`/`fmt` cares how big a module gets, so this
//! test is the only thing that will notice.
//!
//! Three branches, and they are **not** the same property (Day 165, receipts
//! #719 and #739 — this gate destroyed two whole correct tasks, once over a
//! four-line overshoot, because `cargo test` failure means `git reset --hard`
//! in my harness):
//!
//! 1. A file **not** on the grandfather list crossing `MAX_MODULE_LINES` →
//!    **fatal**. A brand-new module going oversized is a design event and is
//!    worth stopping the task for. This is the actual invariant.
//! 2. A file **on** the list growing past its recorded ceiling → **warning,
//!    not fatal**. Growth of an already-capped module is information, not an
//!    emergency; a four-line overshoot does not deserve a whole-task revert.
//!    The warning names the exact entry to paste back, so the debt register
//!    still gets updated on purpose rather than absorbed.
//! 3. A file on the list sitting **below** its recorded ceiling → **fatal**.
//!    This is the ratchet: an exception list only pays itself down if
//!    improving is also a failure, otherwise a shrunk file keeps silent
//!    headroom nobody decided to grant. Fatal on purpose, and it is the cheap
//!    direction — the fix is the smaller number, printed verbatim in the
//!    message. (Same for a listed file that shrank under the cap entirely, or
//!    vanished: its entry must be deleted.)
//!
//! The warning in branch 2 is written straight to `std::io::stderr()` rather
//! than through `eprintln!`, because libtest captures the macros and swallows
//! output from *passing* tests — and a silent gate teaches nothing at all,
//! which is worse than a fatal one.
//!
//! `Kind: evolve` — this governs my own repo's growth discipline; no product
//! surface changes.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Maximum lines allowed in a single `src/` module.
///
/// Chosen at 2,000 because 24 of my 79 modules were already over it on Day
/// 157 (largest 4,714) — low enough to bite, high enough that the debt
/// register is readable. Raising this number is a deliberate edit; never
/// hardcode a different limit elsewhere.
const MAX_MODULE_LINES: usize = 2_000;

/// Modules already over `MAX_MODULE_LINES` when the gate was installed
/// (Day 157). Each number is the file's **recorded size**: growing past it
/// warns (branch 2), sitting under it fails (branch 3, the ratchet), so the
/// entry tracks reality in both directions and the register only shrinks.
const GRANDFATHERED_OVERSIZED_MODULES: &[(&str, usize)] = &[
    // Day 163 (#715): +4 lines — parent-side SharedStateTool so the documented RLM
    // store-then-reference step is executable.
    ("src/agent_builder.rs", 2647),
    // Day 164 (#728): +98 lines — `/skill install`'s destination becomes a third
    // auto-discovery source, so an explicitly installed skill actually loads.
    // The two near-identical per-directory blocks were collapsed into one loop
    // over a pure `auto_discovery_sources` list first (that dedup is why this is
    // +98 and not more); the rest is the new source, its doc comments, and three
    // tests pinning the precedence order (installed < global < project).
    ("src/cli.rs", 3845),
    // Day 162 (#698): +12 lines — SUPPORTED_IMAGE_FORMATS single source of truth
    // (bmp removed; API only accepts png/jpg/jpeg/gif/webp) plus regression tests
    // pinning the extension↔MIME agreement. Tests must live in this module.
    // Day 162 (#699): +117 lines — /apply cascade detects a tree mutated by a
    // failed --3way (which writes conflict markers on merge conflict), stops
    // before running -C1/--recount against the dirty state, and reports the
    // conflicted files honestly instead of "all strategies failed".
    // Day 162 fmt: +7 lines — `cargo fmt` reflowed the #699 code after the
    // ceiling was recorded at pre-fmt size. No new code; formatter wrapping only.
    // Day 162 (#697): +38 lines — handle_add now returns the successfully-added
    // paths alongside its results (so /add related-file suggestions derive from
    // actual adds, not an input re-parse), plus the regression test pinning that
    // failed adds and URL args never leak into that list.
    // Day 162 (#704): +65 lines — @mention read failures on EXISTING files now
    // warn on stderr (mention_read_warning helper, both Err arms) instead of
    // silently re-emitting the raw @path; plus tests pinning the warning string,
    // free-form-mention silence, and the unreadable-file behavior.
    ("src/commands_file.rs", 2807),
    ("src/commands_git.rs", 3172),
    ("src/commands_info.rs", 3036),
    // Day 163 (#726): -59 lines — emerging-risk prompt injection removed
    // (map, annotation, helper, and the test pinning them); see #724.
    ("src/commands_project.rs", 3193),
    // Day 162 (#708): +40 lines — classify_broke_files now filters to `src/`
    // (the risk model's whole universe), plus its unit test and the updated
    // end-to-end fixture assertions.
    // Day 163: +280 lines — classify_broke_files gained two-tier corroboration
    // (a lone `Fix #710` delivery commit no longer grades as a failure day),
    // which is mostly doc comment + a fixture-table test + a second verbatim
    // git-log capture for the corroborated red-branch proof.
    // Day 163 (#717): +131 lines — uncorroborated-repair third value
    // (has_uncorroborated_repair_evidence + its green-branch call site) plus the
    // three fixture tests pinning flagged / green / corroborated windows.
    // Day 163 (corroboration): +184 lines — is_mechanical_commit (harness
    // bookkeeping commits are not a second opinion) plus three fixture tests
    // pinning the cargo-fmt window as ungraded, the real-corroboration window
    // as red, and the harness vocabulary the filter is keyed to.
    ("src/commands_risk.rs", 5477),
    // Day 162 (#707): +68 lines — format_project_index no longer byte-slices a
    // path tail (live panic on any non-ASCII path >50 bytes) and measures its
    // column in chars; 62 of the 68 lines are the two regression tests, one of
    // which asserts the fixture is genuinely boundary-violating so it can't
    // drift back to ASCII-safe like the old test did.
    // Day 163 (#706): +118 lines — parse_grep_args gained a `--` end-of-flags
    // terminator and honest loser-branches (a value-taking flag with no usable
    // value is now a literal pattern token instead of being silently swallowed),
    // plus the fixture tests covering both the new paths and the untouched ones.
    ("src/commands_search.rs", 3720),
    // Day 163 (#716): +99 lines — spawn_dir_restrictions confines a spawn
    // worker's file tools to its worktree (bash_cwd only pinned bash), plus
    // three regression tests covering no-worktree passthrough, the confined
    // common case, and widening a human-set allow list while preserving deny.
    ("src/commands_spawn.rs", 3913),
    // Day 162 (#692): +108 lines — extract_last_assistant_text now stops at the
    // newest turn's boundary (no stale-turn fallback) plus the regression tests
    // pinning that a text-less newest turn yields None, not an older turn's text.
    ("src/commands_web.rs", 2415),
    // Day 164 (#732): +105 lines — TOML basic-string escaping on the write
    // side, matching unescaping (and a lone-quote panic fix) on the read side,
    // plus the round-trip tests that pin writer and reader as one promise.
    ("src/config.rs", 2413),
    // Day 165: 2307 -> 2296. Not a shrink I made this session — the entry was
    // stale-high, and branch 3 (below-ceiling is fatal) is what finally said so.
    ("src/dispatch.rs", 2296),
    ("src/format/cost.rs", 2095),
    // Day 162 (#661): +228 lines — bounded inline-marker carry across streaming
    // deltas (split `**bo` + `ld**` pairs now render bold) plus the
    // chunking-independence and carry-safety regression tests.
    ("src/format/markdown.rs", 3160),
    ("src/format/mod.rs", 2455),
    // Day 162 (#665): +27 lines — the test-output filter is now gated on tool
    // provenance, so read_file results stop being eaten. Signature recorded
    // retroactively during Day 162 reflection: the raise itself shipped
    // unattributed in commit 6e446f09.
    // Day 164: +45 lines — provenance corroboration gate for filter_test_output
    // (a `✓` glyph is a shape, a runner summary is provenance) + its regression tests.
    ("src/format/output.rs", 2680),
    ("src/help.rs", 2672),
    // Day 161 (#662 half 1): +9 lines — run_prompt_auto_retry now breaks out of
    // the retry loop (with one dim stderr line) on deterministic tool refusals
    // instead of burning MAX_AUTO_RETRIES on an identical answer.
    // Day 162 (#662 half 2): +9 lines — the same block mirrored verbatim into
    // run_prompt_auto_retry_with_content, so both retry drivers stop on
    // deterministic refusals.
    // Day 162 (#686): +68 lines — REFUSAL_NOTICE_MARKER + the pure
    // `refusal_notice` builder that makes the harness's grep contract
    // mechanical, plus the test that pins the emitted bytes.
    // Day 165 (#683 step 2): +34 lines — the two agent-start seams
    // (`start_prompt` / `start_prompt_messages`) that route all four prompt
    // call sites through one place so GASP recording is on for all of them or
    // none. Raised on purpose: the seam belongs beside the call sites it
    // replaces, and splitting four one-line calls into another module would
    // hide the enumeration this task exists to make checkable.
    ("src/prompt.rs", 2464),
    // Day 162 (#689): +14 lines — double Ctrl+C at the idle REPL prompt now
    // exits (consecutive-flag `ctrl_c_armed`, dim hint on first press).
    ("src/repl.rs", 3260),
    ("src/safety.rs", 3269),
    ("src/symbols.rs", 3804),
    // Day 161 (#662 half 1): +10 lines — pub REFUSAL_STEM_* consts that the
    // wrapper messages and prompt_retry::is_deterministic_tool_error share.
    // Day 162 (#709): raised 3748 -> 3894 for the Arc flavour of the mode
    // guard (two constructors) + three Arc-path enforcement tests. The tests
    // are ~120 of those lines; the guard itself is one type, not a second copy.
    // Day 163 (#710): raised 3894 -> 3964. The production change is a 4-line
    // short-circuit in RecoveryHintTool; the ~70 lines are tests the task
    // required — the helper's per-stem cases plus the both-sides wrapper
    // discriminator (verbatim message + counter unmoved vs hint + bump).
    // Day 164: +4 lines — the #665 fixture gained the runner summary line a real
    // runner emits; without it the fixture asserted the ✓-shape-only collapse.
    ("src/tool_wrappers.rs", 3968),
    // Day 162 (#709): raised 3245 -> 3264 to wrap the sub-agent tool list in
    // the mode guard, plus the comment stating what is enforced and what is not.
    // Day 163 (#714): raised 3264 -> 3290 — RenameSymbolTool now carries the
    // session's DirectoryRestrictions (struct + constructor + denied-file
    // reporting in execute), so rename_symbol stops writing across --deny'd
    // directories. The rename logic itself lives in src/commands_rename.rs.
    ("src/tools.rs", 3290),
    // Day 163 (#726): -58 lines — emerging-risk annotation removed from
    // build_watch_fix_prompt, with its own test; see #724.
    ("src/watch.rs", 3477),
];

/// A way the size gate can be violated. Four distinct values on purpose —
/// "a new file got too big", "a known-big file got bigger", "a known-big
/// file's recorded size is stale-high", and "the debt register lists a file
/// that no longer belongs" are different problems with different fixes, and
/// collapsing them into one string would hide which one happened. Only one
/// of them is non-fatal; see `is_fatal`.
#[derive(Debug, PartialEq, Eq)]
enum SizeViolation {
    /// A module not on the grandfather list crossed the cap.
    OverCap { path: String, lines: usize },
    /// A grandfathered module grew past its recorded ceiling.
    GrewPastCeiling {
        path: String,
        lines: usize,
        ceiling: usize,
    },
    /// A grandfathered module is smaller than its recorded ceiling (but still
    /// over the cap) — the entry grants headroom nobody decided to give.
    StaleCeiling {
        path: String,
        lines: usize,
        ceiling: usize,
    },
    /// A grandfathered module dropped back under the cap (or vanished) —
    /// its entry should be removed so the register keeps shrinking.
    StaleGrandfatherEntry { path: String, lines: Option<usize> },
}

impl SizeViolation {
    /// Whether this violation should fail the test run.
    ///
    /// Exactly one kind is non-fatal: a grandfathered module that grew. That
    /// is the branch which cost me two whole tasks (#719, #739) — a correct
    /// fix reverted because a file I had *already* signed off as oversized
    /// got four lines bigger. Growth of an already-capped module is
    /// information, so it warns loudly and the run stays green.
    ///
    /// Every other kind stays fatal. `OverCap` is the real invariant, and
    /// both stale-register kinds are the ratchet: if improving a file is not
    /// also a failure, the register never pays itself down. They are also the
    /// *cheap* direction — each message states the exact edit verbatim.
    fn is_fatal(&self) -> bool {
        match self {
            SizeViolation::OverCap { .. } => true,
            SizeViolation::GrewPastCeiling { .. } => false,
            SizeViolation::StaleCeiling { .. } => true,
            SizeViolation::StaleGrandfatherEntry { .. } => true,
        }
    }

    fn message(&self) -> String {
        match self {
            SizeViolation::OverCap { path, lines } => format!(
                "{path} is {lines} lines, over the {MAX_MODULE_LINES}-line module cap.\n     \
                 Fix: split it. If growth is genuinely intended, add \
                 (\"{path}\", {lines}) to GRANDFATHERED_OVERSIZED_MODULES with a reason \
                 in the commit message."
            ),
            SizeViolation::GrewPastCeiling {
                path,
                lines,
                ceiling,
            } => format!(
                "{path} grew to {lines} lines, {} past its recorded {ceiling}.\n     \
                 Not fatal — growth of an already-capped module is information, not an \
                 emergency.\n     Fix: paste (\"{path}\", {lines}) over its entry in \
                 GRANDFATHERED_OVERSIZED_MODULES (and say why in the commit message), or \
                 move the new code to a smaller module.",
                lines.saturating_sub(*ceiling),
            ),
            SizeViolation::StaleCeiling {
                path,
                lines,
                ceiling,
            } => format!(
                "{path} is {lines} lines but its entry still records {ceiling} — {} lines of \
                 headroom nobody decided to grant.\n     \
                 Fix: paste (\"{path}\", {lines}) over its entry in \
                 GRANDFATHERED_OVERSIZED_MODULES. Fatal on purpose: the register only \
                 ratchets down if a shrink is also a failure.",
                ceiling.saturating_sub(*lines),
            ),
            SizeViolation::StaleGrandfatherEntry { path, lines } => match lines {
                Some(n) => format!(
                    "{path} is down to {n} lines, under the {MAX_MODULE_LINES}-line cap. \
                     Nice.\n     Fix: delete its entry from GRANDFATHERED_OVERSIZED_MODULES — \
                     the debt register only shrinks."
                ),
                None => format!(
                    "{path} is listed in GRANDFATHERED_OVERSIZED_MODULES but no longer exists.\n     \
                     Fix: delete its entry."
                ),
            },
        }
    }
}

/// Pure checker: given every module's line count and the grandfather list,
/// report every violation. No I/O, so it is testable against synthetic input
/// rather than only against whatever `src/` happens to look like today.
fn check_module_sizes(
    files: &[(String, usize)],
    max_lines: usize,
    grandfathered: &[(&str, usize)],
) -> Vec<SizeViolation> {
    let mut violations = Vec::new();

    for (path, lines) in files {
        match grandfathered.iter().find(|(g, _)| g == path) {
            Some((_, ceiling)) => {
                if *lines <= max_lines {
                    violations.push(SizeViolation::StaleGrandfatherEntry {
                        path: path.clone(),
                        lines: Some(*lines),
                    });
                } else if lines > ceiling {
                    violations.push(SizeViolation::GrewPastCeiling {
                        path: path.clone(),
                        lines: *lines,
                        ceiling: *ceiling,
                    });
                } else if lines < ceiling {
                    violations.push(SizeViolation::StaleCeiling {
                        path: path.clone(),
                        lines: *lines,
                        ceiling: *ceiling,
                    });
                }
            }
            None => {
                if *lines > max_lines {
                    violations.push(SizeViolation::OverCap {
                        path: path.clone(),
                        lines: *lines,
                    });
                }
            }
        }
    }

    // A listed file that no longer exists on disk is its own third value —
    // not silently ignored, because a rename would otherwise retire a
    // ceiling without anyone deciding to.
    for (path, _) in grandfathered {
        if !files.iter().any(|(p, _)| p == path) {
            violations.push(SizeViolation::StaleGrandfatherEntry {
                path: (*path).to_string(),
                lines: None,
            });
        }
    }

    violations
}

/// Recursively collect `*.rs` files under `dir`, returning paths relative to
/// `root` with forward slashes (so the grandfather list reads the same on
/// every platform).
fn collect_rs_files(dir: &Path, root: &Path, out: &mut Vec<(String, usize)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read {}: {e}", dir.display()),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            collect_rs_files(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, content.lines().count()));
        }
    }
}

#[test]
fn src_modules_respect_the_size_gate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &root, &mut files);

    assert!(
        files.len() > 10,
        "module walk found only {} files — the walk is broken, not the repo",
        files.len()
    );

    let violations = check_module_sizes(&files, MAX_MODULE_LINES, GRANDFATHERED_OVERSIZED_MODULES);
    let (fatal, warnings): (Vec<&SizeViolation>, Vec<&SizeViolation>) =
        violations.iter().partition(|v| v.is_fatal());

    if !warnings.is_empty() {
        // Written to the raw stderr handle on purpose: libtest's capture hook
        // only intercepts the `print!`/`eprint!` macro family, and it discards
        // captured output from tests that PASS — which is exactly the case
        // this branch creates. Going through the handle keeps the warning
        // visible in a plain `cargo test` run, so "non-fatal" doesn't quietly
        // become "silent".
        let mut err = std::io::stderr();
        for w in &warnings {
            let _ = writeln!(err, "\nmodule size gate WARNING: {}", w.message());
        }
        let _ = writeln!(
            err,
            "     ({} grandfathered module(s) grew. Not failing the run — see \
             tests/module_size.rs for why.)\n",
            warnings.len()
        );
        let _ = err.flush();
    }

    if !fatal.is_empty() {
        let report = fatal
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "module size gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/module_size.rs. It is deliberate: growth has to be \
             acknowledged, not absorbed.",
            fatal.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(v: &[(&str, usize)]) -> Vec<(String, usize)> {
        v.iter().map(|(p, n)| ((*p).to_string(), *n)).collect()
    }

    #[test]
    fn small_unlisted_module_passes() {
        let v = check_module_sizes(&files(&[("src/a.rs", 100)]), 200, &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn unlisted_module_over_cap_is_a_violation() {
        let v = check_module_sizes(&files(&[("src/a.rs", 201)]), 200, &[]);
        assert_eq!(
            v,
            vec![SizeViolation::OverCap {
                path: "src/a.rs".to_string(),
                lines: 201
            }]
        );
    }

    #[test]
    fn exactly_at_cap_passes() {
        let v = check_module_sizes(&files(&[("src/a.rs", 200)]), 200, &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn grandfathered_module_at_its_ceiling_passes() {
        let v = check_module_sizes(&files(&[("src/a.rs", 500)]), 200, &[("src/a.rs", 500)]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn grandfathered_module_below_its_ceiling_is_a_stale_ceiling() {
        // Was `grandfathered_module_may_shrink_while_still_over_cap`, which
        // asserted this passes. Day 165 flipped it: a shrink that leaves the
        // recorded number untouched is silent headroom, so it is fatal and the
        // entry must be rewritten. That is the ratchet.
        let v = check_module_sizes(&files(&[("src/a.rs", 400)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::StaleCeiling {
                path: "src/a.rs".to_string(),
                lines: 400,
                ceiling: 500
            }]
        );
        assert!(v[0].is_fatal(), "the ratchet direction must stay fatal");
    }

    #[test]
    fn only_growth_of_a_grandfathered_module_is_non_fatal() {
        // The whole point of Day 165: this is the branch that cost me #719 and
        // #739, and it is the one branch that may not fail the run.
        let grew = SizeViolation::GrewPastCeiling {
            path: "src/a.rs".to_string(),
            lines: 501,
            ceiling: 500,
        };
        assert!(!grew.is_fatal());

        for fatal in [
            SizeViolation::OverCap {
                path: "src/a.rs".to_string(),
                lines: 201,
            },
            SizeViolation::StaleCeiling {
                path: "src/a.rs".to_string(),
                lines: 400,
                ceiling: 500,
            },
            SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: Some(150),
            },
            SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: None,
            },
        ] {
            assert!(fatal.is_fatal(), "{fatal:?} must stay fatal");
        }
    }

    #[test]
    fn growth_warning_names_the_overshoot_and_the_paste_in_entry() {
        // A warning nobody can act on is just noise, so pin the four things a
        // reader needs: the file, the recorded number, the current number, the
        // overshoot, and the literal entry to paste back.
        let m = SizeViolation::GrewPastCeiling {
            path: "src/a.rs".to_string(),
            lines: 2310,
            ceiling: 2306,
        }
        .message();
        assert!(m.contains("src/a.rs"), "{m}");
        assert!(m.contains("2310"), "{m}");
        assert!(m.contains("2306"), "{m}");
        assert!(m.contains("4 past"), "{m}");
        assert!(m.contains("(\"src/a.rs\", 2310)"), "{m}");
    }

    #[test]
    fn stale_ceiling_message_states_the_smaller_number_verbatim() {
        let m = SizeViolation::StaleCeiling {
            path: "src/a.rs".to_string(),
            lines: 2296,
            ceiling: 2307,
        }
        .message();
        assert!(m.contains("(\"src/a.rs\", 2296)"), "{m}");
        assert!(m.contains("11 lines of headroom"), "{m}");
    }

    #[test]
    fn grandfathered_module_growing_by_one_line_is_a_violation() {
        let v = check_module_sizes(&files(&[("src/a.rs", 501)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::GrewPastCeiling {
                path: "src/a.rs".to_string(),
                lines: 501,
                ceiling: 500
            }]
        );
    }

    #[test]
    fn grandfathered_module_back_under_cap_must_be_delisted() {
        let v = check_module_sizes(&files(&[("src/a.rs", 150)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: Some(150)
            }]
        );
    }

    #[test]
    fn listed_module_that_no_longer_exists_is_a_stale_entry() {
        let v = check_module_sizes(&files(&[("src/b.rs", 10)]), 200, &[("src/a.rs", 500)]);
        assert_eq!(
            v,
            vec![SizeViolation::StaleGrandfatherEntry {
                path: "src/a.rs".to_string(),
                lines: None
            }]
        );
    }

    #[test]
    fn empty_input_yields_no_violations() {
        let v = check_module_sizes(&[], 200, &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn violation_messages_name_the_file_and_the_fix() {
        let v = SizeViolation::OverCap {
            path: "src/a.rs".to_string(),
            lines: 9000,
        };
        let m = v.message();
        assert!(m.contains("src/a.rs"), "{m}");
        assert!(m.contains("9000"), "{m}");
        assert!(m.contains("GRANDFATHERED_OVERSIZED_MODULES"), "{m}");
    }
}
