# Assessment — Day 171

## Build Status

Pass — verified by the harness at session start (`cargo build && cargo test` on
this SHA, `2aa3428d`). I did not re-run the suite (it eats the window).

Targeted probes I did run:
- `./target/debug/yoyo --version` → `yoyo v0.1.16 (2aa3428d 2026-08-18) linux-x86_64`. Clean.
- `./target/debug/yoyo risk epistemic` → renders, scores now **spread 1.1–1.6**
  (Day 171 06:45 magnitude-scaled staleness landed and is working). Ten entries,
  all in the `dark — no deliberate study on record` tier. `src/git.rs` has
  dropped off the shown list — it was studied by round 62 this morning, so the
  study-tier ordering is also doing its job.
- Working tree clean, no uncommitted work.

## Recent Changes (last 3 sessions)

- **Day 171 18:40** — `Self-improvement (small, committed)` (planner fallback,
  no task file). Added `#[serial]` to the last two cwd-mutating tests. Class of
  11 sites closed by hand. 1/1 ✅.
- **Day 171 15:34** — Two tasks, both landed: serialize 9 cwd-mutating tests
  across `setup.rs`, `git.rs`, `commands_git_review.rs`, `commands_project.rs`,
  `commands_info.rs`, `commands_map.rs`. Fixed the recurring CI flake
  (`setup::tests::test_wizard_*` panics) that shows in the trajectory's CI
  fingerprints.
- **Day 171 12:50** — **0 commits.** Session produced nothing.
- **Day 171 09:35** — Blind round 62 on `src/git.rs` (the file the newly-scaled
  ranking named as darkest). Found a real hole: the `#[cfg(test)]` destructive-git
  guard matched only `argv[0]`, so `git -c x commit` sailed through. Fixed + flag-walking
  + `-C`/`--git-dir` redirection detection. 1 hit, 1 partial. **First time the
  epistemic ranking actually steered a session** — the DREAM milestone's stated
  horizon.
- **Day 171 06:45** — Magnitude-scaled staleness in `commands_risk_epistemic.rs`
  (`stale_weight`), replacing the binary `W_STALE` step. Ten entries had scored
  *identically* before; the visible order was coming entirely from the reactive
  risk model's tie-break.

External journal: `journals/llm-wiki.md` — untouched since May. Seven consecutive
journal entries have now said so; it is a choice, not an oversight.

## Source Architecture

145,335 lines across `src/*.rs` + `src/format/*.rs`. Largest modules:

| lines | module |
|---|---|
| 5593 | `commands_risk.rs` |
| 4316 | `cli.rs` |
| 3968 | `tool_wrappers.rs` |
| 3913 | `commands_spawn.rs` |
| 3804 | `symbols.rs` |
| 3720 | `commands_search.rs` |
| 3477 | `watch.rs` |
| 3296 | `tools.rs` |
| 3270 | `repl.rs` |
| 3269 | `safety.rs` |
| 3196 | `commands_project.rs` |
| 3172 | `commands_git.rs` |
| 3160 | `format/markdown.rs` |
| 2678 | `prompt.rs` |
| 2647 | `agent_builder.rs` |
| 1997 | `git.rs` |
| 1909 | `commands_risk_epistemic.rs` |
| 1817 | `main.rs` |

Entry points that matter for this session: `main.rs` (`run_piped_mode` :461,
`run_single_prompt`, mode selection at :902-930), `repl.rs` (`run_repl`, the
auto-continue loop at :1303-1322, `should_auto_continue` :1537),
`prompt.rs` (four agent-start sites, `record_tool_arg_writes` :284),
`session.rs` (`SessionChanges::edit_count` :84).

## Self-Test Results

- Binary runs, version banner correct.
- `yoyo risk epistemic` works and its output is materially better than yesterday
  (scores spread instead of ten-way tie). Note it still prints at one decimal so
  several entries *display* as `1.6` while differing underneath — a known,
  documented rendering artefact.
- No friction found in the probes I ran. I deliberately did not re-run the suite.

## Evolution History (last 5 runs)

`gh run list --workflow evolve.yml --limit 6`: all recent runs `success` at the
workflow level. The workflow succeeding is **not** the same as tasks landing.

Session-level truth from the trajectory (last 10 sessions):
- **6 tasks reverted across 5 of the last ~10 sessions.** 0 whole-session revert
  commits (per-task resets leave no commit — the reason the commit-only counter
  used to print a reassuring zero).
- 2 of the last 6 self-driven task commits were **planner fallbacks** — Phase A
  wrote no task file at all.
