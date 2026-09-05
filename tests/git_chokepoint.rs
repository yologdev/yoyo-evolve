//! Git-chokepoint gate: every `std::process::Command::new("git")` in **non-test**
//! code under `src/` must either live in `src/git.rs` — which *is* the chokepoint
//! and is exempt by definition — or be named in `REGISTERED_GIT_BYPASSES`.
//!
//! **The measured reason.** Day 182 (#863) injected `-c core.quotepath=off` at one
//! site, `git_command()` in `src/git.rs`, and **14 chokepoint consumers inherited
//! the fix with zero caller edits** — including three that outranked the reported
//! symptom (`context.rs`, whose output is injected into every prompt;
//! `commands_risk.rs`, which feeds the risk model; `commands_rename.rs`, which was
//! silently skipping files). The write-up then disclosed, honestly, that **11
//! production sites shell `git` directly and receive none of it**. That disclosure
//! is the shape my archive calls durable debt: *honesty discharges the obligation
//! and then substitutes for the fix*. Nothing enumerated those 11, so a 12th could
//! appear unnamed and nothing anywhere would notice.
//!
//! **Why a gate rather than a third fix.** Path-quoting had been repaired twice
//! with no grader between them — #829 inside `git_commit_msg.rs`, #863 at the
//! chokepoint. My own stopping rule: at N≥3 repairs of one named class with no
//! grader between them, the diff must become an **instrument**, not a third fix.
//!
//! **What it buys, and it outlives quotepath.** *Any* future global applied at the
//! chokepoint — `-c core.autocrlf`, `-c i18n.logOutputEncoding`, `--no-pager`, an
//! env pin, a timeout — reaches the chokepoint's callers and silently misses these
//! 11. The gate does not stop that; it stops it happening to a **twelfth,
//! unnamed** site.
//!
//! **The rule.** Two branches, running in opposite directions:
//!
//! 1. An **unregistered bypass → fatal**, with both remedies printed verbatim
//!    (route it through `src/git.rs`, or paste the literal register line). The
//!    escape hatch *is* the point — **the gate does not forbid a bypass, it
//!    forbids an unnamed one** (the Day-166 module-size lesson: a gate whose only
//!    remedy is a whole-task revert eats the correct work sitting beside the
//!    violation).
//! 2. A **registered entry that vanished, or is now inside `src/git.rs` → fatal**.
//!    The ratchet: an exception list only pays itself down if *improving* is a
//!    failure too, otherwise progress leaves silent headroom nobody granted. A
//!    register entry with an **empty or whitespace-only reason is fatal** as well —
//!    an unnamed debt wearing a name is not a name.
//!
//! It is **anti-vacuous**, and that branch is asserted **first**: if the scan finds
//! zero `Command::new("git")` anywhere under `src/` — including test regions — it
//! fails loudly. A scanner that finds nothing and passes is this very defect
//! wearing the opposite sign, and it is quieter than the bug.
//!
//! **Four limits, printed on every passing run** so "could not check" cannot read
//! as "checked; clean":
//!
//! 1. It is a **textual** match on the literal `Command::new("git")`. A git
//!    invocation built through a variable, a `PathBuf`, an env-driven binary path,
//!    or a shell string handed to `bash` is invisible to it.
//! 2. It **cannot tell a harmful bypass from a harmless one**. A `git grep`
//!    builder that needs incremental args and a `git commit --amend` that needs
//!    merged stdout/stderr look identical to it — which is exactly why every
//!    register entry carries a reason a human wrote.
//! 3. Each file is truncated at its **module-level `#[cfg(test)] mod` marker**, so
//!    production code placed *after* that module is unscanned. Checked rather than
//!    implied at the census below: of the 94 files scanned, none places a
//!    `Command::new("git")` after its test module.
//! 4. The register key is `(path, fn)`, **not** `(path, line)`, so one entry
//!    covers **every** site inside that function — three of the seven entries
//!    already stand for two sites each. A second bypass added to an
//!    already-registered `fn` is therefore inherited **silently**, with no new
//!    violation and no census movement beyond the site count. Deliberate: a line
//!    key would fire on every unrelated edit that shifts a line number, which is
//!    the churn that trains a reader to paste past a gate without reading it.
//!
//! **What it deliberately does not do:** it **enumerates** the bypasses, it does
//! **not** fix them. #864 stays open on the per-site work, which is a design
//! decision per site (streaming output, custom stdio, a `Child` handle, a
//! `.status()` probe) and not a mechanical sweep.

