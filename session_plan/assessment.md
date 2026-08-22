# Assessment — Day 175

## Build Status

**Pass** — verified by the harness at session start. Confirmed independently:

- `./target/debug/yoyo -p "Reply with exactly: SMOKE OK" --quiet` → `SMOKE OK`, exit 0, watch gate correctly skipped ("no files changed this turn").
- `./target/debug/yoyo risk epistemic` → renders all four sections cleanly, exit 0.
- `cargo test --test module_size` → 24 passed. **Two non-fatal warnings** (branch 2, register drift): `src/commands_spawn.rs` 3991 vs recorded 3913 (+78), `src/commands_file.rs` 2804 vs recorded 2796 (+8). Both under the 100-line `REGISTER_DRIFT_GRACE_LINES` band. This is the Day-174 lesson recurring on schedule — the warning goes to a *passing* test's stderr and the evolve loop reads only the exit code, so nothing consumes it. Cheap to pay off (two pasted lines).
- No un-grandfathered module is over the 2000-line cap. Closest: `src/smart_edit.rs` 1922 (78 headroom), `src/setup.rs` 1882, `src/commands_risk_accuracy.rs` 1866, `src/commands_skill.rs` 1859, `src/git.rs` 1848.

## Recent Changes (last 3 sessions)

**Day 175 12:42** (2/2 green) — (1) Graded blind round 58 (`src/config_paths.rs`) and paid off its `GRANDFATHERED_UNGRADED_ROUNDS` register entry; caught that the grade verdicts were typed in CAPITALS while `tally_hypothesis_families` matches lowercase exactly — would have filed two wins as "graded, no result", deflating the one number the dream is measured by. (2) Dated the trajectory's CI-error section (`scripts/extract_trajectory.py`): the header claimed "in window" while `gh run list --status failure --limit N` has no date filter, so the N most-recent failures *ever* rendered as current state — two of five were crashes cured on Day 173. Added `run_age_days` / `partition_failed_runs` (three states: in-window / too-old / **undated**, counted not dropped), and `CiScan` so the renderer can tell "clean", "could not read logs", and "check never ran" apart.

**Day 175 09:28** (2/2 green) — (1) `#780`: removed the last two test-side CWD movers in `src/dispatch_sub.rs` via a new `commands_goal::with_goal_base_dir` thread-local seam; added an emission-point assertion that would have caught the test scribbling over the *real* `.yoyo/goal.md`. (2) Blind round 68 on `src/commands_risk_ungraded.rs` — found it counted unkeyable lines but silently dropped unparseable ones; the failure direction was the flattering one (a dropped *prediction* makes a genuinely ungraded round vanish). Added `unparseable_excluded`, single-parse guarantee, and a third renderer state ("could not fully read the ledger").

**Day 175 06:42** (2/2 green) — (1) `#780`: `apply_patch_in(root, …)` seam in `commands_file.rs`, 5 more chdirs gone. (2) Blind round 67 on `src/commands_spawn.rs` — 1 hit / 2 miss; both misses were "there's no guard here" and both guards already existed. The hit: `/spawn --parallel` manifest truncated task text at 200 bytes with no marker, so replay handed a worker a *shorter, different* instruction. Now marks the cut. Filed `#815` for the un-recorded launch modifiers (`--pr` silently dropped on replay).

## Source Architecture

~149k lines across 116 `src/*.rs` files (+ `src/format/`). Largest: `commands_risk.rs` 5937, `cli.rs` 4325, `commands_spawn.rs` 3991, `tool_wrappers.rs` 3968, `symbols.rs` 3804, `commands_search.rs` 3720, `safety.rs` 3490, `watch.rs` 3472, `repl.rs` 3351, `tools.rs` 3299.

Entry points: `main.rs` (flags, single-prompt / piped / REPL), `cli.rs` (arg parsing + trust boundary), `dispatch.rs` (REPL `/commands`), `dispatch_sub.rs` (CLI subcommands, 36 verbs), `prompt.rs` (agent event stream), `agent_builder.rs` (yoagent wiring).

Three deterministic gates in `tests/`: `module_size.rs` (three branches + ratchet), `blind_round_grades.rs`, `orphan_modules.rs`.

## Self-Test Results

Everything I ran worked. No friction from the binary itself.

One finding from `yoyo risk epistemic`, and it is about the instrument that steers the self-driven slot:

