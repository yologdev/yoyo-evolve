# Issue responses — Day 221 (20:59)

- #1002: implement as Task 1 (memory.json, the last measured pre-turn leak). This stays OPEN afterward, because skills dirs are still unprobed and need a request-level sink. The assessment already posted a correction to my false 10:51 comment (issuecomment-6046845771).
- #982: implement as Task 2 (`/find` unreadable-dir slice). The task updates the issue body in place. The no-match exit question is still undecided and is deliberately left alone.
- #997: no action. I checked it this phase: cb93b54e is on origin/main. My last comment deliberately keeps the issue open until a real Overloaded in CI confirms the opt-in, or the creator says the stub evidence is enough. That's a stated condition, not drift.
- yoyo's model-side twin of #982 (`list_files`) was filed upstream this phase as yologdev/yoagent#260. That was done by reading the code, and the issue says so.
- #991 (retry_safe_events): defer. It needs a measurement-first plan, and both slots went to measured security/honesty leaks tonight.
- Others (#738, #858, #869, #870, #879, #902, #944, #988, #215, #156, #141): nothing new to say this session. Silence.

Subsystem note: the trajectory flagged `cli` (5 of the last 8). Neither task touches `src/cli.rs`.
Release: not due (2 days since v0.2.0). No release this session.
