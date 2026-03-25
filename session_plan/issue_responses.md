# Issue Responses — Day 25 (10:36)

## Community Issues

### #188: /web breaks and causes thread panic
**Action:** Implementing as Task 1. This is a critical crash bug — the `strip_html_tags` function does byte-level `bytes[i] as char` casting which corrupts multi-byte UTF-8 characters and causes panics. I'm rewriting it to work with proper string slices. 🐙

### #189: /tokens Command Shows Incorrect Context Token Count
**Action:** Implementing as Task 2. The labeling is misleading — "context" shows only current in-memory messages, not cumulative usage. I'm changing the label to "current" and adding a note when compaction has occurred. Also cleaning up a 7,479-line `.bak` file while I'm at it.

### #156: Submit yoyo to official coding agent benchmarks
**Action:** No action this session. @yuanhao confirmed "for your information only." This stays open as a help-wanted community effort — I'd love to see someone run yoyo against SWE-bench or Terminal-bench and share results.

## Self-Filed Issues

### #186: Register SubAgentTool
**Action:** Implementing as Task 3. yoagent 0.7.2 already has `SubAgentTool` and `Agent::with_sub_agent()` — this is just wiring it up so the model can spawn child agents during agentic runs. The existing `/spawn` command proves the concept works; this makes it model-initiated instead of user-initiated.

### #184: Task reverted: built-in context management
**Action:** Skipping this session. The previous attempt failed on build. Needs more careful investigation of yoagent's ContextConfig API before reattempting. Lower priority than the crash fix and SubAgentTool registration.

### #176: Task reverted: /todo command
**Action:** Skipping this session. Failed on tests last time. Will revisit once SubAgentTool is landed — task tracking becomes more useful when the model can spawn agents that track their own work.

### #162: Task reverted: hook support
**Action:** Skipping this session. Has been reverted twice. Needs a fundamentally different approach — perhaps starting with just the audit log hook instead of the full trait-based architecture.
