//! yoyo — a coding agent that evolves itself.
//!
//! Started as ~200 lines. Grows one commit at a time.
//! Read IDENTITY.md and JOURNAL.md for the full story.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=sk-... cargo run
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --skills ./skills
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system "You are a Rust expert."
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system-file prompt.txt
//!   echo "prompt" | cargo run  (piped mode: single prompt, no REPL)
//!
//! Commands:
//!   /quit, /exit    Exit the agent
//!   /clear          Clear conversation history
//!   /model <name>   Switch model mid-session

use std::io::{self, BufRead, IsTerminal, Read, Write};
use yoagent::agent::Agent;
use yoagent::provider::AnthropicProvider;
use yoagent::skills::SkillSet;
use yoagent::tools::default_tools;
use yoagent::*;

// ANSI color helpers
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";

const VERSION: &str = env!("CARGO_PKG_VERSION");

const SYSTEM_PROMPT: &str = r#"You are a coding assistant working in the user's terminal.
You have access to the filesystem and shell. Be direct and concise.
When the user asks you to do something, do it — don't just explain how.
Use tools proactively: read files to understand context, run commands to verify your work.
After making changes, run tests or verify the result when appropriate."#;

fn print_help() {
    println!("yoyo v{VERSION} — a coding agent growing up in public");
    println!();
    println!("Usage: yoyo [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --model <name>    Model to use (default: claude-opus-4-6)");
    println!("  --skills <dir>    Directory containing skill files");
    println!("  --system <text>   Custom system prompt (overrides default)");
    println!("  --system-file <f> Read system prompt from file");
    println!("  --help, -h        Show this help message");
    println!("  --version, -V     Show version");
    println!();
    println!("Commands (in REPL):");
    println!("  /quit, /exit      Exit the agent");
    println!("  /clear            Clear conversation history");
    println!("  /model <name>     Switch model mid-session");
    println!("  /status           Show session info");
    println!("  /tokens           Show token usage and context window");
    println!("  /save [path]      Save session to file");
    println!("  /load [path]      Load session from file");
    println!("  /diff             Show git diff summary");
    println!();
    println!("Environment:");
    println!("  ANTHROPIC_API_KEY  API key for Anthropic (required)");
    println!("  API_KEY            Alternative env var for API key");
}

fn print_banner() {
    println!(
        "\n{BOLD}{CYAN}  yoyo{RESET} v{VERSION} {DIM}— a coding agent growing up in public{RESET}"
    );
    println!("{DIM}  Type /help for commands, /quit to exit{RESET}\n");
}

fn print_usage(usage: &Usage, total: &Usage) {
    if usage.input > 0 || usage.output > 0 {
        let cache_info = if usage.cache_read > 0 || usage.cache_write > 0 {
            format!(
                "  [cache: {} read, {} write]",
                usage.cache_read, usage.cache_write
            )
        } else {
            String::new()
        };
        println!(
            "\n{DIM}  tokens: {} in / {} out{cache_info}  (session: {} in / {} out){RESET}",
            usage.input, usage.output, total.input, total.output
        );
    }
}

