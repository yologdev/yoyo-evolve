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
    "model",
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

    /// Routed `yoyo <verb>` subcommands deliberately absent from `--help`'s
    /// Subcommands block, each with the reason a human wrote.
    ///
    /// **Ships EMPTY (Day 196).** The census that landed this guard found all 39
    /// of `ROUTED_SUBCOMMANDS`' verbs documented, so there is no debt to name. An
    /// empty register is a legitimate terminal state and not a broken scan —
    /// `REGISTERED_ORPHANS` and `REGISTERED_GIT_BYPASSES` both ship empty for the
    /// same reason — and the anti-vacuous branch is asserted on the *scan*, never
    /// on this list, so a broken extractor still fails loudly.
    const REGISTERED_UNDOCUMENTED_SUBCOMMANDS: &[(&str, &str)] = &[];

    /// The `--help` Subcommands block, sliced from its header to the blank line
    /// that ends it; `None` when the header is absent.
    ///
    /// Slicing rather than scanning the whole help text is the **stricter**
    /// direction and it is deliberate: a verb named only in an unrelated example
    /// elsewhere in `--help` must not read as documented.
    fn subcommands_block(text: &str) -> Option<&str> {
        let header = "Subcommands (run from shell, no REPL):";
        let start = text.find(header)? + header.len();
        let rest = &text[start..];
        let end = rest.find("\n\n").map_or(text.len(), |i| start + i);
        Some(&text[start..end])
    }

    /// The verb of every entry line in `block` — the first token of each line
    /// beginning with exactly two spaces followed by a non-space.
    ///
    /// **Exact-token extraction, never a substring scan**, and that is
    /// load-bearing rather than hygiene. `ROUTED_SUBCOMMANDS` is full of
    /// strict-prefix pairs (`def` inside `default`, `run` inside `running`,
    /// `ast`, `pr`, `risk`), so a bare `contains` reports a verb as documented on
    /// the strength of an unrelated word. Matching whole tokens is immune to that
    /// by construction, and anchoring on the *entry shape* additionally kills the
    /// collision slicing cannot remove: descriptions inside this very block carry
    /// examples (`e.g. yoyo run cargo clippy`, `e.g. yoyo def handle_goal`), so
    /// even a boundary-aware `contains` would let one entry's example document a
    /// different verb.
    ///
    /// `help::help_mentions` was checked for reuse and is **not** usable here, for
    /// two independent reasons: it lives inside `help.rs`'s own private
    /// `#[cfg(test)] mod tests`, so no sibling module can reach it without
    /// widening visibility — a production change smuggled in under a test — and it
    /// checks the **right** edge only, where the requirement here is a boundary on
    /// both sides.
    fn documented_subcommand_entries(block: &str) -> Vec<&str> {
        block
            .lines()
            .filter_map(|line| {
                let rest = line.strip_prefix("  ")?;
                if rest.starts_with(' ') {
                    return None;
                }
                rest.split_whitespace().next()
            })
            .collect()
    }

    #[test]
    fn documented_subcommand_entries_matches_whole_tokens_only() {
        // The near-miss guard for the matcher above. A discriminator tested only
        // on the side that fires is vacuous green, so both directions are pinned.
        let block = [
            "",
            "  def               Find where a symbol is defined",
            "  run               Run a shell command (e.g. yoyo run cargo clippy)",
            "  version",
            "    /help             a REPL entry, indented four — not a subcommand",
        ]
        .join("\n");
        let entries = documented_subcommand_entries(&block);

        // Fires: a real entry, and an entry carrying no description at all.
        assert!(entries.contains(&"def"), "a real entry must be found");
        assert!(
            entries.contains(&"version"),
            "an entry with no trailing description still counts"
        );

        // Must NOT fire: a word inside a neighbouring entry's example is not an
        // entry — this is the half slicing to the block cannot remove.
        assert!(
            !entries.contains(&"cargo"),
            "an example inside a description must not read as a documented verb"
        );
        // Must NOT fire: a four-space REPL line is a different section's shape.
        assert!(
            !entries.contains(&"/help"),
            "a REPL command line is not a subcommand entry"
        );

        // Must NOT fire: a longer word that merely starts or ends with the verb
        // does not document it, in either direction.
        let prefix_only = "  default           Something\n  predef            Something\n";
        let got = documented_subcommand_entries(prefix_only);
        assert!(
            !got.contains(&"def"),
            "`default`/`predef` entries must not document `def`"
        );
        assert!(
            got.contains(&"default") && got.contains(&"predef"),
            "anti-vacuous: those two really were extracted as their own entries"
        );

        // The slice stops at the blank line that ends the block.
        let text = "Subcommands (run from shell, no REPL):\n  one  A\n\n  two  B\n";
        let sliced = subcommands_block(text).expect("header is present");
        let sliced_entries = documented_subcommand_entries(sliced);
        assert_eq!(
            sliced_entries,
            vec!["one"],
            "the block must end at the blank line, not run into the next section"
        );
        assert!(
            subcommands_block("no header here").is_none(),
            "a missing header is None, never an empty block silently passing"
        );
    }

    /// `--help`'s Subcommands block is prose; `ROUTED_SUBCOMMANDS` is the table
    /// the bare-word near-miss guard consults. Nothing tied them together, so
    /// `gasp` shipped routed and undiscoverable (Day 187 recorded it as a known
    /// gap; this is the reader that closes it).
    ///
    /// Both authorities are read **by calling them** — this const, and
    /// `help::cli_help_text()` — never a third hand-typed list. Three branches in
    /// two directions, the shape all five sibling gates use.
    #[test]
    fn cli_help_documents_every_verb_the_near_miss_table_routes() {
        let text = crate::help::cli_help_text();

        // Branch 3 — ANTI-VACUOUS, asserted FIRST. A scanner that finds nothing
        // and passes is this very defect wearing the opposite sign, and it is
        // quieter than the bug.
        assert!(
            !ROUTED_SUBCOMMANDS.is_empty(),
            "ROUTED_SUBCOMMANDS is empty — this is a REFUSAL, not a clean scan"
        );
        assert!(
            !text.is_empty(),
            "cli_help_text() came back empty — this is a REFUSAL, not a clean scan"
        );
        let block = subcommands_block(&text).expect(
            "the `Subcommands (run from shell, no REPL):` header is missing from --help — \
             this is a REFUSAL, not `no undocumented subcommands`",
        );
        let entries = documented_subcommand_entries(block);
        assert!(
            entries.len() > 20,
            "extracted only {} Subcommands entries — the block shape probably changed, \
             and a scan that finds nothing must not read as a clean bill",
            entries.len()
        );

        let documented = |verb: &str| entries.contains(&verb);

        // Branch 1 — fatal on the UNNAMED case. The gate does not forbid an
        // undocumented subcommand, it forbids an unnamed one.
        let mut undocumented: Vec<&str> = Vec::new();
        for &verb in ROUTED_SUBCOMMANDS {
            if !documented(verb)
                && !REGISTERED_UNDOCUMENTED_SUBCOMMANDS
                    .iter()
                    .any(|(v, _)| *v == verb)
            {
                undocumented.push(verb);
            }
        }
        assert!(
            undocumented.is_empty(),
            "these verbs are routed as `yoyo <verb>` but appear in no --help Subcommands \
             entry: {undocumented:?}. A shipped subcommand nobody can discover is the \
             defect this guard exists for. Two remedies, either is fine: (1) add a line to \
             the `Subcommands (run from shell, no REPL):` block in help::cli_help_text(), \
             in the same shape as its neighbours; or (2) paste the literal register line \
             (\"verb\", \"why this is deliberately undocumented\") into \
             REGISTERED_UNDOCUMENTED_SUBCOMMANDS."
        );

        // Branch 2 — the ratchet, fatal in BOTH directions: an exception list
        // only pays itself down if *improving* is a failure too.
        for &(verb, reason) in REGISTERED_UNDOCUMENTED_SUBCOMMANDS {
            assert!(
                !reason.trim().is_empty(),
                "`{verb}` is registered with an empty reason — an unnamed debt wearing a \
                 name is not a name. Write why it is deliberately undocumented."
            );
            assert!(
                ROUTED_SUBCOMMANDS.contains(&verb),
                "`{verb}` is registered as deliberately undocumented but is no longer in \
                 ROUTED_SUBCOMMANDS — delete the register entry ({reason})"
            );
            assert!(
                !documented(verb),
                "`{verb}` is now documented in --help — the debt is paid, so delete its \
                 REGISTERED_UNDOCUMENTED_SUBCOMMANDS entry ({reason})"
            );
        }
    }
}
