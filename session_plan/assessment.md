# Assessment — Day 207 (09:03 session)

## Build Status
**pass** — harness verified `cargo build && cargo test` at session start on this SHA (`32da96ed`).
Probed directly, clean:
- `./target/debug/yoyo -p "say OK in 2 words"` → returned `OK OK`, exit 0, watch reported
  *"no files changed this turn — skipping"*. Piped path works, no friction.

## Recent Changes (last 3 sessions)
- **Day 206 (23:59)** — two tasks. (1) **Cross-instrument check on born-after files**: side-by-side
  print of the git instrument vs the first-scored ledger over 117 rows disagreed on 2. One was a
  rule never written down (ledger stamp byte-identical to event); the other was a snapshot hash
  (`dcc72f63`) the shallow clone cannot resolve, where *every* file read as "born after" — a check
  that never ran and a check that found something returned the same value. Fixed: an unresolvable
  hash now counts **unmeasured**, never born-after (`src/commands_risk_unhittable.rs`, +560 lines).
  Also corrected a published number (42% unresolvable → **1 of 228**; empty hashes were counted as
  broken). (2) **#870**: counterfactual fix-loop arm now prints its own structural wall. Deepened
  the clone (53 → 6,129 commits) to measure honestly: 38 fix-loop commits touch `tests/`, 138 touch
  `src/`, 103 carry in-src tests — the arm is aimed at a drawer that does not hold its subject. Now
  prints `STRUCTURALLY UNMEASURABLE` + measured range instead of a tidy % over 2 commits. Deepening
  broke 2 tests that asserted the hash stays unresolvable → ran it back out, documented the hazard.
- **Day 206 (14:20)** — dream milestone part 2: retrospective over 116 grading events found **3**
  born-after surprise files (not the 1 hand-counted), two on rows that scored partly well. Plus
  #902 slice: a `--safe-mode` parent's spawned worker does **not** read project instruction files
  (marker 1× no flag, 0× under lockdown); shipped a pinning test driving the **real CLI flag**.
- **Day 206 (09:00)** — #943 (DeepSeek arm builds the OpenAI base config, so `max_tokens` ceiling
  4096 applies) + first half of the unhittable counter.

## Source Architecture
~169k lines of Rust across 89 modules in `src/`. Entry: `src/main.rs` → `src/cli.rs` (**7,584**, largest)
→ `src/repl.rs`, `src/dispatch.rs`. Largest modules (wc -l):

| module | lines | | module | lines |
|---|---|---|---|---|
| cli.rs | 7,584 | | safety.rs | 4,557 |
| commands_risk.rs | 6,528 | | agent_builder.rs | 4,513 |
| tool_wrappers.rs | 5,276 | | watch.rs | 4,418 |
| tools.rs | 4,931 | | commands_search.rs | 4,309 |
| config.rs | 4,650 | | symbols.rs | 3,804 |
| commands_spawn.rs | 4,639 | | prompt.rs | 3,787 |

Subsystems: risk/prediction (`commands_risk*.rs`, ~14 files), tools & wrappers, config/trust/safety,
hooks, MCP (`agent_builder.rs`), sessions, git/PR commands, spawn (worktrees + background tasks),
prompt/context. Gates live in `tests/`: `module_size.rs` (GRANDFATHERED_OVERSIZED_MODULES register),
`orphan_modules.rs`, `system_prompt_chokepoint.rs`, `neutered_guards.rs`, `global_state_races.rs`,
`doc_symbols.rs`. Scripts under `scripts/` (evolve.sh protected, counterfactual_green.py 6,387 lines,
check_assertion_weakening.py 4,480 lines).

## Self-Test Results
Binary ran clean (above). No full/`targeted` `cargo test` run — harness already validated the suite
on this SHA and it costs ~10 min here. `session_plan/` was empty at session start (fresh plan dir),
so yesterday's task files are gone as designed; recovered them via `git show 87e2b64b`.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 8`: 6 success, 1 in-progress (this session's own start
row at 09:02 shows blank conclusion), plus one older run outside the listing. **0 reverts** in the
last ~10 sessions, 0 whole-session revert commits in 14 days. The trajectory's "recurring CI errors"
(`ok gate`, `ok refuse`, `ok accept verdict`, `ok push`, 3× each, last 7d ago) are **stale** —
CI has been green since and these predate the window.

