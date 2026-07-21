//! Spawn subsystem: /spawn command, task tracking, subagent context building.
//!
//! Extracted from `commands_session.rs` — the spawn feature is self-contained
//! with its own types (SpawnStatus, SpawnTask, SpawnTracker, SpawnArgs),
//! parser, context builder, and handler.

use crate::format::*;
use crate::prompt::run_prompt;
use crate::prompt_utils::summarize_message;
use crate::sync_util::lock_or_recover;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use yoagent::types::{AgentMessage, Usage};

// ── /spawn ────────────────────────────────────────────────────────────────

/// Status of a tracked spawn task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnStatus {
    Running,
    Completed,
    Failed(String),
}

impl std::fmt::Display for SpawnStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnStatus::Running => write!(f, "running"),
            SpawnStatus::Completed => write!(f, "completed"),
            SpawnStatus::Failed(e) => write!(f, "failed: {e}"),
        }
    }
}

/// A completed spawn worker's committed work, ready for review.
///
/// When a spawn worker finishes in an isolated worktree and left file changes
/// behind, those changes are committed to a named branch that survives
/// worktree cleanup. This record carries the branch name and a one-line
/// diffstat so the user (and the main agent) know where to look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnHandoff {
    /// The branch the worker's commit lives on (e.g. `spawn/3-1719...`).
    pub branch: String,
    /// One-line diffstat summary (e.g. `3 files changed (+42/-7)`).
    pub diffstat: String,
}

impl SpawnHandoff {
    /// One-line human summary: `ready to review: branch <b> — <diffstat>`.
    pub fn summary_line(&self) -> String {
        format!(
            "ready to review: branch {} — {}",
            self.branch, self.diffstat
        )
    }

    /// Hint showing how to inspect the handoff branch.
    pub fn review_hint(&self) -> String {
        format!("git diff main...{}", self.branch)
    }
}

/// A tracked spawn task with its metadata and result.
#[derive(Debug, Clone)]
pub struct SpawnTask {
    /// Unique identifier for this spawn (1-indexed).
    pub id: usize,
    /// The task description given by the user.
    pub task: String,
    /// Current status.
    pub status: SpawnStatus,
    /// The subagent's output, if completed.
    pub result: Option<String>,
    /// Optional output file path.
    pub output_path: Option<String>,
    /// Whether this spawn was launched in the background.
    pub background: bool,
    /// Committed worktree changes ready for review, if any.
    pub handoff: Option<SpawnHandoff>,
    /// Whether a post-turn completion notification has already been surfaced
    /// for this (background) spawn. Ensures each finish is announced exactly
    /// once. Non-consuming: the result stays collectable via `/spawn collect`.
    pub notified: bool,
}

/// Thread-safe tracker for multiple spawn tasks.
#[derive(Debug, Clone)]
pub struct SpawnTracker {
    inner: Arc<Mutex<Vec<SpawnTask>>>,
    /// JoinHandles for background spawns, keyed by spawn ID.
    handles: Arc<Mutex<std::collections::HashMap<usize, tokio::task::JoinHandle<()>>>>,
}

impl SpawnTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
            handles: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Register a new spawn task and return its ID.
    pub fn register(&self, task: &str, output_path: Option<String>) -> usize {
        self.register_with_bg(task, output_path, false)
    }

    /// Register a new spawn task with background flag and return its ID.
    pub fn register_with_bg(
        &self,
        task: &str,
        output_path: Option<String>,
        background: bool,
    ) -> usize {
        let mut tasks = lock_or_recover(&self.inner);
        let id = tasks.len() + 1;
        tasks.push(SpawnTask {
            id,
            task: task.to_string(),
            status: SpawnStatus::Running,
            result: None,
            output_path,
            background,
            handoff: None,
            notified: false,
        });
        id
    }

    /// Mark a task as completed with its result.
    /// Convenience wrapper over `complete_with_handoff` (production paths now
    /// always go through the handoff-aware variant; this remains for tests
    /// and future callers that have no worktree).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn complete(&self, id: usize, result: String) {
        self.complete_with_handoff(id, result, None);
    }

    /// Mark a task as completed with its result and an optional handoff
    /// (committed worktree changes ready for review).
    pub fn complete_with_handoff(&self, id: usize, result: String, handoff: Option<SpawnHandoff>) {
        let mut tasks = lock_or_recover(&self.inner);
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.status = SpawnStatus::Completed;
            task.result = Some(result);
            task.handoff = handoff;
        }
    }

    /// Mark a task as failed.
    pub fn fail(&self, id: usize, error: String) {
        let mut tasks = lock_or_recover(&self.inner);
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.status = SpawnStatus::Failed(error);
            task.result = None;
        }
    }

    /// Get a snapshot of all tracked tasks.
    pub fn snapshot(&self) -> Vec<SpawnTask> {
        lock_or_recover(&self.inner).clone()
    }

    /// Count tasks by status.
    pub fn count_by_status(&self) -> (usize, usize, usize) {
        let tasks = lock_or_recover(&self.inner);
        let running = tasks
            .iter()
            .filter(|t| t.status == SpawnStatus::Running)
            .count();
        let completed = tasks
            .iter()
            .filter(|t| t.status == SpawnStatus::Completed)
            .count();
        let failed = tasks
            .iter()
            .filter(|t| matches!(t.status, SpawnStatus::Failed(_)))
            .count();
        (running, completed, failed)
    }

    /// Store a JoinHandle for a background spawn (for abort if needed).
    pub fn store_handle(&self, id: usize, handle: tokio::task::JoinHandle<()>) {
        let mut handles = lock_or_recover(&self.handles);
        handles.insert(id, handle);
    }

    /// Try to collect a background spawn's result.
    /// Returns `Ok(Some((task, result, handoff)))` if finished, `Ok(None)` if
    /// still running. Returns `Err` if the spawn doesn't exist or wasn't a
    /// background spawn.
    #[allow(clippy::type_complexity)]
    pub fn try_collect(
        &self,
        id: usize,
    ) -> Result<Option<(String, String, Option<SpawnHandoff>)>, String> {
        let tasks = lock_or_recover(&self.inner);
        let task = tasks.iter().find(|t| t.id == id);
        match task {
            None => Err(format!("no spawn #{id} found")),
            Some(t) if !t.background => {
                if t.status == SpawnStatus::Completed {
                    Ok(t.result
                        .clone()
                        .map(|r| (t.task.clone(), r, t.handoff.clone())))
                } else {
                    Err(format!("spawn #{id} was not a background spawn"))
                }
            }
            Some(t) if t.status == SpawnStatus::Completed => Ok(t
                .result
                .clone()
                .map(|r| (t.task.clone(), r, t.handoff.clone()))),
            Some(t) if matches!(t.status, SpawnStatus::Failed(_)) => {
                Err(format!("spawn #{id} {}", t.status))
            }
            _ => Ok(None), // Still running
        }
    }

    /// Return one-line review hints for background spawns that have finished
    /// (Completed or Failed) since the last check, marking them notified so
    /// each is surfaced exactly once. Non-consuming: the result stays
    /// collectable via `/spawn collect <id>`.
    ///
    /// Foreground spawns are never reported here — the user already blocked on
    /// them, so there is nothing to notify.
    pub fn newly_finished_background(&self) -> Vec<String> {
        let mut inner = lock_or_recover(&self.inner);
        let mut out = Vec::new();
        for t in inner.iter_mut() {
            if t.background && !t.notified {
                match &t.status {
                    SpawnStatus::Completed => {
                        t.notified = true;
                        out.push(format!(
                            "✓ background spawn #{} finished — `/spawn collect {}` to review",
                            t.id, t.id
                        ));
                    }
                    SpawnStatus::Failed(msg) => {
                        t.notified = true;
                        // char-boundary-safe truncation (CLAUDE.md safety rule;
                        // byte slicing caused #250 crashes on multi-byte chars).
                        let mut end = msg.len().min(120);
                        while end > 0 && !msg.is_char_boundary(end) {
                            end -= 1;
                        }
                        let truncated = if end < msg.len() {
                            format!("{}…", &msg[..end])
                        } else {
                            msg.clone()
                        };
                        out.push(format!(
                            "✗ background spawn #{} failed: {}",
                            t.id, truncated
                        ));
                    }
                    SpawnStatus::Running => {}
                }
            }
        }
        out
    }
}

#[cfg(test)]
impl SpawnTracker {
    /// Get a task by ID.
    pub fn get(&self, id: usize) -> Option<SpawnTask> {
        let tasks = lock_or_recover(&self.inner);
        tasks.iter().find(|t| t.id == id).cloned()
    }

    /// Number of tracked tasks.
    pub fn len(&self) -> usize {
        lock_or_recover(&self.inner).len()
    }

    /// Whether the tracker has no tasks.
    pub fn is_empty(&self) -> bool {
        lock_or_recover(&self.inner).is_empty()
    }
}

/// Maximum number of parallel tasks allowed in a single `/spawn --parallel`.
pub const MAX_PARALLEL_TASKS: usize = 10;

/// Parsed `/spawn` command input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnArgs {
    /// The task for the subagent.
    pub task: String,
    /// Optional output file path (`-o <path>`).
    pub output_path: Option<String>,
    /// Whether to run in background (`--bg` flag).
    pub background: bool,
    /// If set, this is a `/spawn collect <id>` request.
    pub collect_id: Option<usize>,
    /// Optional model override for the subagent (`--model <name>`).
    pub model: Option<String>,
    /// Optional custom system prompt for the subagent (`--system <prompt>`).
    pub system_prompt: Option<String>,
    /// If set, multiple tasks to run in parallel (from `--parallel` flag).
    /// `None` for normal spawn, `Some(tasks)` for parallel dispatch.
    pub parallel_tasks: Option<Vec<String>>,
    /// Whether to push the handoff branch and open a draft PR on completion
    /// (`--pr` flag). Strictly opt-in — defaults to false.
    pub pr: bool,
}

/// Parse the `/spawn` command input, extracting flags and task.
///
/// Supports:
/// - `/spawn <task>` — run a task synchronously
/// - `/spawn --bg <task>` — run a task in the background
/// - `/spawn -o <path> <task>` — run a task and capture output to a file
/// - `/spawn --bg -o <path> <task>` — background with output capture
/// - `/spawn --model <name> <task>` — use a specific model
/// - `/spawn --system <prompt> <task>` — custom system prompt (quoted for multi-word)
/// - `/spawn --pr <task>` — push handoff branch + open a draft PR on completion
/// - `/spawn collect <id>` — collect a finished background spawn
///
/// Returns `None` if no task or if this is a subcommand like `status`.
pub fn parse_spawn_args(input: &str) -> Option<SpawnArgs> {
    let rest = input.strip_prefix("/spawn").unwrap_or("").trim();
    if rest.is_empty() || rest == "status" {
        return None;
    }

    // Handle `/spawn collect <id>`
    if let Some(collect_rest) = rest.strip_prefix("collect") {
        let collect_rest = collect_rest.trim();
        if let Ok(id) = collect_rest.parse::<usize>() {
            return Some(SpawnArgs {
                task: String::new(),
                output_path: None,
                background: false,
                collect_id: Some(id),
                model: None,
                system_prompt: None,
                parallel_tasks: None,
                pr: false,
            });
        }
        // "collect" without valid id — fall through to show usage
        return None;
    }

    let mut words: Vec<&str> = rest.split_whitespace().collect();
    let mut background = false;
    let mut parallel = false;
    let mut pr = false;
    let mut output_path = None;
    let mut model = None;
    let mut system_prompt = None;

    // Extract flags from the front.
    // For --system, the value can be a quoted multi-word string. Since we
    // split on whitespace above, we need to rejoin quoted segments when we
    // encounter --system. We do this by checking if the next token after
    // --system starts with a quote.
    while !words.is_empty() {
        if words[0] == "--bg" {
            background = true;
            words.remove(0);
        } else if words[0] == "--parallel" {
            parallel = true;
            words.remove(0);
        } else if words[0] == "--pr" {
            pr = true;
            words.remove(0);
        } else if words[0] == "-o" && words.len() > 1 {
            output_path = Some(words[1].to_string());
            words.remove(0); // remove "-o"
            words.remove(0); // remove the path (now at position 0)
        } else if words[0] == "--model" && words.len() > 1 {
            model = Some(words[1].to_string());
            words.remove(0); // remove "--model"
            words.remove(0); // remove the model name (now at position 0)
        } else if words[0] == "--system" && words.len() > 1 {
            words.remove(0); // remove "--system"
                             // Check if next token starts with a quote
            if words[0].starts_with('"') {
                // Consume tokens until we find one that ends with a quote
                let mut prompt_parts: Vec<String> = Vec::new();
                while !words.is_empty() {
                    let w = words.remove(0).to_string();
                    prompt_parts.push(w.clone());
                    if w.ends_with('"') {
                        break;
                    }
                }
                let joined = prompt_parts.join(" ");
                // Strip surrounding quotes
                let trimmed = joined
                    .strip_prefix('"')
                    .unwrap_or(&joined)
                    .strip_suffix('"')
                    .unwrap_or(&joined);
                system_prompt = Some(trimmed.to_string());
            } else {
                // Single unquoted word
                system_prompt = Some(words.remove(0).to_string());
            }
        } else {
            break; // stop processing flags once we hit a non-flag word
        }
    }

    // `--pr` may also appear after the task text (e.g. `/spawn do thing --pr`).
    // Strip any remaining standalone occurrences so the flag never leaks into
    // the task description.
    if words.contains(&"--pr") {
        pr = true;
        words.retain(|w| *w != "--pr");
    }

    let task = words.join(" ");
    if task.is_empty() {
        return None;
    }

    // In parallel mode, split the task text on triple-dash separators
    if parallel {
        let tasks: Vec<String> = task
            .split("---")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if tasks.is_empty() {
            return None;
        }
        return Some(SpawnArgs {
            task: String::new(),
            output_path: None, // -o is incompatible with --parallel
            background: false, // --parallel is implicitly background
            collect_id: None,
            model,
            system_prompt,
            parallel_tasks: Some(tasks),
            pr,
        });
    }

    Some(SpawnArgs {
        task,
        output_path,
        background,
        collect_id: None,
        model,
        system_prompt,
        parallel_tasks: None,
        pr,
    })
}

/// Parse the task from a `/spawn <task>` input (legacy compat).
/// Returns None if no task is provided.
#[cfg(test)]
pub fn parse_spawn_task(input: &str) -> Option<String> {
    parse_spawn_args(input).map(|args| args.task)
}

/// Build a context prompt for a subagent, including project context and
/// a brief summary of the current conversation. This gives the subagent
/// enough context to be useful without overwhelming it.
///
/// Includes:
/// - A base instruction explaining the subagent's role
/// - Project context (CLAUDE.md, git status, etc.) if available
/// - Worktree working directory instruction (if isolated)
/// - A brief summary of the current conversation state
pub fn spawn_context_prompt(
    main_messages: &[AgentMessage],
    project_context: Option<&str>,
    worktree_path: Option<&Path>,
) -> String {
    let mut parts = Vec::new();

    parts.push(
        "You are a subagent spawned from a main coding agent session. \
         Complete the task you are given thoroughly and concisely. \
         Your output will be reported back to the main agent."
            .to_string(),
    );

    // Include worktree isolation notice
    if let Some(wt) = worktree_path {
        parts.push(format!(
            "## Working Directory\n\n\
             You are working in an isolated git worktree at: {}\n\
             All file operations and bash commands should run inside this directory. \
             Use `cd {}` before running commands, or use absolute paths within this worktree.",
            wt.display(),
            wt.display()
        ));
    }

    // Include project context if available
    if let Some(ctx) = project_context {
        let truncated = if ctx.len() > 8000 {
            format!("{}...\n(truncated)", safe_truncate(ctx, 8000))
        } else {
            ctx.to_string()
        };
        parts.push(format!("## Project Context\n\n{truncated}"));
    }

    // Summarize recent conversation for context
    if !main_messages.is_empty() {
        let summary = summarize_conversation_for_spawn(main_messages);
        if !summary.is_empty() {
            parts.push(format!(
                "## Current Conversation Context\n\n\
                 The main agent's recent conversation (for context):\n\n{summary}"
            ));
        }
    }

    parts.join("\n\n")
}

