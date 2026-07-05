//! `/pr` subcommand handling: listing, viewing, diffing, reviewing,
//! commenting on, checking out, and creating pull requests via the `gh` CLI.
//!
//! Extracted from `commands_git.rs` (Day 127); re-exported there so call
//! sites are unchanged.

use crate::commands_session::auto_compact_if_needed;
use crate::format::*;
use crate::git::*;
use crate::prompt::run_prompt;
use yoagent::agent::Agent;
use yoagent::*;

/// Run a `gh` CLI command and return its stdout on success, or print a
/// user-friendly error on failure. Returns `None` if the command failed
/// or `gh` is not installed.
fn run_gh(args: &[&str]) -> Option<String> {
    match std::process::Command::new("gh").args(args).output() {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
            None
        }
        Err(_) => {
            eprintln!(
                "{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n"
            );
            None
        }
    }
}

/// Represents a parsed `/pr` subcommand.
#[derive(Debug, PartialEq)]
pub enum PrSubcommand {
    List,
    View(u32),
    Diff(u32),
    Review(u32, bool), // (PR number, post_to_github)
    Comment(u32, String),
    Checkout(u32),
    Create { draft: bool },
    Help,
}

/// Parse the argument string after `/pr` into a `PrSubcommand`.
pub fn parse_pr_args(arg: &str) -> PrSubcommand {
    let arg = arg.trim();
    if arg.is_empty() {
        return PrSubcommand::List;
    }

    let parts: Vec<&str> = arg.splitn(3, char::is_whitespace).collect();

    // Check for "review" subcommand first (before trying to parse as number)
    if parts[0].eq_ignore_ascii_case("review") {
        if let Some(num_str) = parts.get(1) {
            if let Ok(n) = num_str.parse::<u32>() {
                let post = parts
                    .get(2)
                    .map(|s| s.eq_ignore_ascii_case("--post"))
                    .unwrap_or(false);
                return PrSubcommand::Review(n, post);
            }
        }
        return PrSubcommand::Help;
    }

    // Check for "create" subcommand first (before trying to parse as number)
    if parts[0].eq_ignore_ascii_case("create") {
        let draft = parts
            .get(1)
            .map(|s| s.trim_start_matches('-').eq_ignore_ascii_case("draft"))
            .unwrap_or(false);
        return PrSubcommand::Create { draft };
    }

    let number = match parts[0].parse::<u32>() {
        Ok(n) => n,
        Err(_) => return PrSubcommand::Help,
    };

    if parts.len() == 1 {
        return PrSubcommand::View(number);
    }

    match parts[1].to_lowercase().as_str() {
        "diff" => PrSubcommand::Diff(number),
        "review" => {
            let post = if parts.len() == 3 {
                parts[2].eq_ignore_ascii_case("--post")
            } else {
                false
            };
            PrSubcommand::Review(number, post)
        }
        "checkout" => PrSubcommand::Checkout(number),
        "comment" => {
            let text = if parts.len() == 3 {
                parts[2].trim().to_string()
            } else {
                String::new()
            };
            if text.is_empty() {
                PrSubcommand::Help
            } else {
                PrSubcommand::Comment(number, text)
            }
        }
        _ => PrSubcommand::Help,
    }
}

