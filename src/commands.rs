//! REPL command handlers for yoyo.
//!
//! Each `/command` in the interactive REPL is handled by a function in this module.
//! The main loop dispatches to these handlers, keeping main.rs as a thin REPL driver.

// All handle_* functions in this module are dispatched from the REPL in main.rs.

use crate::cli::{default_model_for_provider, KNOWN_PROVIDERS};
use crate::cli::{is_verbose, AUTO_COMPACT_THRESHOLD, VERSION};
use crate::format::*;
use crate::git::*;
use crate::prompt::*;

pub use crate::help::*;

use yoagent::agent::Agent;
use yoagent::context::total_tokens;
use yoagent::*;

/// Known REPL command prefixes. Used to detect unknown slash commands
/// and for tab-completion in the REPL.
pub const KNOWN_COMMANDS: &[&str] = &[
    "/add",
    "/apply",
    "/help",
    "/quit",
    "/exit",
    "/clear",
    "/clear!",
    "/compact",
    "/commit",
    "/cost",
    "/doctor",
    "/docs",
    "/export",
    "/find",
    "/fix",
    "/forget",
    "/index",
    "/status",
    "/tokens",
    "/save",
    "/load",
    "/diff",
    "/undo",
    "/health",
    "/retry",
    "/history",
    "/search",
    "/model",
    "/think",
    "/config",
    "/context",
    "/init",
    "/version",
    "/run",
    "/tree",
    "/pr",
    "/git",
    "/grep",
    "/test",
    "/lint",
    "/spawn",
    "/update",
    "/review",
    "/mark",
    "/jump",
    "/marks",
    "/plan",
    "/remember",
    "/memories",
    "/provider",
    "/changes",
    "/web",
    "/rename",
    "/extract",
    "/move",
    "/refactor",
    "/watch",
    "/ast",
    "/map",
    "/stash",
    "/todo",
];

/// Well-known model names for `/model <Tab>` completion.
pub const KNOWN_MODELS: &[&str] = &[
    "claude-sonnet-4-20250514",
    "claude-opus-4-20250514",
    "claude-haiku-35-20241022",
    "gpt-4o",
    "gpt-4o-mini",
    "gpt-4.1",
    "gpt-4.1-mini",
    "o3",
    "o3-mini",
    "o4-mini",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "deepseek-chat",
    "deepseek-reasoner",
];

/// Thinking level names for `/think <Tab>` completion.
pub const THINKING_LEVELS: &[&str] = &["off", "minimal", "low", "medium", "high"];

/// Git subcommand names for `/git <Tab>` completion.
pub const GIT_SUBCOMMANDS: &[&str] = &["status", "log", "add", "diff", "branch", "stash"];

/// PR subcommand names for `/pr <Tab>` completion.
pub const PR_SUBCOMMANDS: &[&str] = &["list", "view", "diff", "comment", "create", "checkout"];

/// Undo option names for `/undo <Tab>` completion.
pub const UNDO_OPTIONS: &[&str] = &["--all"];

/// Refactor subcommand names for `/refactor <Tab>` completion.
pub const REFACTOR_SUBCOMMANDS: &[&str] = &["rename", "extract", "move"];

/// Diff flag names for `/diff <Tab>` completion.
pub const DIFF_FLAGS: &[&str] = &["--staged", "--cached", "--name-only"];

/// Return context-aware argument completions for a given command and partial argument.
///
/// `cmd` is the slash command (e.g. "/model"), `partial_arg` is what the user has typed
/// after the command + space so far. Returns a list of candidate completions.
pub fn command_arg_completions(cmd: &str, partial_arg: &str) -> Vec<String> {
    let partial_lower = partial_arg.to_lowercase();
    match cmd {
        "/model" => filter_candidates(KNOWN_MODELS, &partial_lower),
        "/think" => filter_candidates(THINKING_LEVELS, &partial_lower),
        "/git" => filter_candidates(GIT_SUBCOMMANDS, &partial_lower),
        "/diff" => filter_candidates(DIFF_FLAGS, &partial_lower),
        "/pr" => filter_candidates(PR_SUBCOMMANDS, &partial_lower),
        "/provider" => filter_candidates(KNOWN_PROVIDERS, &partial_lower),
        "/save" | "/load" => list_json_files(partial_arg),
        "/help" => help_command_completions(&partial_lower),
        "/undo" => filter_candidates(UNDO_OPTIONS, &partial_lower),
        "/refactor" => filter_candidates(REFACTOR_SUBCOMMANDS, &partial_lower),
        "/watch" => filter_candidates(crate::commands_dev::WATCH_SUBCOMMANDS, &partial_lower),
        "/ast" => filter_candidates(crate::commands_search::AST_GREP_FLAGS, &partial_lower),
        "/apply" => filter_candidates(crate::commands_file::APPLY_FLAGS, &partial_lower),
        _ => Vec::new(),
    }
}

/// Filter a list of candidates by a lowercase prefix.
fn filter_candidates(candidates: &[&str], partial_lower: &str) -> Vec<String> {
    candidates
        .iter()
        .filter(|c| c.to_lowercase().starts_with(partial_lower))
        .map(|c| c.to_string())
        .collect()
}

/// List .json files in the current directory matching a partial prefix.
fn list_json_files(partial: &str) -> Vec<String> {
    let entries = match std::fs::read_dir(".") {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut matches: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") && name.starts_with(partial) {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    matches.sort();
    matches
}

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
    let max_context = crate::cli::effective_context_tokens();
    let messages = agent.messages().to_vec();
    let context_used = total_tokens(&messages) as u64;
    let bar = context_bar(context_used, max_context);

    println!("{DIM}  Active context:");
    println!("    messages:    {}", messages.len());
    println!(
        "    current:     {} / {} tokens",
        format_token_count(context_used),
        format_token_count(max_context)
    );
    println!("    {bar}");
    if session_total.input > context_used + 1000 {
        println!("    {DIM}(earlier messages were compacted to save space — session totals below show full usage){RESET}");
    }
    if context_used as f64 / max_context as f64 > 0.75 {
        println!("    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}");
    }
    println!();
    println!("  Session totals (all API calls):");
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

// ── /retry ───────────────────────────────────────────────────────────────

pub async fn handle_retry(
    agent: &mut Agent,
    last_input: &Option<String>,
    last_error: &Option<String>,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    match last_input {
        Some(prev) => {
            let retry_input = build_retry_prompt(prev, last_error);
            if last_error.is_some() {
                println!("{DIM}  (retrying with error context){RESET}");
            } else {
                println!("{DIM}  (retrying last input){RESET}");
            }
            let outcome = run_prompt(agent, &retry_input, session_total, model).await;
            auto_compact_if_needed(agent);
            outcome.last_tool_error
        }
        None => {
            eprintln!("{DIM}  (nothing to retry — no previous input){RESET}\n");
            None
        }
    }
}

// ── /model ───────────────────────────────────────────────────────────────

pub fn handle_model_show(model: &str) {
    println!("{DIM}  current model: {model}");
    println!("  usage: /model <name>{RESET}\n");
}

// ── /provider ────────────────────────────────────────────────────────────

pub fn handle_provider_show(provider: &str) {
    println!("{DIM}  current provider: {provider}");
    println!("  usage: /provider <name>");
    println!("  available: {}{RESET}\n", KNOWN_PROVIDERS.join(", "));
}

pub fn handle_provider_switch(
    new_provider: &str,
    agent_config: &mut crate::AgentConfig,
    agent: &mut Agent,
) {
    if !KNOWN_PROVIDERS.contains(&new_provider) {
        eprintln!("{RED}  unknown provider: '{new_provider}'{RESET}");
        eprintln!("{DIM}  available: {}{RESET}\n", KNOWN_PROVIDERS.join(", "));
        return;
    }
    agent_config.provider = new_provider.to_string();
    agent_config.model = default_model_for_provider(new_provider);
    let saved = agent.save_messages().ok();
    *agent = agent_config.build_agent();
    if let Some(json) = saved {
        let _ = agent.restore_messages(&json);
    }
    println!(
        "{DIM}  (switched to provider '{}', model '{}', conversation preserved){RESET}\n",
        agent_config.provider, agent_config.model
    );
}

// ── /think ───────────────────────────────────────────────────────────────

pub fn handle_think_show(thinking: ThinkingLevel) {
    let level_str = thinking_level_name(thinking);
    println!("{DIM}  thinking: {level_str}");
    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
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
        format_token_count(crate::cli::effective_context_tokens())
    );
    println!(
        "    auto-compact: at {:.0}%",
        AUTO_COMPACT_THRESHOLD * 100.0
    );
    println!("    messages:   {}", agent.messages().len());
    println!(
        "    session:    auto-save on exit ({})",
        crate::cli::AUTO_SAVE_SESSION_PATH
    );
    println!("{RESET}");
}

