Title: Add simple tool execution audit log (--audit-log flag)
Files: src/main.rs, src/cli.rs, src/format.rs
Issue: #21

## Context
Issue #21 has been open for a long time asking for tool execution hooks. The full hook system (Issue #162) was tried and reverted because it was overengineered. This task takes the simplest useful slice: an append-only audit log.

## What to build

### 1. Add `--audit-log` CLI flag in `src/cli.rs`
- Add `audit_log: bool` to the `AgentConfig` struct (or equivalent config holder)
- Parse `--audit-log` flag. When present, enables tool execution logging to `.yoyo/audit.jsonl`
- Also support `audit_log = true` in `.yoyo.toml` config file

### 2. Implement audit logging in `src/main.rs`
The logging should happen in the event handling loop where tool executions are processed. Specifically, look for where `ToolResult` events are handled (the match arms for `AgentEvent::ToolResult` or similar).

For each tool execution, append one JSON line to `.yoyo/audit.jsonl`:
```json
{"ts":"2026-03-23T15:49:00Z","tool":"bash","args":{"command":"cargo test"},"success":true,"duration_ms":1234}
```

Implementation approach:
- Create a helper function `append_audit_log(tool_name: &str, args: &serde_json::Value, success: bool, duration_ms: u64)` 
- Call it after each tool result is received
- Use `OpenOptions::append(true).create(true)` for the file
- Ensure `.yoyo/` directory exists before writing
- If writing fails, print a dim warning but don't crash
- The audit log is best-effort — never block the agent loop

### 3. Add `/audit` command to view recent audit entries
- Parse the last N entries (default 10) from `.yoyo/audit.jsonl`
- Display them in a human-readable format: timestamp, tool, summary, success/fail, duration
- Add to KNOWN_COMMANDS and help text

### 4. Tests
- Test the audit log append function with a temp file
- Test parsing audit log entries
- Test that the `/audit` command appears in help
- Integration test: verify `--audit-log` flag is accepted without error

Keep it simple. No hook traits, no pre/post system, no interceptors. Just logging.
