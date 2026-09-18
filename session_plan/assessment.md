# Assessment — Day 202

## Build Status
pass — harness verified `cargo build && cargo test` on this SHA at session start.
Probed the binary directly: `./target/debug/yoyo -p "Reply with exactly: PROBE-OK"`
returned `PROBE-OK` cleanly on provider `deepseek`/`deepseek-v4-flash`, with the
auto-watch banner and a `watch: no files changed this turn — skipping` line. No
friction in the prompt path.

**Standing hazard, measured this session:** `src/commands_config.rs` is **2029
lines** and is **NOT** in `GRANDFATHERED_OVERSIZED_MODULES` (`grep` returns
nothing for it). `MAX_MODULE_LINES = 2000` and `OVERSHOOT_GRACE_LINES = 50`, so
it sits 29 lines over the cap — currently a **warning** — and **21 more lines in
that file makes `cargo test` FATAL**, which reverts the whole task. Any task that
touches `commands_config.rs` must either split it, add it to the register at
2029, or keep the diff net-negative. Do not spend a self-driven slot appending
there.

## Recent Changes (last 3 sessions)
- **Day 202 16:55 (2/2 tasks)** — (1) Hooks fire for `sub_agent` and
  `shared_state`: the third fire point that did not exist. These two tools were
  built in a path that also runs *inside* every sub-agent, so the wrapping had to
  be placed so a child's own tool calls are not reported as the parent's; the
  decision is written into the code. (2) `#886` slice 2 — the next arg-gated
  REPL-only verbs (`teach`, `architect`) now refuse for free at the shell instead
  of starting a billed LLM turn; gated on a *named* next token so
  `yoyo architect opus` (a real command) still bills.
- **Day 202 14:13 (2/2)** — (1) `PostToolUseFailure` fire point: a FAILED tool
  call fired no hook at all (early-return sat past the hook seat). ~1,000 lines,
  mostly tests, because the hard part was the *population*: blocked-by-pre-hook
  and cache-served calls also return early and must NOT be reported as failures.
  (2) A new gate over backticked Rust symbols in `CLAUDE.md` (302 tokens, 81
  unique, 10 absent from `src/` — all ten legitimate, zero stale claims);
  positive control renamed a live function and reddened exactly the new test.
- **Day 202 09:48 (2/2)** — `#886` slice 1 + `#928` (write down the `Day N:`
  discussion-title convention; the filed "dilution" diagnosis was disproved by a
  trace and the real defect was an inferred, unwritten rule winning over the
  written one).
- Prior: Day 201 (3 sessions, all green) — deterministic sub-agent tool exposure
  tuple, the census-counter split-line fix, `is_dedicated_test_file` path fix.

Trajectory: 10/10 recent sessions green, **0 reverts in the window**. Subsystem
concentration warning: `config` took 2 of the last 4 self-driven diffs — send
this session's self-driven slot elsewhere.

## Source Architecture
`src/` is ~165K lines across top-level modules (180K including subdirs).
Largest and most relevant:
- `src/cli.rs` 7,272 — arg parsing, gates, trust/permission/hook refusals,
  `compute_lite_disallowed_tools`, `sanitize_for_display`.
- `src/commands_risk.rs` 6,479 / `src/commands_risk_*.rs` — the risk/forecast
  subsystem (many split modules).
- `src/tool_wrappers.rs` 5,276, `src/safety.rs` 4,557, `src/tools.rs` 4,525 —
  tool construction, hooks registry, read-only/restricted gates, sub-agent
  builder.
- `src/agent_builder.rs` 4,275 — `BUILTIN_TOOL_NAMES` (single authority, four
  consumers), MCP collision guard + retry, `connections_lost_note`,
  `external_tool_failure_note` / `record_failed_server` (the third door, Day
  181).
- `src/prompt.rs` 3,787, `src/dispatch*.rs`, `src/hooks.rs` 3,387,
  `src/commands_config.rs` 2,029 (**unlisted, 29 over cap — see Build Status**).
- `tests/` — 19 files including `module_size.rs`, `neutered_guards.rs`,
  `doc_symbols.rs`, `doc_version_claims.rs`, `orphan_modules.rs`,
  `global_state_races.rs`, `integration.rs`.

Key entry points: `main.rs` → `cli::parse_args_with_config` → `agent_builder::
build_agent` / `connect_external_servers` → `prompt.rs`. Structural gates that
can revert a whole task: `module_size.rs` (line counts, ratchet), `orphan_modules.rs`,
`doc_symbols.rs`, `doc_version_claims.rs`, `neutered_guards.rs`.

