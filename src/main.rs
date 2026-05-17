//! yoyo — a coding agent that evolves itself.
//!
//! Started as ~200 lines. Grows one commit at a time.
//! Read IDENTITY.md and journals/JOURNAL.md for the full story.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=sk-... cargo run
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --thinking high
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --skills ./skills
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --mcp "npx -y @modelcontextprotocol/server-filesystem /tmp"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system "You are a Rust expert."
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system-file prompt.txt
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "explain this code"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "write a README" -o README.md
//!   echo "prompt" | cargo run  (piped mode: single prompt, no REPL)
//!
//! Commands:
//!   /quit, /exit    Exit the agent
//!   /add <path>     Add file contents to conversation (supports globs and line ranges)
//!   /clear          Clear conversation history
//!   /commit [msg]   Commit staged changes (AI-generates message if no msg)
//!   /docs <crate>   Look up docs.rs documentation for a Rust crate
//!   /docs <c> <i>   Look up a specific item within a crate
//!   /export [path]  Export conversation as readable markdown
//!   /find <pattern> Fuzzy-search project files by name
//!   /fix            Auto-fix build/lint errors (runs checks, sends failures to AI)
//!   /git <subcmd>   Quick git: status, log, add, diff, branch, stash
//!   /model <name>   Switch model mid-session
//!   /search <query> Search conversation history
//!   /spawn <task>   Spawn a subagent with fresh context
//!   /tree [depth]   Show project directory tree
//!   /test           Auto-detect and run project tests
//!   /lint           Auto-detect and run project linter
//!   /pr [number]    List open PRs, view/diff/comment/checkout a PR, or create one
//!   /retry          Re-send the last user input

mod agent_builder;
mod cli;
mod cli_config;
mod commands;
mod commands_ast_grep;
mod commands_bg;
mod commands_config;
mod commands_dev;
mod commands_file;
mod commands_fork;
mod commands_git;
mod commands_git_review;
mod commands_goal;
mod commands_info;
mod commands_lint;
mod commands_map;
mod commands_memory;
mod commands_move;
mod commands_plan;
mod commands_project;
mod commands_refactor;
mod commands_rename;
mod commands_retry;
mod commands_revisit;
mod commands_run;
mod commands_search;
mod commands_session;
mod commands_skill;
mod commands_spawn;
mod commands_stash;
mod commands_todo;
mod commands_tree;
mod commands_update;
mod commands_web;
mod config;
mod context;
mod conversations;
mod dispatch;
mod dispatch_sub;
mod docs;
mod format;
mod git;
mod help;
mod hooks;
mod memory;
mod prompt;
mod prompt_budget;
mod prompt_retry;
mod prompt_utils;
mod providers;
mod repl;
mod rtk;
mod safety;
mod session;
mod setup;
mod sync_util;
mod tool_wrappers;
mod tools;
mod update;
mod watch;

use cli::*;
use format::*;
use prompt::{
    run_prompt, run_prompt_stream_json, run_prompt_stream_json_with_content,
    run_prompt_with_content, PromptOutcome,
};
use prompt_budget::enable_audit_log;
use prompt_utils::write_output_file;
use session::SessionChanges;
use watch::{get_watch_command, run_watch_after_prompt, set_watch_command};

use agent_builder::try_fallback_prompt;
pub(crate) use agent_builder::{connect_external_servers, AgentConfig, FallbackRetry};

