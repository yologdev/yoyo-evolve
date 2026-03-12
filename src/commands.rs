//! REPL command handlers for yoyo.
//!
//! Each `/command` in the interactive REPL is handled by a function in this module.
//! The main loop dispatches to these handlers, keeping main.rs as a thin REPL driver.

// All handle_* functions in this module are dispatched from the REPL in main.rs.

use crate::cli::{
    self, is_verbose, AUTO_COMPACT_THRESHOLD, DEFAULT_SESSION_PATH, MAX_CONTEXT_TOKENS, VERSION,
};
use crate::docs;
use crate::format::*;
use crate::git::*;
use crate::prompt::*;

use std::io::{self, Write};
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::*;

/// Known REPL command prefixes. Used to detect unknown slash commands
/// and for tab-completion in the REPL.
pub const KNOWN_COMMANDS: &[&str] = &[
    "/help", "/quit", "/exit", "/clear", "/compact", "/commit", "/cost", "/docs", "/fix",
    "/status", "/tokens", "/save", "/load", "/diff", "/undo", "/health", "/retry", "/history",
    "/search", "/model", "/think", "/config", "/context", "/init", "/version", "/run", "/tree",
    "/pr", "/git", "/test", "/lint", "/spawn",
];

/// Check if a slash-prefixed input is an unknown command.
/// Extracts the first word and checks against known commands.
pub fn is_unknown_command(input: &str) -> bool {
    let cmd = input.split_whitespace().next().unwrap_or(input);
    !KNOWN_COMMANDS.contains(&cmd)
}

/// Format a ThinkingLevel as a display string.
pub fn thinking_level_name(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Off => "off",
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
    }
}

// ── /help ────────────────────────────────────────────────────────────────

pub fn handle_help() {
    println!("{DIM}  /help              Show this help");
    println!("  /quit, /exit       Exit yoyo");
    println!("  /clear             Clear conversation history");
    println!("  /commit [msg]      Commit staged changes (AI-generates message if no msg)");
    println!("  /compact           Compact conversation to save context space");
    println!("  /config            Show all current settings");
    println!("  /context           Show loaded project context files");
    println!("  /cost              Show estimated session cost");
    println!("  /docs <crate> [item] Look up docs.rs documentation for a Rust crate");
    println!("  /init              Create a starter YOYO.md project context file");
    println!("  /model <name>      Switch model (preserves conversation)");
    println!("  /think [level]     Show or change thinking level (off/low/medium/high)");
    println!("  /status            Show session info");
    println!("  /tokens            Show token usage and context window");
    println!("  /save [path]       Save session to file (default: yoyo-session.json)");
    println!("  /load [path]       Load session from file");
    println!("  /diff              Show git diff summary of uncommitted changes");
    println!("  /fix               Auto-fix build/lint errors (runs checks, sends failures to AI)");
    println!("  /git <subcmd>      Quick git: status, log, add, diff, branch, stash");
    println!("  /undo              Revert all uncommitted changes (git checkout)");
    println!("  /pr [number]       List open PRs, view, diff, comment, or checkout a PR");
    println!("                     /pr <n> diff | /pr <n> comment <text> | /pr <n> checkout");
    println!("  /health            Run project health checks (auto-detects project type)");
    println!("  /retry             Re-send the last user input");
    println!("  /run <cmd>         Run a shell command directly (no AI, no tokens)");
    println!("  !<cmd>             Shortcut for /run");
    println!("  /test              Auto-detect and run project tests (cargo test, npm test, etc.)");
    println!(
        "  /lint              Auto-detect and run project linter (clippy, eslint, ruff, etc.)"
    );
    println!("  /history           Show summary of conversation messages");
    println!("  /search <query>    Search conversation history for matching messages");
    println!("  /spawn <task>      Spawn a subagent to handle a task (separate context)");
    println!("  /tree [depth]      Show project directory tree (default depth: 3)");
    println!("  /version           Show yoyo version");
    println!();
    println!("  Multi-line input:");
    println!("  End a line with \\ to continue on the next line");
    println!("  Start with ``` to enter a fenced code block{RESET}\n");
}

