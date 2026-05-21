//! Interactive REPL loop and related helpers (tab-completion, multi-line input).

use std::time::Instant;

use crate::cli::*;
use crate::commands::{self, auto_compact_if_needed, command_arg_completions, KNOWN_COMMANDS};
use crate::conversations::build_add_content_blocks;
use crate::dispatch::CommandResult;
use crate::format::*;
use crate::git::*;
use crate::prompt::{run_prompt_auto_retry, run_prompt_auto_retry_with_content, PromptOutcome};
use crate::session::{format_turn_changes, SessionChanges, TurnHistory, TurnSnapshot};
use crate::watch::{get_watch_command, run_watch_after_prompt, set_watch_command};
use crate::AgentConfig;

/// Configuration for the REPL session, bundling the positional arguments
/// that don't need mutable borrow semantics.
pub struct ReplConfig {
    pub mcp_count: u32,
    pub openapi_count: u32,
    pub continue_session: bool,
    pub update_available: Option<String>,
    pub mcp_cli_servers: Vec<String>,
    pub mcp_server_configs: Vec<McpServerConfig>,
}

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::Editor;
use yoagent::*;

/// Rustyline helper that provides tab-completion for `/` slash commands.
pub struct YoyoHelper;

impl Completer for YoyoHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = &line[..pos];

        // Slash command completion: starts with '/' and no space yet
        if prefix.starts_with('/') && !prefix.contains(' ') {
            let mut matches: Vec<Pair> = KNOWN_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|cmd| {
                    let cmd_name = &cmd[1..]; // strip leading /
                    let desc = crate::help::command_short_description(cmd_name).unwrap_or("");
                    if desc.is_empty() {
                        Pair {
                            display: cmd.to_string(),
                            replacement: cmd.to_string(),
                        }
                    } else {
                        Pair {
                            display: format!("{cmd:<14} {desc}"),
                            replacement: cmd.to_string(),
                        }
                    }
                })
                .collect();

            // Add custom commands from .yoyo/commands/ and ~/.yoyo/commands/
            for name in crate::commands::custom_command_names() {
                let slash_name = format!("/{name}");
                if slash_name.starts_with(prefix) {
                    matches.push(Pair {
                        display: format!("{slash_name:<14} (custom)"),
                        replacement: slash_name,
                    });
                }
            }

            return Ok((0, matches));
        }

        // Argument-aware completion: command + space + partial arg
        if prefix.starts_with('/') {
            if let Some(space_pos) = prefix.find(' ') {
                let cmd = &prefix[..space_pos];
                let arg_part = &prefix[space_pos + 1..];
                // Only complete the first argument (no nested spaces)
                if !arg_part.contains(' ') {
                    let candidates = command_arg_completions(cmd, arg_part);
                    if !candidates.is_empty() {
                        let pairs = candidates
                            .into_iter()
                            .map(|c| Pair {
                                display: c.clone(),
                                replacement: c,
                            })
                            .collect();
                        return Ok((space_pos + 1, pairs));
                    }
                }
            }
        }

        // File path completion: extract the last whitespace-delimited word
        let word_start = prefix.rfind(char::is_whitespace).map_or(0, |i| i + 1);
        let word = &prefix[word_start..];
        if word.is_empty() {
            return Ok((pos, Vec::new()));
        }

        let matches = complete_file_path(word)
            .into_iter()
            .map(|p| Pair {
                display: p.clone(),
                replacement: p,
            })
            .collect();
        Ok((word_start, matches))
    }
}

/// Complete a partial file path by listing directory entries that match.
/// Appends `/` to directory names for easy continued completion.
pub fn complete_file_path(partial: &str) -> Vec<String> {
    use std::path::Path;

    let path = Path::new(partial);

    // Determine the directory to scan and the filename prefix to match
    let (dir, file_prefix) =
        if partial.ends_with('/') || partial.ends_with(std::path::MAIN_SEPARATOR) {
            // User typed "src/" — list everything inside src/
            (partial.to_string(), String::new())
        } else if let Some(parent) = path.parent() {
            let parent_str = if parent.as_os_str().is_empty() {
                ".".to_string()
            } else {
                parent.to_string_lossy().to_string()
            };
            let file_prefix = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            (parent_str, file_prefix)
        } else {
            (".".to_string(), partial.to_string())
        };

    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let dir_prefix = if dir == "." && !partial.contains('/') {
        String::new()
    } else if partial.ends_with('/') || partial.ends_with(std::path::MAIN_SEPARATOR) {
        partial.to_string()
    } else {
        let parent = path.parent().unwrap_or(Path::new(""));
        if parent.as_os_str().is_empty() {
            String::new()
        } else {
            format!("{}/", parent.display())
        }
    };

    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&file_prefix) {
            continue;
        }
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let candidate = if is_dir {
            format!("{}{}/", dir_prefix, name)
        } else {
            format!("{}{}", dir_prefix, name)
        };
        matches.push(candidate);
    }
    matches.sort();
    matches
}

impl Hinter for YoyoHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        // Only hint when cursor is at the end of the line
        if pos != line.len() {
            return None;
        }
        // Only hint for slash commands
        if !line.starts_with('/') {
            return None;
        }
        let typed = &line[1..]; // strip the leading /
        if typed.is_empty() {
            return None; // Don't hint on bare "/"
        }
        // If user typed a command + space, show argument hints
        if typed.contains(' ') {
            if let Some((cmd_part, arg_part)) = typed.split_once(' ') {
                if arg_part.is_empty() {
                    // User just typed "/cmd " — show available args
                    if let Some(hint) = crate::commands::command_arg_hint(cmd_part) {
                        return Some(hint.to_string());
                    }
                }
            }
            return None;
        }
        // Find the first matching command
        for cmd in KNOWN_COMMANDS {
            let cmd_name = &cmd[1..]; // strip leading /
            if cmd_name.starts_with(typed) && cmd_name != typed {
                // Show the rest of the command + description
                let rest = &cmd_name[typed.len()..];
                if let Some(desc) = crate::help::command_short_description(cmd_name) {
                    return Some(format!("{rest} — {desc}"));
                } else {
                    return Some(rest.to_string());
                }
            }
        }
        // If user typed a complete command name, show its description
        for cmd in KNOWN_COMMANDS {
            let cmd_name = &cmd[1..];
            if cmd_name == typed {
                if let Some(desc) = crate::help::command_short_description(cmd_name) {
                    return Some(format!(" — {desc}"));
                }
            }
        }
        // Check custom commands for hints
        for name in crate::commands::custom_command_names() {
            if name.starts_with(typed) && name != typed {
                let rest = &name[typed.len()..];
                return Some(format!("{rest} (custom)"));
            }
        }
        if crate::commands::is_custom_command(typed) {
            return Some(" (custom)".to_string());
        }
        None
    }
}

impl Highlighter for YoyoHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        // Show hints in dim text
        std::borrow::Cow::Owned(format!("\x1b[2m{hint}\x1b[0m"))
    }
}

impl Validator for YoyoHelper {}

impl rustyline::Helper for YoyoHelper {}

