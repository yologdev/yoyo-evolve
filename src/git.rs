//! Git-related functions: staging, committing, branch detection, and `/git` subcommands.

use crate::format::*;

/// Git subcommands that modify repo state. Used by the `#[cfg(test)]` guard
/// in `run_git()` to prevent accidental destructive operations against the
/// real project repo during `cargo test`.
#[cfg(test)]
const DESTRUCTIVE_GIT_COMMANDS: &[&str] = &[
    "revert",
    "reset",
    "push",
    "commit",
    "checkout",
    "clean",
    "stash",
    "add",
    "merge",
    "rebase",
    "cherry-pick",
    "rm",
    "mv",
    // NOTE: `tag` and `branch` removed — they have common read-only modes
    // (`git tag --sort=...`, `git branch --show-current`) that production code
    // routes through `run_git`. The guard still covers the high-risk commands
    // (commit, push, reset, checkout, revert) that caused the original incident.
];

/// Locate the git subcommand in an argv and resolve the directory the
/// invocation actually targets, skipping any leading *global* flags.
///
/// The subcommand is NOT simply `args[0]`: git accepts globals before the verb
/// (`git -c k=v commit`, `git -C dir reset`), so a positional match reads the
/// flag and silently concludes the call is harmless. Returns `None` when the
/// argv carries no subcommand at all (empty, or flags only).
///
/// Directory resolution follows git's own semantics closely enough to avoid a
/// false positive: `-C <dir>` is applied cumulatively and relative to the
/// directory resolved so far, so `git -C /tmp/x commit` is correctly seen as
/// targeting `/tmp/x` rather than the project root. `--git-dir`/`--work-tree`
/// also move the repo, but where they point cannot be resolved this cheaply —
/// those deliberately leave the directory alone so the caller **fails closed**
/// on the project root rather than being waved through.
#[cfg(test)]
fn resolve_git_invocation<'a>(
    args: &'a [&'a str],
    cwd: &std::path::Path,
) -> Option<(&'a str, std::path::PathBuf)> {
    let mut dir = cwd.to_path_buf();
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        if !arg.starts_with('-') {
            return Some((arg, dir));
        }
        match *arg {
            // Redirecting flag with a separate value.
            "-C" => {
                dir = dir.join(args.get(i + 1)?);
                i += 2;
            }
            // Globals that consume a separate value but don't move the repo.
            "-c" | "--exec-path" | "--namespace" => i += 2,
            // Everything else (including `--git-dir=…`/`--work-tree=…`) is a
            // valueless or unresolvable global: skip it, keep `dir` as-is.
            _ => i += 1,
        }
    }
    None
}

/// Check whether a git invocation targets a destructive subcommand and is
/// running from the project root (i.e., the real repo, not a temp dir).
/// Returns `Some(subcommand)` when the call should be blocked, `None` when safe.
///
/// Accepts an explicit `cwd` so tests don't need `std::env::set_current_dir`
/// (which is process-global and causes flaky races under parallel test execution).
#[cfg(test)]
fn destructive_guard<'a>(args: &'a [&'a str], cwd: &std::path::Path) -> Option<&'a str> {
    let (subcmd, target) = resolve_git_invocation(args, cwd)?;
    if !DESTRUCTIVE_GIT_COMMANDS.contains(&subcmd) {
        return None;
    }
    let cwd = target.as_path();
    // Compare the supplied working dir against the compile-time project root.
    // If they match, we're in the real repo — block it.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if cwd == manifest_dir {
        Some(subcmd)
    } else {
        None
    }
}

/// Run a git command with the given args.
/// Returns `Ok(stdout_trimmed)` on success, `Err(stderr_trimmed)` on failure.
/// This is the common path for most git invocations — use raw `Command` only
/// when you need the full `Output` struct (e.g., for separate stdout+stderr handling).
///
/// # Test safety
/// Under `#[cfg(test)]`, destructive subcommands (commit, reset, revert, push, …)
/// are blocked with a panic when the working directory is the project root.
/// Tests that need destructive git operations should use a temp directory.
pub fn run_git(args: &[&str]) -> Result<String, String> {
    #[cfg(test)]
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(cmd) = destructive_guard(args, &cwd) {
            panic!(
                "SAFETY: run_git() called with destructive command '{}' from project root during \
                 tests. Use a temp directory or mock instead.",
                cmd
            );
        }
    }
    match std::process::Command::new("git").args(args).output() {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        Ok(output) => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        Err(e) => Err(format!("git not found: {e}")),
    }
}

/// Run a git command in a specific directory.
/// Returns `Ok(stdout_trimmed)` on success, `Err(stderr_trimmed)` on failure.
///
/// Uses `git -C <dir>` so the process working directory is unchanged.
/// The test safety guard checks `dir` (not `cwd`) against the project root.
pub fn run_git_in_dir(dir: &std::path::Path, args: &[&str]) -> Result<String, String> {
    #[cfg(test)]
    if let Some(cmd) = args.first() {
        if destructive_guard(args, dir).is_some() {
            panic!(
                "SAFETY: run_git_in_dir() called with destructive command '{}' targeting project \
                 root during tests. Use a temp directory or mock instead.",
                cmd
            );
        }
    }
    match std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
    {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        Ok(output) => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        Err(e) => Err(format!("git not found: {e}")),
    }
}

/// Run a git command and return the raw `Output` struct.
/// Use this when you need both stdout and stderr, or need to inspect the
/// exit status beyond success/failure (e.g. `git apply --check`).
///
/// Callers should prefer `run_git()` or `run_git_in_dir()` for the common
/// case of just needing stdout on success.
pub fn run_git_output(args: &[&str]) -> Result<std::process::Output, String> {
    #[cfg(test)]
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(cmd) = destructive_guard(args, &cwd) {
            panic!(
                "SAFETY: run_git_output() called with destructive command '{}' from project root \
                 during tests. Use a temp directory or mock instead.",
                cmd
            );
        }
    }
    std::process::Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("git not found: {e}"))
}

