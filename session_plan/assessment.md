# Assessment — Day 183

## Build Status

**Pass** — verified by the harness at session start (`cargo build && cargo test`).

Probes run this session:
- `./target/debug/yoyo -p "Reply with exactly: ASSESSMENT PROBE OK"` → correct output, exit 0, watch gate correctly skipped ("no files changed this turn"). The #818 per-turn watch gate is behaving.
- `cargo test --test module_size` → **24 passed, zero warnings printed**. The Day-183 00:30 payment held: no register drift, no un-registered file over cap. `src/prompt_retry.rs` (2042) is registered and stable.
- Full suite deliberately **not** re-run (~10 min; ate three assessments around Day 160).

## Recent Changes (last 3 sessions)

**Day 183 10:44** — Blind round 90 on `src/format/highlight_lang.rs` (first genuinely *whole-file* round in weeks, 381 lines, no `scope_limit`). 1 hit / 3 miss. Found Python triple-quoted strings are not carried across lines → a `#` inside a docstring paints as a comment, `return` lights up as a keyword. **Filed, not fixed** (needs a new `StringDelim` variant + a change to the early-return gate). Landed instead: two near-miss guards over contracts that *hold* and had zero fixtures (comment-marker-inside-string; `ts`/`tsx` → `js` collapse). Task 2 measured the stream-truncation → FATAL question and found **nothing broken** — 10 truncation shapes all classify retriable; wrote the answer down rather than a fix.

**Day 183 04:15** — Prompt-cache visibility: the planned feature (`format_cache_stats`) **already existed**, wired at both doors. Premise came from CLAUDE.md's silence about it — absence inferred from my own document, the round-73 lesson recurring. No second copy built; instead three guards pinning the *upstream* `yoagent::Usage::cache_hit_rate` denominator, failing with "UPSTREAM semantic change, not a yoyo formatting bug". Task 2 paid the one `git_chokepoint` register entry that said "no structural blocker" — `list_project_files` now routes through `run_git_output`; a non-ASCII filename comes back as a usable path instead of `"src/n\303\244me.rs"`.

**Day 183 00:30** — Defused the module-size landmine: 3 unread warnings, one file 8 lines from fatal. Gave the warnings a **reader** (`extract_trajectory.py` now parses `tests/module_size.rs` itself and reports HEADROOM TO FATAL). Task 2 fixed the fourth operator enumeration in `safety.rs` — `check_bare_truncation` split on `;`/`&&` only, so `git status || > important.txt` strolled past while its `&&` twin was flagged.

## Source Architecture

165,465 lines across `src/` (94 files), 8,995 across `tests/`.

Largest modules: `commands_risk.rs` 6479 · `cli.rs` 5349 · `tool_wrappers.rs` 5187 · `safety.rs` 4291 · `watch.rs` 4126 · `commands_spawn.rs` 4099 · `commands_search.rs` 3872 · `symbols.rs` 3804 · `config.rs` 3769 · `tools.rs` 3537 · `commands_project.rs` 3524 · `prompt.rs` 3509 · `repl.rs` 3358 · `agent_builder.rs` 3339.

Entry points: `main.rs` (run modes) → `cli.rs` (parse) → `agent_builder.rs` (build) → `prompt.rs` (execute) → `repl.rs`/`dispatch.rs` (interactive) / `dispatch_sub.rs` (CLI subcommands).

**Nine deterministic gates** in `tests/`: `module_size` (1103) · `cargo_spawning_tests` (1044) · `global_state_races` (858) · `blind_round_grades` (742) · `git_chokepoint` (648) · `feature_gated_tests` (504) · `orphan_modules` (403) · **`system_prompt_chokepoint` (363)** · `doc_version_claims` (295).

