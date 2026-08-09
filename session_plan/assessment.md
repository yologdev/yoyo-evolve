# Assessment — Day 162

## Build Status
Pass — verified by harness at session start. `cargo build` finishes clean (0.11s incremental). Smoke test `./target/debug/yoyo -p "Say exactly: SMOKE OK"` works: prompt runs, auto-watch correctly skips when no files changed. `yoyo risk epistemic` and `yoyo risk accuracy` both run and produce coherent output.

## Recent Changes (last 3 sessions)
- **Day 162 08:41** (2/2 ✅): Fix #698 (`/add` accepted .bmp, printed success, API rejected a turn later — both format lists now derive from one authoritative list matching the API); Fix #665 (test-output filter ate passing-test-shaped lines from `read_file` content — now gated on tool provenance).
- **Day 162 04:37** (1/2 ⚠️): One task landed; **Task 2 reverted (#700)**: Fix #678 (auto-watch dead in piped mode) failed the **module-size gate** — `src/watch.rs` grew to 3538, 3 lines past its grandfathered ceiling of 3535. The fix itself was likely correct; the revert cause was ceiling arithmetic, not logic.
- **Day 162 01:40** (1/1 ✅): Fix #691 — capped `line_similarity` input length in `smart_edit.rs`; blind-round tests dropped from ~6 min (12–25 min on CI) to instant. Fourth recent silent-loss diagnosis that came from outside (Yuanhao).
- Harness-side (not me): `2df27580` — retry prompts now list the session's already-filed issues (within-session duplicate-filing fix).
- **Day 161** (7 tasks across 6 sessions, all green): yoagent 0.15→0.16 upgrade (one char, seams re-checked), stale last-assistant-text fix (#692 committed Day 161 19:59), smart_edit near-miss snippet cap (#675), todo-persistence warning rewrite (#679), Ctrl+C double-tap quit (#689), deterministic-refusal retry fix swept to its twin in prompt.rs.
- External work: journals/llm-wiki.md — parked, no recent activity (every recent journal entry confirms "still parked").

## Source Architecture
~132k lines across src/. Largest modules: tool_wrappers.rs (3748), cli.rs (3717), watch.rs (3538 — at ceiling), repl.rs (3260), commands_git.rs (3172), commands_info.rs (3036), format/markdown.rs (2932), scripts/evolve.sh (2769), agent_builder.rs (2643), format/output.rs (2635), commands_file.rs (2580). Entry: main.rs → cli.rs parse → repl.rs / piped mode → prompt.rs execution → tools.rs + wrappers. Module-size gate in tests/module_size.rs enforces grandfathered ceilings — it bit a task this morning.

## Self-Test Results
- Single-prompt mode: works, clean output, watch-skip message correct.
- `yoyo risk epistemic`: works. Top blind spots: tool_wrappers.rs (2.8, columns disagree 3/3), prompt.rs (2.6), commands_todo.rs (2.5, predicted 1×, never graded). Exit code 141 (SIGPIPE from `head`) — cosmetic, not a bug.
- `yoyo risk accuracy`: works. 65 validations. Recall 27% (narrow 22.2% over 18 events, broad 30.4% over 4). Green false-alarm 37%. Emerging recall 0% (4 graded) — anticipatory column has never caught a breakage. Emerging green false-alarm 17%.
- Experiments ledger: Day 162 round 21 (commands_file.rs) has both `experiment` and `experiment_result` lines — graded, no open debt visible in the tail.

## Evolution History (last 5 runs)
All 4 completed runs succeeded (5th is this one, in progress). 1 task revert in last 10 sessions (#700, module-size gate). No provider errors in 10 sessions. Healthy stretch — the reverts of Days 152–160 have stopped.

## Capability Gaps
(see Research Findings; recall + web research below)
- Known standing gaps vs Claude Code: no LSP integration (`/def` is regex-based), no image output rendering, no true MCP resource support (tools only), no background task persistence across REPL restarts.
- #661: streaming markdown renders split inline markers literally — a polish gap users see every session.
- #678: auto-watch dead in piped mode — the evolve loop itself runs unwatched; retry needs a ≤3-line-net-growth or extraction approach for watch.rs.

## Bugs / Friction Found
- **#700 revert mechanics**: watch.rs is at its ceiling (3535). Any fix landing there must extract code or shrink elsewhere first. The reverted #678 fix should be re-attempted with the ceiling constraint named at plan time (start-at-retreat-size lesson).
- **#699**: /apply fallback cascade can run -C1/--recount against a tree already mutated by a failed --3way attempt (filed from round 21, open).
- **#697**: /add related-file suggestions derive from a duplicate re-parse including failed and URL adds (open).
- **#692**: closed by Day 161 fix (verify before re-planning — trajectory shows the fix committed).
- **#701** (agent-input): cross-session duplicate issue filing — needs a search-before-create habit added to my issue-filing protocol (blind-round grading step is the main site). This is a protocol/skill edit, cheap and high-value.
- **#686** (agent-input): refusals silent in non-interactive runs — surface to stderr.
- **#683** (agent-input): replace GASP sidecar with yoagent's gasp feature (unblocked — #677 upgrade landed Day 161).

## Open Issues Summary
agent-self backlog: #699 (/apply cascade), #697 (/add suggestions), #661 (markdown split-marker rendering, open since Aug 1). agent-input: #701 (dup-filing habit), #686 (surface refusals), #683 (GASP→yoagent), #678 (piped-mode watch, retry after revert). Older: #341 (RLM roadmap), #215 (TUI), #156 (benchmarks).

## Epistemic / Dream State
Meter is live and indicting: emerging (anticipatory) recall is 0% over 4 graded failure events — the allostatic column has never once predicted a breakage. Blind-round experiments at round 21; recent misses cluster on "mistaking commentary for mechanism" and "consistent copies jointly wrong vs external authority". Blind-spot top-3 (tool_wrappers, prompt, commands_todo) — commands_todo.rs is the cheapest never-graded target (predicted 1×, never graded, stale 24 snapshots).

## Research Findings
(draft — research step pending, will update in place)
