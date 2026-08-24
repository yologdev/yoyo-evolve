//! Shared-global test-race gate: a `#[test]` in `src/` that writes a
//! process-global must either carry `#[serial]` or be named in a register.
//!
//! Why this exists. Three separate CI reds in one window were the same class,
//! and each was fixed as a one-site repair:
//!
//! * `CONVERSATION_STASH` shared across 14 stash tests (fixed by @yuanhao in
//!   `04c6d252`);
//! * the cwd-moving tests, which move the **process** working directory out
//!   from under any sibling test resolving a relative path (#780 — it took
//!   *two* tasks);
//! * `EFFORT_LEVEL`, an `AtomicU8` three `apply_effort_hint` tests each wrote,
//!   which failed 6 of 12 runs under libtest's default parallelism (Day 177).
//!
//! Nothing enumerated the remainder, so the class kept re-appearing as a
//! surprise red — and a flaky red in the evolve loop means `git reset --hard`
//! on whatever correct work was sitting beside it. My own archive, verbatim:
//! *"when a mitigation's protection is collective, applying it to the N known
//! offenders gives an N/N counter that feels like closure while the property
//! stays false."* Three one-site fixes were exactly that counter. This gate
//! buys the enumeration.
//!
//! Shape borrowed verbatim from its four siblings (`tests/module_size.rs`,
//! `tests/blind_round_grades.rs`, `tests/orphan_modules.rs`,
//! `tests/doc_version_claims.rs`): a pure decision function with all the
//! filesystem walking at one call site, fatal on the *unnamed* case, a
//! register for deliberate exceptions, and a ratchet so the register can only
//! shrink.
//!
//! **The gate does not forbid an unserialised global write. It forbids an
//! unnamed one.** The escape hatch is the point: a gate whose only remedy is
//! "revert the whole task" eats the correct work sitting beside the violation
//! (the Day-166 module-size lesson), so the compliant response is one line in
//! a register that is itself the enumeration of the debt.
//!
//! # What this gate CANNOT do
//!
//! Read this before trusting a green run. "Could not check" must not read as
//! "checked; clean". These three are printed on every passing run, through a
//! raw stderr handle, for the reason `tests/blind_round_grades.rs` had to pay
//! off after eight days of nobody reading a passing test's warning.
//!
//! 1. **It matches setters by name and by *direct call* only.** A test that
//!    reaches a global through a helper — its own `fn setup()`, a builder, a
//!    production function that happens to write one — is invisible to it.
//! 2. **`#[serial]` only serialises against *other* `#[serial]` tests.** A
//!    test this gate calls compliant is still fully exposed to unmarked
//!    *readers* of the same global. Compliance here means "follows the repo
//!    convention", never "proven race-free". That is not a quibble: #780's
//!    victims were unmarked CWD-*readers*, and every one of them would have
//!    passed this gate.
//! 3. **It says nothing about globals written by production code a test calls
//!    into.** Only the literal setter names below, appearing literally in a
//!    test body.
//!
//! A fourth caveat lives here rather than in the printed note because it is
//! about the parser, not the policy: test bodies are delimited by brace
//! matching over a scanner that skips line/block comments, char literals, and
//! normal *and* raw string literals. It is not a Rust lexer — a macro token
//! tree with deliberately unbalanced braces would mis-scope a body. There are
//! none in `src/` today, and the scanner is pinned by its own table test.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Functions in `src/` whose job is to write a process-global.
///
/// **Hand-written on purpose**, and tied to reality by
/// `every_named_setter_still_exists` below — a rename must fail *that* test
/// rather than silently un-covering every call site it guarded. Same
/// two-direction discipline as `ROUTED_SUBCOMMANDS` in `src/dispatch_sub.rs`.
///
/// Membership rule: a non-test `fn` whose body's purpose is to store into a
/// mutable `static` (an `Atomic*`, a `Mutex`/`RwLock` cell, or a write-once
/// `OnceLock`). Pure *readers* are deliberately absent — a reader cannot
/// corrupt a sibling test, it can only be corrupted by one, and limit 2 above
/// says so out loud.
const GLOBAL_SETTERS: &[&str] = &[
    // src/format/mod.rs
    "disable_bell",
    "disable_color",
    "disable_notify",
    "enable_quiet",
    "set_notify_command",
    "reset_context_budget_warning",
    "reset_shown_hints",
    // src/format/tools.rs
    "set_plain_output",
    // src/cli.rs
    "enable_verbose",
    "set_configured_provider",
    "set_continue_on_silence",
    "set_trust_project",
    // src/cli_config.rs
    "enable_auto_edit",
    "set_effective_context_tokens",
    "set_effort_level",
    "set_safe_mode",
    // src/commands_config.rs
    "set_architect_mode",
    "set_editor_model",
    "set_read_mode",
    "set_teach_mode",
    // src/commands_plan.rs
    "clear_last_plan",
    "mark_plan_applied",
    "mark_step",
    "set_last_plan",
    "set_plan_apply_active",
    "set_plan_mode",
    // src/commands_goal.rs
    "mark_verify_set_this_session",
    "reset_verify_set_this_session",
    // src/commands_run.rs
    "clear_last_failed_run",
    "set_last_failed_run",
    // src/commands_session.rs
    "reset_compact_thrash",
    // src/commands_todo.rs
    "todo_add",
    "todo_clear",
    "todo_remove",
    // src/watch.rs
    "clear_watch_command",
    "set_watch_command",
    "set_watch_commands",
    // src/conversations.rs
    "store_last_side",
    // src/rtk.rs
    "disable_rtk",
    // src/prompt_budget.rs
    "enable_audit_log",
];

