//! Shell-verb flag refusal (#1111): `yoyo find --bogus` used to search for
//! `--bogus` and exit 0, and `yoyo docs --bogus` asked docs.rs about a crate
//! called `--bogus`. For the verbs that take an operand (grep, find, def,
//! outline, memories, docs, changelog), a dash-led word before `--` that is not
//! one of that verb's own flags now exits 2 before the handler runs. `--` ends
//! flag parsing, so `yoyo find -- --weird-name` still searches for it.
//!
//! The decision is pure and lives here; `dispatch_sub.rs` only prints and exits.

use crate::commands_git_review::ReviewEffort;

/// The flags a shell verb knows, as (all of them, the value-taking subset).
/// grep's come from its parser's own list (`GREP_FLAGS`, pinned against
/// `parse_grep_args` by a test); `outline` has `--all`; find, def, memories,
/// docs and changelog have none, so any dash-led word there is a typo.
/// `review` (#1129) has its effort flags, `--pr`, and the four provider flags
/// `run_review_subcommand` reads from argv.
pub(crate) fn verb_own_flags(verb: &str) -> (&'static [&'static str], &'static [&'static str]) {
    use crate::commands_search::{GREP_FLAGS, GREP_VALUE_FLAGS};
    match verb {
        "grep" => (GREP_FLAGS, GREP_VALUE_FLAGS),
        // `/ast <pattern> [--lang <lang>] [--in <path>]`; both take a value.
        "ast" => (&["--lang", "--in"], &["--lang", "--in"]),
        "outline" => (&["--all"], &[]),
        "review" => (REVIEW_FLAGS, REVIEW_VALUE_FLAGS),
        _ => (&[], &[]),
    }
}

/// The refusal for an unknown dash-led word before `--` in `yoyo <verb> …`,
/// or None when every such word is one of the verb's own flags. Same wording
/// and did-you-mean as the top-level refusal (`cli_config::unknown_flags_among`).
pub(crate) fn verb_flag_refusal(args: &[String]) -> Option<String> {
    let verb = args.get(1)?;
    let mut rest = args.get(2..).unwrap_or_default().to_vec();
    if verb == "review" {
        rest = split_pr_spellings(&rest);
    }
    let flags = &rest[..rest.iter().position(|a| a == "--").unwrap_or(rest.len())];
    let (own, valued) = verb_own_flags(verb);
    let msg = crate::cli_config::unknown_flags_among(flags, own, valued)?;
    let verb = crate::cli::sanitize_for_display(verb);
    Some(format!("{msg} for `yoyo {verb}`. Nothing ran."))
}

