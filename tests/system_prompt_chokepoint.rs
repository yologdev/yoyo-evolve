//! Choke-point gate: every production `with_system_prompt` call must route
//! through `agent_builder::compose_system_prompt`.
//!
//! Issue #664 was a model claiming to be something it is not; the fix was to
//! append a factual provider/model grounding note at the *single* place a
//! system prompt is handed to an agent. Issue #671 then found two builders
//! standing outside that choke point — the whole value of "it happens at one
//! place" is destroyed the moment a new builder quietly writes
//! `.with_system_prompt(&self.system_prompt)` instead.
//!
//! Both of those were found by reading. This gate makes a *new* bypass fail
//! `cargo test` instead of waiting to be read.
//!
//! Deliberate limits, stated rather than hidden:
//! - It is a source scan, not a type-level guarantee. It reasons about text.
//! - Only production code is scanned: everything from a file's first
//!   `#[cfg(test)] mod …` marker onward is skipped, because test modules build
//!   throwaway agents with literal prompts and are not a grounding hazard.
//! - A call site is judged by a small window (its own line plus the next two),
//!   so a call split across four lines would read as a bypass. That direction
//!   is the safe one: it fails loudly rather than passing quietly.

use std::path::{Path, PathBuf};

/// The token that hands a system prompt to a `yoagent::Agent`.
const CALL_TOKEN: &str = ".with_system_prompt(";

/// Expressions allowed to appear as the argument of a `with_system_prompt`
/// call. Adding a name here is a deliberate, reviewable diff — that is the
/// point, exactly like the grandfather list in `tests/module_size.rs`.
///
/// - `compose_system_prompt` is the choke point itself.
/// - `sub_agent_system_prompt` is a thin wrapper that composes internally
///   (`src/tools.rs`); `sub_agent_composer_actually_composes` pins that, so
///   this entry cannot become a loophole by having its body rewritten.
const APPROVED_COMPOSERS: &[&str] = &["compose_system_prompt(", "sub_agent_system_prompt("];

/// How many lines after a call site are considered part of its argument.
const ARG_WINDOW_LINES: usize = 2;

/// A way the choke point can be broken. Two distinct values on purpose:
/// collapsing them into one string would hide which one happened.
#[derive(Debug, PartialEq, Eq)]
enum Bypass {
    /// A production call site whose argument is not an approved composer.
    Uncomposed {
        path: String,
        line: usize,
        text: String,
    },
    /// An approved composer nothing calls any more. Not an error of safety but
    /// of hygiene: the allow-list only ever shrinks, so a dead entry must be
    /// deleted rather than left as pre-approval for a future bypass.
    UnusedApprovedComposer { name: String },
}

impl Bypass {
    fn message(&self) -> String {
        match self {
            Bypass::Uncomposed { path, line, text } => format!(
                "{path}:{line} calls with_system_prompt without composing:\n       {}\n     \
                 Fix: wrap the prompt in agent_builder::compose_system_prompt(base, provider, \
                 model) so the model is told which provider/model it is actually running as \
                 (#664, #671).",
                text.trim()
            ),
            Bypass::UnusedApprovedComposer { name } => format!(
                "`{name}` is listed in APPROVED_COMPOSERS but no production call site uses it.\n     \
                 Fix: delete its entry — the allow-list only shrinks."
            ),
        }
    }
}

/// Index of the first line that begins a `#[cfg(test)]` module, if any.
///
/// Matches only a column-0 `#[cfg(test)]` whose next non-blank line declares a
/// module, so a `#[cfg(test)]`-gated *helper function* earlier in a file does
/// not truncate the scan and hide real call sites below it.
fn test_region_start(lines: &[&str]) -> Option<usize> {
    for (i, line) in lines.iter().enumerate() {
        if *line != "#[cfg(test)]" {
            continue;
        }
        let next = lines[i + 1..]
            .iter()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim_start());
        if next.is_some_and(|n| n.starts_with("mod ") || n.starts_with("pub mod ")) {
            return Some(i);
        }
    }
    None
}

/// Pure checker: given each file's full text, report every bypass. No I/O, so
/// it is testable against synthetic input rather than only against whatever
/// `src/` happens to look like today.
fn check_call_sites(files: &[(String, String)], approved: &[&str]) -> Vec<Bypass> {
    let mut violations = Vec::new();
    let mut composer_used = vec![false; approved.len()];

    for (path, content) in files {
        let lines: Vec<&str> = content.lines().collect();
        let end = test_region_start(&lines).unwrap_or(lines.len());

        for (i, line) in lines[..end].iter().enumerate() {
            if !line.contains(CALL_TOKEN) {
                continue;
            }
            let window_end = (i + 1 + ARG_WINDOW_LINES).min(end);
            let window = lines[i..window_end].join("\n");

            match approved.iter().position(|c| window.contains(c)) {
                Some(idx) => composer_used[idx] = true,
                None => violations.push(Bypass::Uncomposed {
                    path: path.clone(),
                    line: i + 1,
                    text: (*line).to_string(),
                }),
            }
        }
    }

    for (idx, used) in composer_used.iter().enumerate() {
        if !used {
            violations.push(Bypass::UnusedApprovedComposer {
                name: approved[idx].trim_end_matches('(').to_string(),
            });
        }
    }

    violations
}

