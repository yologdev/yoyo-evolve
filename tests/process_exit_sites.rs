//! Every direct `process::exit` in `src/` is registered here, by file and
//! count, or the build is red (#1187).
//!
//! Why: `std::process::exit` inside the unit-test binary does not fail a
//! test — it ends the binary, with no test named. On 2026-11-12 the `lint`
//! dispatcher arm did exactly that from `test_try_dispatch_subcommand_lint`,
//! `main` stayed red ~16h, and two evolve runs died at Step 1. The fix is one
//! door, `dispatch_sub::exit_process`, which exits in a real binary and
//! panics under `cfg(test)`, so a unit test that reaches it fails by name.
//! This gate keeps new code from going around the door.
//!
//! The register lists the sites that stay direct, each with the reason no unit
//! test can reach it. A count that goes UP (a new direct exit) fails; a count
//! that goes DOWN (a site converted or removed) fails too, so the register can
//! only be edited on purpose and never keeps vouching for a site that is gone.
//!
//! Limits, stated: textual scan. Text after `//` on a line is ignored, so a
//! string literal containing `//` before the call hides it; a `use
//! std::process::exit` import is caught (the path is on the line), but an
//! `exit(` call through an alias imported some other way is not.

use std::path::{Path, PathBuf};

/// `(file relative to the repo root, direct sites, why no unit test reaches them)`.
const REGISTERED_EXIT_SITES: &[(&str, usize, &str)] = &[
    (
        "src/dispatch_sub.rs",
        1,
        "the door itself, `exit_process`: under cfg(test) this branch is compiled out and the \
         door panics instead",
    ),
    (
        "src/main.rs",
        4,
        "`exit_after_shutdown` and three in `main()`; only `main()` reaches them (via \
         run_single_prompt/run_piped_mode), and no unit test calls `main()`",
    ),
    (
        "src/prompt/sigterm.rs",
        1,
        "inside the task spawned by `arm` and runs only when a real SIGTERM/SIGHUP/SIGQUIT \
         arrives; a test binary receiving one should die",
    ),
];

#[derive(Debug, PartialEq)]
enum Violation {
    Unregistered {
        file: String,
        lines: Vec<usize>,
    },
    CountChanged {
        file: String,
        registered: usize,
        lines: Vec<usize>,
    },
    EmptyReason {
        file: String,
    },
}

impl Violation {
    fn message(&self) -> String {
        match self {
            Violation::Unregistered { file, lines } => format!(
                "{file}: direct process::exit at line(s) {} — call \
                 `crate::dispatch_sub::exit_process(code)` instead, so a unit test that reaches \
                 it fails by name instead of killing the test binary (#1187). If no unit test \
                 can ever reach it, register it in tests/process_exit_sites.rs with the reason.",
                join(lines)
            ),
            Violation::CountChanged {
                file,
                registered,
                lines,
            } => format!(
                "{file}: registered {registered} direct process::exit site(s), found {} at \
                 line(s) [{}]. A new site goes through `dispatch_sub::exit_process`; a converted \
                 or removed one means lowering the register's count.",
                lines.len(),
                join(lines)
            ),
            Violation::EmptyReason { file } => {
                format!("{file}: register entry has an empty reason")
            }
        }
    }
}

fn join(lines: &[usize]) -> String {
    lines
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// 1-based line numbers whose code part (before any `//`) names `process::exit`.
fn exit_lines(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, l)| l.split("//").next().unwrap_or("").contains("process::exit"))
        .map(|(i, _)| i + 1)
        .collect()
}

fn classify(files: &[(String, String)], register: &[(&str, usize, &str)]) -> Vec<Violation> {
    let mut out = Vec::new();
    for (file, count, reason) in register {
        if reason.trim().is_empty() {
            out.push(Violation::EmptyReason {
                file: file.to_string(),
            });
        }
        let lines = files
            .iter()
            .find(|(p, _)| p == file)
            .map(|(_, s)| exit_lines(s))
            .unwrap_or_default();
        if lines.len() != *count {
            out.push(Violation::CountChanged {
                file: file.to_string(),
                registered: *count,
                lines,
            });
        }
    }
    for (path, source) in files {
        if register.iter().any(|(f, _, _)| f == path) {
            continue;
        }
        let lines = exit_lines(source);
        if !lines.is_empty() {
            out.push(Violation::Unregistered {
                file: path.clone(),
                lines,
            });
        }
    }
    out
}

fn collect_rs_files(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {dir:?}: {e}"));
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
            out.push((rel, content));
        }
    }
}

#[test]
fn every_direct_process_exit_in_src_is_registered() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &root, &mut files);
    assert!(
        files.len() > 10,
        "source walk found only {} files — the walk is broken, not the repo",
        files.len()
    );
    // Anti-vacuous: the door's own call must be seen, or the scanner is blind.
    let door = files
        .iter()
        .find(|(p, _)| p == "src/dispatch_sub.rs")
        .map(|(_, s)| exit_lines(s).len())
        .unwrap_or(0);
    assert!(
        door >= 1,
        "the scan did not see exit_process's own exit call — the scanner is broken"
    );

    let violations = classify(&files, REGISTERED_EXIT_SITES);
    let msgs: Vec<String> = violations.iter().map(Violation::message).collect();
    assert!(msgs.is_empty(), "\n{}\n", msgs.join("\n"));
}

#[cfg(test)]
mod classifier {
    use super::*;

    fn files(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect()
    }

    #[test]
    fn comments_are_not_sites_and_trailing_comments_do_not_hide_one() {
        let src = "/// calls process::exit\n// process::exit(1)\n    std::process::exit(2); // why\nlet x = 1;\nuse std::process::exit;\n";
        assert_eq!(exit_lines(src), vec![3, 5]);
    }

    #[test]
    fn an_unregistered_file_is_fatal_by_file_and_line() {
        let v = classify(
            &files(&[("src/a.rs", "fn f() {\n  std::process::exit(3);\n}\n")]),
            &[],
        );
        assert_eq!(
            v,
            vec![Violation::Unregistered {
                file: "src/a.rs".into(),
                lines: vec![2]
            }]
        );
        assert!(v[0]
            .message()
            .contains("src/a.rs: direct process::exit at line(s) 2"));
    }

    #[test]
    fn a_count_that_moves_either_way_is_fatal() {
        let reg: &[(&str, usize, &str)] = &[("src/a.rs", 1, "why")];
        let two = files(&[(
            "src/a.rs",
            "std::process::exit(1);\nstd::process::exit(2);\n",
        )]);
        assert!(matches!(
            classify(&two, reg)[..],
            [Violation::CountChanged { .. }]
        ));
        let zero = files(&[("src/a.rs", "fn f() {}\n")]);
        assert!(matches!(
            classify(&zero, reg)[..],
            [Violation::CountChanged { .. }]
        ));
        // Near-miss: the registered count passes, and the door's name is not a site.
        let one = files(&[
            ("src/a.rs", "std::process::exit(1);\n"),
            ("src/b.rs", "exit_process(2);\n"),
        ]);
        assert!(classify(&one, reg).is_empty());
    }

    #[test]
    fn an_empty_reason_is_fatal() {
        let reg: &[(&str, usize, &str)] = &[("src/a.rs", 1, " ")];
        let one = files(&[("src/a.rs", "std::process::exit(1);\n")]);
        assert_eq!(
            classify(&one, reg),
            vec![Violation::EmptyReason {
                file: "src/a.rs".into()
            }]
        );
    }
}
