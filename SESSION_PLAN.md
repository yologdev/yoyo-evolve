## Session Plan

### Task 1: Add system_prompt and system_file support in .yoyo.toml config
Files: src/cli.rs
Description: Currently `system_prompt` and `system_file` can only be set via CLI flags (`--system`, `--system-file`). Teams want per-project system prompts without passing CLI flags every time. Add two new config file keys:

1. In `parse_args()`, after loading `file_config`, check for `system_prompt` and `system_file` keys:
   - `system_file = "prompts/agent.txt"` — reads the file content as system prompt
   - `system_prompt = "You are a Rust expert..."` — inline system prompt text
   - Priority: `--system-file` CLI > `--system` CLI > `system_file` config > `system_prompt` config > default SYSTEM_PROMPT
   - `system_file` in config resolves relative to the config file's directory (project root for `.yoyo.toml`)

2. Add tests:
   - `test_config_system_prompt_key` — verify `system_prompt` is read from config
   - `test_config_system_file_key` — verify `system_file` is read from config  
   - `test_cli_system_overrides_config` — verify CLI flags take precedence
   - `test_config_system_file_missing` — verify graceful error for nonexistent file

3. Update help text to mention the new config keys in the config file section.

Issue: none

### Task 2: Add tool execution audit log
Files: src/main.rs, src/cli.rs, src/prompt.rs (new: src/audit.rs)
Description: Add a simple audit trail for tool executions that logs to `.yoyo/audit.jsonl`. This is the minimal viable piece from the reverted #162 (hook system was overscoped — just do logging). Each line is a JSON object:

```json
{"ts":"2026-03-23T07:53:00Z","tool":"bash","args_summary":"cargo test","success":true,"duration_ms":1234}
```

Implementation:
1. Create `src/audit.rs` with:
   - `AuditLogger` struct holding an optional `BufWriter<File>` (None when disabled)
   - `AuditLogger::new(enabled: bool) -> Self` — creates `.yoyo/` dir and opens file if enabled
   - `AuditLogger::log_tool_call(tool: &str, args: &serde_json::Value, success: bool, duration: Duration)` — writes one JSONL line
   - `format_args_summary(tool: &str, args: &Value) -> String` — truncated arg summary (reuse `format_tool_summary` from format.rs)
   
2. In `cli.rs`: add `--audit-log` flag parsing and `audit_log = true` config file key. Add to `Config` struct.

3. In `prompt.rs` `run_prompt()`: after each tool execution completes, if audit logger is Some, log the call. The tool result events already have tool name and timing info — tap into the existing event stream.

4. Tests:
   - `test_audit_logger_writes_jsonl` — create logger to temp file, log some calls, verify format
   - `test_audit_logger_disabled_is_noop` — disabled logger doesn't create files
   - `test_audit_args_summary_truncation` — long args get truncated
   - `test_audit_log_flag_parsing` — CLI flag and config file key work

Issue: #162

### Task 3: Tighten streaming latency for numbered list ambiguity and add streaming contract tests
Files: src/format.rs
Description: Issue #147 keeps coming back. The renderer is solid but the `needs_line_buffering()` function is conservative for numbered list detection: `b'0'..=b'9'` buffers until `len() >= 3` to distinguish "1. list item" from "100". This means a line starting with a digit always waits 2-3 tokens. Tighten this:

1. Change the digit case in `needs_line_buffering()`: if the buffer contains a digit followed by a non-digit that isn't `.` or `)`, it's definitely not a numbered list — flush immediately. Example: "2nd" should flush on "2n", not wait for 3 chars. Keep buffering for "1." and "1)" patterns only.

2. Similarly for `-`: if we see `-` followed by a non-space, non-dash char, it's not a list item or HR — flush. Example: "-based" should flush on "-b". Currently waits for 3 chars.

3. Add streaming contract tests that document the expected buffering behavior:
   - `test_streaming_plain_text_flushes_immediately` — "Hello" at line start → immediate output
   - `test_streaming_digit_word_flushes_early` — "2nd" → flushes on "2n" (2 chars, not 3)
   - `test_streaming_dash_word_flushes_early` — "-based" → flushes on "-b"
   - `test_streaming_list_item_buffers_correctly` — "- item" → buffers until "- " confirmed
   - `test_streaming_numbered_list_buffers` — "1. " → buffers until pattern confirmed
   - `test_streaming_fence_buffers` — "```" → buffers until resolved
   - `test_streaming_code_block_content_immediate` — inside code block, content streams immediately
   - `test_streaming_mid_line_always_immediate` — mid-line tokens never buffer

These tests serve as the streaming performance contract — any future change that causes them to fail is a latency regression.

Issue: #147

### Issue Responses
- #156: Acknowledged — @yuanhao said "for your information only, no action required." I'll keep this open as a reference. No work needed this session.
- #147: Task 3 addresses this directly. Tightening digit/dash line-start buffering and adding streaming contract tests to prevent regressions. The renderer is already solid (word-boundary flushing, mid-line fast path, code block streaming) but numbered list ambiguity adds unnecessary delay for prose starting with digits.
- #133: Already substantially addressed — `/rename` (Day 22), `/extract` (Day 22), and `/move` (Day 22) form the refactoring trio requested. `/move` supports both same-file and cross-file method relocation. Will comment on the issue noting the shipped commands.
- #162 (self-filed, reverted hook system): Task 2 takes the simplest useful piece — audit logging — without the full hook architecture that caused test failures. The audit log gives users the visibility they need; hooks can come later on a solid foundation.
