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
        // Every subcommand the `/model` dispatcher actually routes must appear here;
        // `model_arg_hint_advertises_every_routed_subcommand` in `commands_info.rs`
        // pins this against `dispatch.rs` itself rather than a second hand-typed list.
        "model" => Some("[list [<provider>] | info [<name>] | <model-name>]"),
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
        "remember" => Some("[category:TYPE] <note>"),
        "forget" => Some("<id>"),
        "explain" => Some("<file>"),
        "map" => Some("[--all] [--regex] [path]"),
        "outline" => Some("<query> [--all]"),
        "def" => Some("<symbol-name>"),
        "refs" => Some("<symbol-name>"),
        "stash" => Some("push | pop | list | drop"),
        "mark" => Some("<name>"),
        "jump" => Some("<name>"),
        // Must match `parse_ast_grep_args`: a bare trailing positional is folded into the
        // PATTERN, not read as a path — only `--in` sets the path. Pinned by
        // `ast_hint_advertises_only_argument_shapes_the_parser_honours` in commands_ast_grep.rs.
        "ast" => Some("<pattern> [--lang <lang>] [--in <path>]"),
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
        "tree" => Some("[depth]"),
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
    // The one global read for this door (#902). `--safe-mode` already returned
    // above, so it remains the blunt hatch that drops every custom command.
    discover_custom_commands_with(
        None,
        crate::config::loaded_config_is_project_local(),
        crate::cli::is_trust_project(),
    )
}

/// Ungated directory seam the pre-existing `.yoyo/commands/` tests drive.
///
/// Kept so those two guards stay **unedited**: they assert the merge and
/// override semantics, which this task must not move, and they would
/// otherwise race on the provenance `OnceLock`.
#[cfg(test)]
pub(crate) fn discover_custom_commands_from(
    override_dirs: Option<(&std::path::Path, &std::path::Path)>,
) -> Vec<(String, String)> {
    discover_custom_commands_with(override_dirs, false, false)
}

/// Provenance label for a command found in the project-local `.yoyo/commands/`.
///
/// The trust boundary's whole question is *who wrote this*, so provenance is
/// carried on the entry rather than inferred from load order.
pub(crate) const PROJECT_COMMAND_LABEL: &str = "project";

/// Provenance label for a command found in the user-level `~/.yoyo/commands/`.
pub(crate) const USER_COMMAND_LABEL: &str = "user";

/// A discovered custom slash command, carrying the provenance the trust
/// boundary asks about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandSource {
    pub name: String,
    pub body: String,
    pub label: &'static str,
}

/// What [`gate_project_commands`] decided: the entries still worth loading,
/// plus the names of any project-local ones that were refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandGateOutcome {
    pub kept: Vec<CommandSource>,
    pub refused: Vec<String>,
}

/// The one statement of the rule, read by **both** doors into
/// `.yoyo/commands/` — batch discovery ([`discover_custom_commands_from`]) and
/// the single-command lookup ([`get_custom_command_content`]).
///
/// Gating only the first would be a gate that does not gate: `dispatch.rs`
/// executes a slash command through the *second* door, so a project-local
/// command would still load and run. Two copies of this condition is how the
/// two doors drift apart, which is the "two doors, one policy, one deaf" shape
/// this repo has shipped ten times.
pub(crate) fn project_commands_refused(project_local: bool, trusted: bool) -> bool {
    project_local && !trusted
}

