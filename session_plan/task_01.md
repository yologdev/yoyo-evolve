Title: Conversation checkpointing — /checkpoint save/list/restore
Files: src/commands_session.rs, src/dispatch.rs, src/commands.rs
Issue: none

## Description

Build a conversation checkpointing system: `/checkpoint save [name]`, `/checkpoint list`, `/checkpoint restore <name>`. This closes the #1 capability gap vs Claude Code (checkpointing & rewind).

### Implementation

**In `src/commands_session.rs`:**

Add checkpoint functions:
- `checkpoint_dir()` → returns `.yoyo/checkpoints/` path (create if needed)
- `save_checkpoint(agent: &Agent, name: Option<&str>) -> Result<String, String>` — serialize `agent.messages()` to JSON in `.yoyo/checkpoints/{name}.json`. If no name given, use timestamp like `cp-20260607-113100`. Return the checkpoint name.
- `list_checkpoints() -> Vec<(String, std::time::SystemTime, usize)>` — scan checkpoint dir, return (name, modified_time, message_count) for each `.json` file.
- `restore_checkpoint(name: &str) -> Result<Vec<yoagent::Message>, String>` — read and deserialize messages from the named checkpoint file.
- `delete_checkpoint(name: &str) -> Result<(), String>` — remove a checkpoint file.

Add the dispatch handler:
- `handle_checkpoint(args: &str, agent: &mut Agent) -> Result<String, String>` — parse subcommand (save/list/restore/delete), dispatch to above functions. For restore, call `agent.set_messages(messages)` to replace conversation state.

Add `CHECKPOINT_SUBCOMMANDS: &[&str] = &["save", "list", "restore", "delete"]` for tab completion.

**In `src/dispatch.rs`:**
- Add `/checkpoint` to the command routing in `dispatch_command`. Route to `commands_session::handle_checkpoint`.

**In `src/commands.rs`:**
- Add `"checkpoint"` to `KNOWN_COMMANDS`.
- Add tab completion for `/checkpoint` subcommands.

### Behavior

- `/checkpoint save` → saves with auto-generated timestamp name
- `/checkpoint save mywork` → saves as "mywork"
- `/checkpoint list` → shows all checkpoints with timestamps and message counts
- `/checkpoint restore mywork` → replaces current conversation with checkpoint state, prints confirmation
- `/checkpoint delete mywork` → removes the checkpoint file

### Tests

- Test save/list/restore/delete with mock message data (use temp dir for checkpoint storage)
- Test auto-naming generates valid filenames
- Test restore with non-existent name returns error
- Test list on empty dir returns empty vec

### Notes
- Use `serde_json` for serialization (already a dependency via yoagent)
- The `agent.set_messages()` method is available in yoagent — verify this before implementing. If not available, use `agent.clear()` + replay approach or document the limitation.
- Keep checkpoint files small — just the messages array, no metadata beyond what serde gives us
- Don't update CLAUDE.md yet — wait until the feature is proven stable
