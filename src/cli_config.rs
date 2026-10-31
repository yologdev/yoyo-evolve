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
    /// #1062: the key came from a non-empty `--api-key`; skips the setup wizard.
    pub api_key_from_cli: bool,
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
    /// `--continue-strict` (#979): refuse to run if the restore fails.
    pub continue_strict: bool,
    /// `--save-session <path>` (#978): write the session after a `-p`/piped turn.
    pub save_session: Option<std::path::PathBuf>,
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

/// Resolve a numeric setting: `flag_name`'s value in `args` wins, else
/// `file_config[config_key]`, else `None` (the caller's default).
///
/// Pure — returns the warnings instead of printing them, so tests see the
/// exact text a user reads; `cli::parse_numeric_flag` prints each one.
///
/// Since Day 244 the flag door never reaches its warning from startup: an
/// unparseable flag value is refused first (`bad_numeric_flag_refusal`, exit
/// 2). The flag-warning branch below stays as the resolver's own fallback.
///
/// Both doors warn when their value cannot be parsed (#1028: the config door
/// used to drop `max_tokens = "abc"` silently and run on the default, which is
/// how a user ends up cut off under a limit they think they changed). The
/// config value is consulted — and so can only warn — when no valid flag
/// supersedes it: with `--max-tokens 4096` the file's value is never used,
/// so "using default" would be false about this run.
///
/// Why not `config::validate_config_value`: it judges *ranges* the runtime
/// handles differently (it rejects temperature above 2.0 while the runtime
/// clamps above 1.0 and says so; it rejects `max_tokens = 0`, which parses and
/// is used) and it has no `context_window` arm. This warns exactly when the
/// value is dropped, which is the silent case.
pub(crate) fn resolve_numeric_setting<T: std::str::FromStr>(
    args: &[String],
    flag_name: &str,
    file_config: &std::collections::HashMap<String, String>,
    config_key: &str,
) -> (Option<T>, Vec<String>) {
    let mut warnings = Vec::new();
    let from_flag = args
        .iter()
        .position(|a| a == flag_name)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            let parsed = s.parse::<T>().ok();
            if parsed.is_none() {
                warnings.push(format!("Invalid {flag_name} value '{s}', using default"));
            }
            parsed
        });
    if from_flag.is_some() {
        return (from_flag, warnings);
    }
    let from_config = file_config.get(config_key).and_then(|s| {
        let parsed = s.parse::<T>().ok();
        if parsed.is_none() {
            warnings.push(format!(
                "Invalid {config_key} value '{}' in config file, using default",
                crate::cli::sanitize_for_display(s)
            ));
        }
        parsed
    });
    (from_config, warnings)
}

/// Refusal for one numeric flag whose explicit value doesn't parse as `T`
/// (`--max-tokens abc`), or `None` when the flag is absent or parses.
/// Reads the same occurrence `resolve_numeric_setting` reads (the first).
fn numeric_flag_refusal<T: std::str::FromStr>(
    args: &[String],
    flag_name: &str,
    expected: &str,
) -> Option<String> {
    let i = args.iter().position(|a| a == flag_name)?;
    let value = args.get(i + 1)?;
    if value.parse::<T>().is_ok() {
        return None;
    }
    Some(format!(
        "{flag_name} value '{}' is not a number; expected {expected}. \
Nothing was sent to the model.",
        crate::cli::sanitize_for_display(value)
    ))
}