// ── /version ─────────────────────────────────────────────────────────────

pub fn handle_version() {
    println!("{DIM}  yoyo v{VERSION}{RESET}\n");
}

// ── /status ──────────────────────────────────────────────────────────────

pub fn handle_status(model: &str, cwd: &str, session_total: &Usage) {
    println!("{DIM}  model:   {model}");
    if let Some(branch) = git_branch() {
        println!("  git:     {branch}");
    }
    println!("  cwd:     {cwd}");
    println!(
        "  tokens:  {} in / {} out (session total){RESET}\n",
        session_total.input, session_total.output
    );
}

// ── /tokens ──────────────────────────────────────────────────────────────

pub fn handle_tokens(agent: &Agent, session_total: &Usage, model: &str) {
    let max_context = MAX_CONTEXT_TOKENS;
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
    if let Some(cost) = estimate_cost(session_total, model) {
        println!("    est. cost:   {}", format_cost(cost));
    }
    println!("{RESET}");
}

// ── /cost ────────────────────────────────────────────────────────────────

pub fn handle_cost(session_total: &Usage, model: &str) {
    if let Some(cost) = estimate_cost(session_total, model) {
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
        if let Some((input_cost, cw_cost, cr_cost, output_cost)) =
            cost_breakdown(session_total, model)
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
}

// ── compact ──────────────────────────────────────────────────────────────

/// Compact the agent's conversation and return (before_count, before_tokens, after_count, after_tokens).
/// Returns None if nothing changed.
pub fn compact_agent(agent: &mut Agent) -> Option<(usize, u64, usize, u64)> {
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
pub fn auto_compact_if_needed(agent: &mut Agent) {
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

pub fn handle_compact(agent: &mut Agent) {
    let messages = agent.messages();
    let before_count = messages.len();
    let before_tokens = total_tokens(messages) as u64;
    match compact_agent(agent) {
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
}

// ── /save ────────────────────────────────────────────────────────────────

pub fn handle_save(agent: &Agent, input: &str) {
    let path = input.strip_prefix("/save").unwrap_or("").trim();
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
}

// ── /load ────────────────────────────────────────────────────────────────

pub fn handle_load(agent: &mut Agent, input: &str) {
    let path = input.strip_prefix("/load").unwrap_or("").trim();
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
}

// ── /diff ────────────────────────────────────────────────────────────────

pub fn handle_diff() {
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
}

// ── /undo ────────────────────────────────────────────────────────────────

pub fn handle_undo() {
    let diff_output = std::process::Command::new("git")
        .args(["diff", "--stat"])
        .output();
    let untracked = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .output();

    let has_diff = diff_output
        .as_ref()
        .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
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

        if has_diff {
            let _ = std::process::Command::new("git")
                .args(["checkout", "--", "."])
                .output();
        }
        if has_untracked {
            let _ = std::process::Command::new("git")
                .args(["clean", "-fd"])
                .output();
        }
        println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");
    }
}

// ── /commit ──────────────────────────────────────────────────────────────

pub fn handle_commit(input: &str) {
    let arg = input.strip_prefix("/commit").unwrap_or("").trim();
    if !arg.is_empty() {
        let (ok, output) = run_git_commit(arg);
        if ok {
            println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
        } else {
            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
        }
    } else {
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
                                        println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                                    } else {
                                        eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
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
}

// ── /context ─────────────────────────────────────────────────────────────

pub fn handle_context() {
    let files = cli::list_project_context_files();
    if files.is_empty() {
        println!("{DIM}  No project context files found.");
        println!("  Create a YOYO.md to give yoyo project context.");
        println!("  Also supports: CLAUDE.md (compatibility alias), .yoyo/instructions.md");
        println!("  Run /init to create a starter YOYO.md.{RESET}\n");
    } else {
        println!("{DIM}  Project context files:");
        for (name, lines) in &files {
            println!("    {name} ({lines} lines)");
        }
        println!("{RESET}");
    }
}

// ── /init ────────────────────────────────────────────────────────────────

pub fn handle_init() {
    let path = "YOYO.md";
    if std::path::Path::new(path).exists() {
        println!("{DIM}  {path} already exists — not overwriting.{RESET}\n");
    } else if std::path::Path::new("CLAUDE.md").exists() {
        println!("{DIM}  CLAUDE.md already exists — yoyo reads it as a compatibility alias.");
        println!("  Rename it to YOYO.md when you're ready: mv CLAUDE.md YOYO.md{RESET}\n");
    } else {
        let template = concat!(
            "# Project Context\n",
            "\n",
            "<!-- YOYO.md — yoyo's primary project context file. -->\n",
            "<!-- Also works as CLAUDE.md for compatibility with other tools. -->\n",
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
            Ok(_) => {
                println!("{GREEN}  ✓ Created {path} — edit it to add project context.{RESET}\n")
            }
            Err(e) => eprintln!("{RED}  error creating {path}: {e}{RESET}\n"),
        }
    }
}

// ── /retry ───────────────────────────────────────────────────────────────

pub async fn handle_retry(
    agent: &mut Agent,
    last_input: &Option<String>,
    session_total: &mut Usage,
    model: &str,
) {
    match last_input {
        Some(prev) => {
            println!("{DIM}  (retrying last input){RESET}");
            let retry_input = prev.clone();
            run_prompt(agent, &retry_input, session_total, model).await;
            auto_compact_if_needed(agent);
        }
        None => {
            eprintln!("{DIM}  (nothing to retry — no previous input){RESET}\n");
        }
    }
}

// ── /model ───────────────────────────────────────────────────────────────

pub fn handle_model_show(model: &str) {
    println!("{DIM}  current model: {model}");
    println!("  usage: /model <name>{RESET}\n");
}

// ── /think ───────────────────────────────────────────────────────────────

pub fn handle_think_show(thinking: ThinkingLevel) {
    let level_str = thinking_level_name(thinking);
    println!("{DIM}  thinking: {level_str}");
    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
}

// ── /history ─────────────────────────────────────────────────────────────

pub fn handle_history(agent: &Agent) {
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
}

// ── /search ──────────────────────────────────────────────────────────────

pub fn handle_search(agent: &Agent, input: &str) {
    if input == "/search" {
        println!("{DIM}  usage: /search <query>");
        println!("  Search conversation history for messages containing <query>.{RESET}\n");
        return;
    }
    let query = input.trim_start_matches("/search ").trim();
    if query.is_empty() {
        println!("{DIM}  usage: /search <query>{RESET}\n");
        return;
    }
    let messages = agent.messages();
    if messages.is_empty() {
        println!("{DIM}  (no messages to search){RESET}\n");
        return;
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
}

// ── /config ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn handle_config(
    provider: &str,
    model: &str,
    base_url: &Option<String>,
    thinking: ThinkingLevel,
    max_tokens: Option<u32>,
    max_turns: Option<usize>,
    temperature: Option<f32>,
    skills: &yoagent::skills::SkillSet,
    system_prompt: &str,
    mcp_count: u32,
    openapi_count: u32,
    agent: &Agent,
    continue_session: bool,
    cwd: &str,
) {
    println!("{DIM}  Configuration:");
    println!("    provider:   {provider}");
    println!("    model:      {model}");
    if let Some(ref url) = base_url {
        println!("    base_url:   {url}");
    }
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
    if openapi_count > 0 {
        println!("    openapi:    {openapi_count} spec(s)");
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
}

// ── /docs ────────────────────────────────────────────────────────────────

pub fn handle_docs(input: &str) {
    if input == "/docs" {
        println!("{DIM}  usage: /docs <crate> [item]");
        println!("  Look up docs.rs documentation for a Rust crate.");
        println!("  Examples: /docs serde, /docs tokio task{RESET}\n");
        return;
    }
    let args = input.trim_start_matches("/docs ").trim();
    if args.is_empty() {
        println!("{DIM}  usage: /docs <crate> [item]{RESET}\n");
        return;
    }
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
    let crate_name = parts[0].trim();
    let item_name = parts.get(1).map(|s| s.trim()).unwrap_or("");

    let (found, summary) = if item_name.is_empty() {
        docs::fetch_docs_summary(crate_name)
    } else {
        docs::fetch_docs_item(crate_name, item_name)
    };
    if found {
        let label = if item_name.is_empty() {
            crate_name.to_string()
        } else {
            format!("{crate_name}::{item_name}")
        };
        println!("{GREEN}  ✓ {label}{RESET}");
        println!("{DIM}{summary}{RESET}\n");
    } else {
        println!("{RED}  ✗ {summary}{RESET}\n");
    }
}

// ── /health ──────────────────────────────────────────────────────────────

/// Detected project type based on marker files in the working directory.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Make,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust (Cargo)"),
            ProjectType::Node => write!(f, "Node.js (npm)"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Make => write!(f, "Makefile"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect project type by checking for marker files in the given directory.
pub fn detect_project_type(dir: &std::path::Path) -> ProjectType {
    if dir.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if dir.join("package.json").exists() {
        ProjectType::Node
    } else if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("setup.cfg").exists()
    {
        ProjectType::Python
    } else if dir.join("go.mod").exists() {
        ProjectType::Go
    } else if dir.join("Makefile").exists() || dir.join("makefile").exists() {
        ProjectType::Make
    } else {
        ProjectType::Unknown
    }
}

/// Return health check commands for a given project type.
#[allow(clippy::vec_init_then_push, unused_mut)]
pub fn health_checks_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => {
            let mut checks = vec![("build", vec!["cargo", "build"])];
            #[cfg(not(test))]
            checks.push(("test", vec!["cargo", "test"]));
            checks.push((
                "clippy",
                vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
            ));
            checks.push(("fmt", vec!["cargo", "fmt", "--", "--check"]));
            checks
        }
        ProjectType::Node => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["npm", "test"]));
            checks.push(("lint", vec!["npx", "eslint", "."]));
            checks
        }
        ProjectType::Python => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["python", "-m", "pytest"]));
            checks.push(("lint", vec!["python", "-m", "flake8", "."]));
            checks.push(("typecheck", vec!["python", "-m", "mypy", "."]));
            checks
        }
        ProjectType::Go => {
            let mut checks = vec![("build", vec!["go", "build", "./..."])];
            #[cfg(not(test))]
            checks.push(("test", vec!["go", "test", "./..."]));
            checks.push(("vet", vec!["go", "vet", "./..."]));
            checks
        }
        ProjectType::Make => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["make", "test"]));
            checks
        }
        ProjectType::Unknown => vec![],
    }
}

