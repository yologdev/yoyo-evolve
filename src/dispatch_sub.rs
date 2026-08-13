//! CLI subcommand dispatch — early-exit handlers for `yoyo <subcommand>`.
//!
//! Extracted from `dispatch.rs` to separate the two independent concerns:
//! - This module: CLI subcommand routing (`yoyo doctor`, `yoyo help`, etc.)
//!   that runs before the REPL starts.
//! - `dispatch.rs`: REPL `/command` routing for interactive session commands.
//!
//! The [`try_dispatch_subcommand`] function is called by [`crate::cli::parse_args`]
//! before any flag parsing begins. If a known subcommand matches, the handler
//! runs and returns `Some(None)` to signal "handled, exit cleanly".

use crate::cli::{collect_repeatable_flag, load_config_file, print_help, Config, VERSION};
use crate::format::*;
use crate::providers::default_model_for_provider;
use yoagent::skills::SkillSet;

/// Reconstruct the `/risk <args>` input string that `handle_risk` expects
/// from a CLI subcommand tail.
///
/// `yoyo risk snapshot` arrives as the tail `["snapshot"]` and must map to
/// the exact string `"/risk snapshot"` — the contract the evolve harness's
/// paste-diff (`yoyo risk snapshot`) depends on to reach the snapshot-writing
/// path in `commands_risk`. A bare `yoyo risk` (empty tail) maps to `"/risk"`.
///
/// This is a pure function so the CLI→handler contract can be locked by a test
/// without invoking the real, `.yoyo/`-writing snapshot dispatch.
fn build_risk_input(tail: &[String]) -> String {
    let rest = tail.join(" ");
    if rest.is_empty() {
        "/risk".to_string()
    } else {
        format!("/risk {rest}")
    }
}

/// Remove a flag and its value from an args slice.
///
/// Scans `args` for occurrences of `flag` (e.g. `"--skills"`) and drops
/// both the flag and the following argument (its value).  Args that are
/// not the flag pass through unchanged.
fn strip_flag_with_value(args: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == flag {
            skip_next = true;
            continue;
        }
        out.push(arg.clone());
    }
    out
}

/// The note the shell `yoyo todo ...` path owes the user (#679).
///
/// `/todo`'s task list is a process-global in-memory `static` — correct as REPL
/// session state, but each shell invocation is a fresh process, so `yoyo todo
/// add "x"` prints a green checkmark and then discards the task. That is a
/// silent wrong-op reported as success. Until the list is either persisted or
/// dropped from the shell surface (a product decision, not mine to make here),
/// say so at the boundary.
///
/// `todo board` is exempt: it reads and writes `session_plan/*.md` on disk, so
/// it genuinely persists and the note would be a lie about it.
fn todo_cli_session_note(args: &[String]) -> Option<&'static str> {
    match args.get(2).map(|s| s.as_str()) {
        Some("board") => None,
        _ => Some(
            "note: the todo list is in-memory and per-process -- each `yoyo todo` \
             shell call is a fresh process, so nothing persists between calls. Use \
             /todo inside the interactive REPL for a session-scoped list.",
        ),
    }
}

/// Build a `/command ...` string from shell args, preserving multi-word tokens.
///
/// Shell args like `["yoyo", "grep", "fn main", "src/"]` become `/grep "fn main" src/`.
/// Any arg containing whitespace is wrapped in double quotes so downstream parsers
/// (which use `tokenize_quoted`) can distinguish multi-word patterns from separate args.
fn quote_args_as_command(args: &[String]) -> String {
    let parts: Vec<String> = args[1..]
        .iter()
        .map(|a| {
            if a.contains(' ') || a.contains('\t') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        })
        .collect();
    format!("/{}", parts.join(" "))
}

/// Build a `/command ...` string from shell args by plain space-join — no
/// re-quoting. For handlers that take the remainder VERBATIM (`/todo add`,
/// `/goal set`, `/memories <query>`): those never call `tokenize_quoted`, so
/// `quote_args_as_command`'s added `"..."` would be stored/searched as literal
/// quote characters (#679 finding 2). `yoyo todo add "fix the parser"` and
/// `yoyo todo add fix the parser` now store identical descriptions, matching
/// the REPL path.
fn join_args_as_command(args: &[String]) -> String {
    format!("/{}", args[1..].join(" "))
}

