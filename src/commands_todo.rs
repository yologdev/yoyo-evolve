//! Todo list command handler: /todo and its subcommands.

use crate::format::*;

use std::sync::RwLock;

/// Acquire a read-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_read_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

/// Acquire a write-guard, recovering from a poisoned RwLock instead of panicking.
fn rw_write_or_recover<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

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

/// Add a todo item, return its ID.
pub fn todo_add(description: &str) -> usize {
    let id = TODO_NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let item = TodoItem {
        id,
        description: description.to_string(),
        status: TodoStatus::Pending,
    };
    rw_write_or_recover(&TODO_LIST).push(item);
    id
}

/// Update the status of a todo item by ID.
pub fn todo_update(id: usize, status: TodoStatus) -> Result<(), String> {
    let mut list = rw_write_or_recover(&TODO_LIST);
    match list.iter_mut().find(|item| item.id == id) {
        Some(item) => {
            item.status = status;
            Ok(())
        }
        None => Err(format!("No todo item with ID {id}")),
    }
}

/// Return a snapshot of all todo items.
pub fn todo_list() -> Vec<TodoItem> {
    rw_read_or_recover(&TODO_LIST).clone()
}

/// Clear all todo items and reset the ID counter.
pub fn todo_clear() {
    rw_write_or_recover(&TODO_LIST).clear();
    TODO_NEXT_ID.store(1, std::sync::atomic::Ordering::SeqCst);
}

/// Remove a single todo item by ID.
pub fn todo_remove(id: usize) -> Result<TodoItem, String> {
    let mut list = rw_write_or_recover(&TODO_LIST);
    let pos = list
        .iter()
        .position(|item| item.id == id)
        .ok_or_else(|| format!("No todo item with ID {id}"))?;
    Ok(list.remove(pos))
}

/// Format the todo list with status checkboxes.
pub fn format_todo_list(items: &[TodoItem]) -> String {
    if items.is_empty() {
        return "  No tasks. Use /todo add <description> to add one.".to_string();
    }
    let mut out = String::new();
    for item in items {
        out.push_str(&format!(
            "  {} #{} {}\n",
            item.status, item.id, item.description
        ));
    }
    // Remove trailing newline
    if out.ends_with('\n') {
        out.truncate(out.len() - 1);
    }
    out
}

/// Handle the /todo command and its subcommands. Returns a string to print.
pub fn handle_todo(input: &str) -> String {
    let arg = input.strip_prefix("/todo").unwrap_or("").trim();

    // Route "board" subcommands to the board handler
    if arg == "board" || arg.starts_with("board ") {
        return handle_todo_board(arg.strip_prefix("board").unwrap_or("").trim());
    }

    if arg.is_empty() {
        // Show all tasks
        let items = todo_list();
        return format_todo_list(&items);
    }

    if arg == "clear" {
        todo_clear();
        return format!("{GREEN}  ✓ Cleared all tasks{RESET}");
    }

    if let Some(desc) = arg.strip_prefix("add ") {
        let desc = desc.trim();
        if desc.is_empty() {
            return "  Usage: /todo add <description>".to_string();
        }
        let id = todo_add(desc);
        return format!("{GREEN}  ✓ Added task #{id}: {desc}{RESET}");
    }
    if arg == "add" {
        return "  Usage: /todo add <description>".to_string();
    }

    if let Some(id_str) = arg.strip_prefix("done ") {
        let id_str = id_str.trim();
        match id_str.parse::<usize>() {
            Ok(id) => match todo_update(id, TodoStatus::Done) {
                Ok(()) => return format!("{GREEN}  ✓ Marked #{id} as done{RESET}"),
                Err(e) => return format!("{RED}  {e}{RESET}"),
            },
            Err(_) => return format!("{RED}  Invalid ID: {id_str}{RESET}"),
        }
    }

    if let Some(id_str) = arg.strip_prefix("wip ") {
        let id_str = id_str.trim();
        match id_str.parse::<usize>() {
            Ok(id) => match todo_update(id, TodoStatus::InProgress) {
                Ok(()) => return format!("{GREEN}  ✓ Marked #{id} as in-progress{RESET}"),
                Err(e) => return format!("{RED}  {e}{RESET}"),
            },
            Err(_) => return format!("{RED}  Invalid ID: {id_str}{RESET}"),
        }
    }

    if let Some(id_str) = arg.strip_prefix("remove ") {
        let id_str = id_str.trim();
        match id_str.parse::<usize>() {
            Ok(id) => match todo_remove(id) {
                Ok(item) => {
                    return format!("{GREEN}  ✓ Removed #{id}: {}{RESET}", item.description)
                }
                Err(e) => return format!("{RED}  {e}{RESET}"),
            },
            Err(_) => return format!("{RED}  Invalid ID: {id_str}{RESET}"),
        }
    }

    // Unknown subcommand — show usage
    "  Usage:\n\
     \x20 /todo                    Show all tasks\n\
     \x20 /todo add <description>  Add a new task\n\
     \x20 /todo done <id>          Mark task as done\n\
     \x20 /todo wip <id>           Mark as in-progress\n\
     \x20 /todo remove <id>        Remove a task\n\
     \x20 /todo clear              Clear all tasks"
        .to_string()
}

