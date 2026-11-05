//! Subdirectory instruction files, loaded on demand.
//!
//! At startup yoyo loads `PROJECT_CONTEXT_FILES` (YOYO.md, CLAUDE.md,
//! AGENTS.md, ...) from the project root only, so in a monorepo the rules in
//! `packages/api/AGENTS.md` never reach the model: it reads
//! `packages/api/src/x.rs` and gets no sign the package has instructions (an
//! absence, which reads as "there are none").
//!
//! Now, the first successful `read_file` of a file under a directory strictly
//! below the root gets that directory's instruction files appended to the tool
//! result as one NEW trailing text block, each wrapped in the same provenance
//! block the root files use (`context::wrap_project_instruction`). Once per
//! directory per session: the directory is marked seen whether or not it had a
//! file. The root itself is never re-injected (startup already loaded it).
//!
//! The anchor is the current directory at read time, so after `/cd` it moves.
//! Every candidate is put through the fences a `read_file` of that path would
//! hit (startup + `/cd` deny, non-regular file, outside-launch-dir read fence,
//! never prompting); a refused file is skipped silently.
//!
//! A repo with no nested instruction files gets a byte-identical result.
//! Out of scope: edits (Cursor also applies on edit), a per-agent store (the
//! store is process-wide, so a sub-agent's read marks a directory seen for the
//! parent too, as with `partial_read_guard`), and `/context` listing these.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::context::PROJECT_CONTEXT_FILES;

/// Per-file cap on what one nested instruction file contributes. Cut on a
/// char boundary, with the cut stated in band.
pub(crate) const NESTED_INSTRUCTION_MAX_BYTES: usize = 16_000;

/// The directories that may hold nested instruction files for a read of
/// `read_path`: the ancestors of its parent strictly below `root`, nearest to
/// the root first. A relative `read_path` is resolved against `root`; a path
/// outside `root`, or one with `..`/`.` left in it, gives none.
pub(crate) fn nested_instruction_dirs(read_path: &Path, root: &Path) -> Vec<PathBuf> {
    let full = if read_path.is_absolute() {
        read_path.to_path_buf()
    } else {
        root.join(read_path)
    };
    let Ok(rel) = full.strip_prefix(root) else {
        return Vec::new();
    };
    let Some(parent) = rel.parent() else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut cur = root.to_path_buf();
    for c in parent.components() {
        match c {
            Component::Normal(name) => {
                cur.push(name);
                dirs.push(cur.clone());
            }
            _ => return Vec::new(),
        }
    }
    dirs
}

/// Cap `content` at `max` bytes on a char boundary, marking the cut in band.
fn cap_content(content: &str, max: usize) -> String {
    if content.len() <= max {
        return content.to_string();
    }
    let mut b = max;
    while b > 0 && !content.is_char_boundary(b) {
        b -= 1;
    }
    let dropped = content.len() - b;
    let mut kept = content.to_string();
    kept.truncate(b);
    format!(
        "{kept}\n[... cut: {dropped} more bytes of this file not shown; read it with read_file for the rest]"
    )
}

/// The note for a read of `read_path`, or `None` when no directory not yet in
/// `seen` holds an instruction file. `read(path)` returns a file's content, or
/// `None` when it is absent or refused. Every candidate directory is marked
/// seen, found or not.
pub(crate) fn nested_instruction_note(
    read_path: &Path,
    root: &Path,
    seen: &mut HashSet<PathBuf>,
    read: impl Fn(&Path) -> Option<String>,
) -> Option<String> {
    let boundary = crate::context::instruction_boundary();
    let mut blocks = Vec::new();
    for dir in nested_instruction_dirs(read_path, root) {
        if !seen.insert(dir.clone()) {
            continue;
        }
        for name in PROJECT_CONTEXT_FILES {
            let path = dir.join(name);
            let Some(content) = read(&path) else { continue };
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            let shown = path.strip_prefix(root).unwrap_or(&path);
            let shown = shown.to_string_lossy();
            let capped = cap_content(content, NESTED_INSTRUCTION_MAX_BYTES);
            blocks.push(crate::context::wrap_project_instruction(
                &shown, &capped, boundary,
            ));
        }
    }
    if blocks.is_empty() {
        return None;
    }
    Some(format!(
        "Subdirectory instructions: the file you just read sits under a directory \
         with its own instruction file(s), loaded once per directory this session. \
         They apply to work in that directory.\n\n{}",
        blocks.join("\n\n")
    ))
}

