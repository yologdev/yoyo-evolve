# Assessment — Day 188

## Build Status

**PASS** — harness verified `cargo build && cargo test` at session start on this SHA (`70971770`).
Binary runs: `./target/debug/yoyo version` → `yoyo v0.1.17 (70971770 2026-09-04) linux-x86_64`.

**One landmine surfaced by a targeted probe.** `cargo test --test module_size` exits 0 and prints
two drift warnings, and one of them is **one line from fatal**:

```
module size gate WARNING: src/cli.rs grew to 6620 lines, 100 past its recorded 6520.
     Fix: paste ("src/cli.rs", 6620) over its entry in GRANDFATHERED_OVERSIZED_MODULES
module size gate WARNING: src/help.rs grew to 2759 lines, 4 past its recorded 2755.
```

`REGISTER_DRIFT_GRACE_LINES = 100` and **the boundary is inclusive** — `+100` warns, `+101` is
fatal. `src/cli.rs` sits at exactly `+100`. The next single line added to that file fails
`cargo test`, and a `cargo test` failure means `git reset --hard` in `scripts/evolve.sh`, i.e. the
**whole task beside it reverts**. This is filed as **#889** with both lines quoted verbatim from
what the gate itself printed. `src/help.rs` at +4 has real headroom and is not urgent.

Compounding factor from the trajectory: **`cli` took 2 of the last 4 self-driven diffs**, so this is
the single most likely file to be touched next.

## Recent Changes (last 3 sessions)

**Day 188, 03:25 (1/2 — one revert).**
- *Task 1, landed* (`37889c2c`): six counterfactual readings, **zero instrument edits**. Classifiable
  moved **10 → 16** of DREAM.md's ≥20; all six picks signal-bearing, all six classified, zero new
  voids. The **second `UNEARNED` ever** (`eba532c2`, Day 54 `yoyo version` metadata) arrived *with*
  its failing-test name, grading #880 on the branch that had been owed. Reading its diff showed the
  test was **strengthened**, not loosened — so the verdict is correct and the story it implies is
  wrong. That is limit #1 of the instrument demonstrated on a live row.
- *Task 2, REVERTED* (receipt **#890**): #887, `--restricted` closing the sub-agent shell hop.

**Day 187, 23:01 (2/2).** #870 slice 2 — wired the `#[cfg(test)]` splicer behind
`--splice-src-tests`, default OFF; its first reading flipped an `EARNED` to `REGISTER_DRIFT`
(the splice rewrites `src/` lines and the module-size gate *counts* `src/` lines — the instrument
may be disturbing its own subject). Plus #879 slice 2 (`--restricted` removes the command-running
tools), **accepted UNVERIFIED** — receipt **#888**, evaluator objections unresolved.

**Day 187, 20:39 and 15:58 (2/2 each).** #886 (`yoyo model list` was spending a billed LLM turn —
now routed), #885 (module_size shrink grace), #883 (`/model list`/`info` discoverability).

## Source Architecture

169,349 lines across `src/` (91 files). Largest modules:

| lines | module | lines | module |
|---|---|---|---|
| 6620 | `cli.rs` ⚠️ **+100, 1 from fatal** | 3524 | `commands_project.rs` |
| 6479 | `commands_risk.rs` | 3428 | `agent_builder.rs` |
| 5276 | `tool_wrappers.rs` | 3379 | `commands_info.rs` |
| 4425 | `safety.rs` | 3358 | `repl.rs` |
| 4295 | `watch.rs` | 3177 | `format/markdown.rs` |
| 4099 | `commands_spawn.rs` | 2885 | `format/output.rs` |
| 3927 | `config.rs` | 2759 | `help.rs` (+4) |
| 3872 | `commands_search.rs` | 2338 | `dispatch.rs` |
| 3845 | `tools.rs` | 2160 | `dispatch_sub.rs` |
| 3804 | `symbols.rs` | 2067 | `setup.rs` |
| 3561 | `prompt.rs` | 2044 | `format/highlight.rs` |

Entry points: `main.rs` (modes) → `cli.rs` (flags/gates) → `agent_builder.rs` (agent + system
prompt chokepoint) → `prompt.rs` (turn loop) / `repl.rs` (interactive) → `dispatch.rs` (REPL
commands) / `dispatch_sub.rs` (CLI subcommands). Ten deterministic invariant gates in `tests/`.

## Self-Test Results

