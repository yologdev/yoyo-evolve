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

/// Prefix used for auto-checkpoint stash entries.
const AUTO_CHECKPOINT_PREFIX: &str = "auto-checkpoint";

/// Maximum number of auto-checkpoint entries to keep.
const MAX_AUTO_CHECKPOINTS: usize = 10;

/// Automatically checkpoint the conversation every N turns.
///
/// Only triggers when `turn_count` is a positive multiple of 5.
/// Saves the current conversation as a stash entry **without clearing** it
/// (unlike `/stash push`, which clears). Prunes old auto-checkpoints to stay
/// within [`MAX_AUTO_CHECKPOINTS`].
///
/// Returns `true` if a checkpoint was actually saved.
pub fn auto_checkpoint(agent: &mut Agent, turn_count: usize) -> bool {
    if !turn_count.is_multiple_of(5) || turn_count == 0 {
        return false;
    }

    // Serialize the conversation — silently skip on failure.
    let messages_json = match agent.save_messages() {
        Ok(json) => json,
        Err(_) => return false,
    };

    let description = format!("{AUTO_CHECKPOINT_PREFIX} at turn {turn_count}");
    let timestamp = {
        use std::time::SystemTime;
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let h = (secs % 86400) / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    };

    let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
    stash.push(StashEntry {
        description,
        messages_json,
        timestamp,
    });
    drop(stash);

    // Prune excess auto-checkpoints.
    prune_auto_checkpoints();

    true
}

/// Remove the oldest auto-checkpoint entries so at most [`MAX_AUTO_CHECKPOINTS`] remain.
fn prune_auto_checkpoints() {
    let mut stash = rw_write_or_recover(&CONVERSATION_STASH);

    // Count auto-checkpoint entries.
    let auto_count = stash
        .iter()
        .filter(|e| e.description.starts_with(AUTO_CHECKPOINT_PREFIX))
        .count();

    if auto_count <= MAX_AUTO_CHECKPOINTS {
        return;
    }

    let excess = auto_count - MAX_AUTO_CHECKPOINTS;
    let mut removed = 0;
    stash.retain(|e| {
        if removed < excess && e.description.starts_with(AUTO_CHECKPOINT_PREFIX) {
            removed += 1;
            false // drop oldest (front of vec = oldest)
        } else {
            true
        }
    });
}

/// Description label used for the automatic pre-`/clear` snapshot.
pub const PRE_CLEAR_LABEL: &str = "pre-clear";

/// Push a pre-clear entry onto `entries`, replacing any existing one.
///
/// At most ONE pre-clear snapshot is kept, so repeated `/clear` doesn't grow
/// the stash. Ordinary entries are preserved untouched. Pure function so it
/// can be tested against a local `Vec` without touching the global stash.
fn push_pre_clear_entry(entries: &mut Vec<StashEntry>, entry: StashEntry) {
    entries.retain(|e| e.description != PRE_CLEAR_LABEL);
    entries.push(entry);
}

/// Automatically stash the conversation right before `/clear` wipes it.
///
/// Returns `true` if a snapshot was saved (i.e. the conversation was
/// non-empty and serialization succeeded), `false` for a no-op. Unlike
/// `/stash push`, this does NOT clear the agent's messages — the caller
/// (`/clear`) rebuilds the agent itself.
pub fn stash_pre_clear(agent: &Agent) -> bool {
    if agent.messages().is_empty() {
        return false;
    }

    let messages_json = match agent.save_messages() {
        Ok(json) => json,
        Err(_) => return false,
    };

    let timestamp = {
        use std::time::SystemTime;
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let h = (secs % 86400) / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    };

    let entry = StashEntry {
        description: PRE_CLEAR_LABEL.to_string(),
        messages_json,
        timestamp,
    };

    let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
    push_pre_clear_entry(&mut stash, entry);
    true
}

/// Remove and return the most recent pre-clear entry from `entries`.
///
/// Pure function over a `&mut Vec` so it can be unit-tested with a local
/// vector without touching the shared global stash.
fn take_pre_clear_entry(entries: &mut Vec<StashEntry>) -> Option<StashEntry> {
    let idx = entries
        .iter()
        .rposition(|e| e.description == PRE_CLEAR_LABEL)?;
    Some(entries.remove(idx))
}

