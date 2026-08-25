//! The bare-word near-miss guard for `yoyo <word>` — a Day-170 **pure move** out of
//! `src/dispatch_sub.rs`, made to get that file back under the module-size cap.
//!
//! Nothing here changed: the table, the two pure functions, their helpers and every
//! test came over verbatim. `dispatch_sub` re-exports the public items, so all call
//! sites (`main.rs`, and the tests that reference them) are byte-unchanged.

use crate::format::{DIM, RED, RESET, YELLOW};

/// Every verb `try_dispatch_subcommand` routes as `yoyo <verb>`.
///
/// This is the candidate list for the bare-word near-miss guard below. It is a
/// second copy of the match arms above, so `test_routed_subcommands_matches_the_dispatcher`
/// reads this file's source and asserts the two agree in BOTH directions — a verb
/// added to the `match` without being added here would make the guard fire on a verb
/// that actually works, and a verb removed from the `match` but left here would make
/// the guard stay silent on a word that is now a paid prompt.
pub const ROUTED_SUBCOMMANDS: &[&str] = &[
    "ast",
    "blame",
    "changelog",
    "commit",
    "config",
    "def",
    "diff",
    "docs",
    "doctor",
    "evolution",
    "extended",
    "find",
    "gasp",
    "goal",
    "grep",
    "health",
    "help",
    "index",
    "init",
    "lint",
    "map",
    "memories",
    "outline",
    "permissions",
    "review",
    "risk",
    "run",
    "security",
    "setup",
    "skill",
    "status",
    "test",
    "todo",
    "tree",
    "undo",
    "update",
    "version",
    "watch",
];

/// Command names that exist only inside the REPL (`/tokens`, `/cost`, …) — every
/// `commands::KNOWN_COMMANDS` entry, minus its leading `/`, that `try_dispatch_subcommand`
/// does **not** route. Derived, never hand-listed, so a new slash command is covered
/// the day it is added to `KNOWN_COMMANDS`.
fn repl_only_commands() -> Vec<&'static str> {
    crate::commands::KNOWN_COMMANDS
        .iter()
        .filter_map(|c| c.strip_prefix('/'))
        .filter(|name| !name.is_empty() && !ROUTED_SUBCOMMANDS.contains(name))
        .collect()
}

/// The edit-distance budget `commands::suggest_command` already applies to a mistyped
/// slash command: ≤2 for short words, ≤3 for longer ones. Mirrored here (over chars,
/// not bytes) so the bare-word path and the REPL path forgive typos identically.
fn near_miss_threshold(word: &str) -> usize {
    if word.chars().count() <= 5 {
        2
    } else {
        3
    }
}

/// Extract the single bare word from `args` when — and only when — the invocation is
/// the unambiguous shape the guard is allowed to speak about: `yoyo <word>` with
/// nothing else. `args[0]` is the binary path.
///
/// Deliberately narrow. `yoyo do the thing`, `yoyo -p "tokens"`, and
/// `yoyo something --flag` all return `None` and keep today's behavior byte-for-byte.
pub fn bare_word_arg(args: &[String]) -> Option<&str> {
    if args.len() != 2 {
        return None;
    }
    let word = args[1].as_str();
    if word.starts_with('-') || word.is_empty() || word.split_whitespace().count() != 1 {
        return None;
    }
    Some(word)
}