pub async fn handle_pr(input: &str, agent: &mut Agent, session_total: &mut Usage, model: &str) {
    let arg = input.strip_prefix("/pr").unwrap_or("").trim();
    match parse_pr_args(arg) {
        PrSubcommand::List => {
            if let Some(text) = run_gh(&["pr", "list", "--limit", "10"]) {
                if text.trim().is_empty() {
                    println!("{DIM}  (no open pull requests){RESET}\n");
                } else {
                    println!("{DIM}  Open pull requests:");
                    for line in text.lines() {
                        println!("    {line}");
                    }
                    println!("{RESET}");
                }
            }
        }
        PrSubcommand::View(number) => {
            let num_str = number.to_string();
            if let Some(text) = run_gh(&["pr", "view", &num_str]) {
                println!("{DIM}{text}{RESET}");
            }
        }
        PrSubcommand::Diff(number) => {
            let num_str = number.to_string();
            if let Some(text) = run_gh(&["pr", "diff", &num_str]) {
                if text.trim().is_empty() {
                    println!("{DIM}  (no diff for PR #{number}){RESET}\n");
                } else {
                    println!("{DIM}{text}{RESET}");
                }
            }
        }
        PrSubcommand::Comment(number, text) => {
            let num_str = number.to_string();
            if run_gh(&["pr", "comment", &num_str, "--body", &text]).is_some() {
                println!("{GREEN}  ✓ comment added to PR #{number}{RESET}\n");
            }
        }
        PrSubcommand::Checkout(number) => {
            let num_str = number.to_string();
            if run_gh(&["pr", "checkout", &num_str]).is_some() {
                println!("{GREEN}  ✓ checked out PR #{number}{RESET}\n");
            }
        }
        PrSubcommand::Review(number, post) => {
            let num_str = number.to_string();

            // Fetch PR diff
            let diff = match run_gh(&["pr", "diff", &num_str]) {
                Some(d) => d,
                None => return,
            };

            if diff.trim().is_empty() {
                eprintln!("{DIM}  PR #{number} has no diff{RESET}\n");
                return;
            }

            // Optionally fetch PR title/body for context
            let pr_info = run_gh(&[
                "pr",
                "view",
                &num_str,
                "--json",
                "title,body",
                "--jq",
                r#".title + "\n\n" + .body"#,
            ])
            .unwrap_or_default();

            // Truncate diff if very large (50KB limit)
            const PR_REVIEW_MAX_BYTES: usize = 50_000;
            let diff_content = safe_truncate(&diff, PR_REVIEW_MAX_BYTES);
            let truncated_note = if diff.len() > PR_REVIEW_MAX_BYTES {
                "\n\n... (diff truncated for context limit)"
            } else {
                ""
            };

            let diff_with_note = format!("{diff_content}{truncated_note}");

            if post {
                // --post mode: generate structured review and post to GitHub
                use crate::commands_git_review::{
                    build_review_prompt_structured, extract_review_json, parse_review_comments,
                    post_pr_review,
                };

                let prompt = build_review_prompt_structured(number, &pr_info, &diff_with_note);

                eprintln!("{DIM}  [review] analyzing PR #{number} for inline comments...{RESET}");
                auto_compact_if_needed(agent);
                let outcome = run_prompt(agent, &prompt, session_total, model).await;

                // Extract JSON from the response
                match extract_review_json(&outcome.text) {
                    Some(json) => match parse_review_comments(&json) {
                        Ok(comments) => {
                            eprintln!(
                                "{DIM}  [review] posting {} comment{} to PR #{number}...{RESET}",
                                comments.len(),
                                if comments.len() == 1 { "" } else { "s" }
                            );
                            match post_pr_review(number, &comments) {
                                Ok(msg) => {
                                    println!("{GREEN}  {msg}{RESET}\n");
                                }
                                Err(e) => {
                                    eprintln!("{RED}  error posting review: {e}{RESET}\n");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "{RED}  error parsing review comments: {e}{RESET}\n\
                                 {DIM}  (the review was still displayed above){RESET}\n"
                            );
                        }
                    },
                    None => {
                        eprintln!(
                            "{YELLOW}  warning: could not extract JSON review from response{RESET}\n\
                             {DIM}  (the review was still displayed above — try without --post){RESET}\n"
                        );
                    }
                }
            } else {
                // Normal mode: just display the review
                let pr_section = if pr_info.trim().is_empty() {
                    String::new()
                } else {
                    format!("## PR Description\n\n{}\n\n", pr_info.trim())
                };

                let prompt = format!(
                    "Review this pull request (PR #{number}). Analyze the diff for:\n\
                     - Potential bugs or logic errors\n\
                     - Code quality issues\n\
                     - Missing error handling\n\
                     - Performance concerns\n\
                     - Suggestions for improvement\n\n\
                     Be specific — reference file names and line numbers from the diff.\n\
                     Praise good patterns too. Be constructive.\n\n\
                     {pr_section}\
                     ## Diff\n\n```diff\n{diff_with_note}\n```"
                );

                eprintln!("{DIM}  [review] analyzing PR #{number}...{RESET}");
                auto_compact_if_needed(agent);
                run_prompt(agent, &prompt, session_total, model).await;
            }
        }
        PrSubcommand::Create { draft } => {
            // 1. Detect current branch
            let branch = match git_branch() {
                Some(b) => b,
                None => {
                    eprintln!("{RED}  error: not in a git repository{RESET}\n");
                    return;
                }
            };
            let base = detect_base_branch();

            if branch == base {
                eprintln!(
                    "{RED}  error: already on {base} — switch to a feature branch first{RESET}\n"
                );
                return;
            }

            // 2. Get diff and commits
            let diff = get_branch_diff(&base).unwrap_or_default();
            let commits = get_branch_commits(&base).unwrap_or_default();

            if diff.trim().is_empty() && commits.trim().is_empty() {
                println!(
                    "{DIM}  (no changes between {branch} and {base} — nothing to create a PR for){RESET}\n"
                );
                return;
            }

            // 3. Show what we found
            let commit_count = commits.lines().filter(|l| !l.is_empty()).count();
            println!(
                "{DIM}  Branch: {branch} → {base} ({commit_count} commit{s}){RESET}",
                s = if commit_count == 1 { "" } else { "s" }
            );
            println!("{DIM}  Generating PR description with AI...{RESET}");

            // 4. Ask AI to generate title + description
            let prompt = build_pr_description_prompt(&branch, &base, &commits, &diff);
            let response = run_prompt(agent, &prompt, session_total, model).await.text;

            // 5. Parse the AI's response
            let (title, body) = match parse_pr_description(&response) {
                Some(parsed) => parsed,
                None => {
                    eprintln!(
                        "{RED}  error: could not parse AI response into PR title/description{RESET}"
                    );
                    eprintln!("{DIM}  (try again or create manually with `gh pr create`){RESET}\n");
                    return;
                }
            };

            println!("{DIM}  Title: {BOLD}{title}{RESET}");
            println!("{DIM}  Draft: {}{RESET}", if draft { "yes" } else { "no" });

            // 6. Create the PR via gh CLI
            let mut gh_args = vec![
                "pr".to_string(),
                "create".to_string(),
                "--title".to_string(),
                title.clone(),
                "--body".to_string(),
                body,
                "--base".to_string(),
                base.clone(),
            ];
            if draft {
                gh_args.push("--draft".to_string());
            }

            let gh_str_args: Vec<&str> = gh_args.iter().map(|s| s.as_str()).collect();
            match std::process::Command::new("gh").args(&gh_str_args).output() {
                Ok(output) if output.status.success() => {
                    let url = String::from_utf8_lossy(&output.stdout);
                    let url = url.trim();
                    if url.is_empty() {
                        println!("{GREEN}  ✓ PR created: {title}{RESET}\n");
                    } else {
                        println!("{GREEN}  ✓ PR created: {url}{RESET}\n");
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Help => {
            println!("{DIM}  usage: /pr                         List open pull requests");
            println!(
                "         /pr create [--draft]        Create PR with AI-generated description"
            );
            println!("         /pr <number>                View details of a specific PR");
            println!("         /pr <number> diff           Show the diff of a PR");
            println!("         /pr <number> review         AI-powered code review of a PR");
            println!("         /pr <number> comment <text> Add a comment to a PR");
            println!("         /pr <number> checkout       Checkout a PR locally{RESET}\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::is_unknown_command;

    #[test]
    fn parse_pr_args_empty_is_list() {
        assert_eq!(parse_pr_args(""), PrSubcommand::List);
        assert_eq!(parse_pr_args("  "), PrSubcommand::List);
    }

    #[test]
    fn parse_pr_args_number_is_view() {
        assert_eq!(parse_pr_args("42"), PrSubcommand::View(42));
        assert_eq!(parse_pr_args("1"), PrSubcommand::View(1));
        assert_eq!(parse_pr_args("  99  "), PrSubcommand::View(99));
    }

    #[test]
    fn parse_pr_args_number_diff() {
        assert_eq!(parse_pr_args("42 diff"), PrSubcommand::Diff(42));
    }

    #[test]
    fn parse_pr_args_number_checkout() {
        assert_eq!(parse_pr_args("7 checkout"), PrSubcommand::Checkout(7));
    }

    #[test]
    fn parse_pr_args_number_review() {
        assert_eq!(parse_pr_args("42 review"), PrSubcommand::Review(42, false));
    }

    #[test]
    fn parse_pr_args_review_number() {
        assert_eq!(parse_pr_args("review 42"), PrSubcommand::Review(42, false));
    }

    #[test]
    fn parse_pr_args_review_case_insensitive() {
        assert_eq!(parse_pr_args("Review 10"), PrSubcommand::Review(10, false));
        assert_eq!(parse_pr_args("REVIEW 10"), PrSubcommand::Review(10, false));
    }

    #[test]
    fn parse_pr_args_review_no_number_is_help() {
        assert_eq!(parse_pr_args("review"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("review abc"), PrSubcommand::Help);
    }

    #[test]
    fn parse_pr_args_review_post() {
        assert_eq!(
            parse_pr_args("42 review --post"),
            PrSubcommand::Review(42, true)
        );
        assert_eq!(
            parse_pr_args("review 42 --post"),
            PrSubcommand::Review(42, true)
        );
    }

    #[test]
    fn parse_pr_args_review_post_case_insensitive() {
        assert_eq!(
            parse_pr_args("42 REVIEW --POST"),
            PrSubcommand::Review(42, true)
        );
    }

    #[test]
    fn parse_pr_args_review_no_post_flag() {
        // Without --post, post should be false
        assert_eq!(parse_pr_args("42 review"), PrSubcommand::Review(42, false));
        assert_eq!(parse_pr_args("review 42"), PrSubcommand::Review(42, false));
    }

    #[test]
    fn parse_pr_args_number_comment() {
        assert_eq!(
            parse_pr_args("5 comment looks good!"),
            PrSubcommand::Comment(5, "looks good!".to_string())
        );
    }

    #[test]
    fn parse_pr_args_comment_without_text_is_help() {
        assert_eq!(parse_pr_args("5 comment"), PrSubcommand::Help);
    }

    #[test]
    fn parse_pr_args_create() {
        assert_eq!(
            parse_pr_args("create"),
            PrSubcommand::Create { draft: false }
        );
    }

    #[test]
    fn parse_pr_args_create_draft() {
        assert_eq!(
            parse_pr_args("create --draft"),
            PrSubcommand::Create { draft: true }
        );
    }

    #[test]
    fn parse_pr_args_create_case_insensitive() {
        assert_eq!(
            parse_pr_args("CREATE"),
            PrSubcommand::Create { draft: false }
        );
        // --Draft with capital D: trim_start_matches('-') → "Draft", eq_ignore_ascii_case("draft") → true
        assert_eq!(
            parse_pr_args("Create --Draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("create -draft"),
            PrSubcommand::Create { draft: true }
        );
    }

    #[test]
    fn parse_pr_args_invalid_is_help() {
        assert_eq!(parse_pr_args("foobar"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("abc 123"), PrSubcommand::Help);
    }

    #[test]
    fn parse_pr_args_unknown_subcommand_is_help() {
        assert_eq!(parse_pr_args("42 merge"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 close"), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_command_recognized() {
        assert!(!is_unknown_command("/pr"));
        assert!(!is_unknown_command("/pr 42"));
        assert!(!is_unknown_command("/pr 123"));
    }

    #[test]
    fn test_pr_command_matching() {
        // /pr should match exact or with space separator, not /print etc.
        let pr_matches = |s: &str| s == "/pr" || s.starts_with("/pr ");
        assert!(pr_matches("/pr"));
        assert!(pr_matches("/pr 42"));
        assert!(pr_matches("/pr 123"));
        assert!(!pr_matches("/print"));
        assert!(!pr_matches("/process"));
    }

    #[test]
    fn test_pr_number_parsing() {
        // Verify we can parse a PR number from /pr <number>
        let input = "/pr 42";
        let arg = input.strip_prefix("/pr").unwrap_or("").trim();
        assert_eq!(arg, "42");
        assert!(arg.parse::<u32>().is_ok());
        assert_eq!(arg.parse::<u32>().unwrap(), 42);

        // Bare /pr has empty arg
        let input_bare = "/pr";
        let arg_bare = input_bare.strip_prefix("/pr").unwrap_or("").trim();
        assert!(arg_bare.is_empty());
    }

    #[test]
    fn test_pr_subcommand_list() {
        assert_eq!(parse_pr_args(""), PrSubcommand::List);
        assert_eq!(parse_pr_args("  "), PrSubcommand::List);
    }

    #[test]
    fn test_pr_subcommand_view() {
        assert_eq!(parse_pr_args("42"), PrSubcommand::View(42));
        assert_eq!(parse_pr_args("123"), PrSubcommand::View(123));
        assert_eq!(parse_pr_args("1"), PrSubcommand::View(1));
    }

    #[test]
    fn test_pr_subcommand_diff() {
        assert_eq!(parse_pr_args("42 diff"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("7 diff"), PrSubcommand::Diff(7));
    }

    #[test]
    fn test_pr_subcommand_checkout() {
        assert_eq!(parse_pr_args("42 checkout"), PrSubcommand::Checkout(42));
        assert_eq!(parse_pr_args("99 checkout"), PrSubcommand::Checkout(99));
    }

    #[test]
    fn test_pr_subcommand_comment() {
        assert_eq!(
            parse_pr_args("42 comment looks good!"),
            PrSubcommand::Comment(42, "looks good!".to_string())
        );
        assert_eq!(
            parse_pr_args("10 comment LGTM, merging now"),
            PrSubcommand::Comment(10, "LGTM, merging now".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_comment_requires_text() {
        // comment without text should show help
        assert_eq!(parse_pr_args("42 comment"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 comment  "), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_invalid() {
        assert_eq!(parse_pr_args("abc"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 unknown"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 merge"), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_case_insensitive() {
        assert_eq!(parse_pr_args("42 DIFF"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("42 Checkout"), PrSubcommand::Checkout(42));
        assert_eq!(
            parse_pr_args("42 Comment nice work"),
            PrSubcommand::Comment(42, "nice work".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_create() {
        assert_eq!(
            parse_pr_args("create"),
            PrSubcommand::Create { draft: false }
        );
        assert_eq!(
            parse_pr_args("CREATE"),
            PrSubcommand::Create { draft: false }
        );
        assert_eq!(
            parse_pr_args("Create"),
            PrSubcommand::Create { draft: false }
        );
    }

    #[test]
    fn test_pr_subcommand_create_draft() {
        assert_eq!(
            parse_pr_args("create --draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("create draft"),
            PrSubcommand::Create { draft: true }
        );
        assert_eq!(
            parse_pr_args("CREATE --DRAFT"),
            PrSubcommand::Create { draft: true }
        );
    }

    #[test]
    fn test_pr_subcommand_create_no_flag() {
        // "create somethingelse" should still create but not be draft
        assert_eq!(
            parse_pr_args("create --nodraft"),
            PrSubcommand::Create { draft: false }
        );
    }

    #[test]
    fn test_pr_subcommand_recognized() {
        // Subcommands should not be flagged as unknown commands
        assert!(!is_unknown_command("/pr 42 diff"));
        assert!(!is_unknown_command("/pr 42 comment hello"));
        assert!(!is_unknown_command("/pr 42 checkout"));
    }
}
