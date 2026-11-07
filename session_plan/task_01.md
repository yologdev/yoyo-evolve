Title: Part of #1094: wire yoagent's ReadLog into read_file/write_file, one log per agent (closes items 3 and 4)
Kind: product
Files: src/tools.rs, src/partial_read_guard_tests.rs (or a new src/read_log_tests.rs), tests/module_size.rs (only if a size entry must be pasted)
Issue: #1094
Dependency: yoagent 0.27.2 `yoagent::tools::ReadLog` + `ReadFileTool::with_read_log` / `WriteFileTool::with_read_log`; checked the yoagent skill's tools section and the `ReadLog` doc comment in `~/.cargo/registry/src/*/yoagent-0.27.2/src/tools/file.rs` (lines ~353-395)

## Why (user terms)
A model that reads page 1 of a long file and writes "the whole file" back destroys the rest. yoyo's own guard (`src/partial_read_guard.rs`) catches that, but it keeps ONE process-wide store, so a sub-agent's full read lets the parent overwrite a file the parent only half-read (#1094 item 3), and a file shrunk by `bash` after a partial read is judged only by line count (item 4). yoagent 0.27.2 now ships the same guard — modelled on ours — per agent, keyed on file version (length + mtime). yoyo builds `ReadFileTool::default()` / `WriteFileTool::new()` with no log, so none of it runs. CLAUDE.md: "don't reinvent the wheel".

## Decision for this slice: two layers, not a retirement
Keep `partial_read_guard.rs` running. Reason (verify, don't trust): ReadLog records INSIDE `read_file`, before yoyo's `TruncatingTool` cuts the output, so ReadLog alone would count a truncated page as read — item 2 regresses if yoyo's layer goes. With both layers a write must pass both; yoyo's layer still handles truncation, ReadLog adds per-agent scope and version checks. Retiring yoyo's layer is a later decision; write it on the ticket, don't do it here.

## Steps
1. Read `tests/module_size.rs`'s entry for `src/tools.rs` (recorded 5247, file is ~5268) and pay any growth there in the same commit.
2. In `src/tools.rs`, at every site that constructs `ReadFileTool` and `WriteFileTool` (grep them: ~l.1345/1349/1391 for the parent, ~l.1782/1784 and ~l.2211/2214 for sub-agent / explore-agent children), create ONE `ReadLog::new()` per agent being built and pass clones to that agent's read and write tool. The parent and each child builder get their own fresh log — never share the parent's (yoagent's doc: "a sub-agent sharing its parent's would count the parent's reads as its own"). Check whether `smart_edit` or any other wrapper constructs its own `WriteFileTool`/`ReadFileTool` (grep src/ for `WriteFileTool::new\|ReadFileTool::` beyond tools.rs) and cover it or state why not.
3. Check `src/prompt_retry.rs:479` (it matches `partial_read_guard::REFUSAL_STEM_PARTIAL_READ`): capture yoagent's actual refusal text from a test and decide whether that classifier must also recognise it. If it must, that is a 4th file — instead note it in the ticket comment.
4. Tests, on the real tool stack (build via the same builder functions production uses, temp dir only, no real repo writes):
   - item 3: a child agent's tools read a 600-line file in full; the parent's tools read only page 1 (limit < 600); the parent's `write_file` of that path is REFUSED. Before the change this write was allowed (yoyo's store is process-wide) — run the test on HEAD first and record that it fails.
   - item 4: parent reads page 1 of a 600-line file, the file is rewritten on disk to 300 lines (simulating bash), parent `write_file` is REFUSED.
   - near-miss: parent reads the whole file, writes it — ALLOWED; never-read existing file write — ALLOWED (item 1 stays undecided; pin today's behaviour by name, e.g. `never_read_overwrite_is_still_allowed_item_1_undecided`).
   - positive control: drop the `.with_read_log(..)` on the parent's write tool, run, watch exactly the item-3/item-4 tests go red, restore — as one atomic command, serially.
5. `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
6. Update the docs sentence describing the overwrite guard (grep docs/src for "partial" / "read it whole") to say each agent (and each sub-agent) has its own read record and a file changed on disk after a partial read is refused until read whole again.

## Done / commit
Title `Part of #1094: ...` (item 1 — never-read overwrite — remains an open product decision, so NOT Fixes). Commit body lists items 2/3/4 with their status and which layer enforces each. Post a comment on #1094 whose first 200 characters are the new scope line: "Scope: still open = item 1 (never-read overwrite, undecided); retiring partial_read_guard.rs (needs a truncation story). Done = 2, 3, 4 (sha)."
