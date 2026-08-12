//! `/revisit` command handler — review closed/shelved issues that may now be feasible.
//!
//! Subcommands:
//! - `scan` (default): fetch recently closed issues and show candidates for revisiting
//! - `check #N`: inspect a specific closed issue and summarize what changed since closure
//! - `list`: show issues previously marked as revisit candidates (from `.yoyo/revisit.json`)
//! - `add #N <reason>`: mark a closed issue as a revisit candidate
//! - `remove #N`: remove an issue from the revisit list

use crate::format::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

/// File where revisit candidates are persisted.
const REVISIT_FILE: &str = ".yoyo/revisit.json";

/// Labels that indicate an issue was shelved rather than truly resolved.
const SHELVED_LABELS: &[&str] = &[
    "wontfix",
    "deferred",
    "too-complex",
    "won't fix",
    "later",
    "shelved",
    "backlog",
];

/// Subcommands for `/revisit`.
pub const REVISIT_SUBCOMMANDS: &[&str] = &["scan", "check", "list", "add", "remove"];

/// A revisit candidate persisted to `.yoyo/revisit.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevisitCandidate {
    pub number: u64,
    pub title: String,
    pub reason: String,
    pub added_day: u64,
}

/// A closed issue fetched from GitHub.
#[derive(Debug, Clone)]
pub struct ClosedIssue {
    pub number: u64,
    pub title: String,
    pub labels: Vec<String>,
    pub closed_at: String,
}