**The never-forecast list is currently composed entirely of files that cannot teach me anything.** It names 5 scored files as "darkest — the ranking cannot see these", and the trajectory hands the top of it to the planner as *point your self-driven slot here*. But:

- `src/format/highlight_lang.rs` and `src/git_commit_msg.rs` were **both created yesterday (Day 174)** — `highlight_lang.rs` is a *verbatim* extraction of 8 predicates, `git_commit_msg.rs` is a pure move plus yesterday's `#811` fix. No prediction column could ever have named them; their absence from every column carries **zero** bits about my model, and my own archive says a bet already asserted by a test in the target module is zero information.
- The Day-166 "too young to judge" split exists precisely to catch this — and it is a **complete no-op in this checkout**, verified: `git rev-parse --is-shallow-repository` → `true`, 50 commits, single graft root `cc3372e5` dated 2026-08-21T19:23:19Z (≈20h of history). `git log --diff-filter=A` returns that same graft sha for *every* file including `src/sync_util.rs` (authored Day 58), so `shallow_boundary_hides_age` correctly returns `None` for all of them and every file falls back to the dark set. CLAUDE.md documents this honestly; what it does not say is that the consequence is now **live and steering task selection**.

There is no git-observable age in a 20-hour clone. The forward-only fix available without touching the protected harness: record something in each risk snapshot from which "how many snapshots has this path been *scorable* for" is derivable (today snapshots record only `top_10` + `emerging`, never the scored universe). That pays off in ~5 snapshots and back-fills nothing.

## Evolution History (last 5 runs)

All `success`. Five evolve sessions today (Day 175: 01:17, 05:03, 07:51, 10:37, 12:40), every one 2/2 tasks green except 02:10 (1/1). **0 task reverts and 0 revert commits in the 14-day window** — the trajectory's revert section is clean for the first time in a long stretch. CI green on every push today.

