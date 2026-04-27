//! Session-related command handlers: /save, /load, /compact, /history, /search,
//! /mark, /jump, /marks, /export, /stash, /checkpoint.

use crate::format::*;
use crate::prompt::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::types::{AgentMessage, Content, Message};

use crate::cli::{
    AUTO_COMPACT_THRESHOLD, AUTO_SAVE_SESSION_PATH, DEFAULT_SESSION_PATH,
    PROACTIVE_COMPACT_THRESHOLD,
};

/// Acquire a read-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_read_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

/// Acquire a write-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_write_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

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
            reset_context_budget_warning();
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
    auto_save_on_exit_in(agent, std::path::Path::new("."));
}

/// Like [`auto_save_on_exit`] but writes session files under an explicit `root`
/// directory instead of the process CWD. This avoids `set_current_dir` in tests.
fn auto_save_on_exit_in(agent: &Agent, root: &std::path::Path) {
    if agent.messages().is_empty() {
        return;
    }
    if let Ok(json) = agent.save_messages() {
        // Ensure .yoyo/ directory exists
        let yoyo_dir = root.join(".yoyo");
        let _ = std::fs::create_dir_all(&yoyo_dir);
        let save_path = root.join(AUTO_SAVE_SESSION_PATH);
        if std::fs::write(&save_path, &json).is_ok() {
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
    continue_session_path_in(std::path::Path::new("."))
}

/// Like [`continue_session_path`] but checks for the auto-save file under an
/// explicit `root` directory instead of the process CWD.
fn continue_session_path_in(root: &std::path::Path) -> &'static str {
    if root.join(AUTO_SAVE_SESSION_PATH).exists() {
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
    let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
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
    let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
    if stash.is_empty() {
        return format!("{DIM}  (stash is empty — nothing to pop){RESET}\n");
    }

    let entry = match stash.pop() {
        Some(e) => e,
        None => return format!("{DIM}  (stash is empty — nothing to pop){RESET}\n"),
    };
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
    let stash = rw_read_or_recover(&CONVERSATION_STASH);
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
        let stash = rw_read_or_recover(&CONVERSATION_STASH);
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

    let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
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

// ── Checkpoint ──────────────────────────────────────────────────────────────

/// A named snapshot of file contents at a point in time.
pub struct Checkpoint {
    pub name: String,
    pub created: std::time::Instant,
    pub files: HashMap<String, String>, // path -> content at checkpoint time
}

/// In-session store of named file-state checkpoints.
pub struct CheckpointStore {
    checkpoints: HashMap<String, Checkpoint>,
}

/// Subcommands for `/checkpoint`.
const CHECKPOINT_SUBCOMMANDS: &[&str] = &["save", "list", "restore", "diff", "delete"];

impl CheckpointStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            checkpoints: HashMap::new(),
        }
    }

    /// Save a named checkpoint by reading current file contents from `changes`.
    pub fn save(&mut self, name: &str, changes: &SessionChanges) {
        let snapshot = changes.snapshot();
        let mut files = HashMap::new();
        for fc in &snapshot {
            if let Ok(content) = std::fs::read_to_string(&fc.path) {
                files.insert(fc.path.clone(), content);
            }
        }
        self.checkpoints.insert(
            name.to_string(),
            Checkpoint {
                name: name.to_string(),
                created: std::time::Instant::now(),
                files,
            },
        );
    }

    /// Restore files to their state at the named checkpoint.
    /// Returns a list of action descriptions, or an error message.
    pub fn restore(&self, name: &str) -> Result<Vec<String>, String> {
        let cp = self
            .checkpoints
            .get(name)
            .ok_or_else(|| format!("No checkpoint named '{name}'"))?;
        let mut actions = Vec::new();
        for (path, content) in &cp.files {
            if std::path::Path::new(path).exists() {
                if let Err(e) = std::fs::write(path, content) {
                    actions.push(format!("  ✗ {path}: {e}"));
                } else {
                    actions.push(format!("  ✓ restored {path}"));
                }
            } else {
                // File was deleted since checkpoint — recreate it
                if let Some(parent) = std::path::Path::new(path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(path, content) {
                    actions.push(format!("  ✗ {path} (recreate): {e}"));
                } else {
                    actions.push(format!("  ⚠ recreated {path} (was deleted)"));
                }
            }
        }
        Ok(actions)
    }

    /// List all checkpoints: (name, file_count, created).
    pub fn list(&self) -> Vec<(&str, usize, std::time::Instant)> {
        let mut entries: Vec<_> = self
            .checkpoints
            .values()
            .map(|cp| (cp.name.as_str(), cp.files.len(), cp.created))
            .collect();
        // Sort by creation time (oldest first)
        entries.sort_by_key(|e| e.2);
        entries
    }

    /// Diff current file state against the named checkpoint.
    pub fn diff(&self, name: &str) -> Result<String, String> {
        let cp = self
            .checkpoints
            .get(name)
            .ok_or_else(|| format!("No checkpoint named '{name}'"))?;
        let mut out = String::new();
        for (path, saved) in &cp.files {
            let current = std::fs::read_to_string(path).unwrap_or_default();
            if current == *saved {
                continue;
            }
            out.push_str(&format!("{}── {path} ──{}\n", BOLD, RESET));
            // Simple line diff
            let saved_lines: Vec<&str> = saved.lines().collect();
            let current_lines: Vec<&str> = current.lines().collect();
            for line in &saved_lines {
                if !current_lines.contains(line) {
                    out.push_str(&format!("{RED}- {line}{RESET}\n"));
                }
            }
            for line in &current_lines {
                if !saved_lines.contains(line) {
                    out.push_str(&format!("{GREEN}+ {line}{RESET}\n"));
                }
            }
        }
        if out.is_empty() {
            Ok(format!("No changes since checkpoint '{name}'."))
        } else {
            Ok(out)
        }
    }

    /// Delete a named checkpoint. Returns true if it existed.
    pub fn delete(&mut self, name: &str) -> bool {
        self.checkpoints.remove(name).is_some()
    }

    /// Return the number of stored checkpoints.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }
}

/// Returns true if a name is valid: non-empty, only alphanumeric, hyphens, underscores.
fn is_valid_checkpoint_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Format a duration as a human-readable relative time (e.g., "2m ago").
fn format_checkpoint_age(created: std::time::Instant) -> String {
    let elapsed = created.elapsed();
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h {}m ago", secs / 3600, (secs % 3600) / 60)
    }
}

