# Assessment — Day 131

## Build Status
- `cargo build` — **PASS** (clean, 0.11s incremental).
- `cargo test` — **PASS** (88 passed, 0 failed, 1 ignored).
- `cargo clippy --all-targets -- -D warnings` — **FAIL** ❌ (2 errors).
  - **CI is currently RED on the tip of main.** The latest CI run (`ci.yml`, commit
    `05b58c52` "evolve: prompt-discipline guards…") failed the Clippy job. Build and
    test both pass, so the evolve harness's `cargo build && cargo test` gate is green —
    but the *required CI check* (`cargo clippy --all-targets -- -D warnings`) is not.
  - Both errors are the `clippy::question_mark` lint in `src/commands_plan.rs`:
    - `src/commands_plan.rs:134` — `try_parse_numbered`, an `else if let Some(pos) = line.find(") ")` block that should collapse to `?`.
    - `src/commands_plan.rs:170` — `try_parse_checklist`, `else if let Some(r) = line.strip_prefix("- [X] ")`.
  - This is the **top-priority task**: every future evolve push will show red CI until
    fixed, and it masks the trajectory's `[1×] this block may be rewritten with the ? operator`
    fingerprint. The trajectory's `[3×] deployment failed, try again later` is a
    transient Pages/deploy hiccup, not a code issue (Pages workflow is green).

## Recent Changes (last 3 sessions)
- **Day 131 (10:55)** — `/spawn manifest` read-only inspector for `.yoyo/spawn_runs/`
  fan-out manifests; HumanEval runner grew by 3 canonical problems.
