//! Conversation stash subsystem: `/stash push`, `/stash pop`, `/stash list`,
//! `/stash drop`. Like `git stash` but for your conversation.

use crate::format::*;

use std::sync::RwLock;
use yoagent::agent::Agent;

/// Acquire a read-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_read_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

/// Acquire a write-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_write_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::agent::Agent;

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
}
