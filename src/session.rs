//! Session tracking types — file changes, turn snapshots, and undo history.
//!
//! Extracted from `prompt.rs` (Day 54) to keep session-state types separate
//! from prompt execution logic.

use crate::format::pluralize;
use crate::sync_util::lock_or_recover;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tracks files modified during a session via write_file and edit_file tool calls.
/// Thread-safe via Arc<Mutex<...>> so it can be shared across async tasks.
#[derive(Debug, Clone)]
pub struct SessionChanges {
    inner: Arc<Mutex<Vec<FileChange>>>,
}

/// A single file modification event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

/// The kind of file modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Write,
    Edit,
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeKind::Write => write!(f, "write"),
            ChangeKind::Edit => write!(f, "edit"),
        }
    }
}

impl SessionChanges {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record a file modification.
    pub fn record(&self, path: &str, kind: ChangeKind) {
        let mut changes = lock_or_recover(&self.inner);
        // Update existing entry if same path, or add new
        if let Some(existing) = changes.iter_mut().find(|c| c.path == path) {
            existing.kind = kind;
        } else {
            changes.push(FileChange {
                path: path.to_string(),
                kind,
            });
        }
    }

    /// Get a snapshot of all changes, in order of first modification.
    pub fn snapshot(&self) -> Vec<FileChange> {
        lock_or_recover(&self.inner).clone()
    }

    /// Clear all tracked changes.
    pub fn clear(&self) {
        lock_or_recover(&self.inner).clear();
    }

    /// Return a JSON summary of file changes for `--json` output.
    ///
    /// Format:
    /// ```json
    /// { "files_changed": 2, "changes": [
    ///     { "path": "src/main.rs", "kind": "write" },
    ///     { "path": "src/cli.rs", "kind": "edit" }
    /// ]}
    /// ```
    pub fn to_json_summary(&self) -> serde_json::Value {
        let changes = lock_or_recover(&self.inner);
        let entries: Vec<serde_json::Value> = changes
            .iter()
            .map(|c| {
                serde_json::json!({
                    "path": c.path,
                    "kind": c.kind.to_string(),
                })
            })
            .collect();
        serde_json::json!({
            "files_changed": entries.len(),
            "changes": entries,
        })
    }
}

#[cfg(test)]
impl SessionChanges {
    /// Return the number of unique files changed.
    pub fn len(&self) -> usize {
        lock_or_recover(&self.inner).len()
    }

    /// Return true if no files have been changed.
    pub fn is_empty(&self) -> bool {
        lock_or_recover(&self.inner).is_empty()
    }
}

/// A snapshot of file state before a single agent turn.
///
/// Stores the original content of files that existed before the turn,
/// and tracks paths of files that were newly created during the turn.
/// Used by `/undo` to revert only the most recent turn's changes.
#[derive(Debug, Clone)]
pub struct TurnSnapshot {
    /// Files that existed before the turn: path → original content.
    pub originals: HashMap<String, String>,
    /// Files that were created during the turn (didn't exist before).
    pub created: Vec<String>,
}

impl TurnSnapshot {
    /// Create a new empty snapshot.
    pub fn new() -> Self {
        Self {
            originals: HashMap::new(),
            created: Vec::new(),
        }
    }

    /// Snapshot the current content of a file. If the file exists, stores its
    /// content in `originals`. Does nothing if already snapshotted.
    pub fn snapshot_file(&mut self, path: &str) {
        if self.originals.contains_key(path) {
            return; // Already snapshotted
        }
        if let Ok(content) = std::fs::read_to_string(path) {
            self.originals.insert(path.to_string(), content);
        }
        // If file doesn't exist, we'll track it as created when we see it appear
    }

    /// Record a file as newly created during this turn.
    /// Only records if not already in originals (i.e., it truly didn't exist before).
    pub fn record_created(&mut self, path: &str) {
        if !self.originals.contains_key(path) && !self.created.contains(&path.to_string()) {
            self.created.push(path.to_string());
        }
    }

    /// Return true if no files were affected.
    pub fn is_empty(&self) -> bool {
        self.originals.is_empty() && self.created.is_empty()
    }

    /// Restore all files to their pre-turn state:
    /// - Overwrite modified files with their original content
    /// - Delete files that were created during the turn
    ///
    /// Returns a list of actions taken (for display).
    pub fn restore(&self) -> Vec<String> {
        let mut actions = Vec::new();

        // Restore modified files
        for (path, content) in &self.originals {
            if std::fs::write(path, content).is_ok() {
                actions.push(format!("restored {path}"));
            } else {
                actions.push(format!("failed to restore {path}"));
            }
        }

        // Delete newly created files
        for path in &self.created {
            if std::fs::remove_file(path).is_ok() {
                actions.push(format!("deleted {path}"));
            } else {
                actions.push(format!("failed to delete {path}"));
            }
        }

        actions
    }
}

