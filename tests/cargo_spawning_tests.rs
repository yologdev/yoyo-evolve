//! Cargo-spawning-test gate: a `#[test]` under `src/` that reaches a `cargo`
//! spawn must be named in a register.
//!
//! # Why this exists
//!
//! Day 179 (#832) fixed one site. `test_handle_evolution_no_panic` called
//! `handle_evolution`, which shelled `cargo test -- --list` — a **nested,
//! feature-less cargo** that rebuilt the bin without `gasp` and uplifted it
//! over the shared path `target/debug/yoyo`. Integration tests reach the
//! binary through `env!("CARGO_BIN_EXE_yoyo")`, which bakes in that shared,
//! feature-suffix-less path, so `tests/gasp_cli_run_ordering.rs` got the plain
//! binary and its honest refusal reddened `main` for **three sessions**
//! (`a586be84`, `329a9bb8`, `524c50e4`). It read as flakiness because the
//! victim passes in isolation.
//!
//! #832 wrote the standing rule into CLAUDE.md — **no `#[test]` under `src/`
//! may spawn `cargo`** — and gave it exactly one enforcer: a source-level
//! assertion inside `handle_evolution_with`'s own test. Nothing checked the
//! rest of the tree. That is the shape my own archive names verbatim: *"when a
//! mitigation's protection is collective, applying it to the N known offenders
//! gives an N/N counter that feels like closure while the property stays
//! false."* Three CI reds were spent on one instance of a class nobody had
//! enumerated. This gate buys the enumeration.
//!
//! Shape borrowed verbatim from its five siblings (`tests/module_size.rs`,
//! `tests/blind_round_grades.rs`, `tests/orphan_modules.rs`,
//! `tests/doc_version_claims.rs`, `tests/global_state_races.rs`): a pure
//! decision function with all the filesystem walking at one call site, fatal
//! on the *unnamed* case, a register for deliberate exceptions, and a ratchet
//! so the register can only shrink.
//!
//! **The gate does not forbid a test that reaches a cargo spawn. It forbids an
//! unnamed one.** The escape hatch is the point: a gate whose only remedy is
//! "revert the whole task" eats the correct work sitting beside the violation
//! (the Day-166 module-size lesson), so the compliant response is one line in
//! a register that is itself the enumeration of the debt.
//!
//! # The predicate is two-level, one hop
//!
//! A direct-call-only rule (a `#[test]` body literally containing
//! `Command::new("cargo")`) finds **zero** violations today and would be
//! vacuous: both real sites are production functions called *by* tests. So:
//!
//! 1. **Derive the spawner set.** Walk `src/`; for every non-test `fn`, record
//!    its name if its body contains a `cargo` spawn. The set is **derived,
//!    never hand-written** — strictly better than `GLOBAL_SETTERS` in
//!    `tests/global_state_races.rs`, which is hand-written and needs a second
//!    test (`every_named_setter_still_exists`) to tie it to reality. Nothing
//!    here can go stale under a rename, because a rename moves both halves.
//! 2. **Find the violators.** A `#[test]` / `#[tokio::test]` under `src/`
//!    whose body directly calls any name in that set, or that spawns `cargo`
//!    itself, must appear in `REGISTERED_CARGO_SPAWNING_TESTS`.
//!
//! # What this gate CANNOT do
//!
//! Read this before trusting a green run. "Could not check" must not read as
//! "checked; clean". All four are printed on every passing run through a raw
//! stderr handle, for the reason `tests/blind_round_grades.rs` had to pay off
//! after eight days of nobody reading a passing test's warning.
//!
//! 1. **One hop only.** A test reaching a spawner through an intermediate
//!    helper is invisible to it.
//! 2. **Matched by function name, not resolved path.** Two functions sharing a
//!    name in different modules are indistinguishable, so one match can vouch
//!    for the wrong callee.
//! 3. **Textual match on the spawn.** A command built through a variable, a
//!    `PathBuf`, or an env-var-driven binary path is invisible.
//! 4. **It cannot tell a harmful spawn from a harmless one.** `cargo audit
//!    --version` probes — it resolves and execs `cargo-audit` or fails, it
//!    builds nothing, so it cannot clobber `target/debug/yoyo`. `cargo test`
//!    builds and clobbers that shared uplift path. This gate sees both
//!    identically, which is precisely why every register entry carries a
//!    reason a human wrote.
//!
//! A fifth caveat lives here rather than in the printed note because it is
//! about the parser, not the policy: function bodies are delimited by brace
//! matching over a scanner that skips line/block comments (nesting), char
//! literals *without* mistaking a lifetime for one, and normal *and* raw
//! strings. It is not a Rust lexer — a macro token tree with deliberately
//! unbalanced braces would mis-scope a body. There are none in `src/` today,
//! and the scanner is pinned by its own table test.
//!
//! **The brace scanner below is a deliberate second copy.**
//! `tests/global_state_races.rs` already has one. `tests/*.rs` are separate
//! crates, so sharing it needs a `tests/common/mod.rs` plus a refactor of the
//! sibling — larger than this task, and filed as a follow-up rather than
//! smuggled in. Saying so out loud beats pretending the duplication is not
//! there.