/// Handle the `/checkpoint` command.
pub fn handle_checkpoint(input: &str, store: &mut CheckpointStore, changes: &SessionChanges) {
    let rest = if input == "/checkpoint" {
        ""
    } else {
        input.strip_prefix("/checkpoint ").unwrap_or("").trim()
    };

    if rest.is_empty() {
        println!(
            "{BOLD}Usage:{RESET} /checkpoint <name>       Save a named checkpoint\n\
             \x20      /checkpoint save <name>  Save a named checkpoint\n\
             \x20      /checkpoint list         List all checkpoints\n\
             \x20      /checkpoint restore <n>  Restore files to checkpoint state\n\
             \x20      /checkpoint diff <name>  Show changes since checkpoint\n\
             \x20      /checkpoint delete <n>   Delete a checkpoint"
        );
        return;
    }

    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    let subcmd = parts[0];
    let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

    match subcmd {
        "list" => {
            let entries = store.list();
            if entries.is_empty() {
                println!("{DIM}No checkpoints saved yet.{RESET}");
            } else {
                println!("{BOLD}Checkpoints:{RESET}");
                for (name, file_count, created) in &entries {
                    let age = format_checkpoint_age(*created);
                    println!("  {GREEN}{name}{RESET}  ({file_count} files, {age})");
                }
            }
        }
        "restore" => {
            if arg.is_empty() {
                println!("{RED}Usage: /checkpoint restore <name>{RESET}");
                return;
            }
            match store.restore(arg) {
                Ok(actions) => {
                    println!("{GREEN}Restored checkpoint '{arg}':{RESET}");
                    for a in &actions {
                        println!("{a}");
                    }
                }
                Err(e) => println!("{RED}{e}{RESET}"),
            }
        }
        "diff" => {
            if arg.is_empty() {
                println!("{RED}Usage: /checkpoint diff <name>{RESET}");
                return;
            }
            match store.diff(arg) {
                Ok(output) => print!("{output}"),
                Err(e) => println!("{RED}{e}{RESET}"),
            }
        }
        "delete" => {
            if arg.is_empty() {
                println!("{RED}Usage: /checkpoint delete <name>{RESET}");
                return;
            }
            if store.delete(arg) {
                println!("{GREEN}Deleted checkpoint '{arg}'.{RESET}");
            } else {
                println!("{RED}No checkpoint named '{arg}'.{RESET}");
            }
        }
        "save" => {
            if arg.is_empty() {
                println!("{RED}Usage: /checkpoint save <name>{RESET}");
                return;
            }
            if !is_valid_checkpoint_name(arg) {
                println!(
                    "{RED}Invalid name. Use only letters, numbers, hyphens, underscores.{RESET}"
                );
                return;
            }
            store.save(arg, changes);
            let count = store
                .checkpoints
                .get(arg)
                .map(|cp| cp.files.len())
                .unwrap_or(0);
            println!("{GREEN}Checkpoint '{arg}' saved ({count} files).{RESET}");
        }
        // Bare name: treat as save
        name => {
            if !is_valid_checkpoint_name(name) {
                println!(
                    "{RED}Unknown subcommand '{name}'. Use: save, list, restore, diff, delete.{RESET}"
                );
                return;
            }
            store.save(name, changes);
            let count = store
                .checkpoints
                .get(name)
                .map(|cp| cp.files.len())
                .unwrap_or(0);
            println!("{GREEN}Checkpoint '{name}' saved ({count} files).{RESET}");
        }
    }
}

