//! Project context loading — file listing, git status, recently changed files.
//!
//! Extracted from `cli.rs` to keep context assembly separate from CLI argument parsing.

use crate::commands_project::{detect_project_type, project_type_hints};
use crate::format::{is_quiet, DIM, RESET};

/// Project instruction files, checked in order. All found files are concatenated.
/// YOYO.md is the canonical name; CLAUDE.md is a compatibility alias.
pub const PROJECT_CONTEXT_FILES: &[&str] = &["YOYO.md", "CLAUDE.md", ".yoyo/instructions.md"];

/// Maximum number of files to include in the project file listing.
pub const MAX_PROJECT_FILES: usize = 200;

/// Maximum number of recently changed files to include in context.
pub const MAX_RECENT_FILES: usize = 20;

/// Get a listing of project files using `git ls-files`.
/// Returns a newline-separated list of tracked files, capped at MAX_PROJECT_FILES.
/// Returns None if git is not available or the directory is not a git repo.
pub fn get_project_file_listing() -> Option<String> {
    let stdout = crate::git::run_git(&["ls-files"]).ok()?;
    let files: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    if files.is_empty() {
        return None;
    }
    let total = files.len();
    let capped: Vec<&str> = files.into_iter().take(MAX_PROJECT_FILES).collect();
    let mut listing = capped.join("\n");
    if total > MAX_PROJECT_FILES {
        listing.push_str(&format!(
            "\n... and {} more files",
            total - MAX_PROJECT_FILES
        ));
    }
    Some(listing)
}

/// Get a brief git status summary for system prompt injection.
/// Returns None if not in a git repo or git is unavailable.
pub fn get_git_status_context() -> Option<String> {
    let branch = crate::git::git_branch()?;

    let uncommitted = crate::git::run_git(&["status", "--porcelain"])
        .ok()
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);

    let staged = crate::git::run_git(&["diff", "--cached", "--name-only"])
        .ok()
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);

    let mut result = String::from("## Git Status\n\n");
    result.push_str(&format!("Branch: {branch}\n"));
    if uncommitted > 0 {
        result.push_str(&format!(
            "Uncommitted changes: {} file{}\n",
            uncommitted,
            if uncommitted == 1 { "" } else { "s" }
        ));
    }
    if staged > 0 {
        result.push_str(&format!(
            "Staged: {} file{}\n",
            staged,
            if staged == 1 { "" } else { "s" }
        ));
    }

    Some(result)
}

/// Get the most recently changed files from git log, deduplicated.
/// Returns up to `max_files` unique file paths that were modified in recent commits.
/// Returns None if not in a git repo or git is unavailable.
pub fn get_recently_changed_files(max_files: usize) -> Option<Vec<String>> {
    let stdout = crate::git::run_git(&[
        "log",
        "--diff-filter=M",
        "--name-only",
        "--pretty=format:",
        "-n",
        "20",
    ])
    .ok()?;
    let mut seen = std::collections::HashSet::new();
    let files: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|l| seen.insert(l.to_string()))
        .take(max_files)
        .map(|l| l.to_string())
        .collect();
    if files.is_empty() {
        None
    } else {
        Some(files)
    }
}

/// Load project context from YOYO.md (primary), CLAUDE.md (compatibility alias),
/// or .yoyo/instructions.md.
/// Appends project file listing, recently changed files, git status, and memories
/// when available.
pub fn load_project_context() -> Option<String> {
    let mut context = String::new();
    let mut found = Vec::new();
    for name in PROJECT_CONTEXT_FILES {
        if let Ok(content) = std::fs::read_to_string(name) {
            let content = content.trim();
            if !content.is_empty() {
                if !context.is_empty() {
                    context.push_str("\n\n");
                }
                context.push_str(content);
                found.push(*name);
            }
        }
    }

    // Append project file listing if available
    if let Some(file_listing) = get_project_file_listing() {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str("## Project Files\n\n");
        context.push_str(&file_listing);
        if found.is_empty() && !is_quiet() {
            // Even without context files, file listing alone is useful
            eprintln!("{DIM}  context: project file listing{RESET}");
        }
    }

    // Append recently changed files if available
    if let Some(recent_files) = get_recently_changed_files(MAX_RECENT_FILES) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str("## Recently Changed Files\n\n");
        context.push_str(&recent_files.join("\n"));
    }

    // Append git status if available
    let git_branch_name = if let Some(git_status) = get_git_status_context() {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        let branch = crate::git::git_branch();
        context.push_str(&git_status);
        branch
    } else {
        None
    };

    // Append project-type conventions if no explicit context file was found
    let mut conventions_injected = false;
    if found.is_empty() {
        let project_type = detect_project_type(std::path::Path::new("."));
        if let Some(hints) = project_type_hints(&project_type) {
            if !context.is_empty() {
                context.push_str("\n\n");
            }
            context.push_str("## Development Conventions\n\n");
            context.push_str(&hints);
            conventions_injected = true;
        }
    }

    // Append project memories if available
    let memory = crate::memory::load_memories();
    if let Some(memories_section) = crate::memory::format_memories_for_prompt(&memory) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str(&memories_section);
    }

    if found.is_empty() && context.is_empty() {
        None
    } else {
        if !is_quiet() {
            for name in &found {
                eprintln!("{DIM}  context: {name}{RESET}");
            }
            if conventions_injected {
                let project_type = detect_project_type(std::path::Path::new("."));
                eprintln!("{DIM}  context: {project_type} conventions{RESET}");
            }
            if context.contains("## Recently Changed Files") {
                eprintln!("{DIM}  context: recently changed files{RESET}");
            }
            if let Some(branch) = &git_branch_name {
                eprintln!("{DIM}  context: git status (branch: {branch}){RESET}");
            }
            if !memory.entries.is_empty() {
                eprintln!(
                    "{DIM}  context: {} project memories{RESET}",
                    memory.entries.len()
                );
            }
        }
        Some(context)
    }
}