/// `--version`/`-V` — both print and bail out before any config is built.
/// This helper is the first slice of the parse_args refactor (#261); it
/// exists so the "did I handle this?" decision can be unit-tested in
/// isolation, and so future positional subcommands (`yoyo setup`,
/// `yoyo doctor`, etc., once they exist) have an obvious place to land.
///
/// Returns:
/// - `Some(None)` — a subcommand matched, was handled (printed output),
///   and `parse_args` should return `None` to its caller.
/// - `Some(Some(cfg))` — a subcommand matched and produced a usable
///   `Config` (no current subcommand does this; reserved for future use).
/// - `None` — no subcommand matched; fall through to flag parsing.
pub(crate) fn try_dispatch_subcommand(args: &[String]) -> Option<Option<Config>> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Some(None);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", crate::commands_info::version_line());
        return Some(None);
    }

    // Positional subcommands: `yoyo <subcmd>`.
    // args[0] is the binary path; args[1] is the subcommand name.
    // Each arm calls the existing REPL handler from commands_dev and exits cleanly
    // (handlers return () and print directly to stdout).
    if let Some(sub) = args.get(1) {
        match sub.as_str() {
            "doctor" => {
                // Respect --provider / --model flags if present, else fall back to
                // config-file values, else compiled-in defaults. We deliberately
                // do NOT run the full parse_args pipeline because `yoyo doctor`
                // should work even when the API key / model setup is incomplete
                // (that's exactly the failure mode the diagnostic exists to detect).
                let (file_config, _) = load_config_file();
                let provider = flag_value(args, &["--provider"])
                    .or_else(|| file_config.get("provider").cloned())
                    .unwrap_or_else(|| "anthropic".into())
                    .to_lowercase();
                let model = flag_value(args, &["--model"])
                    .or_else(|| file_config.get("model").cloned())
                    .unwrap_or_else(|| default_model_for_provider(&provider));
                crate::commands_dev::handle_doctor(&provider, &model);
                return Some(None);
            }
            "health" => {
                // handle_health takes no arguments — it auto-detects project type
                // from the current directory and runs the appropriate checks.
                crate::commands_dev::handle_health();
                return Some(None);
            }
            "help" => {
                print_help();
                return Some(None);
            }
            "version" => {
                let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
                if verbose {
                    let (file_config, _) = load_config_file();
                    let provider = flag_value(args, &["--provider"])
                        .or_else(|| file_config.get("provider").cloned())
                        .unwrap_or_else(|| "anthropic".into())
                        .to_lowercase();
                    let model = flag_value(args, &["--model"])
                        .or_else(|| file_config.get("model").cloned())
                        .unwrap_or_else(|| default_model_for_provider(&provider));
                    crate::commands_info::handle_version_verbose(&provider, &model);
                } else {
                    println!("{}", crate::commands_info::version_line());
                }
                return Some(None);
            }
            "setup" => {
                crate::setup::run_setup_wizard();
                return Some(None);
            }
            "init" => {
                crate::commands_project::handle_init();
                return Some(None);
            }
            "lint" => {
                let input = quote_args_as_command(args);
                crate::commands_lint::handle_lint(&input);
                return Some(None);
            }
            "test" => {
                // #745: args after `yoyo test` used to be silently discarded and
                // the full suite ran. Forward them verbatim.
                crate::commands_lint::handle_test(&args[2..]);
                return Some(None);
            }
            "tree" => {
                let input = quote_args_as_command(args);
                crate::commands_tree::handle_tree(&input);
                return Some(None);
            }
            "map" => {
                let input = quote_args_as_command(args);
                crate::commands_map::handle_map(&input);
                return Some(None);
            }
            "outline" => {
                let input = quote_args_as_command(args);
                crate::commands_search::handle_outline(&input);
                return Some(None);
            }
            "run" => {
                let input = quote_args_as_command(args);
                crate::commands_run::handle_run(&input);
                return Some(None);
            }
            "diff" => {
                let input = quote_args_as_command(args);
                crate::commands_git::handle_diff(&input);
                return Some(None);
            }
            "commit" => {
                let input = quote_args_as_command(args);
                crate::commands_git::handle_commit(&input);
                return Some(None);
            }
            "review" => {
                // Non-interactive code review: build an agent, run the review
                // prompt, print the result to stdout, and exit.
                let review_arg = build_review_arg(args);
                let exit_code = run_review_subcommand(args, &review_arg);
                std::process::exit(exit_code);
            }
            "blame" => {
                let input = quote_args_as_command(args);
                crate::commands_git_review::handle_blame(&input);
                return Some(None);
            }
            "risk" => {
                // Reconstruct the `/risk <args>` input string that handle_risk
                // expects. args[0] = binary, args[1] = "risk", args[2..] = risk
                // subcommand + flags. This makes the whole risk subsystem
                // (snapshot/validate/history/predict/accuracy/effectiveness/--all)
                // reachable from any non-interactive context — the prerequisite
                // for the harness (or a human) to record daily risk snapshots
                // where the DREAM measurement data actually accumulates.
                let input = build_risk_input(&args[2..]);
                crate::commands_risk::handle_risk(&input);
                return Some(None);
            }
            "grep" => {
                let input = quote_args_as_command(args);
                crate::commands_search::handle_grep(&input);
                return Some(None);
            }
            "find" => {
                let input = quote_args_as_command(args);
                crate::commands_search::handle_find(&input);
                return Some(None);
            }
            "index" => {
                crate::commands_search::handle_index();
                return Some(None);
            }
            "update" => {
                if let Err(e) = crate::commands_update::handle_update() {
                    eprintln!("{RED}  {e}{RESET}");
                }
                return Some(None);
            }
            "docs" => {
                let input = quote_args_as_command(args);
                crate::commands_project::handle_docs(&input);
                return Some(None);
            }
            "skill" => {
                let skill_dirs = collect_repeatable_flag(args, "--skills");
                let filtered = strip_flag_with_value(args, "--skills");
                let input = quote_args_as_command(&filtered);
                let skills = if skill_dirs.is_empty() {
                    SkillSet::empty()
                } else {
                    SkillSet::load(&skill_dirs).unwrap_or_else(|e| {
                        eprintln!("{YELLOW}warning:{RESET} Failed to load skills: {e}");
                        SkillSet::empty()
                    })
                };
                crate::commands_skill::handle_skill(&input, &skills);
                return Some(None);
            }
            "watch" => {
                let input = quote_args_as_command(args);
                crate::watch::handle_watch(&input);
                return Some(None);
            }
            "status" => {
                // Bare subcommand: no active session, so show what we can
                // without agent state (version, git branch, cwd).
                let cwd = std::env::current_dir()
                    .map_or_else(|_| "?".into(), |p| p.display().to_string());
                println!("{DIM}  yoyo v{VERSION}");
                if let Some(branch) = crate::git::git_branch() {
                    println!("  git:     {branch}");
                }
                println!("  cwd:     {cwd}");
                println!("  (no active session — start yoyo for full status){RESET}\n");
                return Some(None);
            }
            "undo" => {
                // Bare subcommand: no turn history available (no active session).
                // Support --last-commit which works standalone; for other args,
                // explain that turn-based undo requires a session.
                let input = quote_args_as_command(args);
                let mut history = crate::session::TurnHistory::new();
                crate::commands_git::handle_undo(&input, &mut history);
                return Some(None);
            }
            "changelog" => {
                let input = quote_args_as_command(args);
                crate::commands_info::handle_changelog(&input);
                return Some(None);
            }
            "evolution" => {
                let input = quote_args_as_command(args);
                crate::commands_info::handle_evolution(&input);
                return Some(None);
            }
            "config" => {
                // `yoyo config show`, `yoyo config get <key>`, and bare `yoyo config`
                // work without an interactive session. `set` and `edit` require agent state.
                let sub2 = args.get(2).map(|s| s.as_str());
                match sub2 {
                    None | Some("show") => {
                        crate::commands_config::handle_config_show();
                    }
                    Some("get") => {
                        // Reconstruct as /config get <key>
                        let key = args.get(3).map(|s| s.as_str()).unwrap_or("");
                        let input = format!("/config get {key}");
                        crate::commands_config::handle_config_get(&input);
                    }
                    Some(other) => {
                        eprintln!(
                            "{YELLOW}  `config {other}` requires an interactive session.{RESET}"
                        );
                        eprintln!("{DIM}  Try: yoyo config show (works from the shell){RESET}");
                    }
                }
                return Some(None);
            }
            "permissions" => {
                // Load permission config from config file (same as parse_args does)
                // so the user can inspect their effective permissions from the shell.
                let (_, raw_config) = load_config_file();
                let raw_permissions = crate::config::parse_permissions_from_config(&raw_config);
                // #749 item 3: route through the same gate parse_args uses, so
                // this view shows what is in force rather than what the project
                // config asked for. Displaying refused patterns as active would
                // be a lie in the one place a user goes to check.
                // `--trust-project` is read straight off `args` here as well:
                // this dispatcher runs *before* `parse_args` calls
                // `set_trust_project`, so consulting only the global would make
                // `yoyo permissions --trust-project` understate what a real
                // session would apply.
                let trusted =
                    crate::cli::is_trust_project() || args.iter().any(|a| a == "--trust-project");
                let gated = crate::cli::gate_project_permissions(
                    raw_permissions,
                    false,
                    crate::config::loaded_config_is_project_local(),
                    trusted,
                );
                if !gated.refused_allow.is_empty() && !crate::format::is_quiet() {
                    let msg = crate::cli::project_permission_refusal_message(
                        &gated.refused_allow,
                        crate::format::is_plain_output(),
                    );
                    eprintln!("{YELLOW}{msg}{RESET}");
                }
                let permissions = gated.permissions;
                let dir_restrictions = crate::config::parse_directories_from_config(&raw_config);
                let auto_approve = args.iter().any(|a| a == "--yes" || a == "-y");
                crate::commands_config::handle_permissions(
                    auto_approve,
                    &permissions,
                    &dir_restrictions,
                );
                return Some(None);
            }
            "todo" => {
                // Plain join, not quote_args_as_command: handle_todo's add arm
                // takes the remainder verbatim (#679 finding 2).
                let input = join_args_as_command(args);
                let output = crate::commands_todo::handle_todo(&input);
                println!("{output}");
                // Honesty at the boundary: the in-memory list dies with this
                // process (#679). stderr, so piped stdout stays clean.
                if let Some(note) = todo_cli_session_note(args) {
                    eprintln!("{DIM}  {note}{RESET}");
                }
                return Some(None);
            }
            "goal" => {
                // Plain join: handle_goal's `set` arm takes its remainder
                // verbatim (strip_prefix("set").trim(), no tokenize_quoted in
                // commands_goal.rs), so re-quoting a multi-word goal would
                // store literal `"` chars — same shape as #679 finding 2.
                let input = join_args_as_command(args);
                let result = crate::commands_goal::handle_goal(&input);
                // /goal check sends to agent which requires a session — just print
                // the goal info for shell usage.
                if let crate::dispatch::CommandResult::SendToAgent(_) = result {
                    eprintln!("{YELLOW}  /goal check requires an interactive session.{RESET}");
                    eprintln!("{DIM}  Start yoyo and use: /goal check{RESET}\n");
                }
                return Some(None);
            }
            "memories" => {
                // Plain join: handle_memories treats the remainder verbatim as
                // a search query (no tokenize_quoted in commands_memory.rs), so
                // re-quoting a multi-word query would search for literal `"`s.
                let input = join_args_as_command(args);
                crate::commands_memory::handle_memories(&input);
                return Some(None);
            }
            "extended" => {
                // Extended mode requires an active agent session — print usage and
                // suggest starting yoyo interactively.
                eprintln!("{YELLOW}  /extended requires an interactive session.{RESET}");
                eprintln!("{DIM}  Start yoyo and use: /extended <task> [--turns N]{RESET}\n");
                return Some(None);
            }
            _ => {}
        }
    }

    None
}

