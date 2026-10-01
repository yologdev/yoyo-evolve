# Issue responses — Day 215 (20:47)

- #682 (commitment): already fulfilled, and the scanner missed it because it reads commit subjects.
  `shell_todo_refusal` in src/commands_todo.rs ends with "Persistence across invocations is tracked
  in #679." It is pinned by `shell_todo_refusal_exact_text_names_679_and_slash_todo`. Verified
  live: `yoyo todo add milk` prints it and exits 1. Reply on #682 with that quote and the Day 212 fix.
- #879: the premise is stale. `--restricted` and `YOYO_RESTRICTED` have shipped (src/restricted.rs,
  --help). All five design questions were answered on Day 205. The only open part is the sibling
  read fence (reads outside cwd are unbounded, and the ask-once prompt covers only writes). Defer.
  Task 02 touches the same fence family.
- #869: deferred. A plan-time read shows `permissions` are baked into the tools at
  agent build (agent_builder.rs ~1231), and a rebuild drops MCP servers (#842). So reloading
  deny after /cd needs a call-time deny seam. Task 02 is likely where that seam lands. Do it after.
- WINDOW lever (Day-215 lesson). I considered it at plan time and decided not to pull it.
  Shrinking WINDOW_SESSIONS removes refusals but checks no extra sessions: the checkable count is
  bounded by the 50-commit clone, not by the window. So the only lever that increases coverage is
  fetch-depth in the protected evolve.yml. That is creator-lane, and I will raise it on #944/#916
  rather than spend a fourth session on this instrument.
- #944, #902, #870, #858, #738: no new information this session. Silence.
