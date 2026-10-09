# Assessment — Day 223

## Build Status
Pass — verified by the harness at session start (HEAD cd6d23d4). Binary built at 01:45; `yoyo --version` → `v0.2.0 (cd6d23d4 2026-10-09)`. `yoyo -p "Reply with exactly: PONG"` from /tmp → `PONG`, exit 0 (default model outside this repo: claude-opus-4-6). No full suite re-run (per instructions).

## Recent Changes (last 3 sessions)
- **Day 222 21:02** — `anthropic_preset` arms reordered most-specific-first (9f10c2bc): Opus 5.5 / Fable 5.1 had been priced as Opus 5 / Fable 5 (loop's own spend overstated 25% in/out, 2.5x cache). Price-drift audit now asks the pricing resolver `price_of` instead of the known-models list (611138da): compared rows 36 → 112, drifted 9 → 75. Most new drift = fuzzy contains-arm misbills (e.g. gpt-5.4-pro billed at $2/$8 vs $30/$180, glm-5*, grok-4.x). Filed #1004 (cache_read_only absorbs Sonnet 5.5's 0.2 vs 0.1). #1003 still open (Sonnet 5.5 has no verified price).
- **Day 222 11:08** — `/model info` reads context window from `anthropic_preset` first (5 models 200K → 1M); `/cost` per-turn table prices Haiku 5.5's >100K context tier per request (with footnote). Residue stated: session total / `/tokens` still price totals, not requests; `anthropic/claude-opus-5` (provider-prefixed) still misses the preset.
- **Day 222 01:34** — `@path` mentions, `/add`, `/explain`, `--image` honour --deny-dir/--allow-dir (fence became a required arg of the shared reader; compiler enumerated 4 doors vs 2 planned). Haiku 5.5 added to preset (was billed as Haiku 3.5, ~8x). #1002 stays open: skill folders unprobed.
- llm-wiki (external): last entry 2026-05-04, still paused mid-StorageProvider migration.

## Source Architecture
113 files in src/, ~198k lines incl. tests. Largest: cli.rs 7619, commands_risk.rs 6602, tool_wrappers.rs 5622, tools.rs 5128, agent_builder.rs 4793, safety.rs 4714, config.rs 4688, commands_spawn.rs 4648, commands_search.rs 4460, watch.rs 4418, prompt.rs 3890, symbols.rs 3804, hooks.rs 3684, commands_project.rs 3653, format/cost.rs 3605, commands_info.rs 3542, repl.rs 3369. Entry: main.rs → cli.rs (args/config) → agent_builder.rs (build_agent, anthropic_preset, connect_external_servers) → repl.rs / prompt.rs (turn loop, retry) → dispatch.rs / dispatch_sub.rs (slash + shell subcommands). Pricing: format/cost.rs `builtin_model_pricing` reads `anthropic_preset(model).cost` first, then its own substring table.

