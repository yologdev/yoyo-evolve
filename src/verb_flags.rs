//! Shell-verb flag refusal (#1111): `yoyo find --bogus` used to search for
//! `--bogus` and exit 0, and `yoyo docs --bogus` asked docs.rs about a crate
//! called `--bogus`. For the verbs that take an operand (grep, find, def,
//! outline, memories, docs, changelog), a dash-led word before `--` that is not
//! one of that verb's own flags now exits 2 before the handler runs. `--` ends
//! flag parsing, so `yoyo find -- --weird-name` still searches for it.
//!
//! The decision is pure and lives here; `dispatch_sub.rs` only prints and exits.

/// The flags a shell verb knows, as (all of them, the value-taking subset).
/// grep's come from its parser's own list (`GREP_FLAGS`, pinned against
/// `parse_grep_args` by a test); `outline` has `--all`; find, def, memories,
/// docs and changelog have none, so any dash-led word there is a typo.
pub(crate) fn verb_own_flags(verb: &str) -> (&'static [&'static str], &'static [&'static str]) {
    use crate::commands_search::{GREP_FLAGS, GREP_VALUE_FLAGS};
    match verb {
        "grep" => (GREP_FLAGS, GREP_VALUE_FLAGS),
        "outline" => (&["--all"], &[]),
        _ => (&[], &[]),
    }
}

/// The refusal for an unknown dash-led word before `--` in `yoyo <verb> …`,
/// or None when every such word is one of the verb's own flags. Same wording
/// and did-you-mean as the top-level refusal (`cli_config::unknown_flags_among`).
pub(crate) fn verb_flag_refusal(args: &[String]) -> Option<String> {
    let verb = args.get(1)?;
    let rest = args.get(2..).unwrap_or_default();
    let flags = &rest[..rest.iter().position(|a| a == "--").unwrap_or(rest.len())];
    let (own, valued) = verb_own_flags(verb);
    let msg = crate::cli_config::unknown_flags_among(flags, own, valued)?;
    let verb = crate::cli::sanitize_for_display(verb);
    Some(format!("{msg} for `yoyo {verb}`. Nothing ran."))
}

/// `rest` without its first `--`: the end-of-flags marker is not an operand.
pub(crate) fn drop_end_of_flags(rest: &[String]) -> Vec<String> {
    let mut out = rest.to_vec();
    if let Some(i) = out.iter().position(|a| a == "--") {
        out.remove(i);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    /// One refused row per verb #1111 named, each with the full message.
    #[test]
    fn each_operand_verb_refuses_an_unknown_flag() {
        for verb in [
            "changelog",
            "memories",
            "docs",
            "def",
            "find",
            "outline",
            "grep",
        ] {
            assert_eq!(
                verb_flag_refusal(&argv(&["yoyo", verb, "--bogus"])).as_deref(),
                Some(format!("unknown flag '--bogus' for `yoyo {verb}`. Nothing ran.").as_str()),
                "{verb}"
            );
        }
    }

    /// The did-you-mean comes from the verb's own list, not the global one.
    #[test]
    fn did_you_mean_uses_the_verbs_own_flags() {
        assert_eq!(
            verb_flag_refusal(&argv(&["yoyo", "grep", "--cout", "TODO"])).as_deref(),
            Some("unknown flag '--cout' (did you mean --count?) for `yoyo grep`. Nothing ran.")
        );
        assert_eq!(
            verb_flag_refusal(&argv(&["yoyo", "outline", "--al", "x"])).as_deref(),
            Some("unknown flag '--al' (did you mean --all?) for `yoyo outline`. Nothing ran.")
        );
        // A global flag is not one of find's, so it refuses too (it was
        // searched for as a pattern before).
        assert!(verb_flag_refusal(&argv(&["yoyo", "find", "--model", "x"])).is_some());
        // `-5` looks like a count but is a flag-shaped typo for changelog.
        assert!(verb_flag_refusal(&argv(&["yoyo", "changelog", "-5"])).is_some());
    }

    /// Near-misses that must still run: own flags, a value flag's value,
    /// anything after `--`, plain operands, bare verbs.
    #[test]
    fn near_misses_still_run() {
        let pass: &[&[&str]] = &[
            &["yoyo", "outline", "--all", "x"],
            &["yoyo", "outline", "x", "--all"],
            &["yoyo", "grep", "-c", "TODO"],
            &["yoyo", "grep", "-s", "-C", "2", "--include", "*.rs", "TODO"],
            &["yoyo", "grep", "--exclude", "-weird-glob", "TODO"],
            &["yoyo", "grep", "--", "--bogus"],
            &["yoyo", "find", "--", "--bogus"],
            &["yoyo", "def", "--", "-x"],
            &["yoyo", "changelog", "5"],
            &["yoyo", "changelog"],
            &["yoyo", "memories", "foo"],
            &["yoyo", "memories"],
            &["yoyo", "docs", "serde"],
            &["yoyo", "docs", "tokio", "task"],
            &["yoyo", "find", "-"],
            &["yoyo", "grep", "-x y"],
        ];
        for row in pass {
            assert_eq!(verb_flag_refusal(&argv(row)), None, "{row:?}");
        }
    }

    #[test]
    fn end_of_flags_marker_is_dropped_once() {
        assert_eq!(
            drop_end_of_flags(&argv(&["--", "--bogus"])),
            argv(&["--bogus"])
        );
        assert_eq!(
            drop_end_of_flags(&argv(&["a", "--", "--"])),
            argv(&["a", "--"])
        );
        assert_eq!(drop_end_of_flags(&argv(&["serde"])), argv(&["serde"]));
        assert!(drop_end_of_flags(&[]).is_empty());
    }
}
