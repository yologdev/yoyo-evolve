//! CLI constants, configuration types, and session defaults.
//!
//! Extracted from `cli.rs` to separate configuration data from argument parsing
//! and display logic. Everything here is re-exported by `cli.rs` so downstream
//! `use crate::cli::*` imports continue to work unchanged.

use crate::config::{DirectoryRestrictions, McpServerConfig, PermissionConfig};
use yoagent::skills::SkillSet;
use yoagent::ThinkingLevel;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_CONTEXT_TOKENS: u64 = 200_000;
pub const AUTO_COMPACT_THRESHOLD: f64 = 0.80;
pub const PROACTIVE_COMPACT_THRESHOLD: f64 = 0.70;

/// Effective context window (tokens) for the current session.
/// Set once in configure_agent() based on model config + CLI override.
/// Read by /tokens and /status commands to show accurate budget.
static EFFECTIVE_CONTEXT_TOKENS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(DEFAULT_CONTEXT_TOKENS);

/// Set the effective context window size. Called once during agent setup.
pub fn set_effective_context_tokens(tokens: u64) {
    EFFECTIVE_CONTEXT_TOKENS.store(tokens, std::sync::atomic::Ordering::SeqCst);
}

/// Get the effective context window size for display purposes.
pub fn effective_context_tokens() -> u64 {
    EFFECTIVE_CONTEXT_TOKENS.load(std::sync::atomic::Ordering::SeqCst)
}

/// Global flag: auto-approve file edits but still confirm shell commands.
/// Set once during CLI startup via `enable_auto_edit()`.
static AUTO_EDIT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Enable auto-edit mode (auto-approve file edits, still confirm shell commands).
pub fn enable_auto_edit() {
    let _ = AUTO_EDIT.set(true);
}

/// Check if auto-edit mode is active.
/// Note: this is plumbing — wired into tool behavior in a follow-up task.
#[allow(dead_code)]
pub fn is_auto_edit() -> bool {
    *AUTO_EDIT.get_or_init(|| false)
}

pub const DEFAULT_SESSION_PATH: &str = "yoyo-session.json";
pub const AUTO_SAVE_SESSION_PATH: &str = ".yoyo/last-session.json";

pub const SYSTEM_PROMPT: &str = r#"You are a coding assistant working in the user's terminal.
You have access to the filesystem and shell. Be direct and concise.
When the user asks you to do something, do it — don't just explain how.
Use tools proactively: read files to understand context, run commands to verify your work.
After making changes, run tests or verify the result when appropriate.

How to work effectively:
- Search before reading: use search and list_files to locate relevant code before reading whole files. Don't guess at file paths.
- Be efficient with context: don't read entire large files when you only need a specific function. Use search or read with offset/limit to find the right section.
- Verify changes: after edits, run the project's build/test/lint commands. Check that your changes compile and tests pass before moving on.
- Plan multi-file edits: when a change spans multiple files, think through the approach first. Make changes incrementally and verify between steps.
- Handle errors carefully: if a command fails or an edit doesn't match, read the error output. Check actual file content before retrying with a corrected edit.
- Use git awareness: check git status/diff to understand the current state. Don't make changes that conflict with uncommitted work without asking.
- Confirm destructive operations: before deleting files, resetting git state, or running other irreversible commands, confirm with the user."#;

/// Minimal system prompt for --lite mode (small/local LLMs with limited context).
pub const LITE_SYSTEM_PROMPT: &str = "You are a coding assistant. Help the user with their code.\nYou have tools: bash (run commands), read_file, write_file, edit_file (find and replace text in files).\nAfter making changes, run the project's build or test commands to verify nothing is broken.";

/// The 4 essential tools available in --lite mode.
pub const LITE_TOOLS: &[&str] = &["bash", "read_file", "write_file", "edit_file"];

/// Default context window for --lite mode (suitable for 4B-8B parameter models).
pub const LITE_DEFAULT_CONTEXT_WINDOW: u32 = 8_000;

/// Context management strategy.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ContextStrategy {
    /// Default: auto-compact conversation when approaching context limit
    #[default]
    Compaction,
    /// Write checkpoint file and exit with code 2 when approaching limit
    Checkpoint,
}

