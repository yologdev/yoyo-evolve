//! Project tree command handler: /tree.

use crate::format::*;

/// Build a project tree from git-tracked files, up to `max_depth` levels deep.
pub fn build_project_tree(max_depth: usize) -> String {
    let files = match crate::git::run_git(&["ls-files"]) {
        Ok(text) => {
            let mut files: Vec<String> = text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            files.sort();
            files
        }
        Err(_) => return "(not a git repository — /tree requires git)".to_string(),
    };

    if files.is_empty() {
        return "(no tracked files)".to_string();
    }

    format_tree_from_paths(&files, max_depth)
}

/// Format a sorted list of file paths into an indented tree string.
pub fn format_tree_from_paths(paths: &[String], max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut output = String::new();
    let mut printed_dirs: BTreeSet<String> = BTreeSet::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let depth = parts.len() - 1;

        for level in 0..parts.len().saturating_sub(1).min(max_depth) {
            let dir_path: String = parts[..=level].join("/");
            let dir_key = format!("{}/", dir_path);
            if printed_dirs.insert(dir_key) {
                let indent = "  ".repeat(level);
                let dir_name = parts[level];
                output.push_str(&format!("{indent}{dir_name}/\n"));
            }
        }

        if depth <= max_depth {
            let indent = "  ".repeat(depth.min(max_depth));
            let file_name = parts.last().unwrap_or(&"");
            output.push_str(&format!("{indent}{file_name}\n"));
        }
    }

    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

/// Default tree depth when `/tree` is given no argument.
///
/// Named rather than inlined because six artifacts quote it back to the user
/// (`help_data.rs`, `help.rs` x2, `docs/src/usage/commands.md`, `README.md`,
/// and the usage line below), so the number needs one statement.
pub const DEFAULT_TREE_DEPTH: usize = 3;

/// What `/tree`'s argument asked for.
///
/// The whole grammar `/tree` accepts: nothing, or a bare non-negative integer.
/// Deliberately *not* `Option<usize>` — "no argument" and "unparseable
/// argument" are different facts with different outcomes (default vs. usage),
/// and folding them would print a tree when the user typed something we could
/// not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeArg {
    /// Render at this depth. Reached by both `/tree` (the default) and `/tree N`.
    Depth(usize),
    /// The argument was not a bare integer — print usage and render nothing.
    Usage,
}

/// Decide what `/tree <arg>` asked for. Pure: no I/O, no printing.
///
/// Split out of `handle_tree` on Day 182 (blind round 89) so the grammar this
/// command actually accepts can be asserted against the grammar it *advertises*
/// — see `tree_arg_hint_advertises_only_forms_the_parser_accepts`. Before the
/// split the decision was inline in a function whose only output is `println!`,
/// so nothing could ask it a question.
pub fn parse_tree_arg(input: &str) -> TreeArg {
    let arg = input.strip_prefix("/tree").unwrap_or("").trim();
    if arg.is_empty() {
        return TreeArg::Depth(DEFAULT_TREE_DEPTH);
    }
    match arg.parse::<usize>() {
        Ok(d) => TreeArg::Depth(d),
        Err(_) => TreeArg::Usage,
    }
}

