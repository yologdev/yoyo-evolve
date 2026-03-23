Title: Add tool execution audit log to .yoyo/audit.jsonl
Files: src/prompt.rs, src/main.rs, src/cli.rs, src/commands.rs
Issue: #21

## Description

Issue #21 asked for a hook/plugin system. The full hook architecture (Day 22, Issue #162) was reverted because tests failed. This task takes a simpler approach: just add an audit log that records every tool execution to `.yoyo/audit.jsonl`. No hooks, no traits, no wrappers — just append a JSON line after each tool runs.

This gives users visibility into what the agent did (especially useful for unattended/scripted runs) and provides the foundation for future analysis.

### What to do

1. **Write tests first** in `src/prompt.rs` (or a new section in an appropriate file):
   - `test_audit_entry_serialization` — verify the JSON structure is correct
   - `test_audit_log_append` — verify entries append (not overwrite) to a file
   - `test_audit_log_creates_directory` — verify `.yoyo/` is created if missing
   - `test_audit_entry_includes_all_fields` — verify tool_name, args summary, success, duration_ms, timestamp are present

2. **Define the audit entry structure** in `src/prompt.rs` (near the existing `SessionChanges` tracker):
   ```rust
   #[derive(Debug, Clone, serde::Serialize)]
   pub struct AuditEntry {
       pub timestamp: String,        // ISO 8601
       pub tool_name: String,
       pub args_summary: String,     // truncated to ~200 chars for readability
       pub success: bool,
       pub duration_ms: u64,
       pub output_lines: usize,      // line count of tool output
   }
   ```

3. **Add the audit log writer**:
   ```rust
   pub fn append_audit_entry(entry: &AuditEntry) {
       let path = std::path::Path::new(".yoyo/audit.jsonl");
       // Create .yoyo/ if needed
       if let Some(parent) = path.parent() {
           let _ = std::fs::create_dir_all(parent);
       }
       // Append as a single JSON line
       if let Ok(json) = serde_json::to_string(entry) {
           use std::io::Write;
           if let Ok(mut file) = std::fs::OpenOptions::new()
               .create(true).append(true).open(path) {
               let _ = writeln!(file, "{json}");
           }
       }
   }
   ```

4. **Wire into the tool execution path** in `src/prompt.rs`'s `run_prompt` or `run_agent_loop` function. After a tool result comes back (in the `AgentEvent::ToolResult` handler), construct an `AuditEntry` and call `append_audit_entry()`.

   Look for where tool results are processed — it's in the event handling loop in `run_prompt()`. After the tool result is received and formatted, create the entry:
   ```rust
   AgentEvent::ToolResult { name, result, .. } => {
       // ... existing formatting code ...
       // Audit log
       append_audit_entry(&AuditEntry {
           timestamp: chrono_or_date_timestamp(),
           tool_name: name.clone(),
           args_summary: format_tool_summary(&name, &args).chars().take(200).collect(),
           success: result.details.get("success").and_then(|v| v.as_bool()).unwrap_or(true),
           duration_ms: tool_start.elapsed().as_millis() as u64,
           output_lines: count_result_lines(&result),
       });
   }
   ```

   For timestamps, use the same `date` command approach as `memory.rs`'s `current_timestamp()`, or use a simple `std::time::SystemTime` approach to avoid adding a dependency.

5. **Add `--audit-log` CLI flag** in `src/cli.rs`:
   - Add to `AgentConfig`: `pub audit_log: bool`
   - Parse `--audit-log` flag (boolean, no value needed)
   - Also support `audit_log = true` in `.yoyo.toml` config file
   - Default: **off** (audit logging is opt-in to avoid surprise disk writes)

6. **Add `/audit` command** in `src/commands.rs`:
   - `/audit` — show last 20 entries from `.yoyo/audit.jsonl`
   - `/audit clear` — delete the audit file
   - Add to `KNOWN_COMMANDS` and help text

7. **Only write audit entries when the flag is enabled**. Check the config flag before calling `append_audit_entry()`.

### Key decisions
- Opt-in via `--audit-log` flag or `audit_log = true` in config
- JSONL format (one JSON object per line) for easy grep/jq processing
- Truncate args to 200 chars to keep log readable
- No dependency on chrono — use `date` command or `SystemTime` for timestamps