/// Check if a line needs continuation (backslash at end, or opens a code fence).
pub fn needs_continuation(line: &str) -> bool {
    line.ends_with('\\') || line.starts_with("```")
}

/// Collect multi-line input using rustyline (for interactive REPL mode).
/// Same logic as `collect_multiline` but uses rustyline's readline for continuation prompts.
pub fn collect_multiline_rl(
    first_line: &str,
    rl: &mut Editor<YoyoHelper, DefaultHistory>,
) -> String {
    let mut buf = String::new();
    let cont_prompt = format!("{DIM}  ...{RESET} ");

    if first_line.starts_with("```") {
        // Code fence mode: collect until closing ```
        buf.push_str(first_line);
        buf.push('\n');
        while let Ok(line) = rl.readline(&cont_prompt) {
            buf.push_str(&line);
            buf.push('\n');
            if line.trim() == "```" {
                break;
            }
        }
    } else {
        // Backslash continuation mode
        let mut current = first_line.to_string();
        loop {
            if current.ends_with('\\') {
                current.truncate(current.len() - 1);
                buf.push_str(&current);
                buf.push('\n');
                match rl.readline(&cont_prompt) {
                    Ok(line) => {
                        current = line;
                    }
                    _ => break,
                }
            } else {
                buf.push_str(&current);
                break;
            }
        }
    }

    buf
}

/// Run the architect-mode two-phase turn: plan with a strong model, then
/// implement with a cheaper editor model. Returns the editor's `PromptOutcome`
/// (or a default if the architect returned an empty plan).
async fn run_architect_turn(
    agent_config: &mut AgentConfig,
    effective_input: &str,
    session_total: &mut Usage,
    session_changes: &SessionChanges,
) -> PromptOutcome {
    let arch_model = commands::architect_model().unwrap_or_else(|| agent_config.model.clone());
    let editor_model = commands::default_editor_model(&arch_model);

    eprintln!("{DIM}  🏗️ architect mode: planning with {arch_model}...{RESET}");

    // Phase 1: Get the plan from the architect (no tools, text-only)
    let architect_input = format!("{}\n\n{}", commands::ARCHITECT_PROMPT, effective_input);
    let mut arch_agent = agent_config.build_architect_agent(&arch_model);
    let mut rx = arch_agent.prompt(&architect_input).await;

    let mut md_renderer = MarkdownRenderer::new();
    let mut plan_text = String::new();
    let mut started = false;

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                if !started {
                    eprintln!();
                    eprint!("{DIM}[architect]{RESET} ");
                    started = true;
                }
                plan_text.push_str(&delta);
                let rendered = md_renderer.render_delta(&delta);
                if !rendered.is_empty() {
                    print!("{rendered}");
                }
            }
            Some(AgentEvent::MessageEnd { .. }) => {
                let tail = md_renderer.flush();
                if !tail.is_empty() {
                    print!("{tail}");
                }
            }
            Some(AgentEvent::AgentEnd { .. }) => break,
            None => break,
            _ => {}
        }
    }

    arch_agent.finish().await;

    // Show architect cost
    let arch_messages = arch_agent.messages();
    let mut arch_usage = Usage::default();
    for msg in arch_messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            arch_usage.input += usage.input;
            arch_usage.output += usage.output;
            arch_usage.cache_read += usage.cache_read;
            arch_usage.cache_write += usage.cache_write;
        }
    }
    let arch_tokens = arch_usage.input + arch_usage.output;
    if arch_tokens > 0 {
        let cost = estimate_cost(&arch_usage, &arch_model);
        if let Some(c) = cost {
            eprintln!(
                "\n{DIM}  [architect] {} tokens, ${:.4}{RESET}",
                arch_tokens, c
            );
        } else {
            eprintln!("\n{DIM}  [architect] {} tokens{RESET}", arch_tokens);
        }
    } else {
        eprintln!();
    }

    if plan_text.trim().is_empty() {
        eprintln!("{YELLOW}  architect returned empty plan — skipping editor phase{RESET}\n");
        return PromptOutcome::default();
    }

    // Phase 2: Feed the plan to the editor model for implementation
    eprintln!("{DIM}  🔧 implementing with {editor_model}...{RESET}\n");

    let editor_prompt = format!(
        "Implement the following plan exactly. The plan was written by an architect model \
         analyzing the user's request.\n\n\
         ## Original request\n\n{effective_input}\n\n\
         ## Architect's plan\n\n{plan_text}"
    );

    // Build a separate editor agent with the cheaper model
    let mut editor_agent = agent_config.build_editor_agent(&editor_model);
    let editor_outcome = run_prompt_auto_retry(
        &mut editor_agent,
        &editor_prompt,
        session_total,
        &editor_model,
        session_changes,
    )
    .await;

    // Show editor cost
    editor_agent.finish().await;
    let editor_messages = editor_agent.messages();
    let mut editor_usage = Usage::default();
    for msg in editor_messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            editor_usage.input += usage.input;
            editor_usage.output += usage.output;
            editor_usage.cache_read += usage.cache_read;
            editor_usage.cache_write += usage.cache_write;
        }
    }
    let editor_tokens = editor_usage.input + editor_usage.output;
    if editor_tokens > 0 {
        let cost = estimate_cost(&editor_usage, &editor_model);
        if let Some(c) = cost {
            eprintln!("{DIM}  [editor] {} tokens, ${:.4}{RESET}", editor_tokens, c);
        } else {
            eprintln!("{DIM}  [editor] {} tokens{RESET}", editor_tokens);
        }
    }

    editor_outcome
}

/// Bundles the parameters for [`handle_post_prompt`] into a single struct,
/// eliminating a 13-parameter function signature.
struct PostPromptContext<'a> {
    outcome: &'a PromptOutcome,
    agent: &'a mut yoagent::agent::Agent,
    agent_config: &'a mut AgentConfig,
    session_total: &'a mut Usage,
    session_changes: &'a SessionChanges,
    turn_history: &'a mut TurnHistory,
    turn_snap: TurnSnapshot,
    changes_before: &'a [String],
    last_error: &'a mut Option<String>,
    prompt_start: Instant,
    effective_input: &'a str,
}