/// Startup refusal for an explicit numeric flag whose value doesn't parse
/// (`--max-tokens abc`, `--temperature hot`, `--max-turns x`,
/// `--context-window big`). The caller prints it and exits 2 before any
/// agent is built, so a typo can't run a paid prompt on the default.
///
/// Only the FLAG door refuses. A bad value in `.yoyo.toml` still warns
/// (naming the key and "in config file") and falls back, because that file
/// may be repo-authored: refusing would block every user of the repo over
/// someone else's typo. Parseable but out-of-range values (`--temperature 5`,
/// `--max-tokens 0`) are not decided here.
pub(crate) fn bad_numeric_flag_refusal(args: &[String]) -> Option<String> {
    numeric_flag_refusal::<u32>(
        args,
        "--max-tokens",
        "a whole number, e.g. --max-tokens 4096",
    )
    .or_else(|| {
        numeric_flag_refusal::<f32>(
            args,
            "--temperature",
            "a number from 0.0 to 1.0, e.g. --temperature 0.3",
        )
    })
    .or_else(|| {
        numeric_flag_refusal::<usize>(args, "--max-turns", "a whole number, e.g. --max-turns 20")
    })
    .or_else(|| {
        numeric_flag_refusal::<u32>(
            args,
            "--context-window",
            "a whole number of tokens, e.g. --context-window 200000",
        )
    })
}

