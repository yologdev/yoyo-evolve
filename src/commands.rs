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
    handle_changelog, handle_cost, handle_evolution, handle_model_info, handle_model_list,
    handle_model_show, handle_profile, handle_provider_show, handle_status, handle_think_show,
    handle_tips, handle_tokens, handle_version,
};
pub(crate) use crate::commands_risk::handle_risk;

// Re-export /retry and /changes handlers extracted to commands_retry.rs
// (issue #260 slice). Same stability contract as commands_info above.
pub(crate) use crate::commands_retry::{
    changes_conflict_message, handle_changes_summary, wants_summary,
};
pub use crate::commands_retry::{format_exit_summary, handle_changes, handle_retry};

// Re-export /revisit handler for shelved-issue review.
pub use crate::commands_revisit::handle_revisit;

// Re-export /remember, /memories, /forget handlers extracted to
// commands_memory.rs (issue #260 slice). Same stability contract as above.
pub use crate::commands_memory::{handle_forget, handle_memories, handle_remember};

// Re-export config, hooks, permissions, teach, and MCP handlers extracted
// to commands_config.rs (issue #260 slice). Same stability contract as above.
pub use crate::commands_config::{
    architect_model, default_editor_model, handle_architect, handle_config, handle_config_edit,
    handle_config_get, handle_config_set, handle_config_show, handle_hooks, handle_mcp,
    handle_permissions, handle_read, handle_teach, is_architect_mode, is_read_mode, is_teach_mode,
    ConfigDisplay, ARCHITECT_PROMPT, READ_MODE_PROMPT, TEACH_MODE_PROMPT,
};

use yoagent::agent::Agent;
use yoagent::*;

/// Known REPL command prefixes. Used to detect unknown slash commands
/// and for tab-completion in the REPL.
pub const KNOWN_COMMANDS: &[&str] = &[
    "/add",
    "/apply",
    "/architect",
    "/bg",
    "/cd",
    "/checkpoint",
    "/fork",
    "/help",
    "/quit",
    "/exit",
    "/clear",
    "/clear!",
    "/compact",
    "/commit",
    "/copy",
    "/cost",
    "/doctor",
    "/docs",
    "/export",
    "/evolution",
    "/explain",
    "/extended",
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
    "/goal",
    "/grep",
    "/test",
    "/lint",
    "/security",
    "/loop",
    "/spawn",
    "/update",
    "/review",
    "/revisit",
    "/risk",
    "/mark",
    "/jump",
    "/marks",
    "/plan",
    "/read",
    "/remember",
    "/memories",
    "/provider",
    "/changes",
    "/web",
    "/quick",
    "/rename",
    "/extract",
    "/move",
    "/refactor",
    "/side",
    "/watch",
    "/ast",
    "/changelog",
    "/map",
    "/open",
    "/outline",
    "/def",
    "/refs",
    "/stash",
    "/rewind",
    "/teach",
    "/tips",
    "/todo",
    "/mcp",
    "/permissions",
    "/profile",
    "/quick",
];

/// Well-known model names for `/model <Tab>` completion.
pub const KNOWN_MODELS: &[&str] = &[
    "info",
    "list",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-4-7",
    "claude-sonnet-4-6",
    "claude-sonnet-4-5",
    "claude-sonnet-4-20250514",
    "claude-haiku-4-5",
    "gpt-5",
    "gpt-5-mini",
    "gpt-5.5",
    "gpt-5.5-mini",
    "gpt-4o",
    "gpt-4o-mini",
    "gpt-4.1",
    "gpt-4.1-mini",
    "codex-mini",
    "o3",
    "o3-mini",
    "o4-mini",
    "o4-mini-high",
    "gemini-3.0-pro",
    "gemini-3.0-flash",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "grok-4",
    "grok-4-mini",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
];

/// Thinking level names for `/think <Tab>` completion.
pub const THINKING_LEVELS: &[&str] = &["off", "minimal", "low", "medium", "high"];