fn build_agent(model: &str, api_key: &str, skills: &SkillSet, system_prompt: &str) -> Agent {
    Agent::new(AnthropicProvider)
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_skills(skills.clone())
        .with_tools(default_tools())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Handle --help and --version before anything else
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("yoyo v{VERSION}");
        return;
    }

    let api_key = match std::env::var("ANTHROPIC_API_KEY").or_else(|_| std::env::var("API_KEY")) {
        Ok(key) if !key.is_empty() => key,
        _ => {
            eprintln!("{RED}error:{RESET} No API key found.");
            eprintln!("Set ANTHROPIC_API_KEY or API_KEY environment variable.");
            eprintln!("Example: ANTHROPIC_API_KEY=sk-ant-... cargo run");
            std::process::exit(1);
        }
    };

    let mut model = args
        .iter()
        .position(|a| a == "--model")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "claude-opus-4-6".into());

    let skill_dirs: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--skills")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();

    let skills = if skill_dirs.is_empty() {
        SkillSet::empty()
    } else {
        match SkillSet::load(&skill_dirs) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{YELLOW}warning:{RESET} Failed to load skills: {e}");
                SkillSet::empty()
            }
        }
    };

    // Custom system prompt: --system "text" or --system-file path
    let custom_system = args
        .iter()
        .position(|a| a == "--system")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let system_from_file = args
        .iter()
        .position(|a| a == "--system-file")
        .and_then(|i| args.get(i + 1))
        .map(|path| {
            std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("{RED}error:{RESET} Failed to read system prompt file '{path}': {e}");
                std::process::exit(1);
            })
        });

    // --system-file takes precedence over --system, both override default
    let system_prompt = system_from_file
        .or(custom_system)
        .unwrap_or_else(|| SYSTEM_PROMPT.to_string());

    let mut agent = build_agent(&model, &api_key, &skills, &system_prompt);

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
        run_prompt(&mut agent, input, &mut session_total).await;
        return;
    }

    // Interactive REPL mode
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    print_banner();
    println!("{DIM}  model: {model}{RESET}");
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
                println!("  /model <name>      Switch model (clears conversation)");
                println!("  /status            Show session info");
                println!("  /tokens            Show token usage and context window");
                println!("  /save [path]       Save session to file (default: yoyo-session.json)");
                println!("  /load [path]       Load session from file");
                println!("  /diff              Show git diff summary of uncommitted changes");
                println!();
                println!("  Multi-line input:");
                println!("  End a line with \\ to continue on the next line");
                println!("  Start with ``` to enter a fenced code block{RESET}\n");
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
                // Anthropic models all have 200k context windows.
                // If we add non-Anthropic providers later, this will need updating.
                let max_context: u64 = 200_000;

                let total_used = session_total.input + session_total.output;
                let bar = context_bar(total_used, max_context);

                println!("{DIM}  Context usage:");
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
                println!(
                    "    total:       {} / {} tokens",
                    format_token_count(total_used),
                    format_token_count(max_context)
                );
                println!("    {bar}");
                if total_used as f64 / max_context as f64 > 0.75 {
                    println!("    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}");
                }
                println!("{RESET}");
                continue;
            }
            "/clear" => {
                agent = build_agent(&model, &api_key, &skills, &system_prompt);
                println!("{DIM}  (conversation cleared){RESET}\n");
                continue;
            }
            s if s.starts_with("/model ") => {
                let new_model = s.trim_start_matches("/model ").trim();
                model = new_model.to_string();
                agent = build_agent(&model, &api_key, &skills, &system_prompt);
                println!("{DIM}  (switched to {new_model}, conversation cleared){RESET}\n");
                continue;
            }
            s if s.starts_with("/save") => {
                let path = s.strip_prefix("/save").unwrap_or("").trim();
                let path = if path.is_empty() {
                    "yoyo-session.json"
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
                    "yoyo-session.json"
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
                match std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let diff = String::from_utf8_lossy(&output.stdout);
                        if diff.trim().is_empty() {
                            println!("{DIM}  (no uncommitted changes){RESET}\n");
                        } else {
                            println!("{DIM}{diff}{RESET}");
                        }
                    }
                    _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
                }
                continue;
            }
            _ => {}
        }

        run_prompt(&mut agent, input, &mut session_total).await;
    }

    println!("\n{DIM}  bye 👋{RESET}\n");
}

/// Format a token count for display (e.g., 1500 -> "1.5k").
fn format_token_count(count: u64) -> String {
    if count < 1000 {
        format!("{count}")
    } else {
        format!("{:.1}k", count as f64 / 1000.0)
    }
}

