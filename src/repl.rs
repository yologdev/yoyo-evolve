//! Interactive REPL loop and related helpers (tab-completion, multi-line input).

use crate::cli::*;
use crate::commands::{
    self, auto_compact_if_needed, is_unknown_command, thinking_level_name, KNOWN_COMMANDS,
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
            "/help" => {
                commands::handle_help();
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
                *agent = agent_config.build_agent();
                println!("{DIM}  (conversation cleared){RESET}\n");
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
                    continue_session,
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
            "/docs" => {
                commands::handle_docs(input);
                continue;
            }
            s if s.starts_with("/docs ") => {
                commands::handle_docs(input);
                continue;
            }
            "/init" => {
                commands::handle_init();
                continue;
            }
            "/retry" => {
                commands::handle_retry(agent, &last_input, &mut session_total, &agent_config.model)
                    .await;
                continue;
            }
            s if s == "/tree" || s.starts_with("/tree ") => {
                commands::handle_tree(input);
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
                commands::handle_pr(input);
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
                )
                .await
                {
                    last_input = Some(context_msg.clone());
                    run_prompt(agent, &context_msg, &mut session_total, &agent_config.model).await;
                    auto_compact_if_needed(agent);
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
        run_prompt(agent, input, &mut session_total, &agent_config.model).await;

        // Auto-compact when context window is getting full
        auto_compact_if_needed(agent);
    }

    // Save readline history
    if let Some(history_path) = history_file_path() {
        let _ = rl.save_history(&history_path);
    }

    // Auto-save session on exit when --continue was used
    if continue_session {
        if let Ok(json) = agent.save_messages() {
            if std::fs::write(DEFAULT_SESSION_PATH, &json).is_ok() {
                eprintln!(
                    "{DIM}  session saved to {DEFAULT_SESSION_PATH} ({} messages){RESET}",
                    agent.messages().len()
                );
            }
        }
    }

    println!("\n{DIM}  bye 👋{RESET}\n");
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

        // Typing "/model " (with space) should return no completions
        let (_, candidates) = helper.complete("/model claude", 13, &ctx).unwrap();
        assert!(candidates.is_empty());

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
}
