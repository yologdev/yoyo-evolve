//! Tests for `src/partial_read_guard.rs`. Every row drives a LOCAL store
//! (the `drain_failure_note(&store)` seam), except the one end-to-end row
//! through `maybe_guard`, which uses the process-wide store under a unique
//! tempdir path so no other test can collide with it.

use super::*;
use yoagent::tools::file::{ReadFileTool, WriteFileTool};
use yoagent::types::AgentTool;

fn ctx() -> yoagent::types::ToolContext {
    yoagent::types::ToolContext::new("test", "test")
}

fn store() -> Store {
    Mutex::new(HashMap::new())
}

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}\n")).collect()
}

fn write_tmp(dir: &tempfile::TempDir, name: &str, body: &str) -> String {
    let p = dir.path().join(name);
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().into_owned()
}

fn first_text(r: &ToolResult) -> String {
    match &r.content[0] {
        yoagent::Content::Text { text } => text.clone(),
        _ => panic!("expected text content"),
    }
}

/// Drive the REAL yoagent `read_file` through the guard's gate/record pair.
async fn read(store: &Store, path: &str, offset: Option<u64>) -> String {
    let mut params = serde_json::json!({ "path": path });
    if let Some(o) = offset {
        params["offset"] = serde_json::json!(o);
    }
    let pending = gate_with(store, "read_file", Some(path)).unwrap();
    let r = ReadFileTool::default()
        .execute(params, ctx())
        .await
        .unwrap();
    record_with(store, pending, &r);
    first_text(&r)
}

/// Drive the REAL yoagent `write_file` through the guard. `Err` = refused.
async fn write(store: &Store, path: &str, content: &str) -> Result<(), String> {
    let pending = match gate_with(store, "write_file", Some(path)) {
        Ok(p) => p,
        Err(ToolError::Failed(m)) => return Err(m),
        Err(e) => panic!("unexpected error kind: {e:?}"),
    };
    let params = serde_json::json!({ "path": path, "content": content });
    let r = WriteFileTool::new().execute(params, ctx()).await.unwrap();
    record_with(store, pending, &r);
    Ok(())
}

// --- the parser, fed by the real tool's output (d211) ---------------------

#[tokio::test]
async fn real_read_file_header_on_600_lines_parses_as_a_page() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "big.txt", &numbered(600));
    let text = read(&store(), &p, None).await;
    let header = text.lines().next().unwrap();
    // Copied from the real tool, not typed from memory.
    assert_eq!(
        header,
        "[Lines 1-500 of 600 — use offset/limit to read the rest]"
    );
    assert_eq!(parse_read_header(&text), Some((1, 500, 600)));
}

#[tokio::test]
async fn real_read_file_header_on_a_whole_file_parses_as_whole() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "small.txt", &numbered(100));
    let text = read(&store(), &p, None).await;
    assert_eq!(text.lines().next().unwrap(), "[100 lines]");
    assert_eq!(parse_read_header(&text), Some((1, 100, 100)));
}

#[test]
fn unknown_header_shapes_record_nothing() {
    assert_eq!(parse_read_header("no header here"), None);
    assert_eq!(parse_read_header(""), None);
    assert_eq!(parse_read_header("[Lines x-y of z]"), None);
}

// --- the pure decision ----------------------------------------------------

#[test]
fn overwrite_refusal_names_path_count_and_lines_read_and_both_moves() {
    let r = overwrite_refusal("src/big.rs", &[(1, 500)], 2000).expect("must refuse");
    assert_eq!(
        r,
        "Refusing to overwrite 'src/big.rs': it has 2000 lines but this session has only \
         read lines 1-500, so writing it whole would destroy the lines you have not seen. \
         Read the rest with read_file offset/limit (a page whose output was truncated does not \
         count, so use a smaller limit), or change it in place with edit_file."
    );
    assert!(crate::prompt_retry::is_deterministic_tool_error(&r));
}

#[test]
fn overwrite_refusal_allows_full_coverage_including_merged_pages() {
    assert_eq!(overwrite_refusal("a", &[(1, 500), (501, 600)], 600), None);
    assert_eq!(overwrite_refusal("a", &[(501, 600), (1, 500)], 600), None);
    assert_eq!(overwrite_refusal("a", &[(1, 100)], 100), None);
    assert_eq!(overwrite_refusal("a", &[], 0), None);
}

#[test]
fn overwrite_refusal_refuses_a_gap_in_the_middle() {
    assert!(overwrite_refusal("a", &[(1, 200), (300, 600)], 600).is_some());
}

#[test]
fn overwrite_refusal_sanitizes_the_path_for_display() {
    let r = overwrite_refusal("evil\x1b[31m.rs", &[(1, 1)], 5).unwrap();
    assert!(!r.as_bytes().contains(&0x1b));
    assert!(r.contains("evil\\x1b[31m.rs"));
}

