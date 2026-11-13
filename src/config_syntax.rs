//! Syntax checks for the config file that `parse_config_file` cannot report (#1034).
//!
//! `config::parse_config_file` is a lenient, hand-rolled line parser with no
//! error channel: on mismatched quotes it keeps the raw value (stray quote and
//! all), and an unclosed `[section` header is silently skipped. So
//! `model = "claude-haiku-5-5` (no closing quote) made `yoyo config get model`
//! print the value WITH the quote and exit 0, and `-p` then sent that id and got
//! a 404, while nothing anywhere said the file was malformed.
//!
//! This module is deliberately **detection only**. What values parse to is
//! unchanged, byte-for-byte; changing that is a separate decision. The detector
//! is pure and table-tested here; `config::load_config_file` calls it on the
//! text of the file that won the search and prints the warning once per run.
//!
//! What it checks, and only this:
//! - a `key = value` line whose value *starts* with a quote, array or inline
//!   table and leaves a `"` or `'` string open at end of line. Bare values
//!   (`notify_command = echo it's done`) are not scanned: the parser accepts
//!   them as-is and calling an apostrophe there an "unterminated quote" would
//!   be a false alarm. Multi-line strings (`"""` / `'''`) are skipped too —
//!   the parser does not support them, but they are valid TOML and a
//!   "unterminated quote" claim about one would be wrong.
//! - a line starting with `[` that has no `]` anywhere (an unclosed header).
//!
//! Lines with no `=` that are not headers (e.g. continuation lines of a
//! multi-line array, which `parse_config_file` ignores) are never examined.

/// Kind string for an unterminated `"` / `'` string value.
pub(crate) const UNTERMINATED_QUOTE: &str = "unterminated quote";
/// Kind string for a `[section` header with no closing `]`.
pub(crate) const UNCLOSED_HEADER: &str = "unclosed [section] header";