use std::io::Write;
use std::path::{Path, PathBuf};

mod common;
use common::{calls_directly, match_body};

/// Tests under `src/` that reach a `cargo` spawn, recorded by hand with the
/// reason each one is acceptable.
///
/// **This register is debt, not absolution.** An entry does not make a test
/// safe; it records that a human looked at it and decided. It can only shrink
/// — a registered test that stops reaching a spawn is fatal (the ratchet).
///
/// Generated by running the scan and pasting what it found, **not curated**.
/// Its size *is* the finding: #832's `grep` sweep predicted three entries and
/// the scan returned eight, because `security_audit_command` has eight callers
/// and a `grep` for the function name in a 1700-line file undercounts.
///
/// Key is `(path, test_name, reason)` — pinned by test, not by name alone,
/// since names repeat across modules. An empty or whitespace-only reason is
/// fatal: an unnamed debt wearing a name is not a name.
const REGISTERED_CARGO_SPAWNING_TESTS: &[(&str, &str, &str)] = &[
    // EMPTY, and that is a payment rather than a deletion (Day 188, #834).
    //
    // This shipped on Day 179 holding 8 entries, every one a
    // `security_audit_command_*` arm in src/commands_lint.rs that reached
    // `cargo audit --version`. All 8 were paid: the probe is now injected
    // (`security_audit_command_with` takes an `installed` resolver), so each
    // test drives the pure core with the answer supplied explicitly and
    // asserts BOTH directions instead of accommodating whichever answer the
    // machine happened to give.
    //
    // The debt was never #832's clobbering defect and this register never
    // said it was: a subcommand probe builds nothing and cannot touch the
    // shared target/debug/yoyo uplift path. It was toolchain-dependence and
    // vacuous green.
    //
    // An empty register is a legitimate terminal state for this gate family,
    // not a broken scan: `tests/orphan_modules.rs` ships `REGISTERED_ORPHANS`
    // empty for the same reason. The scan is kept honest by the live gate's
    // anti-vacuous branch, which asserts on the DERIVED SPAWNER SET and not
    // on this list, so nothing here can make a broken scanner pass.
];

/// One violation. Two variants running in **opposite directions**: the first
/// fires on an unnamed debt, the second on a debt that was paid.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SpawnViolation {
    /// A test reaches a `cargo` spawn and nobody named it.
    UnregisteredCargoSpawn {
        path: String,
        test: String,
        via: Vec<String>,
    },
    /// A registered test that no longer reaches a spawn, or that vanished. The
    /// ratchet: an exception list only pays itself down if *improving* is also
    /// a failure, otherwise progress leaves silent headroom nobody granted.
    RegisteredDebtPaid { path: String, test: String },
    /// A register entry whose reason is empty or whitespace-only. An unnamed
    /// debt wearing a name is not a name.
    RegisteredReasonMissing { path: String, test: String },
}

