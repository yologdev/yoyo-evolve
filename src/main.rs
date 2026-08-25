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
mod banner;
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
mod commands_git_pr;
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
mod commands_risk;
mod commands_risk_accuracy;
mod commands_risk_emerging;
mod commands_risk_epistemic;
#[cfg(test)]
mod commands_risk_epistemic_tests;
mod commands_risk_families;
mod commands_risk_neverforecast;
mod commands_risk_parse;
mod commands_risk_report;
mod commands_risk_snapshots;
mod commands_risk_ungraded;
mod commands_risk_weights;
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
mod config_paths;
mod context;
mod conversations;
mod dispatch;
mod dispatch_near_miss;
mod dispatch_sub;
mod docs;
mod format;
#[cfg(feature = "gasp")]
mod gasp;
// Unconditional: the argument contract in here is compiled and table-tested by a
// plain `cargo test`, which compiles none of `gasp` above. Only the half that
// touches the store is behind the feature.
mod gasp_cli;
mod git;
mod git_commit_msg;
mod help;
mod help_data;
#[cfg(test)]
mod help_data_guards;
mod hooks;
/// Unit tests for this file, moved out on Day 172 so `main.rs` has room under
/// the module-size gate. Sibling of `help_data_guards` / `commands_risk_epistemic_tests`.
#[cfg(test)]
mod main_tests;
mod memory;
mod prompt;
mod prompt_budget;
mod prompt_retry;
mod prompt_retry_limits;
mod prompt_utils;
mod providers;
mod repl;
mod rtk;
mod safety;
mod session;
mod setup;
mod smart_edit;
mod symbols;
mod sync_util;
mod tool_truncate;
mod tool_wrappers;
mod tools;
mod update;
mod watch;

use cli::*;
use format::*;
use prompt::{
    run_prompt_stream_json, run_prompt_stream_json_with_content, run_prompt_with_changes,
    run_prompt_with_content_and_changes, PromptOutcome,
};
use prompt_budget::enable_audit_log;
use prompt_utils::write_output_file;
use session::SessionChanges;
use watch::{get_watch_command, run_watch_after_prompt};

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
    duration: std::time::Duration,
    num_turns: usize,
) -> String {
    let cost_usd = estimate_cost(usage, model);
    let json_obj = serde_json::json!({
        "response": response.text,
        "model": model,
        "usage": {
            "input_tokens": usage.input,
            "output_tokens": usage.output,
            "cache_read_input_tokens": usage.cache_read,
            "cache_creation_input_tokens": usage.cache_write,
        },
        "cost_usd": cost_usd,
        "duration_ms": duration.as_millis() as u64,
        "num_turns": num_turns,
        "is_error": is_error,
        "session": session_changes.to_json_summary(),
    });
    serde_json::to_string(&json_obj).unwrap_or_else(|_| "{}".to_string())
}

/// Count the number of assistant turns in the agent's message history.
fn count_assistant_turns(agent: &Agent) -> usize {
    agent
        .messages()
        .iter()
        .filter(|m| matches!(m.as_llm(), Some(yoagent::Message::Assistant { .. })))
        .count()
}