pub fn handle_tree(input: &str) {
    let max_depth = match parse_tree_arg(input) {
        TreeArg::Depth(d) => d,
        TreeArg::Usage => {
            println!("{DIM}  usage: /tree [depth]  (default depth: {DEFAULT_TREE_DEPTH}){RESET}\n");
            return;
        }
    };
    let tree = build_project_tree(max_depth);
    println!("{DIM}{tree}{RESET}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::is_unknown_command;

    #[test]
    fn format_tree_basic() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("main.rs"));
        assert!(tree.contains("lib.rs"));
        assert!(tree.contains("Cargo.toml"));
    }

    #[test]
    fn format_tree_depth_limit() {
        let paths = vec!["a/b/c/d/e.txt".to_string()];
        let tree_shallow = format_tree_from_paths(&paths, 1);
        // At depth 1, we see dir 'a/' but 'b/' is at level 1 so still shown
        // The file at depth 4 should NOT appear since depth > max_depth
        assert!(tree_shallow.contains("a/"));
        // File at depth 4 should not appear when max_depth=1
        assert!(!tree_shallow.contains("e.txt"));
    }

    #[test]
    fn format_tree_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn format_tree_root_files() {
        let paths = vec!["README.md".to_string()];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("README.md"));
    }

    #[test]
    fn test_format_tree_from_paths_basic() {
        let paths = vec![
            "Cargo.toml".to_string(),
            "README.md".to_string(),
            "src/cli.rs".to_string(),
            "src/format.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("  main.rs"));
        assert!(tree.contains("  cli.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_nested() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/utils/helpers.rs".to_string(),
            "src/utils/format.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("  utils/"));
        assert!(tree.contains("    helpers.rs"));
        assert!(tree.contains("    format.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_limit() {
        let paths = vec![
            "a/b/c/d/deep.txt".to_string(),
            "a/shallow.txt".to_string(),
            "top.txt".to_string(),
        ];
        // depth 1: show dirs at level 0 ('a/'), files at depth ≤ 1
        let tree = format_tree_from_paths(&paths, 1);
        assert!(tree.contains("top.txt"));
        assert!(tree.contains("a/"));
        assert!(tree.contains("  shallow.txt"));
        // Files deeper than max_depth should not appear
        assert!(!tree.contains("deep.txt"));
        // Directory 'b/' is at level 1, beyond max_depth=1 for dirs
        assert!(!tree.contains("b/"));
    }

    #[test]
    fn test_format_tree_from_paths_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_format_tree_from_paths_root_files_only() {
        let paths = vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // No directories, just root files
        assert!(!tree.contains('/'));
        assert!(tree.contains("Cargo.lock"));
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_zero() {
        let paths = vec!["README.md".to_string(), "src/main.rs".to_string()];
        let tree = format_tree_from_paths(&paths, 0);
        // Depth 0: only root-level files shown
        assert!(tree.contains("README.md"));
        // main.rs is at depth 1, should not show at depth 0
        assert!(!tree.contains("main.rs"));
    }

    #[test]
    fn test_format_tree_dir_printed_once() {
        let paths = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // "src/" should appear exactly once
        assert_eq!(tree.matches("src/").count(), 1);
    }

    #[test]
    fn test_build_project_tree_runs() {
        // build_project_tree should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_tree_command_recognized() {
        assert!(!is_unknown_command("/tree"));
        assert!(!is_unknown_command("/tree 2"));
        assert!(!is_unknown_command("/tree 5"));
    }

    // ---- Blind round 89 (Day 182): the arg hint advertised a grammar the
    // parser rejects on contact. `commands.rs` said `[path] [--depth N]` while
    // `handle_tree` accepts only a bare integer — the `/map --depth` defect
    // (Day 164) verbatim, one row down in the same table. The hint was
    // corrected; these tests are the detector that replaces the mismatch,
    // because a doc-side repair of a doc/code mismatch otherwise deletes the
    // only thing that could have caught it.

    /// The parser's whole grammar, asserted at the emission point — the value a
    /// caller of `parse_tree_arg` receives, never a helper one layer below.
    #[test]
    fn parse_tree_arg_table() {
        let cases: &[(&str, TreeArg)] = &[
            // Absent argument is the default, not an error.
            ("/tree", TreeArg::Depth(DEFAULT_TREE_DEPTH)),
            ("/tree   ", TreeArg::Depth(DEFAULT_TREE_DEPTH)),
            // The documented form.
            ("/tree 5", TreeArg::Depth(5)),
            ("/tree 0", TreeArg::Depth(0)),
            ("/tree  2 ", TreeArg::Depth(2)),
            // Everything the hint used to advertise, and does not work.
            ("/tree --depth 5", TreeArg::Usage),
            ("/tree --depth=5", TreeArg::Usage),
            ("/tree src/", TreeArg::Usage),
            ("/tree docs/", TreeArg::Usage),
            ("/tree src/ --depth 2", TreeArg::Usage),
            // Neighbouring shapes that must also refuse rather than guess.
            ("/tree -1", TreeArg::Usage),
            ("/tree 2.5", TreeArg::Usage),
            ("/tree five", TreeArg::Usage),
        ];
        for (input, want) in cases {
            assert_eq!(
                parse_tree_arg(input),
                *want,
                "parse_tree_arg({input:?}) should be {want:?}"
            );
        }
    }

    /// Near-miss guard: the two shapes every existing user types are
    /// byte-identical to the pre-round-89 behaviour. `assert_eq!` on the whole
    /// value, not a `contains` — a discriminator tested only on the side that
    /// fires is vacuous green, and this is the side that must never move.
    #[test]
    fn parse_tree_arg_common_path_is_unchanged() {
        assert_eq!(parse_tree_arg("/tree"), TreeArg::Depth(3));
        assert_eq!(parse_tree_arg("/tree 5"), TreeArg::Depth(5));
        assert_eq!(DEFAULT_TREE_DEPTH, 3, "six artifacts quote 3 back to users");
    }

    /// The guard proper, ported from `/map`'s
    /// `map_arg_hint_advertises_only_flags_the_parser_accepts` (Day 164).
    /// Asserted against the parser itself — the real judge — never against a
    /// second hand-typed list that could agree with the hint while both are
    /// wrong.
    #[test]
    fn tree_arg_hint_advertises_only_forms_the_parser_accepts() {
        let hint =
            crate::commands::command_arg_hint("tree").expect("/tree should have an arg hint");

        // Anti-vacuous: an empty hint would satisfy every assertion below by
        // having nothing to check, which is this defect wearing the opposite
        // sign. It must still advertise the one form that works.
        assert!(
            hint.contains("depth"),
            "hint {hint:?} should still advertise the depth argument"
        );

        for token in hint.split_whitespace() {
            let bare = token.trim_matches(|c| c == '[' || c == ']' || c == '<' || c == '>');

            // No flag may be advertised: `parse_tree_arg` has no flag branch at
            // all, so every `--x` the hint names is rejected on contact.
            assert!(
                !bare.starts_with('-'),
                "hint {hint:?} advertises flag {bare:?}, which parse_tree_arg rejects \
                 (it accepts only a bare integer). Either teach the parser the flag \
                 or drop it from the hint."
            );

            // A placeholder the parser cannot read is the same defect wearing a
            // different token. `depth` stands for an integer, which parses.
            if !bare.is_empty() && bare != "depth" {
                assert!(
                    parse_tree_arg(&format!("/tree {bare}")) != TreeArg::Usage,
                    "hint {hint:?} advertises {bare:?}, which parse_tree_arg refuses"
                );
            }
        }
    }

    /// The hint and the detailed help are two hand-written artifacts with no
    /// compiler between them; before round 89 they disagreed for the whole life
    /// of the file. Pin them to each other so the next drift fails a test.
    #[test]
    fn tree_arg_hint_agrees_with_the_detailed_help() {
        let hint =
            crate::commands::command_arg_hint("tree").expect("/tree should have an arg hint");
        let help = crate::help_data::command_help("tree").expect("/tree should have detailed help");

        assert!(
            help.contains("/tree [depth]"),
            "detailed help should document the bare-integer form"
        );
        assert!(
            !help.contains("--depth"),
            "detailed help should not advertise a flag the parser rejects"
        );
        assert_eq!(
            hint, "[depth]",
            "the arg hint must state the same grammar the detailed help documents"
        );
    }
}