/// Run health checks for a specific project type. Returns (name, passed, detail) tuples.
pub fn run_health_check_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

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

/// Run health checks and capture full error output for failures.
pub fn run_health_checks_full_output(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, String::new()));
            }
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                let mut full_output = String::new();
                if !stdout.is_empty() {
                    full_output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !full_output.is_empty() {
                        full_output.push('\n');
                    }
                    full_output.push_str(&stderr);
                }
                results.push((name, false, full_output));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Build a prompt describing health check failures for the AI to fix.
pub fn build_fix_prompt(failures: &[(&str, &str)]) -> String {
    if failures.is_empty() {
        return String::new();
    }
    let mut prompt = String::from(
        "Fix the following build/lint errors in this project. Read the relevant files, understand the errors, and apply fixes:\n\n",
    );
    for (name, output) in failures {
        prompt.push_str(&format!("## {name} errors:\n```\n{output}\n```\n\n"));
    }
    prompt.push_str(
        "After fixing, run the failing checks again to verify. Fix any remaining issues.",
    );
    prompt
}

pub fn handle_health() {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return;
    }
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_check_for_project(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return;
    }
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
}

/// Handle the /fix command. Returns Some(fix_prompt) if failures were sent to AI, None otherwise.
pub async fn handle_fix(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }
    println!("{DIM}  Detected project: {project_type}{RESET}");
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_checks_full_output(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return None;
    }
    for (name, passed, _) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        let status = if *passed { "ok" } else { "FAIL" };
        println!("  {icon} {name}: {status}");
    }
    let failures: Vec<(&str, &str)> = results
        .iter()
        .filter(|(_, passed, _)| !passed)
        .map(|(name, _, output)| (*name, output.as_str()))
        .collect();
    if failures.is_empty() {
        println!("\n{GREEN}  All checks passed — nothing to fix ✓{RESET}\n");
        return None;
    }
    let fail_count = failures.len();
    println!("\n{YELLOW}  Sending {fail_count} failure(s) to AI for fixing...{RESET}\n");
    let fix_prompt = build_fix_prompt(&failures);
    run_prompt(agent, &fix_prompt, session_total, model).await;
    auto_compact_if_needed(agent);
    Some(fix_prompt)
}