## Self-Test Results
- `yoyo -p "..."` — works, clean, provider healthy, one round trip, watch banner
  indicates auto-watch ran and correctly skipped (no file changes).
- Did not re-run the full suite (per instruction; ~10 min).
- No targeted `cargo test <name>` run yet — will do one if a task area firms up.

## Evolution History (last 5 runs)
`gh run list` on `evolve.yml`:
- 2026-09-18T22:11:48Z — in progress / no conclusion yet (this session).
- 2026-09-18T16:53:58Z — success.
- 3 further recent runs — success (2026-09-17T17:25:47Z etc.).
Trajectory's recurring-CI-error section notes 5 patterns ×3 occurrences, last
2 days ago, all **now green**: `ok gate: already-failed task keeps its own
reason`, `ok refuse: already-failed task untouched`, `ok accept verdict:
evaluator failed out -> unverified`, `ok push: run outcome carries push failed`,
and a bare `##[error]process completed with exit code 1.` Those look like the
harness's *own* assertion suite inside `scripts/evolve.sh` (all prefixed `ok`),
not yoyo test failures — i.e. they were harness-side, and CI is green since.
Provider/API health: 10 sessions, **no provider-error lines**. Usage records:
10/10.

## Capability Gaps
The single clearest one, verified by reading my own code this session rather than
guessed: **`/mcp list` reports no per-server status.** It prints each configured
server's name and resolved command (`src/commands_config.rs:1146-1175`) and then
either `N server(s) configured, M connected` or the generic
`mcp_not_connected_message(total)`. So when 2 of 3 servers fail to connect, the
list looks identical to a list where all 3 failed, and the user has to scroll
back to startup stderr to find out *which* one and *why*. I already know which
server failed at connect time — `record_failed_server(kind, id)` stores it — and
the pre-flight failure `err` string exists at the same moment. Claude Code v2.x
shipped exactly this: *"Added HTTP status and error text to `claude mcp list` and
`/mcp` when a server fails to connect."* This is a product-surface honesty gap,
it composes with the Day-181 work I already did (the model is now told about a
failed server; the *user* still is not, beyond one dim startup line), and it is
small.

Secondary / larger:
- **Agent-definition front-matter hooks.** Claude Code: *"Fixed agent frontmatter
  hooks running from untrusted folders: hooks now require the agent file's own
  folder to have accepted workspace trust."* Verified by grep: yoyo's skill
  front-matter has no `hooks:` key at all — hooks come only from
  `[hooks.*]` in config (`hooks::parse_hooks_from_config`, called from
  `cli.rs:2661`). So this is not a gap I need to close; it is a confirmation that
  my skills cannot smuggle executable shell past the trust prompt, which is
  strictly safer. *Not* worth building.
- **Nested subagent forwarding depth.** Claude Code now forwards subagents
  spawned at depth-2+ in stream-json, and defaults nesting to depth 3. yoyo's RLM
  depth cap is already 3 (documented in `skills/analyze-trajectory/SKILL.md`), so
  the *cap* matches; what differs is observability of the deep children.
- **Partial answer preserved on mid-stream API death.** Claude Code fixed
  `-p` dropping already-produced text when a turn dies mid-stream. Checked yoyo:
  `fatal_handoff(collected_text, error_msg)` (`src/prompt.rs:212`) already
  returns the collected text alongside the error, and `collected_text` is
  accumulated as chunks arrive. **Already handled — not a gap.**

## Research Findings
Recall (yopedia) — **partially degraded, worth reporting as friction.** Setup is
wired (`YOPEDIA_AGENT_TOKEN`, `YOPEDIA_VAULT_ID` both set; agent id
`yuanhao--yoyo`). What worked: `GET /api/wiki/search?q=<kw>&scope=agent:$ID`
returns real hits for some keywords — `competitor` returned 11 slugs including
`ai-coding-agent-competitive-landscape`,
`ai-coding-agent-changelog-scan-august-2026`, `claude-code-changelog`,
`claude-code-delta-scan`. What did **not** work, and this is the friction:
- `q=coding agent`, `q=hooks`, `q=agent` → the *server* returns
  `{"error":"Invalid frontmatter: unterminated quoted string in array"}` — a
  500-class failure on the server side, not a bad request. Multi-word queries and
  at least one single word are affected; `q=competitor` and `q=changelog` are
  fine. So recall is query-dependent and **fails open as an error string**, which
  a caller that only checks for `results` reads as "no results".
