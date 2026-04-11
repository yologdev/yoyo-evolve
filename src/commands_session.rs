//! Session-related command handlers: /save, /load, /compact, /history, /search,
//! /mark, /jump, /marks, /spawn, /export, /stash.

use crate::format::*;
use crate::prompt::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::types::{AgentMessage, Content, Message};
use yoagent::*;

use crate::cli::{
    AUTO_COMPACT_THRESHOLD, AUTO_SAVE_SESSION_PATH, DEFAULT_SESSION_PATH,
    PROACTIVE_COMPACT_THRESHOLD,
};

// ── compact thrash detection ─────────────────────────────────────────────

/// Tracks consecutive low-yield compactions to avoid thrashing.
static COMPACT_THRASH_COUNT: AtomicU32 = AtomicU32::new(0);

/// Number of consecutive low-yield compactions before we stop auto-compacting.
const COMPACT_THRASH_THRESHOLD: u32 = 2;

/// Minimum token reduction ratio to count as a "meaningful" compaction.
const COMPACT_MIN_REDUCTION: f64 = 0.10;

/// Reset the thrash counter (call when context changes significantly, e.g. /clear, /load).
pub fn reset_compact_thrash() {
    COMPACT_THRASH_COUNT.store(0, Ordering::Relaxed);
}

/// Check whether auto-compaction is currently suppressed due to thrashing.
pub fn is_compact_thrashing() -> bool {
    COMPACT_THRASH_COUNT.load(Ordering::Relaxed) >= COMPACT_THRASH_THRESHOLD
}

// ── compact ──────────────────────────────────────────────────────────────

/// Compact the agent's conversation and return (before_count, before_tokens, after_count, after_tokens).
/// Returns None if nothing changed. Updates the thrash counter based on reduction quality.
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
        // Track whether the compaction was meaningful for thrash detection
        let reduction = if before_tokens > 0 {
            (before_tokens - after_tokens) as f64 / before_tokens as f64
        } else {
            0.0
        };
        if reduction < COMPACT_MIN_REDUCTION {
            COMPACT_THRASH_COUNT.fetch_add(1, Ordering::Relaxed);
        } else {
            COMPACT_THRASH_COUNT.store(0, Ordering::Relaxed);
        }
        Some((before_count, before_tokens, after_count, after_tokens))
    }
}