impl SpawnViolation {
    fn message(&self) -> String {
        match self {
            SpawnViolation::UnregisteredCargoSpawn { path, test, via } => format!(
                "{path}::{test} reaches a `cargo` spawn ({}) — a nested cargo inside a #[test] \
                 rebuilds the bin over the SHARED path target/debug/yoyo, and every integration \
                 test resolves env!(\"CARGO_BIN_EXE_yoyo\") to that same path, so whichever build \
                 ran last decides what they receive (#832: three CI reds).\n     \
                 Fix (any one): inject the subprocess as a resolver and drive the pure core (best \
                 — the discipline already used for added_ts, revisit_add_at's resolved title, and \
                 count_tests_via_cargo); or move the test to tests/ where it owns its own \
                 process; or — if this is deliberate — paste\n       \
                 (\"{path}\", \"{test}\", \"why this is acceptable and what actually runs it\"),\n     \
                 into REGISTERED_CARGO_SPAWNING_TESTS in tests/cargo_spawning_tests.rs.\n     \
                 The gate does not forbid a test that reaches a cargo spawn. It forbids an \
                 unnamed one.",
                via.join(", "),
            ),
            SpawnViolation::RegisteredDebtPaid { path, test } => format!(
                "{path}::{test} is listed in REGISTERED_CARGO_SPAWNING_TESTS but no longer \
                 reaches a `cargo` spawn (it stopped calling a spawner, or no longer exists) — \
                 the debt is paid.\n     \
                 Fix: delete its (\"{path}\", \"{test}\", ...) entry from \
                 REGISTERED_CARGO_SPAWNING_TESTS in tests/cargo_spawning_tests.rs. Fatal on \
                 purpose: the register only ratchets down if a repair is also a failure."
            ),
            SpawnViolation::RegisteredReasonMissing { path, test } => format!(
                "{path}::{test} is registered in REGISTERED_CARGO_SPAWNING_TESTS with an empty \
                 reason. An unnamed debt wearing a name is not a name.\n     \
                 Fix: write what is actually true — why this spawn is acceptable and what runs \
                 it — in the third field of its entry in tests/cargo_spawning_tests.rs."
            ),
        }
    }
}

/// One `fn` in `src/`, as the scanner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FnItem {
    path: String,
    name: String,
    body: String,
    is_test: bool,
}

/// A test that reaches a `cargo` spawn, with the route(s) it took.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SpawningTest {
    path: String,
    name: String,
    /// Spawner names called directly, or the literal `(direct spawn)`.
    via: Vec<String>,
}