/// Recursively collect `*.rs` files under `dir` as (relative path, contents).
fn collect_rs_sources(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read {}: {e}", dir.display()),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            collect_rs_sources(&path, root, out);
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
fn every_agent_builder_routes_through_the_system_prompt_chokepoint() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_sources(&root.join("src"), &root, &mut files);

    // Reachability check: prove the scan actually sees the known builders
    // rather than passing because it found nothing (a persistent zero is a
    // defect hypothesis about the feed first, Day 147).
    let production_sites: usize = files
        .iter()
        .map(|(_, content)| {
            let lines: Vec<&str> = content.lines().collect();
            let end = test_region_start(&lines).unwrap_or(lines.len());
            lines[..end]
                .iter()
                .filter(|l| l.contains(CALL_TOKEN))
                .count()
        })
        .sum();
    assert!(
        production_sites >= 4,
        "scan found only {production_sites} production with_system_prompt call sites; \
         at least 4 are known to exist (main, side, architect, sub-agent) — \
         the scan is broken, not the repo"
    );

    let violations = check_call_sites(&files, APPROVED_COMPOSERS);

    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "system-prompt choke point broken ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/system_prompt_chokepoint.rs. The grounding note is \
             only worth anything if every builder gets it.",
            violations.len()
        );
    }
}

/// The one entry that is trusted rather than checked inline: pin that the
/// sub-agent's prompt helper really does compose, so approving it by name in
/// `APPROVED_COMPOSERS` cannot silently become a loophole.
#[test]
fn sub_agent_composer_actually_composes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join("src/tools.rs")).expect("src/tools.rs");
    let start = src
        .find("fn sub_agent_system_prompt")
        .expect("sub_agent_system_prompt should exist in src/tools.rs");
    let body: String = src[start..].lines().take(12).collect::<Vec<_>>().join("\n");
    assert!(
        body.contains("compose_system_prompt"),
        "sub_agent_system_prompt no longer composes — remove it from APPROVED_COMPOSERS:\n{body}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
            .collect()
    }

    const APPROVED: &[&str] = &["compose_system_prompt("];

    /// Just the call-site classification, dropping the allow-list hygiene
    /// violation. A synthetic corpus that deliberately contains no *composed*
    /// call site always also trips `UnusedApprovedComposer`, which those tests
    /// are not about; that behaviour is pinned separately by
    /// `unused_approved_composer_is_reported` and
    /// `empty_input_reports_only_the_unused_composer`, so filtering here hides
    /// nothing.
    fn uncomposed_only(files: &[(String, String)], approved: &[&str]) -> Vec<Bypass> {
        check_call_sites(files, approved)
            .into_iter()
            .filter(|b| matches!(b, Bypass::Uncomposed { .. }))
            .collect()
    }

    #[test]
    fn composed_call_site_passes() {
        let v = check_call_sites(
            &files(&[(
                "src/a.rs",
                "agent.with_system_prompt(compose_system_prompt(BASE, p, m))",
            )]),
            APPROVED,
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn raw_call_site_is_a_bypass() {
        let v = uncomposed_only(
            &files(&[("src/a.rs", "agent.with_system_prompt(&self.system_prompt)")]),
            APPROVED,
        );
        assert_eq!(
            v,
            vec![Bypass::Uncomposed {
                path: "src/a.rs".to_string(),
                line: 1,
                text: "agent.with_system_prompt(&self.system_prompt)".to_string(),
            }]
        );
    }

    #[test]
    fn composer_on_a_following_line_still_passes() {
        let v = check_call_sites(
            &files(&[(
                "src/a.rs",
                ".with_system_prompt(\n    compose_system_prompt(\n        BASE,\n    ),\n)",
            )]),
            APPROVED,
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn call_sites_inside_a_cfg_test_module_are_skipped() {
        let v = check_call_sites(
            &files(&[(
                "src/a.rs",
                ".with_system_prompt(compose_system_prompt(B, p, m))\n\
                 #[cfg(test)]\n\
                 mod tests {\n\
                 .with_system_prompt(\"test\")\n\
                 }",
            )]),
            APPROVED,
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn a_cfg_test_helper_fn_does_not_hide_later_call_sites() {
        // `#[cfg(test)]` on a *function* must not truncate the scan, or a
        // bypass below it would be invisible. This fixture has no composed
        // call site, so it also trips the allow-list hygiene check — that is
        // not what this test is about, so classify call sites only.
        let v = uncomposed_only(
            &files(&[(
                "src/a.rs",
                "#[cfg(test)]\n\
                 fn helper() {}\n\
                 .with_system_prompt(&raw)",
            )]),
            APPROVED,
        );
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(matches!(v[0], Bypass::Uncomposed { line: 3, .. }), "{v:?}");
    }

    #[test]
    fn unused_approved_composer_is_reported() {
        let v = check_call_sites(&files(&[("src/a.rs", "fn main() {}")]), APPROVED);
        assert_eq!(
            v,
            vec![Bypass::UnusedApprovedComposer {
                name: "compose_system_prompt".to_string()
            }]
        );
    }

    #[test]
    fn empty_input_reports_only_the_unused_composer() {
        let v = check_call_sites(&[], APPROVED);
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn test_region_start_finds_the_module_marker() {
        let lines = vec!["fn a() {}", "", "#[cfg(test)]", "mod tests {", "}"];
        assert_eq!(test_region_start(&lines), Some(2));
    }

    #[test]
    fn test_region_start_ignores_a_gated_function() {
        let lines = vec!["#[cfg(test)]", "fn helper() {}", "fn b() {}"];
        assert_eq!(test_region_start(&lines), None);
    }

    #[test]
    fn violation_messages_name_the_site_and_the_fix() {
        let m = Bypass::Uncomposed {
            path: "src/a.rs".to_string(),
            line: 42,
            text: "  .with_system_prompt(x)".to_string(),
        }
        .message();
        assert!(m.contains("src/a.rs:42"), "{m}");
        assert!(m.contains("compose_system_prompt"), "{m}");
    }
}
