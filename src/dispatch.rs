//! REPL slash-command routing.
//!
//! The [`dispatch_command`] function routes `/`-prefixed REPL commands to their
//! handlers. It was extracted from `repl.rs` to keep the REPL loop focused on
//! readline mechanics and the command table easy to navigate.
//!
//! The pure routing layer ([`route_command`]) maps input strings to
//! [`CommandRoute`] variants without any side effects — making command dispatch
//! fully testable without a live agent.
//!
//! CLI subcommand dispatch (`yoyo <subcmd>`) lives in [`crate::dispatch_sub`].

use std::time::Instant;

use crate::cli::{effective_context_tokens, parse_thinking_level, McpServerConfig};
use crate::commands::{
    self, auto_compact_if_needed, clear_confirmation_message, is_unknown_command,
    reset_compact_thrash, suggest_command, thinking_level_name, ConfigDisplay,
};
use crate::format::*;
use crate::prompt::run_prompt_with_changes;
use crate::session::{SessionChanges, TurnHistory};
use crate::AgentConfig;
use yoagent::context::total_tokens;
use yoagent::*;

/// What category of command the input maps to, determined purely from the input
/// string. This is the testable, pure routing layer — no agent state needed.
#[derive(Debug, PartialEq)]
pub(crate) enum CommandRoute {
    Quit,
    Help,
    Version,
    Status,
    Tokens,
    Cost,
    Profile,
    Changelog,
    Evolution,
    Clear,
    ClearForce,
    Model,
    Provider,
    Think,
    Save,
    Load,
    Stash,
    Fork,
    Checkpoint,
    Diff,
    Blame,
    Undo,
    Health,
    Doctor,
    Test,
    LintFix,
    Lint,
    Fix,
    Security,
    History,
    Search,
    Marks,
    Changes,
    Export,
    Mark,
    Jump,
    Config,
    ConfigShow,
    ConfigEdit,
    ConfigSet,
    ConfigGet,
    Hooks,
    Permissions,
    Compact,
    Commit,
    Context,
    Add,
    Docs,
    Find,
    Grep,
    Init,
    Rename,
    Extract,
    Move,
    Refactor,
    Remember,
    Memories,
    Forget,
    Index,
    Map,
    Outline,
    Retry,
    Tree,
    Web,
    Open,
    Copy,
    Watch,
    Loop,
    Todo,
    Teach,
    Read,
    Architect,
    Mcp,
    Ast,
    Apply,
    Bg,
    Run,
    Pr,
    Git,
    Goal,
    Spawn,
    Review,
    Revisit,
    Update,
    Skill,
    Explain,
    Plan,
    Extended,
    Side,
    Quick,
    Tips,
    /// Input starts with `/` but doesn't match any known command.
    UnknownSlash,
    /// Input is not a slash command at all.
    NotACommand,
}

/// Pure routing: maps an input string to a [`CommandRoute`] without side effects.
///
/// This extracts the pattern-matching logic from [`dispatch_command`] so it can
/// be tested without constructing a full [`DispatchContext`].
pub(crate) fn route_command(input: &str) -> CommandRoute {
    match input {
        "/quit" | "/exit" => CommandRoute::Quit,
        "/version" => CommandRoute::Version,
        "/status" => CommandRoute::Status,
        "/tokens" => CommandRoute::Tokens,
        "/cost" => CommandRoute::Cost,
        "/profile" => CommandRoute::Profile,
        "/clear" => CommandRoute::Clear,
        "/clear!" => CommandRoute::ClearForce,
        "/model" => CommandRoute::Model,
        "/provider" => CommandRoute::Provider,
        "/think" => CommandRoute::Think,
        "/health" => CommandRoute::Health,
        "/doctor" => CommandRoute::Doctor,
        "/test" => CommandRoute::Test,
        "/security" => CommandRoute::Security,
        "/lint fix" => CommandRoute::LintFix,
        "/fix" => CommandRoute::Fix,
        "/marks" => CommandRoute::Marks,
        "/config" => CommandRoute::Config,
        "/hooks" => CommandRoute::Hooks,
        "/permissions" => CommandRoute::Permissions,
        "/init" => CommandRoute::Init,
        "/index" => CommandRoute::Index,
        "/retry" => CommandRoute::Retry,
        "/run" => CommandRoute::Run,
        "/update" => CommandRoute::Update,
        "/docs" => CommandRoute::Docs,
        "/find" => CommandRoute::Find,
        "/grep" => CommandRoute::Grep,
        "/search" => CommandRoute::Search,
        "/tips" => CommandRoute::Tips,
        _ => route_command_prefix(input),
    }
}

/// Helper for prefix-based and fallback routing.
fn route_command_prefix(input: &str) -> CommandRoute {
    // Special case: `!cmd` is equivalent to `/run cmd`
    if input.starts_with('!') && input.len() > 1 {
        return CommandRoute::Run;
    }

    if let Some(rest) = input.strip_prefix('/') {
        // Commands with subcommands that need ordering (e.g. /config show before /config set)
        if rest == "help" || rest.starts_with("help ") {
            return CommandRoute::Help;
        }
        if rest.starts_with("config show") {
            return CommandRoute::ConfigShow;
        }
        if rest.starts_with("config edit") {
            return CommandRoute::ConfigEdit;
        }
        if rest.starts_with("config set") {
            return CommandRoute::ConfigSet;
        }
        if rest == "config get" || rest.starts_with("config get ") {
            return CommandRoute::ConfigGet;
        }
        // /lint fix is handled as exact match above; /lint <other> falls here
        if rest == "lint" || rest.starts_with("lint ") {
            return CommandRoute::Lint;
        }

        // Simple prefix commands: "cmd" or "cmd ..."
        let cmd = rest.split_whitespace().next().unwrap_or(rest);
        match cmd {
            "changelog" => CommandRoute::Changelog,
            "evolution" => CommandRoute::Evolution,
            "model" => CommandRoute::Model,
            "provider" => CommandRoute::Provider,
            "think" => CommandRoute::Think,
            "save" => CommandRoute::Save,
            "load" => CommandRoute::Load,
            "stash" => CommandRoute::Stash,
            "fork" => CommandRoute::Fork,
            "checkpoint" => CommandRoute::Checkpoint,
            "diff" => CommandRoute::Diff,
            "blame" => CommandRoute::Blame,
            "undo" => CommandRoute::Undo,
            "history" => CommandRoute::History,
            "search" => CommandRoute::Search,
            "changes" => CommandRoute::Changes,
            "export" => CommandRoute::Export,
            "mark" => CommandRoute::Mark,
            "jump" => CommandRoute::Jump,
            "commit" => CommandRoute::Commit,
            "context" => CommandRoute::Context,
            "add" => CommandRoute::Add,
            "docs" => CommandRoute::Docs,
            "find" => CommandRoute::Find,
            "grep" => CommandRoute::Grep,
            "rename" => CommandRoute::Rename,
            "extract" => CommandRoute::Extract,
            "move" => CommandRoute::Move,
            "refactor" => CommandRoute::Refactor,
            "remember" => CommandRoute::Remember,
            "memories" => CommandRoute::Memories,
            "forget" => CommandRoute::Forget,
            "map" => CommandRoute::Map,
            "outline" => CommandRoute::Outline,
            "tree" => CommandRoute::Tree,
            "web" => CommandRoute::Web,
            "open" => CommandRoute::Open,
            "copy" => CommandRoute::Copy,
            "watch" => CommandRoute::Watch,
            "loop" => CommandRoute::Loop,
            "todo" => CommandRoute::Todo,
            "teach" => CommandRoute::Teach,
            "read" => CommandRoute::Read,
            "architect" => CommandRoute::Architect,
            "mcp" => CommandRoute::Mcp,
            "compact" => CommandRoute::Compact,
            "ast" => CommandRoute::Ast,
            "apply" => CommandRoute::Apply,
            "bg" => CommandRoute::Bg,
            "run" => CommandRoute::Run,
            "pr" => CommandRoute::Pr,
            "git" => CommandRoute::Git,
            "goal" => CommandRoute::Goal,
            "spawn" => CommandRoute::Spawn,
            "review" => CommandRoute::Review,
            "revisit" => CommandRoute::Revisit,
            "skill" => CommandRoute::Skill,
            "explain" => CommandRoute::Explain,
            "plan" => CommandRoute::Plan,
            "extended" => CommandRoute::Extended,
            "side" => CommandRoute::Side,
            "quick" => CommandRoute::Quick,
            _ => CommandRoute::UnknownSlash,
        }
    } else {
        CommandRoute::NotACommand
    }
}

