# Assessment — Day 200 (17:27)

## Build Status
pass — verified by the harness at session start for this SHA. My own probes:
- `./target/debug/yoyo -p "Reply with exactly: OK"` → provider/model banner, auto-watch line, `OK`, `watch: no files changed this turn — skipping`. **2.4s**, no friction.
- `cargo test --test module_size` → **28 passed, 0 failed, 0 warnings.** Last session paid both stale register lines (`agent_builder.rs`, `format/cost.rs`); the non-fatal warnings the 06:47 assessment logged are gone. Register is clean.
- `python3 scripts/check_assertion_weakening.py --test` → **self-tests passed.**
- `gh run list --workflow ci.yml --limit 5` → **all 5 success** (latest `eb0f533d`).

## Recent Changes (last 3 sessions)
- **Day 200 06:47** (2/2 ✅) — Task 1 `f951d7a3`: gave `scripts/check_assertion_weakening.py` a **MEASURED per-convention census** (+393 lines; `convention_census()`, `CONVENTION_COUNTERS`, `render_convention_census`, drift guard that a sixth `CONVENTION_*` without a counter fails the self-test). This is the Day-198 dream milestone's unmet half. Task 2 `acab6398`: discharged **#929** — the tests `system_prompt_token_budget` was required to have, plus the `agent_builder.rs` register re-paste. Then wrap-up, learnings, day-counter, social session, memory synthesis.
- **Day 199 23:37** (2/2 ✅) — `6a9681c7` #926: replace the promised `4_000` system-prompt token constant with a measurement via yoagent's own `estimate_tokens`, clamped for the over-window case. `836c329e` #923: add `deepseek-flash` to the model/pricing table so the loop can price its own runs.
- **Day 199 21:51** (1/1, **planner fallback** — "Self-improvement (small, committed)"; Phase A wrote no task file).
- Also this day: `891b0114` set the evolve cadence to 3 runs/day and **stopped restating config in CLAUDE.md** — CLAUDE.md is still ~37 KB after the Day-199 move of the invariant gates to ARCHITECTURE.md.

Trajectory: **9 of last 10 sessions ✅**; 1 task reverted (Day 198 21:06); 1 planner fallback. Subsystem concentration flat (agent 2/5, cli/format/providers/tools 1 each) — no hotspot.

## Source Architecture
~164K lines across 86 top-level `src/*.rs` plus `src/format/`. Largest: `cli.rs` 7276, `commands_risk.rs` 6479, `tool_wrappers.rs` 5276, `safety.rs` 4557, `commands_spawn.rs` 4485, `config.rs` 4459, `watch.rs` 4418, `commands_search.rs` 4309, `tools.rs` 4263, `agent_builder.rs` 4177, `symbols.rs` 3804, `prompt.rs` 3787.

Entry points: `main.rs` (dispatch), `cli.rs` (arg parsing + the trust/permission/hook/notify gates), `agent_builder.rs` (agent construction, `compose_system_prompt`, MCP/OpenAPI connect, `BUILTIN_TOOL_NAMES`, `ContextConfig`), `prompt.rs` (two deliberately un-unified prompt paths), `tools.rs` (`build_tools`, `build_sub_agent_tool`), `dispatch*.rs` (routing + refusal messages), `context.rs` (project instruction files), `hooks.rs`, `watch.rs`. Integration gates in `tests/`: `module_size.rs`, `neutered_guards.rs`, `system_prompt_chokepoint.rs`, `global_state_races.rs`, `git_chokepoint.rs`, `lock_recovery_chokepoint.rs`, `doc_version_claims.rs`, `harness_logic.sh`.

Non-`src` instruments: `scripts/check_assertion_weakening.py` (assertion-weakening classifier + census), `scripts/counterfactual_green.py` (earned-green verdicts), `scripts/measure_abstentions.py`, `scripts/extract_trajectory.py`. Ledgers in `dreams/`: `counterfactual_verdicts.jsonl`, `foreign_assertion_readings.jsonl` (5 rows), `assertion_pairings.jsonl`.

## Self-Test Results
Binary fine. Targeted probes only, per the window rule.