/// Tests that write a process-global without `#[serial]`, recorded by hand.
///
/// **Debt, not absolution.** An entry does not make a test safe; it records
/// that a test mutates shared process state, that I knew, and that I chose not
/// to fix it in that session. The ratchet below (a registered test that stops
/// violating, or vanishes, is *also* fatal) is what stops it from becoming
/// permission.
///
/// Generated by running this gate against the tree and pasting what it found —
/// not hand-curated, and deliberately not trimmed to look better. The size of
/// this list *is* the finding.
const REGISTERED_GLOBAL_RACES: &[(&str, &str)] = &[
    ("src/cli_config.rs", "test_auto_edit_toggle"),
    (
        "src/cli_config.rs",
        "test_effective_context_tokens_roundtrip",
    ),
    ("src/cli_config.rs", "test_effort_level_roundtrip"),
    (
        "src/commands_run.rs",
        "test_last_failed_run_cleared_on_success",
    ),
    ("src/commands_run.rs", "test_last_failed_run_initially_none"),
    (
        "src/commands_run.rs",
        "test_last_failed_run_store_and_retrieve",
    ),
    (
        "src/commands_session.rs",
        "test_compact_thrash_detection_increments_on_low_reduction",
    ),
    (
        "src/commands_session.rs",
        "test_compact_thrash_detection_resets_on_meaningful_reduction",
    ),
    (
        "src/commands_session.rs",
        "test_is_compact_thrashing_boundary",
    ),
    ("src/commands_session.rs", "test_reset_compact_thrash"),
    (
        "src/conversations.rs",
        "test_last_side_store_set_get_overwrite_empty",
    ),
    ("src/format/mod.rs", "test_disable_notify_is_callable"),
    ("src/format/mod.rs", "test_enable_quiet_is_callable"),
    ("src/format/mod.rs", "test_hint_priority_first_turn_wins"),
    (
        "src/format/mod.rs",
        "test_print_context_usage_quiet_suppressed",
    ),
    ("src/format/mod.rs", "test_print_usage_quiet_suppressed"),
    ("src/format/tools.rs", "test_plain_output_flag_roundtrip"),
];