/// `/rewind` — restore the conversation auto-stashed by the last `/clear`.
///
/// Consuming semantics: on a successful restore the pre-clear snapshot is
/// REMOVED from the stash, so `/rewind` works at most once per `/clear`.
/// If the restore itself fails, the snapshot is put back so it isn't lost.
pub fn handle_rewind(agent: &mut Agent) -> String {
    let entry = {
        let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
        take_pre_clear_entry(&mut stash)
    };

    let Some(entry) = entry else {
        return format!("{DIM}  nothing to rewind — /clear hasn't been used this session{RESET}\n");
    };

    match agent.restore_messages(&entry.messages_json) {
        Ok(_) => format!(
            "{GREEN}  ✓ rewound to pre-clear snapshot ({} messages restored){RESET}\n",
            agent.messages().len()
        ),
        Err(e) => {
            // Restore failed — put the snapshot back so it isn't consumed.
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
            push_pre_clear_entry(&mut stash, entry);
            format!("{RED}  failed to rewind: {e}{RESET}\n")
        }
    }
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
        let mut agent =
            Agent::from_provider(AnthropicProvider, yoagent::provider::ModelConfig::mock())
                .with_system_prompt("test")
                .with_api_key("test-key");
        let result = handle_stash_pop(&mut agent);
        assert!(
            result.contains("empty"),
            "Pop on empty stash should say so, got: {result}"
        );
    }

    #[test]
    fn test_auto_checkpoint_triggers_at_5() {
        use yoagent::provider::AnthropicProvider;
        {
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
            stash.clear();
        }
        let mut agent =
            Agent::from_provider(AnthropicProvider, yoagent::provider::ModelConfig::mock())
                .with_system_prompt("test")
                .with_api_key("test-key");

        // Turn 5 should trigger a checkpoint
        let saved = auto_checkpoint(&mut agent, 5);
        assert!(saved, "Should save checkpoint at turn 5");

        let stash = rw_read_or_recover(&CONVERSATION_STASH);
        assert_eq!(stash.len(), 1);
        assert!(
            stash[0].description.starts_with(AUTO_CHECKPOINT_PREFIX),
            "Description should start with auto-checkpoint prefix"
        );
        assert!(
            stash[0].description.contains("turn 5"),
            "Description should mention turn number"
        );
    }

    #[test]
    fn test_auto_checkpoint_skips_non_multiple() {
        use yoagent::provider::AnthropicProvider;
        {
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
            stash.clear();
        }
        let mut agent =
            Agent::from_provider(AnthropicProvider, yoagent::provider::ModelConfig::mock())
                .with_system_prompt("test")
                .with_api_key("test-key");

        // Turns 1-4 should not trigger
        for t in 1..5 {
            let saved = auto_checkpoint(&mut agent, t);
            assert!(!saved, "Should not save checkpoint at turn {t}");
        }

        // Turn 0 should not trigger either
        let saved = auto_checkpoint(&mut agent, 0);
        assert!(!saved, "Should not save checkpoint at turn 0");

        let stash = rw_read_or_recover(&CONVERSATION_STASH);
        assert!(
            stash.is_empty(),
            "Stash should be empty after non-multiples"
        );
    }

    #[test]
    fn test_push_pre_clear_entry_replaces_existing() {
        let mut entries = vec![
            StashEntry {
                description: "ordinary".to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:01".to_string(),
            },
            StashEntry {
                description: PRE_CLEAR_LABEL.to_string(),
                messages_json: "[\"old\"]".to_string(),
                timestamp: "00:00:02".to_string(),
            },
        ];

        push_pre_clear_entry(
            &mut entries,
            StashEntry {
                description: PRE_CLEAR_LABEL.to_string(),
                messages_json: "[\"new\"]".to_string(),
                timestamp: "00:00:03".to_string(),
            },
        );

        // Len stays constant: old pre-clear replaced, ordinary preserved.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].description, "ordinary");
        assert_eq!(entries[1].description, PRE_CLEAR_LABEL);
        assert_eq!(entries[1].messages_json, "[\"new\"]");
    }

    #[test]
    fn test_push_pre_clear_entry_appends_when_no_existing() {
        let mut entries = vec![
            StashEntry {
                description: "one".to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:01".to_string(),
            },
            StashEntry {
                description: "two".to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:02".to_string(),
            },
        ];

        push_pre_clear_entry(
            &mut entries,
            StashEntry {
                description: PRE_CLEAR_LABEL.to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:03".to_string(),
            },
        );

        // Appends: len grows by 1, ordinary entries preserved.
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].description, "one");
        assert_eq!(entries[1].description, "two");
        assert_eq!(entries[2].description, PRE_CLEAR_LABEL);
    }

    #[test]
    fn test_stash_pre_clear_noop_on_empty_conversation() {
        use yoagent::provider::AnthropicProvider;
        let agent = Agent::from_provider(AnthropicProvider, yoagent::provider::ModelConfig::mock())
            .with_system_prompt("test")
            .with_api_key("test-key");

        // No messages → no-op, and the global stash is never touched.
        assert!(!stash_pre_clear(&agent));
    }

    #[test]
    fn test_take_pre_clear_entry_empty() {
        let mut entries: Vec<StashEntry> = Vec::new();
        assert!(take_pre_clear_entry(&mut entries).is_none());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_take_pre_clear_entry_none_present() {
        let mut entries = vec![
            StashEntry {
                description: "WIP".to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:00".to_string(),
            },
            StashEntry {
                description: "other".to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:01".to_string(),
            },
        ];
        assert!(take_pre_clear_entry(&mut entries).is_none());
        // Untouched: both ordinary entries survive.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].description, "WIP");
        assert_eq!(entries[1].description, "other");
    }

    #[test]
    fn test_take_pre_clear_entry_removes_and_returns() {
        let mut entries = vec![
            StashEntry {
                description: "WIP".to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:00".to_string(),
            },
            StashEntry {
                description: PRE_CLEAR_LABEL.to_string(),
                messages_json: "[{\"snapshot\":true}]".to_string(),
                timestamp: "00:00:01".to_string(),
            },
            StashEntry {
                description: "later".to_string(),
                messages_json: "[]".to_string(),
                timestamp: "00:00:02".to_string(),
            },
        ];
        let taken = take_pre_clear_entry(&mut entries).expect("should find pre-clear entry");
        assert_eq!(taken.description, PRE_CLEAR_LABEL);
        assert_eq!(taken.messages_json, "[{\"snapshot\":true}]");
        // Consuming semantics: entry removed, neighbors preserved in order.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].description, "WIP");
        assert_eq!(entries[1].description, "later");
        // Second take finds nothing — one rewind per clear.
        assert!(take_pre_clear_entry(&mut entries).is_none());
    }

    #[test]
    fn test_take_pre_clear_entry_takes_most_recent() {
        // Two pre-clear entries (shouldn't happen via push_pre_clear_entry,
        // but be defensive): the most recent one wins.
        let mut entries = vec![
            StashEntry {
                description: PRE_CLEAR_LABEL.to_string(),
                messages_json: "old".to_string(),
                timestamp: "00:00:00".to_string(),
            },
            StashEntry {
                description: PRE_CLEAR_LABEL.to_string(),
                messages_json: "new".to_string(),
                timestamp: "00:00:01".to_string(),
            },
        ];
        let taken = take_pre_clear_entry(&mut entries).expect("should find pre-clear entry");
        assert_eq!(taken.messages_json, "new");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].messages_json, "old");
    }

    #[test]
    fn test_auto_checkpoint_prune_limit() {
        use yoagent::provider::AnthropicProvider;
        {
            let mut stash = rw_write_or_recover(&CONVERSATION_STASH);
            stash.clear();
        }
        let mut agent =
            Agent::from_provider(AnthropicProvider, yoagent::provider::ModelConfig::mock())
                .with_system_prompt("test")
                .with_api_key("test-key");

        // Create 12 auto-checkpoints (turns 5, 10, 15, ..., 60)
        for i in 1..=12 {
            let saved = auto_checkpoint(&mut agent, i * 5);
            assert!(saved, "Should save checkpoint at turn {}", i * 5);
        }

        // Should have been pruned to MAX_AUTO_CHECKPOINTS (10)
        let stash = rw_read_or_recover(&CONVERSATION_STASH);
        let auto_count = stash
            .iter()
            .filter(|e| e.description.starts_with(AUTO_CHECKPOINT_PREFIX))
            .count();
        assert!(
            auto_count <= MAX_AUTO_CHECKPOINTS,
            "Auto-checkpoints should be pruned to {MAX_AUTO_CHECKPOINTS}, got {auto_count}"
        );

        // The oldest should have been removed — first remaining should be turn 15
        // (turns 5, 10 were the first two added, and we added 12 total, so 2 excess were pruned)
        let first_auto = stash
            .iter()
            .find(|e| e.description.starts_with(AUTO_CHECKPOINT_PREFIX))
            .expect("Should have at least one auto-checkpoint");
        assert!(
            first_auto.description.contains("turn 15"),
            "Oldest remaining should be turn 15, got: {}",
            first_auto.description
        );
    }
}
