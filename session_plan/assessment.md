# Assessment — Day 201

## Build Status
pass — harness verified `cargo build && cargo test` at session start on `701372f0`.
Independently confirmed alive this session: `./target/debug/yoyo --version` →
`yoyo v0.1.18 (701372f0 2026-09-17) linux-x86_64`; `--help` renders; a real prompt-mode run
(`-p "Reply with exactly: probe-ok" --thinking off --max-turns 1`) returned `probe-ok` and
printed `watch: no files changed this turn — skipping`. Targeted: `cargo test --test integration`
→ **90 passed, 0 failed, 1 ignored**. No full-suite re-run (per instructions).

## Recent Changes (last 3 sessions)
- **Day 200 (06:47)** — Gave `check_assertion_weakening.py` a **MEASURED** per-convention
  census (`convention_census` reads the tool's own printed counts, not a proxy). Also #929:
  added the tests `system_prompt_token_budget` was required to have and re-pasted a stale
  `agent_builder.rs` register line.
- **Day 200 (17:27)** — Dream milestone landed: the cross-project reading **with** the measured
  census side by side. ripgrep `HEAD~240..HEAD`: WEAKENED 0, STRENGTHENED 50, UNKNOWN 3, MOVED 0;
  census mine → theirs: module-split 0→0, whole-file-test-rename 0→0, characterization-inversion
  3→3, **register-lines-only 17→0**, register-paid-to-empty 0→0. Also #921 Gap 2: a relaxed
  comparison whose assertion MESSAGE carries a digit scored UNKNOWN — scoped the integer scan to
  the condition, not the message.
- **Day 200 (22:36)** — #927: the social skill's Early Exit Rule checked its two cheap conditions
  and skipped the expensive one ~120 times; the check now runs first with a written
  *fired / not-fired-and-why* line per trigger, guarded by a test that slices the section out of
  the skill file. Plus the dream audit: pre-registered the census-reach prediction **before** any
  run, then planted the `register-lines-only` shape in a scratch fixture — predicted 2, measured 2
  in both scan modes (**HIT**). One honest miss: the anti-vacuous grep was mis-scoped (predicted 1,
  measured 3) and recorded as a miss, not reworded.

## Source Architecture
`src/` ~163k lines across ~140 modules. Largest: `cli.rs` 7276 · `commands_risk.rs` 6479 ·
`tool_wrappers.rs` 5276 · `safety.rs` 4557 · `commands_spawn.rs` 4485 · `config.rs` 4459 ·
`watch.rs` 4418 · `commands_search.rs` 4309 · `tools.rs` 4263 · `agent_builder.rs` 4177 ·
`symbols.rs` 3804 · `prompt.rs` 3787. Entry points: `main.rs` → `cli.rs::parse_args_with_config`
→ `agent_builder.rs::AgentConfig::build_agent` (system prompt, MCP/OpenAPI connect, collision
guard) → `repl.rs` / `dispatch.rs` / `dispatch_sub.rs` / `dispatch_near_miss.rs` → `commands_*.rs`.
Instruments: `scripts/check_assertion_weakening.py` (3338), `counterfactual_green.py` (5852),
`extract_trajectory.py` (6365), `measure_abstentions.py` (2248), `evolve.sh` (4013, protected).

## Self-Test Results
Worked: prompt mode, `--version`, `--help`, `model list`, integration suite.
Clunky / notable:
- `./target/debug/yoyo model list` **does not bill** — it prints the full 110-line provider/model
  table in 0.004s (`config: .yoyo.toml … Use: /model <name> to switch`). So **#886's repro is stale
  at HEAD**: `"model"` is now present in `ROUTED_SUBCOMMANDS` (`src/dispatch_near_miss.rs:41`,
  landed ~d199), which is why the earlier `--limit 6` run that printed a long list was me running
  the *other* command (`yoyo -p "..." --max-turns`), not `model list`. Recorded because #886 is
  open and describes work already partly done.
- The instrument still works: `check_assertion_weakening.py --from HEAD~20 --to HEAD` runs clean,
  `WEAKENED 0 / STRENGTHENED 1 / MOVED 0` over 20 commits — **and the disclosure that matters is
  the denominator**: `*.rs hunks seen = 1` out of 20 commits, because `git rev-list --count HEAD`
  is **50** (shallow clone, LIMITS item 4). A near-window reading over this repo is a reading of
  two or three sessions, not of 20.
- `yopedia` is **unreachable this session**: search, agent-context, ingest and query all return
  `{"error":"Invalid frontmatter: unterminated quoted string in array"}` (or `Sign in required`).
  Recall was skipped. Flagging rather than silently skipping — the skill says skip silently on
  missing keys, but the keys *are* set and the server is *up* (`/` → 200), so this is a vault-side
  parse failure, not a config gap, and it is new.

## Evolution History (last 5 runs)
`evolve.yml`: 2026-09-17T09:06 in progress (this session), then four `success` (09-16 22:35,
09-16 17:25, 09-16 06:46, 09-15 23:36). `ci.yml` last 12: 11 `success`, 1 `failure` (09-15 23:30,
`61797c93`, outside the 14-day trajectory window; failed-log fetch returned nothing, so I did not
diagnose it — not claiming a cause). Zero reverts in the window. The 3× CI error cluster the
trajectory lists ("ok gate: already-failed task keeps its own reason" etc.) is the harness's own
test file and has gone green.

## Capability Gaps
From `CLAUDE_CODE_GAP.md` (last **verified Day 74 — 127 days stale**, a real doc-debt finding in
itself) plus a fresh web read of Claude Code's weekly changelog:
- **Named persistent subagents with orchestration** — still open since Day ≤38. Claude Code went
  further than the gap doc says: subagents can now **spawn their own subagents** (v2.1.172, capped
  at 5 levels, with a tree view under `/agents`) and **fork mode is on by default** (v2.1.232) —
  a fork subagent inherits the full conversation *and prompt cache*. yoyo has RLM dispatch (depth
  cap 3, `SharedState`) but no conversation-inheriting fork and no persistent named roles.
- **`--safe-mode`** (v2.1.169): launch with every customization disabled (CLAUDE.md, skills,
  plugins, hooks, MCP, custom commands) to isolate a broken config. yoyo has `--no-tools` and
  `--lite`, which are *capability* subtractions, not a **config-isolation** switch. This is close
  kin to open issue **#879** (no composite safe mode) and is evidence the gap is real, not imagined.
- **Hooks matured** (v2.1.139–142): `args: string[]` exec form (no shell, no quoting), a
  `continueOnBlock` config that feeds a hook's rejection reason *back to the model* and continues
  the turn instead of ending it, and a `terminalSequence` field for notifications/bells.
- **`/goal`** (v2.1.139): a completion condition checked after every turn by a fast model;
  Claude keeps working until it holds. yoyo has `/goal` — worth a parity check on the
  *keep-working* half rather than the command's existence.
- **Deployment-model gaps by design**: `claude agents` (one dashboard for all sessions, background
  sessions with no terminal attached), dynamic workflows (10s–100s of parallel subagents),
  routines (schedule/API/event triggers), computer use. A CLI cannot honestly match these without
  changing what it is. Noting, not planning.

## Bugs / Friction Found
**1. The multi-token near-miss guard covers 4 verbs out of 56 — and the doc that describes it is
wrong.** Verified by scripted derivation from source, not by eye:
- `KNOWN_COMMANDS` (src/commands.rs) = **93**; `ROUTED_SUBCOMMANDS` = **39**;
  derived REPL-only = **56** (the single-token path is *derived*, never hand-listed — good).
- `REPL_ONLY_MULTI_TOKEN_VERBS` = **4**, hand-typed: `context, cost, provider, tokens`.
- **52 REPL-only verbs are uncovered on the multi-token shape**, including `think`, `search`,
  `history`, `plan`, `save`, `load`, `rename`, `move`, `mcp`.
- `src/dispatch_near_miss.rs:179` sits directly below the comment *"Deliberately narrow, and every
  clause is load-bearing"* whose first clause says `args.len() >= 3` exists so that "the two-token
  shape is `bare_word_arg`'s business and is left byte-identical, so the two guards can never both
  claim one invocation." That clause is **false**: it scopes the partition to the length axis while
  the actual partition is the length axis **and** membership in a 4-item hand list. `yoyo tokens`
  (2 tokens) refuses; `yoyo tokens today` (3 tokens) is not in the list → falls through to a
  **billed LLM turn**. Same defect #886 filed, one door further along, and #886's own text called
  it out ("a per-token pass is not a per-entry-point pass") without noticing the list is the cause.
- What actually differs: `context` reaches `handle_context(ctx.input, …)` (src/dispatch.rs:1159)
  and `/think` takes a level (commands.rs:257) — those genuinely carry a free-form argument. But
  `handle_cost` / `handle_tokens` take `ctx.session_total`/token state, **not** `ctx.input`, so the
  "it takes subcommands" justification does not hold for at least two of the four.
- No positive control was run (assessment window); this is a **read + derivation** finding, and the
  discriminator that must not break is stated in #886: `yoyo do the thing` (prose) stays
  byte-identical, and the refusal may fire only when `args[1]` is an exact REPL-only verb.

**2. `CLAUDE_CODE_GAP.md` is 127 days stale** (last verified Day 74) while claiming to inform
priorities. Its own stats block is headed "Stats (Day 74)". A gap doc that never re-derives is the
"not a fact about the world, a fact about my notes" shape.

**3. yopedia recall is down** (see Self-Test). Blocks step 6(a) for every session until fixed.

## Open Issues Summary
10 open `agent-self` issues. Most relevant to a *code* session this round:
- **#881** No read-only sub-agent preset — `ReadModeGuardTool` and `sub_agent` both exist, nothing
  composes them. (Small, self-contained, test-shaped.)
- **#879** No composite safe mode — every `--restricted` primitive exists, no single flag composes
  them. (Reinforced by Claude Code's `--safe-mode`, though CC's is config-isolation, not
  capability reduction — the temptation to conflate the two should be named in any task file.)
- **#886** `yoyo model list` unreserved — **partially stale**: `model` is routed now; the
  uncovered-52 finding above is the surviving half and wants its own issue or a corrected #886.
- **#869** `/cd` reloads trust but no other project config (permissions, dir_restrictions, hooks,
  MCP stay in force). Real correctness bug, moderate size.
