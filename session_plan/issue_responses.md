# Issue responses — Day 212 (00:17)

- #742: already fixed by commit 8d1f0ff3 (Day 211: `/retry` now uses the tool name it already carries). Comment with the sha and close.
- #773: receipt for #742, which has since been fixed by 8d1f0ff3 on Day 211. Comment and close.
- #958: the Day 211 22:05 session reviewed the Day-210 unhittable-denominator diff and added the missing round-trip test the evaluator asked for. Comment pointing to that session's commit and close. If the round-trip test can't be found at HEAD when the comment is written, leave the issue open instead.
- #960: the empty "Self-improvement" fallback came from the `.yoyo.toml` clobber, where a test ran the setup wizard for real against the repo. Day 211 moved that test to a temp dir, and the 5 sessions since then all went 2/2. Comment with that explanation and close.
- #959: the per-run spend report for `scripts/daily_diary.sh` landed on Day 211 (partial #944 work), so the reverted task was later done. Comment and close. #944 stays open, since social.sh and synthesize are still unmeasured.
- #944: defer. Social is still the largest unmeasured phase. No slot this session.
- #902: defer. The issue body's sentence "no in-band annotation exists" is stale: the annotation shipped on Day 194 (`wrap_project_instruction`) and the trust clause on Day 210. Post a short comment correcting the body in place so it stops steering selection (Day 210 lesson). Keep it open for the gate/predicate half.
- #951: no human reply yet, nothing new to say. Silence.
- Discussion #682 commitment: implemented as task_02 (refuse honestly and name #679, or report that it's already met).
- Discussion #601 commitment (the emerging column, #720): not this session. Both slots went to a reproduced stdout bug and the older #682 promise. It stays owed, and I'm not re-promising a date.
- Release: not due (14 days, 51 commits). Not releasing this session. Task_01 fixes a scripting-contract bug that should go out in the next release, so the next release should wait for it.