/// Summarize the main agent's conversation for a subagent.
/// Takes the last N messages and produces a brief overview.
pub fn summarize_conversation_for_spawn(messages: &[AgentMessage]) -> String {
    // Take last 10 messages at most for a reasonable summary
    let recent = if messages.len() > 10 {
        &messages[messages.len() - 10..]
    } else {
        messages
    };

    let mut lines = Vec::new();
    for msg in recent {
        let (role, preview) = summarize_message(msg);
        lines.push(format!("- [{role}] {preview}"));
    }
    lines.join("\n")
}

/// Format a spawn result as a context message for the main agent.
pub fn format_spawn_result(
    task: &str,
    result: &str,
    spawn_id: usize,
    handoff: Option<&SpawnHandoff>,
) -> String {
    let result_text = if result.trim().is_empty() {
        "(no output)".to_string()
    } else {
        result.trim().to_string()
    };

    let handoff_text = match handoff {
        Some(h) => format!(
            "\n\n**Handoff:** {}\nInspect with: `{}`",
            h.summary_line(),
            h.review_hint()
        ),
        None => String::new(),
    };

    format!(
        "Subagent #{spawn_id} completed a task. Here is its result:\n\n\
         **Task:** {task}\n\n\
         **Result:**\n{result_text}{handoff_text}"
    )
}

/// Display the status of all tracked spawn tasks.
pub fn handle_spawn_status(tracker: &SpawnTracker) {
    let tasks = tracker.snapshot();
    if tasks.is_empty() {
        println!("{DIM}  (no spawn tasks this session){RESET}\n");
        return;
    }

    let (running, completed, failed) = tracker.count_by_status();
    println!(
        "{DIM}  Spawn tasks: {total} total ({running} running, {completed} completed, {failed} failed)",
        total = tasks.len()
    );
    for task in &tasks {
        let status_icon = match &task.status {
            SpawnStatus::Running => "⏳",
            SpawnStatus::Completed => "✓",
            SpawnStatus::Failed(_) => "✗",
        };
        let task_preview = crate::format::truncate_with_ellipsis(&task.task, 60);
        let output_note = task
            .output_path
            .as_ref()
            .map(|p| format!(" → {p}"))
            .unwrap_or_default();
        match &task.status {
            SpawnStatus::Running => {
                let bg_label = if task.background { " (background)" } else { "" };
                println!(
                    "    {CYAN}{status_icon} #{id}: {task_preview}{bg_label}{output_note}{RESET}",
                    id = task.id
                );
            }
            SpawnStatus::Completed => {
                println!(
                    "    {GREEN}{status_icon} #{id}: {task_preview}{output_note}{RESET}",
                    id = task.id
                );
                if let Some(ref h) = task.handoff {
                    println!("      {GREEN}{}{RESET}", h.summary_line());
                    println!("      {DIM}review with: {}{RESET}", h.review_hint());
                }
            }
            SpawnStatus::Failed(_) => println!(
                "    {RED}{status_icon} #{id}: {task_preview}{output_note}{RESET}",
                id = task.id
            ),
        }
    }
    println!("{RESET}");
}

/// Handle the /spawn command: create a subagent with project context, run a task,
/// and return the result. Supports output capture, background execution, and task tracking.
///
/// Sub-agents run in isolated git worktrees when possible, enabling parallel file
/// edits without git conflicts. Falls back to the current directory if worktree
/// creation fails.
///
/// Returns Some(context_msg) to be injected back into the main conversation, or None.
pub async fn handle_spawn(
    input: &str,
    agent_config: &crate::AgentConfig,
    session_total: &mut Usage,
    model: &str,
    main_messages: &[AgentMessage],
    tracker: &SpawnTracker,
) -> Option<String> {
    let rest = input.strip_prefix("/spawn").unwrap_or("").trim();

    // Handle /spawn status subcommand
    if rest == "status" {
        handle_spawn_status(tracker);
        return None;
    }

    // Handle /spawn worktrees subcommand
    if rest == "worktrees" {
        handle_spawn_worktrees();
        return None;
    }

    // Handle /spawn manifest[s] — read-only inspector for fan-out run manifests
    if rest == "manifest" || rest == "manifests" {
        handle_spawn_manifest_list();
        return None;
    }
    if let Some(id) = rest.strip_prefix("manifest ").map(str::trim) {
        handle_spawn_manifest_show(id);
        return None;
    }

    // Near-miss typo guard: a single word within edit distance 2 of a known
    // subcommand (e.g. "statsu") is a typo, not a task — refuse rather than
    // spawn an agent whose task is the word "statsu".
    if let Some(suggestion) = spawn_near_miss(rest) {
        eprintln!("{RED}  ✗ unknown word '{rest}' — did you mean /spawn {suggestion}?{RESET}");
        println!("{DIM}  (a real spawn task needs a longer description){RESET}\n");
        return None;
    }

    // Clean up stale worktrees from crashed sessions (max age: 1 hour)
    let cwd = std::env::current_dir().unwrap_or_default();
    cleanup_stale_worktrees(&cwd, std::time::Duration::from_secs(3600));

    // Handle /spawn replay [--list|latest|<run_id>] and /spawn runs — the
    // read-back half of the --parallel manifest door (#341): re-launch a
    // recorded fan-out from its manifest.
    if rest == "runs" || rest == "replay --list" {
        handle_spawn_manifest_list();
        return None;
    }
    if rest == "replay" || rest.starts_with("replay ") {
        let arg = rest.strip_prefix("replay").unwrap_or("").trim();
        let id_arg = if arg.is_empty() { None } else { Some(arg) };
        match load_replay_tasks(Path::new(SPAWN_RUNS_DIR), id_arg) {
            Ok((manifest, tasks)) => {
                println!(
                    "{CYAN}  🐙 replaying run {} ({} task(s), recorded {}){RESET}",
                    manifest.run_id,
                    tasks.len(),
                    manifest.created_ts
                );
                let replay_args = SpawnArgs {
                    task: String::new(),
                    output_path: None,
                    background: true,
                    collect_id: None,
                    model: None,
                    system_prompt: None,
                    parallel_tasks: Some(tasks.clone()),
                    pr: false,
                };
                return handle_spawn_parallel(
                    &tasks,
                    &replay_args,
                    agent_config,
                    model,
                    main_messages,
                    tracker,
                );
            }
            Err(e) => {
                eprintln!("{RED}  ✗ {e}{RESET}");
                return None;
            }
        }
    }

    let args = match parse_spawn_args(input) {
        Some(a) => a,
        None => {
            println!("{DIM}  usage: /spawn <task>");
            println!("         /spawn --bg <task>              (run in background)");
            println!("         /spawn --parallel <t1> --- <t2> (run multiple tasks concurrently)");
            println!("         /spawn -o <file> <task>         (capture output to file)");
            println!("         /spawn --model <name> <task>    (use a specific model)");
            println!("         /spawn --system <prompt> <task> (custom system prompt)");
            println!("         /spawn collect <id>             (collect background result)");
            println!("         /spawn status                   (show tracked spawns)");
            println!("         /spawn worktrees                (list active spawn worktrees)");
            println!("  Spawn a subagent with project context to handle a task.");
            println!("  The result is summarized back into your main conversation.");
            println!("  Sub-agents run in isolated git worktrees for parallel safety.");
            println!("  Example: /spawn read src/main.rs and summarize the architecture");
            println!("           /spawn --model claude-haiku-4-5 summarize this file");
            println!("           /spawn --system \"You are a security auditor\" review src/safety.rs{RESET}\n");
            return None;
        }
    };

    // Handle /spawn collect <id>
    if let Some(id) = args.collect_id {
        return handle_spawn_collect(tracker, id);
    }

    // Handle --parallel: launch multiple tasks concurrently
    if let Some(ref tasks) = args.parallel_tasks {
        return handle_spawn_parallel(tasks, &args, agent_config, model, main_messages, tracker);
    }

    // Handle --bg: launch in background
    if args.background {
        return handle_spawn_bg(&args, agent_config, model, main_messages, tracker);
    }

    // Synchronous spawn (existing behavior)
    // Non-intrusive fan-out hint: if the single task actually decomposes into
    // ≥2 independent tasks, point the user at --parallel. Hint only — the single
    // task still launches exactly as before.
    if let Some(hint) = parallel_suggestion(&args.task) {
        println!("{DIM}  {hint}{RESET}");
    }

    // Register task in tracker
    let spawn_id = tracker.register(&args.task, args.output_path.clone());

    // Determine the effective model (override or inherited)
    let effective_model = args.model.as_deref().unwrap_or(model);

    println!("{CYAN}  🐙 spawning subagent #{spawn_id}...{RESET}");
    println!(
        "{DIM}  task: {}{RESET}",
        crate::format::truncate_with_ellipsis(&args.task, 100)
    );
    if args.model.is_some() {
        println!("{DIM}  model: {effective_model}{RESET}");
    }
    if args.system_prompt.is_some() {
        println!("{DIM}  system: (custom){RESET}");
    }

    // Try to create an isolated worktree for this spawn
    let worktree = match create_spawn_worktree(&cwd, spawn_id) {
        Ok(info) => {
            println!(
                "{DIM}  worktree: {} (@ {}){RESET}",
                info.path.display(),
                info.branch
            );
            Some(info)
        }
        Err(e) => {
            eprintln!("{YELLOW}  ⚠ worktree isolation unavailable: {e} (using current dir){RESET}");
            None
        }
    };

    // Load project context for the subagent
    let project_context = crate::cli::load_project_context();
    let context_prompt = spawn_context_prompt(
        main_messages,
        project_context.as_deref(),
        worktree.as_ref().map(|w| w.path.as_path()),
    );

    // Prepend custom system prompt if provided
    let effective_prompt = if let Some(ref sp) = args.system_prompt {
        format!("{sp}\n\n{context_prompt}")
    } else {
        context_prompt
    };

    // Build a fresh agent with context-enriched system prompt.
    // Pin the worker's bash cwd to the worktree when one exists (enforced
    // default confinement, not a sandbox — absolute paths can still escape).
    let mut sub_config = crate::AgentConfig {
        system_prompt: effective_prompt,
        bash_cwd: spawn_bash_cwd(worktree.as_ref()),
        ..clone_agent_config(agent_config)
    };

    // Apply model override — update model, provider, and API key if needed
    if let Some(ref model_override) = args.model {
        apply_model_override(&mut sub_config, model_override);
    }

    // Subagent inherits the same tools and permissions
    let mut sub_agent = sub_config.build_agent();

    // Run the task
    let response = run_prompt(&mut sub_agent, &args.task, session_total, effective_model)
        .await
        .text;

    // Write output to file if -o was specified
    if let Some(ref output_path) = args.output_path {
        match std::fs::write(output_path, &response) {
            Ok(_) => {
                println!("{GREEN}  ✓ output written to {output_path}{RESET}");
            }
            Err(e) => {
                eprintln!("{RED}  error writing to {output_path}: {e}{RESET}");
                tracker.fail(spawn_id, format!("write error: {e}"));
                // Clean up worktree even on failure
                if let Some(ref wt) = worktree {
                    let _ = cleanup_spawn_worktree(&cwd, wt);
                }
                return None;
            }
        }
    }

    // Mark completed in tracker — commit worktree changes first so the
    // handoff (if any) is recorded alongside the result.
    let handoff = worktree
        .as_ref()
        .and_then(|wt| try_worktree_handoff(wt, &args.task, spawn_id, args.pr));
    tracker.complete_with_handoff(spawn_id, response.clone(), handoff.clone());

    // Clean up the worktree
    if let Some(ref wt) = worktree {
        let elapsed = wt.created_at.elapsed();
        if let Err(e) = cleanup_spawn_worktree(&cwd, wt) {
            eprintln!("{YELLOW}  ⚠ worktree cleanup failed: {e}{RESET}");
        } else {
            eprintln!("{DIM}  worktree cleaned up ({elapsed:.1?}){RESET}");
        }
    }

    println!("\n{GREEN}  ✓ subagent #{spawn_id} completed{RESET}");
    println!("{DIM}  injecting result into main conversation...{RESET}\n");

    let context_msg = format_spawn_result(&args.task, &response, spawn_id, handoff.as_ref());
    Some(context_msg)
}

/// Display active spawn worktrees.
fn handle_spawn_worktrees() {
    let cwd = std::env::current_dir().unwrap_or_default();
    let worktrees = list_spawn_worktrees(&cwd);
    if worktrees.is_empty() {
        println!("{DIM}  (no active spawn worktrees){RESET}\n");
        return;
    }
    println!("{DIM}  Active spawn worktrees:{RESET}");
    for wt in &worktrees {
        let name = wt
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| wt.display().to_string());
        // Show the HEAD ref if we can read it
        let head =
            run_git_in(wt, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|_| "?".to_string());
        println!(
            "    {CYAN}{name}{RESET}  {DIM}@ {head}  {}{RESET}",
            wt.display()
        );
    }
    println!();
}