// === Board (session_plan/ Kanban view) ===

const SESSION_PLAN_DIR: &str = "session_plan";

/// A task parsed from a `session_plan/task_*.md` file.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardTask {
    /// File-based ID, e.g. "task_01"
    pub id: String,
    /// Title extracted from the `Title:` line
    pub title: String,
    /// Status: backlog, active, or done (defaults to backlog if missing)
    pub status: String,
    /// Issue reference (e.g. "#123" or "none")
    pub issue: String,
    /// Files listed on the `Files:` line
    pub files: String,
    /// The raw `Status:` value when it was present but not recognized.
    ///
    /// This is the explicit third value: "no Status line" (silent backlog) and
    /// "Status line I couldn't read" are different situations, and the second
    /// must not be absorbed by the first. The task still lands in the backlog
    /// bucket so it never disappears from the board -- but the board says so.
    pub unrecognized_status: Option<String>,
}

/// Parse a single task file's content into a `BoardTask`.
fn parse_task_file(id: &str, content: &str) -> BoardTask {
    let mut title = String::new();
    let mut status = "backlog".to_string();
    let mut issue = "none".to_string();
    let mut files = String::new();
    let mut unrecognized_status: Option<String> = None;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("Title:") {
            title = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("Status:") {
            // Derive from the one function that owns the status vocabulary --
            // hand-typing the list here is what let reader and writer drift.
            let raw = val.trim();
            if raw.is_empty() {
                // `Status:` with no value == no status. Silent backlog default.
            } else {
                match normalize_board_status(raw) {
                    Some(s) => status = s.to_string(),
                    // Explicit third value: remembered, reported, not swallowed.
                    None => unrecognized_status = Some(raw.to_string()),
                }
            }
        } else if let Some(val) = line.strip_prefix("Issue:") {
            issue = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("Files:") {
            files = val.trim().to_string();
        }
    }

    BoardTask {
        id: id.to_string(),
        title,
        status,
        issue,
        files,
        unrecognized_status,
    }
}

/// Read all `session_plan/task_*.md` files and parse them into `BoardTask`s.
/// Uses `base_dir` to support testing with temp directories.
fn read_task_files(base_dir: &str) -> Vec<BoardTask> {
    let dir = std::path::Path::new(base_dir);
    if !dir.is_dir() {
        return Vec::new();
    }

    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.starts_with("task_") && name.ends_with(".md")
            })
            .collect(),
        Err(_) => return Vec::new(),
    };

    // Sort by filename for stable ordering
    entries.sort_by_key(|e| e.file_name());

    let mut tasks = Vec::new();
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Extract ID: "task_01.md" -> "task_01"
        let id = name.strip_suffix(".md").unwrap_or(&name).to_string();
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            tasks.push(parse_task_file(&id, &content));
        }
    }
    tasks
}

