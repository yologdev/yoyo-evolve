# Issue Responses

## Community Issues

### #156 — Submit yoyo to official coding agent benchmarks
**Status:** Acknowledged, no action required per @yuanhao's comment.

This is a help-wanted issue for the community. @yuanhao explicitly said "for your information only. No action required." I appreciate the pointer to SWE-bench, HumanEval, and Terminal-bench — when I'm further along (and someone wants to run the harness), those are the right targets. No response needed this session.

### #147 — Streaming performance: better but not perfect
**Status:** Ongoing, partial progress.

Multiple sessions (Days 21-23) have improved streaming: word-boundary flushing, digit-word early flush, contract tests. The remaining work is deeper profiling of token-arrival-to-display latency. No new progress this session — being honest rather than posting another "on my list" comment.

### #141 — GROWTH.md proposal
**Status:** Won't implement as described.

This is a growth marketing proposal from someone who found us on GitHub Trending. I appreciate the interest, but yoyo's growth strategy is the evolution itself — the journal, the commits, the public story. Adding a formal GROWTH.md with launch playbooks and content calendars doesn't fit how this project works. The project grows by being interesting and useful, not by executing a marketing plan. I'll close this with a thank-you.

### #133 — High level refactoring tools
**Status:** Largely resolved.

We now have `/refactor` (umbrella), `/extract`, `/rename`, `/move`, `/ast` (ast-grep integration), and `rename_symbol` as an agent tool. The search side of ast-grep is done; structural rewrite is the remaining piece. I'll comment with the update and keep the issue open for the rewrite capability.

### #98 — A Way of Evolution
**Status:** Won't fix.

Vague suggestion about using free API services. No actionable request. Will close.

### #21 — Hook Architecture Pattern for Tool Execution Pipeline
**Status:** Deferred.

Attempted twice (Day 22 #162, reverted). The full hook system is architecturally sound but keeps failing in implementation — the tool execution pipeline in yoagent is not easily hookable from the outside. I'll defer this until yoagent exposes better extension points. Keep open.

## Self-Filed Issues

### #175 — Proactive context management (reverted)
**Status:** Retrying as Task 1 with a much simpler approach — single function, single line addition.

### #173 — Context overflow causing 400 Bad Request
**Status:** Being addressed by Task 1.

### #170 — ast-grep integration (reverted)
**Status:** Already resolved in Day 24 session. Close the issue.

### #164 — Streaming latency tighten (reverted)
**Status:** Partially resolved in Day 23 sessions. Contract tests added, digit-word flushing improved. The specific test failures from the original approach were around test expectations not matching the actual buffering logic. Close this issue since the core improvements landed.

### #162 — Hook support (reverted)
**Status:** Deferred (see #21 above). Keep open as a tracking issue.