use std::io::{self, IsTerminal, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use yoagent::agent::Agent;
use yoagent::*;

/// Global flag: set to `true` when checkpoint mode's `on_before_turn` fires.
/// Checked at the end of `main()` to exit with code 2.
static CHECKPOINT_TRIGGERED: AtomicBool = AtomicBool::new(false);

/// Build a JSON output object for --json mode.
/// Used by both --prompt and piped modes to produce structured output.
fn build_json_output(
    response: &PromptOutcome,
    model: &str,
    usage: &Usage,
    is_error: bool,
    session_changes: &SessionChanges,
) -> String {
    let cost_usd = estimate_cost(usage, model);
    let json_obj = serde_json::json!({
        "response": response.text,
        "model": model,
        "usage": {
            "input_tokens": usage.input,
            "output_tokens": usage.output,
        },
        "cost_usd": cost_usd,
        "is_error": is_error,
        "session": session_changes.to_json_summary(),
    });
    serde_json::to_string(&json_obj).unwrap_or_else(|_| "{}".to_string())
}

/// Handle `--prompt / -p` single-shot mode: run one prompt (optionally with an
/// image), print the result (or write to `--output`), and return. Calls
/// `std::process::exit` on fatal errors (bad image, API failure with no
/// fallback).
#[allow(clippy::too_many_arguments)]
async fn run_single_prompt(
    agent_config: &mut AgentConfig,
    agent: &mut Agent,
    prompt_text: &str,
    image_path: &Option<String>,
    output_path: &Option<String>,
    json_output: bool,
    output_format: cli::OutputFormat,
    print_mode: bool,
) {
    // Stream-JSON mode: emit NDJSON events and return early
    if output_format == cli::OutputFormat::StreamJson {
        let mut session_total = Usage::default();
        let response = if let Some(ref img_path) = image_path {
            match commands_file::read_image_for_add(img_path) {
                Ok((data, mime_type)) => {
                    let content_blocks = vec![
                        Content::Text {
                            text: prompt_text.trim().to_string(),
                        },
                        Content::Image { data, mime_type },
                    ];
                    run_prompt_stream_json_with_content(
                        agent,
                        content_blocks,
                        &mut session_total,
                        &agent_config.model,
                    )
                    .await
                }
                Err(e) => {
                    eprintln!("{RED}  error: {e}{RESET}");
                    std::process::exit(1);
                }
            }
        } else {
            run_prompt_stream_json(
                agent,
                prompt_text.trim(),
                &mut session_total,
                &agent_config.model,
            )
            .await
        };
        if response.last_api_error.is_some() {
            std::process::exit(1);
        }
        return;
    }

    if !print_mode {
        if agent_config.provider != "anthropic" {
            eprintln!(
                "{DIM}  yoyo (prompt mode) — provider: {}, model: {}{RESET}",
                agent_config.provider, agent_config.model
            );
        } else {
            eprintln!(
                "{DIM}  yoyo (prompt mode) — model: {}{RESET}",
                agent_config.model
            );
        }
    }

    // Auto-enable watch mode if a project type is detected and config allows it
    if get_watch_command().is_none() && agent_config.auto_watch {
        if let Some(cmd) = watch::auto_detect_watch_command() {
            set_watch_command(&cmd);
            if !print_mode {
                eprintln!("{DIM}  👀 Auto-watch: `{cmd}` (disable with auto_watch = false){RESET}");
            }
        }
    }

    let mut session_total = Usage::default();
    let session_changes = SessionChanges::new();
    let prompt_start = Instant::now();
    let response = if let Some(ref img_path) = image_path {
        // Multi-modal prompt: text + image
        match commands_file::read_image_for_add(img_path) {
            Ok((data, mime_type)) => {
                let content_blocks = vec![
                    Content::Text {
                        text: prompt_text.trim().to_string(),
                    },
                    Content::Image {
                        data: data.clone(),
                        mime_type: mime_type.clone(),
                    },
                ];
                let initial = run_prompt_with_content(
                    agent,
                    content_blocks,
                    &mut session_total,
                    &agent_config.model,
                )
                .await;
                // Fallback retry for multi-modal prompts
                let retry_blocks = vec![
                    Content::Text {
                        text: prompt_text.trim().to_string(),
                    },
                    Content::Image { data, mime_type },
                ];
                let (final_response, should_exit_error) = try_fallback_prompt(
                    agent_config,
                    agent,
                    FallbackRetry::Content(retry_blocks),
                    &mut session_total,
                    initial,
                )
                .await;
                if should_exit_error {
                    format::maybe_ring_bell(prompt_start.elapsed());
                    if print_mode {
                        print!("{}", final_response.text);
                    } else if json_output {
                        println!(
                            "{}",
                            build_json_output(
                                &final_response,
                                &agent_config.model,
                                &session_total,
                                true,
                                &session_changes,
                            )
                        );
                    } else {
                        write_output_file(output_path, &final_response.text);
                    }
                    std::process::exit(1);
                }
                final_response
            }
            Err(e) => {
                eprintln!("{RED}  error: {e}{RESET}");
                std::process::exit(1);
            }
        }
    } else {
        // Text-only prompt
        let initial = run_prompt(
            agent,
            prompt_text.trim(),
            &mut session_total,
            &agent_config.model,
        )
        .await;
        // Fallback retry for text-only prompts
        let (final_response, should_exit_error) = try_fallback_prompt(
            agent_config,
            agent,
            FallbackRetry::Text(prompt_text.trim()),
            &mut session_total,
            initial,
        )
        .await;
        if should_exit_error {
            format::maybe_ring_bell(prompt_start.elapsed());
            if print_mode {
                print!("{}", final_response.text);
            } else if json_output {
                println!(
                    "{}",
                    build_json_output(
                        &final_response,
                        &agent_config.model,
                        &session_total,
                        true,
                        &session_changes
                    )
                );
            } else {
                write_output_file(output_path, &final_response.text);
            }
            std::process::exit(1);
        }
        final_response
    };

    // Run watch command after prompt if active (auto lint/test loop)
    run_watch_after_prompt(
        agent,
        &mut session_total,
        &agent_config.model,
        &session_changes,
    )
    .await;

    format::maybe_ring_bell(prompt_start.elapsed());
    if print_mode {
        print!("{}", response.text);
    } else if json_output {
        println!(
            "{}",
            build_json_output(
                &response,
                &agent_config.model,
                &session_total,
                false,
                &session_changes
            )
        );
    } else {
        write_output_file(output_path, &response.text);
    }
    if CHECKPOINT_TRIGGERED.load(Ordering::SeqCst) {
        std::process::exit(2);
    }
}

/// Handle piped mode: read all of stdin, run a single prompt, print/write the
/// result, and return. Calls `std::process::exit` on empty input or fatal API
/// errors.
/// Returns true if `input` looks like a slash command (its first non-whitespace
/// character is `/`). Slash commands belong to the REPL; piped mode can't
/// dispatch them, so we use this to warn the user instead of wasting a turn.
fn looks_like_slash_command(input: &str) -> bool {
    matches!(input.trim_start().chars().next(), Some('/'))
}

async fn run_piped_mode(
    agent_config: &mut AgentConfig,
    agent: &mut Agent,
    output_path: &Option<String>,
    json_output: bool,
    output_format: cli::OutputFormat,
    print_mode: bool,
) {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("Error reading stdin: {e}");
        std::process::exit(1);
    }
    let input = input.trim();
    if input.is_empty() {
        eprintln!("No input on stdin.");
        std::process::exit(1);
    }

    // Piped mode can't dispatch slash commands (they need REPL state). If the
    // user piped one in, warn them and exit instead of burning tokens letting
    // the model puzzle over the literal string.
    if looks_like_slash_command(input) {
        eprintln!("{YELLOW}yoyo: slash commands aren't available in piped mode.{RESET}");
        eprintln!("  Try one of:");
        eprintln!("    yoyo doctor                    # run a subcommand directly");
        eprintln!("    yoyo --prompt \"{input}\"        # send the literal text to the agent");
        eprintln!("    yoyo                           # interactive REPL");
        std::process::exit(2);
    }

    // Stream-JSON mode: emit NDJSON events and return early
    if output_format == cli::OutputFormat::StreamJson {
        let mut session_total = Usage::default();
        let response =
            run_prompt_stream_json(agent, input, &mut session_total, &agent_config.model).await;
        if response.last_api_error.is_some() {
            std::process::exit(1);
        }
        return;
    }

    if !print_mode {
        eprintln!(
            "{DIM}  yoyo (piped mode) — model: {}{RESET}",
            agent_config.model
        );
    }

    // Auto-enable watch mode if a project type is detected and config allows it
    if get_watch_command().is_none() && agent_config.auto_watch {
        if let Some(cmd) = watch::auto_detect_watch_command() {
            set_watch_command(&cmd);
            if !print_mode {
                eprintln!("{DIM}  👀 Auto-watch: `{cmd}` (disable with auto_watch = false){RESET}");
            }
        }
    }

    let mut session_total = Usage::default();
    let session_changes = SessionChanges::new();
    let prompt_start = Instant::now();
    let initial = run_prompt(agent, input, &mut session_total, &agent_config.model).await;
    // Fallback retry for piped mode
    let (response, should_exit_error) = try_fallback_prompt(
        agent_config,
        agent,
        FallbackRetry::Text(input),
        &mut session_total,
        initial,
    )
    .await;

    // Run watch command after prompt if active (auto lint/test loop)
    if !should_exit_error {
        run_watch_after_prompt(
            agent,
            &mut session_total,
            &agent_config.model,
            &session_changes,
        )
        .await;
    }

    format::maybe_ring_bell(prompt_start.elapsed());
    if print_mode {
        print!("{}", response.text);
    } else if json_output {
        println!(
            "{}",
            build_json_output(
                &response,
                &agent_config.model,
                &session_total,
                should_exit_error,
                &session_changes,
            )
        );
    } else {
        write_output_file(output_path, &response.text);
    }
    if should_exit_error {
        std::process::exit(1);
    }
    if CHECKPOINT_TRIGGERED.load(Ordering::SeqCst) {
        std::process::exit(2);
    }
}

