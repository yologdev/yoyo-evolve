//! Resolve a named sub-agent file's `tools` / `disallowedTools` (#1250,
//! part of milestone 2) from Claude Code's tool names to yoyo's builtins.
//!
//! Mapping (Claude Code name → yoyo builtin):
//!
//! | Claude Code         | yoyo         |
//! |---------------------|--------------|
//! | `Read`              | `read_file`  |
//! | `Write`             | `write_file` |
//! | `Edit`, `MultiEdit` | `edit_file`  |
//! | `Glob`              | `list_files` |
//! | `Grep`              | `search`     |
//! | `Bash`              | `bash`       |
//! | `WebSearch`         | `web_search` |
//! | `TodoWrite`         | `todo`       |
//!
//! A yoyo builtin name written directly (`read_file`) resolves to itself.
//! Every target is checked against `BUILTIN_TOOL_NAMES` by a test; the list is
//! read, never copied. `Task` / `Agent` are deliberately **not** mapped to
//! `sub_agent` yet: nested delegation depth is milestone-2 design, so they are
//! reported unresolved with that reason.
//!
//! Matching is **exact and case-sensitive**: Claude Code's docs treat these as
//! identifiers ("must match ... exactly" for field names, and a misspelled
//! entry is one that "fails to resolve"), so `read` is not `Read`.
//!
//! Contract, from Claude Code's sub-agent docs (read Day 272):
//! - `disallowedTools` is applied first; `tools` is resolved against what is
//!   left. A tool listed in both is removed.
//! - A specifier such as `Bash(git push *)` names the whole tool: the `(...)`
//!   is stripped for matching, and in `disallowedTools` the whole tool goes
//!   (yoyo has no per-pattern child rule; the result says so).
//! - `tools` omitted → every available tool minus the disallowed ones.
//! - `tools` given but no entry resolves → **not usable** (Claude Code
//!   "refuses to launch"). Fail closed, never "run with everything".
//! - Entries that do not resolve (`WebFetch`, `NotebookEdit`, MCP names) are
//!   named with a reason, never silently dropped.
//!
//! User docs page: next slice (milestone 4).

use std::sync::{Arc, Mutex};

/// Claude Code tool name → yoyo builtin name.
const CLAUDE_CODE_TOOL_MAP: &[(&str, &str)] = &[
    ("Read", "read_file"),
    ("Write", "write_file"),
    ("Edit", "edit_file"),
    ("MultiEdit", "edit_file"),
    ("Glob", "list_files"),
    ("Grep", "search"),
    ("Bash", "bash"),
    ("WebSearch", "web_search"),
    ("TodoWrite", "todo"),
];

/// Claude Code names that would mean nested delegation.
const NESTED_DELEGATION_NAMES: &[&str] = &["Task", "Agent"];

/// One entry from the file that did not end up as a granted tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// `"tools"` or `"disallowedTools"`.
    pub field: &'static str,
    /// The entry as written in the file.
    pub entry: String,
    pub reason: String,
}

/// What a definition's tool fields resolve to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResolution {
    /// Granted yoyo tool names, in `tools` order (or `available` order when
    /// `tools` is omitted), deduplicated.
    pub resolved: Vec<String>,
    pub unresolved: Vec<Unresolved>,
    /// `disallowedTools` entries with a `(...)` specifier: each removed the
    /// whole tool, as written.
    pub whole_tool_removals: Vec<String>,
    /// False iff `tools` was given and nothing in it resolved.
    pub usable: bool,
}

/// Strip a `(...)` specifier: `Bash(git push *)` → `Bash`.
fn base_name(entry: &str) -> &str {
    entry.split('(').next().unwrap_or(entry).trim()
}

/// Map one name to a yoyo builtin, or say why it cannot be.
fn map_name(name: &str) -> Result<&'static str, String> {
    if let Some((_, yoyo)) = CLAUDE_CODE_TOOL_MAP.iter().find(|(cc, _)| *cc == name) {
        return Ok(yoyo);
    }
    if let Some(b) = crate::agent_builder::BUILTIN_TOOL_NAMES
        .iter()
        .find(|b| **b == name)
    {
        return Ok(b);
    }
    if NESTED_DELEGATION_NAMES.contains(&name) {
        return Err("nested delegation not supported yet".to_string());
    }
    Err("no yoyo tool".to_string())
}

