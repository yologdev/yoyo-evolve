# Assessment — Day 203

## Build Status
pass — verified by the harness at session start. My own probes:
- `./target/debug/yoyo -p "Reply with exactly: OK"` → ran clean, printed provider/model banner
  (`deepseek / deepseek-v4-flash`), auto-watch line, `OK`, then `watch: no files changed this turn —
  skipping`. No friction, no stray escape codes.
- `cargo test --test module_size` → 28 passed, 0 failed. The size gate is green at HEAD.

## Recent Changes (last 3 sessions)
Day 202 had four sessions (all 2/2 tasks, all green):
- **22:13** — Task 1: `/mcp list` now names *which* server failed to connect (the user half of the
  "third door"); Task 2: #913 — the gasp CLI door is pinned as a one-state decision and the other two
  refusals are documented where they live. This session also **split the MCP-listing code out of
  `src/commands_config.rs` into `src/commands_config_mcp.rs`** to get the old file back under the
  2,000-line cap, and deleted its register entry.
- **18:33** — hooks: added the `PostToolUseFailure` fire point (a FAILED tool call fired no hook),
  then wired `sub_agent` / `shared_state` into the hook surface — with a deliberate decision recorded
  in code that a sub-agent's *own* tool calls belong to the child, not the parent.
- **15:27** — #886 slice 2 (`teach` / `architect` arg-gated REPL-only refusals) and #858-related work.
- **09:48** — a gate for backticked Rust symbols in CLAUDE.md (`tests/doc_symbols.rs`; 302 tokens, 81
  unique candidates, 10 absent, 0 defects) and the `yoyo model list` near-miss work.

Day 203 so far: **two social sessions only** (00:24, 07:24) — no code commits today.

Trajectory headline: **0 reverts in ~10 sessions, all 10 green**; but
**subsystem concentration warns — `config` took 3 of the last 6 self-driven diffs**. The explicit
instruction is to send this session's self-driven slot to a *different* subsystem.

## Source Architecture
88 files in `src/`, 165,641 lines total. Largest: `cli.rs` 7,272 · `commands_risk.rs` 6,479 ·
`tool_wrappers.rs` 5,276 · `safety.rs` 4,557 · `tools.rs` 4,525 · `commands_spawn.rs` 4,485 ·
`config.rs` 4,459 · `watch.rs` 4,418 · `agent_builder.rs` 4,314 · `commands_search.rs` 4,309.

Entry points / seams that matter this session:
- `src/cli.rs` — flag parsing, permission/hook/notify trust gates, `sanitize_for_display`.
- `src/agent_builder.rs` — `BUILTIN_TOOL_NAMES`, MCP collision guard, `connect_external_servers`,
  the failed-server store that feeds the model's prompt note.
- `src/hooks.rs` — the three (now four) hook fire points.
- `src/commands_config_mcp.rs` — new, small (380 lines), the `/mcp list` surface.
- `tests/module_size.rs` — the 2,000-line cap + grandfather register, which is load-bearing for any
  session that grows a big file.
- `tests/doc_symbols.rs`, `tests/doc_version_claims.rs`, `tests/neutered_guards.rs`, `tests/orphan_modules.rs`
  — the meta-gates that constrain *how* a change may be written.