/// Git subcommand names for `/git <Tab>` completion.
pub const GIT_SUBCOMMANDS: &[&str] = &["status", "log", "add", "stage", "diff", "branch", "stash"];

/// PR subcommand names for `/pr <Tab>` completion.
pub const PR_SUBCOMMANDS: &[&str] = &[
    "list", "view", "diff", "review", "comment", "create", "checkout",
];

/// Undo option names for `/undo <Tab>` completion.
pub const UNDO_OPTIONS: &[&str] = &["--all", "--last-commit"];

/// Refactor subcommand names for `/refactor <Tab>` completion.
pub const REFACTOR_SUBCOMMANDS: &[&str] = &["rename", "extract", "move"];

/// Diff flag names for `/diff <Tab>` completion.
pub const DIFF_FLAGS: &[&str] = &["--staged", "--cached", "--name-only", "--stat", "--explain"];

pub const BG_SUBCOMMANDS: &[&str] = &["run", "list", "output", "kill"];

/// Config subcommand names for `/config <Tab>` completion.
pub const CONFIG_SUBCOMMANDS: &[&str] = &["show", "edit", "set", "get"];

/// Subcommands for `/goal`. Mirrors the arms `handle_goal` actually routes —
/// `verify` is routed (and documented) but was missing here until #722, so the
/// inline hint and `/goal ver<Tab>` silently under-reported a working feature.
pub const GOAL_SUBCOMMANDS: &[&str] = &["set", "show", "clear", "check", "verify"];

pub const HISTORY_SUBCOMMANDS: &[&str] = &["detail"];

/// Build (once) a `" | "`-joined hint string from a subcommand constant,
/// skipping flag entries (`-`-prefixed). Deriving the hint from the same
/// constant that tab-completion reads makes hint drift structurally
/// impossible — the Day 140 lesson: never hand-type an enumeration of
/// facts the code already owns.
fn derived_hint(
    cell: &'static std::sync::OnceLock<String>,
    subs: &'static [&'static str],
) -> &'static str {
    cell.get_or_init(|| {
        subs.iter()
            .filter(|s| !s.starts_with('-'))
            .copied()
            .collect::<Vec<_>>()
            .join(" | ")
    })
}

/// Expands to `Some(&'static str)` derived from a subcommand constant via
/// [`derived_hint`], with a per-arm `OnceLock` cache.
macro_rules! hint_from {
    ($subs:expr) => {{
        static HINT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        Some(derived_hint(&HINT, $subs))
    }};
}

