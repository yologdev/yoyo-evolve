//! CLI subcommand dispatch — early-exit handlers for `yoyo <subcommand>` and
//! REPL slash-command routing.
//!
//! Extracted from `cli.rs` to keep that module focused on config/flag parsing.
//! The [`try_dispatch_subcommand`] function is called by [`crate::cli::parse_args`]
//! before any flag parsing begins. If a known subcommand matches, the handler
//! runs and returns `Some(None)` to signal "handled, exit cleanly".
//!
//! The [`dispatch_command`] function routes `/`-prefixed REPL commands to their
//! handlers. It was extracted from `repl.rs` to keep the REPL loop focused on
//! readline mechanics and the command table easy to navigate.

use std::time::Instant;

use crate::cli::{
    collect_repeatable_flag, effective_context_tokens, load_config_file, parse_thinking_level,
    print_help, Config, McpServerConfig, VERSION,
};
use crate::commands::{
    self, auto_compact_if_needed, clear_confirmation_message, is_unknown_command,
    reset_compact_thrash, suggest_command, thinking_level_name,
};
use crate::format::*;
use crate::prompt::*;
use crate::providers::default_model_for_provider;
use crate::AgentConfig;
use yoagent::context::total_tokens;
use yoagent::skills::SkillSet;
use yoagent::*;

/// Result of dispatching a slash command in the REPL.
pub(crate) enum CommandResult {
    /// Command handled, go to next prompt.
    Continue,
    /// User wants to exit.
    Quit,
    /// Command produced a prompt to send to the agent.
    SendToAgent(String),
    /// Input isn't a slash command, fall through to agent.
    NotACommand,
}