/// List which project context files exist and their sizes.
/// Returns a vec of (filename, line_count) for display by /context.
pub fn list_project_context_files() -> Vec<(&'static str, usize)> {
    let mut result = Vec::new();
    for name in PROJECT_CONTEXT_FILES {
        if let Ok(content) = std::fs::read_to_string(name) {
            let content = content.trim();
            if !content.is_empty() {
                let lines = content.lines().count();
                result.push((*name, lines));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_context_file_names_not_empty() {
        assert_eq!(PROJECT_CONTEXT_FILES.len(), 3);
        // YOYO.md must be first — it's the canonical context file name
        assert_eq!(PROJECT_CONTEXT_FILES[0], "YOYO.md");
        // CLAUDE.md is a compatibility alias
        assert_eq!(PROJECT_CONTEXT_FILES[1], "CLAUDE.md");
        assert_eq!(PROJECT_CONTEXT_FILES[2], ".yoyo/instructions.md");
        for name in PROJECT_CONTEXT_FILES {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_max_project_files_constant() {
        assert_eq!(MAX_PROJECT_FILES, 200);
    }

    #[test]
    fn test_max_recent_files_constant() {
        assert_eq!(MAX_RECENT_FILES, 20);
    }

    #[test]
    fn test_list_project_context_files_returns_vec() {
        // This test verifies the function runs without panicking.
        // In CI the project may or may not have YOYO.md present.
        let files = list_project_context_files();
        for (name, lines) in &files {
            assert!(!name.is_empty());
            assert!(*lines > 0);
        }
    }

    #[test]
    fn test_get_project_file_listing_no_panic() {
        // Should not panic regardless of whether we're in a git repo or not.
        // In CI this runs inside a git repo, so we expect Some with files.
        let result = get_project_file_listing();
        // If we're in a git repo (likely in CI), verify the output is reasonable
        if let Some(listing) = &result {
            assert!(!listing.is_empty(), "File listing should not be empty");
            let lines: Vec<&str> = listing.lines().collect();
            assert!(
                lines.len() <= MAX_PROJECT_FILES + 1, // +1 for possible "... and N more" line
                "File listing should be capped at {} files",
                MAX_PROJECT_FILES
            );
            // Should contain at least Cargo.toml (we're in a Rust project)
            assert!(
                listing.contains("Cargo.toml"),
                "File listing should contain Cargo.toml"
            );
        }
    }

    #[test]
    fn test_load_project_context_includes_file_listing() {
        // load_project_context should include project file listing when in a git repo
        let result = load_project_context();
        if let Some(context) = &result {
            // If we're in a git repo, context should include the file listing section
            if get_project_file_listing().is_some() {
                assert!(
                    context.contains("## Project Files"),
                    "Context should contain Project Files section"
                );
            }
        }
    }

    #[test]
    fn test_get_recently_changed_files_in_git_repo() {
        // We're running in a git repo (CI or local), so this should return Some
        let result = get_recently_changed_files(20);
        if let Some(files) = &result {
            assert!(!files.is_empty(), "Should have recently changed files");
            // Files should be deduplicated
            let unique: std::collections::HashSet<&String> = files.iter().collect();
            assert_eq!(
                files.len(),
                unique.len(),
                "Recently changed files should be deduplicated"
            );
            // Should respect the max limit
            assert!(files.len() <= 20, "Should not exceed max_files limit");
        }
    }

    #[test]
    fn test_get_recently_changed_files_respects_limit() {
        // Request only 2 files — should return at most 2
        let result = get_recently_changed_files(2);
        if let Some(files) = &result {
            assert!(
                files.len() <= 2,
                "Should respect max_files=2, got {}",
                files.len()
            );
        }
    }

    #[test]
    fn test_get_recently_changed_files_no_duplicates() {
        let result = get_recently_changed_files(50);
        if let Some(files) = &result {
            let unique: std::collections::HashSet<&String> = files.iter().collect();
            assert_eq!(files.len(), unique.len(), "Files should be deduplicated");
        }
    }

    #[test]
    fn test_load_project_context_includes_recently_changed() {
        // In a git repo with commits, context should include recently changed files
        let result = load_project_context();
        if let Some(context) = &result {
            if get_recently_changed_files(MAX_RECENT_FILES).is_some() {
                assert!(
                    context.contains("## Recently Changed Files"),
                    "Context should contain Recently Changed Files section"
                );
            }
        }
    }

    #[test]
    fn test_get_git_status_context_in_repo() {
        // We're running inside a git repo, so this should return Some
        let result = get_git_status_context();
        assert!(result.is_some(), "Should return Some when in a git repo");
        assert!(
            result.as_ref().unwrap().contains("Branch:"),
            "Should contain 'Branch:' label"
        );
    }

    #[test]
    fn test_get_git_status_context_contains_branch() {
        let result = get_git_status_context().expect("Should be in a git repo");
        // Get the actual branch name to verify it's in the output
        let branch = crate::git::git_branch().expect("Should get branch name");
        assert!(
            result.contains(&format!("Branch: {branch}")),
            "Should contain actual branch name: {branch}"
        );
    }

    #[test]
    fn test_git_status_context_format() {
        let result = get_git_status_context().expect("Should be in a git repo");
        assert!(
            result.starts_with("## Git Status\n\n"),
            "Should start with '## Git Status' header"
        );
    }

    #[test]
    fn test_load_project_context_includes_git_status() {
        // In a git repo, load_project_context should include git status
        let result = load_project_context();
        if let Some(context) = &result {
            if get_git_status_context().is_some() {
                assert!(
                    context.contains("## Git Status"),
                    "Context should contain Git Status section"
                );
            }
        }
    }

    #[test]
    fn test_yoyo_md_is_primary_context_file() {
        // YOYO.md should be the first (primary) context file
        assert_eq!(
            PROJECT_CONTEXT_FILES[0], "YOYO.md",
            "YOYO.md must be the primary context file"
        );
        // CLAUDE.md should be present as compatibility alias but not first
        assert!(
            PROJECT_CONTEXT_FILES.contains(&"CLAUDE.md"),
            "CLAUDE.md should still be supported for compatibility"
        );
        assert_ne!(
            PROJECT_CONTEXT_FILES[0], "CLAUDE.md",
            "CLAUDE.md should not be the primary context file"
        );
    }

    #[test]
    fn test_project_context_includes_conventions() {
        // When run in a directory with no YOYO.md but with a Cargo.toml,
        // load_project_context should include development conventions.
        // We run in a temp dir with a git repo and Cargo.toml but no YOYO.md.
        use std::process::Command;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        // Initialize a git repo so file listing works
        Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .ok();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .ok();

        // Change to temp dir, call load_project_context, change back
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = load_project_context();
        std::env::set_current_dir(&original_dir).unwrap();

        let ctx = ctx.unwrap();
        assert!(
            ctx.contains("## Development Conventions"),
            "Should include conventions section"
        );
        assert!(
            ctx.contains("cargo"),
            "Rust conventions should mention cargo"
        );
    }

    #[test]
    fn test_project_context_skips_conventions_with_context_file() {
        // When YOYO.md exists, conventions should NOT be injected
        use std::process::Command;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        std::fs::write(
            dir.path().join("YOYO.md"),
            "# My Project\nCustom instructions",
        )
        .unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .ok();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .ok();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = load_project_context();
        std::env::set_current_dir(&original_dir).unwrap();

        let ctx = ctx.unwrap();
        assert!(
            !ctx.contains("## Development Conventions"),
            "Should NOT include conventions when YOYO.md exists"
        );
        assert!(
            ctx.contains("Custom instructions"),
            "Should include YOYO.md content"
        );
    }
}