/// Derive the spawner set: every **non-test** `fn` whose body contains a
/// `cargo` spawn, by name, deduplicated and sorted.
///
/// Non-test only, deliberately: a test that spawns `cargo` itself is caught by
/// the direct branch of [`find_spawning_tests`], and letting a test name enter
/// the spawner set would make it vouch for itself.
fn derive_spawners(fns: &[FnItem]) -> Vec<String> {
    let mut names: Vec<String> = fns
        .iter()
        .filter(|f| !f.is_test && spawns_cargo(&f.body))
        .map(|f| f.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Every `#[test]` that reaches a `cargo` spawn, one hop.
fn find_spawning_tests(fns: &[FnItem], spawners: &[String]) -> Vec<SpawningTest> {
    let mut out = Vec::new();
    for f in fns.iter().filter(|f| f.is_test) {
        let mut via: Vec<String> = spawners
            .iter()
            .filter(|s| calls_directly(&f.body, s))
            .cloned()
            .collect();
        if spawns_cargo(&f.body) {
            via.insert(0, "(direct spawn)".to_string());
        }
        if !via.is_empty() {
            out.push(SpawningTest {
                path: f.path.clone(),
                name: f.name.clone(),
                via,
            });
        }
    }
    out
}

/// Pure checker: given every spawning test the scanner found and the register,
/// report every violation.
///
/// No I/O, so both fatal branches are provable against a **fabricated** input
/// rather than by planting a real cargo-spawning test in `src/` — the same
/// discipline `tests/orphan_modules.rs` and `tests/feature_gated_tests.rs` use.
fn classify(spawning: &[SpawningTest], register: &[(&str, &str, &str)]) -> Vec<SpawnViolation> {
    let mut violations = Vec::new();

    for t in spawning {
        let registered = register
            .iter()
            .any(|(p, n, _)| *p == t.path && *n == t.name);
        if !registered {
            violations.push(SpawnViolation::UnregisteredCargoSpawn {
                path: t.path.clone(),
                test: t.name.clone(),
                via: t.via.clone(),
            });
        }
    }

    for (path, name, reason) in register {
        let still_violates = spawning.iter().any(|t| t.path == *path && t.name == *name);
        if !still_violates {
            // The ratchet, and the vanished case with it: a rename would
            // otherwise retire an entry without anyone deciding to.
            violations.push(SpawnViolation::RegisteredDebtPaid {
                path: (*path).to_string(),
                test: (*name).to_string(),
            });
        } else if reason.trim().is_empty() {
            violations.push(SpawnViolation::RegisteredReasonMissing {
                path: (*path).to_string(),
                test: (*name).to_string(),
            });
        }
    }

    violations
}

/// Does this body contain a `cargo` subprocess spawn?
///
/// Textual: `Command::new(` followed (across whitespace and newlines) by the
/// literal `"cargo"`. Limit 3 in the module doc — a path built through a
/// variable or an env var is invisible to this.
fn spawns_cargo(body: &str) -> bool {
    let b: Vec<char> = body.chars().collect();
    let needle: Vec<char> = "Command::new(".chars().collect();
    let arg: Vec<char> = "\"cargo\"".chars().collect();
    let mut i = 0usize;
    while i + needle.len() <= b.len() {
        if b[i..i + needle.len()] == needle[..] {
            let mut j = i + needle.len();
            while j < b.len() && b[j].is_whitespace() {
                j += 1;
            }
            if j + arg.len() <= b.len() && b[j..j + arg.len()] == arg[..] {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Qualifiers that may sit between an attribute and `fn` on a signature line.
const FN_QUALIFIERS: &[&str] = &[
    "pub",
    "pub(crate)",
    "pub(super)",
    "pub(self)",
    "async",
    "unsafe",
    "const",
    "extern",
    "\"C\"",
    "default",
];

/// The function name declared on this line, if the line is a `fn` signature.
///
/// Rejects a `fn` in type position (`f: fn(u8) -> u8`, `&dyn Fn(..)`) by
/// requiring everything before `fn ` to be an attribute chunk plus qualifiers.
fn fn_name_on_line(t: &str) -> Option<String> {
    let idx = t.find("fn ")?;
    // `fn` must be a word start.
    if idx > 0 {
        let prev = t[..idx].chars().next_back()?;
        if prev.is_alphanumeric() || prev == '_' {
            return None;
        }
    }
    let mut prefix = &t[..idx];
    // Drop a leading attribute chunk (`#[test] fn foo()`).
    if let Some(close) = prefix.rfind(']') {
        prefix = &prefix[close + 1..];
    }
    if !prefix
        .split_whitespace()
        .all(|w| FN_QUALIFIERS.contains(&w))
    {
        return None;
    }
    let name: String = t[idx + 3..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Scan one source file for every `fn`, with its body and whether it is a test.
///
/// Attributes are gathered line by line and reset by any line that is neither
/// an attribute, a comment, nor blank — so a `#[test]` three lines above an
/// unrelated `fn` cannot vouch for it.
fn extract_fns(path: &str, source: &str) -> Vec<FnItem> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    let mut offset = 0usize;

    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.chars().count();
        let t = line.trim();

        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if t.starts_with("#[") && !t.contains(" fn ") && !t.contains("] fn ") {
            attrs.push(t.to_string());
            continue;
        }

        let Some(name) = fn_name_on_line(t) else {
            attrs.clear();
            continue;
        };

        let mut all = attrs.clone();
        all.push(t.to_string());
        attrs.clear();

        let is_test = all
            .iter()
            .any(|a| a.contains("#[test]") || a.contains("#[tokio::test"));

        let Some((body, _)) = match_body(&chars, line_start) else {
            continue;
        };

        out.push(FnItem {
            path: path.to_string(),
            name,
            body,
            is_test,
        });
    }
    out
}

/// Recursively collect `(relative path, source)` for every `*.rs` under `dir`.
fn collect_rs_files(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    if !dir.exists() {
        return;
    }
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
            out.push((rel, content));
        }
    }
}

/// Say out loud what the gate did **not** check, through a raw stderr handle
/// rather than `eprintln!` — libtest's capture hook discards macro output from
/// *passing* tests, and a limit disclosure that only prints on failure is a
/// disclosure nobody reads. That eight-day silence is what
/// `tests/blind_round_grades.rs` had to pay off.
fn write_scope_note(files: usize, fns: usize, tests: usize, spawners: &[String], spawning: usize) {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "\ncargo-spawning-test gate: {files} file(s), {fns} fn(s), {tests} #[test](s) scanned in \
         src/; {} spawner(s) DERIVED ({}); {spawning} test(s) reach one, {} registered as debt.\n  \
         Limit 1: ONE HOP only — a test reaching a spawner through an intermediate helper is \
         invisible to this gate.\n  \
         Limit 2: matched by function NAME, not resolved path — two functions sharing a name in \
         different modules are indistinguishable, so one match can vouch for the wrong callee.\n  \
         Limit 3: textual match on the spawn — a command built through a variable, a PathBuf, or \
         an env-var-driven binary path is invisible.\n  \
         Limit 4: it CANNOT tell a harmful spawn from a harmless one. `cargo audit --version` \
         probes and builds nothing; `cargo test` builds and clobbers the shared target/debug/yoyo \
         uplift path. Both look identical here — which is why every register entry carries a \
         reason a human wrote.\n  \
         See tests/cargo_spawning_tests.rs.\n",
        spawners.len(),
        spawners.join(", "),
        REGISTERED_CARGO_SPAWNING_TESTS.len(),
    );
    let _ = err.flush();
}

fn scan_src() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &root, &mut files);
    files
}

#[test]
fn tests_that_reach_a_cargo_spawn_are_registered() {
    let files = scan_src();
    assert!(
        files.len() > 10,
        "source walk found only {} files — the walk is broken, not the repo",
        files.len()
    );

    let fns: Vec<FnItem> = files
        .iter()
        .flat_map(|(p, src)| extract_fns(p, src))
        .collect();
    let test_count = fns.iter().filter(|f| f.is_test).count();

    // Anti-vacuous, and the mirror failure is quieter than the bug: an empty
    // spawner set means the scanner broke or the API moved, and a gate that
    // can never fire is the "cannot fail loudly" defect wearing the opposite
    // sign. Fail loudly rather than pass on an empty scan.
    let spawners = derive_spawners(&fns);
    assert!(
        !spawners.is_empty(),
        "no non-test fn in src/ was seen spawning `cargo` — the scanner is broken, or every spawn \
         moved behind a variable. A gate that derives nothing and passes is not a pass. \
         ({} fn(s) scanned across {} file(s).)",
        fns.len(),
        files.len(),
    );
    assert!(
        test_count > 0,
        "no #[test] found anywhere in src/ — the test scanner is broken."
    );

    let spawning = find_spawning_tests(&fns, &spawners);
    write_scope_note(
        files.len(),
        fns.len(),
        test_count,
        &spawners,
        spawning.len(),
    );

    let violations = classify(&spawning, REGISTERED_CARGO_SPAWNING_TESTS);
    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "cargo-spawning-test gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/cargo_spawning_tests.rs.",
            violations.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(path: &str, name: &str, is_test: bool, body: &str) -> FnItem {
        FnItem {
            path: path.to_string(),
            name: name.to_string(),
            body: body.to_string(),
            is_test,
        }
    }

    fn s(path: &str, name: &str, via: &[&str]) -> SpawningTest {
        SpawningTest {
            path: path.to_string(),
            name: name.to_string(),
            via: via.iter().map(|v| (*v).to_string()).collect(),
        }
    }

    /// The fatal branch, proven against a **fabricated** input — never by
    /// planting a real cargo-spawning test in `src/`.
    #[test]
    fn an_unregistered_spawning_test_is_fatal() {
        let v = classify(&[s("src/a.rs", "test_x", &["run_cargo"])], &[]);
        assert_eq!(
            v,
            vec![SpawnViolation::UnregisteredCargoSpawn {
                path: "src/a.rs".to_string(),
                test: "test_x".to_string(),
                via: vec!["run_cargo".to_string()],
            }]
        );
        let m = v[0].message();
        // The remedy must be pasteable verbatim.
        assert!(
            m.contains("(\"src/a.rs\", \"test_x\", \"why this is acceptable"),
            "{m}"
        );
        assert!(
            m.contains("It forbids an \nunnamed one.") || m.contains("unnamed one."),
            "{m}"
        );
        assert!(m.contains("run_cargo"), "{m}");
    }

    /// The near-miss guard, both directions: a discriminator tested only on
    /// the side that fires is vacuous green.
    #[test]
    fn registered_and_non_spawning_tests_pass_through() {
        let v = classify(
            &[s("src/a.rs", "test_x", &["run_cargo"])],
            &[("src/a.rs", "test_x", "deliberate: probes only")],
        );
        assert!(v.is_empty(), "{v:?}");

        // A test that reaches nothing produces no row at all.
        let none: Vec<SpawningTest> = Vec::new();
        assert!(classify(&none, &[]).is_empty());
    }

    /// The ratchet, direction two: a registered test that stopped violating.
    #[test]
    fn a_registered_test_that_stopped_spawning_is_fatal() {
        let v = classify(&[], &[("src/a.rs", "test_x", "was deliberate")]);
        assert_eq!(
            v,
            vec![SpawnViolation::RegisteredDebtPaid {
                path: "src/a.rs".to_string(),
                test: "test_x".to_string(),
            }]
        );
        assert!(
            v[0].message().contains("the debt is paid"),
            "{}",
            v[0].message()
        );
    }

    /// The ratchet also catches a vanished test — a rename must not retire an
    /// entry without anyone deciding to.
    #[test]
    fn a_registered_test_that_vanished_is_fatal() {
        let v = classify(
            &[s("src/a.rs", "test_renamed", &["run_cargo"])],
            &[("src/a.rs", "test_old_name", "deliberate")],
        );
        // Both directions fire: the new name is unregistered, the old is paid.
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v.iter().any(|x| matches!(
            x,
            SpawnViolation::RegisteredDebtPaid { test, .. } if test == "test_old_name"
        )));
    }

    /// An unnamed debt wearing a name is not a name.
    #[test]
    fn an_empty_register_reason_is_fatal() {
        let v = classify(
            &[s("src/a.rs", "test_x", &["run_cargo"])],
            &[("src/a.rs", "test_x", "   \n ")],
        );
        assert_eq!(
            v,
            vec![SpawnViolation::RegisteredReasonMissing {
                path: "src/a.rs".to_string(),
                test: "test_x".to_string(),
            }]
        );
    }

    /// The spawner set is DERIVED. Non-test fns that spawn are in; test fns
    /// are deliberately excluded so a test cannot vouch for itself.
    #[test]
    fn spawners_are_derived_from_non_test_fns_only() {
        let fns = vec![
            f(
                "src/a.rs",
                "runner",
                false,
                "{ Command::new(\"cargo\").arg(\"test\"); }",
            ),
            f("src/a.rs", "innocent", false, "{ let x = 1; }"),
            f("src/a.rs", "a_test", true, "{ Command::new(\"cargo\"); }"),
        ];
        assert_eq!(derive_spawners(&fns), vec!["runner".to_string()]);
    }

    /// Anti-vacuous, driven from a fabricated file list: no spawner anywhere
    /// must be visible as an empty set, not silently pass.
    #[test]
    fn an_empty_spawner_set_is_detectable() {
        let fns = vec![f("src/a.rs", "innocent", false, "{ let x = 1; }")];
        assert!(
            derive_spawners(&fns).is_empty(),
            "the anti-vacuous branch in the live test keys on exactly this being empty"
        );
    }

    /// One hop, plus the direct branch.
    #[test]
    fn find_spawning_tests_covers_one_hop_and_direct() {
        let fns = vec![
            f("src/a.rs", "runner", false, "{ Command::new(\"cargo\"); }"),
            f("src/a.rs", "hops", true, "{ let n = runner(); }"),
            f(
                "src/a.rs",
                "direct",
                true,
                "{ Command::new(\"cargo\").arg(\"x\"); }",
            ),
            f("src/a.rs", "clean", true, "{ assert!(true); }"),
            // Two hops away: invisible, and limit 1 says so.
            f("src/a.rs", "middle", false, "{ runner(); }"),
            f("src/a.rs", "two_hops", true, "{ middle(); }"),
        ];
        let spawners = derive_spawners(&fns);
        let got = find_spawning_tests(&fns, &spawners);
        let names: Vec<&str> = got.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["hops", "direct"], "{got:?}");
        assert_eq!(got[1].via, vec!["(direct spawn)".to_string()]);
    }

    #[test]
    fn spawns_cargo_table() {
        let cases: &[(&str, bool)] = &[
            ("Command::new(\"cargo\")", true),
            ("std::process::Command::new(\"cargo\")", true),
            ("Command::new(\n    \"cargo\",\n)", true),
            ("Command::new(\"npm\")", false),
            ("Command::new(\"cargo-audit\")", false),
            ("let cargo = 1;", false),
            // Limit 3, pinned as a KNOWN blind spot rather than implied.
            ("Command::new(bin)", false),
        ];
        for (src, want) in cases {
            assert_eq!(spawns_cargo(src), *want, "spawns_cargo({src:?})");
        }
    }

    #[test]
    fn fn_name_on_line_table() {
        let cases: &[(&str, Option<&str>)] = &[
            ("fn foo() {", Some("foo")),
            ("pub fn foo() {", Some("foo")),
            ("pub(crate) fn foo() {", Some("foo")),
            ("async fn foo() {", Some("foo")),
            ("pub(crate) async fn foo() {", Some("foo")),
            ("#[test] fn foo() {", Some("foo")),
            ("    unsafe fn foo() {", Some("foo")),
            // A fn in *type* position is not a declaration.
            ("f: fn(u8) -> u8,", None),
            ("added_ts: &dyn Fn(&str) -> Option<String>,", None),
            ("let x = 1;", None),
        ];
        for (src, want) in cases {
            assert_eq!(
                fn_name_on_line(src).as_deref(),
                *want,
                "fn_name_on_line({src:?})"
            );
        }
    }

    /// The brace scanner: the four things that can carry a stray brace.
    #[test]
    fn match_body_skips_literals_and_comments() {
        let src: Vec<char> = "fn f() { let s = \"}\"; let c = '}'; /* } /* } */ */ // }\n }"
            .chars()
            .collect();
        let (body, _) = match_body(&src, 0).expect("body");
        assert!(body.ends_with('}'));
        assert_eq!(body.matches('{').count(), 1, "{body}");
    }

    /// A lifetime tick must not open a char literal.
    #[test]
    fn match_body_does_not_mistake_a_lifetime_for_a_char() {
        let src: Vec<char> = "fn f<'a>(x: &'a str) -> &'a str { x }".chars().collect();
        let (body, _) = match_body(&src, 0).expect("body");
        assert_eq!(body, "{ x }");
    }

    #[test]
    fn match_body_skips_raw_strings() {
        let src: Vec<char> = "fn f() { let s = r#\"}\"#; }".chars().collect();
        let (body, _) = match_body(&src, 0).expect("body");
        assert!(body.ends_with("}"), "{body}");
        assert!(body.contains("r#"), "{body}");
    }

    #[test]
    fn calls_directly_requires_a_word_boundary_and_parens() {
        assert!(calls_directly(
            "{ security_audit_command(&x) }",
            "security_audit_command"
        ));
        assert!(calls_directly(
            "{ lint::security_audit_command(&x) }",
            "security_audit_command"
        ));
        assert!(!calls_directly(
            "{ x.security_audit_command() }",
            "security_audit_command"
        ));
        assert!(!calls_directly(
            "{ my_security_audit_command() }",
            "security_audit_command"
        ));
        // A mention without a call is not a call.
        assert!(!calls_directly(
            "{ let s = \"security_audit_command\"; }",
            "security_audit_command"
        ));
    }

    /// `extract_fns` must see attributes, bodies and test-ness, and must not
    /// let an attribute leak onto the following item.
    #[test]
    fn extract_fns_reads_attributes_and_bodies() {
        let src = "\
#[test]
fn a_test() { helper(); }

// A #[test] far above must not vouch for the next fn.
#[serial]
fn not_a_test() {}

#[tokio::test]
async fn asyncy() {}
";
        let got = extract_fns("src/x.rs", src);
        let names: Vec<(&str, bool)> = got.iter().map(|f| (f.name.as_str(), f.is_test)).collect();
        assert_eq!(
            names,
            vec![("a_test", true), ("not_a_test", false), ("asyncy", true)]
        );
    }

    /// Every register entry must carry a real reason and name a `src/` path.
    ///
    /// **Superseded clause, recorded rather than erased (Day 188, #834).** This
    /// test was `..._is_non_empty_and_every_reason_is_real` and opened with
    /// `assert!(!REGISTERED_CARGO_SPAWNING_TESTS.is_empty())`, because on Day
    /// 179 the register's SIZE was the finding. The 8 entries it guarded have
    /// now been paid, so that clause would forbid the ratchet's own terminal
    /// state — an exception list that may never reach zero is not a ratchet.
    ///
    /// It is dropped rather than weakened, and the distinction matters: both
    /// failure modes its message named are guarded elsewhere, so nothing is
    /// left uncovered. "The scan found nothing" is the live gate's anti-vacuous
    /// branch, which asserts on the DERIVED SPAWNER SET (`!spawners.is_empty()`,
    /// plus the file and `#[test]` counts) and never on this list. "Someone
    /// deleted the finding" is branch 1: a test that reaches a spawn and is not
    /// registered is fatal, so an entry cannot be dropped without converting
    /// the test it named. Precedent for an empty register:
    /// `tests/orphan_modules.rs` ships `REGISTERED_ORPHANS` empty.
    ///
    /// What this still pins is what it always really pinned — that no entry is
    /// an unnamed debt wearing a name, and that none points outside `src/`.
    #[test]
    fn every_register_entry_carries_a_real_reason_and_a_src_path() {
        for (path, test, reason) in REGISTERED_CARGO_SPAWNING_TESTS {
            assert!(
                !reason.trim().is_empty(),
                "{path}::{test} is registered with no reason"
            );
            assert!(
                path.starts_with("src/"),
                "{path}::{test}: this gate only scans src/"
            );
        }
    }
}
