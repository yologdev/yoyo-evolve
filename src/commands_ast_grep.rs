//! Ast-grep structural search command handler: /ast.

use crate::format::*;

/// Subcommand completions for `/ast <Tab>`.
pub const AST_GREP_FLAGS: &[&str] = &["--lang", "--in"];

/// Check if ast-grep's `sg` binary is available on PATH.
pub fn is_ast_grep_available() -> bool {
    std::process::Command::new("sg")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Maximum number of lines `/ast` prints from one search.
///
/// Both sibling search commands bound their output (`GREP_MAX_MATCHES` for
/// `/grep`, `take(20)` for `/find`); `/ast` printed `sg`'s stdout unbounded
/// until Day 168. A judgment threshold for readability, not a measurement.
pub const AST_MAX_OUTPUT_LINES: usize = 200;

/// Cap `sg` output at [`AST_MAX_OUTPUT_LINES`], marking the cut in band.
///
/// Under budget the input is returned byte-identically. A silent elision would
/// be worse than a noisy one, so the marker states how many lines were dropped.
pub fn cap_ast_output(out: &str) -> String {
    let total = out.lines().count();
    if total <= AST_MAX_OUTPUT_LINES {
        return out.to_string();
    }
    let kept: Vec<&str> = out.lines().take(AST_MAX_OUTPUT_LINES).collect();
    format!(
        "{}\n… {} more line(s) elided — /ast shows the first {} (narrow the pattern, or scope it with --in)",
        kept.join("\n"),
        total - AST_MAX_OUTPUT_LINES,
        AST_MAX_OUTPUT_LINES
    )
}

/// Run ast-grep structural search.
/// Returns Ok(output) or Err(error message).
pub fn run_ast_grep_search(
    pattern: &str,
    lang: Option<&str>,
    path: Option<&str>,
) -> Result<String, String> {
    if !is_ast_grep_available() {
        return Err(
            "ast-grep (sg) is not installed. Install from: https://ast-grep.github.io/".into(),
        );
    }
    let mut cmd = std::process::Command::new("sg");
    cmd.arg("run").arg("--pattern").arg(pattern);
    if let Some(l) = lang {
        cmd.arg("--lang").arg(l);
    }
    if let Some(p) = path {
        cmd.arg(p);
    }
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            if stdout.trim().is_empty() {
                Ok("No matches found.".into())
            } else {
                Ok(cap_ast_output(&stdout))
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if stderr.trim().is_empty() {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                if stdout.trim().is_empty() {
                    Ok("No matches found.".into())
                } else {
                    Ok(cap_ast_output(&stdout))
                }
            } else {
                Err(format!("ast-grep error: {}", stderr.trim()))
            }
        }
        Err(e) => Err(format!("Failed to run sg: {e}")),
    }
}

/// Parse `/ast` command arguments into (pattern, lang, path).
/// Decide whether a bare trailing positional is a path the user meant to scope
/// the search to, rather than part of the pattern (#767).
///
/// `/ast` takes its path via `--in`, but a bare second positional is the natural
/// thing to reach for — and the catch-all arm of `parse_ast_grep_args` used to
/// fold it into the *pattern* string, so `/ast $X.unwrap() src/` searched for the
/// literal pattern `"$X.unwrap() src/"` and reported `No matches found.`: a silent
/// wrong-op, the worst of the three available behaviours.
///
/// This does **not** guess the user's intent by treating the token as the path —
/// guessing would silently mis-scope a legitimate pattern. It only recognises the
/// unambiguous shape so the caller can refuse with an actionable error:
///
/// - there must be **≥2** pattern tokens, so a single-token pattern that happens
///   to name a file (`/ast mod.rs`) is untouched and keeps its escape hatch;
/// - the **last** token only, since a path argument trails;
/// - and that token must actually exist on disk, per the injected `exists`
///   resolver — the I/O stays at the call site so this stays pure and testable.
///
/// Returns the offending token, or `None` when the input is not that shape.
fn bare_path_argument<'a>(
    pattern_parts: &[&'a str],
    exists: &dyn Fn(&str) -> bool,
) -> Option<&'a str> {
    if pattern_parts.len() < 2 {
        return None;
    }
    let last = *pattern_parts.last()?;
    if exists(last) {
        Some(last)
    } else {
        None
    }
}