/// Load revisit candidates from disk.
pub fn load_revisit_list() -> Vec<RevisitCandidate> {
    let path = Path::new(REVISIT_FILE);
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Save revisit candidates to disk, creating the directory if needed.
fn save_revisit_list(candidates: &[RevisitCandidate]) -> Result<(), String> {
    let path = Path::new(REVISIT_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .yoyo/ directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(candidates)
        .map_err(|e| format!("Failed to serialize revisit list: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Failed to write revisit file: {e}"))?;
    Ok(())
}

/// Get the current day count from the DAY_COUNT file.
fn current_day() -> u64 {
    fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Parse the subcommand from `/revisit <args>`.
pub fn parse_revisit_args(input: &str) -> (&str, &str) {
    let arg = input.strip_prefix("/revisit").unwrap_or("").trim();
    if arg.is_empty() {
        return ("scan", "");
    }
    match arg.split_once(char::is_whitespace) {
        Some((cmd, rest)) => (cmd, rest.trim()),
        None => (arg, ""),
    }
}

/// Parse an issue number from a string like "#123" or "123".
fn parse_issue_number(s: &str) -> Option<u64> {
    s.trim().trim_start_matches('#').parse().ok()
}

/// Fetch closed issues via `gh` CLI. Returns parsed issues or an error message.
fn fetch_closed_issues(limit: u32) -> Result<Vec<ClosedIssue>, String> {
    let output = Command::new("gh")
        .args([
            "issue",
            "list",
            "--state",
            "closed",
            "--limit",
            &limit.to_string(),
            "--json",
            "number,title,labels,closedAt,body",
        ])
        .output()
        .map_err(|e| format!("Failed to run `gh`: {e}. Is the GitHub CLI installed?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh issue list failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_closed_issues_json(&stdout)
}

/// Parse the JSON output from `gh issue list`.
fn parse_closed_issues_json(json_str: &str) -> Result<Vec<ClosedIssue>, String> {
    let items: Vec<serde_json::Value> =
        serde_json::from_str(json_str).map_err(|e| format!("Failed to parse gh output: {e}"))?;

    let mut issues = Vec::new();
    for item in items {
        let number = item["number"].as_u64().unwrap_or(0);
        let title = item["title"].as_str().unwrap_or("").to_string();
        let closed_at = item["closedAt"].as_str().unwrap_or("").to_string();
        let _body = item["body"].as_str().unwrap_or("");

        let labels: Vec<String> = item["labels"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        issues.push(ClosedIssue {
            number,
            title,
            labels,
            closed_at,
        });
    }
    Ok(issues)
}

/// Check if an issue looks like it was shelved (based on labels).
fn is_shelved(issue: &ClosedIssue) -> bool {
    issue.labels.iter().any(|label| {
        let lower = label.to_lowercase();
        SHELVED_LABELS.iter().any(|s| lower.contains(s))
    })
}

/// Format a date string for display (extract just the date part from ISO 8601).
fn format_date(iso: &str) -> &str {
    // "2025-05-13T09:33:00Z" → "2025-05-13"
    // `iso` is server-controlled JSON text, so byte 10 may land inside a
    // multi-byte char — slicing there panics. Fall back to the whole string.
    if iso.len() >= 10 && iso.is_char_boundary(10) {
        &iso[..10]
    } else {
        iso
    }
}

/// Format the scan output — a table of shelved/closed issues.
pub fn format_scan_results(issues: &[ClosedIssue]) -> String {
    if issues.is_empty() {
        return format!("{DIM}  No recently closed issues found.{RESET}");
    }

    let shelved: Vec<&ClosedIssue> = issues.iter().filter(|i| is_shelved(i)).collect();
    let other: Vec<&ClosedIssue> = issues.iter().filter(|i| !is_shelved(i)).collect();

    let mut out = String::new();

    if !shelved.is_empty() {
        out.push_str(&format!(
            "{BOLD}  Shelved issues ({} found):{RESET}\n\n",
            shelved.len()
        ));
        for issue in &shelved {
            let labels_str = if issue.labels.is_empty() {
                String::new()
            } else {
                format!(" {DIM}[{}]{RESET}", issue.labels.join(", "))
            };
            out.push_str(&format!(
                "  {GREEN}#{:<5}{RESET} {}{}\n",
                issue.number, issue.title, labels_str
            ));
            out.push_str(&format!(
                "         {DIM}closed {}{RESET}\n",
                format_date(&issue.closed_at)
            ));
        }
    }

    if !other.is_empty() {
        if !shelved.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!(
            "{DIM}  Other recently closed ({}):{RESET}\n\n",
            other.len()
        ));
        for issue in other.iter().take(10) {
            out.push_str(&format!(
                "  {DIM}#{:<5}{RESET} {}\n",
                issue.number, issue.title
            ));
        }
        if other.len() > 10 {
            out.push_str(&format!(
                "  {DIM}  ... and {} more{RESET}\n",
                other.len() - 10
            ));
        }
    }

    out.push_str(&format!(
        "\n{DIM}  Tip: /revisit check #N to inspect a specific issue{RESET}"
    ));
    out
}

/// Format the revisit candidate list.
pub fn format_revisit_list(candidates: &[RevisitCandidate]) -> String {
    if candidates.is_empty() {
        return format!(
            "{DIM}  No revisit candidates. Use /revisit add #N <reason> to add one.{RESET}"
        );
    }

    let mut out = format!(
        "{BOLD}  Revisit candidates ({}):{RESET}\n\n",
        candidates.len()
    );
    for c in candidates {
        out.push_str(&format!("  {GREEN}#{:<5}{RESET} {}\n", c.number, c.title));
        out.push_str(&format!(
            "         {DIM}reason: {} (added day {}){RESET}\n",
            c.reason, c.added_day
        ));
    }
    out.push_str(&format!(
        "\n{DIM}  /revisit check #N for details, /revisit remove #N to drop{RESET}"
    ));
    out
}

/// Fetch details for a specific closed issue.
fn fetch_issue_detail(number: u64) -> Result<String, String> {
    let output = Command::new("gh")
        .args([
            "issue",
            "view",
            &number.to_string(),
            "--json",
            "number,title,state,labels,closedAt,body,comments",
        ])
        .output()
        .map_err(|e| format!("Failed to run `gh`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh issue view failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    format_issue_check(&stdout)
}

/// Format the check output for a specific issue.
fn format_issue_check(json_str: &str) -> Result<String, String> {
    let item: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Failed to parse: {e}"))?;

    let number = item["number"].as_u64().unwrap_or(0);
    let title = item["title"].as_str().unwrap_or("");
    let state = item["state"].as_str().unwrap_or("unknown");
    let closed_at = item["closedAt"].as_str().unwrap_or("");
    let body = item["body"].as_str().unwrap_or("(no description)");

    let labels: Vec<&str> = item["labels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l["name"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Extract the last comment as close reason hint
    let last_comment = item["comments"]
        .as_array()
        .and_then(|arr| arr.last())
        .and_then(|c| c["body"].as_str())
        .unwrap_or("(no closing comment)");

    let day = current_day();

    let mut out = String::new();
    out.push_str(&format!("{BOLD}  Issue #{number}: {title}{RESET}\n\n"));
    out.push_str(&format!("  {DIM}State:{RESET} {state}\n"));
    if !closed_at.is_empty() {
        out.push_str(&format!(
            "  {DIM}Closed:{RESET} {}\n",
            format_date(closed_at)
        ));
    }
    if !labels.is_empty() {
        out.push_str(&format!("  {DIM}Labels:{RESET} {}\n", labels.join(", ")));
    }

    // Body preview (first 300 chars)
    let body_preview = safe_truncate_with_suffix(body, 300, "…");
    out.push_str(&format!("\n  {DIM}Description:{RESET}\n"));
    for line in body_preview.lines() {
        out.push_str(&format!("    {line}\n"));
    }

    // Last comment as close-reason hint
    let comment_preview = safe_truncate_with_suffix(last_comment, 200, "…");
    out.push_str(&format!(
        "\n  {DIM}Last comment (close reason hint):{RESET}\n"
    ));
    for line in comment_preview.lines() {
        out.push_str(&format!("    {line}\n"));
    }

    out.push_str(&format!(
        "\n  {DIM}Current day: {day} — review whether new capabilities make this feasible.{RESET}"
    ));
    out.push_str(&format!(
        "\n  {DIM}Tip: /revisit add #{number} <reason> to track for future review{RESET}"
    ));

    Ok(out)
}

/// Handle the `/revisit` command.
pub fn handle_revisit(input: &str) -> String {
    let (subcmd, args) = parse_revisit_args(input);

    match subcmd {
        "scan" => match fetch_closed_issues(30) {
            Ok(issues) => format_scan_results(&issues),
            Err(e) => format!("{RED}  Error: {e}{RESET}"),
        },
        "check" => {
            let Some(number) = parse_issue_number(args) else {
                return format!(
                    "{YELLOW}  Usage: /revisit check #N{RESET}\n\n  \
                     Example: /revisit check #42"
                );
            };
            match fetch_issue_detail(number) {
                Ok(detail) => detail,
                Err(e) => format!("{RED}  Error: {e}{RESET}"),
            }
        }
        "list" => {
            let candidates = load_revisit_list();
            format_revisit_list(&candidates)
        }
        "add" => {
            // Parse: add #N reason text
            let (num_part, reason) = match args.split_once(char::is_whitespace) {
                Some((n, r)) => (n, r.trim()),
                None => {
                    return format!(
                        "{YELLOW}  Usage: /revisit add #N <reason>{RESET}\n\n  \
                         Example: /revisit add #42 Too complex at the time, now have better infra"
                    );
                }
            };
            let Some(number) = parse_issue_number(num_part) else {
                return format!("{YELLOW}  Could not parse issue number from '{num_part}'{RESET}");
            };
            if reason.is_empty() {
                return format!(
                    "{YELLOW}  Please provide a reason: /revisit add #{number} <reason>{RESET}"
                );
            }

            let mut candidates = load_revisit_list();

            // Check for duplicates
            if candidates.iter().any(|c| c.number == number) {
                return format!("{YELLOW}  Issue #{number} is already on the revisit list.{RESET}");
            }

            candidates.push(RevisitCandidate {
                number,
                title: format!("(issue #{number})"),
                reason: reason.to_string(),
                added_day: current_day(),
            });

            match save_revisit_list(&candidates) {
                Ok(()) => format!(
                    "{GREEN}  ✓ Added #{number} to revisit list.{RESET}\n  \
                     {DIM}Reason: {reason}{RESET}"
                ),
                Err(e) => format!("{RED}  Error saving: {e}{RESET}"),
            }
        }
        "remove" => {
            let Some(number) = parse_issue_number(args) else {
                return format!(
                    "{YELLOW}  Usage: /revisit remove #N{RESET}\n\n  \
                     Example: /revisit remove #42"
                );
            };

            let mut candidates = load_revisit_list();
            let before = candidates.len();
            candidates.retain(|c| c.number != number);

            if candidates.len() == before {
                return format!("{YELLOW}  Issue #{number} was not on the revisit list.{RESET}");
            }

            match save_revisit_list(&candidates) {
                Ok(()) => format!("{GREEN}  ✓ Removed #{number} from revisit list.{RESET}"),
                Err(e) => format!("{RED}  Error saving: {e}{RESET}"),
            }
        }
        _ => {
            format!(
                "{YELLOW}  Unknown subcommand: {subcmd}{RESET}\n\n  \
                 Usage: /revisit [scan | check #N | list | add #N <reason> | remove #N]"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_revisit_args_default() {
        let (cmd, args) = parse_revisit_args("/revisit");
        assert_eq!(cmd, "scan");
        assert_eq!(args, "");
    }

    #[test]
    fn test_parse_revisit_args_scan() {
        let (cmd, args) = parse_revisit_args("/revisit scan");
        assert_eq!(cmd, "scan");
        assert_eq!(args, "");
    }

    #[test]
    fn test_parse_revisit_args_check() {
        let (cmd, args) = parse_revisit_args("/revisit check #42");
        assert_eq!(cmd, "check");
        assert_eq!(args, "#42");
    }

    #[test]
    fn test_parse_revisit_args_add() {
        let (cmd, args) = parse_revisit_args("/revisit add #10 too complex before");
        assert_eq!(cmd, "add");
        assert_eq!(args, "#10 too complex before");
    }

    #[test]
    fn test_parse_revisit_args_remove() {
        let (cmd, args) = parse_revisit_args("/revisit remove #5");
        assert_eq!(cmd, "remove");
        assert_eq!(args, "#5");
    }

    #[test]
    fn test_parse_issue_number_with_hash() {
        assert_eq!(parse_issue_number("#42"), Some(42));
    }

    #[test]
    fn test_parse_issue_number_without_hash() {
        assert_eq!(parse_issue_number("42"), Some(42));
    }

    #[test]
    fn test_parse_issue_number_invalid() {
        assert_eq!(parse_issue_number("abc"), None);
        assert_eq!(parse_issue_number(""), None);
    }

    #[test]
    fn test_revisit_candidate_serde() {
        let candidate = RevisitCandidate {
            number: 42,
            title: "Test issue".to_string(),
            reason: "Too complex at the time".to_string(),
            added_day: 70,
        };
        let json = serde_json::to_string(&candidate).unwrap();
        let deserialized: RevisitCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(candidate, deserialized);
    }

    #[test]
    fn test_revisit_list_serde_roundtrip() {
        let candidates = vec![
            RevisitCandidate {
                number: 1,
                title: "First".to_string(),
                reason: "shelved".to_string(),
                added_day: 10,
            },
            RevisitCandidate {
                number: 2,
                title: "Second".to_string(),
                reason: "deferred".to_string(),
                added_day: 20,
            },
        ];
        let json = serde_json::to_string_pretty(&candidates).unwrap();
        let deserialized: Vec<RevisitCandidate> = serde_json::from_str(&json).unwrap();
        assert_eq!(candidates, deserialized);
    }

    #[test]
    fn test_format_revisit_list_empty() {
        let result = format_revisit_list(&[]);
        assert!(result.contains("No revisit candidates"));
    }

    #[test]
    fn test_format_revisit_list_with_items() {
        let candidates = vec![RevisitCandidate {
            number: 42,
            title: "Feature X".to_string(),
            reason: "needed better infra".to_string(),
            added_day: 50,
        }];
        let result = format_revisit_list(&candidates);
        assert!(result.contains("#42"));
        assert!(result.contains("Feature X"));
        assert!(result.contains("needed better infra"));
        assert!(result.contains("day 50"));
    }

    #[test]
    fn test_parse_closed_issues_json() {
        let json = r#"[
            {
                "number": 10,
                "title": "Add feature X",
                "labels": [{"name": "wontfix"}],
                "closedAt": "2025-05-01T12:00:00Z",
                "body": "We should add feature X"
            },
            {
                "number": 20,
                "title": "Bug fix Y",
                "labels": [],
                "closedAt": "2025-05-02T12:00:00Z",
                "body": "Fixed a bug"
            }
        ]"#;

        let issues = parse_closed_issues_json(json).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 10);
        assert_eq!(issues[0].title, "Add feature X");
        assert!(issues[0].labels.contains(&"wontfix".to_string()));
        assert_eq!(issues[1].number, 20);
    }

    #[test]
    fn test_is_shelved() {
        let shelved = ClosedIssue {
            number: 1,
            title: "Test".to_string(),
            labels: vec!["wontfix".to_string()],
            closed_at: String::new(),
        };
        assert!(is_shelved(&shelved));

        let not_shelved = ClosedIssue {
            number: 2,
            title: "Test".to_string(),
            labels: vec!["bug".to_string()],
            closed_at: String::new(),
        };
        assert!(!is_shelved(&not_shelved));
    }

    #[test]
    fn test_is_shelved_case_insensitive() {
        let issue = ClosedIssue {
            number: 1,
            title: "Test".to_string(),
            labels: vec!["WontFix".to_string()],
            closed_at: String::new(),
        };
        assert!(is_shelved(&issue));
    }

    #[test]
    fn test_format_date() {
        assert_eq!(format_date("2025-05-13T09:33:00Z"), "2025-05-13");
        assert_eq!(format_date("short"), "short");
        assert_eq!(format_date(""), "");
    }

    #[test]
    fn test_format_date_multibyte_at_the_cut_does_not_panic() {
        // `closedAt` is server-controlled JSON text, so `format_date` must not
        // assume byte 10 is a char boundary. "2026-08-1✓…" puts a 3-byte char
        // across bytes 9..12, which the old `&iso[..10]` sliced into (panic).
        let iso = "2026-08-1✓T09:33:00Z";
        assert!(iso.len() >= 10 && !iso.is_char_boundary(10));
        assert_eq!(format_date(iso), iso);
        // A date-shaped prefix ending exactly on a boundary still truncates.
        assert_eq!(format_date("2026-08-13T09:33:00Z"), "2026-08-13");
        // Multi-byte before the cut, boundary at 10: still truncates at byte 10.
        assert_eq!(format_date("✓026-08-1x"), "✓026-08-");
    }

    #[test]
    fn test_format_scan_results_empty() {
        let result = format_scan_results(&[]);
        assert!(result.contains("No recently closed issues"));
    }

    #[test]
    fn test_format_scan_results_with_shelved() {
        let issues = vec![
            ClosedIssue {
                number: 10,
                title: "Shelved feature".to_string(),
                labels: vec!["deferred".to_string()],
                closed_at: "2025-05-01T12:00:00Z".to_string(),
            },
            ClosedIssue {
                number: 20,
                title: "Normal close".to_string(),
                labels: vec!["bug".to_string()],
                closed_at: "2025-05-02T12:00:00Z".to_string(),
            },
        ];
        let result = format_scan_results(&issues);
        assert!(result.contains("Shelved issues"));
        assert!(result.contains("#10"));
        assert!(result.contains("Shelved feature"));
        assert!(result.contains("Other recently closed"));
        assert!(result.contains("#20"));
    }

    #[test]
    fn test_format_issue_check() {
        let json = r#"{
            "number": 42,
            "title": "Complex feature",
            "state": "CLOSED",
            "closedAt": "2025-04-15T10:00:00Z",
            "labels": [{"name": "deferred"}],
            "body": "We need a complex feature that does X, Y, and Z.",
            "comments": [
                {"body": "Closing — too complex for now, revisit later."}
            ]
        }"#;
        let result = format_issue_check(json).unwrap();
        assert!(result.contains("#42"));
        assert!(result.contains("Complex feature"));
        assert!(result.contains("CLOSED"));
        assert!(result.contains("2025-04-15"));
        assert!(result.contains("deferred"));
        assert!(result.contains("too complex for now"));
    }

    #[test]
    fn test_handle_revisit_unknown_subcommand() {
        let result = handle_revisit("/revisit bogus");
        assert!(result.contains("Unknown subcommand"));
        assert!(result.contains("bogus"));
    }

    #[test]
    fn test_handle_revisit_list_empty() {
        // This test reads from disk — should return empty if no file
        // (works in test environment where .yoyo/revisit.json doesn't exist)
        let result = handle_revisit("/revisit list");
        // It should either show candidates or "No revisit candidates"
        assert!(result.contains("revisit candidates") || result.contains("Revisit candidates"));
    }

    #[test]
    fn test_handle_revisit_check_no_number() {
        let result = handle_revisit("/revisit check");
        assert!(result.contains("Usage"));
    }

    #[test]
    fn test_handle_revisit_add_no_args() {
        let result = handle_revisit("/revisit add");
        assert!(result.contains("Usage"));
    }

    #[test]
    fn test_handle_revisit_remove_no_args() {
        let result = handle_revisit("/revisit remove");
        assert!(result.contains("Usage"));
    }

    #[test]
    fn test_revisit_add_remove_roundtrip() {
        // Use a temp directory to avoid polluting the project
        let dir = tempfile::tempdir().unwrap();
        let revisit_path = dir.path().join(".yoyo").join("revisit.json");
        std::fs::create_dir_all(revisit_path.parent().unwrap()).unwrap();

        let candidates = vec![RevisitCandidate {
            number: 99,
            title: "(issue #99)".to_string(),
            reason: "test reason".to_string(),
            added_day: 74,
        }];

        let json = serde_json::to_string_pretty(&candidates).unwrap();
        std::fs::write(&revisit_path, &json).unwrap();

        let loaded: Vec<RevisitCandidate> =
            serde_json::from_str(&std::fs::read_to_string(&revisit_path).unwrap()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].number, 99);

        // Remove
        let filtered: Vec<RevisitCandidate> =
            loaded.into_iter().filter(|c| c.number != 99).collect();
        assert!(filtered.is_empty());
    }
}
