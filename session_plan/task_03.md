Title: Add /stash command for quick context saving during work
Files: src/commands_session.rs, src/commands.rs, src/help.rs
Issue: none

## Context

When working on a complex task, developers often need to quickly save their conversation context, switch to something else, then come back. We have `/save` and `/load` for full session persistence, but those are heavyweight — they save/restore the entire conversation to a JSON file.

What's missing is a lightweight "stash" for conversation context — analogous to `git stash` but for the conversation itself. `/stash` would let you quickly push the current conversation aside, do something else with a clean slate, then pop it back.

This is distinct from `/git stash` (which stashes file changes) — this stashes the *conversation*.

## Implementation

### 1. Add conversation stash system in `src/commands_session.rs`

Add a stash stack using a global `RwLock<Vec<StashEntry>>`:

```rust
use std::sync::RwLock;

struct StashEntry {
    description: String,
    messages_json: String,  // serialized via agent.save_messages()
    timestamp: String,
}

static CONVERSATION_STASH: RwLock<Vec<StashEntry>> = RwLock::new(Vec::new());
```

Add handlers:

```rust
/// Push current conversation onto the stash, then clear the conversation.
pub fn handle_stash_push(agent: &mut Agent, description: &str) -> String {
    let messages_json = agent.save_messages().map_err(|e| format!("Failed to save: {e}"))?;
    let desc = if description.is_empty() { 
        format!("stash@{}", stash.len()) 
    } else { 
        description.to_string() 
    };
    // Push onto stash
    // Clear agent messages
    // Return confirmation
}

/// Pop the most recent stash entry and restore it.
pub fn handle_stash_pop(agent: &mut Agent) -> String {
    // Pop from stack
    // Restore messages via agent.restore_messages()
    // Return confirmation
}

/// List all stash entries.
pub fn handle_stash_list() -> String {
    // Format entries with index, description, timestamp
}

/// Drop a stash entry by index.
pub fn handle_stash_drop(index: usize) -> String {
    // Remove entry at index
}
```

### 2. Add `/stash` to KNOWN_COMMANDS in `src/commands.rs`

Add `/stash` to the array and add dispatch logic:
- `/stash` or `/stash push [desc]` — push current conversation
- `/stash pop` — restore most recent
- `/stash list` — show all entries
- `/stash drop [N]` — remove entry N

### 3. Add help text in `src/help.rs`

```
/stash — Save and restore conversation context

Usage:
  /stash [desc]        Push current conversation and start fresh
  /stash push [desc]   Same as above
  /stash pop           Restore the most recent stashed conversation
  /stash list          Show all stashed conversations
  /stash drop [N]      Remove stash entry N (default: 0, most recent)

Like git stash, but for your conversation. Useful when you need to
quickly switch tasks and come back later.
```

### 4. Tests

- `test_stash_in_known_commands`
- `test_stash_in_help_text`
- `test_stash_help_detail_exists`
- `test_parse_stash_subcommand_push` — parse `/stash push WIP`
- `test_parse_stash_subcommand_pop` — parse `/stash pop`
- `test_parse_stash_subcommand_list` — parse `/stash list`
- `test_parse_stash_subcommand_drop` — parse `/stash drop 2`
- `test_parse_stash_subcommand_default` — parse `/stash` defaults to push
- `test_stash_entry_description_default` — empty desc gets auto-generated name
