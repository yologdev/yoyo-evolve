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

    // ---------------------------------------------------------------------
    // Mutation-repair block (Day 178, reading #8).
    //
    // Reading #1 (Day 177) measured this module at 16 surviving mutants, 14 of
    // them inside `generate_commit_message` and 2 in `commit_file_category`.
    // Every test above stops at `assert!(msg.contains(..))` or `starts_with(..)`,
    // which is a green light over the fragment it does not inspect — the exact
    // shape that let #811 ship (`msg.contains("4 files")` stayed green over a
    // message asserting the opposite of the diff).
    //
    // These assert the **whole string a caller receives**, and drive every
    // comparison guard on **both sides** of its boundary: a discriminator
    // exercised only on the side that fires is vacuous green.
    // ---------------------------------------------------------------------

    /// Build a unified-diff fixture: one hunk per file, `ins` added lines then
    /// `del` removed lines, wrapped in the real headers `/commit` actually sees.
    ///
    /// The header shapes are load-bearing, not decoration: the `--- a/<path>`
    /// line is precisely what the deletion guard has to exclude, and the
    /// `diff --git` / `@@` / context lines are what an over-permissive guard
    /// would start miscounting as deletions.
    fn diff_of(files: &[(&str, usize, usize)]) -> String {
        let mut out = String::new();
        for (path, ins, del) in files {
            out.push_str(&format!("diff --git a/{path} b/{path}\n"));
            out.push_str(&format!("--- a/{path}\n"));
            out.push_str(&format!("+++ b/{path}\n"));
            out.push_str("@@ -1,2 +1,3 @@\n");
            out.push_str(" context line\n");
            for i in 0..*ins {
                out.push_str(&format!("+// added {i}\n"));
            }
            for i in 0..*del {
                out.push_str(&format!("-// removed {i}\n"));
            }
        }
        out
    }

    /// Verb selection, add side — and the `"add" => "add changes"` match arm.
    ///
    /// Kills: deleting the `"add"` arm (falls through to `update code`), and the
    /// `deletions == 0` / `insertions > 0` swaps that decide it.
    #[test]
    fn mutation_guard_add_only_diff_is_exact_at_the_emission_point() {
        let msg = generate_commit_message(&diff_of(&[("src/thing.rs", 2, 0)]));
        assert_eq!(msg, "feat(thing): add changes");
    }

    /// Verb selection, remove side.
    ///
    /// Kills the `insertions == 0` / `deletions > 0` swaps. The existing
    /// `test_generate_commit_message_deletions_only` asserts only
    /// `contains("remove code")`, so it says nothing about the prefix or scope.
    #[test]
    fn mutation_guard_delete_only_diff_is_exact_at_the_emission_point() {
        let msg = generate_commit_message(&diff_of(&[("src/thing.rs", 0, 3)]));
        assert_eq!(msg, "refactor(thing): remove code");
    }

    /// The abstention case: a file changed with **no counted lines** (rename,
    /// mode change) is neither an add nor a remove.
    ///
    /// This is the near-miss both verb guards need. It is the only input that
    /// separates `insertions > 0` from `insertions >= 0` and `deletions > 0`
    /// from `deletions >= 0`; every other fixture agrees under those mutations.
    #[test]
    fn mutation_guard_zero_counted_lines_is_update_not_add_or_remove() {
        let msg = generate_commit_message(&diff_of(&[("src/thing.rs", 0, 0)]));
        assert_eq!(msg, "feat(thing): update code");
    }

    /// The refactor/feat split `deletions > insertions * 2`, driven on **both**
    /// sides of the boundary and at it.
    ///
    /// Round 73's one exact hit was this expression. The three rows are chosen
    /// so no single substituted operator can satisfy all of them:
    ///
    /// - 10/15 : `d > i*2` false, but `d > i+2` and `d > i/2` are true, and
    ///   `d < i*2` is true — kills `*`→`+`, `*`→`/`, `>`→`<`.
    /// - 10/20 : exactly at the boundary — kills `>`→`>=`.
    /// - 10/25 : strictly past it — kills `>`→`==`.
    #[test]
    fn mutation_guard_refactor_boundary_is_strictly_greater_than_double() {
        // Below the line: more deletions than insertions, but not double.
        assert_eq!(
            generate_commit_message(&diff_of(&[("src/thing.rs", 10, 15)])),
            "feat(thing): update code"
        );
        // Exactly double: still not a refactor. `>` is not `>=`.
        assert_eq!(
            generate_commit_message(&diff_of(&[("src/thing.rs", 10, 20)])),
            "feat(thing): update code"
        );
        // Past double: refactor.
        assert_eq!(
            generate_commit_message(&diff_of(&[("src/thing.rs", 10, 25)])),
            "refactor(thing): update code"
        );
    }

    /// The deletion guard `starts_with('-') && !starts_with("---")`.
    ///
    /// Both mutations are silent on a fixture that only asks "is this a deletion
    /// commit?" — they change the deletion *count*, so they are only observable
    /// where the count crosses a threshold. Both rows sit on the refactor line:
    ///
    /// - `&&`→`||` also counts `diff --git`, `--- a/`, `@@` and context lines,
    ///   pushing 20 deletions past the boundary into `refactor`.
    /// - `delete !` counts *only* the `--- a/` header, collapsing 25 deletions
    ///   to 1 and dropping `refactor` back to `feat`.
    #[test]
    fn mutation_guard_deletion_line_guard_counts_exactly_the_minus_lines() {
        // If the guard over-counts, 20 deletions become >20 and this flips.
        assert_eq!(
            generate_commit_message(&diff_of(&[("src/thing.rs", 10, 20)])),
            "feat(thing): update code"
        );
        // If the guard under-counts, 25 deletions become 1 and this flips.
        assert_eq!(
            generate_commit_message(&diff_of(&[("src/thing.rs", 10, 25)])),
            "refactor(thing): update code"
        );
    }

    /// Per-file **insertion** accumulation picks the focus file.
    ///
    /// Line counts are distinct and non-commutative under the substituted
    /// operator: a counter that starts at 0 stays 0 under `*=`, so every file
    /// ties at zero and the focus falls back to the first file (`a`) instead of
    /// the biggest (`d`). A fixture where the biggest file is also the first
    /// cannot see this — which is why the #811 fixture above did not.
    #[test]
    fn mutation_guard_insertion_counts_choose_the_focus_file() {
        let msg = generate_commit_message(&diff_of(&[
            ("src/a.rs", 1, 0),
            ("src/b.rs", 2, 0),
            ("src/c.rs", 3, 0),
            ("src/d.rs", 10, 0),
        ]));
        assert_eq!(msg, "feat(4 files): add d (+3 more)");
    }

    /// Per-file **deletion** accumulation picks the focus file.
    ///
    /// Mirror of the test above, one branch over: `d` earns the focus purely
    /// through deleted lines, so a broken deletion counter hands it to `a`.
    #[test]
    fn mutation_guard_deletion_counts_choose_the_focus_file() {
        let msg = generate_commit_message(&diff_of(&[
            ("src/a.rs", 5, 0),
            ("src/b.rs", 1, 0),
            ("src/c.rs", 1, 0),
            ("src/d.rs", 0, 20),
        ]));
        assert_eq!(msg, "refactor(4 files): update d (+3 more)");
    }

    /// The large-commit threshold `files_changed.len() > 3`, at the boundary.
    ///
    /// Three files must still name themselves and take the plain summary; the
    /// four-file case above is the other side. Together they kill `>`→`>=`,
    /// `>`→`<` and `>`→`==`.
    #[test]
    fn mutation_guard_three_files_stay_under_the_scope_threshold() {
        let msg = generate_commit_message(&diff_of(&[
            ("src/a.rs", 1, 0),
            ("src/b.rs", 2, 0),
            ("src/c.rs", 3, 0),
        ]));
        assert_eq!(msg, "feat(a, b, c): add changes");
    }

    /// The Chore chain's two `||`, each reached by a path only it admits.
    ///
    /// `||`→`&&` on either operator drops one of these three back to `feat`,
    /// and no single row catches both: the first `||` is pinned by the
    /// `.github/` and `scripts/` rows, the second by the `scripts/` and
    /// `Cargo.toml` rows. The last row is the near-miss — ordinary source must
    /// not become a chore.
    #[test]
    fn mutation_guard_every_chore_path_reaches_the_chore_arm() {
        for (path, expected) in [
            (".github/workflows/ci.yml", "chore(ci): add changes"),
            ("scripts/evolve.sh", "chore(evolve): add changes"),
            ("Cargo.toml", "chore(Cargo): add changes"),
            // Near-miss: a discriminator tested only where it fires is vacuous.
            ("src/main.rs", "feat(main): add changes"),
        ] {
            let msg = generate_commit_message(&diff_of(&[(path, 2, 0)]));
            assert_eq!(msg, expected, "wrong classification for {path}");
        }
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