// --- end to end through the real tools, local store -----------------------

#[tokio::test]
async fn first_page_only_then_write_is_refused_and_the_file_is_untouched() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "big.txt", &numbered(600));
    let before = std::fs::read(&p).unwrap();
    read(&s, &p, None).await;
    let err = write(&s, &p, "clobbered\n").await.expect_err("must refuse");
    assert!(err.contains("it has 600 lines"), "{err}");
    assert!(err.contains("only read lines 1-500"), "{err}");
    assert_eq!(
        std::fs::read(&p).unwrap(),
        before,
        "file must be byte-identical"
    );
}

#[tokio::test]
async fn reading_the_second_page_unlocks_the_write() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "big.txt", &numbered(600));
    read(&s, &p, None).await;
    read(&s, &p, Some(501)).await;
    write(&s, &p, "new\n")
        .await
        .expect("whole file seen: allowed");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "new\n");
}

#[tokio::test]
async fn near_miss_small_file_read_whole_is_allowed() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "small.txt", &numbered(100));
    read(&s, &p, None).await;
    write(&s, &p, "x\n").await.expect("allowed");
}

#[tokio::test]
async fn near_miss_new_file_is_allowed() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("new.txt").to_string_lossy().into_owned();
    write(&s, &p, "hello\n").await.expect("allowed");
}

#[tokio::test]
async fn near_miss_existing_file_never_read_is_allowed_in_this_slice() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "big.txt", &numbered(600));
    write(&s, &p, "x\n")
        .await
        .expect("never read: not refused in this slice");
}

#[tokio::test]
async fn near_miss_write_after_own_write_is_allowed() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "big.txt", &numbered(600));
    read(&s, &p, Some(501)).await; // partial record exists
    read(&s, &p, None).await;
    write(&s, &p, &numbered(900)).await.expect("full coverage");
    // The model wrote all 900 lines, so a second write is allowed.
    write(&s, &p, "y\n")
        .await
        .expect("own write covers the file");
}

#[tokio::test]
async fn file_that_grew_after_a_whole_read_is_refused() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "grow.txt", &numbered(100));
    read(&s, &p, None).await;
    std::fs::write(&p, numbered(700)).unwrap(); // grows on disk behind our back
    let err = write(&s, &p, "x\n")
        .await
        .expect_err("stale coverage must refuse");
    assert!(err.contains("it has 700 lines"), "{err}");
    assert!(err.contains("only read lines 1-100"), "{err}");
}

#[tokio::test]
async fn edit_on_a_fully_read_file_keeps_it_fully_covered() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "e.txt", &numbered(100));
    read(&s, &p, None).await;
    let pending = gate_with(&s, "edit_file", Some(&p)).unwrap();
    std::fs::write(&p, numbered(105)).unwrap(); // what a successful edit did
    let ok = ToolResult {
        content: vec![],
        details: serde_json::Value::Null,
    };
    record_with(&s, pending, &ok);
    write(&s, &p, "x\n").await.expect("edit kept full coverage");
}

#[tokio::test]
async fn edit_on_a_partly_read_file_does_not_grant_coverage() {
    let s = store();
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "e.txt", &numbered(600));
    read(&s, &p, None).await;
    let pending = gate_with(&s, "edit_file", Some(&p)).unwrap();
    let ok = ToolResult {
        content: vec![],
        details: serde_json::Value::Null,
    };
    record_with(&s, pending, &ok);
    assert!(write(&s, &p, "x\n").await.is_err());
}

// --- end to end through the real wrapper (process-wide store) -------------

#[tokio::test]
async fn guarded_write_file_refuses_after_a_guarded_partial_read() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_tmp(&dir, "wrapped.txt", &numbered(600));
    let before = std::fs::read(&p).unwrap();
    let none = crate::cli::DirectoryRestrictions::default();
    let reader = crate::tool_wrappers::maybe_guard(Box::new(ReadFileTool::default()), &none);
    let writer = crate::tool_wrappers::maybe_guard(Box::new(WriteFileTool::new()), &none);
    reader
        .execute(serde_json::json!({ "path": p }), ctx())
        .await
        .unwrap();
    let err = writer
        .execute(serde_json::json!({ "path": p, "content": "x\n" }), ctx())
        .await
        .expect_err("guard must refuse");
    let msg = match err {
        ToolError::Failed(m) => m,
        e => panic!("unexpected: {e:?}"),
    };
    assert!(msg.contains(REFUSAL_STEM_PARTIAL_READ), "{msg}");
    assert_eq!(std::fs::read(&p).unwrap(), before);
}

