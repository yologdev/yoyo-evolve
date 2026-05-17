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
                crate::commands_lint::handle_test();
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
                let input = quote_args_as_command(args);
                let skill_dirs = collect_repeatable_flag(args, "--skills");
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
                let permissions = crate::config::parse_permissions_from_config(&raw_config);
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
                let input = quote_args_as_command(args);
                let output = crate::commands_todo::handle_todo(&input);
                println!("{output}");
                return Some(None);
            }
            "goal" => {
                let input = quote_args_as_command(args);
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
                let input = quote_args_as_command(args);
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
        .cloned()
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
        disallowed_tools: vec![],
        no_tools: false,
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
    fn test_resolve_api_key_from_env() {
        // This tests the env var fallback chain — set a test var and verify
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test-review");
        let args: Vec<String> = vec!["yoyo".into(), "review".into()];
        let key = resolve_api_key(&args, "anthropic");
        assert_eq!(key, Some("sk-test-review".to_string()));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
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
}
