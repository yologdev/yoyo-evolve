Title: Add TodoTool — agent-accessible task tracking during autonomous runs
Files: src/main.rs, src/help.rs
Issue: #176

## Why this matters

yoyo has a working `/todo` REPL command (in commands_project.rs) but the AI model can't use it as a tool during autonomous operation. Claude Code has `TodoRead`/`TodoWrite` tools that let the model track multi-step plans. Without a TodoTool, yoyo's model loses track of complex plans on long sessions.

## CRITICAL: Why previous 3 attempts failed

All three previous attempts (Issues #176, #198) used **WRONG trait method signatures** from outdated API assumptions:
1. Used `parameters()` — correct method is `parameters_schema()`
2. Used `fn execute(&self, args: &serde_json::Value, _cx: &yoagent::tools::ToolContext) -> Result<String, String>` — correct signature is `async fn execute(&self, params: serde_json::Value, ctx: yoagent::types::ToolContext) -> Result<ToolResult, ToolError>`
3. Used `ToolContext::default()` in tests — ToolContext has NO Default impl (it has required fields: tool_call_id, tool_name, cancel)
4. The return type is `Result<ToolResult, ToolError>`, not `Result<String, String>`

The correct yoagent `AgentTool` trait (v0.7.4) is:
```rust
#[async_trait::async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn label(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, params: serde_json::Value, ctx: ToolContext) -> Result<ToolResult, ToolError>;
}
```

Where:
- `ToolResult` = `{ content: Vec<Content>, details: serde_json::Value }`
- `Content::Text { text: String }` is the text variant
- `ToolError::InvalidArgs(String)` for bad args, `ToolError::Failed(String)` for execution errors
- `ToolContext` has fields: `tool_call_id: String`, `tool_name: String`, `cancel: CancellationToken`, `on_update: Option<...>`, `on_progress: Option<...>`

## Implementation

### Step 1: Add TodoTool struct in `src/main.rs`

Place it after the `AskUserTool` implementation (around line 820). Follow AskUserTool's pattern exactly:

```rust
// ── todo agent tool ──────────────────────────────────────────────────────

/// Agent tool for managing a task list during complex multi-step operations.
pub struct TodoTool;

#[async_trait::async_trait]
impl AgentTool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn label(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Manage a task list to track progress on complex multi-step operations. \
         Use this to plan work, check off completed steps, and see what's remaining. \
         Available actions: list, add, done, wip, remove, clear."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "add", "done", "wip", "remove", "clear"],
                    "description": "Action: list (show all), add (create task), done (mark complete), wip (mark in-progress), remove (delete task), clear (delete all)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (required for 'add')"
                },
                "id": {
                    "type": "integer",
                    "description": "Task ID number (required for 'done', 'wip', 'remove')"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: yoagent::types::ToolContext,
    ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
        use yoagent::types::{Content, ToolError, ToolResult as TR};

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("Missing required 'action' parameter".into()))?;

        let text = match action {
            "list" => {
                let items = commands_project::todo_list();
                if items.is_empty() {
                    "No tasks. Use action 'add' to create one.".to_string()
                } else {
                    commands_project::format_todo_list(&items)
                }
            }
            "add" => {
                let desc = params
                    .get("description")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArgs("Missing 'description' for add action".into()))?;
                let id = commands_project::todo_add(desc);
                format!("Added task #{id}: {desc}")
            }
            "done" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ToolError::InvalidArgs("Missing 'id' for done action".into()))? as usize;
                commands_project::todo_update(id, commands_project::TodoStatus::Done)
                    .map_err(|e| ToolError::Failed(e))?;
                format!("Task #{id} marked as done ✓")
            }
            "wip" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ToolError::InvalidArgs("Missing 'id' for wip action".into()))? as usize;
                commands_project::todo_update(id, commands_project::TodoStatus::InProgress)
                    .map_err(|e| ToolError::Failed(e))?;
                format!("Task #{id} marked as in-progress")
            }
            "remove" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ToolError::InvalidArgs("Missing 'id' for remove action".into()))? as usize;
                let item = commands_project::todo_remove(id)
                    .map_err(|e| ToolError::Failed(e))?;
                format!("Removed task #{id}: {}", item.description)
            }
            "clear" => {
                commands_project::todo_clear();
                "All tasks cleared.".to_string()
            }
            other => {
                return Err(ToolError::InvalidArgs(
                    format!("Unknown action '{other}'. Use: list, add, done, wip, remove, clear")
                ));
            }
        };

        Ok(TR {
            content: vec![Content::Text { text }],
            details: serde_json::Value::Null,
        })
    }
}
```

### Step 2: Add TodoTool to `build_tools()`

After the AskUserTool block (around line 941-943), add:

```rust
// TodoTool is always available — it only modifies in-memory state, not filesystem
tools.push(Box::new(TodoTool));
```

This goes AFTER the `if std::io::stdin().is_terminal()` block for AskUserTool, so TodoTool is always available (even in piped mode).

### Step 3: Update tool count tests

This is CRITICAL — search for ALL tests that assert on `tools.len()` and update them:

Current expected counts (7 tools in non-terminal test environment):
- `test_build_tools_returns_six_tools` (line ~1584) — expects 7, change to 8
- `test_build_tools_count_unchanged_with_sub_agent` (line ~1626) — expects 7, change to 8
- `test_build_tools_auto_approve_skips_confirmation` (line ~1978) — expects 7, change to 8
- `test_build_tools_no_approve_includes_confirmation` (line ~1993) — expects 7, change to 8
- `test_build_tools_with_piped_limit` (line ~2646) — expects 7, change to 8

Also update any test name/comment that says "six" or "seven". The function name `test_build_tools_returns_six_tools` should be renamed to `test_build_tools_returns_eight_tools` (or just verify the count inline).

### Step 4: Add TodoTool tests

Use the existing `test_tool_context(None)` helper (line ~2305) for creating ToolContext in tests. This is the correct way — NOT `ToolContext::default()`.

```rust
#[test]
fn test_todo_tool_schema() {
    let tool = TodoTool;
    assert_eq!(tool.name(), "todo");
    assert_eq!(tool.label(), "todo");
    let schema = tool.parameters_schema();
    assert!(schema["properties"]["action"].is_object());
    assert!(schema["properties"]["description"].is_object());
    assert!(schema["properties"]["id"].is_object());
}

#[tokio::test]
async fn test_todo_tool_list_empty() {
    commands_project::todo_clear();
    let tool = TodoTool;
    let ctx = test_tool_context(None);
    let result = tool.execute(serde_json::json!({"action": "list"}), ctx).await;
    assert!(result.is_ok());
    let text = match &result.unwrap().content[0] {
        yoagent::types::Content::Text { text } => text.clone(),
        _ => panic!("Expected text content"),
    };
    assert!(text.contains("No tasks"));
}

#[tokio::test]
async fn test_todo_tool_add_and_list() {
    commands_project::todo_clear();
    let tool = TodoTool;
    
    let ctx = test_tool_context(None);
    let result = tool.execute(serde_json::json!({"action": "add", "description": "Write tests"}), ctx).await;
    assert!(result.is_ok());
    
    let ctx = test_tool_context(None);
    let result = tool.execute(serde_json::json!({"action": "list"}), ctx).await;
    let text = match &result.unwrap().content[0] {
        yoagent::types::Content::Text { text } => text.clone(),
        _ => panic!("Expected text content"),
    };
    assert!(text.contains("Write tests"));
}

#[tokio::test]
async fn test_todo_tool_done() {
    commands_project::todo_clear();
    let tool = TodoTool;
    let ctx = test_tool_context(None);
    tool.execute(serde_json::json!({"action": "add", "description": "Task A"}), ctx).await.unwrap();
    
    let ctx = test_tool_context(None);
    let result = tool.execute(serde_json::json!({"action": "done", "id": 1}), ctx).await;
    let text = match &result.unwrap().content[0] {
        yoagent::types::Content::Text { text } => text.clone(),
        _ => panic!("Expected text content"),
    };
    assert!(text.contains("done ✓"));
}

#[tokio::test]
async fn test_todo_tool_invalid_action() {
    let tool = TodoTool;
    let ctx = test_tool_context(None);
    let result = tool.execute(serde_json::json!({"action": "explode"}), ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_todo_tool_missing_description() {
    let tool = TodoTool;
    let ctx = test_tool_context(None);
    let result = tool.execute(serde_json::json!({"action": "add"}), ctx).await;
    assert!(result.is_err());
}

#[test]
fn test_todo_tool_in_build_tools() {
    let perms = cli::PermissionConfig::default();
    let dirs = cli::DirectoryRestrictions::default();
    let tools = build_tools(true, &perms, &dirs, TOOL_OUTPUT_MAX_CHARS);
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(names.contains(&"todo"), "build_tools should include todo, got: {names:?}");
}
```

NOTE: Each async test that modifies the TODO_LIST must call `commands_project::todo_clear()` at the start. Tests run in parallel and share the global state.

### Step 5: Update help

In `src/help.rs`, find the `/todo` help entry and add a note about the agent tool:
```
The AI agent can also manage tasks via the todo tool during
agentic runs, helping it stay organized on multi-step operations.
```

### Step 6: Verify

Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.

If ANY test fails on tool count, grep ALL `tools.len()` assertions and fix them.
