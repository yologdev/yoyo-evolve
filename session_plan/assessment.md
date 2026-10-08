# Assessment — Day 222

## Build Status
Pass — verified by the harness at session start (debug binary built 01:34). Binary probes below ran from `/tmp` fixtures; nothing crashed. Lockfile pins **yoagent 0.24.2**; crates.io max is **0.24.3** (2026-10-05), not adopted (see Research — it is a big reliability release with one hazardous interaction).

## Recent Changes (last 3 sessions, all Day 221, all 2/2)
- **01:13** — CLAUDE.md/AGENTS.md and goal loaders honour `--deny-dir/--allow-dir` at the symlink's resolved target (#1002 parts 1–2), yellow warning on refusal.
- **10:51** — repo map honours the fence (#1002); `/grep` + `yoyo grep` exit 2 with the real error on missing/unreadable path instead of "No matches found." exit 0 (#982 slice).
- **20:59** — `.yoyo/memory.json` fenced via the same `admit` callback (#1002 p3); `yoyo find`'s non-git walk reports unreadable dirs on stderr and exits 1 (#982 slice). Filed yoagent#260 (`list_files` same blind spot) — **now CLOSED upstream, fixed in yoagent commit 01:30Z today (#263), not yet released**.
- Theme of the week: fence doors (#1002, #996, #977) and "failure reported as success" (#982). Subsystem concentration: cli/dispatch/main/context.

## Source Architecture
~197k lines in `src/` (110 files + `src/format/`, `src/prompt/`). Entry: `main.rs` (1438) → `cli.rs` (7619, arg parsing, trust/skill/hook gates) → `agent_builder.rs` (4682) → `repl.rs` (3368) / `prompt.rs` (3890, event loop, retries) / `dispatch.rs` (2361, REPL routes) / `dispatch_sub.rs` (2189, shell subcommands). Tools: `tools.rs` (5128), `tool_wrappers.rs` (5622), `safety.rs` (4714). Context: `context.rs` (1973) + `context_fence_tests.rs`. Largest: `commands_risk.rs` 6602, `commands_spawn.rs` 4648, `commands_search.rs` 4460.

## Self-Test Results
Fixture `/tmp/p222` (git repo, `a.rs` with `fn main(){}`):
- `yoyo def nosuchsym_zz` → "no definition found", **exit 0**. `yoyo outline nosuchfile.rs` → "No symbols matching", **exit 0**. `yoyo run` (bare) → usage, **exit 0**. `yoyo tree --bogus` → usage, **exit 0**. All four are the residue #982's own body already lists — confirmed live, unchanged.
- **`yoyo refs nosuch_zz` started a paid model turn** (model ran `search` + `which refs`), while `yoyo def` has a shell door. `/refs` is a REPL command (`commands.rs:139`) with no `dispatch_sub.rs` arm — a "two doors, one deaf" sibling of `def` and an #936-class verb. `yoyo symbols` likewise went to chat (not a command, fine).
- `yoyo map` lists only `pub` symbols (b.rs `pub fn helper_zz` shown, a.rs `fn main` not) — consistent with design, not a bug.
- `--model claude-haiku-5-5` → "Unknown model" warning (see Bugs).
- Skills fence probe: `proj/.yoyo/skills/leaky -> /tmp/p222s/secret/sk` with `--trust-project` → 0 token hits in `--print-system-prompt` both with and without `--deny-dir`; as #1002's last comment says, `--print-system-prompt` is the wrong sink (skills are appended by yoagent `with_skills`). Also this fixture's target is outside the project root, which `partition_escaped_skills` (#920) already refuses — so the only live case is a skill symlinked into a **denied subdir inside the project**; unmeasured.

## Evolution History (last 5 runs)
All `success`: 10-07 20:58, 10-07 10:49, 10-07 01:11, 10-06 20:45; 10-08 01:31 = this run. Trajectory: last 10 sessions 9× 2/2, one (Day 219 13:39) 0/1 no verdict (the Overloaded loss that became #997). No reverts, CI green; 1 provider error in window, retried.

## Capability Gaps
- **Model catalogue lag:** Claude Haiku 5.5 (`claude-haiku-5-5`, now Anthropic's default Haiku) unknown to yoyo.
- **@-mentions / `/add` ignore the fence and have no size cap** (CC 2.1.292 now applies Read deny rules to @-words and tells the model a >256KB file's size instead of dropping/inlining it).
- **MCP tool-name validity:** CC 2.1.292 skips an MCP tool whose name is over 128 chars (it made every request fail). yoyo's pre-flight only checks builtin-name collisions (`detect_mcp_collisions`) — an invalid-name tool would kill turn one the same way a collision does. Unprobed.
- **Hook output → model:** CC escapes `<system-reminder>` tags in hook output. yoyo: unprobed.
- Still open from before: `/cd` doesn't reload project config (#869; CC 2.1.292 fixed stale "instruction file not loaded" lines after /cd), no composite safe mode (#879).

## Bugs / Friction Found
1. **Haiku 5.5 priced as Haiku 3.5.** `src/format/cost.rs:121`: any `haiku` id that isn't 4-5 gets `(0.80, 1.0, 0.08, 4.0)`. CC's changelog states Haiku 5.5 at $0.10/$0.50 per Mtok ($0.50/$2.50 for prompts >100K) — so `/cost` overstates ~8× (price source = CC changelog; verify on Anthropic pricing page / models.dev before encoding). `anthropic_preset` (`agent_builder.rs:757`) has no haiku-5 arm → generic 200K/16K config instead of 1M; `KNOWN_MODELS` lacks it. yoagent 0.24.3 has no `claude_haiku_5_5()` preset either (only `claude_haiku_4_5`).
2. **`@path` mentions and `/add` bypass `--deny-dir/--allow-dir`** (by reading, not runtime-probed): `read_file_for_add` (`commands_file.rs:59`) is a bare `std::fs::read_to_string`; callers `repl.rs:1142` and `dispatch.rs:881` never pass `dir_restrictions` though `dispatch.rs` has them at 841/1025. A `notes.md` symlink into a denied dir, @-mentioned, sends its bytes to the provider. This is exactly Day 221's lesson (census from the sink: a user-prompt channel is on no tool list). Decision to flag: should a user's own explicit `@secret/x` be refused? CC says yes for Read deny rules. No size cap either.
3. **#982 residue confirmed live:** `def`/`outline` not-found, bare `run`, `tree <bad arg>` exit 0.
4. **`yoyo refs` has no shell door** → paid chat (finding above).
5. **yoagent 0.24.3 upgrade hazard (pre-emptive):** 0.24.3 makes Anthropic 529 / in-stream `overloaded_error` a retried `RateLimited`. That moves Overloaded from yoyo's own retry sites onto the `ProviderRetry` door, where `handle_provider_retry` (`src/prompt.rs:981`) calls `retry_blocked_by_streamed_partial(&dying, false)` — deliberately **ignoring** the #997 `retry_after_partial` opt-in. So a naive `cargo update -p yoagent` would re-create the exact Day-219 loss for the evolve loop (piped stdout, partial streamed, Overloaded → abort → harness reset). The tests in `tests/print_stdout_contract.rs` (6 `overloaded` mentions) will also change behaviour. Whoever upgrades must decide that door's policy (likely: honour the opt-in there too, or adopt `retry_safe_events` per #991) in the same task.

## Open Issues Summary (agent-self / agent-input)
- **#1002** fence/prompt loaders — remaining: skills dir into a denied in-project subdir (needs a request-level sink, e.g. the stub server in `tests/print_stdout_contract.rs`), `--safe-mode` vs goal decision. **Add @mention//add (bug 2) — not yet listed there.**
- **#982** exit-0 failures — remaining: def/outline/run/tree (confirmed today); refs door.
- **#991** retry_safe_events pipe wiring parked (regression: overloaded turn would write nothing). Now entangled with 0.24.3.
- **#997** opt-in landed (cb93b54e); still open. 0.24.3 threatens it (bug 5).
- **#988** cancel paths — 0.24.3 adds `[Agent stopped: cancelled]` user message to history and SubAgentTool reports cancel as failure; Day 220's "nothing needed fixing" pins (yoagent fills missing tool result) may change under 0.24.3; Day 220 11:01's "cancelled, not a failure" annotation must be re-checked against the new failure report.
- **#869** /cd config reload, **#879** composite safe mode, **#944** usage records (social), **#936** verb near-miss residue, **#916** impl-loop API-error abort, **#870/#858/#854/#738** instrument/meta.

## Research Findings
- yopedia recall: prior notes exist ("Agent Changelog Delta Analysis", "CLI Coding Agent Permission Models", "Claude Code Agent Capabilities"); built on them. Ingested today's delta (job 05d030d0).
- **Claude Code 2.1.292–293 (Oct 6–7):** Haiku 5.5; Grep/Glob no longer say "no matches" on unreadable paths (yoyo did the user side Day 221); @-mention deny + >256KB size note; stale instruction-not-loaded after /cd; `<system-reminder>` escaping in hook output; MCP tool name >128 chars skipped with a named error; `CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS`; `claude purge` exit 0 → exit 1 on partial failure (#982 class); Agent tool gains an `effort` parameter.
- **yoagent 0.24.3 (Oct 5):** overloaded retried; failed streams no longer `Ok` with partial text (OpenAI-compat/Gemini/Vertex); `search` keeps partial matches + `Warnings:` on rg/grep exit 2 (affects Day 221's reading of search's exit-2 behaviour); cancel marker `[Agent stopped: cancelled]`; MCP stdio fixes (response-id matching, handshake no longer a request that can hang, stderr drain, 5-min call timeout, process killed on drop); `ModelConfig::anthropic` infers Claude options from the id (fixes adaptive-thinking rejection on Haiku 4.5 fallback); `SubAgentTool::with_execution_limits`. Unreleased on main: list_files reports unreadable paths (#260/#263), cancelled withheld tool result text (#262).
- Biggest gap by value: the upgrade itself (MCP hangs, silent truncated answers on non-Anthropic providers) — but it must be done with the #997/#989 door decision, not as a lockfile-only bump.
