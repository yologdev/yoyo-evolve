//! REPL command handlers for yoyo.
//!
//! Each `/command` in the interactive REPL is handled by a function in this module.
//! The main loop dispatches to these handlers, keeping main.rs as a thin REPL driver.

// All handle_* functions in this module are dispatched from the REPL in main.rs.

use crate::cli::{default_model_for_provider, KNOWN_PROVIDERS};
use crate::format::*;

pub use crate::help::*;

// Re-export read-only "info" handlers extracted to commands_info.rs (issue #260).
// Re-export /bg command handler and tracker for background process management.
// Wired into REPL dispatch in task 2.
pub use crate::commands_bg::{handle_bg, BackgroundJobTracker};

// Explicit re-exports keep the public API of `commands` unchanged so REPL
// dispatch sites in main.rs / repl.rs don't need to know about the split.
pub use crate::commands_info::{
    handle_changelog, handle_cost, handle_model_show, handle_profile, handle_provider_show,
    handle_status, handle_think_show, handle_tokens, handle_version,
};

// Re-export /retry and /changes handlers extracted to commands_retry.rs
// (issue #260 slice). Same stability contract as commands_info above.
pub use crate::commands_retry::{format_exit_summary, handle_changes, handle_retry};

// Re-export /remember, /memories, /forget handlers extracted to
// commands_memory.rs (issue #260 slice). Same stability contract as above.
pub use crate::commands_memory::{handle_forget, handle_memories, handle_remember};

// Re-export config, hooks, permissions, teach, and MCP handlers extracted
// to commands_config.rs (issue #260 slice). Same stability contract as above.
pub use crate::commands_config::{
    handle_config, handle_config_edit, handle_config_show, handle_hooks, handle_mcp,
    handle_permissions, handle_teach, is_teach_mode, TEACH_MODE_PROMPT,
};

use yoagent::agent::Agent;
use yoagent::*;

/// Known REPL command prefixes. Used to detect unknown slash commands
/// and for tab-completion in the REPL.
pub const KNOWN_COMMANDS: &[&str] = &[
    "/add",
    "/apply",
    "/bg",
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
    "/explain",
    "/find",
    "/fix",
    "/forget",
    "/index",
    "/status",
    "/tokens",
    "/save",
    "/skill",
    "/load",
    "/diff",
    "/blame",
    "/undo",
    "/health",
    "/hooks",
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
    "/changelog",
    "/map",
    "/stash",
    "/teach",
    "/todo",
    "/mcp",
    "/permissions",
    "/profile",
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
pub const UNDO_OPTIONS: &[&str] = &["--all", "--last-commit"];

/// Refactor subcommand names for `/refactor <Tab>` completion.
pub const REFACTOR_SUBCOMMANDS: &[&str] = &["rename", "extract", "move"];

/// Diff flag names for `/diff <Tab>` completion.
pub const DIFF_FLAGS: &[&str] = &["--staged", "--cached", "--name-only"];

pub const BG_SUBCOMMANDS: &[&str] = &["run", "list", "output", "kill"];

/// Config subcommand names for `/config <Tab>` completion.
pub const CONFIG_SUBCOMMANDS: &[&str] = &["show", "edit"];

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
        "/bg" => filter_candidates(BG_SUBCOMMANDS, &partial_lower),
        "/config" => filter_candidates(CONFIG_SUBCOMMANDS, &partial_lower),
        "/save" | "/load" => list_json_files(partial_arg),
        "/help" => help_command_completions(&partial_lower),
        "/undo" => filter_candidates(UNDO_OPTIONS, &partial_lower),
        "/refactor" => filter_candidates(REFACTOR_SUBCOMMANDS, &partial_lower),
        "/watch" => filter_candidates(crate::commands_dev::WATCH_SUBCOMMANDS, &partial_lower),
        "/lint" => filter_candidates(crate::commands_dev::LINT_SUBCOMMANDS, &partial_lower),
        "/ast" => filter_candidates(crate::commands_search::AST_GREP_FLAGS, &partial_lower),
        "/apply" => filter_candidates(crate::commands_file::APPLY_FLAGS, &partial_lower),
        "/context" => filter_candidates(
            crate::commands_project::context_subcommands(),
            &partial_lower,
        ),
        "/skill" => filter_candidates(crate::commands_project::SKILL_SUBCOMMANDS, &partial_lower),
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

/// Compute Levenshtein edit distance between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, val) in dp[0].iter_mut().enumerate() {
        *val = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

/// Suggest the closest known command for a mistyped slash command.
///
/// Returns `Some("/command")` if there's a close match, `None` otherwise.
/// Uses Levenshtein distance with thresholds based on command length,
/// and also checks for unique prefix matches.
pub fn suggest_command(input: &str) -> Option<&'static str> {
    let cmd = input.split_whitespace().next().unwrap_or(input);

    // Don't suggest for valid commands
    if KNOWN_COMMANDS.contains(&cmd) {
        return None;
    }

    // Check for unique prefix match first
    let prefix_matches: Vec<&str> = KNOWN_COMMANDS
        .iter()
        .filter(|known| known.starts_with(cmd))
        .copied()
        .collect();
    if prefix_matches.len() == 1 {
        return Some(prefix_matches[0]);
    }

    // Find closest by edit distance
    let mut best: Option<(&str, usize)> = None;
    for &known in KNOWN_COMMANDS {
        let dist = edit_distance(cmd, known);
        if let Some((_, best_dist)) = best {
            if dist < best_dist {
                best = Some((known, dist));
            }
        } else {
            best = Some((known, dist));
        }
    }

    // Threshold: ≤2 for short commands (≤5 chars), ≤3 for longer ones
    if let Some((suggestion, dist)) = best {
        let threshold = if cmd.len() <= 5 { 2 } else { 3 };
        if dist <= threshold {
            return Some(suggestion);
        }
    }

    None
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

// ── /retry ───────────────────────────────────────────────────────────────
// Moved to commands_retry.rs (issue #260 slice). Re-exported below so
// `commands::handle_retry` still resolves from repl.rs without churn.

// ── /model ───────────────────────────────────────────────────────────────

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
    let restored = if let Some(json) = saved {
        agent.restore_messages(&json).is_ok()
    } else {
        false
    };
    if restored {
        println!(
            "{DIM}  (switched to provider '{}', model '{}', conversation preserved){RESET}\n",
            agent_config.provider, agent_config.model
        );
    } else {
        println!(
            "{YELLOW}  (switched to provider '{}', model '{}', conversation could not be preserved){RESET}\n",
            agent_config.provider, agent_config.model
        );
    }
}

