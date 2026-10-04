//! Deny patterns added by `/cd` (#869 slice): the one part of a directory's
//! `.yoyo.toml` that `/cd` applies mid-session.
//!
//! Why deny only, and why no trust check: a deny entry can only NARROW what
//! bash may run — `cli::gate_project_permissions` already keeps a project's
//! `deny` verbatim while refusing its `allow` (#749 item 3). Allow,
//! `[directories]`, hooks, MCP and skills can widen, or need an agent rebuild
//! that drops MCP connections (#842), so they are still NOT reloaded on `/cd`.
//!
//! The list is **append-only for the session**: `/cd`-ing back to the launch
//! directory removes nothing. Both bash executors — the parent's
//! `StreamingBashTool` and the sub-agent child's `UserDenyBashTool` — hold a
//! handle to the same list and read it **at call time**, so an entry appended
//! after the tools were built still binds them. (Before this, `user_deny` was a
//! `Vec` cloned at build time, which nothing set later could reach.)
//!
//! Scope, measured rather than assumed: `permissions.deny` is a bash-command
//! glob list; the file tools (read/write/edit/list/search) never consult it, at
//! startup or after `/cd`. Path fencing for file tools is `[directories]`.

use std::sync::{Arc, LazyLock, Mutex};

/// A shared, call-time-read list of deny patterns. Production tools hold a
/// clone of the process-wide handle; tests build their own local one.
pub(crate) type DenyAdditions = Arc<Mutex<Vec<String>>>;

static CD_ADDED_DENY: LazyLock<DenyAdditions> = LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// The process-wide handle every production bash executor holds.
pub(crate) fn cd_added_deny_handle() -> DenyAdditions {
    Arc::clone(&CD_ADDED_DENY)
}

/// `existing` followed by every entry of `added` not already present.
/// Never removes, never reorders, dedupes (including within `added`).
pub(crate) fn merge_deny(existing: &[String], added: &[String]) -> Vec<String> {
    let mut out = existing.to_vec();
    for p in added {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

/// Append `added` to `list` (deduped). Returns how many entries were new.
pub(crate) fn append_deny_with(list: &DenyAdditions, added: &[String]) -> usize {
    let mut guard = crate::sync_util::lock_or_recover(list);
    let before = guard.len();
    *guard = merge_deny(&guard, added);
    guard.len() - before
}

/// The `/cd` consumer: append to the process-wide list.
pub(crate) fn append_cd_deny(added: &[String]) -> usize {
    append_deny_with(&CD_ADDED_DENY, added)
}

/// The refusal both bash executors apply: the build-time `base` list first,
/// then whatever `/cd` has added so far, read now. Same wording as
/// `tools::user_deny_refusal`, because it IS that predicate.
pub(crate) fn effective_user_deny_refusal(
    base: &[String],
    added: &DenyAdditions,
    command: &str,
) -> Option<String> {
    crate::tools::user_deny_refusal(base, command).or_else(|| {
        let guard = crate::sync_util::lock_or_recover(added);
        crate::tools::user_deny_refusal(&guard, command)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::StreamingBashTool;
    use yoagent::types::AgentTool;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn local() -> DenyAdditions {
        Arc::new(Mutex::new(Vec::new()))
    }

    async fn run(tool: &dyn AgentTool, cmd: &str) -> Result<String, String> {
        let ctx = yoagent::types::ToolContext::new("call-cd-deny", "bash");
        match tool
            .execute(serde_json::json!({ "command": cmd }), ctx)
            .await
        {
            Ok(r) => Ok(format!("{:?}", r.content)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[test]
    fn merge_deny_never_drops_and_dedupes() {
        let cases: &[(&[&str], &[&str], &[&str])] = &[
            (&[], &[], &[]),
            (&["a"], &[], &["a"]),
            (&[], &["a", "b"], &["a", "b"]),
            (&["a", "b"], &["b", "c"], &["a", "b", "c"]),
            (&["a"], &["c", "c", "a"], &["a", "c"]),
        ];
        for (existing, added, want) in cases {
            let got = merge_deny(&s(existing), &s(added));
            assert_eq!(got, s(want), "{existing:?} + {added:?}");
            for e in *existing {
                assert!(got.contains(&e.to_string()), "dropped {e}");
            }
        }
    }

    #[test]
    fn append_is_monotonic_and_counts_new_entries() {
        let list = local();
        assert_eq!(append_deny_with(&list, &s(&["rm *"])), 1);
        assert_eq!(append_deny_with(&list, &s(&["rm *", "curl *"])), 1);
        // A `/cd` back to a directory with no deny removes nothing.
        assert_eq!(append_deny_with(&list, &[]), 0);
        assert_eq!(*list.lock().unwrap(), s(&["rm *", "curl *"]));
    }

    #[tokio::test]
    async fn parent_bash_refuses_an_entry_added_after_build() {
        let list = local();
        let tool = StreamingBashTool {
            cd_deny: Arc::clone(&list),
            ..Default::default()
        };
        // Near-miss before the append: the command runs.
        let before = run(&tool, "echo CD_DENY_ZZ").await;
        assert!(before.is_ok(), "{before:?}");
        append_deny_with(&list, &s(&["echo CD_DENY_ZZ"]));
        let err = run(&tool, "echo CD_DENY_ZZ")
            .await
            .expect_err("must refuse");
        assert_eq!(
            err,
            "Command blocked by your permission rules: matches deny pattern 'echo CD_DENY_ZZ'."
        );
        // Near-miss after: a command matching nothing still runs.
        let out = run(&tool, "echo ok_ZZ").await.expect("must run");
        assert!(out.contains("ok_ZZ"), "{out}");
    }

    #[tokio::test]
    async fn child_bash_refuses_the_same_added_entry() {
        let list = local();
        let child = crate::tools::sub_agent_child_tools_with(
            &crate::cli::DirectoryRestrictions::default(),
            &[],
            &[],
            false,
            None,
            Arc::clone(&list),
        )
        .into_iter()
        .find(|t| t.name() == "bash")
        .expect("child bash");
        append_deny_with(&list, &s(&["echo CD_DENY_ZZ"]));
        let err = run(child.as_ref(), "echo CD_DENY_ZZ")
            .await
            .expect_err("child must refuse");
        assert_eq!(
            err,
            "Command blocked by your permission rules: matches deny pattern 'echo CD_DENY_ZZ'."
        );
        let out = run(child.as_ref(), "echo ok_ZZ").await.expect("runs");
        assert!(out.contains("ok_ZZ"), "{out}");
    }

    #[test]
    fn effective_refusal_reads_base_then_added() {
        let list = local();
        let base = s(&["sudo *"]);
        assert!(effective_user_deny_refusal(&base, &list, "sudo ls").is_some());
        assert_eq!(effective_user_deny_refusal(&base, &list, "rm x"), None);
        append_deny_with(&list, &s(&["rm *"]));
        assert!(effective_user_deny_refusal(&base, &list, "rm x").is_some());
        assert_eq!(effective_user_deny_refusal(&base, &list, "ls"), None);
    }
}
