//! RTK (Rust Token Killer) detection, proxy integration, output compression.
//!
//! RTK is an optional tool that compresses verbose CLI output before it reaches
//! the agent, saving context-window tokens. When installed and not disabled
//! (`--no-rtk`), supported commands are automatically prefixed with `rtk`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Whether RTK integration is disabled via --no-rtk flag.
static RTK_DISABLED: AtomicBool = AtomicBool::new(false);

/// Cached result of RTK availability detection.
static RTK_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Whether we've already printed the RTK detection message.
static RTK_ANNOUNCED: AtomicBool = AtomicBool::new(false);

/// Disable RTK integration (called when --no-rtk flag is present).
pub fn disable_rtk() {
    RTK_DISABLED.store(true, Ordering::Relaxed);
}

/// Check if RTK is disabled.
pub fn is_rtk_disabled() -> bool {
    RTK_DISABLED.load(Ordering::Relaxed)
}

/// Detect whether `rtk` is available in PATH. Result is cached.
pub fn detect_rtk() -> bool {
    *RTK_AVAILABLE.get_or_init(|| {
        std::process::Command::new("rtk")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// Commands that RTK supports and can compress output for.
const RTK_SUPPORTED_COMMANDS: &[&str] = &[
    "git",
    "ls",
    "find",
    "grep",
    "cat",
    "head",
    "tail",
    "cargo",
    "npm",
    "pip",
    "docker",
    "kubectl",
    "gh",
    "tree",
    "diff",
    "du",
    "wc",
    "ps",
    "rg",
    "fd",
    "ag",
    "ack",
    "svn",
    "hg",
    "yarn",
    "pnpm",
    "go",
    "rustc",
    "make",
    "cmake",
    "apt",
    "brew",
    "pacman",
    "systemctl",
    "journalctl",
    "df",
    "mount",
    "ip",
    "ss",
    "netstat",
    "curl",
    "wget",
];

/// Check if a command string is a simple command (no pipes, redirects, or control flow).
fn is_simple_command(command: &str) -> bool {
    // Check for shell metacharacters that indicate complex expressions
    // We only match top-level pipes/redirects (not inside quotes)
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_char = '\0';

    for ch in command.chars() {
        match ch {
            '\'' if !in_double_quote && prev_char != '\\' => in_single_quote = !in_single_quote,
            '"' if !in_single_quote && prev_char != '\\' => in_double_quote = !in_double_quote,
            '|' | ';' | '>' | '<' if !in_single_quote && !in_double_quote => return false,
            '&' if !in_single_quote && !in_double_quote => return false,
            _ => {}
        }
        prev_char = ch;
    }
    true
}

/// The one-time RTK detection notice, or `None` when it must not be printed.
///
/// Pure decision half of the announcement at the single `eprintln!` call site in
/// `maybe_prefix_rtk`: silent under `--quiet` and glyph-free under `--screen-reader`,
/// the same two conventions every other stderr notice in the codebase follows.
pub fn rtk_announcement(quiet: bool, plain: bool) -> Option<&'static str> {
    if quiet {
        return None;
    }
    Some(if plain {
        "RTK detected — using compressed output (disable with --no-rtk)"
    } else {
        "📦 RTK detected — using compressed output (disable with --no-rtk)"
    })
}

/// Prefix a command with `rtk` if appropriate.
/// Returns the command unchanged if:
/// - RTK is not installed
/// - RTK is disabled via --no-rtk
/// - The command already starts with `rtk`
/// - The command is not a simple command (has pipes, redirects, control flow)
/// - The command's base program is not in RTK's supported list
pub fn maybe_prefix_rtk(command: &str) -> String {
    if is_rtk_disabled() || !detect_rtk() {
        return command.to_string();
    }

    let trimmed = command.trim();

    // Don't double-prefix
    if trimmed.starts_with("rtk ") || trimmed == "rtk" {
        return command.to_string();
    }

    // Only prefix simple commands
    if !is_simple_command(trimmed) {
        return command.to_string();
    }

    // Extract the base command (first word, skipping env var assignments)
    let base_cmd = trimmed
        .split_whitespace()
        .find(|word| !word.contains('='))
        .unwrap_or("");

    // Check if this is a supported command
    if RTK_SUPPORTED_COMMANDS.contains(&base_cmd) {
        // Print announcement once
        if !RTK_ANNOUNCED.swap(true, Ordering::Relaxed) {
            if let Some(msg) =
                rtk_announcement(crate::format::is_quiet(), crate::format::is_plain_output())
            {
                eprintln!("{msg}");
            }
        }
        format!("rtk {trimmed}")
    } else {
        command.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtk_announcement_respects_quiet_and_plain_output() {
        // The exact string the caller prints, in all three states.
        assert_eq!(
            rtk_announcement(false, false),
            Some("📦 RTK detected — using compressed output (disable with --no-rtk)")
        );
        assert_eq!(
            rtk_announcement(false, true),
            Some("RTK detected — using compressed output (disable with --no-rtk)")
        );
        // Screen-reader mode keeps the words, drops the glyph.
        assert!(!rtk_announcement(false, true).unwrap().contains('📦'));
        // --quiet prints nothing at all.
        assert_eq!(rtk_announcement(true, false), None);
        assert_eq!(rtk_announcement(true, true), None);
    }

    #[test]
    fn test_detect_rtk_returns_bool() {
        // In CI, RTK is likely not installed, so this should return false
        // The important thing is it doesn't panic
        let _result: bool = detect_rtk();
    }

    #[test]
    fn test_maybe_prefix_rtk_when_disabled() {
        // Disable RTK for this test
        RTK_DISABLED.store(true, Ordering::Relaxed);
        assert_eq!(maybe_prefix_rtk("git status"), "git status");
        assert_eq!(maybe_prefix_rtk("cargo test"), "cargo test");
        // Re-enable for other tests
        RTK_DISABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn test_maybe_prefix_rtk_no_double_prefix() {
        // Even if RTK were available, shouldn't double-prefix
        // (This works regardless of RTK availability)
        let cmd = "rtk git status";
        let result = maybe_prefix_rtk(cmd);
        assert_eq!(result, "rtk git status");
    }

    #[test]
    fn test_maybe_prefix_rtk_complex_commands_not_prefixed() {
        // These should never be prefixed regardless of RTK availability
        RTK_DISABLED.store(false, Ordering::Relaxed);
        // Force RTK "available" for this test by checking the logic path
        // Since we can't fake RTK being available, test the is_simple_command helper
        assert!(!is_simple_command("git status | grep main"));
        assert!(!is_simple_command("echo hello && cargo test"));
        assert!(!is_simple_command("ls; rm -rf /"));
        assert!(!is_simple_command("cat file > output.txt"));
        assert!(!is_simple_command("sort < input.txt"));
        assert!(!is_simple_command("cmd1 & cmd2"));
    }

    #[test]
    fn test_is_simple_command_positive() {
        assert!(is_simple_command("git status"));
        assert!(is_simple_command("cargo test --release"));
        assert!(is_simple_command("ls -la"));
        assert!(is_simple_command("echo hello"));
        assert!(is_simple_command("grep -r pattern ."));
    }

    #[test]
    fn test_is_simple_command_quoted_metacharacters() {
        // Pipes/redirects inside quotes should NOT break simplicity
        assert!(is_simple_command("echo 'hello | world'"));
        assert!(is_simple_command("grep \"pattern > here\""));
        assert!(is_simple_command("echo 'a && b'"));
    }

    #[test]
    fn test_maybe_prefix_rtk_unsupported_commands() {
        // These commands are not in the RTK supported list
        // Even if RTK is installed, they shouldn't be prefixed
        // We test the logic by checking the supported list directly
        assert!(!RTK_SUPPORTED_COMMANDS.contains(&"echo"));
        assert!(!RTK_SUPPORTED_COMMANDS.contains(&"cd"));
        assert!(!RTK_SUPPORTED_COMMANDS.contains(&"python"));
        assert!(!RTK_SUPPORTED_COMMANDS.contains(&"python3"));
        assert!(!RTK_SUPPORTED_COMMANDS.contains(&"ruby"));
        assert!(!RTK_SUPPORTED_COMMANDS.contains(&"node"));
    }

    #[test]
    fn test_rtk_supported_commands_includes_expected() {
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"git"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"ls"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"cargo"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"npm"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"docker"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"kubectl"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"grep"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"find"));
        assert!(RTK_SUPPORTED_COMMANDS.contains(&"gh"));
    }

    #[test]
    fn test_maybe_prefix_rtk_with_env_var_prefix() {
        // Commands with env var assignments before the actual command
        // The function should skip env vars and find the actual command
        // Testing indirectly: "FOO=bar git status" - base_cmd should be "git"
        // Since RTK may not be installed in CI, we just verify the logic doesn't panic
        let _ = maybe_prefix_rtk("FOO=bar git status");
        let _ = maybe_prefix_rtk("HOME=/tmp ls");
    }
}