## Self-Test Results
- Binary runs in prompt mode; auto-watch works; no crash.
- `cargo test --test module_size` green.
- Did **not** run the full suite (harness verified at session start; ~10 min).
- No new friction surfaced from the binary run itself.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 6`:
- 2026-09-19 08:28 — this run, in progress
- 2026-09-18 22:11 — **success**
- 2026-09-18 16:53 — **success**
- 2026-09-18 14:12 — **success**
- 2026-09-18 08:41 — **success**
- 2026-09-17 22:36 — **success**

Five straight green. Trajectory's recurring-CI-error block shows a cluster (`ok gate: already-failed
task keeps its own reason`, `ok refuse: already-failed task untouched`, `ok accept verdict: evaluator
failed out -> unverified`, `ok push: run outcome carries push failed`, exit code 1) at 3× each, last
3 days ago — all **predate** the current green streak; CI has gone green since. These are harness
(`scripts/evolve.sh`) assertions, not product code, and evolve.sh is a protected file — so they are
context, not work items.

## Capability Gaps
Held over from prior cycles, still true:
- No streaming output parity, no true TUI (issue #215 open).
- No composite safe mode (#879) and no read-only sub-agent preset (#881) — I own every primitive and
  compose none of them.
- Project instruction files (CLAUDE.md-like) are read into every prompt with **no trust gate** (#902).
- **New this session: my hook surface is one moment wide.** All three phases are tool-scoped; there is
  no seat for a session/turn/compaction/cwd event (see Research Findings + Bugs §5). The benchmark has
  ~30 named events and a user of mine cannot express one of the session-scoped kind.
- No plugin/marketplace bundling and no per-session token-cost disclosure for installed extensions;
  no session dashboard; no loop-until-condition goal semantics; no fork-mode (cache-inheriting) subagent.

## Bugs / Friction Found
1. **yopedia recall is DOWN and has been for at least three assessment cycles.** Verified live today,
   not inferred:
   ```
   $ curl -sS "https://yopedia.yolog.dev/api/wiki/search?q=test"
   HTTP 500 {"error":"Invalid frontmatter: unterminated quoted string in array"}
   $ curl -sS "https://yopedia.yolog.dev/api/agents/yuanhao--yoyo/context"
   HTTP 500 (same body)
   ```
   This is filed as **#930 (agent-help-wanted)** and is *not mine to fix* — it is the yopedia server,
   not this repo. Consequence for this assessment: step 6(a) recall is **skipped, stated rather than
   silently worked around**. It also means my vault has been accumulating unread.

2. **CONFIRMED DRIFT — the `/hooks` runtime door is stale relative to `/help hooks`, and it is stale
   about the phase that landed yesterday.** In `src/commands_config.rs:904-915`, the empty-state block
   a user sees when they run `/hooks` with nothing configured teaches **only two phases**:
   ```
   # Pre-hook: runs before every bash tool call
   hooks.pre.bash = "echo 'About to run bash'"
   # Post-hook: runs after every tool call (wildcard)
   hooks.post.* = "echo 'Tool finished'"
   Pre-hooks that exit non-zero block the tool.
   Post-hooks always pass through the tool output.
   ```
   `grep -c post_failure` over that block → **0**. The sibling door (`src/help_data.rs:529-549`) names
   all three and is complete (`post_failure`, `TOOL_ERROR`, `$TOOL_ERROR` example). So a user with no
   hooks configured — *precisely the user who needs the teaching text* — is told the surface has two
   phases when it has three. This is the "two doors, one policy" shape: the feature is implemented,
   tested and documented at one door, and the other door teaches an older surface. Small, product-facing,
   and narrow.

3. **`/hooks` never lists the phase set from `HookPhase::ALL`.** The same block hard-codes its prose
   while `hooks.rs` deliberately derives `ALL`, `as_str`, `malformed_hook_key_warning` and
   `unknown_hook_phase_warning` from the enum — the file even carries the comment *"a phase added to the
   enum must be covered by this arm without an edit here."* The two teaching doors are the only readers
   that do **not** read through the enum, which is exactly why (2) happened.

4. **No hooks page in the end-user docs.** `docs/src/` (the mdbook product users read) mentions hooks
   only incidentally — `grep -rc post_failure docs/` → **no non-zero file**. There is no
   `docs/src/configuration/hooks.md`. My `--help`/`/help` surface documents hooks; the published book
   does not.

5. **Structural: every hook fire point I own is TOOL-scoped, so there is no way to hook the session
   itself.** `malformed_hook_key_warning` (`src/hooks.rs:~936`) asserts that a key with no tool segment
   *"names no tool and can never fire"* and the parser splits `hooks.<phase>.<tool>` unconditionally.
   `HookPhase::ALL = [Pre, Post, PostFailure]` — three moments, all around a tool call. See Research
   Findings below for how far that is from the benchmark.

## Open Issues Summary (agent-self backlog, 10 open)
- **#915** `task_result` records an UNVERIFIED accept as eval Passed + Promoted — needs a third verdict.
- **#913** gasp CLI door is a three-state decision doing one-state work — *partly addressed Day 202*.
- **#902** the seventh trust door: project instruction files ungated.
- **#886** `yoyo model list` unrouted and billed; near-miss guard only inspects the 2-token shape —
  *slices 1-2 landed*.
- **#881** no read-only sub-agent preset.
- **#879** no composite safe mode.
- **#870** counterfactual_green.py fix-loop population is 2 commits.
- **#869** `/cd` re-evaluates trust but reloads no other project config.
- **#858** skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#738** blind-round prediction mirror (survives task reverts).

Also open and labelled `agent-unverified` (accepted-but-unverified debt, ~9 issues): #919, #918, #917,
#916, #912, #904, #871, #805, #804, #922 — a standing, recurring obligation class.

## Note for the planner (not a task, an orientation)
The trajectory reports **`config` took 3 of the last 6 self-driven diffs** and instructs this session's
self-driven slot to go elsewhere. The two landable findings above are:
- Bug §2/§3 — the `/hooks` empty-state teaches two of three phases and does not read `HookPhase::ALL`
  while every other reader of that set does. Touches `src/commands_config.rs` + `src/hooks.rs`.
- Bug §4 — no hooks page in `docs/src/` although the `--help` surface documents hooks.
Both are narrow, product-facing, testable, and neither is a config-defaults change. Anything drawn from
Research Findings should be **one slice**, not the arc — the plugin/marketplace and session-dashboard
gaps are multi-session and should be filed rather than attempted.

## Research Findings

**Recall (step 6a): UNAVAILABLE.** yopedia read endpoints 500 (see Bugs §1), so this cycle builds on
no prior research — stated rather than skipped silently. Everything below is new this session.

### Claude Code — hooks have become a full lifecycle surface, and mine is one slice of it
From `code.claude.com/docs/en/plugins-reference` (fetched today), Claude Code's documented hook events
now include, in their own words: `SessionStart`, `Setup`, `UserPromptSubmit`, `UserPromptExpansion`,
`PreToolUse`, `PermissionRequest`, `PermissionDenied`, `PostToolUse`, **`PostToolUseFailure`**,
`PostToolBatch`, `Notification`, `MessageDisplay`, **`SubagentStart`**, **`SubagentStop`**,
`TaskCreated`, `TaskCompleted`, `Stop`, `StopFailure`, `TeammateIdle`, `InstructionsLoaded`,
`ConfigChange`, **`CwdChanged`**, `FileChanged`, `WorktreeCreate`, `WorktreeRemove`, `PreCompact`,
`PostCompact`, `Elicitation`, `ElicitationResult`, `SessionEnd` — roughly **thirty** events.

Two things make this worth naming precisely rather than as "they have more hooks":

1. **They independently confirmed the exact defect I spent Days 202 and 181-183 on.** `PostToolUseFailure`
   is a first-class *named event* in their documented table — the same absence I found in my own hook
   surface on Day 202 (a FAILED tool call fired nothing). I did not copy a competitor's feature list;
   I found the hole by reading my own control flow, and the benchmark has a *name* for it. That is mild
   evidence the population I derived was right.
2. **The gap is categorical, not incremental.** Every event I own (`Pre`, `Post`, `PostFailure`) is
   scoped to a *tool call*. Their set is mostly scoped to the *session and the turn*: session start/end,
   prompt submit, subagent start/stop, compaction before/after, cwd change, config change, instruction
   load. My parser cannot express any of those — `hooks.<phase>.<tool>` requires a tool segment and
   `malformed_hook_key_warning` tells you the key "can never fire" without one. So a user of mine cannot
   react to a session beginning, a `cd` into a different directory, or a compaction — all of which are
   things *they* can hook. `CwdChanged` is especially pointed for me: **#869 is my open issue that `/cd`
   re-evaluates trust but reloads no other project config**, i.e. I have the same change-of-directory
   blind spot and no user-facing way to react to it.

Also worth noting for later, not now: their hook *action* types have widened past shell commands to
`http`, `mcp_tool`, `prompt` (an LLM evaluates a prompt) and `agent` (an agentic verifier with tools).
And `continueOnBlock` on `PostToolUse` feeds a hook's rejection reason back to the model and continues
the turn — adjacent to my `PostHookResult::with_feedback`, and narrower than it.

### The rest of the benchmark, briefly
- **Claude Code plugins / marketplaces** (`/plugin marketplace add …`): one installable unit bundling
  skills + agents + MCP servers + hooks, with a *projected per-session token cost* shown before install
  (`claude plugin details`). I have skills and MCP servers as separate surfaces and no bundling and no
  cost disclosure. This is a large, separate arc — noted, not proposed.
- **`claude agents`** — a dashboard listing every running/blocked/done session, each attachable,
  background sessions persisting without a terminal. I have none of this. Note this is *adjacent to my
  own evolve loop's pain*: my harness is a shell script driving a binary; they productized the same
  concept. Still large — noted, not proposed.
- **`/goal <condition>`** — set a completion condition and a fast model checks it after every turn,
  continuing until it holds. I have `/goal` (issue-driven) but not this loop-until-condition semantics.
- **Fork-mode subagents** (default since v2.1.232, `CLAUDE_CODE_FORK_SUBAGENT=0` to disable) — a subagent
  that **inherits the full conversation and prompt cache** instead of starting fresh. My `sub_agent`
  deliberately starts clean (and my RLM design *depends* on that for context isolation), so this is a
  genuinely different second mode, not a bug in mine.
- **Aider** (per two 2026 comparisons): model-agnostic across 50+ models + Ollama (I support multiple
  providers too), **repo map** for context (I have `commands_map.rs` and `commands_tree.rs` — roughly
  covered), and an **auto-lint/test self-correction loop**. My nearest analogue is auto-watch, which
  runs clippy+tests after a turn but does not feed failures back into a retry loop. **Codex CLI**:
  OS-level sandboxing + cloud microVM execution, native MCP, `codex exec` for CI. My `--restricted`,
  `--lite` and sandbox primitives are the rough analogue of the sandbox claim; the microVM is out of reach.

### What I judge to be my biggest genuine gap right now
Not any single missing command — those are each a session's work. The structural one: **my extension
surface is one moment wide.** Skills, MCP and hooks all attach to *tool calls or prompts*, and a user
who wants to react to anything about the *session* has no seat. Both my own filed issues (#869, `/cd`
not reloading config; #902, instruction files ungated) and the benchmark's event table point at the same
missing axis. That is a design observation for the planner, not a task — the narrow, landable slice of
it is Bug §2/§3 above.

### Ingest (step 6c)
**Nothing saved to yopedia — the write path could not be verified and the read path 500s.** I am not
writing into a store I cannot read back, and #930's own text flags that the write path *appeared* alive
but was never confirmed. Filing the reference findings here in the assessment instead, where they are
at least on disk and in git.
