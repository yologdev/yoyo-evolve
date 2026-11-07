//! #1094 items 3 and 4: yoagent's per-agent `ReadLog`, wired into the real
//! tool stacks yoyo builds (`build_tools_with_hooks` for the parent,
//! `sub_agent_child_tools` for a child). Every file lives in its own tempdir,
//! so yoyo's process-wide `partial_read_guard` store never collides across
//! tests; that store still runs here, on purpose (both layers must pass).

use crate::cli;
use crate::hooks::HookRegistry;
use std::sync::Arc;
use yoagent::types::{AgentTool, ToolError};

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}\n")).collect()
}

fn big_file(dir: &tempfile::TempDir) -> String {
    let p = dir.path().join("big.txt");
    std::fs::write(&p, numbered(600)).unwrap();
    p.to_string_lossy().into_owned()
}

/// The parent's tools exactly as production builds them (auto-approve so no
/// confirm prompt; a huge output cap so nothing is truncated).
fn parent_tools() -> Vec<Box<dyn AgentTool>> {
    crate::tools::build_tools_with_hooks(
        true,
        &cli::PermissionConfig::default(),
        &cli::DirectoryRestrictions::default(),
        10_000_000,
        &Arc::new(HookRegistry::new()),
        None,
    )
}

fn child_tools() -> Vec<Arc<dyn AgentTool>> {
    crate::tools::sub_agent_child_tools(
        &cli::DirectoryRestrictions::default(),
        &[],
        &[],
        false,
        None,
    )
}

fn named<'a>(tools: &'a [Box<dyn AgentTool>], name: &str) -> &'a dyn AgentTool {
    tools
        .iter()
        .find(|t| t.name() == name)
        .unwrap_or_else(|| panic!("parent must carry {name}"))
        .as_ref()
}

fn child_named(tools: &[Arc<dyn AgentTool>], name: &str) -> Arc<dyn AgentTool> {
    tools
        .iter()
        .find(|t| t.name() == name)
        .unwrap_or_else(|| panic!("child must carry {name}"))
        .clone()
}

fn ctx(name: &str) -> yoagent::types::ToolContext {
    yoagent::types::ToolContext::new("call-1094", name)
}

async fn read(tool: &dyn AgentTool, path: &str, offset: Option<u64>, limit: Option<u64>) {
    let mut params = serde_json::json!({ "path": path });
    if let Some(o) = offset {
        params["offset"] = serde_json::json!(o);
    }
    if let Some(l) = limit {
        params["limit"] = serde_json::json!(l);
    }
    tool.execute(params, ctx("read_file"))
        .await
        .expect("read_file must succeed");
}

/// Read every line, in the default 500-line pages.
async fn read_whole(tool: &dyn AgentTool, path: &str) {
    read(tool, path, None, None).await;
    read(tool, path, Some(501), None).await;
}

/// `Err(message)` = refused.
async fn write(tool: &dyn AgentTool, path: &str) -> Result<(), String> {
    let params = serde_json::json!({ "path": path, "content": "replaced\n" });
    match tool.execute(params, ctx("write_file")).await {
        Ok(_) => Ok(()),
        Err(ToolError::Failed(m)) => Err(m),
        Err(e) => Err(format!("{e:?}")),
    }
}

#[tokio::test]
async fn child_full_read_does_not_let_the_parent_overwrite_its_own_partial_read_item_3() {
    let dir = tempfile::tempdir().unwrap();
    let p = big_file(&dir);
    let parent = parent_tools();
    let child = child_tools();

    read_whole(child_named(&child, "read_file").as_ref(), &p).await;
    read(named(&parent, "read_file"), &p, None, Some(100)).await;

    let err = write(named(&parent, "write_file"), &p)
        .await
        .expect_err("the parent saw lines 1-100 only; its write must be refused");
    assert!(err.contains("only lines 1-100"), "{err}");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), numbered(600));
}

#[tokio::test]
async fn file_shrunk_on_disk_after_a_partial_read_is_refused_item_4() {
    let dir = tempfile::tempdir().unwrap();
    let p = big_file(&dir);
    let parent = parent_tools();

    read(named(&parent, "read_file"), &p, None, None).await; // lines 1-500
    std::fs::write(&p, numbered(300)).unwrap(); // what a `bash` rewrite does

    let err = write(named(&parent, "write_file"), &p)
        .await
        .expect_err("a file changed since a partial read must be refused");
    assert!(err.contains("changed on disk"), "{err}");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), numbered(300));
}

#[tokio::test]
async fn yoagent_read_log_refusal_is_a_deterministic_refusal_so_no_auto_retry() {
    let dir = tempfile::tempdir().unwrap();
    let p = big_file(&dir);
    let parent = parent_tools();
    let child = child_tools();
    read_whole(child_named(&child, "read_file").as_ref(), &p).await;
    read(named(&parent, "read_file"), &p, None, Some(100)).await;
    let err = write(named(&parent, "write_file"), &p).await.unwrap_err();
    // Captured from yoagent 0.27.2, not typed: only ReadLog refused here.
    assert!(err.starts_with("Refusing to overwrite "), "{err}");
    assert!(err.contains(crate::tools::READ_LOG_REFUSAL_STEM), "{err}");
    assert!(
        crate::prompt_retry::is_deterministic_tool_error(&err),
        "{err}"
    );
}

#[tokio::test]
async fn whole_read_then_write_is_still_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let p = big_file(&dir);
    let parent = parent_tools();
    read_whole(named(&parent, "read_file"), &p).await;
    write(named(&parent, "write_file"), &p).await.unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "replaced\n");
}

#[tokio::test]
async fn child_whole_read_then_child_write_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let p = big_file(&dir);
    let child = child_tools();
    read_whole(child_named(&child, "read_file").as_ref(), &p).await;
    write(child_named(&child, "write_file").as_ref(), &p)
        .await
        .unwrap();
}

#[tokio::test]
async fn never_read_overwrite_is_still_allowed_item_1_undecided() {
    let dir = tempfile::tempdir().unwrap();
    let p = big_file(&dir);
    let parent = parent_tools();
    write(named(&parent, "write_file"), &p).await.unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "replaced\n");
}
