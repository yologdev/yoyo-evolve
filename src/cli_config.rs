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

// ── Effort level ──

/// Graduated effort presets controlling response depth and thoroughness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
}

impl EffortLevel {
    /// Human-readable label for display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// System prompt hint prepended to user messages when this effort level is active.
    /// Returns empty string for medium (default behavior, no extra hint).
    pub fn system_hint(&self) -> &'static str {
        match self {
            Self::Low => "Be concise. Give short, direct answers. Skip lengthy analysis.",
            Self::Medium => "",
            Self::High => "Be thorough. Analyze carefully. Consider alternatives. Explain your reasoning in detail.",
        }
    }
}

/// Global effort level — defaults to Medium.
/// Stored as AtomicU8: 0=Low, 1=Medium, 2=High.
static EFFORT_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

/// Set the current effort level.
pub fn set_effort_level(level: EffortLevel) {
    let val = match level {
        EffortLevel::Low => 0,
        EffortLevel::Medium => 1,
        EffortLevel::High => 2,
    };
    EFFORT_LEVEL.store(val, std::sync::atomic::Ordering::Relaxed);
}

/// Get the current effort level.
pub fn effort_level() -> EffortLevel {
    match EFFORT_LEVEL.load(std::sync::atomic::Ordering::Relaxed) {
        0 => EffortLevel::Low,
        2 => EffortLevel::High,
        _ => EffortLevel::Medium,
    }
}