use std::io::Write;
use std::path::Path;

/// Non-test `Command::new("git")` sites outside `src/git.rs` that deliberately do
/// not go through the chokepoint.
///
/// `(path, context, why)` — keyed by `(path, context)` rather than by path alone,
/// because several files carry more than one bypass and a per-path key would let
/// one entry vouch for a sibling it knows nothing about.
///
/// **Generated by running the scan and pasting what it found, not curated.** Its
/// size *is* the finding, the same discipline `tests/global_state_races.rs` and
/// `tests/cargo_spawning_tests.rs` used for theirs. Each reason states what is
/// true and no more — including, for one entry, that there is **no** structural
/// blocker.
const REGISTERED_GIT_BYPASSES: &[(&str, &str, &str)] = &[
    (
        "src/commands_git.rs",
        "run_git_amend_with_message",
        "returns (bool, String) merging exit status with stdout-or-stderr; run_git collapses \
         that into Result (stdout on success, stderr on failure) and loses the \
         success-with-stderr case. `commit` is also in DESTRUCTIVE_GIT_COMMANDS, so the \
         chokepoint's #[cfg(test)] guard would panic from the project root.",
    ),
    (
        "src/commands_git.rs",
        "run_git_amend_no_edit",
        "same shape as run_git_amend_with_message: merged (bool, String) return, and \
         `commit` trips the chokepoint's destructive-command test guard.",
    ),
    (
        "src/commands_search.rs",
        "run_grep",
        "two sites in one fn: a .status() probe with both stdout and stderr nulled (no \
         status-only chokepoint helper exists, and routing it through .output() changes what \
         the probe costs and captures), and a `git grep` builder whose argv is assembled \
         incrementally under conditionals for -i, -c, pattern and path — which the \
         chokepoint's &[&str] arg-slice signature cannot express without materialising every \
         combination.",
    ),
    (
        "src/commands_search.rs",
        "run_grep_count",
        "two sites in one fn, same shapes as run_grep's: the nulled-stdio .status() probe and \
         an incremental `git grep -c` builder.",
    ),
    (
        "src/commands_search.rs",
        "run_grep_with_context",
        "two sites in one fn, same shapes as run_grep's: the nulled-stdio .status() probe and \
         an incremental `git grep` builder, here with -B/-A context flags computed at runtime.",
    ),
];

/// The one file that *is* the chokepoint, and is exempt by definition.
const CHOKEPOINT_FILE: &str = "src/git.rs";

/// One `Command::new("git")` occurrence in a file's non-test region.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitSite {
    path: String,
    line: usize,
    /// The enclosing `fn` name, used only to make the remedy message pasteable.
    context: String,
}

/// A way the chokepoint gate can be violated. Two values, running in **opposite
/// directions** — the same two-direction discipline every sibling gate uses.
#[derive(Debug, PartialEq, Eq)]
enum BypassViolation {
    /// A non-test `Command::new("git")` outside `src/git.rs` that nothing names.
    /// The defect this gate exists for.
    Unregistered { site: GitSite },
    /// A registered entry whose site no longer exists, or that now points inside
    /// `src/git.rs`. The ratchet.
    RegisteredDebtPaid { path: String, context: String },
    /// A register entry whose reason is empty or whitespace-only. An unnamed debt
    /// wearing a name is not a name.
    EmptyReason { path: String, context: String },
}

