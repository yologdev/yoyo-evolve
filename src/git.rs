//! Git-related functions: staging, committing, branch detection, and `/git` subcommands.

use crate::format::*;

/// Get the current git branch name, if we're in a git repo.
pub fn git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Get staged changes (git diff --cached).
/// Returns None if git fails, Some("") if nothing staged, or Some(diff) with the diff text.
pub fn get_staged_diff() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--cached"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `git commit -m "<message>"` and return (success, output_text).
pub fn run_git_commit(message: &str) -> (bool, String) {
    match std::process::Command::new("git")
        .args(["commit", "-m", message])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let text = if stdout.is_empty() { stderr } else { stdout };
            (output.status.success(), text)
        }
        Err(e) => (false, format!("error: {e}")),
    }
}

/// Generate a conventional commit message from a diff using simple heuristics.
/// This is a local, token-free approach — no AI calls needed.
pub fn generate_commit_message(diff: &str) -> String {
    let mut files_changed: Vec<String> = Vec::new();
    let mut insertions = 0usize;
    let mut deletions = 0usize;

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            files_changed.push(path.to_string());
        } else if line.starts_with('+') && !line.starts_with("+++") {
            insertions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    // Determine type prefix based on file paths
    let prefix = if files_changed.iter().any(|f| f.contains("test")) {
        "test"
    } else if files_changed
        .iter()
        .any(|f| f.ends_with(".md") || f.starts_with("docs/"))
    {
        "docs"
    } else if files_changed
        .iter()
        .any(|f| f.starts_with(".github/") || f.starts_with("scripts/") || f == "Cargo.toml")
    {
        "chore"
    } else if deletions > insertions * 2 {
        "refactor"
    } else {
        "feat"
    };

    // Build a concise scope from changed files
    let scope = if files_changed.len() == 1 {
        let f = &files_changed[0];
        let name = f.rsplit('/').next().unwrap_or(f);
        // Strip extension for scope
        name.split('.').next().unwrap_or(name).to_string()
    } else if files_changed.len() <= 3 {
        files_changed
            .iter()
            .map(|f| {
                let name = f.rsplit('/').next().unwrap_or(f);
                name.split('.').next().unwrap_or(name).to_string()
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        format!("{} files", files_changed.len())
    };

    let summary = if deletions == 0 && insertions > 0 {
        "add changes"
    } else if insertions == 0 && deletions > 0 {
        "remove code"
    } else {
        "update code"
    };

    format!("{prefix}({scope}): {summary}")
}

/// Represents a parsed `/git` subcommand.
#[derive(Debug, PartialEq)]
pub enum GitSubcommand {
    /// `/git status` — run `git status --short`
    Status,
    /// `/git log [n]` — show last n commits (default 5)
    Log(usize),
    /// `/git add <path>` — stage files
    Add(String),
    /// `/git stash` — stash changes
    Stash,
    /// `/git stash pop` — pop stashed changes
    StashPop,
    /// `/git diff` — show diff (unstaged by default, `--cached` for staged)
    Diff { cached: bool },
    /// `/git branch` — list branches or create/switch to a new one
    Branch(Option<String>),
    /// Invalid or missing subcommand — show help
    Help,
}

/// Parse the argument string after `/git` into a `GitSubcommand`.
pub fn parse_git_args(arg: &str) -> GitSubcommand {
    let arg = arg.trim();
    if arg.is_empty() {
        return GitSubcommand::Help;
    }

    let parts: Vec<&str> = arg.splitn(3, char::is_whitespace).collect();
    match parts[0].to_lowercase().as_str() {
        "status" => GitSubcommand::Status,
        "log" => {
            let n = parts
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(5);
            GitSubcommand::Log(n)
        }
        "add" => {
            if parts.len() < 2 || parts[1].trim().is_empty() {
                GitSubcommand::Help
            } else {
                // Rejoin remaining parts as the path (handles spaces in filenames via quoting at shell level)
                let path = parts[1..].join(" ");
                GitSubcommand::Add(path)
            }
        }
        "stash" => {
            if parts.len() >= 2 && parts[1].to_lowercase() == "pop" {
                GitSubcommand::StashPop
            } else {
                GitSubcommand::Stash
            }
        }
        "diff" => {
            let cached =
                parts.len() >= 2 && parts[1].trim_start_matches('-').to_lowercase() == "cached";
            GitSubcommand::Diff { cached }
        }
        "branch" => {
            if parts.len() >= 2 && !parts[1].trim().is_empty() {
                let name = parts[1..].join(" ");
                GitSubcommand::Branch(Some(name))
            } else {
                GitSubcommand::Branch(None)
            }
        }
        _ => GitSubcommand::Help,
    }
}

/// Execute a `/git` subcommand directly (no AI, no tokens).
pub fn run_git_subcommand(subcmd: &GitSubcommand) {
    match subcmd {
        GitSubcommand::Status => {
            match std::process::Command::new("git")
                .args(["status", "--short"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (clean working tree){RESET}\n");
                    } else {
                        println!("{DIM}{text}{RESET}");
                    }
                }
                _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            }
        }
        GitSubcommand::Log(n) => {
            let n_str = n.to_string();
            match std::process::Command::new("git")
                .args(["log", "--oneline", "-n", &n_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (no commits yet){RESET}\n");
                    } else {
                        println!("{DIM}{text}{RESET}");
                    }
                }
                _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            }
        }
        GitSubcommand::Add(path) => {
            match std::process::Command::new("git")
                .args(["add", path])
                .output()
            {
                Ok(output) if output.status.success() => {
                    println!("{GREEN}  ✓ staged: {path}{RESET}\n");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => eprintln!("{RED}  error: git not found{RESET}\n"),
            }
        }
        GitSubcommand::Stash => match std::process::Command::new("git").args(["stash"]).output() {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout);
                println!("{GREEN}  ✓ {}{RESET}\n", text.trim());
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
            }
            Err(_) => eprintln!("{RED}  error: git not found{RESET}\n"),
        },
        GitSubcommand::StashPop => {
            match std::process::Command::new("git")
                .args(["stash", "pop"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    println!("{GREEN}  ✓ {}{RESET}\n", text.trim());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => eprintln!("{RED}  error: git not found{RESET}\n"),
            }
        }
        GitSubcommand::Diff { cached } => {
            let args: Vec<&str> = if *cached {
                vec!["diff", "--cached"]
            } else {
                vec!["diff"]
            };
            match std::process::Command::new("git").args(&args).output() {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        let scope = if *cached { "staged" } else { "unstaged" };
                        println!("{DIM}  (no {scope} changes){RESET}\n");
                    } else {
                        println!("{text}");
                    }
                }
                _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            }
        }
        GitSubcommand::Branch(name) => match name {
            Some(branch_name) => {
                match std::process::Command::new("git")
                    .args(["checkout", "-b", branch_name])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        println!("{GREEN}  ✓ switched to new branch '{branch_name}'{RESET}\n");
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                    }
                    Err(_) => eprintln!("{RED}  error: git not found{RESET}\n"),
                }
            }
            None => {
                match std::process::Command::new("git")
                    .args(["branch", "--list", "-a"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let text = String::from_utf8_lossy(&output.stdout);
                        if text.trim().is_empty() {
                            println!("{DIM}  (no branches yet){RESET}\n");
                        } else {
                            // Current branch line starts with "* ", highlight it
                            for line in text.lines() {
                                if line.starts_with("* ") {
                                    println!("{GREEN}{line}{RESET}");
                                } else {
                                    println!("{DIM}{line}{RESET}");
                                }
                            }
                            println!();
                        }
                    }
                    _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
                }
            }
        },
        GitSubcommand::Help => {
            println!("{DIM}  usage: /git status             Show working tree status");
            println!("         /git log [n]             Show last n commits (default: 5)");
            println!("         /git add <path>          Stage files for commit");
            println!("         /git diff [--cached]     Show diff (unstaged or staged changes)");
            println!("         /git branch [name]       List branches or create & switch");
            println!("         /git stash               Stash uncommitted changes");
            println!("         /git stash pop           Restore stashed changes{RESET}\n");
        }
    }
}

