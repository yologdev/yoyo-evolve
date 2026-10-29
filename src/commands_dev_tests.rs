//! Tests for `commands_dev::project_context_check` (#1045), kept out of
//! `commands_dev.rs`, which sits near the module size cap.

use crate::commands_dev::{command_form, project_context_check, DoctorStatus, DoctorSurface};

const REASON: &str = "denied: /proj";

#[test]
fn refused_and_admitted_files_are_both_named() {
    let listing = (
        vec![("YOYO.md", 3)],
        vec![("CLAUDE.md", REASON.to_string())],
    );
    let row = project_context_check(&listing, DoctorSurface::Repl);
    assert_eq!(row.name, "Project context");
    assert_eq!(row.status, DoctorStatus::Pass);
    assert_eq!(
        row.detail,
        "YOYO.md (3 lines), CLAUDE.md (refused by the directory fence: denied: /proj)"
    );
}

#[test]
fn all_refused_names_the_file_and_gives_no_create_advice_on_either_surface() {
    let listing = (vec![], vec![("CLAUDE.md", REASON.to_string())]);
    for surface in [DoctorSurface::Repl, DoctorSurface::Shell] {
        let row = project_context_check(&listing, surface);
        assert_eq!(row.status, DoctorStatus::Warn);
        assert_eq!(
            row.detail,
            "CLAUDE.md (refused by the directory fence: denied: /proj)"
        );
        assert!(!row.detail.contains("init"), "{}", row.detail);
        assert!(!row.detail.contains("create YOYO.md"), "{}", row.detail);
    }
}

#[test]
fn refused_reason_is_sanitized() {
    let listing = (
        vec![],
        vec![("CLAUDE.md", "denied: /p\x1b[31m".to_string())],
    );
    // Anti-vacuous: the fixture really carries the hostile byte.
    assert!(listing.1[0].1.as_bytes().contains(&0x1b));
    let row = project_context_check(&listing, DoctorSurface::Shell);
    assert!(!row.detail.as_bytes().contains(&0x1b), "{:?}", row.detail);
}

#[test]
fn nothing_refused_is_byte_identical_to_before() {
    // Near-miss: no fence, the pre-#1045 rows exactly.
    let listing = (vec![("YOYO.md", 3), ("CLAUDE.md", 7)], vec![]);
    let row = project_context_check(&listing, DoctorSurface::Repl);
    assert_eq!(row.status, DoctorStatus::Pass);
    assert_eq!(row.detail, "YOYO.md (3 lines), CLAUDE.md (7 lines)");

    let empty = (vec![], vec![]);
    let repl = project_context_check(&empty, DoctorSurface::Repl);
    assert_eq!(repl.status, DoctorStatus::Warn);
    assert_eq!(repl.detail, "no context file (create YOYO.md or run /init)");
    let shell = project_context_check(&empty, DoctorSurface::Shell);
    assert_eq!(
        shell.detail,
        "no context file (create YOYO.md or run yoyo init)"
    );
}

#[test]
fn command_form_names_the_verb_as_each_surface_types_it() {
    // #1086: one surface-aware prefix for every shared usage line.
    assert_eq!(command_form(DoctorSurface::Repl, "run"), "/run");
    assert_eq!(command_form(DoctorSurface::Shell, "run"), "yoyo run");
    assert_eq!(command_form(DoctorSurface::Shell, "tree"), "yoyo tree");
    assert!(!command_form(DoctorSurface::Shell, "run").contains('/'));
}

#[test]
fn run_usage_repl_is_byte_identical_and_shell_drops_the_bang_form() {
    use crate::commands_run::run_usage_line;
    // Near-miss: the REPL line is the pre-#1086 literal, byte for byte.
    assert_eq!(
        run_usage_line(DoctorSurface::Repl),
        "usage: /run <command>  or  !<command>"
    );
    // `!<command>` is REPL-only syntax; the shell has no such door.
    assert_eq!(
        run_usage_line(DoctorSurface::Shell),
        "usage: yoyo run <command>"
    );
}
