//! Project context loading — file listing, git status, recently changed files.
//!
//! Extracted from `cli.rs` to keep context assembly separate from CLI argument parsing.

use crate::commands_project::{detect_project_type, project_type_hints};
use crate::format::{is_quiet, DIM, RESET};

/// Project instruction files, checked in order. All found files are concatenated.
///
/// YOYO.md is the canonical name for yoyo projects. The remaining entries are
/// compatibility aliases so that yoyo automatically picks up project instructions
/// written for other AI coding tools:
///
/// - **CLAUDE.md** — Claude Code
/// - **.yoyo/instructions.md** — yoyo alternate location
/// - **AGENTS.md** — Google Gemini CLI / generic agents
/// - **.cursorrules** — Cursor
/// - **.github/copilot-instructions.md** — GitHub Copilot
///
/// When a developer already has any of these in their project, yoyo reads them
/// at startup — no configuration needed.
pub const PROJECT_CONTEXT_FILES: &[&str] = &[
    "YOYO.md",
    "CLAUDE.md",
    ".yoyo/instructions.md",
    "AGENTS.md",
    ".cursorrules",
    ".github/copilot-instructions.md",
];

/// Maximum number of files to include in the project file listing.
pub const MAX_PROJECT_FILES: usize = 200;

/// Maximum number of recently changed files to include in context.
pub const MAX_RECENT_FILES: usize = 20;

/// Get a listing of project files using `git ls-files`.
/// Returns a newline-separated list of tracked files, capped at MAX_PROJECT_FILES.
/// Returns None if git is not available or the directory is not a git repo.
/// Only exercised by smoke tests today; kept as the CWD-convenience entry point.
#[cfg_attr(not(test), allow(dead_code))]
pub fn get_project_file_listing() -> Option<String> {
    get_project_file_listing_from(std::path::Path::new("."))
}

