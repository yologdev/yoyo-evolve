# Issue Responses

## #172: Suppress partial tool output in piped/CI mode
**Action:** Implement as Task 1. Simple TTY guard on the `ToolExecutionUpdate` handler in `prompt.rs` — skip partial output rendering when stdout isn't a terminal. The final `ToolExecutionEnd` result still shows up, so CI logs stay informative without the ~6500 noise lines.

## #173: Investigate context overflow causing 400 Bad Request from API overflow
**Action:** Implement as Task 2 (partial fix). Reducing `TOOL_OUTPUT_MAX_CHARS` from 30K to 15K in piped mode directly halves the context growth rate from tool outputs. This doesn't solve the fundamental "no mid-turn compaction" problem, but it makes 400 errors much less likely by slowing the growth. Combined with Task 1 (suppressing partial output), this addresses the two biggest contributors to context bloat in evolution sessions.

## #156: Submit yoyo to official coding agent benchmarks
**Action:** No action needed. @yuanhao's comment says "for your information only. No action required." This is a help-wanted issue for community contributors — stays open as-is.

## #175 (self-filed): Task reverted — proactive context management
**Action:** Partially addressed by Task 2 (reduced truncation limit). The full proactive compaction approach (modifying PromptResult enum) was too complex and caused the revert. The simpler approach — reducing tool output size in piped mode — achieves the same goal (preventing 400 errors) without touching internal APIs.

## #170 (self-filed): Task reverted — ast-grep integration
**Action:** Already resolved. `/ast` was successfully built in Day 24's earlier session (07:11). The Day 23 revert was from a different approach; the retry landed clean.

## #167 (self-filed): Task reverted — terminal bell notification
**Action:** Implement as Task 3. The bell infrastructure (`bell_enabled()`, `maybe_ring_bell()`, `--no-bell` flag) was already built in Day 23. What's missing is actually *calling* `maybe_ring_bell()` after prompts complete. This is a 3-line wiring task, not a feature build.

## #164 (self-filed): Task reverted — streaming flush tightening
**Action:** Already resolved. The streaming flush logic was successfully improved in Day 23's 08:40 and Day 24's 07:11 sessions. Contract tests now pin the behavior.

## #162 (self-filed): Task reverted — pre/post hook support
**Action:** Deferred. The full hook system is too complex for reliable implementation right now. The audit log piece could be retried as a standalone feature in a future session, but it's not the highest priority.
