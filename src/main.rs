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
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --mcp "npx -y @modelcontextprotocol/server-filesystem /tmp"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system "You are a Rust expert."
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system-file prompt.txt
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "explain this code"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "write a README" -o README.md
//!   echo "prompt" | cargo run  (piped mode: single prompt, no REPL)
//!
//! Commands:
//!   /quit, /exit    Exit the agent
//!   /clear          Clear conversation history
//!   /commit [msg]   Commit staged changes (AI-generates message if no msg)
//!   /git <subcmd>   Quick git: status, log, add, stash, stash pop
//!   /model <name>   Switch model mid-session
//!   /search <query> Search conversation history
//!   /tree [depth]   Show project directory tree
//!   /pr [number]    List open PRs, view/diff/comment/checkout a PR
//!   /retry          Re-send the last user input

mod cli;
mod format;
mod prompt;

use cli::*;
use format::*;
use prompt::*;

use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::Editor;
use std::io::{self, IsTerminal, Read, Write};
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig, ExecutionLimits};
use yoagent::provider::AnthropicProvider;
use yoagent::tools::bash::BashTool;
use yoagent::tools::edit::EditFileTool;
use yoagent::tools::file::{ReadFileTool, WriteFileTool};
use yoagent::tools::list::ListFilesTool;
use yoagent::tools::search::SearchTool;
use yoagent::types::AgentTool;
use yoagent::*;