/// Every verb `try_dispatch_subcommand` routes as `yoyo <verb>`.
///
/// This is the candidate list for the bare-word near-miss guard below. It is a
/// second copy of the match arms above, so `test_routed_subcommands_matches_the_dispatcher`
/// reads this file's source and asserts the two agree in BOTH directions — a verb
/// added to the `match` without being added here would make the guard fire on a verb
/// that actually works, and a verb removed from the `match` but left here would make
/// the guard stay silent on a word that is now a paid prompt.
pub const ROUTED_SUBCOMMANDS: &[&str] = &[
    "blame",
    "changelog",
    "commit",
    "config",
    "diff",
    "docs",
    "doctor",
    "evolution",
    "extended",
    "find",
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

/// Look up the value that follows a `--flag VALUE` pair in `args`.
///
/// Returns the cloned value string if `flag` (or any of its aliases, like
/// `-p` for `--prompt`) appears in `args` and is followed by another token.
/// Returns `None` if the flag is missing or has no value after it.
///
/// Centralizes the `args.iter().position(...).and_then(get(i+1)).cloned()`
/// pattern that's repeated ~16 times across `parse_args`. This is the
/// follow-up to the Day 38 09:55 task that landed `try_dispatch_subcommand`
/// (#261) — see `journals/JOURNAL.md` for the full premise correction.
pub(crate) fn flag_value(args: &[String], flag_names: &[&str]) -> Option<String> {
    args.iter()
        .position(|a| flag_names.contains(&a.as_str()))
        .and_then(|i| args.get(i + 1))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Outcome of checking whether a flag is followed by a real value.
///
/// Pure classifier for `--flag <value>` style arguments. Caller decides how
/// to present the result (warn vs. hard-exit) — this keeps the helper
/// free of I/O so it can be unit-tested in isolation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FlagValueCheck<'a> {
    /// Next token is a usable value.
    Ok(&'a str),
    /// Next token exists but looks like another flag (e.g. `--model --provider ...`).
    /// The caller should surface a warning; not fatal because a leading `-` may
    /// also be a negative number (e.g. `--temperature -0.1`).
    FlagLike(&'a str),
    /// Next token is empty or whitespace-only (e.g. `--model ""` or `--model "  "`).
    Empty,
    /// There is no next token at all (`--model` at end of args).
    Missing,
}

/// Classify the token that follows a flag expecting a value.
///
/// This is the pure validation kernel for the `flags_needing_values` loop in
/// [`parse_args`]. The loop body used to inline this logic, which made it
/// impossible to unit-test directly and left subtle behaviour (negative
/// numbers being valid values, end-of-args being fatal) undocumented.
///
/// Behaviour:
/// - `None` → [`FlagValueCheck::Missing`]
/// - `Some("-")` or `Some("--anything")` → [`FlagValueCheck::FlagLike`]
///   (warning territory, not a hard error — the old code only warned here)
/// - `Some("-5")`, `Some("-0.1")` etc. → [`FlagValueCheck::Ok`]
///   (leading dash followed by a digit is a negative number, not a flag)
/// - anything else → [`FlagValueCheck::Ok`]
pub(crate) fn require_flag_value<'a>(next: Option<&'a String>) -> FlagValueCheck<'a> {
    match next {
        None => FlagValueCheck::Missing,
        Some(v) if v.trim().is_empty() => FlagValueCheck::Empty,
        Some(v) => {
            if v.starts_with('-') && !v.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
                FlagValueCheck::FlagLike(v.as_str())
            } else {
                FlagValueCheck::Ok(v.as_str())
            }
        }
    }
}

