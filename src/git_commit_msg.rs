//! Deterministic commit-message generation for `/commit`.
//!
//! Split out of `src/git.rs` on Day 174 as a **pure move** while fixing #811: the
//! parent module was sitting at 1997 lines against the fatal 2000-line gate, and
//! the fix plus its tests is ~260 lines. Nothing here was rewritten by the move —
//! `git::generate_commit_message` re-exports this module's entry point, so every
//! call site is unchanged.

/// What kind of file a changed path is, for commit-type classification.
///
/// `Code` is the production/default bucket — anything that is not recognisably a
/// test, a doc, or repo housekeeping. It deliberately wins ties (see
/// `dominant_commit_category`): a commit that touches production code and a
/// fixture in equal measure is not a test commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitFileCategory {
    Test,
    Docs,
    Chore,
    Code,
}

/// Is this path a test file *by shape*, not by substring?
///
/// The old rule was `path.contains("test")`, which fires on `src/commands_latest.rs`,
/// `contest.rs` and `src/protest_handler.rs` — and, with `any()`, let one fixture
/// re-type an entire production commit (#811). Recognised shapes, kept
/// language-agnostic because every `/commit` user gets this string in their history:
///
/// - any path component equal to `tests`, `test`, `spec`, `__tests__`
///   (Rust `tests/`, Python `test/`, Ruby/Elixir `spec/`, JS `__tests__/`)
/// - a file stem ending in `_test` / `_tests` (Go, Python, Rust siblings)
/// - a filename containing `.test.` / `.spec.` (JS/TS convention)
fn path_is_test(path: &str) -> bool {
    let mut components = path.split('/');
    let file_name = path.rsplit('/').next().unwrap_or(path);

    if components.any(|c| matches!(c, "tests" | "test" | "spec" | "__tests__")) {
        return true;
    }
    if file_name.contains(".test.") || file_name.contains(".spec.") {
        return true;
    }
    // Stem = everything before the final dot (or the whole name when there is none).
    let stem = match file_name.rsplit_once('.') {
        Some((stem, _ext)) => stem,
        None => file_name,
    };
    stem.ends_with("_test") || stem.ends_with("_tests")
}

/// Classify one changed path. Pure — the I/O free half of the commit-type decision.
fn commit_file_category(path: &str) -> CommitFileCategory {
    if path_is_test(path) {
        CommitFileCategory::Test
    } else if path.ends_with(".md") || path.starts_with("docs/") {
        CommitFileCategory::Docs
    } else if path.starts_with(".github/") || path.starts_with("scripts/") || path == "Cargo.toml" {
        CommitFileCategory::Chore
    } else {
        CommitFileCategory::Code
    }
}

/// Pick the category carrying the most *weight*, not the one that happens to appear first.
///
/// The old ladder was an `if/else if` chain, so the commit type was decided by the
/// mere presence of one file: +281 production lines and +2 fixture lines typed
/// identically to a pure test commit (#811). `weights` is indexed by category.
///
/// Ties: production code wins outright (a mixed commit is neither a test commit nor
/// a chore). Among the remaining three, ties fall back to the legacy ladder order
/// (test, docs, chore) so nothing that used to be decided one way flips for free.
fn dominant_commit_category(weights: &[(CommitFileCategory, usize)]) -> CommitFileCategory {
    let weight_of = |cat: CommitFileCategory| -> usize {
        weights
            .iter()
            .filter(|(c, _)| *c == cat)
            .map(|(_, w)| *w)
            .sum()
    };
    let ranked = [
        CommitFileCategory::Code,
        CommitFileCategory::Test,
        CommitFileCategory::Docs,
        CommitFileCategory::Chore,
    ];
    let mut best = CommitFileCategory::Code;
    let mut best_weight = 0usize;
    for cat in ranked {
        let w = weight_of(cat);
        if w > best_weight {
            best = cat;
            best_weight = w;
        }
    }
    best
}