// ── /test ─────────────────────────────────────────────────────────────

/// Return the test command for a given project type.
pub fn test_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some(("cargo test", vec!["cargo", "test"])),
        ProjectType::Node => Some(("npm test", vec!["npm", "test"])),
        ProjectType::Python => Some(("python -m pytest", vec!["python", "-m", "pytest"])),
        ProjectType::Go => Some(("go test ./...", vec!["go", "test", "./..."])),
        ProjectType::Make => Some(("make test", vec!["make", "test"])),
        ProjectType::Unknown => None,
    }
}

/// Handle the /test command: auto-detect project type and run tests.
/// Returns a summary string suitable for AI context.
pub fn handle_test() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match test_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No test command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Tests passed ({elapsed}){RESET}\n");
                Some(format!("Tests passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Tests failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Tests FAILED (exit {code}, {elapsed}): {label}");
                // Include a preview of the error output for AI context
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /lint ──────────────────────────────────────────────────────────────

/// Return the lint command for a given project type.
pub fn lint_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some((
            "cargo clippy --all-targets -- -D warnings",
            vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
        )),
        ProjectType::Node => Some(("npx eslint .", vec!["npx", "eslint", "."])),
        ProjectType::Python => Some(("ruff check .", vec!["ruff", "check", "."])),
        ProjectType::Go => Some(("golangci-lint run", vec!["golangci-lint", "run"])),
        ProjectType::Make | ProjectType::Unknown => None,
    }
}