/// Return a hint string showing available arguments/subcommands for a command.
///
/// Used by the hinter to display dim text after the user types a command + space.
/// Returns `None` for commands that take no arguments.
pub fn command_arg_hint(cmd: &str) -> Option<&'static str> {
    match cmd {
        "architect" => Some("on | off | <model>"),
        "diff" => Some("[file] [--stat] [--cached] [--staged] [--name-only]"),
        "model" => Some("<model-name>"),
        "think" => hint_from!(THINKING_LEVELS),
        "git" => hint_from!(GIT_SUBCOMMANDS),
        "goal" => hint_from!(GOAL_SUBCOMMANDS),
        "pr" => hint_from!(PR_SUBCOMMANDS),
        "help" => Some("<command>"),
        "config" => Some("show | edit | set <key> <value> | get <key>"),
        "copy" => Some("last | code | <text>"),
        "save" => Some("<filename.json>"),
        "load" => Some("<filename.json>"),
        "add" => Some("<file-or-url> ..."),
        "apply" => Some("<patch-file> [--check]"),
        "bg" => hint_from!(BG_SUBCOMMANDS),
        "checkpoint" => hint_from!(checkpoint_subcommands()),
        "undo" => Some("[--all] [--last-commit]"),
        "refactor" => hint_from!(REFACTOR_SUBCOMMANDS),
        "watch" => hint_from!(crate::watch::WATCH_SUBCOMMANDS),
        "lint" => hint_from!(crate::commands_lint::LINT_SUBCOMMANDS),
        "risk" => hint_from!(crate::commands_risk::RISK_SUBCOMMANDS),
        "fork" => hint_from!(FORK_SUBCOMMANDS),
        "loop" => Some("<N|until-pass> <prompt>"),
        "provider" => Some("<provider-name>"),
        "context" => hint_from!(crate::commands_project::context_subcommands()),
        "skill" => hint_from!(crate::commands_skill::SKILL_SUBCOMMANDS),
        "spawn" => {
            Some("<task> | status | worktrees | collect | manifest | manifests | replay [run|latest] | runs")
        }
        "grep" => Some("<pattern> [path] [-i] [-n]"),
        "find" => Some("<filename-pattern>"),
        "blame" => Some("<file> [line-range]"),
        "review" => Some("[branch]"),
        "revisit" => Some("scan | check #N | list | add #N <reason> | remove #N"),
        "web" => Some("<url> | search <query>"),
        "run" => Some("<command>"),
        "test" => Some("[args...]"),
        "export" => Some("[filename]"),
        "search" => Some("<query>"),
        "remember" => Some("<note>"),
        "forget" => Some("<id>"),
        "explain" => Some("<file>"),
        "map" => Some("[--all] [--regex] [path]"),
        "outline" => Some("<query> [--all]"),
        "def" => Some("<symbol-name>"),
        "refs" => Some("<symbol-name>"),
        "stash" => Some("push | pop | list | drop"),
        "mark" => Some("<name>"),
        "jump" => Some("<name>"),
        "ast" => Some("<pattern> [path]"),
        "todo" => hint_from!(crate::commands_todo::TODO_VERBS),
        "docs" => Some("<crate-name>"),
        "rename" => Some("<old> <new> [path]"),
        "side" => Some("<prompt>"),
        "quick" => Some("<question>"),
        "changelog" => Some("[count]"),
        "changes" => Some("summary | --diff"),
        "evolution" => Some("[count]"),
        "extended" | "ext" => Some("<prompt>"),
        "plan" => Some("on | off | open | close | show | apply | clear | status | step | [--deep] <task>"),
        "tree" => Some("[path] [--depth N]"),
        "index" => Some("[path]"),
        "history" => hint_from!(HISTORY_SUBCOMMANDS),
        _ => None,
    }
}

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
        "/goal" => filter_candidates(GOAL_SUBCOMMANDS, &partial_lower),
        "/diff" => filter_candidates(DIFF_FLAGS, &partial_lower),
        "/pr" => filter_candidates(PR_SUBCOMMANDS, &partial_lower),
        "/provider" => filter_candidates(KNOWN_PROVIDERS, &partial_lower),
        "/bg" => filter_candidates(BG_SUBCOMMANDS, &partial_lower),
        "/checkpoint" => filter_candidates(checkpoint_subcommands(), &partial_lower),
        "/fork" => filter_candidates(FORK_SUBCOMMANDS, &partial_lower),
        "/config" => filter_candidates(CONFIG_SUBCOMMANDS, &partial_lower),
        "/save" | "/load" => list_json_files(partial_arg),
        "/help" => help_command_completions(&partial_lower),
        "/undo" => filter_candidates(UNDO_OPTIONS, &partial_lower),
        "/refactor" => filter_candidates(REFACTOR_SUBCOMMANDS, &partial_lower),
        "/watch" => filter_candidates(crate::watch::WATCH_SUBCOMMANDS, &partial_lower),
        "/lint" => filter_candidates(crate::commands_lint::LINT_SUBCOMMANDS, &partial_lower),
        "/ast" => filter_candidates(crate::commands_ast_grep::AST_GREP_FLAGS, &partial_lower),
        "/apply" => filter_candidates(crate::commands_file::APPLY_FLAGS, &partial_lower),
        "/copy" => filter_candidates(COPY_SUBCOMMANDS, &partial_lower),
        "/context" => filter_candidates(
            crate::commands_project::context_subcommands(),
            &partial_lower,
        ),
        "/skill" => filter_candidates(crate::commands_skill::SKILL_SUBCOMMANDS, &partial_lower),
        "/plan" => filter_candidates(crate::commands_plan::PLAN_SUBCOMMANDS, &partial_lower),
        "/history" => filter_candidates(HISTORY_SUBCOMMANDS, &partial_lower),
        "/revisit" => {
            filter_candidates(crate::commands_revisit::REVISIT_SUBCOMMANDS, &partial_lower)
        }
        "/web" => filter_candidates(crate::commands_web::WEB_SUBCOMMANDS, &partial_lower),
        "/risk" => filter_candidates(crate::commands_risk::RISK_SUBCOMMANDS, &partial_lower),
        "/spawn" => filter_candidates(crate::commands_spawn::SPAWN_SUBCOMMANDS, &partial_lower),
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
    if KNOWN_COMMANDS.contains(&cmd) {
        return false;
    }
    // Check custom commands: strip leading '/' and check
    if let Some(name) = cmd.strip_prefix('/') {
        if is_custom_command(name) {
            return false;
        }
    }
    true
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