/// Get the current git branch name, if we're in a git repo.
pub fn git_branch() -> Option<String> {
    run_git(&["rev-parse", "--abbrev-ref", "HEAD"]).ok()
}

/// Get staged changes (git diff --cached).
/// Returns None if git fails, Some("") if nothing staged, or Some(diff) with the diff text.
pub fn get_staged_diff() -> Option<String> {
    run_git(&["diff", "--cached"]).ok()
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

/// The co-authored-by trailer appended to commits made through yoyo.
const CO_AUTHORED_TRAILER: &str = "Co-authored-by: yoyo <yoyo@users.noreply.github.com>";

/// Append a `Co-authored-by: yoyo` trailer to a commit message.
/// If the trailer is already present, returns the message unchanged.
pub fn append_co_authored_trailer(message: &str) -> String {
    if message.contains(CO_AUTHORED_TRAILER) {
        return message.to_string();
    }
    format!("{message}\n\n{CO_AUTHORED_TRAILER}")
}

/// Like `run_git_commit`, but appends a co-authored-by trailer first.
pub fn run_git_commit_with_trailer(message: &str) -> (bool, String) {
    let with_trailer = append_co_authored_trailer(message);
    run_git_commit(&with_trailer)
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

/// Apply ANSI colors to a unified diff string, line by line.
///
/// - Lines starting with `+` (but not `+++`): green (additions)
/// - Lines starting with `-` (but not `---`): red (deletions)
/// - Lines starting with `@@`: cyan (hunk headers)
/// - Lines starting with `diff --git`, `---`, `+++`: bold (file headers)
/// - All other lines: unchanged
pub fn colorize_diff(diff: &str) -> String {
    if diff.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(diff.len() * 2);
    for line in diff.lines() {
        if line.starts_with("diff --git") || line.starts_with("---") || line.starts_with("+++") {
            result.push_str(&format!("{BOLD}{line}{RESET}\n"));
        } else if line.starts_with("@@") {
            result.push_str(&format!("{CYAN}{line}{RESET}\n"));
        } else if line.starts_with('+') {
            result.push_str(&format!("{GREEN}{line}{RESET}\n"));
        } else if line.starts_with('-') {
            result.push_str(&format!("{RED}{line}{RESET}\n"));
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    // Remove trailing newline if the original didn't end with one
    if !diff.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Format `git stash list` output with colored entries.
///
/// Each line looks like: `stash@{0}: WIP on main: abc1234 commit message`
/// We dim the date/index part and bold the description.
pub fn format_stash_list(raw: &str) -> String {
    if raw.is_empty() {
        return format!("{DIM}  (no stashes){RESET}\n");
    }

    let mut result = String::with_capacity(raw.len() * 2);
    for line in raw.lines() {
        // Lines look like: stash@{N}: <type> on <branch>: <message>
        if let Some(colon_pos) = line.find(':') {
            let stash_ref = &line[..colon_pos];
            let rest = &line[colon_pos..];
            // Second colon separates "WIP on branch" from the commit message
            if let Some(second_colon) = rest[1..].find(':') {
                let middle = &rest[..second_colon + 1];
                let message = &rest[second_colon + 1..];
                result.push_str(&format!(
                    "  {YELLOW}{stash_ref}{RESET}{DIM}{middle}{RESET}:{BOLD}{message}{RESET}\n"
                ));
            } else {
                result.push_str(&format!("  {YELLOW}{stash_ref}{RESET}{DIM}{rest}{RESET}\n"));
            }
        } else {
            result.push_str(&format!("  {DIM}{line}{RESET}\n"));
        }
    }
    result
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
    /// `/git stash` or `/git stash push` — stash changes
    Stash,
    /// `/git stash pop` — pop stashed changes
    StashPop,
    /// `/git stash list` — list all stash entries
    StashList,
    /// `/git stash drop [n]` — drop a stash entry (default: stash@{0})
    StashDrop(Option<usize>),
    /// `/git stash show [n]` — show diff of a stash entry (default: stash@{0})
    StashShow(Option<usize>),
    /// `/git stage` — interactive file staging picker
    Stage,
    /// `/git diff` — show diff (unstaged by default, `--cached` for staged)
    Diff { cached: bool },
    /// `/git branch` — list branches or create/switch to a new one
    Branch(Option<String>),
    /// Invalid or missing subcommand — show help
    Help,
    /// Unrecognized top-level word (e.g. `/git statsu`) — carries the word as
    /// typed so the error can name it and suggest the nearest real one
    Unknown(String),
    /// Unrecognized word after `stash` (e.g. `/git stash pip`) — must NOT fall
    /// through to `stash push`, which would silently stash the working tree
    UnknownStash(String),
    /// `stash drop`/`stash show` with a non-numeric index — must NOT default
    /// to acting on stash@{0} (a typo'd index would destroy the wrong stash)
    BadStashIndex { action: String, arg: String },
}

/// Valid words after `/git stash` — single source of truth for both
/// `parse_git_args` and unknown-word error messages (never hand-copy this
/// list into a message; derive from it).
pub const STASH_SUBCOMMANDS: &[&str] = &["push", "pop", "list", "show", "drop"];

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
            if parts.len() >= 2 {
                match parts[1].to_lowercase().as_str() {
                    "pop" => GitSubcommand::StashPop,
                    "list" => GitSubcommand::StashList,
                    "show" => match parts.get(2).map(|s| s.trim()) {
                        None | Some("") => GitSubcommand::StashShow(None),
                        Some(s) => match s.parse::<usize>() {
                            Ok(n) => GitSubcommand::StashShow(Some(n)),
                            // A typo'd index must never silently mean stash@{0}
                            Err(_) => GitSubcommand::BadStashIndex {
                                action: "show".to_string(),
                                arg: s.to_string(),
                            },
                        },
                    },
                    "drop" => match parts.get(2).map(|s| s.trim()) {
                        None | Some("") => GitSubcommand::StashDrop(None),
                        Some(s) => match s.parse::<usize>() {
                            Ok(n) => GitSubcommand::StashDrop(Some(n)),
                            // Dropping stash@{0} on a typo'd index is data loss
                            Err(_) => GitSubcommand::BadStashIndex {
                                action: "drop".to_string(),
                                arg: s.to_string(),
                            },
                        },
                    },
                    "push" => GitSubcommand::Stash,
                    // A typo (e.g. `stash pip` for `pop`) must NOT fall through
                    // to `stash push` — that would stash the entire working
                    // tree when the user meant to restore it.
                    _ => GitSubcommand::UnknownStash(parts[1].to_string()),
                }
            } else {
                GitSubcommand::Stash
            }
        }
        "stage" => GitSubcommand::Stage,
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
        _ => GitSubcommand::Unknown(parts[0].to_string()),
    }
}

/// Parse `git status --porcelain` output into (status_code, filename) pairs.
pub fn parse_status_files(porcelain: &str) -> Vec<(String, String)> {
    porcelain
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let status = line[..2].trim().to_string();
            // Porcelain format: XY <space> filename (or XY <space> old -> new for renames)
            let filename = line[3..].trim().to_string();
            if filename.is_empty() {
                return None;
            }
            Some((status, filename))
        })
        .collect()
}