/// Launch a spawn in the background using tokio::spawn.
/// Returns None immediately — the result is collected later with `/spawn collect <id>`.
fn handle_spawn_bg(
    args: &SpawnArgs,
    agent_config: &crate::AgentConfig,
    model: &str,
    main_messages: &[AgentMessage],
    tracker: &SpawnTracker,
) -> Option<String> {
    // Register task in tracker with background flag
    let spawn_id = tracker.register_with_bg(&args.task, args.output_path.clone(), true);

    // Determine the effective model (override or inherited)
    let effective_model = args.model.as_deref().unwrap_or(model);

    println!("{CYAN}  🐙 spawning subagent #{spawn_id} in background...{RESET}");
    println!(
        "{DIM}  task: {}{RESET}",
        crate::format::truncate_with_ellipsis(&args.task, 100)
    );
    if args.model.is_some() {
        println!("{DIM}  model: {effective_model}{RESET}");
    }
    if args.system_prompt.is_some() {
        println!("{DIM}  system: (custom){RESET}");
    }
    println!("{DIM}  use /spawn status to check progress, /spawn collect {spawn_id} to get results{RESET}\n");

    // Try to create an isolated worktree for the background spawn
    let cwd = std::env::current_dir().unwrap_or_default();
    let worktree = match create_spawn_worktree(&cwd, spawn_id) {
        Ok(info) => {
            println!(
                "{DIM}  worktree: {} (@ {}){RESET}",
                info.path.display(),
                info.branch
            );
            Some(info)
        }
        Err(e) => {
            eprintln!("{YELLOW}  ⚠ worktree isolation unavailable: {e} (using current dir){RESET}");
            None
        }
    };

    // Prepare everything the background task needs (clone before moving)
    let project_context = crate::cli::load_project_context();
    let context_prompt = spawn_context_prompt(
        main_messages,
        project_context.as_deref(),
        worktree.as_ref().map(|w| w.path.as_path()),
    );

    // Prepend custom system prompt if provided
    let effective_prompt = if let Some(ref sp) = args.system_prompt {
        format!("{sp}\n\n{context_prompt}")
    } else {
        context_prompt
    };

    // Pin the worker's bash cwd to the worktree when one exists (enforced
    // default confinement, not a sandbox — absolute paths can still escape).
    let mut sub_config = crate::AgentConfig {
        system_prompt: effective_prompt,
        bash_cwd: spawn_bash_cwd(worktree.as_ref()),
        ..clone_agent_config(agent_config)
    };

    // Apply model override — update model, provider, and API key if needed
    if let Some(ref model_override) = args.model {
        apply_model_override(&mut sub_config, model_override);
    }

    let task_text = args.task.clone();
    let open_pr = args.pr;
    let output_path = args.output_path.clone();
    let model = effective_model.to_string();
    let tracker_clone = tracker.clone();

    let handle = tokio::spawn(async move {
        let mut sub_agent = sub_config.build_agent();
        let mut bg_usage = Usage::default();

        let response = run_prompt(&mut sub_agent, &task_text, &mut bg_usage, &model)
            .await
            .text;

        // Write output to file if -o was specified
        if let Some(ref out_path) = output_path {
            if let Err(e) = std::fs::write(out_path, &response) {
                eprintln!("{RED}  ✗ bg spawn #{spawn_id}: error writing to {out_path}: {e}{RESET}");
                tracker_clone.fail(spawn_id, format!("write error: {e}"));
                // Clean up worktree on failure
                if let Some(ref wt) = worktree {
                    let _ = cleanup_spawn_worktree(&cwd, wt);
                }
                return;
            }
        }

        // Mark completed in tracker — commit worktree changes first so the
        // handoff (if any) travels with the result.
        let handoff = worktree
            .as_ref()
            .and_then(|wt| try_worktree_handoff(wt, &task_text, spawn_id, open_pr));
        tracker_clone.complete_with_handoff(spawn_id, response, handoff);

        // Clean up worktree after completion
        if let Some(ref wt) = worktree {
            let elapsed = wt.created_at.elapsed();
            if let Err(e) = cleanup_spawn_worktree(&cwd, wt) {
                eprintln!("{YELLOW}  ⚠ bg spawn #{spawn_id}: worktree cleanup failed: {e}{RESET}");
            } else {
                eprintln!(
                    "{DIM}  bg spawn #{spawn_id}: worktree cleaned up ({elapsed:.1?}){RESET}"
                );
            }
        }
    });

    tracker.store_handle(spawn_id, handle);
    None
}

/// Launch multiple tasks concurrently as background spawns.
///
/// Each task is registered in the tracker and launched via the same background
/// infrastructure as `--bg`. Returns `None` (all tasks are background).
fn handle_spawn_parallel(
    tasks: &[String],
    args: &SpawnArgs,
    agent_config: &crate::AgentConfig,
    model: &str,
    main_messages: &[AgentMessage],
    tracker: &SpawnTracker,
) -> Option<String> {
    if tasks.len() > MAX_PARALLEL_TASKS {
        eprintln!(
            "{RED}  ✗ too many parallel tasks ({}, max {MAX_PARALLEL_TASKS}){RESET}",
            tasks.len()
        );
        return None;
    }

    println!(
        "{CYAN}  🐙 spawning {} parallel subagents...{RESET}",
        tasks.len()
    );

    let mut ids = Vec::with_capacity(tasks.len());
    for task_text in tasks.iter() {
        // Build a SpawnArgs for each individual task, inheriting model/system
        let single_args = SpawnArgs {
            task: task_text.clone(),
            output_path: None,
            background: true,
            collect_id: None,
            model: args.model.clone(),
            system_prompt: args.system_prompt.clone(),
            parallel_tasks: None,
            pr: args.pr,
        };
        // Reuse the existing --bg infrastructure — it prints per-task info
        // and registers + launches the background task, returning None.
        handle_spawn_bg(&single_args, agent_config, model, main_messages, tracker);
        // The most recently registered task has the highest ID. We can read
        // it back from the tracker.
        let last_id = {
            let tasks_vec = lock_or_recover(&tracker.inner);
            tasks_vec.last().map(|t| t.id).unwrap_or(0)
        };
        ids.push(last_id);
    }

    // Record the fan-out as a rerunnable JSON manifest so the run is
    // inspectable/reproducible after the session ends. Best-effort: a write
    // failure is a dim note, never a blocker (#341, orchestration scale).
    let results: Vec<(String, SpawnStatus)> = {
        let tasks_vec = lock_or_recover(&tracker.inner);
        ids.iter()
            .zip(tasks.iter())
            .map(|(id, task_text)| {
                let status = tasks_vec
                    .iter()
                    .find(|t| t.id == *id)
                    .map(|t| t.status.clone())
                    .unwrap_or(SpawnStatus::Running);
                (task_text.clone(), status)
            })
            .collect()
    };
    let run_id = spawn_run_id();
    let manifest = build_spawn_manifest(&run_id, tasks, &results);
    match write_spawn_manifest(Path::new(SPAWN_RUNS_DIR), &manifest) {
        Ok(path) => println!("{DIM}  manifest: {}{RESET}", path.display()),
        Err(e) => println!("{DIM}  (manifest not written: {e}){RESET}"),
    }

    println!(
        "\n{DIM}  Use /spawn status to check progress.\n  Use /spawn collect <id> to retrieve results.{RESET}\n"
    );

    None
}

/// Directory where `/spawn --parallel` fan-out manifests are written.
const SPAWN_RUNS_DIR: &str = ".yoyo/spawn_runs";

/// Subcommands/flags for `/spawn` tab-completion in the REPL.
pub const SPAWN_SUBCOMMANDS: &[&str] = &[
    "status",
    "worktrees",
    "collect",
    "manifest",
    "manifests",
    "replay",
    "runs",
    "--bg",
    "--parallel",
    "--model",
    "--system",
    "--pr",
    "-o",
];

/// Near-miss typo guard for `/spawn`'s free-text argument.
///
/// A single word that isn't a known subcommand but sits within edit distance 2
/// of one (e.g. "statsu" → "status") is almost certainly a typo — spawning a
/// whole sub-agent whose task is that word costs real money and time. Returns
/// the suggested subcommand when the guard should fire. Multi-word arguments
/// (real tasks) and words far from any subcommand are never touched.
pub fn spawn_near_miss(rest: &str) -> Option<&'static str> {
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        return None;
    }
    // Exact subcommands/flags are handled (or shown usage for) elsewhere.
    if SPAWN_SUBCOMMANDS.contains(&rest) {
        return None;
    }
    crate::commands::closest_match(rest, SPAWN_SUBCOMMANDS, 2)
}

/// Max bytes stored for a task string in a manifest entry (char-boundary safe).
const MANIFEST_TASK_CAP: usize = 200;

/// Generate a run id for a `/spawn --parallel` fan-out manifest.
/// Timestamp-based (UTC, compact), falling back to a fixed label if `date`
/// is unavailable so the write still succeeds.
fn spawn_run_id() -> String {
    std::process::Command::new("date")
        .args(["-u", "+%Y%m%dT%H%M%SZ"])
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
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "run".to_string())
}

/// Map a `SpawnStatus` to the stable manifest status string.
fn manifest_status_str(status: &SpawnStatus) -> &'static str {
    match status {
        SpawnStatus::Running => "running",
        SpawnStatus::Completed => "completed",
        SpawnStatus::Failed(_) => "failed",
    }
}

/// Build a rerunnable JSON manifest of a `/spawn --parallel` fan-out.
///
/// Pure (no I/O, no spawning) so it is unit-testable. Captures the run id,
/// a UTC timestamp, the task count, and per-task `{index, task, status}`
/// where `task` is char-boundary-truncated to a sane cap.
///
/// First step toward codified/replayable orchestration (#341).
pub fn build_spawn_manifest(
    run_id: &str,
    tasks: &[String],
    results: &[(String, SpawnStatus)],
) -> serde_json::Value {
    let created_ts = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
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
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let entries: Vec<serde_json::Value> = results
        .iter()
        .enumerate()
        .map(|(index, (task, status))| {
            serde_json::json!({
                "index": index,
                "task": safe_truncate(task, MANIFEST_TASK_CAP),
                "status": manifest_status_str(status),
            })
        })
        .collect();

    serde_json::json!({
        "run_id": run_id,
        "created_ts": created_ts,
        "task_count": tasks.len(),
        "tasks": entries,
    })
}

/// Write a spawn manifest to `<dir>/<run_id>.json`, creating `dir` if needed.
/// Follows the risk-snapshot write pattern (`create_dir_all` +
/// `to_string_pretty`). Returns the path so the caller can print it.
pub fn write_spawn_manifest(dir: &Path, manifest: &serde_json::Value) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let run_id = manifest
        .get("run_id")
        .and_then(|v| v.as_str())
        .unwrap_or("run");
    let path = dir.join(format!("{run_id}.json"));
    let pretty = serde_json::to_string_pretty(manifest).unwrap_or_else(|_| manifest.to_string());
    std::fs::write(&path, pretty)?;
    Ok(path)
}

/// A single task entry parsed back out of a fan-out manifest.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedManifestTask {
    pub index: usize,
    pub task: String,
    pub status: String,
}

/// A fan-out manifest read back from disk (inverse of `build_spawn_manifest`).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedManifest {
    pub run_id: String,
    pub created_ts: String,
    pub task_count: usize,
    pub tasks: Vec<ParsedManifestTask>,
}

/// Parse a fan-out manifest JSON string back into structured data.
///
/// Inverse of `build_spawn_manifest` → `write_spawn_manifest`. This is the
/// "gather half" of the scatter/gather fan-out: `--parallel` writes the
/// manifest, this reads it back into consumable Rust data.
///
/// Defensive: missing/malformed fields degrade gracefully (empty string / 0 /
/// skip the bad entry) rather than erroring, matching the read-only spirit of
/// the manifest inspector. Returns `None` only if the top-level JSON does not
/// parse or is not an object.
#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_spawn_manifest(json: &str) -> Option<ParsedManifest> {
    let value = serde_json::from_str::<serde_json::Value>(json).ok()?;
    let obj = value.as_object()?;

    let run_id = obj
        .get("run_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let created_ts = obj
        .get("created_ts")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let task_count = obj.get("task_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let tasks = obj
        .get("tasks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| {
                    let e = entry.as_object()?;
                    Some(ParsedManifestTask {
                        index: e.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                        task: e
                            .get("task")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        status: e
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(ParsedManifest {
        run_id,
        created_ts,
        task_count,
        tasks,
    })
}

/// Read and parse a manifest file from disk. Returns `None` if the file is
/// missing/unreadable or the JSON is malformed. Read-only, never panics.
pub fn read_spawn_manifest(path: &std::path::Path) -> Option<ParsedManifest> {
    let json = std::fs::read_to_string(path).ok()?;
    parse_spawn_manifest(&json)
}

/// List spawn manifest files in `dir` as `(run_id, path)` pairs.
///
/// `run_id` is the file stem of each `*.json`. Sorted newest-first: run_ids
/// are timestamp-prefixed (see `spawn_run_id`), so a reverse lexical sort
/// puts the most recent run on top. Returns an empty Vec if `dir` doesn't
/// exist — read-only and product-safe (never panics, never errors).
fn list_spawn_manifests(dir: &Path) -> Vec<(String, PathBuf)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|stem| (stem.to_string(), p.clone()))
        })
        .collect();
    // Newest-first: run_ids are timestamp-prefixed, so reverse lexical works.
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

/// Resolve which manifest a `/spawn replay` invocation refers to.
///
/// `None` or `Some("latest")` picks the newest manifest in `dir` (run ids are
/// timestamp-prefixed, so `list_spawn_manifests`' newest-first order is
/// chronological). A specific id resolves to `<dir>/<id>.json`. Each failure
/// mode gets one honest message — never a silent no-op.
fn select_replay_manifest(dir: &Path, id_arg: Option<&str>) -> Result<PathBuf, String> {
    match id_arg {
        None | Some("latest") => list_spawn_manifests(dir)
            .into_iter()
            .next()
            .map(|(_, path)| path)
            .ok_or_else(|| {
                format!(
                    "no spawn manifests found in {} — run /spawn --parallel first",
                    dir.display()
                )
            }),
        Some(id) => {
            let path = dir.join(format!("{id}.json"));
            if path.is_file() {
                Ok(path)
            } else {
                Err(format!(
                    "no manifest for run '{id}' (looked in {}) — /spawn runs lists available runs",
                    dir.display()
                ))
            }
        }
    }
}

/// Extract the replayable task list from a parsed manifest.
///
/// Empty-task entries are skipped (defensive parse artifacts); an entirely
/// empty list is an error worth naming, never a silent no-op.
fn manifest_replay_tasks(manifest: &ParsedManifest) -> Result<Vec<String>, String> {
    let tasks: Vec<String> = manifest
        .tasks
        .iter()
        .map(|t| t.task.clone())
        .filter(|t| !t.is_empty())
        .collect();
    if tasks.is_empty() {
        Err(format!(
            "manifest '{}' has no tasks to replay",
            manifest.run_id
        ))
    } else {
        Ok(tasks)
    }
}

/// Load the tasks for `/spawn replay [<run_id>|latest]`.
///
/// Combines manifest selection, read, and task extraction. Honest errors for
/// each failure mode: no manifests at all, unknown run id, corrupt JSON,
/// empty task list. Returns the parsed manifest (for the replay banner) plus
/// the task list to re-launch.
fn load_replay_tasks(
    dir: &Path,
    id_arg: Option<&str>,
) -> Result<(ParsedManifest, Vec<String>), String> {
    let path = select_replay_manifest(dir, id_arg)?;
    let manifest = read_spawn_manifest(&path)
        .ok_or_else(|| format!("manifest {} is unreadable or corrupt JSON", path.display()))?;
    let tasks = manifest_replay_tasks(&manifest)?;
    Ok((manifest, tasks))
}

/// Pure formatter: render a parsed manifest JSON as a human line-per-task
/// summary. Emits the run_id, task count, then one line per task with its
/// status and a char-boundary-safe truncated task string.
fn format_manifest_summary(manifest: &serde_json::Value) -> String {
    let run_id = manifest
        .get("run_id")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)");
    let count = manifest
        .get("task_count")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| {
            manifest
                .get("tasks")
                .and_then(|v| v.as_array())
                .map(|a| a.len() as u64)
                .unwrap_or(0)
        });

    let mut out = format!("{run_id} — {count} task(s)\n");
    if let Some(tasks) = manifest.get("tasks").and_then(|v| v.as_array()) {
        for entry in tasks {
            let status = entry
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let task = entry.get("task").and_then(|v| v.as_str()).unwrap_or("");
            let preview = truncate_with_ellipsis(task, 60);
            out.push_str(&format!("  [{status}] {preview}\n"));
        }
    }
    out
}

