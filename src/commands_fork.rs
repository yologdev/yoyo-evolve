//! Fork and checkpoint command handlers: /fork, /checkpoint, clear_confirmation.
//!
//! Conversation branching (/fork) lets you create named branches from
//! the current conversation, switch between them, and manage them.
//! Checkpoints (/checkpoint) snapshot file state at a point in time
//! for later restore or diff.

use crate::format::*;
use crate::session::SessionChanges;

use std::collections::HashMap;
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

// ---------------------------------------------------------------------------
// Conversation branching — /fork
// ---------------------------------------------------------------------------

/// A named branch of the conversation.
struct ConversationBranch {
    name: String,
    messages_json: String,
    created_at: String, // HH:MM:SS UTC
    message_count: usize,
}

/// Global branch store: named conversation branches + current branch tracker.
struct BranchStore {
    branches: HashMap<String, ConversationBranch>,
    current: Option<String>,
}

static BRANCH_STORE: RwLock<Option<BranchStore>> = RwLock::new(None);

/// Acquire a mutable reference to the branch store, initializing it if needed.
fn with_branch_store_mut<R>(f: impl FnOnce(&mut BranchStore) -> R) -> R {
    let mut guard = rw_write_or_recover(&BRANCH_STORE);
    let store = guard.get_or_insert_with(|| BranchStore {
        branches: HashMap::new(),
        current: None,
    });
    f(store)
}

/// Acquire a read reference to the branch store.
fn with_branch_store<R>(f: impl FnOnce(&BranchStore) -> R) -> R {
    let guard = rw_read_or_recover(&BRANCH_STORE);
    match guard.as_ref() {
        Some(store) => f(store),
        None => {
            // Store not initialized — treat as empty
            let empty = BranchStore {
                branches: HashMap::new(),
                current: None,
            };
            f(&empty)
        }
    }
}

/// Subcommands for `/fork <Tab>` completion.
pub const FORK_SUBCOMMANDS: &[&str] = &["switch", "list", "delete", "rename"];

/// Return the name of the current conversation branch, if any.
#[allow(dead_code)]
pub fn current_branch_name() -> Option<String> {
    with_branch_store(|store| store.current.clone())
}

/// Generate a UTC HH:MM:SS timestamp.
fn utc_timestamp() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Parse a `/fork` subcommand from user input.
///
/// Returns `(subcommand, rest)` where subcommand is one of:
/// `"create"`, `"switch"`, `"list"`, `"delete"`, `"rename"`, or `"help"`.
pub fn parse_fork_subcommand(input: &str) -> (&str, &str) {
    let rest = input.strip_prefix("/fork").unwrap_or("").trim();
    if rest.is_empty() {
        return ("help", "");
    }
    if rest == "list" {
        return ("list", "");
    }
    if let Some(arg) = rest.strip_prefix("switch") {
        return ("switch", arg.trim());
    }
    if let Some(arg) = rest.strip_prefix("delete") {
        return ("delete", arg.trim());
    }
    if let Some(arg) = rest.strip_prefix("rename") {
        return ("rename", arg.trim());
    }
    // Bare name → create
    ("create", rest)
}

/// Create a new named branch from the current conversation.
pub fn handle_fork_create(agent: &mut Agent, name: &str) -> String {
    if name.is_empty() {
        return format!("{DIM}  usage: /fork <name> — create a named branch{RESET}\n");
    }

    let messages_json = match agent.save_messages() {
        Ok(json) => json,
        Err(e) => return format!("{RED}  failed to save conversation: {e}{RESET}\n"),
    };

    let msg_count = agent.messages().len();
    let timestamp = utc_timestamp();

    with_branch_store_mut(|store| {
        // If we're currently on a branch, auto-save it before creating the new one
        if let Some(cur) = store.current.clone() {
            if let Some(branch) = store.branches.get_mut(&cur) {
                branch.messages_json = messages_json.clone();
                branch.message_count = msg_count;
            }
        }

        let branch = ConversationBranch {
            name: name.to_string(),
            messages_json,
            created_at: timestamp,
            message_count: msg_count,
        };

        let overwritten = store.branches.insert(name.to_string(), branch).is_some();
        store.current = Some(name.to_string());

        let verb = if overwritten { "updated" } else { "created" };
        format!(
            "{GREEN}  ✓ branch {verb}: \"{name}\" ({msg_count} messages) — now on this branch{RESET}\n"
        )
    })
}