/// Global flag: safe mode — disable all customizations (MCP, skills, custom commands, config).
/// Set once during CLI startup via `set_safe_mode(true)`.
static SAFE_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enable or disable safe mode.
pub fn set_safe_mode(enabled: bool) {
    SAFE_MODE.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Check if safe mode is active.
pub fn is_safe_mode() -> bool {
    SAFE_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Global flag: auto-approve file edits but still confirm shell commands.
/// Set once during CLI startup via `enable_auto_edit()`.
static AUTO_EDIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enable auto-edit mode (auto-approve file edits, still confirm shell commands).
pub fn enable_auto_edit() {
    AUTO_EDIT.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Check if auto-edit mode is active.
pub fn is_auto_edit() -> bool {
    AUTO_EDIT.load(std::sync::atomic::Ordering::Relaxed)
}

pub const DEFAULT_SESSION_PATH: &str = "yoyo-session.json";
pub const AUTO_SAVE_SESSION_PATH: &str = ".yoyo/last-session.json";

pub const SYSTEM_PROMPT: &str = r#"You are a coding assistant working in the user's terminal.
You have access to the filesystem and shell. Be direct and concise.

# Role
When the user asks you to do something, do it — don't just explain how.
Use tools proactively: read files to understand context, run commands to verify your work.
After making changes, run tests or verify the result when appropriate.

# Evidence and honesty
Ground every claim in what you actually observed. Don't invent file paths, function
names, APIs, command output, or test results — read or run to confirm before stating
something as fact. If you haven't verified a claim, say so rather than guess. When you
don't know or a tool didn't reveal the answer, say that plainly instead of fabricating
a plausible-sounding one. Prefer "let me check" over a confident guess.

# Search craft
Locate before reading: use search and list_files to find the relevant code before
opening whole files. Don't guess at file paths. Prefer targeted search over reading
entire large files — use search or read with offset/limit to jump to the right section
and keep context focused.

# Change discipline
Make narrow, surgical edits rather than sweeping rewrites. When a request is ambiguous,
clarify before changing code. Plan multi-file edits: think through the approach first,
make changes incrementally, and verify between steps. Handle errors carefully — if a
command fails or an edit doesn't match, read the error output and check actual file
content before retrying. Use git awareness: check git status/diff to understand the
current state, and don't make changes that conflict with uncommitted work without
asking. Before deleting files, resetting git state, or running other irreversible
commands, confirm with the user.

# Bounded verification
After edits, run the project's build/test/lint commands to confirm your changes work.
Verify enough to be confident the task is done, then stop and give a verdict — report
what you changed and that it passed. Don't loop indefinitely re-checking work that is
already green; once verified, move on."#;

/// Minimal system prompt for --lite mode (small/local LLMs with limited context).
///
/// Kept terse on purpose — this is substituted *wholesale* for [`SYSTEM_PROMPT`]
/// (see `cli.rs`, the `if lite && !user_set_system_prompt` branch), so anything
/// missing here is missing outright rather than inherited. The evidence line is
/// the one section that must survive the shrink: small models are the most prone
/// to inventing file paths and command output, so the guidance matters *more*
/// here than in the full prompt, not less. Costs ~30 tokens of an 8k window.
pub const LITE_SYSTEM_PROMPT: &str = "You are a coding assistant. Help the user with their code.\nYou have tools: bash (run commands), read_file, write_file, edit_file (find and replace text in files).\nDon't invent file paths, function names, or command output — read or run to confirm first, and say so when you don't know.\nAfter making changes, run the project's build or test commands to verify nothing is broken.";

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
    pub safe_mode: bool,
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
    fn system_prompt_has_behavioral_sections() {
        // The default system prompt is structured as named, sectioned behavioral
        // defaults. Each section header must be present; one assertion per section
        // so a dropped section fails loudly. These substrings are the actual
        // human-readable headers in SYSTEM_PROMPT — keep them in sync if reworded.
        let p = SYSTEM_PROMPT;
        assert!(p.contains("# Role"), "missing Role section");
        assert!(
            p.contains("# Evidence and honesty"),
            "missing anti-fabrication / evidence-grounding section"
        );
        assert!(p.contains("# Search craft"), "missing Search craft section");
        assert!(
            p.contains("# Change discipline"),
            "missing Change discipline section"
        );
        assert!(
            p.contains("# Bounded verification"),
            "missing Bounded verification section"
        );
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
    fn lite_system_prompt_keeps_the_anti_fabrication_guidance() {
        // --lite substitutes this string wholesale for SYSTEM_PROMPT, so the
        // evidence/anti-fabrication guidance is not inherited — it either lives
        // here or a --lite session runs without it. Asserted on the constant a
        // caller actually receives (cli.rs sets config.system_prompt to exactly
        // this value), not on some intermediate.
        let p = LITE_SYSTEM_PROMPT;
        assert!(
            p.contains("Don't invent"),
            "lite prompt lost its anti-fabrication instruction"
        );
        assert!(
            p.contains("read or run to confirm"),
            "lite prompt lost its verify-before-asserting instruction"
        );
        // Still minimal: the whole point of --lite is a small context budget.
        assert!(p.len() < SYSTEM_PROMPT.len() / 2);
    }

    #[test]
    fn test_auto_edit_toggle() {
        // AtomicBool starts false; enable flips to true; can reset for other tests.
        // Note: other tests may have set this, so just verify enable works.
        enable_auto_edit();
        assert!(is_auto_edit());
        // Reset for other tests (AtomicBool allows this, unlike OnceLock).
        AUTO_EDIT.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(!is_auto_edit());
    }

    #[test]
    fn test_effort_level_label() {
        assert_eq!(EffortLevel::Low.label(), "low");
        assert_eq!(EffortLevel::Medium.label(), "medium");
        assert_eq!(EffortLevel::High.label(), "high");
    }

    #[test]
    fn test_effort_level_system_hint() {
        // Low and High have non-empty hints; Medium is empty (default behavior)
        assert!(!EffortLevel::Low.system_hint().is_empty());
        assert!(EffortLevel::Medium.system_hint().is_empty());
        assert!(!EffortLevel::High.system_hint().is_empty());
        assert!(EffortLevel::Low.system_hint().contains("concise"));
        assert!(EffortLevel::High.system_hint().contains("thorough"));
    }

    #[test]
    fn test_effort_level_roundtrip() {
        // Save original to restore (tests run concurrently)
        let original = effort_level();

        set_effort_level(EffortLevel::Low);
        assert_eq!(effort_level(), EffortLevel::Low);

        set_effort_level(EffortLevel::High);
        assert_eq!(effort_level(), EffortLevel::High);

        set_effort_level(EffortLevel::Medium);
        assert_eq!(effort_level(), EffortLevel::Medium);

        // Restore
        set_effort_level(original);
    }

    #[test]
    fn test_safe_mode_defaults_to_false() {
        // SAFE_MODE static defaults to false — safe mode must be explicitly opted into
        assert!(!is_safe_mode());
    }
}
