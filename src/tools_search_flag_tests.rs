//! #996 part 1: a `search` pattern that starts with `-` is searched as a
//! literal, never read as an `rg` / `grep` flag.
//!
//! yoyo's `search` builtin is yoagent's `SearchTool` (wrapped by `maybe_guard`
//! and the usual decorators), in the parent's tool list and in every
//! sub-agent child's. yoagent 0.24.1 passed the pattern as a bare argument, so
//! `--pre=<cmd>` made ripgrep run `<cmd>` on every file it searched (and made
//! the grep fallback fail on an unknown option). 0.24.2 passes it as
//! `--regexp=` / `-e` with `--` before the path.
//!
//! The probe holds whichever backend the machine has: with `rg`, 0.24.1 would
//! run the marker script (the marker assertion reddens); with only `grep`,
//! 0.24.1 errors on the unknown flag (the literal-match assertion reddens).
//! Both are asserted through yoyo's own construction path, not a bare
//! `SearchTool::default()`.

use crate::cli;
use std::path::Path;
use std::sync::Arc;
use yoagent::types::AgentTool;

/// Lay out a temp dir with a marker-writing script outside the searched dir,
/// and a file inside it whose text is exactly the `--pre=` pattern.
/// Returns (tempdir guard, searched dir, marker path, pattern).
fn fixture() -> (tempfile::TempDir, String, std::path::PathBuf, String) {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("PRE_EXECUTED");
    let script = tmp.path().join("probe.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\ncat \"$1\"\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let searched = tmp.path().join("searched");
    std::fs::create_dir(&searched).unwrap();
    let pattern = format!("--pre={}", script.display());
    std::fs::write(searched.join("needle.txt"), format!("{pattern}\n")).unwrap();
    let searched = searched.to_string_lossy().into_owned();
    (tmp, searched, marker, pattern)
}

async fn run_search(tool: &dyn AgentTool, pattern: &str, path: &str) -> Result<String, String> {
    let ctx = yoagent::types::ToolContext::new("call-996", "search");
    match tool
        .execute(serde_json::json!({ "pattern": pattern, "path": path }), ctx)
        .await
    {
        Ok(r) => Ok(format!("{:?}", r.content)),
        Err(e) => Err(e.to_string()),
    }
}

async fn assert_literal(tool: &dyn AgentTool, who: &str) {
    let (_tmp, searched, marker, pattern) = fixture();
    // Anti-vacuous: the fixture really does carry a flag-shaped pattern, and
    // the file really does contain it, so a match proves literal treatment.
    assert!(pattern.starts_with("--pre="));
    let on_disk = std::fs::read_to_string(Path::new(&searched).join("needle.txt")).unwrap();
    assert!(on_disk.contains(&pattern));

    let out = run_search(tool, &pattern, &searched)
        .await
        .unwrap_or_else(|e| panic!("{who}: search must succeed, got error: {e}"));
    assert!(
        !marker.exists(),
        "{who}: the --pre= command was EXECUTED (marker written); output: {out}"
    );
    assert!(
        out.contains("needle.txt"),
        "{who}: the pattern must be searched as a literal and match the file; output: {out}"
    );
}

#[tokio::test]
async fn parent_search_treats_pre_flag_pattern_as_literal() {
    let tools = crate::tools::build_tools(
        true,
        &cli::PermissionConfig::default(),
        &cli::DirectoryRestrictions::default(),
        crate::format::TOOL_OUTPUT_MAX_CHARS,
        false,
        vec![],
        None,
    );
    let search = tools
        .iter()
        .find(|t| t.name() == "search")
        .expect("the parent must carry a search tool, or this test is vacuous");
    assert_literal(search.as_ref(), "parent").await;
}

#[tokio::test]
async fn child_search_treats_pre_flag_pattern_as_literal() {
    let tools: Vec<Arc<dyn AgentTool>> = crate::tools::sub_agent_child_tools(
        &cli::DirectoryRestrictions::default(),
        &[],
        &[],
        false,
        None,
    );
    let search = tools
        .iter()
        .find(|t| t.name() == "search")
        .expect("the child must carry a search tool, or this test is vacuous");
    assert_literal(search.as_ref(), "child").await;
}