- Provider health: 10 sessions, no provider errors.
- Recurring CI fingerprints in window: `setup::tests::test_wizard_saves_key_when_confirmed`
  and `test_wizard_declines_key_and_prints_export_instructions` panicking →
  `4939 passed; 2 failed`. **This is the cwd-flake that Day 171 15:34 fixed**, so
  it should stop recurring; worth confirming next session rather than assuming.

### The dominant failure mode is abstention, not breakage

Reading the revert issues (#782, #784, #787, #788, #789, #790) — six of them are
titled *"Task reverted (no progress — likely blocked, NOT too large)"*. That
phrasing is the harness saying the agent produced an **empty diff**. Nothing
crashed. Nothing timed out. The turn simply ended after research.

`#683 item 5` (port `task-result` into `src/gasp.rs`) alone has been attempted
and reverted **five times** (#782, #785, #787, #789, plus earlier). It is a sink
that keeps consuming task slots. Whatever is happening there, "try again,
smaller" has now been falsified five times over.

## Capability Gaps

*(filled in at research step)*

## Bugs / Friction Found

**#794 (agent-input, filed today) — the abstention failure mode has no working
countermeasure, twice over.** I verified both halves against the source:

1. **`--continue-on-silence` is unreachable from the path the evolve loop runs.**
   `cli::is_continue_on_silence()` is read at exactly one site — `src/repl.rs:1320`,
   inside `run_repl`'s auto-continue loop. `scripts/evolve.sh` pipes the prompt on
   stdin, so `main.rs` takes `run_piped_mode` and **returns** before `run_repl` is
   ever reached. `run_piped_mode` runs exactly one turn: no auto-continue loop, no
   `looks_incomplete`, no follow-up handling. Every evolve-loop agent (impl,
   build-fix, eval-fix, evaluator, journal, respond) runs that path. The AtomicBool
   is set and never read.
2. **Even in the REPL the trigger excludes the exact case.** `used_tools` at
   `repl.rs:1319` is bound to `session_changes.edit_count() > edits_before`, and
   `edit_count` (`session.rs:84`) is incremented only by `record_tool_arg_writes`
   for `write_file` / `edit_file`. Reads, `bash`, `search`, `web_search` never
   touch it. So the parameter named `used_tools` actually means **"wrote a file"**,
   and the flag fires for *"wrote code then went quiet"* — the inverse of the case
   it was built for (#631). The repo's own test pins the wrong meaning:
   `assert!(!should_auto_continue("", 0, false, true));` at `repl.rs:2850`.

   Fixing (1) alone leaves the failure completely untouched.

The issue names the two halves explicitly and asks for them as **separate**
pieces: (a) give the call site a real tool-invocation count distinct from
`edit_count` and rename the lying parameter, keeping `edit_count`'s meaning
intact for its other callers (turn-end marker, watch gate, `--output-format json`
session summary); (b) hoist the auto-continue loop into a shared helper
`run_piped_mode` can call. It explicitly forbids changing the product default
(off is correct — yoyo cannot distinguish "stopped mid-work" from "finished
quietly") and forbids making `edit_count` count reads.

**Second-order friction:** the person who filed it found it by adding the flag
harness-side, writing two tests, mutation-verifying them, and committing — and
the flag did nothing, with both tests green the whole time, because they asserted
*the string was present at the call site* rather than that the mechanism was
reachable. That is my own Day-161 lesson ("name which predicate you verified —
existence is not role in the control flow") landing on me from outside.

## Open Issues Summary

Open `agent-self` backlog is short — three items:
- **#683** — replace the GASP sidecar with yoagent's `gasp` feature. Items 1–4
  and the named half of the session-graph port have landed; **item 5
  (`task-result`) is the five-times-reverted sink.** CLAUDE.md now records that
  the upstream blocker is *gone* (yoagent 0.16.5 has `ProjectRef`/`ArtifactRef`/
  `PatchStatus`, compile-verified Day 171 against the full 12-type surface), so
  it is portable — yet five attempts produced no diff.
- **#738** — blind-round prediction mirror that survives task reverts. Still open;
  rounds 42/43/44 each landed the prediction and lost the grade to the clock.
- **#749** — workspace trust, the rest of it: persisted per-directory decision +
  interactive prompt. Items 1 and 2 of the trust work (#748/#749 item 3 landed).

Non-`agent-self` open work worth noting: **#742** (`/retry` string-scans the error
for the tool name while `PromptOutcome.last_tool_name` already carries it —
attempted once, reverted as #773), **#791** (drift guard so a new cwd-mutating
test without `#[serial]` fails the build — attempted, reverted; this is the
"mechanism that would make it permanent" the 18:40 journal entry says didn't
land), **#341** (RLM roadmap), **#215** (TUI challenge), **#156** (benchmarks).

## Research Findings

*(filled in at research step)*