/// Find the candidate closest to `word` by Levenshtein distance, if any is
/// within `max_distance` edits. Ties break toward the earlier candidate.
///
/// Shared by the near-miss typo guards for free-text commands (`/spawn`,
/// `/watch`) so the edit-distance logic lives in exactly one place. Note an
/// exact match (distance 0) is returned too — callers that treat exact words
/// specially must check membership first.
pub fn closest_match<'a>(
    word: &str,
    candidates: &[&'a str],
    max_distance: usize,
) -> Option<&'a str> {
    let mut best: Option<(&'a str, usize)> = None;
    for &cand in candidates {
        let dist = edit_distance(word, cand);
        if best.is_none_or(|(_, d)| dist < d) {
            best = Some((cand, dist));
        }
    }
    best.filter(|&(_, d)| d <= max_distance).map(|(c, _)| c)
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

    // A bare "/" (or empty input) carries no signal — every command is a prefix
    // match and the nearest edit-distance neighbor is spurious. Requiring at
    // least one character after the slash avoids "did you mean /ls?" when the
    // user has only typed the leading slash. (chars, not bytes, for safety.)
    if cmd
        .strip_prefix('/')
        .is_none_or(|rest| rest.chars().next().is_none())
    {
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
    let saved = match agent.save_messages() {
        Ok(json) => Some(json),
        Err(e) => {
            eprintln!("{DIM}  ⚠ could not preserve conversation: {e}{RESET}");
            None
        }
    };
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
    handle_commit, handle_commit_ai, handle_diff, handle_diff_explain, handle_git, handle_pr,
    handle_undo, parse_diff_args, wants_ai_commit,
};
pub use crate::commands_goal::handle_goal;

// Git review/blame handlers
pub use crate::commands_git_review::{handle_blame, handle_review};

// Project-related handlers
pub use crate::commands_plan::{
    handle_plan, is_plan_apply_active, is_plan_mode, set_plan_apply_active, PlanResult,
    PLAN_MODE_PROMPT,
};
pub use crate::commands_project::{
    auto_context_for_prompt, format_auto_context, handle_context, handle_docs, handle_extract,
    handle_init, handle_move, handle_refactor, handle_rename,
};

pub use crate::commands_todo::handle_todo;

// Skill-related handlers
pub use crate::commands_skill::handle_skill;

pub use crate::commands_ast_grep::handle_ast_grep;
pub use crate::commands_map::handle_map;
pub use crate::commands_search::{
    handle_def, handle_find, handle_grep, handle_index, handle_outline, handle_refs,
};

pub use crate::commands_dev::{handle_doctor, handle_fix, handle_health};

pub use crate::commands_tree::handle_tree;

pub use crate::watch::handle_watch;

pub use crate::commands_update::handle_update;

pub use crate::commands_lint::{handle_lint, handle_lint_fix, handle_security, handle_test};

pub use crate::commands_run::{handle_loop, handle_run, handle_run_usage};

pub use crate::commands_file::{
    build_explain_prompt, expand_file_mentions, handle_add, handle_apply, handle_open,
    suggest_related_files, AddResult,
};

pub use crate::commands_web::{handle_copy, handle_web, COPY_SUBCOMMANDS};

// Session-related handlers
pub use crate::commands_session::{
    auto_compact_if_needed, auto_save_on_exit, handle_compact, handle_export, handle_history,
    handle_history_detail, handle_jump, handle_load, handle_mark, handle_marks, handle_save,
    handle_search, reset_compact_thrash, Bookmarks,
};

// Stash subsystem
pub use crate::commands_stash::{auto_checkpoint, handle_rewind, handle_stash, stash_pre_clear};

// Fork and checkpoint handlers
pub use crate::commands_fork::{
    checkpoint_subcommands, clear_confirmation_message, handle_checkpoint, handle_fork,
    CheckpointStore, FORK_SUBCOMMANDS,
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

// ---------------------------------------------------------------------------
// Custom slash commands — load user-defined .md files from
//   .yoyo/commands/  (project-local, higher priority)
//   ~/.yoyo/commands/ (global/user-level)
// ---------------------------------------------------------------------------

/// Discover custom slash commands from `.yoyo/commands/` and `~/.yoyo/commands/`.
/// Returns `Vec<(name, content)>` — project-local commands override global ones
/// with the same name. Silently returns an empty vec if directories don't exist.
/// In safe mode, returns an empty vec to disable all custom commands.
pub fn discover_custom_commands() -> Vec<(String, String)> {
    if crate::cli_config::is_safe_mode() {
        return Vec::new();
    }
    discover_custom_commands_from(None)
}

/// Discover custom slash commands from explicit directories (for testing).
/// If `override_dirs` is `None`, uses the default project + home paths.
pub(crate) fn discover_custom_commands_from(
    override_dirs: Option<(&std::path::Path, &std::path::Path)>,
) -> Vec<(String, String)> {
    let project_dir;
    let global_dir;
    let (proj_path, glob_path) = match override_dirs {
        Some((p, g)) => (p, g),
        None => {
            project_dir = std::path::PathBuf::from(".yoyo/commands");
            global_dir = match std::env::var("HOME") {
                Ok(h) => std::path::PathBuf::from(h).join(".yoyo/commands"),
                Err(_) => {
                    return load_single_dir_commands(&std::path::PathBuf::from(".yoyo/commands"))
                }
            };
            (project_dir.as_path(), global_dir.as_path())
        }
    };

    let mut commands: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Load global commands first (lower priority)
    load_commands_from_dir(glob_path, &mut commands);
    // Load project-local commands second (higher priority — overwrites global)
    load_commands_from_dir(proj_path, &mut commands);

    let mut result: Vec<(String, String)> = commands.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// Helper: load commands from a single dir and return as a sorted vec.
fn load_single_dir_commands(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut commands = std::collections::HashMap::new();
    load_commands_from_dir(dir, &mut commands);
    let mut result: Vec<(String, String)> = commands.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

fn load_commands_from_dir(
    dir: &std::path::Path,
    commands: &mut std::collections::HashMap<String, String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                commands.insert(stem.to_string(), content);
            }
        }
    }
}

/// Check if a slash command name (without leading `/`) matches a custom command.
pub fn is_custom_command(cmd: &str) -> bool {
    get_custom_command_content(cmd).is_some()
}

/// Get the content of a custom command by name (without leading `/`).
/// Checks project-local `.yoyo/commands/` first, then global `~/.yoyo/commands/`.
pub fn get_custom_command_content(cmd: &str) -> Option<String> {
    // Check project-local first
    let project_path = std::path::PathBuf::from(format!(".yoyo/commands/{cmd}.md"));
    if let Ok(content) = std::fs::read_to_string(&project_path) {
        return Some(content);
    }
    // Check global
    if let Ok(home) = std::env::var("HOME") {
        let global_path = std::path::PathBuf::from(home).join(format!(".yoyo/commands/{cmd}.md"));
        if let Ok(content) = std::fs::read_to_string(&global_path) {
            return Some(content);
        }
    }
    None
}

/// Return names of all discovered custom commands (for tab-completion).
pub fn custom_command_names() -> Vec<String> {
    discover_custom_commands()
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Structural drift guard (Day 141): every command whose tab-completions
    /// are wired to a subcommand constant must have an arg hint that mentions
    /// every non-flag entry of that constant. Fails loudly when a subcommand
    /// is added to a constant without the hint keeping up — the exact drift
    /// class that produced stale `watch`/`git`/`pr` hints and a missing
    /// `risk` hint (Day 140 lesson: never hand-type an enumeration of facts
    /// the code already owns).
    #[test]
    fn test_arg_hints_cover_wired_subcommand_constants() {
        // Commands wired to constants in `command_arg_completions` that take
        // subcommand words (flag-only wirings like /undo and /diff are
        // exempt: their constants hold no bare subcommand words).
        let wired = [
            "git",
            "pr",
            "watch",
            "risk",
            "fork",
            "goal",
            "bg",
            "lint",
            "refactor",
            "checkpoint",
            "history",
            "think",
            "context",
            "skill",
            "plan",
            "spawn",
            "config",
            "revisit",
            "web",
            "copy",
        ];
        for cmd in wired {
            let entries = command_arg_completions(&format!("/{cmd}"), "");
            assert!(
                !entries.is_empty(),
                "/{cmd} listed as wired but command_arg_completions returned nothing"
            );
            let hint = command_arg_hint(cmd)
                .unwrap_or_else(|| panic!("/{cmd} has wired completions but no arg hint"));
            let hint_words: Vec<&str> = hint
                .split(|c: char| c == '|' || c == '[' || c == ']' || c.is_whitespace())
                .filter(|w| !w.is_empty())
                .collect();
            for entry in &entries {
                if entry.starts_with('-') || entry.starts_with('<') {
                    continue; // flags and placeholders are not subcommand words
                }
                assert!(
                    hint_words.iter().any(|w| w == entry),
                    "/{cmd} hint {hint:?} is missing subcommand {entry:?} from its completion constant"
                );
            }
        }
    }

    /// Bidirectional pin for the pure-subcommand-list hints named in the
    /// Day 141 task: these hints are DERIVED from their constants, so the
    /// hint must be exactly the " | "-join of the constant's non-flag
    /// entries — no extra advertised words (the old `pr` hint advertised
    /// `describe`/`status`, which don't exist), none missing.
    #[test]
    fn test_pure_list_hints_match_constants_exactly() {
        let cases: [(&str, &[&str]); 4] = [
            ("git", GIT_SUBCOMMANDS),
            ("pr", PR_SUBCOMMANDS),
            ("watch", crate::watch::WATCH_SUBCOMMANDS),
            ("risk", crate::commands_risk::RISK_SUBCOMMANDS),
        ];
        for (cmd, subs) in cases {
            let expected = subs
                .iter()
                .filter(|s| !s.starts_with('-'))
                .copied()
                .collect::<Vec<_>>()
                .join(" | ");
            assert_eq!(
                command_arg_hint(cmd),
                Some(expected.as_str()),
                "/{cmd} hint must be derived from its subcommand constant"
            );
        }
    }
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
            auto_watch: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        };
        let mut agent = config.build_agent();
        handle_provider_switch("openai", &mut config, &mut agent);
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-5");
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
            auto_watch: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
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
            auto_watch: true,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        };
        let mut agent = config.build_agent();
        // Switch to google → should use gemini default
        handle_provider_switch("google", &mut config, &mut agent);
        assert_eq!(config.provider, "google");
        assert_eq!(config.model, "gemini-2.5-flash");
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
            3,
            "Should match 'status', 'stage', and 'stash': {candidates:?}"
        );
        assert!(candidates.contains(&"status".to_string()));
        assert!(candidates.contains(&"stage".to_string()));
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
        assert!(
            candidates.contains(&"set".to_string()),
            "Should include 'set': {candidates:?}"
        );
        assert!(
            candidates.contains(&"get".to_string()),
            "Should include 'get': {candidates:?}"
        );
        assert_eq!(candidates.len(), 4);
    }

    #[test]
    fn test_arg_completions_config_partial() {
        let candidates = command_arg_completions("/config", "e");
        assert_eq!(candidates, vec!["edit"]);
        let candidates = command_arg_completions("/config", "s");
        assert_eq!(candidates, vec!["show", "set"]);
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
    fn test_suggest_command_bare_slash_is_silent() {
        // A bare "/" (or no content after the slash) carries no signal — every
        // command is a prefix match and the nearest neighbor is spurious.
        assert_eq!(suggest_command("/"), None);
        assert_eq!(suggest_command(""), None);
        assert_eq!(suggest_command("/ arg"), None);
        // But a single meaningful character after the slash still suggests.
        assert!(suggest_command("/hel").is_some());
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

    #[test]
    fn test_command_arg_hint_diff_contains_stat() {
        let hint = command_arg_hint("diff");
        assert!(hint.is_some());
        assert!(
            hint.unwrap().contains("--stat"),
            "diff hint should contain --stat"
        );
    }

    #[test]
    fn test_command_arg_hint_help_contains_command() {
        let hint = command_arg_hint("help");
        assert!(hint.is_some());
        assert!(
            hint.unwrap().contains("command"),
            "help hint should contain 'command'"
        );
    }

    #[test]
    fn test_command_arg_hint_version_returns_none() {
        // /version takes no arguments
        assert!(command_arg_hint("version").is_none());
    }

    #[test]
    fn test_command_arg_hint_model_shows_placeholder() {
        let hint = command_arg_hint("model");
        assert!(hint.is_some());
        assert!(
            hint.unwrap().contains("model"),
            "model hint should reference model-name"
        );
    }

    #[test]
    fn test_command_arg_hint_think_shows_levels() {
        let hint = command_arg_hint("think");
        assert!(hint.is_some());
        let h = hint.unwrap();
        assert!(h.contains("off"), "think hint should contain 'off'");
        assert!(h.contains("high"), "think hint should contain 'high'");
    }

    #[test]
    fn test_command_arg_hint_no_args_commands() {
        // Commands with no arguments
        for cmd in &[
            "version", "quit", "exit", "clear", "status", "tokens", "cost", "marks",
        ] {
            assert!(
                command_arg_hint(cmd).is_none(),
                "{cmd} should have no arg hint"
            );
        }
    }

    #[test]
    fn test_command_arg_hint_git_shows_subcommands() {
        let hint = command_arg_hint("git").unwrap();
        assert!(hint.contains("status"));
        assert!(hint.contains("log"));
    }

    #[test]
    fn test_command_arg_hint_pr_shows_subcommands() {
        let hint = command_arg_hint("pr").unwrap();
        assert!(hint.contains("create"));
        assert!(hint.contains("diff"));
    }

    #[test]
    fn test_quick_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/quick"),
            "/quick should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_quick_arg_hint() {
        let hint = command_arg_hint("quick");
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("question"));
    }

    #[test]
    fn test_quick_not_unknown() {
        assert!(!is_unknown_command("/quick"));
        assert!(!is_unknown_command("/quick how do I reverse a list?"));
    }

    #[test]
    fn test_discover_custom_commands_empty() {
        // Non-existent directories should return empty vec
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project_cmds");
        let global = tmp.path().join("global_cmds");
        let result = discover_custom_commands_from(Some((project.as_path(), global.as_path())));
        assert!(result.is_empty());
    }

    #[test]
    fn test_discover_custom_commands_finds_files() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project_cmds");
        let global = tmp.path().join("global_cmds");
        std::fs::create_dir_all(&project).unwrap();

        std::fs::write(project.join("review.md"), "Review the diff").unwrap();
        std::fs::write(project.join("deploy.md"), "Deploy to prod").unwrap();
        // Non-.md files should be ignored
        std::fs::write(project.join("notes.txt"), "not a command").unwrap();

        let result = discover_custom_commands_from(Some((project.as_path(), global.as_path())));
        assert_eq!(result.len(), 2);
        assert!(result
            .iter()
            .any(|(n, c)| n == "review" && c == "Review the diff"));
        assert!(result
            .iter()
            .any(|(n, c)| n == "deploy" && c == "Deploy to prod"));
    }

    #[test]
    fn test_custom_command_project_overrides_global() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project_cmds");
        let global = tmp.path().join("global_cmds");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&global).unwrap();

        std::fs::write(project.join("review.md"), "project review").unwrap();
        std::fs::write(global.join("review.md"), "global review").unwrap();
        std::fs::write(global.join("lint.md"), "global lint").unwrap();

        let result = discover_custom_commands_from(Some((project.as_path(), global.as_path())));
        assert_eq!(result.len(), 2);
        // Project-local should override global for same name
        let review = result.iter().find(|(n, _)| n == "review").unwrap();
        assert_eq!(review.1, "project review");
        // Global-only command should still be present
        let lint = result.iter().find(|(n, _)| n == "lint").unwrap();
        assert_eq!(lint.1, "global lint");
    }

    #[test]
    fn test_closest_match_finds_near_miss() {
        let candidates = &["status", "off", "all", "lint"];
        assert_eq!(closest_match("statsu", candidates, 2), Some("status"));
        assert_eq!(closest_match("of", candidates, 2), Some("off"));
    }

    #[test]
    fn test_closest_match_rejects_far_words() {
        // Negative side (Days 122-124 lesson): a word far from every
        // candidate must return None so callers can accept it as free text.
        let candidates = &["status", "off", "all", "lint"];
        assert_eq!(closest_match("pytest", candidates, 2), None);
        assert_eq!(closest_match("investigate", candidates, 2), None);
    }

    #[test]
    fn test_closest_match_exact_word_is_distance_zero() {
        // closest_match does NOT filter exact matches — callers that treat
        // exact subcommands specially must check membership first (documented
        // contract). An exact word matches itself at distance 0.
        let candidates = &["status", "off"];
        assert_eq!(closest_match("status", candidates, 2), Some("status"));
    }

    #[test]
    fn test_closest_match_empty_candidates() {
        assert_eq!(closest_match("anything", &[], 2), None);
    }

    #[test]
    fn test_goal_subcommands_cover_every_documented_verb() {
        // Drift guard (#722): `/goal verify` was routed by handle_goal and
        // documented in /help goal, but missing from GOAL_SUBCOMMANDS — so the
        // inline hint never named it and `/goal ver<Tab>` completed nothing. A
        // working feature invisible to the surfaces users discover it through.
        //
        // Direction that catches THAT drift: every verb the help entry writes
        // as `/goal <verb>` must be in the completion table.
        let entry = crate::help_data::command_help("goal").expect("/goal has a help entry");
        for line in entry.lines() {
            for occurrence in line.split("/goal ").skip(1) {
                let verb = occurrence.split_whitespace().next().unwrap_or("");
                // A verb is a bare lowercase word. Skip placeholders (`<desc>`)
                // and prose that merely follows the command name (the entry's
                // own title line reads "/goal — Set, view, or check ...").
                if verb.is_empty() || !verb.chars().all(|c| c.is_ascii_lowercase()) {
                    continue;
                }
                assert!(
                    GOAL_SUBCOMMANDS.contains(&verb),
                    "/help goal documents `/goal {verb}` but GOAL_SUBCOMMANDS omits it \
                     — the hint and tab-completion will never offer it"
                );
            }
        }
    }

    #[test]
    fn test_goal_subcommands_have_no_phantoms() {
        // Opposite direction (#702's failure): a completion table must not
        // advertise a verb the help entry doesn't document.
        let entry = crate::help_data::command_help("goal").expect("/goal has a help entry");
        for sub in GOAL_SUBCOMMANDS {
            assert!(
                entry.contains(&format!("/goal {sub}")),
                "GOAL_SUBCOMMANDS offers `{sub}` but /help goal never documents `/goal {sub}`"
            );
        }
    }
}