// ── /think ───────────────────────────────────────────────────────────────

// ── /config, /config show, /hooks, /permissions ──────────────────────────
// Moved to commands_config.rs (issue #260 slice). Re-exported at the top
// of this file so `commands::handle_config` etc. still resolve.

// ── /changes ─────────────────────────────────────────────────────────────
// Moved to commands_retry.rs (issue #260 slice). Re-exported below so
// `commands::handle_changes` still resolves from repl.rs without churn.

// ── Re-exports from submodules ────────────────────────────────────────────
// These re-exports keep the public API stable so repl.rs continues to work
// with `commands::handle_*` calls unchanged.

// Git-related handlers
pub use crate::commands_git::{
    handle_blame, handle_commit, handle_diff, handle_git, handle_pr, handle_review, handle_undo,
};

// Project-related handlers
pub use crate::commands_project::{
    handle_context, handle_docs, handle_extract, handle_init, handle_move, handle_plan,
    handle_refactor, handle_rename, handle_skill, handle_todo,
};

pub use crate::commands_map::handle_map;
pub use crate::commands_search::{handle_ast_grep, handle_find, handle_grep, handle_index};

pub use crate::commands_dev::{
    handle_doctor, handle_fix, handle_health, handle_lint, handle_lint_fix, handle_run,
    handle_run_usage, handle_test, handle_tree, handle_update, handle_watch,
};

pub use crate::commands_file::{
    build_explain_prompt, expand_file_mentions, handle_add, handle_apply, handle_web, AddResult,
};

// Session-related handlers
pub use crate::commands_session::{
    auto_compact_if_needed, auto_save_on_exit, clear_confirmation_message, handle_compact,
    handle_export, handle_history, handle_jump, handle_load, handle_mark, handle_marks,
    handle_save, handle_search, handle_stash, last_session_exists, reset_compact_thrash, Bookmarks,
};

// Spawn subsystem
pub use crate::commands_spawn::{handle_spawn, SpawnTracker};

// Memory-related handlers live in commands_memory.rs (#260 slice).
// The memory-module helpers they use (add_memory, load_memories,
// remove_memory, save_memories) are imported directly from crate::memory
// in that file and in the test module below — no module-level re-export
// is needed here since nothing in commands.rs itself calls them anymore.

