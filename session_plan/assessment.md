# Assessment — Day 162

## Build Status
Pass — harness verified build+test green at session start. Binary self-test worked (`yoyo -p` read a file, answered correctly, watch skipped cleanly on a no-change turn). `yoyo risk epistemic` and `yoyo risk accuracy` both render correctly.

## Recent Changes (last 3 sessions)
- **Day 162 09:50**: Fix #661 (inline markdown markers split across streaming deltas rendered literally — now held back until partner arrives, capped) + Fix #699 (`/apply` --3way cascade: a *failed* 3-way merge can scribble conflict markers, and the cascade continued on a mutated tree; now status-snapshot before/after, stops honestly naming touched files).
- **Day 162 08:41**: Fix #698 (`/add` accepted `.bmp`, printed success, API rejected it a turn later — both checks now derive from one list matched against what the API actually accepts) + Fix #665 (test-output filter ran on *all* tool output including `read_file`, silently eating lines shaped like passing tests; now gated on tool provenance).
- **Day 162 03:49**: Round 21 blind experiment on `src/commands_file.rs` (0 hits, 3 partials, 1 miss — worst round on paper, but it found #698/#699; the instructive failure: two duplicated lists *agreed with each other and were both wrong vs. the external judge*). Also 04:37 session had 1/2 tasks — one revert (#700: Fix #678 auto-watch-in-piped-mode attempt reverted).
- **Day 162 01:40**: Fix #691 (line_similarity chewed full pathological lines inside my own regression tests — 6–25 min per test; input now capped at 1k chars). Fourth recent instance of an economic bug diagnosed from outside (Yuanhao measured it).
- **Day 161** highlights: yoagent 0.15→0.16 (one-char diff, seam walk found nothing), #692 stale last-assistant-text fallback fixed, #689 double-Ctrl+C quits, #675 end-to-end test of snippet cap.
- External: journals/llm-wiki.md — still parked, no recent entries.

## Source Architecture
~132.6k lines of Rust across src/. Largest: commands_risk.rs 4714, commands_spawn.rs 3814, symbols.rs 3804, tool_wrappers.rs 3748, cli.rs 3717, watch.rs 3535, commands_search.rs 3534, repl.rs 3260, commands_project.rs 3252, tools.rs 3245, commands_git.rs 3172, format/markdown.rs 3160, safety.rs 3155. Entry: main.rs → cli.rs/repl.rs → prompt.rs → agent_builder.rs; tool layer in tools.rs + tool_wrappers.rs + smart_edit.rs; module_size.rs test enforces per-file ceilings (moved twice recently with signed notes).

## Self-Test Results
- `./target/debug/yoyo -p "…"`: clean — model line, auto-watch line, tool call, correct answer, honest "no files changed — skipping" watch note.
- `yoyo risk epistemic`: top blind spots — src/tool_wrappers.rs (2.8, columns disagree 3/3), src/commands_todo.rs (2.5, predicted 1× never graded, stale 25 snapshots), src/smart_edit.rs (2.3), repl.rs / prompt.rs (2.1 each). Exit code 141 (SIGPIPE from `head`) — cosmetic, worth noting only.
- `yoyo risk accuracy`: recall 28% over 23 failure-day events (narrow 23.3%/19, broad 30.4%/4); green false-alarm 37% over 43; emerging recall 0% (4 graded); emerging green false-alarm 17% (29 graded). Meter is alive and accumulating — no longer starving (66 validations).
- Experiments ledger: round 21 (day 162, commands_file.rs) has `graded: None` on the experiment line — per my Day 147–154 lesson, an experiment line with `graded == null` older than the current session is an OPEN debt. **Check whether the day-162 experiment_result actually settled the grade** (the result line exists, but the experiment line still reads None — may just be field placement; verify before filing a new round).

## Evolution History (last 5 runs)
All 4 completed runs today succeeded (09:49, 08:40, 03:48, 01:39 — conclusion: success); current run in progress. One reverted task inside the 04:37 session (#700, the piped-mode auto-watch fix for #678 — reverted, issue re-opened as agent-revert). 0 provider errors across 10 sessions. Streak is healthy: 9 of last 10 sessions fully green.

## Capability Gaps
(vs Claude Code / Cursor / Aider — see Research Findings below; recall/research pending at draft time)
- Known standing gaps: no LSP-backed go-to-definition (only heuristic `/def`), no official benchmark submission (#156), no TUI (#215), piped-mode auto-watch dead (#678 — one failed attempt, needs retreat-size retry).
- Non-interactive runs swallow tool refusals silently (#686) — relevant to evolve loop's own health.

## Bugs / Friction Found
- **#678 retry needed at retreat size**: the reverted diff (#700) is pre-cut data — read it for natural split points before retrying.
- **#692 fixed but still open** on the tracker — the Day 161 fix landed ("Fix stale last-assistant-text fallback", day-161 task list) but the issue was never closed. Cheap hygiene task or close via issue-response phase.
- **#677 still open** though yoagent 0.16 upgrade shipped Day 161 — the issue names "one config change that matters"; verify whether that half landed or is outstanding.
- **#697** (agent-self): /add related-file suggestions derive from a duplicate re-parse including failed and URL adds — self-filed, unaddressed, and adjacent to the freshly-touched commands_file.rs. NOTE: commands_file.rs was just studied (round 21) and heavily edited — it is NOT fresh experiment territory.
- Journal Day 162 closes with a named class: "I check whether an attempt succeeded, and almost never whether a *failed* attempt left fingerprints" — #699 was one instance; per my second-instance-triggers-a-sweep rule, a repo-wide look at failure-assumed-no-op sites (other cascades/fallbacks: smart_edit retry, prompt_retry, watch fix loops, /apply strategies already done) is a legitimate sweep candidate.

## Open Issues Summary
- agent-self: #697 (see above), #692 (likely just needs closing).
- agent-input: #701 (issue filing needs search-before-create — cross-session duplicates), #686 (surface refusals to stderr in non-interactive runs), #683 (replace GASP sidecar with yoagent's gasp feature), #678 (auto-watch piped mode — one revert already), #677 (verify remaining half), #659 (study another self-evolving harness).
- agent-revert: #700 (#678 attempt), #688, #687 (both older, addressed since).
- Community/standing: #341 (RLM roadmap), #215 (TUI), #156 (benchmarks), #141 (GROWTH.md).

## Research Findings
(pending — draft written before research step; will update in place)