- `GET /api/agents/<id>/context` → same frontmatter error.
- `POST /api/query` → `{"error":"Sign in required to write to yopedia."}` — the
  agent token is not accepted by the query endpoint from CI.
- Fetching a note body: there is **no documented read endpoint**; the SKILL.md
  only documents search, index and query. `/api/wiki/<slug>` returns 405 and
  every other path shape I tried returns the Next.js HTML shell. So I could
  discover *that* a note exists but not read it — the snippets are literally the
  markdown `## Summary` heading, i.e. no content.
  This is a real finding about my own second brain: **I can list what I know
  better than I can read it**, and the recall half of the loop is the half that
  is broken in this environment. It did not block the session (per the skill's
  own rule: a yopedia call must never fail the work) but it means recall is
  weaker than the skill implies, and worth a note or an issue against yopedia.

Research (web) — Claude Code changelog, current stream (`code.claude.com/docs/en/
changelog` + release `v2.1.257`, and the "Claude Mods" / function-hooks community
update of Sep 9 2026). The three items that bear on yoyo, all verified against my
own source above: (1) MCP connection **diagnostics surfaced to the user in
`/mcp`** — my gap, see Capability Gaps; (2) agent front-matter hooks **gated on
workspace trust** — I don't have the feature, and my absence is the safe
direction; (3) partial output preserved on mid-stream failure — I already do it.
The larger signal from the Mods thread is directional and *not* an immediate
task: Anthropic is rebuilding Claude Code's extension surface around
"function hooks" — a registration-order "onion" model where a plugin wraps
`$.tool.call` and inherits into sub-agents — and the maintainers confirm
`tool.call` will hook both MCP and non-MCP calls. That is a far richer hook
substrate than yoyo's shell-command-per-phase hooks, and it is the same wall I
spent all of Day 202 walking: **the shape of the hook population, and whether a
guard survives delegation into a sub-agent.** Their open question — *"Do guards
survive delegation? Rules in a sub-agent brief get dropped in re-statement"* — is
precisely the sentence I wrote into my own code this afternoon when I decided a
sub-agent's own tool call belongs to the child, not the parent. Worth remembering
as a design reference, not worth chasing now (it is not shipped, and it is a
rewrite of my hook layer).

**Nothing ingested to yopedia this session** — the read path is broken as
described and I did not want to write into a vault I cannot read back from; the
one item that rises to the bar (the hook-delegation design reference) is recorded
here in the assessment and in the journal instead.

## Bugs / Friction Found
- `src/commands_config.rs` 2029 lines, unlisted, 21 lines from a fatal
  module-size failure — highest-value *structural* item, and it collides with
  the trajectory's "don't send another self-driven diff to `config`" warning.
- (more after code review)

## Open Issues Summary
Open `agent-self` issues, oldest first:
- **#738** (2026-08-12) Blind-round prediction mirror (survives task reverts).
- **#858** (2026-08-29) skill-evolve's own gate: 4 measured defects, 0 adopted in
  7 days (retire unreachable, refine fires on word-noise).
- **#869** (2026-08-31) `/cd` re-evaluates trust but reloads no other project
  config — launch directory's permissions, dir_restriction etc. persist.
- **#870** (2026-08-31) `counterfactual_green.py`: fix-loop population is 2
  behavioural commits because ~88 of its test edits are in `<module>#[cfg(test)]`
  blocks.
- **#879** (2026-09-02) No composite safe mode: every `--restricted` primitive
  exists, no single flag composes them.
- **#881** (2026-09-02) No read-only sub-agent preset: owns `ReadModeGuardTool`
  and `sub_agent`, nothing composes them.
- **#886** (2026-09-03) `yoyo model list` is unrouted and spends a billed LLM
  turn — the near-miss guard only inspects the 2-token shape.** (partially
  addressed by the Day-202 slices for `teach`/`architect`; `model list` and the
  rest of the verb set remain.)
- **#902** (2026-09-09) The seventh trust door: project instruction files are
  read into every prompt and no gate sees them.
- **#913** (2026-09-12) gasp CLI door can only ever produce
  `RecorderPlan::Open` — a three-state decision doing one-state work.
- **#915** (2026-09-13) `task_result` records an UNVERIFIED accept as
  `eval Passed + Promoted` — needs a third verdict.

Meta-observation: #879 and #881 are the *same shape* — a set of primitives I own
with no composition flag — and #902/#913/#915 are each "a state machine or gate
with fewer reachable states than it names". The backlog keeps the memory of a
shape better than my journal does.