/// The tool-result half: append the note for `path` as a new trailing text
/// block. Does nothing unless `tool` is `read_file`.
fn append_with(
    tool: &str,
    path: Option<&str>,
    root: &Path,
    seen: &mut HashSet<PathBuf>,
    read: impl Fn(&Path) -> Option<String>,
    result: &mut yoagent::types::ToolResult,
) {
    if tool != "read_file" {
        return;
    }
    let Some(path) = path else { return };
    // Canonical when it resolves, so a symlinked root and a relative read meet.
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    let read_path = std::fs::canonicalize(&joined).unwrap_or(joined);
    if let Some(note) = nested_instruction_note(&read_path, root, seen, read) {
        result.content.push(yoagent::Content::Text { text: note });
    }
}

fn global_seen() -> &'static Mutex<HashSet<PathBuf>> {
    static SEEN: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Read one candidate instruction file through the fences a `read_file` of it
/// would hit; `None` when absent or refused (silently).
fn fenced_read(path: &Path, restrictions: &crate::cli::DirectoryRestrictions) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let s = path.to_string_lossy();
    if crate::cd_config_note::path_guard_refusal(restrictions, &s).is_some() {
        return None;
    }
    if crate::outside_read::check_without_asking(&s, restrictions).is_some() {
        return None;
    }
    crate::non_regular_read::read_regular_to_string(path).ok()
}

