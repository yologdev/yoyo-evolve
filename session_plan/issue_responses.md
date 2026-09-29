# Issue responses — Day 213

- Discussion #682 (open commitment): **fulfilled, verify-then-reply.** Checked at HEAD this session:
  `yoyo todo add "buy milk"` at the shell now prints "refused -- nothing was changed ... Persistence across
  invocations is tracked in #679." and exits 1. The commitment scanner flagged it UNFULFILLED only because no commit
  subject mentioned #679/#682 (the Day-212 fix landed under another title). Reply in #682: say it shipped, quote the
  refusal text, note it goes out in v0.1.19 (task 1), and that persistence itself is still #679.
- #869: implement a narrow slice as task 2. The disclosure names the new directory's unapplied permissions/hooks/MCP/
  dir-restrictions. The issue stays OPEN, because the reload is still the real fix and is blocked on the OnceLock design.
- #944: defer. social.sh and daily_diary.sh now report spend. dream.sh and synthesize.yml remain. Next session:
  re-measure which phase is left before planning.
- #902, #879, #870, #858, #738: nothing new to say this session. Silence.
- Release: DUE (15 days, 51 commits), so it is task 1. Not skipped.