/// Decide whether a single bare CLI word is a near miss for a command yoyo knows,
/// and if so return the message to print. Pure: no I/O, no exit, no side effects.
///
/// Returns `None` — meaning "keep today's behavior, send it to the model" — for:
/// - anything `try_dispatch_subcommand` already routes (the guard must never
///   intercept a word that works),
/// - anything containing whitespace (a real prompt),
/// - a word that resembles no known command (`zzqqx` is a legitimate prompt).
pub fn bare_word_near_miss(arg: &str) -> Option<String> {
    if arg.is_empty() || arg.split_whitespace().count() != 1 {
        return None;
    }
    if ROUTED_SUBCOMMANDS.contains(&arg) {
        return None;
    }

    let repl_only = repl_only_commands();

    // Exact name of a REPL-only command: `yoyo tokens`. Not a typo — a real command
    // typed at the wrong entry point, so say where it lives instead of guessing.
    if repl_only.contains(&arg) {
        return Some(format!(
            "{RED}✗ unknown command: {arg}{RESET}\n\
             {YELLOW}  /{arg} is a REPL command — start yoyo and use: /{arg}{RESET}\n\
             {DIM}  to send this as a prompt: yoyo -p \"{arg}\"{RESET}"
        ));
    }

    // Typo: pick the closest known name across both families, then report which
    // family it landed in, because the fix differs (`yoyo status` vs `/tokens`).
    let mut candidates: Vec<&str> = ROUTED_SUBCOMMANDS.to_vec();
    candidates.extend_from_slice(&repl_only);
    let best = crate::commands::closest_match(arg, &candidates, near_miss_threshold(arg))?;

    let did_you_mean = if ROUTED_SUBCOMMANDS.contains(&best) {
        format!("did you mean: yoyo {best}")
    } else {
        format!("did you mean: /{best}   (a REPL command — start yoyo and use it there)")
    };

    Some(format!(
        "{RED}✗ unknown command: {arg}{RESET}\n\
         {YELLOW}  {did_you_mean}{RESET}\n\
         {DIM}  to send this as a prompt: yoyo -p \"{arg}\"{RESET}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `#[cfg(test)] mod tests` marker, built at runtime so this needle can
    /// never match itself when the source-reading test below scans this file.
    fn test_module_marker() -> String {
        format!("#[cfg{}]\nmod tests", "(test)")
    }

    /// Parse the verbs `try_dispatch_subcommand` actually routes out of this
    /// file's own source: the top-level arms of `match sub.as_str()`, from that
    /// line down to the closing `_ => {}` catch-all.
    ///
    /// What this is and is not: it is a *shape* parse of match-arm lines, not a
    /// Rust parse. It collects every quoted literal on a line that reaches
    /// `=> {`, so `"a" | "b" => {` yields both; nested arms inside a handler
    /// (`Some("get") => {`) do not start with a quote and are skipped. If the
    /// dispatcher ever grows a top-level arm written in some other shape, this
    /// under-reports rather than over-reports — which fails the "every routed
    /// verb is in the table" direction only if the table also lacks it.
    fn dispatcher_routed_verbs() -> Vec<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch_sub.rs");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        // Cut at this module's own tests: the table is quoted verbatim below,
        // and reading it back would make the guard vacuously green.
        let non_test = src
            .split(test_module_marker().as_str())
            .next()
            .expect("split always yields at least one piece");

        let mut lines = non_test.lines();
        let mut verbs = Vec::new();
        let found = lines.any(|l| l.trim() == "match sub.as_str() {");
        assert!(
            found,
            "the dispatcher's `match sub.as_str() {{` line moved — this drift guard \
             is reading nothing and would pass vacuously"
        );
        for line in lines {
            let trimmed = line.trim();
            if trimmed == "_ => {}" {
                break;
            }
            if !trimmed.starts_with('"') || !trimmed.contains("=> {") {
                continue;
            }
            // Everything before `=>` is the pattern; take each quoted literal.
            let pattern = trimmed.split("=>").next().unwrap_or("");
            for (i, piece) in pattern.split('"').enumerate() {
                if i % 2 == 1 {
                    verbs.push(piece.to_string());
                }
            }
        }
        assert!(
            verbs.len() > 20,
            "parsed only {} dispatcher arms — the parse shape broke, not the table",
            verbs.len()
        );
        verbs
    }

    #[test]
    fn test_routed_subcommands_matches_the_dispatcher() {
        let parsed = dispatcher_routed_verbs();

        let missing_from_table: Vec<&String> = parsed
            .iter()
            .filter(|v| !ROUTED_SUBCOMMANDS.contains(&v.as_str()))
            .collect();
        assert!(
            missing_from_table.is_empty(),
            "these verbs are routed by try_dispatch_subcommand but absent from \
             ROUTED_SUBCOMMANDS, so the near-miss guard would fire on a verb that \
             actually works: {missing_from_table:?}"
        );

        let missing_from_dispatcher: Vec<&&str> = ROUTED_SUBCOMMANDS
            .iter()
            .filter(|v| !parsed.iter().any(|p| p == *v))
            .collect();
        assert!(
            missing_from_dispatcher.is_empty(),
            "these verbs are listed in ROUTED_SUBCOMMANDS but no longer routed by \
             try_dispatch_subcommand, so the guard stays silent on a word that is \
             now a paid prompt: {missing_from_dispatcher:?}"
        );
    }

    #[test]
    fn test_bare_word_near_miss_table() {
        // (input, expected) — `None` means "keep today's behavior: send it as a
        // prompt"; `Some(needles)` means the guard fires and the message names
        // each needle.
        let cases: &[(&str, Option<&[&str]>)] = &[
            // A real REPL-only command typed at the wrong entry point.
            (
                "tokens",
                Some(&["unknown command: tokens", "/tokens", "yoyo -p"]),
            ),
            ("cost", Some(&["unknown command: cost", "/cost"])),
            ("context", Some(&["unknown command: context", "/context"])),
            // A typo for a routed subcommand: suggest the shell form.
            ("statsu", Some(&["status", "yoyo status"])),
            ("doctorr", Some(&["doctor", "yoyo doctor"])),
            // Routed verbs dispatch before the guard is ever consulted.
            ("doctor", None),
            ("status", None),
            // Whitespace: a real prompt, never a command.
            ("refactor this file for me", None),
            // Resembles nothing known: still a legitimate prompt.
            ("zzqqx", None),
            ("", None),
        ];

        for (input, expected) in cases {
            let got = bare_word_near_miss(input);
            match expected {
                None => assert_eq!(
                    got, None,
                    "expected {input:?} to stay a prompt, got a guard message: {got:?}"
                ),
                Some(needles) => {
                    let msg = got.unwrap_or_else(|| {
                        panic!("expected the guard to fire for {input:?}, got None")
                    });
                    for needle in *needles {
                        assert!(
                            msg.contains(needle),
                            "guard message for {input:?} does not mention {needle:?}: {msg}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_bare_word_near_miss_is_silent_for_every_routed_verb() {
        // The guard must never see a verb the dispatcher already handles.
        for verb in ROUTED_SUBCOMMANDS {
            assert_eq!(
                bare_word_near_miss(verb),
                None,
                "{verb} is routed by try_dispatch_subcommand — the guard must not fire on it"
            );
        }
    }

    #[test]
    fn test_bare_word_arg_only_fires_on_a_single_unflagged_word() {
        let argv =
            |parts: &[&str]| -> Vec<String> { parts.iter().map(|s| s.to_string()).collect() };
        assert_eq!(
            bare_word_arg(&argv(&["yoyo", "tokens"])),
            Some("tokens"),
            "one bare word is the case the guard exists for"
        );
        assert_eq!(bare_word_arg(&argv(&["yoyo"])), None, "no args is the REPL");
        assert_eq!(
            bare_word_arg(&argv(&["yoyo", "do", "the", "thing"])),
            None,
            "multiple positionals stay a prompt"
        );
        assert_eq!(
            bare_word_arg(&argv(&["yoyo", "explain this repo"])),
            None,
            "one quoted multi-word arg stays a prompt"
        );
        assert_eq!(
            bare_word_arg(&argv(&["yoyo", "-p", "tokens"])),
            None,
            "the -p escape hatch must reach the prompt path"
        );
        assert_eq!(
            bare_word_arg(&argv(&["yoyo", "--help"])),
            None,
            "flags are not bare words"
        );
    }

    #[test]
    fn test_repl_only_commands_excludes_routed_verbs() {
        let repl_only = repl_only_commands();
        assert!(
            repl_only.contains(&"tokens"),
            "/tokens is a REPL-only command and should be a near-miss candidate"
        );
        for verb in ROUTED_SUBCOMMANDS {
            assert!(
                !repl_only.contains(verb),
                "{verb} is routed as a subcommand, so it is not REPL-only"
            );
        }
    }
}