/// Result of dispatching a slash command in the REPL.
#[derive(Debug)]
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

/// Dispatch a slash command entered at the REPL prompt.
///
/// Handles all `/`-prefixed commands, returning a [`CommandResult`] that tells
/// the main loop what to do next.  This was extracted from `run_repl` to keep
/// the outer loop small and the command table easy to navigate.
///
/// Routing is delegated to the pure [`route_command`] function, keeping the
/// dispatch logic testable without a live agent.
pub(crate) async fn dispatch_command(ctx: &mut DispatchContext<'_>) -> CommandResult {
    match route_command(ctx.input) {
        CommandRoute::Quit => CommandResult::Quit,
        CommandRoute::Help => {
            if !commands::handle_help_command(ctx.input) {
                commands::handle_help();
            }
            CommandResult::Continue
        }
        CommandRoute::Version => {
            commands::handle_version();
            CommandResult::Continue
        }
        CommandRoute::Status => {
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
        CommandRoute::Tokens => {
            commands::handle_tokens(ctx.agent, ctx.session_total, &ctx.agent_config.model);
            CommandResult::Continue
        }
        CommandRoute::Cost => {
            commands::handle_cost(
                ctx.session_total,
                &ctx.agent_config.model,
                ctx.agent.messages(),
            );
            CommandResult::Continue
        }
        CommandRoute::Profile => {
            commands::handle_profile(
                ctx.agent,
                &ctx.agent_config.model,
                &ctx.agent_config.provider,
                ctx.session_start,
                ctx.session_total,
            );
            CommandResult::Continue
        }
        CommandRoute::Changelog => {
            commands::handle_changelog(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Evolution => {
            commands::handle_evolution(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Tips => {
            commands::handle_tips();
            CommandResult::Continue
        }
        CommandRoute::Clear => {
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
        CommandRoute::ClearForce => {
            *ctx.agent = ctx.agent_config.build_agent();
            ctx.session_changes.clear();
            ctx.turn_history.clear();
            reset_compact_thrash();
            reset_context_budget_warning();
            println!("{DIM}  (conversation force-cleared){RESET}\n");
            CommandResult::Continue
        }
        CommandRoute::Model => {
            let s = ctx.input;
            if s == "/model" {
                commands::handle_model_show(&ctx.agent_config.model);
                return CommandResult::Continue;
            }
            let arg = s.trim_start_matches("/model ").trim();
            if arg.is_empty() {
                println!("{DIM}  current model: {}", ctx.agent_config.model);
                println!("  usage: /model <name>{RESET}\n");
                return CommandResult::Continue;
            }
            if arg == "list" || arg.starts_with("list ") {
                let filter = arg.strip_prefix("list").unwrap_or("").trim();
                commands::handle_model_list(
                    &ctx.agent_config.model,
                    &ctx.agent_config.provider,
                    filter,
                );
                return CommandResult::Continue;
            }
            if arg == "info" || arg.starts_with("info ") {
                let model_name = arg.strip_prefix("info").unwrap_or("").trim();
                let target = if model_name.is_empty() {
                    &ctx.agent_config.model
                } else {
                    model_name
                };
                commands::handle_model_info(target, &ctx.agent_config.model);
                return CommandResult::Continue;
            }
            let new_model = arg;
            ctx.agent_config.model = new_model.to_string();
            // Rebuild ctx.agent with new model, preserving conversation
            let saved = match ctx.agent.save_messages() {
                Ok(json) => Some(json),
                Err(e) => {
                    eprintln!("{DIM}  ⚠ could not preserve conversation: {e}{RESET}");
                    None
                }
            };
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
        CommandRoute::Provider => {
            let s = ctx.input;
            if s == "/provider" {
                commands::handle_provider_show(&ctx.agent_config.provider);
                return CommandResult::Continue;
            }
            let new_provider = s.trim_start_matches("/provider ").trim();
            if new_provider.is_empty() {
                commands::handle_provider_show(&ctx.agent_config.provider);
                return CommandResult::Continue;
            }
            commands::handle_provider_switch(new_provider, ctx.agent_config, ctx.agent);
            CommandResult::Continue
        }
        CommandRoute::Think => {
            let s = ctx.input;
            if s == "/think" {
                commands::handle_think_show(ctx.agent_config.thinking);
                return CommandResult::Continue;
            }
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
            let saved = match ctx.agent.save_messages() {
                Ok(json) => Some(json),
                Err(e) => {
                    eprintln!("{DIM}  ⚠ could not preserve conversation: {e}{RESET}");
                    None
                }
            };
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
        CommandRoute::Save => {
            commands::handle_save(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Load => {
            commands::handle_load(ctx.agent, ctx.input);
            reset_compact_thrash();
            CommandResult::Continue
        }
        CommandRoute::Stash => {
            let result = commands::handle_stash(ctx.agent, ctx.input);
            print!("{result}");
            CommandResult::Continue
        }
        CommandRoute::Fork => {
            let result = commands::handle_fork(ctx.agent, ctx.input);
            print!("{result}");
            CommandResult::Continue
        }
        CommandRoute::Checkpoint => {
            commands::handle_checkpoint(ctx.input, ctx.checkpoint_store, ctx.session_changes);
            CommandResult::Continue
        }
        CommandRoute::Diff => {
            let opts = commands::parse_diff_args(ctx.input);
            if opts.explain {
                if let Some(prompt) = commands::handle_diff_explain(
                    ctx.input,
                    ctx.agent,
                    ctx.session_total,
                    &ctx.agent_config.model,
                )
                .await
                {
                    *ctx.last_input = Some(prompt);
                }
            } else {
                commands::handle_diff(ctx.input);
            }
            CommandResult::Continue
        }
        CommandRoute::Blame => {
            commands::handle_blame(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Undo => {
            if let Some(undo_ctx) = commands::handle_undo(ctx.input, ctx.turn_history) {
                *ctx.undo_context = Some(undo_ctx);
            }
            CommandResult::Continue
        }
        CommandRoute::Health => {
            commands::handle_health();
            CommandResult::Continue
        }
        CommandRoute::Doctor => {
            commands::handle_doctor(&ctx.agent_config.provider, &ctx.agent_config.model);
            CommandResult::Continue
        }
        CommandRoute::Test => {
            commands::handle_test();
            CommandResult::Continue
        }
        CommandRoute::Security => {
            commands::handle_security();
            CommandResult::Continue
        }
        CommandRoute::LintFix => {
            if let Some(fix_prompt) =
                commands::handle_lint_fix(ctx.agent, ctx.session_total, &ctx.agent_config.model)
                    .await
            {
                *ctx.last_input = Some(fix_prompt);
            }
            CommandResult::Continue
        }
        CommandRoute::Lint => {
            if let Some(lint_result) = commands::handle_lint(ctx.input) {
                if lint_result.starts_with("Lint FAILED")
                    || lint_result.starts_with("Failed to run")
                {
                    *ctx.last_input = Some(lint_result);
                }
            }
            CommandResult::Continue
        }
        CommandRoute::Fix => {
            if let Some(fix_prompt) =
                commands::handle_fix(ctx.agent, ctx.session_total, &ctx.agent_config.model).await
            {
                *ctx.last_input = Some(fix_prompt);
            }
            CommandResult::Continue
        }
        CommandRoute::History => {
            let sub = ctx.input.strip_prefix("/history").unwrap_or("").trim();
            if sub == "detail" {
                commands::handle_history_detail(ctx.agent);
            } else {
                commands::handle_history(ctx.agent);
            }
            CommandResult::Continue
        }
        CommandRoute::Search => {
            commands::handle_search(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Marks => {
            commands::handle_marks(ctx.bookmarks);
            CommandResult::Continue
        }
        CommandRoute::Changes => {
            if commands::wants_summary(ctx.input) {
                commands::handle_changes_summary(ctx.session_changes, ctx.agent_config).await;
            } else {
                commands::handle_changes(ctx.session_changes, ctx.input);
            }
            CommandResult::Continue
        }
        CommandRoute::Export => {
            commands::handle_export(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Mark => {
            commands::handle_mark(ctx.agent, ctx.input, ctx.bookmarks);
            CommandResult::Continue
        }
        CommandRoute::Jump => {
            commands::handle_jump(ctx.agent, ctx.input, ctx.bookmarks);
            CommandResult::Continue
        }
        CommandRoute::Config => {
            commands::handle_config(&ConfigDisplay {
                provider: &ctx.agent_config.provider,
                model: &ctx.agent_config.model,
                base_url: &ctx.agent_config.base_url,
                thinking: ctx.agent_config.thinking,
                max_tokens: ctx.agent_config.max_tokens,
                max_turns: ctx.agent_config.max_turns,
                temperature: ctx.agent_config.temperature,
                skills: &ctx.agent_config.skills,
                system_prompt: &ctx.agent_config.system_prompt,
                mcp_count: ctx.mcp_count,
                openapi_count: ctx.openapi_count,
                hook_count: ctx.agent_config.shell_hooks.len(),
                agent: ctx.agent,
                cwd: ctx.cwd,
            });
            CommandResult::Continue
        }
        CommandRoute::ConfigShow => {
            commands::handle_config_show();
            CommandResult::Continue
        }
        CommandRoute::ConfigEdit => {
            commands::handle_config_edit();
            CommandResult::Continue
        }
        CommandRoute::ConfigSet => {
            commands::handle_config_set(ctx.input, ctx.agent_config, ctx.agent);
            CommandResult::Continue
        }
        CommandRoute::ConfigGet => {
            commands::handle_config_get(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Hooks => {
            commands::handle_hooks(&ctx.agent_config.shell_hooks);
            CommandResult::Continue
        }
        CommandRoute::Permissions => {
            commands::handle_permissions(
                ctx.agent_config.auto_approve,
                &ctx.agent_config.permissions,
                &ctx.agent_config.dir_restrictions,
            );
            CommandResult::Continue
        }
        CommandRoute::Compact => {
            commands::handle_compact(ctx.agent, ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Commit => {
            if commands::wants_ai_commit(ctx.input) {
                commands::handle_commit_ai(ctx.input, ctx.agent_config).await;
            } else {
                commands::handle_commit(ctx.input);
            }
            CommandResult::Continue
        }
        CommandRoute::Context => {
            commands::handle_context(ctx.input, &ctx.agent_config.system_prompt, ctx.agent);
            CommandResult::Continue
        }
        CommandRoute::Add => {
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
                let content_blocks = crate::conversations::build_add_content_blocks(&results);
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
        CommandRoute::Docs => {
            commands::handle_docs(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Find => {
            commands::handle_find(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Grep => {
            commands::handle_grep(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Init => {
            commands::handle_init();
            CommandResult::Continue
        }
        CommandRoute::Rename => {
            commands::handle_rename(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Extract => {
            commands::handle_extract(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Move => {
            commands::handle_move(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Refactor => {
            commands::handle_refactor(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Remember => {
            commands::handle_remember(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Memories => {
            commands::handle_memories(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Forget => {
            commands::handle_forget(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Index => {
            commands::handle_index();
            CommandResult::Continue
        }
        CommandRoute::Map => {
            commands::handle_map(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Outline => {
            commands::handle_outline(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Retry => {
            *ctx.last_error = commands::handle_retry(
                ctx.agent,
                ctx.input,
                ctx.last_input,
                ctx.last_error,
                ctx.session_total,
                &ctx.agent_config.model,
            )
            .await;
            CommandResult::Continue
        }
        CommandRoute::Tree => {
            commands::handle_tree(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Web => {
            commands::handle_web(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Open => {
            commands::handle_open(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Copy => {
            commands::handle_copy(ctx.input, ctx.agent.messages());
            CommandResult::Continue
        }
        CommandRoute::Watch => {
            commands::handle_watch(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Loop => {
            commands::handle_loop(
                ctx.input,
                ctx.agent,
                ctx.session_total,
                ctx.agent_config,
                ctx.session_changes,
            )
            .await;
            CommandResult::Continue
        }
        CommandRoute::Todo => {
            let result = commands::handle_todo(ctx.input);
            println!("{result}\n");
            CommandResult::Continue
        }
        CommandRoute::Teach => {
            commands::handle_teach(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Read => {
            commands::handle_read(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Architect => {
            commands::handle_architect(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Mcp => {
            commands::handle_mcp(
                ctx.input,
                ctx.mcp_cli_servers,
                ctx.mcp_server_configs,
                ctx.mcp_count,
            );
            CommandResult::Continue
        }
        CommandRoute::Ast => {
            commands::handle_ast_grep(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Apply => {
            commands::handle_apply(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Bg => {
            let args = ctx.input.strip_prefix("/bg").unwrap_or("").trim();
            commands::handle_bg(args, ctx.bg_tracker).await;
            CommandResult::Continue
        }
        CommandRoute::Run => {
            if ctx.input == "/run" {
                commands::handle_run_usage();
            } else {
                commands::handle_run(ctx.input);
            }
            CommandResult::Continue
        }
        CommandRoute::Pr => {
            commands::handle_pr(
                ctx.input,
                ctx.agent,
                ctx.session_total,
                &ctx.agent_config.model,
            )
            .await;
            CommandResult::Continue
        }
        CommandRoute::Git => {
            commands::handle_git(ctx.input);
            CommandResult::Continue
        }
        CommandRoute::Goal => commands::handle_goal(ctx.input),
        CommandRoute::Spawn => {
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
        CommandRoute::Review => {
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
        CommandRoute::Revisit => {
            let result = commands::handle_revisit(ctx.input);
            println!("{result}\n");
            CommandResult::Continue
        }
        CommandRoute::Update => {
            match commands::handle_update() {
                Ok(_) => println!(
                    "Update completed successfully. Please restart yoyo to use the new version."
                ),
                Err(e) => eprintln!("Update failed: {}", e),
            }
            CommandResult::Continue
        }
        CommandRoute::Skill => {
            commands::handle_skill(ctx.input, &ctx.agent_config.skills);
            CommandResult::Continue
        }
        CommandRoute::Explain => {
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
        CommandRoute::Plan => {
            match commands::handle_plan(
                ctx.input,
                ctx.agent,
                ctx.session_total,
                &ctx.agent_config.model,
            )
            .await
            {
                commands::PlanResult::PlanGenerated(plan_prompt) => {
                    *ctx.last_input = Some(plan_prompt);
                    *ctx.last_error = None;
                    auto_compact_if_needed(ctx.agent);
                    CommandResult::Continue
                }
                commands::PlanResult::Apply(apply_prompt) => {
                    *ctx.last_error = None;
                    commands::set_plan_apply_active(true);
                    CommandResult::SendToAgent(apply_prompt)
                }
                commands::PlanResult::Handled => CommandResult::Continue,
            }
        }
        CommandRoute::Extended => {
            if let Some(extended_prompt) = crate::conversations::handle_extended(
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
        CommandRoute::Side => {
            crate::conversations::handle_side(ctx.input, ctx.agent_config).await;
            CommandResult::Continue
        }
        CommandRoute::Quick => {
            crate::conversations::handle_quick(ctx.input, ctx.agent_config).await;
            CommandResult::Continue
        }
        // Custom slash commands: loaded from .yoyo/commands/ and ~/.yoyo/commands/
        // route_command returns UnknownSlash for anything starting with '/' not
        // matched above — we check custom commands here (requires file I/O).
        CommandRoute::UnknownSlash => {
            let s = ctx.input;
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
        CommandRoute::NotACommand => CommandResult::NotACommand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_quit() {
        assert_eq!(route_command("/quit"), CommandRoute::Quit);
        assert_eq!(route_command("/exit"), CommandRoute::Quit);
    }

    #[test]
    fn test_route_help() {
        assert_eq!(route_command("/help"), CommandRoute::Help);
        assert_eq!(route_command("/help commands"), CommandRoute::Help);
        assert_eq!(route_command("/help model"), CommandRoute::Help);
    }

    #[test]
    fn test_route_not_a_command() {
        assert_eq!(route_command("hello world"), CommandRoute::NotACommand);
        assert_eq!(route_command(""), CommandRoute::NotACommand);
        assert_eq!(route_command("some text here"), CommandRoute::NotACommand);
    }

    #[test]
    fn test_route_unknown_slash() {
        assert_eq!(route_command("/nonexistent"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/foobar arg"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/xyz"), CommandRoute::UnknownSlash);
    }

    #[test]
    fn test_route_info_commands() {
        assert_eq!(route_command("/version"), CommandRoute::Version);
        assert_eq!(route_command("/status"), CommandRoute::Status);
        assert_eq!(route_command("/tokens"), CommandRoute::Tokens);
        assert_eq!(route_command("/cost"), CommandRoute::Cost);
        assert_eq!(route_command("/profile"), CommandRoute::Profile);
    }

    #[test]
    fn test_route_model() {
        assert_eq!(route_command("/model"), CommandRoute::Model);
        assert_eq!(route_command("/model sonnet"), CommandRoute::Model);
        assert_eq!(route_command("/model list"), CommandRoute::Model);
        assert_eq!(route_command("/model info gpt-4o"), CommandRoute::Model);
    }

    #[test]
    fn test_route_provider() {
        assert_eq!(route_command("/provider"), CommandRoute::Provider);
        assert_eq!(route_command("/provider openai"), CommandRoute::Provider);
    }

    #[test]
    fn test_route_think() {
        assert_eq!(route_command("/think"), CommandRoute::Think);
        assert_eq!(route_command("/think high"), CommandRoute::Think);
    }

    #[test]
    fn test_route_clear() {
        assert_eq!(route_command("/clear"), CommandRoute::Clear);
        assert_eq!(route_command("/clear!"), CommandRoute::ClearForce);
    }

    #[test]
    fn test_route_session_commands() {
        assert_eq!(route_command("/save"), CommandRoute::Save);
        assert_eq!(route_command("/save mysession"), CommandRoute::Save);
        assert_eq!(route_command("/load"), CommandRoute::Load);
        assert_eq!(route_command("/load mysession"), CommandRoute::Load);
        assert_eq!(route_command("/stash"), CommandRoute::Stash);
        assert_eq!(route_command("/stash push"), CommandRoute::Stash);
        assert_eq!(route_command("/fork"), CommandRoute::Fork);
        assert_eq!(route_command("/fork create name"), CommandRoute::Fork);
        assert_eq!(route_command("/checkpoint"), CommandRoute::Checkpoint);
        assert_eq!(route_command("/checkpoint save"), CommandRoute::Checkpoint);
    }

    #[test]
    fn test_route_git_commands() {
        assert_eq!(route_command("/diff"), CommandRoute::Diff);
        assert_eq!(route_command("/diff --staged"), CommandRoute::Diff);
        assert_eq!(route_command("/blame"), CommandRoute::Blame);
        assert_eq!(route_command("/blame src/main.rs"), CommandRoute::Blame);
        assert_eq!(route_command("/undo"), CommandRoute::Undo);
        assert_eq!(route_command("/undo file.rs"), CommandRoute::Undo);
        assert_eq!(route_command("/commit"), CommandRoute::Commit);
        assert_eq!(route_command("/commit fix typo"), CommandRoute::Commit);
        assert_eq!(route_command("/pr"), CommandRoute::Pr);
        assert_eq!(route_command("/pr create"), CommandRoute::Pr);
        assert_eq!(route_command("/git"), CommandRoute::Git);
        assert_eq!(route_command("/git log"), CommandRoute::Git);
    }

    #[test]
    fn test_route_lint_and_test() {
        assert_eq!(route_command("/test"), CommandRoute::Test);
        assert_eq!(route_command("/lint"), CommandRoute::Lint);
        assert_eq!(route_command("/lint strict"), CommandRoute::Lint);
        assert_eq!(route_command("/lint fix"), CommandRoute::LintFix);
        assert_eq!(route_command("/fix"), CommandRoute::Fix);
    }

    #[test]
    fn test_route_search_and_nav() {
        assert_eq!(route_command("/find"), CommandRoute::Find);
        assert_eq!(route_command("/find main.rs"), CommandRoute::Find);
        assert_eq!(route_command("/grep"), CommandRoute::Grep);
        assert_eq!(route_command("/grep pattern"), CommandRoute::Grep);
        assert_eq!(route_command("/search"), CommandRoute::Search);
        assert_eq!(route_command("/search term"), CommandRoute::Search);
        assert_eq!(route_command("/index"), CommandRoute::Index);
        assert_eq!(route_command("/map"), CommandRoute::Map);
        assert_eq!(route_command("/map src/"), CommandRoute::Map);
        assert_eq!(route_command("/outline"), CommandRoute::Outline);
        assert_eq!(route_command("/outline src/main.rs"), CommandRoute::Outline);
        assert_eq!(route_command("/tree"), CommandRoute::Tree);
        assert_eq!(route_command("/tree src/"), CommandRoute::Tree);
    }

    #[test]
    fn test_route_config_subcommands() {
        assert_eq!(route_command("/config"), CommandRoute::Config);
        assert_eq!(route_command("/config show"), CommandRoute::ConfigShow);
        assert_eq!(route_command("/config show all"), CommandRoute::ConfigShow);
        assert_eq!(route_command("/config edit"), CommandRoute::ConfigEdit);
        assert_eq!(
            route_command("/config edit global"),
            CommandRoute::ConfigEdit
        );
        assert_eq!(
            route_command("/config set model gpt-4o"),
            CommandRoute::ConfigSet
        );
        assert_eq!(route_command("/config get"), CommandRoute::ConfigGet);
        assert_eq!(route_command("/config get model"), CommandRoute::ConfigGet);
    }

    #[test]
    fn test_route_file_commands() {
        assert_eq!(route_command("/add"), CommandRoute::Add);
        assert_eq!(route_command("/add file.rs"), CommandRoute::Add);
        assert_eq!(route_command("/web"), CommandRoute::Web);
        assert_eq!(route_command("/web https://example.com"), CommandRoute::Web);
        assert_eq!(route_command("/apply"), CommandRoute::Apply);
        assert_eq!(route_command("/apply patch.diff"), CommandRoute::Apply);
        assert_eq!(route_command("/copy"), CommandRoute::Copy);
        assert_eq!(route_command("/copy code"), CommandRoute::Copy);
        assert_eq!(route_command("/open"), CommandRoute::Open);
        assert_eq!(route_command("/open file.rs"), CommandRoute::Open);
    }

    #[test]
    fn test_route_refactor_commands() {
        assert_eq!(route_command("/rename"), CommandRoute::Rename);
        assert_eq!(route_command("/rename old new"), CommandRoute::Rename);
        assert_eq!(route_command("/extract"), CommandRoute::Extract);
        assert_eq!(route_command("/extract fn helper"), CommandRoute::Extract);
        assert_eq!(route_command("/move"), CommandRoute::Move);
        assert_eq!(route_command("/move method impl"), CommandRoute::Move);
        assert_eq!(route_command("/refactor"), CommandRoute::Refactor);
        assert_eq!(route_command("/refactor inline"), CommandRoute::Refactor);
    }

    #[test]
    fn test_route_memory_commands() {
        assert_eq!(route_command("/remember"), CommandRoute::Remember);
        assert_eq!(route_command("/remember something"), CommandRoute::Remember);
        assert_eq!(route_command("/memories"), CommandRoute::Memories);
        assert_eq!(route_command("/memories search"), CommandRoute::Memories);
        assert_eq!(route_command("/forget"), CommandRoute::Forget);
        assert_eq!(route_command("/forget 1"), CommandRoute::Forget);
    }

    #[test]
    fn test_route_conversation_commands() {
        assert_eq!(route_command("/extended"), CommandRoute::Extended);
        assert_eq!(
            route_command("/extended prompt here"),
            CommandRoute::Extended
        );
        assert_eq!(route_command("/side"), CommandRoute::Side);
        assert_eq!(route_command("/side question"), CommandRoute::Side);
        assert_eq!(route_command("/quick"), CommandRoute::Quick);
        assert_eq!(route_command("/quick ask"), CommandRoute::Quick);
    }

    #[test]
    fn test_route_misc_commands() {
        assert_eq!(route_command("/run"), CommandRoute::Run);
        assert_eq!(route_command("/run ls -la"), CommandRoute::Run);
        assert_eq!(route_command("!ls"), CommandRoute::Run);
        assert_eq!(route_command("!echo hello"), CommandRoute::Run);
        assert_eq!(route_command("/update"), CommandRoute::Update);
        assert_eq!(route_command("/doctor"), CommandRoute::Doctor);
        assert_eq!(route_command("/health"), CommandRoute::Health);
        assert_eq!(route_command("/compact"), CommandRoute::Compact);
        assert_eq!(route_command("/compact 5"), CommandRoute::Compact);
        assert_eq!(route_command("/compact all"), CommandRoute::Compact);
        assert_eq!(route_command("/retry"), CommandRoute::Retry);
        assert_eq!(route_command("/explain"), CommandRoute::Explain);
        assert_eq!(route_command("/explain function"), CommandRoute::Explain);
    }

    #[test]
    fn test_route_workflow_commands() {
        assert_eq!(route_command("/watch"), CommandRoute::Watch);
        assert_eq!(route_command("/watch set cargo test"), CommandRoute::Watch);
        assert_eq!(route_command("/loop"), CommandRoute::Loop);
        assert_eq!(route_command("/loop 5 do stuff"), CommandRoute::Loop);
        assert_eq!(route_command("/spawn"), CommandRoute::Spawn);
        assert_eq!(route_command("/spawn task here"), CommandRoute::Spawn);
        assert_eq!(route_command("/todo"), CommandRoute::Todo);
        assert_eq!(route_command("/todo add task"), CommandRoute::Todo);
        assert_eq!(route_command("/bg"), CommandRoute::Bg);
        assert_eq!(route_command("/bg list"), CommandRoute::Bg);
        assert_eq!(route_command("/goal"), CommandRoute::Goal);
        assert_eq!(route_command("/goal set something"), CommandRoute::Goal);
        assert_eq!(route_command("/plan"), CommandRoute::Plan);
        assert_eq!(route_command("/plan create"), CommandRoute::Plan);
        assert_eq!(route_command("/skill"), CommandRoute::Skill);
        assert_eq!(route_command("/skill list"), CommandRoute::Skill);
    }

    #[test]
    fn test_route_display_commands() {
        assert_eq!(route_command("/changelog"), CommandRoute::Changelog);
        assert_eq!(route_command("/changelog 5"), CommandRoute::Changelog);
        assert_eq!(route_command("/evolution"), CommandRoute::Evolution);
        assert_eq!(route_command("/evolution 3"), CommandRoute::Evolution);
        assert_eq!(route_command("/history"), CommandRoute::History);
        assert_eq!(route_command("/history detail"), CommandRoute::History);
        assert_eq!(route_command("/marks"), CommandRoute::Marks);
        assert_eq!(route_command("/changes"), CommandRoute::Changes);
        assert_eq!(route_command("/changes summary"), CommandRoute::Changes);
        assert_eq!(route_command("/export"), CommandRoute::Export);
        assert_eq!(route_command("/export output.md"), CommandRoute::Export);
    }

    #[test]
    fn test_route_navigation_marks() {
        assert_eq!(route_command("/mark"), CommandRoute::Mark);
        assert_eq!(route_command("/mark name"), CommandRoute::Mark);
        assert_eq!(route_command("/jump"), CommandRoute::Jump);
        assert_eq!(route_command("/jump name"), CommandRoute::Jump);
    }

    #[test]
    fn test_route_bang_prefix_not_bare() {
        // Bare `!` is not a command
        assert_eq!(route_command("!"), CommandRoute::NotACommand);
    }

    #[test]
    fn test_route_remaining_commands() {
        assert_eq!(route_command("/hooks"), CommandRoute::Hooks);
        assert_eq!(route_command("/permissions"), CommandRoute::Permissions);
        assert_eq!(route_command("/teach"), CommandRoute::Teach);
        assert_eq!(route_command("/teach on"), CommandRoute::Teach);
        assert_eq!(route_command("/architect"), CommandRoute::Architect);
        assert_eq!(route_command("/architect on"), CommandRoute::Architect);
        assert_eq!(route_command("/mcp"), CommandRoute::Mcp);
        assert_eq!(route_command("/mcp list"), CommandRoute::Mcp);
        assert_eq!(route_command("/ast"), CommandRoute::Ast);
        assert_eq!(route_command("/ast pattern"), CommandRoute::Ast);
        assert_eq!(route_command("/context"), CommandRoute::Context);
        assert_eq!(route_command("/context show"), CommandRoute::Context);
        assert_eq!(route_command("/init"), CommandRoute::Init);
        assert_eq!(route_command("/docs"), CommandRoute::Docs);
        assert_eq!(route_command("/docs search"), CommandRoute::Docs);
        assert_eq!(route_command("/review"), CommandRoute::Review);
        assert_eq!(route_command("/review main"), CommandRoute::Review);
    }

    // --- New tests: edge cases and expanded coverage ---

    #[test]
    fn test_route_case_sensitive() {
        // Commands are case-sensitive — uppercase variants should not match
        assert_eq!(route_command("/HELP"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/Help"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/QUIT"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/Quit"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/VERSION"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/Model"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/DIFF"), CommandRoute::UnknownSlash);
    }

    #[test]
    fn test_route_empty_and_whitespace_inputs() {
        // Empty string is not a command
        assert_eq!(route_command(""), CommandRoute::NotACommand);
        // Whitespace-only is not a command
        assert_eq!(route_command("   "), CommandRoute::NotACommand);
        assert_eq!(route_command("\t"), CommandRoute::NotACommand);
        // Bare slash routes through prefix path — no command after /
        assert_eq!(route_command("/"), CommandRoute::UnknownSlash);
    }

    #[test]
    fn test_route_trailing_whitespace_exact_match() {
        // Exact matches require exact strings — trailing space means no exact match.
        // Commands only in the exact-match arm (not in prefix table) become UnknownSlash.
        assert_eq!(route_command("/quit "), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/version "), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/cost "), CommandRoute::UnknownSlash);
        // But commands that ARE in the prefix table still route correctly with trailing space
        assert_eq!(route_command("/diff "), CommandRoute::Diff);
        assert_eq!(route_command("/model "), CommandRoute::Model);
        assert_eq!(route_command("/grep "), CommandRoute::Grep);
    }

    #[test]
    fn test_route_leading_whitespace_not_command() {
        // Leading whitespace means it doesn't start with '/' or '!'
        assert_eq!(route_command(" /help"), CommandRoute::NotACommand);
        assert_eq!(route_command("  /quit"), CommandRoute::NotACommand);
        assert_eq!(route_command(" !ls"), CommandRoute::NotACommand);
    }

    #[test]
    fn test_route_map_vs_mark_vs_marks_no_collision() {
        // /map, /mark, /marks are distinct commands — ensure no prefix collision
        assert_eq!(route_command("/map"), CommandRoute::Map);
        assert_eq!(route_command("/map src/"), CommandRoute::Map);
        assert_eq!(route_command("/mark"), CommandRoute::Mark);
        assert_eq!(route_command("/mark checkpoint1"), CommandRoute::Mark);
        assert_eq!(route_command("/marks"), CommandRoute::Marks);
    }

    #[test]
    fn test_route_find_vs_fix_vs_fork_no_collision() {
        // /find, /fix, /fork are distinct first-words — no prefix confusion
        assert_eq!(route_command("/find"), CommandRoute::Find);
        assert_eq!(route_command("/find file.rs"), CommandRoute::Find);
        assert_eq!(route_command("/fix"), CommandRoute::Fix);
        assert_eq!(route_command("/fork"), CommandRoute::Fork);
        assert_eq!(route_command("/fork create mybranch"), CommandRoute::Fork);
    }

    #[test]
    fn test_route_bang_prefix_variations() {
        // `!cmd` routes to Run
        assert_eq!(route_command("!ls"), CommandRoute::Run);
        assert_eq!(route_command("!echo hello world"), CommandRoute::Run);
        assert_eq!(route_command("!cat /etc/hosts"), CommandRoute::Run);
        // Bare `!` is NOT a command (len == 1)
        assert_eq!(route_command("!"), CommandRoute::NotACommand);
        // `!!` is a valid bang command (len > 1)
        assert_eq!(route_command("!!"), CommandRoute::Run);
    }

    #[test]
    fn test_route_lint_fix_exact_vs_prefix() {
        // "/lint fix" is an exact match in route_command's first arm
        assert_eq!(route_command("/lint fix"), CommandRoute::LintFix);
        // "/lint" alone falls to prefix routing -> Lint
        assert_eq!(route_command("/lint"), CommandRoute::Lint);
        // "/lint strict" falls to prefix routing -> Lint (not LintFix)
        assert_eq!(route_command("/lint strict"), CommandRoute::Lint);
        // "/lint fixme" — first word is "lint", not exact "lint fix"
        assert_eq!(route_command("/lint fixme"), CommandRoute::Lint);
        // "/lint fix --all" — starts with "lint fix" in prefix check
        // This goes through prefix routing since no exact match for "/lint fix --all"
        assert_eq!(route_command("/lint fix --all"), CommandRoute::Lint);
    }

    #[test]
    fn test_route_config_subcommand_priority() {
        // Config subcommand routing has specific ordering:
        // /config show must match before /config set
        assert_eq!(route_command("/config show"), CommandRoute::ConfigShow);
        assert_eq!(route_command("/config show all"), CommandRoute::ConfigShow);
        assert_eq!(route_command("/config set"), CommandRoute::ConfigSet);
        assert_eq!(
            route_command("/config set key val"),
            CommandRoute::ConfigSet
        );
        assert_eq!(route_command("/config edit"), CommandRoute::ConfigEdit);
        assert_eq!(
            route_command("/config edit global"),
            CommandRoute::ConfigEdit
        );
        assert_eq!(route_command("/config get"), CommandRoute::ConfigGet);
        assert_eq!(route_command("/config get key"), CommandRoute::ConfigGet);
        // /config alone (no subcommand) is an exact match -> Config
        assert_eq!(route_command("/config"), CommandRoute::Config);
        // /config with unknown subcommand: "config" is not in prefix table,
        // so it falls through to UnknownSlash
        assert_eq!(route_command("/config unknown"), CommandRoute::UnknownSlash);
    }

    #[test]
    fn test_route_unknown_slash_various() {
        // Various unknown slash commands
        assert_eq!(route_command("/banana"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/testing123"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/hello world"), CommandRoute::UnknownSlash);
        assert_eq!(route_command("/x"), CommandRoute::UnknownSlash);
        assert_eq!(
            route_command("/superlongcommandthatdoesnotexist"),
            CommandRoute::UnknownSlash
        );
    }

    #[test]
    fn test_route_not_a_command_various() {
        // Regular text is not a command
        assert_eq!(route_command("help"), CommandRoute::NotACommand);
        assert_eq!(route_command("quit"), CommandRoute::NotACommand);
        assert_eq!(
            route_command("please run my tests"),
            CommandRoute::NotACommand
        );
        // Numbers, symbols (non-/ non-!) are not commands
        assert_eq!(route_command("42"), CommandRoute::NotACommand);
        assert_eq!(route_command("#comment"), CommandRoute::NotACommand);
        assert_eq!(route_command("@mention"), CommandRoute::NotACommand);
    }

    #[test]
    fn test_route_revisit_and_skill() {
        assert_eq!(route_command("/revisit"), CommandRoute::Revisit);
        assert_eq!(route_command("/revisit scan"), CommandRoute::Revisit);
        assert_eq!(route_command("/skill"), CommandRoute::Skill);
        assert_eq!(route_command("/skill list"), CommandRoute::Skill);
        assert_eq!(route_command("/skill show myskill"), CommandRoute::Skill);
    }

    #[test]
    fn test_route_goal_and_plan() {
        assert_eq!(route_command("/goal"), CommandRoute::Goal);
        assert_eq!(
            route_command("/goal set build better tests"),
            CommandRoute::Goal
        );
        assert_eq!(route_command("/goal clear"), CommandRoute::Goal);
        assert_eq!(route_command("/plan"), CommandRoute::Plan);
        assert_eq!(route_command("/plan create"), CommandRoute::Plan);
        assert_eq!(route_command("/plan show"), CommandRoute::Plan);
    }

    #[test]
    fn test_route_spawn_and_bg() {
        assert_eq!(route_command("/spawn"), CommandRoute::Spawn);
        assert_eq!(
            route_command("/spawn run all the tests"),
            CommandRoute::Spawn
        );
        assert_eq!(route_command("/bg"), CommandRoute::Bg);
        assert_eq!(route_command("/bg list"), CommandRoute::Bg);
        assert_eq!(route_command("/bg kill 1"), CommandRoute::Bg);
    }

    #[test]
    fn test_route_read() {
        assert_eq!(route_command("/read"), CommandRoute::Read);
        assert_eq!(route_command("/read on"), CommandRoute::Read);
        assert_eq!(route_command("/read off"), CommandRoute::Read);
    }

    #[test]
    fn test_route_slash_only() {
        // A bare "/" has no command after it — splits to empty, falls to UnknownSlash
        assert_eq!(route_command("/"), CommandRoute::UnknownSlash);
    }

    #[test]
    fn test_route_exact_match_trumps_prefix_for_single_word() {
        // These commands have exact matches in the first match arm AND prefix
        // matches in route_command_prefix. The exact match should win for bare forms.
        assert_eq!(route_command("/quit"), CommandRoute::Quit);
        assert_eq!(route_command("/version"), CommandRoute::Version);
        assert_eq!(route_command("/status"), CommandRoute::Status);
        assert_eq!(route_command("/tokens"), CommandRoute::Tokens);
        assert_eq!(route_command("/cost"), CommandRoute::Cost);
        assert_eq!(route_command("/profile"), CommandRoute::Profile);
        assert_eq!(route_command("/clear"), CommandRoute::Clear);
        assert_eq!(route_command("/clear!"), CommandRoute::ClearForce);
        assert_eq!(route_command("/health"), CommandRoute::Health);
        assert_eq!(route_command("/doctor"), CommandRoute::Doctor);
        assert_eq!(route_command("/test"), CommandRoute::Test);
        assert_eq!(route_command("/fix"), CommandRoute::Fix);
        assert_eq!(route_command("/marks"), CommandRoute::Marks);
        assert_eq!(route_command("/hooks"), CommandRoute::Hooks);
        assert_eq!(route_command("/permissions"), CommandRoute::Permissions);
        assert_eq!(route_command("/init"), CommandRoute::Init);
        assert_eq!(route_command("/index"), CommandRoute::Index);
        assert_eq!(route_command("/retry"), CommandRoute::Retry);
        assert_eq!(route_command("/run"), CommandRoute::Run);
        assert_eq!(route_command("/update"), CommandRoute::Update);
        assert_eq!(route_command("/docs"), CommandRoute::Docs);
        assert_eq!(route_command("/find"), CommandRoute::Find);
        assert_eq!(route_command("/grep"), CommandRoute::Grep);
        assert_eq!(route_command("/search"), CommandRoute::Search);
    }

    #[test]
    fn test_route_all_prefix_commands_with_args() {
        // Verify every prefix-routed command correctly routes when given arguments
        let cases: Vec<(&str, CommandRoute)> = vec![
            ("/changelog 10", CommandRoute::Changelog),
            ("/evolution latest", CommandRoute::Evolution),
            ("/model switch opus", CommandRoute::Model),
            ("/provider anthropic", CommandRoute::Provider),
            ("/think medium", CommandRoute::Think),
            ("/save my_session.json", CommandRoute::Save),
            ("/load prev.json", CommandRoute::Load),
            ("/stash pop", CommandRoute::Stash),
            ("/fork delete old", CommandRoute::Fork),
            ("/checkpoint restore cp1", CommandRoute::Checkpoint),
            ("/diff HEAD~3", CommandRoute::Diff),
            ("/blame src/dispatch.rs", CommandRoute::Blame),
            ("/undo src/main.rs", CommandRoute::Undo),
            ("/history detail 5", CommandRoute::History),
            ("/search pattern here", CommandRoute::Search),
            ("/changes json", CommandRoute::Changes),
            ("/export report.md", CommandRoute::Export),
            ("/mark milestone-1", CommandRoute::Mark),
            ("/jump milestone-1", CommandRoute::Jump),
            ("/commit initial commit", CommandRoute::Commit),
            ("/context files", CommandRoute::Context),
            ("/add src/lib.rs", CommandRoute::Add),
            ("/docs api", CommandRoute::Docs),
            ("/find *.toml", CommandRoute::Find),
            ("/grep TODO", CommandRoute::Grep),
            ("/rename old_fn new_fn", CommandRoute::Rename),
            ("/extract fn my_helper", CommandRoute::Extract),
            ("/move method to_impl", CommandRoute::Move),
            ("/refactor inline var", CommandRoute::Refactor),
            ("/remember this is important", CommandRoute::Remember),
            ("/memories list", CommandRoute::Memories),
            ("/forget 3", CommandRoute::Forget),
            ("/map src/tools.rs", CommandRoute::Map),
            ("/outline src/main.rs", CommandRoute::Outline),
            ("/tree docs/", CommandRoute::Tree),
            ("/web https://example.com", CommandRoute::Web),
            ("/open src/main.rs", CommandRoute::Open),
            ("/copy code", CommandRoute::Copy),
            ("/watch set cargo test", CommandRoute::Watch),
            ("/loop 3 do something", CommandRoute::Loop),
            ("/todo add fix tests", CommandRoute::Todo),
            ("/teach on", CommandRoute::Teach),
            ("/architect on", CommandRoute::Architect),
            ("/mcp status", CommandRoute::Mcp),
            ("/compact 10", CommandRoute::Compact),
            ("/ast pattern here", CommandRoute::Ast),
            ("/apply patch.diff", CommandRoute::Apply),
            ("/bg status 1", CommandRoute::Bg),
            ("/run echo hi", CommandRoute::Run),
            ("/pr create", CommandRoute::Pr),
            ("/git status", CommandRoute::Git),
            ("/goal set ship v1", CommandRoute::Goal),
            ("/spawn do task", CommandRoute::Spawn),
            ("/review main", CommandRoute::Review),
            ("/revisit scan", CommandRoute::Revisit),
            ("/skill show core", CommandRoute::Skill),
            ("/explain this function", CommandRoute::Explain),
            ("/plan next steps", CommandRoute::Plan),
            ("/extended long prompt", CommandRoute::Extended),
            ("/side quick question", CommandRoute::Side),
            ("/quick check this", CommandRoute::Quick),
            ("/tips", CommandRoute::Tips),
        ];
        for (input, expected) in cases {
            assert_eq!(
                route_command(input),
                expected,
                "Failed for input: {input:?}"
            );
        }
    }

    #[test]
    fn test_route_commands_with_multiple_spaces() {
        // Commands with extra spaces between command and args
        // split_whitespace handles multiple spaces gracefully
        assert_eq!(route_command("/add   file.rs"), CommandRoute::Add);
        assert_eq!(route_command("/grep   pattern"), CommandRoute::Grep);
        assert_eq!(route_command("/model   sonnet"), CommandRoute::Model);
    }

    #[test]
    fn test_route_help_with_various_args() {
        // /help routes through prefix matching (starts_with("help"))
        assert_eq!(route_command("/help"), CommandRoute::Help);
        assert_eq!(route_command("/help commands"), CommandRoute::Help);
        assert_eq!(route_command("/help model"), CommandRoute::Help);
        assert_eq!(route_command("/help config"), CommandRoute::Help);
        assert_eq!(route_command("/help nonexistent"), CommandRoute::Help);
    }

    #[test]
    fn test_route_debug_format() {
        // CommandRoute derives Debug — verify it's usable (compile-time + runtime)
        let route = route_command("/help");
        let debug_str = format!("{:?}", route);
        assert_eq!(debug_str, "Help");

        let route = route_command("/nonexistent");
        let debug_str = format!("{:?}", route);
        assert_eq!(debug_str, "UnknownSlash");

        let route = route_command("plain text");
        let debug_str = format!("{:?}", route);
        assert_eq!(debug_str, "NotACommand");
    }

    #[test]
    fn test_command_result_debug_format() {
        // CommandResult derives Debug — verify variants are formattable
        let cr = CommandResult::Continue;
        assert!(format!("{:?}", cr).contains("Continue"));

        let cr = CommandResult::Quit;
        assert!(format!("{:?}", cr).contains("Quit"));

        let cr = CommandResult::SendToAgent("test prompt".to_string());
        let debug = format!("{:?}", cr);
        assert!(debug.contains("SendToAgent"));
        assert!(debug.contains("test prompt"));

        let cr = CommandResult::NotACommand;
        assert!(format!("{:?}", cr).contains("NotACommand"));
    }

    #[test]
    fn test_route_exhaustive_exact_matches() {
        // Verify every exact-match branch in route_command's first match arm
        // maps to the correct variant (completeness check)
        let exact_matches: Vec<(&str, CommandRoute)> = vec![
            ("/quit", CommandRoute::Quit),
            ("/exit", CommandRoute::Quit),
            ("/version", CommandRoute::Version),
            ("/status", CommandRoute::Status),
            ("/tokens", CommandRoute::Tokens),
            ("/cost", CommandRoute::Cost),
            ("/profile", CommandRoute::Profile),
            ("/clear", CommandRoute::Clear),
            ("/clear!", CommandRoute::ClearForce),
            ("/model", CommandRoute::Model),
            ("/provider", CommandRoute::Provider),
            ("/think", CommandRoute::Think),
            ("/health", CommandRoute::Health),
            ("/doctor", CommandRoute::Doctor),
            ("/test", CommandRoute::Test),
            ("/lint fix", CommandRoute::LintFix),
            ("/fix", CommandRoute::Fix),
            ("/marks", CommandRoute::Marks),
            ("/config", CommandRoute::Config),
            ("/hooks", CommandRoute::Hooks),
            ("/permissions", CommandRoute::Permissions),
            ("/init", CommandRoute::Init),
            ("/index", CommandRoute::Index),
            ("/retry", CommandRoute::Retry),
            ("/run", CommandRoute::Run),
            ("/update", CommandRoute::Update),
            ("/docs", CommandRoute::Docs),
            ("/find", CommandRoute::Find),
            ("/grep", CommandRoute::Grep),
            ("/search", CommandRoute::Search),
            ("/tips", CommandRoute::Tips),
            ("/read", CommandRoute::Read),
        ];
        for (input, expected) in exact_matches {
            assert_eq!(
                route_command(input),
                expected,
                "Exact match failed for: {input:?}"
            );
        }
    }
}
