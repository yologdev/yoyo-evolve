//! Interactive REPL loop and related helpers (tab-completion, multi-line input).

use std::time::{Duration, Instant};

use crate::cli::*;
use crate::commands::{self, auto_compact_if_needed, command_arg_completions, KNOWN_COMMANDS};
use crate::dispatch::CommandResult;
use crate::format::*;
use crate::git::*;
use crate::prompt::*;
use crate::AgentConfig;

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

/// Returns when the user exits (via /quit, /exit, Ctrl-D, etc.).
#[allow(clippy::too_many_arguments)]
pub async fn run_repl(
    agent_config: &mut AgentConfig,
    agent: &mut yoagent::agent::Agent,
    mcp_count: u32,
    openapi_count: u32,
    continue_session: bool,
    update_available: Option<String>,
    mcp_cli_servers: Vec<String>,
    mcp_server_configs: Vec<crate::cli::McpServerConfig>,
) {
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
    if mcp_count > 0 {
        println!("{DIM}  mcp: {mcp_count} server(s) connected{RESET}");
    }
    if openapi_count > 0 {
        println!("{DIM}  openapi: {openapi_count} spec(s) loaded{RESET}");
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
    if let Some(ref latest) = update_available {
        println!(
            "  {YELLOW}⬆ Update available: v{latest} (you have v{VERSION}) — https://github.com/yologdev/yoyo-evolve/releases{RESET}\n"
        );
    }

    // Hint about previous session if one exists and --continue wasn't used
    if !continue_session && commands::last_session_exists() {
        println!(
            "{DIM}  💡 Previous session found. Use {YELLOW}--continue{RESET}{DIM} or {YELLOW}/load .yoyo/last-session.json{RESET}{DIM} to resume.{RESET}\n"
        );
    }

    // Auto-enable watch mode if a project type is detected and config allows it
    if get_watch_command().is_none() && agent_config.auto_watch {
        if let Some(cmd) = crate::commands_dev::auto_detect_watch_command() {
            set_watch_command(&cmd);
            println!(
                "{DIM}  👀 Auto-watch: `{cmd}` (disable with /watch off or auto_watch = false){RESET}\n"
            );
        }
    }

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

    loop {
        let prompt = if let Some(branch) = git_branch() {
            if commands::is_plan_mode() {
                format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{YELLOW}📋{RESET} {BOLD}{GREEN}🐙 › {RESET}")
            } else {
                format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{GREEN}🐙 › {RESET}")
            }
        } else if commands::is_plan_mode() {
            format!("{BOLD}{YELLOW}📋{RESET} {BOLD}{GREEN}🐙 › {RESET}")
        } else {
            format!("{BOLD}{GREEN}🐙 › {RESET}")
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
            mcp_cli_servers: &mcp_cli_servers,
            mcp_server_configs: &mcp_server_configs,
            mcp_count,
            openapi_count,
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
        let outcome = if !file_results.is_empty() {
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
        crate::format::maybe_ring_bell(prompt_start.elapsed());
        last_error = outcome.last_tool_error.clone();

        // Notify the user if the context was auto-compacted due to overflow
        if outcome.was_overflow {
            eprintln!("{YELLOW}  ℹ Context was auto-compacted (overflow detected){RESET}");
        }

        // Fallback provider: if the API failed and a fallback is configured, switch and retry
        if outcome.last_api_error.is_some() {
            let old_provider = agent_config.provider.clone();
            let fallback_name = agent_config.fallback_provider.clone();
            if agent_config.try_switch_to_fallback() {
                let fallback = fallback_name.as_deref().unwrap_or("unknown");
                eprintln!(
                    "\n{YELLOW}  ⚡ Primary provider '{}' failed. Switching to fallback '{}'...{RESET}",
                    old_provider, fallback
                );

                // Rebuild agent with the new provider
                *agent = agent_config.build_agent();

                eprintln!(
                    "{DIM}  now using: {} / {}{RESET}\n",
                    agent_config.provider, agent_config.model
                );

                // Retry the same prompt with the fallback provider
                let retry_outcome = run_prompt_auto_retry(
                    agent,
                    input,
                    &mut session_total,
                    &agent_config.model,
                    &session_changes,
                )
                .await;
                last_error = retry_outcome.last_tool_error.clone();

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
        let changes_after: Vec<String> = session_changes
            .snapshot()
            .iter()
            .map(|c| c.path.clone())
            .collect();
        for path in &changes_after {
            if !changes_before.contains(path) {
                // This file was touched for the first time in this turn
                if turn_snap.originals.contains_key(path.as_str()) {
                    // Already snapshotted (e.g., was in git diff) — keep the original
                } else if std::path::Path::new(path).exists() {
                    // File was created during this turn
                    turn_snap.record_created(path);
                }
            }
        }
        // Also check for new files from git that weren't in session_changes
        if let Ok(diff_files) = crate::git::run_git(&["diff", "--name-only"]) {
            for f in diff_files.lines().filter(|l| !l.is_empty()) {
                if !turn_snap.originals.contains_key(f) {
                    turn_snap.snapshot_file(f);
                }
            }
        }
        turn_history.push(turn_snap);

        // ── Watch mode: auto-run test/lint command after agent edits ───────
        let files_modified = changes_after.len() > changes_before.len();
        if files_modified {
            if let Some(watch_cmd) = get_watch_command() {
                let (ok, output) = run_watch_command(&watch_cmd);
                if ok {
                    eprintln!("{GREEN}  ✓ Watch passed: `{watch_cmd}`{RESET}");
                } else {
                    eprintln!("{RED}  ✗ Watch failed: `{watch_cmd}`{RESET}");
                    // Show truncated output
                    let display_output = if output.len() > 2000 {
                        format!("{}...\n(truncated)", safe_truncate(&output, 2000))
                    } else {
                        output.clone()
                    };
                    eprintln!("{DIM}{display_output}{RESET}");
                    // Multi-attempt auto-fix loop
                    let mut current_output = output;
                    for attempt in 1..=MAX_WATCH_FIX_ATTEMPTS {
                        if session_budget_exhausted(30) {
                            eprintln!(
                                "{DIM}  ⏱ session budget nearly exhausted, stopping watch fix loop early{RESET}"
                            );
                            break;
                        }
                        eprintln!(
                            "{YELLOW}  → Auto-fixing (attempt {attempt}/{MAX_WATCH_FIX_ATTEMPTS})...{RESET}"
                        );

                        let fix_prompt = build_watch_fix_prompt(&watch_cmd, &current_output);
                        let fix_outcome = run_prompt_auto_retry(
                            agent,
                            &fix_prompt,
                            &mut session_total,
                            &agent_config.model,
                            &session_changes,
                        )
                        .await;
                        last_error = fix_outcome.last_tool_error.clone();

                        // Re-run watch command to see if fix worked
                        let (fix_ok, fix_output) = run_watch_command(&watch_cmd);
                        if fix_ok {
                            eprintln!(
                                "{GREEN}  ✓ Watch passed after fix (attempt {attempt}){RESET}"
                            );
                            break;
                        } else if attempt == MAX_WATCH_FIX_ATTEMPTS {
                            eprintln!(
                                "{RED}  ✗ Watch still failing after {MAX_WATCH_FIX_ATTEMPTS} attempts — manual fix needed{RESET}"
                            );
                        } else {
                            eprintln!("{RED}  ✗ Attempt {attempt} failed, retrying...{RESET}");
                            // Feed the latest failure output into the next fix attempt
                            current_output = fix_output;
                        }
                    }
                }
            }
        }

        // ── Auto-commit: stage and commit if flag is on and files changed ─────
        if agent_config.auto_commit && files_modified {
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
        auto_compact_if_needed(agent);
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

/// Build content blocks from `/add` results, ensuring images always have
/// accompanying text context so the model can see them.
///
/// For each `AddResult::Image`, a `Content::Text` label is inserted *before*
/// the `Content::Image` block (e.g. `"[Image: photo.png (42 KB, image/png)]"`).
/// If the entire batch contains only images (no text files), a general
/// introductory text block is prepended at the start.
pub fn build_add_content_blocks(results: &[commands::AddResult]) -> Vec<yoagent::types::Content> {
    if results.is_empty() {
        return Vec::new();
    }

    let mut blocks: Vec<yoagent::types::Content> = Vec::new();

    let has_text_file = results
        .iter()
        .any(|r| matches!(r, commands::AddResult::Text { .. }));

    // If there are only images and no text files, prepend a contextual intro
    if !has_text_file {
        blocks.push(yoagent::types::Content::Text {
            text: "The user is sharing the following image(s) for you to analyze:".to_string(),
        });
    }

    for result in results {
        match result {
            commands::AddResult::Text { content, .. } => {
                blocks.push(yoagent::types::Content::Text {
                    text: content.clone(),
                });
            }
            commands::AddResult::Image {
                summary,
                data,
                mime_type,
            } => {
                // Extract a readable label from the summary (which contains the
                // filename, size, and mime type). The summary looks like:
                //   "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m"
                // We extract what's between "added image " and the RESET code,
                // but if parsing fails, fall back to the mime_type alone.
                let label = extract_image_label(summary, mime_type);
                blocks.push(yoagent::types::Content::Text {
                    text: format!("[Image: {label}]"),
                });
                blocks.push(yoagent::types::Content::Image {
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                });
            }
        }
    }

    blocks
}

/// Extract a human-readable label from an AddResult::Image summary string.
/// The summary has ANSI codes and looks like:
///   "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m"
/// We want: "photo.png (42 KB, image/png)"
fn extract_image_label(summary: &str, fallback_mime: &str) -> String {
    // Strip ANSI escape codes first
    let stripped: String = {
        let mut out = String::new();
        let mut in_escape = false;
        for ch in summary.chars() {
            if ch == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if ch.is_ascii_alphabetic() {
                    in_escape = false;
                }
            } else {
                out.push(ch);
            }
        }
        out
    };

    // Try to find "added image " and extract everything after it
    if let Some(idx) = stripped.find("added image ") {
        let after = &stripped[idx + "added image ".len()..];
        let trimmed = after.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // Fallback
    format!("image ({fallback_mime})")
}

// ── Side conversations ──

/// Parse a `/side` question from the input. Returns `None` if no question provided.
fn parse_side_question(input: &str) -> Option<String> {
    let question = input.strip_prefix("/side").unwrap_or("").trim().to_string();
    if question.is_empty() {
        None
    } else {
        Some(question)
    }
}

/// Handle a `/side <question>` command — quick Q&A without touching main context.
pub(crate) async fn handle_side(input: &str, agent_config: &AgentConfig) {
    let question = match parse_side_question(input) {
        Some(q) => q,
        None => {
            eprintln!(
                "{YELLOW}  Usage: /side <question>{RESET}\n\
                 {DIM}  Ask a quick question without affecting the main conversation.\n\
                 {DIM}  No tools — just text Q&A. Fast and cheap.\n\n\
                 {DIM}  Examples:\n\
                 {DIM}    /side what's the syntax for a match guard in Rust?\n\
                 {DIM}    /side explain the difference between clone and copy{RESET}\n"
            );
            return;
        }
    };

    eprintln!("{DIM}  [side] thinking...{RESET}");

    let mut side_agent = agent_config.build_side_agent();
    let mut rx = side_agent.prompt(&question).await;

    let mut md_renderer = MarkdownRenderer::new();
    let mut collected_text = String::new();
    let mut started = false;

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                if !started {
                    // Print a side-conversation header on first text
                    print!("\n{DIM}[side]{RESET} ");
                    started = true;
                }
                collected_text.push_str(&delta);
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

    side_agent.finish().await;

    if !started {
        eprintln!("{DIM}  [side] (no response){RESET}");
    } else {
        println!(); // newline after streamed text
    }

    // Show side conversation cost
    let messages = side_agent.messages();
    let mut side_usage = Usage::default();
    for msg in messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            side_usage.input += usage.input;
            side_usage.output += usage.output;
            side_usage.cache_read += usage.cache_read;
            side_usage.cache_write += usage.cache_write;
        }
    }
    let total_tokens = side_usage.input + side_usage.output;
    if total_tokens > 0 {
        let cost = estimate_cost(&side_usage, &agent_config.model);
        if let Some(c) = cost {
            eprintln!("{DIM}  [side] {} tokens, ${:.4}{RESET}\n", total_tokens, c);
        } else {
            eprintln!("{DIM}  [side] {} tokens{RESET}\n", total_tokens);
        }
    } else {
        eprintln!();
    }
}

// ── Quick mode ──

fn parse_quick_question(input: &str) -> Option<String> {
    let question = input
        .strip_prefix("/quick")
        .unwrap_or("")
        .trim()
        .to_string();
    if question.is_empty() {
        None
    } else {
        Some(question)
    }
}

/// Handle a `/quick <question>` command — fast single-turn answer without tools or agent loop.
pub(crate) async fn handle_quick(input: &str, agent_config: &AgentConfig) {
    let question = match parse_quick_question(input) {
        Some(q) => q,
        None => {
            eprintln!(
                "{YELLOW}  Usage: /quick <question>{RESET}\n\
                 {DIM}  Fast single-turn answer without tools or agent loop.\n\
                 {DIM}  Great for quick lookups, syntax help, and explanations.\n\n\
                 {DIM}  Examples:\n\
                 {DIM}    /quick what does this error mean: borrow of moved value?\n\
                 {DIM}    /quick how do I use sed to replace X with Y?\n\
                 {DIM}    /quick explain the difference between async and threading{RESET}\n"
            );
            return;
        }
    };

    eprintln!("{DIM}  [quick] thinking...{RESET}");

    let mut side_agent = agent_config.build_side_agent();
    let mut rx = side_agent.prompt(&question).await;

    let mut md_renderer = MarkdownRenderer::new();
    let mut collected_text = String::new();
    let mut started = false;

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                if !started {
                    print!("\n{DIM}[quick]{RESET} ");
                    started = true;
                }
                collected_text.push_str(&delta);
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

    side_agent.finish().await;

    if !started {
        eprintln!("{DIM}  [quick] (no response){RESET}");
    } else {
        println!(); // newline after streamed text
    }

    // Show quick query cost
    let messages = side_agent.messages();
    let mut quick_usage = Usage::default();
    for msg in messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            quick_usage.input += usage.input;
            quick_usage.output += usage.output;
            quick_usage.cache_read += usage.cache_read;
            quick_usage.cache_write += usage.cache_write;
        }
    }
    let total_tokens = quick_usage.input + quick_usage.output;
    if total_tokens > 0 {
        let cost = estimate_cost(&quick_usage, &agent_config.model);
        if let Some(c) = cost {
            eprintln!("{DIM}  [quick] {} tokens, ${:.4}{RESET}\n", total_tokens, c);
        } else {
            eprintln!("{DIM}  [quick] {} tokens{RESET}\n", total_tokens);
        }
    } else {
        eprintln!();
    }
}

// ── Extended mode ──

/// Default maximum turns for extended autonomous mode.
const DEFAULT_EXTENDED_TURNS: usize = 20;

/// Parse the `/extended` command input, extracting the prompt, optional `--turns N`,
/// and optional `--budget N` (time limit in minutes).
///
/// Returns `(prompt, max_turns, budget)`. If `--turns N` is present, it is stripped
/// from the prompt and used as the turn limit. If `--budget N` is present, it is
/// stripped and returned as `Some(Duration)`. Otherwise defaults apply.
fn parse_extended_args(input: &str) -> (String, usize, Option<Duration>) {
    let raw = input
        .strip_prefix("/extended")
        .unwrap_or(input)
        .trim()
        .to_string();

    // Look for --turns N and --budget N anywhere in the string
    let mut turns = DEFAULT_EXTENDED_TURNS;
    let mut budget: Option<Duration> = None;
    let mut prompt_parts: Vec<&str> = Vec::new();
    let words: Vec<&str> = raw.split_whitespace().collect();
    let mut skip_next = false;

    for (i, word) in words.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if *word == "--turns" {
            if let Some(next) = words.get(i + 1) {
                if let Ok(n) = next.parse::<usize>() {
                    turns = n.max(1); // At least 1 turn
                    skip_next = true;
                    continue;
                }
            }
        }
        if *word == "--budget" {
            if let Some(next) = words.get(i + 1) {
                if let Ok(mins) = next.parse::<u64>() {
                    if mins > 0 {
                        budget = Some(Duration::from_secs(mins * 60));
                    }
                    skip_next = true;
                    continue;
                }
            }
        }
        prompt_parts.push(word);
    }

    let prompt = prompt_parts.join(" ");
    (prompt, turns, budget)
}

/// Build the system-level instruction for extended autonomous mode.
fn build_extended_system_prompt(task: &str, max_turns: usize) -> String {
    format!(
        "You are in EXTENDED AUTONOMOUS MODE. Work on this task step by step:\n\n\
         {task}\n\n\
         Rules for extended mode:\n\
         - Work autonomously — do NOT ask the user questions. Make your best judgment.\n\
         - Break the task into steps and execute them one at a time.\n\
         - Run tests after making changes to verify correctness.\n\
         - If you get stuck, explain what you tried and move on.\n\
         - You have up to {max_turns} turns to complete this task.\n\
         - When the task is complete, summarize what you did and what files were modified."
    )
}

/// Handle the `/extended` command — run the agent in autonomous mode with a turn budget.
pub(crate) async fn handle_extended(
    input: &str,
    agent: &mut yoagent::agent::Agent,
    session_total: &mut Usage,
    model: &str,
    session_changes: &SessionChanges,
) -> Option<String> {
    let (prompt, max_turns, budget) = parse_extended_args(input);

    if prompt.is_empty() {
        eprintln!(
            "{YELLOW}  Usage: /extended <task description> [--turns N] [--budget N]{RESET}\n\
             {DIM}  Run the agent autonomously on a task (default: {DEFAULT_EXTENDED_TURNS} turns).\n\
             {DIM}  --budget N sets a wall-clock time limit in minutes.\n\
             \n\
             {DIM}  Examples:\n\
             {DIM}    /extended add error handling to the parser module\n\
             {DIM}    /extended refactor the auth system --turns 30\n\
             {DIM}    /extended rebuild the test suite --budget 15{RESET}\n"
        );
        return None;
    }

    let budget_label = if let Some(dur) = budget {
        format!(" | budget: {} min", dur.as_secs() / 60)
    } else {
        String::new()
    };

    eprintln!(
        "\n{BOLD_CYAN}  🐙 Extended mode{RESET} — working autonomously ({max_turns} turns max{budget_label})\n\
         {DIM}  Task: {prompt}{RESET}\n"
    );

    let extended_prompt = build_extended_system_prompt(&prompt, max_turns);

    // Run the task using the existing prompt infrastructure with auto-retry.
    // If a budget is set, wrap in tokio::time::timeout.
    let prompt_start = Instant::now();
    let timed_out;

    if let Some(dur) = budget {
        match tokio::time::timeout(
            dur,
            run_prompt_auto_retry(
                agent,
                &extended_prompt,
                session_total,
                model,
                session_changes,
            ),
        )
        .await
        {
            Ok(_outcome) => {
                timed_out = false;
            }
            Err(_elapsed) => {
                timed_out = true;
            }
        }
    } else {
        let _outcome = run_prompt_auto_retry(
            agent,
            &extended_prompt,
            session_total,
            model,
            session_changes,
        )
        .await;
        timed_out = false;
    }

    let elapsed = prompt_start.elapsed();

    if timed_out {
        let budget_mins = budget.map(|d| d.as_secs() / 60).unwrap_or(0);
        eprintln!(
            "\n{YELLOW}  🐙 Extended mode — time budget exhausted ({budget_mins} min){RESET}"
        );
    }

    // Run watch command after prompt if active (auto lint/test loop)
    if !timed_out {
        run_watch_after_prompt(agent, session_total, model, session_changes).await;
    }

    // Summary
    let files_changed = session_changes.snapshot().len();
    eprintln!(
        "\n{BOLD_CYAN}  🐙 Extended mode complete{RESET}\n\
         {DIM}  Time: {elapsed:.1?} | Files modified: {files_changed}{RESET}\n"
    );

    // Return the prompt so it can be set as last_input for /retry
    Some(extended_prompt)
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

    // ── build_add_content_blocks ─────────────────────────────────────

    #[test]
    fn add_content_blocks_image_only_has_intro_and_label() {
        let results = vec![commands::AddResult::Image {
            summary: "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m".to_string(),
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        }];
        let blocks = build_add_content_blocks(&results);

        // Should be: intro text, label text, image = 3 blocks
        assert_eq!(blocks.len(), 3, "expected intro + label + image");

        // First block: introductory text
        match &blocks[0] {
            yoagent::types::Content::Text { text } => {
                assert!(
                    text.contains("image(s)"),
                    "intro should mention images: {text}"
                );
            }
            other => panic!("expected Text intro, got {other:?}"),
        }

        // Second block: image label text
        match &blocks[1] {
            yoagent::types::Content::Text { text } => {
                assert!(
                    text.starts_with("[Image:"),
                    "label should start with [Image:: {text}"
                );
                assert!(
                    text.contains("photo.png"),
                    "label should contain filename: {text}"
                );
            }
            other => panic!("expected Text label, got {other:?}"),
        }

        // Third block: actual image
        match &blocks[2] {
            yoagent::types::Content::Image { data, mime_type } => {
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn add_content_blocks_text_only_no_intro() {
        let results = vec![commands::AddResult::Text {
            summary: "added foo.rs".to_string(),
            content: "fn main() {}".to_string(),
        }];
        let blocks = build_add_content_blocks(&results);

        // Text-only: no intro, just the text block
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            yoagent::types::Content::Text { text } => {
                assert_eq!(text, "fn main() {}");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn add_content_blocks_mixed_text_and_image() {
        let results = vec![
            commands::AddResult::Text {
                summary: "added main.rs".to_string(),
                content: "fn main() {}".to_string(),
            },
            commands::AddResult::Image {
                summary: "\x1b[32m  ✓ added image logo.png (10 KB, image/png)\x1b[0m".to_string(),
                data: "imgdata".to_string(),
                mime_type: "image/png".to_string(),
            },
        ];
        let blocks = build_add_content_blocks(&results);

        // Mixed: no intro (text file present), text + label + image = 3 blocks
        assert_eq!(blocks.len(), 3, "expected text + label + image");

        // First: text file content
        match &blocks[0] {
            yoagent::types::Content::Text { text } => {
                assert_eq!(text, "fn main() {}");
            }
            other => panic!("expected Text, got {other:?}"),
        }

        // Second: image label
        match &blocks[1] {
            yoagent::types::Content::Text { text } => {
                assert!(text.starts_with("[Image:"), "label: {text}");
                assert!(
                    text.contains("logo.png"),
                    "label should have filename: {text}"
                );
            }
            other => panic!("expected Text label, got {other:?}"),
        }

        // Third: image data
        match &blocks[2] {
            yoagent::types::Content::Image { data, mime_type } => {
                assert_eq!(data, "imgdata");
                assert_eq!(mime_type, "image/png");
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn add_content_blocks_multiple_images_each_has_label() {
        let results = vec![
            commands::AddResult::Image {
                summary: "\x1b[32m  ✓ added image a.jpg (5 KB, image/jpeg)\x1b[0m".to_string(),
                data: "d1".to_string(),
                mime_type: "image/jpeg".to_string(),
            },
            commands::AddResult::Image {
                summary: "\x1b[32m  ✓ added image b.webp (8 KB, image/webp)\x1b[0m".to_string(),
                data: "d2".to_string(),
                mime_type: "image/webp".to_string(),
            },
        ];
        let blocks = build_add_content_blocks(&results);

        // intro + (label + image) × 2 = 5 blocks
        assert_eq!(blocks.len(), 5, "expected intro + 2×(label+image)");

        // Verify intro
        assert!(
            matches!(&blocks[0], yoagent::types::Content::Text { text } if text.contains("image(s)"))
        );

        // Verify label-then-image ordering for first image
        assert!(
            matches!(&blocks[1], yoagent::types::Content::Text { text } if text.contains("a.jpg"))
        );
        assert!(matches!(&blocks[2], yoagent::types::Content::Image { data, .. } if data == "d1"));

        // Verify label-then-image ordering for second image
        assert!(
            matches!(&blocks[3], yoagent::types::Content::Text { text } if text.contains("b.webp"))
        );
        assert!(matches!(&blocks[4], yoagent::types::Content::Image { data, .. } if data == "d2"));
    }

    #[test]
    fn add_content_blocks_empty_input() {
        let blocks = build_add_content_blocks(&[]);
        assert!(blocks.is_empty(), "empty input should produce empty output");
    }

    #[test]
    fn extract_image_label_parses_ansi_summary() {
        let label = extract_image_label(
            "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m",
            "image/png",
        );
        assert_eq!(label, "photo.png (42 KB, image/png)");
    }

    #[test]
    fn extract_image_label_fallback() {
        let label = extract_image_label("something unexpected", "image/jpeg");
        assert_eq!(label, "image (image/jpeg)");
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

    // ── parse_extended_args tests ──

    #[test]
    fn test_parse_extended_args_basic_prompt() {
        let (prompt, turns, budget) = parse_extended_args("/extended build a REST API");
        assert_eq!(prompt, "build a REST API");
        assert_eq!(turns, DEFAULT_EXTENDED_TURNS);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_with_turns() {
        let (prompt, turns, budget) = parse_extended_args("/extended refactor auth --turns 10");
        assert_eq!(prompt, "refactor auth");
        assert_eq!(turns, 10);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_at_start() {
        let (prompt, turns, budget) = parse_extended_args("/extended --turns 5 fix all bugs");
        assert_eq!(prompt, "fix all bugs");
        assert_eq!(turns, 5);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_in_middle() {
        let (prompt, turns, budget) =
            parse_extended_args("/extended add tests --turns 15 for parser");
        assert_eq!(prompt, "add tests for parser");
        assert_eq!(turns, 15);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_no_prompt() {
        let (prompt, turns, budget) = parse_extended_args("/extended");
        assert!(prompt.is_empty());
        assert_eq!(turns, DEFAULT_EXTENDED_TURNS);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_minimum_one() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --turns 0");
        assert_eq!(prompt, "do stuff");
        assert_eq!(turns, 1); // Clamped to 1
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_invalid_turns_kept_as_prompt() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --turns abc");
        assert_eq!(prompt, "do stuff --turns abc");
        assert_eq!(turns, DEFAULT_EXTENDED_TURNS);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_without_value() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --turns");
        assert_eq!(prompt, "do stuff --turns");
        assert_eq!(turns, DEFAULT_EXTENDED_TURNS);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --budget 10");
        assert_eq!(prompt, "do stuff");
        assert_eq!(turns, DEFAULT_EXTENDED_TURNS);
        assert_eq!(budget, Some(Duration::from_secs(600)));
    }

    #[test]
    fn test_parse_extended_turns_and_budget() {
        let (prompt, turns, budget) =
            parse_extended_args("/extended rebuild tests --turns 30 --budget 15");
        assert_eq!(prompt, "rebuild tests");
        assert_eq!(turns, 30);
        assert_eq!(budget, Some(Duration::from_secs(900)));
    }

    #[test]
    fn test_parse_extended_no_budget() {
        let (prompt, turns, budget) = parse_extended_args("/extended simple task");
        assert_eq!(prompt, "simple task");
        assert_eq!(turns, DEFAULT_EXTENDED_TURNS);
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget_zero_ignored() {
        let (prompt, _turns, budget) = parse_extended_args("/extended task --budget 0");
        assert_eq!(prompt, "task");
        // --budget 0 is consumed (skip_next fires) but budget stays None
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget_invalid_kept_as_prompt() {
        let (prompt, _turns, budget) = parse_extended_args("/extended task --budget abc");
        assert_eq!(prompt, "task --budget abc");
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget_without_value() {
        let (prompt, _turns, budget) = parse_extended_args("/extended task --budget");
        assert_eq!(prompt, "task --budget");
        assert!(budget.is_none());
    }

    #[test]
    fn test_build_extended_system_prompt_contains_task() {
        let prompt = build_extended_system_prompt("build a REST API", 20);
        assert!(prompt.contains("build a REST API"));
        assert!(prompt.contains("20"));
        assert!(prompt.contains("EXTENDED AUTONOMOUS MODE"));
        assert!(prompt.contains("do NOT ask the user questions"));
    }

    // ── /side parsing tests ──

    #[test]
    fn test_parse_side_question_basic() {
        let q = parse_side_question("/side what is a monad?");
        assert_eq!(q.unwrap(), "what is a monad?");
    }

    #[test]
    fn test_parse_side_question_empty() {
        assert!(parse_side_question("/side").is_none());
        assert!(parse_side_question("/side   ").is_none());
    }

    #[test]
    fn test_parse_side_question_preserves_whitespace_in_question() {
        let q = parse_side_question("/side   what   is   this  ");
        assert_eq!(q.unwrap(), "what   is   this");
    }

    #[test]
    fn test_parse_side_question_multiword() {
        let q = parse_side_question("/side how do I convert Vec<u8> to String in Rust?");
        assert_eq!(q.unwrap(), "how do I convert Vec<u8> to String in Rust?");
    }

    #[test]
    fn test_parse_quick_question_basic() {
        let q = parse_quick_question("/quick what does borrow of moved value mean?");
        assert_eq!(q.unwrap(), "what does borrow of moved value mean?");
    }

    #[test]
    fn test_parse_quick_question_empty() {
        assert!(parse_quick_question("/quick").is_none());
        assert!(parse_quick_question("/quick   ").is_none());
    }

    #[test]
    fn test_parse_quick_question_preserves_content() {
        let q = parse_quick_question("/quick   how do I use sed?  ");
        assert_eq!(q.unwrap(), "how do I use sed?");
    }

    #[test]
    fn test_parse_quick_question_multiword() {
        let q = parse_quick_question("/quick explain async vs threading in Rust");
        assert_eq!(q.unwrap(), "explain async vs threading in Rust");
    }
}