impl BypassViolation {
    fn message(&self) -> String {
        match self {
            BypassViolation::Unregistered { site } => format!(
                "{}:{} ({}) shells git directly with Command::new(\"git\"), so it receives \
                 nothing applied at the src/git.rs chokepoint — today that is \
                 `-c core.quotepath=off` (#863), tomorrow whatever else lands there.\n     \
                 Fix (either one): route it through src/git.rs (run_git / run_git_in_dir / \
                 run_git_output, all of which build their Command via git_command()), or — \
                 if it genuinely cannot go through the chokepoint — add\n       \
                 (\"{}\", \"{}\", \"<why it cannot go through the chokepoint>\"),\n     \
                 to REGISTERED_GIT_BYPASSES in tests/git_chokepoint.rs.\n     \
                 The gate does not forbid a bypass. It forbids an unnamed one.",
                site.path, site.line, site.context, site.path, site.context,
            ),
            BypassViolation::RegisteredDebtPaid { path, context } => format!(
                "({path}, {context}) is listed in REGISTERED_GIT_BYPASSES but no matching \
                 non-test Command::new(\"git\") site exists there any more — it was either \
                 routed through the chokepoint, moved into {CHOKEPOINT_FILE}, or deleted. \
                 The debt is paid.\n     \
                 Fix: delete that entry from REGISTERED_GIT_BYPASSES in \
                 tests/git_chokepoint.rs. Fatal on purpose: the register only ratchets down \
                 if a repair is also a failure."
            ),
            BypassViolation::EmptyReason { path, context } => format!(
                "({path}, {context}) is registered in REGISTERED_GIT_BYPASSES with an empty \
                 reason.\n     \
                 Fix: write why that site cannot go through the src/git.rs chokepoint. An \
                 unnamed debt wearing a name is not a name — the reason is the only part of \
                 the entry a human can act on."
            ),
        }
    }
}

/// Pure checker: given every non-test git site found under `src/` and the
/// register, return every violation.
///
/// All filesystem walking stays at the single call site, so both fatal branches
/// are provable against **fabricated** inputs rather than by planting a real
/// bypass in `src/` — the discipline `tests/orphan_modules.rs` and
/// `tests/feature_gated_tests.rs` use.
fn classify(sites: &[GitSite], register: &[(&str, &str, &str)]) -> Vec<BypassViolation> {
    let mut out = Vec::new();

    for site in sites {
        if site.path == CHOKEPOINT_FILE {
            continue;
        }
        let named = register
            .iter()
            .any(|(p, c, _)| *p == site.path && *c == site.context);
        if !named {
            out.push(BypassViolation::Unregistered { site: site.clone() });
        }
    }

    for (path, context, reason) in register {
        if reason.trim().is_empty() {
            out.push(BypassViolation::EmptyReason {
                path: (*path).to_string(),
                context: (*context).to_string(),
            });
        }
        let still_bypassing = *path != CHOKEPOINT_FILE
            && sites
                .iter()
                .any(|s| s.path == *path && s.context == *context);
        if !still_bypassing {
            out.push(BypassViolation::RegisteredDebtPaid {
                path: (*path).to_string(),
                context: (*context).to_string(),
            });
        }
    }

    out
}

/// The literal this gate searches for, assembled at **runtime** so this file's own
/// source can never match itself if it is ever scanned.
fn git_command_needle() -> String {
    format!("Command::new({}git{})", '"', '"')
}

/// The module-level test marker, also assembled at runtime for the same reason.
fn cfg_test_attr() -> String {
    format!("#[cfg{}test{}]", '(', ')')
}

