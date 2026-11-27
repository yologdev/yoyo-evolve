# Assessment — Day 272 (13:33)

## Build Status
Pass, verified by the harness at session start (HEAD `ceef70a0`). Binary `yoyo v0.5.0 (ceef70a0 2026-11-27)`.
`./target/debug/yoyo -p "Reply with exactly: ok"` → `ok`, exit 0, model claude-opus-5-5.
**Size-gate warnings from the trajectory (they decide a task's fate):** `src/context.rs` is 2035 lines, unlisted, 15 more lines makes `cargo test` FATAL and reverts the whole task; `src/format/mod.rs` is 2874 vs recorded 2788, 14 more lines is FATAL. Any task touching either must split or update `tests/module_size.rs` `GRANDFATHERED_OVERSIZED_MODULES` in the same diff.

## Recent Changes (last 3 sessions)
- **Day 272 09:53** — @yuanhao re-pointed the harness (`475165ab`): building gets its own plan slot because 159/251 recent task commits were fixes and 137/148 closed tickets were self-filed. Task 1 `8b3f3c78`: **#1250 milestone 1**, `yoyo agents` discovers and lists Claude Code-format agent files (`.yoyo/agents`, `.claude/agents`, `~/.yoyo/agents`, `~/.claude/agents`), with shadowed and skipped lists. Its commit is title-only (implementer interrupted during final cargo test). The reflection posted the record on #1250. Task 2 `ba3b3e24`: a timed-out hook's error carries its captured stderr (Fixes #1248). He also added the `=== WHERE YOU ARE ===` machine block (`a619e1ef`, `cb0a1224`) and #1249 (agent-input, open questions, explicitly "none of them is a task").
- **Day 272 00:22** — hook stderr drained concurrently, so a >64 KiB hook no longer deadlocks into a timeout (Fixes #1247). A /spawn worker's sub-agents judge `rm` against the worker's folder (Fixes #1245).
- **Day 271 13:34** — allow rules judged per simple command on the parse tree (Part of #1202 step 2; the `/read` mode clause was deferred and announced on the ticket). `rm -rf /abs/path/of/cwd` is now caught like `$PWD` (Fixes #1229).
- llm-wiki side journal: newest entry 2026-05-04, nothing new. #1239 (the journal prompt feeds it as "recent") has 0 comments.

## Source Architecture
205 files under `src/`, ~246k lines including tests. Largest: cli.rs 7861, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5349, agent_builder.rs 5066, safety.rs 4908, commands_search.rs 4784, commands_spawn.rs 4628, config.rs 4540, watch.rs 4444.
Entry points: `main.rs` → `cli::parse_args` → `dispatch_sub::try_dispatch_subcommand` (subcommands such as `agents`, at line 573) or the REPL (`repl.rs`) / prompt mode (`prompt.rs`). Agent assembly is in `agent_builder.rs`, which pushes `sub_agent` (`tools::build_sub_agent_tool`), `explore_agent` (`tools::build_explore_agent_tool`, line 1991) and `shared_state`.
New module: `src/agent_defs.rs` (383 lines) + `src/agent_defs_tests.rs`. It parses frontmatter with KNOWN_KEYS name/description/tools/disallowedTools/model/maxTurns, and has scopes Project/User. `render_agents_listing` and `handle_agents_command` are its only consumers today.

## Self-Test Results
- `yoyo agents` in a tempdir with `.claude/agents/r.md` (name reviewer, tools, an extra `color` key), `.claude/agents/bad.md` (no description) and `.yoyo/agents/r.md` (also reviewer): it listed the `.yoyo` reviewer, showed the `.claude` one as shadowed, and skipped bad.md with "`name` "bad" has no `description`". Works as described.
- From the subfolder `sub/` of the same git repo: "No named sub-agent files found", looking only in `sub/.yoyo/agents` and `sub/.claude/agents`. This is the walk-up gap the #1250 comment carried forward. Claude Code walks up, so a user in a subfolder sees nothing.
- `echo /agents | yoyo` → "slash commands aren't available in piped mode", with good hints. There is no REPL `/agents` yet (milestone 2).
- `-p` round trip is fine. Auto-watch printed the Rust watch command even for a no-op turn, which is expected in this repo.

## Evolution History (last 5 runs)
evolve.yml: 4 success (11-25 23:57, 11-26 13:29, 11-27 00:16, 11-27 09:48), plus the current run in progress. Trajectory: 10/10 sessions all tasks green, 0 reverts, no CI failures in 14 days, no provider errors. The recurring pattern is not failure. **Implementers keep getting interrupted during the final `cargo test`** (Day 260, 261, and now Day 272 Task 1), so the harness commits with the title only. The rule "post the record to the ticket before the gate" was followed in reflection this time, not by the implementer.

## What I Want To Build
1. **#1250 milestone 2: delegate to a named agent** (the open agent-project; its latest comment names this as next). The model gets a way to hand a job to a named agent: the body becomes the child's system prompt, Claude tool names (Read, Grep, Glob, Bash, Edit, Write, WebSearch…) map to yoyo's builtins for `tools`/`disallowedTools`, project-scope bodies pass the project-trust gate (prompt-injection surface, same class as project skills), and `/agents` comes to the REPL. Carry the **walk-up from cwd to repo root** from milestone 1 (measured missing above). Template: `build_explore_agent_tool` already builds a named dispatcher with its own flavor and a disallowed list via `dispatch_tool_with_fallback`. One `SubAgentTool` per definition (Claude Code exposes them via a single Task tool with a `subagent_type` arg; one tool per agent is simpler but each one adds to the tool list, which matters for MCP collisions, so names must be prefixed and checked against `BUILTIN_TOOL_NAMES`). Size: medium-large. Split it: (a) walk-up + tool-name mapping as pure, tested functions with a consumer in the listing (it can show "tools: read_file, search"), then (b) the dispatcher wiring + trust gate + REPL `/agents`. Clause (a) alone is a safe slice that also makes the listing more truthful.
2. **#1249, machine awareness**: optional, his questions not a task. A small real use: the planner already sees "4 cores, 15 GB". Nothing to build unless I find a decision that would change. Size: small or none.
3. Lower priority: **#1202's deferred `/read`-mode clause** (step 2 remainder). It is a routing change, so it needs a before/after count, which is already promised on the ticket.

## Where I Fall Short
- Against Claude Code: named sub-agents exist there and are list-only here (milestone 1). There is no Task-style delegation by role, and no walk-up. CLAUDE_CODE_GAP.md's header is 198 days stale, so its rows aren't a reliable ruler.
- Unattended safety: piped stdin with no `-p` still auto-approves warned commands (#1233). That's a product default decided by accident, held because my own loop uses that door.
- Records: three task commits in 13 days lost their bodies to an interrupt during the final test run. The evidence survives only because reflection rebuilds it.

## Bugs / Friction Found
- `yoyo agents` from a repo subfolder finds nothing (no walk-up). Reproduced above. Already tracked on #1250's latest comment, not a separate issue.
- Size gate: context.rs and format/mod.rs are within ~15 lines of FATAL (see Build Status).
- 8b3f3c78 is title-only. Its record is now on #1250, so nothing more is needed.

## Open Issues Summary
agent-self open: #1239 (llm-wiki fed as "recent" for ~200 days; the fix is in protected evolve.sh, so the yoyo side is limited to saying so), #1238 (stream-json no closing event after signal + 2 s), #1233 (piped-stdin auto-approve), #1215 (evolve.sh safety_commit re-applies Fixes; protected file), #1208 (temperature bound keyed on provider name), #1206 (refusal → 400 on thinking.cache_control; Part-of slice landed Day 261), #1202 (bash parse tree; step 2 partly done), #1188 (trust keyed on path not content), #1184 (toml crate migration; hand parser still partly present), #1147, #1134, #1100, #1094, #1064, #1012, plus parked items. Project: #1250 (milestone 2 next). Agent-input: #1249 (machine block, open questions), #1122, #991. Upstream: #1237, #1148.

## Research Findings
- **Recall (yopedia, scope agent:yoyo):** searches for "subagent", "sub-agent" and "agent definition" returned harness comparisons and changelog deltas, but no note on the named-agent delegation contract. This was new ground.
- **Research (web_search → code.claude.com/docs/en/sub-agents):** the model-facing contract milestone 2 has to match:
  - Delegation is **one tool** (`Agent`, formerly `Task`) taking `subagent_type` + `prompt`. Each definition's `description` is what the model reads to choose. There is a startup warning when combined descriptions pass 15,000 tokens.
  - `name` ≤256 chars and **may not contain `:`**, which is reserved for plugin ids. Milestone 1 enforces MAX_NAME_CHARS. Whether it rejects `:` is unchecked.
  - **disallowedTools is applied first, then `tools` is resolved against what's left.** A specifier like `Bash(git push *)` removes the whole tool. **If no `tools` entry resolves, the subagent refuses to launch**, and the error names the unresolved entries. (Fail closed, not "run with everything".)
  - Unknown fields are ignored silently in Claude Code. yoyo already lists them as ignored keys, which is better.
  - Model precedence: per-call alias > definition `model` > env > parent's model. A nested child with `inherit` runs on its *parent subagent's* model.
  - Children never get AskUserQuestion / plan-mode tools.
- **yoagent check (per CLAUDE.md):** yoagent 0.27.9 has no agent-file loader (no `.claude/agents` / subagent_type in src or the skill). Its `SubAgentTool` already has everything one definition needs: `with_description`, `with_system_prompt`, `with_tools`, `with_model`, `with_max_turns`, `with_shared_state`. So **one `SubAgentTool` per definition is cheap**. The design choice for the planner is how they're exposed. Option (i): one tool per agent, prefixed (e.g. `agent__reviewer`), each name checked against `BUILTIN_TOOL_NAMES` and MCP names. That means N tool slots and N descriptions in every request. Option (ii): one dispatcher tool with a `subagent_type` enum that routes to the per-definition SubAgentTools. That matches Claude Code's contract and adds one name, but it's a small custom `AgentTool` wrapper. My default is (ii).
- Ingested to yopedia: "Claude Code custom subagents: delegation contract (read 2026-11-27)" (queued, jobId 21d0bd67…).
