Title: Add /todo command and TodoTool agent tool for task tracking
Files: src/commands.rs, src/commands_project.rs, src/main.rs, src/help.rs
Issue: none

## Context

Claude Code has `TodoRead`/`TodoWrite` tools that let the model track tasks during complex multi-step operations. When working on something with many steps, the model can write a task list, check items off as it goes, and refer back to what's remaining. yoyo has no equivalent — the model has to remember what it's doing purely from conversation context, which degrades on long sessions.

This is a real capability gap that affects both agentic quality (the model loses track of multi-step plans) and user experience (users can't see what the agent thinks it needs to do).

## Implementation

### 1. In-memory task store in `src/commands_project.rs`

Add a global `RwLock<Vec<TodoItem>>` (same pattern as `WATCH_COMMAND` and `CONVERSATION_STASH`):

```rust
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub id: usize,        // 1-indexed
    pub description: String,
    pub status: TodoStatus,
}

static TODO_LIST: RwLock<Vec<TodoItem>> = RwLock::new(Vec::new());
```

Functions:
- `pub fn todo_add(description: &str) -> usize` — add item, return its ID
- `pub fn todo_update(id: usize, status: TodoStatus) -> Result<(), String>` — mark an item's status
- `pub fn todo_list() -> Vec<TodoItem>` — snapshot of all items
- `pub fn todo_clear()` — clear all items
- `pub fn todo_remove(id: usize) -> Result<TodoItem, String>` — remove a single item
- `pub fn format_todo_list(items: &[TodoItem]) -> String` — formatted display with checkboxes: `[ ] pending`, `[~] in progress`, `[✓] done`

### 2. REPL command `/todo` in `src/commands_project.rs`

```
/todo                    Show all tasks
/todo add <description>  Add a new task
/todo done <id>          Mark task as done
/todo wip <id>           Mark task as in-progress
/todo remove <id>        Remove a task
/todo clear              Clear all tasks
```

Add `handle_todo(input: &str)` function. Parse subcommands from the input after `/todo `.

### 3. Agent tool `TodoTool` in `src/main.rs`

Create a `TodoTool` struct implementing `AgentTool`:
- **name**: `"todo"`
- **description**: `"Manage a task list to track progress on complex multi-step operations. Use this to plan work, track what's done, and remember what's remaining."`
- **parameters**: 
  - `action` (required): one of `"add"`, `"update"`, `"list"`, `"remove"`
  - `description` (optional): task description (required for `"add"`)
  - `id` (optional): task ID number (required for `"update"` and `"remove"`)
  - `status` (optional): one of `"pending"`, `"in_progress"`, `"done"` (required for `"update"`)

The tool should NOT require permission confirmation (it doesn't touch the filesystem).

Add it to `build_tools()` — no guarding needed since it's read/write to in-memory state only.

### 4. Wire into REPL dispatch

In the command dispatch section of `src/repl.rs` (or wherever `/todo` would be dispatched), add the routing.

### 5. Update known commands and help

- Add `/todo` to `KNOWN_COMMANDS` in `src/commands.rs`
- Add help entry in `src/help.rs`:
  ```
  "/todo — Track tasks during complex operations\n\n\
   Usage:\n\
   \x20 /todo                    Show all tasks\n\
   \x20 /todo add <description>  Add a new task\n\
   \x20 /todo done <id>          Mark task as done  \n\
   \x20 /todo wip <id>           Mark as in-progress\n\
   \x20 /todo remove <id>        Remove a task\n\
   \x20 /todo clear              Clear all tasks\n\n\
   The AI agent can also manage tasks via the todo tool during\n\
   agentic runs, helping it stay organized on multi-step operations."
  ```
- Add `/todo` to help_text() command listing

### 6. Tests

In `src/commands_project.rs` tests:
- `test_todo_add_returns_incrementing_ids`
- `test_todo_update_status`
- `test_todo_update_invalid_id`
- `test_todo_remove`
- `test_todo_remove_invalid_id`
- `test_todo_clear`
- `test_todo_list_empty`
- `test_format_todo_list_with_items`
- `test_format_todo_list_empty`
- `test_todo_status_display` — verify `[ ]`, `[~]`, `[✓]` formatting

In `src/main.rs` tests:
- `test_todo_tool_name`
- `test_todo_tool_schema_has_action`
- `test_todo_tool_in_build_tools`

In `src/help.rs` tests:
- `test_todo_in_known_commands`
- `test_todo_in_help_text`

**Important**: Since the todo list is global state (`static RwLock`), tests that modify it need to clear the list at the start to avoid interference. Add a `todo_clear_for_testing()` or use `todo_clear()` in test setup.