/// A way this gate can be violated. Two values, running in **opposite
/// directions** — the same two-direction discipline every sibling gate uses.
#[derive(Debug, PartialEq, Eq)]
enum RaceViolation {
    /// A `#[test]` that calls a global setter, carries no `#[serial]`, and is
    /// not registered. The defect this gate exists for.
    UnserialisedGlobalWrite {
        path: String,
        test: String,
        setters: Vec<String>,
    },
    /// A registered test that no longer violates — it gained `#[serial]`,
    /// stopped calling a setter, or vanished. The ratchet: an exception list
    /// only pays itself down if *improving* is also a failure, otherwise
    /// progress leaves silent headroom nobody granted.
    RegisteredDebtPaid { path: String, test: String },
}

impl RaceViolation {
    fn message(&self) -> String {
        match self {
            RaceViolation::UnserialisedGlobalWrite {
                path,
                test,
                setters,
            } => format!(
                "{path}::{test} writes process-global state ({}) and carries no #[serial], so it \
                 races every sibling test libtest happens to schedule beside it.\n     \
                 Fix (any one): pass the value explicitly instead of writing the global (best — \
                 #[serial] only serialises against other #[serial] tests, so it leaves unmarked \
                 readers exposed); or add #[serial]; or — if this is deliberate — paste\n       \
                 (\"{path}\", \"{test}\"),\n     \
                 into REGISTERED_GLOBAL_RACES in tests/global_state_races.rs.\n     \
                 The gate does not forbid an unserialised global write. It forbids an unnamed one.",
                setters.join(", "),
            ),
            RaceViolation::RegisteredDebtPaid { path, test } => format!(
                "{path}::{test} is listed in REGISTERED_GLOBAL_RACES but no longer violates (it \
                 gained #[serial], stopped calling a global setter, or no longer exists) — the \
                 debt is paid.\n     \
                 Fix: delete the line (\"{path}\", \"{test}\") from REGISTERED_GLOBAL_RACES in \
                 tests/global_state_races.rs. Fatal on purpose: the register only ratchets down \
                 if a repair is also a failure."
            ),
        }
    }
}

/// One `#[test]` function, as the scanner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestFn {
    path: String,
    name: String,
    serial: bool,
    /// Global setters called directly from this test's body, in
    /// `GLOBAL_SETTERS` order, deduplicated.
    setters: Vec<String>,
}

/// Pure checker: given every test the scanner found and the register, report
/// every violation.
///
/// No I/O, so the fatal branch is provable against a **fabricated** test list
/// rather than by planting a real racing test in `src/` — the same discipline
/// `tests/orphan_modules.rs` uses.
fn classify(tests: &[TestFn], register: &[(&str, &str)]) -> Vec<RaceViolation> {
    let mut violations = Vec::new();
    let registered = |t: &TestFn| register.iter().any(|(p, n)| *p == t.path && *n == t.name);

    for t in tests {
        let violates = !t.setters.is_empty() && !t.serial;
        match (violates, registered(t)) {
            (true, false) => violations.push(RaceViolation::UnserialisedGlobalWrite {
                path: t.path.clone(),
                test: t.name.clone(),
                setters: t.setters.clone(),
            }),
            (false, true) => violations.push(RaceViolation::RegisteredDebtPaid {
                path: t.path.clone(),
                test: t.name.clone(),
            }),
            _ => {}
        }
    }

    // A registered test that no longer exists is its own case — reported, not
    // silently ignored, because a rename would otherwise retire an entry
    // without anyone deciding to.
    for (path, name) in register {
        if !tests.iter().any(|t| t.path == *path && t.name == *name) {
            violations.push(RaceViolation::RegisteredDebtPaid {
                path: (*path).to_string(),
                test: (*name).to_string(),
            });
        }
    }

    violations
}

