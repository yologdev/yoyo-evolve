//! Drift guards for the subcommand vocabulary documented in `help_data.rs`.
//!
//! Moved verbatim out of `src/help_data.rs` (Day 166) — that file sat at 1995
//! of the 2000-line `MAX_MODULE_LINES` cap, so any edit to the help data risked
//! a fatal `tests/module_size.rs` failure and a whole-task revert. These guards
//! read *other* modules' source text off disk to compare three copies of the
//! subcommand vocabulary, which is a different concern from the help data
//! itself.

#[cfg(test)]
mod tests {
    use crate::commands::KNOWN_COMMANDS;
    use crate::help_data::*;
    use std::collections::HashSet;
    use std::path::Path;

    // ── Completeness tests ──

    /// Every `*_SUBCOMMANDS` completion table paired with the command it belongs
    /// to, plus the usage prefix its lines are written under. Consts are
    /// IMPORTED, never re-typed — a hand-copied verb list pins my belief about
    /// the table, not the table (Day 147).
    ///
    /// The prefix is separate from the help key because one table documents a
    /// sub-subcommand: `git::STASH_SUBCOMMANDS` is the verb set after
    /// `/git stash`, and lives in `command_help("git")`. Pairing it with the
    /// conversation-level `/stash` would have silently "passed" on push/pop/
    /// list/drop, which both commands happen to share — an accidental green of
    /// exactly the kind this test exists to stop.
    fn subcommand_tables() -> Vec<(&'static str, &'static str, &'static [&'static str])> {
        vec![
            ("bg", "/bg", crate::commands::BG_SUBCOMMANDS),
            (
                "checkpoint",
                "/checkpoint",
                crate::commands_fork::CHECKPOINT_SUBCOMMANDS,
            ),
            ("config", "/config", crate::commands::CONFIG_SUBCOMMANDS),
            (
                "context",
                "/context",
                crate::commands_project::CONTEXT_SUBCOMMANDS,
            ),
            ("copy", "/copy", crate::commands_web::COPY_SUBCOMMANDS),
            ("fork", "/fork", crate::commands_fork::FORK_SUBCOMMANDS),
            ("git", "/git", crate::commands::GIT_SUBCOMMANDS),
            ("git", "/git stash", crate::git::STASH_SUBCOMMANDS),
            ("goal", "/goal", crate::commands::GOAL_SUBCOMMANDS),
            ("history", "/history", crate::commands::HISTORY_SUBCOMMANDS),
            ("lint", "/lint", crate::commands_lint::LINT_SUBCOMMANDS),
            ("plan", "/plan", crate::commands_plan::PLAN_SUBCOMMANDS),
            ("pr", "/pr", crate::commands::PR_SUBCOMMANDS),
            (
                "refactor",
                "/refactor",
                crate::commands::REFACTOR_SUBCOMMANDS,
            ),
            (
                "revisit",
                "/revisit",
                crate::commands_revisit::REVISIT_SUBCOMMANDS,
            ),
            ("risk", "/risk", crate::commands_risk::RISK_SUBCOMMANDS),
            ("skill", "/skill", crate::commands_skill::SKILL_SUBCOMMANDS),
            ("spawn", "/spawn", crate::commands_spawn::SPAWN_SUBCOMMANDS),
            ("watch", "/watch", crate::watch::WATCH_SUBCOMMANDS),
            ("web", "/web", crate::commands_web::WEB_SUBCOMMANDS),
        ]
    }

    #[test]
    fn test_every_subcommand_table_is_documented_in_usage_form() {
        // Arithmetic version of a guard I had been re-remembering per command.
        // Three copies of every verb set exist — dispatcher, completion table,
        // prose help — and nothing kept them in agreement, so the class was hit
        // three times by accident (#702 /todo, #722 /goal verify + /spawn).
        //
        // The assertion is the literal USAGE FORM "/<cmd> <verb>", not a bare
        // substring: /spawn's old bespoke guard stayed green while the drift was
        // live because "manifests" happened to appear as a noun in prose.
        //
        // Disclosed scope limits (known gaps, NOT claims of coverage):
        //  - Tokens starting with '-' (--deep, --all, -o, …) are skipped; flags
        //    are documented in their own way and are not usage-form lines.
        //  - CHECKPOINT_SUBCOMMANDS (src/commands_fork.rs) and
        //    CONTEXT_SUBCOMMANDS (src/commands_project.rs) are private to their
        //    modules and are NOT covered here. Widening their visibility is a
        //    separate sweep, deliberately not done in this task.
        //  - A verb documented only inside an alternation ("/plan on|open")
        //    does not count: `/help` readers scan usage lines, and an alias
        //    hidden behind a pipe is what the drift looked like in practice.
        let mut missing: Vec<String> = Vec::new();

        for (cmd, prefix, subs) in subcommand_tables() {
            let help =
                command_help(cmd).unwrap_or_else(|| panic!("/{cmd} should have a help entry"));
            for sub in subs {
                if sub.starts_with('-') {
                    continue;
                }
                let usage = format!("{prefix} {sub}");
                if !help.contains(&usage) {
                    missing.push(format!(
                        "  {prefix} — table lists `{sub}` but `command_help(\"{cmd}\")` never \
                         contains the usage form \"{usage}\""
                    ));
                }
            }
        }

        assert!(
            missing.is_empty(),
            "{} subcommand(s) are in a completion table but undocumented in usage form.\n\
             Either add a `\\x20 /<cmd> <verb>   <description>` line to that command's Usage \
             block in src/help_data.rs, or — if the dispatcher does not implement the verb — \
             remove the phantom token from its table.\n{}",
            missing.len(),
            missing.join("\n")
        );
    }

    /// The dispatcher module that actually *executes* each command's verbs.
    /// Every command in `subcommand_tables()` must have an entry — a command
    /// whose verbs are handled in several modules would be listed here at its
    /// entry point, with a comment, never silently omitted.
    fn dispatcher_source(cmd: &str) -> &'static str {
        match cmd {
            "bg" => "src/commands_bg.rs", // handle_bg
            // /checkpoint's verbs are matched in handle_checkpoint, which lives
            // in commands_fork.rs beside CHECKPOINT_SUBCOMMANDS. Weak for `list`
            // and `delete`: handle_fork has arms on those same literals in this
            // file, so a presence check can't tell the two dispatchers apart.
            "checkpoint" => "src/commands_fork.rs",
            "config" => "src/commands_config.rs", // handle_config / handle_config_set
            "context" => "src/commands_project.rs", // handle_context
            "copy" => "src/commands_web.rs",      // handle_copy
            "fork" => "src/commands_fork.rs",     // handle_fork
            // Both /git and `/git stash` route through parse_git_args.
            "git" => "src/git.rs",
            "goal" => "src/commands_goal.rs", // handle_goal
            // /history's only verb ("detail") is routed in dispatch_command,
            // not inside commands_session.rs — the entry point is the REPL router.
            "history" => "src/dispatch.rs",
            "lint" => "src/commands_lint.rs",
            "plan" => "src/commands_plan.rs",
            "pr" => "src/commands_git_pr.rs", // parse_pr_args
            "refactor" => "src/commands_refactor.rs",
            "revisit" => "src/commands_revisit.rs",
            "risk" => "src/commands_risk.rs",
            "skill" => "src/commands_skill.rs",
            "spawn" => "src/commands_spawn.rs",
            "watch" => "src/watch.rs",
            "web" => "src/commands_web.rs",
            other => panic!(
                "subcommand_tables() lists `/{other}` but dispatcher_source() has no entry for it \
                 — add the module that executes its verbs (or say why it has none)"
            ),
        }
    }

    /// Verbs that ARE implemented, but never as the bare literal `"verb"` in
    /// their dispatcher's non-test half. Every entry names the code that
    /// actually handles it — a bare entry is how a deleted feature keeps
    /// breathing.
    const KNOWN_INDIRECT: &[(&str, &str, &str)] = &[
        // (command, token, the code that actually handles it)
        // /config's verbs are routed on the COMPOUND literal in the REPL router,
        // then executed by handle_config_* in src/commands_config.rs.
        (
            "config",
            "show",
            "src/dispatch.rs:198 `rest.starts_with(\"config show\")`",
        ),
        (
            "config",
            "edit",
            "src/dispatch.rs:201 `rest.starts_with(\"config edit\")`",
        ),
        (
            "config",
            "set",
            "src/dispatch.rs:204 `rest.starts_with(\"config set\")`",
        ),
        (
            "config",
            "get",
            "src/dispatch.rs:207 `rest == \"config get\"`",
        ),
        // /lint fix is an exact-match arm on the whole command string, and runs
        // commands::handle_lint_fix (src/dispatch.rs:662) — not handle_lint.
        (
            "lint",
            "fix",
            "src/dispatch.rs:163 `\"/lint fix\" => CommandRoute::LintFix`",
        ),
    ];

    /// Marker for the start of a module's own `#[cfg(test)]` block. Built from
    /// pieces at runtime so this file's own test module doesn't match it.
    fn test_module_marker() -> String {
        format!("#[cfg{}]\nmod tests", "(test)")
    }

    /// Drop `*_SUBCOMMANDS` table declarations from a dispatcher's source.
    /// Ten of the eighteen tables live in the same file as their dispatcher, so
    /// without this the search would find the token in the table itself and the
    /// whole test would be vacuously green — a guard that cannot fail.
    fn strip_subcommand_tables(src: &str) -> String {
        let mut out = String::new();
        let mut skipping = false;
        for line in src.lines() {
            if skipping {
                if line.contains("];") {
                    skipping = false;
                }
                continue;
            }
            if line.contains("_SUBCOMMANDS") && line.contains("&[") {
                if !line.contains("];") {
                    skipping = true;
                }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    #[test]
    fn test_every_subcommand_table_token_appears_in_its_dispatcher() {
        // The OTHER direction of the drift guard (#725). The sibling test above
        // compares the completion table against the prose help (copy 2 vs copy
        // 3); this one compares it against the DISPATCHER (copy 2 vs copy 1).
        //
        // Both real bugs lived in this direction and were invisible to a
        // prose check, because the help entry documented the phantom too:
        // /todo hinted a `list` verb four mirrors described and no code
        // implemented (#702), and /map advertised `--depth N` that
        // parse_map_args rejects (Day 164).
        //
        // What this does NOT catch, stated plainly: it is a PRESENCE check, not
        // a match-arm parse. A token that appears as a quoted literal anywhere
        // in the non-test half of the dispatcher module counts as present, even
        // if that occurrence is a printed hint, an error string, or a match arm
        // for a different command. Cutting the haystack at `#[cfg(test)]` and
        // removing the table declarations is the cheap sharpening available
        // today; a real parse of the dispatch match is a separate task.
        let mut missing: Vec<String> = Vec::new();
        let marker = test_module_marker();

        for (cmd, prefix, subs) in subcommand_tables() {
            let rel = dispatcher_source(cmd);
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
            let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "dispatcher source for /{cmd} is unreadable at {}: {e}",
                    path.display()
                )
            });
            // Cut at the module's own test module: a test quoting the token is
            // not evidence the dispatcher accepts it. The marker is the
            // `#[cfg(test)] mod tests` pair, not a bare `#[cfg(test)]` —
            // src/git.rs carries `#[cfg(test)]` guards inside real functions
            // near the top, and splitting on those would blank the whole file.
            let non_test = src.split(marker.as_str()).next().unwrap_or("");
            let haystack = strip_subcommand_tables(non_test);

            for sub in subs {
                if sub.starts_with('-') {
                    continue; // flags, same skip as the usage-form test
                }
                if KNOWN_INDIRECT.iter().any(|(c, t, _)| *c == cmd && t == sub) {
                    continue;
                }
                if !haystack.contains(&format!("\"{sub}\"")) {
                    missing.push(format!(
                        "  {prefix} {sub} — no `\"{sub}\"` literal in {rel} (outside its tests)"
                    ));
                }
            }
        }

        assert!(
            missing.is_empty(),
            "{} subcommand token(s) are offered by a completion table but never \
             appear in the dispatcher that would have to accept them.\n\
             Either implement the verb, or delete the phantom from its table AND from its \
             `command_help` usage line. If the verb IS implemented under a different literal \
             (alias, prefix match, parsed enum, helper in another module), add it to \
             KNOWN_INDIRECT with a comment naming the code that handles it.\n{}",
            missing.len(),
            missing.join("\n")
        );
    }

    #[test]
    fn test_risk_help_mentions_every_subcommand() {
        // Discoverability guard: every word-like /risk subcommand must appear in
        // the /risk help text (Day 139 — predict/history/effectiveness were
        // implemented but undocumented, invisible to `/help risk`).
        let help = command_help("risk").expect("/risk has a help entry");
        for sub in crate::commands_risk::RISK_SUBCOMMANDS {
            if sub.starts_with('-') {
                continue; // flags like --all are covered separately
            }
            assert!(
                help.contains(sub),
                "/risk help is missing subcommand `{sub}`"
            );
        }
        assert!(help.contains("--all"), "/risk help is missing --all");
    }

    #[test]
    fn test_spawn_help_mentions_every_subcommand() {
        // Discoverability guard: every /spawn subcommand, flag, and alias must
        // appear in the /spawn help text (Day 142 — worktrees and --pr were
        // shipped but undocumented, invisible to `/help spawn`). Flags and
        // aliases are included on purpose: exceptions in completeness tests
        // are how drift gets back in.
        let help = command_help("spawn").expect("/spawn has a help entry");
        for sub in crate::commands_spawn::SPAWN_SUBCOMMANDS {
            assert!(
                help.contains(sub),
                "/spawn help is missing subcommand `{sub}`"
            );
        }
    }

    #[test]
    fn test_spawn_help_names_every_verb_in_a_usage_line() {
        // Stronger than the sibling above, which passes on a bare substring
        // match anywhere in the entry. `manifest`/`manifests` were routed by
        // handle_spawn and listed in SPAWN_SUBCOMMANDS, yet appeared in the
        // help only as a NOUN in prose ("--parallel run manifests") — so the
        // substring check was green while the usage block never named them as
        // verbs (#722). Assert the payload (a `/spawn <verb>` usage line), not
        // the container (the word occurring somewhere).
        let help = command_help("spawn").expect("/spawn has a help entry");
        for sub in crate::commands_spawn::SPAWN_SUBCOMMANDS {
            if sub.starts_with('-') {
                continue; // flags are documented as `/spawn --flag <task>` forms
            }
            assert!(
                help.contains(&format!("/spawn {sub}")),
                "/spawn help never writes `/spawn {sub}` as a usage line — the verb is \
                 routed but users can't discover it"
            );
        }
    }

    #[test]
    fn test_every_known_command_has_help() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            // /exit is an alias for /quit — no dedicated help entry
            if name == "exit" {
                continue;
            }
            assert!(
                command_help(name).is_some(),
                "KNOWN_COMMAND {cmd} has no command_help entry"
            );
        }
    }

    #[test]
    fn test_every_known_command_has_short_description() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            assert!(
                command_short_description(name).is_some(),
                "KNOWN_COMMAND {cmd} has no command_short_description entry"
            );
        }
    }

    #[test]
    fn test_no_orphan_help_entries() {
        // Verify a fake command returns None (no catch-all that leaks)
        assert!(
            command_help("zzz_nonexistent").is_none(),
            "Fake command should not have a help entry"
        );
        assert!(
            command_short_description("zzz_nonexistent").is_none(),
            "Fake command should not have a short description"
        );
    }

    // ── Content quality tests ──

    #[test]
    fn test_short_descriptions_are_actually_short() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            if let Some(desc) = command_short_description(name) {
                assert!(
                    desc.len() <= 80,
                    "Short description for {cmd} is too long ({} chars): {desc}",
                    desc.len()
                );
            }
        }
    }

    #[test]
    fn test_help_entries_are_non_empty() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            if name == "exit" {
                continue;
            }
            if let Some(help) = command_help(name) {
                assert!(
                    help.len() >= 20,
                    "Help entry for {cmd} is suspiciously short ({} chars): {help}",
                    help.len()
                );
            }
        }
    }

    #[test]
    fn test_help_uses_bare_names_not_slashes() {
        // command_help matches on bare names like "add", not "/add"
        // Verify that passing a slash-prefixed name returns None
        // (callers are expected to strip the slash before calling)
        assert!(
            command_help("/add").is_none(),
            "command_help should not match slash-prefixed names"
        );
        assert!(
            command_short_description("/add").is_none(),
            "command_short_description should not match slash-prefixed names"
        );
        // But bare name works
        assert!(command_help("add").is_some());
        assert!(command_short_description("add").is_some());
    }

    // ── Edge case tests ──

    #[test]
    fn test_command_help_returns_none_for_empty() {
        assert!(command_help("").is_none());
    }

    #[test]
    fn test_cd_help_documents_key_behaviors() {
        let help = command_help("cd").expect("cd should have detailed help");
        assert!(help.contains("~"), "cd help should mention ~ expansion");
        assert!(
            help.contains("NOT reloaded"),
            "cd help should warn that project context is not reloaded"
        );
        let short = command_short_description("cd").expect("cd should have a short description");
        assert!(
            short.to_lowercase().contains("working directory"),
            "cd short description should mention the working directory"
        );
    }

    #[test]
    fn test_command_short_description_returns_none_for_empty() {
        assert!(command_short_description("").is_none());
    }

    #[test]
    fn test_command_short_description_returns_none_for_unknown() {
        assert!(command_short_description("zzz_nonexistent").is_none());
    }

    #[test]
    fn test_no_duplicate_short_descriptions() {
        // Deduplicate KNOWN_COMMANDS (e.g. /quick appears twice)
        let unique_cmds: HashSet<&str> = KNOWN_COMMANDS.iter().copied().collect();

        let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        let mut duplicates: Vec<(&str, &str)> = Vec::new();

        for cmd in &unique_cmds {
            let name = cmd.trim_start_matches('/');
            if let Some(desc) = command_short_description(name) {
                if let Some(first) = seen.get(desc) {
                    duplicates.push((cmd, first));
                } else {
                    seen.insert(desc, cmd);
                }
            }
        }

        // Allow known aliases: /exit and /quit share a description
        duplicates.retain(|(a, b)| {
            let pair = [a.trim_start_matches('/'), b.trim_start_matches('/')];
            !(pair.contains(&"exit") && pair.contains(&"quit"))
        });

        assert!(
            duplicates.is_empty(),
            "Duplicate short descriptions found: {duplicates:?}"
        );
    }

    // ── Consistency tests ──

    #[test]
    fn test_help_entries_start_with_command_name() {
        // Help text should mention the command (usually starts with /cmd)
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            if name == "exit" {
                continue;
            }
            if let Some(help) = command_help(name) {
                assert!(
                    help.contains(name) || help.contains(cmd),
                    "Help entry for {cmd} doesn't mention the command name anywhere"
                );
            }
        }
    }

    #[test]
    fn test_quit_and_exit_share_short_description() {
        let quit_desc = command_short_description("quit");
        let exit_desc = command_short_description("exit");
        assert!(quit_desc.is_some(), "/quit should have a short description");
        assert!(exit_desc.is_some(), "/exit should have a short description");
        assert_eq!(
            quit_desc, exit_desc,
            "/quit and /exit should share the same short description"
        );
    }
}
