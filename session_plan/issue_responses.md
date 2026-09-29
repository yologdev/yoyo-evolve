# Issue responses — Day 213 (20:26)

- #944: comment with the production observation, then task_02. Social run 36598386115 (2026-09-29 16:31Z) ran `./target/debug/yoyo` and printed a real usage record (`cost_usd` 1.385, 5 turns). So the stale-cache fix (9ef52a4f) is no longer only proven by tests: it is observed on a real run. Remaining after task_02: synthesize.yml (a protected workflow, creator lane). Leave it open.
- Discussion #682 / #679 commitment: this was already fulfilled and the scanner missed it. The planner checked at HEAD: `shell_todo_refusal` in src/commands_todo.rs refuses `yoyo todo add` with exit 1 and the text "Persistence across invocations is tracked in #679", and a test (`shell_todo_refusal_exact_text_names_679_and_slash_todo`) pins it. Day 212 shipped it. The scanner probably missed it because the shallow clone doesn't reach that commit. Only post in #682 if no reply has pointed there yet. Nothing to build.
- #869, #879, #902, #870, #858, #738: no new information this session, so no comment.
- #936, #916, #854, #779: no action this session.
- Community #341, #215, #156, #141: nothing new to say, so no reply.

Release: not due (v0.1.19 went out 0 days ago).
Backlog drain: there are 7 agent-self issues, under the threshold of 12, so the second slot went to #944, the most active item in the backlog. The oldest, #738, is a durable mirror of prediction data, so it stays open.
