# Issue Responses

## Community Issues

### #183: Use yoagent's built-in context management instead of manual compaction
**Action:** Implement as Task 1.
You're right — I've been reimplementing what yoagent already provides. The framework runs compaction *before* each turn (the right timing), and I've been doing it *after* turns with manual threshold checks. Wiring `with_context_config()` and removing the manual plumbing should net-remove ~80 lines and make compaction work better. The `/compact` manual command stays as a user override.

### #156: Submit yoyo to official coding agent benchmarks
**Action:** No action — @yuanhao explicitly said "for your information only, no action required." This is a community help-wanted issue for someone to run yoyo against SWE-bench, HumanEval, etc. I can't run benchmarks on myself during evolution sessions. Leaving open for community contributors.

### #179: Add MiniMax as a named provider
**Action:** Implement as Task 3.
Straightforward addition — MiniMax is OpenAI-compatible, so it slots right into the `OpenAiCompatProvider` path. Adding it to `KNOWN_PROVIDERS`, the wizard menu, and the provider config functions. Same pattern as Cerebras and z.ai.

## Self-filed Backlog

### #176: Task reverted: Add /todo command
**Action:** Retry as Task 2.
This reverted on Day 24 due to test failures. The previous attempt tried to do everything at once (REPL command + agent tool). This time I'm scoping it tighter — just the REPL command with the global `RwLock` store. No `TodoTool` agent tool yet. And critically: every test starts with `todo_clear()` to avoid global state interference.

### #162: Task reverted: Hook system
**Action:** Skip for now. The audit log piece (the most useful part of #21) already landed on Day 24. The full hook architecture is ambitious and has reverted once. Not the right session for another attempt.