/// Build the review argument string from CLI args.
///
/// Handles `yoyo review`, `yoyo review HEAD~3..HEAD`, `yoyo review --pr 123`,
/// and `yoyo review path/to/file.rs`.
fn build_review_arg(args: &[String]) -> String {
    // args[0] = binary, args[1] = "review", args[2..] = review arguments
    if args.len() <= 2 {
        return String::new();
    }
    // Preserve --pr as a single token with its argument
    args[2..].join(" ")
}

/// Resolve API key from flags, env vars, and config file.
/// Returns `Some(key)` or `None` if no key is available.
fn resolve_api_key(args: &[String], provider: &str) -> Option<String> {
    // --api-key flag
    if let Some(key) = flag_value(args, &["--api-key"]) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // Provider-specific env var
    if let Some(env_var) = crate::providers::provider_api_key_env(provider) {
        if let Ok(key) = std::env::var(env_var) {
            if !key.is_empty() {
                return Some(key);
            }
        }
    }

    // Fallback env vars
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    if let Ok(key) = std::env::var("API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // Config file
    let (file_config, _) = load_config_file();
    if let Some(key) = file_config.get("api_key") {
        if !key.is_empty() {
            return Some(key.clone());
        }
    }

    None
}

/// Run the `yoyo review` subcommand — resolve config, build an agent,
/// run the review, and print the result. Returns an exit code (0 or 1).
fn run_review_subcommand(args: &[String], review_arg: &str) -> i32 {
    let (file_config, _) = load_config_file();

    let provider = flag_value(args, &["--provider"])
        .or_else(|| file_config.get("provider").cloned())
        .unwrap_or_else(|| "anthropic".into())
        .to_lowercase();

    let model = flag_value(args, &["--model"])
        .or_else(|| file_config.get("model").cloned())
        .unwrap_or_else(|| default_model_for_provider(&provider));

    let api_key = match resolve_api_key(args, &provider) {
        Some(key) => key,
        None => {
            let env_hint =
                crate::providers::provider_api_key_env(&provider).unwrap_or("ANTHROPIC_API_KEY");
            eprintln!(
                "{RED}error:{RESET} No API key found.\n\
                 Set {env_hint} env var, use --api-key <key>, or add api_key to .yoyo.toml."
            );
            return 1;
        }
    };

    let base_url =
        flag_value(args, &["--base-url"]).or_else(|| file_config.get("base_url").cloned());

    let agent_config = crate::agent_builder::AgentConfig {
        model,
        api_key,
        provider,
        base_url,
        skills: SkillSet::empty(),
        system_prompt: String::new(),
        thinking: yoagent::ThinkingLevel::Off,
        max_tokens: None,
        temperature: None,
        max_turns: None,
        auto_approve: true,
        auto_commit: false,
        permissions: crate::cli::PermissionConfig::default(),
        dir_restrictions: crate::cli::DirectoryRestrictions::default(),
        context_strategy: crate::cli::ContextStrategy::Compaction,
        context_window: None,
        shell_hooks: Vec::new(),
        fallback_provider: None,
        fallback_model: None,
        auto_watch: false,
        allowed_tools: vec![],
        disallowed_tools: vec![],
        no_tools: false,
        lite: false,
        bash_cwd: None,
    };

    // We're inside a tokio runtime (called from parse_args in async main),
    // so use block_in_place + block_on to run the async review.
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(
            crate::commands_git_review::run_non_interactive_review(review_arg, &agent_config),
        )
    });

    match result {
        Ok(review_text) => {
            // Print the clean review text to stdout (for piping)
            println!("{review_text}");
            0
        }
        Err(e) => {
            // Error already printed to stderr by build_review_content
            if e != "nothing to review" {
                eprintln!("{RED}  review failed: {e}{RESET}");
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_flag_value_finds_value_for_single_flag() {
        let args = vec!["yoyo".into(), "--model".into(), "claude-sonnet".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            Some("claude-sonnet".into()),
            "expected to find the value following --model"
        );
    }

    #[test]
    fn test_flag_value_returns_none_when_flag_missing() {
        let args = vec!["yoyo".into(), "--verbose".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            None,
            "expected None when --model is not present"
        );
    }

    #[test]
    fn test_flag_value_returns_none_when_value_missing() {
        // Flag is the last argument — there's no value after it.
        let args = vec!["yoyo".into(), "--model".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            None,
            "expected None when --model has no value after it"
        );
    }

    #[test]
    fn test_flag_value_supports_aliases() {
        // -p is an alias for --prompt; both should resolve.
        let short = vec!["yoyo".into(), "-p".into(), "hello".into()];
        let long = vec!["yoyo".into(), "--prompt".into(), "hello".into()];
        assert_eq!(
            flag_value(&short, &["--prompt", "-p"]),
            Some("hello".into())
        );
        assert_eq!(flag_value(&long, &["--prompt", "-p"]), Some("hello".into()));
    }

    #[test]
    fn test_flag_value_finds_first_occurrence() {
        // If a flag is repeated, take the first value (matches existing
        // .position()-based behavior in parse_args).
        let args = vec![
            "yoyo".into(),
            "--model".into(),
            "first".into(),
            "--model".into(),
            "second".into(),
        ];
        assert_eq!(
            flag_value(&args, &["--model"]),
            Some("first".into()),
            "expected the first --model value (matches prior position-based behavior)"
        );
    }

    #[test]
    fn test_flag_value_returns_none_for_empty_string() {
        // `--model ""` — the shell produces an empty string argument.
        // flag_value should treat this the same as missing.
        let args = vec!["yoyo".into(), "--model".into(), "".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            None,
            "empty string should be treated as missing"
        );
    }

    #[test]
    fn test_flag_value_returns_none_for_whitespace_only() {
        // `--model "  "` — whitespace-only is not a valid model name.
        let args = vec!["yoyo".into(), "--model".into(), "   ".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            None,
            "whitespace-only string should be treated as missing"
        );
    }

    #[test]
    fn test_flag_value_trims_whitespace() {
        // `--model " claude-sonnet "` — leading/trailing whitespace should be trimmed.
        let args = vec!["yoyo".into(), "--model".into(), " claude-sonnet ".into()];
        assert_eq!(
            flag_value(&args, &["--model"]),
            Some("claude-sonnet".into()),
            "leading/trailing whitespace should be trimmed"
        );
    }

    #[test]
    fn test_require_flag_value_empty_is_empty() {
        // `--model ""` — the value exists but is empty. Should be classified as Empty.
        let empty = String::new();
        assert_eq!(
            require_flag_value(Some(&empty)),
            FlagValueCheck::Empty,
            "empty string should classify as Empty"
        );
    }

    #[test]
    fn test_require_flag_value_whitespace_is_empty() {
        // `--model "  "` — whitespace-only should also be Empty.
        let spaces = "   ".to_string();
        assert_eq!(
            require_flag_value(Some(&spaces)),
            FlagValueCheck::Empty,
            "whitespace-only string should classify as Empty"
        );
    }

    #[test]
    fn test_require_flag_value_ok_on_plain_value() {
        let next = "claude-opus-4".to_string();
        assert_eq!(
            require_flag_value(Some(&next)),
            FlagValueCheck::Ok("claude-opus-4"),
            "a plain token should be accepted as the flag's value"
        );
    }

    #[test]
    fn test_require_flag_value_missing_on_end_of_args() {
        assert_eq!(
            require_flag_value(None),
            FlagValueCheck::Missing,
            "None should classify as Missing so the caller can hard-exit"
        );
    }

    #[test]
    fn test_require_flag_value_flag_like_on_double_dash() {
        // The classic bug: `yoyo --model --provider anthropic` — the value slot
        // is occupied by another flag. Should be flagged (warning territory).
        let next = "--provider".to_string();
        assert_eq!(
            require_flag_value(Some(&next)),
            FlagValueCheck::FlagLike("--provider"),
            "a --flag next-token should classify as FlagLike, not Ok"
        );
    }

    #[test]
    fn test_require_flag_value_flag_like_on_bare_dash() {
        // Bare `-` is not a value anywhere in yoyo (no stdin marker). Treat it
        // the same way the old inline code did: warn but don't hard-exit.
        let next = "-".to_string();
        assert_eq!(
            require_flag_value(Some(&next)),
            FlagValueCheck::FlagLike("-"),
            "bare '-' is not a yoyo value and should be flagged"
        );
    }

    #[test]
    fn test_require_flag_value_accepts_negative_numbers() {
        // `--temperature -0.1` is a real use case — leading `-` followed by a
        // digit is a negative number, not a flag. This is the exact invariant
        // the old inline regex-free check was protecting; pinning it in a test
        // so a future refactor can't quietly break temperature/top-p flags.
        let negative = "-0.1".to_string();
        assert_eq!(
            require_flag_value(Some(&negative)),
            FlagValueCheck::Ok("-0.1"),
            "negative numbers must survive as plain values"
        );

        let neg_int = "-5".to_string();
        assert_eq!(
            require_flag_value(Some(&neg_int)),
            FlagValueCheck::Ok("-5"),
            "negative integers must survive as plain values"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_help_long() {
        // --help should be dispatched (returns Some(None) — handled, parse_args returns None)
        let args = vec!["yoyo".into(), "--help".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for --help"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_help_short() {
        // -h alias should also dispatch
        let args = vec!["yoyo".into(), "-h".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(matches!(result, Some(None)), "expected Some(None) for -h");
    }

    #[test]
    fn test_try_dispatch_subcommand_version_long() {
        let args = vec!["yoyo".into(), "--version".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for --version"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_version_short() {
        let args = vec!["yoyo".into(), "-V".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(matches!(result, Some(None)), "expected Some(None) for -V");
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_unknown_flag() {
        // An unknown flag should NOT be dispatched as a subcommand —
        // returns None so parse_args continues to flag parsing.
        let args = vec!["yoyo".into(), "--unknown-flag".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(result.is_none(), "expected None for --unknown-flag");
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_empty_args() {
        // Empty args list should fall through (no subcommand to dispatch).
        let args: Vec<String> = vec![];
        let result = try_dispatch_subcommand(&args);
        assert!(result.is_none(), "expected None for empty args");
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_normal_flags() {
        // Normal flag combinations should fall through to parse_args's main loop.
        let args = vec![
            "yoyo".into(),
            "--model".into(),
            "claude-sonnet-4-5".into(),
            "--prompt".into(),
            "hello".into(),
        ];
        let result = try_dispatch_subcommand(&args);
        assert!(result.is_none(), "expected None for normal flag combo");
    }

    #[test]
    fn test_try_dispatch_subcommand_help_wins_over_other_flags() {
        // If --help appears anywhere in the args, it should still dispatch.
        let args = vec![
            "yoyo".into(),
            "--model".into(),
            "claude-sonnet-4-5".into(),
            "--help".into(),
        ];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected --help to dispatch even with other flags"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_falls_through_on_unknown_subcommand() {
        // Regression guard for the doctor/health wiring (Day 47): unknown
        // positional subcommands must still fall through to flag parsing.
        // If we accidentally swallow them in try_dispatch_subcommand, every
        // positional token (e.g. a stray filename) would silently exit yoyo.
        let args = vec!["yoyo".into(), "not-a-real-subcommand".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            result.is_none(),
            "expected None for an unknown positional subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_help_bare() {
        // `yoyo help` (bare word, no dashes) should dispatch the same as --help.
        let args = vec!["yoyo".into(), "help".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `help` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_version_bare() {
        // `yoyo version` (bare word) should dispatch the same as --version.
        let args = vec!["yoyo".into(), "version".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `version` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_setup_bare() {
        // `yoyo setup` should dispatch the setup wizard (returns Some(None)).
        let args = vec!["yoyo".into(), "setup".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `setup` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_init_bare() {
        // `yoyo init` should dispatch the init handler (returns Some(None)).
        let args = vec!["yoyo".into(), "init".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `init` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_lint() {
        let args = vec!["yoyo".into(), "lint".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `lint` subcommand"
        );
    }

    #[test]
    #[ignore] // Runs `cargo test` recursively — verified manually, skip in CI
    fn test_try_dispatch_subcommand_test() {
        let args = vec!["yoyo".into(), "test".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `test` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_tree() {
        let args = vec!["yoyo".into(), "tree".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `tree` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_map() {
        let args = vec!["yoyo".into(), "map".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `map` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_run_no_args() {
        // `yoyo run` with no command should still dispatch (shows usage).
        let args = vec!["yoyo".into(), "run".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `run` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_diff() {
        let args = vec!["yoyo".into(), "diff".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `diff` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_commit() {
        // `yoyo commit` with no message should still dispatch (shows "nothing staged" or similar).
        let args = vec!["yoyo".into(), "commit".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `commit` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_blame() {
        // `yoyo blame` with no file should still dispatch (shows error message).
        let args = vec!["yoyo".into(), "blame".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `blame` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_risk_bare() {
        // `yoyo risk` (bare, read-only) dispatches to handle_risk, which reads
        // the source tree and prints risk scores. Assert only the routing
        // decision — don't invoke the snapshot-writing path here.
        let args = vec!["yoyo".into(), "risk".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `risk` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_risk_near_miss_falls_through() {
        // Paired negative: the discriminator must stay silent on a near-miss.
        // `risky` differs from `risk` by one char and must NOT be swallowed —
        // it falls through to flag parsing (returns None), so a stray token
        // never silently exits yoyo.
        let args = vec!["yoyo".into(), "risky".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            result.is_none(),
            "expected None for near-miss `risky` (must not be swallowed by the risk arm)"
        );
    }

    #[test]
    fn test_build_risk_input_snapshot_contract() {
        // Contract test for the `yoyo risk snapshot` CLI path the evolve
        // harness's paste-diff depends on. The dispatch arm reconstructs the
        // `/risk <sub>` string that `handle_risk` routes on; a wrong mapping
        // here silently sends the snapshot to nowhere. We assert the pure
        // reconstruction (no `.yoyo/` write) so the contract is locked without
        // triggering the real snapshot-append side effect.
        let snapshot_tail = vec!["snapshot".to_string()];
        assert_eq!(
            build_risk_input(&snapshot_tail),
            "/risk snapshot",
            "`yoyo risk snapshot` must reconstruct exactly `/risk snapshot`"
        );

        // Bare `yoyo risk` (empty tail) → `/risk` (read-only score display).
        assert_eq!(
            build_risk_input(&[]),
            "/risk",
            "bare `yoyo risk` must reconstruct `/risk`"
        );

        // A sibling writing subcommand routes the same way.
        let validate_tail = vec!["validate".to_string()];
        assert_eq!(build_risk_input(&validate_tail), "/risk validate");

        // Flags after the subcommand are preserved verbatim.
        let flagged = vec!["--all".to_string()];
        assert_eq!(build_risk_input(&flagged), "/risk --all");
    }

    #[test]
    fn test_build_risk_input_validate_contract() {
        // Contract test for the `yoyo risk validate` CLI path that the evolve
        // harness's wrap-up patch (issue: "wire risk validate into evolve.sh")
        // depends on. The harness will call `$YOYO_BIN risk validate` once per
        // session so the prediction meter's *validation* half accumulates the
        // same way `risk snapshot` (#575) feeds the snapshot half.
        //
        // `handle_risk` routes on the reconstructed `/risk <sub>` string, so if
        // this mapping ever drifts the harness call silently writes nothing to
        // `.yoyo/risk_validations.jsonl` and the meter stays starved. We assert
        // the pure reconstruction only (no `.yoyo/` write side effect), so the
        // contract is locked without invoking the real validate dispatch.
        let validate_tail = vec!["validate".to_string()];
        assert_eq!(
            build_risk_input(&validate_tail),
            "/risk validate",
            "`yoyo risk validate` must reconstruct exactly `/risk validate`"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_grep() {
        let args = vec!["yoyo".into(), "grep".into(), "TODO".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `grep` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_find() {
        let args = vec!["yoyo".into(), "find".into(), "main".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `find` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_index() {
        let args = vec!["yoyo".into(), "index".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `index` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_update() {
        let args = vec!["yoyo".into(), "update".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `update` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_docs() {
        let args = vec!["yoyo".into(), "docs".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `docs` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_watch() {
        // `yoyo watch status` should dispatch (shows current watch state).
        let args = vec!["yoyo".into(), "watch".into(), "status".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `watch` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_status() {
        let args = vec!["yoyo".into(), "status".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `status` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_undo() {
        // Bare `yoyo undo` with no session — should dispatch (shows fallback message).
        let args = vec!["yoyo".into(), "undo".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `undo` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_changelog() {
        let args = vec!["yoyo".into(), "changelog".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `changelog` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_changelog_with_count() {
        let args = vec!["yoyo".into(), "changelog".into(), "20".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `changelog 20` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_config() {
        let args = vec!["yoyo".into(), "config".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `config` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_config_show() {
        let args = vec!["yoyo".into(), "config".into(), "show".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `config show` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_config_unknown() {
        // Unknown config subcommands still dispatch (print a message, don't hang)
        let args = vec!["yoyo".into(), "config".into(), "edit".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `config edit` (requires session message)"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_permissions() {
        let args = vec!["yoyo".into(), "permissions".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `permissions` subcommand"
        );
    }

    #[test]
    fn test_permissions_subcommand_routes_through_the_project_gate() {
        // #749 item 3: `yoyo permissions` parses the raw config independently,
        // so it is a *second* entry point that must apply the same gate — a
        // per-token pass is not a per-entry-point pass. This view is where a
        // user goes to check what is in force; showing a refused `allow`
        // pattern as active would be a lie exactly there. Source-level pin so
        // a later refactor of the arm cannot silently drop the gate.
        let src =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch_sub.rs"))
                .expect("read own source");
        let marker = "\"permissions\" =>";
        let arm_start = src.find(marker).expect("permissions arm exists");
        // Bound the search to this arm, not the whole file.
        let arm = &src[arm_start..];
        let arm_end = arm.find("\"todo\" =>").unwrap_or(arm.len());
        let arm = &arm[..arm_end];
        assert!(
            arm.contains("gate_project_permissions"),
            "the `permissions` arm must route through cli::gate_project_permissions"
        );
        assert!(
            arm.contains("project_permission_refusal_message"),
            "a refusal in this arm must be announced, never silent"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_todo() {
        let args = vec!["yoyo".into(), "todo".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for bare `todo` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_todo_list() {
        let args = vec!["yoyo".into(), "todo".into(), "list".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `todo list` subcommand"
        );
    }

    #[test]
    fn test_try_dispatch_subcommand_memories() {
        let args = vec!["yoyo".into(), "memories".into()];
        let result = try_dispatch_subcommand(&args);
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `memories` subcommand"
        );
    }

    #[test]
    fn join_args_multi_word_has_no_quote_chars() {
        // #679 finding 2: `yoyo todo add "fix the parser"` must not store
        // literal quote characters — handle_todo takes the remainder verbatim.
        let args: Vec<String> = vec!["yoyo", "todo", "add", "fix the parser"]
            .into_iter()
            .map(String::from)
            .collect();
        let input = join_args_as_command(&args);
        assert_eq!(input, "/todo add fix the parser");
        assert!(
            !input.contains('"'),
            "plain join must not introduce quote chars"
        );
    }

    #[test]
    fn join_args_single_word_unchanged() {
        let args: Vec<String> = vec!["yoyo", "todo", "add", "refactor"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(join_args_as_command(&args), "/todo add refactor");
    }

    #[test]
    fn join_args_goal_set_multi_word() {
        // Helper-level check for the goal arm's input shape; the routing-level
        // proof lives in goal_set_routing_stores_multi_word_goal_without_quotes.
        let args: Vec<String> = vec!["yoyo", "goal", "set", "ship the parser fix"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(join_args_as_command(&args), "/goal set ship the parser fix");
    }

    #[test]
    #[serial]
    fn goal_set_routing_stores_multi_word_goal_without_quotes() {
        // #679 finding 2, twin arm (Day 151 sweep): the `goal` CLI arm must
        // plain-join, not re-quote, because handle_goal's `set` takes its
        // remainder verbatim (no tokenize_quoted in commands_goal.rs).
        // End-to-end through try_dispatch_subcommand: what lands in
        // .yoyo/goal.md must be the words, never `"words"`.
        let tmp = tempfile::TempDir::new().expect("create temp dir");
        let prev = std::env::current_dir().expect("get cwd");
        std::env::set_current_dir(tmp.path()).expect("set cwd");
        let args: Vec<String> = vec!["yoyo", "goal", "set", "ship the parser fix"]
            .into_iter()
            .map(String::from)
            .collect();
        let result = try_dispatch_subcommand(&args);
        let stored = crate::commands_goal::load_goal();
        // Restore cwd before asserting so a failure can't leak the temp cwd
        // into other tests.
        std::env::set_current_dir(prev).expect("restore cwd");
        assert!(
            matches!(result, Some(None)),
            "expected Some(None) for `goal set` subcommand"
        );
        let stored = stored.expect("goal should have been saved");
        assert_eq!(stored, "ship the parser fix");
        assert!(
            !stored.contains('"'),
            "stored goal must not contain literal quote chars, got: {stored}"
        );
    }

    #[test]
    fn quote_args_simple() {
        let args: Vec<String> = vec!["yoyo", "grep", "TODO"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), "/grep TODO");
    }

    #[test]
    fn quote_args_multi_word() {
        let args: Vec<String> = vec!["yoyo", "grep", "fn main"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), r#"/grep "fn main""#);
    }

    #[test]
    fn quote_args_multi_word_with_path() {
        let args: Vec<String> = vec!["yoyo", "grep", "fn main", "src/"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), r#"/grep "fn main" src/"#);
    }

    #[test]
    fn quote_args_no_unnecessary_quoting() {
        let args: Vec<String> = vec!["yoyo", "diff", "--staged"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), "/diff --staged");
    }

    #[test]
    fn quote_args_tab_in_arg() {
        let args: Vec<String> = vec!["yoyo", "grep", "has\ttab"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(quote_args_as_command(&args), "/grep \"has\ttab\"");
    }

    #[test]
    fn test_build_review_arg_empty() {
        let args: Vec<String> = vec!["yoyo", "review"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(build_review_arg(&args), "");
    }

    #[test]
    fn test_build_review_arg_commit_range() {
        let args: Vec<String> = vec!["yoyo", "review", "HEAD~3..HEAD"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(build_review_arg(&args), "HEAD~3..HEAD");
    }

    #[test]
    fn test_build_review_arg_pr_flag() {
        let args: Vec<String> = vec!["yoyo", "review", "--pr", "123"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(build_review_arg(&args), "--pr 123");
    }

    #[test]
    fn test_build_review_arg_file() {
        let args: Vec<String> = vec!["yoyo", "review", "src/main.rs"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(build_review_arg(&args), "src/main.rs");
    }

    #[test]
    #[serial]
    fn test_resolve_api_key_from_env() {
        // This tests the env var fallback chain — set a test var and verify
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test-review");
        let args: Vec<String> = vec!["yoyo".into(), "review".into()];
        let key = resolve_api_key(&args, "anthropic");
        assert_eq!(key, Some("sk-test-review".to_string()));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    #[serial]
    fn test_resolve_api_key_flag_overrides_env() {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-from-env");
        let args: Vec<String> = vec![
            "yoyo".into(),
            "review".into(),
            "--api-key".into(),
            "sk-from-flag".into(),
        ];
        let key = resolve_api_key(&args, "anthropic");
        assert_eq!(key, Some("sk-from-flag".to_string()));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_strip_flag_with_value_removes_skills_flag() {
        let args: Vec<String> = vec!["skill", "list", "--skills", "./skills"]
            .into_iter()
            .map(String::from)
            .collect();
        let filtered = strip_flag_with_value(&args, "--skills");
        assert_eq!(filtered, vec!["skill", "list"]);
    }

    #[test]
    fn test_strip_flag_with_value_preserves_other_args() {
        let args: Vec<String> = vec!["skill", "show", "my-skill", "--skills", "./skills"]
            .into_iter()
            .map(String::from)
            .collect();
        let filtered = strip_flag_with_value(&args, "--skills");
        assert_eq!(filtered, vec!["skill", "show", "my-skill"]);
    }

    #[test]
    fn test_strip_flag_with_value_no_flag_unchanged() {
        let args: Vec<String> = vec!["skill", "list"]
            .into_iter()
            .map(String::from)
            .collect();
        let filtered = strip_flag_with_value(&args, "--skills");
        assert_eq!(filtered, vec!["skill", "list"]);
    }

    #[test]
    fn test_strip_flag_with_value_multiple_flags() {
        let args: Vec<String> = vec!["skill", "list", "--skills", "./a", "--skills", "./b"]
            .into_iter()
            .map(String::from)
            .collect();
        let filtered = strip_flag_with_value(&args, "--skills");
        assert_eq!(filtered, vec!["skill", "list"]);
    }

    #[test]
    fn test_skill_subcommand_strips_skills_flag_from_input() {
        // Simulates what the "skill" match arm does: strip --skills, then
        // build the /command string that handle_skill receives.
        let args: Vec<String> = vec!["skill", "list", "--skills", "./skills"]
            .into_iter()
            .map(String::from)
            .collect();
        let filtered = strip_flag_with_value(&args, "--skills");
        let input = quote_args_as_command(&filtered);
        assert_eq!(input, "/list");
    }

    // ---- bare-word near-miss guard (`yoyo tokens`, `yoyo statsu`) ----

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
