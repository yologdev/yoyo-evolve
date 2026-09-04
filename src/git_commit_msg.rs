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

/// Split a git-quoted token off the front of `s`, returning `(token_including_quotes,
/// rest)`.
///
/// The closing quote is the first unescaped `"`, so a `\"` inside the path does not
/// terminate it. Returns `None` when `s` does not start with a quote or the token is
/// never closed. Every index handed to a slice comes from `char_indices`, so this can
/// never split inside a multi-byte character.
fn split_first_quoted(s: &str) -> Option<(&str, &str)> {
    if !s.starts_with('"') {
        return None;
    }
    let mut escaped = false;
    for (i, c) in s.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => {
                let end = i + c.len_utf8();
                return Some((&s[..end], &s[end..]));
            }
            _ => {}
        }
    }
    None
}

/// Decode a git-quoted path token (quotes included) into the real path, or `None`.
///
/// Git renders a path in double quotes with C-style escapes whenever it contains a
/// non-ASCII byte, a `"`, a `\` or a control character. The escape set it emits and
/// this decodes: three-digit **octal byte** escapes (`\303\244`), `\"`, `\\`, `\t`,
/// `\n`, `\r`. Octal escapes are *bytes*, not characters, so they are accumulated into
/// a byte buffer and the whole buffer is interpreted as UTF-8 exactly once at the end.
///
/// **It refuses rather than guesses**, and that is the whole difference from its
/// sibling `commands_risk::unquote_git_path` — do not "de-duplicate" them without
/// reading this paragraph. That one is deliberately *lossy* (`from_utf8_lossy`, unknown
/// escapes kept as literals) because it collects churn paths for a risk score, where a
/// slightly-wrong path costs a slightly-wrong number. This string goes into a commit
/// message and then into git history forever, so a byte sequence that is not valid
/// UTF-8, an unknown escape, a short octal run or a lone trailing backslash all return
/// `None` — the file is dropped, which is wrong but honest, rather than a path being
/// invented that is not in the diff (round 81's refusal branch, same reasoning).
fn unquote_diff_path(quoted: &str) -> Option<String> {
    let inner = quoted.strip_prefix('"')?.strip_suffix('"')?;
    let mut bytes: Vec<u8> = Vec::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        // A lone trailing backslash has no escape to read: refuse.
        match chars.next()? {
            '"' => bytes.push(b'"'),
            '\\' => bytes.push(b'\\'),
            't' => bytes.push(b'\t'),
            'n' => bytes.push(b'\n'),
            'r' => bytes.push(b'\r'),
            d @ '0'..='7' => {
                // Exactly three octal digits, which is what git emits. A shorter run
                // is a shape we have not seen, so refuse rather than mis-decode.
                let mut val = d.to_digit(8)?;
                for _ in 0..2 {
                    val = val * 8 + chars.next()?.to_digit(8)?;
                }
                bytes.push(u8::try_from(val).ok()?);
            }
            // An escape git is not known to emit: refuse rather than invent a byte.
            _ => return None,
        }
    }
    String::from_utf8(bytes).ok()
}