/// Post-prompt handling: bell, error tracking, fallback retry, turn snapshots,
/// watch-after-prompt, auto-commit, and auto-compact.
async fn handle_post_prompt(mut ctx: PostPromptContext<'_>) {
    crate::format::maybe_ring_bell(ctx.prompt_start.elapsed());
    *ctx.last_error = ctx.outcome.last_tool_error.clone();

    // Notify the user if the context was auto-compacted due to overflow
    if ctx.outcome.was_overflow {
        eprintln!("{YELLOW}  ℹ Context was auto-compacted (overflow detected){RESET}");
    }

    // Fallback provider: if the API failed and a fallback is configured, switch and retry
    if ctx.outcome.last_api_error.is_some() {
        let old_provider = ctx.agent_config.provider.clone();
        let fallback_name = ctx.agent_config.fallback_provider.clone();
        if ctx.agent_config.try_switch_to_fallback() {
            let fallback = fallback_name.as_deref().unwrap_or("unknown");
            eprintln!(
                "\n{YELLOW}  ⚡ Primary provider '{}' failed. Switching to fallback '{}'...{RESET}",
                old_provider, fallback
            );

            // Rebuild agent with the new provider
            *ctx.agent = ctx.agent_config.build_agent();

            eprintln!(
                "{DIM}  now using: {} / {}{RESET}\n",
                ctx.agent_config.provider, ctx.agent_config.model
            );

            // Retry the same prompt with the fallback provider
            let retry_outcome = run_prompt_auto_retry(
                ctx.agent,
                ctx.effective_input,
                ctx.session_total,
                &ctx.agent_config.model,
                ctx.session_changes,
            )
            .await;
            *ctx.last_error = retry_outcome.last_tool_error.clone();

            // If fallback also failed, restore original provider info for display
            // but keep the fallback agent since the original was already broken
            if retry_outcome.last_api_error.is_some() {
                eprintln!(
                    "{RED}  fallback provider '{}' also failed.{RESET}",
                    fallback
                );
                eprintln!(
                    "{DIM}  original provider was '{}'. Use /provider to switch manually.{RESET}",
                    old_provider
                );
            }
        }
    }

    // After the turn, find newly modified files and update the snapshot
    let changes_after: Vec<String> = ctx
        .session_changes
        .snapshot()
        .iter()
        .map(|c| c.path.clone())
        .collect();
    for path in &changes_after {
        if !ctx.changes_before.contains(path) {
            // This file was touched for the first time in this turn
            if ctx.turn_snap.originals.contains_key(path.as_str()) {
                // Already snapshotted (e.g., was in git diff) — keep the original
            } else if std::path::Path::new(path).exists() {
                // File was created during this turn
                ctx.turn_snap.record_created(path);
            }
        }
    }
    // Also check for new files from git that weren't in session_changes
    if let Ok(diff_files) = crate::git::run_git(&["diff", "--name-only"]) {
        for f in diff_files.lines().filter(|l| !l.is_empty()) {
            if !ctx.turn_snap.originals.contains_key(f) {
                ctx.turn_snap.snapshot_file(f);
            }
        }
    }
    ctx.turn_history.push(ctx.turn_snap);

    let files_modified = changes_after.len() > ctx.changes_before.len();

    // Show a compact summary of files changed in this turn
    if files_modified && !is_quiet() {
        let summary = format_turn_changes(ctx.changes_before, ctx.session_changes);
        if !summary.is_empty() {
            eprintln!("{DIM}{summary}{RESET}");
        }
    }

    if files_modified {
        let watch_result = run_watch_after_prompt(
            ctx.agent,
            ctx.session_total,
            &ctx.agent_config.model,
            ctx.session_changes,
        )
        .await;
        if !watch_result.passed {
            *ctx.last_error = watch_result.last_tool_error;
        }
    }

    // ── Auto-commit: stage and commit if flag is on and files changed ─────
    if ctx.agent_config.auto_commit && files_modified {
        let _ = run_git(&["add", "-A"]);
        if let Some(diff) = get_staged_diff() {
            if !diff.trim().is_empty() {
                let msg = generate_commit_message(&diff);
                let (ok, output) = run_git_commit(&msg);
                if ok {
                    eprintln!("{GREEN}  ✓ Auto-committed: {}{RESET}", output.trim());
                } else {
                    eprintln!("{DIM}  (auto-commit failed: {}){RESET}", output.trim());
                }
            }
        }
    }

    // Auto-compact when context window is getting full
    auto_compact_if_needed(ctx.agent);
}

/// Print the startup banner and all session configuration info.
fn print_startup_info(agent_config: &AgentConfig, repl_config: &ReplConfig) {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    print_banner();
    if agent_config.provider != "anthropic" {
        println!("{DIM}  provider: {}{RESET}", agent_config.provider);
    }
    println!("{DIM}  model: {}{RESET}", agent_config.model);
    if let Some(ref url) = agent_config.base_url {
        println!("{DIM}  base_url: {url}{RESET}");
    }
    if agent_config.thinking != ThinkingLevel::Off {
        println!("{DIM}  thinking: {:?}{RESET}", agent_config.thinking);
    }
    if let Some(temp) = agent_config.temperature {
        println!("{DIM}  temperature: {temp:.1}{RESET}");
    }
    if !agent_config.skills.is_empty() {
        println!("{DIM}  skills: {} loaded{RESET}", agent_config.skills.len());
    }
    if repl_config.mcp_count > 0 {
        println!(
            "{DIM}  mcp: {} server(s) connected{RESET}",
            repl_config.mcp_count
        );
    }
    if repl_config.openapi_count > 0 {
        println!(
            "{DIM}  openapi: {} spec(s) loaded{RESET}",
            repl_config.openapi_count
        );
    }
    if is_verbose() {
        println!("{DIM}  verbose: on{RESET}");
    }
    if !agent_config.auto_approve {
        println!("{DIM}  tools: confirmation required (use --yes to skip){RESET}");
    }
    if !agent_config.permissions.is_empty() {
        println!(
            "{DIM}  permissions: {} allow, {} deny pattern(s){RESET}",
            agent_config.permissions.allow.len(),
            agent_config.permissions.deny.len()
        );
    }
    if let Some(ref fallback) = agent_config.fallback_provider {
        println!("{DIM}  fallback: {fallback}{RESET}");
    }
    if let Some(branch) = git_branch() {
        println!("{DIM}  git:   {branch}{RESET}");
    }
    println!("{DIM}  cwd:   {cwd}{RESET}\n");

    // Show update notification if a newer version is available
    if let Some(ref latest) = repl_config.update_available {
        println!(
            "  {YELLOW}⬆ Update available: v{latest} (you have v{VERSION}) — https://github.com/yologdev/yoyo-evolve/releases{RESET}\n"
        );
    }

    // Hint about previous session if one exists and --continue wasn't used
    if !repl_config.continue_session && commands::last_session_exists() {
        println!(
            "{DIM}  💡 Previous session found. Use {YELLOW}--continue{RESET}{DIM} or {YELLOW}/load .yoyo/last-session.json{RESET}{DIM} to resume.{RESET}\n"
        );
    }

    // Auto-enable watch mode if a project type is detected and config allows it
    if get_watch_command().is_none() && agent_config.auto_watch {
        if let Some(cmd) = crate::watch::auto_detect_watch_command() {
            set_watch_command(&cmd);
            println!(
                "{DIM}  👀 Auto-watch: `{cmd}` (disable with /watch off or auto_watch = false){RESET}\n"
            );
        }
    }
}