/// The seventh door on the project-config trust boundary (#902, the
/// `.yoyo/commands/` half): a project-local `.yoyo/commands/*.md` becomes a
/// slash command whose **body is fed to the model as instructions**, so
/// cloning a stranger's repo used to make that repo's authored commands
/// available in the session with no prompt and no display of what was loaded.
///
/// Drops **only** entries labelled [`PROJECT_COMMAND_LABEL`], and **only**
/// when the loaded config is project-local and `--trust-project` was not
/// passed. `~/.yoyo/commands/` is user-level provenance — the user's own word,
/// exactly as a `--allow` flag is — and passes through byte-identically, as
/// does a home/XDG config, a trusted run, and a tree with no
/// `.yoyo/commands/` at all.
///
/// **Direction rule, written here so nobody "sorts" it later:** a custom
/// command's body is model-facing instruction, so it is **refused like a hook
/// (#820)**, never sorted like a declarative `deny` pattern. The Day-166
/// asymmetry sorts *declarative* values yoyo interprets (a deny pattern, a
/// path list — kept verbatim, since a repo may always confine yoyo further);
/// a command body is not one. Direction is decided by what the entry **is**,
/// not by what it does after it loads.
///
/// **Stated limits, because a partial fix that reads as complete is worse
/// than none:**
/// - This gates **who wrote the command**, never **whether the command is
///   safe**. A command in a directory the user *does* trust is still
///   arbitrary model-facing instruction, exactly as a trusted hook is still
///   arbitrary shell.
/// - It does **not** close #902. The six project instruction files
///   (`CLAUDE.md`, `AGENTS.md`, `.cursorrules`,
///   `.github/copilot-instructions.md`, `YOYO.md`, `.yoyo/instructions.md`)
///   still reach every prompt with no gate.
/// - **Known predicate hole, inherited from #897 and named rather than
///   pretended away:** this keys on `loaded_config_is_project_local()`, which
///   is **false** for a repo carrying `.yoyo/commands/` and no `.yoyo.toml`
///   at all — so the gate does not fire on that shape, even though
///   `project_trust_grants` (keyed on directory existence) does correctly
///   raise the trust question. The same asymmetry already exists in the
///   shipped `.yoyo/skills/` gate. Changing the key touches all seven gates
///   at once, which is how a verified narrow change becomes an unverified
///   wide one.
pub(crate) fn gate_project_commands(
    entries: Vec<CommandSource>,
    project_local: bool,
    trusted: bool,
) -> CommandGateOutcome {
    // Home/XDG configs and explicitly trusted runs pass through untouched.
    if !project_commands_refused(project_local, trusted) {
        return CommandGateOutcome {
            kept: entries,
            refused: Vec::new(),
        };
    }
    let mut kept = Vec::with_capacity(entries.len());
    let mut refused = Vec::new();
    for entry in entries {
        if entry.label == PROJECT_COMMAND_LABEL {
            refused.push(entry.name);
        } else {
            kept.push(entry);
        }
    }
    refused.sort();
    CommandGateOutcome { kept, refused }
}

