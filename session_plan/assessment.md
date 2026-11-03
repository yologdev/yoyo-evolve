# Assessment — Day 248

## Build Status
pass — verified by the harness at session start (HEAD a2398582). Binary probes: `yoyo --version` → `v0.3.0 (a2398582 2026-11-03)`; `yoyo -p "Reply with exactly: pong"` → `pong`, exit 0; `yoyo config get model` → `claude-opus-5-5 (.yoyo.toml)`. CI (ci.yml) green on the last 5 runs.

**Size-gate hazards (from trajectory, any task touching these must budget for it):**
- `src/commands_risk_unhittable.rs` 2036 lines, UNLISTED, 14 lines from FATAL.
- `src/config.rs` 4851 vs recorded 4762 — 11 lines from FATAL. Any new `.yoyo.toml` key (e.g. #1012's live-price opt-in) lands here: paste the new count into `tests/module_size.rs` in the same task.

## Recent Changes (last 3 sessions)
- **Day 247 23:22** — Fixes #1115: `-p` network errors print one-line summary (`network_error_summary.rs`), `/retry` advice REPL-only, full error under `--verbose`. Fixes #1116: bedrock default `anthropic.claude-sonnet-4-6` gets Anthropic's published limits via a hand-written sonnet-4-6 row in `anthropic_preset` + `providers::bedrock_anthropic_preset_id`; `/cost` bills `global.` vs regional prefix (`format/cost/bedrock_region.rs`). Residue → #1121 (inference-profile ids), #1120 (DNS retries).
- **Day 247 12:01** — loopback "connection refused" fails fast (73s → ~9s) with "is the server running?" (`prompt/loopback_refused.rs`). Fixes #1108: `/cost` attributes sub-agent spend to the turn that spawned it (`prompt/sub_agent_spend.rs`); REPL wiring compiler-checked only.
- **Day 247 10:50** — OpenRouter Claude ids (dotted) borrow Anthropic presets; default moved to a dotted 5.5 id (reflection: chosen to quiet my own warning). Part of #1114: 34 boolean flag scans skip value slots.
- Commit 721150bd (creator): backlog now skips `parked` issues and reviews umbrellas instead of slicing them.
- llm-wiki external journal: still paused since 2026-05-04 (storage migration).

## Source Architecture
161 files in `src/`, ~225k lines total (incl. tests). Largest: `cli.rs` 7863, `commands_risk.rs` 6602, `tool_wrappers.rs` 5746, `tools.rs` 5268, `agent_builder.rs` 4976, `config.rs` 4851, `safety.rs` 4720, `commands_search.rs` 4593, `commands_spawn.rs` 4566, `watch.rs` 4444, `prompt.rs` 3936, `commands_git.rs` 3923, `format/cost.rs` 3656. Entry: `main.rs` → `cli::parse_args_with_config` → `dispatch_sub::try_dispatch_subcommand` / REPL; agent built in `agent_builder.rs` (`build_agent`, `create_model_config`, presets, MCP via `mcp_prefix::connect_mcp`); prompt loop in `prompt.rs` + `prompt_retry.rs`. yoagent pinned `"0.25"` (lock 0.25.16).

## Self-Test Results
- `-p` round trip works; auto-watch fires and correctly skips when nothing changed.
- `yoyo status --bogus` → exit 2 "unknown flag … Nothing ran." (fixed; #1111's title still says status exits 0 — title stale, scope comment is current).
- `yoyo changelog --bogus` → prints recent commits, carries on (still #1111 territory).
- `yoyo docs --bogus` → looks `--bogus` up on docs.rs, HTTP 400, exit 1 (#1111 "search operand" bucket).
- `yoyo find --help` → prints the top-level help page, not find's usage (minor; related to #1086/#1111).

## Evolution History (last 5 runs)
- 41612248158 (now, 09:35) in progress.
- 41544294744 (Day 247 23:17), 41440880697 (11:57), 41431339783 (10:45): success, 2/2 tasks each.
- 41371266841 (2026-11-02 01:12): **failure** — `--log-failed` and `--log` both return nothing (logs unavailable); jobs API shows only `evolve failure` with no failed step named. Undiagnosed; the three runs since are green. Trajectory shows 10/10 recent sessions 2/2 ✅, 0 reverts in window, 1 retried provider error. Only CI failure in 14 days is the hook_feedback_request SIGPIPE flake (fixed Day 242, green since).

## Capability Gaps
- **Not building on yoagent** (creator's #1122 diagnosis): the `yoagent` skill is loaded every session and never opened; Day 247 hand-added a sonnet-4-6 preset and a Bedrock id→preset map that belong upstream. yoyo keeps its own price table (`format/cost.rs` + promo/xai_region/bedrock_region) that yoagent's `prices::enable_bundled()` / `enable_live_cached(ModelsDev…)` already provides (#1012, 19 comments of row fixes).
- yoagent `Extension`/`RunHooks` (policies, verifiers, `extension::Budget` dollar budget counting sub-agents) unused; yoyo hand-rolls wrappers (`tool_wrappers.rs` 5746 lines).
- No HTTP MCP (stdio only); no plugin ecosystem (yoagent-rutis: pi / DSH packages) — out of scope for now per #1122.

## Bugs / Friction Found
- size-gate near-FATAL on two files (above).
- #1111 title outdated vs its scope comment (status now refuses).
- `yoyo find --help` shows global help.

## Open Issues Summary
**Creator decisions posted today 08:39–08:41 UTC (all fresh, all actionable — verdicts, not returned questions):**
- **#1122 (agent-input, creator: "Please do the upgrade yourself")** — bump `yoagent = "0.26"`, `cargo update -p yoagent` (0.25.16 → 0.26.6, rand 0.10.0 → 0.10.3). Verified this session: 0.26.6 is on crates.io; `McpError` appears only in a return type in `src/mcp_prefix.rs:38,186` (no match → non_exhaustive change is harmless); exactly **32** `yoagent-version-claim: 0.25` markers exist → `tests/doc_version_claims.rs` fails until each marked paragraph is re-read against 0.26.6 and re-marked. Also two `vendor_preset_tests` fail by design (0.26.4 presets `gpt_6_1_terra`, `qwen_3_8_omni_flash`, `glm_5_3_flashx`: route or register in `PRESETS_NOT_YET_ROUTED`; replace the glm-5.3-flashx near-miss). Behaviour changes with no test: MCP stdio own-process-group kill on launcher exit (0.26.0, relevant to #1100), compaction keeps task prompt at small budgets (0.26.2), Gemini UTF-8 chunk fix (0.26.3), mistral_large_4 repriced, opt-in `TokenAccounting::Growth` (0.26.5). 0.26.6 has **no** sonnet-4-6 preset and no Bedrock inference-profile lookup → file both `--label upstream` (issue's item 3). Item 1 (plan/implement prompts naming the skill path) lives in `scripts/evolve.sh` — **protected**; that half is an ask back to @yuanhao on #1122, not a task. Item 2's "`yoagent:` line per planned task" could live in `skills/evolve/SKILL.md` and/or a `tests/harness_logic.sh` check (both editable).
- **#1120** decision (a): fail fast on RFC 6761 reserved names (`.invalid`, `.test`, `.example`, `.localhost`), keep retries for other DNS failures; reason beside the retry classifier; tests for one reserved name and one ordinary NXDOMAIN. Small, well-specified.
- **#1026** decision: exit code follows who set the limit — user-set `max_tokens` → exit 0 + note; default/preset limit → nonzero; `StopReason::Refusal` → nonzero; add the note to json and stream-json.
- **#1064** decision: scrub MCP stdio servers' env by default (like hooks); bash//run//bg//watch stay opt-in (`YOYO_SUBPROCESS_ENV_SCRUB=1`); `=0` turns default scrubs off, misspelling fails closed. **Blocker measured this session:** yoyo spawns MCP servers via `McpClient::connect_stdio_with(command, args, env, StdioOptions)` (`src/mcp_prefix.rs:187`); in yoagent 0.26.6 `StdioOptions` has only `own_process_group` and the transport only ADDS env (`cmd.env(k, v)`, `src/mcp/transport.rs:323`) — no `env_remove`/`env_clear`. So yoyo cannot unset an inherited key through yoagent. Setting it to `""` is wrong (Claude Code 2.1.317 just fixed exactly that: an empty `AWS_BEARER_TOKEN_BEDROCK` broke MCP servers; now unset). Per #1122 item 3 this is an upstream ask (`StdioOptions::env_remove`), with a yoyo stopgap only if marked with the upstream number (e.g. wrapping the command in `env -u KEY …` on Unix).
- **#1012** decision: delete yoyo's price table, price through yoagent (`enable_bundled` floor; `enable_live_cached` only behind an explicit opt-in, never blocking startup, cached under config dir; show price provenance bundled/live/cached in `/cost`; unpriced stays unpriced). Large, multi-slice; closes #1084 too if policy covers it. Pin bump not required for it.
- **#1102** decision A: keep refusing; keep counting incidences here; revisit at 5. No code.

**Other open agent-self:** #1121 (Bedrock profile ids), #1119/#1118/#1117 (provider default/known-model lists — note #1117/#1121 are now upstream-first per #1122), #1114 (value lookups `flag_value` 54 sites), #1111 (search-operand verbs), #1100 (MCP outlives panicking yoyo — re-check after 0.26 upgrade), #1094, #1086 (REPL syntax on shell, +diagnose_api_error hints), #1041, #991. Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.

## Research Findings
Recall (yopedia): prior Claude Code changelog scans ran through 2.1.314. Scanned 2.1.316–2.1.317 (raw CHANGELOG.md) and saved the delta to yopedia. Transferable:
1. **MCP env: unset, never empty** (2.1.317) — directly shapes #1064 (see blocker above).
2. **403 region/network block misread as sign-in failure** (2.1.317). yoyo's `diagnose_api_error` (`src/prompt_retry.rs:703`) maps every 403/"forbidden" to "your API key doesn't have access to model X" — same misdiagnosis class; a region block or proxy 403 gets key/tier advice. Small product fix candidate.
3. Shell commands left running when the CLI exits by itself (credentials refused etc.) — yoyo analog is #1100 (MCP servers outlive a panic); yoagent 0.26.0's own-process-group kill-on-launcher-exit is relevant there, re-measure after the upgrade.
4. Requests hanging on a cut-off compressed response; fresh connection after a 5xx. Not probed in yoyo.

**Biggest gap, stated plainly:** not features vs Claude Code but *not using what yoagent already ships* — pricing (`prices::enable_bundled`/`enable_live_cached`, #1012), presets (upstream asks instead of hand rows), `Extension`/`extension::Budget` instead of wrappers. The creator has now said this four ways in one morning (#1122, #1012, plus decisions on #1064/#1026/#1120/#1102).

## Suggested priorities for the planner (my read, not a plan)
1. **#1122 yoagent 0.26.6 upgrade** — creator-requested, all claims re-verified above; the real work is re-reading the 32 marked paragraphs and the 3 preset tests. Read the crate's `skills/yoagent/SKILL.md` ("Pick the feature", "What changed" 0.26.x) as step 1 — the issue's whole point. File the two upstream issues (sonnet-4-6 preset; Bedrock inference-profile id lookup) with `--label upstream`; mark the Day-247 stopgaps with those numbers. Reply on #1122 that item 1 (evolve.sh prompt edits) is in a protected file → his half.
2. **#1120 (a)** — RFC 6761 reserved-name fast fail, small and fully specified by the owner.
3. **#1026** — exit code by who set the limit + refusal nonzero + json/stream-json note; owner-specified.
4. #1012 (delete own price table) is the largest and most valuable; first slice could route `/cost` through yoagent's bundled prices for rows yoagent already has, after the upgrade. #1064's MCP default is blocked on an upstream env-remove option — file it.
