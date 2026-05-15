//! Spawn subsystem: /spawn command, task tracking, subagent context building.
//!
//! Extracted from `commands_session.rs` — the spawn feature is self-contained
//! with its own types (SpawnStatus, SpawnTask, SpawnTracker, SpawnArgs),
//! parser, context builder, and handler.

use crate::format::*;
use crate::prompt::run_prompt;
use crate::prompt_utils::summarize_message;
use crate::sync_util::lock_or_recover;

use std::sync::{Arc, Mutex};
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
        });
        id
    }

    /// Mark a task as completed with its result.
    pub fn complete(&self, id: usize, result: String) {
        let mut tasks = lock_or_recover(&self.inner);
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.status = SpawnStatus::Completed;
            task.result = Some(result);
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
    /// Returns `Ok(Some(result))` if finished, `Ok(None)` if still running.
    /// Returns `Err` if the spawn doesn't exist or wasn't a background spawn.
    pub fn try_collect(&self, id: usize) -> Result<Option<(String, String)>, String> {
        let tasks = lock_or_recover(&self.inner);
        let task = tasks.iter().find(|t| t.id == id);
        match task {
            None => Err(format!("no spawn #{id} found")),
            Some(t) if !t.background => {
                if t.status == SpawnStatus::Completed {
                    Ok(t.result.clone().map(|r| (t.task.clone(), r)))
                } else {
                    Err(format!("spawn #{id} was not a background spawn"))
                }
            }
            Some(t) if t.status == SpawnStatus::Completed => {
                Ok(t.result.clone().map(|r| (t.task.clone(), r)))
            }
            Some(t) if matches!(t.status, SpawnStatus::Failed(_)) => {
                Err(format!("spawn #{id} {}", t.status))
            }
            _ => Ok(None), // Still running
        }
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
}

/// Parse the `/spawn` command input, extracting flags and task.
///
/// Supports:
/// - `/spawn <task>` — run a task synchronously
/// - `/spawn --bg <task>` — run a task in the background
/// - `/spawn -o <path> <task>` — run a task and capture output to a file
/// - `/spawn --bg -o <path> <task>` — background with output capture
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
            });
        }
        // "collect" without valid id — fall through to show usage
        return None;
    }

    let mut words: Vec<&str> = rest.split_whitespace().collect();
    let mut background = false;
    let mut output_path = None;

    // Extract flags from the front
    while !words.is_empty() {
        if words[0] == "--bg" {
            background = true;
            words.remove(0);
        } else if words[0] == "-o" && words.len() > 1 {
            output_path = Some(words[1].to_string());
            words.remove(0); // remove "-o"
            words.remove(0); // remove the path (now at position 0)
        } else {
            break; // stop processing flags once we hit a non-flag word
        }
    }

    let task = words.join(" ");
    if task.is_empty() {
        return None;
    }

    Some(SpawnArgs {
        task,
        output_path,
        background,
        collect_id: None,
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
            SpawnStatus::Running => {
                let bg_label = if task.background { " (background)" } else { "" };
                println!(
                    "    {CYAN}{status_icon} #{id}: {task_preview}{bg_label}{output_note}{RESET}",
                    id = task.id
                );
            }
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
/// and return the result. Supports output capture, background execution, and task tracking.
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
            println!("         /spawn --bg <task>         (run in background)");
            println!("         /spawn -o <file> <task>    (capture output to file)");
            println!("         /spawn collect <id>        (collect background result)");
            println!("         /spawn status              (show tracked spawns)");
            println!("  Spawn a subagent with project context to handle a task.");
            println!("  The result is summarized back into your main conversation.");
            println!("  Example: /spawn read src/main.rs and summarize the architecture{RESET}\n");
            return None;
        }
    };

    // Handle /spawn collect <id>
    if let Some(id) = args.collect_id {
        return handle_spawn_collect(tracker, id);
    }

    // Handle --bg: launch in background
    if args.background {
        return handle_spawn_bg(&args, agent_config, model, main_messages, tracker);
    }

    // Synchronous spawn (existing behavior)
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

    println!("{CYAN}  🐙 spawning subagent #{spawn_id} in background...{RESET}");
    println!(
        "{DIM}  task: {}{RESET}",
        crate::format::truncate_with_ellipsis(&args.task, 100)
    );
    println!("{DIM}  use /spawn status to check progress, /spawn collect {spawn_id} to get results{RESET}\n");

    // Prepare everything the background task needs (clone before moving)
    let project_context = crate::cli::load_project_context();
    let context_prompt = spawn_context_prompt(main_messages, project_context.as_deref());
    let sub_config = crate::AgentConfig {
        system_prompt: context_prompt,
        ..clone_agent_config(agent_config)
    };
    let task_text = args.task.clone();
    let output_path = args.output_path.clone();
    let model = model.to_string();
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
                return;
            }
        }

        // Mark completed in tracker
        tracker_clone.complete(spawn_id, response);
    });

    tracker.store_handle(spawn_id, handle);
    None
}

/// Collect a finished background spawn's result.
/// Returns Some(context_msg) if the spawn is done, None otherwise.
fn handle_spawn_collect(tracker: &SpawnTracker, id: usize) -> Option<String> {
    match tracker.try_collect(id) {
        Ok(Some((task, result))) => {
            println!("{GREEN}  ✓ subagent #{id} completed{RESET}");
            println!("{DIM}  injecting result into main conversation...{RESET}\n");
            Some(format_spawn_result(&task, &result, id))
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
        disallowed_tools: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
    use yoagent::types::{Content, Message, Usage};

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
        let (task, result) = collected.unwrap();
        assert_eq!(task, "bg task");
        assert_eq!(result, "bg result");
    }
}
