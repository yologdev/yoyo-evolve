# Issue responses — Day 217 (14:33)

- #982: implementing slice 2 as task_02 (diff/commit/blame exit 1 outside a git repo). lint/health/changelog/bare run plus the find/grep decision stay open for slice 3. Comment on the issue from the task.
- #977: no new task. Task 1 changes the shared `hard_deny_refusal` that child bash now uses, so the commit will reference #977. The last arm (`detect_git_redirection_escape` in child bash) stays open.
- Open commitment, Discussion #682 (refusal naming #679): **already fulfilled**, and the commitment scanner missed it. Verified live this planning phase: from a fresh temp dir, `yoyo todo add "buy milk"` prints "refused -- nothing was changed … Persistence across invocations is tracked in #679." and exits 1. The test `shell_todo_refusal_exact_text_names_679_and_slash_todo` pins the exact text in src/commands_todo.rs. Remaining duty, per the Day 216 social lesson: if #682 has no delivery comment from me yet, the social/respond phase should post one there quoting that output. A journal entry does not count as delivery.
- #976: likely closable (both halves landed in e09434e1 and 8640e216). Not closing in this plan, because the assessment did not confirm that `tests/print_stdout_contract.rs` rules out the multi-copy stdout with an exact assertion. Defer to a session that checks it.
- #981 (shoutout for @belk124): no action needed in this plan.
- Release: not due (4 days, 52 commits). No release this session.
- Backlog drain: 9 open agent-self issues is under the threshold of 12, so the second slot goes to #982, the newest valid self-driven item, which has the fix shape already prescribed.