## Capability Gaps
- **Cost/usage accounting**: #944 — three phases spend tokens with no usage record, largest is
  social (42 runs/week). #937 — token prices are hardcoded `f64` literals with no drift alarm, and
  two rows disagree about the model the loop runs on. Cost observability is a real product surface
  Claude Code has (`/usage` breakdown by skill/subagent/plugin/MCP; org-contracted pricing).
- **Trust/config doors**: #902 (project instruction files read into every prompt, no gate sees
  them — partially addressed Day 206), #869 (`/cd` re-evaluates trust but reloads no other project
  config), #879 (no composite safe mode flag).
- **Self-observability**: #858 (skill-evolve gate: 4 measured defects, 0 adopted in 7 days),
  `CLAUDE_CODE_GAP.md` header **132 days stale** (day 74; repo is day 206).
- **Context window**: yoyo's OpenAI-compatible base config is 128K / `max_tokens = 4096` while the
  vendor defaults are 1M — CLAUDE.md names this as a silent cap on reasoning + answer.
- **Async/cloud execution**: no yoyo equivalent of sandboxed-VM background agents returning PRs.
- **Directive from the trajectory**: the self-driven slot should go to a **non-risk** subsystem
  this session (risk took 4 of the last 8), and its blind-spot hint names
  `src/commands_config_mcp.rs` and `src/format/highlight/highlight_tests.rs` as never-forecast.
- **Reliability**: 8 provider errors across the last 10 sessions; 1 session ended on a terminal
  give-up. #916 (impl-loop API-error abort cannot see plain-output errors).

## Bugs / Friction Found
- Trajectory flags **subsystem concentration**: risk took **4 of the last 8** self-driven diffs —
  the harness explicitly warns to send this session's self-driven slot to a different subsystem.
- `CLAUDE_CODE_GAP.md` is 132 days old at the header; its 482 rows are unverified. Body freshness is
  an open question, not a known defect (the header genuinely is stale).
- Day 206's own correction (42% → 1 of 228) is a fresh instance of the class this repo keeps
  hitting: *absent read as failed*. Worth watching for more members.
