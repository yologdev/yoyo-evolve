//! #1192: `/read` and `/plan` said "read-only commands are allowed" and let a
//! bash delete through, because neither classifier the read guard composes
//! knew deleting was writing. The guard (src/tool_wrappers.rs, the
//! `ReadGuardKind::Bash` arm) is, verbatim:
//!
//! ```text
//! if let Some(reason) = crate::safety::analyze_bash_command(cmd) { return Err(..) }
//! if let Some(what) = crate::safety::detect_write_command(cmd) { return Err(..) }
//! ```
//!
//! so a command is refused in read/plan mode iff either returns `Some`.
//! [`read_guard_refuses`] mirrors exactly that composition, and every row
//! here is judged at the consumer, not per classifier: the Day-258 lesson is
//! that two classifiers each saying "not mine" is the union of their blind
//! spots, which is what the user got.
//!
//! Census before the fix (Day 258): 7 of the 11 delete rows were allowed.
//! After adding `rm`, `rmdir`, `unlink`, `shred` to `WRITE_VERBS`, all 17
//! must-refuse rows are refused and all near-miss rows stay allowed.

use super::{analyze_bash_command, detect_write_command};

/// The read guard's composition, mirroring tool_wrappers.rs's Bash arm.
fn read_guard_refuses(cmd: &str) -> bool {
    analyze_bash_command(cmd).is_some() || detect_write_command(cmd).is_some()
}

/// Commands that destroy or write files, with project-relative and /tmp
/// paths. Every one must be refused in read/plan mode.
const MUST_REFUSE: &[&str] = &[
    "rm f",
    "rm -rf src/",
    "rm -rf /tmp/yoyo-probe",
    "FOO=1 rm x",
    "sudo rm x",
    "env rm x",
    "/bin/rm x",
    "rmdir d",
    "unlink f",
    "shred -u f",
    "find . -name '*.o' -delete",
    "dd if=/dev/zero of=f",
    "git clean -fdx",
    "rsync -a --delete a/ b/",
    "echo $(rm x)",
    "ls && rm x",
    "git rm f",
    // Controls that were already refused before #1192.
    "chmod 000 f",
    "touch x",
];

/// Reads that merely mention a delete verb. These must stay allowed, or the
/// fix turns read mode into a mode that refuses reading.
const MUST_ALLOW: &[&str] = &[
    "ls rm",
    "grep -rn rm src/",
    "cat rmdir.txt",
    "echo \"rm is a verb\"",
    "grep -n unlink src/lib.rs",
    "ls -la",
    "find . -name '*.rs'",
    "git status",
];

#[test]
fn read_guard_refuses_every_delete_and_write_row() {
    let allowed: Vec<&str> = MUST_REFUSE
        .iter()
        .copied()
        .filter(|cmd| !read_guard_refuses(cmd))
        .collect();
    assert!(
        allowed.is_empty(),
        "read/plan mode would run these destructive commands: {allowed:?}"
    );
}

#[test]
fn read_guard_still_allows_reads_that_mention_delete_verbs() {
    let refused: Vec<&str> = MUST_ALLOW
        .iter()
        .copied()
        .filter(|cmd| read_guard_refuses(cmd))
        .collect();
    assert!(
        refused.is_empty(),
        "read/plan mode would refuse these reads: {refused:?}"
    );
}

/// The refusal names the verb, so the model and the user see why: a bare
/// "blocked" reads as arbitrary.
#[test]
fn a_delete_refusal_names_the_verb() {
    for verb in ["rm", "rmdir", "unlink", "shred"] {
        let what = detect_write_command(&format!("{verb} f"))
            .unwrap_or_else(|| panic!("{verb} must be a write"));
        assert!(what.contains(&format!("`{verb}`")), "{verb}: {what}");
    }
}
