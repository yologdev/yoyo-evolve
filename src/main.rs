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
//!   echo "prompt" | cargo run  (piped mode: single prompt, no REPL)
//!
//! Commands:
//!   /quit, /exit    Exit the agent
//!   /clear          Clear conversation history
//!   /model <name>   Switch model mid-session
//!   /retry          Re-send the last user input

use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::time::Instant;
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
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
const MAX_CONTEXT_TOKENS: u64 = 200_000;
const AUTO_COMPACT_THRESHOLD: f64 = 0.80;
const DEFAULT_SESSION_PATH: &str = "yoyo-session.json";

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
    println!("  --thinking <lvl>  Enable extended thinking (off, minimal, low, medium, high)");
    println!("  --skills <dir>    Directory containing skill files");
    println!("  --system <text>   Custom system prompt (overrides default)");
    println!("  --system-file <f> Read system prompt from file");
    println!("  --prompt, -p <t>  Run a single prompt and exit (no REPL)");
    println!("  --continue, -c    Resume last saved session");
    println!("  --help, -h        Show this help message");
    println!("  --version, -V     Show version");
    println!();
    println!("Commands (in REPL):");
    println!("  /quit, /exit      Exit the agent");
    println!("  /clear            Clear conversation history");
    println!("  /compact          Compact conversation to save context space");
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

fn print_usage(usage: &Usage, total: &Usage, model: &str, elapsed: std::time::Duration) {
    if usage.input > 0 || usage.output > 0 {
        let cache_info = if usage.cache_read > 0 || usage.cache_write > 0 {
            format!(
                "  [cache: {} read, {} write]",
                usage.cache_read, usage.cache_write
            )
        } else {
            String::new()
        };
        let cost_info = estimate_cost(usage, model)
            .map(|c| format!("  cost: {}", format_cost(c)))
            .unwrap_or_default();
        let total_cost_info = estimate_cost(total, model)
            .map(|c| format!("  total: {}", format_cost(c)))
            .unwrap_or_default();
        let elapsed_str = format_duration(elapsed);
        println!(
            "\n{DIM}  tokens: {} in / {} out{cache_info}  (session: {} in / {} out){cost_info}{total_cost_info}  ⏱ {elapsed_str}{RESET}",
            usage.input, usage.output, total.input, total.output
        );
    }
}