// ── /teach, /mcp ─────────────────────────────────────────────────────────
// Moved to commands_config.rs (issue #260 slice). Re-exported at the top
// of this file so `commands::handle_teach`, `commands::handle_mcp`, etc.
// still resolve.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_config::format_config_output;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use yoagent::ThinkingLevel;

    // ── /config show tests ────────────────────────────────────────────
    // Runtime config introspection — see `format_config_output` and
    // `is_secret_key` above. These tests pin the two most important
    // invariants: (1) secrets are NEVER printed raw, and (2) the
    // no-config-loaded path produces a clear message instead of
    // crashing or printing an empty block.

    #[test]
    fn test_format_config_masks_secret_values() {
        let mut config = HashMap::new();
        let raw_key = "sk-ant-super-secret-do-not-leak-12345";
        config.insert("anthropic_api_key".to_string(), raw_key.to_string());
        config.insert("model".to_string(), "claude-sonnet-4-6".to_string());

        let path = PathBuf::from("/fake/path/.yoyo.toml");
        let out = format_config_output(&config, Some(&path));

        // The raw secret value must never appear in the output.
        assert!(
            !out.contains(raw_key),
            "raw secret leaked into /config show output:\n{out}"
        );
        // The mask must appear so the user can see the key exists.
        assert!(
            out.contains("***"),
            "expected masked placeholder in output:\n{out}"
        );
        // Non-secret keys should be visible as-is.
        assert!(
            out.contains("claude-sonnet-4-6"),
            "non-secret value should be visible:\n{out}"
        );
        // The loaded path should be named.
        assert!(
            out.contains("/fake/path/.yoyo.toml"),
            "loaded config path should be shown:\n{out}"
        );
    }

    #[test]
    fn test_format_config_no_file_loaded() {
        let config: HashMap<String, String> = HashMap::new();
        let out = format_config_output(&config, None);

        // Must say something clear about the no-config case.
        assert!(
            out.to_lowercase().contains("no config file loaded"),
            "expected 'no config file loaded' message, got:\n{out}"
        );
        // Must not crash and must not print stale path markers.
        assert!(
            !out.contains("Loaded config:"),
            "should not claim a config was loaded:\n{out}"
        );
    }

    #[test]
    fn test_format_config_sorts_keys_deterministically() {
        let mut config = HashMap::new();
        config.insert("zebra".to_string(), "z".to_string());
        config.insert("alpha".to_string(), "a".to_string());
        config.insert("mike".to_string(), "m".to_string());
        let path = PathBuf::from(".yoyo.toml");
        let out = format_config_output(&config, Some(&path));

        let alpha_pos = out.find("alpha").expect("alpha should appear");
        let mike_pos = out.find("mike").expect("mike should appear");
        let zebra_pos = out.find("zebra").expect("zebra should appear");
        assert!(
            alpha_pos < mike_pos && mike_pos < zebra_pos,
            "keys should be sorted alphabetically:\n{out}"
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
            auto_commit: false,
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
            auto_commit: false,
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
            auto_commit: false,
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
    fn test_arg_completions_bg_empty() {
        let candidates = command_arg_completions("/bg", "");
        assert!(
            candidates.contains(&"run".to_string()),
            "Should include 'run': {candidates:?}"
        );
        assert!(
            candidates.contains(&"list".to_string()),
            "Should include 'list': {candidates:?}"
        );
        assert!(
            candidates.contains(&"output".to_string()),
            "Should include 'output': {candidates:?}"
        );
        assert!(
            candidates.contains(&"kill".to_string()),
            "Should include 'kill': {candidates:?}"
        );
        assert_eq!(candidates.len(), 4);
    }

    #[test]
    fn test_arg_completions_bg_partial() {
        let candidates = command_arg_completions("/bg", "k");
        assert_eq!(candidates, vec!["kill"]);
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

    #[test]
    fn test_arg_completions_config_subcommands() {
        let candidates = command_arg_completions("/config", "");
        assert!(
            candidates.contains(&"show".to_string()),
            "Should include 'show': {candidates:?}"
        );
        assert!(
            candidates.contains(&"edit".to_string()),
            "Should include 'edit': {candidates:?}"
        );
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn test_arg_completions_config_partial() {
        let candidates = command_arg_completions("/config", "e");
        assert_eq!(candidates, vec!["edit"]);
        let candidates = command_arg_completions("/config", "s");
        assert_eq!(candidates, vec!["show"]);
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(edit_distance("help", "help"), 0);
        assert_eq!(edit_distance("help", "hlep"), 2);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_suggest_command_typos() {
        // Common typos should suggest the right command
        assert_eq!(suggest_command("/hlep"), Some("/help"));
        assert_eq!(suggest_command("/comit"), Some("/commit"));
        assert_eq!(suggest_command("/savee"), Some("/save"));
    }

    #[test]
    fn test_suggest_command_no_match() {
        // Too far from anything → None
        assert_eq!(suggest_command("/zzzzz"), None);
        assert_eq!(suggest_command("/xyzabc"), None);
    }

    #[test]
    fn test_suggest_command_prefix_match() {
        // Unique prefix should suggest the full command
        assert_eq!(suggest_command("/comp"), Some("/compact"));
        assert_eq!(suggest_command("/expl"), Some("/explain"));
    }

    #[test]
    fn test_suggest_command_valid_command_returns_none() {
        // Valid commands should not generate suggestions
        assert_eq!(suggest_command("/model"), None);
        assert_eq!(suggest_command("/help"), None);
        assert_eq!(suggest_command("/save"), None);
    }

    #[test]
    fn test_suggest_command_with_args() {
        // Should extract just the command part, ignoring args
        assert_eq!(suggest_command("/hlep commands"), Some("/help"));
        assert_eq!(suggest_command("/savee myfile.json"), Some("/save"));
    }
}