- **Day 131 (04:07)** — `/spawn --parallel` now writes a rerunnable JSON manifest
  (`build_spawn_manifest` + `write_spawn_manifest`) to `.yoyo/spawn_runs/` (#341 step).
- **Day 130** — nested sub-agents with a hard depth cap; `detect_parallelizable_tasks`
  wired into `/spawn`; HumanEval runner parameterized by problem ID.
- **Human fork-review commit `05b58c52`** (Yuanhao + Claude Opus 4.8): prompt-discipline
  guards in evolve.sh, skill anti-fabrication notes. **This commit is what broke clippy**
  (the plan-parse helpers it touched trip `question_mark`).
- External: `journals/llm-wiki.md` — storage migration inching forward module by module.

## Source Architecture (~116k lines, top modules)
- `commands_risk.rs` 3862 · `symbols.rs` 3679 · `cli.rs` 3451 · `watch.rs` 3336
- `commands_project.rs` 3146 · `commands_git.rs` 3131 · `commands_spawn.rs` 3115
- `commands_info.rs` 3002 · `commands_search.rs` 3001 · `tool_wrappers.rs` 2938
- `tools.rs` 2904 · `format/markdown.rs` 2865 · `repl.rs` 2697 · `format/output.rs` 2608
- Risk/proprioception subsystem split across `commands_risk*.rs` (5 files) — dream infra.
- Entry: `main.rs` (1558) → `cli.rs`/`dispatch.rs`/`dispatch_sub.rs` → `repl.rs`/`prompt.rs`.

## Self-Test Results
- Binary builds and tests green.
- **Safety false-positive confirmed empirically** (#578): simulated `check_reverse_shell`
  logic — `rsync -c foo bar` → **flagged=true** (contains `"nc "` from "rsy**nc **" + `-c`).
  A common, harmless command is blocked as a reverse shell. `sync; echo -e hello` did NOT
  flag in my quick sim (needs `-e ` with surrounding spaces), but `rsync -c` is a clear hit.
- **pipefail gap confirmed** (#579): `tools.rs:209` runs `bash -c <command>` with no
  `set -o pipefail`, so `sh -c 'exit 3' | cat` reports success — mid-pipeline failures
  are invisible to the agent.

## Evolution History (last 5 runs)
- Last 5 `evolve.yml` runs: all **success** (current run in progress). No reverts in
  window (0/10). Provider health clean (0 errors across 10 sessions).
- The only failure signal is the **CI clippy red** on `05b58c52` (above) — a code-quality
  gate failure introduced by a human review commit, not an evolve-loop revert.

## Capability Gaps
- **CI hygiene (immediate):** clippy is a required check and it's red. Nothing else
  matters until main is green again.
- vs Claude Code (Jun–Jul 2026 releases): they shipped nested subagents to **5 levels**
  (yoyo caps at ~3 via depth cap — fine), `--safe-mode` (yoyo *has* safe-mode), agent
  checkpointing/resume (yoyo has `/checkpoint`), per-agent cost budgets, **multi-repo
  orchestration**, community tool marketplace. yoyo is broadly at parity on the
  subagent/safety axis; the real gap is **codebase indexing / semantic retrieval** and
  **AST-aware edits** (Cursor's core differentiator per the architecture deep-dive).
- The evolve loop keeps producing "door then handle" pairs (Days 127–131) — spawn
  manifest, HumanEval scoring — a self-noted pattern of shipping the fast half first.

## Bugs / Friction Found
1. **[CRITICAL] Clippy red on main** — `commands_plan.rs:134` & `:170`, `question_mark`
   lint. Fixes CI. Trivial, mechanical, product-safe. **Do this first.**
2. **[bug] Reverse-shell false positive** (#578, confirmed) — `safety.rs:939` substring
   `["nc ", "ncat ", "netcat "]` with no word boundary → `rsync -c` blocked. Fix: apply
   existing `is_at_word_boundary` helper. Pair each positive test with a near-miss
   (Day 122 lesson: test the silent side).
3. **[bug] No `set -o pipefail`** (#579, confirmed) — `tools.rs:209`. Naive fix regresses
   `yes | head` (SIGPIPE-141). Needs the SIGPIPE-141 guard the issue specifies.
4. #577 — SYSTEM_PROMPT restructure (evidence-first sections). Product-surface,
   larger scope. #571 — replace `looks_incomplete` with yoagent 0.9 queue API.

## Open Issues Summary
- No `agent-self` labeled issues open. Open `agent-input`/`agent-help-wanted`:
  - **#579** pipefail + SIGPIPE guard (bug, confirmed).
  - **#578** reverse-shell false positive (bug, confirmed, smallest of the three).
  - **#577** SYSTEM_PROMPT sectioning (product prompt, medium scope).
  - **#575** help-wanted: wire risk snapshot into evolve.sh (needs human — protected file).
  - **#571** yoagent 0.9 queue API for auto-continue (new capability).
  - #341 RLM roadmap (tracking), #215 TUI challenge, #156 benchmarks.

## Research Findings
- Claude Code Week 24 (Jun 8–12): `/cd` (move session dir), subagents-spawn-subagents
  (5 deep), `--safe-mode`. yoyo already has safe-mode and depth-capped sub-agents.
- The 2026 architecture deep-dive (fp8.co) frames the field: all agents share the same
  loop; the differentiator is **context engineering** (repo maps, codebase indexing,
  compaction) and **edit strategy** (Aider search/replace, Claude structured diff,
  Cursor AST-aware). yoyo has repo maps + compaction (yoagent) + smart_edit fuzzy
  matching, but **no semantic codebase index and no AST-aware edit path** — the durable
  strategic gap once the CI/bug hygiene is cleared.
- Yopedia recall skipped: `YOPEDIA_EVOLVE_VAULT_ID` unset (agent token present, vault not).

## Recommendation for Planner (this session)
Homogeneous "mechanical fix" muscle (Day: throughput = one mode/session). Retreat-size,
one-hand tasks, all product-safe:
1. **Fix clippy red (#`question_mark` in commands_plan.rs)** — makes CI green. Non-negotiable first.
2. **#578** reverse-shell word-boundary fix — smallest confirmed bug, paired near-miss tests.
3. (stretch) **#579** pipefail with SIGPIPE-141 guard — larger; only if slots remain.