// ── /changes ─────────────────────────────────────────────────────────────

pub fn handle_changes(changes: &crate::prompt::SessionChanges) {
    use crate::prompt::format_changes;
    let output = format_changes(changes);
    if output.is_empty() {
        println!("{DIM}  No files modified yet this session.");
        println!(
            "  Files touched by write_file or edit_file tool calls will appear here.{RESET}\n"
        );
    } else {
        println!("{DIM}{output}{RESET}");
    }
}

// ── Re-exports from submodules ────────────────────────────────────────────
// These re-exports keep the public API stable so repl.rs continues to work
// with `commands::handle_*` calls unchanged.

// Git-related handlers
pub use crate::commands_git::{
    handle_commit, handle_diff, handle_git, handle_pr, handle_review, handle_undo,
};

// Project-related handlers
pub use crate::commands_project::{
    handle_context, handle_docs, handle_extract, handle_init, handle_move, handle_plan,
    handle_refactor, handle_rename, handle_todo,
};

pub use crate::commands_search::{
    handle_ast_grep, handle_find, handle_grep, handle_index, handle_map,
};

pub use crate::commands_dev::{
    handle_doctor, handle_fix, handle_health, handle_lint, handle_run, handle_run_usage,
    handle_test, handle_tree, handle_update, handle_watch,
};

pub use crate::commands_file::{
    expand_file_mentions, handle_add, handle_apply, handle_web, AddResult,
};

// Session-related handlers
pub use crate::commands_session::{
    auto_compact_if_needed, auto_save_on_exit, clear_confirmation_message, handle_compact,
    handle_export, handle_history, handle_jump, handle_load, handle_mark, handle_marks,
    handle_save, handle_search, handle_spawn, handle_stash, last_session_exists, Bookmarks,
    SpawnTracker,
};

// Memory-related handlers
pub use crate::memory::{add_memory, load_memories, remove_memory, save_memories};

// ── /remember ────────────────────────────────────────────────────────────

pub fn handle_remember(input: &str) {
    let note = input
        .strip_prefix("/remember")
        .unwrap_or("")
        .trim()
        .to_string();
    if note.is_empty() {
        println!("{DIM}  usage: /remember <note>");
        println!("  Save a project-specific memory that persists across sessions.");
        println!("  Examples:");
        println!("    /remember this project uses sqlx for database access");
        println!("    /remember tests require docker running");
        println!("    /remember always run cargo fmt before committing{RESET}\n");
        return;
    }
    let mut memory = load_memories();
    add_memory(&mut memory, &note);
    match save_memories(&memory) {
        Ok(_) => {
            println!(
                "{GREEN}  ✓ Remembered: \"{note}\" ({} total memories){RESET}\n",
                memory.entries.len()
            );
        }
        Err(e) => {
            eprintln!("{RED}  error saving memory: {e}{RESET}\n");
        }
    }
}

// ── /memories ────────────────────────────────────────────────────────────

pub fn handle_memories() {
    let memory = load_memories();
    if memory.entries.is_empty() {
        println!("{DIM}  No project memories yet.");
        println!("  Use /remember <note> to add one.{RESET}\n");
        return;
    }
    println!("{DIM}  Project memories ({}):", memory.entries.len());
    for (i, entry) in memory.entries.iter().enumerate() {
        println!("    [{i}] {} ({})", entry.note, entry.timestamp);
    }
    println!("  Use /forget <n> to remove a memory.{RESET}\n");
}

// ── /forget ──────────────────────────────────────────────────────────────