/// Resolve `tools` / `disallowedTools` against `available`, the tool names a
/// yoyo sub-agent child actually has (see [`child_tool_names`]). Passed in so
/// tests are hermetic.
pub fn resolve_agent_tools(
    tools: Option<&[String]>,
    disallowed: &[String],
    available: &[&str],
) -> ToolResolution {
    let mut unresolved = Vec::new();
    let mut whole_tool_removals = Vec::new();
    let mut removed: Vec<&'static str> = Vec::new();
    for entry in disallowed {
        let base = base_name(entry);
        match map_name(base) {
            Ok(yoyo) => {
                if base != entry.trim() {
                    whole_tool_removals.push(entry.clone());
                }
                if !removed.contains(&yoyo) {
                    removed.push(yoyo);
                }
            }
            Err(reason) => unresolved.push(Unresolved {
                field: "disallowedTools",
                entry: entry.clone(),
                reason,
            }),
        }
    }
    let pool: Vec<&str> = available
        .iter()
        .copied()
        .filter(|a| !removed.contains(a))
        .collect();

    let Some(tools) = tools else {
        let mut resolved: Vec<String> = Vec::new();
        for name in pool {
            if !resolved.iter().any(|r| r == name) {
                resolved.push(name.to_string());
            }
        }
        return ToolResolution {
            resolved,
            unresolved,
            whole_tool_removals,
            usable: true,
        };
    };

    let mut resolved: Vec<String> = Vec::new();
    for entry in tools {
        let reason = match map_name(base_name(entry)) {
            Ok(yoyo) if pool.contains(&yoyo) => {
                if !resolved.iter().any(|r| r == yoyo) {
                    resolved.push(yoyo.to_string());
                }
                continue;
            }
            Ok(yoyo) if removed.contains(&yoyo) => {
                format!("{yoyo} removed by disallowedTools")
            }
            Ok(yoyo) => format!("{yoyo} is not available to yoyo sub-agents"),
            Err(reason) => reason,
        };
        unresolved.push(Unresolved {
            field: "tools",
            entry: entry.clone(),
            reason,
        });
    }
    let usable = !resolved.is_empty();
    ToolResolution {
        resolved,
        unresolved,
        whole_tool_removals,
        usable,
    }
}

