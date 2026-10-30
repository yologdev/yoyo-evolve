# Assessment — Day 244 (20:27)

## Build Status
Pass, as verified by the harness at session start (HEAD 2f5f4abc, a synthesize commit on top of the 10:44 session). The binary is `yoyo v0.3.0 (2f5f4abc 2026-10-30)`. `yoyo -p "Reply with exactly: PONG"` returned PONG with exit 0 on claude-opus-5-5. I did not re-run the full suite.

**Size-gate hazards the planner must price before touching these files** (from the trajectory, cross-checked with `wc -l`):
- `src/config.rs` is 4851 lines against a recorded 4762. **11 more lines makes `cargo test` FATAL**, which reverts the whole task. Any task that edits config.rs must either paste `("src/config.rs", 4851)` over its entry in `tests/module_size.rs` or put new code and tests in a sibling file.
- `src/commands_risk_unhittable.rs` is 2036 lines and **UNLISTED** (cap 2000). 14 more lines is FATAL. It needs a grandfather entry or a split before anyone edits it.

## Recent Changes (last 3 sessions, all 2/2 green)
- **10:44**: Numeric flags (`--max-tokens abc`, `--temperature hot`, `--max-turns x`, `--context-window big`) now refuse with exit 2 and "Nothing was sent". In `.yoyo.toml` they still only warn, and the reason is recorded (repo-authored file). `list_files`' "Warnings from find" lines that name a denied path are withheld (Fixes #1023). The residue, a warning cut inside a denied root's name, was filed as #1105.
- **09:15**: A global flag before a subcommand (`yoyo --yes doctor`) refuses with exit 2 instead of becoming a paid prompt. Seven subcommands missing from `KNOWN_SUBCOMMANDS` were added, with a guard test. The trajectory cap now fits the whole block, and a cut names the dropped sections (Fixes #1104).
- **07:12**: `yoyo ast` exits 0/1/2 like `grep`. A bare `yoyo goal set` exits 2. Synthesize spend is counted in the trajectory (Fixes #1071).
- **01:34**: An unknown `--provider` with no base URL refuses with "did you mean" (Fixes #1103). Well-known `eval "$(ssh-agent)"`-style setup tools are allowed past the deny-list eval guard (Fixes #1066).
- Theme across Day 243–244: "a shape only a mistake produces should refuse, not run a paid prompt". The exit-code and argument hygiene class is close to drained. The trajectory shows subsystem concentration in cli/dispatch (6 of 8).

## Source Architecture
There are 176 `.rs` files under `src/`, about 222k lines including inline tests. The largest are cli.rs 7873, commands_risk.rs 6602, tool_wrappers.rs 5736, tools.rs 5268, agent_builder.rs 4955, config.rs 4851, safety.rs 4720, commands_search.rs 4593, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3924. Entry points:
- `main.rs` → `cli::parse_args` → `dispatch_sub::try_dispatch_subcommand` (shell verbs) or the REPL/prompt.
- `agent_builder::build_agent` + `connect_external_servers` (MCP/OpenAPI).
- Tool wrapper stack: `tool_wrappers.rs`, plus `fence_result_filter.rs` and `partial_read_guard.rs`.

## Self-Test Results
- Ran from an empty `/tmp` dir and from the repo. Refusals verified at HEAD: `--max-tokens abc` exits 2 with a clear message, `--provider anthropc` exits 2 with "did you mean 'anthropic'", `init zzz` and `help zzz` exit 2, `yoyo web` exits 2 and names the REPL form, `ast zz1` with sg absent exits 2, bare `goal set` exits 2.
- **#1086 residue, re-probed at HEAD 2f5f4abc:** these shell verbs still print REPL slash syntax in their usage or errors: `grep` (`usage: /grep ...`), `docs` (`usage: /docs ...`, `Examples: /docs serde`), `ast` (`Usage: /ast ...`), `def` (`usage: /def`), `outline` (`Usage: /outline`), `blame` (`✗ Usage: /blame`), `lint zzz` (`Unknown /lint subcommand`), `skill zzz` (`try: /skill list, ...`). That is 8 verbs I probed. `find` and `run` are already fixed (`usage: yoyo find`, `usage: yoyo run`). The exit codes are all correct (2); only the wording is wrong. This is a mechanical, product-facing slice that is cheap to verify by probe.
- **Auto-memory is frozen full (measured, no ticket found).** `.yoyo/memory.json` holds exactly 20 entries. 18 are `[bug] Test expectation in ...: thread 'X' panicked at file:line:` (a truncated panic header with no content), and 2 are `Watch fix: ...`. The newest is from 2026-08-12. `src/memory.rs:196` `MAX_AUTO_MEMORIES = 20`, and `auto_remember_to` **refuses new entries when full** (it keeps the oldest and drops the newest), with only an eprintln `⚠ memory full (20) — use /forget`. So for 2.5 months: (a) auto-watch learned nothing new in this repo, and (b) `format_memories_for_prompt` (`src/memory.rs:441`) injects these 20 low-information lines into **every** system prompt as "## Project Memories". A product user on auto-watch hits the same thing: the first 20 fixes are kept forever, and the stderr warning scrolls past. Design choices to price: evict the oldest auto entry (keep the newest 20), stop recording the content-free `test_assertion` notes, or cap what reaches the prompt. A `gh issue list --state all` search found only #672 (the dedup prefix, closed), so this needs a ticket or a task.
- Auto-watch runs on a `-p` prompt in this repo. That is expected, since `.yoyo.toml` opts in.

## Evolution History (last 5 runs)
The last 4 completed evolve runs (01:31, 07:09, 09:13, 10:41) all succeeded. The current run (20:21) is in progress. The trajectory shows 10 of 10 sessions at 2/2 tasks, 0 reverts, and CI green since a flake 2 days ago (`post_hook_feedback_reaches_the_model...`, the SIGPIPE flake that was fixed Day 242). No provider errors. Phase spend over 7 days: social $34.61 across 27 runs, which is the largest line, dream $2.27, synthesize $7.95. These are floors, because sub-agent spend is excluded (#1073).

## Capability Gaps
- Sub-agent spend is still missing from `--output-format json`, stream-json, and the trajectory's phase spend (#1073). `/cost` is done.
- MCP: there is no reconnection of survivors, and a server orphaned by a panic is not cleaned up (#1100). Mid-session `/cd` does not load the project's hooks, MCP or skills (#1040).
- `write_file` still overwrites a never-read existing file (#1094 item 1; Claude Code requires a read first). This is a product decision with options on the ticket.
- No TUI (#215) and no benchmark submission (#156). Both are long-standing.

## Bugs / Friction Found
1. #1086's wording residue (8 verbs, listed above). Measured at HEAD.
2. The size-gate cliffs on config.rs and commands_risk_unhittable.rs (above). They are not bugs, but they are the most likely way tonight's task gets reverted.
3. Auto-memory frozen at its 20-entry cap with content-free notes, keep-oldest/refuse-newest, injected into every prompt (details in Self-Test). Measured at HEAD.

## Open Issues Summary (agent-self)
#1105 (find warning cut leak), #1102 (heredoc mention refused by hard_deny), #1100 (MCP survives panic), #1094 (write_file partial-read residue: never-read overwrite), #1086 (shell verbs print slash syntax, 8 verbs left), #1084 (cost rows reading low: tiered session line, DeepSeek off-peak), #1073 (sub-agent spend in json/stream-json/trajectory), #1070 (multi-MB CLAUDE.md goes in silently; partly done Day 238), #1064 (credential scrub; the hand-back from @yuanhao on Day 236 means the split decision is mine), #1060 (openai wire residue), #1041 (MCP prefix: done Day 231/240; check whether residue remains), #1040 (/cd trust load), #1037/#1021 (stale defaults: anthropic and google done), #1026 (output-limit cut note in json), #1018 (durable todos), #1014 (/spawn tap), #1012 (price drift).

**Owed decisions held on an owner:**
- **#991 item 3**: I posted an A/B to @yuanhao (A = drop the unread stream-json collected text, as he originally said). His last word predates that ask, so the hold is real: do not touch it until he answers. Items 4a and 4b on #991 are unaffected and open.
- **#1096** (agent-input, @JC351, X outreach challenge): replied, leaving it open for @yuanhao's ownership call. No action for the loop.

## Research Findings
- **Recall first:** yopedia already holds deltas through Claude Code **2.1.314** (2026-10-30, the latest on code.claude.com/docs/en/changelog as of this run) and Codex CLI through **0.166** (GPT-6.1 Sol default, Terra in the catalogs, `/presets`, mid-turn steering, built-in Vertex provider, managed worktrees). Re-scanning found nothing new, so **nothing was ingested this session** because nothing cleared the bar.
- **Re-checked against HEAD:**
  - Claude Code 2.1.311 escapes DEL, C1 and format characters in json/stream-json. yoyo already does this (`stream_session_restored.rs` tests, #1092), so it is not a gap.
  - Claude Code 2.1.312 refuses to overwrite a large CLAUDE.md after seeing only its start. yoyo's `partial_read_guard.rs` (Day 241–242) covers the partial-read case. The remaining delta is #1094 item 1, a never-read overwrite, which Claude Code also refuses.
  - Codex's default is `gpt-6.1-sol`. yoyo has its preset (`agent_builder.rs:823`), but `default_model_for_provider` still returns `gpt-6-sol` for openai (`providers.rs:247`). The #1037 openai row is worth re-checking with the same three checks (preset, priced, max_tokens ≤ output ceiling). Other defaults that look stale and are unverified: xai `grok-4` (yoyo has priced grok-4.6, Day 241) and openrouter `anthropic/claude-sonnet-4-6`.
  - `gpt-6.1-terra`: yoagent 0.25.16 has no preset (I grepped the registry source), so it is upstream-blocked.
- **Biggest real gaps vs. rivals (unchanged):** mid-turn steering (Codex 0.163, Claude Code), managed worktrees per lane (Codex 0.162; yoyo's `/spawn` has worktrees but no write tap, #1014), a TUI, and MCP lifecycle (`/mcp login`, reconnect, #1040/#1100). These are all large. The cheap, product-facing work available tonight is #1086 (8 verbs of wording), the auto-memory freeze, the openai/xai default rows of #1037, and #1084's two low-reading cost rows.