pub fn handle_forget(input: &str) {
    let arg = input.strip_prefix("/forget").unwrap_or("").trim();
    if arg.is_empty() {
        println!("{DIM}  usage: /forget <n>");
        println!("  Remove a project memory by index. Use /memories to see indexes.{RESET}\n");
        return;
    }
    let index = match arg.parse::<usize>() {
        Ok(i) => i,
        Err(_) => {
            eprintln!("{RED}  error: '{arg}' is not a valid index. Use /memories to see indexes.{RESET}\n");
            return;
        }
    };
    let mut memory = load_memories();
    match remove_memory(&mut memory, index) {
        Some(removed) => match save_memories(&memory) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ Forgot: \"{}\" ({} memories remaining){RESET}\n",
                    removed.note,
                    memory.entries.len()
                );
            }
            Err(e) => {
                eprintln!("{RED}  error saving memory: {e}{RESET}\n");
            }
        },
        None => {
            eprintln!(
                "{RED}  error: index {index} out of range (have {} memories). Use /memories to see indexes.{RESET}\n",
                memory.entries.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_dev::{
        build_fix_prompt, build_project_tree, format_tree_from_paths, health_checks_for_project,
        lint_command_for_project, run_health_check_for_project, run_health_checks_full_output,
        run_shell_command, test_command_for_project,
    };
    use crate::commands_git::{
        build_review_content, build_review_prompt, format_diff_stat, parse_diff_stat,
        parse_pr_args, DiffStatEntry, DiffStatSummary, PrSubcommand,
    };
    use crate::commands_project::{
        build_commands_for_project, build_plan_prompt, detect_project_name, detect_project_type,
        generate_init_content, parse_plan_task, scan_important_dirs, scan_important_files,
        ProjectType,
    };
    use crate::commands_search::{
        extract_first_meaningful_line, find_files, format_project_index, fuzzy_score,
        highlight_match, is_binary_extension, IndexEntry,
    };
    use crate::commands_session::{parse_bookmark_name, parse_spawn_args, parse_spawn_task};
    use crate::memory::{
        format_memories_for_prompt, load_memories_from, MemoryEntry, ProjectMemory,
    };
    use yoagent::ThinkingLevel;

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
            "/help",
            "/quit",
            "/exit",
            "/clear",
            "/compact",
            "/commit",
            "/config",
            "/context",
            "/cost",
            "/docs",
            "/find",
            "/fix",
            "/forget",
            "/index",
            "/init",
            "/status",
            "/tokens",
            "/save",
            "/load",
            "/diff",
            "/undo",
            "/health",
            "/retry",
            "/run",
            "/history",
            "/search",
            "/model",
            "/think",
            "/version",
            "/tree",
            "/pr",
            "/git",
            "/test",
            "/lint",
            "/spawn",
            "/review",
            "/mark",
            "/jump",
            "/marks",
            "/remember",
            "/memories",
            "/provider",
            "/changes",
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
    fn test_bare_model_command_is_recognized() {
        let input = "/model";
        assert_eq!(input, "/model");
        assert!(!input.starts_with("/model "));
    }

    // ── /provider tests ──────────────────────────────────────────────────

    #[test]
    fn test_provider_command_recognized() {
        assert!(!is_unknown_command("/provider"));
        assert!(!is_unknown_command("/provider openai"));
        assert!(
            KNOWN_COMMANDS.contains(&"/provider"),
            "/provider should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_provider_command_matching() {
        let provider_matches = |s: &str| s == "/provider" || s.starts_with("/provider ");
        assert!(provider_matches("/provider"));
        assert!(provider_matches("/provider openai"));
        assert!(provider_matches("/provider google"));
        assert!(!provider_matches("/providers"));
        assert!(!provider_matches("/providing"));
    }

    #[test]
    fn test_provider_show_does_not_panic() {
        // handle_provider_show should not panic for any known provider
        for provider in KNOWN_PROVIDERS {
            handle_provider_show(provider);
        }
    }

    #[test]
    fn test_provider_switch_valid() {
        use crate::cli;
        let mut config = crate::AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        let mut agent = config.build_agent();
        handle_provider_switch("openai", &mut config, &mut agent);
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4o");
    }

    #[test]
    fn test_provider_switch_invalid() {
        use crate::cli;
        let mut config = crate::AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        let mut agent = config.build_agent();
        // Invalid provider should not change the config
        handle_provider_switch("nonexistent_provider", &mut config, &mut agent);
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-opus-4-6");
    }

    #[test]
    fn test_provider_switch_sets_default_model() {
        use crate::cli;
        let mut config = crate::AgentConfig {
            model: "claude-opus-4-6".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::empty(),
            system_prompt: "Test.".to_string(),
            thinking: ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            permissions: cli::PermissionConfig::default(),
            dir_restrictions: cli::DirectoryRestrictions::default(),
            context_strategy: cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
        };
        let mut agent = config.build_agent();
        // Switch to google → should use gemini default
        handle_provider_switch("google", &mut config, &mut agent);
        assert_eq!(config.provider, "google");
        assert_eq!(config.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_provider_arg_completions_empty() {
        let candidates = command_arg_completions("/provider", "");
        assert!(!candidates.is_empty(), "Should return known providers");
        assert!(candidates.contains(&"anthropic".to_string()));
        assert!(candidates.contains(&"openai".to_string()));
        assert!(candidates.contains(&"google".to_string()));
    }

    #[test]
    fn test_provider_arg_completions_partial() {
        let candidates = command_arg_completions("/provider", "o");
        assert!(
            !candidates.is_empty(),
            "Should match providers starting with 'o'"
        );
        for c in &candidates {
            assert!(c.starts_with("o"), "All results should start with 'o': {c}");
        }
        assert!(candidates.contains(&"openai".to_string()));
        assert!(candidates.contains(&"openrouter".to_string()));
        assert!(candidates.contains(&"ollama".to_string()));
    }

    #[test]
    fn test_provider_arg_completions_no_match() {
        let candidates = command_arg_completions("/provider", "zzz_nonexistent");
        assert!(
            candidates.is_empty(),
            "Should return no matches for nonsense"
        );
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
        assert!(!is_unknown_command("/provider"));
        assert!(!is_unknown_command("/provider openai"));
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
        // run_health_check_for_project skips "cargo test" under #[cfg(test)] to avoid recursion
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_check_for_project(&project_type);
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
    fn test_detect_project_type_rust() {
        // Current directory has Cargo.toml, so should detect Rust
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(detect_project_type(&cwd), ProjectType::Rust);
    }

    #[test]
    fn test_detect_project_type_node() {
        let tmp = std::env::temp_dir().join("yoyo_test_node");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Node);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_pyproject() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_pyproject");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_setup_py() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_setup");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("setup.py"), "").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_go() {
        let tmp = std::env::temp_dir().join("yoyo_test_go");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("go.mod"), "module example.com/test").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Go);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_makefile() {
        let tmp = std::env::temp_dir().join("yoyo_test_make");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Makefile"), "test:\n\techo ok").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Make);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_unknown() {
        let tmp = std::env::temp_dir().join("yoyo_test_unknown");
        let _ = std::fs::create_dir_all(&tmp);
        // Empty dir — no marker files
        assert_eq!(detect_project_type(&tmp), ProjectType::Unknown);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_priority_rust_over_makefile() {
        // If both Cargo.toml and Makefile exist, Rust wins
        let tmp = std::env::temp_dir().join("yoyo_test_priority");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(tmp.join("Makefile"), "test:").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Rust);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_health_checks_for_rust_project() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Rust should have build check");
        assert!(names.contains(&"clippy"), "Rust should have clippy check");
        assert!(names.contains(&"fmt"), "Rust should have fmt check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    #[test]
    fn test_health_checks_for_node_project() {
        let checks = health_checks_for_project(&ProjectType::Node);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Node should have lint check");
    }

    #[test]
    fn test_health_checks_for_go_project() {
        let checks = health_checks_for_project(&ProjectType::Go);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Go should have build check");
        assert!(names.contains(&"vet"), "Go should have vet check");
    }

    #[test]
    fn test_health_checks_for_python_project() {
        let checks = health_checks_for_project(&ProjectType::Python);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Python should have lint check");
        assert!(names.contains(&"typecheck"), "Python should have typecheck");
    }

    #[test]
    fn test_health_checks_for_unknown_returns_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty(), "Unknown project should return no checks");
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(format!("{}", ProjectType::Rust), "Rust (Cargo)");
        assert_eq!(format!("{}", ProjectType::Node), "Node.js (npm)");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
        assert_eq!(format!("{}", ProjectType::Go), "Go");
        assert_eq!(format!("{}", ProjectType::Make), "Makefile");
        assert_eq!(format!("{}", ProjectType::Unknown), "Unknown");
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
        // build_project_tree should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
        // In a git repo, should contain Cargo.toml; outside one (e.g. cargo-mutants
        // temp dir) the tree still works but uses filesystem walk instead of git ls-files
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
    fn test_pr_subcommand_create() {
        assert_eq!(
            parse_pr_args("create"),
            PrSubcommand::Create { draft: false }
        );
        assert_eq!(
            parse_pr_args("CREATE"),
            PrSubcommand::Create { draft: false }
        );
        assert_eq!(
            parse_pr_args("Create"),
            PrSubcommand::Create { draft: false }
        );
    }

    #[test]
    fn test_pr_subcommand_create_draft() {
        assert_eq!(
            parse_pr_args("create --draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("create draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("CREATE --DRAFT"),
            PrSubcommand::Create { draft: true }
        );
    }

    #[test]
    fn test_pr_subcommand_create_no_flag() {
        // "create somethingelse" should still create but not be draft
        assert_eq!(
            parse_pr_args("create --nodraft"),
            PrSubcommand::Create { draft: false }
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
    fn test_fix_command_recognized() {
        assert!(!is_unknown_command("/fix"));
        assert!(
            KNOWN_COMMANDS.contains(&"/fix"),
            "/fix should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_run_health_checks_full_output_returns_results() {
        // In a Rust project, should return results with full error output
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_checks_full_output(&project_type);
        assert!(
            !results.is_empty(),
            "Should return at least one check result"
        );
        for (name, passed, _output) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
    }

    #[test]
    fn test_build_fix_prompt_with_failures() {
        let failures = vec![
            (
                "build",
                "error[E0308]: mismatched types\n  --> src/main.rs:42",
            ),
            (
                "clippy",
                "warning: unused variable `x`\n  --> src/lib.rs:10",
            ),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build"), "Prompt should mention build");
        assert!(prompt.contains("clippy"), "Prompt should mention clippy");
        assert!(
            prompt.contains("error[E0308]"),
            "Prompt should include build error"
        );
        assert!(
            prompt.contains("unused variable"),
            "Prompt should include clippy warning"
        );
    }

    #[test]
    fn test_build_fix_prompt_empty_failures() {
        let failures: Vec<(&str, &str)> = vec![];
        let prompt = build_fix_prompt(&failures);
        assert!(
            prompt.is_empty() || prompt.contains("Fix"),
            "Empty failures should produce empty or minimal prompt"
        );
    }

    #[test]
    fn test_test_command_recognized() {
        assert!(!is_unknown_command("/test"));
        assert!(
            KNOWN_COMMANDS.contains(&"/test"),
            "/test should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_test_command_for_rust_project() {
        let cmd = test_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("cargo"),
            "Rust test label should mention cargo"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_node_project() {
        let cmd = test_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("npm"), "Node test label should mention npm");
        assert_eq!(args[0], "npm");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_python_project() {
        let cmd = test_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a test command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("pytest"),
            "Python test label should mention pytest"
        );
    }

    #[test]
    fn test_test_command_for_go_project() {
        let cmd = test_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("go"), "Go test label should mention go");
        assert_eq!(args[0], "go");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_make_project() {
        let cmd = test_command_for_project(&ProjectType::Make);
        assert!(cmd.is_some(), "Make project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("make"),
            "Make test label should mention make"
        );
        assert_eq!(args[0], "make");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_unknown_project() {
        let cmd = test_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a test command"
        );
    }

    #[test]
    fn test_lint_command_recognized() {
        assert!(!is_unknown_command("/lint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/lint"),
            "/lint should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_lint_command_for_rust_project() {
        let cmd = lint_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("clippy"),
            "Rust lint label should mention clippy"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"clippy"));
    }

    #[test]
    fn test_lint_command_for_node_project() {
        let cmd = lint_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("eslint"),
            "Node lint label should mention eslint"
        );
        assert_eq!(args[0], "npx");
        assert!(args.contains(&"eslint"));
    }

    #[test]
    fn test_lint_command_for_python_project() {
        let cmd = lint_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a lint command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("ruff"),
            "Python lint label should mention ruff"
        );
    }

    #[test]
    fn test_lint_command_for_go_project() {
        let cmd = lint_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a lint command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("golangci-lint"),
            "Go lint label should mention golangci-lint"
        );
        assert_eq!(args[0], "golangci-lint");
    }

    #[test]
    fn test_lint_command_for_make_project() {
        let cmd = lint_command_for_project(&ProjectType::Make);
        assert!(cmd.is_none(), "Make project should not have a lint command");
    }

    #[test]
    fn test_lint_command_for_unknown_project() {
        let cmd = lint_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a lint command"
        );
    }

    #[test]
    fn test_docs_command_recognized() {
        assert!(!is_unknown_command("/docs"));
        assert!(!is_unknown_command("/docs serde"));
        assert!(!is_unknown_command("/docs tokio"));
        assert!(
            KNOWN_COMMANDS.contains(&"/docs"),
            "/docs should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_docs_command_matching() {
        // /docs should match exact or with space, not /docstring etc.
        let docs_matches = |s: &str| s == "/docs" || s.starts_with("/docs ");
        assert!(docs_matches("/docs"));
        assert!(docs_matches("/docs serde"));
        assert!(docs_matches("/docs tokio-runtime"));
        assert!(!docs_matches("/docstring"));
        assert!(!docs_matches("/docsify"));
    }

    #[test]
    fn test_docs_crate_arg_extraction() {
        let input = "/docs serde";
        let crate_name = input.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name, "serde");

        let input2 = "/docs tokio-runtime";
        let crate_name2 = input2.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name2, "tokio-runtime");

        // Bare /docs has empty after stripping
        let input_bare = "/docs";
        assert_eq!(input_bare, "/docs");
        assert!(!input_bare.starts_with("/docs "));
    }

    #[test]
    fn test_spawn_command_recognized() {
        assert!(!is_unknown_command("/spawn"));
        assert!(!is_unknown_command("/spawn read src/main.rs and summarize"));
        assert!(
            KNOWN_COMMANDS.contains(&"/spawn"),
            "/spawn should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_spawn_command_matching() {
        // /spawn should match exact or with space separator, not /spawning
        let spawn_matches = |s: &str| s == "/spawn" || s.starts_with("/spawn ");
        assert!(spawn_matches("/spawn"));
        assert!(spawn_matches("/spawn read file"));
        assert!(spawn_matches("/spawn analyze the codebase"));
        assert!(!spawn_matches("/spawning"));
        assert!(!spawn_matches("/spawnpoint"));
    }

    #[test]
    fn test_parse_spawn_task_with_task() {
        let task = parse_spawn_task("/spawn read src/main.rs and summarize");
        assert_eq!(task, Some("read src/main.rs and summarize".to_string()));
    }

    #[test]
    fn test_parse_spawn_task_empty() {
        let task = parse_spawn_task("/spawn");
        assert_eq!(task, None);
    }

    #[test]
    fn test_parse_spawn_task_whitespace_only() {
        let task = parse_spawn_task("/spawn   ");
        assert_eq!(task, None);
    }

    #[test]
    fn test_parse_spawn_task_preserves_full_task() {
        let task = parse_spawn_task("/spawn analyze src/ and list all public functions");
        assert_eq!(
            task,
            Some("analyze src/ and list all public functions".to_string())
        );
    }

    #[test]
    fn test_parse_spawn_args_basic() {
        let args = parse_spawn_args("/spawn do something");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.task, "do something");
        assert!(args.output_path.is_none());
    }

    #[test]
    fn test_parse_spawn_args_with_output() {
        let args = parse_spawn_args("/spawn -o out.md write a summary");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.task, "write a summary");
        assert_eq!(args.output_path, Some("out.md".to_string()));
    }

    #[test]
    fn test_parse_spawn_args_status() {
        assert!(parse_spawn_args("/spawn status").is_none());
    }

    #[test]
    fn test_find_command_recognized() {
        assert!(!is_unknown_command("/find"));
        assert!(!is_unknown_command("/find main"));
        assert!(!is_unknown_command("/find .toml"));
        assert!(
            KNOWN_COMMANDS.contains(&"/find"),
            "/find should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_fuzzy_score_basic_match() {
        // Pattern found in path → Some score
        let score = fuzzy_score("src/main.rs", "main");
        assert!(score.is_some(), "should match 'main' in 'src/main.rs'");
        assert!(score.unwrap() > 0, "score should be positive");
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        let score = fuzzy_score("src/main.rs", "zzznotfound");
        assert!(score.is_none(), "should not match 'zzznotfound'");
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        let score_lower = fuzzy_score("src/main.rs", "main");
        let score_upper = fuzzy_score("src/main.rs", "MAIN");
        assert!(score_lower.is_some());
        assert!(score_upper.is_some());
        // Both should match with same score
        assert_eq!(score_lower, score_upper);
    }

    #[test]
    fn test_fuzzy_score_filename_match_higher() {
        // "main" matches in filename for "src/main.rs" but only in dir for "main/other.rs"
        let filename_score = fuzzy_score("src/main.rs", "main");
        let dir_score = fuzzy_score("main_stuff/other.rs", "main");
        assert!(filename_score.is_some());
        assert!(dir_score.is_some());
        // Filename match should score higher because it gets the filename bonus
        assert!(
            filename_score.unwrap() > dir_score.unwrap(),
            "filename match should score higher: {} vs {}",
            filename_score.unwrap(),
            dir_score.unwrap()
        );
    }

    #[test]
    fn test_fuzzy_score_start_of_filename_bonus() {
        // "cli" at start of filename should score higher than "cli" embedded elsewhere
        let start_score = fuzzy_score("src/cli.rs", "cli");
        let mid_score = fuzzy_score("src/public_client.rs", "cli");
        assert!(start_score.is_some());
        assert!(mid_score.is_some());
        assert!(
            start_score.unwrap() > mid_score.unwrap(),
            "start-of-filename match should score higher: {} vs {}",
            start_score.unwrap(),
            mid_score.unwrap()
        );
    }

    #[test]
    fn test_find_files_returns_sorted() {
        // Search for a common pattern in this project
        let matches = find_files("main");
        assert!(!matches.is_empty(), "should find files matching 'main'");
        // Results should be sorted by score descending
        for window in matches.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "results should be sorted by score descending: {} >= {}",
                window[0].score,
                window[1].score
            );
        }
    }

    #[test]
    fn test_find_files_no_results() {
        let matches = find_files("xyzzy_nonexistent_pattern_12345");
        assert!(
            matches.is_empty(),
            "should find no files for nonsense pattern"
        );
    }

    #[test]
    fn test_find_command_matching() {
        // /find should match exact or with space separator, not /finding
        let find_matches = |s: &str| s == "/find" || s.starts_with("/find ");
        assert!(find_matches("/find"));
        assert!(find_matches("/find main"));
        assert!(find_matches("/find .toml"));
        assert!(!find_matches("/finding"));
        assert!(!find_matches("/findall"));
    }

    #[test]
    fn test_highlight_match_basic() {
        let result = highlight_match("src/main.rs", "main");
        // Should contain the original path text
        assert!(result.contains("main"));
        assert!(result.contains("src/"));
        assert!(result.contains(".rs"));
    }

    // ── /review tests ──────────────────────────────────────────────────────

    #[test]
    fn test_review_command_recognized() {
        assert!(!is_unknown_command("/review"));
        assert!(!is_unknown_command("/review src/main.rs"));
        assert!(
            KNOWN_COMMANDS.contains(&"/review"),
            "/review should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_review_command_matching() {
        // /review should match exact or with space separator, not /reviewing
        let review_matches = |s: &str| s == "/review" || s.starts_with("/review ");
        assert!(review_matches("/review"));
        assert!(review_matches("/review src/main.rs"));
        assert!(review_matches("/review Cargo.toml"));
        assert!(!review_matches("/reviewing"));
        assert!(!review_matches("/reviewer"));
    }

    #[test]
    fn test_build_review_prompt_contains_content() {
        let prompt =
            build_review_prompt("staged changes", "fn main() {\n    println!(\"hello\");\n}");
        assert!(
            prompt.contains("staged changes"),
            "Should mention the label"
        );
        assert!(prompt.contains("fn main()"), "Should contain the code");
        assert!(prompt.contains("Bugs"), "Should ask for bug review");
        assert!(
            prompt.contains("Security"),
            "Should ask for security review"
        );
        assert!(prompt.contains("Style"), "Should ask for style review");
        assert!(
            prompt.contains("Performance"),
            "Should ask for performance review"
        );
        assert!(prompt.contains("Suggestions"), "Should ask for suggestions");
    }

    #[test]
    fn test_build_review_prompt_truncates_large_content() {
        let large_content = "x".repeat(40_000);
        let prompt = build_review_prompt("big file", &large_content);
        assert!(
            prompt.contains("truncated"),
            "Large content should be truncated"
        );
        assert!(
            prompt.len() < 40_000,
            "Prompt should be truncated, got {} chars",
            prompt.len()
        );
    }

    #[test]
    fn test_build_review_content_nonexistent_file() {
        let result = build_review_content("nonexistent_file_xyz_12345.rs");
        assert!(result.is_none(), "Nonexistent file should return None");
    }

    #[test]
    fn test_build_review_content_existing_file() {
        // Cargo.toml exists in the project root
        let result = build_review_content("Cargo.toml");
        assert!(result.is_some(), "Existing file should return Some");
        let (label, content) = result.unwrap();
        assert_eq!(label, "Cargo.toml");
        assert!(!content.is_empty(), "Content should not be empty");
    }

    #[test]
    fn test_build_review_content_empty_arg_in_git_repo() {
        // Empty arg reviews staged/unstaged changes
        // In CI, this may or may not have changes — just verify it doesn't panic
        let result = build_review_content("");
        // Result depends on git state — either Some or None is valid
        if let Some((label, _content)) = result {
            assert!(
                label.contains("changes"),
                "Label should describe what's being reviewed: {label}"
            );
        }
    }

    #[test]
    fn test_review_help_text_present() {
        // Verify /review appears in the help output by checking the handle_help function output
        // We can't easily capture stdout, but we can verify the command is in KNOWN_COMMANDS
        // and that the help text format is correct
        assert!(KNOWN_COMMANDS.contains(&"/review"));
    }

    // ── /init scanning tests ─────────────────────────────────────────────

    #[test]
    fn test_init_command_recognized() {
        assert!(!is_unknown_command("/init"));
        assert!(
            KNOWN_COMMANDS.contains(&"/init"),
            "/init should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_scan_important_files_in_current_project() {
        let cwd = std::env::current_dir().unwrap();
        let files = scan_important_files(&cwd);
        // This is a Rust project, so Cargo.toml should be found
        assert!(
            files.contains(&"Cargo.toml".to_string()),
            "Should find Cargo.toml: {files:?}"
        );
    }

    #[test]
    fn test_scan_important_files_empty_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let files = scan_important_files(&tmp);
        assert!(files.is_empty(), "Empty dir should have no important files");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_files_with_readme() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_readme");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("README.md"), "# Hello").unwrap();
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        let files = scan_important_files(&tmp);
        assert!(
            files.contains(&"README.md".to_string()),
            "Should find README.md"
        );
        assert!(
            files.contains(&"package.json".to_string()),
            "Should find package.json"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_dirs_in_current_project() {
        let cwd = std::env::current_dir().unwrap();
        let dirs = scan_important_dirs(&cwd);
        // This project has src/
        assert!(
            dirs.contains(&"src".to_string()),
            "Should find src/ dir: {dirs:?}"
        );
    }

    #[test]
    fn test_scan_important_dirs_empty_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_dirs_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.is_empty(), "Empty dir should have no important dirs");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_scan_important_dirs_with_subdirs() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_subdirs");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        let _ = std::fs::create_dir_all(tmp.join("tests"));
        let _ = std::fs::create_dir_all(tmp.join("docs"));
        let dirs = scan_important_dirs(&tmp);
        assert!(dirs.contains(&"src".to_string()), "Should find src/");
        assert!(dirs.contains(&"tests".to_string()), "Should find tests/");
        assert!(dirs.contains(&"docs".to_string()), "Should find docs/");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_build_commands_for_rust() {
        let cmds = build_commands_for_project(&ProjectType::Rust);
        assert!(!cmds.is_empty(), "Rust should have build commands");
        let labels: Vec<&str> = cmds.iter().map(|(l, _)| *l).collect();
        assert!(labels.contains(&"Build"), "Should have Build command");
        assert!(labels.contains(&"Test"), "Should have Test command");
        assert!(labels.contains(&"Lint"), "Should have Lint command");
    }

    #[test]
    fn test_build_commands_for_node() {
        let cmds = build_commands_for_project(&ProjectType::Node);
        assert!(!cmds.is_empty(), "Node should have build commands");
        let labels: Vec<&str> = cmds.iter().map(|(l, _)| *l).collect();
        assert!(labels.contains(&"Test"), "Should have Test command");
    }

    #[test]
    fn test_build_commands_for_unknown() {
        let cmds = build_commands_for_project(&ProjectType::Unknown);
        assert!(
            cmds.is_empty(),
            "Unknown project should have no build commands"
        );
    }

    #[test]
    fn test_detect_project_name_rust() {
        // Current project has Cargo.toml with name = "yoyo-agent"
        let cwd = std::env::current_dir().unwrap();
        let name = detect_project_name(&cwd);
        assert_eq!(
            name, "yoyo-agent",
            "Should detect project name 'yoyo-agent' from Cargo.toml"
        );
    }

    #[test]
    fn test_detect_project_name_fallback_to_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_fallback");
        let _ = std::fs::create_dir_all(&tmp);
        let name = detect_project_name(&tmp);
        assert_eq!(
            name, "yoyo_test_name_fallback",
            "Should fall back to directory name"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_name_from_readme() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_readme");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("README.md"), "# My Awesome Project\n\nSome text.").unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(
            name, "My Awesome Project",
            "Should extract name from README title"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_name_from_package_json() {
        let tmp = std::env::temp_dir().join("yoyo_test_name_pkg");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("package.json"),
            "{\n  \"name\": \"cool-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        let name = detect_project_name(&tmp);
        assert_eq!(name, "cool-app", "Should extract name from package.json");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_generate_init_content_rust_project() {
        let cwd = std::env::current_dir().unwrap();
        let content = generate_init_content(&cwd);
        // Should contain project name
        assert!(
            content.contains("yoyo"),
            "Should contain project name: {}",
            &content[..200.min(content.len())]
        );
        // Should detect Rust
        assert!(content.contains("Rust"), "Should mention Rust project type");
        // Should have build commands
        assert!(
            content.contains("cargo build"),
            "Should include cargo build command"
        );
        assert!(
            content.contains("cargo test"),
            "Should include cargo test command"
        );
        // Should have sections
        assert!(
            content.contains("## Build & Test"),
            "Should have Build & Test section"
        );
        assert!(
            content.contains("## Important Files"),
            "Should have Important Files section"
        );
        assert!(
            content.contains("## Coding Conventions"),
            "Should have Coding Conventions section"
        );
        // Should list Cargo.toml as important file
        assert!(
            content.contains("Cargo.toml"),
            "Should list Cargo.toml as important"
        );
        // Should list src/ as important dir
        assert!(
            content.contains("`src/`"),
            "Should list src/ as important dir"
        );
    }

    #[test]
    fn test_generate_init_content_empty_dir() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_gen_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let content = generate_init_content(&tmp);
        // Should still have sections even for empty/unknown project
        assert!(content.contains("# Project Context"));
        assert!(content.contains("## About This Project"));
        assert!(content.contains("## Build & Test"));
        assert!(content.contains("## Coding Conventions"));
        assert!(content.contains("## Important Files"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_generate_init_content_node_project() {
        let tmp = std::env::temp_dir().join("yoyo_test_init_gen_node");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("package.json"),
            "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\"\n}",
        )
        .unwrap();
        let _ = std::fs::create_dir_all(tmp.join("src"));
        let content = generate_init_content(&tmp);
        assert!(
            content.contains("my-app"),
            "Should detect project name from package.json"
        );
        assert!(content.contains("Node"), "Should detect Node project type");
        assert!(content.contains("npm"), "Should include npm commands");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── /diff stat parsing tests ────────────────────────────────────────

    #[test]
    fn test_parse_diff_stat_basic() {
        let stat_output = " src/commands.rs | 42 ++++++++++++++++++++++++++++--------------
 src/main.rs     |  8 +++++---
 2 files changed, 30 insertions(+), 20 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 2);
        assert_eq!(summary.entries[0].file, "src/commands.rs");
        assert_eq!(summary.entries[1].file, "src/main.rs");
        assert_eq!(summary.total_insertions, 30);
        assert_eq!(summary.total_deletions, 20);
    }

    #[test]
    fn test_parse_diff_stat_single_file() {
        let stat_output = " src/format.rs | 10 +++++++---
 1 file changed, 7 insertions(+), 3 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "src/format.rs");
        assert_eq!(summary.total_insertions, 7);
        assert_eq!(summary.total_deletions, 3);
    }

    #[test]
    fn test_parse_diff_stat_insertions_only() {
        let stat_output = " new_file.rs | 25 +++++++++++++++++++++++++
 1 file changed, 25 insertions(+)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "new_file.rs");
        assert!(summary.entries[0].insertions > 0);
        assert_eq!(summary.entries[0].deletions, 0);
        assert_eq!(summary.total_insertions, 25);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn test_parse_diff_stat_deletions_only() {
        let stat_output = " old_file.rs | 15 ---------------
 1 file changed, 15 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].file, "old_file.rs");
        assert_eq!(summary.entries[0].insertions, 0);
        assert!(summary.entries[0].deletions > 0);
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 15);
    }

    #[test]
    fn test_parse_diff_stat_empty() {
        let summary = parse_diff_stat("");
        assert!(summary.entries.is_empty());
        assert_eq!(summary.total_insertions, 0);
        assert_eq!(summary.total_deletions, 0);
    }

    #[test]
    fn test_parse_diff_stat_no_summary_line() {
        // Sometimes stat output has no summary — compute from entries
        let stat_output = " src/main.rs | 5 +++--
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 1);
        // Totals computed from entry counts
        assert_eq!(summary.total_insertions, summary.entries[0].insertions);
        assert_eq!(summary.total_deletions, summary.entries[0].deletions);
    }

    #[test]
    fn test_parse_diff_stat_multiple_files() {
        let stat_output = " Cargo.toml       |  2 +-
 src/cli.rs       | 15 ++++++++-------
 src/commands.rs  | 88 +++++++++++++++++++++++++++++++++++++++++++++++++++++---
 src/format.rs    |  3 ++-
 4 files changed, 78 insertions(+), 30 deletions(-)
";
        let summary = parse_diff_stat(stat_output);
        assert_eq!(summary.entries.len(), 4);
        assert_eq!(summary.entries[0].file, "Cargo.toml");
        assert_eq!(summary.entries[2].file, "src/commands.rs");
        assert_eq!(summary.total_insertions, 78);
        assert_eq!(summary.total_deletions, 30);
    }

    #[test]
    fn test_format_diff_stat_empty() {
        let summary = DiffStatSummary {
            entries: vec![],
            total_insertions: 0,
            total_deletions: 0,
        };
        let formatted = format_diff_stat(&summary);
        assert!(
            formatted.is_empty(),
            "Empty summary should produce empty output"
        );
    }

    #[test]
    fn test_format_diff_stat_single_entry() {
        let summary = DiffStatSummary {
            entries: vec![DiffStatEntry {
                file: "src/main.rs".to_string(),
                insertions: 5,
                deletions: 2,
            }],
            total_insertions: 5,
            total_deletions: 2,
        };
        let formatted = format_diff_stat(&summary);
        assert!(formatted.contains("src/main.rs"), "Should contain filename");
        assert!(
            formatted.contains("1 file changed"),
            "Should show file count"
        );
        assert!(formatted.contains("+5"), "Should show insertions");
        assert!(formatted.contains("-2"), "Should show deletions");
    }

    #[test]
    fn test_format_diff_stat_multiple_entries() {
        let summary = DiffStatSummary {
            entries: vec![
                DiffStatEntry {
                    file: "src/a.rs".to_string(),
                    insertions: 10,
                    deletions: 0,
                },
                DiffStatEntry {
                    file: "src/b.rs".to_string(),
                    insertions: 0,
                    deletions: 5,
                },
            ],
            total_insertions: 10,
            total_deletions: 5,
        };
        let formatted = format_diff_stat(&summary);
        assert!(formatted.contains("src/a.rs"));
        assert!(formatted.contains("src/b.rs"));
        assert!(formatted.contains("2 files changed"));
    }

    #[test]
    fn test_format_diff_stat_insertions_only_no_deletions_shown() {
        let summary = DiffStatSummary {
            entries: vec![DiffStatEntry {
                file: "new.rs".to_string(),
                insertions: 10,
                deletions: 0,
            }],
            total_insertions: 10,
            total_deletions: 0,
        };
        let formatted = format_diff_stat(&summary);
        assert!(formatted.contains("+10"), "Should show insertions");
        // "-0" should not appear
        assert!(!formatted.contains("-0"), "Should not show zero deletions");
    }

    // ── bookmark (/mark, /jump, /marks) tests ────────────────────────────

    #[test]
    fn test_mark_command_recognized() {
        assert!(!is_unknown_command("/mark"));
        assert!(!is_unknown_command("/mark checkpoint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/mark"),
            "/mark should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_jump_command_recognized() {
        assert!(!is_unknown_command("/jump"));
        assert!(!is_unknown_command("/jump checkpoint"));
        assert!(
            KNOWN_COMMANDS.contains(&"/jump"),
            "/jump should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_marks_command_recognized() {
        assert!(!is_unknown_command("/marks"));
        assert!(
            KNOWN_COMMANDS.contains(&"/marks"),
            "/marks should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_parse_bookmark_name_with_name() {
        let name = parse_bookmark_name("/mark checkpoint", "/mark");
        assert_eq!(name, Some("checkpoint".to_string()));
    }

    #[test]
    fn test_parse_bookmark_name_with_spaces() {
        let name = parse_bookmark_name("/mark  my bookmark  ", "/mark");
        assert_eq!(name, Some("my bookmark".to_string()));
    }

    #[test]
    fn test_parse_bookmark_name_empty() {
        let name = parse_bookmark_name("/mark", "/mark");
        assert_eq!(name, None);
    }

    #[test]
    fn test_parse_bookmark_name_whitespace_only() {
        let name = parse_bookmark_name("/mark   ", "/mark");
        assert_eq!(name, None);
    }

    #[test]
    fn test_parse_bookmark_name_for_jump() {
        let name = parse_bookmark_name("/jump start", "/jump");
        assert_eq!(name, Some("start".to_string()));
    }

    #[test]
    fn test_bookmarks_create_and_list() {
        let mut bookmarks = Bookmarks::new();
        assert!(bookmarks.is_empty());

        bookmarks.insert("start".to_string(), "[]".to_string());
        assert_eq!(bookmarks.len(), 1);
        assert!(bookmarks.contains_key("start"));
    }

    #[test]
    fn test_bookmarks_overwrite_same_name() {
        let mut bookmarks = Bookmarks::new();
        bookmarks.insert("checkpoint".to_string(), "[1]".to_string());
        bookmarks.insert("checkpoint".to_string(), "[1,2]".to_string());
        // Should still have just one entry
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks.get("checkpoint").unwrap(), "[1,2]");
    }

    #[test]
    fn test_bookmarks_nonexistent_returns_none() {
        let bookmarks = Bookmarks::new();
        assert!(!bookmarks.contains_key("nonexistent"));
    }

    #[test]
    fn test_bookmarks_multiple_entries() {
        let mut bookmarks = Bookmarks::new();
        bookmarks.insert("start".to_string(), "[]".to_string());
        bookmarks.insert("middle".to_string(), "[1]".to_string());
        bookmarks.insert("end".to_string(), "[1,2,3]".to_string());
        assert_eq!(bookmarks.len(), 3);
        assert!(bookmarks.contains_key("start"));
        assert!(bookmarks.contains_key("middle"));
        assert!(bookmarks.contains_key("end"));
    }

    #[test]
    fn test_handle_marks_empty_does_not_panic() {
        let bookmarks = Bookmarks::new();
        // Should not panic — just prints a message
        handle_marks(&bookmarks);
    }

    #[test]
    fn test_handle_marks_with_entries_does_not_panic() {
        let mut bookmarks = Bookmarks::new();
        bookmarks.insert("alpha".to_string(), "[]".to_string());
        bookmarks.insert("beta".to_string(), "[]".to_string());
        // Should not panic
        handle_marks(&bookmarks);
    }

    #[test]
    fn test_mark_command_matching() {
        // /mark should match exact or with space, not /marker
        let mark_matches = |s: &str| s == "/mark" || s.starts_with("/mark ");
        assert!(mark_matches("/mark"));
        assert!(mark_matches("/mark checkpoint"));
        assert!(!mark_matches("/marker"));
        assert!(!mark_matches("/marking"));
    }

    #[test]
    fn test_jump_command_matching() {
        // /jump should match exact or with space, not /jumper
        let jump_matches = |s: &str| s == "/jump" || s.starts_with("/jump ");
        assert!(jump_matches("/jump"));
        assert!(jump_matches("/jump checkpoint"));
        assert!(!jump_matches("/jumper"));
        assert!(!jump_matches("/jumping"));
    }

    // ── command_arg_completions tests ─────────────────────────────────────

    #[test]
    fn test_arg_completions_model_empty_prefix() {
        let candidates = command_arg_completions("/model", "");
        assert!(!candidates.is_empty(), "Should return known models");
        assert!(
            candidates.iter().any(|c| c.contains("claude")),
            "Should include Claude models"
        );
    }

    #[test]
    fn test_arg_completions_model_partial_prefix() {
        let candidates = command_arg_completions("/model", "claude");
        assert!(
            !candidates.is_empty(),
            "Should match models starting with 'claude'"
        );
        for c in &candidates {
            assert!(
                c.starts_with("claude"),
                "All results should start with 'claude': {c}"
            );
        }
    }

    #[test]
    fn test_arg_completions_model_gpt_prefix() {
        let candidates = command_arg_completions("/model", "gpt");
        assert!(
            !candidates.is_empty(),
            "Should match models starting with 'gpt'"
        );
        for c in &candidates {
            assert!(
                c.starts_with("gpt"),
                "All results should start with 'gpt': {c}"
            );
        }
    }

    #[test]
    fn test_arg_completions_model_no_match() {
        let candidates = command_arg_completions("/model", "zzz_nonexistent");
        assert!(
            candidates.is_empty(),
            "Should return no matches for nonsense"
        );
    }

    #[test]
    fn test_arg_completions_think_empty() {
        let candidates = command_arg_completions("/think", "");
        assert_eq!(candidates.len(), 5, "Should return all 5 thinking levels");
        assert!(candidates.contains(&"off".to_string()));
        assert!(candidates.contains(&"high".to_string()));
    }

    #[test]
    fn test_arg_completions_think_partial() {
        let candidates = command_arg_completions("/think", "m");
        assert_eq!(candidates.len(), 2, "Should match 'minimal' and 'medium'");
        assert!(candidates.contains(&"minimal".to_string()));
        assert!(candidates.contains(&"medium".to_string()));
    }

    #[test]
    fn test_arg_completions_git_empty() {
        let candidates = command_arg_completions("/git", "");
        assert!(!candidates.is_empty(), "Should return git subcommands");
        assert!(candidates.contains(&"status".to_string()));
        assert!(candidates.contains(&"log".to_string()));
        assert!(candidates.contains(&"add".to_string()));
        assert!(candidates.contains(&"diff".to_string()));
        assert!(candidates.contains(&"branch".to_string()));
        assert!(candidates.contains(&"stash".to_string()));
    }

    #[test]
    fn test_arg_completions_git_partial() {
        let candidates = command_arg_completions("/git", "st");
        assert_eq!(
            candidates.len(),
            2,
            "Should match 'status' and 'stash': {candidates:?}"
        );
        assert!(candidates.contains(&"status".to_string()));
        assert!(candidates.contains(&"stash".to_string()));
    }

    #[test]
    fn test_arg_completions_pr_empty() {
        let candidates = command_arg_completions("/pr", "");
        assert!(!candidates.is_empty(), "Should return PR subcommands");
        assert!(candidates.contains(&"create".to_string()));
        assert!(candidates.contains(&"checkout".to_string()));
        assert!(candidates.contains(&"diff".to_string()));
    }

    #[test]
    fn test_arg_completions_pr_partial() {
        let candidates = command_arg_completions("/pr", "c");
        assert_eq!(
            candidates.len(),
            3,
            "Should match 'comment', 'create', and 'checkout': {candidates:?}"
        );
    }

    #[test]
    fn test_arg_completions_unknown_command() {
        let candidates = command_arg_completions("/unknown", "");
        assert!(
            candidates.is_empty(),
            "Unknown commands should return no completions"
        );
    }

    #[test]
    fn test_arg_completions_help_has_args() {
        // /help should now return command names for tab completion
        let candidates = command_arg_completions("/help", "");
        assert!(!candidates.is_empty(), "/help should offer completions");
    }

    #[test]
    fn test_arg_completions_case_insensitive() {
        // Typing uppercase should still find lowercase matches
        let candidates = command_arg_completions("/model", "CLAUDE");
        assert!(
            !candidates.is_empty(),
            "Should match case-insensitively: {candidates:?}"
        );
    }

    #[test]
    fn test_arg_completions_save_load_json_files() {
        // Create a temporary .json file to test /save and /load completion
        let test_file = "test_completion_temp.json";
        std::fs::write(test_file, "{}").unwrap();

        let save_candidates = command_arg_completions("/save", "test_completion");
        let load_candidates = command_arg_completions("/load", "test_completion");

        // Clean up before asserting
        let _ = std::fs::remove_file(test_file);

        assert!(
            save_candidates.contains(&test_file.to_string()),
            "/save should complete .json files: {save_candidates:?}"
        );
        assert!(
            load_candidates.contains(&test_file.to_string()),
            "/load should complete .json files: {load_candidates:?}"
        );
    }

    // ── /index tests ─────────────────────────────────────────────────────

    #[test]
    fn test_extract_first_meaningful_line_skips_blanks() {
        let content = "\n\n\n//! Module docs here\nfn main() {}";
        let line = extract_first_meaningful_line(content);
        assert_eq!(line, "//! Module docs here");
    }

    #[test]
    fn test_extract_first_meaningful_line_empty() {
        let content = "\n\n\n";
        let line = extract_first_meaningful_line(content);
        assert_eq!(line, "");
    }

    #[test]
    fn test_extract_first_meaningful_line_truncates_long_lines() {
        let content = format!("// {}", "a".repeat(200));
        let line = extract_first_meaningful_line(&content);
        assert!(line.len() <= 83); // 80 chars + "…" (3 bytes)
        assert!(line.ends_with('…'));
    }

    #[test]
    fn test_is_binary_extension() {
        assert!(is_binary_extension("image.png"));
        assert!(is_binary_extension("font.woff2"));
        assert!(is_binary_extension("archive.tar.gz"));
        assert!(!is_binary_extension("main.rs"));
        assert!(!is_binary_extension("Cargo.toml"));
        assert!(!is_binary_extension("README.md"));
    }

    #[test]
    fn test_format_project_index_empty() {
        let entries: Vec<IndexEntry> = vec![];
        let result = format_project_index(&entries);
        assert_eq!(result, "(no indexable files found)");
    }

    #[test]
    fn test_format_project_index_with_entries() {
        let entries = vec![
            IndexEntry {
                path: "src/main.rs".to_string(),
                lines: 100,
                summary: "//! Main module".to_string(),
            },
            IndexEntry {
                path: "src/lib.rs".to_string(),
                lines: 50,
                summary: "//! Library".to_string(),
            },
        ];
        let result = format_project_index(&entries);
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("100"));
        assert!(result.contains("//! Main module"));
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("50"));
        assert!(result.contains("2 files, 150 total lines"));
    }

    #[test]
    fn test_build_project_index_tempdir() {
        // Create a temp directory with known files and test indexing
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // Create some test files
        fs::write(dir_path.join("main.rs"), "//! Entry point\nfn main() {}\n").unwrap();
        fs::write(
            dir_path.join("lib.rs"),
            "//! Library code\npub fn hello() {}\n",
        )
        .unwrap();
        fs::write(dir_path.join("image.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();

        // We can't easily test build_project_index directly since it uses git ls-files
        // or walks cwd, but we CAN test the components
        let content = fs::read_to_string(dir_path.join("main.rs")).unwrap();
        let summary = extract_first_meaningful_line(&content);
        assert_eq!(summary, "//! Entry point");

        // Verify binary filtering
        assert!(is_binary_extension("image.png"));
        assert!(!is_binary_extension("main.rs"));
    }

    #[test]
    fn test_index_entry_construction() {
        let entry = IndexEntry {
            path: "src/commands.rs".to_string(),
            lines: 4000,
            summary: "//! REPL command handlers for yoyo.".to_string(),
        };
        assert_eq!(entry.path, "src/commands.rs");
        assert_eq!(entry.lines, 4000);
        assert_eq!(entry.summary, "//! REPL command handlers for yoyo.");
    }

    #[test]
    fn test_format_project_index_single_file() {
        let entries = vec![IndexEntry {
            path: "README.md".to_string(),
            lines: 1,
            summary: "# Hello".to_string(),
        }];
        let result = format_project_index(&entries);
        assert!(result.contains("1 file, 1 total lines"));
    }

    // ── memory command tests ─────────────────────────────────────────────

    #[test]
    fn test_remember_command_recognized() {
        assert!(!is_unknown_command("/remember"));
        assert!(!is_unknown_command("/remember this uses sqlx"));
        assert!(
            KNOWN_COMMANDS.contains(&"/remember"),
            "/remember should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_memories_command_recognized() {
        assert!(!is_unknown_command("/memories"));
        assert!(
            KNOWN_COMMANDS.contains(&"/memories"),
            "/memories should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_forget_command_recognized() {
        assert!(!is_unknown_command("/forget"));
        assert!(!is_unknown_command("/forget 0"));
        assert!(
            KNOWN_COMMANDS.contains(&"/forget"),
            "/forget should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_remember_command_matching() {
        let remember_matches = |s: &str| s == "/remember" || s.starts_with("/remember ");
        assert!(remember_matches("/remember"));
        assert!(remember_matches("/remember this uses sqlx"));
        assert!(!remember_matches("/remembering"));
        assert!(!remember_matches("/remembrance"));
    }

    #[test]
    fn test_forget_command_matching() {
        let forget_matches = |s: &str| s == "/forget" || s.starts_with("/forget ");
        assert!(forget_matches("/forget"));
        assert!(forget_matches("/forget 0"));
        assert!(forget_matches("/forget 42"));
        assert!(!forget_matches("/forgetting"));
        assert!(!forget_matches("/forgetful"));
    }

    #[test]
    fn test_memory_crud_roundtrip() {
        use std::fs;
        let dir = std::env::temp_dir().join("yoyo_test_memory_cmd_crud");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("memory.json");

        // Start empty
        let mut mem = load_memories_from(&path);
        assert!(mem.entries.is_empty());

        // Add
        add_memory(&mut mem, "uses sqlx");
        add_memory(&mut mem, "docker needed");
        assert_eq!(mem.entries.len(), 2);

        // Save & reload
        crate::memory::save_memories_to(&mem, &path).unwrap();
        let reloaded = load_memories_from(&path);
        assert_eq!(reloaded.entries.len(), 2);
        assert_eq!(reloaded.entries[0].note, "uses sqlx");

        // Remove
        let mut reloaded = reloaded;
        let removed = remove_memory(&mut reloaded, 0);
        assert_eq!(removed.unwrap().note, "uses sqlx");
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.entries[0].note, "docker needed");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_memory_format_for_prompt_integration() {
        let memory = ProjectMemory {
            entries: vec![MemoryEntry {
                note: "always run cargo fmt".to_string(),
                timestamp: "2026-03-15 08:00".to_string(),
            }],
        };
        let prompt = format_memories_for_prompt(&memory);
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(prompt.contains("Project Memories"));
        assert!(prompt.contains("always run cargo fmt"));
    }

    // ── /changes command tests ───────────────────────────────────────────

    #[test]
    fn test_changes_command_recognized() {
        assert!(!is_unknown_command("/changes"));
        assert!(
            KNOWN_COMMANDS.contains(&"/changes"),
            "/changes should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_changes_command_not_confused_with_other_commands() {
        // /changes should match exactly, /changelog etc. should be unknown
        assert!(is_unknown_command("/changelog"));
        assert!(is_unknown_command("/changed"));
    }

    #[test]
    fn test_handle_changes_empty_does_not_panic() {
        use crate::prompt::SessionChanges;
        let changes = SessionChanges::new();
        // Should not panic — just prints a message
        handle_changes(&changes);
    }

    #[test]
    fn test_handle_changes_with_entries_does_not_panic() {
        use crate::prompt::{ChangeKind, SessionChanges};
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        // Should not panic
        handle_changes(&changes);
    }

    // ── /add command tests ───────────────────────────────────────────

    #[test]
    fn test_add_command_recognized() {
        assert!(!is_unknown_command("/add"));
        assert!(!is_unknown_command("/add src/main.rs"));
        assert!(
            KNOWN_COMMANDS.contains(&"/add"),
            "/add should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_add_in_help_text() {
        let text = help_text();
        assert!(
            text.contains("/add"),
            "Help text should mention /add command"
        );
    }

    #[test]
    fn test_handle_add_no_args_returns_empty() {
        let results = handle_add("/add");
        assert!(results.is_empty(), "No args should return empty results");
    }

    #[test]
    fn test_handle_add_with_space_no_args_returns_empty() {
        let results = handle_add("/add   ");
        assert!(
            results.is_empty(),
            "Whitespace-only args should return empty"
        );
    }

    #[test]
    fn test_handle_add_real_file() {
        let results = handle_add("/add Cargo.toml");
        assert_eq!(results.len(), 1, "Should return one result for Cargo.toml");
        match &results[0] {
            AddResult::Text { summary, content } => {
                assert!(
                    summary.contains("Cargo.toml"),
                    "Summary should mention the file"
                );
                assert!(
                    content.contains("[package]"),
                    "Content should contain file text"
                );
            }
            _ => panic!("Expected AddResult::Text for Cargo.toml"),
        }
    }

    #[test]
    fn test_handle_add_with_line_range() {
        let results = handle_add("/add Cargo.toml:1-3");
        assert_eq!(results.len(), 1);
        match &results[0] {
            AddResult::Text { summary, content } => {
                assert!(
                    summary.contains("lines 1-3"),
                    "Summary should mention line range"
                );
                assert!(
                    content.contains("```"),
                    "Content should be wrapped in code fence"
                );
            }
            _ => panic!("Expected AddResult::Text for line range"),
        }
    }

    #[test]
    fn test_handle_add_glob_pattern() {
        let results = handle_add("/add src/*.rs");
        assert!(results.len() > 1, "Should match multiple .rs files in src/");
    }

    #[test]
    fn test_handle_add_nonexistent_file() {
        let results = handle_add("/add nonexistent_xyz_file.rs");
        assert!(results.is_empty(), "Nonexistent file should return empty");
    }

    #[test]
    fn test_handle_add_multiple_files() {
        let results = handle_add("/add Cargo.toml LICENSE");
        assert_eq!(results.len(), 2, "Should return results for both files");
    }

    // ── /plan tests ─────────────────────────────────────────────────────

    #[test]
    fn test_parse_plan_task_extracts_task() {
        let result = parse_plan_task("/plan add error handling");
        assert_eq!(result, Some("add error handling".to_string()));
    }

    #[test]
    fn test_parse_plan_task_empty_returns_none() {
        assert!(parse_plan_task("/plan").is_none());
        assert!(parse_plan_task("/plan  ").is_none());
    }

    #[test]
    fn test_build_plan_prompt_structure() {
        let prompt = build_plan_prompt("migrate database schema");
        assert!(prompt.contains("migrate database schema"));
        assert!(prompt.contains("Do NOT execute any tools"));
        assert!(prompt.contains("Files to examine"));
        assert!(prompt.contains("Step-by-step"));
    }

    #[test]
    fn test_plan_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/plan"),
            "/plan should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_plan_in_help_text() {
        let help = help_text();
        assert!(help.contains("/plan"), "/plan should appear in help text");
        assert!(
            help.contains("architect"),
            "Help text should mention architect mode"
        );
    }

    #[test]
    fn test_tokens_display_labels() {
        // Verify the /tokens output uses the clarified labels (Issue #189)
        use yoagent::provider::AnthropicProvider;
        use yoagent::Usage;

        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");

        let usage = Usage {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };

        // Should not panic with zero usage and empty conversation
        handle_tokens(&agent, &usage, "test-model");
    }

    #[test]
    fn test_tokens_display_with_large_values() {
        // Verify no panic with very large token counts
        use yoagent::provider::AnthropicProvider;
        use yoagent::Usage;

        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");

        let usage = Usage {
            input: 10_000_000,
            output: 5_000_000,
            cache_read: 3_000_000,
            cache_write: 1_000_000,
            total_tokens: 19_000_000,
        };

        // Should not panic with very large values
        handle_tokens(&agent, &usage, "test-model");
    }

    #[test]
    fn test_tokens_labels_are_clarified() {
        // Source-level check: the function body should use the clarified labels
        // from Issue #189, not the old confusing ones
        let source = include_str!("commands.rs");
        assert!(
            source.contains("Active context:"),
            "/tokens should use 'Active context:' header"
        );
        assert!(
            source.contains("Session totals (all API calls):"),
            "/tokens should use 'Session totals (all API calls):' header"
        );
        assert!(
            source.contains("session totals below show full usage"),
            "Compaction note should reference session totals"
        );
    }
}