/// `/spawn manifest` — list all recorded fan-out run manifests.
fn handle_spawn_manifest_list() {
    let runs = list_spawn_manifests(Path::new(SPAWN_RUNS_DIR));
    if runs.is_empty() {
        println!(
            "{DIM}  no spawn manifests yet (run `/spawn --parallel ...` to create one){RESET}"
        );
        return;
    }
    println!("{BOLD}Spawn run manifests{RESET} ({} total)\n", runs.len());
    for (run_id, path) in &runs {
        // Read + parse for a one-line tally; fall back to just the id on error.
        match std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            Some(manifest) => {
                let count = manifest
                    .get("task_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let mut completed = 0u64;
                let mut failed = 0u64;
                let mut running = 0u64;
                if let Some(tasks) = manifest.get("tasks").and_then(|v| v.as_array()) {
                    for t in tasks {
                        match t.get("status").and_then(|v| v.as_str()) {
                            Some("completed") => completed += 1,
                            Some("failed") => failed += 1,
                            Some("running") => running += 1,
                            _ => {}
                        }
                    }
                }
                println!(
                    "  {CYAN}{run_id}{RESET}  {count} task(s)  {DIM}({completed} completed, {failed} failed, {running} running){RESET}"
                );
            }
            None => println!("  {CYAN}{run_id}{RESET}  {DIM}(unreadable manifest){RESET}"),
        }
    }
    println!("\n{DIM}  Use /spawn manifest <run_id> to inspect one run.{RESET}");
}

/// `/spawn manifest <run_id>` — show one run's tasks and statuses.
fn handle_spawn_manifest_show(id: &str) {
    let path = Path::new(SPAWN_RUNS_DIR).join(format!("{id}.json"));
    let manifest = match std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        Some(m) => m,
        None => {
            println!(
                "{DIM}  no readable manifest for run '{id}' (looked in {}){RESET}",
                path.display()
            );
            return;
        }
    };
    print!("{}", format_manifest_summary(&manifest));
}

/// Collect a finished background spawn's result.
/// Returns Some(context_msg) if the spawn is done, None otherwise.
fn handle_spawn_collect(tracker: &SpawnTracker, id: usize) -> Option<String> {
    match tracker.try_collect(id) {
        Ok(Some((task, result, handoff))) => {
            println!("{GREEN}  ✓ subagent #{id} completed{RESET}");
            if let Some(ref h) = handoff {
                println!("{GREEN}  {}{RESET}", h.summary_line());
                println!("{DIM}  review with: {}{RESET}", h.review_hint());
            }
            println!("{DIM}  injecting result into main conversation...{RESET}\n");
            Some(format_spawn_result(&task, &result, id, handoff.as_ref()))
        }
        Ok(None) => {
            println!("{CYAN}  ⏳ subagent #{id} is still running...{RESET}");
            println!("{DIM}  try again later or use /spawn status to check progress{RESET}\n");
            None
        }
        Err(e) => {
            println!("{RED}  ✗ {e}{RESET}\n");
            None
        }
    }
}

/// Apply a model override to an AgentConfig.
/// Updates the model name and, if the model implies a different provider,
/// switches the provider and API key accordingly.
fn apply_model_override(config: &mut crate::AgentConfig, model_name: &str) {
    config.model = model_name.to_string();

    // Try to detect the provider for the given model
    if let Some(provider) = crate::commands_info::find_provider_for_model(model_name) {
        if provider != config.provider {
            config.provider = provider.to_string();
            // Try to load the API key for the new provider
            if let Some(env_var) = crate::providers::provider_api_key_env(provider) {
                if let Ok(key) = std::env::var(env_var) {
                    config.api_key = key;
                }
            }
        }
    }
}

/// Clone an AgentConfig for building subagents.
/// Since AgentConfig doesn't derive Clone, we reconstruct it field by field.
fn clone_agent_config(config: &crate::AgentConfig) -> crate::AgentConfig {
    crate::AgentConfig {
        model: config.model.clone(),
        api_key: config.api_key.clone(),
        provider: config.provider.clone(),
        base_url: config.base_url.clone(),
        skills: config.skills.clone(),
        system_prompt: config.system_prompt.clone(),
        thinking: config.thinking,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        max_turns: config.max_turns,
        auto_approve: config.auto_approve,
        auto_commit: false,
        permissions: config.permissions.clone(),
        dir_restrictions: config.dir_restrictions.clone(),
        context_strategy: config.context_strategy,
        context_window: config.context_window,
        shell_hooks: config.shell_hooks.clone(),
        fallback_provider: config.fallback_provider.clone(),
        fallback_model: config.fallback_model.clone(),
        auto_watch: config.auto_watch,
        allowed_tools: vec![],
        disallowed_tools: vec![],
        no_tools: false,
        lite: false,
        bash_cwd: config.bash_cwd.clone(),
    }
}

// ---------------------------------------------------------------------------
// Git worktree lifecycle — primitives for parallel sub-agent isolation
// ---------------------------------------------------------------------------

/// Information about a spawned git worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// The branch/ref name used (detached HEAD label).
    pub branch: String,
    /// When the worktree was created (for stale-cleanup).
    pub created_at: Instant,
}

/// The bash cwd to pin a spawn worker to: the worktree path when isolation
/// succeeded, `None` (inherit process cwd) when it didn't.
///
/// This is enforced default confinement, NOT a sandbox — relative paths and
/// bare `git` operate in the worktree, but absolute paths can still escape.
pub fn spawn_bash_cwd(worktree: Option<&WorktreeInfo>) -> Option<String> {
    worktree.map(|w| w.path.display().to_string())
}

/// Run a git command in a specific directory.
/// Delegates to the centralized `git::run_git_in_dir` for consistent error
/// handling and test safety.
fn run_git_in(repo: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run_git_in_dir(repo, args)
}

/// Resolve the root of the git repository that contains `start`.
/// Falls back to `start` itself if rev-parse fails.
fn repo_root(start: &Path) -> PathBuf {
    run_git_in(start, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .unwrap_or_else(|_| start.to_path_buf())
}

/// Pre-flight symlink check for a worktree target path.
///
/// Walks each already-existing component of `wt_path` below `repo_root` and
/// refuses if any component is a symlink whose canonicalized target resolves
/// OUTSIDE the canonicalized repo root — otherwise `git worktree add` would
/// follow the link and write files outside the repository the user thinks
/// they're isolated in (bug class transferred from Claude Code v2.1.212's
/// fix log). Non-existent components are fine: they'll be created as real
/// directories. Symlinks that resolve back inside the repo are allowed.
///
/// Fails open (returns Ok) if the repo root itself can't be canonicalized —
/// the check can't judge escape without a trustworthy root, and product
/// behavior must not regress on exotic filesystems.
fn check_worktree_path_escape(repo_root: &Path, wt_path: &Path) -> Result<(), String> {
    let canon_root = match repo_root.canonicalize() {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };
    let rel = match wt_path.strip_prefix(repo_root) {
        Ok(r) => r,
        // Target isn't lexically under the root at all — refuse honestly.
        Err(_) => {
            return Err(format!(
                "worktree path {} is not under repo root {}",
                wt_path.display(),
                repo_root.display()
            ))
        }
    };
    let mut cur = repo_root.to_path_buf();
    for component in rel.components() {
        cur.push(component);
        let meta = match std::fs::symlink_metadata(&cur) {
            Ok(m) => m,
            // First non-existent component: the rest will be created as
            // real directories — nothing left to escape through.
            Err(_) => break,
        };
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&cur)
                .map(|t| t.display().to_string())
                .unwrap_or_else(|_| "<unreadable>".to_string());
            match cur.canonicalize() {
                Ok(resolved) if resolved.starts_with(&canon_root) => {
                    // Symlink stays inside the repo — not an escape.
                }
                Ok(resolved) => {
                    return Err(format!(
                        "refusing worktree: {} is a symlink to {} (resolves to {}, outside repo root {})",
                        cur.display(),
                        target,
                        resolved.display(),
                        canon_root.display()
                    ));
                }
                Err(_) => {
                    return Err(format!(
                        "refusing worktree: {} is a symlink to {} that cannot be resolved",
                        cur.display(),
                        target
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Create a worktree for a spawn task.
///
/// The worktree is placed under `<repo>/.yoyo/worktrees/spawn-{task_id}-{ts}/`
/// and is detached at the current HEAD of `repo_dir`.
///
/// `repo_dir` must be inside an existing git repository.
pub fn create_spawn_worktree(repo_dir: &Path, task_id: usize) -> Result<WorktreeInfo, String> {
    let root = repo_root(repo_dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dir_name = format!("spawn-{task_id}-{ts}");
    let wt_path = root.join(".yoyo").join("worktrees").join(&dir_name);

    // Pre-flight: refuse if any existing path component is a symlink that
    // resolves outside the repo root (worker writes would escape the repo).
    check_worktree_path_escape(&root, &wt_path)?;

    // Make sure parent exists.
    if let Err(e) = std::fs::create_dir_all(wt_path.parent().unwrap_or(&root)) {
        return Err(format!("failed to create worktree parent dir: {e}"));
    }

    let wt_str = wt_path.to_string_lossy().to_string();
    run_git_in(&root, &["worktree", "add", "--detach", &wt_str]).map_err(|e| {
        // Clean up the (possibly partially-created) directory.
        let _ = std::fs::remove_dir_all(&wt_path);
        format!("git worktree add failed: {e}")
    })?;

    // Read the HEAD of the new worktree so we can record the branch/ref.
    let head = run_git_in(&wt_path, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();

    Ok(WorktreeInfo {
        path: wt_path,
        branch: head,
        created_at: Instant::now(),
    })
}

/// Maximum characters of the task description used in the handoff commit message.
const HANDOFF_COMMIT_DESC_CHARS: usize = 60;

/// Derive the handoff branch name for a worktree.
/// The worktree dir is named `spawn-<id>-<ts>`; the branch becomes
/// `spawn/<id>-<ts>` so the commit survives worktree cleanup under a
/// discoverable ref.
fn spawn_branch_name(info: &WorktreeInfo) -> String {
    let dir_name = info
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "spawn-unknown".to_string());
    format!(
        "spawn/{}",
        dir_name.strip_prefix("spawn-").unwrap_or(&dir_name)
    )
}

/// Condense a `git diff --shortstat` line into a compact summary like
/// `3 files changed (+42/-7)`. Falls back to the trimmed raw line if the
/// format is unrecognized.
fn format_diffstat_summary(shortstat: &str) -> String {
    let raw = shortstat.trim();
    if raw.is_empty() {
        return "no diffstat available".to_string();
    }
    let mut files: Option<&str> = None;
    let mut insertions = 0u64;
    let mut deletions = 0u64;
    for part in raw.split(',') {
        let part = part.trim();
        if part.contains("file") {
            files = part.split_whitespace().next();
        } else if part.contains("insertion") {
            insertions = part
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
        } else if part.contains("deletion") {
            deletions = part
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
        }
    }
    match files {
        Some(n) => {
            let plural = if n == "1" { "file" } else { "files" };
            format!("{n} {plural} changed (+{insertions}/-{deletions})")
        }
        None => raw.to_string(),
    }
}

/// Commit any uncommitted changes in a spawn worktree and summarize them.
///
/// If the worktree is clean, returns `Ok(None)` (no handoff — nothing to
/// review). Otherwise stages everything, commits with a `spawn: <task>`
/// message (task description truncated at a char boundary), points
/// `branch_name` at the new commit so it survives worktree removal, and
/// returns a `SpawnHandoff` with the branch and a one-line diffstat.
///
/// The commit is performed *before* any success is reported — if the commit
/// fails, the error propagates and callers must report it honestly.
pub fn commit_worktree_handoff(
    worktree: &Path,
    branch_name: &str,
    task: &str,
) -> Result<Option<SpawnHandoff>, String> {
    let status = run_git_in(worktree, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Ok(None);
    }

    run_git_in(worktree, &["add", "-A"])?;

    let desc: String = task.chars().take(HANDOFF_COMMIT_DESC_CHARS).collect();
    let msg = format!("spawn: {}", desc.trim());
    run_git_in(worktree, &["commit", "-m", &msg])?;

    // Name the commit with a branch so it survives worktree cleanup.
    run_git_in(worktree, &["branch", "--force", branch_name, "HEAD"])?;

    // One-line summary of what the commit changed.
    let shortstat = run_git_in(worktree, &["diff", "--shortstat", "HEAD~1..HEAD"])
        .or_else(|_| run_git_in(worktree, &["show", "--shortstat", "--format=", "HEAD"]))
        .unwrap_or_default();

    Ok(Some(SpawnHandoff {
        branch: branch_name.to_string(),
        diffstat: format_diffstat_summary(&shortstat),
    }))
}

/// Attempt the completion handoff for a finished worker: commit worktree
/// changes (if any) and return the handoff record. Never pre-announces
/// success — warnings are printed only when the commit itself fails, and the
/// worker's result is still delivered without a handoff in that case.
fn try_worktree_handoff(
    worktree: &WorktreeInfo,
    task: &str,
    spawn_id: usize,
    open_pr: bool,
) -> Option<SpawnHandoff> {
    let branch = spawn_branch_name(worktree);
    match commit_worktree_handoff(&worktree.path, &branch, task) {
        Ok(Some(handoff)) => {
            println!("{GREEN}  {}{RESET}", handoff.summary_line());
            println!("{DIM}  review with: {}{RESET}", handoff.review_hint());
            if open_pr {
                push_and_open_pr(&worktree.path, task, &handoff);
            }
            Some(handoff)
        }
        Ok(None) => {
            println!("{DIM}  spawn #{spawn_id}: no file changes to hand off{RESET}");
            if open_pr {
                println!("{DIM}  skipped PR: no changes to hand off{RESET}");
            }
            None
        }
        Err(e) => {
            eprintln!("{YELLOW}  ⚠ spawn #{spawn_id}: handoff commit failed: {e}{RESET}");
            None
        }
    }
}

/// Build the `gh` argument vector for opening a draft PR from a spawn handoff.
///
/// Pure — touches neither git nor the network, so it is unit-testable. The
/// title reuses the handoff commit description (task truncated to
/// `HANDOFF_COMMIT_DESC_CHARS` *characters* via `chars().take`, never byte
/// indexing, so multi-byte task text cannot panic). The body carries the
/// handoff summary plus the review hint.
fn build_spawn_pr_args(task: &str, handoff: &SpawnHandoff) -> Vec<String> {
    let desc: String = task.chars().take(HANDOFF_COMMIT_DESC_CHARS).collect();
    let title = format!("spawn: {}", desc.trim());
    let body = format!(
        "{}\n\nReview with: `{}`",
        handoff.summary_line(),
        handoff.review_hint()
    );
    vec![
        "pr".to_string(),
        "create".to_string(),
        "--draft".to_string(),
        "--head".to_string(),
        handoff.branch.clone(),
        "--title".to_string(),
        title,
        "--body".to_string(),
        body,
    ]
}

/// Push the handoff branch and open a draft PR via `gh` (opt-in `--pr` flag).
///
/// Degrades gracefully and reports honestly (never pre-announces success):
/// - `gh` not on PATH → keep the local-branch line, note "skipped PR".
/// - push fails (no remote, auth, network) → "push failed: <first stderr line>";
///   the local branch is still the result.
/// - PR creation fails after a successful push → report the pushed branch and
///   the failure line.
///
/// Uses `std::process::Command` directly (NOT `run_git`, whose `#[cfg(test)]`
/// destructive-command guard panics on `push` during `cargo test`).
fn push_and_open_pr(repo_dir: &Path, task: &str, handoff: &SpawnHandoff) {
    let gh_ok = std::process::Command::new("gh")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !gh_ok {
        println!("{DIM}  skipped PR: gh not found{RESET}");
        return;
    }

    let push = std::process::Command::new("git")
        .current_dir(repo_dir)
        .args(["push", "-u", "origin", &handoff.branch])
        .output();
    match push {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let first = stderr.lines().next().unwrap_or("unknown error");
            println!("{YELLOW}  push failed: {first}{RESET}");
            return;
        }
        Err(e) => {
            println!("{YELLOW}  push failed: {e}{RESET}");
            return;
        }
    }

    let pr_args = build_spawn_pr_args(task, handoff);
    let pr = std::process::Command::new("gh")
        .current_dir(repo_dir)
        .args(&pr_args)
        .output();
    match pr {
        Ok(out) if out.status.success() => {
            let url = String::from_utf8_lossy(&out.stdout);
            let url = url.trim();
            if url.is_empty() {
                println!("{GREEN}  draft PR opened for {}{RESET}", handoff.branch);
            } else {
                println!("{GREEN}  draft PR: {url}{RESET}");
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let first = stderr.lines().next().unwrap_or("unknown error");
            println!(
                "{YELLOW}  pushed {} but PR creation failed: {first}{RESET}",
                handoff.branch
            );
        }
        Err(e) => {
            println!(
                "{YELLOW}  pushed {} but PR creation failed: {e}{RESET}",
                handoff.branch
            );
        }
    }
}

/// Remove a spawn worktree, cleaning up both the directory and the git metadata.
///
/// This is idempotent — calling it on an already-removed worktree is a no-op.
pub fn cleanup_spawn_worktree(repo_dir: &Path, info: &WorktreeInfo) -> Result<(), String> {
    let root = repo_root(repo_dir);
    let wt_str = info.path.to_string_lossy().to_string();

    // If the worktree directory doesn't exist any more, just prune stale metadata.
    if !info.path.exists() {
        let _ = run_git_in(&root, &["worktree", "prune"]);
        return Ok(());
    }

    // Try force-remove first, then plain remove, then manual cleanup.
    if run_git_in(&root, &["worktree", "remove", "--force", &wt_str]).is_ok() {
        return Ok(());
    }
    if run_git_in(&root, &["worktree", "remove", &wt_str]).is_ok() {
        return Ok(());
    }

    // Manual fallback: remove directory and prune.
    let _ = std::fs::remove_dir_all(&info.path);
    let _ = run_git_in(&root, &["worktree", "prune"]);
    if info.path.exists() {
        Err(format!(
            "failed to remove worktree directory: {}",
            info.path.display()
        ))
    } else {
        Ok(())
    }
}

/// List spawn worktrees under `<repo>/.yoyo/worktrees/spawn-*`.
pub fn list_spawn_worktrees(repo_dir: &Path) -> Vec<PathBuf> {
    let root = repo_root(repo_dir);
    let wt_dir = root.join(".yoyo").join("worktrees");
    if !wt_dir.is_dir() {
        return Vec::new();
    }
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&wt_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("spawn-") && entry.path().is_dir() {
                result.push(entry.path());
            }
        }
    }
    result
}

/// Clean up worktrees older than `max_age`.
///
/// Scans `<repo>/.yoyo/worktrees/spawn-*` and removes any whose directory
/// modified-time is older than `max_age` ago.
pub fn cleanup_stale_worktrees(repo_dir: &Path, max_age: std::time::Duration) {
    let root = repo_root(repo_dir);
    let wt_dir = root.join(".yoyo").join("worktrees");
    if !wt_dir.is_dir() {
        return;
    }
    let now = std::time::SystemTime::now();
    if let Ok(entries) = std::fs::read_dir(&wt_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("spawn-") {
                continue;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Check modification time.
            let stale = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|mt| now.duration_since(mt).ok())
                .is_some_and(|age| age > max_age);
            if stale {
                let wt_str = path.to_string_lossy().to_string();
                let _ = run_git_in(&root, &["worktree", "remove", "--force", &wt_str]);
                if path.exists() {
                    let _ = std::fs::remove_dir_all(&path);
                }
            }
        }
        // Prune any dangling worktree metadata.
        let _ = run_git_in(&root, &["worktree", "prune"]);
    }
}

// ─── Parallelizable-prompt detection ────────────────────────────────────────

/// Phrases that indicate sequential dependency between steps. If any appears
/// (as a whole word/phrase) anywhere in the prompt, the listed tasks are
/// treated as dependent and NOT parallelizable. Kept small and easy to extend.
const SEQUENTIAL_DEPENDENCY_MARKERS: &[&str] = &[
    "then",
    "after that",
    "once that's done",
    "once that is done",
    "using the result",
    "based on the above",
    "afterwards",
];

/// Minimum number of list items required before the prompt looks like a
/// decomposable batch of tasks.
const MIN_PARALLEL_ITEMS: usize = 3;

/// Minimum character length of a list item (after the marker) to count as a
/// real task rather than a fragment like "- yes".
const MIN_ITEM_CHARS: usize = 15;

/// File extensions used by the path-conflict heuristic.
const PATH_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "c", "cpp", "h", "hpp", "rb", "php", "md",
    "toml", "yaml", "yml", "json", "sh", "css", "html", "txt",
];

/// Word-boundary-aware phrase search on an already-lowercased haystack.
/// `marker` must be ASCII. Boundaries are non-alphanumeric ASCII bytes (or
/// string edges), so "then" does not match inside "authentication".
fn contains_marker_phrase(haystack_lower: &str, marker: &str) -> bool {
    let bytes = haystack_lower.as_bytes();
    let mut start = 0;
    while let Some(pos) = haystack_lower[start..].find(marker) {
        let abs = start + pos;
        let end = abs + marker.len();
        let before_ok = abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric();
        let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = end;
    }
    false
}

/// Parse a line as a list item: `- item`, `* item`, `1. item`, or `1) item`.
/// Returns the item text (trimmed) if the line matches.
fn parse_list_item(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        return Some(rest.trim());
    }
    // Numbered: one or more digits, then '.' or ')', then a space.
    let digits_end = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
    if digits_end == 0 {
        return None;
    }
    let rest = &t[digits_end..];
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    let rest = rest.strip_prefix(' ')?;
    Some(rest.trim())
}

/// Heuristic: does this token look like a file path?
/// Either it contains a `/` (but isn't a URL), or it ends with a known
/// source-file extension.
fn looks_like_path(token: &str) -> bool {
    if token.starts_with("http://") || token.starts_with("https://") {
        return false;
    }
    if token.contains('/') && token.len() > 2 {
        return true;
    }
    if let Some((stem, ext)) = token.rsplit_once('.') {
        if !stem.is_empty() && PATH_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            return true;
        }
    }
    false
}

/// Extract path-looking tokens from a list item (deduplicated, lowercased).
fn extract_path_tokens(item: &str) -> std::collections::HashSet<String> {
    item.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | '(' | ')' | '`' | '"' | '\'' | '<' | '>' | '@'
            )
    })
    .map(|t| t.trim_matches(|c: char| matches!(c, '.' | ':' | '!' | '?')))
    .filter(|t| looks_like_path(t))
    .map(|t| t.to_lowercase())
    .collect()
}