/// Auto-compact conversation if context window usage exceeds threshold.
/// Skips compaction if recent attempts haven't freed meaningful tokens (thrash detection).
pub fn auto_compact_if_needed(agent: &mut Agent) {
    let messages = agent.messages().to_vec();
    let used = total_tokens(&messages) as u64;
    let ratio = used as f64 / crate::cli::effective_context_tokens() as f64;

    if ratio > AUTO_COMPACT_THRESHOLD {
        if is_compact_thrashing() {
            eprintln!(
                "{DIM}  ⚠ Context is mostly incompressible — consider /clear or starting a new session{RESET}"
            );
            return;
        }
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

/// Proactively compact conversation if context usage exceeds the proactive threshold.
/// This runs BEFORE a prompt attempt (not after) to prevent overflow during agentic execution.
/// Uses a tighter threshold (0.70) than the post-turn auto-compact (0.80).
/// Skips compaction if recent attempts haven't freed meaningful tokens (thrash detection).
/// Returns true if compaction was performed.
pub fn proactive_compact_if_needed(agent: &mut Agent) -> bool {
    let messages = agent.messages().to_vec();
    let used = total_tokens(&messages) as u64;
    let ratio = used as f64 / crate::cli::effective_context_tokens() as f64;

    if ratio > PROACTIVE_COMPACT_THRESHOLD {
        if is_compact_thrashing() {
            eprintln!(
                "{DIM}  ⚠ Context is mostly incompressible — consider /clear or starting a new session{RESET}"
            );
            return false;
        }
        if let Some((before_count, before_tokens, after_count, after_tokens)) = compact_agent(agent)
        {
            eprintln!(
                "{DIM}  ⚡ proactive compact: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}",
                format_token_count(before_tokens),
                format_token_count(after_tokens)
            );
            return true;
        }
    }
    false
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

// ── auto-save ────────────────────────────────────────────────────────────

/// Check whether a previous auto-saved session exists at `.yoyo/last-session.json`.
pub fn last_session_exists() -> bool {
    std::path::Path::new(AUTO_SAVE_SESSION_PATH).exists()
}

/// Auto-save the current conversation to `.yoyo/last-session.json`.
/// Creates the `.yoyo/` directory if it doesn't exist.
/// Silently ignores errors (best-effort crash recovery).
pub fn auto_save_on_exit(agent: &Agent) {
    if agent.messages().is_empty() {
        return;
    }
    if let Ok(json) = agent.save_messages() {
        // Ensure .yoyo/ directory exists
        let _ = std::fs::create_dir_all(".yoyo");
        if std::fs::write(AUTO_SAVE_SESSION_PATH, &json).is_ok() {
            eprintln!(
                "{DIM}  session auto-saved to {AUTO_SAVE_SESSION_PATH} ({} messages){RESET}",
                agent.messages().len()
            );
        }
    }
}

/// Return the path to load for `--continue`: use `.yoyo/last-session.json` if it exists,
/// otherwise fall back to the legacy `yoyo-session.json`.
pub fn continue_session_path() -> &'static str {
    if last_session_exists() {
        AUTO_SAVE_SESSION_PATH
    } else {
        DEFAULT_SESSION_PATH
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

// ── /mark, /jump, /marks (bookmarks) ─────────────────────────────────────

/// Storage for conversation bookmarks: named snapshots of the message list.
pub type Bookmarks = HashMap<String, String>;

/// Parse the bookmark name from `/mark <name>` input.
/// Returns None if no name is provided.
pub fn parse_bookmark_name(input: &str, prefix: &str) -> Option<String> {
    let name = input.strip_prefix(prefix).unwrap_or("").trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Handle `/mark <name>`: save the current conversation state as a named bookmark.
pub fn handle_mark(agent: &Agent, input: &str, bookmarks: &mut Bookmarks) {
    let name = match parse_bookmark_name(input, "/mark") {
        Some(n) => n,
        None => {
            println!("{DIM}  usage: /mark <name>");
            println!("  Save a bookmark at the current point in the conversation.");
            println!("  Use /jump <name> to return to this point later.{RESET}\n");
            return;
        }
    };

    match agent.save_messages() {
        Ok(json) => {
            let msg_count = agent.messages().len();
            let overwriting = bookmarks.contains_key(&name);
            bookmarks.insert(name.clone(), json);
            if overwriting {
                println!("{GREEN}  ✓ bookmark '{name}' updated ({msg_count} messages){RESET}\n");
            } else {
                println!("{GREEN}  ✓ bookmark '{name}' saved ({msg_count} messages){RESET}\n");
            }
        }
        Err(e) => eprintln!("{RED}  error saving bookmark: {e}{RESET}\n"),
    }
}

/// Handle `/jump <name>`: restore conversation to a previously saved bookmark.
pub fn handle_jump(agent: &mut Agent, input: &str, bookmarks: &Bookmarks) {
    let name = match parse_bookmark_name(input, "/jump") {
        Some(n) => n,
        None => {
            println!("{DIM}  usage: /jump <name>");
            println!("  Restore the conversation to a previously saved bookmark.");
            println!("  Messages added after the bookmark will be discarded.{RESET}\n");
            return;
        }
    };

    match bookmarks.get(&name) {
        Some(json) => match agent.restore_messages(json) {
            Ok(_) => {
                let msg_count = agent.messages().len();
                println!("{GREEN}  ✓ jumped to bookmark '{name}' ({msg_count} messages){RESET}\n");
            }
            Err(e) => eprintln!("{RED}  error restoring bookmark: {e}{RESET}\n"),
        },
        None => {
            let available: Vec<&str> = bookmarks.keys().map(|k| k.as_str()).collect();
            if available.is_empty() {
                eprintln!("{RED}  bookmark '{name}' not found — no bookmarks saved yet.");
                eprintln!("  Use /mark <name> to save one.{RESET}\n");
            } else {
                eprintln!("{RED}  bookmark '{name}' not found.");
                eprintln!("{DIM}  available: {}{RESET}\n", available.join(", "));
            }
        }
    }
}

/// Handle `/marks`: list all saved bookmarks.
pub fn handle_marks(bookmarks: &Bookmarks) {
    if bookmarks.is_empty() {
        println!("{DIM}  (no bookmarks saved)");
        println!("  Use /mark <name> to save a bookmark.{RESET}\n");
    } else {
        println!("{DIM}  Saved bookmarks:");
        let mut names: Vec<&String> = bookmarks.keys().collect();
        names.sort();
        for name in names {
            println!("    • {name}");
        }
        println!("{RESET}");
    }
}

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
}

/// Thread-safe tracker for multiple spawn tasks.
#[derive(Debug, Clone)]
pub struct SpawnTracker {
    inner: Arc<Mutex<Vec<SpawnTask>>>,
}

impl SpawnTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a new spawn task and return its ID.
    pub fn register(&self, task: &str, output_path: Option<String>) -> usize {
        let mut tasks = self.inner.lock().unwrap();
        let id = tasks.len() + 1;
        tasks.push(SpawnTask {
            id,
            task: task.to_string(),
            status: SpawnStatus::Running,
            result: None,
            output_path,
        });
        id
    }

    /// Mark a task as completed with its result.
    pub fn complete(&self, id: usize, result: String) {
        let mut tasks = self.inner.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.status = SpawnStatus::Completed;
            task.result = Some(result);
        }
    }

    /// Mark a task as failed.
    pub fn fail(&self, id: usize, error: String) {
        let mut tasks = self.inner.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.status = SpawnStatus::Failed(error);
            task.result = None;
        }
    }

    /// Get a snapshot of all tracked tasks.
    pub fn snapshot(&self) -> Vec<SpawnTask> {
        self.inner.lock().unwrap().clone()
    }

    /// Count tasks by status.
    pub fn count_by_status(&self) -> (usize, usize, usize) {
        let tasks = self.inner.lock().unwrap();
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
}

#[cfg(test)]
impl SpawnTracker {
    /// Get a specific task by ID.
    pub fn get(&self, id: usize) -> Option<SpawnTask> {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id)
            .cloned()
    }

    /// Return the number of tracked tasks.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Return true if no tasks have been tracked.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

/// Parsed `/spawn` command input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnArgs {
    /// The task for the subagent.
    pub task: String,
    /// Optional output file path (`-o <path>`).
    pub output_path: Option<String>,
}

/// Parse the `/spawn` command input, extracting flags and task.
///
/// Supports:
/// - `/spawn <task>` — basic spawn
/// - `/spawn -o <path> <task>` — spawn with output capture to file
/// - `/spawn status` — show status (returns None to signal status request)
///
/// Returns `None` if no task or if this is a subcommand like `status`.
pub fn parse_spawn_args(input: &str) -> Option<SpawnArgs> {
    let rest = input.strip_prefix("/spawn").unwrap_or("").trim();
    if rest.is_empty() || rest == "status" {
        return None;
    }

    let parts: Vec<&str> = rest.splitn(3, ' ').collect();

    // Check for -o flag
    if parts.len() >= 3 && parts[0] == "-o" {
        let output_path = parts[1].to_string();
        let task = parts[2].to_string();
        if task.is_empty() {
            return None;
        }
        return Some(SpawnArgs {
            task,
            output_path: Some(output_path),
        });
    }

    // No flags, entire rest is the task
    Some(SpawnArgs {
        task: rest.to_string(),
        output_path: None,
    })
}

/// Parse the task from a `/spawn <task>` input (legacy compat).
/// Returns None if no task is provided.
#[cfg(test)]
pub fn parse_spawn_task(input: &str) -> Option<String> {
    parse_spawn_args(input).map(|args| args.task)
}

/// Build a context prompt for a subagent, including project context and
/// a summary of the current conversation.
///
/// This gives the subagent awareness of:
/// - Project context files (YOYO.md, CLAUDE.md)
/// - Project memories
/// - A brief summary of the current conversation state
pub fn spawn_context_prompt(
    main_messages: &[AgentMessage],
    project_context: Option<&str>,
) -> String {
    let mut parts = Vec::new();

    parts.push(
        "You are a subagent spawned from a main coding agent session. \
         Complete the task you are given thoroughly and concisely. \
         Your output will be reported back to the main agent."
            .to_string(),
    );

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
pub fn format_spawn_result(task: &str, result: &str, spawn_id: usize) -> String {
    let result_text = if result.trim().is_empty() {
        "(no output)".to_string()
    } else {
        result.trim().to_string()
    };

    format!(
        "Subagent #{spawn_id} completed a task. Here is its result:\n\n\
         **Task:** {task}\n\n\
         **Result:**\n{result_text}"
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
            SpawnStatus::Running => println!(
                "    {CYAN}{status_icon} #{id}: {task_preview}{output_note}{RESET}",
                id = task.id
            ),
            SpawnStatus::Completed => println!(
                "    {GREEN}{status_icon} #{id}: {task_preview}{output_note}{RESET}",
                id = task.id
            ),
            SpawnStatus::Failed(_) => println!(
                "    {RED}{status_icon} #{id}: {task_preview}{output_note}{RESET}",
                id = task.id
            ),
        }
    }
    println!("{RESET}");
}

/// Handle the /spawn command: create a subagent with project context, run a task,
/// and return the result. Supports output capture and task tracking.
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

    let args = match parse_spawn_args(input) {
        Some(a) => a,
        None => {
            println!("{DIM}  usage: /spawn <task>");
            println!("         /spawn -o <file> <task>   (capture output to file)");
            println!("         /spawn status             (show tracked spawns)");
            println!("  Spawn a subagent with project context to handle a task.");
            println!("  The result is summarized back into your main conversation.");
            println!("  Example: /spawn read src/main.rs and summarize the architecture{RESET}\n");
            return None;
        }
    };

    // Register task in tracker
    let spawn_id = tracker.register(&args.task, args.output_path.clone());

    println!("{CYAN}  🐙 spawning subagent #{spawn_id}...{RESET}");
    println!(
        "{DIM}  task: {}{RESET}",
        crate::format::truncate_with_ellipsis(&args.task, 100)
    );

    // Load project context for the subagent
    let project_context = crate::cli::load_project_context();
    let context_prompt = spawn_context_prompt(main_messages, project_context.as_deref());

    // Build a fresh agent with context-enriched system prompt
    let sub_config = crate::AgentConfig {
        system_prompt: context_prompt,
        ..clone_agent_config(agent_config)
    };
    // Subagent inherits the same tools and permissions
    let mut sub_agent = sub_config.build_agent();

    // Run the task
    let response = run_prompt(&mut sub_agent, &args.task, session_total, model)
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
                return None;
            }
        }
    }

    // Mark completed in tracker
    tracker.complete(spawn_id, response.clone());

    println!("\n{GREEN}  ✓ subagent #{spawn_id} completed{RESET}");
    println!("{DIM}  injecting result into main conversation...{RESET}\n");

    let context_msg = format_spawn_result(&args.task, &response, spawn_id);
    Some(context_msg)
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
    }
}

