//! Startup banner, welcome text, and git status summary display.

use crate::cli_config::VERSION;
use crate::format::YELLOW;
use crate::format::{BOLD, CYAN, DIM, RESET};

/// Check whether `EXA_API_KEY` is set to a non-empty value.
pub(crate) fn exa_api_key_set() -> bool {
    std::env::var("EXA_API_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Print the startup banner with version, project context, and git status.
pub fn print_banner() {
    let day_str = option_env!("DAY_COUNT").unwrap_or("");
    let day_suffix = if day_str.is_empty() {
        String::new()
    } else {
        format!(" — Day {day_str}")
    };
    println!(
        "\n{BOLD}{CYAN}  yoyo{RESET} v{VERSION}{day_suffix} {DIM}— a coding agent growing up in public{RESET}"
    );

    // Show project context if we can detect it
    let dir = std::path::Path::new(".");
    let project_type = crate::commands_project::detect_project_type(dir);
    let name = crate::commands_project::detect_project_name(dir);
    let branch = crate::git::git_branch();
    if let Some(line) = banner_project_line(&project_type, &name, branch.as_deref()) {
        let status_suffix = git_status_summary()
            .map(|s| format!(" · {s}"))
            .unwrap_or_default();
        println!("{DIM}  {line}{status_suffix}{RESET}");
    }

    println!("{DIM}  Type /help for commands, /quit to exit{RESET}");

    // Show auto-loaded skill count if any were found
    let auto_count = crate::cli::auto_discovered_skill_count();
    if auto_count > 0 {
        println!("{DIM}  \u{1F9E0} {auto_count} skill(s) auto-loaded from .yoyo/skills/{RESET}");
    }

    // Warn if EXA_API_KEY is missing — web search won't work without it
    if !exa_api_key_set() {
        println!(
            "{DIM}  {YELLOW}⚠{RESET}{DIM} Web search requires EXA_API_KEY — get one at https://exa.ai{RESET}"
        );
    }

    println!();
}

/// Build the project context line for the startup banner.
/// Returns `None` if the project type is Unknown (graceful degradation).
pub fn banner_project_line(
    project_type: &crate::commands_project::ProjectType,
    name: &str,
    branch: Option<&str>,
) -> Option<String> {
    use crate::commands_project::ProjectType;

    if *project_type == ProjectType::Unknown {
        return None;
    }

    let type_label = match project_type {
        ProjectType::Rust => "Rust",
        ProjectType::Node => "Node.js",
        ProjectType::Python => "Python",
        ProjectType::Go => "Go",
        ProjectType::Java => "Java",
        ProjectType::Ruby => "Ruby",
        ProjectType::Cpp => "C/C++",
        ProjectType::Make => "Make",
        ProjectType::Unknown => unreachable!(),
    };

    let name_part = if name.is_empty() {
        String::new()
    } else {
        format!(" ({name})")
    };

    let branch_part = if let Some(b) = branch {
        format!(" on {b}")
    } else {
        String::new()
    };

    Some(format!(
        "\u{1F4C1} {type_label} project{name_part}{branch_part}"
    ))
}

/// Parse `git status --porcelain` output into (staged, modified, untracked) counts.
///
/// Each line of porcelain output has two status columns:
///   - Column 0 = index (staging area) status
///   - Column 1 = worktree status
///   - Lines starting with `??` are untracked files.
pub(crate) fn parse_git_status_counts(porcelain: &str) -> (u32, u32, u32) {
    let mut staged = 0u32;
    let mut modified = 0u32;
    let mut untracked = 0u32;

    for line in porcelain.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 2 {
            continue;
        }

        let index = bytes[0];
        let worktree = bytes[1];

        if index == b'?' && worktree == b'?' {
            untracked += 1;
        } else if index == b'!' && worktree == b'!' {
            // `!!` = ignored (only present with `git status --ignored`).
            // Ignored files are neither staged, modified, nor untracked.
            continue;
        } else {
            // Staged changes (index column has a letter, not space/?/!)
            if index != b' ' && index != b'?' && index != b'!' {
                staged += 1;
            }
            // Worktree changes (worktree column has a letter, not space/?/!)
            if worktree != b' ' && worktree != b'?' && worktree != b'!' {
                modified += 1;
            }
        }
    }

    (staged, modified, untracked)
}

/// Build a compact git status summary for the banner.
/// Returns `None` if not in a git repo.
fn git_status_summary() -> Option<String> {
    let output = crate::git::run_git(&["status", "--porcelain"]).ok()?;

    if output.trim().is_empty() {
        return Some("clean".to_string());
    }

    let (staged, modified, untracked) = parse_git_status_counts(&output);

    let mut parts = Vec::new();
    if staged > 0 {
        parts.push(format!("{staged} staged"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modified"));
    }
    if untracked > 0 {
        parts.push(format!("{untracked} untracked"));
    }

    if parts.is_empty() {
        Some("clean".to_string())
    } else {
        Some(parts.join(", "))
    }
}

/// Build the welcome message text for first-run users.
pub fn get_welcome_text() -> String {
    format!(
        r#"
  {BOLD}Welcome to yoyo! 🐙{RESET}

  {BOLD}Quick setup:{RESET}

  1. Get an API key from {CYAN}https://console.anthropic.com{RESET}
  2. Set it:
     {DIM}export ANTHROPIC_API_KEY=sk-ant-...{RESET}
  3. Run {BOLD}yoyo{RESET} again — you're in!

  {BOLD}Other providers:{RESET}
  Use {CYAN}--provider{RESET} to switch backends:
     openai, google, ollama (local), deepseek, groq, bedrock, and more.
  Example: {DIM}yoyo --provider ollama --model llama3.2{RESET}
  AWS Bedrock: {DIM}yoyo --provider bedrock --base-url https://bedrock-runtime.us-east-1.amazonaws.com{RESET}

  {BOLD}Persistent config:{RESET}
  Create a {CYAN}.yoyo.toml{RESET} file in your project or home directory:
     {DIM}api_key = "sk-ant-..."{RESET}
     {DIM}model = "claude-sonnet-4-20250514"{RESET}
     {DIM}provider = "anthropic"{RESET}
  Or use {CYAN}~/.config/yoyo/config.toml{RESET} for XDG-style config.

  Run {CYAN}yoyo --help{RESET} for all options.
"#
    )
}

/// Print a friendly welcome message for first-run users who haven't configured an API key.
/// This replaces the terse error when running interactively (REPL mode) without setup.
pub fn print_welcome() {
    print!("{}", get_welcome_text());
}

/// Format a duration as a human-friendly relative time string.
///
/// Returns "just now" (<1min), "Xm ago" (<1h), "Xh ago" (<24h), "Xd ago" (<7d).
pub(crate) fn format_session_age(elapsed_secs: u64) -> String {
    if elapsed_secs < 60 {
        "just now".to_string()
    } else if elapsed_secs < 3600 {
        format!("{}m ago", elapsed_secs / 60)
    } else if elapsed_secs < 86400 {
        format!("{}h ago", elapsed_secs / 3600)
    } else {
        format!("{}d ago", elapsed_secs / 86400)
    }
}

/// Maximum session age (7 days) before we consider it stale and stop hinting.
const MAX_SESSION_AGE_SECS: u64 = 7 * 24 * 3600;

/// Largest session file we will parse to confirm it holds a conversation.
/// Anything bigger is self-evidently non-empty, so we skip the parse rather
/// than pay it on every startup.
const MAX_SESSION_VERIFY_BYTES: u64 = 256 * 1024;

/// Whether a saved-session file's *contents* actually hold a conversation.
///
/// `Agent::save_messages` serializes the message list with `serde_json`, so a
/// real session is a JSON **array** with at least one element. An empty file, a
/// whitespace-only file, `[]`, `{}`, and a write truncated by a crash all mean
/// "there is nothing here to resume", and the banner must not promise a resume
/// for any of them.
///
/// This verifies the payload, not merely that the bytes parse: it deliberately
/// answers `false` for malformed JSON, because `--continue` cannot restore it.
pub(crate) fn session_file_has_messages(contents: &str) -> bool {
    if contents.trim().is_empty() {
        return false;
    }
    match serde_json::from_str::<serde_json::Value>(contents) {
        Ok(serde_json::Value::Array(messages)) => !messages.is_empty(),
        _ => false,
    }
}

/// Check for a resumable session at `path` and return a resume hint if one
/// exists, is less than 7 days old, and actually contains messages. Returns
/// `None` if there is no session file, if it's stale, or if it holds no
/// conversation.
pub fn session_resume_hint_at(path: &std::path::Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let elapsed = modified.elapsed().ok()?;
    let elapsed_secs = elapsed.as_secs();

    if elapsed_secs > MAX_SESSION_AGE_SECS {
        return None;
    }

    // A file's existence and mtime say nothing about whether a conversation is
    // inside it. Check the payload before promising a resume, otherwise an
    // interrupted or truncated write produces a confident hint and `--continue`
    // then restores nothing — the banner would be truthful about the object it
    // checked (a file, this old) and wrong about the object the user cares
    // about (a conversation worth resuming).
    if metadata.len() <= MAX_SESSION_VERIFY_BYTES {
        let contents = std::fs::read_to_string(path).ok()?;
        if !session_file_has_messages(&contents) {
            return None;
        }
    }

    let age = format_session_age(elapsed_secs);
    Some(format!(
        "{DIM}  \u{1F4AC} Previous session available ({age}) — use {YELLOW}--continue{RESET}{DIM} to resume{RESET}"
    ))
}

/// Check for a resumable session and return a resume hint if one exists.
///
/// Uses the same path `--continue` would load, via
/// [`crate::commands_session::continue_session_path`], so the banner's notion of
/// "a resumable session exists" cannot drift from what `--continue` actually
/// restores. Returns `None` if the file doesn't exist, is older than 7 days, or
/// holds no conversation.
pub fn session_resume_hint() -> Option<String> {
    session_resume_hint_at(std::path::Path::new(
        crate::commands_session::continue_session_path(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_project::ProjectType;
    use serial_test::serial;

    #[test]
    fn test_print_banner_does_not_panic() {
        // print_banner uses compile-time DAY_COUNT via option_env!().
        // When built from yoyo's repo, DAY_COUNT is baked in.
        // When built externally, option_env! returns None gracefully.
        // Either way, it must not panic.
        print_banner();
    }

    #[test]
    fn test_banner_project_line_rust() {
        let line = banner_project_line(&ProjectType::Rust, "my-app", Some("main"));
        assert_eq!(
            line,
            Some("\u{1F4C1} Rust project (my-app) on main".to_string())
        );
    }

    #[test]
    fn test_banner_project_line_node_no_branch() {
        let line = banner_project_line(&ProjectType::Node, "webapp", None);
        assert_eq!(line, Some("\u{1F4C1} Node.js project (webapp)".to_string()));
    }

    #[test]
    fn test_banner_project_line_unknown_returns_none() {
        let line = banner_project_line(&ProjectType::Unknown, "something", Some("main"));
        assert_eq!(line, None);
    }

    #[test]
    fn test_banner_project_line_empty_name() {
        let line = banner_project_line(&ProjectType::Python, "", Some("dev"));
        assert_eq!(line, Some("\u{1F4C1} Python project on dev".to_string()));
    }

    #[test]
    fn test_banner_project_line_go() {
        let line = banner_project_line(&ProjectType::Go, "myservice", Some("feature/x"));
        assert_eq!(
            line,
            Some("\u{1F4C1} Go project (myservice) on feature/x".to_string())
        );
    }

    #[test]
    fn test_banner_project_line_make_no_name_no_branch() {
        let line = banner_project_line(&ProjectType::Make, "", None);
        assert_eq!(line, Some("\u{1F4C1} Make project".to_string()));
    }

    #[test]
    fn test_parse_git_status_counts_empty() {
        assert_eq!(parse_git_status_counts(""), (0, 0, 0));
    }

    #[test]
    fn test_parse_git_status_counts_worktree_modified() {
        // " M src/main.rs" = modified in worktree, not staged
        assert_eq!(parse_git_status_counts(" M src/main.rs"), (0, 1, 0));
    }

    #[test]
    fn test_parse_git_status_counts_untracked() {
        assert_eq!(parse_git_status_counts("?? new.txt"), (0, 0, 1));
    }

    #[test]
    fn test_parse_git_status_counts_staged() {
        // "A  added.rs" = added to index
        assert_eq!(parse_git_status_counts("A  added.rs"), (1, 0, 0));
    }

    #[test]
    fn test_parse_git_status_counts_mixed() {
        let porcelain = " M src/main.rs\n?? new.txt\nA  added.rs\n";
        assert_eq!(parse_git_status_counts(porcelain), (1, 1, 1));
    }

    #[test]
    fn test_parse_git_status_counts_both_staged_and_modified() {
        // "MM both.rs" = staged AND modified in worktree
        assert_eq!(parse_git_status_counts("MM both.rs"), (1, 1, 0));
    }

    #[test]
    fn test_parse_git_status_counts_multiple_untracked() {
        let porcelain = "?? a.txt\n?? b.txt\n?? c.txt\n";
        assert_eq!(parse_git_status_counts(porcelain), (0, 0, 3));
    }

    #[test]
    fn test_parse_git_status_counts_deleted() {
        // " D deleted.rs" = deleted in worktree
        assert_eq!(parse_git_status_counts(" D deleted.rs"), (0, 1, 0));
        // "D  deleted.rs" = deleted in index (staged)
        assert_eq!(parse_git_status_counts("D  deleted.rs"), (1, 0, 0));
    }

    #[test]
    fn test_parse_git_status_counts_renamed() {
        // "R  old -> new" = renamed in index
        assert_eq!(parse_git_status_counts("R  old -> new"), (1, 0, 0));
    }

    #[test]
    fn test_parse_git_status_counts_short_lines_ignored() {
        // Lines shorter than 2 bytes are skipped
        assert_eq!(parse_git_status_counts("X"), (0, 0, 0));
        assert_eq!(parse_git_status_counts(""), (0, 0, 0));
    }

    #[test]
    fn test_parse_git_status_counts_ignored() {
        // `git status --porcelain --ignored` emits `!!` for ignored files;
        // they must count as neither staged nor modified (nor untracked).
        assert_eq!(parse_git_status_counts("!! target/"), (0, 0, 0));
        // Ignored entries mixed with real changes: only the real ones count.
        let porcelain = "!! target/\n M src/main.rs\n?? new.txt\n";
        assert_eq!(parse_git_status_counts(porcelain), (0, 1, 1));
    }

    #[test]
    fn test_print_welcome_contains_key_phrases() {
        let welcome = get_welcome_text();
        assert!(
            welcome.contains("API key") || welcome.contains("api_key"),
            "welcome should mention API key"
        );
        assert!(
            welcome.contains("ANTHROPIC_API_KEY"),
            "welcome should mention ANTHROPIC_API_KEY env var"
        );
        assert!(
            welcome.contains("ollama"),
            "welcome should mention ollama for local usage"
        );
        assert!(
            welcome.contains(".yoyo.toml"),
            "welcome should mention .yoyo.toml config file"
        );
        assert!(welcome.contains("--help"), "welcome should mention --help");
        assert!(
            welcome.contains("Welcome to yoyo"),
            "welcome should have greeting"
        );
    }

    #[test]
    fn test_print_welcome_mentions_setup_steps() {
        let welcome = get_welcome_text();
        assert!(welcome.contains("1."), "welcome should have step 1");
        assert!(welcome.contains("2."), "welcome should have step 2");
        assert!(welcome.contains("3."), "welcome should have step 3");
        assert!(
            welcome.contains("console.anthropic.com"),
            "welcome should link to Anthropic console"
        );
    }

    #[test]
    fn test_print_welcome_mentions_other_providers() {
        let welcome = get_welcome_text();
        assert!(
            welcome.contains("--provider"),
            "welcome should mention --provider flag"
        );
        assert!(
            welcome.contains("openai"),
            "welcome should mention openai provider"
        );
        assert!(
            welcome.contains("google"),
            "welcome should mention google provider"
        );
    }

    #[test]
    fn test_welcome_text_mentions_bedrock() {
        let welcome = get_welcome_text();
        assert!(
            welcome.contains("bedrock"),
            "welcome text should mention bedrock"
        );
    }

    #[test]
    fn test_format_session_age() {
        assert_eq!(format_session_age(0), "just now");
        assert_eq!(format_session_age(30), "just now");
        assert_eq!(format_session_age(59), "just now");
        assert_eq!(format_session_age(60), "1m ago");
        assert_eq!(format_session_age(300), "5m ago");
        assert_eq!(format_session_age(3599), "59m ago");
        assert_eq!(format_session_age(3600), "1h ago");
        assert_eq!(format_session_age(7200), "2h ago");
        assert_eq!(format_session_age(86399), "23h ago");
        assert_eq!(format_session_age(86400), "1d ago");
        assert_eq!(format_session_age(259200), "3d ago");
    }

    #[test]
    fn test_session_resume_hint_no_file() {
        let path = std::path::Path::new("/tmp/nonexistent-yoyo-session-test.json");
        assert!(session_resume_hint_at(path).is_none());
    }

    #[test]
    fn test_session_resume_hint_with_recent_file() {
        let dir = std::env::temp_dir().join("yoyo-test-session-hint");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("last-session.json");
        // A real saved session is `serde_json::to_string(&messages)` — a
        // non-empty JSON array. The previous fixture here was `{}`, which
        // `--continue` cannot restore at all.
        std::fs::write(&path, r#"[{"role":"user","content":"hi"}]"#).expect("write test session");

        let hint = session_resume_hint_at(&path);
        assert!(hint.is_some(), "should return hint for recent file");
        let hint_text = hint.unwrap();
        assert!(
            hint_text.contains("Previous session available"),
            "hint should mention previous session"
        );
        assert!(
            hint_text.contains("--continue"),
            "hint should mention --continue flag"
        );
        assert!(
            hint_text.contains("just now"),
            "freshly created file should show 'just now'"
        );

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_session_file_has_messages_payload_not_container() {
        // Real session: non-empty JSON array of messages.
        assert!(session_file_has_messages(
            r#"[{"role":"user","content":"hi"}]"#
        ));
        assert!(session_file_has_messages("[ {\"a\":1}, {\"b\":2} ]"));

        // Nothing to resume — the file exists, but holds no conversation.
        assert!(!session_file_has_messages(""), "empty file");
        assert!(!session_file_has_messages("   \n\t "), "whitespace only");
        assert!(!session_file_has_messages("[]"), "empty message array");
        assert!(!session_file_has_messages("{}"), "empty object");
        assert!(!session_file_has_messages("null"), "json null");
        // Truncated by a crash mid-write: unparseable, so unresumable.
        assert!(
            !session_file_has_messages(r#"[{"role":"user","conte"#),
            "truncated write"
        );
    }

    /// Regression: the hint must be built from the file's *contents*, not from
    /// its existence and mtime. A present-but-contentless session file used to
    /// produce a confident "Previous session available" line while `--continue`
    /// restored nothing.
    #[test]
    fn test_session_resume_hint_none_for_contentless_files() {
        let dir = std::env::temp_dir().join("yoyo-test-session-hint-empty");
        let _ = std::fs::create_dir_all(&dir);

        for (label, body) in [
            ("zero-byte", ""),
            ("empty-array", "[]"),
            ("empty-object", "{}"),
            ("truncated", r#"[{"role":"user","conte"#),
        ] {
            let path = dir.join(format!("{label}.json"));
            std::fs::write(&path, body).expect("write test session");
            assert!(
                session_resume_hint_at(&path).is_none(),
                "{label}: fresh file with no conversation must not promise a resume"
            );
            let _ = std::fs::remove_file(&path);
        }

        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    #[serial]
    fn test_exa_api_key_set_when_present() {
        std::env::set_var("EXA_API_KEY", "test-key-123");
        assert!(exa_api_key_set());
    }

    #[test]
    #[serial]
    fn test_exa_api_key_set_when_empty() {
        std::env::set_var("EXA_API_KEY", "");
        assert!(!exa_api_key_set());
    }

    #[test]
    #[serial]
    fn test_exa_api_key_set_when_missing() {
        std::env::remove_var("EXA_API_KEY");
        assert!(!exa_api_key_set());
    }
}