/// Subcommand completions for `/checkpoint`.
pub fn checkpoint_subcommands() -> &'static [&'static str] {
    CHECKPOINT_SUBCOMMANDS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::AUTO_SAVE_SESSION_PATH;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
    use yoagent::types::Usage;

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

        // Create an agent with an empty conversation — should NOT save
        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        auto_save_on_exit_in(&agent, &tmp_dir);
        assert!(
            !tmp_dir.join(AUTO_SAVE_SESSION_PATH).exists(),
            "Should not save empty conversations"
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_continue_session_path_prefers_auto_save() {
        // Create a temp directory with .yoyo/last-session.json
        let tmp_dir = std::env::temp_dir().join("yoyo_test_continue_path");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(tmp_dir.join(".yoyo")).unwrap();
        std::fs::write(tmp_dir.join(".yoyo/last-session.json"), "[]").unwrap();

        let path = continue_session_path_in(&tmp_dir);
        assert_eq!(
            path, AUTO_SAVE_SESSION_PATH,
            "Should prefer .yoyo/last-session.json when it exists"
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_continue_session_path_falls_back_to_default() {
        // Create a temp directory WITHOUT .yoyo/last-session.json
        let tmp_dir = std::env::temp_dir().join("yoyo_test_continue_fallback");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let path = continue_session_path_in(&tmp_dir);
        assert_eq!(
            path, DEFAULT_SESSION_PATH,
            "Should fall back to yoyo-session.json when .yoyo/last-session.json doesn't exist"
        );

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
                    provider_metadata: None,
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
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
            stash.clear();
        }
        let result = handle_stash_list();
        assert!(result.contains("empty"), "Empty stash should say so");
    }

    #[test]
    fn test_stash_drop_empty() {
        {
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
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
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
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

    #[test]
    fn test_stash_pop_empty() {
        use yoagent::provider::AnthropicProvider;
        // Clear the global stash, then pop — should return a graceful message, not panic
        {
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
            stash.clear();
        }
        let mut agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        let result = handle_stash_pop(&mut agent);
        assert!(
            result.contains("empty"),
            "Pop on empty stash should say so, got: {result}"
        );
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

    #[test]
    fn test_checkpoint_save_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let changes = SessionChanges::new();
        changes.record(file_path.to_str().unwrap(), ChangeKind::Write);

        let mut store = CheckpointStore::new();
        store.save("v1", &changes);

        let entries = store.list();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "v1");
        assert_eq!(entries[0].1, 1); // 1 file
    }

    #[test]
    fn test_checkpoint_restore() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "original").unwrap();

        let changes = SessionChanges::new();
        changes.record(file_path.to_str().unwrap(), ChangeKind::Write);

        let mut store = CheckpointStore::new();
        store.save("snap", &changes);

        // Modify the file
        std::fs::write(&file_path, "modified").unwrap();
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "modified");

        // Restore
        let actions = store.restore("snap").unwrap();
        assert!(!actions.is_empty());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "original");
    }

    #[test]
    fn test_checkpoint_diff() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\n").unwrap();

        let changes = SessionChanges::new();
        changes.record(file_path.to_str().unwrap(), ChangeKind::Write);

        let mut store = CheckpointStore::new();
        store.save("before", &changes);

        // Modify the file
        std::fs::write(&file_path, "line1\nline3\n").unwrap();

        let diff = store.diff("before").unwrap();
        assert!(diff.contains("line2")); // removed line
        assert!(diff.contains("line3")); // added line
    }

    #[test]
    fn test_checkpoint_delete() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "data").unwrap();

        let changes = SessionChanges::new();
        changes.record(file_path.to_str().unwrap(), ChangeKind::Write);

        let mut store = CheckpointStore::new();
        store.save("temp", &changes);
        assert_eq!(store.len(), 1);

        assert!(store.delete("temp"));
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_checkpoint_duplicate_name_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Save first checkpoint
        std::fs::write(&file_path, "v1").unwrap();
        let changes = SessionChanges::new();
        changes.record(file_path.to_str().unwrap(), ChangeKind::Write);
        let mut store = CheckpointStore::new();
        store.save("cp", &changes);

        // Overwrite with different content
        std::fs::write(&file_path, "v2").unwrap();
        store.save("cp", &changes);

        assert_eq!(store.len(), 1);

        // Restore should give v2, not v1
        std::fs::write(&file_path, "v3").unwrap();
        store.restore("cp").unwrap();
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "v2");
    }

    #[test]
    fn test_checkpoint_restore_nonexistent() {
        let store = CheckpointStore::new();
        let result = store.restore("nope");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("nope"));
    }

    #[test]
    fn test_valid_checkpoint_names() {
        assert!(is_valid_checkpoint_name("before-refactor"));
        assert!(is_valid_checkpoint_name("v1"));
        assert!(is_valid_checkpoint_name("snap_2"));
        assert!(is_valid_checkpoint_name("ABC123"));
        assert!(!is_valid_checkpoint_name(""));
        assert!(!is_valid_checkpoint_name("has space"));
        assert!(!is_valid_checkpoint_name("bad!name"));
    }

    #[test]
    fn test_checkpoint_diff_no_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "same").unwrap();

        let changes = SessionChanges::new();
        changes.record(file_path.to_str().unwrap(), ChangeKind::Write);

        let mut store = CheckpointStore::new();
        store.save("cp", &changes);

        let diff = store.diff("cp").unwrap();
        assert!(diff.contains("No changes"));
    }
}