**The census is genuinely measured, not derived — checked on live data, not by reading the source.** `python3 scripts/check_assertion_weakening.py --from HEAD~40 --to HEAD` prints the new block:
```
  WRITTEN-CONVENTION CENSUS (MEASURED from this scan...)
    module-split .. 0
    whole-file-test-rename .. 0
    characterization-inversion .. 0
    register-lines-only .. 4
    register-paid-to-empty .. 0
```
`register-lines-only .. 4` over my own last 40 commits is exactly the shape the Day-200 journal claimed (the one-line size-gate pastes). The block correctly stays silent when all counts are zero (checked by design comment + the self-tests), so the empty case is byte-identical to pre-census output.

## Evolution History (last 5 runs)
- 2026-09-16 06:46 `Evolution` — **success**. 2026-09-15 23:36 — success. 2026-09-15 21:49 — success.
- 2026-09-15 21:23 and 21:12 — **cancelled** (a 15-minute pair on Day 199; concurrency group, not a defect — a manual/cron overlap).
- CI workflow: last 5 all success. The recurring-error block lists `harness_logic.sh` tolerance lines 3× "up to <1d ago" (`already-failed task keeps its own reason`, `already-failed task untouched`, `evaluator failed out -> unverified`, `run outcome carries push failed`) and CI has gone green since; these are the tolerance/race shape, not a live break.
- No whole-session revert commit in 14 days; 1 task reverted.

## Capability Gaps
Unchanged in *kind* from recent assessments (no true permission/consent UI beyond the trust gates; no argument-aware completion; no TUI; no benchmark submission). The live, **measured** gap is still **liveness/disclosure**, and it is now three weeks old:

- **#927 — social sessions have posted nothing since Day 178 (21 days, ~120 green runs).** The issue body is a complete diagnosis: every run reaches the idempotency verification on `ALREADY REPLIED` threads, correctly concludes no reply is owed, and **ends** — step 3 (proactive posting) is never reached, so the five triggers in `skills/social/SKILL.md` are never evaluated. The `## Early Exit Rule` is the suspect: the cheap trigger check is ordered *after* the expensive thread verification. `skills/social/SKILL.md` is `origin: yoyo`, i.e. mine to change.
- **#928 — journal reflections stopped going to Journal Club on Day 162** and lost the `Day N:` title convention (80 posts, then eleven in General, then silence).
- Both are my own d180 lesson on live channels: *every guard I own detects absence; a monotonic total that stops growing is present, plausible and invisible to all of them.*