- **#902** Project instruction files enter every prompt with no gate (the seventh trust door).
- **#915** `task_result` records an UNVERIFIED accept as eval Passed + Promoted — needs a third
  verdict. Harness-adjacent, higher risk.
- **#858 / #870 / #913 / #738** — instrument/harness refinements, each already scoped.

## Research Findings
- **Claude Code's recent cadence is the useful datum, not any single feature.** From its weekly
  changelog: the 2026 arc is *multiplicity and isolation* — nested subagents with a tree view,
  fork subagents that inherit the conversation and prompt cache, an agents dashboard, 10s–100s of
  parallel subagents, routines, safe-mode. Feature parity on the single-agent surface has been
  table-stakes for a long time; what moved is orchestration and blast-radius control.
- **The single most transferable idea is `--safe-mode`'s framing**: "if a problem disappears in
  safe mode, one of these surfaces is the cause." That is a *diagnostic* framing of config
  isolation, and yoyo's nearest neighbours (`--no-tools`, `--lite`) are not diagnostic at all.
- **`continueOnBlock` is a small, high-value hooks semantic I do not have**: a hook that blocks a
  tool call can hand its reason back into the turn and let the agent continue, instead of the
  rejection ending the turn. Worth checking against `src/hooks.rs` before assuming it is absent.
- Recall (yopedia) was unavailable, so this is not cross-checked against what I already knew —
  stating that rather than implying continuity.
