//! Interactive REPL loop and related helpers (tab-completion, multi-line input).

use crate::cli::*;
use crate::commands::{
    self, auto_compact_if_needed, clear_confirmation_message, command_arg_completions,
    is_unknown_command, thinking_level_name, KNOWN_COMMANDS,
};
use crate::format::*;
use crate::git::*;
use crate::prompt::*;
use crate::AgentConfig;

use rustyline::completion::Completer;
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
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        let prefix = &line[..pos];

        // Slash command completion: starts with '/' and no space yet
        if prefix.starts_with('/') && !prefix.contains(' ') {
            let matches: Vec<String> = KNOWN_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|cmd| cmd.to_string())
                .collect();
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
                        return Ok((space_pos + 1, candidates));
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

        let matches = complete_file_path(word);
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
}

impl Highlighter for YoyoHelper {}

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

/// Run the interactive REPL loop.
///
/// Takes ownership of the agent config and agent, plus state flags from main.
/// Returns when the user exits (via /quit, /exit, Ctrl-D, etc.).
pub async fn run_repl(
    agent_config: &mut AgentConfig,
    agent: &mut yoagent::agent::Agent,
    mcp_count: u32,
    openapi_count: u32,
    continue_session: bool,
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
    if let Some(branch) = git_branch() {
        println!("{DIM}  git:   {branch}{RESET}");
    }
    println!("{DIM}  cwd:   {cwd}{RESET}\n");

    // Hint about previous session if one exists and --continue wasn't used
    if !continue_session && commands::last_session_exists() {
        println!(
            "{DIM}  💡 Previous session found. Use {YELLOW}--continue{RESET}{DIM} or {YELLOW}/load .yoyo/last-session.json{RESET}{DIM} to resume.{RESET}\n"
        );
    }

    // Set up rustyline editor with slash-command tab-completion
    let mut rl = Editor::new().expect("Failed to initialize readline");
    rl.set_helper(Some(YoyoHelper));
    if let Some(history_path) = history_file_path() {
        if rl.load_history(&history_path).is_err() {
            // First run or history file doesn't exist yet — that's fine
        }
    }

    let mut session_total = Usage::default();
    let mut last_input: Option<String> = None;
    let mut last_error: Option<String> = None;
    let mut bookmarks = commands::Bookmarks::new();
    let session_changes = SessionChanges::new();
    let spawn_tracker = commands::SpawnTracker::new();

    loop {
        let prompt = if let Some(branch) = git_branch() {
            format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{GREEN}> {RESET}")
        } else {
            format!("{BOLD}{GREEN}> {RESET}")
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

        match input {
            "/quit" | "/exit" => break,
            s if s == "/help" || s.starts_with("/help ") => {
                if !commands::handle_help_command(s) {
                    commands::handle_help();
                }
                continue;
            }
            "/version" => {
                commands::handle_version();
                continue;
            }
            "/status" => {
                commands::handle_status(&agent_config.model, &cwd, &session_total);
                continue;
            }
            "/tokens" => {
                commands::handle_tokens(agent, &session_total, &agent_config.model);
                continue;
            }
            "/cost" => {
                commands::handle_cost(&session_total, &agent_config.model);
                continue;
            }
            "/clear" => {
                let messages = agent.messages();
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
                            continue;
                        }
                    } else {
                        println!("{DIM}  (clear cancelled){RESET}\n");
                        continue;
                    }
                }
                *agent = agent_config.build_agent();
                session_changes.clear();
                println!("{DIM}  (conversation cleared){RESET}\n");
                continue;
            }
            "/clear!" => {
                *agent = agent_config.build_agent();
                session_changes.clear();
                println!("{DIM}  (conversation force-cleared){RESET}\n");
                continue;
            }
            "/model" => {
                commands::handle_model_show(&agent_config.model);
                continue;
            }
            s if s.starts_with("/model ") => {
                let new_model = s.trim_start_matches("/model ").trim();
                if new_model.is_empty() {
                    println!("{DIM}  current model: {}", agent_config.model);
                    println!("  usage: /model <name>{RESET}\n");
                    continue;
                }
                agent_config.model = new_model.to_string();
                // Rebuild agent with new model, preserving conversation
                let saved = agent.save_messages().ok();
                *agent = agent_config.build_agent();
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                println!("{DIM}  (switched to {new_model}, conversation preserved){RESET}\n");
                continue;
            }
            "/provider" => {
                commands::handle_provider_show(&agent_config.provider);
                continue;
            }
            s if s.starts_with("/provider ") => {
                let new_provider = s.trim_start_matches("/provider ").trim();
                if new_provider.is_empty() {
                    commands::handle_provider_show(&agent_config.provider);
                    continue;
                }
                commands::handle_provider_switch(new_provider, agent_config, agent);
                continue;
            }
            "/think" => {
                commands::handle_think_show(agent_config.thinking);
                continue;
            }
            s if s.starts_with("/think ") => {
                let level_str = s.trim_start_matches("/think ").trim();
                if level_str.is_empty() {
                    let current = thinking_level_name(agent_config.thinking);
                    println!("{DIM}  thinking: {current}");
                    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
                    continue;
                }
                let new_thinking = parse_thinking_level(level_str);
                if new_thinking == agent_config.thinking {
                    let current = thinking_level_name(agent_config.thinking);
                    println!("{DIM}  thinking already set to {current}{RESET}\n");
                    continue;
                }
                agent_config.thinking = new_thinking;
                // Rebuild agent with new thinking level, preserving conversation
                let saved = agent.save_messages().ok();
                *agent = agent_config.build_agent();
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                let level_name = thinking_level_name(agent_config.thinking);
                println!("{DIM}  (thinking set to {level_name}, conversation preserved){RESET}\n");
                continue;
            }
            s if s == "/save" || s.starts_with("/save ") => {
                commands::handle_save(agent, input);
                continue;
            }
            s if s == "/load" || s.starts_with("/load ") => {
                commands::handle_load(agent, input);
                continue;
            }
            "/diff" => {
                commands::handle_diff();
                continue;
            }
            "/undo" => {
                commands::handle_undo();
                continue;
            }
            "/health" => {
                commands::handle_health();
                continue;
            }
            "/test" => {
                commands::handle_test();
                continue;
            }
            "/lint" => {
                commands::handle_lint();
                continue;
            }
            "/fix" => {
                if let Some(fix_prompt) =
                    commands::handle_fix(agent, &mut session_total, &agent_config.model).await
                {
                    last_input = Some(fix_prompt);
                }
                continue;
            }
            "/history" => {
                commands::handle_history(agent);
                continue;
            }
            "/search" => {
                commands::handle_search(agent, input);
                continue;
            }
            s if s.starts_with("/search ") => {
                commands::handle_search(agent, input);
                continue;
            }
            "/marks" => {
                commands::handle_marks(&bookmarks);
                continue;
            }
            "/changes" => {
                commands::handle_changes(&session_changes);
                continue;
            }
            s if s == "/export" || s.starts_with("/export ") => {
                commands::handle_export(agent, input);
                continue;
            }
            s if s == "/mark" || s.starts_with("/mark ") => {
                commands::handle_mark(agent, input, &mut bookmarks);
                continue;
            }
            s if s == "/jump" || s.starts_with("/jump ") => {
                commands::handle_jump(agent, input, &bookmarks);
                continue;
            }
            "/config" => {
                commands::handle_config(
                    &agent_config.provider,
                    &agent_config.model,
                    &agent_config.base_url,
                    agent_config.thinking,
                    agent_config.max_tokens,
                    agent_config.max_turns,
                    agent_config.temperature,
                    &agent_config.skills,
                    &agent_config.system_prompt,
                    mcp_count,
                    openapi_count,
                    agent,
                    &cwd,
                );
                continue;
            }
            "/compact" => {
                commands::handle_compact(agent);
                continue;
            }
            s if s == "/commit" || s.starts_with("/commit ") => {
                commands::handle_commit(input);
                continue;
            }
            "/context" => {
                commands::handle_context();
                continue;
            }
            s if s == "/add" || s.starts_with("/add ") => {
                let results = commands::handle_add(input);
                if !results.is_empty() {
                    // Print summaries
                    for result in &results {
                        match result {
                            commands::AddResult::Text { summary, .. } => println!("{summary}"),
                            commands::AddResult::Image { summary, .. } => println!("{summary}"),
                        }
                    }
                    // Build content blocks with proper text context for images
                    let content_blocks = build_add_content_blocks(&results);
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
                    agent.append_message(msg);
                }
                continue;
            }
            "/docs" => {
                commands::handle_docs(input);
                continue;
            }
            s if s.starts_with("/docs ") => {
                commands::handle_docs(input);
                continue;
            }
            "/find" => {
                commands::handle_find(input);
                continue;
            }
            s if s.starts_with("/find ") => {
                commands::handle_find(input);
                continue;
            }
            "/grep" => {
                commands::handle_grep(input);
                continue;
            }
            s if s.starts_with("/grep ") => {
                commands::handle_grep(input);
                continue;
            }
            "/init" => {
                commands::handle_init();
                continue;
            }
            s if s == "/remember" || s.starts_with("/remember ") => {
                commands::handle_remember(input);
                continue;
            }
            "/memories" => {
                commands::handle_memories();
                continue;
            }
            s if s == "/forget" || s.starts_with("/forget ") => {
                commands::handle_forget(input);
                continue;
            }
            "/index" => {
                commands::handle_index();
                continue;
            }
            "/retry" => {
                last_error = commands::handle_retry(
                    agent,
                    &last_input,
                    &last_error,
                    &mut session_total,
                    &agent_config.model,
                )
                .await;
                continue;
            }
            s if s == "/tree" || s.starts_with("/tree ") => {
                commands::handle_tree(input);
                continue;
            }
            s if s == "/web" || s.starts_with("/web ") => {
                commands::handle_web(input);
                continue;
            }
            s if s.starts_with("/run ") || (s.starts_with('!') && s.len() > 1) => {
                commands::handle_run(input);
                continue;
            }
            "/run" => {
                commands::handle_run_usage();
                continue;
            }
            s if s == "/pr" || s.starts_with("/pr ") => {
                commands::handle_pr(input, agent, &mut session_total, &agent_config.model).await;
                continue;
            }
            s if s == "/git" || s.starts_with("/git ") => {
                commands::handle_git(input);
                continue;
            }
            s if s == "/spawn" || s.starts_with("/spawn ") => {
                if let Some(context_msg) = commands::handle_spawn(
                    input,
                    agent_config,
                    &mut session_total,
                    &agent_config.model,
                    agent.messages(),
                    &spawn_tracker,
                )
                .await
                {
                    last_input = Some(context_msg.clone());
                    let outcome = run_prompt_with_changes(
                        agent,
                        &context_msg,
                        &mut session_total,
                        &agent_config.model,
                        &session_changes,
                    )
                    .await;
                    last_error = outcome.last_tool_error;
                    auto_compact_if_needed(agent);
                }
                continue;
            }
            s if s == "/review" || s.starts_with("/review ") => {
                if let Some(review_prompt) =
                    commands::handle_review(input, agent, &mut session_total, &agent_config.model)
                        .await
                {
                    last_input = Some(review_prompt);
                }
                continue;
            }
            s if s == "/plan" || s.starts_with("/plan ") => {
                if let Some(plan_prompt) =
                    commands::handle_plan(input, agent, &mut session_total, &agent_config.model)
                        .await
                {
                    last_input = Some(plan_prompt);
                }
                continue;
            }
            s if s.starts_with('/') && is_unknown_command(s) => {
                let cmd = s.split_whitespace().next().unwrap_or(s);
                eprintln!("{RED}  unknown command: {cmd}{RESET}");
                eprintln!("{DIM}  type /help for available commands{RESET}\n");
                continue;
            }
            _ => {}
        }

        last_input = Some(input.to_string());

        // Expand @file mentions (e.g. "explain @src/main.rs") into file content
        let (cleaned_text, file_results) = commands::expand_file_mentions(input);

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
                text: cleaned_text.clone(),
            }];
            content_blocks.extend(build_add_content_blocks(&file_results));

            run_prompt_with_content(
                agent,
                content_blocks,
                &mut session_total,
                &agent_config.model,
            )
            .await
        } else {
            run_prompt_auto_retry(
                agent,
                input,
                &mut session_total,
                &agent_config.model,
                &session_changes,
            )
            .await
        };
        last_error = outcome.last_tool_error;

        // Auto-compact when context window is getting full
        auto_compact_if_needed(agent);
    }

    // Save readline history
    if let Some(history_path) = history_file_path() {
        let _ = rl.save_history(&history_path);
    }

    // Auto-save session on exit (always — crash recovery for everyone)
    commands::auto_save_on_exit(agent);

    println!("\n{DIM}  bye 👋{RESET}\n");
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(candidates.contains(&"/help".to_string()));
        assert!(candidates.contains(&"/quit".to_string()));

        // Typing "/he" should suggest "/help" and "/health"
        let (start, candidates) = helper.complete("/he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(candidates.contains(&"/help".to_string()));
        assert!(candidates.contains(&"/health".to_string()));
        assert!(!candidates.contains(&"/quit".to_string()));

        // Typing "/model " (with space) should return model completions
        let (start, candidates) = helper.complete("/model ", 7, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(
            !candidates.is_empty(),
            "Should offer model name completions after /model "
        );
        assert!(
            candidates.iter().any(|c| c.contains("claude")),
            "Should include Claude models"
        );

        // "/model cl" should filter to Claude models
        let (start, candidates) = helper.complete("/model cl", 9, &ctx).unwrap();
        assert_eq!(start, 7);
        for c in &candidates {
            assert!(
                c.starts_with("cl"),
                "All completions should start with 'cl': {c}"
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
        assert!(candidates.iter().any(|c| c == "Cargo.toml"));
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
        assert!(candidates.contains(&"src/main.rs".to_string()));
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
        assert!(candidates.contains(&"src/main.rs".to_string()));
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
        assert!(candidates.contains(&"src/".to_string()));
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
        assert!(candidates.contains(&"/help".to_string()));
        assert!(candidates.contains(&"/health".to_string()));
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
        assert!(candidates.contains(&"off".to_string()));
        assert!(candidates.contains(&"high".to_string()));

        // "/think m" should filter to medium/minimal
        let (start, candidates) = helper.complete("/think m", 8, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(candidates.contains(&"medium".to_string()));
        assert!(candidates.contains(&"minimal".to_string()));
        assert!(!candidates.contains(&"off".to_string()));
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
        assert!(candidates.contains(&"status".to_string()));
        assert!(candidates.contains(&"branch".to_string()));

        // "/git s" should filter to status and stash
        let (start, candidates) = helper.complete("/git s", 6, &ctx).unwrap();
        assert_eq!(start, 5);
        assert!(candidates.contains(&"status".to_string()));
        assert!(candidates.contains(&"stash".to_string()));
        assert!(!candidates.contains(&"log".to_string()));
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
        assert!(candidates.contains(&"create".to_string()));
        assert!(candidates.contains(&"checkout".to_string()));
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
        assert!(candidates.contains(&"anthropic".to_string()));
        assert!(candidates.contains(&"openai".to_string()));
        assert!(candidates.contains(&"google".to_string()));

        // "/provider o" should filter to providers starting with 'o'
        let (start, candidates) = helper.complete("/provider o", 11, &ctx).unwrap();
        assert_eq!(start, 10);
        assert!(candidates.contains(&"openai".to_string()));
        assert!(candidates.contains(&"openrouter".to_string()));
        assert!(candidates.contains(&"ollama".to_string()));
        assert!(!candidates.contains(&"anthropic".to_string()));
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
        assert!(candidates.iter().any(|c| c == "Cargo.toml"));
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
            candidates.contains(&"src/".to_string()),
            "Second arg should use file path completion: {candidates:?}"
        );
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
}