/// Switch to an existing named branch.
pub fn handle_fork_switch(agent: &mut Agent, name: &str) -> String {
    if name.is_empty() {
        return format!("{DIM}  usage: /fork switch <name>{RESET}\n");
    }

    // First, save the current conversation to the current branch (if any)
    let current_json = match agent.save_messages() {
        Ok(json) => json,
        Err(e) => return format!("{RED}  failed to save current conversation: {e}{RESET}\n"),
    };
    let current_count = agent.messages().len();

    let target_json = with_branch_store_mut(|store| {
        // Auto-save current branch
        if let Some(cur) = store.current.clone() {
            if let Some(branch) = store.branches.get_mut(&cur) {
                branch.messages_json = current_json;
                branch.message_count = current_count;
            }
        }

        // Look up the target branch
        match store.branches.get(name) {
            Some(b) => {
                let json = b.messages_json.clone();
                store.current = Some(name.to_string());
                Ok(json)
            }
            None => {
                let available: Vec<String> = store.branches.keys().cloned().collect();
                Err(available)
            }
        }
    });

    match target_json {
        Ok(json) => match agent.restore_messages(&json) {
            Ok(_) => format!(
                "{GREEN}  ✓ switched to branch \"{name}\" ({} messages){RESET}\n",
                agent.messages().len()
            ),
            Err(e) => format!("{RED}  failed to restore branch: {e}{RESET}\n"),
        },
        Err(available) => {
            if available.is_empty() {
                format!("{RED}  branch \"{name}\" not found (no branches exist){RESET}\n")
            } else {
                format!(
                    "{RED}  branch \"{name}\" not found. Available: {}{RESET}\n",
                    available.join(", ")
                )
            }
        }
    }
}

/// List all conversation branches.
pub fn handle_fork_list() -> String {
    with_branch_store(|store| {
        if store.branches.is_empty() {
            return format!("{DIM}  (no branches — use /fork <name> to create one){RESET}\n");
        }

        let mut out = String::new();
        out.push_str(&format!(
            "{DIM}  Conversation branches ({} total):\n",
            store.branches.len()
        ));

        // Sort by name for stable output
        let mut names: Vec<&String> = store.branches.keys().collect();
        names.sort();

        for name in names {
            let branch = &store.branches[name];
            let marker = if store.current.as_deref() == Some(name.as_str()) {
                format!("{GREEN}* ")
            } else {
                "  ".to_string()
            };
            out.push_str(&format!(
                "  {marker}{}{DIM} ({} messages) [{}]{RESET}\n",
                branch.name, branch.message_count, branch.created_at
            ));
        }
        out
    })
}

/// Delete a named branch.
pub fn handle_fork_delete(name: &str) -> String {
    if name.is_empty() {
        return format!("{DIM}  usage: /fork delete <name>{RESET}\n");
    }

    with_branch_store_mut(|store| {
        if store.current.as_deref() == Some(name) {
            return format!(
                "{RED}  cannot delete the current branch \"{name}\" — switch to another first{RESET}\n"
            );
        }

        match store.branches.remove(name) {
            Some(_) => format!("{GREEN}  ✓ deleted branch \"{name}\"{RESET}\n"),
            None => format!("{RED}  branch \"{name}\" not found{RESET}\n"),
        }
    })
}