/// Rustyline helper that provides tab-completion for `/` slash commands.
struct YoyoHelper;

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
fn complete_file_path(partial: &str) -> Vec<String> {
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

/// Build the tool set, optionally with a bash confirmation prompt.
/// When `auto_approve` is false (default), bash commands require user approval.
fn build_tools(auto_approve: bool) -> Vec<Box<dyn AgentTool>> {
    let bash = if auto_approve {
        BashTool::default()
    } else {
        BashTool::default().with_confirm(|cmd: &str| {
            use std::io::BufRead;
            // Show the command and ask for approval
            eprint!(
                "{YELLOW}  ⚠ Allow: {RESET}{}{YELLOW} ? {RESET}({GREEN}y{RESET}/{RED}n{RESET}/{GREEN}a{RESET}lways) ",
                truncate_with_ellipsis(cmd, 120)
            );
            io::stderr().flush().ok();
            let mut response = String::new();
            let stdin = io::stdin();
            if stdin.lock().read_line(&mut response).is_err() {
                return false;
            }
            let response = response.trim().to_lowercase();
            matches!(response.as_str(), "y" | "yes" | "a" | "always")
        })
    };
    vec![
        Box::new(bash),
        Box::new(ReadFileTool::default()),
        Box::new(WriteFileTool::new()),
        Box::new(EditFileTool::new()),
        Box::new(ListFilesTool::default()),
        Box::new(SearchTool::default()),
    ]
}

#[allow(clippy::too_many_arguments)]
fn build_agent(
    model: &str,
    api_key: &str,
    skills: &yoagent::skills::SkillSet,
    system_prompt: &str,
    thinking: ThinkingLevel,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    max_turns: Option<usize>,
    auto_approve: bool,
) -> Agent {
    let mut agent = Agent::new(AnthropicProvider)
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_thinking(thinking)
        .with_skills(skills.clone())
        .with_tools(build_tools(auto_approve));
    if let Some(max) = max_tokens {
        agent = agent.with_max_tokens(max);
    }
    if let Some(temp) = temperature {
        agent.temperature = Some(temp);
    }
    if let Some(turns) = max_turns {
        agent = agent.with_execution_limits(ExecutionLimits {
            max_turns: turns,
            ..ExecutionLimits::default()
        });
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

    if config.verbose {
        enable_verbose();
    }

    let mut model = config.model;
    let api_key = config.api_key;
    let skills = config.skills;
    let system_prompt = config.system_prompt;
    let mut thinking = config.thinking;
    let max_tokens = config.max_tokens;
    let temperature = config.temperature;
    let max_turns = config.max_turns;
    let continue_session = config.continue_session;
    let output_path = config.output_path;
    let mcp_servers = config.mcp_servers;
    // Auto-approve in non-interactive modes (piped, --prompt) or when --yes is set
    let is_interactive = io::stdin().is_terminal() && config.prompt_arg.is_none();
    let auto_approve = config.auto_approve || !is_interactive;

    let mut agent = build_agent(
        &model,
        &api_key,
        &skills,
        &system_prompt,
        thinking,
        max_tokens,
        temperature,
        max_turns,
        auto_approve,
    );

    // Connect to MCP servers (--mcp flags)
    let mut mcp_count = 0u32;
    for mcp_cmd in &mcp_servers {
        let parts: Vec<&str> = mcp_cmd.split_whitespace().collect();
        if parts.is_empty() {
            eprintln!("{YELLOW}warning:{RESET} Empty --mcp command, skipping");
            continue;
        }
        let command = parts[0];
        let args_slice: Vec<&str> = parts[1..].to_vec();
        eprintln!("{DIM}  mcp: connecting to {mcp_cmd}...{RESET}");
        // with_mcp_server_stdio consumes self; we must always update agent
        let result = agent
            .with_mcp_server_stdio(command, &args_slice, None)
            .await;
        match result {
            Ok(updated) => {
                agent = updated;
                mcp_count += 1;
                eprintln!("{GREEN}  ✓ mcp: {command} connected{RESET}");
            }
            Err(e) => {
                eprintln!("{RED}  ✗ mcp: failed to connect to '{mcp_cmd}': {e}{RESET}");
                // Agent was consumed on error — rebuild it with previous MCP connections lost
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                );
                eprintln!("{DIM}  mcp: agent rebuilt (previous MCP connections lost){RESET}");
            }
        }
    }

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
    if let Some(temp) = temperature {
        println!("{DIM}  temperature: {temp:.1}{RESET}");
    }
    if !skills.is_empty() {
        println!("{DIM}  skills: {} loaded{RESET}", skills.len());
    }
    if mcp_count > 0 {
        println!("{DIM}  mcp: {mcp_count} server(s) connected{RESET}");
    }
    if is_verbose() {
        println!("{DIM}  verbose: on{RESET}");
    }
    if !auto_approve {
        println!("{DIM}  tools: confirmation required (use --yes to skip){RESET}");
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
                println!("{DIM}  /help              Show this help");
                println!("  /quit, /exit       Exit yoyo");
                println!("  /clear             Clear conversation history");
                println!(
                    "  /commit [msg]      Commit staged changes (AI-generates message if no msg)"
                );
                println!("  /compact           Compact conversation to save context space");
                println!("  /config            Show all current settings");
                println!("  /context           Show loaded project context files");
                println!("  /cost              Show estimated session cost");
                println!("  /init              Create a starter YOYO.md project context file");
                println!("  /model <name>      Switch model (preserves conversation)");
                println!(
                    "  /think [level]     Show or change thinking level (off/low/medium/high)"
                );
                println!("  /status            Show session info");
                println!("  /tokens            Show token usage and context window");
                println!("  /save [path]       Save session to file (default: yoyo-session.json)");
                println!("  /load [path]       Load session from file");
                println!("  /diff              Show git diff summary of uncommitted changes");
                println!(
                    "  /git <subcmd>      Quick git: status, log [n], add <path>, stash, stash pop"
                );
                println!("  /undo              Revert all uncommitted changes (git checkout)");
                println!(
                    "  /pr [number]       List open PRs, view, diff, comment, or checkout a PR"
                );
                println!(
                    "                     /pr <n> diff | /pr <n> comment <text> | /pr <n> checkout"
                );
                println!("  /health            Run health checks (build, test, clippy, fmt)");
                println!("  /retry             Re-send the last user input");
                println!("  /run <cmd>         Run a shell command directly (no AI, no tokens)");
                println!("  !<cmd>             Shortcut for /run");
                println!("  /history           Show summary of conversation messages");
                println!("  /search <query>    Search conversation history for matching messages");
                println!("  /tree [depth]      Show project directory tree (default depth: 3)");
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
            "/cost" => {
                if let Some(cost) = estimate_cost(&session_total, &model) {
                    println!("{DIM}  Session cost: {}", format_cost(cost));
                    println!(
                        "    {} in / {} out",
                        format_token_count(session_total.input),
                        format_token_count(session_total.output)
                    );
                    if session_total.cache_read > 0 || session_total.cache_write > 0 {
                        println!(
                            "    cache: {} read / {} write",
                            format_token_count(session_total.cache_read),
                            format_token_count(session_total.cache_write)
                        );
                    }
                    // Show cost breakdown by category
                    if let Some((input_cost, cw_cost, cr_cost, output_cost)) =
                        cost_breakdown(&session_total, &model)
                    {
                        println!();
                        println!("    Breakdown:");
                        println!("      input:       {}", format_cost(input_cost));
                        println!("      output:      {}", format_cost(output_cost));
                        if cw_cost > 0.0 {
                            println!("      cache write: {}", format_cost(cw_cost));
                        }
                        if cr_cost > 0.0 {
                            println!("      cache read:  {}", format_cost(cr_cost));
                        }
                    }
                    println!("{RESET}");
                } else {
                    println!("{DIM}  Cost estimation not available for model '{model}'.{RESET}\n");
                }
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
                    temperature,
                    max_turns,
                    auto_approve,
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
                // Rebuild agent with new model, preserving conversation
                let saved = agent.save_messages().ok();
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                );
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                println!("{DIM}  (switched to {new_model}, conversation preserved){RESET}\n");
                continue;
            }
            "/think" => {
                let level_str = thinking_level_name(thinking);
                println!("{DIM}  thinking: {level_str}");
                println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
                continue;
            }
            s if s.starts_with("/think ") => {
                let level_str = s.trim_start_matches("/think ").trim();
                if level_str.is_empty() {
                    let current = thinking_level_name(thinking);
                    println!("{DIM}  thinking: {current}");
                    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
                    continue;
                }
                let new_thinking = parse_thinking_level(level_str);
                if new_thinking == thinking {
                    let current = thinking_level_name(thinking);
                    println!("{DIM}  thinking already set to {current}{RESET}\n");
                    continue;
                }
                thinking = new_thinking;
                // Rebuild agent with new thinking level, preserving conversation
                let saved = agent.save_messages().ok();
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                );
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                let level_name = thinking_level_name(thinking);
                println!("{DIM}  (thinking set to {level_name}, conversation preserved){RESET}\n");
                continue;
            }
            s if s == "/save" || s.starts_with("/save ") => {
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
            s if s == "/load" || s.starts_with("/load ") => {
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
            "/health" => {
                println!("{DIM}  Running health checks...{RESET}");
                let results = run_health_check();
                let all_passed = results.iter().all(|(_, passed, _)| *passed);
                for (name, passed, detail) in &results {
                    let icon = if *passed {
                        format!("{GREEN}✓{RESET}")
                    } else {
                        format!("{RED}✗{RESET}")
                    };
                    println!("  {icon} {name}: {detail}");
                }
                if all_passed {
                    println!("\n{GREEN}  All checks passed ✓{RESET}\n");
                } else {
                    println!("\n{RED}  Some checks failed ✗{RESET}\n");
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
            "/search" => {
                println!("{DIM}  usage: /search <query>");
                println!("  Search conversation history for messages containing <query>.{RESET}\n");
                continue;
            }
            s if s.starts_with("/search ") => {
                let query = s.trim_start_matches("/search ").trim();
                if query.is_empty() {
                    println!("{DIM}  usage: /search <query>{RESET}\n");
                    continue;
                }
                let messages = agent.messages();
                if messages.is_empty() {
                    println!("{DIM}  (no messages to search){RESET}\n");
                    continue;
                }
                let results = search_messages(messages, query);
                if results.is_empty() {
                    println!(
                        "{DIM}  No matches for '{query}' in {len} messages.{RESET}\n",
                        len = messages.len()
                    );
                } else {
                    println!(
                        "{DIM}  {count} match{es} for '{query}':",
                        count = results.len(),
                        es = if results.len() == 1 { "" } else { "es" }
                    );
                    for (idx, role, preview) in &results {
                        println!("    {idx:>3}. [{role}] {preview}");
                    }
                    println!("{RESET}");
                }
                continue;
            }
            "/config" => {
                println!("{DIM}  Configuration:");
                println!("    model:      {model}");
                println!("    thinking:   {}", thinking_level_name(thinking));
                println!(
                    "    max_tokens: {}",
                    max_tokens
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "default (8192)".to_string())
                );
                println!(
                    "    max_turns:  {}",
                    max_turns
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "default (50)".to_string())
                );
                println!(
                    "    temperature: {}",
                    temperature
                        .map(|t| format!("{t:.1}"))
                        .unwrap_or_else(|| "default".to_string())
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
                if mcp_count > 0 {
                    println!("    mcp:        {mcp_count} server(s)");
                }
                println!(
                    "    verbose:    {}",
                    if is_verbose() { "on" } else { "off" }
                );
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
            s if s == "/commit" || s.starts_with("/commit ") => {
                let arg = s.strip_prefix("/commit").unwrap_or("").trim();
                if !arg.is_empty() {
                    // Direct commit with provided message
                    let (ok, output) = run_git_commit(arg);
                    if ok {
                        println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                    } else {
                        eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                    }
                } else {
                    // AI-generate a commit message from staged diff
                    match get_staged_diff() {
                        None => {
                            eprintln!("{RED}  error: not in a git repository{RESET}\n");
                        }
                        Some(diff) if diff.trim().is_empty() => {
                            println!("{DIM}  nothing staged — use `git add` first{RESET}\n");
                        }
                        Some(diff) => {
                            let suggested = generate_commit_message(&diff);
                            println!("{DIM}  Suggested commit message:{RESET}");
                            println!("    {BOLD}{suggested}{RESET}");
                            eprint!(
                                "\n  {DIM}({GREEN}y{RESET}{DIM})es / ({RED}n{RESET}{DIM})o / ({CYAN}e{RESET}{DIM})dit: {RESET}"
                            );
                            io::stderr().flush().ok();
                            let mut response = String::new();
                            if io::stdin().read_line(&mut response).is_ok() {
                                let response = response.trim().to_lowercase();
                                match response.as_str() {
                                    "y" | "yes" | "" => {
                                        let (ok, output) = run_git_commit(&suggested);
                                        if ok {
                                            println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                                        } else {
                                            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                                        }
                                    }
                                    "e" | "edit" => {
                                        println!("{DIM}  Enter your commit message:{RESET}");
                                        eprint!("  > ");
                                        io::stderr().flush().ok();
                                        let mut custom_msg = String::new();
                                        if io::stdin().read_line(&mut custom_msg).is_ok() {
                                            let custom_msg = custom_msg.trim();
                                            if custom_msg.is_empty() {
                                                println!("{DIM}  (commit cancelled — empty message){RESET}\n");
                                            } else {
                                                let (ok, output) = run_git_commit(custom_msg);
                                                if ok {
                                                    println!(
                                                        "{GREEN}  ✓ {}{RESET}\n",
                                                        output.trim()
                                                    );
                                                } else {
                                                    eprintln!(
                                                        "{RED}  ✗ {}{RESET}\n",
                                                        output.trim()
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        println!("{DIM}  (commit cancelled){RESET}\n");
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }
            "/context" => {
                let files = cli::list_project_context_files();
                if files.is_empty() {
                    println!("{DIM}  No project context files found.");
                    println!("  Searched for: {}", PROJECT_CONTEXT_FILES.join(", "));
                    println!("  Create YOYO.md, CLAUDE.md, or .yoyo/instructions.md to add project context.");
                    println!("  Or run /init to create a starter YOYO.md.{RESET}\n");
                } else {
                    println!("{DIM}  Project context files:");
                    for (name, lines) in &files {
                        println!("    {name} ({lines} lines)");
                    }
                    println!("{RESET}");
                }
                continue;
            }
            "/init" => {
                let path = "YOYO.md";
                if std::path::Path::new(path).exists() {
                    println!("{DIM}  {path} already exists — not overwriting.{RESET}\n");
                } else {
                    let template = concat!(
                        "# Project Context\n",
                        "\n",
                        "<!-- This file is read by yoyo at startup to understand your project. -->\n",
                        "<!-- Customize it with project-specific instructions, conventions, and context. -->\n",
                        "\n",
                        "## About This Project\n",
                        "\n",
                        "<!-- Describe what this project does and its tech stack. -->\n",
                        "\n",
                        "## Coding Conventions\n",
                        "\n",
                        "<!-- List any coding standards, naming conventions, or patterns to follow. -->\n",
                        "\n",
                        "## Build & Test\n",
                        "\n",
                        "<!-- How to build, test, and run the project. -->\n",
                        "\n",
                        "## Important Files\n",
                        "\n",
                        "<!-- List key files and directories the agent should know about. -->\n",
                    );
                    match std::fs::write(path, template) {
                        Ok(_) => println!(
                            "{GREEN}  ✓ Created {path} — edit it to add project context.{RESET}\n"
                        ),
                        Err(e) => eprintln!("{RED}  error creating {path}: {e}{RESET}\n"),
                    }
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
            s if s == "/tree" || s.starts_with("/tree ") => {
                let arg = s.strip_prefix("/tree").unwrap_or("").trim();
                let max_depth = if arg.is_empty() {
                    3
                } else {
                    match arg.parse::<usize>() {
                        Ok(d) => d,
                        Err(_) => {
                            println!("{DIM}  usage: /tree [depth]  (default depth: 3){RESET}\n");
                            continue;
                        }
                    }
                };
                let tree = build_project_tree(max_depth);
                println!("{DIM}{tree}{RESET}\n");
                continue;
            }
            s if s.starts_with("/run ") || (s.starts_with('!') && s.len() > 1) => {
                let cmd = if s.starts_with("/run ") {
                    s.trim_start_matches("/run ").trim()
                } else {
                    s[1..].trim()
                };
                if cmd.is_empty() {
                    println!("{DIM}  usage: /run <command>  or  !<command>{RESET}\n");
                } else {
                    run_shell_command(cmd);
                }
                continue;
            }
            "/run" => {
                println!("{DIM}  usage: /run <command>  or  !<command>");
                println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
                continue;
            }
            s if s == "/pr" || s.starts_with("/pr ") => {
                let arg = s.strip_prefix("/pr").unwrap_or("").trim();
                match parse_pr_args(arg) {
                    PrSubcommand::List => {
                        match std::process::Command::new("gh")
                            .args(["pr", "list", "--limit", "10"])
                            .output()
                        {
                            Ok(output) if output.status.success() => {
                                let text = String::from_utf8_lossy(&output.stdout);
                                if text.trim().is_empty() {
                                    println!("{DIM}  (no open pull requests){RESET}\n");
                                } else {
                                    println!("{DIM}  Open pull requests:");
                                    for line in text.lines() {
                                        println!("    {line}");
                                    }
                                    println!("{RESET}");
                                }
                            }
                            Ok(output) => {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                            }
                            Err(_) => {
                                eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                            }
                        }
                    }
                    PrSubcommand::View(number) => {
                        let num_str = number.to_string();
                        match std::process::Command::new("gh")
                            .args(["pr", "view", &num_str])
                            .output()
                        {
                            Ok(output) if output.status.success() => {
                                let text = String::from_utf8_lossy(&output.stdout);
                                println!("{DIM}{text}{RESET}");
                            }
                            Ok(output) => {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                            }
                            Err(_) => {
                                eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                            }
                        }
                    }
                    PrSubcommand::Diff(number) => {
                        let num_str = number.to_string();
                        match std::process::Command::new("gh")
                            .args(["pr", "diff", &num_str])
                            .output()
                        {
                            Ok(output) if output.status.success() => {
                                let text = String::from_utf8_lossy(&output.stdout);
                                if text.trim().is_empty() {
                                    println!("{DIM}  (no diff for PR #{number}){RESET}\n");
                                } else {
                                    println!("{DIM}{text}{RESET}");
                                }
                            }
                            Ok(output) => {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                            }
                            Err(_) => {
                                eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                            }
                        }
                    }
                    PrSubcommand::Comment(number, text) => {
                        let num_str = number.to_string();
                        match std::process::Command::new("gh")
                            .args(["pr", "comment", &num_str, "--body", &text])
                            .output()
                        {
                            Ok(output) if output.status.success() => {
                                println!("{GREEN}  ✓ comment added to PR #{number}{RESET}\n");
                            }
                            Ok(output) => {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                            }
                            Err(_) => {
                                eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                            }
                        }
                    }
                    PrSubcommand::Checkout(number) => {
                        let num_str = number.to_string();
                        match std::process::Command::new("gh")
                            .args(["pr", "checkout", &num_str])
                            .output()
                        {
                            Ok(output) if output.status.success() => {
                                println!("{GREEN}  ✓ checked out PR #{number}{RESET}\n");
                            }
                            Ok(output) => {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                            }
                            Err(_) => {
                                eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                            }
                        }
                    }
                    PrSubcommand::Help => {
                        println!(
                            "{DIM}  usage: /pr                         List open pull requests"
                        );
                        println!(
                            "         /pr <number>                View details of a specific PR"
                        );
                        println!("         /pr <number> diff           Show the diff of a PR");
                        println!("         /pr <number> comment <text> Add a comment to a PR");
                        println!(
                            "         /pr <number> checkout       Checkout a PR locally{RESET}\n"
                        );
                    }
                }
                continue;
            }
            s if s == "/git" || s.starts_with("/git ") => {
                let arg = s.strip_prefix("/git").unwrap_or("").trim();
                let subcmd = parse_git_args(arg);
                run_git_subcommand(&subcmd);
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

/// Get staged changes (git diff --cached).
/// Returns None if git fails, Some("") if nothing staged, or Some(diff) with the diff text.
fn get_staged_diff() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--cached"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `git commit -m "<message>"` and return (success, output_text).
fn run_git_commit(message: &str) -> (bool, String) {
    match std::process::Command::new("git")
        .args(["commit", "-m", message])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let text = if stdout.is_empty() { stderr } else { stdout };
            (output.status.success(), text)
        }
        Err(e) => (false, format!("error: {e}")),
    }
}

/// Generate a conventional commit message from a diff using simple heuristics.
/// This is a local, token-free approach — no AI calls needed.
fn generate_commit_message(diff: &str) -> String {
    let mut files_changed: Vec<String> = Vec::new();
    let mut insertions = 0usize;
    let mut deletions = 0usize;

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            files_changed.push(path.to_string());
        } else if line.starts_with('+') && !line.starts_with("+++") {
            insertions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    // Determine type prefix based on file paths
    let prefix = if files_changed.iter().any(|f| f.contains("test")) {
        "test"
    } else if files_changed
        .iter()
        .any(|f| f.ends_with(".md") || f.starts_with("docs/") || f.starts_with("guide/"))
    {
        "docs"
    } else if files_changed
        .iter()
        .any(|f| f.starts_with(".github/") || f.starts_with("scripts/") || f == "Cargo.toml")
    {
        "chore"
    } else if deletions > insertions * 2 {
        "refactor"
    } else {
        "feat"
    };

    // Build a concise scope from changed files
    let scope = if files_changed.len() == 1 {
        let f = &files_changed[0];
        let name = f.rsplit('/').next().unwrap_or(f);
        // Strip extension for scope
        name.split('.').next().unwrap_or(name).to_string()
    } else if files_changed.len() <= 3 {
        files_changed
            .iter()
            .map(|f| {
                let name = f.rsplit('/').next().unwrap_or(f);
                name.split('.').next().unwrap_or(name).to_string()
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        format!("{} files", files_changed.len())
    };

    let summary = if deletions == 0 && insertions > 0 {
        "add changes"
    } else if insertions == 0 && deletions > 0 {
        "remove code"
    } else {
        "update code"
    };

    format!("{prefix}({scope}): {summary}")
}

/// Represents a parsed `/pr` subcommand.
#[derive(Debug, PartialEq)]
enum PrSubcommand {
    /// `/pr` — list open pull requests
    List,
    /// `/pr <number>` — view details of a specific PR
    View(u32),
    /// `/pr <number> diff` — show the diff of a PR
    Diff(u32),
    /// `/pr <number> comment <text>` — add a comment to a PR
    Comment(u32, String),
    /// `/pr <number> checkout` — checkout a PR locally
    Checkout(u32),
    /// Invalid usage — show help
    Help,
}

/// Parse the argument string after `/pr` into a `PrSubcommand`.
fn parse_pr_args(arg: &str) -> PrSubcommand {
    let arg = arg.trim();
    if arg.is_empty() {
        return PrSubcommand::List;
    }

    let parts: Vec<&str> = arg.splitn(3, char::is_whitespace).collect();
    let number = match parts[0].parse::<u32>() {
        Ok(n) => n,
        Err(_) => return PrSubcommand::Help,
    };

    if parts.len() == 1 {
        return PrSubcommand::View(number);
    }

    match parts[1].to_lowercase().as_str() {
        "diff" => PrSubcommand::Diff(number),
        "checkout" => PrSubcommand::Checkout(number),
        "comment" => {
            let text = if parts.len() == 3 {
                parts[2].trim().to_string()
            } else {
                String::new()
            };
            if text.is_empty() {
                PrSubcommand::Help
            } else {
                PrSubcommand::Comment(number, text)
            }
        }
        _ => PrSubcommand::Help,
    }
}

/// Run a shell command directly and print its output.
/// Used by the /run command to execute without going through the AI.
fn run_shell_command(cmd: &str) {
    let start = std::time::Instant::now();
    let output = std::process::Command::new("sh").args(["-c", cmd]).output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{RED}{stderr}{RESET}");
            }
            let code = o.status.code().unwrap_or(-1);
            if code == 0 {
                println!("{DIM}  ✓ exit {code} ({elapsed}){RESET}\n");
            } else {
                println!("{RED}  ✗ exit {code} ({elapsed}){RESET}\n");
            }
        }
        Err(e) => {
            eprintln!("{RED}  error running command: {e}{RESET}\n");
        }
    }
}

/// Check if a line needs continuation (backslash at end, or opens a code fence).
fn needs_continuation(line: &str) -> bool {
    line.ends_with('\\') || line.starts_with("```")
}

/// Collect multi-line input using rustyline (for interactive REPL mode).
/// Same logic as `collect_multiline` but uses rustyline's readline for continuation prompts.
fn collect_multiline_rl(first_line: &str, rl: &mut Editor<YoyoHelper, DefaultHistory>) -> String {
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

/// Format a ThinkingLevel as a display string.
fn thinking_level_name(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Off => "off",
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
    }
}

/// Run health checks (build, test, clippy, fmt) and return results.
/// Each result is (name, passed, detail_message).
fn run_health_check() -> Vec<(&'static str, bool, String)> {
    let mut checks: Vec<(&str, &[&str])> = vec![("build", &["cargo", "build"])];
    // Skip "cargo test" when built with #[cfg(test)] to avoid infinite recursion
    // (test_health_check_function → run_health_check → cargo test → test_health_check_function → …)
    #[cfg(not(test))]
    checks.push(("test", &["cargo", "test"]));
    checks.push((
        "clippy",
        &["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
    ));
    checks.push(("fmt", &["cargo", "fmt", "--", "--check"]));

    let mut results = Vec::new();
    for (name, args) in checks {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        let elapsed = format_duration(start.elapsed());
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, format!("ok ({elapsed})")));
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let first_line = stderr.lines().next().unwrap_or("(unknown error)");
                results.push((
                    name,
                    false,
                    format!(
                        "FAIL ({elapsed}): {}",
                        truncate_with_ellipsis(first_line, 80)
                    ),
                ));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Build a directory tree from `git ls-files`, grouped by directory.
/// Returns a formatted string showing the project structure up to `max_depth` levels.
/// Falls back to a simple directory walk if not in a git repo.
fn build_project_tree(max_depth: usize) -> String {
    // Try git ls-files first (respects .gitignore)
    let files = match std::process::Command::new("git")
        .args(["ls-files"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut files: Vec<String> = text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            files.sort();
            files
        }
        _ => return "(not a git repository — /tree requires git)".to_string(),
    };

    if files.is_empty() {
        return "(no tracked files)".to_string();
    }

    format_tree_from_paths(&files, max_depth)
}

/// Format a sorted list of file paths into an indented tree string.
/// Limits output to `max_depth` levels (0 = root files only, 1 = one level of dirs, etc.).
fn format_tree_from_paths(paths: &[String], max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut output = String::new();
    // Collect all unique directory prefixes and files at each level
    let mut printed_dirs: BTreeSet<String> = BTreeSet::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let depth = parts.len() - 1; // number of directory components

        // Print directory prefixes we haven't seen yet
        for level in 0..parts.len().saturating_sub(1).min(max_depth) {
            let dir_path: String = parts[..=level].join("/");
            let dir_key = format!("{}/", dir_path);
            if printed_dirs.insert(dir_key) {
                let indent = "  ".repeat(level);
                let dir_name = parts[level];
                output.push_str(&format!("{indent}{dir_name}/\n"));
            }
        }

        // Print the file itself if within depth
        if depth <= max_depth {
            let indent = "  ".repeat(depth.min(max_depth));
            let file_name = parts.last().unwrap_or(&"");
            output.push_str(&format!("{indent}{file_name}\n"));
        }
    }

    // Trim trailing newline
    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

/// Known REPL command prefixes. Used to detect unknown slash commands.
const KNOWN_COMMANDS: &[&str] = &[
    "/help", "/quit", "/exit", "/clear", "/compact", "/commit", "/cost", "/status", "/tokens",
    "/save", "/load", "/diff", "/undo", "/health", "/retry", "/history", "/search", "/model",
    "/think", "/config", "/context", "/init", "/version", "/run", "/tree", "/pr", "/git",
];

/// Represents a parsed `/git` subcommand.
#[derive(Debug, PartialEq)]
enum GitSubcommand {
    /// `/git status` — run `git status --short`
    Status,
    /// `/git log [n]` — show last n commits (default 5)
    Log(usize),
    /// `/git add <path>` — stage files
    Add(String),
    /// `/git stash` — stash changes
    Stash,
    /// `/git stash pop` — pop stashed changes
    StashPop,
    /// Invalid or missing subcommand — show help
    Help,
}

/// Parse the argument string after `/git` into a `GitSubcommand`.
fn parse_git_args(arg: &str) -> GitSubcommand {
    let arg = arg.trim();
    if arg.is_empty() {
        return GitSubcommand::Help;
    }

    let parts: Vec<&str> = arg.splitn(3, char::is_whitespace).collect();
    match parts[0].to_lowercase().as_str() {
        "status" => GitSubcommand::Status,
        "log" => {
            let n = parts
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(5);
            GitSubcommand::Log(n)
        }
        "add" => {
            if parts.len() < 2 || parts[1].trim().is_empty() {
                GitSubcommand::Help
            } else {
                // Rejoin remaining parts as the path (handles spaces in filenames via quoting at shell level)
                let path = parts[1..].join(" ");
                GitSubcommand::Add(path)
            }
        }
        "stash" => {
            if parts.len() >= 2 && parts[1].to_lowercase() == "pop" {
                GitSubcommand::StashPop
            } else {
                GitSubcommand::Stash
            }
        }
        _ => GitSubcommand::Help,
    }
}

/// Execute a `/git` subcommand directly (no AI, no tokens).
fn run_git_subcommand(subcmd: &GitSubcommand) {
    match subcmd {
        GitSubcommand::Status => {
            match std::process::Command::new("git")
                .args(["status", "--short"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (clean working tree){RESET}\n");
                    } else {
                        println!("{DIM}{text}{RESET}");
                    }
                }
                _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            }
        }
        GitSubcommand::Log(n) => {
            let n_str = n.to_string();
            match std::process::Command::new("git")
                .args(["log", "--oneline", "-n", &n_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (no commits yet){RESET}\n");
                    } else {
                        println!("{DIM}{text}{RESET}");
                    }
                }
                _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            }
        }
        GitSubcommand::Add(path) => {
            match std::process::Command::new("git")
                .args(["add", path])
                .output()
            {
                Ok(output) if output.status.success() => {
                    println!("{GREEN}  ✓ staged: {path}{RESET}\n");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => eprintln!("{RED}  error: git not found{RESET}\n"),
            }
        }
        GitSubcommand::Stash => match std::process::Command::new("git").args(["stash"]).output() {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout);
                println!("{GREEN}  ✓ {}{RESET}\n", text.trim());
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
            }
            Err(_) => eprintln!("{RED}  error: git not found{RESET}\n"),
        },
        GitSubcommand::StashPop => {
            match std::process::Command::new("git")
                .args(["stash", "pop"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    println!("{GREEN}  ✓ {}{RESET}\n", text.trim());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => eprintln!("{RED}  error: git not found{RESET}\n"),
            }
        }
        GitSubcommand::Help => {
            println!("{DIM}  usage: /git status             Show working tree status");
            println!("         /git log [n]             Show last n commits (default: 5)");
            println!("         /git add <path>          Stage files for commit");
            println!("         /git stash               Stash uncommitted changes");
            println!("         /git stash pop           Restore stashed changes{RESET}\n");
        }
    }
}

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
            "/help", "/quit", "/exit", "/clear", "/compact", "/commit", "/config", "/context",
            "/init", "/status", "/tokens", "/save", "/load", "/diff", "/undo", "/health", "/retry",
            "/run", "/history", "/search", "/model", "/think", "/version", "/tree", "/pr", "/git",
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
        // Verify typo-like commands are caught as unknown
        assert!(is_unknown_command("/savefile"));
        assert!(is_unknown_command("/loadfile"));

        assert!(!is_unknown_command("/help"));
        assert!(!is_unknown_command("/quit"));
        assert!(!is_unknown_command("/model"));
        assert!(!is_unknown_command("/model claude-opus-4-6"));
        assert!(!is_unknown_command("/save"));
        assert!(!is_unknown_command("/save myfile.json"));
        assert!(!is_unknown_command("/load"));
        assert!(!is_unknown_command("/load myfile.json"));
        assert!(!is_unknown_command("/config"));
        assert!(!is_unknown_command("/context"));
        assert!(!is_unknown_command("/version"));
    }

    #[test]
    fn test_thinking_level_name() {
        assert_eq!(thinking_level_name(ThinkingLevel::Off), "off");
        assert_eq!(thinking_level_name(ThinkingLevel::Minimal), "minimal");
        assert_eq!(thinking_level_name(ThinkingLevel::Low), "low");
        assert_eq!(thinking_level_name(ThinkingLevel::Medium), "medium");
        assert_eq!(thinking_level_name(ThinkingLevel::High), "high");
    }

    #[test]
    fn test_health_check_function() {
        // run_health_check skips "cargo test" under #[cfg(test)] to avoid recursion
        let results = run_health_check();
        assert!(
            !results.is_empty(),
            "Health check should return at least one result"
        );
        for (name, passed, _) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
        // "test" check should be excluded under cfg(test)
        assert!(
            !results.iter().any(|(name, _, _)| *name == "test"),
            "cargo test check should be skipped to avoid recursion"
        );
    }

    #[test]
    fn test_run_command_recognized() {
        assert!(!is_unknown_command("/run"));
        assert!(!is_unknown_command("/run echo hello"));
        assert!(!is_unknown_command("/run ls -la"));
    }

    #[test]
    fn test_run_shell_command_basic() {
        // Verify run_shell_command doesn't panic on basic commands
        // (output goes to stdout/stderr, we just check it doesn't crash)
        run_shell_command("echo hello");
    }

    #[test]
    fn test_run_shell_command_failing() {
        // Non-zero exit should not panic
        run_shell_command("false");
    }

    #[test]
    fn test_bang_shortcut_matching() {
        // ! prefix should match for /run shortcut
        let bang_matches = |s: &str| s.starts_with('!') && s.len() > 1;
        assert!(bang_matches("!ls"));
        assert!(bang_matches("!echo hello"));
        assert!(bang_matches("! ls")); // space after bang is fine
        assert!(!bang_matches("!")); // bare bang alone should not match
    }

    #[test]
    fn test_run_command_matching() {
        // /run should only match /run or /run <cmd>, not /running
        let run_matches = |s: &str| s == "/run" || s.starts_with("/run ");
        assert!(run_matches("/run"));
        assert!(run_matches("/run echo hello"));
        assert!(!run_matches("/running"));
        assert!(!run_matches("/runaway"));
    }

    #[test]
    fn test_format_tree_from_paths_basic() {
        let paths = vec![
            "Cargo.toml".to_string(),
            "README.md".to_string(),
            "src/cli.rs".to_string(),
            "src/format.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("  main.rs"));
        assert!(tree.contains("  cli.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_nested() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/utils/helpers.rs".to_string(),
            "src/utils/format.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("  utils/"));
        assert!(tree.contains("    helpers.rs"));
        assert!(tree.contains("    format.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_limit() {
        let paths = vec![
            "a/b/c/d/deep.txt".to_string(),
            "a/shallow.txt".to_string(),
            "top.txt".to_string(),
        ];
        // depth 1: show dirs at level 0 ('a/'), files at depth ≤ 1
        let tree = format_tree_from_paths(&paths, 1);
        assert!(tree.contains("top.txt"));
        assert!(tree.contains("a/"));
        assert!(tree.contains("  shallow.txt"));
        // Files deeper than max_depth should not appear
        assert!(!tree.contains("deep.txt"));
        // Directory 'b/' is at level 1, beyond max_depth=1 for dirs
        assert!(!tree.contains("b/"));
    }

    #[test]
    fn test_format_tree_from_paths_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_format_tree_from_paths_root_files_only() {
        let paths = vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // No directories, just root files
        assert!(!tree.contains('/'));
        assert!(tree.contains("Cargo.lock"));
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_zero() {
        let paths = vec!["README.md".to_string(), "src/main.rs".to_string()];
        let tree = format_tree_from_paths(&paths, 0);
        // Depth 0: only root-level files shown
        assert!(tree.contains("README.md"));
        // main.rs is at depth 1, should not show at depth 0
        assert!(!tree.contains("main.rs"));
    }

    #[test]
    fn test_format_tree_dir_printed_once() {
        let paths = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // "src/" should appear exactly once
        assert_eq!(tree.matches("src/").count(), 1);
    }

    #[test]
    fn test_build_project_tree_runs() {
        // In a git repo, this should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
        // Should contain at least Cargo.toml (we're in a Rust project)
        assert!(tree.contains("Cargo.toml"));
    }

    #[test]
    fn test_tree_command_recognized() {
        assert!(!is_unknown_command("/tree"));
        assert!(!is_unknown_command("/tree 2"));
        assert!(!is_unknown_command("/tree 5"));
    }

    #[test]
    fn test_pr_command_recognized() {
        assert!(!is_unknown_command("/pr"));
        assert!(!is_unknown_command("/pr 42"));
        assert!(!is_unknown_command("/pr 123"));
    }

    #[test]
    fn test_pr_command_matching() {
        // /pr should match exact or with space separator, not /print etc.
        let pr_matches = |s: &str| s == "/pr" || s.starts_with("/pr ");
        assert!(pr_matches("/pr"));
        assert!(pr_matches("/pr 42"));
        assert!(pr_matches("/pr 123"));
        assert!(!pr_matches("/print"));
        assert!(!pr_matches("/process"));
    }

    #[test]
    fn test_pr_number_parsing() {
        // Verify we can parse a PR number from /pr <number>
        let input = "/pr 42";
        let arg = input.strip_prefix("/pr").unwrap_or("").trim();
        assert_eq!(arg, "42");
        assert!(arg.parse::<u32>().is_ok());
        assert_eq!(arg.parse::<u32>().unwrap(), 42);

        // Bare /pr has empty arg
        let input_bare = "/pr";
        let arg_bare = input_bare.strip_prefix("/pr").unwrap_or("").trim();
        assert!(arg_bare.is_empty());
    }

    #[test]
    fn test_pr_subcommand_list() {
        assert_eq!(parse_pr_args(""), PrSubcommand::List);
        assert_eq!(parse_pr_args("  "), PrSubcommand::List);
    }

    #[test]
    fn test_pr_subcommand_view() {
        assert_eq!(parse_pr_args("42"), PrSubcommand::View(42));
        assert_eq!(parse_pr_args("123"), PrSubcommand::View(123));
        assert_eq!(parse_pr_args("1"), PrSubcommand::View(1));
    }

    #[test]
    fn test_pr_subcommand_diff() {
        assert_eq!(parse_pr_args("42 diff"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("7 diff"), PrSubcommand::Diff(7));
    }

    #[test]
    fn test_pr_subcommand_checkout() {
        assert_eq!(parse_pr_args("42 checkout"), PrSubcommand::Checkout(42));
        assert_eq!(parse_pr_args("99 checkout"), PrSubcommand::Checkout(99));
    }

    #[test]
    fn test_pr_subcommand_comment() {
        assert_eq!(
            parse_pr_args("42 comment looks good!"),
            PrSubcommand::Comment(42, "looks good!".to_string())
        );
        assert_eq!(
            parse_pr_args("10 comment LGTM, merging now"),
            PrSubcommand::Comment(10, "LGTM, merging now".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_comment_requires_text() {
        // comment without text should show help
        assert_eq!(parse_pr_args("42 comment"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 comment  "), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_invalid() {
        assert_eq!(parse_pr_args("abc"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 unknown"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 merge"), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_case_insensitive() {
        assert_eq!(parse_pr_args("42 DIFF"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("42 Checkout"), PrSubcommand::Checkout(42));
        assert_eq!(
            parse_pr_args("42 Comment nice work"),
            PrSubcommand::Comment(42, "nice work".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_recognized() {
        // Subcommands should not be flagged as unknown commands
        assert!(!is_unknown_command("/pr 42 diff"));
        assert!(!is_unknown_command("/pr 42 comment hello"));
        assert!(!is_unknown_command("/pr 42 checkout"));
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

    #[test]
    fn test_save_load_command_matching() {
        // /save and /load should only match exact word or with space separator
        // This tests the fix for /savefile being treated as /save
        let save_matches = |s: &str| s == "/save" || s.starts_with("/save ");
        let load_matches = |s: &str| s == "/load" || s.starts_with("/load ");

        assert!(save_matches("/save"));
        assert!(save_matches("/save myfile.json"));
        assert!(!save_matches("/savefile"));
        assert!(!save_matches("/saveXYZ"));

        assert!(load_matches("/load"));
        assert!(load_matches("/load myfile.json"));
        assert!(!load_matches("/loadfile"));
        assert!(!load_matches("/loadXYZ"));
    }

    #[test]
    fn test_commit_command_recognized() {
        assert!(!is_unknown_command("/commit"));
        assert!(!is_unknown_command("/commit fix typo in README"));
    }

    #[test]
    fn test_commit_command_matching() {
        // /commit should match exact or with space separator, not /committing etc.
        let commit_matches = |s: &str| s == "/commit" || s.starts_with("/commit ");
        assert!(commit_matches("/commit"));
        assert!(commit_matches("/commit fix: typo"));
        assert!(commit_matches("/commit feat(cli): add commit command"));
        assert!(!commit_matches("/committing"));
        assert!(!commit_matches("/commits"));
    }

    #[test]
    fn test_commit_arg_extraction() {
        let input = "/commit feat: add new feature";
        let arg = input.strip_prefix("/commit").unwrap_or("").trim();
        assert_eq!(arg, "feat: add new feature");

        // Bare /commit has empty arg
        let input_bare = "/commit";
        let arg_bare = input_bare.strip_prefix("/commit").unwrap_or("").trim();
        assert!(arg_bare.is_empty());
    }

    #[test]
    fn test_get_staged_diff_runs() {
        // Should not panic; returns None if not in git repo, or Some (possibly empty)
        let result = get_staged_diff();
        // We're in a git repo in CI, so it should return Some
        assert!(result.is_some(), "Should return Some in a git repo");
    }

    #[test]
    fn test_generate_commit_message_basic() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,5 @@
+// new comment
+use std::io;
 fn main() {
     println!(\"hello\");
 }
";
        let msg = generate_commit_message(diff);
        // Should produce a conventional commit format: type(scope): description
        assert!(msg.contains('('), "Should have scope: {msg}");
        assert!(msg.contains("):"), "Should have conventional format: {msg}");
        assert!(msg.contains("main"), "Scope should mention 'main': {msg}");
    }

    #[test]
    fn test_generate_commit_message_docs() {
        let diff = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,2 +1,3 @@
 # Project
+New docs line
";
        let msg = generate_commit_message(diff);
        assert!(
            msg.starts_with("docs("),
            "Markdown changes should use docs prefix: {msg}"
        );
    }

    #[test]
    fn test_generate_commit_message_multiple_files() {
        let diff = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
+// change a
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -1 +1,2 @@
+// change b
diff --git a/src/c.rs b/src/c.rs
--- a/src/c.rs
+++ b/src/c.rs
@@ -1 +1,2 @@
+// change c
diff --git a/src/d.rs b/src/d.rs
--- a/src/d.rs
+++ b/src/d.rs
@@ -1 +1,2 @@
+// change d
";
        let msg = generate_commit_message(diff);
        // More than 3 files should show "N files"
        assert!(
            msg.contains("4 files"),
            "Should show file count for many files: {msg}"
        );
    }

    #[test]
    fn test_generate_commit_message_deletions_only() {
        let diff = "\
diff --git a/src/old.rs b/src/old.rs
--- a/src/old.rs
+++ b/src/old.rs
@@ -1,5 +1,2 @@
-// removed line 1
-// removed line 2
-// removed line 3
 fn keep() {}
";
        let msg = generate_commit_message(diff);
        assert!(
            msg.contains("remove code"),
            "Pure deletion should say 'remove code': {msg}"
        );
    }

    #[test]
    fn test_git_command_recognized() {
        assert!(!is_unknown_command("/git"));
        assert!(!is_unknown_command("/git status"));
        assert!(!is_unknown_command("/git log"));
        assert!(!is_unknown_command("/git log 10"));
        assert!(!is_unknown_command("/git add src/main.rs"));
        assert!(!is_unknown_command("/git stash"));
        assert!(!is_unknown_command("/git stash pop"));
    }

    #[test]
    fn test_git_command_matching() {
        // /git should match exact or with space, not /github etc.
        let git_matches = |s: &str| s == "/git" || s.starts_with("/git ");
        assert!(git_matches("/git"));
        assert!(git_matches("/git status"));
        assert!(git_matches("/git log 5"));
        assert!(!git_matches("/github"));
        assert!(!git_matches("/gitignore"));
    }

    #[test]
    fn test_git_subcommand_help() {
        assert_eq!(parse_git_args(""), GitSubcommand::Help);
        assert_eq!(parse_git_args("  "), GitSubcommand::Help);
        assert_eq!(parse_git_args("unknown"), GitSubcommand::Help);
        assert_eq!(parse_git_args("push"), GitSubcommand::Help);
    }

    #[test]
    fn test_git_subcommand_status() {
        assert_eq!(parse_git_args("status"), GitSubcommand::Status);
        assert_eq!(parse_git_args("STATUS"), GitSubcommand::Status);
        assert_eq!(parse_git_args("Status"), GitSubcommand::Status);
    }

    #[test]
    fn test_git_subcommand_log() {
        assert_eq!(parse_git_args("log"), GitSubcommand::Log(5));
        assert_eq!(parse_git_args("log 10"), GitSubcommand::Log(10));
        assert_eq!(parse_git_args("log 1"), GitSubcommand::Log(1));
        assert_eq!(parse_git_args("LOG 20"), GitSubcommand::Log(20));
        // Invalid number falls back to default 5
        assert_eq!(parse_git_args("log abc"), GitSubcommand::Log(5));
    }

    #[test]
    fn test_git_subcommand_add() {
        assert_eq!(
            parse_git_args("add src/main.rs"),
            GitSubcommand::Add("src/main.rs".to_string())
        );
        assert_eq!(parse_git_args("add ."), GitSubcommand::Add(".".to_string()));
        assert_eq!(
            parse_git_args("ADD Cargo.toml"),
            GitSubcommand::Add("Cargo.toml".to_string())
        );
        // add without path shows help
        assert_eq!(parse_git_args("add"), GitSubcommand::Help);
        assert_eq!(parse_git_args("add  "), GitSubcommand::Help);
    }

    #[test]
    fn test_git_subcommand_stash() {
        assert_eq!(parse_git_args("stash"), GitSubcommand::Stash);
        assert_eq!(parse_git_args("STASH"), GitSubcommand::Stash);
    }

    #[test]
    fn test_git_subcommand_stash_pop() {
        assert_eq!(parse_git_args("stash pop"), GitSubcommand::StashPop);
        assert_eq!(parse_git_args("STASH POP"), GitSubcommand::StashPop);
        assert_eq!(parse_git_args("stash Pop"), GitSubcommand::StashPop);
    }
}