// ── /export ───────────────────────────────────────────────────────────────

/// Default export file path.
const DEFAULT_EXPORT_PATH: &str = "conversation.md";

/// Format a conversation as readable markdown.
///
/// For each message:
/// - User messages → `## User\n\n{text}\n\n`
/// - Assistant messages → `## Assistant\n\n{text}\n\n` (text and thinking blocks, skips tool calls)
/// - Tool results → `### Tool: {name}\n\n```\n{output}\n```\n\n`
pub fn format_conversation_as_markdown(messages: &[AgentMessage]) -> String {
    let mut out = String::new();
    out.push_str("# Conversation\n\n");

    for msg in messages {
        match msg {
            AgentMessage::Llm(Message::User { content, .. }) => {
                out.push_str("## User\n\n");
                for c in content {
                    if let Content::Text { text } = c {
                        out.push_str(text);
                        out.push_str("\n\n");
                    }
                }
            }
            AgentMessage::Llm(Message::Assistant { content, .. }) => {
                out.push_str("## Assistant\n\n");
                for c in content {
                    match c {
                        Content::Text { text } if !text.is_empty() => {
                            out.push_str(text);
                            out.push_str("\n\n");
                        }
                        Content::Thinking { thinking, .. } if !thinking.is_empty() => {
                            out.push_str("*Thinking:*\n\n> ");
                            // Indent thinking text as a blockquote
                            out.push_str(&thinking.replace('\n', "\n> "));
                            out.push_str("\n\n");
                        }
                        _ => {} // skip tool calls, empty text/thinking
                    }
                }
            }
            AgentMessage::Llm(Message::ToolResult {
                tool_name, content, ..
            }) => {
                out.push_str(&format!("### Tool: {tool_name}\n\n"));
                let text: String = content
                    .iter()
                    .filter_map(|c| match c {
                        Content::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    out.push_str("```\n");
                    out.push_str(&text);
                    out.push_str("\n```\n\n");
                }
            }
            AgentMessage::Extension(_) => {} // skip extension messages
        }
    }

    out
}

/// Parse the export path from `/export [path]` input.
pub fn parse_export_path(input: &str) -> &str {
    let path = input.strip_prefix("/export").unwrap_or("").trim();
    if path.is_empty() {
        DEFAULT_EXPORT_PATH
    } else {
        path
    }
}

/// Handle `/export [path]`: save the current conversation as a readable markdown file.
pub fn handle_export(agent: &Agent, input: &str) {
    let path = parse_export_path(input);
    let messages = agent.messages();

    if messages.is_empty() {
        println!("{DIM}  (no messages to export){RESET}\n");
        return;
    }

    let markdown = format_conversation_as_markdown(messages);
    match std::fs::write(path, &markdown) {
        Ok(_) => println!(
            "{GREEN}  ✓ conversation exported to {path} ({} messages){RESET}\n",
            messages.len()
        ),
        Err(e) => eprintln!("{RED}  error writing to {path}: {e}{RESET}\n"),
    }
}

// ── /stash ──────────────────────────────────────────────────────────────

/// A single stash entry holding a serialized conversation snapshot.
struct StashEntry {
    description: String,
    messages_json: String,
    timestamp: String,
}

/// Global conversation stash stack. Like `git stash` but for your conversation.
static CONVERSATION_STASH: RwLock<Vec<StashEntry>> = RwLock::new(Vec::new());

/// Parse a `/stash` subcommand from user input.
///
/// Returns `(subcommand, argument)` where subcommand is one of:
/// `"push"`, `"pop"`, `"list"`, `"drop"`, or `"push"` as default.
pub fn parse_stash_subcommand(input: &str) -> (&str, &str) {
    let rest = input.strip_prefix("/stash").unwrap_or("").trim();

    if rest.is_empty() {
        return ("push", "");
    }

    // Check for explicit subcommands
    if rest == "pop" || rest.starts_with("pop ") {
        return ("pop", rest.strip_prefix("pop").unwrap_or("").trim());
    }
    if rest == "list" {
        return ("list", "");
    }
    if rest == "drop" || rest.starts_with("drop ") {
        return ("drop", rest.strip_prefix("drop").unwrap_or("").trim());
    }
    if rest.starts_with("push ") {
        return ("push", rest.strip_prefix("push").unwrap_or("").trim());
    }
    if rest == "push" {
        return ("push", "");
    }

    // Anything else is treated as a description for push
    ("push", rest)
}

/// Push the current conversation onto the stash and clear the agent's messages.
pub fn handle_stash_push(agent: &mut Agent, description: &str) -> String {
    let messages_json = match agent.save_messages() {
        Ok(json) => json,
        Err(e) => return format!("{RED}  failed to save conversation: {e}{RESET}\n"),
    };

    let msg_count = agent.messages().len();
    let mut stash = CONVERSATION_STASH.write().unwrap();
    let idx = stash.len();
    let desc = if description.is_empty() {
        format!("stash@{{{idx}}}")
    } else {
        description.to_string()
    };

    let timestamp = {
        use std::time::SystemTime;
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Simple HH:MM:SS from epoch seconds (UTC)
        let h = (secs % 86400) / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    };

    stash.push(StashEntry {
        description: desc.clone(),
        messages_json,
        timestamp,
    });

    // Clear the conversation
    agent.replace_messages(Vec::new());

    format!("{GREEN}  ✓ stashed: \"{desc}\" ({msg_count} messages) — conversation cleared{RESET}\n")
}

/// Pop the most recent stash entry and restore it.
pub fn handle_stash_pop(agent: &mut Agent) -> String {
    let mut stash = CONVERSATION_STASH.write().unwrap();
    if stash.is_empty() {
        return format!("{DIM}  (stash is empty — nothing to pop){RESET}\n");
    }

    let entry = stash.pop().unwrap();
    drop(stash); // release lock before restoring

    match agent.restore_messages(&entry.messages_json) {
        Ok(_) => format!(
            "{GREEN}  ✓ popped: \"{}\" ({} messages restored){RESET}\n",
            entry.description,
            agent.messages().len()
        ),
        Err(e) => format!("{RED}  failed to restore stash: {e}{RESET}\n"),
    }
}

/// List all stash entries.
pub fn handle_stash_list() -> String {
    let stash = CONVERSATION_STASH.read().unwrap();
    if stash.is_empty() {
        return format!("{DIM}  (stash is empty){RESET}\n");
    }

    let mut out = String::new();
    out.push_str(&format!(
        "{DIM}  Conversation stash ({} entries):\n",
        stash.len()
    ));
    for (i, entry) in stash.iter().rev().enumerate() {
        let idx = stash.len() - 1 - i;
        out.push_str(&format!(
            "    {idx}: {} [{}]\n",
            entry.description, entry.timestamp
        ));
    }
    out.push_str(&format!("{RESET}"));
    out
}

/// Drop a stash entry by index.
pub fn handle_stash_drop(index_str: &str) -> String {
    let index: usize = if index_str.is_empty() {
        // Default: drop the most recent (top of stack)
        let stash = CONVERSATION_STASH.read().unwrap();
        if stash.is_empty() {
            return format!("{DIM}  (stash is empty — nothing to drop){RESET}\n");
        }
        stash.len() - 1
    } else {
        match index_str.parse() {
            Ok(n) => n,
            Err(_) => return format!("{RED}  invalid index: {index_str}{RESET}\n"),
        }
    };

    let mut stash = CONVERSATION_STASH.write().unwrap();
    if index >= stash.len() {
        return format!(
            "{RED}  stash index {index} out of range (have {} entries){RESET}\n",
            stash.len()
        );
    }

    let entry = stash.remove(index);
    format!(
        "{GREEN}  ✓ dropped: \"{}\" (index {index}){RESET}\n",
        entry.description
    )
}

/// Dispatch a `/stash` command.
pub fn handle_stash(agent: &mut Agent, input: &str) -> String {
    let (subcmd, arg) = parse_stash_subcommand(input);
    match subcmd {
        "push" => handle_stash_push(agent, arg),
        "pop" => handle_stash_pop(agent),
        "list" => handle_stash_list(),
        "drop" => handle_stash_drop(arg),
        _ => format!("{DIM}  unknown stash subcommand: {subcmd}{RESET}\n"),
    }
}

/// Return the description used for a stash entry when none is provided.
/// Useful for testing the auto-generated name.
#[cfg(test)]
pub fn stash_default_description(index: usize) -> String {
    format!("stash@{{{index}}}")
}

// ── clear confirmation ──────────────────────────────────────────────────

/// Build a confirmation prompt for `/clear` when the conversation has significant history.
///
/// Returns `None` if the message count is ≤ 4 (clear immediately, no prompt needed).
/// Returns `Some(prompt_string)` if confirmation should be requested.
pub fn clear_confirmation_message(message_count: usize, token_count: u64) -> Option<String> {
    if message_count <= 4 {
        return None;
    }
    Some(format!(
        "Clear {} messages (~{} tokens)? [y/N] ",
        message_count,
        format_token_count(token_count)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::AUTO_SAVE_SESSION_PATH;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};

    // ── compact thrash detection tests ────────────────────────────────────

    #[test]
    fn test_compact_thrash_constants() {
        assert_eq!(COMPACT_THRASH_THRESHOLD, 2);
        assert!((COMPACT_MIN_REDUCTION - 0.10).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reset_compact_thrash() {
        // Set to some value, then reset
        COMPACT_THRASH_COUNT.store(5, Ordering::Relaxed);
        reset_compact_thrash();
        assert_eq!(COMPACT_THRASH_COUNT.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_compact_thrash_detection_increments_on_low_reduction() {
        reset_compact_thrash();
        assert!(!is_compact_thrashing());

        // Simulate two low-yield compactions
        COMPACT_THRASH_COUNT.fetch_add(1, Ordering::Relaxed);
        assert!(!is_compact_thrashing()); // 1 < 2
        COMPACT_THRASH_COUNT.fetch_add(1, Ordering::Relaxed);
        assert!(is_compact_thrashing()); // 2 >= 2

        reset_compact_thrash(); // cleanup
    }

    #[test]
    fn test_compact_thrash_detection_resets_on_meaningful_reduction() {
        reset_compact_thrash();

        // Simulate hitting thrash state
        COMPACT_THRASH_COUNT.store(2, Ordering::Relaxed);
        assert!(is_compact_thrashing());

        // A meaningful compaction resets it
        COMPACT_THRASH_COUNT.store(0, Ordering::Relaxed);
        assert!(!is_compact_thrashing());

        reset_compact_thrash(); // cleanup
    }

    #[test]
    fn test_is_compact_thrashing_boundary() {
        reset_compact_thrash();

        // Below threshold
        COMPACT_THRASH_COUNT.store(1, Ordering::Relaxed);
        assert!(!is_compact_thrashing());

        // At threshold
        COMPACT_THRASH_COUNT.store(2, Ordering::Relaxed);
        assert!(is_compact_thrashing());

        // Above threshold
        COMPACT_THRASH_COUNT.store(10, Ordering::Relaxed);
        assert!(is_compact_thrashing());

        reset_compact_thrash(); // cleanup
    }

    #[test]
    fn test_auto_save_session_path_constant() {
        assert_eq!(AUTO_SAVE_SESSION_PATH, ".yoyo/last-session.json");
    }

    #[test]
    fn test_continue_session_path_fallback() {
        // When .yoyo/last-session.json doesn't exist, should fall back to yoyo-session.json
        // (In CI, .yoyo/last-session.json won't exist unless created by a prior test)
        let path = continue_session_path();
        // Should be one of the two valid paths
        assert!(
            path == AUTO_SAVE_SESSION_PATH || path == DEFAULT_SESSION_PATH,
            "continue_session_path should return a valid session path, got: {path}"
        );
    }

    #[test]
    fn test_last_session_exists_returns_bool() {
        // Should not panic regardless of whether the file exists
        let _exists = last_session_exists();
    }

    #[test]
    fn test_auto_save_creates_directory_and_file() {
        use yoagent::agent::Agent;
        use yoagent::provider::AnthropicProvider;

        // Use a temp directory to avoid polluting the project
        let tmp_dir = std::env::temp_dir().join("yoyo_test_autosave");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&tmp_dir).unwrap();

        // Create an agent with an empty conversation — should NOT save
        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        auto_save_on_exit(&agent);
        assert!(
            !std::path::Path::new(AUTO_SAVE_SESSION_PATH).exists(),
            "Should not save empty conversations"
        );

        // Restore directory
        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_continue_session_path_prefers_auto_save() {
        // Create a temp directory with .yoyo/last-session.json
        let tmp_dir = std::env::temp_dir().join("yoyo_test_continue_path");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(tmp_dir.join(".yoyo")).unwrap();
        std::fs::write(tmp_dir.join(".yoyo/last-session.json"), "[]").unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp_dir).unwrap();

        let path = continue_session_path();
        assert_eq!(
            path, AUTO_SAVE_SESSION_PATH,
            "Should prefer .yoyo/last-session.json when it exists"
        );

        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_continue_session_path_falls_back_to_default() {
        // Create a temp directory WITHOUT .yoyo/last-session.json
        let tmp_dir = std::env::temp_dir().join("yoyo_test_continue_fallback");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp_dir).unwrap();

        let path = continue_session_path();
        assert_eq!(
            path, DEFAULT_SESSION_PATH,
            "Should fall back to yoyo-session.json when .yoyo/last-session.json doesn't exist"
        );

        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    // ── /export tests ────────────────────────────────────────────────────

    #[test]
    fn test_format_conversation_as_markdown_empty() {
        let messages: Vec<AgentMessage> = vec![];
        let md = format_conversation_as_markdown(&messages);
        assert_eq!(md, "# Conversation\n\n");
    }

    #[test]
    fn test_format_conversation_as_markdown_user_message() {
        let messages = vec![AgentMessage::Llm(Message::user("Hello, world!"))];
        let md = format_conversation_as_markdown(&messages);
        assert!(md.contains("## User"));
        assert!(md.contains("Hello, world!"));
    }

    #[test]
    fn test_format_conversation_as_markdown_mixed_messages() {
        let messages = vec![
            AgentMessage::Llm(Message::user("What is 2+2?")),
            AgentMessage::Llm(Message::Assistant {
                content: vec![Content::Text {
                    text: "The answer is 4.".to_string(),
                }],
                stop_reason: yoagent::types::StopReason::Stop,
                model: "test".to_string(),
                provider: "test".to_string(),
                usage: Usage::default(),
                timestamp: 0,
                error_message: None,
            }),
            AgentMessage::Llm(Message::ToolResult {
                tool_call_id: "tc_1".to_string(),
                tool_name: "bash".to_string(),
                content: vec![Content::Text {
                    text: "file.txt".to_string(),
                }],
                is_error: false,
                timestamp: 0,
            }),
        ];
        let md = format_conversation_as_markdown(&messages);
        assert!(md.contains("## User"), "Should have user heading");
        assert!(md.contains("What is 2+2?"), "Should have user text");
        assert!(md.contains("## Assistant"), "Should have assistant heading");
        assert!(
            md.contains("The answer is 4."),
            "Should have assistant text"
        );
        assert!(md.contains("### Tool: bash"), "Should have tool heading");
        assert!(md.contains("file.txt"), "Should have tool output");
        assert!(md.contains("```"), "Tool output should be in code block");
    }

    #[test]
    fn test_format_conversation_as_markdown_thinking_block() {
        let messages = vec![AgentMessage::Llm(Message::Assistant {
            content: vec![
                Content::Thinking {
                    thinking: "Let me think about this.".to_string(),
                    signature: None,
                },
                Content::Text {
                    text: "Here's my answer.".to_string(),
                },
            ],
            stop_reason: yoagent::types::StopReason::Stop,
            model: "test".to_string(),
            provider: "test".to_string(),
            usage: Usage::default(),
            timestamp: 0,
            error_message: None,
        })];
        let md = format_conversation_as_markdown(&messages);
        assert!(md.contains("*Thinking:*"), "Should contain thinking label");
        assert!(
            md.contains("Let me think about this."),
            "Should contain thinking text"
        );
        assert!(
            md.contains("Here's my answer."),
            "Should contain response text"
        );
    }

    #[test]
    fn test_format_conversation_as_markdown_skips_tool_calls() {
        let messages = vec![AgentMessage::Llm(Message::Assistant {
            content: vec![
                Content::Text {
                    text: "I'll check that.".to_string(),
                },
                Content::ToolCall {
                    id: "tc_1".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({"command": "ls"}),
                },
            ],
            stop_reason: yoagent::types::StopReason::Stop,
            model: "test".to_string(),
            provider: "test".to_string(),
            usage: Usage::default(),
            timestamp: 0,
            error_message: None,
        })];
        let md = format_conversation_as_markdown(&messages);
        assert!(
            md.contains("I'll check that."),
            "Should include text blocks"
        );
        // Tool calls should not appear as raw JSON in the output
        assert!(
            !md.contains("\"command\""),
            "Should not include tool call arguments"
        );
    }

    #[test]
    fn test_parse_export_path_default() {
        assert_eq!(parse_export_path("/export"), "conversation.md");
    }

    #[test]
    fn test_parse_export_path_custom() {
        assert_eq!(parse_export_path("/export myfile.md"), "myfile.md");
    }

    #[test]
    fn test_parse_export_path_with_directory() {
        assert_eq!(
            parse_export_path("/export output/chat.md"),
            "output/chat.md"
        );
    }

    #[test]
    fn test_parse_export_path_whitespace() {
        assert_eq!(parse_export_path("/export   notes.md  "), "notes.md");
    }

    // ── clear confirmation tests ────────────────────────────────────────

    #[test]
    fn test_clear_confirmation_empty_conversation() {
        assert_eq!(clear_confirmation_message(0, 0), None);
    }

    #[test]
    fn test_clear_confirmation_at_threshold() {
        assert_eq!(clear_confirmation_message(4, 1000), None);
    }

    #[test]
    fn test_clear_confirmation_above_threshold_contains_count() {
        let msg = clear_confirmation_message(10, 5000);
        assert!(msg.is_some(), "should prompt for 10 messages");
        let text = msg.unwrap();
        assert!(
            text.contains("10 messages"),
            "should mention message count: {text}"
        );
    }

    #[test]
    fn test_clear_confirmation_above_threshold_contains_tokens() {
        let msg = clear_confirmation_message(10, 5000);
        assert!(msg.is_some());
        let text = msg.unwrap();
        assert!(
            text.contains("5.0k"),
            "should contain formatted token count: {text}"
        );
    }

    #[test]
    fn test_clear_confirmation_just_above_threshold() {
        let msg = clear_confirmation_message(5, 200);
        assert!(msg.is_some(), "5 messages should trigger confirmation");
        let text = msg.unwrap();
        assert!(text.contains("5 messages"));
        assert!(text.contains("200"));
    }

    #[test]
    fn test_clear_force_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/clear!"),
            "/clear! should be in KNOWN_COMMANDS"
        );
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
        let prompt = spawn_context_prompt(&[], None);
        assert!(prompt.contains("subagent"));
        assert!(!prompt.contains("Project Context"));
        assert!(!prompt.contains("Conversation Context"));
    }

    #[test]
    fn test_spawn_context_prompt_with_project_context() {
        let prompt = spawn_context_prompt(&[], Some("# My Project\nA great tool."));
        assert!(prompt.contains("subagent"));
        assert!(prompt.contains("## Project Context"));
        assert!(prompt.contains("My Project"));
    }

    #[test]
    fn test_spawn_context_prompt_with_messages() {
        let messages = vec![AgentMessage::Llm(Message::user("hello world"))];
        let prompt = spawn_context_prompt(&messages, None);
        assert!(prompt.contains("subagent"));
        assert!(prompt.contains("Conversation Context"));
        assert!(prompt.contains("hello world"));
    }

    #[test]
    fn test_spawn_context_prompt_truncates_large_context() {
        let large_context = "x".repeat(10000);
        let prompt = spawn_context_prompt(&[], Some(&large_context));
        assert!(prompt.contains("(truncated)"));
        // Should contain less than the full 10000 chars
        assert!(prompt.len() < 10000);
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
            AgentMessage::Llm(Message::Assistant {
                content: vec![Content::Text {
                    text: "Rust is a systems programming language.".to_string(),
                }],
                stop_reason: yoagent::types::StopReason::Stop,
                model: "test".to_string(),
                provider: "test".to_string(),
                usage: Usage::default(),
                timestamp: 0,
                error_message: None,
            }),
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
        let result = format_spawn_result("read file", "contents here", 3);
        assert!(result.contains("#3"));
        assert!(result.contains("read file"));
        assert!(result.contains("contents here"));
    }

    #[test]
    fn test_format_spawn_result_empty_output() {
        let result = format_spawn_result("task", "   ", 1);
        assert!(result.contains("(no output)"));
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

    // ── proactive compact tests ──────────────────────────────────────────

    #[test]
    fn test_proactive_compact_threshold_is_lower_than_auto() {
        // Proactive compact (0.70) fires before auto-compact (0.80).
        // This ensures we try to shrink the context BEFORE hitting the API limit,
        // rather than only reacting after an overflow error.
        use crate::cli::{AUTO_COMPACT_THRESHOLD, PROACTIVE_COMPACT_THRESHOLD};
        const {
            assert!(PROACTIVE_COMPACT_THRESHOLD < AUTO_COMPACT_THRESHOLD);
        }
    }

    #[test]
    fn test_proactive_compact_threshold_in_valid_range() {
        use crate::cli::PROACTIVE_COMPACT_THRESHOLD;
        // Should be between 0.5 and 0.8 — not so aggressive it compacts tiny contexts,
        // not so high it's redundant with auto-compact.
        const {
            assert!(PROACTIVE_COMPACT_THRESHOLD > 0.5);
            assert!(PROACTIVE_COMPACT_THRESHOLD < 0.8);
        }
    }

    // ── /stash tests ────────────────────────────────────────────────────────

    #[test]
    fn test_parse_stash_subcommand_push() {
        let (cmd, arg) = parse_stash_subcommand("/stash push WIP");
        assert_eq!(cmd, "push");
        assert_eq!(arg, "WIP");
    }

    #[test]
    fn test_parse_stash_subcommand_pop() {
        let (cmd, arg) = parse_stash_subcommand("/stash pop");
        assert_eq!(cmd, "pop");
        assert_eq!(arg, "");
    }

    #[test]
    fn test_parse_stash_subcommand_list() {
        let (cmd, arg) = parse_stash_subcommand("/stash list");
        assert_eq!(cmd, "list");
        assert_eq!(arg, "");
    }

    #[test]
    fn test_parse_stash_subcommand_drop() {
        let (cmd, arg) = parse_stash_subcommand("/stash drop 2");
        assert_eq!(cmd, "drop");
        assert_eq!(arg, "2");
    }

    #[test]
    fn test_parse_stash_subcommand_default() {
        // Bare `/stash` defaults to push
        let (cmd, arg) = parse_stash_subcommand("/stash");
        assert_eq!(cmd, "push");
        assert_eq!(arg, "");
    }

    #[test]
    fn test_parse_stash_subcommand_implicit_push_with_description() {
        // `/stash some description` is treated as push with description
        let (cmd, arg) = parse_stash_subcommand("/stash some description");
        assert_eq!(cmd, "push");
        assert_eq!(arg, "some description");
    }

    #[test]
    fn test_stash_entry_description_default() {
        // When no description provided, auto-generate stash@{N}
        let desc = stash_default_description(0);
        assert_eq!(desc, "stash@{0}");
        let desc2 = stash_default_description(3);
        assert_eq!(desc2, "stash@{3}");
    }

    #[test]
    fn test_stash_list_empty() {
        // Clear the global stash for this test
        {
            let mut stash = CONVERSATION_STASH.write().unwrap();
            stash.clear();
        }
        let result = handle_stash_list();
        assert!(result.contains("empty"), "Empty stash should say so");
    }

    #[test]
    fn test_stash_drop_empty() {
        {
            let mut stash = CONVERSATION_STASH.write().unwrap();
            stash.clear();
        }
        let result = handle_stash_drop("");
        assert!(
            result.contains("empty"),
            "Drop on empty stash should say so"
        );
    }

    #[test]
    fn test_stash_drop_out_of_range() {
        {
            let mut stash = CONVERSATION_STASH.write().unwrap();
            stash.clear();
        }
        let result = handle_stash_drop("5");
        assert!(
            result.contains("out of range"),
            "Should report out of range"
        );
    }

    #[test]
    fn test_stash_drop_invalid_index() {
        let result = handle_stash_drop("abc");
        assert!(result.contains("invalid"), "Should report invalid index");
    }

    // ── Tests moved from commands.rs — session command tests ──────────

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
        // /jump should match exact or with space
        let jump_matches = |s: &str| s == "/jump" || s.starts_with("/jump ");
        assert!(jump_matches("/jump"));
        assert!(jump_matches("/jump checkpoint"));
        assert!(!jump_matches("/jumping"));
        assert!(!jump_matches("/jumped"));
    }
}
