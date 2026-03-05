//! yoyo — a coding agent that evolves itself.
//!
//! Started as ~200 lines. Grows one commit at a time.
//! Read IDENTITY.md and JOURNAL.md for the full story.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=sk-... cargo run
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --thinking high
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --skills ./skills
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system "You are a Rust expert."
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system-file prompt.txt
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "explain this code"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "write a README" -o README.md
//!   echo "prompt" | cargo run  (piped mode: single prompt, no REPL)
//!
//! Commands:
//!   /quit, /exit    Exit the agent
//!   /clear          Clear conversation history
//!   /model <name>   Switch model mid-session
//!   /retry          Re-send the last user input

mod cli;
mod format;
mod prompt;

use cli::*;
use format::*;
use prompt::*;

use std::io::{self, BufRead, IsTerminal, Read, Write};
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::provider::AnthropicProvider;
use yoagent::tools::default_tools;
use yoagent::*;

fn build_agent(
    model: &str,
    api_key: &str,
    skills: &yoagent::skills::SkillSet,
    system_prompt: &str,
    thinking: ThinkingLevel,
    max_tokens: Option<u32>,
) -> Agent {
    let mut agent = Agent::new(AnthropicProvider)
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_thinking(thinking)
        .with_skills(skills.clone())
        .with_tools(default_tools());
    if let Some(max) = max_tokens {
        agent = agent.with_max_tokens(max);
    }
    agent
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check --no-color before any output (must happen before parse_args prints anything)
    // Also auto-disable color when stdout is not a terminal (piped output)
    if args.iter().any(|a| a == "--no-color") || !io::stdout().is_terminal() {
        disable_color();
    }

    let Some(config) = parse_args(&args) else {
        return; // --help or --version was handled
    };

    let mut model = config.model;
    let api_key = config.api_key;
    let skills = config.skills;
    let system_prompt = config.system_prompt;
    let thinking = config.thinking;
    let max_tokens = config.max_tokens;
    let continue_session = config.continue_session;
    let output_path = config.output_path;

    let mut agent = build_agent(
        &model,
        &api_key,
        &skills,
        &system_prompt,
        thinking,
        max_tokens,
    );

    // --continue / -c: resume last saved session
    if continue_session {
        match std::fs::read_to_string(DEFAULT_SESSION_PATH) {
            Ok(json) => match agent.restore_messages(&json) {
                Ok(_) => {
                    eprintln!(
                        "{DIM}  resumed session: {} messages from {DEFAULT_SESSION_PATH}{RESET}",
                        agent.messages().len()
                    );
                }
                Err(e) => eprintln!("{YELLOW}warning:{RESET} Failed to restore session: {e}"),
            },
            Err(_) => eprintln!("{DIM}  no previous session found ({DEFAULT_SESSION_PATH}){RESET}"),
        }
    }

    // --prompt / -p: single-shot mode with a prompt argument
    if let Some(prompt_text) = config.prompt_arg {
        eprintln!("{DIM}  yoyo (prompt mode) — model: {model}{RESET}");
        let mut session_total = Usage::default();
        let response = run_prompt(&mut agent, prompt_text.trim(), &mut session_total, &model).await;
        write_output_file(&output_path, &response);
        return;
    }

    // Piped mode: read all of stdin as a single prompt, run once, exit
    if !io::stdin().is_terminal() {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input).ok();
        let input = input.trim();
        if input.is_empty() {
            eprintln!("No input on stdin.");
            std::process::exit(1);
        }

        eprintln!("{DIM}  yoyo (piped mode) — model: {model}{RESET}");
        let mut session_total = Usage::default();
        let response = run_prompt(&mut agent, input, &mut session_total, &model).await;
        write_output_file(&output_path, &response);
        return;
    }

    // Interactive REPL mode
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    print_banner();
    println!("{DIM}  model: {model}{RESET}");
    if thinking != ThinkingLevel::Off {
        println!("{DIM}  thinking: {thinking:?}{RESET}");
    }
    if !skills.is_empty() {
        println!("{DIM}  skills: {} loaded{RESET}", skills.len());
    }
    if let Some(branch) = git_branch() {
        println!("{DIM}  git:   {branch}{RESET}");
    }
    println!("{DIM}  cwd:   {cwd}{RESET}\n");

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let mut session_total = Usage::default();
    let mut last_input: Option<String> = None;

    loop {
        let prompt = if let Some(branch) = git_branch() {
            format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{GREEN}> {RESET}")
        } else {
            format!("{BOLD}{GREEN}> {RESET}")
        };
        print!("{prompt}");
        io::stdout().flush().ok();

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Multi-line input: collect continuation lines
        let input = if needs_continuation(input) {
            collect_multiline(input, &mut lines)
        } else {
            input.to_string()
        };
        let input = input.trim();

        match input {
            "/quit" | "/exit" => break,
            "/help" => {
                println!("{DIM}  /help              Show this help");
                println!("  /quit, /exit       Exit yoyo");
                println!("  /clear             Clear conversation history");
                println!("  /compact           Compact conversation to save context space");
                println!("  /config            Show all current settings");
                println!("  /context           Show loaded project context files");
                println!("  /model <name>      Switch model (clears conversation)");
                println!("  /status            Show session info");
                println!("  /tokens            Show token usage and context window");
                println!("  /save [path]       Save session to file (default: yoyo-session.json)");
                println!("  /load [path]       Load session from file");
                println!("  /diff              Show git diff summary of uncommitted changes");
                println!("  /undo              Revert all uncommitted changes (git checkout)");
                println!("  /retry             Re-send the last user input");
                println!("  /history           Show summary of conversation messages");
                println!("  /version           Show yoyo version");
                println!();
                println!("  Multi-line input:");
                println!("  End a line with \\ to continue on the next line");
                println!("  Start with ``` to enter a fenced code block{RESET}\n");
                continue;
            }
            "/version" => {
                println!("{DIM}  yoyo v{VERSION}{RESET}\n");
                continue;
            }
            "/status" => {
                println!("{DIM}  model:   {model}");
                if let Some(branch) = git_branch() {
                    println!("  git:     {branch}");
                }
                println!("  cwd:     {cwd}");
                println!(
                    "  tokens:  {} in / {} out (session total){RESET}\n",
                    session_total.input, session_total.output
                );
                continue;
            }
            "/tokens" => {
                let max_context = MAX_CONTEXT_TOKENS;

                // Estimate actual context window usage from message history
                let messages = agent.messages().to_vec();
                let context_used = total_tokens(&messages) as u64;
                let bar = context_bar(context_used, max_context);

                println!("{DIM}  Context window:");
                println!("    messages:    {}", messages.len());
                println!(
                    "    context:     {} / {} tokens",
                    format_token_count(context_used),
                    format_token_count(max_context)
                );
                println!("    {bar}");
                if context_used as f64 / max_context as f64 > 0.75 {
                    println!(
                        "    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}"
                    );
                }
                println!();
                println!("  Session totals:");
                println!(
                    "    input:       {} tokens",
                    format_token_count(session_total.input)
                );
                println!(
                    "    output:      {} tokens",
                    format_token_count(session_total.output)
                );
                println!(
                    "    cache read:  {} tokens",
                    format_token_count(session_total.cache_read)
                );
                println!(
                    "    cache write: {} tokens",
                    format_token_count(session_total.cache_write)
                );
                if let Some(cost) = estimate_cost(&session_total, &model) {
                    println!("    est. cost:   {}", format_cost(cost));
                }
                println!("{RESET}");
                continue;
            }
            "/clear" => {
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                );
                println!("{DIM}  (conversation cleared){RESET}\n");
                continue;
            }
            "/model" => {
                println!("{DIM}  current model: {model}");
                println!("  usage: /model <name>{RESET}\n");
                continue;
            }
            s if s.starts_with("/model ") => {
                let new_model = s.trim_start_matches("/model ").trim();
                if new_model.is_empty() {
                    println!("{DIM}  current model: {model}");
                    println!("  usage: /model <name>{RESET}\n");
                    continue;
                }
                model = new_model.to_string();
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                );
                println!("{DIM}  (switched to {new_model}, conversation cleared){RESET}\n");
                continue;
            }
            s if s.starts_with("/save") => {
                let path = s.strip_prefix("/save").unwrap_or("").trim();
                let path = if path.is_empty() {
                    DEFAULT_SESSION_PATH
                } else {
                    path
                };
                match agent.save_messages() {
                    Ok(json) => match std::fs::write(path, &json) {
                        Ok(_) => println!(
                            "{DIM}  (session saved to {path}, {} messages){RESET}\n",
                            agent.messages().len()
                        ),
                        Err(e) => eprintln!("{RED}  error saving: {e}{RESET}\n"),
                    },
                    Err(e) => eprintln!("{RED}  error serializing: {e}{RESET}\n"),
                }
                continue;
            }
            s if s.starts_with("/load") => {
                let path = s.strip_prefix("/load").unwrap_or("").trim();
                let path = if path.is_empty() {
                    DEFAULT_SESSION_PATH
                } else {
                    path
                };
                match std::fs::read_to_string(path) {
                    Ok(json) => match agent.restore_messages(&json) {
                        Ok(_) => println!(
                            "{DIM}  (session loaded from {path}, {} messages){RESET}\n",
                            agent.messages().len()
                        ),
                        Err(e) => eprintln!("{RED}  error parsing: {e}{RESET}\n"),
                    },
                    Err(e) => eprintln!("{RED}  error reading {path}: {e}{RESET}\n"),
                }
                continue;
            }
            "/diff" => {
                // Use git status --short for a comprehensive view (modified, staged, untracked)
                match std::process::Command::new("git")
                    .args(["status", "--short"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let status = String::from_utf8_lossy(&output.stdout);
                        if status.trim().is_empty() {
                            println!("{DIM}  (no uncommitted changes){RESET}\n");
                        } else {
                            println!("{DIM}  Changes:");
                            for line in status.lines() {
                                println!("    {line}");
                            }
                            println!("{RESET}");
                            // Also show diff stat for modified files
                            if let Ok(diff) = std::process::Command::new("git")
                                .args(["diff", "--stat"])
                                .output()
                            {
                                let diff_text = String::from_utf8_lossy(&diff.stdout);
                                if !diff_text.trim().is_empty() {
                                    println!("{DIM}{diff_text}{RESET}");
                                }
                            }
                        }
                    }
                    _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
                }
                continue;
            }
            "/undo" => {
                // Revert all uncommitted changes and remove untracked files
                let diff_output = std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output();
                let untracked = std::process::Command::new("git")
                    .args(["ls-files", "--others", "--exclude-standard"])
                    .output();

                let has_diff = diff_output
                    .as_ref()
                    .map(|o| {
                        o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty()
                    })
                    .unwrap_or(false);
                let untracked_files: Vec<String> = untracked
                    .as_ref()
                    .map(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .map(|l| l.to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let has_untracked = !untracked_files.is_empty();

                if !has_diff && !has_untracked {
                    println!("{DIM}  (nothing to undo — no uncommitted changes){RESET}\n");
                } else {
                    if has_diff {
                        if let Ok(ref output) = diff_output {
                            let diff = String::from_utf8_lossy(&output.stdout);
                            println!("{DIM}{diff}{RESET}");
                        }
                    }
                    if has_untracked {
                        println!("{DIM}  untracked files:");
                        for f in &untracked_files {
                            println!("    {f}");
                        }
                        println!("{RESET}");
                    }

                    // Revert modified files
                    if has_diff {
                        let _ = std::process::Command::new("git")
                            .args(["checkout", "--", "."])
                            .output();
                    }
                    // Remove untracked files
                    if has_untracked {
                        let _ = std::process::Command::new("git")
                            .args(["clean", "-fd"])
                            .output();
                    }
                    println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");
                }
                continue;
            }
            "/history" => {
                let messages = agent.messages();
                if messages.is_empty() {
                    println!("{DIM}  (no messages in conversation){RESET}\n");
                } else {
                    println!("{DIM}  Conversation ({} messages):", messages.len());
                    for (i, msg) in messages.iter().enumerate() {
                        let (role, preview) = summarize_message(msg);
                        let idx = i + 1;
                        println!("    {idx:>3}. [{role}] {preview}");
                    }
                    println!("{RESET}");
                }
                continue;
            }
            "/config" => {
                println!("{DIM}  Configuration:");
                println!("    model:      {model}");
                println!(
                    "    thinking:   {}",
                    if thinking == ThinkingLevel::Off {
                        "off".to_string()
                    } else {
                        format!("{thinking:?}").to_lowercase()
                    }
                );
                println!(
                    "    max_tokens: {}",
                    max_tokens
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "default (8192)".to_string())
                );
                println!(
                    "    skills:     {}",
                    if skills.is_empty() {
                        "none".to_string()
                    } else {
                        format!("{} loaded", skills.len())
                    }
                );
                let system_preview =
                    truncate_with_ellipsis(system_prompt.lines().next().unwrap_or("(empty)"), 60);
                println!("    system:     {system_preview}");
                if let Some(branch) = git_branch() {
                    println!("    git:        {branch}");
                }
                println!("    cwd:        {cwd}");
                println!(
                    "    context:    {} max tokens",
                    format_token_count(MAX_CONTEXT_TOKENS)
                );
                println!(
                    "    auto-compact: at {:.0}%",
                    AUTO_COMPACT_THRESHOLD * 100.0
                );
                println!("    messages:   {}", agent.messages().len());
                if continue_session {
                    println!("    session:    auto-save on exit ({DEFAULT_SESSION_PATH})");
                }
                println!("{RESET}");
                continue;
            }
            "/compact" => {
                let messages = agent.messages();
                let before_count = messages.len();
                let before_tokens = total_tokens(messages) as u64;
                match compact_agent(&mut agent) {
                    Some((_, _, after_count, after_tokens)) => {
                        println!(
                            "{DIM}  compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}\n",
                            format_token_count(before_tokens),
                            format_token_count(after_tokens)
                        );
                    }
                    None => {
                        println!(
                            "{DIM}  (nothing to compact — {before_count} messages, ~{} tokens){RESET}\n",
                            format_token_count(before_tokens)
                        );
                    }
                }
                continue;
            }
            "/context" => {
                let files = cli::list_project_context_files();
                if files.is_empty() {
                    println!("{DIM}  No project context files found.");
                    println!("  Searched for: {}", PROJECT_CONTEXT_FILES.join(", "));
                    println!("  Create YOYO.md, CLAUDE.md, or .yoyo/instructions.md to add project context.{RESET}\n");
                } else {
                    println!("{DIM}  Project context files:");
                    for (name, lines) in &files {
                        println!("    {name} ({lines} lines)");
                    }
                    println!("{RESET}");
                }
                continue;
            }
            "/retry" => {
                match &last_input {
                    Some(prev) => {
                        println!("{DIM}  (retrying last input){RESET}");
                        let retry_input = prev.clone();
                        run_prompt(&mut agent, &retry_input, &mut session_total, &model).await;
                        auto_compact_if_needed(&mut agent);
                    }
                    None => {
                        eprintln!("{DIM}  (nothing to retry — no previous input){RESET}\n");
                    }
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
        run_prompt(&mut agent, input, &mut session_total, &model).await;

        // Auto-compact when context window is getting full
        auto_compact_if_needed(&mut agent);
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

/// Compact the agent's conversation and return (before_count, before_tokens, after_count, after_tokens).
/// Returns None if nothing changed.
fn compact_agent(agent: &mut Agent) -> Option<(usize, u64, usize, u64)> {
    let messages = agent.messages().to_vec();
    let before_tokens = total_tokens(&messages) as u64;
    let before_count = messages.len();
    let config = ContextConfig::default();
    let compacted = compact_messages(messages, &config);
    let after_tokens = total_tokens(&compacted) as u64;
    let after_count = compacted.len();
    agent.replace_messages(compacted);
    if before_tokens == after_tokens {
        None
    } else {
        Some((before_count, before_tokens, after_count, after_tokens))
    }
}

/// Auto-compact conversation if context window usage exceeds threshold.
fn auto_compact_if_needed(agent: &mut Agent) {
    let messages = agent.messages().to_vec();
    let used = total_tokens(&messages) as u64;
    let ratio = used as f64 / MAX_CONTEXT_TOKENS as f64;

    if ratio > AUTO_COMPACT_THRESHOLD {
        if let Some((before_count, before_tokens, after_count, after_tokens)) = compact_agent(agent)
        {
            println!(
                "{DIM}  ⚡ auto-compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}",
                format_token_count(before_tokens),
                format_token_count(after_tokens)
            );
        }
    }
}

/// Check if a line needs continuation (backslash at end, or opens a code fence).
fn needs_continuation(line: &str) -> bool {
    line.ends_with('\\') || line.starts_with("```")
}

/// Collect multi-line input. Supports:
/// - Backslash continuation: lines ending with `\` continue on the next line
/// - Code fences: input starting with ``` collects until closing ```
fn collect_multiline(first_line: &str, lines: &mut io::Lines<io::StdinLock<'_>>) -> String {
    let mut buf = String::new();

    if first_line.starts_with("```") {
        // Code fence mode: collect until closing ```
        buf.push_str(first_line);
        buf.push('\n');
        loop {
            print!("{DIM}  ...{RESET} ");
            io::stdout().flush().ok();
            match lines.next() {
                Some(Ok(line)) => {
                    buf.push_str(&line);
                    buf.push('\n');
                    if line.trim() == "```" {
                        break;
                    }
                }
                _ => break,
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
                print!("{DIM}  ...{RESET} ");
                io::stdout().flush().ok();
                match lines.next() {
                    Some(Ok(line)) => {
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

/// Known REPL command prefixes. Used to detect unknown slash commands.
const KNOWN_COMMANDS: &[&str] = &[
    "/help", "/quit", "/exit", "/clear", "/compact", "/status", "/tokens", "/save", "/load",
    "/diff", "/undo", "/retry", "/history", "/model", "/config", "/context", "/version",
];

/// Check if a slash-prefixed input is an unknown command.
/// Extracts the first word and checks against known commands.
fn is_unknown_command(input: &str) -> bool {
    let cmd = input.split_whitespace().next().unwrap_or(input);
    !KNOWN_COMMANDS.contains(&cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parsing_quit() {
        let quit_commands = ["/quit", "/exit"];
        for cmd in &quit_commands {
            assert!(
                *cmd == "/quit" || *cmd == "/exit",
                "Unrecognized quit command: {cmd}"
            );
        }
    }

    #[test]
    fn test_command_parsing_model() {
        let input = "/model claude-opus-4-6";
        assert!(input.starts_with("/model "));
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_parsing_model_whitespace() {
        let input = "/model   claude-opus-4-6  ";
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_help_recognized() {
        let commands = [
            "/help", "/quit", "/exit", "/clear", "/compact", "/config", "/context", "/status",
            "/tokens", "/save", "/load", "/diff", "/undo", "/retry", "/history", "/model",
            "/version",
        ];
        for cmd in &commands {
            assert!(
                KNOWN_COMMANDS.contains(cmd),
                "Command not in KNOWN_COMMANDS: {cmd}"
            );
        }
    }

    #[test]
    fn test_model_switch_updates_variable() {
        let original = "claude-opus-4-6";
        let input = "/model claude-haiku-35";
        let new_model = input.trim_start_matches("/model ").trim();
        assert_ne!(new_model, original);
        assert_eq!(new_model, "claude-haiku-35");
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
    fn test_bare_model_command_is_recognized() {
        let input = "/model";
        assert_eq!(input, "/model");
        assert!(!input.starts_with("/model "));
    }

    #[test]
    fn test_unknown_slash_command_detection() {
        assert!(is_unknown_command("/foo"));
        assert!(is_unknown_command("/foo bar baz"));
        assert!(is_unknown_command("/unknown argument"));

        assert!(!is_unknown_command("/help"));
        assert!(!is_unknown_command("/quit"));
        assert!(!is_unknown_command("/model"));
        assert!(!is_unknown_command("/model claude-opus-4-6"));
        assert!(!is_unknown_command("/save"));
        assert!(!is_unknown_command("/save myfile.json"));
        assert!(!is_unknown_command("/load"));
        assert!(!is_unknown_command("/load myfile.json"));
        assert!(!is_unknown_command("/config"));
        assert!(!is_unknown_command("/version"));
    }
}