/// Find the body of the function whose signature starts at or after `from`,
/// returning `(body_including_braces, end_offset)`.
///
/// Brace matching that skips the four things that can carry a stray brace:
/// line comments, block comments (nesting, as Rust allows), char literals, and
/// string literals — normal (with `\` escapes) and raw (`r"…"` / `r#"…"#` at
/// any hash count). Not a Rust lexer; see the module doc's fourth caveat.
fn match_body(src: &[char], from: usize) -> Option<(String, usize)> {
    let open = (from..src.len()).find(|&i| src[i] == '{')?;
    let mut depth = 0usize;
    let mut i = open;
    while i < src.len() {
        let c = src[i];
        match c {
            '/' if i + 1 < src.len() && src[i + 1] == '/' => {
                while i < src.len() && src[i] != '\n' {
                    i += 1;
                }
            }
            '/' if i + 1 < src.len() && src[i + 1] == '*' => {
                let mut nest = 1usize;
                i += 2;
                while i < src.len() && nest > 0 {
                    if src[i] == '/' && i + 1 < src.len() && src[i + 1] == '*' {
                        nest += 1;
                        i += 2;
                    } else if src[i] == '*' && i + 1 < src.len() && src[i + 1] == '/' {
                        nest -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            'r' if raw_string_hashes(src, i).is_some() => {
                let hashes = raw_string_hashes(src, i).unwrap();
                i += 1 + hashes + 1; // r + #* + "
                i = skip_raw_string(src, i, hashes);
            }
            'b' if i + 1 < src.len()
                && src[i + 1] == 'r'
                && raw_string_hashes(src, i + 1).is_some() =>
            {
                let hashes = raw_string_hashes(src, i + 1).unwrap();
                i += 2 + hashes + 1;
                i = skip_raw_string(src, i, hashes);
            }
            '"' => {
                i += 1;
                while i < src.len() {
                    if src[i] == '\\' {
                        i += 2;
                    } else if src[i] == '"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            '\'' => {
                // A char literal, or a lifetime tick. `char_literal_len`-style
                // discrimination: only advance past a *closed* literal.
                match char_literal_len(src, i) {
                    Some(n) => i += n,
                    None => i += 1,
                }
            }
            '{' => {
                depth += 1;
                i += 1;
            }
            '}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some((src[open..i].iter().collect(), i));
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// `Some(hash_count)` when a raw string literal opens at `i` (`src[i] == 'r'`).
fn raw_string_hashes(src: &[char], i: usize) -> Option<usize> {
    if src.get(i) != Some(&'r') {
        return None;
    }
    // An `r` that is merely the tail of an identifier opens nothing.
    if i > 0 && (src[i - 1].is_alphanumeric() || src[i - 1] == '_') {
        return None;
    }
    let mut j = i + 1;
    let mut hashes = 0usize;
    while src.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if src.get(j) == Some(&'"') {
        Some(hashes)
    } else {
        None
    }
}

/// Advance past a raw string body, given the opening hash count. `i` points
/// just past the opening quote.
fn skip_raw_string(src: &[char], mut i: usize, hashes: usize) -> usize {
    while i < src.len() {
        if src[i] == '"' {
            let mut k = i + 1;
            let mut seen = 0usize;
            while seen < hashes && src.get(k) == Some(&'#') {
                seen += 1;
                k += 1;
            }
            if seen == hashes {
                return k;
            }
        }
        i += 1;
    }
    i
}

/// Length of the char literal starting at `i`, or `None` if the tick opens no
/// closed literal (a lifetime: `&'a str`, `'static`).
fn char_literal_len(src: &[char], i: usize) -> Option<usize> {
    if src.get(i) != Some(&'\'') {
        return None;
    }
    let mut j = i + 1;
    if src.get(j) == Some(&'\\') {
        j += 1;
        // `\u{7d}` and friends: run to the closing tick.
        while j < src.len() && src[j] != '\'' {
            j += 1;
        }
    } else if j < src.len() {
        j += 1;
    }
    if src.get(j) == Some(&'\'') {
        Some(j - i + 1)
    } else {
        None
    }
}

/// Does `body` contain a *direct* call to `name`?
///
/// Requires the char before the name to be neither an identifier char nor `.`
/// — so `format::enable_quiet()` and a bare `enable_quiet()` both count, while
/// `x.enable_quiet()` and `my_enable_quiet()` do not.
fn calls_directly(body: &str, name: &str) -> bool {
    let b: Vec<char> = body.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let mut i = 0usize;
    while i + n.len() <= b.len() {
        if b[i..i + n.len()] == n[..] {
            let before_ok =
                i == 0 || !(b[i - 1].is_alphanumeric() || b[i - 1] == '_' || b[i - 1] == '.');
            let mut j = i + n.len();
            while j < b.len() && b[j].is_whitespace() {
                j += 1;
            }
            if before_ok && b.get(j) == Some(&'(') {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Scan one source file for `#[test]` / `#[tokio::test]` functions.
///
/// Attributes are gathered line by line and reset by any line that is neither
/// an attribute, a comment, nor blank — so a `#[serial]` three lines above an
/// unrelated `fn` cannot vouch for it.
fn extract_tests(path: &str, source: &str, setters: &[&str]) -> Vec<TestFn> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    // Char offset of the start of each line.
    let mut offset = 0usize;

    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.chars().count();
        let t = line.trim();

        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if t.starts_with("#[") && !t.contains(" fn ") && !t.starts_with("#[test] fn") {
            attrs.push(t.to_string());
            continue;
        }

        let is_fn = t.starts_with("fn ")
            || t.starts_with("async fn ")
            || t.starts_with("pub fn ")
            || t.contains("] fn ")
            || t.contains("] async fn ");
        if !is_fn {
            attrs.clear();
            continue;
        }

        // The attribute may be on this same line (`#[test] fn foo()`).
        let mut all = attrs.clone();
        all.push(t.to_string());
        attrs.clear();

        let is_test = all
            .iter()
            .any(|a| a.contains("#[test]") || a.contains("#[tokio::test"));
        if !is_test {
            continue;
        }
        let serial = all.iter().any(|a| a.contains("serial"));

        let Some(rest) = t.split("fn ").nth(1) else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }

        let Some((body, _)) = match_body(&chars, line_start) else {
            continue;
        };
        let called: Vec<String> = setters
            .iter()
            .filter(|s| calls_directly(&body, s))
            .map(|s| (*s).to_string())
            .collect();

        out.push(TestFn {
            path: path.to_string(),
            name,
            serial,
            setters: called,
        });
    }
    out
}

/// Recursively collect `(relative path, source)` for every `*.rs` under `dir`.
fn collect_rs_files(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
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
fn write_scope_note(tests: usize, writers: usize, serial: usize, registered: usize) {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "\nshared-global race gate: {tests} test(s) scanned in src/; {writers} call a global \
         setter directly ({serial} #[serial], {registered} registered as debt).\n  \
         Limit 1: setters are matched by NAME and by DIRECT CALL only — a test that reaches a \
         global through a helper is invisible to this gate.\n  \
         Limit 2: #[serial] only serialises against other #[serial] tests, so a test this gate \
         calls compliant is still exposed to unmarked READERS of the same global. Compliance \
         here means \"follows the repo convention\", not \"proven race-free\".\n  \
         Limit 3: it says nothing about globals written by production code a test calls into.\n  \
         See tests/global_state_races.rs.\n"
    );
    let _ = err.flush();
}

fn scan_src() -> Vec<TestFn> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &root, &mut files);
    assert!(
        files.len() > 10,
        "source walk found only {} files — the walk is broken, not the repo",
        files.len()
    );
    files
        .iter()
        .flat_map(|(p, src)| extract_tests(p, src, GLOBAL_SETTERS))
        .collect()
}

#[test]
fn tests_that_write_process_globals_are_serialised_or_registered() {
    let tests = scan_src();

    // Anti-vacuous: a scanner that finds nothing and passes is the
    // vacuous-green shape. Deleting the convention must fail loudly.
    let writers = tests.iter().filter(|t| !t.setters.is_empty()).count();
    assert!(
        writers > 0,
        "no test in src/ was seen calling any of the {} names in GLOBAL_SETTERS — the scanner is \
         broken, or every setter was renamed. A gate that finds nothing and passes is not a pass.",
        GLOBAL_SETTERS.len()
    );

    let serial = tests
        .iter()
        .filter(|t| !t.setters.is_empty() && t.serial)
        .count();
    write_scope_note(tests.len(), writers, serial, REGISTERED_GLOBAL_RACES.len());

    let violations = classify(&tests, REGISTERED_GLOBAL_RACES);
    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "shared-global race gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/global_state_races.rs.",
            violations.len()
        );
    }
}

/// Drift guard: every name in `GLOBAL_SETTERS` must still be defined in
/// `src/`. A rename must fail *here* rather than silently un-covering every
/// call site that name was guarding — the direction a presence-only scanner is
/// structurally blind to.
#[test]
fn every_named_setter_still_exists() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &root, &mut files);
    let all: String = files.iter().map(|(_, s)| s.as_str()).collect();

    let missing: Vec<&str> = GLOBAL_SETTERS
        .iter()
        .filter(|name| !all.contains(&format!("fn {name}(")))
        .copied()
        .collect();

    assert!(
        missing.is_empty(),
        "GLOBAL_SETTERS names {} function(s) that no longer exist in src/: {missing:?}\n\
         Fix: rename them here too, or drop them — a stale name silently covers nothing.",
        missing.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(path: &str, name: &str, serial: bool, setters: &[&str]) -> TestFn {
        TestFn {
            path: path.to_string(),
            name: name.to_string(),
            serial,
            setters: setters.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    /// The fatal branch, proven against a **fabricated** test list — never by
    /// planting a real racing test in `src/`.
    #[test]
    fn an_unserialised_unregistered_writer_is_fatal() {
        let v = classify(&[t("src/a.rs", "test_x", false, &["set_read_mode"])], &[]);
        assert_eq!(
            v,
            vec![RaceViolation::UnserialisedGlobalWrite {
                path: "src/a.rs".to_string(),
                test: "test_x".to_string(),
                setters: vec!["set_read_mode".to_string()],
            }]
        );
        let m = v[0].message();
        // The remedy must be pasteable verbatim.
        assert!(m.contains("(\"src/a.rs\", \"test_x\"),"), "{m}");
        assert!(m.contains("It forbids an unnamed one."), "{m}");
    }

    /// The near-miss guard, both directions: a discriminator tested only on
    /// the side that fires is vacuous green.
    #[test]
    fn serialised_and_non_writing_tests_pass_through() {
        let v = classify(
            &[
                t("src/a.rs", "serial_one", true, &["set_read_mode"]),
                t("src/a.rs", "writes_nothing", false, &[]),
            ],
            &[],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn a_registered_writer_is_silent() {
        let v = classify(
            &[t("src/a.rs", "test_x", false, &["set_read_mode"])],
            &[("src/a.rs", "test_x")],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    /// The ratchet, direction 1: a registered test that stopped violating.
    #[test]
    fn a_registered_test_that_gained_serial_is_fatal() {
        let v = classify(
            &[t("src/a.rs", "test_x", true, &["set_read_mode"])],
            &[("src/a.rs", "test_x")],
        );
        assert_eq!(
            v,
            vec![RaceViolation::RegisteredDebtPaid {
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

    /// The ratchet, direction 2: the test vanished.
    #[test]
    fn a_registered_test_that_vanished_is_fatal() {
        let v = classify(&[], &[("src/gone.rs", "test_x")]);
        assert_eq!(
            v,
            vec![RaceViolation::RegisteredDebtPaid {
                path: "src/gone.rs".to_string(),
                test: "test_x".to_string(),
            }]
        );
    }

    /// Same test name in two files must not be confused — the key is
    /// `(path, name)`, not `name`.
    #[test]
    fn the_register_key_is_path_and_name() {
        let v = classify(
            &[
                t("src/a.rs", "test_x", false, &["set_read_mode"]),
                t("src/b.rs", "test_x", false, &["set_read_mode"]),
            ],
            &[("src/a.rs", "test_x")],
        );
        assert_eq!(v.len(), 1);
        assert!(matches!(
            &v[0],
            RaceViolation::UnserialisedGlobalWrite { path, .. } if path == "src/b.rs"
        ));
    }

    #[test]
    fn calls_directly_requires_a_call_not_a_substring() {
        assert!(calls_directly("    enable_quiet();", "enable_quiet"));
        assert!(calls_directly("format::enable_quiet ();", "enable_quiet"));
        assert!(!calls_directly("my_enable_quiet();", "enable_quiet"));
        assert!(!calls_directly("x.enable_quiet();", "enable_quiet"));
        assert!(!calls_directly("// enable_quiet is nice", "enable_quiet"));
        assert!(!calls_directly("let enable_quiet = 1;", "enable_quiet"));
    }

    /// The brace matcher must survive the four things that carry stray braces.
    #[test]
    fn match_body_skips_literals_and_comments() {
        let cases: &[(&str, &str)] = &[
            ("fn f() { let s = \"}\"; }", "{ let s = \"}\"; }"),
            ("fn f() { let s = r#\"}\"#; }", "{ let s = r#\"}\"#; }"),
            ("fn f() { // }\n }", "{ // }\n }"),
            ("fn f() { /* } /* } */ */ }", "{ /* } /* } */ */ }"),
            ("fn f() { let c = '}'; }", "{ let c = '}'; }"),
            ("fn f() { let c = '\\u{7d}'; }", "{ let c = '\\u{7d}'; }"),
            ("fn f<'a>(x: &'a str) { g(x); }", "{ g(x); }"),
            ("fn f() { if x { y } }", "{ if x { y } }"),
            ("fn f() { let s = \"\\\"}\"; }", "{ let s = \"\\\"}\"; }"),
        ];
        for (src, want) in cases {
            let chars: Vec<char> = src.chars().collect();
            let got = match_body(&chars, 0).map(|(b, _)| b);
            assert_eq!(got.as_deref(), Some(*want), "input: {src}");
        }
    }

    #[test]
    fn extract_tests_reads_attributes_and_bodies() {
        let src = "\
#[test]
fn plain() { set_read_mode(true); }

#[test]
#[serial]
fn marked() { set_read_mode(true); }

// A #[serial] far above must not vouch for the next fn.
#[serial]
fn not_a_test() {}

fn ordinary() { set_read_mode(true); }

#[tokio::test]
async fn asyncy() { set_read_mode(true); }
";
        let got = extract_tests("src/x.rs", src, &["set_read_mode"]);
        let names: Vec<(&str, bool, usize)> = got
            .iter()
            .map(|t| (t.name.as_str(), t.serial, t.setters.len()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("plain", false, 1),
                ("marked", true, 1),
                ("asyncy", false, 1)
            ]
        );
    }

    /// A `#[serial]` attached to a *previous* item must not leak onto the next
    /// test — the attribute buffer has to reset on any real code line.
    #[test]
    fn attributes_do_not_leak_across_items() {
        let src = "\
#[serial]
fn helper() {}

#[test]
fn racy() { enable_quiet(); }
";
        let got = extract_tests("src/x.rs", src, &["enable_quiet"]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "racy");
        assert!(!got[0].serial, "serial leaked from the previous item");
    }
}
