//! Session-related command handlers: /save, /load, /compact, /history, /search,
//! /mark, /jump, /marks, /export.

use crate::format::*;
use crate::prompt_utils::{search_messages, summarize_message};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::types::{AgentMessage, Content, Message};

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

/// Count tool calls by name from an assistant message's content blocks.
fn count_tool_calls(content: &[Content]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for block in content {
        if let Content::ToolCall { name, .. } = block {
            *counts.entry(name.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// Format tool call counts as a compact summary like "bash ×2, read_file ×1".
fn format_tool_summary(counts: &HashMap<String, usize>) -> String {
    if counts.is_empty() {
        return "no tool calls".to_string();
    }
    let mut entries: Vec<_> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    entries
        .iter()
        .map(|(name, count)| format!("{name} ×{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extract the text preview from a user message's content blocks.
fn extract_user_text(content: &[Content]) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Show per-turn breakdown with tools used and token counts.
pub fn handle_history_detail(agent: &Agent) {
    let messages = agent.messages();
    if messages.is_empty() {
        println!("{DIM}  (no messages in conversation){RESET}\n");
        return;
    }

    // Group into turns: a turn = user message + following assistant/tool messages
    // until the next user message (or end).
    let mut turns: Vec<(Option<&[Content]>, Vec<&Message>)> = Vec::new();

    for msg in messages {
        match msg {
            AgentMessage::Llm(m @ Message::User { content, .. }) => {
                // Start a new turn
                turns.push((Some(content), vec![m]));
            }
            AgentMessage::Llm(m) => {
                // Assistant or ToolResult — append to current turn (or create orphan turn)
                if let Some(turn) = turns.last_mut() {
                    turn.1.push(m);
                } else {
                    turns.push((None, vec![m]));
                }
            }
            AgentMessage::Extension(_) => {}
        }
    }

    println!();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    for (turn_idx, (user_content, msgs)) in turns.iter().enumerate() {
        let turn_num = turn_idx + 1;

        // Extract user text preview
        let user_preview = if let Some(content) = user_content {
            let text = extract_user_text(content);
            if text.is_empty() {
                "(no text)".to_string()
            } else {
                format!("\"{}\"", truncate_with_ellipsis(&text, 60))
            }
        } else {
            "(system)".to_string()
        };

        // Gather assistant stats: tool calls, tokens
        let mut all_tool_counts: HashMap<String, usize> = HashMap::new();
        let mut turn_input: u64 = 0;
        let mut turn_output: u64 = 0;
        let mut has_assistant = false;

        for m in msgs {
            if let Message::Assistant { content, usage, .. } = m {
                has_assistant = true;
                turn_input += usage.input + usage.cache_read + usage.cache_write;
                turn_output += usage.output;
                for (name, count) in count_tool_calls(content) {
                    *all_tool_counts.entry(name).or_insert(0) += count;
                }
            }
        }

        total_input_tokens += turn_input;
        total_output_tokens += turn_output;

        println!("  {BOLD}Turn {turn_num}{RESET}");
        println!("    {GREEN}You:{RESET}   {user_preview}");
        if has_assistant {
            let tool_total: usize = all_tool_counts.values().sum();
            let tool_summary = format_tool_summary(&all_tool_counts);
            println!(
                "    {CYAN}Agent:{RESET} {tool_total} tool call{}, {}, {} tok in / {} tok out",
                if tool_total == 1 { "" } else { "s" },
                tool_summary,
                format_token_count(turn_input),
                format_token_count(turn_output),
            );
        } else {
            println!("    {DIM}(no assistant response){RESET}");
        }
        println!();
    }

    let total = total_input_tokens + total_output_tokens;
    println!(
        "  {BOLD}Total:{RESET} {} turn{}, ~{} tokens ({} in + {} out)",
        turns.len(),
        if turns.len() == 1 { "" } else { "s" },
        format_token_count(total),
        format_token_count(total_input_tokens),
        format_token_count(total_output_tokens),
    );
    println!();
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
    fn test_count_tool_calls_empty() {
        let content: Vec<Content> = vec![Content::Text {
            text: "hello".to_string(),
        }];
        let counts = count_tool_calls(&content);
        assert!(counts.is_empty());
    }

    #[test]
    fn test_count_tool_calls_multiple() {
        let content = vec![
            Content::ToolCall {
                id: "1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::Value::Null,
                provider_metadata: None,
            },
            Content::Text {
                text: "thinking...".to_string(),
            },
            Content::ToolCall {
                id: "2".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::Value::Null,
                provider_metadata: None,
            },
            Content::ToolCall {
                id: "3".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::Value::Null,
                provider_metadata: None,
            },
        ];
        let counts = count_tool_calls(&content);
        assert_eq!(counts.get("bash"), Some(&2));
        assert_eq!(counts.get("read_file"), Some(&1));
        assert_eq!(counts.len(), 2);
    }

    #[test]
    fn test_format_tool_summary_empty() {
        let counts = HashMap::new();
        assert_eq!(format_tool_summary(&counts), "no tool calls");
    }

    #[test]
    fn test_format_tool_summary_sorted() {
        let mut counts = HashMap::new();
        counts.insert("read_file".to_string(), 1);
        counts.insert("bash".to_string(), 3);
        counts.insert("edit_file".to_string(), 2);
        let summary = format_tool_summary(&counts);
        // Should be sorted by count descending, then name ascending
        assert_eq!(summary, "bash ×3, edit_file ×2, read_file ×1");
    }

    #[test]
    fn test_extract_user_text() {
        let content = vec![
            Content::Text {
                text: "hello".to_string(),
            },
            Content::Text {
                text: "world".to_string(),
            },
        ];
        assert_eq!(extract_user_text(&content), "hello world");
    }

    #[test]
    fn test_extract_user_text_empty() {
        let content: Vec<Content> = vec![];
        assert_eq!(extract_user_text(&content), "");
    }

    #[test]
    fn test_handle_history_detail_empty() {
        use yoagent::agent::Agent;
        use yoagent::provider::AnthropicProvider;

        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        // Should not panic with no messages
        handle_history_detail(&agent);
    }
}