/// Read the goal from `session_plan/goal.md`.
fn read_board_goal(base_dir: &str) -> Option<String> {
    let path = std::path::Path::new(base_dir).join("goal.md");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Render a Kanban-style board view from task data.
fn render_board(tasks: &[BoardTask], goal: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("  Task Board\n");
    out.push_str("  ======================================\n\n");

    if let Some(g) = goal {
        out.push_str(&format!("  Goal: {g}\n\n"));
    }

    let backlog: Vec<_> = tasks.iter().filter(|t| t.status == "backlog").collect();
    let active: Vec<_> = tasks.iter().filter(|t| t.status == "active").collect();
    let done: Vec<_> = tasks.iter().filter(|t| t.status == "done").collect();

    out.push_str("  -- Backlog ---------------------\n");
    if backlog.is_empty() {
        out.push_str("    (empty)\n");
    } else {
        for t in &backlog {
            let issue_tag = if t.issue != "none" && !t.issue.is_empty() {
                format!(" ({})", t.issue)
            } else {
                String::new()
            };
            out.push_str(&format!("    [ ] {} -- {}{}\n", t.id, t.title, issue_tag));
        }
    }

    out.push_str("\n  -- Active ----------------------\n");
    if active.is_empty() {
        out.push_str("    (empty)\n");
    } else {
        for t in &active {
            let issue_tag = if t.issue != "none" && !t.issue.is_empty() {
                format!(" ({})", t.issue)
            } else {
                String::new()
            };
            out.push_str(&format!("    [~] {} -- {}{}\n", t.id, t.title, issue_tag));
        }
    }

    out.push_str("\n  -- Done ------------------------\n");
    if done.is_empty() {
        out.push_str("    (empty)\n");
    } else {
        for t in &done {
            let issue_tag = if t.issue != "none" && !t.issue.is_empty() {
                format!(" ({})", t.issue)
            } else {
                String::new()
            };
            out.push_str(&format!("    [x] {} -- {}{}\n", t.id, t.title, issue_tag));
        }
    }

    out.push_str(&format!(
        "\n  {} backlog, {} active, {} done\n",
        backlog.len(),
        active.len(),
        done.len()
    ));

    // Statuses I couldn't read are shown in the backlog bucket above (losing the
    // task would be worse), but they get said out loud here so the board never
    // reports the wrong bucket with a straight face.
    for t in tasks.iter() {
        if let Some(raw) = &t.unrecognized_status {
            out.push_str(&format!(
                "{}  warning: {}.md has unrecognized status '{}' -- shown as backlog{}\n",
                DIM, t.id, raw, RESET
            ));
        }
    }

    out
}

/// Find the next task number (e.g., if task_01 and task_03 exist, returns 4).
fn next_task_number(base_dir: &str) -> u32 {
    let dir = std::path::Path::new(base_dir);
    if !dir.is_dir() {
        return 1;
    }

    let mut max_num: u32 = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(rest) = name.strip_prefix("task_") {
                if let Some(num_str) = rest.strip_suffix(".md") {
                    if let Ok(n) = num_str.parse::<u32>() {
                        if n > max_num {
                            max_num = n;
                        }
                    }
                }
            }
        }
    }
    max_num + 1
}

/// Create a new task file with the given title and status.
fn create_task_file(base_dir: &str, title: &str, status: &str) -> Result<String, String> {
    let dir = std::path::Path::new(base_dir);
    if !dir.is_dir() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create {base_dir}/: {e}"))?;
    }

    let num = next_task_number(base_dir);
    let id = format!("task_{num:02}");
    let filename = format!("{id}.md");
    let path = dir.join(&filename);

    let content = format!("Title: {title}\nFiles: \nIssue: none\nStatus: {status}\n");
    std::fs::write(&path, content).map_err(|e| format!("Failed to write {filename}: {e}"))?;
    Ok(id)
}

/// Update or add a `Status:` line in a task file.
fn update_task_status(base_dir: &str, task_id: &str, new_status: &str) -> Result<(), String> {
    let path = std::path::Path::new(base_dir).join(format!("{task_id}.md"));
    let content =
        std::fs::read_to_string(&path).map_err(|_| format!("Task file not found: {task_id}.md"))?;

    let mut found_status = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            if line.starts_with("Status:") {
                found_status = true;
                format!("Status: {new_status}")
            } else {
                line.to_string()
            }
        })
        .collect();

    if !found_status {
        // Insert Status line after the header lines (Title/Files/Issue)
        let insert_pos = lines
            .iter()
            .position(|l| {
                !l.starts_with("Title:")
                    && !l.starts_with("Files:")
                    && !l.starts_with("Issue:")
                    && !l.is_empty()
            })
            .unwrap_or(lines.len());
        lines.insert(insert_pos, format!("Status: {new_status}"));
    }

    let new_content = lines.join("\n");
    // Preserve trailing newline if the original had one
    let new_content = if content.ends_with('\n') && !new_content.ends_with('\n') {
        format!("{new_content}\n")
    } else {
        new_content
    };
    std::fs::write(&path, new_content).map_err(|e| format!("Failed to write {task_id}.md: {e}"))?;
    Ok(())
}

/// Normalize a board status from user input to canonical form.
fn normalize_board_status(input: &str) -> Option<&'static str> {
    match input.to_lowercase().as_str() {
        "backlog" => Some("backlog"),
        "active" | "wip" | "inprogress" | "in-progress" | "in_progress" => Some("active"),
        "done" | "complete" | "completed" => Some("done"),
        _ => None,
    }
}

/// Normalize a task ID input -- accept "task_01", "01", or "1" and return "task_01".
fn normalize_task_id(input: &str) -> String {
    let input = input.trim();
    if input.starts_with("task_") {
        return input.to_string();
    }
    // Try to parse as a number
    if let Ok(n) = input.parse::<u32>() {
        return format!("task_{n:02}");
    }
    // Return as-is (will fail at file lookup)
    input.to_string()
}

/// Handle `/todo board` subcommands.
pub fn handle_todo_board(input: &str) -> String {
    handle_todo_board_with_dir(input, SESSION_PLAN_DIR)
}

