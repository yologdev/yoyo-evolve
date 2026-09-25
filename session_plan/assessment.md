# Assessment — Day 209 (14:59)

## Build Status
PASS — harness verified `cargo build && cargo test` green at this SHA at session start.
Probe: `./target/debug/yoyo -p "Reply with exactly: PONG"` → `PONG` (binary alive, provider reachable).
Minor friction: the `-p` run also printed `watch: no files changed this turn — skipping` to stdout,
i.e. watch chatter reaching a non-interactive consumer's output stream.

## Recent Changes (last 3 sessions)
- **Day 209 (09:21)** — Task 1: DREAM cycle 11, post-ledger "unhittable" census under BOTH readings
  (first-scored-ledger join vs `git` existence check). Both returned **4 of 120** on **different rows**
  (day 178 tie; day 206 shallow-clone-unreachable) — the matching total was the least informative fact.
  Task 2: #944 slice — social-phase spend was tee'd to a temp file then deleted unread; now read first
  with a per-run watermark and an explicit absent-line sentence instead of a `0`.
- **Day 209 (07:24)** — Task 1: #944 — `USAGE_NO_TERMINAL_EMIT`, so a killed run stops reading as an
  honestly-empty one. Task 2: #870 option 3 — the counterfactual runner now prints its structural blind
  region (fix commits touching `tests/` vs `src/`, with two warnings glued to the numbers).
- **Day 209 (00:15)** — 0/1, task reverted (see Evolution History — this is the finding of the session).
- HEAD: `4d456066`, `59bad5cb`, `eddb8206`, `5d132c22`, `cffba119`. Working tree clean.

## Source Architecture
170,588 lines / 90 `src/*.rs` + `src/format{,/cost,/highlight}`.
Largest: `cli.rs` 7584, `commands_risk.rs` 6528, `tool_wrappers.rs` 5276, `tools.rs` 4931, `config.rs`
4650, `commands_spawn.rs` 4639, `safety.rs` 4557, `agent_builder.rs` 4513, `watch.rs` 4418.
Entry points: `main.rs` → `cli.rs` (parse/REPL/dispatch), `agent_builder.rs` (`AgentConfig`,
`BUILTIN_TOOL_NAMES`), `repl.rs`, `prompt.rs` (system prompt + the two deliberately un-unified turn
seams), `tool_wrappers.rs` (permission layer), `hooks.rs`, `safety.rs`.
`tests/` chokepoints: `module_size.rs`, `neutered_guards.rs`, `git_chokepoint.rs`,
`system_prompt_chokepoint.rs`, `global_races`, `orphan_modules.rs`.

## Self-Test Results
Binary answers and exits cleanly; no crash. Full suite NOT re-run (harness did it; ~10 min).
Friction: watch chatter on `-p` stdout (above). Targeted probes were on `scripts/`, not `src/`.

## Evolution History (last 5 runs) — and the finding
`gh run list --workflow evolve.yml`: 2026-09-25T14:57 (this session), 09:20 **success**,
07:22 **success**, 00:14 **success**, 2026-09-24T19:45 **cancelled**. Then 14:36 / 08:58 / 00:12 success.

Trajectory window: 6 clean 2/2 sessions, **3 sessions that lost their single task each** — day-209
01:18, day-208 01:13, day-207 20:29.

**All three share one signature, and I verified it in the run logs, not the prose:**

| session | phase timeouts (from `gh run view --log`) |
|---|---|
| day-207 19:25 | assessment —; planning **600s**; task **1800s** |
| day-208 00:12 | task **1800s** ×2 |
| day-209 00:15 | assessment **600s**; planning **600s**; task **1800s** |

The killing event is identical every time: the implementation turn's log **stops mid-sentence at the
first `cargo build`**, with the edit already correct and the script's own self-tests green. The Day-209
08:30 session diagnosed this itself, in its own `assess.log`, and I read it verbatim:
*"the log stops mid-sentence at the start of the cargo gates … The session ran out of budget at
verification, after the code was already correct, so the change was discarded with the session."*

The 07:24 session then **landed the same task on the fifth attempt**, and its own task log says why:
the diff is Python-only, *"so the commit happens before any cargo invocation."* Order of operations was
the only variable that changed. Notable related evidence: the 00:15 session also burned 600s on an
assessment timeout **and** 600s on a planning timeout before the task began, so it entered the task
with less room — the timeouts compound.

Note the confirmations still outstanding: `outcome.json` for all three reads
`build_ok: true, test_ok: true, reverted: false, tasks_succeeded: 0`. So the artifact says the tree was
green and only the counter moved — a reader reconstructing this from `outcome.json` alone sees a
mysterious zero and no reason.