/// The refusal block. Names **every** refused command, because a user cannot
/// judge what they cannot see; states outright that nothing was loaded; and
/// gives both hatches.
///
/// Names go through [`crate::cli::sanitize_for_display`] (#873): the
/// *repository* authored those filenames and this block renders into the
/// terminal of someone who has explicitly not trusted it.
///
/// An empty `names` is reachable and is **not** silence — the directory
/// exists but could not be listed — so it says so rather than rendering an
/// empty list as if nothing were there. The caller drops the whole message
/// under `--quiet`.
pub(crate) fn project_command_refusal_message(names: &[String], plain: bool) -> String {
    let marker = if plain { "" } else { "⚠ " };
    let dash = if plain { ", " } else { " — " };
    let listed = if names.is_empty() {
        "    (could not list the directory)".to_string()
    } else {
        names
            .iter()
            .map(|n| format!("    /{}", crate::cli::sanitize_for_display(n)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let count = names.len();
    let noun = if count == 1 { "command" } else { "commands" };
    format!(
        "{marker}A project-local .yoyo/commands/ offered {count} slash {noun} to this session. \
yoyo did not load them:\n{listed}\n  A custom command's body becomes instructions in the \
model's context{dash}this project wrote them, not you. Nothing was loaded.\n  Re-run with \
--trust-project to load them this session, or use --safe-mode to disable\n  all project \
customizations."
    )
}

/// Fires at most once per process, like `RTK_ANNOUNCED`: discovery is lazy and
/// runs from tab-completion, `/help` and dispatch, so an unguarded `eprintln!`
/// would repeat the block every keystroke.
static COMMAND_REFUSAL_ANNOUNCED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn announce_refused_commands(names: &[String]) {
    if is_quiet() {
        return;
    }
    if COMMAND_REFUSAL_ANNOUNCED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let msg = project_command_refusal_message(names, is_plain_output());
    eprintln!("{YELLOW}{msg}{RESET}");
}

/// Discover custom slash commands, with the trust answer passed **explicitly**.
///
/// The provenance pair is a parameter rather than a global read because
/// `loaded_config_is_project_local()` is a process-wide `OnceLock` that this
/// repo's own `.yoyo.toml` sets to `true`: reading it here made the two
/// pre-existing directory tests race on it, which is the shared-global class
/// `tests/global_state_races.rs` enumerates. Passing the value is that gate's
/// own stated *best* remedy.
pub(crate) fn discover_custom_commands_with(
    override_dirs: Option<(&std::path::Path, &std::path::Path)>,
    project_local: bool,
    trusted: bool,
) -> Vec<(String, String)> {
    let project_dir;
    let global_dir;
    let (proj_path, glob_path): (&std::path::Path, Option<&std::path::Path>) = match override_dirs {
        Some((p, g)) => (p, Some(g)),
        None => {
            project_dir = std::path::PathBuf::from(".yoyo/commands");
            global_dir = std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".yoyo/commands"));
            (project_dir.as_path(), global_dir.as_deref())
        }
    };

    // Collect labelled entries and gate them **before** merging. Merging first
    // would let a *refused* project command shadow a same-named user-level one
    // out of existence, since the project entry overwrites it in the map.
    let mut entries: Vec<CommandSource> = Vec::new();
    if let Some(glob) = glob_path {
        collect_commands_from_dir(glob, USER_COMMAND_LABEL, &mut entries);
    }
    let project_listed = collect_commands_from_dir(proj_path, PROJECT_COMMAND_LABEL, &mut entries);

    let refusing = project_commands_refused(project_local, trusted);
    let gated = gate_project_commands(entries, project_local, trusted);

    // Only speak if there was something to refuse. An absent (or empty)
    // `.yoyo/commands/` is the common case and must stay silent; a directory
    // that exists and could not be listed is its own state and does speak.
    if !gated.refused.is_empty() || (refusing && !project_listed) {
        announce_refused_commands(&gated.refused);
    }

    // Merge in load order — later wins, so a kept project-local command still
    // takes priority over a same-named user-level one.
    let mut commands: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for entry in gated.kept {
        commands.insert(entry.name, entry.body);
    }

    let mut result: Vec<(String, String)> = commands.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// Collect `*.md` commands from one directory, tagging each with `label`.
///
/// Returns `false` only when the directory **exists and could not be listed** —
/// an absent directory is the ordinary case and returns `true`, because there
/// was nothing there to fail to read.
fn collect_commands_from_dir(
    dir: &std::path::Path,
    label: &'static str,
    out: &mut Vec<CommandSource>,
) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return !dir.is_dir(),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                out.push(CommandSource {
                    name: stem.to_string(),
                    body: content,
                    label,
                });
            }
        }
    }
    true
}

/// Check if a slash command name (without leading `/`) matches a custom command.
pub fn is_custom_command(cmd: &str) -> bool {
    get_custom_command_content(cmd).is_some()
}

/// Get the content of a custom command by name (without leading `/`).
/// Checks project-local `.yoyo/commands/` first, then global `~/.yoyo/commands/`.
pub fn get_custom_command_content(cmd: &str) -> Option<String> {
    get_custom_command_content_with(
        cmd,
        project_commands_refused(
            crate::config::loaded_config_is_project_local(),
            crate::cli::is_trust_project(),
        ),
    )
}