/// What `yoyo <verb>` does with a dash-led word it does not know (#1132).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DashPolicy {
    /// [`verb_flag_refusal`] runs before the verb's handler: exit 2,
    /// "Nothing ran", did-you-mean from [`verb_own_flags`]. `--` escapes.
    Refuse,
    /// Refused with exit 2 before anything runs, by the verb's own parser
    /// or door; the reason names which one.
    OwnRefusal(&'static str),
    /// Handed on by design; the reason says to whom.
    Passthrough(&'static str),
}

use DashPolicy::{OwnRefusal, Passthrough, Refuse};

const GLOBAL: DashPolicy = OwnRefusal(
    "positional_after_verb refuses it (same message), but accepts yoyo's global flags (status --model m)",
);

/// One row per [`crate::cli::KNOWN_SUBCOMMANDS`] entry, in its order (#1132).
/// A test fails when the two lists differ in either direction, so a new verb
/// cannot ship without a decision here; the dispatch arms are pinned to
/// KNOWN_SUBCOMMANDS by `known_subcommands_names_every_dispatch_arm`.
/// `tests/unknown_flag_refusal.rs` reads the `Refuse` rows from this source.
pub(crate) const VERB_DASH_POLICY: &[(&str, DashPolicy)] = &[
    ("ast", Refuse),
    ("blame", Refuse),
    ("changelog", Refuse),
    (
        "commit",
        OwnRefusal("its own git-style flag list, with did-you-mean (#1107)"),
    ),
    (
        "config",
        OwnRefusal("an unknown subcommand needs a session: exit 2, nothing runs"),
    ),
    ("def", Refuse),
    (
        "diff",
        OwnRefusal("its own git-style flag list: usage line, exit 2 (#1107)"),
    ),
    ("docs", Refuse),
    ("doctor", GLOBAL),
    ("evolution", GLOBAL),
    (
        "extended",
        OwnRefusal("session-only: every form exits 2 and nothing runs"),
    ),
    ("find", Refuse),
    (
        "gasp",
        OwnRefusal("the first word must be one of its arms; anything else exits 2"),
    ),
    (
        "goal",
        OwnRefusal("the first word is a subcommand; an unknown one exits 2"),
    ),
    ("grep", Refuse),
    ("health", GLOBAL),
    ("help", GLOBAL),
    (
        "index",
        OwnRefusal("its argv parser takes no flags: usage, exit 2"),
    ),
    ("init", GLOBAL),
    (
        "lint",
        OwnRefusal("the first word is a subcommand; an unknown one exits 2"),
    ),
    (
        "map",
        OwnRefusal("its argv parser knows --all and --regex: usage, exit 2"),
    ),
    ("memories", Refuse),
    (
        "model",
        OwnRefusal("the first word is a subcommand; an unknown one exits 2"),
    ),
    ("outline", Refuse),
    ("permissions", GLOBAL),
    ("review", Refuse),
    (
        "risk",
        OwnRefusal("the first word is a subcommand (--all is one); unknown exits 2"),
    ),
    (
        "run",
        Passthrough("the words are a shell command line: `yoyo run ls -la`"),
    ),
    ("security", GLOBAL),
    (
        "setup",
        OwnRefusal("interactive wizard: refused without a terminal, nothing written"),
    ),
    (
        "skill",
        OwnRefusal("the first word is a subcommand; an unknown one exits 2"),
    ),
    ("status", GLOBAL),
    (
        "test",
        Passthrough(
            "extra words go verbatim to the project's test runner (#745): `yoyo test --release`",
        ),
    ),
    (
        "todo",
        OwnRefusal("the first word is a subcommand; an unknown one exits 2"),
    ),
    ("tree", OwnRefusal("takes only a depth: usage, exit 2")),
    (
        "undo",
        OwnRefusal("takes N, --all or --last-commit: usage, exit 2"),
    ),
    ("update", GLOBAL),
    ("version", GLOBAL),
    (
        "watch",
        OwnRefusal("session setting: every form but `status` exits 2, nothing runs"),
    ),
];

/// The verb's row in [`VERB_DASH_POLICY`], or None for a word that is not a verb.
pub(crate) fn dash_policy(verb: &str) -> Option<DashPolicy> {
    VERB_DASH_POLICY
        .iter()
        .find(|(v, _)| *v == verb)
        .map(|(_, p)| *p)
}

/// [`verb_flag_refusal`] for the verbs whose row is `Refuse`, else None.
/// `dispatch_sub` calls this once before its `match`, so the table, not each
/// arm, decides which verbs refuse (#1132).
pub(crate) fn policy_refusal(args: &[String]) -> Option<String> {
    let verb = args.get(1)?;
    (dash_policy(verb) == Some(Refuse))
        .then(|| verb_flag_refusal(args))
        .flatten()
}

/// `yoyo review`'s own flags (#1129). Before this, `review --quik` was read
/// as a file path and `review --print` as `--pr int`, because the review
/// parser matched `starts_with("--pr")`.
const REVIEW_FLAGS: &[&str] = &[
    "--quick",
    "--thorough",
    "--pr",
    "--provider",
    "--model",
    "--api-key",
    "--base-url",
];
/// The value-taking subset: the next word is `--pr`'s number or a provider
/// setting, never a review target.
const REVIEW_VALUE_FLAGS: &[&str] = &["--pr", "--provider", "--model", "--api-key", "--base-url"];
/// Provider flags `run_review_subcommand` reads itself; they and their values
/// are not part of the review target.
const REVIEW_PROVIDER_FLAGS: &[&str] = &["--provider", "--model", "--api-key", "--base-url"];

/// `--pr=N` and `--prN` (N all digits) as the two words `--pr N`, before
/// `--` only. Every other dash-led word is left alone, so `--print` stays
/// one word and is refused as unknown instead of becoming PR `int`.
fn split_pr_spellings(rest: &[String]) -> Vec<String> {
    let end = rest.iter().position(|a| a == "--").unwrap_or(rest.len());
    let mut out = Vec::with_capacity(rest.len() + 1);
    for (i, word) in rest.iter().enumerate() {
        let tail = word.strip_prefix("--pr").filter(|_| i < end);
        let value = match tail {
            Some(t) if t.starts_with('=') => t.strip_prefix('='),
            Some(t) if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) => Some(t),
            _ => None,
        };
        match value {
            Some(v) => out.extend(["--pr".to_string(), v.to_string()]),
            None => out.push(word.clone()),
        }
    }
    out
}

