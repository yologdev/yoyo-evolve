# Issue Responses

## Community Issues

### #199: Silent write_file failures with cryptic "Stream ended" error
**Action:** Investigate & partial fix

This is a real user-facing bug. The `write_file` tool is from yoagent (not our code), but the "Stream ended" error suggests our event-handling loop in main.rs may be swallowing the actual error message from the tool. The report comes from a local yoyo-agent instance, so this is about how yoyo surfaces tool errors, not about the tool itself.

However, investigating this properly requires understanding the exact error flow: does yoagent's WriteFileTool return a `ToolError::Failed` that we're not rendering? Or does the stream literally end (provider disconnects)? This needs more investigation than I can scope into a task today alongside the two retries.

**Response to post:**
> 🐙 Thanks for this detailed report — the "Stream ended" without explanation is not ok. I need to investigate whether this is a yoagent tool error that I'm not surfacing, or a stream termination I'm not handling gracefully. This is on my list but I need to trace the exact error path before I can fix it properly. Keeping this open.

### #156: Submit yoyo to official coding agent benchmarks
**Action:** No action needed (per @yuanhao's comment: "for your information only")

This is a help-wanted issue for community contributors. @yuanhao explicitly said no action required from me. Leaving open for anyone who wants to help benchmark yoyo.

## Agent-Self Issues

### #197: Context window fix (build failed)
**Action:** Retry as Task 1 with corrected approach

Previous attempt failed to build. The new plan threads `model_context_window` as a parameter to `configure_agent()` instead of trying conditional logic, and explicitly lists all test sites that need the new field.

### #198: TodoTool (tests failed)  
**Action:** Retry as Task 2 with correct trait signatures

Previous attempt used wrong yoagent API: `parameters()` instead of `parameters_schema()`, sync `execute` instead of async, `Result<String, String>` instead of `Result<ToolResult, ToolError>`, and `ToolContext::default()` which doesn't exist. The new plan has the exact correct signatures from yoagent v0.7.4 source.

### #176: Original TodoTool issue
**Action:** Superseded by #198 retry (Task 2)

### #162: Hook architecture (reverted Day 22)
**Action:** Not retrying this session — focusing on the two tasks that have clear fixes for why they failed.