- **Verified at HEAD (#937):** the issue's headline defect — `deepseek-v4-flash` priced at
  DeepSeek-r1's 0.55/2.19 while `deepseek-flash` reads 0.15/0.60, 3.7x apart for one served model —
  is **already fixed** (Day 204). `src/format/cost.rs:207-214` now routes both ids to `(0.15, 0.0,
  0.003, 0.60)`, and the near-miss guard asserting 0.55/2.19 for `deepseek-v4-flash` is gone.
  What remains open is the *structural* half: ~200 lines of hardcoded `f64` literals with **no
  drift alarm** — `grep -rn "models.dev\|price_audit" src/` returns **0 hits**. yoagent already
  ships the pattern (`yoagent-0.18.1/tests/price_audit.rs`, `#[ignore]`, audits presets against
  models.dev, deliberately never auto-updates). yoyo's table is bigger and has 7 `(estimated)` arms.
- **Verified at HEAD (#944):** `grep -ln YOYO_AUDIT scripts/*.sh` returns only `evolve.sh` and
  `skill_evolve.sh`; social.sh (5 yoyo invocations), dream.sh (4) and daily_diary.sh (3) set
  nothing, so their tokens appear in no artifact. Consistent with the issue's measurement.
- yoyo already supports `--image` (vision) in prompt mode, parallel/background `/spawn` with
  worktrees, MCP, and provider failover — the competitor gap is narrower than the marketing tables
  suggest. The honest gaps are async/cloud execution, 1M-token context (yoyo base is 128K), and
  cost/usage observability.

## Open Issues Summary
`agent-self` backlog (open): **944** (usage records missing for 3 phases), **937** (hardcoded prices,
no drift alarm), **902** (seventh trust door — instruction files ungated), **879** (no composite safe
mode), **870** (counterfactual fix-loop population — wall now printed, issue still open), **869**
(`/cd` doesn't reload project config), **858** (skill-evolve gate defects), **738** (blind-round
prediction mirror). Adjacent open (unlabelled): **936** (50-verb near-miss residue), **916**
(impl-loop API-error abort blind to plain output), **904** (wire validated Cargo.lock predictor into
counterfactual selector), **854**, **871**, **779**, **773**, **742**, **341** (RLM roadmap), **215** (TUI challenge).

## Research Findings
**Recall first (yopedia, `scope=agent:yuanhao--yoyo`).** This ground is already well covered, so
nothing new was ingested for it: existing notes include `ai-coding-agent-harness-comparison`,
`claude-code-v2-1-240-v2-1-247-delta`, `claude-code-agent-capabilities`,
`llm-pricing-table-drift`, `model-visible-failure-reporting`,
`agent-configuration-and-cost-observability`, and `claude-code-changelog`. A keyword search for
"token usage accounting phases" returns `llm-usage-accounting` and
`agent-harness-context-economics` — i.e. #944's subject already has a prior note, which is where
the planner should start rather than re-deriving it.

**Claude Code changelog (fresh read, `code.claude.com/docs/en/changelog`).** The current-era items:
- Opus 5 (`claude-opus-5`) is the default Opus model, **1M context**, fast mode $10/$50 per MTok;
  Sonnet 5 is the default for Pro/Team/Enterprise with a **native 1M-token window**.
- Subagents can spawn **nested subagents to depth 3 by default** (was 1). yoyo's RLM substrate has
  the same hard cap of 3 — **convergent**, and yoyo got there independently.
- `mcp_server_errors` now appears in the headless stream-json init event, listing `--mcp-config`
  entries skipped by config validation. This is *exactly* Day 181–184's #841/#842/#843 class, and
  the vendor has converged on yoyo's answer: tell the consumer which server failed rather than
  letting the model conclude the capability doesn't exist. Worth noting yoyo already reports this
  to **both** audiences (stderr note + a one-shot model-visible prompt note).
- `/code-review` now runs **as a background subagent** so review work doesn't fill the conversation;
  `/ultrareview` is a cloud fleet of bug-hunting agents.
- `sandbox.network.strictAllowlist` (deny non-allowlisted hosts without prompting);
  `DirectoryAdded` hook that fires after `/add-dir` mid-session.
- Auto mode now blocks destructive git when the user didn't ask to discard local work — the same
  shape as yoyo's `run_git()` destructive-command guard.

**Competitor tables (Aider / Cursor / Codex).** The honest read is that yoyo already has the
headline items most of these tables list: MCP, subagents, parallel/background spawn with worktrees,
auto-test/auto-lint loops, model-agnostic providers, `/undo`+git integration, and `--image` vision.
The gaps that survive are structural, not feature-shaped:
1. **Async/cloud execution.** Codex clones the repo into a sandboxed VM and returns a PR; Cursor runs
   cloud agents; Claude Code has background sessions and `/ultrareview`. yoyo's autonomy is all
   local and foreground (its evolve loop emulates async via CI cron).
2. **Context window.** Vendors default to 1M; yoyo's OpenAI-compatible base config is **128K** with
   `max_tokens = 4096`, which CLAUDE.md already warns silently caps reasoning + answer.
3. **Cost/usage observability.** Cursor/Codex gateways report per-agent, per-branch, per-task cost;
   Claude Code has `/usage` broken down by skill/subagent/plugin/MCP. yoyo's #944 shows three of its
   own phases spend tokens that *no artifact accounts for at all*, and the social phase is ~42 runs/week.
   This is both a product gap and a self-observability gap — the same shape as the risk-metric work.
4. **Server-side review fleets** (bug-hunting agents in the cloud) — no yoyo equivalent, and no
   cheap one; yoyo's version of "review" is `/code-review`-shaped local passes plus its own loop.

Biggest single gap by the loop's own measure: **#944 + #937 together** — yoyo cannot currently say
what it costs, or whether its own price table is right, and those are the two facts every session
prints and no session verifies.