/// The tool names a yoyo sub-agent child gets, read from the real child tool
/// set (`sub_agent_child_tools_with` with no restrictions, no deny list and a
/// fresh `/cd` deny list), so this never copies the list. The nested
/// `sub_agent` a child may also get is left out: it is not mapped yet.
pub fn child_tool_names() -> Vec<&'static str> {
    let tools = crate::tools::sub_agent_child_tools_with(
        &crate::config::DirectoryRestrictions::default(),
        &[],
        &[],
        false,
        None,
        Arc::new(Mutex::new(Vec::new())),
    );
    tools
        .iter()
        .filter_map(|t| {
            let name = t.name();
            crate::agent_builder::BUILTIN_TOOL_NAMES
                .iter()
                .find(|b| **b == name)
                .copied()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_builder::BUILTIN_TOOL_NAMES;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    const AVAIL: &[&str] = &[
        "bash",
        "read_file",
        "write_file",
        "edit_file",
        "list_files",
        "search",
        "web_search",
    ];

    #[test]
    fn every_mapping_target_is_a_builtin() {
        for (cc, yoyo) in CLAUDE_CODE_TOOL_MAP {
            assert!(
                BUILTIN_TOOL_NAMES.contains(yoyo),
                "{cc} maps to {yoyo}, which is not in BUILTIN_TOOL_NAMES"
            );
        }
    }

    #[test]
    fn each_mapping_row_resolves() {
        let rows = [
            ("Read", "read_file"),
            ("Write", "write_file"),
            ("Edit", "edit_file"),
            ("MultiEdit", "edit_file"),
            ("Glob", "list_files"),
            ("Grep", "search"),
            ("Bash", "bash"),
            ("WebSearch", "web_search"),
            ("TodoWrite", "todo"),
        ];
        let all: Vec<&str> = BUILTIN_TOOL_NAMES.to_vec();
        for (cc, want) in rows {
            let r = resolve_agent_tools(Some(&s(&[cc])), &[], &all);
            assert_eq!(r.resolved, s(&[want]), "row {cc}");
            assert!(r.usable && r.unresolved.is_empty(), "row {cc}");
        }
    }

    #[test]
    fn disallowed_is_applied_before_tools_and_specifier_removes_whole_tool() {
        let r = resolve_agent_tools(
            Some(&s(&["Read", "Bash"])),
            &s(&["Bash(git push *)"]),
            AVAIL,
        );
        assert_eq!(r.resolved, s(&["read_file"]));
        assert_eq!(
            r.unresolved,
            vec![Unresolved {
                field: "tools",
                entry: "Bash".to_string(),
                reason: "bash removed by disallowedTools".to_string(),
            }]
        );
        assert_eq!(r.whole_tool_removals, s(&["Bash(git push *)"]));
        assert!(r.usable);
    }

    #[test]
    fn omitted_tools_means_all_available_minus_disallowed() {
        let r = resolve_agent_tools(None, &s(&["Write", "Edit"]), AVAIL);
        assert_eq!(
            r.resolved,
            s(&["bash", "read_file", "list_files", "search", "web_search"])
        );
        assert!(r.usable);
        assert!(r.unresolved.is_empty());
    }

    #[test]
    fn nothing_resolving_is_not_usable_and_names_entries() {
        let r = resolve_agent_tools(Some(&s(&["WebFetch", "Task", "read"])), &[], AVAIL);
        assert!(!r.usable);
        assert!(r.resolved.is_empty());
        let got: Vec<(&str, &str)> = r
            .unresolved
            .iter()
            .map(|u| (u.entry.as_str(), u.reason.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("WebFetch", "no yoyo tool"),
                ("Task", "nested delegation not supported yet"),
                ("read", "no yoyo tool"),
            ]
        );
    }

    #[test]
    fn empty_tools_list_is_not_usable() {
        let r = resolve_agent_tools(Some(&[]), &[], AVAIL);
        assert!(!r.usable);
    }

    #[test]
    fn tool_in_both_lists_is_removed_and_can_make_agent_unusable() {
        let r = resolve_agent_tools(Some(&s(&["Bash"])), &s(&["Bash"]), AVAIL);
        assert!(!r.usable);
        assert!(r.whole_tool_removals.is_empty());
    }

    #[test]
    fn yoyo_native_name_resolves_to_itself() {
        let r = resolve_agent_tools(Some(&s(&["read_file", "Grep"])), &[], AVAIL);
        assert_eq!(r.resolved, s(&["read_file", "search"]));
    }

    #[test]
    fn known_but_unavailable_tool_is_named() {
        let r = resolve_agent_tools(Some(&s(&["TodoWrite", "Read"])), &[], AVAIL);
        assert_eq!(r.resolved, s(&["read_file"]));
        assert_eq!(
            r.unresolved[0].reason,
            "todo is not available to yoyo sub-agents"
        );
    }

    #[test]
    fn duplicates_are_deduped() {
        let r = resolve_agent_tools(
            Some(&s(&["Edit", "MultiEdit", "edit_file", "Read", "Read"])),
            &[],
            AVAIL,
        );
        assert_eq!(r.resolved, s(&["edit_file", "read_file"]));
        assert!(r.unresolved.is_empty());
    }

    #[test]
    fn unresolved_disallowed_entry_is_named() {
        let r = resolve_agent_tools(None, &s(&["mcp__github"]), AVAIL);
        assert_eq!(r.resolved.len(), AVAIL.len());
        assert_eq!(r.unresolved[0].field, "disallowedTools");
        assert_eq!(r.unresolved[0].entry, "mcp__github");
    }

    #[test]
    fn child_tool_names_are_the_real_child_set() {
        assert_eq!(child_tool_names(), AVAIL.to_vec());
    }
}
