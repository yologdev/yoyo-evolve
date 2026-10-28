//! The shell door onto `/watch` (#1088).
//!
//! Watch mode lives in an in-process global (`watch::set_watch_commands`),
//! read after agent edits in the same process. `yoyo watch <cmd>` on the
//! shell used to set that global, print "Watch mode ON — will run …" and exit
//! 0, and then the process ended: no agent edit can ever happen there and
//! nothing is persisted. Nobody who types it chose a no-op; they believe it
//! configures watch for later. So every form that claims a state change
//! (bare, `all`, `lint`, a command, `off`) is refused with exit 2, the house
//! code `yoyo extended` and `yoyo setup` use without a session, and the
//! message names the two real levers: `/watch …` inside a session, and
//! `auto_watch = true` in `.yoyo.toml`, which arms the *detected* command
//! (`watch::arm_auto_watch`) and takes no command of its own.
//!
//! `yoyo watch status` keeps exit 0, because it can say something true: no
//! session is running, so nothing is on here, plus what `auto_watch` in the
//! loaded config would do in a session started in this directory.
//!
//! The REPL `/watch` path (`watch::handle_watch`) is untouched.

use crate::format::{DIM, RESET, YELLOW};

/// Exit status of every refused shell `watch` form: the same `2` as
/// `yoyo extended` (`setup::SETUP_NOT_INTERACTIVE_EXIT`).
pub(crate) const WATCH_SHELL_REFUSAL_EXIT: i32 = crate::setup::SETUP_NOT_INTERACTIVE_EXIT;

/// The refusal for a shell `watch` form that would change a session setting.
/// `session_input` is the exact `/watch …` line the user would type in a
/// session (built by the dispatcher with the #1009 quoting rule);
/// `detected` is what `auto_watch = true` would arm in this directory.
pub(crate) fn watch_shell_refusal(session_input: &str, detected: Option<&str>) -> String {
    let arg = session_input
        .strip_prefix("/watch")
        .unwrap_or(session_input)
        .trim();
    let mut out = format!(
        "{YELLOW}  watch is a session setting: this process exits now, so nothing would ever run after agent edits.{RESET}\n\
         {DIM}  In a session: start yoyo and use: {session_input}\n"
    );
    if arg == "off" {
        out.push_str(
            "  Watch is never on outside a session. To keep it off when a session starts, leave auto_watch unset or false in .yoyo.toml.",
        );
    } else {
        match detected {
            Some(cmd) => out.push_str(&format!(
                "  On every start: set auto_watch = true in .yoyo.toml. It arms the detected command (`{cmd}`); it does not take a command of its own."
            )),
            None => out.push_str(
                "  On every start: auto_watch = true in .yoyo.toml arms a detected lint/test command, and none is detected here.",
            ),
        }
    }
    out.push_str(&format!("{RESET}\n"));
    out
}

/// The true report shell `watch status` can give: nothing is on in this
/// process, and what `auto_watch` would do in a session started here,
/// mirroring `watch::arm_auto_watch`'s own branches (no command detected,
/// detected but its tool missing from PATH, armed).
pub(crate) fn watch_shell_status_report(
    auto_watch: bool,
    detected: Option<&str>,
    missing_tool: Option<&str>,
) -> String {
    let second = if !auto_watch {
        "auto_watch is off: a session started here begins with watch off (turn it on there with /watch).".to_string()
    } else {
        match (detected, missing_tool) {
            (None, _) => "auto_watch = true, but no lint/test command is detected here: a session started here begins with watch off.".to_string(),
            (Some(cmd), Some(tool)) => format!(
                "auto_watch = true, but `{tool}` is not on PATH: a session started here would not arm `{cmd}`."
            ),
            (Some(cmd), None) => {
                format!("auto_watch = true: a session started here arms `{cmd}`.")
            }
        }
    };
    format!(
        "{DIM}  Watch is a session setting: no session is running, so nothing is on in this process.\n  {second}{RESET}\n"
    )
}

/// Shell `yoyo watch …`: `status` reports (exit 0), every other form is
/// refused on stderr with [`WATCH_SHELL_REFUSAL_EXIT`] and sets nothing.
pub(crate) fn watch_shell_status(session_input: &str) -> i32 {
    let arg = session_input
        .strip_prefix("/watch")
        .unwrap_or(session_input)
        .trim();
    let detected = crate::watch::auto_detect_watch_command();
    if arg == "status" {
        let (config, _) = crate::config::load_config_file();
        let auto_watch = crate::config::parse_auto_watch_from_config(&config);
        let missing = detected
            .as_deref()
            .and_then(crate::watch::missing_watch_tool);
        print!(
            "{}",
            watch_shell_status_report(auto_watch, detected.as_deref(), missing.as_deref())
        );
        return 0;
    }
    eprint!(
        "{}",
        watch_shell_refusal(session_input, detected.as_deref())
    );
    WATCH_SHELL_REFUSAL_EXIT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn refusal_names_the_session_line_and_the_detected_auto_watch_command() {
        let out = strip(&watch_shell_refusal(
            "/watch echo a b",
            Some("cargo clippy && cargo test"),
        ));
        assert_eq!(
            out,
            "  watch is a session setting: this process exits now, so nothing would ever run after agent edits.\n\
             \x20 In a session: start yoyo and use: /watch echo a b\n\
             \x20 On every start: set auto_watch = true in .yoyo.toml. It arms the detected command (`cargo clippy && cargo test`); it does not take a command of its own.\n"
        );
        assert!(!out.contains("Watch mode ON"));
    }

    #[test]
    fn refusal_without_detection_does_not_promise_auto_watch_will_arm() {
        let out = strip(&watch_shell_refusal("/watch", None));
        assert!(out.contains("use: /watch\n"), "{out:?}");
        assert!(out.contains("none is detected here"), "{out:?}");
        assert!(!out.contains("set auto_watch = true"), "{out:?}");
    }

    #[test]
    fn refusal_for_off_points_at_keeping_it_off_not_at_arming() {
        let out = strip(&watch_shell_refusal("/watch off", Some("cargo test")));
        assert!(out.contains("use: /watch off\n"), "{out:?}");
        assert!(out.contains("never on outside a session"), "{out:?}");
        assert!(!out.contains("auto_watch = true"), "{out:?}");
        assert!(!out.contains("Watch mode OFF"), "{out:?}");
    }

    #[test]
    fn status_report_mirrors_each_arm_auto_watch_branch() {
        let first = "  Watch is a session setting: no session is running, so nothing is on in this process.\n";
        let rows: [(bool, Option<&str>, Option<&str>, &str); 5] = [
            (false, Some("cargo test"), None, "auto_watch is off: a session started here begins with watch off (turn it on there with /watch)."),
            (false, None, None, "auto_watch is off: a session started here begins with watch off (turn it on there with /watch)."),
            (true, None, None, "auto_watch = true, but no lint/test command is detected here: a session started here begins with watch off."),
            (true, Some("npm test"), Some("npm"), "auto_watch = true, but `npm` is not on PATH: a session started here would not arm `npm test`."),
            (true, Some("cargo test"), None, "auto_watch = true: a session started here arms `cargo test`."),
        ];
        for (aw, det, miss, second) in rows {
            assert_eq!(
                strip(&watch_shell_status_report(aw, det, miss)),
                format!("{first}  {second}\n"),
                "row {aw} {det:?} {miss:?}"
            );
        }
    }

    #[test]
    fn refusal_exit_is_the_house_not_interactive_code() {
        assert_eq!(WATCH_SHELL_REFUSAL_EXIT, 2);
    }
}