/// Rename a branch.
pub fn handle_fork_rename(args: &str) -> String {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() != 2 {
        return format!("{DIM}  usage: /fork rename <old> <new>{RESET}\n");
    }
    let old = parts[0];
    let new = parts[1];

    with_branch_store_mut(|store| {
        if store.branches.contains_key(new) {
            return format!("{RED}  branch \"{new}\" already exists{RESET}\n");
        }

        match store.branches.remove(old) {
            Some(mut branch) => {
                branch.name = new.to_string();
                // Update current pointer if we're renaming the current branch
                if store.current.as_deref() == Some(old) {
                    store.current = Some(new.to_string());
                }
                store.branches.insert(new.to_string(), branch);
                format!("{GREEN}  ✓ renamed branch \"{old}\" → \"{new}\"{RESET}\n")
            }
            None => format!("{RED}  branch \"{old}\" not found{RESET}\n"),
        }
    })
}

/// Show fork help text.
fn fork_help() -> String {
    format!(
        "{DIM}  /fork — conversation branching\n\n\
         \x20 /fork <name>             Create/update a named branch from current conversation\n\
         \x20 /fork switch <name>      Switch to a named branch (auto-saves current)\n\
         \x20 /fork list               List all branches\n\
         \x20 /fork delete <name>      Delete a branch (cannot delete current)\n\
         \x20 /fork rename <old> <new> Rename a branch\n\n\
         \x20 Like git branches for your conversation. Explore different\n\
         \x20 directions and switch between them freely.{RESET}\n"
    )
}

/// Dispatch a `/fork` command.
pub fn handle_fork(agent: &mut Agent, input: &str) -> String {
    let (subcmd, arg) = parse_fork_subcommand(input);
    match subcmd {
        "create" => handle_fork_create(agent, arg),
        "switch" => handle_fork_switch(agent, arg),
        "list" => handle_fork_list(),
        "delete" => handle_fork_delete(arg),
        "rename" => handle_fork_rename(arg),
        "help" => fork_help(),
        _ => fork_help(),
    }
}

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
    use crate::commands::KNOWN_COMMANDS;
    use crate::session::ChangeKind;

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

    // --- Fork tests ---

    #[test]
    fn test_fork_list_empty() {
        // Clear state for test isolation
        {
            let mut guard = rw_write_or_recover(&BRANCH_STORE);
            *guard = None;
        }
        let result = handle_fork_list();
        assert!(result.contains("no branches"));
    }

    #[test]
    fn test_fork_delete_nonexistent() {
        {
            let mut guard = rw_write_or_recover(&BRANCH_STORE);
            *guard = None;
        }
        let result = handle_fork_delete("nope");
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_fork_rename_nonexistent() {
        {
            let mut guard = rw_write_or_recover(&BRANCH_STORE);
            *guard = None;
        }
        let result = handle_fork_rename("old new");
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_fork_parse_subcommands() {
        assert_eq!(parse_fork_subcommand("/fork"), ("help", ""));
        assert_eq!(parse_fork_subcommand("/fork list"), ("list", ""));
        assert_eq!(
            parse_fork_subcommand("/fork switch main"),
            ("switch", "main")
        );
        assert_eq!(parse_fork_subcommand("/fork delete old"), ("delete", "old"));
        assert_eq!(
            parse_fork_subcommand("/fork rename old new"),
            ("rename", "old new")
        );
        assert_eq!(
            parse_fork_subcommand("/fork my-branch"),
            ("create", "my-branch")
        );
    }

    #[test]
    fn test_fork_delete_empty_name() {
        let result = handle_fork_delete("");
        assert!(result.contains("usage:"));
    }

    #[test]
    fn test_fork_rename_wrong_arg_count() {
        let result = handle_fork_rename("only-one");
        assert!(result.contains("usage:"));
        let result = handle_fork_rename("");
        assert!(result.contains("usage:"));
    }

    #[test]
    fn test_fork_help_on_bare_command() {
        use yoagent::agent::Agent;
        use yoagent::provider::AnthropicProvider;

        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        let mut agent = agent;
        let result = handle_fork(&mut agent, "/fork");
        assert!(result.contains("/fork"));
        assert!(result.contains("switch"));
        assert!(result.contains("list"));
    }
}