/// Emit the final response in the appropriate output mode.
///
/// Three stdout modes:
/// - `print_mode`: raw text to stdout (for shell scripting)
/// - `json_output`: JSON envelope to stdout (for structured consumers)
/// - otherwise: nothing extra (the response already streamed to the terminal)
///
/// `output_path` (`--output`) is independent of all three and is honored in
/// every one of them.
///
/// Returns `true` when an `--output` write was attempted and **failed**, so the
/// caller can set a non-zero exit code. Scripts are the only audience `-o` has,
/// and exiting 0 after writing no file tells them the opposite of the truth.
#[allow(clippy::too_many_arguments)]
fn emit_output(
    response: &PromptOutcome,
    model: &str,
    usage: &Usage,
    is_error: bool,
    session_changes: &SessionChanges,
    output_path: &Option<String>,
    json_output: bool,
    print_mode: bool,
    duration: std::time::Duration,
    num_turns: usize,
) -> bool {
    if print_mode {
        print!("{}", response.text);
    } else if json_output {
        println!(
            "{}",
            build_json_output(
                response,
                model,
                usage,
                is_error,
                session_changes,
                duration,
                num_turns,
            )
        );
    }
    // `--output` is honored in *every* mode, not just the default one. It and
    // `--print` / `--output-format json` are all scripting flags, so reaching
    // for them together is the natural thing to do — and until #766 the file
    // write was simply skipped under the other two branches, with no file, no
    // error, and nothing on stderr to notice.
    write_output_file(output_path, &response.text).is_err()
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
    // #786: the one tracker for this whole function. Constructed above the
    // stream-json early return so that branch records into the *same* tracker
    // the later `build_json_output` / `emit_output` read — a second throwaway
    // one here is the exact defect of #678.
    let session_changes = SessionChanges::new();

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
                        &session_changes,
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
                &session_changes,
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
        watch::arm_auto_watch("auto_watch = false", print_mode);
    } else if get_watch_command().is_none() && !agent_config.auto_watch && !print_mode {
        watch::hint_auto_watch_available();
    }

    let mut session_total = Usage::default();
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
                let initial = run_prompt_with_content_and_changes(
                    agent,
                    content_blocks,
                    &mut session_total,
                    &agent_config.model,
                    &session_changes,
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
                    // Return ignored on purpose: this branch already exits 1.
                    let _ = emit_output(
                        &final_response,
                        &agent_config.model,
                        &session_total,
                        true,
                        &session_changes,
                        output_path,
                        json_output,
                        print_mode,
                        prompt_start.elapsed(),
                        count_assistant_turns(agent),
                    );
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
        let initial = run_prompt_with_changes(
            agent,
            prompt_text.trim(),
            &mut session_total,
            &agent_config.model,
            &session_changes,
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
            // Return ignored on purpose: this branch already exits 1.
            let _ = emit_output(
                &final_response,
                &agent_config.model,
                &session_total,
                true,
                &session_changes,
                output_path,
                json_output,
                print_mode,
                prompt_start.elapsed(),
                count_assistant_turns(agent),
            );
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
        // Per-process entry point: this tracker was created for this run, so
        // its session-wide state IS the turn's state and the baseline is 0.
        0,
    )
    .await;

    format::maybe_ring_bell(prompt_start.elapsed());
    let output_write_failed = emit_output(
        &response,
        &agent_config.model,
        &session_total,
        false,
        &session_changes,
        output_path,
        json_output,
        print_mode,
        prompt_start.elapsed(),
        count_assistant_turns(agent),
    );
    // A failed `--output` write is a failed run, whatever the model said (#766).
    if output_write_failed {
        std::process::exit(1);
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

/// Per-iteration stop/continue decision for piped mode's auto-continue loop
/// (#794 half (b)).
///
/// Pure on purpose: the loop body it guards is async and does network I/O, so
/// this is the only part of the loop that can be driven by a test. Every input
/// is a stop condition copied from the REPL's `while` guard
/// (`src/repl.rs`, the auto-continue block) — none of them are dropped:
///
/// * `opted_in`   — `cli::is_continue_on_silence()`. Piped mode's loop is
///   gated on the existing opt-in flag rather than on REPL parity; see the
///   comment at the call site for why.
/// * `count`/`max` — the bounded budget from `repl::get_max_auto_continues`.
/// * `had_error`  — `last_tool_error` or `last_api_error` from the last turn.
/// * `budget_exhausted` — `prompt_budget::session_budget_exhausted(30)`.
/// * `predicate`  — `repl::should_auto_continue(..)`, the shared decision.
fn piped_should_continue(
    opted_in: bool,
    count: u32,
    max: u32,
    had_error: bool,
    budget_exhausted: bool,
    predicate: bool,
) -> bool {
    opted_in && count < max && !had_error && !budget_exhausted && predicate
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

    // #786: the one tracker for this whole function. Constructed above the
    // stream-json early return so that branch records into the *same* tracker
    // the later `build_json_output` / `emit_output` read — a second throwaway
    // one here is the exact defect of #678.
    let session_changes = SessionChanges::new();

    // Stream-JSON mode: emit NDJSON events and return early
    if output_format == cli::OutputFormat::StreamJson {
        let mut session_total = Usage::default();
        let response = run_prompt_stream_json(
            agent,
            input,
            &mut session_total,
            &agent_config.model,
            &session_changes,
        )
        .await;
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
        watch::arm_auto_watch("auto_watch = false", print_mode);
    } else if get_watch_command().is_none() && !agent_config.auto_watch && !print_mode {
        watch::hint_auto_watch_available();
    }

    let mut session_total = Usage::default();
    let prompt_start = Instant::now();
    // #794 half (a): the auto-continue gate needs "did this turn run any
    // tool", which the edit counter cannot answer (it sees write-class only).
    let tools_before = session_changes.tool_call_count();
    let initial = run_prompt_with_changes(
        agent,
        input,
        &mut session_total,
        &agent_config.model,
        &session_changes,
    )
    .await;
    // Fallback retry for piped mode
    let (mut response, should_exit_error) = try_fallback_prompt(
        agent_config,
        agent,
        FallbackRetry::Text(input),
        &mut session_total,
        initial,
    )
    .await;

    // ── Auto-continue (#794 half (b)) ────────────────────────────────────
    //
    // Every agent in `scripts/evolve.sh` runs THIS path, not the REPL: the
    // prompt arrives on stdin, so `run_repl` is never reached and the only
    // reader of `cli::is_continue_on_silence()` used to live inside the REPL
    // loop. The flag was set and never read.
    //
    // This is a deliberate duplicate of the REPL's *sequencing*, not a hoist
    // of its loop: that loop is entangled with REPL-only state (turn_count,
    // TurnSnapshot, handle_post_prompt, spawn_tracker) which piped mode does
    // not have. What is SHARED is the decision — `should_auto_continue` and
    // `get_max_auto_continues` are called here, so the two paths cannot
    // disagree about *whether* to continue. Please don't "fix" the ~20 lines
    // of duplicated sequencing by dragging the REPL loop out; the bodies
    // genuinely differ.
    //
    // Why the gate is narrower than REPL parity: full parity would let the
    // `looks_incomplete` / follow-up-queue branches fire for every existing
    // `yoyo -p` user, turning one-turn scripts into multi-turn ones with no
    // opt-in — an evolve-loop convenience shipped as a product default, which
    // is #448 verbatim. So the LOOP is gated on `--continue-on-silence`
    // (default OFF, unchanged); once opted in, the full predicate runs, so
    // queue-pending and `looks_incomplete` do drive continuation. Widening
    // the gate to unconditional parity is a separate, arguable task.
    if !should_exit_error {
        let opted_in = cli::is_continue_on_silence();
        let piped_file_config = crate::config::load_config_file().0;
        let max_continues = crate::repl::get_max_auto_continues(
            &piped_file_config,
            crate::commands::is_plan_apply_active(),
        );
        let mut auto_continue_count: u32 = 0;
        let mut last_text = response.text.clone();
        let mut last_closing = response.text_since_last_tool.clone();
        let mut had_error = response.last_tool_error.is_some() || response.last_api_error.is_some();
        // #794 half (a): a real tool-call count. The edit tracker only sees
        // write-class tools, so a turn that only READ files used to read as
        // "no tools ran" here — the abstention shape this gate exists for.
        let mut ran_tools = session_changes.tool_call_count() > tools_before;

        while piped_should_continue(
            opted_in,
            auto_continue_count,
            max_continues,
            had_error,
            crate::prompt_budget::session_budget_exhausted(30),
            crate::repl::should_auto_continue(
                &last_text,
                // #808: closing text for the silence branch — see repl.rs.
                &last_closing,
                agent.follow_up_queue_len(),
                ran_tools,
                opted_in,
            ),
        ) {
            auto_continue_count += 1;
            if !print_mode && !format::is_quiet() {
                eprintln!(
                    "\n{DIM}  ⚡ auto-continuing ({auto_continue_count}/{max_continues} \
                     — more work pending)...{RESET}"
                );
            }

            let cont_tools_before = session_changes.tool_call_count();
            let cont_outcome = prompt::run_prompt_auto_retry(
                agent,
                "Continue with the remaining work. Pick up where you left off.",
                &mut session_total,
                &agent_config.model,
                // The caller's own tracker — a fresh SessionChanges here is
                // #678's defect verbatim.
                &session_changes,
            )
            .await;

            last_text = cont_outcome.text.clone();
            last_closing = cont_outcome.text_since_last_tool.clone();
            had_error =
                cont_outcome.last_tool_error.is_some() || cont_outcome.last_api_error.is_some();
            ran_tools = session_changes.tool_call_count() > cont_tools_before;

            // Accumulate so `emit_output` (and `--output` / `--output-format
            // json`) report the whole turn, not just its first slice.
            if !cont_outcome.text.trim().is_empty() {
                if !response.text.is_empty() {
                    response.text.push_str("\n\n");
                }
                response.text.push_str(&cont_outcome.text);
            }
            response.last_tool_error = cont_outcome.last_tool_error;
            response.last_tool_name = cont_outcome.last_tool_name;
            response.last_api_error = cont_outcome.last_api_error;
        }
    }

    // Run watch command after prompt if active (auto lint/test loop)
    if !should_exit_error {
        run_watch_after_prompt(
            agent,
            &mut session_total,
            &agent_config.model,
            &session_changes,
            // Per-process entry point: baseline 0 (see run_single_prompt).
            0,
        )
        .await;
    }

    format::maybe_ring_bell(prompt_start.elapsed());
    let output_write_failed = emit_output(
        &response,
        &agent_config.model,
        &session_total,
        should_exit_error,
        &session_changes,
        output_path,
        json_output,
        print_mode,
        prompt_start.elapsed(),
        count_assistant_turns(agent),
    );
    // A failed `--output` write is a failed run, whatever the model said (#766).
    if should_exit_error || output_write_failed {
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

    if config.auto_edit {
        crate::cli_config::enable_auto_edit();
    }

    if config.safe_mode {
        crate::cli_config::set_safe_mode(true);
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
                let summary = commands_session::session_resume_summary(agent.messages());
                eprint!("{DIM}{summary}{RESET}");
            }
            Err(e) => eprintln!("{YELLOW}warning:{RESET} Failed to restore session: {e}"),
        },
        Err(_) => eprintln!("{DIM}  no previous session found ({session_path}){RESET}"),
    }
}

#[tokio::main]
async fn main() {
    // Restore the default SIGPIPE disposition (Unix only). Rust sets SIGPIPE
    // to SIG_IGN at startup, so when the reader of a pipe closes early
    // (`yoyo risk accuracy | head -1`), `println!` gets EPIPE and panics with
    // exit 101 ("failed printing to stdout: Broken pipe"). SIG_DFL makes yoyo
    // die quietly with SIGPIPE (exit 141) like every normal Unix CLI (cat,
    // grep, ls). Do NOT "clean this up" — removing it reintroduces the panic.
    // (Ironically, src/tools.rs already restores SIGPIPE for *child*
    // processes; this is the same guard for yoyo's own process.)
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let args: Vec<String> = std::env::args().collect();

    apply_cli_flags(&args);

    // Near-miss guard on the bare-word path: `yoyo tokens` / `yoyo statsu` used to
    // fall through to the single-prompt path and be answered by the model — a paid
    // API call with write-capable tools attached, for a typo. This runs before any
    // agent is built and spends zero tokens. It fires only on `yoyo <single-word>`
    // where the word is a near miss; anything else (including a single word that
    // resembles nothing) still becomes a prompt.
    if let Some(word) = dispatch_sub::bare_word_arg(&args) {
        if let Some(msg) = dispatch_sub::bare_word_near_miss(word) {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    }

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

    if crate::cli_config::is_safe_mode() {
        eprintln!(
            "{}⚠ Safe mode:{} MCP servers, skills, custom commands, and config disabled",
            crate::format::YELLOW,
            crate::format::RESET,
        );
    }

    let continue_session = config.continue_session;
    let output_path = config.output_path;
    // In safe mode, skip MCP servers, skills, custom commands, and config
    let mcp_servers = if crate::cli_config::is_safe_mode() {
        vec![]
    } else {
        config.mcp_servers
    };
    let mcp_server_configs = if crate::cli_config::is_safe_mode() {
        vec![]
    } else {
        config.mcp_server_configs
    };
    let openapi_specs = if crate::cli_config::is_safe_mode() {
        vec![]
    } else {
        config.openapi_specs
    };
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
        skills: if crate::cli_config::is_safe_mode() {
            yoagent::skills::SkillSet::empty()
        } else {
            config.skills
        },
        system_prompt: config.system_prompt,
        thinking: config.thinking,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        max_turns: config.max_turns,
        auto_approve,
        auto_commit: config.auto_commit,
        permissions: if crate::cli_config::is_safe_mode() {
            cli::PermissionConfig::default()
        } else {
            config.permissions
        },
        dir_restrictions: if crate::cli_config::is_safe_mode() {
            cli::DirectoryRestrictions::default()
        } else {
            config.dir_restrictions
        },
        context_strategy: config.context_strategy,
        context_window: config.context_window,
        shell_hooks: if crate::cli_config::is_safe_mode() {
            vec![]
        } else {
            config.shell_hooks
        },
        fallback_provider: config.fallback_provider,
        fallback_model: config.fallback_model,
        auto_watch: if crate::cli_config::is_safe_mode() {
            false
        } else {
            config.auto_watch
        },
        allowed_tools: config.allowed_tools,
        disallowed_tools: config.disallowed_tools,
        no_tools: config.no_tools,
        lite: config.lite,
        bash_cwd: None,
    };

    if !run_setup_wizard_if_needed(is_interactive, &mut agent_config) {
        return;
    }

    apply_bedrock_credentials(&mut agent_config);

    // Record the resolved provider so error diagnosis names the *configured*
    // provider's env var (not one guessed from the model name) on auth failures (#590).
    cli::set_configured_provider(&agent_config.provider);

    // GASP recording (default-off `gasp` feature, opt-in via env). Installing
    // the recorder here is what makes the four agent-start call sites in
    // `prompt.rs` tee their event streams into the store (#683 step 2); without
    // the install they fall back to the plain `agent.prompt*` path.
    #[cfg(feature = "gasp")]
    {
        let recorder = gasp::open_recorder_from_env().await;
        if cli::is_verbose() {
            match &recorder {
                Some(r) => eprintln!(
                    "gasp: recording into goal {} — all four prompt paths tee'd \
                     (failures are logged by yoagent, not surfaced here)",
                    r.goal()
                ),
                None => eprintln!("gasp: recording disabled"),
            }
        }
        if let Some(r) = recorder {
            gasp::install(r);
        }
    }

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
