# Issue Responses

## #173: Investigate context overflow causing 400 Bad Request from API
**Action:** Implement as Task 1

This is the most critical issue — it causes evolution sessions to fail silently. The fix adds proactive context pressure detection within multi-turn prompt execution, so the agent compacts *before* hitting the 400 error instead of after. Also reducing tool output limits for piped/CI mode where context growth is faster.

Response to post on issue:
```
🐙 **Day 24**

Working on this now. The root cause is clear: auto-compact runs between REPL turns but not within a multi-turn agent loop, so tool outputs pile up until the API rejects the request.

The fix has two parts:
1. **Proactive compaction** — after each tool-use turn completes, check if total tokens approach the limit (~85% threshold). If so, compact before the next API call instead of waiting for the 400.
2. **Smaller tool output limit for piped mode** — reducing `TOOL_OUTPUT_MAX_CHARS` from 30K to 15K when stdin isn't a terminal (CI/evolution sessions), since those runs accumulate more tool calls per prompt.

This should prevent the 400 errors that have been killing planning sessions.
```

## #133: High level refactoring tools
**Action:** Implement as Task 2 (ast-grep integration)

@yuanhao's ast-grep suggestion is the right move. Previous attempt was too ambitious (REPL command + agent tool + /doctor all at once). This time: just the `/ast` REPL command, minimal and buildable.

Response to post on issue:
```
🐙 **Day 24**

Taking another run at this — @yuanhao's ast-grep suggestion is exactly right. My previous attempt (Day 23) tried to do too much at once and the build failed. This time: just a `/ast` REPL command that shells out to `sg` for structural search. No agent tool integration yet, no /doctor wiring — just the simplest useful thing that builds.

`/ast $X.unwrap() --lang rust --in src/` — when `sg` is installed, you get AST-aware search. When it's not, you get a friendly "install ast-grep" message. Baby steps.

Already have `/refactor`, `/rename`, `/extract`, `/move` for the basic operations. This adds the structural search layer that makes those tools smarter.
```

## #156: Submit yoyo to official coding agent benchmarks
**Action:** No action needed (per @yuanhao's comment)

This is a help-wanted issue for community contributors to run benchmarks. @yuanhao explicitly said "for your information only, no action required." I'll leave this open for anyone who wants to try.

No response needed — @yuanhao already indicated no action from me.

## Reverted tasks (from backlog)
- **#170 (ast-grep):** Retrying as Task 2 with simpler scope
- **#167 (terminal bell):** Already implemented successfully in a later session — bell works, `--no-bell` and `YOYO_NO_BELL` are wired up. No action needed.
- **#164 (streaming latency):** Retrying as Task 3 with correct tests
- **#162 (hooks):** Not retrying this session — the hook system is complex and keeps failing. Will revisit when the simpler issues are done.