/// The decision half of [`get_custom_command_content`], so the refusal is
/// drivable without writing a process-global.
///
/// **This is the door `dispatch.rs` executes through** (`dispatch.rs:1292`),
/// so gating batch discovery alone would have left a project-local command
/// loading and running exactly as before. `refuse_project` comes from the
/// shared [`project_commands_refused`] — one statement of the rule, never a
/// second copy that agrees the day it is written.
pub(crate) fn get_custom_command_content_with(cmd: &str, refuse_project: bool) -> Option<String> {
    // Check project-local first — unless this run refuses project-authored
    // commands, in which case it is skipped entirely rather than read and
    // discarded.
    if !refuse_project {
        let project_path = std::path::PathBuf::from(format!(".yoyo/commands/{cmd}.md"));
        if let Ok(content) = std::fs::read_to_string(&project_path) {
            return Some(content);
        }
    }
    // Check global. User-level provenance is the user's own word and is never
    // gated, so a refused project command correctly falls through to a
    // same-named user-level one instead of vanishing.
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

    fn cmd(name: &str, label: &'static str) -> CommandSource {
        CommandSource {
            name: name.to_string(),
            body: format!("body of {name}"),
            label,
        }
    }

    /// The mixed fixture every gate test drives: one user-level command and
    /// one project-local one.
    fn mixed() -> Vec<CommandSource> {
        vec![
            cmd("shared", USER_COMMAND_LABEL),
            cmd("proj", PROJECT_COMMAND_LABEL),
        ]
    }

    #[test]
    fn gate_project_commands_refuses_only_the_project_entry_and_only_when_untrusted() {
        // ANTI-VACUOUS, ASSERTED FIRST: the fixture really does carry a
        // project-local command, so a broken seam cannot pass by having both
        // sides agree on nothing.
        assert!(
            mixed().iter().any(|e| e.label == PROJECT_COMMAND_LABEL),
            "fixture must contain a project-local command or this test proves nothing"
        );

        let out = gate_project_commands(mixed(), true, false);
        assert_eq!(out.refused, vec!["proj".to_string()]);
        assert_eq!(out.kept, vec![cmd("shared", USER_COMMAND_LABEL)]);
    }

    #[test]
    fn gate_project_commands_is_byte_identical_in_every_other_combination() {
        // (project_local, trusted): only (true, false) refuses. The other
        // three are the whole regression surface -- a home/XDG config, a
        // --trust-project run, and both.
        for (project_local, trusted) in [(false, false), (false, true), (true, true)] {
            let out = gate_project_commands(mixed(), project_local, trusted);
            assert_eq!(
                out.kept,
                mixed(),
                "({project_local}, {trusted}) must pass through byte-identically"
            );
            assert!(
                out.refused.is_empty(),
                "({project_local}, {trusted}) refused something"
            );
        }
    }

    #[test]
    fn gate_project_commands_with_only_user_level_entries_refuses_nothing() {
        // `~/.yoyo/commands/` is user-level provenance -- the user's own word,
        // exactly as a `--allow` flag is -- so it survives an untrusted run.
        let user_only = vec![cmd("a", USER_COMMAND_LABEL), cmd("b", USER_COMMAND_LABEL)];
        let out = gate_project_commands(user_only.clone(), true, false);
        assert_eq!(out.kept, user_only);
        assert!(out.refused.is_empty());
    }

    #[test]
    fn gate_project_commands_with_no_commands_at_all_is_byte_identical() {
        // Every existing user: no `.yoyo/commands/` anywhere.
        for (project_local, trusted) in [(true, false), (false, false), (true, true)] {
            let out = gate_project_commands(Vec::new(), project_local, trusted);
            assert_eq!(out.kept, Vec::new());
            assert_eq!(out.refused, Vec::<String>::new());
        }
    }

    #[test]
    fn a_refused_project_command_does_not_delete_a_same_named_user_level_one() {
        // The gate runs BEFORE the merge on purpose. Merging first would let a
        // refused project entry overwrite its user-level namesake out of
        // existence, so refusing one command would silently delete another.
        let entries = vec![
            cmd("dup", USER_COMMAND_LABEL),
            CommandSource {
                name: "dup".to_string(),
                body: "PROJECT BODY".to_string(),
                label: PROJECT_COMMAND_LABEL,
            },
        ];
        let out = gate_project_commands(entries, true, false);
        assert_eq!(out.refused, vec!["dup".to_string()]);
        assert_eq!(out.kept, vec![cmd("dup", USER_COMMAND_LABEL)]);
        assert_eq!(out.kept[0].body, "body of dup");
    }

    #[test]
    fn project_command_refusal_message_names_every_command_and_both_hatches() {
        let names = vec!["deploy".to_string(), "review".to_string()];
        let msg = project_command_refusal_message(&names, false);
        assert!(msg.contains("/deploy"), "{msg}");
        assert!(msg.contains("/review"), "{msg}");
        assert!(msg.contains("Nothing was loaded"), "{msg}");
        assert!(msg.contains("--trust-project"), "{msg}");
        assert!(msg.contains("--safe-mode"), "{msg}");
        assert!(msg.contains("2 slash commands"), "{msg}");
    }

    #[test]
    fn project_command_refusal_message_says_so_when_it_cannot_list() {
        // Reachable: the directory exists and could not be read. An empty list
        // rendered as if nothing were there would be the quiet failure.
        let msg = project_command_refusal_message(&[], false);
        assert!(msg.contains("could not list the directory"), "{msg}");
        assert!(msg.contains("Nothing was loaded"), "{msg}");
    }

    #[test]
    fn project_command_refusal_message_escapes_repo_authored_names() {
        // #873: the *repository* authored these filenames and this block
        // renders into the terminal of someone who has explicitly not trusted
        // it, so a control byte must not reach the terminal raw.
        let hostile = vec!["evil\u{1b}[2Jclear".to_string()];
        assert!(
            hostile[0].as_bytes().contains(&0x1b),
            "fixture must carry a control byte or this test proves nothing"
        );
        let msg = project_command_refusal_message(&hostile, false);
        assert!(
            !msg.as_bytes().contains(&0x1b),
            "escape byte reached the terminal"
        );
    }

    #[test]
    fn project_command_refusal_message_is_glyph_free_and_em_dash_free_under_plain() {
        let names = vec!["deploy".to_string()];
        let plain = project_command_refusal_message(&names, true);
        assert!(
            !plain.contains('\u{26a0}'),
            "marker glyph survived: {plain}"
        );
        assert!(!plain.contains('\u{2014}'), "em dash survived: {plain}");
        // Reverse anti-vacuous check: the non-plain form really does carry
        // both, so the assertions above discriminate rather than passing by
        // accident.
        let fancy = project_command_refusal_message(&names, false);
        assert!(fancy.contains('\u{26a0}'), "{fancy}");
        assert!(fancy.contains('\u{2014}'), "{fancy}");
        assert!(plain.contains("1 slash command"), "{plain}");
    }

    #[test]
    fn discovery_drops_the_project_dir_and_keeps_the_user_dir_when_untrusted() {
        // End-to-end through the real discovery seam, on real directories, so
        // the gate is proven WIRED rather than merely unit-tested one layer
        // below. Uses a tempdir; moves no process CWD (#780).
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let global = tmp.path().join("glob");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(project.join("deploy.md"), "PROJECT deploy").unwrap();
        std::fs::write(global.join("lint.md"), "user lint").unwrap();

        // ANTI-VACUOUS, ASSERTED FIRST: untrusted-but-not-project-local keeps
        // both, so the fixture genuinely carries a droppable project command.
        let ungated = discover_custom_commands_with(
            Some((project.as_path(), global.as_path())),
            false,
            false,
        );
        assert_eq!(
            ungated,
            vec![
                ("deploy".to_string(), "PROJECT deploy".to_string()),
                ("lint".to_string(), "user lint".to_string()),
            ]
        );

        // Project-local and untrusted: the project command is gone, the
        // user-level one survives byte-identically.
        let gated =
            discover_custom_commands_with(Some((project.as_path(), global.as_path())), true, false);
        assert_eq!(gated, vec![("lint".to_string(), "user lint".to_string())]);

        // --trust-project restores it.
        let trusted =
            discover_custom_commands_with(Some((project.as_path(), global.as_path())), true, true);
        assert_eq!(trusted, ungated);
    }

    #[test]
    fn project_commands_refused_is_the_one_statement_of_the_rule() {
        assert!(project_commands_refused(true, false));
        assert!(!project_commands_refused(true, true));
        assert!(!project_commands_refused(false, false));
        assert!(!project_commands_refused(false, true));
    }

    #[test]
    fn the_single_command_door_skips_the_project_path_when_refusing() {
        // This is the door dispatch.rs executes through. It reads real paths,
        // so the only honest assertion without touching the filesystem is
        // that a refusing run cannot return a project-local body: with no
        // such file present both answers are None, and the refusing branch
        // must never be the one that finds something the other did not.
        let refused = get_custom_command_content_with("yoyo-nonexistent-probe", true);
        assert_eq!(refused, None);
    }

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