/// Detect whether a prompt looks like a list of INDEPENDENT tasks that could
/// run in parallel worktrees via `/spawn`.
///
/// Returns `Some(items)` only when ALL of these hold:
/// - the prompt contains a numbered or bulleted list with ≥3 items
/// - every item is non-trivial (>15 chars after the marker)
/// - no sequential-dependency marker appears anywhere in the prompt
///   (including "first ... second" ordinal narrative)
/// - no file path is referenced by 2+ items (they'd conflict in worktrees)
///
/// Deliberately conservative: a false negative costs nothing, a false
/// positive is an annoying wrong hint.
pub fn detect_parallelizable_tasks(prompt: &str) -> Option<Vec<String>> {
    let lower = prompt.to_lowercase();
    for marker in SEQUENTIAL_DEPENDENCY_MARKERS {
        if contains_marker_phrase(&lower, marker) {
            return None;
        }
    }
    // "first ... second" as ordinal narrative implies ordering.
    if contains_marker_phrase(&lower, "first") && contains_marker_phrase(&lower, "second") {
        return None;
    }

    let items: Vec<&str> = prompt.lines().filter_map(parse_list_item).collect();
    if items.len() < MIN_PARALLEL_ITEMS {
        return None;
    }
    // Every item must be a real task, not a fragment.
    if items.iter().any(|i| i.chars().count() <= MIN_ITEM_CHARS) {
        return None;
    }

    // Same file path in 2+ items → worktree conflict, not parallelizable.
    let mut path_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for item in &items {
        for path in extract_path_tokens(item) {
            *path_counts.entry(path).or_insert(0) += 1;
        }
    }
    if path_counts.values().any(|&c| c >= 2) {
        return None;
    }

    Some(items.into_iter().map(String::from).collect())
}