/// Bundles the full REPL state needed by slash-command dispatch.
///
/// Introduced to replace the 20-parameter `dispatch_command` signature — adding
/// new state no longer requires touching every call-site.
pub(crate) struct DispatchContext<'a> {
    pub input: &'a str,
    pub agent: &'a mut yoagent::agent::Agent,
    pub agent_config: &'a mut AgentConfig,
    pub session_total: &'a mut Usage,
    pub session_changes: &'a SessionChanges,
    pub turn_history: &'a mut TurnHistory,
    pub bg_tracker: &'a commands::BackgroundJobTracker,
    pub spawn_tracker: &'a commands::SpawnTracker,
    pub undo_context: &'a mut Option<String>,
    pub last_input: &'a mut Option<String>,
    pub last_error: &'a mut Option<String>,
    pub bookmarks: &'a mut commands::Bookmarks,
    pub checkpoint_store: &'a mut commands::CheckpointStore,
    pub session_start: Instant,
    pub turn_count: usize,
    pub cwd: &'a str,
    pub mcp_cli_servers: &'a [String],
    pub mcp_server_configs: &'a [McpServerConfig],
    pub mcp_count: u32,
    pub openapi_count: u32,
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
                crate::commands_dev::handle_lint(&input);
                return Some(None);
            }
            "test" => {
                crate::commands_dev::handle_test();
                return Some(None);
            }
            "tree" => {
                let input = quote_args_as_command(args);
                crate::commands_dev::handle_tree(&input);
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
                crate::commands_dev::handle_run(&input);
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
                // handle_review is async and needs an agent — for bare
                // subcommand, gather the content and print the review prompt
                // so the user can see what would be sent to the model.
                let input = quote_args_as_command(args);
                let arg = input.strip_prefix("/review").unwrap_or("").trim();
                match crate::commands_git::build_review_content(arg) {
                    Some((label, content)) => {
                        let prompt = crate::commands_git::build_review_prompt(&label, &content);
                        println!("{prompt}");
                    }
                    None => {
                        // build_review_content already printed the error/status
                    }
                }
                return Some(None);
            }
            "blame" => {
                let input = quote_args_as_command(args);
                crate::commands_git::handle_blame(&input);
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
                if let Err(e) = crate::commands_dev::handle_update() {
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
                crate::commands_project::handle_skill(&input, &skills);
                return Some(None);
            }
            "watch" => {
                let input = quote_args_as_command(args);
                crate::commands_dev::handle_watch(&input);
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
                let mut history = crate::prompt::TurnHistory::new();
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
                let output = crate::commands_project::handle_todo(&input);
                println!("{output}");
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

/// Dispatch a slash command entered at the REPL prompt.
///
/// Handles all `/`-prefixed commands, returning a [`CommandResult`] that tells
/// the main loop what to do next.  This was extracted from `run_repl` to keep
/// the outer loop small and the command table easy to navigate.
pub(crate) async fn dispatch_command(ctx: &mut DispatchContext<'_>) -> CommandResult {
    match ctx.input {
        "/quit" | "/exit" => CommandResult::Quit,
        s if s == "/help" || s.starts_with("/help ") => {
            if !commands::handle_help_command(s) {
                commands::handle_help();
            }
            CommandResult::Continue
        }
        "/version" => {
            commands::handle_version();
            CommandResult::Continue
        }
        "/status" => {
            let ctx_used = total_tokens(ctx.agent.messages()) as u64;
            let ctx_max = effective_context_tokens();
            commands::handle_status(
                &ctx.agent_config.model,
                ctx.cwd,
                ctx.session_total,
                ctx.session_start.elapsed(),
                ctx.turn_count,
                ctx_used,
                ctx_max,
            );
            CommandResult::Continue
        }
        "/tokens" => {
            commands::handle_tokens(ctx.agent, ctx.session_total, &ctx.agent_config.model);
            CommandResult::Continue
        }
        "/cost" => {
            commands::handle_cost(
                ctx.session_total,
                &ctx.agent_config.model,
                ctx.agent.messages(),
            );
            CommandResult::Continue
        }
        "/profile" => {
            commands::handle_profile(
                ctx.agent,
                &ctx.agent_config.model,
                &ctx.agent_config.provider,
                ctx.session_start,
                ctx.session_total,
            );
            CommandResult::Continue
        }
        s if s == "/changelog" || s.starts_with("/changelog ") => {
            commands::handle_changelog(ctx.input);
            CommandResult::Continue
        }
        s if s == "/evolution" || s.starts_with("/evolution ") => {
            commands::handle_evolution(ctx.input);
            CommandResult::Continue
        }
        "/clear" => {
            let messages = ctx.agent.messages();
            let msg_count = messages.len();
            let token_count = yoagent::context::total_tokens(messages) as u64;
            if let Some(prompt) = clear_confirmation_message(msg_count, token_count) {
                use std::io::Write;
                print!("{DIM}  {prompt}{RESET}");
                let _ = std::io::stdout().flush();
                let mut answer = String::new();
                if std::io::stdin().read_line(&mut answer).is_ok() {
                    let answer = answer.trim().to_lowercase();
                    if answer != "y" && answer != "yes" {
                        println!("{DIM}  (clear cancelled){RESET}\n");
                        return CommandResult::Continue;
                    }
                } else {
                    println!("{DIM}  (clear cancelled){RESET}\n");
                    return CommandResult::Continue;
                }
            }
            *ctx.agent = ctx.agent_config.build_agent();
            ctx.session_changes.clear();
            ctx.turn_history.clear();
            reset_compact_thrash();
            reset_context_budget_warning();
            println!("{DIM}  (conversation cleared){RESET}\n");
            CommandResult::Continue
        }
        "/clear!" => {
            *ctx.agent = ctx.agent_config.build_agent();
            ctx.session_changes.clear();
            ctx.turn_history.clear();
            reset_compact_thrash();
            reset_context_budget_warning();
            println!("{DIM}  (conversation force-cleared){RESET}\n");
            CommandResult::Continue
        }
        "/model" => {
            commands::handle_model_show(&ctx.agent_config.model);
            CommandResult::Continue
        }
        s if s.starts_with("/model ") => {
            let new_model = s.trim_start_matches("/model ").trim();
            if new_model.is_empty() {
                println!("{DIM}  current model: {}", ctx.agent_config.model);
                println!("  usage: /model <name>{RESET}\n");
                return CommandResult::Continue;
            }
            ctx.agent_config.model = new_model.to_string();
            // Rebuild ctx.agent with new model, preserving conversation
            let saved = ctx.agent.save_messages().ok();
            *ctx.agent = ctx.agent_config.build_agent();
            let restored = if let Some(json) = saved {
                ctx.agent.restore_messages(&json).is_ok()
            } else {
                false
            };
            if restored {
                println!("{DIM}  (switched to {new_model}, conversation preserved){RESET}\n");
            } else {
                println!("{YELLOW}  (switched to {new_model}, conversation could not be preserved){RESET}\n");
            }
            CommandResult::Continue
        }
        "/provider" => {
            commands::handle_provider_show(&ctx.agent_config.provider);
            CommandResult::Continue
        }
        s if s.starts_with("/provider ") => {
            let new_provider = s.trim_start_matches("/provider ").trim();
            if new_provider.is_empty() {
                commands::handle_provider_show(&ctx.agent_config.provider);
                return CommandResult::Continue;
            }
            commands::handle_provider_switch(new_provider, ctx.agent_config, ctx.agent);
            CommandResult::Continue
        }
        "/think" => {
            commands::handle_think_show(ctx.agent_config.thinking);
            CommandResult::Continue
        }
        s if s.starts_with("/think ") => {
            let level_str = s.trim_start_matches("/think ").trim();
            if level_str.is_empty() {
                let current = thinking_level_name(ctx.agent_config.thinking);
                println!("{DIM}  thinking: {current}");
                println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
                return CommandResult::Continue;
            }
            let new_thinking = parse_thinking_level(level_str);
            if new_thinking == ctx.agent_config.thinking {
                let current = thinking_level_name(ctx.agent_config.thinking);
                println!("{DIM}  thinking already set to {current}{RESET}\n");
                return CommandResult::Continue;
            }
            ctx.agent_config.thinking = new_thinking;
            // Rebuild ctx.agent with new thinking level, preserving conversation
            let saved = ctx.agent.save_messages().ok();
            *ctx.agent = ctx.agent_config.build_agent();
            let restored = if let Some(json) = saved {
                ctx.agent.restore_messages(&json).is_ok()
            } else {
                false
            };
            let level_name = thinking_level_name(ctx.agent_config.thinking);
            if restored {
                println!("{DIM}  (thinking set to {level_name}, conversation preserved){RESET}\n");
            } else {
                println!("{YELLOW}  (thinking set to {level_name}, conversation could not be preserved){RESET}\n");
            }
            CommandResult::Continue
        }
        s if s == "/save" || s.starts_with("/save ") => {
            commands::handle_save(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        s if s == "/load" || s.starts_with("/load ") => {
            commands::handle_load(ctx.agent, ctx.input);
            reset_compact_thrash();
            CommandResult::Continue
        }
        s if s == "/stash" || s.starts_with("/stash ") => {
            let result = commands::handle_stash(ctx.agent, s);
            print!("{result}");
            CommandResult::Continue
        }
        s if s == "/checkpoint" || s.starts_with("/checkpoint ") => {
            commands::handle_checkpoint(s, ctx.checkpoint_store, ctx.session_changes);
            CommandResult::Continue
        }
        s if s == "/diff" || s.starts_with("/diff ") => {
            commands::handle_diff(s);
            CommandResult::Continue
        }
        s if s == "/blame" || s.starts_with("/blame ") => {
            commands::handle_blame(s);
            CommandResult::Continue
        }
        s if s == "/undo" || s.starts_with("/undo ") => {
            if let Some(undo_ctx) = commands::handle_undo(s, ctx.turn_history) {
                *ctx.undo_context = Some(undo_ctx);
            }
            CommandResult::Continue
        }
        "/health" => {
            commands::handle_health();
            CommandResult::Continue
        }
        "/doctor" => {
            commands::handle_doctor(&ctx.agent_config.provider, &ctx.agent_config.model);
            CommandResult::Continue
        }
        "/test" => {
            commands::handle_test();
            CommandResult::Continue
        }
        "/lint fix" => {
            if let Some(fix_prompt) =
                commands::handle_lint_fix(ctx.agent, ctx.session_total, &ctx.agent_config.model)
                    .await
            {
                *ctx.last_input = Some(fix_prompt);
            }
            CommandResult::Continue
        }
        s if s == "/lint" || s.starts_with("/lint ") => {
            if let Some(lint_result) = commands::handle_lint(s) {
                if lint_result.starts_with("Lint FAILED")
                    || lint_result.starts_with("Failed to run")
                {
                    *ctx.last_input = Some(lint_result);
                }
            }
            CommandResult::Continue
        }
        "/fix" => {
            if let Some(fix_prompt) =
                commands::handle_fix(ctx.agent, ctx.session_total, &ctx.agent_config.model).await
            {
                *ctx.last_input = Some(fix_prompt);
            }
            CommandResult::Continue
        }
        "/history" => {
            commands::handle_history(ctx.agent);
            CommandResult::Continue
        }
        "/search" => {
            commands::handle_search(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        s if s.starts_with("/search ") => {
            commands::handle_search(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        "/marks" => {
            commands::handle_marks(ctx.bookmarks);
            CommandResult::Continue
        }
        s if s == "/changes" || s.starts_with("/changes ") => {
            commands::handle_changes(ctx.session_changes, ctx.input);
            CommandResult::Continue
        }
        s if s == "/export" || s.starts_with("/export ") => {
            commands::handle_export(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        s if s == "/mark" || s.starts_with("/mark ") => {
            commands::handle_mark(ctx.agent, ctx.input, ctx.bookmarks);
            CommandResult::Continue
        }
        s if s == "/jump" || s.starts_with("/jump ") => {
            commands::handle_jump(ctx.agent, ctx.input, ctx.bookmarks);
            CommandResult::Continue
        }
        "/config" => {
            commands::handle_config(
                &ctx.agent_config.provider,
                &ctx.agent_config.model,
                &ctx.agent_config.base_url,
                ctx.agent_config.thinking,
                ctx.agent_config.max_tokens,
                ctx.agent_config.max_turns,
                ctx.agent_config.temperature,
                &ctx.agent_config.skills,
                &ctx.agent_config.system_prompt,
                ctx.mcp_count,
                ctx.openapi_count,
                ctx.agent_config.shell_hooks.len(),
                ctx.agent,
                ctx.cwd,
            );
            CommandResult::Continue
        }
        s if s == "/config show" || s.starts_with("/config show ") => {
            commands::handle_config_show();
            CommandResult::Continue
        }
        s if s == "/config edit" || s.starts_with("/config edit ") => {
            commands::handle_config_edit();
            CommandResult::Continue
        }
        s if s.starts_with("/config set") => {
            commands::handle_config_set(ctx.input, ctx.agent_config, ctx.agent);
            CommandResult::Continue
        }
        s if s == "/config get" || s.starts_with("/config get ") => {
            commands::handle_config_get(ctx.input);
            CommandResult::Continue
        }
        "/hooks" => {
            commands::handle_hooks(&ctx.agent_config.shell_hooks);
            CommandResult::Continue
        }
        "/permissions" => {
            commands::handle_permissions(
                ctx.agent_config.auto_approve,
                &ctx.agent_config.permissions,
                &ctx.agent_config.dir_restrictions,
            );
            CommandResult::Continue
        }
        "/compact" => {
            commands::handle_compact(ctx.agent);
            CommandResult::Continue
        }
        s if s == "/commit" || s.starts_with("/commit ") => {
            commands::handle_commit(ctx.input);
            CommandResult::Continue
        }
        s if s == "/context" || s.starts_with("/context ") => {
            commands::handle_context(ctx.input, &ctx.agent_config.system_prompt, ctx.agent);
            CommandResult::Continue
        }
        s if s == "/add" || s.starts_with("/add ") => {
            let results = commands::handle_add(ctx.input);
            if !results.is_empty() {
                // Print summaries
                for result in &results {
                    match result {
                        commands::AddResult::Text { summary, .. } => println!("{summary}"),
                        commands::AddResult::Image { summary, .. } => println!("{summary}"),
                    }
                }
                // Build content blocks with proper text context for images
                let content_blocks = crate::repl::build_add_content_blocks(&results);
                let word = crate::format::pluralize(results.len(), "file", "files");
                println!(
                    "{}  ({} {word} added to conversation){}\n",
                    DIM,
                    results.len(),
                    RESET
                );
                // Inject as a user message so the AI sees the file contents
                let msg = yoagent::types::AgentMessage::Llm(yoagent::types::Message::User {
                    content: content_blocks,
                    timestamp: yoagent::types::now_ms(),
                });
                ctx.agent.append_message(msg);
            }
            CommandResult::Continue
        }
        "/docs" => {
            commands::handle_docs(ctx.input);
            CommandResult::Continue
        }
        s if s.starts_with("/docs ") => {
            commands::handle_docs(ctx.input);
            CommandResult::Continue
        }
        "/find" => {
            commands::handle_find(ctx.input);
            CommandResult::Continue
        }
        s if s.starts_with("/find ") => {
            commands::handle_find(ctx.input);
            CommandResult::Continue
        }
        "/grep" => {
            commands::handle_grep(ctx.input);
            CommandResult::Continue
        }
        s if s.starts_with("/grep ") => {
            commands::handle_grep(ctx.input);
            CommandResult::Continue
        }
        "/init" => {
            commands::handle_init();
            CommandResult::Continue
        }
        s if s == "/rename" || s.starts_with("/rename ") => {
            commands::handle_rename(ctx.input);
            CommandResult::Continue
        }
        s if s == "/extract" || s.starts_with("/extract ") => {
            commands::handle_extract(ctx.input);
            CommandResult::Continue
        }
        s if s == "/move" || s.starts_with("/move ") => {
            commands::handle_move(ctx.input);
            CommandResult::Continue
        }
        s if s == "/refactor" || s.starts_with("/refactor ") => {
            commands::handle_refactor(ctx.input);
            CommandResult::Continue
        }
        s if s == "/remember" || s.starts_with("/remember ") => {
            commands::handle_remember(ctx.input);
            CommandResult::Continue
        }
        s if s == "/memories" || s.starts_with("/memories ") => {
            commands::handle_memories(ctx.input);
            CommandResult::Continue
        }
        s if s == "/forget" || s.starts_with("/forget ") => {
            commands::handle_forget(ctx.input);
            CommandResult::Continue
        }
        "/index" => {
            commands::handle_index();
            CommandResult::Continue
        }
        s if s == "/map" || s.starts_with("/map ") => {
            commands::handle_map(ctx.input);
            CommandResult::Continue
        }
        s if s == "/outline" || s.starts_with("/outline ") => {
            commands::handle_outline(ctx.input);
            CommandResult::Continue
        }
        "/retry" => {
            *ctx.last_error = commands::handle_retry(
                ctx.agent,
                ctx.last_input,
                ctx.last_error,
                ctx.session_total,
                &ctx.agent_config.model,
            )
            .await;
            CommandResult::Continue
        }
        s if s == "/tree" || s.starts_with("/tree ") => {
            commands::handle_tree(ctx.input);
            CommandResult::Continue
        }
        s if s == "/web" || s.starts_with("/web ") => {
            commands::handle_web(ctx.input);
            CommandResult::Continue
        }
        s if s == "/watch" || s.starts_with("/watch ") => {
            commands::handle_watch(ctx.input);
            CommandResult::Continue
        }
        s if s == "/todo" || s.starts_with("/todo ") => {
            let result = commands::handle_todo(ctx.input);
            println!("{result}\n");
            CommandResult::Continue
        }
        s if s == "/teach" || s.starts_with("/teach ") => {
            commands::handle_teach(ctx.input);
            CommandResult::Continue
        }
        s if s == "/mcp" || s.starts_with("/mcp ") => {
            commands::handle_mcp(
                ctx.input,
                ctx.mcp_cli_servers,
                ctx.mcp_server_configs,
                ctx.mcp_count,
            );
            CommandResult::Continue
        }
        s if s == "/ast" || s.starts_with("/ast ") => {
            commands::handle_ast_grep(ctx.input);
            CommandResult::Continue
        }
        s if s == "/apply" || s.starts_with("/apply ") => {
            commands::handle_apply(ctx.input);
            CommandResult::Continue
        }
        s if s == "/bg" || s.starts_with("/bg ") => {
            let args = ctx.input.strip_prefix("/bg").unwrap_or("").trim();
            commands::handle_bg(args, ctx.bg_tracker).await;
            CommandResult::Continue
        }
        s if s.starts_with("/run ") || (s.starts_with('!') && s.len() > 1) => {
            commands::handle_run(ctx.input);
            CommandResult::Continue
        }
        "/run" => {
            commands::handle_run_usage();
            CommandResult::Continue
        }
        s if s == "/pr" || s.starts_with("/pr ") => {
            commands::handle_pr(
                ctx.input,
                ctx.agent,
                ctx.session_total,
                &ctx.agent_config.model,
            )
            .await;
            CommandResult::Continue
        }
        s if s == "/git" || s.starts_with("/git ") => {
            commands::handle_git(ctx.input);
            CommandResult::Continue
        }
        s if s == "/spawn" || s.starts_with("/spawn ") => {
            if let Some(context_msg) = commands::handle_spawn(
                ctx.input,
                ctx.agent_config,
                ctx.session_total,
                &ctx.agent_config.model,
                ctx.agent.messages(),
                ctx.spawn_tracker,
            )
            .await
            {
                *ctx.last_input = Some(context_msg.clone());
                let prompt_start = Instant::now();
                let outcome = run_prompt_with_changes(
                    ctx.agent,
                    &context_msg,
                    ctx.session_total,
                    &ctx.agent_config.model,
                    ctx.session_changes,
                )
                .await;
                crate::format::maybe_ring_bell(prompt_start.elapsed());
                *ctx.last_error = outcome.last_tool_error;
                auto_compact_if_needed(ctx.agent);
            }
            CommandResult::Continue
        }
        s if s == "/review" || s.starts_with("/review ") => {
            if let Some(review_prompt) = commands::handle_review(
                ctx.input,
                ctx.agent,
                ctx.session_total,
                &ctx.agent_config.model,
            )
            .await
            {
                *ctx.last_input = Some(review_prompt);
            }
            CommandResult::Continue
        }
        "/update" => {
            match commands::handle_update() {
                Ok(_) => println!(
                    "Update completed successfully. Please restart yoyo to use the new version."
                ),
                Err(e) => eprintln!("Update failed: {}", e),
            }
            CommandResult::Continue
        }
        s if s == "/skill" || s.starts_with("/skill ") => {
            commands::handle_skill(ctx.input, &ctx.agent_config.skills);
            CommandResult::Continue
        }
        s if s == "/explain" || s.starts_with("/explain ") => {
            if let Some(prompt) = commands::build_explain_prompt(ctx.input) {
                *ctx.last_input = Some(prompt.clone());
                let prompt_start = Instant::now();
                let outcome = run_prompt_with_changes(
                    ctx.agent,
                    &prompt,
                    ctx.session_total,
                    &ctx.agent_config.model,
                    ctx.session_changes,
                )
                .await;
                crate::format::maybe_ring_bell(prompt_start.elapsed());
                *ctx.last_error = outcome.last_tool_error;
                auto_compact_if_needed(ctx.agent);
            }
            CommandResult::Continue
        }
        s if s == "/plan" || s.starts_with("/plan ") => {
            if let Some(plan_prompt) = commands::handle_plan(
                ctx.input,
                ctx.agent,
                ctx.session_total,
                &ctx.agent_config.model,
            )
            .await
            {
                *ctx.last_input = Some(plan_prompt);
            }
            CommandResult::Continue
        }
        s if s == "/extended" || s.starts_with("/extended ") => {
            if let Some(extended_prompt) = crate::repl::handle_extended(
                ctx.input,
                ctx.agent,
                ctx.session_total,
                &ctx.agent_config.model,
                ctx.session_changes,
            )
            .await
            {
                *ctx.last_input = Some(extended_prompt);
                *ctx.last_error = None; // Clear — handle_extended reports its own errors
                auto_compact_if_needed(ctx.agent);
            }
            CommandResult::Continue
        }
        s if s == "/side" || s.starts_with("/side ") => {
            crate::repl::handle_side(ctx.input, ctx.agent_config).await;
            CommandResult::Continue
        }
        s if s == "/quick" || s.starts_with("/quick ") => {
            crate::repl::handle_quick(ctx.input, ctx.agent_config).await;
            CommandResult::Continue
        }
        // Custom slash commands: loaded from .yoyo/commands/ and ~/.yoyo/commands/
        // Also catches unknown commands (anything starting with '/' not matched above)
        s if s.starts_with('/') => {
            let cmd_name = s[1..].split_whitespace().next().unwrap_or(&s[1..]);
            if let Some(content) = crate::commands::get_custom_command_content(cmd_name) {
                eprintln!("{DIM}  running custom command /{cmd_name}{RESET}");
                CommandResult::SendToAgent(content)
            } else if is_unknown_command(s) {
                let cmd = s.split_whitespace().next().unwrap_or(s);
                eprintln!("{RED}  unknown command: {cmd}{RESET}");
                if let Some(suggestion) = suggest_command(s) {
                    eprintln!("{YELLOW}  did you mean {suggestion}?{RESET}");
                }
                eprintln!("{DIM}  type /help for available commands{RESET}\n");
                CommandResult::Continue
            } else {
                // Shouldn't happen — known command not matched above
                CommandResult::Continue
            }
        }
        _ => CommandResult::NotACommand,
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
}