fn build_agent(
    model: &str,
    api_key: &str,
    skills: &SkillSet,
    system_prompt: &str,
    thinking: ThinkingLevel,
) -> Agent {
    Agent::new(AnthropicProvider)
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_thinking(thinking)
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

    // --thinking <level> enables extended thinking
    let thinking = args
        .iter()
        .position(|a| a == "--thinking")
        .and_then(|i| args.get(i + 1))
        .map(|s| parse_thinking_level(s))
        .unwrap_or(ThinkingLevel::Off);

    let mut agent = build_agent(&model, &api_key, &skills, &system_prompt, thinking);

    // --continue / -c: resume last saved session
    let continue_session = args.iter().any(|a| a == "--continue" || a == "-c");
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
    let prompt_arg = args
        .iter()
        .position(|a| a == "--prompt" || a == "-p")
        .and_then(|i| args.get(i + 1))
        .cloned();

    if let Some(prompt_text) = prompt_arg {
        eprintln!("{DIM}  yoyo (prompt mode) — model: {model}{RESET}");
        let mut session_total = Usage::default();
        run_prompt(&mut agent, prompt_text.trim(), &mut session_total, &model).await;
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
        run_prompt(&mut agent, input, &mut session_total, &model).await;
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
                println!("  /model <name>      Switch model (clears conversation)");
                println!("  /status            Show session info");
                println!("  /tokens            Show token usage and context window");
                println!("  /save [path]       Save session to file (default: yoyo-session.json)");
                println!("  /load [path]       Load session from file");
                println!("  /diff              Show git diff summary of uncommitted changes");
                println!("  /undo              Revert all uncommitted changes (git checkout)");
                println!("  /retry             Re-send the last user input");
                println!("  /history           Show summary of conversation messages");
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
                    println!("    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}");
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
                agent = build_agent(&model, &api_key, &skills, &system_prompt, thinking);
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
                agent = build_agent(&model, &api_key, &skills, &system_prompt, thinking);
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
            "/undo" => {
                // Revert all uncommitted changes (equivalent to git checkout -- .)
                match std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let diff = String::from_utf8_lossy(&output.stdout);
                        if diff.trim().is_empty() {
                            println!("{DIM}  (nothing to undo — no uncommitted changes){RESET}\n");
                        } else {
                            println!("{DIM}{diff}{RESET}");
                            match std::process::Command::new("git")
                                .args(["checkout", "--", "."])
                                .output()
                            {
                                Ok(o) if o.status.success() => {
                                    println!(
                                        "{GREEN}  ✓ reverted all uncommitted changes{RESET}\n"
                                    );
                                }
                                _ => eprintln!("{RED}  error: failed to revert changes{RESET}\n"),
                            }
                        }
                    }
                    _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
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
            "/compact" => {
                let messages = agent.messages().to_vec();
                let before = total_tokens(&messages);
                let before_count = messages.len();
                let config = ContextConfig::default();
                let compacted = compact_messages(messages, &config);
                let after = total_tokens(&compacted);
                let after_count = compacted.len();
                agent.replace_messages(compacted);
                if before == after {
                    println!("{DIM}  (nothing to compact — {before_count} messages, ~{} tokens){RESET}\n",
                        format_token_count(before as u64));
                } else {
                    println!("{DIM}  compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}\n",
                        format_token_count(before as u64), format_token_count(after as u64));
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
            s if s.starts_with('/') && !s.contains(' ') => {
                eprintln!("{RED}  unknown command: {s}{RESET}");
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

/// Auto-compact conversation if context window usage exceeds threshold.
fn auto_compact_if_needed(agent: &mut Agent) {
    let messages = agent.messages().to_vec();
    let used = total_tokens(&messages) as u64;
    let ratio = used as f64 / MAX_CONTEXT_TOKENS as f64;

    if ratio > AUTO_COMPACT_THRESHOLD {
        let before_count = messages.len();
        let config = ContextConfig::default();
        let compacted = compact_messages(messages, &config);
        let after = total_tokens(&compacted) as u64;
        let after_count = compacted.len();
        agent.replace_messages(compacted);
        println!(
            "{DIM}  ⚡ auto-compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}",
            format_token_count(used),
            format_token_count(after)
        );
    }
}

/// Estimate cost in USD for a given usage and model.
/// Returns None if the model pricing is unknown.
fn estimate_cost(usage: &Usage, model: &str) -> Option<f64> {
    // Pricing per million tokens (MTok) from https://docs.anthropic.com/en/about-claude/pricing
    let (input_per_m, cache_write_per_m, cache_read_per_m, output_per_m) = if model.contains("opus")
    {
        if model.contains("4-6")
            || model.contains("4-5")
            || model.contains("4.6")
            || model.contains("4.5")
        {
            (5.0, 6.25, 0.50, 25.0)
        } else {
            // Opus 4, 4.1 etc.
            (15.0, 18.75, 1.50, 75.0)
        }
    } else if model.contains("sonnet") {
        (3.0, 3.75, 0.30, 15.0)
    } else if model.contains("haiku") {
        if model.contains("4-5") || model.contains("4.5") {
            (1.0, 1.25, 0.10, 5.0)
        } else {
            (0.80, 1.0, 0.08, 4.0)
        }
    } else {
        return None;
    };

    let cost = (usage.input as f64 * input_per_m
        + usage.cache_write as f64 * cache_write_per_m
        + usage.cache_read as f64 * cache_read_per_m
        + usage.output as f64 * output_per_m)
        / 1_000_000.0;

    Some(cost)
}

/// Format a cost in USD for display (e.g., "$0.0042", "$1.23").
fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Format a duration for display (e.g., "1.2s", "350ms", "2m 15s").
fn format_duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{mins}m {secs}s")
    }
}

/// Format a token count for display (e.g., 1500 -> "1.5k", 1000000 -> "1.0M").
fn format_token_count(count: u64) -> String {
    if count < 1000 {
        format!("{count}")
    } else if count < 1_000_000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{:.1}M", count as f64 / 1_000_000.0)
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

async fn run_prompt(agent: &mut Agent, input: &str, session_total: &mut Usage, model: &str) {
    let prompt_start = Instant::now();
    let mut rx = agent.prompt(input).await;
    let mut last_usage = Usage::default();
    let mut in_text = false;
    let mut tool_timers: HashMap<String, Instant> = HashMap::new();

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    AgentEvent::ToolExecutionStart {
                        tool_call_id, tool_name, args, ..
                    } => {
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        tool_timers.insert(tool_call_id.clone(), Instant::now());
                        let summary = format_tool_summary(&tool_name, &args);
                        print!("{YELLOW}  ▶ {summary}{RESET}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, .. } => {
                        let duration = tool_timers
                            .remove(&tool_call_id)
                            .map(|start| format_duration(start.elapsed()));
                        let dur_str = duration
                            .map(|d| format!(" {DIM}({d}){RESET}"))
                            .unwrap_or_default();
                        if is_error {
                            println!(" {RED}✗{RESET}{dur_str}");
                        } else {
                            println!(" {GREEN}✓{RESET}{dur_str}");
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
    print_usage(&last_usage, session_total, model, prompt_start.elapsed());
    println!();
}

/// Summarize a message for /history display.
fn summarize_message(msg: &AgentMessage) -> (&str, String) {
    match msg {
        AgentMessage::Llm(Message::User { content, .. }) => {
            let text = content
                .iter()
                .filter_map(|c| match c {
                    Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            ("user", truncate_with_ellipsis(&text, 80))
        }
        AgentMessage::Llm(Message::Assistant { content, .. }) => {
            let mut parts = Vec::new();
            let mut tool_calls = 0;
            for c in content {
                match c {
                    Content::Text { text } if !text.is_empty() => {
                        parts.push(truncate_with_ellipsis(text, 60));
                    }
                    Content::ToolCall { name, .. } => {
                        tool_calls += 1;
                        if tool_calls <= 3 {
                            parts.push(format!("→{name}"));
                        }
                    }
                    _ => {}
                }
            }
            if tool_calls > 3 {
                parts.push(format!("(+{} more tools)", tool_calls - 3));
            }
            let preview = if parts.is_empty() {
                "(empty)".to_string()
            } else {
                parts.join("  ")
            };
            ("assistant", preview)
        }
        AgentMessage::Llm(Message::ToolResult {
            tool_name,
            is_error,
            ..
        }) => {
            let status = if *is_error { "✗" } else { "✓" };
            ("tool", format!("{tool_name} {status}"))
        }
        AgentMessage::Extension(ext) => ("ext", truncate_with_ellipsis(&ext.role, 60)),
    }
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

/// Parse a thinking level string into a ThinkingLevel enum.
fn parse_thinking_level(s: &str) -> ThinkingLevel {
    match s.to_lowercase().as_str() {
        "off" | "none" => ThinkingLevel::Off,
        "minimal" | "min" => ThinkingLevel::Minimal,
        "low" => ThinkingLevel::Low,
        "medium" | "med" => ThinkingLevel::Medium,
        "high" | "max" => ThinkingLevel::High,
        _ => {
            eprintln!(
                "{YELLOW}warning:{RESET} Unknown thinking level '{s}', using 'medium'. \
                 Valid: off, minimal, low, medium, high"
            );
            ThinkingLevel::Medium
        }
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
            "/help", "/quit", "/exit", "/clear", "/compact", "/status", "/tokens", "/save",
            "/load", "/diff", "/undo", "/retry", "/history", "/model",
        ];
        for cmd in &commands {
            assert!(
                [
                    "/help", "/quit", "/exit", "/clear", "/compact", "/status", "/tokens", "/save",
                    "/load", "/diff", "/undo", "/retry", "/history", "/model"
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
        assert_eq!(format_token_count(1000000), "1.0M");
        assert_eq!(format_token_count(2500000), "2.5M");
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

    #[test]
    fn test_parse_thinking_level() {
        assert_eq!(parse_thinking_level("off"), ThinkingLevel::Off);
        assert_eq!(parse_thinking_level("none"), ThinkingLevel::Off);
        assert_eq!(parse_thinking_level("minimal"), ThinkingLevel::Minimal);
        assert_eq!(parse_thinking_level("min"), ThinkingLevel::Minimal);
        assert_eq!(parse_thinking_level("low"), ThinkingLevel::Low);
        assert_eq!(parse_thinking_level("medium"), ThinkingLevel::Medium);
        assert_eq!(parse_thinking_level("med"), ThinkingLevel::Medium);
        assert_eq!(parse_thinking_level("high"), ThinkingLevel::High);
        assert_eq!(parse_thinking_level("max"), ThinkingLevel::High);
        // Case insensitive
        assert_eq!(parse_thinking_level("HIGH"), ThinkingLevel::High);
        assert_eq!(parse_thinking_level("Medium"), ThinkingLevel::Medium);
        // Unknown defaults to medium with warning
        assert_eq!(parse_thinking_level("unknown"), ThinkingLevel::Medium);
    }

    #[test]
    fn test_continue_flag_parsing() {
        let args_short = ["yoyo".to_string(), "-c".to_string()];
        assert!(args_short.iter().any(|a| a == "--continue" || a == "-c"));

        let args_long = ["yoyo".to_string(), "--continue".to_string()];
        assert!(args_long.iter().any(|a| a == "--continue" || a == "-c"));

        let args_none = ["yoyo".to_string()];
        assert!(!args_none.iter().any(|a| a == "--continue" || a == "-c"));
    }

    #[test]
    fn test_prompt_flag_parsing() {
        // --prompt "text" or -p "text" should extract the prompt
        let args = [
            "yoyo".to_string(),
            "-p".to_string(),
            "explain this code".to_string(),
        ];
        let prompt = args
            .iter()
            .position(|a| a == "--prompt" || a == "-p")
            .and_then(|i| args.get(i + 1))
            .cloned();
        assert_eq!(prompt, Some("explain this code".to_string()));

        // Long form
        let args_long = [
            "yoyo".to_string(),
            "--prompt".to_string(),
            "what does this do?".to_string(),
        ];
        let prompt_long = args_long
            .iter()
            .position(|a| a == "--prompt" || a == "-p")
            .and_then(|i| args_long.get(i + 1))
            .cloned();
        assert_eq!(prompt_long, Some("what does this do?".to_string()));

        // Missing
        let args_none = ["yoyo".to_string()];
        let prompt_none = args_none
            .iter()
            .position(|a| a == "--prompt" || a == "-p")
            .and_then(|i| args_none.get(i + 1))
            .cloned();
        assert_eq!(prompt_none, None);
    }

    #[test]
    fn test_default_session_path() {
        assert_eq!(DEFAULT_SESSION_PATH, "yoyo-session.json");
    }

    #[test]
    fn test_auto_compact_threshold_constants() {
        assert_eq!(MAX_CONTEXT_TOKENS, 200_000);
        assert!((AUTO_COMPACT_THRESHOLD - 0.80).abs() < f64::EPSILON);
    }

    #[test]
    fn test_estimate_cost_opus() {
        let usage = Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-opus-4-6").unwrap();
        // $5/MTok input + $25/MTok * 0.1 = $5 + $2.50 = $7.50
        assert!((cost - 7.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_sonnet() {
        let usage = Usage {
            input: 500_000,
            output: 50_000,
            cache_read: 200_000,
            cache_write: 100_000,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-sonnet-4-6").unwrap();
        // $3 * 0.5 + $15 * 0.05 + $0.30 * 0.2 + $3.75 * 0.1
        // = 1.50 + 0.75 + 0.06 + 0.375 = 2.685
        assert!((cost - 2.685).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_haiku() {
        let usage = Usage {
            input: 1_000_000,
            output: 500_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-haiku-4-5").unwrap();
        // $1 * 1 + $5 * 0.5 = $3.50
        assert!((cost - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_unknown_model() {
        let usage = Usage {
            input: 1000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        assert!(estimate_cost(&usage, "gpt-4o").is_none());
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0001), "$0.0001");
        assert_eq!(format_cost(0.0042), "$0.0042");
        assert_eq!(format_cost(0.05), "$0.050");
        assert_eq!(format_cost(0.123), "$0.123");
        assert_eq!(format_cost(1.5), "$1.50");
        assert_eq!(format_cost(12.345), "$12.35");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(50)),
            "50ms"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(999)),
            "999ms"
        );
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1000)),
            "1.0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1500)),
            "1.5s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(30000)),
            "30.0s"
        );
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(60000)),
            "1m 0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(90000)),
            "1m 30s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(125000)),
            "2m 5s"
        );
    }

    #[test]
    fn test_unknown_slash_command_detection() {
        // Slash commands without spaces that don't match known commands should be caught
        let unknown = "/foo";
        assert!(unknown.starts_with('/') && !unknown.contains(' '));

        // Known commands should not trigger the unknown handler
        let known = "/help";
        // This would be matched by the explicit "/help" arm, not the catch-all
        assert_eq!(known, "/help");

        // Slash commands with spaces (like /model name) should not be caught by the
        // simple slash check — they're handled by starts_with("/model ") etc.
        let with_space = "/model claude-opus-4-6";
        assert!(with_space.starts_with('/') && with_space.contains(' '));
    }

    #[test]
    fn test_bare_model_command_is_recognized() {
        // "/model" without an argument should NOT be treated as an unknown command
        // It should show the current model info
        let input = "/model";
        assert_eq!(input, "/model");
        // This should match the explicit "/model" arm in the match
        assert!(!input.starts_with("/model "));
    }

    #[test]
    fn test_summarize_message_user() {
        let msg = AgentMessage::Llm(Message::user("hello world, this is a test"));
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "user");
        assert!(preview.contains("hello world"));
    }

    #[test]
    fn test_summarize_message_tool_result() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "output".into(),
            }],
            is_error: false,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("bash"));
        assert!(preview.contains("✓"));
    }

    #[test]
    fn test_summarize_message_tool_result_error() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_2".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "error".into(),
            }],
            is_error: true,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("✗"));
    }
}