/// Parse a user's stage selection input into a list of 0-based indices.
///
/// Supports:
/// - Single numbers: `1`, `3`
/// - Ranges: `1-3`
/// - Comma-separated mix: `1,3,5-7`
/// - Glob patterns: `*.rs`, `src/*`
/// - `all` or `a`: select everything
///
/// Returns sorted, deduplicated 0-based indices. Out-of-range indices are silently dropped.
pub fn parse_stage_selection(input: &str, files: &[(String, String)]) -> Vec<usize> {
    let input = input.trim();
    if input.is_empty() || input == "q" {
        return vec![];
    }
    if input == "all" || input == "a" {
        return (0..files.len()).collect();
    }

    // Check if input looks like a glob (contains * or ?)
    if input.contains('*') || input.contains('?') {
        return files
            .iter()
            .enumerate()
            .filter(|(_, (_, name))| glob_match_simple(input, name))
            .map(|(i, _)| i)
            .collect();
    }

    // Parse comma-separated numbers and ranges
    let mut indices: Vec<usize> = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.contains('-') {
            // Range: "1-3"
            let bounds: Vec<&str> = part.splitn(2, '-').collect();
            if let (Ok(start), Ok(end)) = (
                bounds[0].trim().parse::<usize>(),
                bounds[1].trim().parse::<usize>(),
            ) {
                if start >= 1 && end >= start {
                    for i in start..=end {
                        if i >= 1 && i <= files.len() {
                            indices.push(i - 1);
                        }
                    }
                }
            }
        } else if let Ok(n) = part.parse::<usize>() {
            if n >= 1 && n <= files.len() {
                indices.push(n - 1);
            }
        }
    }
    indices.sort();
    indices.dedup();
    indices
}

/// Simple glob matching for file paths.
/// Supports `*` (match any sequence) and `?` (match single char).
fn glob_match_simple(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_chars(&pat, &txt)
}