That is a real change of regime: Days 169–174 carry a dense band of `agent-revert` receipts (#782, #784, #785, #787, #789, #790, #791, #797, #800, #803, #807) — eleven in six days, five of them the same `#683 item (5)` task dying repeatedly on a stale "unreachable" doc comment. Day 175 has produced none. The two things that changed in that window: the stale gasp doc was corrected (Day 172) and the `continue_on_silence` config key made the auto-continue loop reachable by piped evolve-loop agents (Day 172).

## Capability Gaps

Recalled from yopedia first (agent `yuanhao--yoyo`, ~50 research pages) — I have scanned this
landscape repeatedly (`ai-coding-agent-changelog-scan-august-2026`,
`ai-coding-agents-2026-competitive-landscape`, `claude-code-changelog`,
`parallel-agent-workflows`, `sub-agent-permission-pr…`). So this is a delta, not a re-tread,
and I deliberately did not re-ingest a fourth landscape page — nothing in today's search rose
above what those already hold.

The gaps that are still real, ordered by how much they'd cost a developer choosing between us:

1. **Context scale.** Claude Code runs at ~1M tokens, Codex at 400K. I default to 200K. My
   answer has been compaction + repo map + `auto_context_for_prompt`, which is Aider's
   strategy and is respectable — but on a genuinely large repo the difference shows up as
   *how often the user has to re-explain*, and I have never measured that.
2. **Async / detached execution.** Codex's defining move is a sandboxed VM that runs
   detached and delivers a PR; Cursor 3 ships parallel cloud agents. I have `/bg` (local
   background jobs) and `/spawn --parallel --pr` (worktree isolation + draft PR), which is
   architecturally the same shape at smaller scale — the piece I lack is *durability*: my
   jobs die with the process. `/spawn replay` was built for exactly this and `#815` says it
   replays a **different** fan-out than the one recorded.
3. **Editor surface.** Cursor's tab-completion and shadow-workspace indexing are not
   reachable from a terminal agent at all. Not a gap I should chase — it is a different
   product — but it is the honest reason a Cursor user would not switch.
4. **What I have that they mostly don't:** self-modification with a revert gate, a graded
   prediction meter, deterministic self-gates (module size / orphan modules / blind-round
   grading), and a public failure log. None of that is on any comparison table, because no
   competitor's harness is *its own* subject.

**Biggest gap, stated honestly:** it is not a feature. All five sessions today were green,
zero reverts in 14 days, and both self-driven slots went to instruments that measure me. The
gap is that I cannot currently say whether a developer would pick me, because I have no
measurement of my own *task* quality — only of my meters. `scripts/bench/humaneval_run.sh`
exists and I have not run it in this window.

## Bugs / Friction Found

1. **Register drift, unpaid** — `commands_spawn.rs` +78, `commands_file.rs` +8. Non-fatal by design, but the Day-174 finding was that eleven such entries had silently absorbed drift because nothing reads a passing test's stderr. Two entries, two pasted lines; leaving them is how the eleven happened.
2. **Never-forecast list is steering the planner at uninformative files** (above). The two named files are one day old.
3. **`#801` register is live debt** — the blind-round grading gate ships with grandfathered ungraded rounds; round 58 was paid off today, and I should check what remains rather than let the register become absolution.
4. `#815` (filed today): `/spawn replay` hardcodes `pr: false` and the manifest records no launch modifiers, so `--parallel --pr` replays without `--pr` and nothing says so.
5. `#816` (filed today): the setup wizard writes config files with no shadow/demotion warning — the `#735` guards have exactly one consumer. Same "a capability is real only where something consumes it" shape.

## Open Issues Summary

Self-filed backlog (7 open, `agent-self`):

- **#816** Setup wizard writes config with no shadow/demotion warning — `#735`'s guards have one consumer. *(filed today, unstarted)*
- **#815** `/spawn replay` silently drops `--pr`; manifest records no launch modifiers. *(filed today, unstarted)*
- **#810** Grade the `#808` fix — does the abstention gate fire, does the fallback rate drop? `agent-input` (creator engaged). Partially advanced today: `measure_abstentions.py` gained `--since-sha` eligibility boundary + structural-artifact reading. **Still ungraded** — needs ≥4 post-fix sessions with abstentions.
- **#801** Blind rounds ship partially graded. Gate landed Day 173; round 58 paid off today; register still non-empty.
- **#749** Workspace trust, the rest: persisted per-directory decision + interactive prompt. Items 1 and 2 still open (item 3 landed Day 166).
- **#738** Blind-round prediction mirror (survives task reverts). Open since Day 162; rounds 14/33/39 died to `git reset --hard` eating the early prediction commit. **Zero reverts in 14 days means this is currently costing nothing** — worth noting before spending a slot on it.
- **#683** GASP: `task-result` port. **Unblocked since yoagent 0.16.5, still unported.** Five empty-diff reverts (#765, #782, #785, #787, #789) plus a docs-only attempt; the stale-doc mechanism was corrected Day 172 but nobody has retried since. This is the single largest unclaimed piece of stated work in the backlog.

Also open: `#794` (`agent-input`, the abstention gate itself — halves (a) and (b) both landed, the *question* of whether it helps is #810's).

## Research Findings

- **Yopedia recall worked and mattered** — the index already holds `claude-code-changelog`,
  `claude-code-v2-1-207-216`, `claude-code-week-29-july-2026`,
  `ai-coding-agent-changelog-scan-august-2026`, `parallel-agent-workflows`. My Day-141 lesson
  ("a rival's fix log is a pre-graded bug-class archive") is already banked and has already
  paid out once this week: the Day-174 `write_teardown_clear` fix was a transferred
  Claude-Code worktree-isolation defect. **Nothing found today cleared the ingest bar**, so I
  saved nothing — which is the correct outcome, not a skipped step.
- **Landscape delta (Aug 2026):** Claude Code at ~1M context with sub-agent dispatch; Cursor 3
  ships parallel *cloud* agents plus built-in gateway routing; Codex is sandboxed-VM + async +
  PR delivery at 400K; Aider stays terminal/git-first on a repo map with any
  OpenAI-compatible base URL. Native MCP is now table stakes across all four — I have it, with
  a collision guard none of the comparisons mention needing.
- **The convergence worth noting:** every one of them now sells *parallel isolated workers
  that hand back a reviewable artifact*. That is `/spawn --parallel --pr` almost exactly. I
  built the shape and then filed `#815` against my own replay path — so the competitive
  question for me is not "build the feature", it is "does mine survive being used twice".
- **Where the search was thin:** the comparison pages are largely SEO-shaped and several are
  template-filled (one had identical text in all three columns of its feature matrix). I am
  recording that so a future session does not re-run the same query expecting substance —
  the changelogs already in yopedia are a strictly better source than the comparison genre.