/// The path named by a *quoted* `--- "a/<path>"` / `+++ "b/<path>"` hunk line.
///
/// The unquoted forms are read by plain `strip_prefix` at the call site and are
/// untouched by this. Git appends a tab separator after a quoted path that contains a
/// space, so one trailing tab is tolerated and dropped — it is a separator, not part of
/// the path.
fn quoted_hunk_path(line: &str, marker: &str, side: &str) -> Option<String> {
    let rest = line.strip_prefix(marker)?;
    let (token, trailing) = split_first_quoted(rest)?;
    if !trailing.is_empty() && trailing != "\t" {
        return None;
    }
    let path = unquote_diff_path(token)?;
    let path = path.strip_prefix(side)?;
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// The *new* path named by a `diff --git a/<old> b/<new>` header line, if it can be
/// read unambiguously.
///
/// This exists because three real diff shapes — a content-identical rename, a binary
/// file change, and a mode-only change — emit **no** `---`/`+++` lines at all, so this
/// header is the only line in the whole hunk that names the file (round 81's census).
/// Without it those files vanish from `files_changed` entirely and the scope renders as
/// the literal empty parens `()`, the same visible symptom as commit `26defce9`.
///
/// Two forms are read. Git *quotes both* paths, or neither: when the path carries a
/// non-ASCII byte, a `"`, or a control character it renders
/// `diff --git "a/n\303\244me\"q.txt" "b/..."` (#829). The quoted form is parsed first
/// and is the unambiguous one — the closing quote terminates the first path, so the
/// separator problem below cannot arise there.
///
/// Deliberately narrow, and the remaining gap is named rather than implied. In the
/// *unquoted* form a path containing a literal `" b/"` is genuinely ambiguous, so this
/// returns `None` whenever the separator does not occur exactly once — degrading to the
/// pre-round-81 behaviour (the file is dropped) instead of inventing a path that is not
/// in the diff. **#830 is still open**: nothing here resolves that ambiguity, it only
/// refuses it. A path with a plain space is unquoted by git and *is* handled, because
/// the separator still occurs exactly once.
fn diff_header_path(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("diff --git ") {
        if rest.starts_with('"') {
            // Quoted form: `"a/<old>" "b/<new>"`. The closing quote of the first token
            // is the separator, so there is nothing to guess.
            let (_old, tail) = split_first_quoted(rest)?;
            let (new_token, trailing) = split_first_quoted(tail.strip_prefix(' ')?)?;
            if !trailing.is_empty() {
                return None;
            }
            let new_path = unquote_diff_path(new_token)?;
            let new_path = new_path.strip_prefix("b/")?;
            if new_path.is_empty() {
                return None;
            }
            return Some(new_path.to_string());
        }
    }
    let rest = line.strip_prefix("diff --git a/")?;
    // Candidate separators. Every offset is the start of an ASCII match, so every
    // index derived from one lands on a char boundary (rule #250 — never a raw byte
    // index into a `&str`).
    let candidates: Vec<usize> = rest.match_indices(" b/").map(|(at, _)| at).collect();
    let at = match candidates.len() {
        0 => return None,
        // Exactly one candidate: nothing to disambiguate, and this stays byte-identical
        // to the pre-#830 behaviour. That is every ordinary header — *including a
        // rename*, whose two paths legitimately differ, which is why the equal-paths
        // rule below must never reach this arm.
        1 => candidates[0],
        _ => {
            // #830: the path itself contains a literal " b/", so several offsets split
            // the header and the old code refused outright — dropping the file from
            // `files_changed`, which then wrecked the scope, the category weights, the
            // >3-file threshold and the summary focus (the `26defce9` class, one shape
            // over).
            //
            // The split point is *checkable* rather than a guess: for a non-rename
            // header git emits the same path on both sides, so keep only the candidates
            // whose left and right halves are byte-equal. Exactly one match resolves the
            // header; zero (an ambiguous *rename*, whose paths genuinely differ) or
            // several leave it refused exactly as before.
            //
            // This is a pure NARROWING of the refusal: the only inputs whose behaviour
            // moves are ones that already returned `None`. Do not "simplify" this into
            // taking the first or last candidate — a confidently wrong path is worse
            // than a dropped one, which is the whole reason the refusal exists (round
            // 81), and it would invent a path that is not in the diff.
            let mut equal = candidates
                .iter()
                .copied()
                .filter(|&at| rest[..at] == rest[at + " b/".len()..]);
            let only = equal.next()?;
            if equal.next().is_some() {
                return None;
            }
            only
        }
    };
    let new_path = &rest[at + " b/".len()..];
    if new_path.is_empty() {
        return None;
    }
    Some(new_path.to_string())
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

    // The path from the most recent `--- a/<path>` line. git renders a whole-file
    // delete as `--- a/<path>` / `+++ /dev/null`, so the *only* place the deleted
    // path appears is the `---` line — the `+++` side names no file. Holding it here
    // is what lets a deletion reach `files_changed` at all (it used to be dropped
    // entirely, so a lone deletion rendered the literal `()` as its scope and could
    // never influence the category, the >3-file threshold, or the summary focus).
    let mut pending_old_path: Option<String> = None;

    // The path from the most recent `diff --git a/<old> b/<new>` header, held until we
    // learn whether the hunk names the file some other way. A pure rename, a binary
    // change and a mode-only change emit no `---`/`+++` at all, so this is their only
    // mention; it is flushed at the next header and at end of diff (round 81).
    let mut pending_header_path: Option<String> = None;

    for line in diff.lines() {
        if let Some(path) = diff_header_path(line) {
            // The previous file was never named by a `---`/`+++` line, so this header
            // was its only mention: record it with zero counted lines, taking the same
            // weight-1 fallback a pure rename already took once recorded.
            if let Some(prev) = pending_header_path.take() {
                files_changed.push(prev);
                per_file_lines.push(0);
                current = None;
            }
            pending_header_path = Some(path);
        } else if let Some(path) = line
            .strip_prefix("+++ b/")
            .map(str::to_string)
            .or_else(|| quoted_hunk_path(line, "+++ ", "b/"))
        {
            // The hunk names the file itself, so the header mention is redundant.
            // A quoted path (`+++ "b/n\303\244me.txt"`) does not match the plain
            // prefix, so it is read by `quoted_hunk_path` — without which a *modify*
            // of a non-ASCII path falls into the `/dev/null` arm below, which clears
            // the pending header and drops the file (#829).
            pending_header_path = None;
            pending_old_path = None;
            files_changed.push(path);
            per_file_lines.push(0);
            current = Some(files_changed.len() - 1);
        } else if line.starts_with("+++") {
            // e.g. `+++ /dev/null` (whole-file delete): the `+++` side names no
            // file, so attribute this hunk to the path the `---` line carried.
            // Its `-` lines follow this line, so they land on the right index.
            pending_header_path = None;
            match pending_old_path.take() {
                Some(path) => {
                    files_changed.push(path);
                    per_file_lines.push(0);
                    current = Some(files_changed.len() - 1);
                }
                // No `---` path to fall back on: nothing to attribute to.
                None => current = None,
            }
        } else if let Some(path) = line
            .strip_prefix("--- a/")
            .map(str::to_string)
            .or_else(|| quoted_hunk_path(line, "--- ", "a/"))
        {
            // An *added* file renders `--- /dev/null`, which matches neither form, so
            // `pending_old_path` stays `None` and the add path is byte-identical to
            // before. The quoted form is what lets a *deleted* non-ASCII path still
            // reach the `+++ /dev/null` arm with a name to attribute to (#829).
            pending_old_path = Some(path);
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
    // Same flush for the last file in the diff, which has no following header.
    if let Some(path) = pending_header_path.take() {
        files_changed.push(path);
        per_file_lines.push(0);
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

    // ---- whole-file deletions (Day 178) ----
    //
    // git renders a delete as `--- a/<path>` / `+++ /dev/null`. The walker used to
    // record nothing for that shape, so the deleted file was absent from
    // `files_changed` and all four downstream decisions were computed over a list
    // missing it. Every fixture below is a verbatim `git diff` chunk including its
    // `---`/`+++` pair, and every assertion is on the string a caller receives.

    /// The reproduction: `26defce9` in this repo's own history reads
    /// `refactor(): remove code` — empty parens where the scope belongs.
    #[test]
    fn test_whole_file_deletion_names_the_file_in_the_scope() {
        let diff = "\
diff --git a/mutants.toml b/mutants.toml
deleted file mode 100644
--- a/mutants.toml
+++ /dev/null
@@ -1,3 +0,0 @@
-[[exclude]]
-function = \"main::run\"
-# gone
";
        let msg = generate_commit_message(diff);
        // `mutants.toml` is `Code` (not `.github/`/`scripts/`/`Cargo.toml`), and a
        // pure deletion trips the `deletions > insertions * 2` refactor split — so
        // `refactor` is the same prefix the real commit got. Only the scope was
        // ever wrong, which is precisely the half the dropped path destroyed.
        assert_eq!(
            msg, "refactor(mutants): remove code",
            "a lone deletion must name the deleted file, not render `()`: {msg}"
        );
        assert!(
            !msg.contains("()"),
            "empty scope is the visible symptom of the dropped path: {msg}"
        );
    }

    /// Near-miss guard: an *added* file renders `--- /dev/null` / `+++ b/path`,
    /// which already worked. Byte-identical to the pre-fix output, captured by
    /// running the old body.
    #[test]
    fn test_added_file_message_is_byte_identical_to_before() {
        let diff = "\
diff --git a/new.rs b/new.rs
new file mode 100644
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,2 @@
+fn a() {}
+fn b() {}
";
        assert_eq!(generate_commit_message(diff), "feat(new): add changes");
    }

    /// Near-miss guard: an ordinary modification is untouched by the fix.
    #[test]
    fn test_modified_file_message_is_byte_identical_to_before() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,1 +1,1 @@
+let x = 1;
-let y = 2;
";
        assert_eq!(generate_commit_message(diff), "feat(main): update code");
    }

    /// The half with no visible symptom: a deleted file never reached `weights`,
    /// so a commit deleting only tests could not type as `test`.
    #[test]
    fn test_deleting_only_a_test_file_types_as_test() {
        let diff = "\
diff --git a/tests/foo_test.rs b/tests/foo_test.rs
deleted file mode 100644
--- a/tests/foo_test.rs
+++ /dev/null
@@ -1,4 +0,0 @@
-#[test]
-fn t() {
-    assert!(true);
-}
";
        let msg = generate_commit_message(diff);
        assert!(
            msg.starts_with("test("),
            "deleting only a test file must type as `test`: {msg}"
        );
        assert_eq!(msg, "test(foo_test): remove code");
    }

    /// The threshold: five deletions must take the `> 3 files` path.
    #[test]
    fn test_five_deleted_files_reach_the_large_commit_threshold() {
        let mut diff = String::new();
        for n in 1..=5 {
            diff.push_str(&format!(
                "diff --git a/src/gone{n}.rs b/src/gone{n}.rs\n\
deleted file mode 100644\n\
--- a/src/gone{n}.rs\n\
+++ /dev/null\n\
@@ -1,2 +0,0 @@\n\
-fn gone{n}() {{}}\n\
-// tail\n"
            ));
        }
        let msg = generate_commit_message(&diff);
        assert!(
            msg.contains("5 files"),
            "five deletions must be counted as five: {msg}"
        );
        assert_eq!(msg, "refactor(5 files): remove gone1 (+4 more)");
    }

    /// Mixed: one added file and one deleted file both appear in the scope.
    #[test]
    fn test_mixed_add_and_delete_lists_both_files_in_the_scope() {
        let diff = "\
diff --git a/src/added.rs b/src/added.rs
new file mode 100644
--- /dev/null
+++ b/src/added.rs
@@ -0,0 +1,1 @@
+fn added() {}
diff --git a/src/removed.rs b/src/removed.rs
deleted file mode 100644
--- a/src/removed.rs
+++ /dev/null
@@ -1,1 +0,0 @@
-fn removed() {}
";
        let msg = generate_commit_message(diff);
        assert!(
            msg.contains("added") && msg.contains("removed"),
            "both files must appear in the scope: {msg}"
        );
        assert_eq!(msg, "feat(added, removed): update code");
    }

    // ---- Round 81: the fixture-shape census ----
    //
    // These four shapes were constructed by NO fixture before this round. The string
    // `dev/null` occurred exactly once in this whole file (a comment inside the branch
    // that drops the path) and ZERO times in this test module, so the Day-178 scoped
    // mutation read that reported 0 survivors was true and answered a narrower question
    // than it looked like: a mutation score is bounded by what the fixtures can ASK.
    // Each fixture below is verbatim `git show` output from a real repo, not hand-typed.

    /// A content-identical rename. git emits NO `---`/`+++` lines at all — the
    /// `diff --git` header is the only line naming the file.
    #[test]
    fn census_pure_rename_names_the_file() {
        let diff = "\
diff --git a/old.txt b/new.txt
similarity index 100%
rename from old.txt
rename to new.txt
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(new): update code");
    }

    /// A binary file change. git emits NO `---`/`+++` lines either — only the
    /// `Binary files ... differ` marker, which starts with neither `+` nor `-`,
    /// so the file carries zero counted lines and falls to the weight-1 fallback.
    #[test]
    fn census_binary_change_names_the_file() {
        let diff = "\
diff --git a/img.png b/img.png
index 93759e4..67ff4b0 100644
Binary files a/img.png and b/img.png differ
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(img): update code");
    }

    /// A mode-only change (chmod +x). Again no `---`/`+++`, no hunks.
    #[test]
    fn census_mode_only_change_names_the_file() {
        let diff = "\
diff --git a/keep.rs b/keep.rs
old mode 100644
new mode 100755
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(keep): update code");
    }

    /// #830: a path containing a literal " b/" reaches the message instead of being
    /// dropped. A mode-only change is the shape that exercises it — the `diff --git`
    /// header is the file's ONLY mention (no `---`/`+++`), so the header's ambiguity
    /// used to delete the file from `files_changed` entirely, taking the scope, the
    /// category weights, the >3-file threshold and the summary focus with it.
    ///
    /// Asserted at the emission point (the `String` a caller receives), never on
    /// `diff_header_path` one layer below.
    #[test]
    fn a_path_containing_the_separator_reaches_the_message() {
        let diff = "\
diff --git a/src/a b/c.rs b/src/a b/c.rs
old mode 100644
new mode 100755
";
        // Anti-vacuous: the fixture must really be ambiguous, or this test could pass
        // by agreeing with itself on a header that was never in question.
        let header = diff.lines().next().unwrap();
        assert!(
            header.matches(" b/").count() > 1,
            "fixture is not ambiguous: {header}"
        );

        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(c): update code");
    }

    /// The residue, pinned on purpose: an ambiguous *rename* names two genuinely
    /// different paths, so no candidate split has byte-equal halves and the file is
    /// still dropped (empty scope). Inventing a path that is not in the diff is worse
    /// than dropping one — a later "improvement" has to face this assertion.
    #[test]
    fn an_ambiguous_rename_is_still_refused() {
        let diff = "\
diff --git a/has b/one.rs b/has b/two.rs
similarity index 100%
rename from has b/one.rs
rename to has b/two.rs
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(): update code");
    }

    /// Near-miss guard: an ordinary path is byte-identical to before. This is every
    /// commit anyone has ever made and the whole regression surface, so it is a
    /// whole-string `assert_eq!` rather than a `contains`.
    #[test]
    fn an_ordinary_header_is_byte_identical() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
old mode 100644
new mode 100755
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(main): update code");
    }

    /// Near-miss guard: a path with a plain SPACE but no " b/". git does not quote a
    /// space, so this takes the unquoted branch with exactly one candidate separator —
    /// which is precisely what makes it the case proving #830's change is a narrowing.
    #[test]
    fn a_spaced_path_without_the_separator_is_byte_identical() {
        let diff = "\
diff --git a/src/we ird.rs b/src/we ird.rs
old mode 100644
new mode 100755
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(we ird): update code");
    }

    /// Near-miss guards for the two `/dev/null` branches, which share the walker with
    /// the header path: an added file and a whole-file deletion are byte-identical.
    #[test]
    fn the_dev_null_branches_are_byte_identical() {
        let added = "\
diff --git a/new.rs b/new.rs
new file mode 100644
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,2 @@
+fn main() {}
+// hi
";
        assert_eq!(generate_commit_message(added), "feat(new): add changes");

        let deleted = "\
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
--- a/gone.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn main() {}
-// bye
";
        assert_eq!(
            generate_commit_message(deleted),
            "refactor(gone): remove code"
        );
    }

    /// A rename WITH a content change. This one git does render with `---`/`+++`,
    /// carrying the *new* path on the `+++` side — so it was already recorded
    /// correctly before this round. The fixture is a regression guard, and the
    /// near-miss that proves the header fallback does not double-count: exactly
    /// one file in the scope, and the `+more` line is counted.
    #[test]
    fn census_rename_with_content_change_records_the_new_path_once() {
        let diff = "\
diff --git a/new.txt b/renamed2.txt
similarity index 70%
rename from new.txt
rename to renamed2.txt
index 94954ab..8b14c4f 100644
--- a/new.txt
+++ b/renamed2.txt
@@ -1,2 +1,3 @@
 hello
 world
+more
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(renamed2): add changes");
    }

    // ---- #829: git-quoted `diff --git` headers ------------------------------------
    //
    // Every fixture below is verbatim `git show --format=` output captured from a
    // scratch repo, not hand-typed — round 81 lost a hypothesis by guessing at an
    // external tool's output format when one `git show` would have settled it.
    // Asserted at the emission point (the string a caller of `generate_commit_message`
    // receives), never on `diff_header_path` one layer below.

    /// A content-identical rename of a non-ASCII path: the quoted header is the *only*
    /// line naming the file, so before #829 this rendered the literal `feat(): update
    /// code` — round 81's symptom arriving from a fourth direction.
    #[test]
    fn quoted_header_rename_of_a_non_ascii_path_names_the_file() {
        let diff = "\
diff --git \"a/n\\303\\244me.txt\" \"b/n\\303\\240me2.txt\"
similarity index 100%
rename from \"n\\303\\244me.txt\"
rename to \"n\\303\\240me2.txt\"
";
        let msg = generate_commit_message(diff);
        assert_eq!(msg, "feat(nàme2): update code");
        assert!(!msg.contains("()"), "empty scope regressed: {msg}");
    }

    /// A `\"` inside the path decodes to a literal quote and does not terminate the
    /// quoted token early.
    #[test]
    fn quoted_header_decodes_an_escaped_quote_in_the_filename() {
        let diff = "\
diff --git \"a/has\\\"quote.txt\" \"b/has\\\"quote2.txt\"
similarity index 100%
rename from \"has\\\"quote.txt\"
rename to \"has\\\"quote2.txt\"
";
        assert_eq!(
            generate_commit_message(diff),
            "feat(has\"quote2): update code"
        );
    }

    /// An ordinary *modification* of a non-ASCII path. This is the shape the header
    /// fix alone does not save: git quotes the `+++` line too, which does not match
    /// the plain `+++ b/` prefix and so used to fall into the `/dev/null` arm, clear
    /// the pending header, and drop the file.
    #[test]
    fn quoted_hunk_lines_keep_a_modified_non_ascii_path() {
        let diff = "\
diff --git \"a/n\\303\\240me2.txt\" \"b/n\\303\\240me2.txt\"
index 94954ab..8b14c4f 100644
--- \"a/n\\303\\240me2.txt\"
+++ \"b/n\\303\\240me2.txt\"
@@ -1,2 +1,3 @@
 hello
 world
+more
";
        assert_eq!(generate_commit_message(diff), "feat(nàme2): add changes");
    }

    /// A whole-file *deletion* of a non-ASCII path: the quoted `---` line is the only
    /// mention of the name (the `+++` side is `/dev/null`), so the quoted form is what
    /// gives the `/dev/null` arm something to attribute to.
    #[test]
    fn quoted_hunk_lines_keep_a_deleted_non_ascii_path() {
        let diff = "\
diff --git \"a/n\\303\\240me2.txt\" \"b/n\\303\\240me2.txt\"
deleted file mode 100644
index 8b14c4f..0000000
--- \"a/n\\303\\240me2.txt\"
+++ /dev/null
@@ -1,3 +0,0 @@
-hello
-world
-more
";
        assert_eq!(
            generate_commit_message(diff),
            "refactor(nàme2): remove code"
        );
    }

    /// A binary change to a non-ASCII path — no `---`/`+++` lines at all.
    #[test]
    fn quoted_header_binary_change_names_the_file() {
        let diff = "\
diff --git \"a/b\\303\\257n.dat\" \"b/b\\303\\257n.dat\"
index 57ac8df..5355707 100644
Binary files \"a/b\\303\\257n.dat\" and \"b/b\\303\\257n.dat\" differ
";
        assert_eq!(generate_commit_message(diff), "feat(bïn): update code");
    }

    /// A quoted path that also contains a space: git appends a tab separator after the
    /// closing quote on the `---`/`+++` lines. The tab is a separator, not part of the
    /// path, so it is dropped rather than becoming a character the diff never named.
    #[test]
    fn quoted_hunk_path_tolerates_gits_trailing_tab_separator() {
        let diff = "\
diff --git \"a/sp \\303\\244ce.txt\" \"b/sp \\303\\244ce.txt\"
index 587be6b..b77b4eb 100644
--- \"a/sp \\303\\244ce.txt\"\t
+++ \"b/sp \\303\\244ce.txt\"\t
@@ -1 +1,2 @@
 x
+y
";
        assert_eq!(generate_commit_message(diff), "feat(sp äce): add changes");
    }

    /// Near-miss guard: a path with a plain **space** is rendered *unquoted* by git,
    /// works today, and must keep working byte-identically — the quoted branch must
    /// not capture it. (Its recorded path keeps git's trailing tab, exactly as before;
    /// the stem cuts at the first dot, so the scope is unaffected.)
    #[test]
    fn unquoted_spaced_path_is_byte_identical_to_before() {
        let diff = "\
diff --git a/we ird.txt b/we ird.txt
index b9bca01..e8df071 100644
--- a/we ird.txt\t
+++ b/we ird.txt\t
@@ -1 +1,2 @@
 plain
+x
";
        assert_eq!(generate_commit_message(diff), "feat(we ird): add changes");
    }

    /// Near-miss guard: the three unquoted shapes fixed on Day 178 (ordinary modify,
    /// added file, whole-file deletion) are untouched by the quoted branch. A
    /// discriminator tested only on the side that fires is vacuous green.
    #[test]
    fn unquoted_shapes_are_untouched_by_the_quoted_branch() {
        let modify = "\
diff --git a/src/main.rs b/src/main.rs
index 111..222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,2 @@
-old
+new
";
        assert_eq!(generate_commit_message(modify), "feat(main): update code");

        let added = "\
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..94954ab
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
";
        assert_eq!(generate_commit_message(added), "feat(new): add changes");

        let deleted = "\
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 94954ab..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-hello
-world
";
        assert_eq!(
            generate_commit_message(deleted),
            "refactor(gone): remove code"
        );
    }

    /// The header parse itself, including the case it deliberately refuses.
    #[test]
    fn diff_header_path_table() {
        let cases: &[(&str, Option<&str>)] = &[
            // The three shapes that have no other mention of the file.
            ("diff --git a/old.txt b/new.txt", Some("new.txt")),
            ("diff --git a/img.png b/img.png", Some("img.png")),
            (
                "diff --git a/src/keep.rs b/src/keep.rs",
                Some("src/keep.rs"),
            ),
            // Not a header at all — every other line in a diff must fall through.
            ("--- a/src/main.rs", None),
            ("+++ b/src/main.rs", None),
            ("+added line", None),
            ("-removed line", None),
            ("Binary files a/x and b/x differ", None),
            ("@@ -1,2 +1,3 @@", None),
            ("", None),
            // A path containing the separator (#830). This row used to assert `None` —
            // it pinned the defect, not a decision, and is inverted deliberately rather
            // than deleted quietly (a fixture asserting a known-wrong output that
            // outlives its fix converts a defect into a green invariant). git emits the
            // same path on both sides of a non-rename header, so exactly one of the
            // three candidate splits has byte-equal halves and the header resolves.
            (
                "diff --git a/has b/dir/f b/has b/dir/f",
                Some("has b/dir/f"),
            ),
            // The worked example from #830, with the separator twice inside one path.
            (
                "diff --git a/src/a b/c.rs b/src/a b/c.rs",
                Some("src/a b/c.rs"),
            ),
            // The residue, kept on purpose: an ambiguous *rename* has two genuinely
            // different paths, so no candidate split has equal halves and the file is
            // still dropped. Inventing a path that is not in the diff is worse than
            // dropping one — that is why the refusal exists, and it survives here.
            ("diff --git a/has b/one.rs b/has b/two.rs", None),
            // Quoted headers (#829). This row used to assert `None` — it pinned the
            // defect, not a decision, and is replaced deliberately rather than
            // deleted quietly: git quotes *both* paths when either contains a
            // non-ASCII byte, a `"`, a `\` or a control char, and the decoded path is
            // what belongs in the message.
            (
                "diff --git \"a/n\\303\\244me.txt\" \"b/n\\303\\244me.txt\"",
                Some("näme.txt"),
            ),
            // A rename: the *new* path wins, exactly as in the unquoted form.
            (
                "diff --git \"a/n\\303\\244me.txt\" \"b/n\\303\\240me2.txt\"",
                Some("nàme2.txt"),
            ),
            // `\"` decodes to a literal quote without terminating the token early.
            (
                "diff --git \"a/has\\\"quote.txt\" \"b/has\\\"quote2.txt\"",
                Some("has\"quote2.txt"),
            ),
            // The escapes git emits beside octal, and a `\\` that must not eat the
            // closing quote.
            (
                "diff --git \"a/t\\tb.txt\" \"b/back\\\\slash.txt\"",
                Some("back\\slash.txt"),
            ),
            // Refusals, all of which drop the file rather than invent a path:
            // a byte run that is not valid UTF-8 (a lone 0xFF)…
            ("diff --git \"a/x\" \"b/\\377bad.txt\"", None),
            // …an escape git is not known to emit…
            ("diff --git \"a/x\" \"b/\\qbad.txt\"", None),
            // …a short octal run…
            ("diff --git \"a/x\" \"b/\\30\"", None),
            // …a token that is never closed…
            ("diff --git \"a/x\" \"b/unterminated", None),
            // …and a quoted path that is empty after `b/`.
            ("diff --git \"a/x\" \"b/\"", None),
            // Empty new path.
            ("diff --git a/x b/", None),
        ];
        for (line, want) in cases {
            assert_eq!(
                diff_header_path(line).as_deref(),
                *want,
                "diff_header_path({line:?})"
            );
        }
    }
}