/// Apply early CLI flags that must take effect before `parse_args()` produces
/// any output.  Handles `--no-color`, `--no-bell`, `--no-notify`, and `--no-rtk`.
fn apply_cli_flags(args: &[String]) {
    // Auto-disable color when stdout is not a terminal (piped output)
    if args.iter().any(|a| a == "--no-color") || !io::stdout().is_terminal() {
        disable_color();
    }

    if args.iter().any(|a| a == "--no-bell") {
        disable_bell();
    }

    if args.iter().any(|a| a == "--no-notify") {
        disable_notify();
    }

    // Also respects YOYO_NO_RTK env var
    if args.iter().any(|a| a == "--no-rtk")
        || std::env::var("YOYO_NO_RTK")
            .map(|v| v == "1")
            .unwrap_or(false)
    {
        rtk::disable_rtk();
    }
}

/// Apply config-level flags that don't need the agent.  Handles
/// `--print-system-prompt` (early exit), `--verbose`, and `--audit`.
/// Returns `false` if main should exit immediately (early-exit path handled).
fn apply_config_flags(config: &Config) -> bool {
    if config.print_system_prompt {
        println!("{}", config.system_prompt);
        return false;
    }

    if config.verbose {
        enable_verbose();
    }

    if config.audit {
        enable_audit_log();
    }

    true
}