/// Handle the /lint command: auto-detect project type and run linter.
/// Returns a summary string suitable for AI context.
pub fn handle_lint() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match lint_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No lint command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Lint passed ({elapsed}){RESET}\n");
                Some(format!("Lint passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Lint failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Lint FAILED (exit {code}, {elapsed}): {label}");
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /tree ────────────────────────────────────────────────────────────────

/// Build a directory tree from `git ls-files`.
pub fn build_project_tree(max_depth: usize) -> String {
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
pub fn format_tree_from_paths(paths: &[String], max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut output = String::new();
    let mut printed_dirs: BTreeSet<String> = BTreeSet::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let depth = parts.len() - 1;

        for level in 0..parts.len().saturating_sub(1).min(max_depth) {
            let dir_path: String = parts[..=level].join("/");
            let dir_key = format!("{}/", dir_path);
            if printed_dirs.insert(dir_key) {
                let indent = "  ".repeat(level);
                let dir_name = parts[level];
                output.push_str(&format!("{indent}{dir_name}/\n"));
            }
        }

        if depth <= max_depth {
            let indent = "  ".repeat(depth.min(max_depth));
            let file_name = parts.last().unwrap_or(&"");
            output.push_str(&format!("{indent}{file_name}\n"));
        }
    }

    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

pub fn handle_tree(input: &str) {
    let arg = input.strip_prefix("/tree").unwrap_or("").trim();
    let max_depth = if arg.is_empty() {
        3
    } else {
        match arg.parse::<usize>() {
            Ok(d) => d,
            Err(_) => {
                println!("{DIM}  usage: /tree [depth]  (default depth: 3){RESET}\n");
                return;
            }
        }
    };
    let tree = build_project_tree(max_depth);
    println!("{DIM}{tree}{RESET}\n");
}

// ── /run ─────────────────────────────────────────────────────────────────

/// Run a shell command directly and print its output.
pub fn run_shell_command(cmd: &str) {
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

pub fn handle_run(input: &str) {
    let cmd = if input.starts_with("/run ") {
        input.trim_start_matches("/run ").trim()
    } else if input.starts_with('!') && input.len() > 1 {
        input[1..].trim()
    } else {
        ""
    };
    if cmd.is_empty() {
        println!("{DIM}  usage: /run <command>  or  !<command>{RESET}\n");
    } else {
        run_shell_command(cmd);
    }
}

pub fn handle_run_usage() {
    println!("{DIM}  usage: /run <command>  or  !<command>");
    println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
}

// ── /pr ──────────────────────────────────────────────────────────────────

/// Represents a parsed `/pr` subcommand.
#[derive(Debug, PartialEq)]
pub enum PrSubcommand {
    List,
    View(u32),
    Diff(u32),
    Comment(u32, String),
    Checkout(u32),
    Help,
}

/// Parse the argument string after `/pr` into a `PrSubcommand`.
pub fn parse_pr_args(arg: &str) -> PrSubcommand {
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

pub fn handle_pr(input: &str) {
    let arg = input.strip_prefix("/pr").unwrap_or("").trim();
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
            println!("{DIM}  usage: /pr                         List open pull requests");
            println!("         /pr <number>                View details of a specific PR");
            println!("         /pr <number> diff           Show the diff of a PR");
            println!("         /pr <number> comment <text> Add a comment to a PR");
            println!("         /pr <number> checkout       Checkout a PR locally{RESET}\n");
        }
    }
}

// ── /spawn ────────────────────────────────────────────────────────────────

/// Parse the task from a `/spawn <task>` input.
/// Returns None if no task is provided.
pub fn parse_spawn_task(input: &str) -> Option<String> {
    let task = input
        .strip_prefix("/spawn")
        .unwrap_or("")
        .trim()
        .to_string();
    if task.is_empty() {
        None
    } else {
        Some(task)
    }
}

/// Handle the /spawn command: create a fresh subagent, run a task, and return the result.
/// The subagent gets its own independent context window so complex tasks don't pollute
/// the main conversation.
/// Returns Some(context_msg) to be injected back into the main conversation, or None.
pub async fn handle_spawn(
    input: &str,
    agent_config: &crate::AgentConfig,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let task = match parse_spawn_task(input) {
        Some(t) => t,
        None => {
            println!("{DIM}  usage: /spawn <task>");
            println!("  Spawn a subagent with a fresh context to handle a task.");
            println!("  The result is summarized back into your main conversation.");
            println!("  Example: /spawn read src/main.rs and summarize the architecture{RESET}\n");
            return None;
        }
    };

    println!("{CYAN}  🐙 spawning subagent...{RESET}");
    println!(
        "{DIM}  task: {}{RESET}",
        crate::format::truncate_with_ellipsis(&task, 100)
    );

    // Build a fresh agent with the same config but independent context
    let mut sub_agent = agent_config.build_agent();

    // Run the task as a single prompt on the subagent
    let response = run_prompt(&mut sub_agent, &task, session_total, model).await;

    println!("\n{GREEN}  ✓ subagent completed{RESET}");
    println!("{DIM}  injecting result into main conversation...{RESET}\n");

    // Build a context message for the main agent summarizing what the subagent did
    let result_text = if response.trim().is_empty() {
        "(no output)".to_string()
    } else {
        response.trim().to_string()
    };

    let context_msg = format!(
        "A subagent just completed a task. Here is its result:\n\n**Task:** {task}\n\n**Result:**\n{result_text}"
    );

    Some(context_msg)
}

// ── /git ─────────────────────────────────────────────────────────────────

pub fn handle_git(input: &str) {
    let arg = input.strip_prefix("/git").unwrap_or("").trim();
    let subcmd = parse_git_args(arg);
    run_git_subcommand(&subcmd);
}