/// Startup refusal for flag-shaped tokens that are not in `KNOWN_FLAGS`
/// (#1109). Before, `yoyo --modle claude-haiku-4-5 -p hi` printed a warning,
/// ignored the flag and sent a paid request on the default model; git, cargo
/// and Claude Code all refuse an unknown option before doing any work.
///
/// Only tokens that look like a flag a person typed are refused: they start
/// with `-`, are not `-` or `--`, carry no whitespace, and come before a `--`
/// terminator. Everything else keeps the old warn-and-continue path
/// byte-identically (`warn_unknown_flags`, which the caller still runs), so a
/// quoted prompt such as `yoyo "- fix the list"` and anything after `--` are
/// untouched. Values of `FLAGS_NEEDING_VALUES` flags are skipped, so
/// `--system -x` is not read as a flag here. Subcommands never reach this:
/// `try_dispatch_subcommand` runs first and parses their own flags.
///
/// Each unknown flag is named (sanitized) with the nearest known flag when it
/// is within edit distance 2, and `--name=value` for a known name says to put
/// the value in its own word. Glyph-free, so it reads the same under plain
/// output.
pub(crate) fn unknown_flag_refusal(args: &[String]) -> Option<String> {
    use crate::cli::{FLAGS_NEEDING_VALUES, KNOWN_FLAGS};
    let mut parts = Vec::new();
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            break;
        }
        if !arg.starts_with('-') || arg == "-" || arg.chars().any(char::is_whitespace) {
            continue;
        }
        if FLAGS_NEEDING_VALUES.contains(&arg.as_str()) {
            skip_next = true;
            continue;
        }
        if KNOWN_FLAGS.contains(&arg.as_str()) {
            continue;
        }
        let shown = crate::cli::sanitize_for_display(arg);
        let hint = match arg.split_once('=') {
            Some((name, _)) if KNOWN_FLAGS.contains(&name) => {
                format!(" (write the value as its own word: {name} <value>)")
            }
            _ => crate::commands::closest_match(arg, KNOWN_FLAGS, 2)
                .map(|m| format!(" (did you mean {m}?)"))
                .unwrap_or_default(),
        };
        parts.push(format!("'{shown}'{hint}"));
    }
    if parts.is_empty() {
        return None;
    }
    let noun = if parts.len() == 1 { "flag" } else { "flags" };
    Some(format!(
        "unknown {noun} {}. Nothing was sent to the model. \
Run yoyo --help for the list of flags.",
        parts.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1109: the refusal names each unknown flag, suggests the nearest
    /// known one, and is None for every input the old path accepted.
    #[test]
    fn unknown_flag_refusal_table() {
        let refused: &[(&[&str], &str)] = &[
            (
                &["--modle", "claude-haiku-4-5", "-p", "hi"],
                "unknown flag '--modle' (did you mean --model?). Nothing was sent to the model. \
Run yoyo --help for the list of flags.",
            ),
            (
                &["log", "--bogus"],
                "unknown flag '--bogus'. Nothing was sent to the model. \
Run yoyo --help for the list of flags.",
            ),
            (
                &["--model=x", "--yse"],
                "unknown flags '--model=x' (write the value as its own word: --model <value>), \
'--yse' (did you mean --yes?). Nothing was sent to the model. \
Run yoyo --help for the list of flags.",
            ),
        ];
        for (rest, want) in refused {
            assert_eq!(
                unknown_flag_refusal(&argv(rest)).as_deref(),
                Some(*want),
                "{rest:?}"
            );
        }
        let passed: &[&[&str]] = &[
            &[],
            &["--model", "x", "--yes", "--print-system-prompt"],
            &["-p", "hi"],
            &["--system", "-x"], // a value, even when flag-shaped
            &["--", "--modle"],  // after the terminator: old warning path
            &["-", "hi"],        // bare dash
            &["- fix the list"], // a quoted prompt that starts with '-'
            &["hello", "world"],
        ];
        for rest in passed {
            assert_eq!(unknown_flag_refusal(&argv(rest)), None, "{rest:?}");
        }
    }

    #[test]
    fn unknown_flag_refusal_escapes_control_bytes() {
        let out = unknown_flag_refusal(&argv(&["--x\u{1b}[31m"])).unwrap();
        assert!("--x\u{1b}[31m".as_bytes().contains(&0x1b)); // anti-vacuous
        assert!(!out.as_bytes().contains(&0x1b), "{out:?}");
    }

    fn cfg(key: &str, value: &str) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::from([(key.to_string(), value.to_string())])
    }

    fn argv(rest: &[&str]) -> Vec<String> {
        std::iter::once("yoyo")
            .chain(rest.iter().copied())
            .map(String::from)
            .collect()
    }

    /// #1028 at the emission point: every value the assessment probed, for
    /// each of the four numeric keys, yields exactly one warning naming key,
    /// value and source, and the setting falls back to `None` (the default).
    #[test]
    fn bad_config_numeric_warns_once_naming_key_value_and_source() {
        let cases: &[(&str, &str, &str)] = &[
            ("--max-tokens", "max_tokens", "abc"),
            ("--max-tokens", "max_tokens", "-5"),
            ("--max-tokens", "max_tokens", "99999999999"),
            ("--max-turns", "max_turns", "x"),
            ("--context-window", "context_window", "big"),
        ];
        for (flag, key, value) in cases {
            let (v, w) = resolve_numeric_setting::<u32>(&argv(&[]), flag, &cfg(key, value), key);
            assert_eq!(v, None, "{key} = {value}");
            assert_eq!(
                w,
                vec![format!(
                    "Invalid {key} value '{value}' in config file, using default"
                )]
            );
        }
        let (t, w) = resolve_numeric_setting::<f32>(
            &argv(&[]),
            "--temperature",
            &cfg("temperature", "hot"),
            "temperature",
        );
        assert_eq!(t, None);
        assert_eq!(
            w,
            vec!["Invalid temperature value 'hot' in config file, using default".to_string()]
        );
    }

    /// Near-miss guard: a valid config value parses exactly as before and
    /// says nothing — every user with a correct `.yoyo.toml`.
    #[test]
    fn valid_config_numeric_is_silent_and_unchanged() {
        let (v, w) = resolve_numeric_setting::<u32>(
            &argv(&[]),
            "--max-tokens",
            &cfg("max_tokens", "2048"),
            "max_tokens",
        );
        assert_eq!((v, w), (Some(2048), Vec::<String>::new()));
        let (t, w) = resolve_numeric_setting::<f32>(
            &argv(&[]),
            "--temperature",
            &cfg("temperature", "0.5"),
            "temperature",
        );
        assert_eq!((t, w), (Some(0.5), Vec::<String>::new()));
        let (n, w) = resolve_numeric_setting::<u32>(
            &argv(&[]),
            "--max-tokens",
            &std::collections::HashMap::new(),
            "max_tokens",
        );
        assert_eq!((n, w), (None, Vec::<String>::new()));
    }

    /// Decision pinned: a valid flag wins and the bad config value does NOT
    /// warn — it is never consulted, so "using default" would be false.
    /// An invalid flag falls through to the config, and then both warn.
    #[test]
    fn flag_supersedes_bad_config_without_config_warning() {
        let bad = cfg("max_tokens", "abc");
        let (v, w) = resolve_numeric_setting::<u32>(
            &argv(&["--max-tokens", "4096"]),
            "--max-tokens",
            &bad,
            "max_tokens",
        );
        assert_eq!((v, w), (Some(4096), Vec::<String>::new()));
        let (v, w) = resolve_numeric_setting::<u32>(
            &argv(&["--max-tokens", "nope"]),
            "--max-tokens",
            &bad,
            "max_tokens",
        );
        assert_eq!(v, None);
        assert_eq!(
            w,
            vec![
                "Invalid --max-tokens value 'nope', using default".to_string(),
                "Invalid max_tokens value 'abc' in config file, using default".to_string(),
            ]
        );
    }

    /// The config value is repository-authored text: control bytes are
    /// escaped before they reach the terminal.
    #[test]
    fn bad_config_value_is_sanitized_for_display() {
        let hostile = "1\u{1b}[2J";
        assert!(hostile.as_bytes().contains(&0x1b), "fixture must carry ESC");
        let (_, w) = resolve_numeric_setting::<u32>(
            &argv(&[]),
            "--max-tokens",
            &cfg("max_tokens", hostile),
            "max_tokens",
        );
        assert_eq!(w.len(), 1);
        assert!(!w[0].as_bytes().contains(&0x1b), "{:?}", w[0]);
    }

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

    /// Day 244: an explicit numeric flag whose value doesn't parse is refused
    /// (the caller exits 2 before any request). Each row names flag and value.
    #[test]
    fn bad_numeric_flag_value_is_refused_naming_flag_and_value() {
        let rows: &[(&str, &str)] = &[
            ("--max-tokens", "abc"),
            ("--max-tokens", "-5"),
            ("--max-tokens", ""),
            ("--temperature", "hot"),
            ("--max-turns", "x"),
            ("--context-window", "big"),
        ];
        for (flag, value) in rows {
            let msg = bad_numeric_flag_refusal(&argv(&["-p", "hi", flag, value]))
                .unwrap_or_else(|| panic!("{flag} {value:?} must be refused"));
            assert!(msg.starts_with(flag), "{msg}");
            assert!(msg.contains(&format!("'{value}'")), "{msg}");
            assert!(msg.contains("Nothing was sent"), "{msg}");
        }
        assert_eq!(
            bad_numeric_flag_refusal(&argv(&["--max-tokens", "abc"])).as_deref(),
            Some(
                "--max-tokens value 'abc' is not a number; expected a whole number, \
e.g. --max-tokens 4096. Nothing was sent to the model."
            )
        );
    }

    /// Near-misses: valid values, an absent flag, and a flag with no value
    /// (that one is `check_flag_values`' job, exit 1) are not refused here.
    #[test]
    fn valid_or_absent_numeric_flags_are_not_refused() {
        let rows: &[&[&str]] = &[
            &["--max-tokens", "4096"],
            &["--temperature", "0.3"],
            &["--temperature", "0"],
            &["--max-turns", "1"],
            &["--context-window", "200000"],
            &["-p", "hi"],
            &["--max-tokens"],
        ];
        for row in rows {
            assert_eq!(bad_numeric_flag_refusal(&argv(row)), None, "{row:?}");
        }
    }

    /// The value is untrusted text on its way to a terminal.
    #[test]
    fn bad_numeric_flag_refusal_escapes_control_bytes() {
        let msg = bad_numeric_flag_refusal(&argv(&["--max-turns", "x\u{1b}[31m"])).unwrap();
        assert!(!msg.as_bytes().contains(&0x1b), "{msg:?}");
    }
}