/// Detect the base branch for PR creation (main or master).
/// Returns "main" if it exists, otherwise "master", falling back to "main".
pub fn detect_base_branch() -> String {
    // Check if "main" branch exists
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "main"])
        .output()
    {
        if output.status.success() {
            return "main".to_string();
        }
    }
    // Check if "master" branch exists
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "master"])
        .output()
    {
        if output.status.success() {
            return "master".to_string();
        }
    }
    // Default to "main"
    "main".to_string()
}

/// Get the diff between the current branch and a base branch.
/// Returns None if git fails, Some(diff) with the diff text otherwise.
pub fn get_branch_diff(base: &str) -> Option<String> {
    let merge_base = std::process::Command::new("git")
        .args(["merge-base", base, "HEAD"])
        .output()
        .ok()?;
    if !merge_base.status.success() {
        return None;
    }
    let merge_base_sha = String::from_utf8_lossy(&merge_base.stdout)
        .trim()
        .to_string();

    let output = std::process::Command::new("git")
        .args(["diff", &merge_base_sha, "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get the list of commits on the current branch since diverging from the base branch.
/// Returns None if git fails, Some(commits) with one-line commit summaries otherwise.
pub fn get_branch_commits(base: &str) -> Option<String> {
    let range = format!("{base}..HEAD");
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", &range])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Build a prompt for the AI to generate a PR title and description.
/// The AI output should be in the format:
/// ```
/// TITLE: <one-line title>
/// ---
/// <markdown description body>
/// ```
pub fn build_pr_description_prompt(branch: &str, base: &str, commits: &str, diff: &str) -> String {
    // Truncate diff if it's very large to stay within context limits
    let max_diff_chars = 15_000;
    let diff_preview = if diff.len() > max_diff_chars {
        let truncated = &diff[..max_diff_chars];
        format!(
            "{truncated}\n\n... (diff truncated, {} more chars)",
            diff.len() - max_diff_chars
        )
    } else {
        diff.to_string()
    };

    format!(
        r#"Generate a pull request title and description for the following changes.

Branch: {branch} → {base}

Commits:
{commits}

Diff:
```
{diff_preview}
```

Respond in EXACTLY this format (no extra text before or after):

TITLE: <concise PR title using conventional commit style>
---
<markdown PR description body>

The description should include:
- A brief summary of what changed and why
- Key changes as bullet points
- Any notable implementation details

Keep it concise but informative."#
    )
}

/// Parse the AI's response into a PR title and body.
/// Expects format: "TITLE: ...\n---\n..."
pub fn parse_pr_description(response: &str) -> Option<(String, String)> {
    let response = response.trim();

    // Find the TITLE: line
    let title_line = response.lines().find(|l| l.starts_with("TITLE:"))?;
    let title = title_line.strip_prefix("TITLE:")?.trim().to_string();

    if title.is_empty() {
        return None;
    }

    // Find the --- separator and take everything after it
    let separator_pos = response.find("\n---\n")?;
    let body = response[separator_pos + 5..].trim().to_string();

    Some((title, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_staged_diff_runs() {
        // Should not panic; returns None if not in git repo (e.g. cargo-mutants temp dir)
        let result = get_staged_diff();
        // We don't assert Some — outside a git repo this returns None, and that's correct
        if let Some(diff) = result {
            // If we are in a git repo, the diff is a string (possibly empty)
            assert!(diff.len() < 10_000_000, "Diff should be reasonable size");
        }
    }

    #[test]
    fn test_generate_commit_message_basic() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,5 @@
+// new comment
+use std::io;
 fn main() {
     println!(\"hello\");
 }
";
        let msg = generate_commit_message(diff);
        // Should produce a conventional commit format: type(scope): description
        assert!(msg.contains('('), "Should have scope: {msg}");
        assert!(msg.contains("):"), "Should have conventional format: {msg}");
        assert!(msg.contains("main"), "Scope should mention 'main': {msg}");
    }

    #[test]
    fn test_generate_commit_message_docs() {
        let diff = "\
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,2 +1,3 @@
 # Project
+New docs line
";
        let msg = generate_commit_message(diff);
        assert!(
            msg.starts_with("docs("),
            "Markdown changes should use docs prefix: {msg}"
        );
    }

    #[test]
    fn test_generate_commit_message_multiple_files() {
        let diff = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
+// change a
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -1 +1,2 @@
+// change b
diff --git a/src/c.rs b/src/c.rs
--- a/src/c.rs
+++ b/src/c.rs
@@ -1 +1,2 @@
+// change c
diff --git a/src/d.rs b/src/d.rs
--- a/src/d.rs
+++ b/src/d.rs
@@ -1 +1,2 @@
+// change d
";
        let msg = generate_commit_message(diff);
        // More than 3 files should show "N files"
        assert!(
            msg.contains("4 files"),
            "Should show file count for many files: {msg}"
        );
    }

    #[test]
    fn test_generate_commit_message_deletions_only() {
        let diff = "\
diff --git a/src/old.rs b/src/old.rs
--- a/src/old.rs
+++ b/src/old.rs
@@ -1,5 +1,2 @@
-// removed line 1
-// removed line 2
-// removed line 3
 fn keep() {}
";
        let msg = generate_commit_message(diff);
        assert!(
            msg.contains("remove code"),
            "Pure deletion should say 'remove code': {msg}"
        );
    }

    #[test]
    fn test_git_subcommand_help() {
        assert_eq!(parse_git_args(""), GitSubcommand::Help);
        assert_eq!(parse_git_args("  "), GitSubcommand::Help);
        assert_eq!(parse_git_args("unknown"), GitSubcommand::Help);
        assert_eq!(parse_git_args("push"), GitSubcommand::Help);
    }

    #[test]
    fn test_git_subcommand_status() {
        assert_eq!(parse_git_args("status"), GitSubcommand::Status);
        assert_eq!(parse_git_args("STATUS"), GitSubcommand::Status);
        assert_eq!(parse_git_args("Status"), GitSubcommand::Status);
    }

    #[test]
    fn test_git_subcommand_log() {
        assert_eq!(parse_git_args("log"), GitSubcommand::Log(5));
        assert_eq!(parse_git_args("log 10"), GitSubcommand::Log(10));
        assert_eq!(parse_git_args("log 1"), GitSubcommand::Log(1));
        assert_eq!(parse_git_args("LOG 20"), GitSubcommand::Log(20));
        // Invalid number falls back to default 5
        assert_eq!(parse_git_args("log abc"), GitSubcommand::Log(5));
    }

    #[test]
    fn test_git_subcommand_add() {
        assert_eq!(
            parse_git_args("add src/main.rs"),
            GitSubcommand::Add("src/main.rs".to_string())
        );
        assert_eq!(parse_git_args("add ."), GitSubcommand::Add(".".to_string()));
        assert_eq!(
            parse_git_args("ADD Cargo.toml"),
            GitSubcommand::Add("Cargo.toml".to_string())
        );
        // add without path shows help
        assert_eq!(parse_git_args("add"), GitSubcommand::Help);
        assert_eq!(parse_git_args("add  "), GitSubcommand::Help);
    }

    #[test]
    fn test_git_subcommand_stash() {
        assert_eq!(parse_git_args("stash"), GitSubcommand::Stash);
        assert_eq!(parse_git_args("STASH"), GitSubcommand::Stash);
    }

    #[test]
    fn test_git_subcommand_stash_pop() {
        assert_eq!(parse_git_args("stash pop"), GitSubcommand::StashPop);
        assert_eq!(parse_git_args("STASH POP"), GitSubcommand::StashPop);
        assert_eq!(parse_git_args("stash Pop"), GitSubcommand::StashPop);
    }

    #[test]
    fn test_git_subcommand_diff() {
        assert_eq!(
            parse_git_args("diff"),
            GitSubcommand::Diff { cached: false }
        );
        assert_eq!(
            parse_git_args("DIFF"),
            GitSubcommand::Diff { cached: false }
        );
        assert_eq!(
            parse_git_args("diff --cached"),
            GitSubcommand::Diff { cached: true }
        );
        assert_eq!(
            parse_git_args("DIFF --CACHED"),
            GitSubcommand::Diff { cached: true }
        );
        // Non-cached flag treated as not cached
        assert_eq!(
            parse_git_args("diff --stat"),
            GitSubcommand::Diff { cached: false }
        );
    }

    #[test]
    fn test_git_subcommand_branch() {
        assert_eq!(parse_git_args("branch"), GitSubcommand::Branch(None));
        assert_eq!(parse_git_args("BRANCH"), GitSubcommand::Branch(None));
        assert_eq!(
            parse_git_args("branch feature/new"),
            GitSubcommand::Branch(Some("feature/new".to_string()))
        );
        assert_eq!(
            parse_git_args("BRANCH my-branch"),
            GitSubcommand::Branch(Some("my-branch".to_string()))
        );
        // branch with empty name is just listing
        assert_eq!(parse_git_args("branch  "), GitSubcommand::Branch(None));
    }

    #[test]
    fn test_git_branch_returns_something_in_repo() {
        let branch = git_branch();
        // Outside a git repo (e.g. cargo-mutants temp dir), branch is None — that's fine
        if let Some(name) = branch {
            assert!(!name.is_empty(), "Branch name should not be empty");
            assert!(
                !name.contains('\n'),
                "Branch name should not contain newlines"
            );
        }
    }

    #[test]
    fn test_detect_base_branch_returns_valid_name() {
        let base = detect_base_branch();
        assert!(
            base == "main" || base == "master",
            "Base branch should be 'main' or 'master', got: {base}"
        );
    }

    #[test]
    fn test_get_branch_diff_runs() {
        // Should not panic; may return None outside a git repo
        let base = detect_base_branch();
        let diff = get_branch_diff(&base);
        if let Some(d) = diff {
            assert!(d.len() < 50_000_000, "Diff should be reasonable size");
        }
    }

    #[test]
    fn test_get_branch_commits_runs() {
        // Should not panic; may return None outside a git repo
        let base = detect_base_branch();
        let commits = get_branch_commits(&base);
        if let Some(c) = commits {
            assert!(c.len() < 10_000_000, "Commits output should be reasonable");
        }
    }

    #[test]
    fn test_build_pr_description_prompt_contains_info() {
        let prompt = build_pr_description_prompt(
            "feature/test",
            "main",
            "abc1234 Add feature\ndef5678 Fix bug\n",
            "+++ b/src/main.rs\n+// new code\n",
        );
        assert!(
            prompt.contains("feature/test"),
            "Prompt should contain branch name"
        );
        assert!(prompt.contains("main"), "Prompt should contain base branch");
        assert!(prompt.contains("abc1234"), "Prompt should contain commits");
        assert!(prompt.contains("new code"), "Prompt should contain diff");
        assert!(
            prompt.contains("TITLE:"),
            "Prompt should ask for TITLE format"
        );
    }

    #[test]
    fn test_build_pr_description_prompt_truncates_large_diff() {
        let large_diff = "x".repeat(20_000);
        let prompt = build_pr_description_prompt("branch", "main", "commit1", &large_diff);
        assert!(
            prompt.contains("diff truncated"),
            "Large diffs should be truncated"
        );
        // The prompt should not be the full 20k+ length
        assert!(
            prompt.len() < 20_000,
            "Prompt should be truncated, got {} chars",
            prompt.len()
        );
    }

    #[test]
    fn test_parse_pr_description_valid() {
        let response = "TITLE: feat: add PR creation command\n---\nThis PR adds the `/pr create` command.\n\n- New command\n- AI-generated descriptions";
        let result = parse_pr_description(response);
        assert!(result.is_some(), "Should parse valid response");
        let (title, body) = result.unwrap();
        assert_eq!(title, "feat: add PR creation command");
        assert!(body.contains("This PR adds"));
        assert!(body.contains("- New command"));
    }

    #[test]
    fn test_parse_pr_description_with_extra_whitespace() {
        let response =
            "\n  TITLE: fix: resolve crash on startup\n---\n\nFixed the null pointer issue.\n  ";
        let result = parse_pr_description(response);
        assert!(result.is_some(), "Should parse with extra whitespace");
        let (title, body) = result.unwrap();
        assert_eq!(title, "fix: resolve crash on startup");
        assert!(body.contains("Fixed the null pointer"));
    }

    #[test]
    fn test_parse_pr_description_missing_title() {
        let response = "Some random text without TITLE line\n---\nbody here";
        let result = parse_pr_description(response);
        assert!(result.is_none(), "Should fail without TITLE: line");
    }

    #[test]
    fn test_parse_pr_description_missing_separator() {
        let response = "TITLE: some title\nbody without separator";
        let result = parse_pr_description(response);
        assert!(result.is_none(), "Should fail without --- separator");
    }

    #[test]
    fn test_parse_pr_description_empty_title() {
        let response = "TITLE: \n---\nbody here";
        let result = parse_pr_description(response);
        assert!(result.is_none(), "Should fail with empty title");
    }
}