/// Run the interactive setup wizard if needed and apply its results to `agent_config`.
/// Returns `false` if the user cancelled and main should exit.
fn run_setup_wizard_if_needed(is_interactive: bool, agent_config: &mut AgentConfig) -> bool {
    if !is_interactive || !setup::needs_setup(&agent_config.provider) {
        return true;
    }

    if let Some(result) = setup::run_setup_wizard() {
        agent_config.provider = result.provider.clone();
        agent_config.api_key = result.api_key.clone();
        agent_config.model = result.model;
        if result.base_url.is_some() {
            agent_config.base_url = result.base_url;
        }
        // Set the env var so the provider builder picks it up
        if let Some(env_var) = cli::provider_api_key_env(&result.provider) {
            // SAFETY: This runs during setup, before any concurrent agent work.
            // The env var is read later by the provider builder on the same thread.
            unsafe {
                std::env::set_var(env_var, &result.api_key);
            }
        }
        true
    } else {
        // User cancelled — show the static welcome screen
        cli::print_welcome();
        false
    }
}

/// Assemble combined AWS credentials for Bedrock if the api_key is a bare
/// access key (no `:` separator).
fn apply_bedrock_credentials(agent_config: &mut AgentConfig) {
    if agent_config.provider != "bedrock" || agent_config.api_key.contains(':') {
        return;
    }
    let access_key = agent_config.api_key.clone();
    if let Ok(secret) = std::env::var("AWS_SECRET_ACCESS_KEY") {
        agent_config.api_key = match std::env::var("AWS_SESSION_TOKEN") {
            Ok(token) if !token.is_empty() => format!("{access_key}:{secret}:{token}"),
            _ => format!("{access_key}:{secret}"),
        };
    }
}