/// Returns when the user exits (via /quit, /exit, Ctrl-D, etc.).
pub async fn run_repl(
    agent_config: &mut AgentConfig,
    agent: &mut yoagent::agent::Agent,
    repl_config: ReplConfig,
) {
    print_startup_info(agent_config, &repl_config);

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    // Set up rustyline editor with slash-command tab-completion
    let config = rustyline::config::Builder::new()
        .completion_type(rustyline::config::CompletionType::List)
        .completion_prompt_limit(50)
        .build();
    let mut rl = Editor::with_config(config).expect("Failed to initialize readline");
    rl.set_helper(Some(YoyoHelper));
    if let Some(history_path) = history_file_path() {
        if rl.load_history(&history_path).is_err() {
            // First run or history file doesn't exist yet — that's fine
        }
    }

    let mut session_total = Usage::default();
    let session_start = Instant::now();
    let mut turn_count: usize = 0;
    let mut last_input: Option<String> = None;
    let mut last_error: Option<String> = None;
    let mut bookmarks = commands::Bookmarks::new();
    let session_changes = SessionChanges::new();
    let mut turn_history = TurnHistory::new();
    let spawn_tracker = commands::SpawnTracker::new();
    let bg_tracker = commands::BackgroundJobTracker::new();
    let mut undo_context: Option<String> = None;
    let mut checkpoint_store = commands::CheckpointStore::new();

    // Load config for auto-continue settings (re-read each turn for live updates)
    let repl_file_config = crate::config::load_config_file().0;

    loop {
        // Build mode indicators
        let mode_indicators = {
            let mut indicators = String::new();
            if commands::is_plan_mode() {
                indicators.push_str(&format!("{BOLD}{YELLOW}📋{RESET} "));
            }
            if commands::is_architect_mode() {
                indicators.push_str(&format!("{BOLD}{YELLOW}🏗️{RESET} "));
            }
            indicators
        };

        let prompt = if let Some(branch) = git_branch() {
            format!("{BOLD}{GREEN}{branch}{RESET} {mode_indicators}{BOLD}{GREEN}🐙 › {RESET}")
        } else {
            format!("{mode_indicators}{BOLD}{GREEN}🐙 › {RESET}")
        };

        let line = match rl.readline(&prompt) {
            Ok(l) => l,
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: cancel current line, print new prompt
                println!();
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: exit
                break;
            }
            Err(_) => break,
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Add to readline history
        let _ = rl.add_history_entry(&line);

        // Multi-line input: collect continuation lines
        let input = if needs_continuation(input) {
            collect_multiline_rl(input, &mut rl)
        } else {
            input.to_string()
        };
        let input = input.trim();

        let mut dispatch_ctx = crate::dispatch::DispatchContext {
            input,
            agent,
            agent_config,
            session_total: &mut session_total,
            session_changes: &session_changes,
            turn_history: &mut turn_history,
            bg_tracker: &bg_tracker,
            spawn_tracker: &spawn_tracker,
            undo_context: &mut undo_context,
            last_input: &mut last_input,
            last_error: &mut last_error,
            bookmarks: &mut bookmarks,
            checkpoint_store: &mut checkpoint_store,
            session_start,
            turn_count,
            cwd: &cwd,
            mcp_cli_servers: &repl_config.mcp_cli_servers,
            mcp_server_configs: &repl_config.mcp_server_configs,
            mcp_count: repl_config.mcp_count,
            openapi_count: repl_config.openapi_count,
        };
        let cmd_result = crate::dispatch::dispatch_command(&mut dispatch_ctx).await;
        match cmd_result {
            CommandResult::Quit => break,
            CommandResult::Continue => continue,
            CommandResult::SendToAgent(prompt) => {
                last_input = Some(prompt);
                // fall through to agent prompt handling
            }
            CommandResult::NotACommand => {
                last_input = Some(input.to_string());
                // fall through to agent prompt handling
            }
        }

        // Snapshot files before the agent turn for per-turn undo
        let changes_before: Vec<String> = session_changes
            .snapshot()
            .iter()
            .map(|c| c.path.clone())
            .collect();
        let mut turn_snap = TurnSnapshot::new();
        for path in &changes_before {
            turn_snap.snapshot_file(path);
        }
        // Also snapshot any files in the git working tree diff
        if let Ok(diff_files) = crate::git::run_git(&["diff", "--name-only"]) {
            for f in diff_files.lines().filter(|l| !l.is_empty()) {
                turn_snap.snapshot_file(f);
            }
        }

        // Expand @file mentions (e.g. "explain @src/main.rs") into file content
        let (cleaned_text, file_results) = commands::expand_file_mentions(input);

        // If teach mode is active, prepend the teaching instruction to the user message
        let effective_input = if commands::is_teach_mode() {
            format!("{}\n\n{}", commands::TEACH_MODE_PROMPT, cleaned_text)
        } else {
            cleaned_text.clone()
        };

        // If plan mode is active, prepend the planning constraint to the user message
        let effective_input = if commands::is_plan_mode() {
            format!("{}\n\n{}", commands::PLAN_MODE_PROMPT, effective_input)
        } else {
            effective_input
        };

        // If /undo was run since the last turn, prepend context so the agent
        // knows files were reverted and its previous references may be stale.
        let effective_input = if let Some(ctx) = undo_context.take() {
            format!("{ctx}\n\n{effective_input}")
        } else {
            effective_input
        };

        let prompt_start = Instant::now();
        turn_count += 1;

        // ── Architect mode: plan with strong model, implement with editor ──
        let outcome = if commands::is_architect_mode() {
            run_architect_turn(
                agent_config,
                &effective_input,
                &mut session_total,
                &session_changes,
            )
            .await
        } else if !file_results.is_empty() {
            // Print summaries like /add does
            for result in &file_results {
                match result {
                    commands::AddResult::Text { summary, .. } => println!("{summary}"),
                    commands::AddResult::Image { summary, .. } => println!("{summary}"),
                }
            }
            let word = crate::format::pluralize(file_results.len(), "file", "files");
            println!(
                "{}  ({} {word} inlined from @mentions){}\n",
                DIM,
                file_results.len(),
                RESET
            );

            // Build content blocks: user text first, then file contents
            let mut content_blocks = vec![yoagent::types::Content::Text {
                text: effective_input.clone(),
            }];
            content_blocks.extend(build_add_content_blocks(&file_results));

            run_prompt_auto_retry_with_content(
                agent,
                content_blocks,
                &mut session_total,
                &agent_config.model,
                &session_changes,
                &effective_input,
            )
            .await
        } else {
            run_prompt_auto_retry(
                agent,
                &effective_input,
                &mut session_total,
                &agent_config.model,
                &session_changes,
            )
            .await
        };
        handle_post_prompt(PostPromptContext {
            outcome: &outcome,
            agent,
            agent_config,
            session_total: &mut session_total,
            session_changes: &session_changes,
            turn_history: &mut turn_history,
            turn_snap,
            changes_before: &changes_before,
            last_error: &mut last_error,
            prompt_start,
            effective_input: &effective_input,
        })
        .await;

        // ── Auto-continue: if the model stopped mid-work, send a follow-up ──
        {
            let max_continues =
                get_max_auto_continues(&repl_file_config, crate::commands::is_plan_apply_active());
            let mut auto_continue_count: u32 = 0;
            let mut last_text = outcome.text.clone();
            let mut had_error =
                outcome.last_tool_error.is_some() || outcome.last_api_error.is_some();

            while auto_continue_count < max_continues
                && !had_error
                && looks_incomplete(&last_text)
                && !crate::prompt_budget::session_budget_exhausted(30)
            {
                auto_continue_count += 1;
                eprintln!(
                    "\n{DIM}  ⚡ auto-continuing ({auto_continue_count}/{max_continues} \
                     — response appears incomplete)...{RESET}"
                );

                // Snapshot state for the continuation turn
                let cont_changes_before: Vec<String> = session_changes
                    .snapshot()
                    .iter()
                    .map(|c| c.path.clone())
                    .collect();
                let mut cont_turn_snap = TurnSnapshot::new();
                for path in &cont_changes_before {
                    cont_turn_snap.snapshot_file(path);
                }
                if let Ok(diff_files) = crate::git::run_git(&["diff", "--name-only"]) {
                    for f in diff_files.lines().filter(|l| !l.is_empty()) {
                        cont_turn_snap.snapshot_file(f);
                    }
                }

                let cont_prompt = "Continue with the remaining work. Pick up where you left off.";
                let cont_start = Instant::now();
                turn_count += 1;

                let cont_outcome = run_prompt_auto_retry(
                    agent,
                    cont_prompt,
                    &mut session_total,
                    &agent_config.model,
                    &session_changes,
                )
                .await;

                // Update tracking for the next iteration
                last_text = cont_outcome.text.clone();
                had_error =
                    cont_outcome.last_tool_error.is_some() || cont_outcome.last_api_error.is_some();

                handle_post_prompt(PostPromptContext {
                    outcome: &cont_outcome,
                    agent,
                    agent_config,
                    session_total: &mut session_total,
                    session_changes: &session_changes,
                    turn_history: &mut turn_history,
                    turn_snap: cont_turn_snap,
                    changes_before: &cont_changes_before,
                    last_error: &mut last_error,
                    prompt_start: cont_start,
                    effective_input: cont_prompt,
                })
                .await;
            }
        }

        // Clear plan-apply flag after prompt + auto-continue finishes
        if crate::commands::is_plan_apply_active() {
            crate::commands::set_plan_apply_active(false);
        }
    }

    // Save readline history
    if let Some(history_path) = history_file_path() {
        let _ = rl.save_history(&history_path);
    }

    // Auto-save session on exit (always — crash recovery for everyone)
    commands::auto_save_on_exit(agent);

    // Show session summary (files, tokens, cost, duration)
    if let Some(summary) = commands::format_exit_summary(
        &session_changes,
        &session_total,
        &agent_config.model,
        session_start,
    ) {
        println!("\n{summary}");
        println!("{DIM}  bye 👋{RESET}\n");
    } else {
        println!("\n{DIM}  bye 👋{RESET}\n");
    }
}