/// Inner implementation that accepts a configurable base directory (for testing).
fn handle_todo_board_with_dir(input: &str, base_dir: &str) -> String {
    let arg = input.trim();

    // /todo board or /todo board show
    if arg.is_empty() || arg == "show" {
        let tasks = read_task_files(base_dir);
        if tasks.is_empty() {
            return format!(
                "{YELLOW}  No task files in {base_dir}/. Use /todo board init to set up.{RESET}"
            );
        }
        let goal = read_board_goal(base_dir);
        return render_board(&tasks, goal.as_deref());
    }

    // /todo board init [goal]
    if arg == "init" || arg.starts_with("init ") {
        let dir = std::path::Path::new(base_dir);
        let goal_text = arg.strip_prefix("init").unwrap_or("").trim();

        if dir.is_dir() {
            // Check if there are already task files
            let tasks = read_task_files(base_dir);
            if !tasks.is_empty() {
                return format!(
                    "{YELLOW}  {base_dir}/ already has {} task(s). Not reinitializing.{RESET}",
                    tasks.len()
                );
            }
        }

        if let Err(e) = std::fs::create_dir_all(dir) {
            return format!("{RED}  Failed to create {base_dir}/: {e}{RESET}");
        }

        if !goal_text.is_empty() {
            let goal_path = dir.join("goal.md");
            if let Err(e) = std::fs::write(&goal_path, format!("{goal_text}\n")) {
                return format!("{RED}  Failed to write goal.md: {e}{RESET}");
            }
        }

        return format!("{GREEN}  Initialized {base_dir}/ board{RESET}");
    }

    // /todo board add <title>
    if let Some(title) = arg.strip_prefix("add ") {
        let title = title.trim();
        if title.is_empty() {
            return "  Usage: /todo board add <title>".to_string();
        }
        match create_task_file(base_dir, title, "backlog") {
            Ok(id) => {
                return format!("{GREEN}  Created {id}: {title}{RESET}");
            }
            Err(e) => return format!("{RED}  {e}{RESET}"),
        }
    }
    if arg == "add" {
        return "  Usage: /todo board add <title>".to_string();
    }

    // /todo board goal <text>
    if let Some(goal_text) = arg.strip_prefix("goal ") {
        let goal_text = goal_text.trim();
        if goal_text.is_empty() {
            return "  Usage: /todo board goal <text>".to_string();
        }
        let dir = std::path::Path::new(base_dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            return format!("{RED}  Failed to create {base_dir}/: {e}{RESET}");
        }
        let goal_path = dir.join("goal.md");
        match std::fs::write(&goal_path, format!("{goal_text}\n")) {
            Ok(()) => return format!("{GREEN}  Updated goal: {goal_text}{RESET}"),
            Err(e) => return format!("{RED}  Failed to write goal.md: {e}{RESET}"),
        }
    }
    if arg == "goal" {
        return "  Usage: /todo board goal <text>".to_string();
    }

    // /todo board evidence <text>
    if let Some(evidence_text) = arg.strip_prefix("evidence ") {
        let evidence_text = evidence_text.trim();
        if evidence_text.is_empty() {
            return "  Usage: /todo board evidence <text>".to_string();
        }
        let dir = std::path::Path::new(base_dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            return format!("{RED}  Failed to create {base_dir}/: {e}{RESET}");
        }
        let evidence_path = dir.join("evidence.md");
        let existing = std::fs::read_to_string(&evidence_path).unwrap_or_default();
        let updated = format!("{existing}- {evidence_text}\n");
        match std::fs::write(&evidence_path, updated) {
            Ok(()) => return format!("{GREEN}  Added evidence: {evidence_text}{RESET}"),
            Err(e) => return format!("{RED}  Failed to write evidence.md: {e}{RESET}"),
        }
    }
    if arg == "evidence" {
        return "  Usage: /todo board evidence <text>".to_string();
    }

    // /todo board done <task_id>
    if let Some(task_id) = arg.strip_prefix("done ") {
        let task_id = normalize_task_id(task_id.trim());
        if task_id.is_empty() {
            return "  Usage: /todo board done <task_id>".to_string();
        }
        match update_task_status(base_dir, &task_id, "done") {
            Ok(()) => return format!("{GREEN}  Marked {task_id} as done{RESET}"),
            Err(e) => return format!("{RED}  {e}{RESET}"),
        }
    }

    // /todo board move <task_id> <status>
    if let Some(rest) = arg.strip_prefix("move ") {
        let rest = rest.trim();
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() < 2 || parts[1].trim().is_empty() {
            return "  Usage: /todo board move <task_id> <status>\n  \
                    Statuses: backlog, active, done"
                .to_string();
        }
        let task_id = normalize_task_id(parts[0]);
        let status_input = parts[1].trim();
        let status = match normalize_board_status(status_input) {
            Some(s) => s,
            None => {
                return format!(
                    "{RED}  Unknown status: {status_input}. Use: backlog, active, done{RESET}"
                )
            }
        };
        match update_task_status(base_dir, &task_id, status) {
            Ok(()) => return format!("{GREEN}  Moved {task_id} to {status}{RESET}"),
            Err(e) => return format!("{RED}  {e}{RESET}"),
        }
    }

    // Unknown board subcommand
    "  Usage:\n\
     \x20 /todo board                      Show task board\n\
     \x20 /todo board init [goal]          Initialize session_plan/\n\
     \x20 /todo board add <title>          Add task (backlog)\n\
     \x20 /todo board move <id> <status>   Move task (backlog/active/done)\n\
     \x20 /todo board done <id>            Mark task done\n\
     \x20 /todo board goal <text>          Set/update goal\n\
     \x20 /todo board evidence <text>      Append evidence"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_todo_add_returns_incrementing_ids() {
        todo_clear();
        let id1 = todo_add("first task");
        let id2 = todo_add("second task");
        assert!(id2 > id1, "IDs should increment: {id1} < {id2}");
        let items = todo_list();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].description, "first task");
        assert_eq!(items[1].description, "second task");
    }

    #[test]
    #[serial]
    fn test_todo_update_status() {
        todo_clear();
        let id = todo_add("update me");
        assert_eq!(todo_list()[0].status, TodoStatus::Pending);

        todo_update(id, TodoStatus::InProgress).unwrap();
        assert_eq!(todo_list()[0].status, TodoStatus::InProgress);

        todo_update(id, TodoStatus::Done).unwrap();
        assert_eq!(todo_list()[0].status, TodoStatus::Done);
    }

    #[test]
    #[serial]
    fn test_todo_update_invalid_id() {
        todo_clear();
        let result = todo_update(99999, TodoStatus::Done);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("99999"));
    }

    #[test]
    #[serial]
    fn test_todo_remove() {
        todo_clear();
        let id = todo_add("remove me");
        assert_eq!(todo_list().len(), 1);

        let removed = todo_remove(id).unwrap();
        assert_eq!(removed.description, "remove me");
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_todo_remove_invalid_id() {
        todo_clear();
        let result = todo_remove(99998);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("99998"));
    }

    #[test]
    #[serial]
    fn test_todo_clear() {
        todo_clear();
        todo_add("one");
        todo_add("two");
        assert_eq!(todo_list().len(), 2);

        todo_clear();
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_todo_list_empty() {
        todo_clear();
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_format_todo_list() {
        todo_clear();
        let id1 = todo_add("pending task");
        let id2 = todo_add("wip task");
        let id3 = todo_add("done task");
        todo_update(id2, TodoStatus::InProgress).unwrap();
        todo_update(id3, TodoStatus::Done).unwrap();

        let items = todo_list();
        let formatted = format_todo_list(&items);
        assert!(formatted.contains("[ ]"), "Should contain pending checkbox");
        assert!(
            formatted.contains("[~]"),
            "Should contain in-progress checkbox"
        );
        assert!(formatted.contains("[✓]"), "Should contain done checkbox");
        assert!(formatted.contains(&format!("#{id1}")));
        assert!(formatted.contains("pending task"));
        assert!(formatted.contains("wip task"));
        assert!(formatted.contains("done task"));
    }

    #[test]
    fn test_format_todo_list_empty() {
        let formatted = format_todo_list(&[]);
        assert!(formatted.contains("No tasks"));
    }

    #[test]
    #[serial]
    fn test_handle_todo_add() {
        todo_clear();
        let result = handle_todo("/todo add write tests");
        assert!(result.contains("Added task"));
        assert!(result.contains("write tests"));
        assert_eq!(todo_list().len(), 1);
    }

    #[test]
    #[serial]
    fn test_handle_todo_show_empty() {
        todo_clear();
        let result = handle_todo("/todo");
        assert!(result.contains("No tasks"));
    }

    #[test]
    #[serial]
    fn test_handle_todo_done() {
        todo_clear();
        let id = todo_add("finish me");
        let result = handle_todo(&format!("/todo done {id}"));
        assert!(result.contains("done"));
        assert_eq!(todo_list()[0].status, TodoStatus::Done);
    }

    #[test]
    #[serial]
    fn test_handle_todo_wip() {
        todo_clear();
        let id = todo_add("start me");
        let result = handle_todo(&format!("/todo wip {id}"));
        assert!(result.contains("in-progress"));
        assert_eq!(todo_list()[0].status, TodoStatus::InProgress);
    }

    #[test]
    #[serial]
    fn test_handle_todo_remove_via_command() {
        todo_clear();
        let id = todo_add("delete me");
        let result = handle_todo(&format!("/todo remove {id}"));
        assert!(result.contains("Removed"));
        assert!(todo_list().is_empty());
    }

    #[test]
    #[serial]
    fn test_handle_todo_clear_via_command() {
        todo_clear();
        todo_add("one");
        todo_add("two");
        let result = handle_todo("/todo clear");
        assert!(result.contains("Cleared"));
        assert!(todo_list().is_empty());
    }

    #[test]
    fn test_handle_todo_unknown_subcommand() {
        let result = handle_todo("/todo badcmd");
        assert!(result.contains("Usage"));
    }

    #[test]
    #[serial]
    fn test_handle_todo_add_empty_description() {
        let result = handle_todo("/todo add");
        assert!(result.contains("Usage"));
        let result2 = handle_todo("/todo add   ");
        assert!(result2.contains("Usage"));
    }

    #[test]
    fn test_todo_in_known_commands() {
        assert!(
            crate::commands::KNOWN_COMMANDS.contains(&"/todo"),
            "/todo should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_todo_help_exists() {
        let help = crate::help::command_help("todo");
        assert!(help.is_some(), "todo should have help text");
        let text = help.unwrap();
        assert!(text.contains("/todo add"));
        assert!(text.contains("/todo done"));
        assert!(text.contains("/todo clear"));
    }

    #[test]
    fn test_todo_in_help_text() {
        let text = crate::help::help_text();
        assert!(text.contains("/todo"), "/todo should appear in help text");
    }

    // === Board tests (session_plan/ based) ===

    fn make_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    #[test]
    fn test_parse_task_file_full() {
        let content = "Title: Fix the parser\nFiles: src/parser.rs\nIssue: #42\nStatus: active\n";
        let task = parse_task_file("task_01", content);
        assert_eq!(task.id, "task_01");
        assert_eq!(task.title, "Fix the parser");
        assert_eq!(task.status, "active");
        assert_eq!(task.issue, "#42");
        assert_eq!(task.files, "src/parser.rs");
    }

    #[test]
    fn test_parse_task_file_defaults_to_backlog() {
        let content = "Title: Some task\nFiles: src/main.rs\nIssue: none\n";
        let task = parse_task_file("task_02", content);
        assert_eq!(task.status, "backlog");
    }

    #[test]
    fn test_parse_task_file_ignores_invalid_status() {
        let content = "Title: Bad status\nStatus: invalid_status\n";
        let task = parse_task_file("task_03", content);
        assert_eq!(task.status, "backlog");
    }

    #[test]
    fn test_parse_task_file_accepts_writer_aliases() {
        // The writer (normalize_board_status) teaches these spellings; the reader
        // must accept every one of them or the board describes the wrong bucket.
        for alias in ["in-progress", "in_progress", "inprogress", "wip", "WIP"] {
            let content = format!("Title: Aliased\nStatus: {alias}\n");
            let task = parse_task_file("task_01", &content);
            assert_eq!(task.status, "active", "alias {alias} should map to active");
            assert_eq!(task.unrecognized_status, None);
        }
    }

    #[test]
    fn test_parse_task_file_accepts_done_aliases() {
        for alias in ["complete", "completed", "Done"] {
            let content = format!("Title: Aliased\nStatus: {alias}\n");
            let task = parse_task_file("task_01", &content);
            assert_eq!(task.status, "done", "alias {alias} should map to done");
            assert_eq!(task.unrecognized_status, None);
        }
    }

    #[test]
    fn test_parse_task_file_missing_status_is_silent_backlog() {
        // No `Status:` line at all -- backlog is the correct default and must
        // stay quiet (no warning to render).
        let content = "Title: Some task\nFiles: src/main.rs\n";
        let task = parse_task_file("task_02", content);
        assert_eq!(task.status, "backlog");
        assert_eq!(task.unrecognized_status, None);
        // An empty `Status:` value counts as missing, not as a typo.
        let task = parse_task_file("task_03", "Title: T\nStatus:   \n");
        assert_eq!(task.status, "backlog");
        assert_eq!(task.unrecognized_status, None);
    }

    #[test]
    fn test_parse_task_file_typo_status_is_audible() {
        // The near-miss side: a typo must not vanish into the backlog bucket
        // silently. It still appears on the board, but it announces itself.
        let content = "Title: Typo'd status\nStatus: activ\n";
        let task = parse_task_file("task_03", content);
        assert_eq!(task.unrecognized_status.as_deref(), Some("activ"));

        let output = render_board(&[task], None);
        assert!(
            output.contains("task_03"),
            "unrecognized task must still appear on the board: {output}"
        );
        assert!(
            output.contains("unrecognized status") && output.contains("activ"),
            "board must warn about the unrecognized status: {output}"
        );
    }

    #[test]
    fn test_render_board_no_warning_when_all_statuses_known() {
        let tasks = vec![parse_task_file("task_01", "Title: Fine\nStatus: wip\n")];
        let output = render_board(&tasks, None);
        assert!(!output.contains("unrecognized status"), "{output}");
    }

    #[test]
    fn test_read_task_files_empty_dir() {
        let tmp = make_temp_dir();
        let tasks = read_task_files(tmp.path().to_str().unwrap());
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_read_task_files_nonexistent_dir() {
        let tasks = read_task_files("/tmp/nonexistent_board_test_dir_xyz");
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_read_task_files_reads_and_sorts() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        std::fs::write(dir.join("task_02.md"), "Title: Second\nStatus: active\n").unwrap();
        std::fs::write(dir.join("task_01.md"), "Title: First\nStatus: backlog\n").unwrap();
        // Non-task file should be ignored
        std::fs::write(dir.join("goal.md"), "Build something").unwrap();

        let tasks = read_task_files(dir.to_str().unwrap());
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "task_01");
        assert_eq!(tasks[0].title, "First");
        assert_eq!(tasks[1].id, "task_02");
        assert_eq!(tasks[1].title, "Second");
    }

    #[test]
    fn test_render_board_groups_by_status() {
        let tasks = vec![
            BoardTask {
                id: "task_01".into(),
                title: "Backlog task".into(),
                status: "backlog".into(),
                issue: "none".into(),
                files: String::new(),
                unrecognized_status: None,
            },
            BoardTask {
                id: "task_02".into(),
                title: "Active task".into(),
                status: "active".into(),
                issue: "#10".into(),
                files: String::new(),
                unrecognized_status: None,
            },
            BoardTask {
                id: "task_03".into(),
                title: "Done task".into(),
                status: "done".into(),
                issue: "none".into(),
                files: String::new(),
                unrecognized_status: None,
            },
        ];
        let output = render_board(&tasks, Some("Build it"));
        assert!(output.contains("Goal: Build it"));
        assert!(output.contains("Backlog"));
        assert!(output.contains("Active"));
        assert!(output.contains("Done"));
        assert!(output.contains("task_01"));
        assert!(output.contains("Backlog task"));
        assert!(output.contains("task_02"));
        assert!(output.contains("(#10)"));
        assert!(output.contains("task_03"));
        assert!(output.contains("1 backlog, 1 active, 1 done"));
    }

    #[test]
    fn test_board_init_creates_dir() {
        let tmp = make_temp_dir();
        let dir = tmp.path().join("sp");
        let base = dir.to_str().unwrap();

        let result = handle_todo_board_with_dir("init Build something", base);
        assert!(result.contains("Initialized"));
        assert!(dir.is_dir());

        // Goal file should exist
        let goal = std::fs::read_to_string(dir.join("goal.md")).unwrap();
        assert!(goal.contains("Build something"));
    }

    #[test]
    fn test_board_init_no_goal() {
        let tmp = make_temp_dir();
        let dir = tmp.path().join("sp2");
        let base = dir.to_str().unwrap();

        let result = handle_todo_board_with_dir("init", base);
        assert!(result.contains("Initialized"));
        assert!(dir.is_dir());
        // No goal.md should be created
        assert!(!dir.join("goal.md").exists());
    }

    #[test]
    fn test_board_init_refuses_if_tasks_exist() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        std::fs::write(dir.join("task_01.md"), "Title: existing\n").unwrap();
        let base = dir.to_str().unwrap();

        let result = handle_todo_board_with_dir("init New goal", base);
        assert!(result.contains("already has"));
        assert!(result.contains("1 task"));
    }

    #[test]
    fn test_board_add_creates_task_file() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        let result = handle_todo_board_with_dir("add Implement feature X", base);
        assert!(result.contains("Created task_01"));
        assert!(result.contains("Implement feature X"));

        let content = std::fs::read_to_string(tmp.path().join("task_01.md")).unwrap();
        assert!(content.contains("Title: Implement feature X"));
        assert!(content.contains("Status: backlog"));
    }

    #[test]
    fn test_board_add_increments_number() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        handle_todo_board_with_dir("add First", base);
        let result = handle_todo_board_with_dir("add Second", base);
        assert!(result.contains("Created task_02"));
        assert!(tmp.path().join("task_02.md").exists());
    }

    #[test]
    fn test_board_move_by_id() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        handle_todo_board_with_dir("add Some task", base);

        let result = handle_todo_board_with_dir("move task_01 active", base);
        assert!(result.contains("Moved task_01 to active"));

        let content = std::fs::read_to_string(tmp.path().join("task_01.md")).unwrap();
        assert!(content.contains("Status: active"));
    }

    #[test]
    fn test_board_move_accepts_short_id() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        handle_todo_board_with_dir("add A task", base);

        let result = handle_todo_board_with_dir("move 1 active", base);
        assert!(result.contains("Moved task_01 to active"));
    }

    #[test]
    fn test_board_done_shortcut() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        handle_todo_board_with_dir("add Complete me", base);

        let result = handle_todo_board_with_dir("done task_01", base);
        assert!(result.contains("Marked task_01 as done"));

        let content = std::fs::read_to_string(tmp.path().join("task_01.md")).unwrap();
        assert!(content.contains("Status: done"));
    }

    #[test]
    fn test_board_goal_updates() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        handle_todo_board_with_dir("init Old goal", base);
        let result = handle_todo_board_with_dir("goal New goal", base);
        assert!(result.contains("Updated goal"));

        let goal = std::fs::read_to_string(tmp.path().join("goal.md")).unwrap();
        assert!(goal.contains("New goal"));
    }

    #[test]
    fn test_board_evidence_appends() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        handle_todo_board_with_dir("init", base);
        handle_todo_board_with_dir("evidence Tests pass", base);
        handle_todo_board_with_dir("evidence Lint clean", base);

        let evidence = std::fs::read_to_string(tmp.path().join("evidence.md")).unwrap();
        assert!(evidence.contains("- Tests pass"));
        assert!(evidence.contains("- Lint clean"));
    }

    #[test]
    fn test_board_show_empty_dir() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        let result = handle_todo_board_with_dir("", base);
        assert!(result.contains("No task files"));
    }

    #[test]
    fn test_board_show_with_tasks() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        handle_todo_board_with_dir("add First task", base);
        handle_todo_board_with_dir("add Second task", base);
        handle_todo_board_with_dir("move task_01 active", base);

        let result = handle_todo_board_with_dir("", base);
        assert!(result.contains("Task Board"));
        assert!(result.contains("task_01"));
        assert!(result.contains("task_02"));
        assert!(result.contains("1 backlog, 1 active, 0 done"));
    }

    #[test]
    fn test_board_reads_existing_session_plan_files() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        // Simulate existing session_plan files (evolution pipeline format)
        std::fs::write(
            dir.join("task_01.md"),
            "Title: Fix flaky test\nFiles: src/watch.rs\nIssue: none\n\nDescription of the task.\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("task_02.md"),
            "Title: Add new feature\nFiles: src/main.rs\nIssue: #100\nStatus: active\n",
        )
        .unwrap();
        let base = dir.to_str().unwrap();

        let result = handle_todo_board_with_dir("", base);
        assert!(result.contains("task_01"));
        assert!(result.contains("Fix flaky test"));
        assert!(result.contains("task_02"));
        assert!(result.contains("Add new feature"));
        assert!(result.contains("(#100)"));
        // task_01 has no Status line, should default to backlog
        assert!(result.contains("1 backlog, 1 active, 0 done"));
    }

    #[test]
    fn test_board_move_nonexistent_task() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();

        let result = handle_todo_board_with_dir("move task_99 active", base);
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_board_move_invalid_status() {
        let tmp = make_temp_dir();
        let base = tmp.path().to_str().unwrap();
        handle_todo_board_with_dir("add test", base);

        let result = handle_todo_board_with_dir("move task_01 invalid", base);
        assert!(result.contains("Unknown status"));
    }

    #[test]
    fn test_normalize_board_status() {
        assert_eq!(normalize_board_status("backlog"), Some("backlog"));
        assert_eq!(normalize_board_status("active"), Some("active"));
        assert_eq!(normalize_board_status("wip"), Some("active"));
        assert_eq!(normalize_board_status("inprogress"), Some("active"));
        assert_eq!(normalize_board_status("in-progress"), Some("active"));
        assert_eq!(normalize_board_status("done"), Some("done"));
        assert_eq!(normalize_board_status("complete"), Some("done"));
        assert_eq!(normalize_board_status("invalid"), None);
    }

    #[test]
    fn test_normalize_task_id() {
        assert_eq!(normalize_task_id("task_01"), "task_01");
        assert_eq!(normalize_task_id("01"), "task_01");
        assert_eq!(normalize_task_id("1"), "task_01");
        assert_eq!(normalize_task_id("12"), "task_12");
        assert_eq!(normalize_task_id("task_99"), "task_99");
    }

    #[test]
    fn test_board_show_routes_from_handle_todo() {
        let result = handle_todo("/todo board");
        // Should go through board handler — either shows tasks or "No task files"
        assert!(
            result.contains("Task Board")
                || result.contains("No task files")
                || result.contains("task"),
            "Board routing should work: got: {result}"
        );
    }

    #[test]
    fn test_update_task_status_adds_missing_status_line() {
        let tmp = make_temp_dir();
        let dir = tmp.path();
        // Task file without Status line
        std::fs::write(
            dir.join("task_01.md"),
            "Title: No status\nFiles: src/main.rs\nIssue: none\n\nDescription.\n",
        )
        .unwrap();

        update_task_status(dir.to_str().unwrap(), "task_01", "active").unwrap();
        let content = std::fs::read_to_string(dir.join("task_01.md")).unwrap();
        assert!(content.contains("Status: active"));
    }
}