// --- #1094 item 2: coverage is what the model saw, not what yoagent read ---

/// `read_file` exactly as `tools::build_tools` stacks it on the main agent:
/// `with_truncation(maybe_guard(ReadFileTool))`, on the process-wide store.
async fn read_through_real_stack(path: &str, offset: Option<u64>, limit: Option<u64>) -> String {
    let tool = crate::tool_wrappers::with_truncation(
        crate::tool_wrappers::maybe_guard(
            Box::new(ReadFileTool::default()),
            &crate::cli::DirectoryRestrictions::default(),
        ),
        crate::format::TOOL_OUTPUT_MAX_CHARS,
    );
    let mut params = serde_json::json!({ "path": path });
    if let Some(o) = offset {
        params["offset"] = serde_json::json!(o);
    }
    if let Some(l) = limit {
        params["limit"] = serde_json::json!(l);
    }
    first_text(&tool.execute(params, ctx()).await.unwrap())
}

/// `n` lines of `width` chars each, numbered so a dropped middle is visible.
fn wide(n: usize, width: usize) -> String {
    (1..=n)
        .map(|i| format!("{:<width$}\n", format!("row {i} "), width = width))
        .collect()
}

#[tokio::test]
async fn truncated_whole_page_read_does_not_cover_the_file_for_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "wide.txt", &wide(200, 300));
    let shown = read_through_real_stack(&path, None, None).await;
    // Anti-vacuous: the model really did get a cut page.
    assert!(
        shown.contains("[... truncated "),
        "not truncated: {shown:.200}"
    );
    assert!(!shown.contains("row 120 "), "middle row survived the cut");
    let err = match gate("write_file", Some(&path)) {
        Err(ToolError::Failed(m)) => m,
        Ok(_) => panic!("whole-file write after a truncated read was admitted"),
        Err(e) => panic!("unexpected error kind: {e:?}"),
    };
    assert!(err.contains(REFUSAL_STEM_PARTIAL_READ), "{err}");
}

#[tokio::test]
async fn untruncated_page_still_covers_the_file_for_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "narrow.txt", &wide(200, 100));
    let shown = read_through_real_stack(&path, None, None).await;
    assert!(!shown.contains("[... truncated "), "unexpected cut");
    assert!(shown.contains("row 120 "));
    assert!(gate("write_file", Some(&path)).is_ok());
}

#[tokio::test]
async fn truncated_read_then_smaller_pages_cover_the_file_for_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "wide2.txt", &wide(200, 300));
    read_through_real_stack(&path, None, None).await;
    assert!(gate("write_file", Some(&path)).is_err());
    // 50 lines x 300 chars = 15k: under the cap, so each page is seen whole.
    for off in [1u64, 51, 101, 151] {
        let shown = read_through_real_stack(&path, Some(off), Some(50)).await;
        assert!(!shown.contains("[... truncated "), "page {off} was cut");
    }
    assert!(gate("write_file", Some(&path)).is_ok());
}

#[tokio::test]
async fn refusal_after_a_truncated_read_names_the_path_and_the_way_out() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "wide3.txt", &wide(200, 300));
    read_through_real_stack(&path, None, None).await;
    let err = match gate("write_file", Some(&path)) {
        Err(ToolError::Failed(m)) => m,
        Ok(_) => panic!("admitted"),
        Err(e) => panic!("unexpected error kind: {e:?}"),
    };
    assert!(err.contains(&format!("'{path}'")), "{err}");
    assert!(err.contains("offset/limit"), "{err}");
    assert!(err.contains("smaller limit"), "{err}");
}

#[tokio::test]
async fn a_cut_page_withdraws_only_itself_not_earlier_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp(&dir, "keep.rs", &numbered(600));
    let s = store();
    read(&s, &path, Some(501)).await; // lines 501-600, seen whole
    let pending = gate_with(&s, "read_file", Some(&path)).unwrap();
    let full = ReadFileTool::default()
        .execute(serde_json::json!({ "path": path }), ctx())
        .await
        .unwrap();
    record_with(&s, pending, &full);
    let cut = crate::tool_truncate::truncate_result(full.clone(), 1_000, false);
    retract_cut_with(&s, Some(&path), &full, &cut);
    let c = canonical(&path).unwrap();
    assert_eq!(s.lock().unwrap().get(&c).cloned(), Some(vec![(501, 600)]));
    // Near-miss: an uncut result withdraws nothing.
    let pending = gate_with(&s, "read_file", Some(&path)).unwrap();
    record_with(&s, pending, &full);
    let same = crate::tool_truncate::truncate_result(full.clone(), usize::MAX, false);
    retract_cut_with(&s, Some(&path), &full, &same);
    assert!(covers_all(&s.lock().unwrap()[&c], 600));
}