/// Directory-parameterized variant of [`get_project_file_listing`].
/// Lets tests point at a hermetic temp git repo instead of the live CWD.
pub fn get_project_file_listing_from(dir: &std::path::Path) -> Option<String> {
    let stdout = crate::git::run_git_in_dir(dir, &["ls-files"]).ok()?;
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
/// Only exercised by smoke tests today; kept as the CWD-convenience entry point.
#[cfg_attr(not(test), allow(dead_code))]
pub fn get_git_status_context() -> Option<String> {
    get_git_status_context_from(std::path::Path::new("."))
}

/// Directory-parameterized variant of [`get_git_status_context`].
/// Lets tests point at a hermetic temp git repo instead of the live CWD.
pub fn get_git_status_context_from(dir: &std::path::Path) -> Option<String> {
    let branch = git_branch_in(dir)?;

    let uncommitted = crate::git::run_git_in_dir(dir, &["status", "--porcelain"])
        .ok()
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);

    let staged = crate::git::run_git_in_dir(dir, &["diff", "--cached", "--name-only"])
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

/// Get the current branch name for a specific directory.
/// Directory-parameterized sibling of `crate::git::git_branch()`.
fn git_branch_in(dir: &std::path::Path) -> Option<String> {
    crate::git::run_git_in_dir(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()
}

/// Get the most recently changed files from git log, deduplicated.
/// Returns up to `max_files` unique file paths that were modified in recent commits.
/// Returns None if not in a git repo or git is unavailable.
/// Only exercised by smoke tests today; kept as the CWD-convenience entry point.
#[cfg_attr(not(test), allow(dead_code))]
pub fn get_recently_changed_files(max_files: usize) -> Option<Vec<String>> {
    get_recently_changed_files_from(std::path::Path::new("."), max_files)
}

/// Directory-parameterized variant of [`get_recently_changed_files`].
/// Lets tests point at a hermetic temp git repo instead of the live CWD.
pub fn get_recently_changed_files_from(
    dir: &std::path::Path,
    max_files: usize,
) -> Option<Vec<String>> {
    let stdout = crate::git::run_git_in_dir(
        dir,
        &[
            "log",
            "--diff-filter=AM",
            "--name-only",
            "--pretty=format:",
            "-n",
            "20",
        ],
    )
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

/// Load project context from instruction files (YOYO.md, CLAUDE.md, AGENTS.md,
/// .cursorrules, .github/copilot-instructions.md, etc.).
/// When multiple instruction files are found, each section is labeled with its
/// origin so the model knows which file each block came from.
/// Appends project file listing, recently changed files, git status, and memories
/// when available.
pub fn load_project_context() -> Option<String> {
    load_project_context_from(std::path::Path::new("."))
}

/// Directory-parameterized variant of [`load_project_context`].
/// Lets tests point at a hermetic temp git repo instead of the live CWD.
pub fn load_project_context_from(dir: &std::path::Path) -> Option<String> {
    let mut context = String::new();
    let mut found = Vec::new();
    for name in PROJECT_CONTEXT_FILES {
        if let Ok(content) = std::fs::read_to_string(dir.join(name)) {
            let content = content.trim();
            if !content.is_empty() {
                if !context.is_empty() {
                    context.push_str("\n\n");
                }
                // When loading multiple files, label each section so the model
                // knows where the instructions came from.
                if !found.is_empty() {
                    context.push_str(&format!("--- From {name} ---\n"));
                }
                context.push_str(content);
                found.push(*name);
            }
        }
    }

    // Append project file listing if available
    if let Some(file_listing) = get_project_file_listing_from(dir) {
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

    // Append recently changed files if available.
    //
    // NOTE: this must use the `_from(dir)` variant, not the CWD one. The CWD
    // variant was the Day 125 flake root cause: under parallel test execution,
    // `#[serial]` tests mutate the process CWD, so the "Recently Changed
    // Files" section was computed from whatever directory the process happened
    // to be in — sometimes a temp repo with no commits (git log fails, the
    // error is swallowed by `.ok()?`, and the section silently vanishes).
    if let Some(recent_files) = get_recently_changed_files_from(dir, MAX_RECENT_FILES) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str("## Recently Changed Files\n\n");
        context.push_str(&recent_files.join("\n"));
    }

    // Append git status if available
    let git_branch_name = if let Some(git_status) = get_git_status_context_from(dir) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        let branch = git_branch_in(dir);
        context.push_str(&git_status);
        branch
    } else {
        None
    };

    // Append project-type conventions (always, regardless of context files —
    // conventions complement explicit instructions rather than replacing them)
    let mut conventions_injected = false;
    let project_type = detect_project_type(dir);
    if let Some(hints) = project_type_hints(&project_type) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str("## Development Conventions\n\n");
        context.push_str(&hints);
        conventions_injected = true;
    }

    // Append project memories if available. Loaded relative to `dir` (not the
    // process CWD) so a hermetic fixture context never picks up the real
    // repo's `.yoyo/memory.json` — memory entries can contain arbitrary text
    // (including section-header-like strings), which made fixture-based test
    // assertions nondeterministic under parallel execution.
    let memory = crate::memory::load_memories_from(&dir.join(crate::memory::memory_file_path()));
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
    use serial_test::serial;

    /// Build a fully environment-isolated git Command for fixture repos.
    ///
    /// In CI and under parallel test execution, a bare `git commit` in a
    /// temp-dir repo can fail or silently no-op for reasons that don't exist
    /// locally: leaked global/system config (`commit.gpgsign=true` with no
    /// key, `core.hooksPath`), leaked env vars (`GIT_DIR`, `GIT_INDEX_FILE`,
    /// ...) redirecting the repo elsewhere, or missing identity when HOME is
    /// overridden. That exact failure killed
    /// `test_load_project_context_includes_recently_changed` on Day 125: the
    /// second fixture commit failed, the file stayed dirty, and the test saw
    /// "Uncommitted changes: 1 file" with no Recently Changed section.
    /// Every fixture git invocation goes through here so no leakage path
    /// exists.
    fn fixture_git_command(dir: &std::path::Path, args: &[&str]) -> std::process::Command {
        let mut cmd = std::process::Command::new("git");
        // Ignore global/system config entirely (supported since git 2.32).
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
        cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
        // Strip env vars that can redirect the repo/index/objects elsewhere
        // or inject config from a parallel test or the harness.
        for var in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_NAMESPACE",
            "GIT_COMMON_DIR",
            "GIT_CONFIG_COUNT",
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_AUTHOR_DATE",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_COMMITTER_DATE",
        ] {
            cmd.env_remove(var);
        }
        // Identity + safety inline on every call: `-c` beats config files
        // because leaked config can't override it.
        cmd.args([
            "-c",
            "user.name=yoyo-test",
            "-c",
            "user.email=test@yoyo.local",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "init.defaultBranch=main",
        ]);
        cmd.args(args);
        cmd.current_dir(dir);
        cmd
    }

    /// Run a git command inside the fixture repo, panicking loudly — full
    /// command, exit status, stdout AND stderr — so broken fixtures surface
    /// as diagnostic test errors instead of confusing downstream assertion
    /// failures. Returns stdout so callers can assert postconditions.
    fn fixture_git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = fixture_git_command(dir, args)
            .output()
            .expect("git should be runnable in tests");
        assert!(
            out.status.success(),
            "git {args:?} failed in fixture repo {dir:?}\n  status: {}\n  stdout: {}\n  stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Assert the postcondition of a fixture commit: HEAD resolves and the
    /// committed file no longer appears in `git status --porcelain`. This
    /// catches the Day 125 CI failure mode (commit "succeeded" but the file
    /// stayed staged/dirty) at the helper level with a precise message.
    fn assert_committed(dir: &std::path::Path, name: &str) {
        let head = fixture_git(dir, &["rev-parse", "HEAD"]);
        assert!(
            !head.trim().is_empty(),
            "git rev-parse HEAD returned empty output in fixture repo {dir:?}"
        );
        let status = fixture_git(dir, &["status", "--porcelain"]);
        assert!(
            !status.lines().any(|l| l.ends_with(name)),
            "fixture commit of {name:?} was a no-op — file still dirty per \
             `git status --porcelain` in {dir:?}:\n{status}"
        );
    }

    /// Build a hermetic git repo fixture: init on branch `main`, local user
    /// config, one committed file (`committed.txt`), and one untracked file
    /// (`untracked.txt`) so git status is deterministic and non-empty —
    /// completely independent of the live repo's state (Day 124/125 lesson:
    /// no vacuous `if let` passes, no live-CWD reads).
    fn init_fixture_repo(dir: &std::path::Path) {
        fixture_git(dir, &["init", "-q"]);
        fixture_git(dir, &["config", "user.name", "yoyo-test"]);
        fixture_git(dir, &["config", "user.email", "test@yoyo.local"]);
        fixture_git(dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("committed.txt"), "hello\n").unwrap();
        fixture_git(dir, &["add", "committed.txt"]);
        fixture_git(dir, &["commit", "-q", "-m", "initial"]);
        assert_committed(dir, "committed.txt");
        // Normalize the branch name (init.defaultBranch varies by environment)
        fixture_git(dir, &["branch", "-M", "main"]);
        std::fs::write(dir.join("untracked.txt"), "scratch\n").unwrap();
    }

    /// Add and commit a file in the fixture repo, asserting the commit
    /// actually landed (not just that git exited 0).
    fn fixture_commit_file(dir: &std::path::Path, name: &str, contents: &str, msg: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
        fixture_git(dir, &["add", name]);
        fixture_git(dir, &["commit", "-q", "-m", msg]);
        assert_committed(dir, name);
    }

    #[test]
    fn test_project_context_file_names_not_empty() {
        assert_eq!(PROJECT_CONTEXT_FILES.len(), 6);
        // YOYO.md must be first — it's the canonical context file name
        assert_eq!(PROJECT_CONTEXT_FILES[0], "YOYO.md");
        // CLAUDE.md is a compatibility alias
        assert_eq!(PROJECT_CONTEXT_FILES[1], "CLAUDE.md");
        assert_eq!(PROJECT_CONTEXT_FILES[2], ".yoyo/instructions.md");
        // Cross-tool compatibility files
        assert_eq!(PROJECT_CONTEXT_FILES[3], "AGENTS.md");
        assert_eq!(PROJECT_CONTEXT_FILES[4], ".cursorrules");
        assert_eq!(PROJECT_CONTEXT_FILES[5], ".github/copilot-instructions.md");
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
    fn test_get_project_file_listing_hermetic() {
        // Hermetic: points at a temp fixture repo, not the live CWD, so it
        // cannot flake on dirty trees or shallow clones (Day 125 lesson).
        let dir = tempfile::TempDir::new().unwrap();
        init_fixture_repo(dir.path());
        let listing = get_project_file_listing_from(dir.path())
            .expect("fixture repo should always produce a file listing");
        assert!(
            listing.contains("committed.txt"),
            "Listing should contain the tracked file, got: {listing}"
        );
        assert!(
            !listing.contains("untracked.txt"),
            "ls-files should not include untracked files, got: {listing}"
        );
        let lines: Vec<&str> = listing.lines().collect();
        assert!(
            lines.len() <= MAX_PROJECT_FILES + 1, // +1 for possible "... and N more" line
            "File listing should be capped at {} files",
            MAX_PROJECT_FILES
        );
    }

    #[test]
    fn test_get_project_file_listing_live_smoke() {
        // Live-CWD smoke check: only asserts flake-proof properties (the CWD
        // wrapper must not panic and must delegate). Content assertions live
        // in the hermetic test above.
        let _ = get_project_file_listing();
    }

    #[test]
    #[serial]
    fn test_load_project_context_includes_file_listing() {
        // load_project_context should include project file listing when in a git repo.
        // Depends on the cwd being the project root. As of #780 no test in this
        // module moves the process CWD any more (the six that did now call the
        // `_from(dir)` twins), but movers remain elsewhere in this same test
        // binary — src/commands_file.rs, src/commands_git.rs, src/dispatch.rs,
        // src/dispatch_sub.rs. #[serial] is kept as partial mitigation and is
        // honestly only that: it orders this test against other #[serial] tests,
        // not against a non-serial test that chdir's. That gap is the whole
        // point of #780; the real fix is removing the remaining movers.
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
        // Hermetic: points at a temp fixture repo via load_project_context_from,
        // so it never reads the live CWD's git state (Day 125 lesson — the old
        // version hard-asserted but still flaked on dirty trees / shallow clones,
        // and before Day 124 it vacuously passed via `if let Some`).
        //
        // Day 125 root cause (evaluator round 2): load_project_context_from used
        // the CWD variant get_recently_changed_files() instead of the _from(dir)
        // variant, so under parallel execution the section came from whatever
        // directory a #[serial] test had chdir'd to — sometimes a repo with no
        // commits, making the section vanish. Fixed in production code; these
        // assertions are section-scoped so a regression can't hide behind the
        // Project Files listing (which also contains the fixture file names).
        let dir = tempfile::TempDir::new().unwrap();
        init_fixture_repo(dir.path());
        // A second commit so "recently changed" has more than the initial file.
        fixture_commit_file(dir.path(), "second.txt", "world\n", "add second");

        let context = load_project_context_from(dir.path())
            .expect("load_project_context_from should return Some in a fixture git repo");

        // Recently Changed Files must be present — the fixture has real commits,
        // so no conditional guard is needed (no vacuous passes).
        assert!(
            context.contains("## Recently Changed Files"),
            "Context should contain Recently Changed Files section, got: {context}"
        );
        let recent_section = recently_changed_section(&context);
        assert!(
            recent_section.contains("committed.txt"),
            "Recently Changed section should include the first committed file, \
             got section: {recent_section}\nfull context: {context}"
        );
        assert!(
            recent_section.contains("second.txt"),
            "Recently Changed section should include the second committed file, \
             got section: {recent_section}\nfull context: {context}"
        );

        // Git Status section is always present in a git repo
        assert!(
            context.contains("## Git Status"),
            "Context should always contain Git Status section, got: {context}"
        );
    }

    /// Extract the "## Recently Changed Files" section body from a context
    /// string (up to the next "## " header or end of string), so tests can
    /// assert on the section's actual contents instead of the whole context
    /// (where the Project Files listing also mentions the same file names).
    fn recently_changed_section(context: &str) -> &str {
        let start = context
            .find("## Recently Changed Files")
            .expect("caller must ensure the section exists");
        let body = &context[start..];
        match body[3..].find("\n## ") {
            Some(end) => &body[..end + 3],
            None => body,
        }
    }

    #[test]
    fn test_load_project_context_recently_changed_is_dir_scoped() {
        // Regression guard for the Day 125 flake root cause: the Recently
        // Changed section must be computed from the *passed dir*, never the
        // process CWD. If the CWD variant leaks back in, the section would
        // (nondeterministically) show live-repo files like src/context.rs —
        // or vanish entirely when a parallel #[serial] test parks the CWD in
        // a commitless temp repo.
        let dir = tempfile::TempDir::new().unwrap();
        init_fixture_repo(dir.path());
        fixture_commit_file(dir.path(), "second.txt", "world\n", "add second");

        let files = get_recently_changed_files_from(dir.path(), MAX_RECENT_FILES)
            .expect("fixture repo with two commits must yield recently changed files");
        assert!(
            files.iter().any(|f| f == "committed.txt"),
            "Fixture recently-changed must contain committed.txt, got: {files:?}"
        );
        assert!(
            files.iter().any(|f| f == "second.txt"),
            "Fixture recently-changed must contain second.txt, got: {files:?}"
        );
        // No live-repo paths may leak in: every entry must be a fixture file.
        for f in &files {
            assert!(
                f == "committed.txt" || f == "second.txt",
                "Recently-changed leaked a non-fixture path {f:?} — \
                 get_recently_changed_files_from is not dir-scoped. All: {files:?}"
            );
        }

        // And the assembled context's section must match the same dir-scoped list.
        let context = load_project_context_from(dir.path())
            .expect("load_project_context_from should return Some in a fixture git repo");
        let recent_section = recently_changed_section(&context);
        for line in recent_section.lines().skip(1).filter(|l| !l.is_empty()) {
            assert!(
                line == "committed.txt" || line == "second.txt",
                "Context Recently Changed section leaked a non-fixture path {line:?}; \
                 section: {recent_section}"
            );
        }
    }

    #[test]
    fn test_load_project_context_memories_are_dir_scoped() {
        // Project memories must load from `<dir>/.yoyo/memory.json`, not the
        // process CWD's — otherwise the real repo's memory entries (arbitrary
        // text) leak into hermetic fixture contexts under parallel execution.
        let dir = tempfile::TempDir::new().unwrap();
        init_fixture_repo(dir.path());

        // Without a fixture memory file, no memories section may appear —
        // regardless of what the live repo's .yoyo/memory.json contains.
        let context = load_project_context_from(dir.path())
            .expect("load_project_context_from should return Some in a fixture git repo");
        assert!(
            !context.contains("## Project Memories"),
            "Fixture without .yoyo/memory.json must not contain a memories \
             section (CWD memory leak), got: {context}"
        );

        // With a fixture memory file, its entry must appear.
        std::fs::create_dir_all(dir.path().join(".yoyo")).unwrap();
        std::fs::write(
            dir.path().join(".yoyo/memory.json"),
            r#"{"entries":[{"note":"fixture memory sentinel","timestamp":"2026-01-01 00:00","category":"general"}]}"#,
        )
        .unwrap();
        let context = load_project_context_from(dir.path())
            .expect("load_project_context_from should return Some in a fixture git repo");
        assert!(
            context.contains("## Project Memories"),
            "Fixture with .yoyo/memory.json must contain a memories section, got: {context}"
        );
        assert!(
            context.contains("fixture memory sentinel"),
            "Fixture memory entry must appear in the context, got: {context}"
        );
    }

    #[test]
    fn test_recently_changed_parsing_deduplicates_and_limits() {
        // Verify the parsing logic used by get_recently_changed_files:
        // empty lines filtered, duplicates removed, max_files respected.
        let simulated_output = "src/main.rs\n\nsrc/context.rs\nsrc/main.rs\nsrc/tools.rs\n";
        let mut seen = std::collections::HashSet::new();
        let max_files = 2;
        let files: Vec<String> = simulated_output
            .lines()
            .filter(|l| !l.is_empty())
            .filter(|l| seen.insert(l.to_string()))
            .take(max_files)
            .map(|l| l.to_string())
            .collect();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "src/main.rs");
        assert_eq!(files[1], "src/context.rs");
    }

    #[test]
    fn test_recently_changed_parsing_empty_input_returns_none() {
        // Simulates what happens when git log returns no matching files
        let simulated_output = "\n\n\n";
        let mut seen = std::collections::HashSet::new();
        let files: Vec<String> = simulated_output
            .lines()
            .filter(|l| !l.is_empty())
            .filter(|l| seen.insert(l.to_string()))
            .take(20)
            .map(|l| l.to_string())
            .collect();
        assert!(files.is_empty());
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
    #[serial]
    fn test_git_status_context_format() {
        // Needs #[serial]: expects the cwd to be a git repo (see above).
        let result = get_git_status_context().expect("Should be in a git repo");
        assert!(
            result.starts_with("## Git Status\n\n"),
            "Should start with '## Git Status' header"
        );
    }

    #[test]
    fn test_load_project_context_includes_git_status() {
        // Hermetic: points at a temp fixture repo via load_project_context_from,
        // so it never reads the live CWD's git state (Day 125 lesson — the old
        // version hard-asserted but still flaked on dirty trees / shallow clones,
        // and before Day 124 it vacuously passed via `if let Some` on both calls).
        let dir = tempfile::TempDir::new().unwrap();
        init_fixture_repo(dir.path());

        let context = load_project_context_from(dir.path())
            .expect("load_project_context_from should return Some in a fixture git repo");

        assert!(
            context.contains("## Git Status"),
            "Context should contain Git Status section, got: {context}"
        );
        // The fixture normalizes the branch to `main`, so the branch line is
        // deterministic regardless of the environment's init.defaultBranch.
        assert!(
            context.contains("Branch: main"),
            "Git status should report the fixture branch, got: {context}"
        );
        // Exactly one untracked file (untracked.txt) in a fresh fixture repo,
        // so the uncommitted count is deterministic and non-empty.
        assert!(
            context.contains("Uncommitted changes: 1 file"),
            "Git status should report the single untracked fixture file, got: {context}"
        );
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
        // Cross-tool compatibility files
        assert!(
            PROJECT_CONTEXT_FILES.contains(&"AGENTS.md"),
            "AGENTS.md should be supported (Gemini CLI)"
        );
        assert!(
            PROJECT_CONTEXT_FILES.contains(&".cursorrules"),
            ".cursorrules should be supported (Cursor)"
        );
        assert!(
            PROJECT_CONTEXT_FILES.contains(&".github/copilot-instructions.md"),
            ".github/copilot-instructions.md should be supported (GitHub Copilot)"
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

        let ctx = load_project_context_from(dir.path());

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
    fn test_project_context_includes_conventions_with_context_file() {
        // When YOYO.md exists, conventions should STILL be injected
        // (they complement explicit instructions)
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

        let ctx = load_project_context_from(dir.path());

        let ctx = ctx.unwrap();
        assert!(
            ctx.contains("## Development Conventions"),
            "Should include conventions even when YOYO.md exists"
        );
        assert!(
            ctx.contains("cargo"),
            "Rust conventions should mention cargo"
        );
        assert!(
            ctx.contains("Custom instructions"),
            "Should include YOYO.md content"
        );
        // Verify ordering: context file content comes BEFORE conventions
        let context_pos = ctx.find("Custom instructions").unwrap();
        let conventions_pos = ctx.find("## Development Conventions").unwrap();
        assert!(
            context_pos < conventions_pos,
            "Context file content should appear before conventions"
        );
    }

    #[test]
    fn test_load_cursorrules_file() {
        // A .cursorrules file should be loaded as project context
        use std::process::Command;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(".cursorrules"),
            "Always use TypeScript strict mode.",
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

        let ctx = load_project_context_from(dir.path());

        let ctx = ctx.unwrap();
        assert!(
            ctx.contains("Always use TypeScript strict mode"),
            "Should load .cursorrules content"
        );
    }

    #[test]
    fn test_load_agents_md_file() {
        // An AGENTS.md file should be loaded as project context
        use std::process::Command;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("AGENTS.md"),
            "# Agent Instructions\nUse pytest for testing.",
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

        let ctx = load_project_context_from(dir.path());

        let ctx = ctx.unwrap();
        assert!(
            ctx.contains("Use pytest for testing"),
            "Should load AGENTS.md content"
        );
    }

    #[test]
    fn test_load_copilot_instructions_file() {
        // A .github/copilot-instructions.md file should be loaded as project context
        use std::process::Command;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".github")).unwrap();
        std::fs::write(
            dir.path().join(".github/copilot-instructions.md"),
            "Follow Google style guide.",
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

        let ctx = load_project_context_from(dir.path());

        let ctx = ctx.unwrap();
        assert!(
            ctx.contains("Follow Google style guide"),
            "Should load .github/copilot-instructions.md content"
        );
    }

    #[test]
    fn test_multiple_context_files_get_separators() {
        // When multiple instruction files exist, secondary files should have
        // a "--- From <file> ---" separator for model clarity.
        use std::process::Command;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("YOYO.md"), "Primary instructions").unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "Agent instructions").unwrap();
        std::fs::write(dir.path().join(".cursorrules"), "Cursor rules").unwrap();
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

        let ctx = load_project_context_from(dir.path());

        let ctx = ctx.unwrap();
        // First file (YOYO.md) should NOT have a separator
        assert!(
            !ctx.contains("--- From YOYO.md ---"),
            "Primary file should not have a separator prefix"
        );
        // Secondary files should have separators
        assert!(
            ctx.contains("--- From AGENTS.md ---"),
            "AGENTS.md should have a separator: got: {ctx}"
        );
        assert!(
            ctx.contains("--- From .cursorrules ---"),
            ".cursorrules should have a separator: got: {ctx}"
        );
        // Content from all files should be present
        assert!(ctx.contains("Primary instructions"));
        assert!(ctx.contains("Agent instructions"));
        assert!(ctx.contains("Cursor rules"));
    }
}