fn glob_match_chars(pat: &[char], txt: &[char]) -> bool {
    let (mut pi, mut ti) = (0, 0);
    let (mut star_pi, mut star_ti) = (usize::MAX, 0);

    while ti < txt.len() {
        if pi < pat.len() && (pat[pi] == '?' || pat[pi] == txt[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == '*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }
    pi == pat.len()
}

/// Format a status code with a colored label for the stage display.
fn format_status_label(status: &str) -> String {
    match status {
        "M" => format!("{YELLOW}M{RESET}"),
        "A" => format!("{GREEN}A{RESET}"),
        "D" => format!("{RED}D{RESET}"),
        "R" => format!("{CYAN}R{RESET}"),
        "??" => format!("{DIM}?{RESET}"),
        other => format!("{DIM}{other}{RESET}"),
    }
}

/// Execute a `/git` subcommand directly (no AI, no tokens).
/// Build the `/git` help text as a single string (pure, testable).
///
/// Un-driftable by construction: the completeness test below walks
/// `crate::commands::GIT_SUBCOMMANDS` and `STASH_SUBCOMMANDS` and asserts
/// every subcommand appears here — adding a `/git` subcommand without
/// updating this help is a test failure, not a silent stale surface
/// (Day 140 lesson: hand-typed enumerations near the real list drift).
fn git_help_text() -> String {
    format!(
        "{DIM}  usage: /git status             Show working tree status\n\
         \x20        /git log [n]             Show last n commits (default: 5)\n\
         \x20        /git add <path>          Stage files for commit\n\
         \x20        /git stage               Interactive file staging picker\n\
         \x20        /git diff [--cached]     Show diff (unstaged or staged changes)\n\
         \x20        /git branch [name]       List branches or create & switch\n\
         \x20        /git stash               Stash uncommitted changes (alias: stash push)\n\
         \x20        /git stash pop           Restore stashed changes\n\
         \x20        /git stash list          List all stash entries\n\
         \x20        /git stash show [n]      Show diff of stash entry n\n\
         \x20        /git stash drop [n]      Drop stash entry n{RESET}\n\n"
    )
}

pub fn run_git_subcommand(subcmd: &GitSubcommand) {
    match subcmd {
        GitSubcommand::Status => match run_git(&["status", "--short"]) {
            Ok(text) if text.is_empty() => {
                println!("{DIM}  (clean working tree){RESET}\n");
            }
            Ok(text) => {
                println!("{DIM}{text}{RESET}");
            }
            Err(_) => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
        },
        GitSubcommand::Log(n) => {
            let n_str = n.to_string();
            match run_git(&["log", "--oneline", "-n", &n_str]) {
                Ok(text) if text.is_empty() => {
                    println!("{DIM}  (no commits yet){RESET}\n");
                }
                Ok(text) => {
                    println!("{DIM}{text}{RESET}");
                }
                Err(_) => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            }
        }
        GitSubcommand::Add(path) => match run_git(&["add", path]) {
            Ok(_) => {
                println!("{GREEN}  ✓ staged: {path}{RESET}\n");
            }
            Err(e) => {
                if e.contains("git not found") {
                    eprintln!("{RED}  error: git not found{RESET}\n");
                } else {
                    eprintln!("{RED}  error: {e}{RESET}\n");
                }
            }
        },
        GitSubcommand::Stash => match run_git(&["stash", "push"]) {
            Ok(text) => {
                println!("{GREEN}  ✓ {text}{RESET}\n");
            }
            Err(e) => {
                if e.contains("git not found") {
                    eprintln!("{RED}  error: git not found{RESET}\n");
                } else {
                    eprintln!("{RED}  error: {e}{RESET}\n");
                }
            }
        },
        GitSubcommand::StashPop => match run_git(&["stash", "pop"]) {
            Ok(text) => {
                println!("{GREEN}  ✓ {text}{RESET}\n");
            }
            Err(e) => {
                if e.contains("git not found") {
                    eprintln!("{RED}  error: git not found{RESET}\n");
                } else {
                    eprintln!("{RED}  error: {e}{RESET}\n");
                }
            }
        },
        GitSubcommand::StashList => match run_git(&["stash", "list"]) {
            Ok(text) => {
                print!("{}", format_stash_list(&text));
            }
            Err(e) => {
                if e.contains("git not found") {
                    eprintln!("{RED}  error: git not found{RESET}\n");
                } else {
                    eprintln!("{RED}  error: {e}{RESET}\n");
                }
            }
        },
        GitSubcommand::StashDrop(n) => {
            let stash_ref = match n {
                Some(idx) => format!("stash@{{{idx}}}"),
                None => "stash@{0}".to_string(),
            };
            match run_git(&["stash", "drop", &stash_ref]) {
                Ok(text) => {
                    println!("{GREEN}  ✓ {text}{RESET}\n");
                }
                Err(e) => {
                    if e.contains("git not found") {
                        eprintln!("{RED}  error: git not found{RESET}\n");
                    } else {
                        eprintln!("{RED}  error: {e}{RESET}\n");
                    }
                }
            }
        }
        GitSubcommand::StashShow(n) => {
            let stash_ref = match n {
                Some(idx) => format!("stash@{{{idx}}}"),
                None => "stash@{0}".to_string(),
            };
            match run_git(&["stash", "show", "-p", &stash_ref]) {
                Ok(text) if text.is_empty() => {
                    println!("{DIM}  (empty stash){RESET}\n");
                }
                Ok(text) => {
                    println!("{}", colorize_diff(&text));
                }
                Err(e) => {
                    if e.contains("git not found") {
                        eprintln!("{RED}  error: git not found{RESET}\n");
                    } else {
                        eprintln!("{RED}  error: {e}{RESET}\n");
                    }
                }
            }
        }
        GitSubcommand::Diff { cached } => {
            let args: Vec<&str> = if *cached {
                vec!["diff", "--cached"]
            } else {
                vec!["diff"]
            };
            match run_git(&args) {
                Ok(text) if text.is_empty() => {
                    let scope = if *cached { "staged" } else { "unstaged" };
                    println!("{DIM}  (no {scope} changes){RESET}\n");
                }
                Ok(text) => {
                    println!("{text}");
                }
                Err(_) => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            }
        }
        GitSubcommand::Branch(name) => match name {
            Some(branch_name) => match run_git(&["checkout", "-b", branch_name]) {
                Ok(_) => {
                    println!("{GREEN}  ✓ switched to new branch '{branch_name}'{RESET}\n");
                }
                Err(e) => {
                    if e.contains("git not found") {
                        eprintln!("{RED}  error: git not found{RESET}\n");
                    } else {
                        eprintln!("{RED}  error: {e}{RESET}\n");
                    }
                }
            },
            None => match run_git(&["branch", "--list", "-a"]) {
                Ok(text) if text.is_empty() => {
                    println!("{DIM}  (no branches yet){RESET}\n");
                }
                Ok(text) => {
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
                Err(_) => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
            },
        },
        GitSubcommand::Stage => {
            let porcelain = match run_git(&["status", "--porcelain"]) {
                Ok(text) => text,
                Err(_) => {
                    eprintln!("{RED}  error: not in a git repository{RESET}\n");
                    return;
                }
            };
            if porcelain.is_empty() {
                println!("{DIM}  (nothing to stage — clean working tree){RESET}\n");
                return;
            }
            let files = parse_status_files(&porcelain);
            if files.is_empty() {
                println!("{DIM}  (nothing to stage){RESET}\n");
                return;
            }
            // Display numbered list
            println!("{DIM}  Modified/untracked files:{RESET}");
            for (i, (status, name)) in files.iter().enumerate() {
                let label = format_status_label(status);
                println!("    {BOLD}{}. {RESET}[{label}] {name}", i + 1);
            }
            // Prompt for selection
            eprint!("\n  {DIM}Stage which? (1,3 / 1-3 / *.rs / all / q): {RESET}");
            std::io::Write::flush(&mut std::io::stderr()).ok();
            let mut input = String::new();
            if std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut input).is_err() {
                return;
            }
            let selected = parse_stage_selection(input.trim(), &files);
            if selected.is_empty() {
                println!("{DIM}  (cancelled){RESET}\n");
                return;
            }
            // Stage selected files
            let mut staged_count = 0;
            for idx in &selected {
                let (_, ref path) = files[*idx];
                if run_git(&["add", path]).is_ok() {
                    staged_count += 1;
                } else {
                    eprintln!("{RED}  error staging: {path}{RESET}");
                }
            }
            if staged_count > 0 {
                println!(
                    "{GREEN}  ✓ staged {staged_count} file{}{RESET}\n",
                    if staged_count == 1 { "" } else { "s" }
                );
            }
        }
        GitSubcommand::Help => {
            print!("{}", git_help_text());
        }
        GitSubcommand::Unknown(_)
        | GitSubcommand::UnknownStash(_)
        | GitSubcommand::BadStashIndex { .. } => {
            // Message logic lives in commands_git::unknown_git_message (single
            // source — it derives suggestions from the real subcommand lists).
            if let Some(msg) = crate::commands_git::unknown_git_message(subcmd) {
                println!("{RED}  {msg}{RESET}\n");
            }
        }
    }
}

/// Detect the base branch for PR creation (main or master).
/// Returns "main" if it exists, otherwise "master", falling back to "main".
pub fn detect_base_branch() -> String {
    if run_git(&["rev-parse", "--verify", "main"]).is_ok() {
        return "main".to_string();
    }
    if run_git(&["rev-parse", "--verify", "master"]).is_ok() {
        return "master".to_string();
    }
    "main".to_string()
}

/// Get the diff between the current branch and a base branch.
/// Returns None if git fails, Some(diff) with the diff text otherwise.
pub fn get_branch_diff(base: &str) -> Option<String> {
    let merge_base_sha = run_git(&["merge-base", base, "HEAD"]).ok()?;
    run_git(&["diff", &merge_base_sha, "HEAD"]).ok()
}

/// Get the list of commits on the current branch since diverging from the base branch.
/// Returns None if git fails, Some(commits) with one-line commit summaries otherwise.
pub fn get_branch_commits(base: &str) -> Option<String> {
    let range = format!("{base}..HEAD");
    run_git(&["log", "--oneline", &range]).ok()
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
        let truncated = safe_truncate(diff, max_diff_chars);
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

/// Count how many commits touched each `src/**/*.rs` file in the last `days` days.
/// Returns `(filename, count)` pairs, unsorted.
pub fn file_change_counts(days: u32) -> Vec<(String, u32)> {
    let since_arg = format!("{days} days ago");
    let output = match run_git(&[
        "log",
        &format!("--since={since_arg}"),
        "--name-only",
        "--pretty=format:",
    ]) {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for line in output.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        // Filter to only src/**/*.rs files
        if path.starts_with("src/") && path.ends_with(".rs") {
            *counts.entry(path.to_string()).or_insert(0) += 1;
        }
    }

    counts.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_git_valid_args() {
        // `git --version` should always succeed
        let result = run_git(&["--version"]);
        assert!(result.is_ok(), "git --version should succeed");
        let stdout = result.unwrap();
        assert!(
            stdout.contains("git version"),
            "Output should contain 'git version', got: {stdout}"
        );
    }

    #[test]
    fn test_run_git_invalid_args_returns_err() {
        // `git --no-such-flag-exists` should fail
        let result = run_git(&["--no-such-flag-exists"]);
        assert!(
            result.is_err(),
            "Invalid git flag should return Err, got: {:?}",
            result
        );
    }

    #[test]
    fn test_run_git_trims_output() {
        // git --version output shouldn't have trailing newlines
        let result = run_git(&["--version"]).unwrap();
        assert_eq!(result, result.trim(), "Output should be trimmed");
    }

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
        // Unknown words no longer silently show generic help — they name the word
        // so handle_git can print an honest error with a did-you-mean suggestion.
        assert_eq!(
            parse_git_args("unknown"),
            GitSubcommand::Unknown("unknown".to_string())
        );
        assert_eq!(
            parse_git_args("push"),
            GitSubcommand::Unknown("push".to_string())
        );
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
    fn test_git_subcommand_stash_list() {
        assert_eq!(parse_git_args("stash list"), GitSubcommand::StashList);
        assert_eq!(parse_git_args("STASH LIST"), GitSubcommand::StashList);
        assert_eq!(parse_git_args("stash List"), GitSubcommand::StashList);
    }

    #[test]
    fn test_git_subcommand_stash_show() {
        assert_eq!(parse_git_args("stash show"), GitSubcommand::StashShow(None));
        assert_eq!(
            parse_git_args("stash show 2"),
            GitSubcommand::StashShow(Some(2))
        );
        assert_eq!(
            parse_git_args("STASH SHOW 0"),
            GitSubcommand::StashShow(Some(0))
        );
        // Non-numeric argument no longer silently falls back to stash@{0} —
        // it surfaces as a bad-index error instead of showing the wrong stash.
        assert_eq!(
            parse_git_args("stash show abc"),
            GitSubcommand::BadStashIndex {
                action: "show".to_string(),
                arg: "abc".to_string(),
            }
        );
    }

    #[test]
    fn test_git_subcommand_stash_drop() {
        assert_eq!(parse_git_args("stash drop"), GitSubcommand::StashDrop(None));
        assert_eq!(
            parse_git_args("stash drop 3"),
            GitSubcommand::StashDrop(Some(3))
        );
        assert_eq!(
            parse_git_args("STASH DROP 1"),
            GitSubcommand::StashDrop(Some(1))
        );
        // Non-numeric argument must NOT silently drop stash@{0} — that's data loss
        assert_eq!(
            parse_git_args("stash drop xyz"),
            GitSubcommand::BadStashIndex {
                action: "drop".to_string(),
                arg: "xyz".to_string()
            }
        );
    }

    /// Fixture table for the destructive-silent-swallow class (Day 142):
    /// typo'd stash words must never fall through to `stash push` (which would
    /// stash the user's entire working tree), and bad indices must never
    /// default to acting on stash@{0}.
    #[test]
    fn test_git_unknown_inputs_never_destructive() {
        let cases: &[(&str, GitSubcommand)] = &[
            // typo'd stash word — must NOT become Stash (push)
            ("stash pip", GitSubcommand::UnknownStash("pip".to_string())),
            (
                "stash pops",
                GitSubcommand::UnknownStash("pops".to_string()),
            ),
            (
                "STASH Aply",
                GitSubcommand::UnknownStash("Aply".to_string()),
            ),
            // bad index — must NOT default to stash@{0}
            (
                "stash drop abc",
                GitSubcommand::BadStashIndex {
                    action: "drop".to_string(),
                    arg: "abc".to_string(),
                },
            ),
            (
                "stash show 1x",
                GitSubcommand::BadStashIndex {
                    action: "show".to_string(),
                    arg: "1x".to_string(),
                },
            ),
            // top-level unknown word — named, not generic help
            ("statsu", GitSubcommand::Unknown("statsu".to_string())),
            ("push", GitSubcommand::Unknown("push".to_string())),
        ];
        for (input, expected) in cases {
            assert_eq!(&parse_git_args(input), expected, "input: {input:?}");
        }
        // Still-valid shapes are untouched
        assert_eq!(parse_git_args("stash"), GitSubcommand::Stash);
        assert_eq!(parse_git_args("stash push"), GitSubcommand::Stash);
        assert_eq!(parse_git_args("stash pop"), GitSubcommand::StashPop);
        assert_eq!(
            parse_git_args("stash drop 2"),
            GitSubcommand::StashDrop(Some(2))
        );
        assert_eq!(parse_git_args("stash drop"), GitSubcommand::StashDrop(None));
        // Trailing whitespace after show/drop still means "no index given"
        assert_eq!(
            parse_git_args("stash show "),
            GitSubcommand::StashShow(None)
        );
    }

    /// Drift guard: every word in STASH_SUBCOMMANDS must parse to a real
    /// (non-Unknown) variant — the const and the match arms are two surfaces
    /// of one fact (Day 140: derive enumerations, don't copy them).
    #[test]
    fn test_stash_subcommands_const_covers_parse() {
        for word in STASH_SUBCOMMANDS {
            let parsed = parse_git_args(&format!("stash {word}"));
            assert!(
                !matches!(
                    parsed,
                    GitSubcommand::UnknownStash(_) | GitSubcommand::Unknown(_)
                ),
                "STASH_SUBCOMMANDS lists '{word}' but parse_git_args treats it as unknown"
            );
        }
    }

    /// Drift guard (Day 142): the `/git` help text must mention every
    /// subcommand in `commands::GIT_SUBCOMMANDS` and every stash sub-word in
    /// `STASH_SUBCOMMANDS`. Adding a subcommand without updating the help is
    /// now a test failure instead of a silently stale surface — the exact
    /// drift class from Day 140 (hand-typed enumeration near the real list).
    #[test]
    fn test_git_help_text_covers_all_subcommands() {
        let help = git_help_text();
        assert!(!help.is_empty(), "git_help_text() must not be empty");
        for sub in crate::commands::GIT_SUBCOMMANDS {
            assert!(
                help.contains(&format!("/git {sub}")),
                "git_help_text() is missing '/git {sub}' — GIT_SUBCOMMANDS lists it"
            );
        }
        for word in STASH_SUBCOMMANDS {
            assert!(
                help.contains(&format!("stash {word}")),
                "git_help_text() is missing 'stash {word}' — STASH_SUBCOMMANDS lists it"
            );
        }
    }

    /// Same guard for the /help surface: the help_data.rs `/git` entry is a
    /// second prose copy of the same enumeration (and was missing 'stage'
    /// when this test landed — live proof of the drift class).
    #[test]
    fn test_help_data_git_entry_covers_all_subcommands() {
        let entry =
            crate::help_data::command_help("git").expect("help_data must have a /git entry");
        for sub in crate::commands::GIT_SUBCOMMANDS {
            assert!(
                entry.contains(&format!("/git {sub}")),
                "help_data /git entry is missing '/git {sub}' — GIT_SUBCOMMANDS lists it"
            );
        }
    }

    #[test]
    fn test_git_subcommand_stash_push() {
        // "stash push" is an explicit alias for "stash"
        assert_eq!(parse_git_args("stash push"), GitSubcommand::Stash);
        assert_eq!(parse_git_args("STASH PUSH"), GitSubcommand::Stash);
    }

    #[test]
    fn test_format_stash_list_empty() {
        let result = format_stash_list("");
        assert!(
            result.contains("no stashes"),
            "Empty input should show 'no stashes': {result}"
        );
    }

    #[test]
    fn test_format_stash_list_single_entry() {
        let input = "stash@{0}: WIP on main: abc1234 fix tests";
        let result = format_stash_list(input);
        // Should contain the stash ref
        assert!(
            result.contains("stash@{0}"),
            "Should contain stash ref: {result}"
        );
        // Should contain the message
        assert!(
            result.contains("fix tests"),
            "Should contain the message: {result}"
        );
    }

    #[test]
    fn test_format_stash_list_multiple_entries() {
        let input = "\
stash@{0}: WIP on main: abc1234 fix tests
stash@{1}: On feature: def5678 wip stuff";
        let result = format_stash_list(input);
        assert!(
            result.contains("stash@{0}"),
            "Should contain first stash ref: {result}"
        );
        assert!(
            result.contains("stash@{1}"),
            "Should contain second stash ref: {result}"
        );
        assert!(
            result.contains("fix tests"),
            "Should contain first message: {result}"
        );
        assert!(
            result.contains("wip stuff"),
            "Should contain second message: {result}"
        );
    }

    #[test]
    fn test_format_stash_list_uses_ansi_colors() {
        let input = "stash@{0}: WIP on main: abc1234 fix tests";
        let result = format_stash_list(input);
        // Should use YELLOW for stash ref
        assert!(
            result.contains("\x1b[33m"),
            "Should use YELLOW ANSI code: {result}"
        );
        // Should use BOLD for message
        assert!(
            result.contains("\x1b[1m"),
            "Should use BOLD ANSI code: {result}"
        );
        // Should use DIM for middle part
        assert!(
            result.contains("\x1b[2m"),
            "Should use DIM ANSI code: {result}"
        );
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

    // ── colorize_diff tests ──────────────────────────────────────────────

    #[test]
    fn colorize_diff_green_for_additions() {
        let diff = "+added line\n context\n";
        let result = colorize_diff(diff);
        assert!(
            result.contains("\x1b[32m+added line\x1b[0m"),
            "Addition lines should be green: {result}"
        );
    }

    #[test]
    fn colorize_diff_red_for_deletions() {
        let diff = "-removed line\n context\n";
        let result = colorize_diff(diff);
        assert!(
            result.contains("\x1b[31m-removed line\x1b[0m"),
            "Deletion lines should be red: {result}"
        );
    }

    #[test]
    fn colorize_diff_cyan_for_hunk_headers() {
        let diff = "@@ -1,3 +1,4 @@\n context\n";
        let result = colorize_diff(diff);
        assert!(
            result.contains("\x1b[36m@@ -1,3 +1,4 @@\x1b[0m"),
            "Hunk headers should be cyan: {result}"
        );
    }

    #[test]
    fn colorize_diff_bold_for_file_headers() {
        let diff = "diff --git a/foo.rs b/foo.rs\n--- a/foo.rs\n+++ b/foo.rs\n";
        let result = colorize_diff(diff);
        assert!(
            result.contains("\x1b[1mdiff --git a/foo.rs b/foo.rs\x1b[0m"),
            "diff --git lines should be bold: {result}"
        );
        assert!(
            result.contains("\x1b[1m--- a/foo.rs\x1b[0m"),
            "--- lines should be bold: {result}"
        );
        assert!(
            result.contains("\x1b[1m+++ b/foo.rs\x1b[0m"),
            "+++ lines should be bold: {result}"
        );
    }

    #[test]
    fn colorize_diff_context_lines_unchanged() {
        let diff = " context line\nanother context\n";
        let result = colorize_diff(diff);
        assert!(
            result.contains(" context line\n"),
            "Context lines should be unchanged: {result}"
        );
        assert!(
            result.contains("another context\n"),
            "Context lines should be unchanged: {result}"
        );
        // Should NOT contain any ANSI codes on context lines
        assert!(
            !result.contains("\x1b[32m context line"),
            "Context lines should not be colored"
        );
    }

    #[test]
    fn colorize_diff_empty_input() {
        let result = colorize_diff("");
        assert_eq!(result, "", "Empty input should return empty output");
    }

    // ── co-authored-by trailer tests ─────────────────────────────────────

    #[test]
    fn co_authored_trailer_normal_message() {
        let result = append_co_authored_trailer("fix: typo");
        assert_eq!(
            result,
            "fix: typo\n\nCo-authored-by: yoyo <yoyo@users.noreply.github.com>"
        );
    }

    #[test]
    fn co_authored_trailer_empty_message() {
        let result = append_co_authored_trailer("");
        assert!(
            result.contains("Co-authored-by: yoyo"),
            "Should still append trailer to empty message"
        );
    }

    #[test]
    fn co_authored_trailer_already_present() {
        let msg = "fix: typo\n\nCo-authored-by: yoyo <yoyo@users.noreply.github.com>";
        let result = append_co_authored_trailer(msg);
        assert_eq!(result, msg, "Should not duplicate existing trailer");
    }

    #[test]
    fn co_authored_trailer_multiline_message() {
        let msg = "feat: add new command\n\nThis adds a cool new feature\nwith multiple lines.";
        let result = append_co_authored_trailer(msg);
        assert!(
            result.starts_with(msg),
            "Original message should be preserved"
        );
        assert!(
            result.ends_with("Co-authored-by: yoyo <yoyo@users.noreply.github.com>"),
            "Trailer should be at the end"
        );
        // Ensure proper blank line separation
        assert!(
            result.contains("\n\nCo-authored-by:"),
            "Trailer should be separated by a blank line"
        );
    }

    // --- Destructive guard tests ---

    #[test]
    fn destructive_guard_allows_safe_commands() {
        // Read-only commands should never be blocked, even from project root
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for safe in &[
            &["--version"][..],
            &["rev-parse", "--abbrev-ref", "HEAD"],
            &["log", "--oneline", "-5"],
            &["diff", "--cached"],
            &["status"],
            &["show", "HEAD"],
            &["tag", "--sort=-creatordate"],
            &["tag", "-l", "evolution-*"],
            &["branch", "--show-current"],
            &["ls-files", "src/"],
        ] {
            assert!(
                destructive_guard(safe, project_root).is_none(),
                "Safe command {:?} should not be blocked",
                safe
            );
        }
    }

    #[test]
    fn destructive_guard_blocks_known_bad_commands_in_project_root() {
        // Pass the project root explicitly — should trigger for every destructive command
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for cmd in DESTRUCTIVE_GIT_COMMANDS {
            let args = &[*cmd, "--help"];
            let result = destructive_guard(&args[..], project_root);
            assert!(
                result.is_some(),
                "Destructive command '{}' should be blocked from project root",
                cmd
            );
            assert_eq!(result.unwrap(), *cmd);
        }
    }

    #[test]
    fn destructive_guard_allows_destructive_in_temp_dir() {
        // Pass a temp directory as cwd — destructive commands should be allowed.
        // No std::env::set_current_dir needed — that was the source of the race.
        let tmp = std::env::temp_dir();
        let result = destructive_guard(&["commit", "-m", "test"], &tmp);
        assert!(
            result.is_none(),
            "Destructive command in temp dir should NOT be blocked"
        );
    }

    #[test]
    fn destructive_guard_empty_args() {
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(
            destructive_guard(&[], project_root).is_none(),
            "Empty args should pass"
        );
    }

    #[test]
    fn destructive_guard_list_covers_original_incident() {
        // The original incident was `run_git(&["revert", "HEAD", "--no-edit"])`
        assert!(
            DESTRUCTIVE_GIT_COMMANDS.contains(&"revert"),
            "revert must be in destructive list (original incident)"
        );
        assert!(
            DESTRUCTIVE_GIT_COMMANDS.contains(&"reset"),
            "reset must be in destructive list"
        );
        assert!(
            DESTRUCTIVE_GIT_COMMANDS.contains(&"push"),
            "push must be in destructive list"
        );
    }

    #[test]
    fn run_git_safe_command_passes_guard() {
        // Sanity check: run_git with a safe command still works
        let result = run_git(&["--version"]);
        assert!(result.is_ok());
    }

    #[test]
    #[should_panic(expected = "SAFETY: run_git() called with destructive command")]
    fn run_git_panics_on_destructive_from_project_root() {
        // This should panic because we're in the project root during cargo test
        let _ = run_git(&["revert", "HEAD", "--no-edit"]);
    }

    #[test]
    fn test_parse_git_args_stage() {
        assert_eq!(parse_git_args("stage"), GitSubcommand::Stage);
        assert_eq!(parse_git_args("STAGE"), GitSubcommand::Stage);
        assert_eq!(parse_git_args("Stage"), GitSubcommand::Stage);
    }

    #[test]
    fn test_parse_status_files_basic() {
        let porcelain = " M src/main.rs\n?? new_file.rs\n D deleted.rs\n";
        let files = parse_status_files(porcelain);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], ("M".to_string(), "src/main.rs".to_string()));
        assert_eq!(files[1], ("??".to_string(), "new_file.rs".to_string()));
        assert_eq!(files[2], ("D".to_string(), "deleted.rs".to_string()));
    }

    #[test]
    fn test_parse_status_files_empty() {
        assert_eq!(parse_status_files(""), Vec::<(String, String)>::new());
        assert_eq!(parse_status_files("\n"), Vec::<(String, String)>::new());
    }

    #[test]
    fn test_parse_stage_selection_all() {
        let files = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("M".to_string(), "b.rs".to_string()),
            ("??".to_string(), "c.rs".to_string()),
        ];
        assert_eq!(parse_stage_selection("all", &files), vec![0, 1, 2]);
        assert_eq!(parse_stage_selection("a", &files), vec![0, 1, 2]);
    }

    #[test]
    fn test_parse_stage_selection_single() {
        let files = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("M".to_string(), "b.rs".to_string()),
            ("??".to_string(), "c.rs".to_string()),
        ];
        assert_eq!(parse_stage_selection("1", &files), vec![0]);
        assert_eq!(parse_stage_selection("2", &files), vec![1]);
        assert_eq!(parse_stage_selection("3", &files), vec![2]);
    }

    #[test]
    fn test_parse_stage_selection_range() {
        let files = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("M".to_string(), "b.rs".to_string()),
            ("??".to_string(), "c.rs".to_string()),
            ("D".to_string(), "d.rs".to_string()),
        ];
        assert_eq!(parse_stage_selection("1-3", &files), vec![0, 1, 2]);
        assert_eq!(parse_stage_selection("2-4", &files), vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_stage_selection_comma_separated() {
        let files = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("M".to_string(), "b.rs".to_string()),
            ("??".to_string(), "c.rs".to_string()),
            ("D".to_string(), "d.rs".to_string()),
        ];
        assert_eq!(parse_stage_selection("1,3", &files), vec![0, 2]);
        assert_eq!(parse_stage_selection("1,3,4", &files), vec![0, 2, 3]);
    }

    #[test]
    fn test_parse_stage_selection_mixed() {
        let files = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("M".to_string(), "b.rs".to_string()),
            ("??".to_string(), "c.rs".to_string()),
            ("D".to_string(), "d.rs".to_string()),
            ("M".to_string(), "e.rs".to_string()),
        ];
        assert_eq!(parse_stage_selection("1,3-5", &files), vec![0, 2, 3, 4]);
    }

    #[test]
    fn test_parse_stage_selection_glob() {
        let files = vec![
            ("M".to_string(), "src/main.rs".to_string()),
            ("M".to_string(), "src/lib.rs".to_string()),
            ("??".to_string(), "README.md".to_string()),
            ("M".to_string(), "Cargo.toml".to_string()),
        ];
        assert_eq!(parse_stage_selection("*.rs", &files), vec![0, 1]);
        assert_eq!(parse_stage_selection("*.md", &files), vec![2]);
        assert_eq!(parse_stage_selection("src/*", &files), vec![0, 1]);
    }

    #[test]
    fn test_parse_stage_selection_quit_and_empty() {
        let files = vec![("M".to_string(), "a.rs".to_string())];
        assert_eq!(parse_stage_selection("q", &files), Vec::<usize>::new());
        assert_eq!(parse_stage_selection("", &files), Vec::<usize>::new());
        assert_eq!(parse_stage_selection("  ", &files), Vec::<usize>::new());
    }

    #[test]
    fn test_parse_stage_selection_out_of_range() {
        let files = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("M".to_string(), "b.rs".to_string()),
        ];
        // Out-of-range numbers are silently dropped
        assert_eq!(parse_stage_selection("5", &files), Vec::<usize>::new());
        assert_eq!(parse_stage_selection("0", &files), Vec::<usize>::new());
        // But valid ones remain
        assert_eq!(parse_stage_selection("1,5", &files), vec![0]);
    }

    #[test]
    fn test_parse_stage_selection_deduplicates() {
        let files = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("M".to_string(), "b.rs".to_string()),
            ("M".to_string(), "c.rs".to_string()),
        ];
        // Overlapping range and explicit number
        assert_eq!(parse_stage_selection("1-2,2", &files), vec![0, 1]);
    }

    #[test]
    fn test_glob_match_simple_patterns() {
        assert!(glob_match_simple("*.rs", "main.rs"));
        assert!(glob_match_simple("*.rs", "src/lib.rs"));
        assert!(!glob_match_simple("*.rs", "main.toml"));
        assert!(glob_match_simple("src/*", "src/main.rs"));
        assert!(!glob_match_simple("src/*", "tests/main.rs"));
        assert!(glob_match_simple("??.rs", "ab.rs"));
        assert!(!glob_match_simple("??.rs", "abc.rs"));
    }

    #[test]
    fn test_file_change_counts_returns_results() {
        // In the yoyo repo, there should be at least some changed files in the last 30 days
        // This test runs against the real repo — it's an integration-style test
        let counts = file_change_counts(30);
        // Don't assert specific files, but the function should not panic
        // and should return some results in a repo with history
        // (may be empty in shallow clones, which is fine)
        let _ = counts; // just verify it doesn't panic
    }
}