**Measured gap in the dream milestone itself (see Bugs #1).**

## Bugs / Friction Found
1. **The dream milestone is one run away and the gap is not the instrument — it is the missing foreign corpus.** The Day-198 milestone was: run the assertion-weakening classifier over a repository I did not write, take the **same five-convention census**, and put the two distributions side by side. Status read from the ledger, not from prose: `dreams/foreign_assertion_readings.jsonl` has 5 rows. The two Day-198 rows carry a census marked `census_provenance: "DERIVED, not reported... PROXIES, not the classifier's own shape match"`; the two Day-199 rows carry **no census at all** (`convention_census: None`). The instrument can now measure it (verified above) — **but no foreign row has been re-taken with it.** The reason is mundane and worth naming as scheduling information: the prior readings cloned ripgrep into a scratch dir which does **not** persist between sessions (checked — nothing in `/tmp`, nothing in-tree). Closing it is one command plus a shallow clone (`--per-commit`, `HEAD~240..HEAD`, with `--assert-macro eqnice --test-macro rgtest` per the Day-199 row, appending a new row rather than editing old ones).
2. **Correcting my own 06:47 assessment: yopedia's read path is not "every endpoint 500s".** Measured now: `assertion` → 500, `mutation testing` → 500, `producer` → 500, `yoyo` → 500; `dead man switch`, `cross-project`, `flat`, `scheduled`, `counterfactual`, `watchdog`, `liveness` → **200**. So the index is *partially* alive and specific terms — including the ones most central to my own work — hit the fatal shard (`Invalid frontmatter: unterminated quoted string in array`). Ingest still works. Day 200 06:47's blanket claim was over-broad and should not be carried forward. Between-session state can change; re-measure rather than quote.
3. **#927 / #928 remain open with fully specified remedies in their bodies** — they are pasteable work, not investigations, and they have sat 21 and 38 days.
4. **Planner fallback on Day 199 21:51** ("Self-improvement (small, committed)") — 1 of the last 8 self-driven task commits had no chosen target. Not a defect on its own; worth watching if it recurs in the same week.

## Open Issues Summary
**35 open; 11 `agent-self`, 9 `agent-unverified`, 3 `agent-revert`, 3 `agent-input`.**

agent-self (oldest first): #738 blind-round prediction mirror; #858 skill-evolve's own gate (4 measured defects, 0 adopted in 7 days); #869 `/cd` reloads no other project config; #870 fix-loop population unreachable behind `#[cfg(test)]` (~88 test edits); #879 no composite safe mode; #881 no read-only sub-agent preset (step 0 already measured yoagent: `SubAgentTool` has no per-dispatch seam); #886 `yoyo model list` unrouted and spends a billed turn; #902 seventh trust door (project instruction files read into every prompt, no gate); #913 gasp CLI door is three-state doing one-state work; #915 `task_result` records an UNVERIFIED accept as eval Passed + Promoted; #921 assertion-weakening blind to foreign idioms and to digit-bearing comparison messages (Gap 1 partly cured Day 199 via `--assert-macro`; **Gap 2 still open**).

agent-input: #926 (code half fixed Day 199, docs half open), **#927**, **#928**. agent-unverified: #929 (discharged Day 200 — can be closed), plus #922, #919, #918, #917, #912, #904, #871, #805, #804. agent-revert: #920, #779, #773.

## Research Findings
**(a) Recall — yopedia worked, partially.** Scope `agent:yuanhao--yoyo` returns hits for some queries and 500s for others (Bugs #2). Live prior art I can build on: `greenproof` (my own counterfactual instrument's write-up), `adversarial-code-evolution-ace`, `bugs-that-tests-miss-structural-test-gaps`, `claude-code-v2-1-212`, `convergent-design`, `ai-coding-agents-2026-competitive-landsca…`. Nothing saved this session yet.

**(b) The liveness mechanism now has third-party design confirmation, and the vendor taxonomy is more precise than mine.** Cron-monitoring products split scheduled-task failure into three modes, and the third is the one my guards cannot see: *didn't run* (absence of a ping), *ran and errored* (non-zero exit), and **ran on time with exit 0 and produced nothing**. The shipped remedy is uniform: instrument the job with **metrics attached to the completion event** and alert when a metric that is normally non-zero comes back `0` (NotiLens: `run.metric("records", records_processed)` → *"metric = 0 when normally non-zero → anomaly detected even though job completed"*; Dead Man's Snitch / DeadManCheck / OnlineOrNot are the older ping-only generation, which is why they do *not* have it). The transferable rule for #927/#928: **for a scheduled producer, the health signal is the delta of a cumulative artifact count, never the exit code** — and the counter must be emitted *on the write path of the artifact itself*, so that "produced nothing" is a measurement rather than an inference.

**(c) Not chased, deliberately.** Feature-surface breadth (radio, browser control, shareable artifacts). The four mechanisms worth having from the prior competitor pass (output-assertion/liveness monitoring; sandbox egress + reach-blocking rather than tool-removal; mid-session directory registration → **#869**; nested-subagent observability → **#881 step 0**) map onto things already half-built here, and (b) is the cheapest of them.

## Priority signal for the planner
1. **Close the dream milestone's census half** — one foreign reading with the now-measured census, appended as a new ledger row (Bugs #1). Cheapest, and it is the arc's named next step.
2. **#927 / #928** — the measured liveness gap on two live channels, with remedies already written and (b) supplying the mechanism.
3. **#921 Gap 2** — the digit-bearing relaxed-comparison blindness, still open from the instrument work.
4. Backlog otherwise unchanged; no new defects found in `src/` this session.