/// What `yoyo review <rest>` asks for, read from argv (#1129, #1130).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewArgs {
    /// `--quick` / `--thorough`, recognised only before `--`.
    pub(crate) effort: ReviewEffort,
    /// What to review: `--pr N`, a range or a path. Words are joined with
    /// one space and never re-split, so one argv element stays one path.
    pub(crate) target: String,
    /// True when the target came only from after `--`: it is an operand,
    /// so `yoyo review -- --pr5` names a file, not PR 5.
    pub(crate) flags_ended: bool,
}

/// Parse `yoyo review <rest>`: `--pr` spellings split into `--pr N`, the
/// provider flags and their values removed (`run_review_subcommand` reads
/// those from argv), the effort flags taken out, and the first `--` dropped
/// so `yoyo review -- --weird-file` reviews that file. #1130: this used to
/// return one joined string that `parse_review_effort` re-split on spaces,
/// so `review -- --quick` set the effort and reviewed the working tree.
pub(crate) fn review_args(rest: &[String]) -> ReviewArgs {
    let rest = split_pr_spellings(rest);
    let end = rest.iter().position(|a| a == "--").unwrap_or(rest.len());
    let mut effort = ReviewEffort::Normal;
    let mut kept = Vec::new();
    let mut skip_next = false;
    for word in &rest[..end] {
        if skip_next {
            skip_next = false;
        } else if REVIEW_PROVIDER_FLAGS.contains(&word.as_str()) {
            skip_next = true;
        } else if word == "--quick" {
            effort = ReviewEffort::Quick;
        } else if word == "--thorough" {
            effort = ReviewEffort::Thorough;
        } else {
            kept.push(word.as_str());
        }
    }
    let flags_ended = kept.is_empty() && end < rest.len();
    kept.extend(rest.iter().skip(end + 1).map(String::as_str));
    ReviewArgs {
        effort,
        target: kept.join(" "),
        flags_ended,
    }
}