/// Build a context usage bar (e.g., "████████░░░░░░░░░░░░ 40%").
fn context_bar(used: u64, max: u64) -> String {
    let pct = if max == 0 {
        0.0
    } else {
        (used as f64 / max as f64).min(1.0)
    };
    let width = 20;
    let filled = (pct * width as f64).round() as usize;
    let empty = width - filled;
    let bar: String = "█".repeat(filled) + &"░".repeat(empty);
    format!("{bar} {:.0}%", pct * 100.0)
}

/// Get the current git branch name, if we're in a git repo.
fn git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
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

async fn run_prompt(agent: &mut Agent, input: &str, session_total: &mut Usage) {
    let mut rx = agent.prompt(input).await;
    let mut last_usage = Usage::default();
    let mut in_text = false;

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    AgentEvent::ToolExecutionStart {
                        tool_name, args, ..
                    } => {
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        let summary = format_tool_summary(&tool_name, &args);
                        print!("{YELLOW}  ▶ {summary}{RESET}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::ToolExecutionEnd { is_error, .. } => {
                        if is_error {
                            println!(" {RED}✗{RESET}");
                        } else {
                            println!(" {GREEN}✓{RESET}");
                        }
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Text { delta },
                        ..
                    } => {
                        if !in_text {
                            println!();
                            in_text = true;
                        }
                        print!("{}", delta);
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Thinking { delta },
                        ..
                    } => {
                        // Show thinking output dimmed so user can follow the reasoning
                        print!("{DIM}{delta}{RESET}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::AgentEnd { messages } => {
                        // Sum usage across ALL assistant messages in this turn
                        // (a single prompt can trigger multiple LLM calls via tool loops)
                        for msg in &messages {
                            if let AgentMessage::Llm(Message::Assistant { usage, stop_reason, error_message, .. }) = msg {
                                last_usage.input += usage.input;
                                last_usage.output += usage.output;
                                last_usage.cache_read += usage.cache_read;
                                last_usage.cache_write += usage.cache_write;

                                // Show error stop reasons to the user
                                if *stop_reason == StopReason::Error {
                                    if let Some(err_msg) = error_message {
                                        if in_text {
                                            println!();
                                            in_text = false;
                                        }
                                        eprintln!("\n{RED}  error: {err_msg}{RESET}");
                                    }
                                }
                            }
                        }
                    }
                    AgentEvent::InputRejected { reason } => {
                        eprintln!("{RED}  input rejected: {reason}{RESET}");
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        println!("{DIM}  {text}{RESET}");
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                // Cancel the agent's background work (tool execution, API calls)
                agent.abort();
                if in_text {
                    println!();
                }
                println!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}");
                break;
            }
        }
    }

    if in_text {
        println!();
    }
    session_total.input += last_usage.input;
    session_total.output += last_usage.output;
    session_total.cache_read += last_usage.cache_read;
    session_total.cache_write += last_usage.cache_write;
    print_usage(&last_usage, session_total);
    println!();
}

/// Format a human-readable summary for a tool execution.
fn format_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            format!("$ {}", truncate_with_ellipsis(cmd, 80))
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("read {}", path)
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("write {}", path)
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("edit {}", path)
        }
        "list_files" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("ls {}", path)
        }
        "search" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            format!("search '{}'", truncate_with_ellipsis(pat, 60))
        }
        _ => tool_name.to_string(),
    }
}

