Title: Add /todo command for in-session task tracking
Files: src/commands.rs, src/commands_project.rs, src/repl.rs, src/help.rs
Issue: #176 (retry — previously reverted due to test failures)

## Context

Claude Code has TodoRead/TodoWrite tools for tracking tasks during complex multi-step operations. yoyo has no equivalent — the model loses track of multi-step plans on long sessions. Issue #176 attempted this on Day 24 but was reverted due to test failures. This is a fresh attempt.

## Implementation

### 1. Task store in `src/commands_project.rs`

Add a global `RwLock<Vec<TodoItem>>` following the same pattern as `WATCH_COMMAND` and `CONVERSATION_STASH`:

```rust
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoStatus::Pending => write!(f, "[ ]"),
            TodoStatus::InProgress => write!(f, "[~]"),
            TodoStatus::Done => write!(f, "[✓]"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub id: usize,
    pub description: String,
    pub status: TodoStatus,
}

static TODO_LIST: RwLock<Vec<TodoItem>> = RwLock::new(Vec::new());
static TODO_NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
```

Functions:
- `pub fn todo_add(description: &str) -> usize` — add item, return its ID (use AtomicUsize for thread-safe ID generation)
- `pub fn todo_update(id: usize, status: TodoStatus) -> Result<(), String>` — update status
- `pub fn todo_list() -> Vec<TodoItem>` — snapshot
- `pub fn todo_clear()` — clear all, reset ID counter
- `pub fn todo_remove(id: usize) -> Result<TodoItem, String>` — remove single item
- `pub fn format_todo_list(items: &[TodoItem]) -> String` — format with status checkboxes

### 2. REPL command `/todo` dispatch

Add `handle_todo(input: &str) -> String` in `src/commands_project.rs`:
```
/todo                    Show all tasks
/todo add <description>  Add a new task
/todo done <id>          Mark task as done
/todo wip <id>           Mark task as in-progress
/todo remove <id>        Remove a task
/todo clear              Clear all tasks
```

Parse subcommands from the input string after `/todo `.

### 3. Wire into REPL dispatch (src/repl.rs)

Add `/todo` to the command dispatch section. The handler returns a String that gets printed — no agent interaction needed.

### 4. Update known commands and help (src/commands.rs, src/help.rs)

- Add `/todo` to `KNOWN_COMMANDS` array in `src/commands.rs`
- Add help entry in `src/help.rs`:
  ```
  "todo" => Some("/todo — Track tasks during complex operations\n\n\
   Usage:\n\
   \x20 /todo                    Show all tasks\n\
   \x20 /todo add <description>  Add a new task\n\
   \x20 /todo done <id>          Mark task as done\n\
   \x20 /todo wip <id>           Mark as in-progress\n\
   \x20 /todo remove <id>        Remove a task\n\
   \x20 /todo clear              Clear all tasks"),
  ```
- Add to the `/help` summary listing in `help_text()`

### 5. Tests (IMPORTANT — this is why it reverted last time)

**Critical**: Use `todo_clear()` at the START of every test that touches the global `TODO_LIST` to avoid interference between tests. The global `RwLock` state persists across tests in the same process.

In `src/commands_project.rs` tests:
- `test_todo_add_returns_incrementing_ids` — clear first, add two, verify IDs
- `test_todo_update_status` — add item, update to Done, verify
- `test_todo_update_invalid_id` — verify error on nonexistent ID
- `test_todo_remove` — add item, remove it, verify gone
- `test_todo_remove_invalid_id` — verify error on nonexistent ID
- `test_todo_clear` — add items, clear, verify empty
- `test_todo_list_empty` — clear first, verify empty
- `test_format_todo_list` — verify checkbox formatting ([ ], [~], [✓])
- `test_handle_todo_add` — test the command parser
- `test_handle_todo_show_empty` — test the show subcommand on empty list

In `src/help.rs` tests:
- `test_todo_in_known_commands`
- `test_todo_help_exists`

### 6. Do NOT add a TodoTool agent tool in this task

Keep it simple — just the REPL command. The agent tool can be added in a future session once the REPL command is stable. This reduces the surface area of the change and makes test failures easier to diagnose.