/// Restore a previously-saved session into the agent.
fn restore_session(agent: &mut Agent) {
    let session_path = commands_session::continue_session_path();
    match std::fs::read_to_string(session_path) {
        Ok(json) => match agent.restore_messages(&json) {
            Ok(_) => {
                eprintln!(
                    "{DIM}  resumed session: {} messages from {session_path}{RESET}",
                    agent.messages().len()
                );
            }
            Err(e) => eprintln!("{YELLOW}warning:{RESET} Failed to restore session: {e}"),
        },
        Err(_) => eprintln!("{DIM}  no previous session found ({session_path}){RESET}"),
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    apply_cli_flags(&args);

    let Some(config) = parse_args(&args) else {
        return; // --help or --version was handled
    };

    if !apply_config_flags(&config) {
        return;
    }

    if config.no_tools {
        eprintln!(
            "{}note:{} All tools disabled (--no-tools). Running in chat-only mode.",
            crate::format::YELLOW,
            crate::format::RESET,
        );
    }

    let continue_session = config.continue_session;
    let output_path = config.output_path;
    let mcp_servers = config.mcp_servers;
    let mcp_server_configs = config.mcp_server_configs;
    let openapi_specs = config.openapi_specs;
    let image_path = config.image_path;
    let no_update_check = config.no_update_check;
    let json_output = config.json_output;
    let output_format = config.output_format;
    let print_mode = config.print_mode;
    let is_interactive = io::stdin().is_terminal() && config.prompt_arg.is_none();
    let auto_approve = config.auto_approve || !is_interactive;

    let mut agent_config = AgentConfig {
        model: config.model,
        api_key: config.api_key,
        provider: config.provider,
        base_url: config.base_url,
        skills: config.skills,
        system_prompt: config.system_prompt,
        thinking: config.thinking,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        max_turns: config.max_turns,
        auto_approve,
        auto_commit: config.auto_commit,
        permissions: config.permissions,
        dir_restrictions: config.dir_restrictions,
        context_strategy: config.context_strategy,
        context_window: config.context_window,
        shell_hooks: config.shell_hooks,
        fallback_provider: config.fallback_provider,
        fallback_model: config.fallback_model,
        auto_watch: config.auto_watch,
        disallowed_tools: config.disallowed_tools,
        no_tools: config.no_tools,
    };

    if !run_setup_wizard_if_needed(is_interactive, &mut agent_config) {
        return;
    }

    apply_bedrock_credentials(&mut agent_config);

    let mut agent = agent_config.build_agent();

    // Connect to external servers (MCP + OpenAPI) — skip when --no-tools is active
    // since external servers only add more tools which won't be used.
    let (mcp_count, openapi_count) = if config.no_tools {
        (0, 0)
    } else {
        let (updated_agent, mc, oa) = connect_external_servers(
            &agent_config,
            agent,
            &mcp_servers,
            &mcp_server_configs,
            &openapi_specs,
        )
        .await;
        agent = updated_agent;
        (mc, oa)
    };

    if continue_session {
        restore_session(&mut agent);
    }

    // --prompt / -p: single-shot mode
    if let Some(prompt_text) = config.prompt_arg {
        // --print: suppress color on terminal stdout and disable color codes
        if print_mode {
            disable_color();
        }
        run_single_prompt(
            &mut agent_config,
            &mut agent,
            &prompt_text,
            &image_path,
            &output_path,
            json_output,
            output_format,
            print_mode,
        )
        .await;
        return;
    }

    // Piped mode: read all of stdin as a single prompt, run once, exit
    if !io::stdin().is_terminal() {
        if print_mode {
            disable_color();
        }
        run_piped_mode(
            &mut agent_config,
            &mut agent,
            &output_path,
            json_output,
            output_format,
            print_mode,
        )
        .await;
        return;
    }

    // --print without -p or piped input: warn and ignore
    if print_mode {
        eprintln!("{YELLOW}warning:{RESET} --print requires -p/--prompt or piped input, ignoring");
    }

    // Interactive REPL mode
    let update_available = if !no_update_check {
        update::check_for_update(cli::VERSION)
    } else {
        None
    };

    repl::run_repl(
        &mut agent_config,
        &mut agent,
        repl::ReplConfig {
            mcp_count,
            openapi_count,
            continue_session,
            update_available,
            mcp_cli_servers: mcp_servers,
            mcp_server_configs,
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn looks_like_slash_command_detects_leading_slash() {
        assert!(looks_like_slash_command("/doctor"));
        assert!(looks_like_slash_command("/help"));
        assert!(looks_like_slash_command("/"));
    }

    #[test]
    fn looks_like_slash_command_handles_leading_whitespace() {
        // The caller already trims, but we should be robust to \n/doctor\n etc.
        assert!(looks_like_slash_command("  /doctor"));
        assert!(looks_like_slash_command("\n/doctor\n"));
        assert!(looks_like_slash_command("\t/status"));
    }

    #[test]
    fn looks_like_slash_command_rejects_mid_string_slash() {
        // A slash that isn't the first non-whitespace character must NOT trigger.
        assert!(!looks_like_slash_command("what does /doctor do?"));
        assert!(!looks_like_slash_command("explain /help to me"));
        assert!(!looks_like_slash_command("path: a/b/c"));
    }

    #[test]
    fn looks_like_slash_command_rejects_non_slash_input() {
        assert!(!looks_like_slash_command("hello"));
        assert!(!looks_like_slash_command(""));
        assert!(!looks_like_slash_command("   "));
        assert!(!looks_like_slash_command("-flag"));
    }

    #[test]
    fn test_always_approve_flag_starts_false() {
        // The "always" flag should start as false
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_checkpoint_triggered_flag_starts_false() {
        // CHECKPOINT_TRIGGERED should default to false
        assert!(!CHECKPOINT_TRIGGERED.load(Ordering::SeqCst));
    }

    #[test]
    fn test_always_approve_flag_persists_across_clones() {
        // Simulates the confirm closure: flag is shared via Arc
        let always_approved = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&always_approved);

        // Initially not set
        assert!(!flag_clone.load(Ordering::Relaxed));

        // User answers "always" — set the flag
        always_approved.store(true, Ordering::Relaxed);

        // The clone sees the update (simulates next confirm call)
        assert!(flag_clone.load(Ordering::Relaxed));
    }

    #[test]
    fn test_always_approve_response_matching() {
        // Verify the response matching logic for "always" variants
        let responses_that_approve = ["y", "yes", "a", "always"];
        let responses_that_deny = ["n", "no", "", "maybe", "nope"];

        for r in &responses_that_approve {
            let normalized = r.trim().to_lowercase();
            assert!(
                matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
                "Expected '{}' to be approved",
                r
            );
        }

        for r in &responses_that_deny {
            let normalized = r.trim().to_lowercase();
            assert!(
                !matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
                "Expected '{}' to be denied",
                r
            );
        }
    }

    #[test]
    fn test_always_approve_only_on_a_or_always() {
        // Only "a" and "always" should set the persist flag, not "y" or "yes"
        let always_responses = ["a", "always"];
        let single_responses = ["y", "yes"];

        for r in &always_responses {
            let normalized = r.trim().to_lowercase();
            assert!(
                matches!(normalized.as_str(), "a" | "always"),
                "Expected '{}' to trigger always-approve",
                r
            );
        }

        for r in &single_responses {
            let normalized = r.trim().to_lowercase();
            assert!(
                !matches!(normalized.as_str(), "a" | "always"),
                "Expected '{}' NOT to trigger always-approve",
                r
            );
        }
    }

    #[test]
    fn test_always_approve_flag_used_in_confirm_simulation() {
        // End-to-end simulation of the confirm flow with "always"
        let always_approved = Arc::new(AtomicBool::new(false));

        // Simulate three bash commands in sequence
        let commands = ["ls", "echo hello", "cat file.txt"];
        let user_responses = ["a", "", ""]; // user answers "always" first time

        for (i, cmd) in commands.iter().enumerate() {
            let approved = if always_approved.load(Ordering::Relaxed) {
                // Auto-approved — no prompt needed
                true
            } else {
                let response = user_responses[i].trim().to_lowercase();
                let result = matches!(response.as_str(), "y" | "yes" | "a" | "always");
                if matches!(response.as_str(), "a" | "always") {
                    always_approved.store(true, Ordering::Relaxed);
                }
                result
            };

            match i {
                0 => assert!(
                    approved,
                    "First command '{}' should be approved via 'a'",
                    cmd
                ),
                1 => assert!(approved, "Second command '{}' should be auto-approved", cmd),
                2 => assert!(approved, "Third command '{}' should be auto-approved", cmd),
                _ => unreachable!(),
            }
        }
    }

    /// Helper to create a default AgentConfig for tests.
    fn test_agent_config(provider: &str, model: &str) -> AgentConfig {
        AgentConfig {
            model: model.to_string(),
            api_key: "test-key".to_string(),
            provider: provider.to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test prompt.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
        }
    }

    #[test]
    fn test_build_json_output_valid_json_with_expected_keys() {
        let response = PromptOutcome {
            text: "Hello, world!".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 150,
        };
        let result = build_json_output(
            &response,
            "claude-sonnet-4-20250514",
            &usage,
            false,
            &SessionChanges::new(),
        );

        // Must be valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("build_json_output should produce valid JSON");

        // Check all expected keys exist
        assert_eq!(parsed["response"], "Hello, world!");
        assert_eq!(parsed["model"], "claude-sonnet-4-20250514");
        assert_eq!(parsed["is_error"], false);
        assert!(parsed["usage"].is_object());
        assert_eq!(parsed["usage"]["input_tokens"], 100);
        assert_eq!(parsed["usage"]["output_tokens"], 50);
        assert!(parsed["cost_usd"].is_number());
    }

    #[test]
    fn test_build_json_output_error_mode() {
        let response = PromptOutcome {
            text: "Something went wrong".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: Some("API error".to_string()),
        };
        let usage = Usage {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 15,
        };
        let result = build_json_output(
            &response,
            "claude-sonnet-4-20250514",
            &usage,
            true,
            &SessionChanges::new(),
        );

        let parsed: serde_json::Value = serde_json::from_str(&result)
            .expect("build_json_output should produce valid JSON even in error mode");

        assert_eq!(parsed["response"], "Something went wrong");
        assert_eq!(parsed["is_error"], true);
        assert!(parsed["usage"].is_object());
        assert!(parsed["cost_usd"].is_number());
    }

    #[test]
    fn bedrock_credentials_noop_for_non_bedrock() {
        let mut config = test_agent_config("anthropic", "test-model");
        config.api_key = "sk-test".to_string();
        apply_bedrock_credentials(&mut config);
        assert_eq!(config.api_key, "sk-test");
    }

    #[test]
    fn bedrock_credentials_noop_when_already_combined() {
        let mut config = test_agent_config("bedrock", "test-model");
        config.api_key = "access:secret".to_string();
        apply_bedrock_credentials(&mut config);
        assert_eq!(config.api_key, "access:secret");
    }

    #[test]
    #[serial]
    fn bedrock_credentials_combines_access_and_secret() {
        // SAFETY: test runs serially, no concurrent readers
        unsafe {
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "my-secret");
            std::env::remove_var("AWS_SESSION_TOKEN");
        }
        let mut config = test_agent_config("bedrock", "test-model");
        config.api_key = "my-access".to_string();
        apply_bedrock_credentials(&mut config);
        assert_eq!(config.api_key, "my-access:my-secret");
        unsafe {
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        }
    }

    #[test]
    #[serial]
    fn bedrock_credentials_includes_session_token() {
        // SAFETY: test runs serially, no concurrent readers
        unsafe {
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "my-secret");
            std::env::set_var("AWS_SESSION_TOKEN", "my-token");
        }
        let mut config = test_agent_config("bedrock", "test-model");
        config.api_key = "my-access".to_string();
        apply_bedrock_credentials(&mut config);
        assert_eq!(config.api_key, "my-access:my-secret:my-token");
        unsafe {
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
            std::env::remove_var("AWS_SESSION_TOKEN");
        }
    }

    // --- build_json_output tests ---

    #[test]
    fn test_build_json_output_empty_text() {
        let response = PromptOutcome {
            text: String::new(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let result = build_json_output(
            &response,
            "test-model",
            &usage,
            false,
            &SessionChanges::new(),
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("empty text should produce valid JSON");
        assert_eq!(parsed["response"], "");
        assert_eq!(parsed["is_error"], false);
    }

    #[test]
    fn test_build_json_output_special_characters() {
        // Quotes, newlines, unicode — all must be properly escaped in JSON
        let response = PromptOutcome {
            text: "He said \"hello\"\nnew line\ttab\u{2713} checkmark".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 10,
            output: 20,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 30,
        };
        let result = build_json_output(
            &response,
            "test-model",
            &usage,
            false,
            &SessionChanges::new(),
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("special chars should produce valid JSON");
        // The response field should contain the original text with special chars intact
        assert!(parsed["response"].as_str().unwrap().contains("\"hello\""));
        assert!(parsed["response"].as_str().unwrap().contains('\n'));
        assert!(parsed["response"].as_str().unwrap().contains('\u{2713}'));
    }

    #[test]
    fn test_build_json_output_structure_completeness() {
        // Verify that all and only the expected top-level keys are present
        let response = PromptOutcome {
            text: "test".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 2,
        };
        let result = build_json_output(&response, "m", &usage, false, &SessionChanges::new());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let obj = parsed.as_object().unwrap();

        // Exactly 6 top-level keys
        assert_eq!(
            obj.len(),
            6,
            "expected 6 top-level keys, got {:?}",
            obj.keys().collect::<Vec<_>>()
        );
        assert!(obj.contains_key("response"));
        assert!(obj.contains_key("model"));
        assert!(obj.contains_key("usage"));
        assert!(obj.contains_key("cost_usd"));
        assert!(obj.contains_key("is_error"));
        assert!(obj.contains_key("session"));

        // usage sub-object has exactly 2 keys
        let usage_obj = parsed["usage"].as_object().unwrap();
        assert_eq!(usage_obj.len(), 2);
        assert!(usage_obj.contains_key("input_tokens"));
        assert!(usage_obj.contains_key("output_tokens"));
    }

    #[test]
    fn test_build_json_output_cost_is_non_negative() {
        let response = PromptOutcome {
            text: "x".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 1000,
            output: 500,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 1500,
        };
        let result = build_json_output(
            &response,
            "claude-sonnet-4-20250514",
            &usage,
            false,
            &SessionChanges::new(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let cost = parsed["cost_usd"].as_f64().unwrap();
        assert!(cost >= 0.0, "cost should be non-negative, got {}", cost);
    }

    #[test]
    fn test_build_json_output_unknown_model_still_valid() {
        // Even with an unknown model (where cost estimation may return 0), JSON is valid
        let response = PromptOutcome {
            text: "result".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 50,
            output: 25,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 75,
        };
        let result = build_json_output(
            &response,
            "unknown-model-xyz",
            &usage,
            false,
            &SessionChanges::new(),
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("unknown model should still produce valid JSON");
        assert_eq!(parsed["model"], "unknown-model-xyz");
    }

    #[test]
    fn test_build_json_output_includes_session_changes() {
        let response = PromptOutcome {
            text: "done".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 100,
            output: 50,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 150,
        };
        let changes = SessionChanges::new();
        changes.record("src/main.rs", session::ChangeKind::Write);
        changes.record("src/cli.rs", session::ChangeKind::Edit);

        let result = build_json_output(&response, "test-model", &usage, false, &changes);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // session key must exist
        assert!(
            parsed["session"].is_object(),
            "expected 'session' key in JSON output"
        );
        assert_eq!(parsed["session"]["files_changed"], 2);

        let arr = parsed["session"]["changes"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["path"], "src/main.rs");
        assert_eq!(arr[0]["kind"], "write");
        assert_eq!(arr[1]["path"], "src/cli.rs");
        assert_eq!(arr[1]["kind"], "edit");
    }

    #[test]
    fn test_build_json_output_empty_session_changes() {
        let response = PromptOutcome {
            text: "nothing changed".to_string(),
            last_tool_error: None,
            last_tool_name: None,
            was_overflow: false,
            last_api_error: None,
        };
        let usage = Usage {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 15,
        };
        let changes = SessionChanges::new();

        let result = build_json_output(&response, "test-model", &usage, false, &changes);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["session"]["files_changed"], 0);
        assert!(parsed["session"]["changes"].as_array().unwrap().is_empty());
    }

    // --- looks_like_slash_command edge case tests ---

    #[test]
    fn looks_like_slash_command_slash_followed_by_numbers() {
        // /123 is technically a slash command (starts with /)
        assert!(looks_like_slash_command("/123"));
        assert!(looks_like_slash_command("/42foo"));
    }

    #[test]
    fn looks_like_slash_command_only_whitespace_before_slash() {
        assert!(looks_like_slash_command("   /test"));
        assert!(looks_like_slash_command("\t\t/test"));
        assert!(looks_like_slash_command(" \n \t /test"));
    }

    #[test]
    fn looks_like_slash_command_empty_and_whitespace() {
        assert!(!looks_like_slash_command(""));
        assert!(!looks_like_slash_command("   "));
        assert!(!looks_like_slash_command("\n\t\n"));
    }

    #[test]
    fn looks_like_slash_command_slash_only() {
        // A single "/" should still be detected as a slash command
        assert!(looks_like_slash_command("/"));
        assert!(looks_like_slash_command("  /"));
    }

    #[test]
    fn looks_like_slash_command_unicode_after_slash() {
        assert!(looks_like_slash_command("/café"));
        assert!(looks_like_slash_command("/日本語"));
    }

    // --- apply_config_flags tests ---

    /// Helper to build a minimal Config for testing apply_config_flags.
    fn test_config() -> Config {
        Config {
            model: String::new(),
            api_key: String::new(),
            provider: String::new(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: String::new(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            continue_session: false,
            output_path: None,
            prompt_arg: None,
            image_path: None,
            verbose: false,
            mcp_servers: vec![],
            mcp_server_configs: vec![],
            openapi_specs: vec![],
            auto_approve: false,
            auto_commit: false,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            no_update_check: false,
            json_output: false,
            output_format: cli::OutputFormat::Text,
            audit: false,
            print_system_prompt: false,
            print_mode: false,
            auto_watch: true,
            disallowed_tools: vec![],
            no_tools: false,
        }
    }

    #[test]
    fn test_apply_config_flags_default_returns_true() {
        // Default config (all false) should return true (continue execution)
        let config = test_config();
        assert!(apply_config_flags(&config));
    }

    #[test]
    fn test_apply_config_flags_print_system_prompt_returns_false() {
        // When print_system_prompt is true, function should return false (early exit)
        // We can't easily capture stdout here, but we can verify the return value.
        // This test will print to stdout as a side effect, which is acceptable.
        let mut config = test_config();
        config.print_system_prompt = true;
        config.system_prompt = "test system prompt".to_string();
        assert!(!apply_config_flags(&config));
    }

    // --- apply_cli_flags tests ---

    #[test]
    fn test_apply_cli_flags_unknown_flags_ignored() {
        // Unknown flags should not panic or cause errors
        let args = vec![
            "yoyo".to_string(),
            "--unknown-flag".to_string(),
            "--another".to_string(),
        ];
        apply_cli_flags(&args); // should not panic
    }

    #[test]
    fn test_apply_cli_flags_empty_args() {
        // Empty args list should not panic
        let args: Vec<String> = vec![];
        apply_cli_flags(&args); // should not panic
    }

    #[test]
    fn test_apply_cli_flags_mixed_known_and_unknown() {
        // Mix of known and unknown flags should process known ones without error
        let args = vec![
            "yoyo".to_string(),
            "--no-bell".to_string(),
            "--unknown".to_string(),
            "--no-notify".to_string(),
        ];
        apply_cli_flags(&args); // should not panic
    }

    #[test]
    #[serial]
    fn test_apply_cli_flags_no_rtk_via_env() {
        // --no-rtk should also be settable via YOYO_NO_RTK=1 env var
        unsafe {
            std::env::set_var("YOYO_NO_RTK", "1");
        }
        let args = vec!["yoyo".to_string()];
        apply_cli_flags(&args); // should trigger rtk disable via env
        unsafe {
            std::env::remove_var("YOYO_NO_RTK");
        }
    }
}