#[cfg(test)]
impl TurnSnapshot {
    /// Return the number of files affected (modified + created).
    pub fn file_count(&self) -> usize {
        self.originals.len() + self.created.len()
    }
}

/// A stack of turn snapshots for multi-level undo.
///
/// Each completed agent turn pushes a snapshot. `/undo` pops the most recent.
/// `/undo N` pops the last N turns.
#[derive(Debug, Clone)]
pub struct TurnHistory {
    turns: Vec<TurnSnapshot>,
}

impl TurnHistory {
    /// Create a new empty history.
    pub fn new() -> Self {
        Self { turns: Vec::new() }
    }

    /// Push a completed turn's snapshot onto the stack.
    /// Skips empty snapshots (turns that didn't modify any files).
    pub fn push(&mut self, snapshot: TurnSnapshot) {
        if !snapshot.is_empty() {
            self.turns.push(snapshot);
        }
    }

    /// Return the number of undoable turns.
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Return true if there are no undoable turns.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Undo the last N turns by popping and restoring each.
    /// Returns a list of all actions taken.
    pub fn undo_last(&mut self, n: usize) -> Vec<String> {
        let mut all_actions = Vec::new();
        let count = n.min(self.turns.len());
        for _ in 0..count {
            if let Some(snapshot) = self.turns.pop() {
                all_actions.extend(snapshot.restore());
            }
        }
        all_actions
    }

    /// Clear the entire history (used after /clear or /undo --all).
    pub fn clear(&mut self) {
        self.turns.clear();
    }
}

#[cfg(test)]
impl TurnHistory {
    /// Pop the most recent turn snapshot.
    pub fn pop(&mut self) -> Option<TurnSnapshot> {
        self.turns.pop()
    }
}

/// Format a compact, one-line summary of files changed in the current turn only.
///
/// `before` is the list of file paths that were already tracked before this turn.
/// Only files in `changes` that are NOT in `before` are shown.
///
/// For 1–3 files: inline list like `Files changed: ✏ src/main.rs, 🔧 src/cli.rs`
/// For 4+ files: `Files changed (6): ✏ src/main.rs, 🔧 src/cli.rs, ✏ src/lib.rs, …`
pub fn format_turn_changes(before: &[String], changes: &SessionChanges) -> String {
    let snapshot = changes.snapshot();
    // Filter to only files NOT in the before list
    let new_changes: Vec<&FileChange> = snapshot
        .iter()
        .filter(|c| !before.contains(&c.path))
        .collect();
    if new_changes.is_empty() {
        return String::new();
    }

    let icon = |kind: ChangeKind| match kind {
        ChangeKind::Write => "✏",
        ChangeKind::Edit => "🔧",
    };

    let total = new_changes.len();
    let show_count = if total > 3 { 3 } else { total };
    let file_parts: Vec<String> = new_changes[..show_count]
        .iter()
        .map(|c| format!("{} {}", icon(c.kind), c.path))
        .collect();
    let mut line = String::new();
    if total <= 3 {
        line.push_str(&format!("  Files changed: {}", file_parts.join(", ")));
    } else {
        line.push_str(&format!(
            "  Files changed ({}): {}, …",
            total,
            file_parts.join(", ")
        ));
    }
    line
}