/// The error shown when a bare positional path is detected (#767). Pure so the
/// wording is pinned by a test at the emission point, not one layer below it.
fn bare_path_error(token: &str) -> String {
    format!(
        "/ast takes its path via --in, so `{token}` would be searched for as part of the pattern.\n\
         Did you mean: /ast <pattern> --in {token}"
    )
}

pub fn parse_ast_grep_args(
    input: &str,
) -> Result<(String, Option<String>, Option<String>), String> {
    let rest = input.strip_prefix("/ast").unwrap_or("").trim();

    if rest.is_empty() {
        return Err("Usage: /ast <pattern> [--lang <lang>] [--in <path>]".into());
    }

    let parts: Vec<&str> = rest.split_whitespace().collect();
    let mut pattern_parts: Vec<&str> = Vec::new();
    let mut lang: Option<String> = None;
    let mut path: Option<String> = None;

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--lang" => {
                if i + 1 < parts.len() {
                    lang = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    return Err("--lang requires a value (e.g. --lang rust)".into());
                }
            }
            "--in" => {
                if i + 1 < parts.len() {
                    path = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    return Err("--in requires a value (e.g. --in src/)".into());
                }
            }
            other => {
                pattern_parts.push(other);
                i += 1;
            }
        }
    }

    if pattern_parts.is_empty() {
        return Err("Usage: /ast <pattern> [--lang <lang>] [--in <path>]".into());
    }

    // Only when no `--in` was given: with an explicit path the trailing token is
    // unambiguously part of the pattern.
    if path.is_none() {
        if let Some(token) =
            bare_path_argument(&pattern_parts, &|p| std::path::Path::new(p).exists())
        {
            return Err(bare_path_error(token));
        }
    }

    Ok((pattern_parts.join(" "), lang, path))
}