/// `yoyo <verb> <rest>` split once into (the verb's own flags, each value
/// flag followed by its value; the operands), the way [`review_args`] reads
/// review (#1133). Flags count only before the first `--`; every word after
/// it is an operand, so a handler that takes this split never sees `--` and
/// cannot re-read an operand as a flag. Unknown dash-led words before `--`
/// are [`policy_refusal`]'s job; here they are left with the operands.
pub(crate) fn verb_split(verb: &str, rest: &[String]) -> (Vec<String>, Vec<String>) {
    let (own, takes_value) = verb_own_flags(verb);
    let end = rest.iter().position(|a| a == "--").unwrap_or(rest.len());
    let (mut flags, mut operands) = (Vec::new(), Vec::new());
    let mut words = rest[..end].iter();
    while let Some(word) = words.next() {
        if own.contains(&word.as_str()) {
            flags.push(word.clone());
            if takes_value.contains(&word.as_str()) {
                flags.extend(words.next().cloned());
            }
        } else {
            operands.push(word.clone());
        }
    }
    operands.extend(rest.iter().skip(end + 1).cloned());
    (flags, operands)
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

    /// #1129: the three inputs the issue reproduced, full message each.
    #[test]
    fn review_refuses_a_mistyped_flag_with_a_suggestion() {
        let rows: &[(&[&str], &str)] = &[
            (
                &["yoyo", "review", "--quik"],
                "unknown flag '--quik' (did you mean --quick?) for `yoyo review`. Nothing ran.",
            ),
            (
                &["yoyo", "review", "--thorogh", "src/x.rs"],
                "unknown flag '--thorogh' (did you mean --thorough?) for `yoyo review`. Nothing ran.",
            ),
            (
                &["yoyo", "review", "--print"],
                "unknown flag '--print' for `yoyo review`. Nothing ran.",
            ),
            (
                &["yoyo", "review", "--pretty"],
                "unknown flag '--pretty' for `yoyo review`. Nothing ran.",
            ),
            // Not digits after `--pr`, so not a PR spelling.
            (
                &["yoyo", "review", "--prx"],
                "unknown flag '--prx' (did you mean --pr?) for `yoyo review`. Nothing ran.",
            ),
        ];
        for (row, want) in rows {
            assert_eq!(
                verb_flag_refusal(&argv(row)).as_deref(),
                Some(*want),
                "{row:?}"
            );
        }
    }

    /// #1129 near-misses: every spelling review accepted before still runs,
    /// and each lands on the same effort and target the old join + re-split
    /// produced (except the provider flags, which used to become part of a
    /// path, and the #1130 rows, where the old output was the bug).
    #[test]
    fn review_near_misses_keep_their_target() {
        use ReviewEffort::{Normal, Quick, Thorough};
        let rows: &[(&[&str], ReviewEffort, &str, bool)] = &[
            (&[], Normal, "", false),
            (&["--quick"], Quick, "", false),
            (&["--thorough", "src/x.rs"], Thorough, "src/x.rs", false),
            (&["src/x.rs", "--quick"], Quick, "src/x.rs", false),
            (&["src/x.rs"], Normal, "src/x.rs", false),
            (&["HEAD~3..HEAD"], Normal, "HEAD~3..HEAD", false),
            (&["--pr"], Normal, "--pr", false),
            (&["--pr", "123"], Normal, "--pr 123", false),
            (&["--pr123"], Normal, "--pr 123", false),
            (&["--pr=123"], Normal, "--pr 123", false),
            (&["--pr", "abc"], Normal, "--pr abc", false),
            (&["--quick", "--pr", "7"], Quick, "--pr 7", false),
            (&["--", "--weird-file"], Normal, "--weird-file", true),
            (&["--model", "m", "src/x.rs"], Normal, "src/x.rs", false),
            (
                &["--provider", "openai", "--api-key", "k", "--base-url", "u"],
                Normal,
                "",
                false,
            ),
            // Empty targets: old output was "" (the staged/unstaged review),
            // and the content builder still treats blank as "no target".
            (&[""], Normal, "", false),
            (&["--", ""], Normal, "", true),
            (&["--"], Normal, "", true),
            (&["--", " a.rs"], Normal, " a.rs", true),
        ];
        for (rest, effort, target, flags_ended) in rows {
            let mut args = vec!["yoyo", "review"];
            args.extend_from_slice(rest);
            assert_eq!(verb_flag_refusal(&argv(&args)), None, "{rest:?}");
            let want = ReviewArgs {
                effort: *effort,
                target: target.to_string(),
                flags_ended: *flags_ended,
            };
            assert_eq!(review_args(&argv(rest)), want, "{rest:?}");
        }
    }

    /// #1130: `--` ends flags, so a word after it is the target verbatim
    /// even when it spells an effort or PR flag, and one argv element with
    /// spaces stays one path. The old join + re-split read `-- --quick` as
    /// the effort flag (and reviewed the working tree), `-- --pr5` as PR 5,
    /// and squeezed `a  b.rs` to `a b.rs`.
    #[test]
    fn review_words_after_end_of_flags_are_the_target() {
        use ReviewEffort::{Normal, Quick};
        let rows: &[(&[&str], ReviewEffort, &str, bool)] = &[
            (&["--", "--quick"], Normal, "--quick", true),
            (&["--", "--thorough"], Normal, "--thorough", true),
            (&["--", "--pr5"], Normal, "--pr5", true),
            (&["--", "--pr", "5"], Normal, "--pr 5", true),
            (&["--", "--pr123"], Normal, "--pr123", true),
            (&["--", "a  b.rs"], Normal, "a  b.rs", true),
            (&["a  b.rs"], Normal, "a  b.rs", false),
            (&["--quick", "--", "--thorough"], Quick, "--thorough", true),
            (
                &["src/x.rs", "--", "--quick"],
                Normal,
                "src/x.rs --quick",
                false,
            ),
        ];
        for (rest, effort, target, flags_ended) in rows {
            let want = ReviewArgs {
                effort: *effort,
                target: target.to_string(),
                flags_ended: *flags_ended,
            };
            assert_eq!(review_args(&argv(rest)), want, "{rest:?}");
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

    /// #1132's forcing function: every verb `yoyo` dispatches has exactly one
    /// policy row and every row is a verb, so a new verb fails here until
    /// someone decides what it does with `--bogus`.
    #[test]
    fn every_known_subcommand_has_exactly_one_dash_policy_row() {
        let verbs = crate::cli::KNOWN_SUBCOMMANDS;
        for verb in verbs {
            let rows = VERB_DASH_POLICY.iter().filter(|(v, _)| v == verb).count();
            assert_eq!(
                rows, 1,
                "KNOWN_SUBCOMMANDS has {verb:?}; VERB_DASH_POLICY rows: {rows}"
            );
        }
        for (verb, _) in VERB_DASH_POLICY {
            assert!(
                verbs.contains(verb),
                "policy row {verb:?} is not a subcommand"
            );
        }
        assert_eq!(VERB_DASH_POLICY.len(), verbs.len());
    }

    /// The three verbs #1132 routed, plus their near-misses: ast's own
    /// flags and values, `--` escaping, and `test`/`run` passing on.
    #[test]
    fn ast_and_blame_refuse_and_test_passes_on() {
        for (row, want) in [
            (
                &["yoyo", "ast", "--bogus"][..],
                "unknown flag '--bogus' for `yoyo ast`. Nothing ran.",
            ),
            (
                &["yoyo", "ast", "--lnag", "rust", "f($A)"],
                "unknown flag '--lnag' (did you mean --lang?) for `yoyo ast`. Nothing ran.",
            ),
            (
                &["yoyo", "blame", "--bogus"],
                "unknown flag '--bogus' for `yoyo blame`. Nothing ran.",
            ),
        ] {
            assert_eq!(policy_refusal(&argv(row)).as_deref(), Some(want), "{row:?}");
        }
        for row in [
            &["yoyo", "ast", "f($A)", "--lang", "rust", "--in", "src/"][..],
            &["yoyo", "ast", "--in", "-dir", "f($A)"],
            &["yoyo", "ast", "-- f($A)"],
            &["yoyo", "ast", "--", "-$X"],
            &["yoyo", "blame", "src/a.rs:1-5"],
            &["yoyo", "blame", "--", "--weird"],
            &["yoyo", "test", "--release"],
            &["yoyo", "run", "ls", "-la"],
            // Global-flag verbs keep their own door, which accepts --model.
            &["yoyo", "status", "--no-color"],
            &["yoyo", "nonverb", "--bogus"],
        ] {
            assert_eq!(policy_refusal(&argv(row)), None, "{row:?}");
        }
    }

    /// #1133: every grep flag placed after `--` is an operand (the pattern),
    /// iterated from GREP_FLAGS, never typed; before `--` it is a flag and a
    /// value flag takes its value with it.
    #[test]
    fn verb_split_counts_flags_only_before_double_dash() {
        use crate::commands_search::{parse_grep_tokens, GREP_FLAGS, GREP_VALUE_FLAGS};
        for flag in GREP_FLAGS {
            let (flags, ops) = verb_split("grep", &argv(&["--", flag, "y.txt"]));
            assert!(flags.is_empty(), "{flag}");
            assert_eq!(ops, argv(&[flag, "y.txt"]), "{flag}");
            let parsed = parse_grep_tokens([flags, argv(&["--"]), ops].concat()).unwrap();
            assert_eq!(parsed.pattern, *flag, "{flag}");
            assert_eq!(parsed.paths, ["y.txt"], "{flag}");

            let before = if GREP_VALUE_FLAGS.contains(flag) {
                argv(&[flag, "1"])
            } else {
                argv(&[flag])
            };
            let rest = [before.clone(), argv(&["pat", "src"])].concat();
            assert_eq!(verb_split("grep", &rest), (before, argv(&["pat", "src"])));
        }
        assert_eq!(
            verb_split("outline", &argv(&["--", "--all", "x.rs"])),
            (vec![], argv(&["--all", "x.rs"]))
        );
        assert_eq!(
            verb_split("outline", &argv(&["foo", "--all"])),
            (argv(&["--all"]), argv(&["foo"]))
        );
        // Only the FIRST `--` ends flags; a second one is an operand.
        assert_eq!(
            verb_split("outline", &argv(&["--", "--"])),
            (vec![], argv(&["--"]))
        );
        assert_eq!(verb_split("outline", &[]), (vec![], vec![]));
    }

    /// Near-miss: the REPL door still parses its text exactly as before, and
    /// the shell door's split reaches the same GrepArgs for input without `--`.
    #[test]
    fn repl_grep_parse_agrees_with_the_shell_split_without_double_dash() {
        use crate::commands_search::{parse_grep_args, parse_grep_tokens};
        for line in [
            "-c foo",
            "foo -c src",
            "-C 2 --include *.rs foo",
            "-s Foo a b",
        ] {
            let words: Vec<String> = line.split(' ').map(String::from).collect();
            let (mut flags, ops) = verb_split("grep", &words);
            flags.push("--".into());
            assert_eq!(
                parse_grep_tokens([flags, ops].concat()),
                parse_grep_args(&format!("/grep {line}")),
                "{line}"
            );
        }
        // REPL `/grep -- --count foo` keeps its own `--` handling (#706).
        let repl = parse_grep_args("/grep -- --count foo").unwrap();
        assert_eq!((repl.pattern.as_str(), repl.count_only), ("--count", false));
    }
}