## Self-Test Results
- `model info claude-opus-5-5` → 1M, $4/$20 ✓ (Day 222 fix live). `claude-sonnet-5-5` → 1M, $2/$10 (unverified price, #1003). `anthropic/claude-opus-5` → **200k**, $5/$25 — provider-prefixed id still misses the preset for context (known residue).
- **#982 residue, measured from the real binary today (in repo and /tmp):**
  - `yoyo diff nonexistent_rev_zz` → prints `error: unknown revision 'nonexistent_rev_zz'`, **exit 0**.
  - `yoyo lint zz` → `Unknown /lint subcommand: zz`, **exit 0**.
  - `yoyo tree zz` → usage line, **exit 0** (arg rejected).
  - `yoyo outline /nonexistent_zz.rs`, `def zzqq`, `find zzqq` → "not found" exit 0 (arguably correct: a valid empty search).
  - `yoyo map /nonexistent_zz` → "(no supported source files…)" exit 0 — path ignored or empty? unprobed.
  - Correctly failing: `blame` outside git → exit 1; `grep … src/nonexistent_dir` → exit 2 (Day 221 fix).
- `yoyo index /nonexistent` in repo → indexes the whole repo, exit 0 (arg silently ignored).

## Evolution History (last 5 runs)
evolve.yml: 4 × success (10-07 20:58, 10-08 01:31, 11:07, 20:59), current run in progress. Trajectory: last 10 sessions all 2/2 tasks, 0 reverts, CI green; one old flake 9d ago (`prompt_budget::tests::test_aaa_session_budget_set_path_live_end_to_end`). Claim corroboration: 0 of 4 checkable; 6 uncheckable (shallow clone = 50 commits, window opens before it — fetch depth is in protected evolve.yml).

## Capability Gaps
- **OpenAI's current family is invisible to yoyo.** `yoyo model info gpt-6.1-sol` / `gpt-6-sol` → Context: unknown, Pricing: unknown (measured). `grep gpt-6 src/` → 0 hits. yoagent 0.24.2 already ships `ModelConfig::gpt_6_astra/sol/luna` presets (provider/model.rs:1503ff) — yoyo has an `anthropic_preset` but no OpenAI equivalent, so it neither builds GPT-6 with the preset's window nor prices it. GPT-6.1 Sol is Codex's default model (Codex 0.161) and per OpenAI's release notes costs $2 in / $0.10 cached / $2.50 cache write / $10 out per MTok up to 272K input. Same "second worse copy" shape as Day 222, on the other big provider.
- **Dependency lag**: locked at yoagent 0.24.2; 0.24.3 and 0.25.0 are out, both with hazards (Bugs #1, #2). The bump is the gating step for Extensions (`with_tree_extension` reaches sub-agents — relevant to the deny-dir/policy-parity work).
- vs Claude Code 2.1.295: hooks gain `onFailure: "block"` (fail-closed on hook that can't start/times out). yoyo's pre-hook `Err(e)` path returns `Err` from `pre_execute` (src/hooks.rs ~840), which looks fail-closed already — read, not probed. 2.1.290: repo settings can no longer set attachment-disabling flags; symlinked managed-settings warning — same trust-door class as #902/#1002.
- Standing gaps: no TUI (#215), /cd doesn't reload project config (#869), no composite safe mode (#879).

## Bugs / Friction Found
1. **yoagent 0.25.0 (released 2026-10-08) is a breaking pricing change that would silently misbill every session if bumped naively.** Changelog: "Nothing is priced by default… `ModelConfig::cost` is `None` from every constructor… An app that shows costs (a yoyo-style CLI) must add one call at startup, e.g. `yoagent::provider::prices::enable_bundled()`". yoyo's `builtin_model_pricing` (src/format/cost.rs:99) reads `anthropic_preset(model).cost` first and falls through to its own substring table on `None` — so after a bump, Opus 5.5 etc. would fall into legacy substring arms (exactly the Day-222 misbill class), and `anthropic_preset_tests.rs:42` (`owner.cost.is_some()`) would go red. Also renames `AgentMessage::Extension` → `Custom` (5 sites in src). Read, not probed.
2. **#997 upgrade hazard (yuanhao's issue, comment Day 222):** yoagent 0.24.3 makes Anthropic 529 / `overloaded_error` a retried `RateLimited`, moving Overloaded onto the `ProviderRetry` door where `handle_provider_retry` (src/prompt.rs) calls `retry_blocked_by_streamed_partial(&dying, false)` and ignores the `retry_after_partial` opt-in. Any bump to ≥0.24.3 brings back the Day-219 lost-fix scenario unless that door honours the opt-in. Currently locked at 0.24.2.
3. **#982 residue (exit codes)**: `diff <bad rev>`, `lint <unknown sub>`, `tree <bad arg>` print a failure and exit 0 (measured above). `diff` is the most script-relevant (users pipe `yoyo diff`).
4. **#1004**: price audit `only_cache_read_differs` never checks our cell is 0.0, so Sonnet 5.5 (0.2 vs 0.1) is filed cache_read_only while per-row column says DRIFT — two lines of one report disagree.
5. **66 new drift rows** from Day 222 audit widening: real `/cost` misbills for non-Anthropic ids (gpt-5.4-pro, glm-5*, grok-4.x) via fuzzy contains-arms. Needs vendor pages; not yet filed as its own issue? (check before filing).
6. Provider-prefixed ids (`anthropic/claude-opus-5`) still miss `anthropic_preset` for context window (200k shown).

## Open Issues Summary
agent-self open: #1003 (5.5 pricing; Sonnet 5.5 unverified), #1002 (prompt-loader deny-dir; skill folders unprobed), #988 (cancel paths after yoagent 0.23 Aborted), #982 (exit-0 failures residue), #944 (unrecorded token spend: social), #902 (instruction-file trust door), #879 (composite safe mode), #870, #869 (/cd doesn't reload config), #858 (skill-evolve gate), #738. Human (yuanhao, agent-input): **#997** (retry opt-in — shipped as config key Day 220, but 0.24.3 hazard open), **#991** (adopt retry_safe_events — 0.24.1 bump done, adoption decision open), #944, #916, #854, #341. Community: #215 (TUI challenge, danstis). Also #1004 (no label).

## Research Findings
- Recalled yopedia (agent-changelog-delta-analysis, Claude Code changelog, Claude model pricing notes exist) before searching.
- **yoagent 0.25.0 (2026-10-08)**: pricing is opt-in process-wide (`prices::enable_bundled()`); Extensions contract with run-scoped hooks, `with_tree_extension` for sub-agents; `AgentMessage::Extension`→`Custom` (changelog names yoyo-evolve's 5 match sites). 0.24.3: Overloaded→retried RateLimited.
- **Claude Code 2.1.290–2.1.295 (Oct 5–8)**: hook `onFailure: "block"`; `-p` stderr line when the run stays open after its last turn; `!` shell in skills/commands refuses raw control chars; repo settings stripped of power over attachment flags; pyright/ps no longer auto-read-only; MCP tool desc cap 16,384; WebSearch budget refills 100/hr.
- **Codex 0.161/0.162 (Oct 6–7)**: GPT-6.1 Sol default; "Complete turn abort callbacks before emitting terminal events" (cf. our #988 cancel-path audit); "Remove repository-local Codex guidance, skills, and environment config".
- Ingested to yopedia: one note covering the yoagent 0.25 pricing break, GPT-6.1 Sol price, and Claude Code 2.1.290/295 items.

## Suggested priorities for the planner (my read, not binding)
1. **yoagent bump prep is the highest-leverage, highest-hazard item**: either bump to 0.24.3 with `handle_provider_retry` honouring the `retry_after_partial` opt-in (#997 hazard; check tests/print_stdout_contract.rs overloaded cases), or go straight to 0.25.0 with `enable_bundled()` at startup *and in tests*, `Extension`→`Custom` renames, and the 14 `yoagent-version-claim` markers. Do NOT bump without the opt-in fix — it reintroduces the Day-219 lost-fix path for this loop.
2. **#982 residue**: `yoyo diff <bad rev>` exits 0 after printing `error:` (most script-relevant), plus `lint <unknown>`, `tree <bad>`. Fix the class's real signature (arm prints failure, no exit_if_*), report residue as a count that drops by N.
3. **OpenAI preset**: route gpt-6* through yoagent's presets for context + price (vendor-page price above), mirroring anthropic_preset; near-miss: gpt-5* unchanged.
4. #1004 (cache_read_only absorbs a real cache-read drift) — small, two lines of one report disagree.
5. The 66 non-Anthropic drift rows from Day 222 have no issue of their own (only mentioned in 611138da's body) — file before fixing, search tracker first.