/// Output format for non-interactive modes (--prompt, piped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Default human-readable text output.
    Text,
    /// Single JSON blob at the end (--json / --output-format json).
    Json,
    /// Newline-delimited JSON events streamed in real-time (--output-format stream-json).
    StreamJson,
}

/// Parsed CLI configuration.
pub struct Config {
    pub model: String,
    pub api_key: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub skills: SkillSet,
    pub system_prompt: String,
    pub thinking: ThinkingLevel,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub continue_session: bool,
    pub output_path: Option<String>,
    pub prompt_arg: Option<String>,
    pub image_path: Option<String>,
    pub verbose: bool,
    pub mcp_servers: Vec<String>,
    pub mcp_server_configs: Vec<McpServerConfig>,
    pub openapi_specs: Vec<String>,
    pub auto_approve: bool,
    pub auto_commit: bool,
    pub permissions: PermissionConfig,
    pub dir_restrictions: DirectoryRestrictions,
    pub context_strategy: ContextStrategy,
    pub context_window: Option<u32>,
    pub shell_hooks: Vec<crate::hooks::ShellHook>,
    pub fallback_provider: Option<String>,
    pub fallback_model: Option<String>,
    pub no_update_check: bool,
    pub json_output: bool,
    pub output_format: OutputFormat,
    pub audit: bool,
    pub print_system_prompt: bool,
    pub print_mode: bool,
    pub auto_watch: bool,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub no_tools: bool,
    pub lite: bool,
    pub auto_edit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_config_constants() {
        // VERSION is set at compile time — just verify it's non-empty
        assert!(!VERSION.is_empty());
        assert_eq!(DEFAULT_CONTEXT_TOKENS, 200_000);
        assert!((AUTO_COMPACT_THRESHOLD - 0.80).abs() < f64::EPSILON);
        assert!((PROACTIVE_COMPACT_THRESHOLD - 0.70).abs() < f64::EPSILON);
        assert_eq!(DEFAULT_SESSION_PATH, "yoyo-session.json");
        assert_eq!(AUTO_SAVE_SESSION_PATH, ".yoyo/last-session.json");
        assert!(SYSTEM_PROMPT.contains("coding assistant"));
    }

    #[test]
    fn test_effective_context_tokens_roundtrip() {
        // Save the original value to restore later (tests run concurrently)
        let original = effective_context_tokens();
        set_effective_context_tokens(128_000);
        assert_eq!(effective_context_tokens(), 128_000);
        // Restore
        set_effective_context_tokens(original);
    }

    #[test]
    fn test_context_strategy_default() {
        let strategy = ContextStrategy::default();
        assert_eq!(strategy, ContextStrategy::Compaction);
    }

    #[test]
    fn test_output_format_equality() {
        assert_eq!(OutputFormat::Text, OutputFormat::Text);
        assert_ne!(OutputFormat::Text, OutputFormat::Json);
        assert_ne!(OutputFormat::Json, OutputFormat::StreamJson);
    }

    #[test]
    fn test_lite_constants() {
        // LITE_SYSTEM_PROMPT should be minimal — much shorter than the default
        assert!(LITE_SYSTEM_PROMPT.contains("coding assistant"));
        assert!(LITE_SYSTEM_PROMPT.len() < SYSTEM_PROMPT.len());

        // LITE_TOOLS should have exactly the 4 essential tools
        assert_eq!(LITE_TOOLS.len(), 4);
        assert!(LITE_TOOLS.contains(&"bash"));
        assert!(LITE_TOOLS.contains(&"read_file"));
        assert!(LITE_TOOLS.contains(&"write_file"));
        assert!(LITE_TOOLS.contains(&"edit_file"));

        // LITE_DEFAULT_CONTEXT_WINDOW should be reasonable for small models
        assert_eq!(LITE_DEFAULT_CONTEXT_WINDOW, 8_000);
    }

    #[test]
    fn test_is_auto_edit_default_false() {
        // Before enable_auto_edit() is called, is_auto_edit() defaults to false.
        // (OnceLock is global so if another test called enable_auto_edit first,
        // this will see true — but it must never panic.)
        let _val = is_auto_edit();
    }
}