/// Default maximum number of automatic follow-up prompts per user turn.
const DEFAULT_MAX_AUTO_CONTINUES: u32 = 5;

/// Compute the effective auto-continue limit for the current turn.
///
/// Takes into account:
/// 1. If `auto_continue` is disabled in config → returns 0
/// 2. If `max_auto_continues` is set in config → uses that value
/// 3. If a `/plan apply` is active → uses `max(config_limit, 10)` so the
///    agent can work through a full plan without hitting the normal cap
pub(crate) fn get_max_auto_continues(
    config: &std::collections::HashMap<String, String>,
    in_plan_apply: bool,
) -> u32 {
    use crate::config::{parse_auto_continue_from_config, parse_max_auto_continues_from_config};

    if !parse_auto_continue_from_config(config) {
        return 0;
    }

    let base = parse_max_auto_continues_from_config(config).unwrap_or(DEFAULT_MAX_AUTO_CONTINUES);

    if in_plan_apply {
        base.max(10)
    } else {
        base
    }
}

/// Heuristic check: does the agent's response text suggest it stopped mid-work
/// and intends to continue? Conservative — only triggers on clear signals.
pub(crate) fn looks_incomplete(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.len() < 20 {
        return false;
    }

    // Check the tail of the response (last ~300 chars) for continuation signals.
    let tail_start = {
        let target = text.len().saturating_sub(300);
        let mut b = target;
        while b > 0 && !text.is_char_boundary(b) {
            b -= 1;
        }
        b
    };
    let tail = &text[tail_start..];
    let tail_lower = tail.to_lowercase();

    // ── Pattern 1: explicit continuation phrases near the end ──
    let continuation_phrases = [
        "next, i'll",
        "next i'll",
        "i'll now ",
        "let me continue",
        "let me proceed",
        "i'll continue",
        "moving on to",
        "let me move on",
        "i'll move on",
        "now let's",
        "now i'll",
        "now i need to",
        "let me now",
        "i still need to",
        "i need to continue",
    ];
    for phrase in &continuation_phrases {
        if tail_lower.contains(phrase) {
            return true;
        }
    }

    // ── Pattern 2: "remaining" + steps/tasks/items near the end ──
    if tail_lower.contains("remaining") {
        for word in &["step", "task", "item", "change", "file", "fix", "issue"] {
            if tail_lower.contains(word) {
                return true;
            }
        }
    }

    // ── Pattern 3: response ends with "..." suggesting continuation ──
    if text.ends_with("...") || text.ends_with("…") {
        return true;
    }

    // ── Pattern 4: numbered steps where later steps haven't been addressed ──
    // Look for "Step N:" or "N." patterns and check if the last mentioned step
    // is less than the total count announced.
    let mut max_announced: u32 = 0;
    let mut max_reached: u32 = 0;
    for line in text.lines() {
        let trimmed = line.trim().to_lowercase();
        // Match patterns like "Step 3:" or "3. " or "**Step 3**"
        if let Some(n) = extract_step_number(&trimmed) {
            if n > max_announced {
                max_announced = n;
            }
        }
    }
    // Now check which steps actually have content after them
    // (simplification: the last step number mentioned in the text is the last reached)
    for line in text.lines().rev() {
        let trimmed = line.trim().to_lowercase();
        if let Some(n) = extract_step_number(&trimmed) {
            max_reached = n;
            break;
        }
    }
    if max_announced >= 3 && max_reached > 0 && max_reached < max_announced {
        // Announced N steps but only reached step M < N
        return true;
    }

    // ── Pattern 5: unclosed code blocks ──
    // If the number of ``` fences in the full text is odd, the model was cut off mid-code.
    {
        let fence_count = text.matches("```").count();
        if fence_count % 2 == 1 {
            return true;
        }
    }

    // ── Pattern 6: action intent phrases near the end ──
    let action_phrases = [
        "let me update",
        "let me fix",
        "let me modify",
        "let me add",
        "let me create",
        "let me write",
        "let me implement",
        "let me handle",
        "i'll update",
        "i'll fix",
        "i'll modify",
        "i'll add",
        "i'll create",
        "i'll implement",
        "i'll handle",
    ];
    for phrase in &action_phrases {
        if tail_lower.contains(phrase) {
            return true;
        }
    }

    // ── Pattern 7: ordinal progression without completion ──
    // "first" or "second" in the tail without "finally" or "lastly" suggests more steps coming.
    if (tail_lower.contains("first,") || tail_lower.contains("first "))
        && !tail_lower.contains("finally")
        && !tail_lower.contains("lastly")
        && !tail_lower.contains("last,")
    {
        // Only trigger if "second" is absent — if both "first" and "second" are present,
        // check that "third" or "finally" is missing
        if !tail_lower.contains("second") {
            return true;
        }
        if !tail_lower.contains("third") && !tail_lower.contains("finally") {
            return true;
        }
    }

    // ── Pattern 8: explicit "step X of Y" where X < Y ──
    // Matches "step 2 of 5", "Step 3/7", "step 1 of 4 done" etc.
    if let Some(incomplete) = step_x_of_y_incomplete(&tail_lower) {
        if incomplete {
            return true;
        }
    }

    false
}