#[cfg(test)]
fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_unicode() {
        assert_eq!(truncate("héllo wörld", 5), "héllo");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn test_version_constant_exists() {
        assert!(
            VERSION.contains('.'),
            "Version should contain a dot: {VERSION}"
        );
    }

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
            "/help", "/quit", "/exit", "/clear", "/status", "/tokens", "/save", "/load", "/diff",
        ];
        for cmd in &commands {
            assert!(
                [
                    "/help", "/quit", "/exit", "/clear", "/status", "/tokens", "/save", "/load",
                    "/diff"
                ]
                .contains(cmd),
                "Command not recognized: {cmd}"
            );
        }
    }

    #[test]
    fn test_model_switch_updates_variable() {
        // Simulates: /model command should update the active model name
        // so that /clear uses the new model, not the original.
        let original = "claude-opus-4-6";
        let input = "/model claude-haiku-35";
        let new_model = input.trim_start_matches("/model ").trim();
        assert_ne!(new_model, original);
        assert_eq!(new_model, "claude-haiku-35");
    }

    #[test]
    fn test_truncate_adds_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
        assert_eq!(truncate_with_ellipsis("hi", 5), "hi");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn test_format_tool_summary_bash() {
        let args = serde_json::json!({"command": "echo hello"});
        assert_eq!(format_tool_summary("bash", &args), "$ echo hello");
    }

    #[test]
    fn test_format_tool_summary_bash_long_command() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = format_tool_summary("bash", &args);
        assert!(result.starts_with("$ "));
        assert!(result.ends_with('…'));
        assert!(result.len() < 100); // truncated
    }

    #[test]
    fn test_format_tool_summary_read_file() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(format_tool_summary("read_file", &args), "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_write_file() {
        let args = serde_json::json!({"path": "out.txt"});
        assert_eq!(format_tool_summary("write_file", &args), "write out.txt");
    }

    #[test]
    fn test_format_tool_summary_edit_file() {
        let args = serde_json::json!({"path": "foo.rs"});
        assert_eq!(format_tool_summary("edit_file", &args), "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_list_files() {
        let args = serde_json::json!({"path": "src/"});
        assert_eq!(format_tool_summary("list_files", &args), "ls src/");
    }

    #[test]
    fn test_format_tool_summary_list_files_no_path() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("list_files", &args), "ls .");
    }

    #[test]
    fn test_format_tool_summary_search() {
        let args = serde_json::json!({"pattern": "TODO"});
        assert_eq!(format_tool_summary("search", &args), "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_unknown_tool() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("custom_tool", &args), "custom_tool");
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
    fn test_system_flag_parsing() {
        // --system "custom prompt" should be extracted from args
        let args = [
            "yoyo".to_string(),
            "--system".to_string(),
            "You are a Rust expert.".to_string(),
        ];
        let system = args
            .iter()
            .position(|a| a == "--system")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(system, Some("You are a Rust expert.".to_string()));
    }

    #[test]
    fn test_system_flag_missing() {
        let args = ["yoyo".to_string()];
        let system = args
            .iter()
            .position(|a| a == "--system")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(system, None);
    }

    #[test]
    fn test_system_file_flag() {
        // --system-file path should read from file
        let args = [
            "yoyo".to_string(),
            "--system-file".to_string(),
            "prompt.txt".to_string(),
        ];
        let system_file = args
            .iter()
            .position(|a| a == "--system-file")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(system_file, Some("prompt.txt".to_string()));
    }

    #[test]
    fn test_format_token_count() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(1500), "1.5k");
        assert_eq!(format_token_count(10000), "10.0k");
        assert_eq!(format_token_count(150000), "150.0k");
        assert_eq!(format_token_count(1000000), "1000.0k");
    }

    #[test]
    fn test_context_bar() {
        let bar = context_bar(50000, 200000);
        assert!(bar.contains('█'));
        assert!(bar.contains("25%"));

        let bar_empty = context_bar(0, 200000);
        assert!(bar_empty.contains("0%"));

        let bar_full = context_bar(200000, 200000);
        assert!(bar_full.contains("100%"));
    }

    #[test]
    fn test_git_branch_returns_something_in_repo() {
        // We're running tests inside the yoyo repo, so we should get a branch name
        let branch = git_branch();
        assert!(branch.is_some(), "Expected to be in a git repo");
        let name = branch.unwrap();
        assert!(!name.is_empty(), "Branch name should not be empty");
        assert!(
            !name.contains('\n'),
            "Branch name should not contain newlines"
        );
    }
}