/// The bare name of a path with its extension(s) stripped — `src/prompt.rs` → `prompt`.
///
/// Cuts at the *first* dot, preserving the scope wording this function has always
/// produced (`foo.test.js` → `foo`). Used for both the scope and the large-commit
/// summary so the two cannot drift.
fn file_stem_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name).to_string()
}

/// Generate a conventional commit message from a diff using simple heuristics.
/// This is a local, token-free approach — no AI calls needed.
pub fn generate_commit_message(diff: &str) -> String {
    let mut files_changed: Vec<String> = Vec::new();
    // Per-file changed-line counts, parsed from the diff we are already walking —
    // no extra git invocation. Parallel to `files_changed` by index.
    let mut per_file_lines: Vec<usize> = Vec::new();
    let mut current: Option<usize> = None;
    let mut insertions = 0usize;
    let mut deletions = 0usize;

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            files_changed.push(path.to_string());
            per_file_lines.push(0);
            current = Some(files_changed.len() - 1);
        } else if line.starts_with("+++") {
            // e.g. `+++ /dev/null` (whole-file delete): no path to attribute to.
            current = None;
        } else if line.starts_with('+') {
            insertions += 1;
            if let Some(i) = current {
                per_file_lines[i] += 1;
            }
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
            if let Some(i) = current {
                per_file_lines[i] += 1;
            }
        }
    }

    // Weight each changed file by the lines it actually changed; a file with no
    // counted lines (pure rename, mode change) still counts as one.
    let weights: Vec<(CommitFileCategory, usize)> = files_changed
        .iter()
        .enumerate()
        .map(|(i, f)| {
            (
                commit_file_category(f),
                per_file_lines.get(i).copied().unwrap_or(0).max(1),
            )
        })
        .collect();

    // Determine type prefix from where the weight of the change actually is.
    let prefix = match dominant_commit_category(&weights) {
        CommitFileCategory::Test => "test",
        CommitFileCategory::Docs => "docs",
        CommitFileCategory::Chore => "chore",
        CommitFileCategory::Code => {
            if deletions > insertions * 2 {
                "refactor"
            } else {
                "feat"
            }
        }
    };

    // Build a concise scope from changed files
    let scope = if files_changed.len() == 1 {
        file_stem_of(&files_changed[0])
    } else if files_changed.len() <= 3 {
        files_changed
            .iter()
            .map(|f| file_stem_of(f))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        format!("{} files", files_changed.len())
    };

    let verb = if deletions == 0 && insertions > 0 {
        "add"
    } else if insertions == 0 && deletions > 0 {
        "remove"
    } else {
        "update"
    };

    // The old summary said the least exactly when the commit was biggest: once the
    // scope collapsed to "N files", every mixed diff read `update code` (#811).
    // When the scope can no longer name the files, spend the summary on a fact the
    // diff already knows — the file carrying the most changed lines — rather than
    // inventing an intent this function cannot know.
    let summary = if files_changed.len() > 3 {
        let focus = per_file_lines
            .iter()
            .enumerate()
            .max_by_key(|(i, lines)| (**lines, std::cmp::Reverse(*i)))
            .map(|(i, _)| file_stem_of(&files_changed[i]))
            .unwrap_or_else(|| "code".to_string());
        format!("{verb} {focus} (+{} more)", files_changed.len() - 1)
    } else {
        match verb {
            "add" => "add changes".to_string(),
            "remove" => "remove code".to_string(),
            _ => "update code".to_string(),
        }
    };

    format!("{prefix}({scope}): {summary}")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // This test used to stop here — it pinned the scope collapse and nothing else,
        // so it was green while the summary said "update code" for every big mixed
        // diff (#811). The scope is unchanged; the summary now names the file carrying
        // the most changed lines instead of saying nothing.
        assert!(
            msg.contains("(+3 more)"),
            "Large-commit summary should disclose the remaining files: {msg}"
        );
        assert!(
            !msg.contains("update code"),
            "Large-commit summary should not be the bare placeholder: {msg}"
        );
    }

    /// The exact shape that produced #811: one test-shaped path plus five
    /// production files, with the weight overwhelmingly on production.
    /// Real commit `c46d8453` came out as `test(6 files): update code`.
    #[test]
    fn test_generate_commit_message_mixed_fixture_is_not_a_test_commit() {
        let mut diff = String::new();
        for (path, lines) in [
            ("src/prompt.rs", 171),
            ("src/repl.rs", 110),
            ("src/main.rs", 12),
            ("src/session.rs", 20),
            ("src/cli.rs", 6),
            ("src/main_tests.rs", 2),
        ] {
            diff.push_str(&format!("diff --git a/{path} b/{path}\n"));
            diff.push_str(&format!("--- a/{path}\n"));
            diff.push_str(&format!("+++ b/{path}\n"));
            diff.push_str("@@ -1 +1,2 @@\n");
            for i in 0..lines {
                diff.push_str(&format!("+// line {i}\n"));
            }
            diff.push_str("-// removed\n");
        }
        let msg = generate_commit_message(&diff);
        assert!(
            !msg.starts_with("test("),
            "One fixture among five production files must not re-type the commit: {msg}"
        );
        assert!(
            !msg.contains("update code"),
            "The biggest commits must not get the emptiest summary: {msg}"
        );
        assert!(
            msg.contains("prompt"),
            "Summary should name the file carrying the most changed lines: {msg}"
        );
        assert_eq!(msg, "feat(6 files): update prompt (+5 more)");
    }

    #[test]
    fn test_generate_commit_message_substring_is_not_a_test_path() {
        // `commands_latest.rs` contains the letters "test" but is production code.
        let diff = "\
diff --git a/src/commands_latest.rs b/src/commands_latest.rs
--- a/src/commands_latest.rs
+++ b/src/commands_latest.rs
@@ -1 +1,2 @@
+// change
";
        let msg = generate_commit_message(diff);
        assert!(
            !msg.starts_with("test("),
            "A path merely containing 'test' is not a test file: {msg}"
        );
    }

    #[test]
    fn test_generate_commit_message_real_test_path_still_types_as_test() {
        // Near-miss guard: the fix must not regress the case the old rule got right.
        let diff = "\
diff --git a/tests/module_size.rs b/tests/module_size.rs
--- a/tests/module_size.rs
+++ b/tests/module_size.rs
@@ -1 +1,2 @@
+// change
";
        let msg = generate_commit_message(diff);
        assert!(
            msg.starts_with("test("),
            "A real test path should still type as test: {msg}"
        );
    }

    #[test]
    fn test_path_is_test_recognises_shapes_across_languages() {
        for yes in [
            "tests/module_size.rs",
            "test/foo.py",
            "spec/models/user_spec.rb",
            "app/__tests__/button.js",
            "src/main_tests.rs",
            "pkg/handler_test.go",
            "src/Button.test.tsx",
            "src/Button.spec.ts",
        ] {
            assert!(path_is_test(yes), "should be a test path: {yes}");
        }
        for no in [
            "src/commands_latest.rs",
            "contest.rs",
            "src/protest_handler.rs",
            "src/git.rs",
            "docs/testing.md",
            ".github/workflows/test.yml",
        ] {
            assert!(!path_is_test(no), "should NOT be a test path: {no}");
        }
    }

    #[test]
    fn test_dominant_commit_category_ties_go_to_production() {
        use CommitFileCategory::*;
        assert_eq!(dominant_commit_category(&[(Code, 5), (Test, 5)]), Code);
        assert_eq!(dominant_commit_category(&[(Docs, 3), (Code, 3)]), Code);
        // A genuine majority still wins.
        assert_eq!(dominant_commit_category(&[(Code, 2), (Test, 9)]), Test);
        assert_eq!(dominant_commit_category(&[(Docs, 40), (Code, 1)]), Docs);
        // Empty diff: default to production rather than inventing a type.
        assert_eq!(dominant_commit_category(&[]), Code);
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
}
