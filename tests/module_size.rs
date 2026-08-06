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
//! Two rules, both deliberate:
//!
//! 1. A file not on the grandfather list may not cross `MAX_MODULE_LINES`.
//! 2. A file **on** the list may not grow past the ceiling recorded for it.
//!    The list is a debt register, not a loophole: raising a number is a
//!    reviewable one-line diff that says "I chose to make this bigger",
//!    never an absorbed threshold bump.
//!
//! When a listed file shrinks back under the cap, its entry must be deleted —
//! that is the register paying itself down, and the gate says so out loud.
//!
//! `Kind: evolve` — this governs my own repo's growth discipline; no product
//! surface changes.

use std::path::{Path, PathBuf};

/// Maximum lines allowed in a single `src/` module.
///
/// Chosen at 2,000 because 24 of my 79 modules were already over it on Day
/// 157 (largest 4,714) — low enough to bite, high enough that the debt
/// register is readable. Raising this number is a deliberate edit; never
/// hardcode a different limit elsewhere.
const MAX_MODULE_LINES: usize = 2_000;

/// Modules already over `MAX_MODULE_LINES` when the gate was installed
/// (Day 157). Each number is a **ceiling**, not a target: the file may
/// shrink freely, and may not grow by even one line without this list being
/// edited on purpose.
const GRANDFATHERED_OVERSIZED_MODULES: &[(&str, usize)] = &[
    ("src/agent_builder.rs", 2643),
    ("src/cli.rs", 3717),
    ("src/commands_file.rs", 2568),
    ("src/commands_git.rs", 3172),
    ("src/commands_info.rs", 3036),
    ("src/commands_project.rs", 3252),
    ("src/commands_risk.rs", 4714),
    ("src/commands_search.rs", 3534),
    ("src/commands_spawn.rs", 3814),
    ("src/commands_web.rs", 2307),
    ("src/config.rs", 2308),
    ("src/dispatch.rs", 2307),
    ("src/format/cost.rs", 2095),
    ("src/format/markdown.rs", 2932),
    ("src/format/mod.rs", 2455),
    ("src/format/output.rs", 2608),
    ("src/help.rs", 2672),
    ("src/prompt.rs", 2344),
    ("src/repl.rs", 3246),
    ("src/safety.rs", 3155),
    ("src/symbols.rs", 3804),
    ("src/tool_wrappers.rs", 3665),
    ("src/tools.rs", 3245),
    ("src/watch.rs", 3535),
];

/// A way the size gate can be violated. Three distinct values on purpose —
/// "a new file got too big", "a known-big file got bigger", and "the debt
/// register is stale" are different problems with different fixes, and
/// collapsing them into one string would hide which one happened.
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
    /// A grandfathered module dropped back under the cap (or vanished) —
    /// its entry should be removed so the register keeps shrinking.
    StaleGrandfatherEntry { path: String, lines: Option<usize> },
}

impl SizeViolation {
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
                "{path} grew to {lines} lines, past its grandfathered ceiling of {ceiling}.\n     \
                 Fix: move the new code to a smaller module, or raise the ceiling to {lines} \
                 on purpose (and say why in the commit message)."
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

    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "module size gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/module_size.rs. It is deliberate: growth has to be \
             acknowledged, not absorbed.",
            violations.len()
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
    fn grandfathered_module_may_shrink_while_still_over_cap() {
        let v = check_module_sizes(&files(&[("src/a.rs", 400)]), 200, &[("src/a.rs", 500)]);
        assert!(v.is_empty(), "{v:?}");
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