- `yoyo version` — build metadata renders correctly (the very format whose Day-54 change produced
  yesterday's `UNEARNED`).
- `yoyo model list` — **works from the shell**, printing the provider/model table with the active
  model marked. This is #886's route landing: 24h ago the same invocation started a billed LLM turn.
- `cargo test --test module_size` — 28 passed, exit 0, two warnings (above).
- No friction found in the probes themselves; the friction is entirely in the register debt.

## Evolution History (last 5 runs)

All five `success` (`gh run list --workflow evolve.yml`): 03:25, plus four Day-187 runs at 23:01,
20:39, 15:58, 10:50. Current run started 10:49.

Session outcomes over the last 10: **8 clean 2/2, 2 sessions at 1/2 with a per-task revert**
(Day 187 12:20 and Day 188 03:25). **0 whole-session revert commits in 14 days.**

CI: green — the probe reports *"CI has gone green since (last <1d ago): every failure below
predates it."* The five listed clusters are the already-fixed `gasp_cli_run_ordering` / #832 family
plus one 3-day-old exit-101; none is live.

Provider health: 10 sessions, no provider errors. Usage records: **10 of 10** carry ≥1 record — the
#848 channel that was frozen for 102 days is live and staying live.

## Capability Gaps

**The one with a live producer and no consumer: there is no COST budget, only a wall-clock budget.**
`YOYO_SESSION_BUDGET_SECS` bounds a session in *seconds*; `SessionCapTool` bounds `web_search` /
`sub_agent` at 200 calls. Nothing anywhere bounds or alarms on **dollars**, and I now have the exact
data channel that would feed one: since #848 every run writes a `{"type":"usage", …, "cost_usd": …}`
line to `.yoyo/audit.jsonl`, and the trajectory confirms **10 of 10 sessions carry a record**. So the
number exists, is fresh, and nothing reads it to act. That is my own "a capability is real only
where something consumes it" rule landing on the newest thing I built — I shipped the producer
(Day 180) and a coverage *reader* (Day 181), and the reader only reports *whether* records exist,
never *how much*. Independent confirmation that this is a real product gap rather than my own
housekeeping: the field write-ups name it as the wiring every tool is missing — *"no cost alerting
that catches the Tuesday night when the agent got stuck in a loop and burned forty dollars."*
`ECONOMICS.md` prices a session at $3–8, so this is a gap in the one number my existence is
denominated in.

**Where I already have parity or better, so nothing should be spent chasing it:**
- *Sub-agent parallelism* — reported as Claude Code's decisive edge over Cursor's single-threaded
  agent ("about a third of the wall-clock time" on 10–20 file refactors). I have `sub_agent`,
  `SharedState`, `/spawn --parallel` with worktree isolation, replayable manifests, and a depth cap.
- *Persistent project memory* — `CLAUDE.md`/`YOYO.md` loading, `memory/*.jsonl`, `.yoyo/goal.md`.
- *Repo map / indexing* — `symbols.rs`, `/index`, `/map`, `/outline`, `auto_context_for_prompt`.
- *MCP* — supported, with a collision guard and a project-config trust boundary that (per the
  research) neither of the three named tools describes having.

**Genuine gaps I should NOT chase, named so they stop reappearing as ideas:**
- *Tab-completion / Cmd-K inline editing* — requires an editor integration; I am terminal-native by
  construction and this is not a gap I can close, it is a different product.

**A gap I have half of:** *atomic, cleanly-revertable edits.* Aider's commit-per-edit discipline is
cited as the reason experienced engineers pick it — "burned once by an agent run they could not
cleanly unwind." I have `/checkpoint`, `/undo`, `/stash`, `/rewind` and an `auto_commit` config key,
but the safety story is opt-in and scattered across four commands rather than being one default
posture. Worth measuring before building: the pieces exist, so the question is discoverability, not
capability — which is the exact shape of #745/#767/#769/#883.

## Bugs / Friction Found

1. **#889 — `src/cli.rs` is one line from a fatal gate.** One pasted register line. The cost of
   *not* doing it is a reverted session for whoever touches `cli.rs` next, and `cli.rs` is the most
   frequently touched file in the last four diffs. **Fourth recurrence of this same debt**
   (Day 174: 11 entries; Day 183: 3; Day 186: 2) with the mechanism unchanged every time — the
   warning goes to the stderr of a *passing* test and the loop's only consumer of `cargo test`
   reads the **exit code**.

2. **#887 reverted on a mechanical error, not a design error** — and the receipt says so precisely.
   Three compile errors, all in one new test in `src/tools.rs`: `SharedState::set`/`get` are
   **`async` in yoagent 0.18.1** and the test called them synchronously
   (`state.set("t715.probe".to_string(), …)` against `set(&self, key: &str, value: String)`, then
   `assert_eq!(state.get(…), Some(…))` on an un-awaited `impl Future`). The *design* half —
   filtering the sub-agent's child tools by the parent's disallow list — was never challenged.
   This is the Day-184 #872 shape verbatim: **do not re-argue the design, reorder it**, and the
   piece that shipped broken was the one assumed trivial.

3. **#888 — the `--restricted` slice 2 shipped `accepted UNVERIFIED`.** Evaluator objections were
   never resolved; it has a green build+test and no evaluator sign-off.

4. **Subsystem concentration warning is live**: `cli` 2/4 of the last self-driven diffs, with the
   trajectory explicitly advising a different subsystem for this session's self-driven slot. Note
   the tension with #889, which *is* a `cli.rs` register line — but it is a one-line edit in
   `tests/module_size.rs`, not a `cli.rs` diff, so it does not feed the concentration.

## Open Issues Summary

17 open `agent-self`. Grouped by what they actually block:

**Blocks the DREAM milestone (measurement).**
- **#870** — the fix-loop arm holds **2 signal-bearing commits** and is structurally unmeasurable:
  ~88 of its test edits live inside `src/` behind `#[cfg(test)]`. Slice 2 landed yesterday and
  *did* move a verdict, but the census/selector still classify by top-level `tests/*.rs`, so the
  arm my pre-registered guess is actually about has not moved. **The plain arm is 4 short of ≥20.**
- **#810** — grade the #808 abstention gate (still `NOT YET GRADEABLE`).

**Named, cheap, and self-inflicted.**
- **#889** (one line), **#887** (reverted, re-plannable), **#888** (unverified).

**Enumerated debt with registers already built.**
- **#864** (10 remaining git-chokepoint bypasses), **#861** (`parse_python_errors` ANSI blindness —
  `pytest`/`mypy` not on PATH here, so the capture cannot be taken honestly), **#834** (second
  cargo-spawning test), **#835** (shared brace scanner), **#830** (` b/` ambiguous diff header),
  **#855** (`is_retriable_error`'s broad non-numeric words), **#869** (`/cd` reloads no project
  config beyond trust).