/// Everything before a file's **module-level** `#[cfg(test)] mod` marker.
///
/// Deliberately **not** a split on a bare `#[cfg(test)]`: `src/git.rs` carries such
/// attributes on real items (`DESTRUCTIVE_GIT_COMMANDS` at :8,
/// `resolve_git_invocation` at :44, `destructive_guard` at :77), so a bare split
/// would truncate that file at line 8 and blank the chokepoint itself — turning
/// the exemption into a vacuous pass. The marker is the attribute line followed by
/// a `mod` declaration at column 0, which is how every module in `src/` opens its
/// test module.
fn non_test_region(source: &str) -> Vec<&str> {
    let attr = cfg_test_attr();
    let lines: Vec<&str> = source.lines().collect();
    for i in 0..lines.len().saturating_sub(1) {
        if lines[i].trim_end() != attr {
            continue;
        }
        let next = lines[i + 1];
        let rest = next
            .strip_prefix("pub(crate) ")
            .or_else(|| next.strip_prefix("pub(super) "))
            .or_else(|| next.strip_prefix("pub "))
            .unwrap_or(next);
        if rest.starts_with("mod ") {
            return lines[..i].to_vec();
        }
    }
    lines
}

/// The enclosing `fn` name for a line, found by walking back to the nearest
/// column-0 `fn` declaration. Only ever used to make the remedy message pasteable,
/// so a miss degrades to `"?"` rather than failing.
fn enclosing_fn(lines: &[&str], idx: usize) -> String {
    for line in lines[..=idx].iter().rev() {
        let rest = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub(super) "))
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        let rest = rest.strip_prefix("async ").unwrap_or(rest);
        if let Some(after) = rest.strip_prefix("fn ") {
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return name;
            }
        }
    }
    "?".to_string()
}

/// What one filesystem walk found. Every field is reported in the census, because
/// a census that only speaks when it fails teaches nothing.
struct Scan {
    files: usize,
    total_sites: usize,
    non_test_sites: Vec<GitSite>,
    test_region_sites: usize,
    /// Non-test sites that live *after* a file's test module — limit 3. Counted so
    /// the limit is a measured zero rather than an assumed one.
    after_test_module: usize,
}

fn scan_src(repo_root: &Path) -> Scan {
    let root = repo_root.join("src");
    let needle = git_command_needle();
    let mut files = 0usize;
    let mut total_sites = 0usize;
    let mut non_test_sites = Vec::new();
    let mut test_region_sites = 0usize;
    let mut after_test_module = 0usize;

    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            files += 1;
            let rel = path
                .strip_prefix(repo_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let all: Vec<&str> = source.lines().collect();
            let prod = non_test_region(&source);
            let cut = prod.len();

            for (i, line) in all.iter().enumerate() {
                if !line.contains(&needle) {
                    continue;
                }
                total_sites += 1;
                if i < cut {
                    non_test_sites.push(GitSite {
                        path: rel.clone(),
                        line: i + 1,
                        context: enclosing_fn(&prod, i),
                    });
                } else {
                    test_region_sites += 1;
                    // Limit 3: is this line inside the test module, or production
                    // code placed after it? A column-0 `fn` after the cut with no
                    // enclosing `mod tests {` indentation would be the latter.
                    if !line.starts_with(' ') && !line.starts_with('\t') {
                        after_test_module += 1;
                    }
                }
            }
        }
    }

    non_test_sites.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    Scan {
        files,
        total_sites,
        non_test_sites,
        test_region_sites,
        after_test_module,
    }
}

/// Write the census and the four stated limits through a **raw** stderr handle.
///
/// Not `eprintln!`: libtest's capture hook intercepts the `print!`/`eprint!` macros
/// and discards output from *passing* tests — which is exactly what this is. The
/// same writer discipline `tests/module_size.rs` and `tests/orphan_modules.rs` use.
fn write_census(scan: &Scan) {
    let mut err = std::io::stderr();
    let chokepoint = scan
        .non_test_sites
        .iter()
        .filter(|s| s.path == CHOKEPOINT_FILE)
        .count();
    let _ = writeln!(
        err,
        "\ngit-chokepoint gate census (Day 183, 2026-08-30):\n  \
         {} files scanned under src/\n  \
         {} total Command::new(\"git\") sites\n  \
         {} in non-test regions: {} at the src/git.rs chokepoint, {} bypass sites named by \
         {} register entries\n  \
         {} in test regions ({} of them production code placed after a test module)\n\
         limits: (1) textual match only — a git invocation built through a variable, a \
         PathBuf or an env-driven path is invisible; (2) it cannot tell a harmful bypass \
         from a harmless one, which is why every register entry carries a human-written \
         reason; (3) files are truncated at the module-level #[cfg(test)] mod marker, so \
         production code after that module is unscanned; (4) the key is (path, fn), so one \
         entry covers EVERY site in that fn — a second bypass added to an already-registered \
         fn is inherited silently.\n\
         it enumerates the bypasses; it does not fix them (#864 stays open on the per-site \
         work).",
        scan.files,
        scan.total_sites,
        scan.non_test_sites.len(),
        chokepoint,
        scan.non_test_sites.len() - chokepoint,
        REGISTERED_GIT_BYPASSES.len(),
        scan.test_region_sites,
        scan.after_test_module,
    );
}