/// Handle the `/ast` REPL command.
pub fn handle_ast_grep(input: &str) {
    match parse_ast_grep_args(input) {
        Err(msg) => {
            println!("{YELLOW}  {msg}{RESET}\n");
        }
        Ok((pattern, lang, path)) => {
            if !is_ast_grep_available() {
                println!("{YELLOW}  ast-grep (sg) is not installed.{RESET}");
                println!("{DIM}  Install from: https://ast-grep.github.io/{RESET}");
                println!("{DIM}  Example: npm i -g @ast-grep/cli{RESET}\n");
                return;
            }
            println!("{DIM}  Searching for pattern: {pattern}{RESET}");
            match run_ast_grep_search(&pattern, lang.as_deref(), path.as_deref()) {
                Ok(output) => {
                    println!("{output}");
                }
                Err(e) => {
                    println!("{YELLOW}  {e}{RESET}\n");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- #767: a bare positional path is refused, not folded into the pattern ---

    #[test]
    fn bare_path_argument_needs_two_tokens_and_an_existing_path() {
        let yes = |_: &str| true;
        let no = |_: &str| false;

        // The shape it exists to catch.
        assert_eq!(
            bare_path_argument(&["$X.unwrap()", "src/"], &yes),
            Some("src/")
        );
        // A single-token pattern keeps its escape hatch even if it names a file.
        assert_eq!(bare_path_argument(&["mod.rs"], &yes), None);
        // A trailing token that is not on disk is pattern syntax, not a path.
        assert_eq!(
            bare_path_argument(&["fn", "$N()", "{", "$$$", "}"], &no),
            None
        );
        // Only the LAST token is considered — a path argument trails.
        assert_eq!(bare_path_argument(&["src/", "$X"], &|p| p == "src/"), None);
        assert_eq!(bare_path_argument(&[], &yes), None);
    }

    #[test]
    fn parse_refuses_a_bare_positional_path_with_an_actionable_error() {
        // `src` exists in this repo, so this is the real live shape.
        let err = parse_ast_grep_args("/ast $X.unwrap() src")
            .expect_err("a bare positional path should be refused, not folded into the pattern");
        assert!(
            err.contains("--in src"),
            "the error must name the escape hatch verbatim, got: {err}"
        );
        assert!(
            err.contains("src"),
            "the error must name the offending token, got: {err}"
        );
    }

    #[test]
    fn parse_leaves_ordinary_patterns_byte_identical() {
        // Multi-token pattern whose tokens are not paths: unchanged.
        let (pattern, lang, path) =
            parse_ast_grep_args("/ast fn $NAME() { $$$ }").expect("plain pattern should parse");
        assert_eq!(pattern, "fn $NAME() { $$$ }");
        assert_eq!(lang, None);
        assert_eq!(path, None);

        // An explicit --in disambiguates: the trailing token stays pattern.
        let (pattern, _, path) =
            parse_ast_grep_args("/ast $X.unwrap() --in src").expect("--in form should parse");
        assert_eq!(pattern, "$X.unwrap()");
        assert_eq!(path.as_deref(), Some("src"));
    }

    #[test]
    fn test_is_ast_grep_available_no_panic() {
        // Should not panic regardless of whether sg is installed
        let _ = is_ast_grep_available();
    }

    #[test]
    fn test_ast_grep_search_no_sg() {
        // When sg is not installed, should return a helpful error
        if !is_ast_grep_available() {
            let result = run_ast_grep_search("$X.unwrap()", None, None);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("not installed"));
        }
    }

    #[test]
    fn test_ast_in_known_commands() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/ast"),
            "/ast should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_ast_in_help_text() {
        use crate::help::help_text;
        let help = help_text();
        assert!(help.contains("/ast"), "/ast should appear in help text");
    }

    #[test]
    fn test_cap_ast_output_under_budget_is_byte_identical() {
        let small = "src/a.rs:1:foo\nsrc/b.rs:2:bar\n";
        assert_eq!(cap_ast_output(small), small);
        let exact = (0..AST_MAX_OUTPUT_LINES)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(cap_ast_output(&exact), exact, "at the cap, nothing is cut");
    }

    #[test]
    fn test_cap_ast_output_marks_the_cut_in_band() {
        let big = (0..AST_MAX_OUTPUT_LINES + 37)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let capped = cap_ast_output(&big);
        // The elision is announced, and the count it reports is the real one.
        assert!(
            capped.contains("37 more line(s) elided"),
            "cut must be marked in band with the true count, got: {capped}"
        );
        // Kept lines + exactly one marker line.
        assert_eq!(capped.lines().count(), AST_MAX_OUTPUT_LINES + 1);
        assert!(capped.lines().next() == Some("line 0"));
        assert!(
            capped.contains(&format!("line {}", AST_MAX_OUTPUT_LINES - 1)),
            "the last kept line must survive"
        );
        assert!(
            !capped.contains(&format!("line {}\n", AST_MAX_OUTPUT_LINES)),
            "the first dropped line must not survive"
        );
    }

    #[test]
    fn test_cap_ast_output_is_char_boundary_safe() {
        // Multi-byte content must not panic and must not be sliced mid-char.
        let big = (0..AST_MAX_OUTPUT_LINES + 5)
            .map(|i| format!("✓ src/файл{i}.rs:1:さくら"))
            .collect::<Vec<_>>()
            .join("\n");
        let capped = cap_ast_output(&big);
        assert!(capped.contains("5 more line(s) elided"));
        assert!(capped.contains("さくら"));
    }

    #[test]
    fn test_parse_ast_grep_args_simple_pattern() {
        let result = parse_ast_grep_args("/ast $X.unwrap()");
        assert!(result.is_ok());
        let (pattern, lang, path) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert!(lang.is_none());
        assert!(path.is_none());
    }

    #[test]
    fn test_parse_ast_grep_args_with_lang() {
        let result = parse_ast_grep_args("/ast $X.unwrap() --lang rust");
        assert!(result.is_ok());
        let (pattern, lang, path) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert_eq!(lang.as_deref(), Some("rust"));
        assert!(path.is_none());
    }

    #[test]
    fn test_parse_ast_grep_args_with_lang_and_path() {
        let result = parse_ast_grep_args("/ast $X.unwrap() --lang rust --in src/");
        assert!(result.is_ok());
        let (pattern, lang, path) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert_eq!(lang.as_deref(), Some("rust"));
        assert_eq!(path.as_deref(), Some("src/"));
    }

    #[test]
    fn test_parse_ast_grep_args_flags_before_pattern() {
        let result = parse_ast_grep_args("/ast --lang rust $X.unwrap()");
        assert!(result.is_ok());
        let (pattern, lang, _) = result.unwrap();
        assert_eq!(pattern, "$X.unwrap()");
        assert_eq!(lang.as_deref(), Some("rust"));
    }

    #[test]
    fn test_parse_ast_grep_args_empty() {
        let result = parse_ast_grep_args("/ast");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Usage"));
    }

    #[test]
    fn test_parse_ast_grep_args_missing_lang_value() {
        let result = parse_ast_grep_args("/ast $X --lang");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("--lang requires"));
    }

    #[test]
    fn test_parse_ast_grep_args_missing_in_value() {
        let result = parse_ast_grep_args("/ast $X --in");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("--in requires"));
    }

    /// A bare trailing positional is never silently turned into the search path.
    ///
    /// This is the parser's real contract (multi-token patterns like
    /// `fn $NAME($$$ARGS) -> $RET` depend on the join), and it was unasserted until
    /// #767: the completion hint advertised `/ast <pattern> [path]`, so
    /// `/ast $X.unwrap() src/` searched for the literal pattern `"$X.unwrap() src/"`
    /// and reported `No matches found.` — a silent wrong-op, not an error.
    #[test]
    fn bare_trailing_positional_joins_the_pattern_rather_than_setting_the_path() {
        // FIXED (#767): a bare trailing positional that names an existing path is
        // now REFUSED with an actionable error. It is still never silently turned
        // into the search path — the parser does not guess, it asks.
        let err = parse_ast_grep_args("/ast $X.unwrap() src/")
            .expect_err("a bare positional naming a real path must be refused");
        assert!(err.contains("--in src/"), "got: {err}");

        // The other direction, unchanged: a trailing token that is not a path is
        // ordinary pattern syntax and still joins the pattern.
        let (pattern, lang, path) =
            parse_ast_grep_args("/ast $X.unwrap() $Y").expect("should parse");
        assert_eq!(pattern, "$X.unwrap() $Y");
        assert_eq!(lang, None);
        assert_eq!(
            path, None,
            "a bare positional must not silently become the search path"
        );
    }

    /// Drift guard for #767, in the direction the bug actually lived.
    ///
    /// The completion hint is the surface most likely to be trusted (the REPL prints it
    /// unprompted), and it disagreed with both the parser and the detailed help for as long
    /// as `/ast` has existed. Two opposite checks:
    ///
    /// 1. every `--flag` the hint advertises is really *consumed* as a flag — not folded
    ///    into the pattern by the catch-all arm, which is how a plausible-but-wrong flag
    ///    (say `--path`) would fail: silently, with no error and no match;
    /// 2. the hint advertises no bare optional positional, because the parser honours none.
    #[test]
    fn ast_hint_advertises_only_argument_shapes_the_parser_honours() {
        let hint = crate::commands::command_arg_hint("ast").expect("/ast should have an arg hint");

        for token in hint.split_whitespace() {
            let flag = token.trim_start_matches('[');
            if flag.starts_with("--") {
                let (pattern, _, _) = parse_ast_grep_args(&format!("/ast PAT {flag} VALUE"))
                    .unwrap_or_else(|e| panic!("hint {hint:?} advertises {flag}, rejected: {e}"));
                assert_eq!(
                    pattern, "PAT",
                    "hint {hint:?} advertises {flag}, but the parser folds it into the pattern \
                     instead of consuming it as a flag"
                );
            } else if token.starts_with('[') {
                panic!(
                    "hint {hint:?} advertises the bare optional positional {token}, but \
                     parse_ast_grep_args folds every non-flag token into the pattern (#767)"
                );
            }
        }

        for flag in ["--lang", "--in"] {
            assert!(hint.contains(flag), "hint {hint:?} should advertise {flag}");
        }
    }

    #[test]
    fn test_ast_tab_completion() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/ast", "");
        assert!(
            candidates.contains(&"--lang".to_string()),
            "Should include '--lang'"
        );
        assert!(
            candidates.contains(&"--in".to_string()),
            "Should include '--in'"
        );
    }

    #[test]
    fn test_ast_tab_completion_filters() {
        use crate::commands::command_arg_completions;
        let candidates = command_arg_completions("/ast", "--l");
        assert!(
            candidates.contains(&"--lang".to_string()),
            "Should include '--lang' for prefix '--l'"
        );
        assert!(
            !candidates.contains(&"--in".to_string()),
            "Should not include '--in' for prefix '--l'"
        );
    }

    #[test]
    fn test_handle_ast_grep_no_panic_empty() {
        // Should not panic on empty input
        handle_ast_grep("/ast");
    }

    #[test]
    fn test_handle_ast_grep_no_panic_with_pattern() {
        // Should not panic even if sg is not installed
        handle_ast_grep("/ast $X.unwrap()");
    }
}