/// The `GuardedTool` / `ArcGuardedTool` call site, after a successful call.
pub(crate) fn append_for_read(
    tool: &str,
    path: Option<&str>,
    restrictions: &crate::cli::DirectoryRestrictions,
    result: &mut yoagent::types::ToolResult,
) {
    if tool != "read_file" {
        return;
    }
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let root = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    let mut seen = crate::sync_util::lock_or_recover(global_seen());
    append_with(
        tool,
        path,
        &root,
        &mut seen,
        |p| fenced_read(p, restrictions),
        result,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fs(files: &[(&str, &str)], root: &Path) -> HashMap<PathBuf, String> {
        files
            .iter()
            .map(|(p, c)| (root.join(p), c.to_string()))
            .collect()
    }

    fn note(
        read: &str,
        root: &Path,
        files: &HashMap<PathBuf, String>,
        seen: &mut HashSet<PathBuf>,
    ) -> Option<String> {
        nested_instruction_note(Path::new(read), root, seen, |p| files.get(p).cloned())
    }

    #[test]
    fn dirs_are_strictly_below_root_nearest_root_first() {
        let root = Path::new("/r");
        assert_eq!(
            nested_instruction_dirs(Path::new("a/b/x.rs"), root),
            vec![PathBuf::from("/r/a"), PathBuf::from("/r/a/b")]
        );
        assert_eq!(
            nested_instruction_dirs(Path::new("/r/a/x.rs"), root),
            vec![PathBuf::from("/r/a")]
        );
        // A file in the root itself: no nested dirs (root is loaded at startup).
        assert!(nested_instruction_dirs(Path::new("/r/x.rs"), root).is_empty());
        assert!(nested_instruction_dirs(Path::new("/elsewhere/a/x.rs"), root).is_empty());
        assert!(nested_instruction_dirs(Path::new("a/../../x/y.rs"), root).is_empty());
    }

    #[test]
    fn nested_file_found_once_and_sibling_read_adds_nothing() {
        let root = Path::new("/r");
        let files = fs(&[("pkg/AGENTS.md", "use tabs in pkg")], root);
        let mut seen = HashSet::new();
        let first = note("pkg/src/a.rs", root, &files, &mut seen).expect("found");
        assert!(first.contains("use tabs in pkg"), "{first}");
        assert!(first.contains("-BEGIN pkg/AGENTS.md]"), "{first}");
        assert!(first.contains("-END pkg/AGENTS.md]"), "{first}");
        assert_eq!(note("pkg/src/b.rs", root, &files, &mut seen), None);
        assert_eq!(note("pkg/a.rs", root, &files, &mut seen), None);
    }

    #[test]
    fn roots_own_instruction_file_is_never_injected() {
        let root = Path::new("/r");
        let files = fs(&[("CLAUDE.md", "root rules")], root);
        let mut seen = HashSet::new();
        assert_eq!(note("x.rs", root, &files, &mut seen), None);
        assert_eq!(note("a/b/x.rs", root, &files, &mut seen), None);
    }

    #[test]
    fn two_levels_both_found_outer_first() {
        let root = Path::new("/r");
        let files = fs(
            &[
                ("a/AGENTS.md", "OUTER rule"),
                ("a/b/CLAUDE.md", "INNER rule"),
            ],
            root,
        );
        let mut seen = HashSet::new();
        let n = note("a/b/c.rs", root, &files, &mut seen).expect("found");
        let outer = n.find("OUTER rule").expect("outer present");
        let inner = n.find("INNER rule").expect("inner present");
        assert!(outer < inner, "{n}");
    }

    #[test]
    fn file_outside_root_and_tree_without_files_give_none() {
        let root = Path::new("/r");
        let files = fs(&[("a/AGENTS.md", "rule")], root);
        let mut seen = HashSet::new();
        assert_eq!(note("/other/a/x.rs", root, &files, &mut seen), None);
        let empty = HashMap::new();
        assert_eq!(note("a/b/x.rs", root, &empty, &mut HashSet::new()), None);
        // Whitespace-only files contribute nothing, like the root loader.
        let blank = fs(&[("a/AGENTS.md", "  \n ")], root);
        assert_eq!(note("a/x.rs", root, &blank, &mut HashSet::new()), None);
    }

    #[test]
    fn oversized_file_is_cut_on_a_char_boundary_with_the_cut_marked() {
        let root = Path::new("/r");
        // A 3-byte char straddles the cap.
        let mut body = "x".repeat(NESTED_INSTRUCTION_MAX_BYTES - 1);
        body.push('✓');
        body.push_str("TAIL");
        // Anti-vacuous: the cap really does fall inside the multi-byte char.
        assert!(!body.is_char_boundary(NESTED_INSTRUCTION_MAX_BYTES));
        let files = fs(&[("a/AGENTS.md", &body)], root);
        let n = note("a/x.rs", root, &files, &mut HashSet::new()).expect("found");
        assert!(!n.contains("TAIL"), "tail must be cut");
        assert!(!n.contains('✓'), "straddling char is dropped whole");
        let dropped = body.len() - (NESTED_INSTRUCTION_MAX_BYTES - 1);
        assert!(
            n.contains(&format!("cut: {dropped} more bytes")),
            "dropped count agrees with what was dropped: {n}"
        );
    }

    #[test]
    fn the_anchor_decides_same_read_under_two_roots_differs() {
        let files: HashMap<PathBuf, String> =
            [(PathBuf::from("/r/a/AGENTS.md"), "a rules".to_string())].into();
        let read = "/r/a/b/x.rs";
        let under_r = note(read, Path::new("/r"), &files, &mut HashSet::new());
        let under_ra = note(read, Path::new("/r/a"), &files, &mut HashSet::new());
        assert!(under_r.is_some_and(|n| n.contains("a rules")));
        // With /r/a as the anchor, /r/a is the root: already loaded, not nested.
        assert_eq!(under_ra, None);
    }

    fn sample_result() -> yoagent::types::ToolResult {
        yoagent::types::ToolResult {
            content: vec![yoagent::Content::Text {
                text: "file body".into(),
            }],
            details: serde_json::Value::Null,
        }
    }

    #[test]
    fn no_nested_files_leaves_the_result_byte_identical() {
        let root = Path::new("/r");
        let empty: HashMap<PathBuf, String> = HashMap::new();
        let mut r = sample_result();
        append_with(
            "read_file",
            Some("a/b/x.rs"),
            root,
            &mut HashSet::new(),
            |p| empty.get(p).cloned(),
            &mut r,
        );
        assert_eq!(r, sample_result());
    }

    #[test]
    fn only_read_file_gets_the_note_and_it_is_a_new_trailing_block() {
        let root = Path::new("/r");
        let files = fs(&[("a/AGENTS.md", "rule")], root);
        let mut r = sample_result();
        append_with(
            "write_file",
            Some("a/x.rs"),
            root,
            &mut HashSet::new(),
            |p| files.get(p).cloned(),
            &mut r,
        );
        assert_eq!(r, sample_result());
        append_with(
            "read_file",
            Some("a/x.rs"),
            root,
            &mut HashSet::new(),
            |p| files.get(p).cloned(),
            &mut r,
        );
        assert_eq!(r.content.len(), 2);
        assert_eq!(r.content[0], sample_result().content[0]);
        match &r.content[1] {
            yoagent::Content::Text { text } => assert!(text.contains("rule"), "{text}"),
            other => panic!("expected text block, got {other:?}"),
        }
    }
}
