//! Startup banner, welcome text, and git status summary display.

use crate::cli_config::VERSION;
use crate::format::{BOLD, CYAN, DIM, RESET};

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

    println!("{DIM}  Type /help for commands, /quit to exit{RESET}\n");
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
        } else {
            // Staged changes (index column has a letter, not space or ?)
            if index != b' ' && index != b'?' {
                staged += 1;
            }
            // Worktree changes (worktree column has a letter, not space or ?)
            if worktree != b' ' && worktree != b'?' {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_project::ProjectType;

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
}
