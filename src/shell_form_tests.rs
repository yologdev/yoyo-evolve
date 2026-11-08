//! Part of #1127: the usage and hint lines `docs`, `risk` and `memories`
//! print name a form the reader can type where they are reading. The REPL
//! keeps `/<verb>`; the shell prints `yoyo <verb>`, and for `/remember` and
//! `/forget`, which have no shell verb, it says they are REPL commands
//! rather than inventing a `yoyo remember` that would fail.

use crate::commands_dev::DoctorSurface::{Repl, Shell};
use crate::commands_memory::{add_memory_hint, forget_memory_hint};
use crate::commands_project::docs_usage;
use crate::commands_risk::unknown_risk_subcommand_line;
use crate::format::{DIM, RESET};

#[test]
fn docs_usage_names_yoyo_docs_on_the_shell_and_slash_docs_in_the_repl() {
    let shell = docs_usage(Shell, true);
    assert!(shell.contains("usage: yoyo docs <crate> [item]"), "{shell}");
    assert!(shell.contains("Examples: yoyo docs serde, yoyo docs tokio task"));
    assert!(!shell.contains("/docs"), "shell usage leaks /docs: {shell}");
    assert!(docs_usage(Shell, false).contains("usage: yoyo docs <crate> [item]"));
    assert!(!docs_usage(Shell, false).contains("/docs"));

    // The REPL reads exactly what it printed before this change.
    assert_eq!(
        docs_usage(Repl, true),
        format!(
            "{DIM}  usage: /docs <crate> [item]\n  Look up docs.rs documentation for a Rust crate.\n  Examples: /docs serde, /docs tokio task{RESET}\n"
        )
    );
    assert_eq!(
        docs_usage(Repl, false),
        format!("{DIM}  usage: /docs <crate> [item]{RESET}\n")
    );
}

#[test]
fn unknown_risk_subcommand_names_yoyo_risk_on_the_shell() {
    assert_eq!(
        unknown_risk_subcommand_line(Shell, "zz1"),
        "Unknown yoyo risk subcommand: zz1"
    );
    assert_eq!(
        unknown_risk_subcommand_line(Repl, "zz1"),
        "Unknown /risk subcommand: zz1"
    );
}

#[test]
fn memory_hints_never_print_a_nonexistent_shell_verb() {
    // There is no `yoyo remember` / `yoyo forget` (KNOWN_SUBCOMMANDS), so the
    // shell hint must not invent one, and must not read as if `/remember`
    // could be typed at the shell prompt.
    for verb in ["remember", "forget"] {
        assert!(
            !crate::cli::KNOWN_SUBCOMMANDS.contains(&verb),
            "`yoyo {verb}` now exists: the shell hint should name it"
        );
    }
    let add = add_memory_hint(Shell);
    let forget = forget_memory_hint(Shell);
    for hint in [add, forget] {
        assert!(!hint.contains("yoyo remember"), "{hint}");
        assert!(!hint.contains("yoyo forget"), "{hint}");
        assert!(hint.contains("REPL"), "shell hint must say where: {hint}");
    }
    assert!(add.contains("/remember <note>"));
    assert!(forget.contains("/forget <n>"));

    assert_eq!(add_memory_hint(Repl), "Use /remember <note> to add one.");
    assert_eq!(
        forget_memory_hint(Repl),
        "Use /forget <n> to remove a memory."
    );
}

#[test]
fn map_argument_errors_name_yoyo_map_on_the_shell_and_slash_map_in_the_repl() {
    use crate::commands_map::{parse_map_args, parse_map_argv, MapArgs};
    let err = |parsed: MapArgs| match parsed {
        MapArgs::Error(msg) => msg,
        MapArgs::Ok { .. } => panic!("expected an argument error"),
    };
    let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    let two = err(parse_map_argv(&argv(&["a", "b"])));
    assert_eq!(
        two,
        "two paths given ('a' and 'b') — yoyo map takes at most one path\n  usage: yoyo map [--all] [--regex] [path]"
    );
    let flag = err(parse_map_argv(&argv(&["--al"])));
    assert!(
        flag.contains("usage: yoyo map [--all] [--regex] [path]"),
        "{flag}"
    );
    for msg in [&two, &flag] {
        assert!(!msg.contains("/map"), "shell text leaks /map: {msg}");
    }

    // The REPL reads exactly what it printed before this change.
    assert_eq!(
        err(parse_map_args("a b")),
        "two paths given ('a' and 'b') — /map takes at most one path\n  usage: /map [--all] [--regex] [path]"
    );
    assert!(err(parse_map_args("--al")).ends_with("\n  usage: /map [--all] [--regex] [path]"));
}

#[test]
fn skill_try_line_offers_only_real_yoyo_skill_forms_on_the_shell() {
    use crate::commands_skill::{skill_try_line, SKILL_SUBCOMMANDS};
    let shell = skill_try_line(Shell);
    assert!(
        !shell.contains("/skill"),
        "shell text leaks /skill: {shell}"
    );
    // Every alternative is `yoyo skill <sub>` for a sub the handler accepts,
    // so none of them is an invented shell verb.
    let alternatives: Vec<&str> = shell.trim_start_matches("try: ").split(", ").collect();
    assert_eq!(alternatives.len(), SKILL_SUBCOMMANDS.len(), "{shell}");
    for alt in alternatives {
        let rest = alt
            .strip_prefix("yoyo skill ")
            .unwrap_or_else(|| panic!("not a shell form: {alt}"));
        let sub = rest.split_whitespace().next().unwrap_or("");
        assert!(SKILL_SUBCOMMANDS.contains(&sub), "{alt}");
    }
    assert!(crate::cli::KNOWN_SUBCOMMANDS.contains(&"skill"));

    assert_eq!(
        skill_try_line(Repl),
        "try: /skill list, /skill show <name>, /skill path, /skill install <path>, /skill search <query>, /skill init <name>"
    );
}

#[test]
fn skill_search_install_hint_names_the_door_it_is_printed_on() {
    // #1127: the last line of `yoyo skill search` said `/skill install ...`.
    // Shell door: handle_skill_status -> handle_skill_on(Shell) passes
    // command_form(Shell, "skill") into handle_skill_search, which prints
    // this line; the search itself needs gh + network, so the formatter is
    // pinned here and the shell wiring by the `skill install <file>` binary
    // test in tests/integration.rs, which goes through the same `skill` value.
    use crate::commands_dev::command_form;
    use crate::commands_skill::skill_search_install_hint;
    assert_eq!(
        skill_search_install_hint(&command_form(Shell, "skill")),
        "install with: yoyo skill install gh:<owner>/<repo>"
    );
    assert_eq!(
        skill_search_install_hint(&command_form(Repl, "skill")),
        "install with: /skill install gh:<owner>/<repo>"
    );
}