/// Pure decision helper: when a single-task `/spawn` prompt actually decomposes
/// into ≥2 independent tasks, return a copy-pasteable `--parallel` suggestion hint.
/// Returns `None` when the prompt is a single task, has sequential markers, or a
/// path conflict — i.e. whenever fan-out would be unsafe or unhelpful. Hint only:
/// this never changes what `/spawn` does, it just points at a faster path.
pub fn parallel_suggestion(prompt: &str) -> Option<String> {
    let tasks = detect_parallelizable_tasks(prompt)?;
    if tasks.len() < 2 {
        return None;
    }
    let joined = tasks.join(" --- ");
    Some(format!(
        "💡 this looks decomposable into {} independent tasks. Run them in parallel with:\n   /spawn --parallel {}",
        tasks.len(),
        joined
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
    use yoagent::types::{Content, Message, Usage};

    #[test]
    fn test_parallel_suggestion_fires_on_independent_tasks() {
        // 3 genuinely independent tasks, no sequential markers, no path conflict.
        let prompt = "- add tests for the parser\n- write docs for the api\n- refactor the logger";
        let hint = parallel_suggestion(prompt).expect("should suggest fan-out");
        // Copy-pasteable command must be present.
        assert!(hint.contains("--parallel"), "hint: {hint}");
        assert!(hint.contains("3 independent tasks"), "hint: {hint}");
    }

    #[test]
    fn test_parallel_suggestion_silent_on_single_line() {
        // A single-line prompt is one task — no fan-out.
        assert!(parallel_suggestion("read src/main.rs and summarize").is_none());
    }

    #[test]
    fn test_parallel_suggestion_silent_on_sequential_marker() {
        // Sequential dependency ("first ... then ...") must NOT fan out even if it
        // looks list-shaped — the tasks depend on ordering.
        let prompt = "- first build the index\n- then query it\n- finally print results";
        assert!(parallel_suggestion(prompt).is_none());
    }

    #[test]
    fn test_parallel_suggestion_silent_on_path_conflict() {
        // Paired negative (Day 122): decomposes structurally but two items touch the
        // SAME file — parallel worktrees would collide. The hint must stay SILENT on
        // this innocent-looking-but-unsafe neighbor.
        let prompt =
            "- add a field to src/config.rs\n- rename a fn in src/config.rs\n- update the readme";
        assert!(
            parallel_suggestion(prompt).is_none(),
            "path conflict must suppress the fan-out hint"
        );
    }

    #[test]
    fn test_newly_finished_background_reports_completed_once() {
        let tracker = SpawnTracker::new();
        let id = tracker.register_with_bg("bg task", None, true);
        tracker.complete(id, "done".to_string());

        let first = tracker.newly_finished_background();
        assert_eq!(first.len(), 1);
        assert!(first[0].contains(&format!("#{id}")));
        assert!(first[0].contains("collect"));

        // Notified-once invariant: a second call reports nothing.
        let second = tracker.newly_finished_background();
        assert!(second.is_empty());
    }

    #[test]
    fn test_newly_finished_background_skips_running() {
        let tracker = SpawnTracker::new();
        tracker.register_with_bg("bg task", None, true);
        // Still Running — no hint.
        assert!(tracker.newly_finished_background().is_empty());
    }

    #[test]
    fn test_newly_finished_background_reports_failed() {
        let tracker = SpawnTracker::new();
        let id = tracker.register_with_bg("bg task", None, true);
        tracker.fail(id, "boom".to_string());

        let first = tracker.newly_finished_background();
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("failed"));
        assert!(first[0].contains(&format!("#{id}")));

        // Reported only once.
        assert!(tracker.newly_finished_background().is_empty());
    }

    #[test]
    fn test_newly_finished_background_ignores_foreground() {
        let tracker = SpawnTracker::new();
        // background: false — user already blocked on it, nothing to notify.
        let id = tracker.register_with_bg("fg task", None, false);
        tracker.complete(id, "done".to_string());
        assert!(tracker.newly_finished_background().is_empty());
    }

    // ── spawn args parsing tests ────────────────────────────────────────

    #[test]
    fn test_parse_spawn_args_basic_task() {
        let args = parse_spawn_args("/spawn read src/main.rs and summarize");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.task, "read src/main.rs and summarize");
        assert_eq!(args.output_path, None);
    }

    #[test]
    fn test_parse_spawn_args_with_output_flag() {
        let args = parse_spawn_args("/spawn -o results.md summarize this codebase");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.task, "summarize this codebase");
        assert_eq!(args.output_path, Some("results.md".to_string()));
    }

    #[test]
    fn test_parse_spawn_args_empty() {
        assert!(parse_spawn_args("/spawn").is_none());
        assert!(parse_spawn_args("/spawn  ").is_none());
    }

    #[test]
    fn test_parse_spawn_args_status_returns_none() {
        assert!(parse_spawn_args("/spawn status").is_none());
    }

    #[test]
    fn test_parse_spawn_args_output_with_complex_path() {
        let args = parse_spawn_args("/spawn -o /tmp/output.md analyze the architecture");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.task, "analyze the architecture");
        assert_eq!(args.output_path, Some("/tmp/output.md".to_string()));
    }

    #[test]
    fn test_parse_spawn_args_pr_flag_trailing() {
        let args = parse_spawn_args("/spawn do thing --pr").unwrap();
        assert!(args.pr, "--pr should set pr: true");
        assert_eq!(args.task, "do thing", "task text must not contain the flag");
    }

    #[test]
    fn test_parse_spawn_args_pr_flag_leading() {
        let args = parse_spawn_args("/spawn --pr do thing").unwrap();
        assert!(args.pr);
        assert_eq!(args.task, "do thing");
    }

    #[test]
    fn test_parse_spawn_args_without_pr_flag_defaults_false() {
        // Paired negative: no --pr means pr stays false (opt-in only).
        let args = parse_spawn_args("/spawn do thing").unwrap();
        assert!(!args.pr, "pr must default to false without --pr");
        assert_eq!(args.task, "do thing");
    }

    #[test]
    fn test_parse_spawn_args_pr_combined_with_other_flags() {
        let args =
            parse_spawn_args("/spawn --bg -o out.md --model claude-haiku-4-5 --pr fix the bug")
                .unwrap();
        assert!(args.pr);
        assert!(args.background);
        assert_eq!(args.output_path, Some("out.md".to_string()));
        assert_eq!(args.model, Some("claude-haiku-4-5".to_string()));
        assert_eq!(args.task, "fix the bug");
    }

    #[test]
    fn test_parse_spawn_args_pr_between_flags_before_bg() {
        // --pr appearing before other flags must not break flag extraction.
        let args = parse_spawn_args("/spawn --pr --bg do the thing").unwrap();
        assert!(args.pr);
        assert!(args.background);
        assert_eq!(args.task, "do the thing");
    }

    #[test]
    fn test_build_spawn_pr_args_draft_head_and_title() {
        let handoff = SpawnHandoff {
            branch: "spawn/3-12345".to_string(),
            diffstat: "3 files changed (+42/-7)".to_string(),
        };
        let args = build_spawn_pr_args("fix the widget rendering", &handoff);
        assert_eq!(args[0], "pr");
        assert_eq!(args[1], "create");
        assert!(args.contains(&"--draft".to_string()), "PR must be a draft");
        let head_pos = args.iter().position(|a| a == "--head").expect("--head");
        assert_eq!(args[head_pos + 1], "spawn/3-12345");
        let title_pos = args.iter().position(|a| a == "--title").expect("--title");
        assert_eq!(args[title_pos + 1], "spawn: fix the widget rendering");
        let body_pos = args.iter().position(|a| a == "--body").expect("--body");
        assert!(args[body_pos + 1].contains("ready to review"));
        assert!(args[body_pos + 1].contains("git diff main...spawn/3-12345"));
    }

    #[test]
    fn test_build_spawn_pr_args_multibyte_title_truncation() {
        // A long multi-byte task must truncate at a char boundary, never panic.
        let task = "✓".repeat(200);
        let handoff = SpawnHandoff {
            branch: "spawn/9-777".to_string(),
            diffstat: "1 file changed (+1/-0)".to_string(),
        };
        let args = build_spawn_pr_args(&task, &handoff);
        let title_pos = args.iter().position(|a| a == "--title").expect("--title");
        let title = &args[title_pos + 1];
        assert!(title.starts_with("spawn: "));
        let desc = title.strip_prefix("spawn: ").unwrap();
        assert_eq!(desc.chars().count(), HANDOFF_COMMIT_DESC_CHARS);
        assert!(desc.chars().all(|c| c == '✓'));
    }

    // ── spawn tracker tests ─────────────────────────────────────────────

    #[test]
    fn test_spawn_tracker_new_is_empty() {
        let tracker = SpawnTracker::new();
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
    }

    #[test]
    fn test_spawn_tracker_register_returns_sequential_ids() {
        let tracker = SpawnTracker::new();
        let id1 = tracker.register("task one", None);
        let id2 = tracker.register("task two", Some("out.md".to_string()));
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(tracker.len(), 2);
    }

    #[test]
    fn test_spawn_tracker_complete_updates_status() {
        let tracker = SpawnTracker::new();
        let id = tracker.register("test task", None);
        assert_eq!(tracker.get(id).unwrap().status, SpawnStatus::Running);

        tracker.complete(id, "done!".to_string());
        let task = tracker.get(id).unwrap();
        assert_eq!(task.status, SpawnStatus::Completed);
        assert_eq!(task.result, Some("done!".to_string()));
    }

    #[test]
    fn test_spawn_tracker_fail_updates_status() {
        let tracker = SpawnTracker::new();
        let id = tracker.register("failing task", None);
        tracker.fail(id, "something broke".to_string());
        let task = tracker.get(id).unwrap();
        assert_eq!(
            task.status,
            SpawnStatus::Failed("something broke".to_string())
        );
        assert_eq!(task.result, None);
    }

    #[test]
    fn test_spawn_tracker_count_by_status() {
        let tracker = SpawnTracker::new();
        let _id1 = tracker.register("running", None);
        let id2 = tracker.register("done", None);
        let id3 = tracker.register("broken", None);
        tracker.complete(id2, "result".to_string());
        tracker.fail(id3, "error".to_string());

        let (running, completed, failed) = tracker.count_by_status();
        assert_eq!(running, 1);
        assert_eq!(completed, 1);
        assert_eq!(failed, 1);
    }

    #[test]
    fn test_spawn_tracker_get_nonexistent() {
        let tracker = SpawnTracker::new();
        assert!(tracker.get(999).is_none());
    }

    #[test]
    fn test_spawn_tracker_snapshot() {
        let tracker = SpawnTracker::new();
        tracker.register("task a", None);
        tracker.register("task b", Some("out.txt".to_string()));
        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].task, "task a");
        assert_eq!(snapshot[1].task, "task b");
        assert_eq!(snapshot[1].output_path, Some("out.txt".to_string()));
    }

    // ── spawn context prompt tests ──────────────────────────────────────

    #[test]
    fn test_spawn_context_prompt_without_context() {
        let prompt = spawn_context_prompt(&[], None, None);
        assert!(prompt.contains("subagent"));
        assert!(!prompt.contains("Project Context"));
        assert!(!prompt.contains("Conversation Context"));
    }

    #[test]
    fn test_spawn_context_prompt_with_project_context() {
        let prompt = spawn_context_prompt(&[], Some("# My Project\nA great tool."), None);
        assert!(prompt.contains("subagent"));
        assert!(prompt.contains("## Project Context"));
        assert!(prompt.contains("My Project"));
    }

    #[test]
    fn test_spawn_context_prompt_with_messages() {
        let messages = vec![AgentMessage::Llm(Message::user("hello world"))];
        let prompt = spawn_context_prompt(&messages, None, None);
        assert!(prompt.contains("subagent"));
        assert!(prompt.contains("Conversation Context"));
        assert!(prompt.contains("hello world"));
    }

    #[test]
    fn test_spawn_context_prompt_truncates_large_context() {
        let large_context = "x".repeat(10000);
        let prompt = spawn_context_prompt(&[], Some(&large_context), None);
        assert!(prompt.contains("(truncated)"));
        // Should contain less than the full 10000 chars
        assert!(prompt.len() < 10000);
    }

    #[test]
    fn test_spawn_context_prompt_with_worktree_path() {
        let wt = Path::new("/tmp/yoyo-worktree/spawn-1-12345");
        let prompt = spawn_context_prompt(&[], None, Some(wt));
        assert!(prompt.contains("Working Directory"));
        assert!(prompt.contains("/tmp/yoyo-worktree/spawn-1-12345"));
        assert!(prompt.contains("isolated git worktree"));
    }

    #[test]
    fn test_spawn_context_prompt_without_worktree() {
        let prompt = spawn_context_prompt(&[], None, None);
        assert!(!prompt.contains("Working Directory"));
        assert!(!prompt.contains("worktree"));
    }

    // ── summarize_conversation_for_spawn tests ──────────────────────────

    #[test]
    fn test_summarize_conversation_empty() {
        let summary = summarize_conversation_for_spawn(&[]);
        assert!(summary.is_empty());
    }

    #[test]
    fn test_summarize_conversation_includes_roles() {
        let messages = vec![
            AgentMessage::Llm(Message::user("What is Rust?")),
            AgentMessage::Llm(
                Message::assistant(
                    vec![Content::Text {
                        text: "Rust is a systems programming language.".to_string(),
                    }],
                    yoagent::types::StopReason::Stop,
                    "test".to_string(),
                    "test".to_string(),
                    Usage::default(),
                )
                .with_timestamp(0),
            ),
        ];
        let summary = summarize_conversation_for_spawn(&messages);
        assert!(summary.contains("[user]"));
        assert!(summary.contains("[assistant]"));
    }

    #[test]
    fn test_summarize_conversation_limits_messages() {
        // Create 15 messages — should only summarize last 10
        let mut messages = Vec::new();
        for i in 0..15 {
            messages.push(AgentMessage::Llm(Message::user(format!("msg {i}"))));
        }
        let summary = summarize_conversation_for_spawn(&messages);
        let line_count = summary.lines().count();
        assert_eq!(line_count, 10, "Should limit to 10 messages");
        // Should contain last 10 (5..15)
        assert!(summary.contains("msg 5"));
        assert!(summary.contains("msg 14"));
        // Should NOT contain first 5 (0..5)
        assert!(!summary.contains("msg 4"));
    }

    // ── format_spawn_result tests ───────────────────────────────────────

    #[test]
    fn test_format_spawn_result_includes_id() {
        let result = format_spawn_result("read file", "contents here", 3, None);
        assert!(result.contains("#3"));
        assert!(result.contains("read file"));
        assert!(result.contains("contents here"));
        assert!(
            !result.contains("Handoff"),
            "no handoff line without a handoff"
        );
    }

    #[test]
    fn test_format_spawn_result_empty_output() {
        let result = format_spawn_result("task", "   ", 1, None);
        assert!(result.contains("(no output)"));
    }

    #[test]
    fn test_format_spawn_result_with_handoff() {
        let handoff = SpawnHandoff {
            branch: "spawn/3-12345".to_string(),
            diffstat: "3 files changed (+42/-7)".to_string(),
        };
        let result = format_spawn_result("fix bug", "done", 3, Some(&handoff));
        assert!(result.contains("ready to review: branch spawn/3-12345"));
        assert!(result.contains("3 files changed (+42/-7)"));
        assert!(result.contains("git diff main...spawn/3-12345"));
    }

    #[test]
    fn test_spawn_handoff_summary_and_hint() {
        let handoff = SpawnHandoff {
            branch: "spawn/1-999".to_string(),
            diffstat: "1 file changed (+2/-0)".to_string(),
        };
        assert_eq!(
            handoff.summary_line(),
            "ready to review: branch spawn/1-999 — 1 file changed (+2/-0)"
        );
        assert_eq!(handoff.review_hint(), "git diff main...spawn/1-999");
    }

    #[test]
    fn test_format_diffstat_summary() {
        assert_eq!(
            format_diffstat_summary(" 3 files changed, 42 insertions(+), 7 deletions(-)"),
            "3 files changed (+42/-7)"
        );
        assert_eq!(
            format_diffstat_summary(" 1 file changed, 2 insertions(+)"),
            "1 file changed (+2/-0)"
        );
        assert_eq!(
            format_diffstat_summary(" 1 file changed, 5 deletions(-)"),
            "1 file changed (+0/-5)"
        );
        assert_eq!(format_diffstat_summary(""), "no diffstat available");
        // Unrecognized format falls back to trimmed raw
        assert_eq!(format_diffstat_summary(" something odd "), "something odd");
    }

    #[test]
    fn test_handoff_commit_message_multibyte_truncation() {
        // A task made of multi-byte chars must not panic when truncated.
        let task = "✓".repeat(200);
        let desc: String = task.chars().take(HANDOFF_COMMIT_DESC_CHARS).collect();
        assert_eq!(desc.chars().count(), HANDOFF_COMMIT_DESC_CHARS);
        // And the full helper path must not panic either (clean repo → no commit).
        let tmp = setup_temp_repo();
        let result = commit_worktree_handoff(tmp.path(), "spawn/test-mb", &task);
        assert_eq!(result, Ok(None), "clean tree yields no handoff");
    }

    #[test]
    fn test_commit_worktree_handoff_dirty_tree_commits_and_summarizes() {
        // Tempdir repo (never the project root — run_git guard) with an
        // uncommitted file: the helper must commit it, point the branch at the
        // new commit, and return a non-empty diffstat summary.
        let tmp = setup_temp_repo();
        let p = tmp.path();
        std::fs::write(p.join("new_work.txt"), "line one\nline two\n").expect("write dirty file");

        let head_before = run_git_in(p, &["rev-parse", "HEAD"]).expect("head before");

        let handoff = commit_worktree_handoff(p, "spawn/7-test", "fix the ✓ widget rendering")
            .expect("handoff should succeed")
            .expect("dirty tree should produce a handoff");

        // A new commit exists and the tree is clean afterwards.
        let head_after = run_git_in(p, &["rev-parse", "HEAD"]).expect("head after");
        assert_ne!(head_before, head_after, "a new commit should exist");
        let status = run_git_in(p, &["status", "--porcelain"]).expect("status");
        assert!(
            status.trim().is_empty(),
            "tree should be clean after commit"
        );

        // Commit message carries the spawn: prefix + task description.
        let msg = run_git_in(p, &["log", "-1", "--format=%s"]).expect("log");
        assert!(
            msg.starts_with("spawn: fix the ✓ widget"),
            "commit message should be 'spawn: <task>', got: {msg}"
        );

        // The branch was created and points at the new commit.
        let branch_sha = run_git_in(p, &["rev-parse", "spawn/7-test"]).expect("branch sha");
        assert_eq!(branch_sha, head_after, "branch should point at the commit");

        // Diffstat summary is non-empty and mentions the change.
        assert_eq!(handoff.branch, "spawn/7-test");
        assert!(!handoff.diffstat.is_empty(), "diffstat should be non-empty");
        assert!(
            handoff.diffstat.contains("1 file changed"),
            "diffstat should summarize the change, got: {}",
            handoff.diffstat
        );
        assert!(
            handoff.diffstat.contains("+2"),
            "diffstat should count 2 insertions, got: {}",
            handoff.diffstat
        );
    }

    // ── SpawnStatus display tests ───────────────────────────────────────

    #[test]
    fn test_spawn_status_display() {
        assert_eq!(format!("{}", SpawnStatus::Running), "running");
        assert_eq!(format!("{}", SpawnStatus::Completed), "completed");
        assert_eq!(
            format!("{}", SpawnStatus::Failed("oops".to_string())),
            "failed: oops"
        );
    }

    // ── spawn command recognition tests ─────────────────────────────────

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
    fn test_parse_spawn_args_bg_flag() {
        let args = parse_spawn_args("/spawn --bg analyze test coverage");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.background);
        assert_eq!(args.task, "analyze test coverage");
        assert!(args.output_path.is_none());
        assert!(args.collect_id.is_none());
    }

    #[test]
    fn test_parse_spawn_args_bg_with_output() {
        let args = parse_spawn_args("/spawn --bg -o out.txt summarize codebase");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.background);
        assert_eq!(args.output_path, Some("out.txt".to_string()));
        assert_eq!(args.task, "summarize codebase");
        assert!(args.collect_id.is_none());
    }

    #[test]
    fn test_parse_spawn_collect() {
        let args = parse_spawn_args("/spawn collect 3");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.collect_id, Some(3));
        assert!(args.task.is_empty());
        assert!(!args.background);

        // collect without valid id returns None
        assert!(parse_spawn_args("/spawn collect").is_none());
        assert!(parse_spawn_args("/spawn collect abc").is_none());
    }

    #[test]
    fn test_spawn_tracker_store_handle() {
        let tracker = SpawnTracker::new();
        let id = tracker.register_with_bg("bg task", None, true);

        // Create a trivial JoinHandle via a dedicated runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        #[allow(clippy::async_yields_async)]
        let handle: tokio::task::JoinHandle<()> = rt.block_on(async { tokio::spawn(async {}) });
        tracker.store_handle(id, handle);

        // Verify the handle is stored (it's in the handles map)
        let handles = crate::sync_util::lock_or_recover(&tracker.handles);
        assert!(handles.contains_key(&id));
    }

    #[test]
    fn test_spawn_status_display_bg() {
        let tracker = SpawnTracker::new();

        // Register a foreground task
        let fg_id = tracker.register("fg task", None);
        // Register a background task
        let bg_id = tracker.register_with_bg("bg task", None, true);

        let fg = tracker.get(fg_id).unwrap();
        let bg = tracker.get(bg_id).unwrap();

        // Foreground task should not be marked as background
        assert!(!fg.background);
        // Background task should be marked as background
        assert!(bg.background);

        // Both should be running
        assert_eq!(fg.status, SpawnStatus::Running);
        assert_eq!(bg.status, SpawnStatus::Running);

        // try_collect on a running bg task should return Ok(None)
        assert_eq!(tracker.try_collect(bg_id).unwrap(), None);

        // Complete the bg task and verify try_collect returns the result
        tracker.complete(bg_id, "bg result".to_string());
        let collected = tracker.try_collect(bg_id).unwrap();
        assert!(collected.is_some());
        let (task, result, handoff) = collected.unwrap();
        assert_eq!(task, "bg task");
        assert_eq!(result, "bg result");
        assert!(handoff.is_none());
    }

    #[test]
    fn test_parse_spawn_args_model_flag() {
        let args = parse_spawn_args("/spawn --model claude-haiku-4-5 summarize this file");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.model, Some("claude-haiku-4-5".to_string()));
        assert_eq!(args.task, "summarize this file");
        assert!(!args.background);
        assert!(args.output_path.is_none());
        assert!(args.collect_id.is_none());
    }

    #[test]
    fn test_parse_spawn_args_model_with_bg_and_output() {
        let args =
            parse_spawn_args("/spawn --bg --model gpt-4o -o report.md review error handling");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.background);
        assert_eq!(args.model, Some("gpt-4o".to_string()));
        assert_eq!(args.output_path, Some("report.md".to_string()));
        assert_eq!(args.task, "review error handling");
    }

    #[test]
    fn test_parse_spawn_args_no_model_flag() {
        let args = parse_spawn_args("/spawn do something normal");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.model.is_none());
        assert_eq!(args.task, "do something normal");
    }

    #[test]
    fn test_parse_spawn_args_model_without_value_becomes_task() {
        // --model at the end without a value — treated as task text since
        // the flag requires a following token. It stops flag processing.
        let args = parse_spawn_args("/spawn --model");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.model.is_none());
        assert_eq!(args.task, "--model");
    }

    #[test]
    fn test_parse_spawn_args_system_quoted_prompt() {
        let args =
            parse_spawn_args("/spawn --system \"You are a security auditor\" review src/safety.rs");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(
            args.system_prompt,
            Some("You are a security auditor".to_string())
        );
        assert_eq!(args.task, "review src/safety.rs");
        assert!(args.model.is_none());
        assert!(!args.background);
    }

    #[test]
    fn test_parse_spawn_args_system_single_word() {
        let args = parse_spawn_args("/spawn --system concise summarize this file");
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.system_prompt, Some("concise".to_string()));
        assert_eq!(args.task, "summarize this file");
    }

    #[test]
    fn test_parse_spawn_args_system_with_model_and_bg() {
        let args = parse_spawn_args(
            "/spawn --bg --model gpt-4o --system \"Be brief\" -o out.md analyze errors",
        );
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.background);
        assert_eq!(args.model, Some("gpt-4o".to_string()));
        assert_eq!(args.system_prompt, Some("Be brief".to_string()));
        assert_eq!(args.output_path, Some("out.md".to_string()));
        assert_eq!(args.task, "analyze errors");
    }

    #[test]
    fn test_parse_spawn_args_no_system_flag() {
        let args = parse_spawn_args("/spawn do something normal");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.system_prompt.is_none());
    }

    #[test]
    fn test_parse_spawn_args_system_without_value_becomes_task() {
        // --system at the end without a value — stops flag processing,
        // treated as task text.
        let args = parse_spawn_args("/spawn --system");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.system_prompt.is_none());
        assert_eq!(args.task, "--system");
    }

    // -------------------------------------------------------------------
    // Worktree lifecycle tests
    // -------------------------------------------------------------------

    /// Create a temporary git repo for worktree tests.
    fn setup_temp_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let p = tmp.path();
        // Initialise a repo with at least one commit (worktree add needs a HEAD).
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(p)
            .output()
            .expect("git init");
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(p)
            .output()
            .expect("git config email");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(p)
            .output()
            .expect("git config name");
        std::fs::write(p.join("README.md"), "hello").expect("write file");
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(p)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(p)
            .output()
            .expect("git commit");
        tmp
    }

    #[test]
    fn test_symlink_preflight_real_dir_passes() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".yoyo").join("worktrees")).expect("mkdir");
        let wt = root.join(".yoyo").join("worktrees").join("spawn-1-123");
        assert!(check_worktree_path_escape(root, &wt).is_ok());
    }

    #[test]
    fn test_symlink_preflight_fresh_nonexistent_parent_passes() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let root = tmp.path();
        // Nothing under root exists yet — the common fresh case.
        let wt = root.join(".yoyo").join("worktrees").join("spawn-1-123");
        assert!(check_worktree_path_escape(root, &wt).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_preflight_refuses_escape_outside_repo() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let root = tmp.path().join("repo");
        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(root.join(".yoyo")).expect("mkdir repo/.yoyo");
        std::fs::create_dir_all(&outside).expect("mkdir elsewhere");
        // .yoyo/worktrees is a symlink pointing OUTSIDE the repo root.
        std::os::unix::fs::symlink(&outside, root.join(".yoyo").join("worktrees"))
            .expect("create symlink");
        let wt = root.join(".yoyo").join("worktrees").join("spawn-1-123");
        let err = check_worktree_path_escape(&root, &wt).expect_err("should refuse escape");
        assert!(
            err.contains("symlink"),
            "error should name the symlink, got: {err}"
        );
        assert!(
            err.contains("elsewhere"),
            "error should name where it points, got: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_preflight_allows_symlink_inside_repo() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let root = tmp.path().join("repo");
        std::fs::create_dir_all(root.join(".yoyo")).expect("mkdir repo/.yoyo");
        std::fs::create_dir_all(root.join("real-worktrees")).expect("mkdir real-worktrees");
        // Symlink that resolves back INSIDE the repo root — not an escape.
        std::os::unix::fs::symlink(
            root.join("real-worktrees"),
            root.join(".yoyo").join("worktrees"),
        )
        .expect("create symlink");
        let wt = root.join(".yoyo").join("worktrees").join("spawn-1-123");
        assert!(check_worktree_path_escape(&root, &wt).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_create_spawn_worktree_refuses_symlinked_parent() {
        let tmp = setup_temp_repo();
        let repo = tmp.path();
        let outside = tempfile::tempdir().expect("create outside dir");
        std::fs::create_dir_all(repo.join(".yoyo")).expect("mkdir .yoyo");
        std::os::unix::fs::symlink(outside.path(), repo.join(".yoyo").join("worktrees"))
            .expect("create symlink");
        let err = create_spawn_worktree(repo, 5).expect_err("should refuse symlinked parent");
        assert!(err.contains("symlink"), "got: {err}");
        // Nothing should have been written outside the repo.
        assert!(
            std::fs::read_dir(outside.path())
                .expect("read outside dir")
                .next()
                .is_none(),
            "no files should land outside the repo"
        );
    }

    #[test]
    fn test_worktree_create_and_cleanup() {
        let tmp = setup_temp_repo();
        let repo = tmp.path();

        let info = create_spawn_worktree(repo, 42).expect("create worktree");

        // The path should exist and be under .yoyo/worktrees/spawn-42-*
        assert!(info.path.exists(), "worktree dir should exist");
        let name = info.path.file_name().unwrap().to_string_lossy();
        assert!(
            name.starts_with("spawn-42-"),
            "dir name should start with spawn-42-, got: {name}"
        );

        // The worktree should contain the same file as the original repo.
        assert!(
            info.path.join("README.md").exists(),
            "README.md should be in worktree"
        );

        // branch field should be a short SHA (non-empty for detached HEAD).
        assert!(!info.branch.is_empty(), "branch/HEAD should be recorded");

        // Verify the worktree HEAD matches the main repo HEAD.
        let main_head = run_git_in(repo, &["rev-parse", "HEAD"]).expect("get main HEAD");
        let wt_head = run_git_in(&info.path, &["rev-parse", "HEAD"]).expect("get wt HEAD");
        assert_eq!(main_head, wt_head, "worktree HEAD should match main HEAD");

        // Clean up.
        cleanup_spawn_worktree(repo, &info).expect("cleanup worktree");
        assert!(!info.path.exists(), "worktree dir should be removed");
    }

    #[test]
    fn test_worktree_cleanup_idempotent() {
        let tmp = setup_temp_repo();
        let repo = tmp.path();

        let info = create_spawn_worktree(repo, 7).expect("create worktree");
        assert!(info.path.exists());

        // Clean up twice — second call should succeed silently.
        cleanup_spawn_worktree(repo, &info).expect("first cleanup");
        cleanup_spawn_worktree(repo, &info).expect("second cleanup (idempotent)");
    }

    #[test]
    fn test_worktree_cleanup_after_manual_delete() {
        let tmp = setup_temp_repo();
        let repo = tmp.path();

        let info = create_spawn_worktree(repo, 99).expect("create worktree");
        assert!(info.path.exists());

        // Manually remove the directory (simulating a crash).
        std::fs::remove_dir_all(&info.path).expect("manual delete");
        assert!(!info.path.exists());

        // Cleanup should still succeed (prune metadata).
        cleanup_spawn_worktree(repo, &info).expect("cleanup after manual delete");
    }

    #[test]
    fn test_list_spawn_worktrees() {
        let tmp = setup_temp_repo();
        let repo = tmp.path();

        // Initially no spawn worktrees.
        assert!(list_spawn_worktrees(repo).is_empty());

        let info1 = create_spawn_worktree(repo, 1).expect("create wt 1");
        let info2 = create_spawn_worktree(repo, 2).expect("create wt 2");

        let wts = list_spawn_worktrees(repo);
        assert_eq!(wts.len(), 2, "should list 2 worktrees");

        // Clean up.
        cleanup_spawn_worktree(repo, &info1).unwrap();
        cleanup_spawn_worktree(repo, &info2).unwrap();

        assert!(list_spawn_worktrees(repo).is_empty());
    }

    #[test]
    fn test_cleanup_stale_worktrees() {
        let tmp = setup_temp_repo();
        let repo = tmp.path();

        let _info = create_spawn_worktree(repo, 10).expect("create worktree");

        // With a very large max_age, nothing should be cleaned.
        cleanup_stale_worktrees(repo, std::time::Duration::from_secs(999_999));
        assert_eq!(list_spawn_worktrees(repo).len(), 1);

        // With zero max_age, everything should be cleaned.
        cleanup_stale_worktrees(repo, std::time::Duration::ZERO);
        assert!(
            list_spawn_worktrees(repo).is_empty(),
            "stale worktree should be cleaned"
        );
    }

    #[test]
    fn test_run_git_in_basic() {
        let tmp = setup_temp_repo();
        let repo = tmp.path();

        // Should be able to get the branch.
        let result = run_git_in(repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
        assert!(result.is_ok(), "rev-parse should succeed");
    }

    #[test]
    fn test_run_git_in_error() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        // Not a git repo — should fail.
        let result = run_git_in(tmp.path(), &["status"]);
        assert!(result.is_err(), "should fail in non-git dir");
    }

    #[test]
    fn test_parse_spawn_parallel_flag() {
        let args = parse_spawn_args("/spawn --parallel fix tests --- write docs");
        assert!(args.is_some());
        let args = args.unwrap();
        assert!(args.parallel_tasks.is_some());
    }

    #[test]
    fn test_parse_spawn_parallel_tasks() {
        let args =
            parse_spawn_args("/spawn --parallel fix the auth tests --- write docs for the parser --- add error handling to main");
        assert!(args.is_some());
        let args = args.unwrap();
        let tasks = args.parallel_tasks.unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0], "fix the auth tests");
        assert_eq!(tasks[1], "write docs for the parser");
        assert_eq!(tasks[2], "add error handling to main");
    }

    #[test]
    fn test_parse_spawn_parallel_with_model() {
        let args = parse_spawn_args(
            "/spawn --parallel --model claude-sonnet-4-20250514 task A --- task B",
        );
        assert!(args.is_some());
        let args = args.unwrap();
        assert_eq!(args.model.as_deref(), Some("claude-sonnet-4-20250514"));
        let tasks = args.parallel_tasks.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0], "task A");
        assert_eq!(tasks[1], "task B");
    }

    #[test]
    fn test_parse_spawn_parallel_no_tasks() {
        // --parallel with no task text should return None
        let args = parse_spawn_args("/spawn --parallel");
        assert!(args.is_none());
    }

    // ── detect_parallelizable_tasks ─────────────────────────────────────

    #[test]
    fn test_detect_numbered_list_of_independent_tasks() {
        let prompt = "Please do these:\n\
                      1. Add error handling to src/parser.rs\n\
                      2. Write integration tests for the config loader\n\
                      3. Update the installation docs in README.md";
        let items = detect_parallelizable_tasks(prompt).expect("should detect 3 tasks");
        assert_eq!(items.len(), 3);
        assert!(items[0].contains("src/parser.rs"));
    }

    #[test]
    fn test_detect_bulleted_list_of_four_tasks() {
        // "authentication" contains "then" — word boundaries must prevent
        // a false sequential-marker match.
        let prompt = "- Refactor the authentication module for clarity\n\
                      - Add pagination to the users endpoint\n\
                      - Improve logging in the worker queue\n\
                      - Document the deployment process end to end";
        let items = detect_parallelizable_tasks(prompt).expect("should detect 4 tasks");
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn test_detect_rejects_sequential_then() {
        // Paired negative: same list as the numbered test, but with "then"
        // signaling ordering — minimum-difference from the positive case.
        let prompt = "Please do these:\n\
                      1. Add error handling to src/parser.rs\n\
                      2. Then write integration tests for the config loader\n\
                      3. Update the installation docs in README.md";
        assert_eq!(detect_parallelizable_tasks(prompt), None);
    }

    #[test]
    fn test_detect_rejects_two_items() {
        let prompt = "1. Add error handling to src/parser.rs\n\
                      2. Write integration tests for the config loader";
        assert_eq!(detect_parallelizable_tasks(prompt), None);
    }

    #[test]
    fn test_detect_rejects_shared_file_path() {
        // Two items touch src/parser.rs — worktrees would conflict.
        let prompt = "1. Add error handling to src/parser.rs\n\
                      2. Improve the doc comments in src/parser.rs\n\
                      3. Update the installation docs in README.md";
        assert_eq!(detect_parallelizable_tasks(prompt), None);
    }

    #[test]
    fn test_detect_rejects_plain_prose() {
        let prompt = "Can you explain how the retry logic works and why it \
                      uses exponential backoff instead of a fixed delay?";
        assert_eq!(detect_parallelizable_tasks(prompt), None);
    }

    #[test]
    fn test_detect_rejects_short_fragments() {
        let prompt = "- yes\n- no\n- maybe so";
        assert_eq!(detect_parallelizable_tasks(prompt), None);
    }

    #[test]
    fn test_detect_rejects_first_second_narrative() {
        let prompt = "First, review these areas. Second, report back.\n\
                      1. Add error handling to the parser module\n\
                      2. Write integration tests for the config loader\n\
                      3. Update the installation documentation pages";
        assert_eq!(detect_parallelizable_tasks(prompt), None);
    }

    // --- spawn manifest (#341: codified/replayable orchestration) ---

    #[test]
    fn test_build_spawn_manifest_shape() {
        let tasks = vec![
            "add tests".to_string(),
            "write docs".to_string(),
            "refactor logger".to_string(),
        ];
        let results = vec![
            ("add tests".to_string(), SpawnStatus::Completed),
            ("write docs".to_string(), SpawnStatus::Completed),
            (
                "refactor logger".to_string(),
                SpawnStatus::Failed("boom".to_string()),
            ),
        ];
        let m = build_spawn_manifest("run-abc", &tasks, &results);

        assert_eq!(m["run_id"], "run-abc");
        assert_eq!(m["task_count"], 3);
        let entries = m["tasks"].as_array().expect("tasks array");
        assert_eq!(entries.len(), 3);
        // Indices 0..2 in order.
        assert_eq!(entries[0]["index"], 0);
        assert_eq!(entries[1]["index"], 1);
        assert_eq!(entries[2]["index"], 2);
        // Status strings map correctly (Failed → "failed", no message leak).
        assert_eq!(entries[0]["status"], "completed");
        assert_eq!(entries[1]["status"], "completed");
        assert_eq!(entries[2]["status"], "failed");
        // created_ts present (some non-empty string).
        assert!(m["created_ts"].as_str().is_some());
    }

    #[test]
    fn test_build_spawn_manifest_truncates_long_task() {
        // A task longer than the cap, with a multi-byte char (✓, 3 bytes)
        // straddling the boundary — must truncate on a char boundary, never panic.
        let long = format!("{}✓{}", "a".repeat(MANIFEST_TASK_CAP - 1), "b".repeat(50));
        let tasks = vec![long.clone()];
        let results = vec![(long, SpawnStatus::Running)];
        let m = build_spawn_manifest("run-xyz", &tasks, &results);

        let stored = m["tasks"][0]["task"].as_str().expect("task string");
        // Truncated to at most the cap.
        assert!(stored.len() <= MANIFEST_TASK_CAP, "len {}", stored.len());
        // Shorter than the original (truncation happened).
        assert!(
            stored.len()
                < format!("{}✓{}", "a".repeat(MANIFEST_TASK_CAP - 1), "b".repeat(50)).len()
        );
        // The stored slice is itself valid UTF-8 (it's a &str, so this is
        // implicit — but assert it's a proper prefix boundary explicitly).
        assert!(stored.is_char_boundary(stored.len()));
    }

    #[test]
    fn test_write_spawn_manifest_roundtrip() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        let tasks = vec!["t1".to_string(), "t2".to_string()];
        let results = vec![
            ("t1".to_string(), SpawnStatus::Completed),
            ("t2".to_string(), SpawnStatus::Running),
        ];
        let manifest = build_spawn_manifest("run-roundtrip", &tasks, &results);

        let path = write_spawn_manifest(&dir, &manifest).expect("write manifest");
        assert!(path.exists(), "manifest file should exist");
        assert!(path.ends_with("run-roundtrip.json"));

        let raw = std::fs::read_to_string(&path).expect("read manifest back");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse json");
        assert_eq!(parsed, manifest, "round-trip must be lossless");
        assert_eq!(parsed["run_id"], "run-roundtrip");
        assert_eq!(parsed["task_count"], 2);
        assert_eq!(parsed["tasks"][0]["status"], "completed");
        assert_eq!(parsed["tasks"][1]["status"], "running");
    }

    // --- spawn manifest inspector (read-only, #341) ---

    #[test]
    fn test_list_spawn_manifests_empty_dir_returns_empty() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        // A dir that doesn't exist under tmp — graceful empty, no panic.
        let missing = tmp.path().join("does_not_exist");
        assert!(list_spawn_manifests(&missing).is_empty());
    }

    #[test]
    fn test_list_spawn_manifests_finds_written_manifest() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        let tasks = vec!["t1".to_string()];
        let results = vec![("t1".to_string(), SpawnStatus::Completed)];
        // Round-trip with the WRITER (fuel-line contract, Day 128).
        let manifest = build_spawn_manifest("20260709T100000Z", &tasks, &results);
        write_spawn_manifest(&dir, &manifest).expect("write manifest");

        let listed = list_spawn_manifests(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "20260709T100000Z");
        assert!(listed[0].1.ends_with("20260709T100000Z.json"));
    }

    #[test]
    fn test_format_manifest_summary_contains_statuses() {
        let tasks = vec!["add tests".to_string(), "break build".to_string()];
        let results = vec![
            ("add tests".to_string(), SpawnStatus::Completed),
            (
                "break build".to_string(),
                SpawnStatus::Failed("boom".to_string()),
            ),
        ];
        let m = build_spawn_manifest("run-mixed", &tasks, &results);
        let out = format_manifest_summary(&m);

        assert!(out.contains("run-mixed"), "run_id in summary: {out}");
        assert!(out.contains("completed"), "completed status: {out}");
        assert!(out.contains("failed"), "failed status: {out}");
        assert!(out.contains("add tests"), "task 1 substring: {out}");
        assert!(out.contains("break build"), "task 2 substring: {out}");
    }

    #[test]
    fn test_format_manifest_summary_truncates_long_task() {
        // Long task with a multi-byte char (✓, 3 bytes) — must not panic and
        // must render a shortened form in the summary (char-boundary safe).
        let long = format!("{}✓{}", "x".repeat(80), "y".repeat(80));
        let tasks = vec![long.clone()];
        let results = vec![(long.clone(), SpawnStatus::Running)];
        let m = build_spawn_manifest("run-long", &tasks, &results);
        let out = format_manifest_summary(&m);

        // The full untruncated task must NOT appear (it was shortened).
        assert!(!out.contains(&long), "long task should be truncated: {out}");
        // Status still present.
        assert!(out.contains("running"), "status present: {out}");
        // No panic occurred (reaching here proves char-boundary safety).
    }

    #[test]
    fn parse_spawn_manifest_roundtrips_build_spawn_manifest() {
        let tasks = vec!["task one".to_string(), "task two".to_string()];
        let results = vec![
            ("task one".to_string(), SpawnStatus::Completed),
            ("task two".to_string(), SpawnStatus::Failed("boom".into())),
        ];
        let manifest = build_spawn_manifest("run-abc", &tasks, &results);
        let json = serde_json::to_string(&manifest).expect("serialize manifest");

        let parsed = parse_spawn_manifest(&json).expect("parse manifest");
        assert_eq!(parsed.run_id, "run-abc");
        assert_eq!(parsed.task_count, 2);
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].task, "task one");
        assert_eq!(parsed.tasks[0].index, 0);
        assert_eq!(parsed.tasks[1].index, 1);
        // Each parsed status matches the stable string mapping.
        assert_eq!(parsed.tasks[0].status, manifest_status_str(&results[0].1));
        assert_eq!(parsed.tasks[1].status, manifest_status_str(&results[1].1));
    }

    #[test]
    fn parse_spawn_manifest_rejects_non_object() {
        assert!(parse_spawn_manifest("[]").is_none());
        assert!(parse_spawn_manifest("not json").is_none());
    }

    #[test]
    fn parse_spawn_manifest_degrades_on_missing_fields() {
        let parsed = parse_spawn_manifest(r#"{"run_id":"r"}"#).expect("parse minimal");
        assert_eq!(parsed.run_id, "r");
        assert_eq!(parsed.created_ts, "");
        assert_eq!(parsed.task_count, 0);
        assert!(parsed.tasks.is_empty());
    }

    #[test]
    fn read_spawn_manifest_missing_file_returns_none() {
        let path = std::path::Path::new("/nonexistent/does/not/exist-xyz.json");
        assert!(read_spawn_manifest(path).is_none());
    }

    // --- /spawn replay (#341: the reader half of the manifest door) ---

    /// Write a manifest for `run_id` with the given tasks into `dir`.
    fn write_test_manifest(dir: &Path, run_id: &str, tasks: &[&str]) -> PathBuf {
        let tasks: Vec<String> = tasks.iter().map(|t| t.to_string()).collect();
        let results: Vec<(String, SpawnStatus)> = tasks
            .iter()
            .map(|t| (t.clone(), SpawnStatus::Completed))
            .collect();
        let manifest = build_spawn_manifest(run_id, &tasks, &results);
        write_spawn_manifest(dir, &manifest).expect("write manifest")
    }

    #[test]
    fn select_replay_manifest_latest_picks_newest_of_multiple() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        write_test_manifest(&dir, "20260101T000000Z", &["old task"]);
        write_test_manifest(&dir, "20260301T120000Z", &["newest task"]);
        write_test_manifest(&dir, "20260201T090000Z", &["middle task"]);

        // Both bare (None) and explicit "latest" resolve to the newest run id.
        let picked = select_replay_manifest(&dir, None).expect("latest");
        assert!(picked.ends_with("20260301T120000Z.json"), "{picked:?}");
        let picked = select_replay_manifest(&dir, Some("latest")).expect("latest");
        assert!(picked.ends_with("20260301T120000Z.json"), "{picked:?}");
    }

    #[test]
    fn select_replay_manifest_specific_id_resolves_path() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        write_test_manifest(&dir, "20260101T000000Z", &["a task"]);

        let picked = select_replay_manifest(&dir, Some("20260101T000000Z")).expect("by id");
        assert!(picked.ends_with("20260101T000000Z.json"), "{picked:?}");
    }

    #[test]
    fn select_replay_manifest_empty_dir_is_honest_error() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let missing = tmp.path().join("no_such_dir");
        let err = select_replay_manifest(&missing, None).expect_err("must error");
        assert!(err.contains("no spawn manifests found"), "{err}");
        assert!(err.contains("--parallel"), "points at the writer: {err}");
    }

    #[test]
    fn select_replay_manifest_unknown_id_is_honest_error() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        write_test_manifest(&dir, "20260101T000000Z", &["a task"]);

        let err = select_replay_manifest(&dir, Some("bogus-id")).expect_err("must error");
        assert!(err.contains("bogus-id"), "names the missing id: {err}");
        assert!(err.contains("/spawn runs"), "points at discovery: {err}");
    }

    #[test]
    fn load_replay_tasks_roundtrips_written_manifest() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        write_test_manifest(&dir, "20260101T000000Z", &["fix auth", "write docs"]);

        let (manifest, tasks) = load_replay_tasks(&dir, Some("20260101T000000Z")).expect("load");
        assert_eq!(manifest.run_id, "20260101T000000Z");
        assert_eq!(
            tasks,
            vec!["fix auth".to_string(), "write docs".to_string()]
        );
    }

    #[test]
    fn load_replay_tasks_corrupt_json_is_honest_error() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("20260101T000000Z.json"), "{not valid json").expect("write");

        let err = load_replay_tasks(&dir, Some("20260101T000000Z")).expect_err("must error");
        assert!(err.contains("corrupt"), "names corruption: {err}");
    }

    #[test]
    fn load_replay_tasks_empty_task_list_is_honest_error() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let dir = tmp.path().join("spawn_runs");
        // A structurally valid manifest with zero tasks.
        let manifest = build_spawn_manifest("20260101T000000Z", &[], &[]);
        write_spawn_manifest(&dir, &manifest).expect("write manifest");

        let err = load_replay_tasks(&dir, Some("20260101T000000Z")).expect_err("must error");
        assert!(err.contains("no tasks to replay"), "{err}");
        assert!(err.contains("20260101T000000Z"), "names the run: {err}");
    }

    #[test]
    fn manifest_replay_tasks_skips_empty_entries() {
        let manifest = ParsedManifest {
            run_id: "r".to_string(),
            created_ts: "ts".to_string(),
            task_count: 3,
            tasks: vec![
                ParsedManifestTask {
                    index: 0,
                    task: "real task".to_string(),
                    status: "completed".to_string(),
                },
                ParsedManifestTask {
                    index: 1,
                    task: String::new(),
                    status: "completed".to_string(),
                },
            ],
        };
        let tasks = manifest_replay_tasks(&manifest).expect("non-empty");
        assert_eq!(tasks, vec!["real task".to_string()]);
    }

    // --- Near-miss typo guard for /spawn free-text args ---

    #[test]
    fn test_spawn_near_miss_fires_on_typo() {
        // "/spawn statsu" is a typo for "status", not a task — refuse
        // instead of spawning an agent whose task is the word "statsu".
        assert_eq!(spawn_near_miss("statsu"), Some("status"));
        assert_eq!(spawn_near_miss("colect"), Some("collect"));
        assert_eq!(spawn_near_miss("repaly"), Some("replay"));
    }

    #[test]
    fn test_spawn_near_miss_silent_on_exact_subcommand() {
        // Exact subcommands are handled by their own match arms — the guard
        // must never fire on them.
        for sub in SPAWN_SUBCOMMANDS {
            assert_eq!(spawn_near_miss(sub), None, "guard fired on exact '{sub}'");
        }
    }

    #[test]
    fn test_spawn_near_miss_silent_on_multi_word_task() {
        // Multi-word args are real tasks — free text stays allowed, even
        // when a word inside is close to a subcommand.
        assert_eq!(spawn_near_miss("statsu the build pipeline"), None);
        assert_eq!(spawn_near_miss("add tests for the parser"), None);
    }

    #[test]
    fn test_spawn_near_miss_silent_on_far_single_word() {
        // Negative side (Days 122-124 lesson): a single word FAR from any
        // subcommand must pass through untouched.
        assert_eq!(spawn_near_miss("investigate"), None);
        assert_eq!(spawn_near_miss("benchmark"), None);
    }

    #[test]
    fn test_spawn_near_miss_silent_on_empty() {
        assert_eq!(spawn_near_miss(""), None);
    }

    // -- spawn worktree cwd pinning (#621) --------------------------------

    fn fake_worktree(path: &str) -> WorktreeInfo {
        WorktreeInfo {
            path: PathBuf::from(path),
            branch: "spawn/1".to_string(),
            created_at: Instant::now(),
        }
    }

    fn base_agent_config() -> crate::AgentConfig {
        crate::AgentConfig {
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            skills: yoagent::skills::SkillSet::default(),
            system_prompt: "test".to_string(),
            thinking: yoagent::ThinkingLevel::Off,
            max_tokens: None,
            temperature: None,
            max_turns: None,
            auto_approve: true,
            auto_commit: false,
            permissions: crate::cli::PermissionConfig::default(),
            dir_restrictions: crate::cli::DirectoryRestrictions::default(),
            context_strategy: crate::cli::ContextStrategy::default(),
            context_window: None,
            shell_hooks: vec![],
            fallback_provider: None,
            fallback_model: None,
            auto_watch: false,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            no_tools: false,
            lite: false,
            bash_cwd: None,
        }
    }

    #[test]
    fn test_spawn_bash_cwd_maps_worktree_path() {
        let wt = fake_worktree("/tmp/yoyo-spawn-42");
        assert_eq!(
            spawn_bash_cwd(Some(&wt)),
            Some("/tmp/yoyo-spawn-42".to_string())
        );
    }

    #[test]
    fn test_spawn_bash_cwd_none_without_worktree() {
        assert_eq!(spawn_bash_cwd(None), None);
    }

    #[test]
    fn test_sub_config_bash_cwd_wired_from_worktree() {
        // Mirrors the production wiring in handle_spawn: the sub_config built
        // for a spawn worker gets bash_cwd pinned to the worktree path.
        let base = base_agent_config();
        let worktree = Some(fake_worktree("/tmp/yoyo-spawn-worktree"));
        let sub_config = crate::AgentConfig {
            system_prompt: "spawn task".to_string(),
            bash_cwd: spawn_bash_cwd(worktree.as_ref()),
            ..clone_agent_config(&base)
        };
        assert_eq!(
            sub_config.bash_cwd,
            Some("/tmp/yoyo-spawn-worktree".to_string())
        );
    }

    #[test]
    fn test_sub_config_bash_cwd_none_when_worktree_unavailable() {
        // When worktree creation failed, the worker inherits the process cwd
        // (bash_cwd stays None) — same fallback as before #621.
        let base = base_agent_config();
        let worktree: Option<WorktreeInfo> = None;
        let sub_config = crate::AgentConfig {
            system_prompt: "spawn task".to_string(),
            bash_cwd: spawn_bash_cwd(worktree.as_ref()),
            ..clone_agent_config(&base)
        };
        assert_eq!(sub_config.bash_cwd, None);
    }
}
