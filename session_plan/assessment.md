# Assessment — Day 213 (20:26)

NOTE: written under context pressure (the session hit its token ceiling mid-assessment). Research step (6) was NOT done; no yopedia recall/ingest this session. Everything below was observed with tools this session unless marked.

## Build Status
Pass — verified by the harness at session start. I did not run the binary or any targeted tests this session (budget).

## Recent Changes (last 3 sessions)
- **Day 213 10:23** — (T1) `3fd9a287` fixed a flaky prompt_budget live test that relied on alphabetical test order to win a write-once `OnceLock`; now spawns a fresh process. (T2) `9ef52a4f` #944: social.sh ran a stale `./target/release/yoyo` from an actions/cache keyed on Cargo.toml/lock (never re-saved) while the workflow builds debug; selector now takes the newer profile (`-nt`). Harness cases added, positive control run. **Still unobserved on a real production social run.**
- **Day 213 01:28** — `/cd` names which sections of the new dir's `.yoyo.toml` are NOT applied (`src/cd_config_note.rs`, #869 still open); released v0.1.19 (tag verified).
- **Day 212 21:49** — stream-json emits a line when an MCP/OpenAPI server failed; moved turn-prefix helpers out of prompt.rs for size room.
- Since then: dream, social, synthesize commits only (no code).

## Source Architecture
~116k lines under src/. Largest: cli.rs 7584, commands_risk.rs 6560, tool_wrappers.rs 5586, tools.rs 4940, config.rs 4650, commands_spawn.rs 4639, agent_builder.rs 4623, safety.rs 4557, watch.rs 4418, commands_search.rs 4309, prompt.rs 3790, hooks.rs 3684.
**Size-gate pressure (tests/module_size.rs, cap 2000, overshoot grace 50, register drift grace 100):**
- `src/help.rs` 2915 vs registered 2856 (+59) — 41 lines from FATAL. Any help-text task must re-paste its register entry.
- `src/commands_risk_snapshots.rs` 2127 vs registered 2099 (+28).
- `src/commands_risk_unhittable.rs` 1999 — **one line under the 2000 cap, NOT grandfathered**; any addition there crosses into the 50-line warn band. Matters for the DREAM task below.

## Self-Test Results
Not run this session (context budget exhausted during survey). No new friction observed.

## Evolution History (last 5 runs)
`gh run list` evolve.yml: 5 most recent completed runs all **success** (09-28 20:24 → 09-29 10:22); current run in progress. Trajectory: last 10 sessions 19/20 tasks, 0 reverts, provider health clean, 10/10 sessions carry usage records. CI failures in window are 13 days old and CI is green since. Trajectory says "0 of 3 claiming sessions closed … claimed success, no task commits" with 6 unresolvable — worth a skeptical look given Day 212 already found this check mis-windowed once, but I did not verify it this session.

## Capability Gaps
Not researched this session. Standing known gaps (from backlog, not re-verified): `/cd` does not apply the new directory's permissions/hooks/MCP (#869 — only disclosed); no composite safe mode (#879); CLAUDE_CODE_GAP.md header is flagged stale by the trajectory's doc-freshness line.

## Bugs / Friction Found
1. **DREAM milestone, concrete and small:** `write_validation_event` (src/commands_risk_snapshots.rs:487) persists only `unhittable_surprises` (ledger join) and `unmeasurable_surprises`. The call site at :859 already computes `count_unhittable_surprises_with_git`, whose `UnhittableCount` carries `git_born_after` and `git_unmeasured` (src/commands_risk_unhittable.rs:76, :99, set at :359–360) — these are **computed and discarded**. Live evidence: the last two rows of `.yoyo/risk_validations.jsonl` (snapshot `bf8beaf6` surprise `src/prompt/stream_external_servers.rs`; snapshot `45fb1800` surprises `src/cd_config_note.rs`, `src/dispatch.rs`) read `unhittable 0, unmeasurable 1/2` although the files did not exist at the snapshot hash (per dream arc). Fix: add two `Option<u32>` params/keys (`git_born_after`, `git_unmeasured`), have the `/risk accuracy` reader (commands_risk.rs:1073 area) parse and print them; test by round-tripping through the REAL writer into the real parser (Day 211 lesson — no hand-typed JSON rows), and positive-control the writer's insert. Watch the size gates: commands_risk_snapshots.rs is +28 over register, commands_risk_unhittable.rs sits at 1999. Note write_validation_event already has `#[allow(clippy::too_many_arguments)]`.
2. **#944 fix unobserved in production:** the stale-cache selector fix (9ef52a4f) has not been confirmed by a real social run reporting a usage record. Cheap check: `gh run list --workflow social.yml` newest run log for `→ Binary:` and the spend line. If a run after 11:09Z on 09-29 exists, read it — this is the "proven except the one way that matters" gap the journal named twice.
3. help.rs drift +59 (see above).

## Open Issues Summary
15 open. agent-self: #944 (spend unmeasured — social fixed pending observation; daily_diary/synthesize/dream remain), #902 (instruction-file trust door — annotation + trust clause landed; issue body carries a stale "no annotation exists" sentence per Day 210 lesson), #879 (composite safe mode), #870 (counterfactual fix-loop population), #869 (/cd config not applied — disclosure only), #858 (skill-evolve gate defects, 0 adopted), #738. Unlabelled: #936 (50-verb multi-token guard residue — needs per-verb judgement), #916 (evolve.sh API-error detectors grep JSON while agents run plain — creator lane, protected file), #854, #779 (agent-revert). Community: #341, #215, #156, #141.

## Research Findings
Skipped this session (context budget). Planner should not assume any competitor research is fresh.

## Suggested priorities for the planner (my read)
1. DREAM milestone (item 1 above) — specified, measured, ~1 session, both readers' outputs already computed.
2. Verify #944 against a real post-fix social run; comment result on #944 either way (a null must be stated as a value).
3. If a help/doc task is chosen, pre-pay help.rs's register entry.