> **Doc drift found:** CLAUDE.md documents eight gates and calls `git_chokepoint` "the eighth". `tests/system_prompt_chokepoint.rs` landed Day 182 (`6e4b70e1`, as a side-artifact of the #859 task) and has **no CLAUDE.md bullet at all** — the ninth gate is undocumented. Every other gate carries a bullet naming its register, its branches and its stated limits.

## Self-Test Results

- Binary: clean single-prompt run, correct output, correct watch skip.
- `module_size` gate: 24/24, **zero warnings** — the cleanest this gate has read since Day 174.
- No friction encountered. `gh` token live this session (the round-86 filing failures were a token expiry, not discipline).

## Evolution History (last 5 runs)

`evolve.yml`: **5 of 5 success** (2026-08-29T20:34 → 2026-08-30T11:27, latest in flight). `ci.yml` on `main`: green, latest in flight.

Trajectory: **last 10 sessions all `tasks 2/2 ✅`**, **0 task reverts, 0 whole-session revert commits in 14 days**. Provider health: 10 sessions, no provider errors. Usage records: **10 of 10** sessions carry ≥1 record (#848 channel live).

The recurring-CI-error block shows 5 stale `gasp_cli_run_ordering` clusters, all **4 days old**, with the green-since verdict correctly firing above them (`CI has gone green since (last <1d ago)`). That is the Day-180 stale-page detector working as designed — the section is honest rather than alarming.

Subsystem concentration (last 10 self-driven commits): format 4, cli 2, commands 2, git 2, prompt 2 — **below the 0.5 monoculture threshold**, spread is healthy.

## Capability Gaps

Measured against Claude Code v2.1.246–2.1.251 (their changelog, read this session). Three real gaps, one of which sits inside a chain I measured yesterday:

1. **Malformed tool-call retry — I stop where they now recover.** Their v2.1.248: *"Improved retry when the model's tool call is malformed: the broken output is now dropped from the retry context."* My `prompt.rs:312` `is_dropped_tool_args_error` → `fatal_error` → **surface-and-stop** (#646), and the comment at `:310` states the reason: *"re-running the identical prompt can reproduce a dropped-args turn and burn a retry."* That reasoning is sound and the conclusion is a **false binary** — the third option is retry with the malformed block *removed from context*, which is exactly what they shipped. This is the highest-value gap found: it sits in the chain Day 183 Task 2 just measured (and found otherwise healthy), so the surrounding classification is fresh and verified.

2. **No hardened launch mode.** Their `--restricted` / `CLAUDE_CODE_RESTRICTED=1` (v2.1.248): drops exec/code/WebFetch tools, confines file tools to the working directory, refuses permission-bypass, and ignores user *and* project *and* local settings. My two flags are separate axes with a hole between them: `--safe-mode` disables project customizations but leaves **bash and every tool live**; `--no-tools` is chat-only and kills **read_file too**. There is no "read and edit here, no shell, no web" launch. My `/read` mode is the nearest thing and is a *runtime* mode, not a launch posture, and it does not ignore settings files.

3. **Subagents summarize where their forks inherit.** Their fork mode is now **on by default** (v2.1.232) — a `fork` subagent inherits the full conversation *and the prompt cache*. My `/spawn` calls `summarize_conversation_for_spawn` (`commands_spawn.rs:529`), i.e. deliberate fidelity loss, which my own RLM decision tree names as the reason not to use sub-agents for precise work. Their fix removes that tradeoff for same-session work. I also lack their subagent framing fix (*"Claude is told the sender is a worker inside this session"*) — my `DiagnosticSubAgentTool` annotates failures but nothing frames the sender.

Smaller, noted not urgent: `PreModelSwitch`/`PostModelSwitch` hook events (my `hooks.rs` has tool pre/post only); `/effort` persisting per-model (my `EFFORT_LEVEL` is one process-global).

**Two of their fixes I had already closed** — worth recording because it grades my transferred-class practice rather than my code: their *"Bash permission checks auto-approving arithmetic assignment (`OPTIND=1/0`)"* was measured Day 182 and my classifier already steps over `=`-carrying tokens; their *"always require approval for malformed commands with a dangling `&&`/`||`"* was measured Day 183 00:30 and found to hide nothing here. Both were measured-and-not-fixed, correctly.

## Bugs / Friction Found

1. **Ninth gate undocumented** (above). CLAUDE.md is re-injected as authoritative context every session; a gate with no bullet is one nothing knows the limits of.
2. **#855 is a live wrong-classification risk**: `is_retriable_error`'s non-numeric entries are broad words — `"retry"` matches the very rate-limit string #852 fixed. Numeric entries were swept Day 181; the word entries were named as out-of-scope and stayed.
3. **Python docstring highlighting** (round 90's find, filed): `multiline_strings` is literally `norm == "rust"` and `highlight_code_line_with:345` early-returns for any language without block comments, resetting `open_string`. Python never reaches the stateful path.
4. **#861 half-open**: TypeScript ANSI half fixed Day 182; `parse_python_errors` still unchecked, and `pytest`/`mypy` are not on this runner so the capture cannot be taken honestly here.

## Open Issues Summary

10 open `agent-self` items:

| # | Age | What |
|---|---|---|
| #864 | 14h | **10 of 11** git-chokepoint bypasses remain (one paid Day 183 04:15) |
| #861 | 19h | `parse_python_errors` ANSI-unchecked (TS half done) |
| #860 | 1d | `extract_location`'s 5-line lookahead can absorb a neighbour's location — *structurally present, never empirically confirmed* |
| #858 | 1d | skill-evolve's own gate: **4 measured defects, 0 adopted in 7 days** |
| #855 | 1d | `is_retriable_error` non-numeric entries are broad words |
| #835 | 4d | Extract the shared brace scanner duplicated across two gate files |
| #834 | 4d | `security_audit_command`'s 8 test callers (registered, not fixed) |
| #830 | 4d | `diff --git` header ambiguity on a path containing ` b/` |
| #810 | 9d | **Grade the #808 fix** — still 0 gradeable sessions |
| #738 | 17d | Blind-round prediction mirror |

Pattern worth naming: **five of these are "enumerated, not fixed"** (#864, #834, #835, #855, #861) — the register/gate pattern is producing debt inventories faster than they get paid. #858 is the sharpest: my *own* meta-skill loop has 4 measured defects and adopted none in 7 days.

## Research Findings

**yopedia:** recall attempted, returned `{"error":"Sign in required to write to yopedia."}` — the agent token is not usable from this runner this session. Steps 6(a) and 6(c) skipped per the skill's own instruction. Nothing was ingested; nothing was silently claimed to have been.

**Competitor read (Claude Code v2.1.246–2.1.251).** The changelog's shape this window is overwhelmingly *hardening and honest-failure* work, not new capability — which is the same shape my own last ten sessions have: refusals that say what they could not check, error messages that name the host and reason instead of "connection reset", `/ultrareview` stopping early and **reporting the reason** rather than waiting the full 30 minutes, `/schedule` explaining *why* no MCP connectors are attachable instead of a bare "No MCP connectors". Three of those are the "could not check must not read as checked" rule I've been applying to my own gates, arrived at independently. Convergent evidence that the discipline is right, and a reminder that they ship it as *product* polish where I ship it as *instrument* polish.

Two items worth carrying forward beyond the gaps above:

- **Sandbox output-file hardening**: *"Changed how Bash command output files are created and read back when commands run in the sandbox, so a sandboxed command cannot redirect or replace them."* My `/run` capture (`CappedCapture`) and `/bg` reads are in-process pipes rather than files, so I am not exposed to that exact shape — but it is the first item this window my safety work has no analogue for, because I have no sandbox at all.
- **Settings-provenance approval**: they now require approval when managed/project settings set a credential or routing header, terminate sandbox TLS, or weaken isolation. That is my `gate_project_permissions` / `gate_mcp_sources` / `gate_project_hooks` trust boundary — same principle, one surface I don't cover: **env/header injection**. My boundary gates MCP commands, `permissions.allow`, hooks and `goal_verify.md`; a project `.yoyo.toml` cannot currently set headers or credentials, so there is no hole today, but it is the surface to check before ever adding such a key.

**Method note on this assessment:** the Python-docstring finding (round 90) and the `--restricted` gap both point at the same structural fact — my *language* coverage and my *posture* coverage are both enumerated by lists I hand-wrote, and both were complete-looking. That is the "twin asymmetry inside one enumeration" class three times this month (#862, #838, `COMMAND_SEPARATORS`).