#[test]
fn every_direct_git_invocation_is_at_the_chokepoint_or_registered() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scan = scan_src(root);

    // Anti-vacuous, asserted FIRST: a scanner that finds nothing and passes is
    // this very defect wearing the opposite sign, and it is quieter than the bug.
    assert!(
        scan.total_sites > 0,
        "git-chokepoint gate found ZERO Command::new(\"git\") sites anywhere under src/. \
         src/git.rs contains at least one by construction (git_command()), so this is a \
         broken scanner reporting a clean bill of health, not a clean tree."
    );

    let violations = classify(&scan.non_test_sites, REGISTERED_GIT_BYPASSES);
    assert!(
        violations.is_empty(),
        "git-chokepoint gate found {} violation(s):\n{}",
        violations.len(),
        violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n")
    );

    write_census(&scan);
}

/// The register is debt, not absolution: every entry must carry a real reason.
#[test]
fn every_register_entry_carries_a_reason() {
    assert!(
        !REGISTERED_GIT_BYPASSES.is_empty(),
        "REGISTERED_GIT_BYPASSES is empty — 11 bypasses were measured on Day 182, so an \
         empty register means the scan or the register was silently emptied."
    );
    for (path, context, reason) in REGISTERED_GIT_BYPASSES {
        assert!(
            !reason.trim().is_empty(),
            "({path}, {context}) has an empty reason"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(path: &str, line: usize, context: &str) -> GitSite {
        GitSite {
            path: path.to_string(),
            line,
            context: context.to_string(),
        }
    }

    #[test]
    fn the_chokepoint_itself_is_exempt() {
        let v = classify(&[site(CHOKEPOINT_FILE, 126, "git_command")], &[]);
        assert!(v.is_empty(), "{v:?}");
    }

    /// Branch 1, the whole point: a fabricated unregistered bypass is fatal, and
    /// the message names **both** remedies verbatim plus the pasteable line.
    #[test]
    fn an_unregistered_bypass_is_fatal() {
        let s = site("src/commands_new.rs", 42, "do_thing");
        let v = classify(std::slice::from_ref(&s), &[]);
        assert_eq!(v, vec![BypassViolation::Unregistered { site: s }]);
        let msg = v[0].message();
        assert!(msg.contains("src/commands_new.rs:42"), "{msg}");
        assert!(msg.contains("run_git"), "{msg}");
        assert!(msg.contains("REGISTERED_GIT_BYPASSES"), "{msg}");
        assert!(
            msg.contains("(\"src/commands_new.rs\", \"do_thing\","),
            "the remedy must be pasteable: {msg}"
        );
        assert!(msg.contains("It forbids an unnamed one"), "{msg}");
    }

    /// The near-miss guard: a registered bypass passes and stays named. A
    /// discriminator tested only on the side that fires is vacuous green.
    #[test]
    fn a_registered_bypass_passes() {
        let v = classify(
            &[site("src/commands_new.rs", 42, "do_thing")],
            &[("src/commands_new.rs", "do_thing", "needs custom stdio")],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    /// The key is `(path, context)`, not `path`: one entry must not vouch for a
    /// sibling bypass in the same file that it knows nothing about.
    #[test]
    fn a_sibling_bypass_in_a_registered_file_is_still_fatal() {
        let v = classify(
            &[
                site("src/commands_search.rs", 134, "list_project_files"),
                site("src/commands_search.rs", 1191, "run_grep"),
            ],
            &[("src/commands_search.rs", "list_project_files", "why")],
        );
        assert_eq!(
            v,
            vec![BypassViolation::Unregistered {
                site: site("src/commands_search.rs", 1191, "run_grep")
            }]
        );
    }

    /// The ratchet, direction 1: the site was routed through the chokepoint.
    #[test]
    fn a_registered_site_that_was_fixed_is_fatal() {
        let v = classify(&[], &[("src/commands_new.rs", "do_thing", "why")]);
        assert_eq!(
            v,
            vec![BypassViolation::RegisteredDebtPaid {
                path: "src/commands_new.rs".to_string(),
                context: "do_thing".to_string(),
            }]
        );
        assert!(
            v[0].message().contains("The debt is paid"),
            "{}",
            v[0].message()
        );
    }

    /// The ratchet, direction 2: an entry that points into the chokepoint itself
    /// is fatal, because `src/git.rs` needs no exemption.
    #[test]
    fn a_registered_entry_inside_the_chokepoint_is_fatal() {
        let v = classify(
            &[site(CHOKEPOINT_FILE, 126, "git_command")],
            &[(CHOKEPOINT_FILE, "git_command", "why")],
        );
        assert_eq!(
            v,
            vec![BypassViolation::RegisteredDebtPaid {
                path: CHOKEPOINT_FILE.to_string(),
                context: "git_command".to_string(),
            }]
        );
    }

    #[test]
    fn an_empty_reason_is_fatal() {
        let v = classify(
            &[site("src/commands_new.rs", 42, "do_thing")],
            &[("src/commands_new.rs", "do_thing", "   ")],
        );
        assert_eq!(
            v,
            vec![BypassViolation::EmptyReason {
                path: "src/commands_new.rs".to_string(),
                context: "do_thing".to_string(),
            }]
        );
        assert!(
            v[0].message().contains("An unnamed debt wearing a name"),
            "{}",
            v[0].message()
        );
    }

    /// The truncation rule, pinned because getting it wrong is silent: a bare
    /// `#[cfg(test)]` on a real item must **not** cut the file, or `src/git.rs`
    /// would be blanked at line 8 and the chokepoint exemption would go vacuous.
    #[test]
    fn a_cfg_test_attribute_on_a_real_item_does_not_truncate() {
        let src = format!(
            "{attr}\nconst X: &[&str] = &[];\n\
             fn real() {{ let _ = 1; }}\n\
             {attr}\nmod tests {{\n    fn t() {{}}\n}}\n",
            attr = cfg_test_attr()
        );
        let prod = non_test_region(&src);
        assert!(
            prod.iter().any(|l| l.contains("fn real()")),
            "production code after an item-level #[cfg(test)] must survive: {prod:?}"
        );
        assert!(
            !prod.iter().any(|l| l.contains("mod tests")),
            "the module-level marker must still cut: {prod:?}"
        );
    }

    #[test]
    fn a_file_with_no_test_module_is_scanned_whole() {
        let src = "fn a() {}\nfn b() {}\n";
        assert_eq!(non_test_region(src).len(), 2);
    }

    #[test]
    fn enclosing_fn_walks_back_to_the_nearest_declaration() {
        let lines = vec![
            "fn first() {",
            "    let x = 1;",
            "}",
            "pub(crate) fn second() {",
            "    let y = 2;",
        ];
        assert_eq!(enclosing_fn(&lines, 1), "first");
        assert_eq!(enclosing_fn(&lines, 4), "second");
    }

    /// The needle is assembled at runtime, so this test file cannot match itself
    /// even if it were ever scanned — the discipline the source-level guards in
    /// `src/` use.
    #[test]
    fn the_needle_is_the_literal_it_claims_to_be() {
        assert_eq!(git_command_needle(), "Command::new(\"git\")");
        assert_eq!(cfg_test_attr(), "#[cfg(test)]");
    }
}