/// Extract a step number from patterns like "step 3:", "3. ", "**step 3**"
fn extract_step_number(line: &str) -> Option<u32> {
    // "step N:" or "**step N**" or "step N."
    if let Some(rest) = line
        .strip_prefix("step ")
        .or_else(|| line.strip_prefix("**step "))
    {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
    }
    // "N. " at the start of a line (numbered list)
    let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 2 {
        let rest = &line[digits.len()..];
        if rest.starts_with(". ") || rest.starts_with(") ") {
            return digits.parse().ok();
        }
    }
    None
}

/// Check for "step X of Y" or "step X/Y" patterns where X < Y, indicating
/// the model stopped before completing all steps.
fn step_x_of_y_incomplete(text: &str) -> Option<bool> {
    // Search for patterns like "step 2 of 5", "step 3/7", "step 1 of 4 complete"
    let mut found = false;
    let mut i = 0;
    let bytes = text.as_bytes();
    while i + 5 < bytes.len() {
        // Look for "step "
        if i + 5 <= bytes.len() && &text[i..i + 5] == "step " {
            let after_step = &text[i + 5..];
            // Parse X
            let x_digits: String = after_step
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !x_digits.is_empty() {
                if let Ok(x) = x_digits.parse::<u32>() {
                    let rest = &after_step[x_digits.len()..];
                    // Look for " of " or "/"
                    let y_str = if let Some(r) = rest.strip_prefix(" of ") {
                        Some(r)
                    } else {
                        rest.strip_prefix('/')
                    };
                    if let Some(y_rest) = y_str {
                        let y_digits: String =
                            y_rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                        if !y_digits.is_empty() {
                            if let Ok(y) = y_digits.parse::<u32>() {
                                if x < y {
                                    return Some(true);
                                }
                                found = true;
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }
    if found {
        Some(false) // Found step X of Y but X >= Y
    } else {
        None // No pattern found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check if any candidate has the given replacement text.
    fn has_replacement(candidates: &[Pair], replacement: &str) -> bool {
        candidates.iter().any(|c| c.replacement == replacement)
    }

    #[test]
    fn test_prompt_has_octopus() {
        // Verify the styled prompt contains the octopus emoji
        let prompt_no_branch = format!("{BOLD}{GREEN}🐙 › {RESET}");
        assert!(
            prompt_no_branch.contains('🐙'),
            "Prompt should contain octopus emoji"
        );

        let prompt_with_branch = format!("{BOLD}{GREEN}main{RESET} {BOLD}{GREEN}🐙 › {RESET}");
        assert!(
            prompt_with_branch.contains('🐙'),
            "Branch prompt should contain octopus emoji"
        );
    }

    #[test]
    fn test_needs_continuation_backslash() {
        assert!(needs_continuation("hello \\"));
        assert!(needs_continuation("line ends with\\"));
        assert!(!needs_continuation("normal line"));
        assert!(!needs_continuation("has \\ in middle"));
    }

    #[test]
    fn test_needs_continuation_code_fence() {
        assert!(needs_continuation("```rust"));
        assert!(needs_continuation("```"));
        assert!(!needs_continuation("some text ```"));
        assert!(!needs_continuation("normal"));
    }

    #[test]
    fn test_yoyo_helper_completes_slash_commands() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Typing "/" should suggest all commands
        let (start, candidates) = helper.complete("/", 1, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(!candidates.is_empty());
        assert!(has_replacement(&candidates, "/help"));
        assert!(has_replacement(&candidates, "/quit"));

        // Typing "/he" should suggest "/help" and "/health"
        let (start, candidates) = helper.complete("/he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "/help"));
        assert!(has_replacement(&candidates, "/health"));
        assert!(!has_replacement(&candidates, "/quit"));

        // Typing "/model " (with space) should return model completions
        let (start, candidates) = helper.complete("/model ", 7, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(
            !candidates.is_empty(),
            "Should offer model name completions after /model "
        );
        assert!(
            candidates.iter().any(|c| c.replacement.contains("claude")),
            "Should include Claude models"
        );

        // "/model cl" should filter to Claude models
        let (start, candidates) = helper.complete("/model cl", 9, &ctx).unwrap();
        assert_eq!(start, 7);
        for c in &candidates {
            assert!(
                c.replacement.starts_with("cl"),
                "All completions should start with 'cl': {:?}",
                c.replacement
            );
        }

        // Regular text that doesn't match any files returns no completions
        let (_, candidates) = helper.complete("zzz_nonexistent_xyz", 19, &ctx).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_file_path_completion_current_dir() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "Cargo" should match Cargo.toml (and possibly Cargo.lock)
        let (start, candidates) = helper.complete("Cargo", 5, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "Cargo.toml"));
    }

    #[test]
    fn test_file_path_completion_with_directory_prefix() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "src/ma" should match "src/main.rs"
        let (start, candidates) = helper.complete("src/ma", 6, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "src/main.rs"));
    }

    #[test]
    fn test_file_path_completion_no_completions_for_empty() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Empty input should return no completions
        let (_, candidates) = helper.complete("", 0, &ctx).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_file_path_completion_after_text() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "read the src/ma" should complete "src/ma" as the last word
        let input = "read the src/ma";
        let (start, candidates) = helper.complete(input, input.len(), &ctx).unwrap();
        assert_eq!(start, 9); // "read the " is 9 chars
        assert!(has_replacement(&candidates, "src/main.rs"));
    }

    #[test]
    fn test_file_path_completion_directories_have_slash() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "sr" should match "src/" (directory with trailing slash)
        let (start, candidates) = helper.complete("sr", 2, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "src/"));
    }

    #[test]
    fn test_file_path_slash_commands_still_work() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Slash commands should still complete normally
        let (start, candidates) = helper.complete("/he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "/help"));
        assert!(has_replacement(&candidates, "/health"));
    }

    #[test]
    fn test_arg_completion_think_levels() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/think " should offer thinking level completions
        let (start, candidates) = helper.complete("/think ", 7, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(has_replacement(&candidates, "off"));
        assert!(has_replacement(&candidates, "high"));

        // "/think m" should filter to medium/minimal
        let (start, candidates) = helper.complete("/think m", 8, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(has_replacement(&candidates, "medium"));
        assert!(has_replacement(&candidates, "minimal"));
        assert!(!has_replacement(&candidates, "off"));
    }

    #[test]
    fn test_arg_completion_git_subcommands() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/git " should offer git subcommand completions
        let (start, candidates) = helper.complete("/git ", 5, &ctx).unwrap();
        assert_eq!(start, 5);
        assert!(has_replacement(&candidates, "status"));
        assert!(has_replacement(&candidates, "branch"));

        // "/git s" should filter to status and stash
        let (start, candidates) = helper.complete("/git s", 6, &ctx).unwrap();
        assert_eq!(start, 5);
        assert!(has_replacement(&candidates, "status"));
        assert!(has_replacement(&candidates, "stash"));
        assert!(!has_replacement(&candidates, "log"));
    }

    #[test]
    fn test_arg_completion_pr_subcommands() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/pr " should offer PR subcommand completions
        let (start, candidates) = helper.complete("/pr ", 4, &ctx).unwrap();
        assert_eq!(start, 4);
        assert!(has_replacement(&candidates, "create"));
        assert!(has_replacement(&candidates, "checkout"));
    }

    #[test]
    fn test_arg_completion_provider_names() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/provider " should offer provider name completions
        let (start, candidates) = helper.complete("/provider ", 10, &ctx).unwrap();
        assert_eq!(start, 10);
        assert!(has_replacement(&candidates, "anthropic"));
        assert!(has_replacement(&candidates, "openai"));
        assert!(has_replacement(&candidates, "google"));

        // "/provider o" should filter to providers starting with 'o'
        let (start, candidates) = helper.complete("/provider o", 11, &ctx).unwrap();
        assert_eq!(start, 10);
        assert!(has_replacement(&candidates, "openai"));
        assert!(has_replacement(&candidates, "openrouter"));
        assert!(has_replacement(&candidates, "ollama"));
        assert!(!has_replacement(&candidates, "anthropic"));
    }

    #[test]
    fn test_arg_completion_falls_through_to_file_path() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/docs Cargo" should fall through to file path completion since /docs
        // has no custom argument completions
        let (start, candidates) = helper.complete("/docs Cargo", 11, &ctx).unwrap();
        assert_eq!(start, 6); // after "/docs "
        assert!(has_replacement(&candidates, "Cargo.toml"));
    }

    #[test]
    fn test_arg_completion_no_nested_spaces() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/git status " (second space) should NOT trigger arg completion again,
        // it should fall through to file path completion
        let input = "/git status sr";
        let (start, candidates) = helper.complete(input, input.len(), &ctx).unwrap();
        // Should be file path completing "sr" → "src/"
        assert_eq!(start, 12); // after "/git status "
        assert!(
            has_replacement(&candidates, "src/"),
            "Second arg should use file path completion"
        );
    }

    // ── Pair description tests ─────────────────────────────────────

    #[test]
    fn test_slash_completion_pairs_include_descriptions() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/" completion should return Pairs where display contains descriptions
        let (_, candidates) = helper.complete("/", 1, &ctx).unwrap();
        let help_pair = candidates.iter().find(|c| c.replacement == "/help");
        assert!(help_pair.is_some(), "Should include /help");
        let help_display = &help_pair.unwrap().display;
        assert!(
            help_display.contains("Show help"),
            "Display should include description: {help_display}"
        );

        let add_pair = candidates.iter().find(|c| c.replacement == "/add");
        assert!(add_pair.is_some(), "Should include /add");
        let add_display = &add_pair.unwrap().display;
        assert!(
            add_display.contains("Add file"),
            "Display should include description: {add_display}"
        );
    }

    #[test]
    fn test_slash_completion_display_is_padded() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        let (_, candidates) = helper.complete("/", 1, &ctx).unwrap();
        // All slash command pairs should have display wider than replacement
        // (because display includes padding + description)
        for c in &candidates {
            assert!(
                c.display.len() > c.replacement.len(),
                "Display '{}' should be wider than replacement '{}'",
                c.display,
                c.replacement
            );
        }
    }

    #[test]
    fn test_subcommand_pairs_have_matching_display_and_replacement() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Subcommand completions (like /think off) should have display == replacement
        let (_, candidates) = helper.complete("/think ", 7, &ctx).unwrap();
        for c in &candidates {
            assert_eq!(
                c.display, c.replacement,
                "Subcommand display and replacement should match"
            );
        }
    }

    #[test]
    fn test_hinter_shows_command_completion() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // Typing "/he" should suggest "lp — Show help for commands"
        let hint = helper.hint("/he", 3, &ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().starts_with("lp"));
    }

    #[test]
    fn test_hinter_shows_description_for_complete_command() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // Typing "/help" exactly should show description
        let hint = helper.hint("/help", 5, &ctx);
        assert!(hint.is_some());
        let hint_text = hint.unwrap();
        assert!(
            hint_text.contains("—"),
            "Hint should contain em-dash: {hint_text}"
        );
    }

    #[test]
    fn test_hinter_no_hint_when_typing_argument() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // When user is already typing an argument, no hint
        let hint = helper.hint("/add src/", 9, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_shows_arg_hint_after_command_space() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // "/diff " should show argument hints
        let hint = helper.hint("/diff ", 6, &ctx);
        assert!(hint.is_some(), "Should show arg hint for /diff ");
        let hint_text = hint.unwrap();
        assert!(
            hint_text.contains("--stat"),
            "Diff arg hint should contain --stat: {hint_text}"
        );
    }

    #[test]
    fn test_hinter_shows_arg_hint_for_help() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let hint = helper.hint("/help ", 6, &ctx);
        assert!(hint.is_some(), "Should show arg hint for /help ");
        assert!(hint.unwrap().contains("command"));
    }

    #[test]
    fn test_hinter_no_arg_hint_for_no_arg_command() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // /version takes no args, so trailing space should give no hint
        let hint = helper.hint("/version ", 9, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_no_hint_for_non_slash() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let hint = helper.hint("hello", 5, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_no_hint_for_bare_slash() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let hint = helper.hint("/", 1, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_no_hint_when_cursor_not_at_end() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // Cursor at position 2, but line is 5 chars
        let hint = helper.hint("/help", 2, &ctx);
        assert!(hint.is_none());
    }

    // ── looks_incomplete tests ──────────────────────────────────────────

    #[test]
    fn test_looks_incomplete_empty_and_short() {
        assert!(!looks_incomplete(""));
        assert!(!looks_incomplete("ok"));
        assert!(!looks_incomplete("short response"));
    }

    #[test]
    fn test_looks_incomplete_continuation_phrases() {
        assert!(looks_incomplete(
            "I've fixed the first file. Next, I'll update the remaining tests."
        ));
        assert!(looks_incomplete(
            "The build passes now. Let me continue with the second module."
        ));
        assert!(looks_incomplete(
            "That's done. I'll now update the configuration."
        ));
        assert!(looks_incomplete(
            "Changes applied successfully. Moving on to the documentation."
        ));
        assert!(looks_incomplete(
            "First part is complete. Now I need to handle the edge cases."
        ));
        assert!(looks_incomplete(
            "I still need to update the tests for this module."
        ));
    }

    #[test]
    fn test_looks_incomplete_remaining_pattern() {
        assert!(looks_incomplete(
            "I've handled 3 of the 5 files. The remaining files need the same fix."
        ));
        assert!(looks_incomplete(
            "Two remaining tasks need to be completed."
        ));
    }

    #[test]
    fn test_looks_incomplete_trailing_ellipsis() {
        assert!(looks_incomplete("I'll update these files next..."));
        assert!(looks_incomplete("There are a few more things to handle…"));
    }

    #[test]
    fn test_looks_incomplete_numbered_steps() {
        let text = "Here's the plan:\n\
                     1. Fix the parser\n\
                     2. Update tests\n\
                     3. Fix formatting\n\
                     4. Update docs\n\n\
                     I've completed step 1. Fixed the parser.\n\
                     2. Now updating the tests.";
        assert!(looks_incomplete(text));
    }

    #[test]
    fn test_looks_incomplete_completed_work() {
        // These should NOT trigger
        assert!(!looks_incomplete(
            "All done! The tests pass and everything is working correctly."
        ));
        assert!(!looks_incomplete(
            "I've completed all the changes. The build succeeds and tests pass."
        ));
        assert!(!looks_incomplete(
            "Everything looks good. All 5 files have been updated successfully."
        ));
        assert!(!looks_incomplete(
            "The refactoring is complete. Here's a summary of what changed."
        ));
    }

    #[test]
    fn test_looks_incomplete_step_number_extraction() {
        assert_eq!(extract_step_number("step 3: do something"), Some(3));
        assert_eq!(extract_step_number("**step 2**"), Some(2));
        assert_eq!(extract_step_number("1. first item"), Some(1));
        assert_eq!(extract_step_number("12) twelfth item"), Some(12));
        assert_eq!(extract_step_number("no step here"), None);
        assert_eq!(extract_step_number(""), None);
    }

    #[test]
    fn test_looks_incomplete_unclosed_code_block() {
        // Odd number of ``` fences = unclosed code block
        assert!(looks_incomplete(
            "Here's the fix:\n```rust\nfn main() {\n    println!(\"hello\");\n"
        ));
        // Even number = properly closed
        assert!(!looks_incomplete(
            "Here's the fix:\n```rust\nfn main() {}\n```\nAll done."
        ));
        // Multiple blocks, last one unclosed
        assert!(looks_incomplete(
            "First block:\n```\nfoo\n```\nSecond block:\n```\nbar\n"
        ));
    }

    #[test]
    fn test_looks_incomplete_action_phrases() {
        assert!(looks_incomplete(
            "I've reviewed the code. Let me update the configuration file now."
        ));
        assert!(looks_incomplete(
            "Found the bug. Let me fix the parser to handle this edge case."
        ));
        assert!(looks_incomplete(
            "Good, that worked. I'll implement the remaining handlers."
        ));
        assert!(looks_incomplete(
            "Tests pass. Let me create the new module for this feature."
        ));
        assert!(looks_incomplete(
            "That looks correct. Let me write the documentation for it."
        ));
        assert!(looks_incomplete(
            "Alright, I'll add the missing error handling next."
        ));
        assert!(looks_incomplete(
            "The structure looks right. Let me handle the edge case."
        ));
        assert!(looks_incomplete(
            "I see the issue. I'll modify the return type to fix it."
        ));
    }

    #[test]
    fn test_looks_incomplete_ordinal_progression() {
        // "first" without "finally" or "second" — more steps expected
        assert!(looks_incomplete(
            "First, I'll fix the parser. Then we need to update the tests and docs."
        ));
        // "first" and "second" without "third" or "finally" — still incomplete
        assert!(looks_incomplete(
            "First, I fixed the parser. Second, I updated the tests."
        ));
        // "first" with "finally" — complete
        assert!(!looks_incomplete(
            "First, I fixed the parser. Second, the tests. Finally, the docs are updated."
        ));
    }

    #[test]
    fn test_looks_incomplete_step_x_of_y() {
        assert!(looks_incomplete(
            "That's step 2 of 5 done. Moving to the next one."
        ));
        assert!(looks_incomplete(
            "Completed step 1 of 4. The parser is now fixed."
        ));
        assert!(looks_incomplete(
            "I've finished step 3/7 for this refactoring."
        ));
        // Step X of Y where X == Y — complete, should NOT trigger
        assert!(!looks_incomplete(
            "That's step 5 of 5 done. Everything is complete."
        ));
        // Step X of Y where X > Y — should NOT trigger
        assert!(!looks_incomplete(
            "We ended up doing step 6 of 5 as a bonus. All done!"
        ));
    }

    #[test]
    fn test_looks_incomplete_negative_cases_new_patterns() {
        // Completed work — should NOT trigger any pattern
        assert!(!looks_incomplete(
            "I've updated all the files and the tests pass. Everything is working."
        ));
        assert!(!looks_incomplete(
            "```rust\nfn main() {}\n```\nThe implementation is complete and all tests pass."
        ));
        assert!(!looks_incomplete(
            "First, I fixed the parser. Second, the tests. Finally, the documentation. All done!"
        ));
    }

    #[test]
    fn test_step_x_of_y_incomplete() {
        assert_eq!(step_x_of_y_incomplete("step 2 of 5"), Some(true));
        assert_eq!(step_x_of_y_incomplete("step 3/7"), Some(true));
        assert_eq!(step_x_of_y_incomplete("step 1 of 4 done"), Some(true));
        assert_eq!(step_x_of_y_incomplete("step 5 of 5"), Some(false));
        assert_eq!(step_x_of_y_incomplete("step 7 of 5"), Some(false));
        assert_eq!(step_x_of_y_incomplete("no steps here"), None);
    }

    #[test]
    fn test_get_max_auto_continues_default() {
        let config = std::collections::HashMap::new();
        assert_eq!(get_max_auto_continues(&config, false), 5);
    }

    #[test]
    fn test_get_max_auto_continues_disabled() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_continue".to_string(), "false".to_string());
        assert_eq!(get_max_auto_continues(&config, false), 0);
    }

    #[test]
    fn test_get_max_auto_continues_custom_value() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "8".to_string());
        assert_eq!(get_max_auto_continues(&config, false), 8);
    }

    #[test]
    fn test_get_max_auto_continues_plan_apply_raises_minimum() {
        let config = std::collections::HashMap::new();
        // Default is 5, but plan-apply mode raises minimum to 10
        assert_eq!(get_max_auto_continues(&config, true), 10);
    }

    #[test]
    fn test_get_max_auto_continues_plan_apply_keeps_higher() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "15".to_string());
        // User set 15, plan-apply minimum is 10, so 15 wins
        assert_eq!(get_max_auto_continues(&config, true), 15);
    }

    #[test]
    fn test_get_max_auto_continues_disabled_overrides_plan_apply() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_continue".to_string(), "false".to_string());
        // Disabled means 0 even in plan-apply mode
        assert_eq!(get_max_auto_continues(&config, true), 0);
    }

    #[test]
    fn test_get_max_auto_continues_zero_explicit() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "0".to_string());
        assert_eq!(get_max_auto_continues(&config, false), 0);
    }
}