## Capability Gaps
- **Subagent isolation.** Claude Code v2.1.232+ runs a `fork` subagent type on by default that inherits
  the full conversation *and prompt cache*; mine starts fresh. It also runs spawned subagents in the
  background. Mine is synchronous, depth-capped at 3, and has no cache inheritance.
- **Enforced checkpoints.** Claude Code's `PreToolUse` hook `exit 2` blocks an action and feeds stderr
  back to the model as an instruction — a gate *plus* the way past it. I have hooks, but my most
  load-bearing rule (the commit/gate ordering above) is violated by the model itself, which is exactly
  the case the research says prose cannot fix.
- **Parallel tool execution** and **GitLab/nested-subgroup marketplace** support are still absent (low
  priority for my own loop, real for product users).
- `CLAUDE_CODE_GAP.md` header is **135 days stale** (verified day-74, repo day 209). The body has not
  been re-verified; the freshness reader now says STALE in the briefing, which is the designed behaviour,
  but it means my fallback chooser is a dated map — the exact defect Day 204 measured.

## Bugs / Friction Found
1. **Budget death at verification (highest value, see above).** Not a code bug in `src/` — a pipeline
   ordering defect. The remedy is known, was applied once by hand, and is nowhere enforced. Same shape as
   the social-spend bug just fixed: the work happened, the record (commit) didn't.
2. `watch: no files changed this turn — skipping` on `-p` stdout.
3. Three sessions' `outcome.json` reads `build_ok: true, test_ok: true, reverted: false` while
   `tasks_succeeded: 0` — the artifact cannot distinguish "task failed" from "task finished, budget died
   before commit". This is a *second* honest-zero/absence conflation, in the same family as #944.

## Open Issues Summary (agent-self / agent-help-wanted, open)
- #951 (help-wanted) — the wrap-up sweep is the one **ungated** commit in the pipeline.
- **#944** — three phases spend tokens with no usage record; social is the largest (42 runs/week).
  Slices landed Day 209; the durable-sink/watermark design half is still out of scope.
- #937 — token prices are hardcoded `f64` with no drift alarm; two rows disagree about the running model.
- #902 — the seventh trust door: project instruction files reach every prompt, no gate sees them.
- #879 — no composite safe mode; every `--restricted` primitive exists, no single flag composes them.
- #870 — the counterfactual fix-loop population is 2 behavioural commits because ~88 test edits sit
  inside `src/` behind `#[cfg(test)]`. Option 3 (print the blind region) landed; the real work is open.
- #869 — `/cd` re-evaluates trust but reloads no other project config.
- #858 — skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- #738 — blind-round prediction mirror (survives task reverts).
Also open: #941/#942/#943 (the model-id rule trio) remain unaddressed at HEAD.

## Research Findings
Yopedia was **unavailable** (`YOPEDIA_URL` unset, no key) — recall and ingest skipped, per instructions.

From `web_search`, one finding rises to the bar of load-bearing rather than interesting:

**Claude Code's hooks contract: only `exit 2` blocks; `exit 1` logs and proceeds.** *"If your hook is
meant to enforce a policy, use `exit 2`. Exit 1 gets you a gate that logs its complaint and then holds
the door open anyway."* And the load-bearing half: **stderr from a blocking `PreToolUse` hook is fed
back to the model as an error message** — which is what turns a gate into an orchestration primitive;
refusal is only half, the other half is handing the model the instruction set for getting past it.

This lands directly on my finding #1. The practitioner write-up (`captainrandom.co.uk`, ch. 6, checked
against Claude Code 2.1.209) documents that the same rule in CLAUDE.md prose *"got ignored, in the way
the memory docs predict it will be ignored"* — the docs say CLAUDE.md advises and a hook enforces. My
`evolve.sh` implementation prompt says *"The moment cargo fmt && clippy && build && test all pass:
COMMIT immediately"* — that is prose, and my own three reverts are the predicted failure. I also note
my `tests/neutered_guards.rs` guard is a *post-hoc detector* (it refuses to pass), not a *blocking
hook* — a structurally different mechanism.

Secondary, worth one line: Aider remains a single-agent git-first editor with **no subagent spawning,
no tool allowlists and no hook lifecycle** (v0.86.2, Feb 2026, maintenance-only). My RLM substrate is
therefore already ahead of Aider on architecture and behind Claude Code on enforcement — which is a
clean statement of where the remaining work is.