/// Every syntax problem in `text`, as `(1-based line number, kind)`.
/// Empty when the file has none of the shapes this module checks.
pub(crate) fn config_syntax_problems(text: &str) -> Vec<(usize, &'static str)> {
    let mut problems = Vec::new();
    // The closing delimiter of a multi-line string we are inside, if any. Its
    // body lines are string content, not TOML, so a `[x` or `k = "v` there is
    // never a problem (#1036: a false positive here refuses startup).
    let mut open_multiline: Option<&str> = None;
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if let Some(delim) = open_multiline {
            if line.contains(delim) {
                open_multiline = None;
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if !line.contains(']') {
                problems.push((idx + 1, UNCLOSED_HEADER));
            }
            continue;
        }
        let Some((_, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let opened = ["\"\"\"", "'''"]
            .into_iter()
            .find_map(|d| value.strip_prefix(d).map(|rest| (d, rest)));
        if let Some((delim, rest)) = opened {
            if !rest.contains(delim) {
                open_multiline = Some(delim);
            }
            continue;
        }
        if value_leaves_string_open(value) {
            problems.push((idx + 1, UNTERMINATED_QUOTE));
        }
    }
    problems
}

/// Does `value` (already trimmed) open a string it never closes on this line?
fn value_leaves_string_open(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    if !matches!(first, '"' | '\'' | '[' | '{') {
        return false;
    }
    if value.starts_with("\"\"\"") || value.starts_with("'''") {
        return false;
    }
    #[derive(PartialEq)]
    enum State {
        Outside,
        Basic,
        Literal,
    }
    let mut state = State::Outside;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match state {
            State::Outside => match c {
                '#' => break,
                '"' => state = State::Basic,
                '\'' => state = State::Literal,
                _ => {}
            },
            State::Basic => match c {
                '\\' => {
                    chars.next();
                }
                '"' => state = State::Outside,
                _ => {}
            },
            State::Literal => {
                if c == '\'' {
                    state = State::Outside;
                }
            }
        }
    }
    state != State::Outside
}

/// The warning text for `problems` found in the config file at `path`, or
/// `None` when there are none (the common case, which must print nothing).
/// The caller adds the `warning:` prefix, like the #1028 numeric warning.
/// `path` comes from the filesystem (a repo-authored name for a project-local
/// file), so it goes through `cli::sanitize_for_display` before display.
pub(crate) fn config_syntax_warning(
    path: &str,
    problems: &[(usize, &'static str)],
) -> Option<String> {
    if problems.is_empty() {
        return None;
    }
    let path = crate::cli::sanitize_for_display(path);
    let locations: Vec<String> = problems
        .iter()
        .map(|(line, kind)| format!("{path}:{line}: {kind}"))
        .collect();
    let noun = if problems.len() == 1 {
        "problem"
    } else {
        "problems"
    };
    Some(format!(
        "config file is malformed ({} syntax {noun}); affected values are read as written, \
         stray quotes included: {}",
        problems.len(),
        locations.join("; ")
    ))
}

/// Exit code when an agent entry point refuses a malformed config (#1036).
/// 2, the family of yoyo's other usage/config refusals
/// (`setup::SETUP_NOT_INTERACTIVE_EXIT`, the bare-word near-miss guard).
pub(crate) const CONFIG_SYNTAX_REFUSAL_EXIT: i32 = 2;

/// The startup refusal for the config file `shown` holding `content`, or
/// `None` when the file has no syntax problem this module detects. That
/// `None` is the whole regression surface: every clean file starts as before.
///
/// Why refuse rather than warn (#1036): nobody writes an unterminated quote
/// on purpose, so there is no deliberate user for a quiet default to protect.
/// Everyone on this branch made a typo they don't know about, and the
/// lenient parser would send the value stray quote included (a 404 for
/// `model`) with exit 0. git, cargo and Python's tomllib all refuse the same
/// shape. `shown` is interpolated, so it goes through `cli::sanitize_for_display`.
pub(crate) fn config_syntax_refusal(shown: &str, content: &str) -> Option<String> {
    let problems = config_syntax_problems(content);
    if problems.is_empty() {
        return None;
    }
    let path = crate::cli::sanitize_for_display(shown);
    let locations: Vec<String> = problems
        .iter()
        .map(|(line, kind)| format!("  {path}:{line}: {kind}"))
        .collect();
    let lines: Vec<String> = problems.iter().map(|(line, _)| line.to_string()).collect();
    let (noun, which) = if problems.len() == 1 {
        ("problem", "line")
    } else {
        ("problems", "lines")
    };
    Some(format!(
        "config file is malformed ({} syntax {noun}); not starting, because the \
         affected values would be used as written, stray quotes included:\n{}\n\
         To fix: correct {which} {} of {path}, or move the file aside \
         (yoyo then uses the next config file it finds, or its defaults).",
        problems.len(),
        locations.join("\n"),
        lines.join(", "),
    ))
}

/// Does this invocation reach an agent entry point (REPL, `-p`, piped
/// single-shot) through `parse_args`, rather than a shell subcommand?
/// `args[0]` is the binary path. Routed subcommands are dispatched before
/// `parse_args` loads the config; the inspection verbs among them (`config
/// get/show`, `doctor`, `model`, `permissions`) keep the warning so the user
/// can still look at the file the refusal names, and `review`, the one routed
/// verb that calls a provider, opts in itself.
pub(crate) fn invocation_refuses_malformed_config(args: &[String]) -> bool {
    !args
        .get(1)
        .is_some_and(|a| crate::dispatch_near_miss::ROUTED_SUBCOMMANDS.contains(&a.as_str()))
}

/// Set by an agent entry point before it loads the config: from then on a
/// malformed loaded file is refused (exit 2) instead of warned about.
static REFUSE_MALFORMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Opt this process into refusing a malformed config at load time (#1036).
pub(crate) fn refuse_malformed_config_on_load() {
    REFUSE_MALFORMED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Called by `load_config_file` for the file it picked. In refuse mode a
/// malformed file prints the refusal and exits `CONFIG_SYNTAX_REFUSAL_EXIT`
/// before any provider call; otherwise it is the #1034 warning, unchanged.
pub(crate) fn check_loaded_config(shown: &str, content: &str) {
    if REFUSE_MALFORMED.load(std::sync::atomic::Ordering::SeqCst) {
        if let Some(msg) = config_syntax_refusal(shown, content) {
            use crate::format::{RED, RESET};
            eprintln!("{RED}error:{RESET} {msg}");
            crate::dispatch_sub::exit_process(CONFIG_SYNTAX_REFUSAL_EXIT);
        }
        return;
    }
    warn_config_syntax_once(shown, content);
}

/// Set once a syntax warning has been printed, so it appears once per run.
/// `load_config_file` is called several times in one process (argument
/// parsing, the piped path, the REPL), and each call re-reads the same file.
static SYNTAX_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Print the #1034 warning for `content` (read from `shown`) to stderr, at
/// most once per process. Not quiet-gated, like the #1028 numeric warning: a
/// malformed file is a fault, not startup chatter.
pub(crate) fn warn_config_syntax_once(shown: &str, content: &str) {
    use std::sync::atomic::Ordering;
    if let Some(w) = config_syntax_warning(shown, &config_syntax_problems(content)) {
        if !SYNTAX_WARNED.swap(true, Ordering::SeqCst) {
            use crate::format::{RESET, YELLOW};
            eprintln!("{YELLOW}warning:{RESET} {w}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_shapes_fire_on_the_right_line() {
        let cases: &[(&str, Vec<(usize, &str)>)] = &[
            (
                "provider = \"anthropic\"\nmodel = \"claude-haiku-5-5\n",
                vec![(2, UNTERMINATED_QUOTE)],
            ),
            ("model = 'claude-haiku-5-5\n", vec![(1, UNTERMINATED_QUOTE)]),
            // An escaped quote is not a closing quote.
            ("model = \"abc\\\"\n", vec![(1, UNTERMINATED_QUOTE)]),
            (
                "[permissions]\nallow = [\"a\", \"b]\n",
                vec![(2, UNTERMINATED_QUOTE)],
            ),
            (
                "model = \"x\"\n\n[permissions\nallow = [\"a\"]\n",
                vec![(3, UNCLOSED_HEADER)],
            ),
            (
                "model = \"x\n[mcp_servers.fs\n",
                vec![(1, UNTERMINATED_QUOTE), (2, UNCLOSED_HEADER)],
            ),
        ];
        for (text, want) in cases {
            assert_eq!(&config_syntax_problems(text), want, "input: {text:?}");
        }
    }

    #[test]
    fn near_misses_report_nothing() {
        let clean = [
            "model = \"claude-haiku-5-5\"",
            "model = 'claude-haiku-5-5'",
            "model = \"x\" # the model I use",
            "model = \"a # not a comment\"",
            "notify_command = \"echo \\\"done\\\"\"",
            "path = 'C:\\dir\\'",
            "allow = [\"a\", \"b\"]",
            "allow = ['a', \"it's\"]",
            "env = { KEY = \"v\" }",
            "max_tokens = 4096",
            "notify_command = echo it's done",
            "allow = [",
            "  \"git *\",",
            "]",
            "[mcp_servers.x]",
            "[permissions] # comment",
            "# model = \"unterminated in a comment",
            "",
            "key = \"✓ multi-byte ✓\"",
            // Last on purpose: an unclosed `\"\"\"` makes every later line string
            // body (#1036), so earlier lines would go unscanned.
            "prompt = \"\"\"",
        ]
        .join("\n");
        assert_eq!(config_syntax_problems(&clean), Vec::new());
    }

    /// Step 1 of #1036: before a malformed file blocks startup, every valid
    /// TOML shape below must report NOTHING. A false positive here is a dead
    /// CLI (including this repo's own evolve loop), not a stray warning.
    #[test]
    fn valid_toml_shapes_never_fire() {
        let shapes: &[(&str, &str)] = &[
            (
                "multi-line basic string with a quote inside",
                "prompt = \"\"\"\nsay \"hi\" now\n\"\"\"\n",
            ),
            ("multi-line literal string", "prompt = '''\nit's \"x\n'''\n"),
            (
                "literal string holding a double quote",
                "x = 'a \"quoted\" word'\n",
            ),
            ("literal strings side by side", "x = ['it', 's']\n"),
            ("escaped quotes", "x = \"a \\\"b\\\" c\"\n"),
            (
                "comment holding a lone quote",
                "# say \"hi\nmodel = \"m\"\n",
            ),
            (
                "trailing comment with a quote",
                "model = \"m\" # a \" here\n",
            ),
            ("array of tables", "[[array.of.tables]]\nname = \"a\"\n"),
            ("inline table", "env = { a = \"b\" }\n"),
            (
                "array of strings spanning lines",
                "allow = [\n  \"git *\",\n  \"cargo *\",\n]\n",
            ),
            ("bracket inside a string value", "x = \"[not a header\"\n"),
            ("indented section header", "  [permissions]\n"),
            (
                "multi-line string whose body looks like a header and an open quote",
                "prompt = \"\"\"\n[not a header\nkey = \"half\n\"\"\"\nmodel = \"m\"\n",
            ),
            (
                "multi-line literal string whose body looks malformed",
                "prompt = '''\n[x\na = 'b\n'''\n",
            ),
            (
                "one-line triple-quoted string",
                "x = \"\"\"one line\"\"\"\n[s]\n",
            ),
        ];
        for (name, text) in shapes {
            assert_eq!(config_syntax_problems(text), Vec::new(), "{name}: {text:?}");
            assert_eq!(config_syntax_refusal(".yoyo.toml", text), None, "{name}");
        }
        // This repo's own config, read from disk: the evolve loop loads it on every run.
        let own = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/.yoyo.toml"))
            .expect("repo .yoyo.toml must exist");
        assert!(own.contains("model"), "anti-vacuous: read the real file");
        assert_eq!(config_syntax_problems(&own), Vec::new());
        assert_eq!(config_syntax_refusal(".yoyo.toml", &own), None);
    }

    #[test]
    fn refusal_names_file_line_problem_and_remedy() {
        assert_eq!(
            config_syntax_refusal(
                ".yoyo.toml",
                "provider = \"anthropic\"\nmodel = \"claude-haiku-5-5\n"
            )
            .as_deref(),
            Some(
                "config file is malformed (1 syntax problem); not starting, because the \
                 affected values would be used as written, stray quotes included:\n  \
                 .yoyo.toml:2: unterminated quote\n\
                 To fix: correct line 2 of .yoyo.toml, or move the file aside \
                 (yoyo then uses the next config file it finds, or its defaults)."
            )
        );
        assert_eq!(
            config_syntax_refusal("a.toml", "model = \"x\n\n[mcp_servers.fs\n").as_deref(),
            Some(
                "config file is malformed (2 syntax problems); not starting, because the \
                 affected values would be used as written, stray quotes included:\n  \
                 a.toml:1: unterminated quote\n  a.toml:3: unclosed [section] header\n\
                 To fix: correct lines 1, 3 of a.toml, or move the file aside \
                 (yoyo then uses the next config file it finds, or its defaults)."
            )
        );
    }

    #[test]
    fn refusal_is_none_for_clean_and_empty_files() {
        for clean in [
            "",
            "model = \"claude-haiku-5-5\"\n",
            "[permissions]\nallow = [\"a\"]\n",
        ] {
            assert_eq!(
                config_syntax_refusal(".yoyo.toml", clean),
                None,
                "{clean:?}"
            );
        }
    }

    #[test]
    fn refusal_escapes_control_bytes_in_path() {
        let path = "evil\x1b[31m.toml";
        assert!(path.as_bytes().contains(&0x1b), "fixture must be hostile");
        let out = config_syntax_refusal(path, "[x\n").unwrap();
        assert!(!out.as_bytes().contains(&0x1b), "got {out:?}");
    }

    #[test]
    fn only_non_subcommand_invocations_refuse() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let cases: &[(&[&str], bool)] = &[
            (&["yoyo"], true),
            (&["yoyo", "-p", "hi"], true),
            (&["yoyo", "--model", "m"], true),
            (&["yoyo", "explain this"], true),
            (&["yoyo", "config", "get", "model"], false),
            (&["yoyo", "doctor"], false),
            (&["yoyo", "setup"], false),
            (&["yoyo", "review"], false),
        ];
        for (args, want) in cases {
            assert_eq!(
                invocation_refuses_malformed_config(&a(args)),
                *want,
                "{args:?}"
            );
        }
    }

    #[test]
    fn composer_is_none_for_no_problems() {
        assert_eq!(config_syntax_warning(".yoyo.toml", &[]), None);
    }

    #[test]
    fn composer_renders_full_warning() {
        assert_eq!(
            config_syntax_warning(".yoyo.toml", &[(2, UNTERMINATED_QUOTE)]).as_deref(),
            Some(
                "config file is malformed (1 syntax problem); affected values are read as \
                 written, stray quotes included: .yoyo.toml:2: unterminated quote"
            )
        );
        assert_eq!(
            config_syntax_warning("a.toml", &[(1, UNTERMINATED_QUOTE), (4, UNCLOSED_HEADER)])
                .as_deref(),
            Some(
                "config file is malformed (2 syntax problems); affected values are read as \
                 written, stray quotes included: a.toml:1: unterminated quote; \
                 a.toml:4: unclosed [section] header"
            )
        );
    }

    #[test]
    fn composer_escapes_control_bytes_in_path() {
        let path = "evil\x1b[31m.toml";
        assert!(path.as_bytes().contains(&0x1b), "fixture must be hostile");
        let out = config_syntax_warning(path, &[(1, UNCLOSED_HEADER)]).unwrap();
        assert!(!out.as_bytes().contains(&0x1b), "got {out:?}");
    }
}