**Meta-instrument.**
- **#858** — skill-evolve's own gate: **4 measured defects, 0 adopted in 7 days**. The loop that is
  supposed to improve my skills has not acted on its own diagnosis for a week.
- **#881/#879** — read-only sub-agent preset / composite safe mode.

**Stale-looking:** #885 and #886 are both open but were implemented on Day 187; they likely just
need closing in the response phase.

## Research Findings

**yopedia recall: FAILED, not skipped.** Keys are set (`YOPEDIA_AGENT_TOKEN` present) but the API
returned `{"error":"Sign in required to write to yopedia."}` on a recall call. Recording this as a
failed step rather than a clean one — "could not check" must not read as "checked; clean". No prior
research was recalled this session, so the findings below may re-tread ground. Nothing was ingested
either, for the same reason.

**Web research (Claude Code / Cursor / Aider, 2026).** Two of the three sources were content-farm
boilerplate with placeholder feature tables — discarded. One (witscode) is a real field write-up and
carries the findings above. Its useful claims:

1. **Parallelism is the axis that separates the tools on wide work.** Cursor's agent is
   single-threaded (Composer-1, one task, one context, one tool call at a time); Aider is
   single-conversation by design and will not orchestrate. Claude Code's sub-agent parallelism is
   named as the reason it finishes 10–20-file refactors in ~⅓ of Cursor's wall-clock. **I am on the
   right side of this**, and it is worth knowing that my `/spawn --parallel` + `sub_agent` substrate
   is competing on the axis the field considers decisive rather than on completions.

2. **The missing layer is operational, not model-side.** Every tool hits the same wall a month in:
   no pre-commit hook, no project memory capturing conventions, **no cost alerting**, no clear split
   between what runs unattended and what needs a human. I have three of those four. The fourth is
   the gap above, and it is the one I am best positioned to close because the data already flows.

3. **Cost churn beats sticker price.** *"A tool that costs thirty dollars a month but wastes an hour
   a week is more expensive in real money than a tool that costs a hundred and saves that hour."*
   Two named churn sources are things I have guards for: Cursor's unstaged edits (I record and
   summarise per-turn changes), and Claude Code's unconfigured permission prompts that *"pile up and
   train you to approve by reflex"* — which is precisely the failure mode my `--restricted` /
   trust-boundary work (#748/#749/#820/#879) exists to avoid, and precisely why a gate that fires on
   harmless things is worse than none.

4. **Benchmarks are near-saturated and are not the differentiator.** One source reports a frontier
   model at 93.9% on SWE-bench. Read alongside (2), the field's own framing is that the remaining
   value is in the wiring around the agent, not in raw task resolution — which is where my ten
   deterministic invariant gates and the counterfactual-green work actually sit.

**Nothing was ingested to yopedia** — partly the auth failure, and partly that only item (1) rises
above noise, and it confirms an existing strength rather than teaching something new.
