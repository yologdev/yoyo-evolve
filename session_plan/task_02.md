Title: Show session summary when restoring with --continue
Files: src/main.rs, src/commands_session.rs
Issue: none

## Problem

When a user runs `yoyo --continue` (or `-c`), the session is restored with only a terse message:
```
  resumed session: 14 messages from yoyo-session.json
```

This tells the user nothing about what was happening in the previous session. They have to scroll back through their terminal or use `/history` to figure out where they left off. This is a real workflow friction point — you come back to your desk and have no idea what the agent was doing.

## Solution

Enhance the `restore_session()` function in `main.rs` to show a brief summary after restoration:

1. After successfully restoring messages, scan the restored messages to extract:
   - The last user message (truncated to ~80 chars if long)
   - A snippet of the last assistant text response (truncated to ~120 chars)
   - Count of tool calls in the conversation
   - Count of total messages

2. Print a formatted summary block:
```
  📋 resumed session (14 messages, 8 tool calls)
  last prompt: "Can you fix the test failures in commands_map.rs?"
  last reply:  "I found 3 failing tests. The issue was..."
```

### Implementation details:

In `commands_session.rs`, add a new function `fn session_resume_summary(messages: &[AgentMessage]) -> String` that:
- Iterates messages in reverse to find the last User message and last Llm(Assistant) message
- Extracts text content from each (using the existing message content extraction patterns)
- Counts tool_use and tool_result messages
- Returns a formatted multi-line string
- Uses `safe_truncate()` from `format/mod.rs` for safe string truncation

In `main.rs`, call this function after successful restore and print the result.

### Tests:
- Test `session_resume_summary` with empty messages → returns minimal output
- Test with a mix of user/assistant/tool messages → correct counts and extraction
- Test truncation of long messages
- Test with messages that have no text content (only tool calls)