/// Format a human-readable summary of session changes.
pub fn format_changes(changes: &SessionChanges) -> String {
    let snapshot = changes.snapshot();
    if snapshot.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!(
        "  {} {} modified this session:\n",
        snapshot.len(),
        pluralize(snapshot.len(), "file", "files")
    ));
    for change in &snapshot {
        let icon = match change.kind {
            ChangeKind::Write => "✏",
            ChangeKind::Edit => "🔧",
        };
        out.push_str(&format!("    {icon} {} ({})\n", change.path, change.kind));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SessionChanges tests ---

    #[test]
    fn test_session_changes_new_is_empty() {
        let changes = SessionChanges::new();
        assert!(changes.is_empty());
        assert_eq!(changes.len(), 0);
        assert!(changes.snapshot().is_empty());
    }

    #[test]
    fn test_session_changes_record_write() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        assert_eq!(changes.len(), 1);
        assert!(!changes.is_empty());
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].path, "src/main.rs");
        assert_eq!(snapshot[0].kind, ChangeKind::Write);
    }

    #[test]
    fn test_session_changes_record_edit() {
        let changes = SessionChanges::new();
        changes.record("src/cli.rs", ChangeKind::Edit);
        assert_eq!(changes.len(), 1);
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].path, "src/cli.rs");
        assert_eq!(snapshot[0].kind, ChangeKind::Edit);
    }

    #[test]
    fn test_session_changes_deduplicates_same_path() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/main.rs", ChangeKind::Edit);
        // Should still be 1 entry, updated to Edit
        assert_eq!(changes.len(), 1);
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].kind, ChangeKind::Edit);
    }

    #[test]
    fn test_session_changes_multiple_files() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        changes.record("README.md", ChangeKind::Write);
        assert_eq!(changes.len(), 3);
        let snapshot = changes.snapshot();
        assert_eq!(snapshot[0].path, "src/main.rs");
        assert_eq!(snapshot[1].path, "src/cli.rs");
        assert_eq!(snapshot[2].path, "README.md");
    }

    #[test]
    fn test_session_changes_clear() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        assert_eq!(changes.len(), 2);
        changes.clear();
        assert!(changes.is_empty());
        assert_eq!(changes.len(), 0);
    }

    #[test]
    fn test_session_changes_clone_is_independent() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        let cloned = changes.clone();
        // They share the same inner Arc, so they should be linked
        changes.record("src/cli.rs", ChangeKind::Edit);
        assert_eq!(cloned.len(), 2);
    }

    #[test]
    fn test_change_kind_display() {
        assert_eq!(format!("{}", ChangeKind::Write), "write");
        assert_eq!(format!("{}", ChangeKind::Edit), "edit");
    }

    #[test]
    fn test_format_changes_empty() {
        let changes = SessionChanges::new();
        let output = format_changes(&changes);
        assert!(output.is_empty());
    }

    #[test]
    fn test_format_changes_single_write() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        let output = format_changes(&changes);
        assert!(output.contains("1 file modified"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("write"));
        assert!(output.contains("✏"));
    }

    #[test]
    fn test_format_changes_multiple_files() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        let output = format_changes(&changes);
        assert!(output.contains("2 files modified"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/cli.rs"));
        assert!(output.contains("write"));
        assert!(output.contains("edit"));
        assert!(output.contains("🔧"));
    }

    #[test]
    fn test_session_changes_shared_across_content_prompts() {
        // Verifies that SessionChanges can be used across multiple prompt styles.
        // When @file mention prompts use the same SessionChanges as regular prompts,
        // all changes should be tracked together.
        let changes = SessionChanges::new();

        // Simulate a regular prompt recording a write
        changes.record("src/main.rs", ChangeKind::Write);

        // Simulate an @file mention prompt recording an edit
        changes.record("src/cli.rs", ChangeKind::Edit);

        // Both should be visible in the snapshot
        let snapshot = changes.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].path, "src/main.rs");
        assert_eq!(snapshot[0].kind, ChangeKind::Write);
        assert_eq!(snapshot[1].path, "src/cli.rs");
        assert_eq!(snapshot[1].kind, ChangeKind::Edit);

        // format_changes should show both
        let output = format_changes(&changes);
        assert!(output.contains("2 files"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/cli.rs"));
    }

    // --- TurnSnapshot tests ---

    #[test]
    fn test_turn_snapshot_new_is_empty() {
        let snap = TurnSnapshot::new();
        assert!(snap.is_empty());
        assert_eq!(snap.file_count(), 0);
    }

    #[test]
    fn test_turn_snapshot_save_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "original content").unwrap();
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);

        assert!(!snap.is_empty());
        assert_eq!(snap.file_count(), 1);
        assert_eq!(snap.originals.get(path_str).unwrap(), "original content");

        // Simulate agent modifying the file
        std::fs::write(&path, "modified content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "modified content");

        // Restore should revert to original
        let actions = snap.restore();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("restored"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original content");
    }

    #[test]
    fn test_turn_snapshot_created_files_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new_file.txt");
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        // File doesn't exist yet — record as created
        snap.record_created(path_str);

        assert!(!snap.is_empty());
        assert_eq!(snap.file_count(), 1);

        // Simulate agent creating the file
        std::fs::write(&path, "new content").unwrap();
        assert!(path.exists());

        // Restore should delete it
        let actions = snap.restore();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("deleted"));
        assert!(!path.exists());
    }

    #[test]
    fn test_turn_snapshot_no_duplicate_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "v1").unwrap();
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);

        // Modify file, then snapshot again — should keep original
        std::fs::write(&path, "v2").unwrap();
        snap.snapshot_file(path_str);

        assert_eq!(snap.originals.get(path_str).unwrap(), "v1");
    }

    #[test]
    fn test_turn_snapshot_nonexistent_file() {
        let mut snap = TurnSnapshot::new();
        snap.snapshot_file("/nonexistent/path/to/file.txt");
        // Should not add to originals since file doesn't exist
        assert!(snap.originals.is_empty());
    }

    #[test]
    fn test_turn_snapshot_created_not_duplicated() {
        let mut snap = TurnSnapshot::new();
        snap.record_created("new.txt");
        snap.record_created("new.txt");
        assert_eq!(snap.created.len(), 1);
    }

    #[test]
    fn test_turn_snapshot_created_ignores_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "content").unwrap();
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);
        // Should not add to created since it was already snapshotted
        snap.record_created(path_str);
        assert!(snap.created.is_empty());
    }

    // --- TurnHistory tests ---

    #[test]
    fn test_turn_history_new_is_empty() {
        let hist = TurnHistory::new();
        assert!(hist.is_empty());
        assert_eq!(hist.len(), 0);
    }

    #[test]
    fn test_turn_history_push_pop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "original").unwrap();

        let mut hist = TurnHistory::new();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());
        hist.push(snap);

        assert_eq!(hist.len(), 1);

        let popped = hist.pop();
        assert!(popped.is_some());
        assert_eq!(hist.len(), 0);
    }

    #[test]
    fn test_turn_history_skips_empty_snapshots() {
        let mut hist = TurnHistory::new();
        hist.push(TurnSnapshot::new()); // empty — should be skipped
        assert!(hist.is_empty());
    }

    #[test]
    fn test_turn_history_undo_last_n() {
        let dir = tempfile::tempdir().unwrap();

        // Turn 1: modify a.txt
        let path_a = dir.path().join("a.txt");
        std::fs::write(&path_a, "a_original").unwrap();
        let mut snap1 = TurnSnapshot::new();
        snap1.snapshot_file(path_a.to_str().unwrap());

        // Turn 2: modify b.txt
        let path_b = dir.path().join("b.txt");
        std::fs::write(&path_b, "b_original").unwrap();
        let mut snap2 = TurnSnapshot::new();
        snap2.snapshot_file(path_b.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap1);
        hist.push(snap2);
        assert_eq!(hist.len(), 2);

        // Simulate modifications
        std::fs::write(&path_a, "a_modified").unwrap();
        std::fs::write(&path_b, "b_modified").unwrap();

        // Undo last 1 — only b.txt should be restored
        let actions = hist.undo_last(1);
        assert!(!actions.is_empty());
        assert_eq!(std::fs::read_to_string(&path_b).unwrap(), "b_original");
        assert_eq!(std::fs::read_to_string(&path_a).unwrap(), "a_modified");
        assert_eq!(hist.len(), 1);

        // Undo last 1 — now a.txt should be restored
        let actions = hist.undo_last(1);
        assert!(!actions.is_empty());
        assert_eq!(std::fs::read_to_string(&path_a).unwrap(), "a_original");
        assert!(hist.is_empty());
    }

    #[test]
    fn test_turn_history_undo_more_than_available() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.txt");
        std::fs::write(&path, "orig").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap);

        // Undo 5 when only 1 exists — should undo 1 without panic
        std::fs::write(&path, "changed").unwrap();
        let actions = hist.undo_last(5);
        assert!(!actions.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "orig");
        assert!(hist.is_empty());
    }

    #[test]
    fn test_turn_history_clear() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.txt");
        std::fs::write(&path, "content").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap);
        assert_eq!(hist.len(), 1);

        hist.clear();
        assert!(hist.is_empty());
    }

    #[test]
    fn test_to_json_summary_empty() {
        let changes = SessionChanges::new();
        let summary = changes.to_json_summary();
        assert_eq!(summary["files_changed"], 0);
        assert!(summary["changes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_to_json_summary_mixed_changes() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        changes.record("src/new.rs", ChangeKind::Write);

        let summary = changes.to_json_summary();
        assert_eq!(summary["files_changed"], 3);

        let arr = summary["changes"].as_array().unwrap();
        assert_eq!(arr.len(), 3);

        assert_eq!(arr[0]["path"], "src/main.rs");
        assert_eq!(arr[0]["kind"], "write");

        assert_eq!(arr[1]["path"], "src/cli.rs");
        assert_eq!(arr[1]["kind"], "edit");

        assert_eq!(arr[2]["path"], "src/new.rs");
        assert_eq!(arr[2]["kind"], "write");
    }

    #[test]
    fn test_to_json_summary_is_valid_json() {
        let changes = SessionChanges::new();
        changes.record("a.rs", ChangeKind::Write);
        let summary = changes.to_json_summary();
        // Round-trip through serde to verify it's valid JSON
        let serialized = serde_json::to_string(&summary).unwrap();
        let _: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    }

    #[test]
    fn test_to_json_summary_deduplicates_paths() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/main.rs", ChangeKind::Edit); // overwrites

        let summary = changes.to_json_summary();
        assert_eq!(summary["files_changed"], 1);

        let arr = summary["changes"].as_array().unwrap();
        assert_eq!(arr[0]["path"], "src/main.rs");
        assert_eq!(arr[0]["kind"], "edit"); // latest kind wins
    }

    // --- Additional SessionChanges tests ---

    #[test]
    fn test_session_changes_record_after_clear() {
        let changes = SessionChanges::new();
        changes.record("a.rs", ChangeKind::Write);
        changes.clear();
        assert!(changes.is_empty());

        // Can record again after clearing
        changes.record("b.rs", ChangeKind::Edit);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes.snapshot()[0].path, "b.rs");
    }

    #[test]
    fn test_session_changes_snapshot_preserves_insertion_order() {
        let changes = SessionChanges::new();
        changes.record("z.rs", ChangeKind::Write);
        changes.record("a.rs", ChangeKind::Edit);
        changes.record("m.rs", ChangeKind::Write);

        let snap = changes.snapshot();
        assert_eq!(snap[0].path, "z.rs");
        assert_eq!(snap[1].path, "a.rs");
        assert_eq!(snap[2].path, "m.rs");
    }

    #[test]
    fn test_session_changes_update_kind_preserves_order() {
        let changes = SessionChanges::new();
        changes.record("first.rs", ChangeKind::Write);
        changes.record("second.rs", ChangeKind::Write);
        // Update first file — should stay in position 0
        changes.record("first.rs", ChangeKind::Edit);

        let snap = changes.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].path, "first.rs");
        assert_eq!(snap[0].kind, ChangeKind::Edit);
        assert_eq!(snap[1].path, "second.rs");
    }

    #[test]
    fn test_session_changes_thread_safety() {
        use std::thread;

        let changes = SessionChanges::new();
        let mut handles = Vec::new();

        for i in 0..10 {
            let c = changes.clone();
            handles.push(thread::spawn(move || {
                c.record(&format!("file_{i}.rs"), ChangeKind::Write);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(changes.len(), 10);
    }

    #[test]
    fn test_session_changes_concurrent_record_and_snapshot() {
        use std::thread;

        let changes = SessionChanges::new();
        changes.record("existing.rs", ChangeKind::Write);

        let c1 = changes.clone();
        let c2 = changes.clone();

        let writer = thread::spawn(move || {
            for i in 0..5 {
                c1.record(&format!("new_{i}.rs"), ChangeKind::Edit);
            }
        });

        let reader = thread::spawn(move || {
            // Snapshot should never panic, even during concurrent writes
            for _ in 0..10 {
                let _ = c2.snapshot();
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();

        // All 6 files should be recorded
        assert_eq!(changes.len(), 6);
    }

    #[test]
    fn test_session_changes_many_files() {
        let changes = SessionChanges::new();
        for i in 0..100 {
            changes.record(&format!("file_{i}.rs"), ChangeKind::Write);
        }
        assert_eq!(changes.len(), 100);

        let snap = changes.snapshot();
        assert_eq!(snap.len(), 100);
        // Verify first and last
        assert_eq!(snap[0].path, "file_0.rs");
        assert_eq!(snap[99].path, "file_99.rs");
    }

    #[test]
    fn test_session_changes_json_summary_single_file() {
        let changes = SessionChanges::new();
        changes.record("only.rs", ChangeKind::Edit);

        let summary = changes.to_json_summary();
        assert_eq!(summary["files_changed"], 1);

        let arr = summary["changes"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["path"], "only.rs");
        assert_eq!(arr[0]["kind"], "edit");
    }

    #[test]
    fn test_session_changes_snapshot_is_independent_copy() {
        let changes = SessionChanges::new();
        changes.record("a.rs", ChangeKind::Write);
        let snap = changes.snapshot();

        // Modify after snapshot
        changes.record("b.rs", ChangeKind::Edit);

        // Original snapshot should be unchanged
        assert_eq!(snap.len(), 1);
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn test_session_changes_clear_then_json_empty() {
        let changes = SessionChanges::new();
        changes.record("a.rs", ChangeKind::Write);
        changes.clear();

        let summary = changes.to_json_summary();
        assert_eq!(summary["files_changed"], 0);
        assert!(summary["changes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_file_change_equality() {
        let a = FileChange {
            path: "src/main.rs".to_string(),
            kind: ChangeKind::Write,
        };
        let b = FileChange {
            path: "src/main.rs".to_string(),
            kind: ChangeKind::Write,
        };
        let c = FileChange {
            path: "src/main.rs".to_string(),
            kind: ChangeKind::Edit,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_change_kind_copy() {
        let kind = ChangeKind::Write;
        let copied = kind; // Copy trait
        assert_eq!(kind, copied);
    }

    // --- Additional TurnSnapshot tests ---

    #[test]
    fn test_turn_snapshot_multiple_files_restore() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("one.txt");
        let p2 = dir.path().join("two.txt");
        std::fs::write(&p1, "orig_one").unwrap();
        std::fs::write(&p2, "orig_two").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(p1.to_str().unwrap());
        snap.snapshot_file(p2.to_str().unwrap());
        assert_eq!(snap.file_count(), 2);

        // Simulate modifications
        std::fs::write(&p1, "changed_one").unwrap();
        std::fs::write(&p2, "changed_two").unwrap();

        let actions = snap.restore();
        assert_eq!(actions.len(), 2);
        assert_eq!(std::fs::read_to_string(&p1).unwrap(), "orig_one");
        assert_eq!(std::fs::read_to_string(&p2).unwrap(), "orig_two");
    }

    #[test]
    fn test_turn_snapshot_mixed_originals_and_created() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("existing.txt");
        let created = dir.path().join("created.txt");
        std::fs::write(&existing, "original content").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(existing.to_str().unwrap());
        snap.record_created(created.to_str().unwrap());

        assert_eq!(snap.file_count(), 2);
        assert!(!snap.is_empty());
        assert_eq!(snap.originals.len(), 1);
        assert_eq!(snap.created.len(), 1);
    }

    #[test]
    fn test_turn_snapshot_mixed_restore() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("existing.txt");
        let created = dir.path().join("created.txt");
        std::fs::write(&existing, "before").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(existing.to_str().unwrap());
        snap.record_created(created.to_str().unwrap());

        // Simulate agent work: modify existing, create new
        std::fs::write(&existing, "after").unwrap();
        std::fs::write(&created, "brand new").unwrap();
        assert!(created.exists());

        let actions = snap.restore();
        assert_eq!(actions.len(), 2);
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "before");
        assert!(!created.exists());
    }

    #[test]
    fn test_turn_snapshot_restore_nonexistent_created_file() {
        // If a file was recorded as created but was already deleted, restore
        // should report failure but not panic
        let mut snap = TurnSnapshot::new();
        snap.record_created("/nonexistent/cannot/delete.txt");

        let actions = snap.restore();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("failed to delete"));
    }

    #[test]
    fn test_turn_snapshot_restore_action_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("msg.txt");
        std::fs::write(&path, "content").unwrap();
        let path_str = path.to_str().unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path_str);

        std::fs::write(&path, "modified").unwrap();

        let actions = snap.restore();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].starts_with("restored "));
        assert!(actions[0].contains(path_str));
    }

    #[test]
    fn test_turn_snapshot_only_originals_is_not_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "data").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());

        assert!(!snap.is_empty());
        assert_eq!(snap.originals.len(), 1);
        assert!(snap.created.is_empty());
    }

    #[test]
    fn test_turn_snapshot_only_created_is_not_empty() {
        let mut snap = TurnSnapshot::new();
        snap.record_created("new.txt");

        assert!(!snap.is_empty());
        assert!(snap.originals.is_empty());
        assert_eq!(snap.created.len(), 1);
    }

    #[test]
    fn test_turn_snapshot_file_count_combined() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.txt");
        let p2 = dir.path().join("b.txt");
        std::fs::write(&p1, "a").unwrap();
        std::fs::write(&p2, "b").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(p1.to_str().unwrap());
        snap.snapshot_file(p2.to_str().unwrap());
        snap.record_created("c.txt");
        snap.record_created("d.txt");
        snap.record_created("e.txt");

        assert_eq!(snap.file_count(), 5);
    }

    #[test]
    fn test_turn_snapshot_restore_preserves_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let tracked = dir.path().join("tracked.txt");
        let untracked = dir.path().join("untracked.txt");
        std::fs::write(&tracked, "original").unwrap();
        std::fs::write(&untracked, "untouched").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(tracked.to_str().unwrap());
        // untracked is NOT snapshotted

        std::fs::write(&tracked, "changed").unwrap();
        std::fs::write(&untracked, "also changed").unwrap();

        snap.restore();

        assert_eq!(std::fs::read_to_string(&tracked).unwrap(), "original");
        // untracked should remain changed — not reverted
        assert_eq!(std::fs::read_to_string(&untracked).unwrap(), "also changed");
    }

    // --- Additional TurnHistory tests ---

    #[test]
    fn test_turn_history_undo_last_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keep.txt");
        std::fs::write(&path, "original").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap);

        std::fs::write(&path, "modified").unwrap();

        // Undo 0 should be a no-op
        let actions = hist.undo_last(0);
        assert!(actions.is_empty());
        assert_eq!(hist.len(), 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "modified");
    }

    #[test]
    fn test_turn_history_pop_empty() {
        let mut hist = TurnHistory::new();
        assert!(hist.pop().is_none());
    }

    #[test]
    fn test_turn_history_multiple_pushes() {
        let dir = tempfile::tempdir().unwrap();

        let mut hist = TurnHistory::new();
        for i in 0..5 {
            let path = dir.path().join(format!("f{i}.txt"));
            std::fs::write(&path, format!("content_{i}")).unwrap();
            let mut snap = TurnSnapshot::new();
            snap.snapshot_file(path.to_str().unwrap());
            hist.push(snap);
        }

        assert_eq!(hist.len(), 5);
    }

    #[test]
    fn test_turn_history_undo_all_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.txt");
        let p2 = dir.path().join("b.txt");
        let p3 = dir.path().join("c.txt");
        std::fs::write(&p1, "a_orig").unwrap();
        std::fs::write(&p2, "b_orig").unwrap();
        std::fs::write(&p3, "c_orig").unwrap();

        let mut snap1 = TurnSnapshot::new();
        snap1.snapshot_file(p1.to_str().unwrap());
        let mut snap2 = TurnSnapshot::new();
        snap2.snapshot_file(p2.to_str().unwrap());
        let mut snap3 = TurnSnapshot::new();
        snap3.snapshot_file(p3.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap1);
        hist.push(snap2);
        hist.push(snap3);

        // Modify all files
        std::fs::write(&p1, "a_mod").unwrap();
        std::fs::write(&p2, "b_mod").unwrap();
        std::fs::write(&p3, "c_mod").unwrap();

        // Undo all 3 at once
        let actions = hist.undo_last(3);
        assert_eq!(actions.len(), 3);
        assert!(hist.is_empty());

        assert_eq!(std::fs::read_to_string(&p1).unwrap(), "a_orig");
        assert_eq!(std::fs::read_to_string(&p2).unwrap(), "b_orig");
        assert_eq!(std::fs::read_to_string(&p3).unwrap(), "c_orig");
    }

    #[test]
    fn test_turn_history_undo_reverses_in_lifo_order() {
        let dir = tempfile::tempdir().unwrap();
        // Single file modified across two turns
        let path = dir.path().join("evolving.txt");
        std::fs::write(&path, "v1").unwrap();
        let path_str = path.to_str().unwrap();

        // Turn 1: snapshot v1
        let mut snap1 = TurnSnapshot::new();
        snap1.snapshot_file(path_str);

        // Simulate turn 1 changing to v2
        std::fs::write(&path, "v2").unwrap();

        // Turn 2: snapshot v2
        let mut snap2 = TurnSnapshot::new();
        snap2.snapshot_file(path_str);

        // Simulate turn 2 changing to v3
        std::fs::write(&path, "v3").unwrap();

        let mut hist = TurnHistory::new();
        hist.push(snap1);
        hist.push(snap2);

        // Undo turn 2 → should restore v2
        hist.undo_last(1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2");

        // Undo turn 1 → should restore v1
        hist.undo_last(1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1");
    }

    #[test]
    fn test_turn_history_undo_with_created_files() {
        let dir = tempfile::tempdir().unwrap();
        let created = dir.path().join("new_file.txt");

        let mut snap = TurnSnapshot::new();
        snap.record_created(created.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap);

        // Simulate agent creating the file
        std::fs::write(&created, "hello").unwrap();
        assert!(created.exists());

        let actions = hist.undo_last(1);
        assert!(!actions.is_empty());
        assert!(!created.exists());
    }

    #[test]
    fn test_turn_history_clear_after_undo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "x").unwrap();

        let mut snap = TurnSnapshot::new();
        snap.snapshot_file(path.to_str().unwrap());

        let mut hist = TurnHistory::new();
        hist.push(snap);

        hist.undo_last(1);
        assert!(hist.is_empty());

        hist.clear();
        assert!(hist.is_empty());
    }

    #[test]
    fn test_turn_history_push_after_clear() {
        let dir = tempfile::tempdir().unwrap();

        let mut hist = TurnHistory::new();
        let p1 = dir.path().join("a.txt");
        std::fs::write(&p1, "a").unwrap();
        let mut s1 = TurnSnapshot::new();
        s1.snapshot_file(p1.to_str().unwrap());
        hist.push(s1);

        hist.clear();
        assert!(hist.is_empty());

        let p2 = dir.path().join("b.txt");
        std::fs::write(&p2, "b").unwrap();
        let mut s2 = TurnSnapshot::new();
        s2.snapshot_file(p2.to_str().unwrap());
        hist.push(s2);

        assert_eq!(hist.len(), 1);
    }

    #[test]
    fn test_turn_history_undo_empty_is_noop() {
        let mut hist = TurnHistory::new();
        let actions = hist.undo_last(5);
        assert!(actions.is_empty());
        assert!(hist.is_empty());
    }

    // --- Additional format_changes tests ---

    #[test]
    fn test_format_changes_single_edit() {
        let changes = SessionChanges::new();
        changes.record("src/tools.rs", ChangeKind::Edit);
        let output = format_changes(&changes);
        assert!(output.contains("1 file modified"));
        assert!(output.contains("src/tools.rs"));
        assert!(output.contains("edit"));
        assert!(output.contains("🔧"));
    }

    #[test]
    fn test_format_changes_write_icon() {
        let changes = SessionChanges::new();
        changes.record("new.rs", ChangeKind::Write);
        let output = format_changes(&changes);
        assert!(output.contains("✏"));
        assert!(!output.contains("🔧"));
    }

    #[test]
    fn test_format_changes_edit_icon() {
        let changes = SessionChanges::new();
        changes.record("existing.rs", ChangeKind::Edit);
        let output = format_changes(&changes);
        assert!(output.contains("🔧"));
        assert!(!output.contains("✏"));
    }

    #[test]
    fn test_format_changes_pluralization_singular() {
        let changes = SessionChanges::new();
        changes.record("only.rs", ChangeKind::Write);
        let output = format_changes(&changes);
        assert!(output.contains("1 file modified"));
        assert!(!output.contains("files"));
    }

    #[test]
    fn test_format_changes_pluralization_plural() {
        let changes = SessionChanges::new();
        changes.record("a.rs", ChangeKind::Write);
        changes.record("b.rs", ChangeKind::Edit);
        changes.record("c.rs", ChangeKind::Write);
        let output = format_changes(&changes);
        assert!(output.contains("3 files modified"));
    }

    #[test]
    fn test_format_changes_mixed_kinds_shows_both_icons() {
        let changes = SessionChanges::new();
        changes.record("written.rs", ChangeKind::Write);
        changes.record("edited.rs", ChangeKind::Edit);
        let output = format_changes(&changes);
        assert!(output.contains("✏"));
        assert!(output.contains("🔧"));
    }

    #[test]
    fn test_format_changes_all_paths_present() {
        let changes = SessionChanges::new();
        let paths = vec!["src/a.rs", "src/b.rs", "README.md", "Cargo.toml"];
        for p in &paths {
            changes.record(p, ChangeKind::Write);
        }
        let output = format_changes(&changes);
        for p in &paths {
            assert!(output.contains(p), "Missing path: {p}");
        }
    }

    // --- format_turn_changes tests ---

    #[test]
    fn test_format_turn_changes_empty_no_changes() {
        let changes = SessionChanges::new();
        let before: Vec<String> = vec![];
        let output = format_turn_changes(&before, &changes);
        assert!(output.is_empty(), "Expected empty for no changes");
    }

    #[test]
    fn test_format_turn_changes_all_before_returns_empty() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        let before = vec!["src/main.rs".to_string()];
        let output = format_turn_changes(&before, &changes);
        assert!(
            output.is_empty(),
            "Expected empty when all files were in before"
        );
    }

    #[test]
    fn test_format_turn_changes_single_new_file() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        let before: Vec<String> = vec![];
        let output = format_turn_changes(&before, &changes);
        assert!(output.contains("Files changed:"), "Missing header");
        assert!(output.contains("✏ src/main.rs"), "Missing file entry");
        // Should NOT contain count in parens for <=3 files
        assert!(!output.contains("(1)"), "Should not show count for 1 file");
    }

    #[test]
    fn test_format_turn_changes_single_edit() {
        let changes = SessionChanges::new();
        changes.record("src/cli.rs", ChangeKind::Edit);
        let before: Vec<String> = vec![];
        let output = format_turn_changes(&before, &changes);
        assert!(output.contains("🔧 src/cli.rs"), "Missing edit icon");
    }

    #[test]
    fn test_format_turn_changes_multiple_files() {
        let changes = SessionChanges::new();
        changes.record("src/main.rs", ChangeKind::Write);
        changes.record("src/cli.rs", ChangeKind::Edit);
        changes.record("README.md", ChangeKind::Write);
        let before: Vec<String> = vec![];
        let output = format_turn_changes(&before, &changes);
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/cli.rs"));
        assert!(output.contains("README.md"));
        assert!(!output.contains("…"), "Should not truncate for 3 files");
    }

    #[test]
    fn test_format_turn_changes_filters_before() {
        let changes = SessionChanges::new();
        changes.record("src/old.rs", ChangeKind::Write);
        changes.record("src/new.rs", ChangeKind::Edit);
        let before = vec!["src/old.rs".to_string()];
        let output = format_turn_changes(&before, &changes);
        assert!(
            !output.contains("src/old.rs"),
            "Should not show files from before"
        );
        assert!(output.contains("src/new.rs"), "Should show new files");
    }

    #[test]
    fn test_format_turn_changes_four_plus_truncates() {
        let changes = SessionChanges::new();
        changes.record("a.rs", ChangeKind::Write);
        changes.record("b.rs", ChangeKind::Edit);
        changes.record("c.rs", ChangeKind::Write);
        changes.record("d.rs", ChangeKind::Edit);
        let before: Vec<String> = vec![];
        let output = format_turn_changes(&before, &changes);
        assert!(output.contains("(4)"), "Should show count for 4+ files");
        assert!(output.contains("…"), "Should show ellipsis for 4+ files");
        // Should show first 3
        assert!(output.contains("a.rs"));
        assert!(output.contains("b.rs"));
        assert!(output.contains("c.rs"));
        // d.rs is truncated
        assert!(!output.contains("d.rs"), "4th file should be truncated");
    }
}
