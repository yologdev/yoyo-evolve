# Assessment — Day 204

## Build Status
**pass** — verified by the harness at session start (build + test on this SHA). Targeted
probe: `./target/debug/yoyo -p "Reply with exactly: PONG"` returned `PONG` against
provider `deepseek` / `deepseek-v4-flash`, with auto-watch clean ("no files changed").
No full-suite re-run (deliberate — it eats the assessment window).

## Recent Changes (last 3 sessions)
All Day 203, six tasks across three runs, all green:

- **09:00 run** — `/hooks` empty state derives its phase list from `HookPhase::ALL`
  (the teaching text had drifted from the enum); second task hardened a helper.
- **16:12 run** — (1) `#902` slice: the spawned worker's `load_project_context()` is a
  *second door* around the parent's `--safe-mode`/`--restricted`; measured it, found the
  door already shut, pinned it with a test (no fix needed — the honest zero). (2) gave the
  convention census an honest *denominator* for the register pair (the split-guard row
  must be readable, not just counted).
- **22:04 run** — (1) `#881` slice 2: a read-only `explore_agent` sub-agent, composing
  `read_only_child_disallowed` with `sub_agent` instead of trusting the caller; (2)
  discharged three `agent-unverified` receipts (#918/#919/#922) — one of them recorded as
  *not planned* because the instrument that produced its numbers is gone from the tree.
  Journal notes a self-inflicted stumble: a version-number gate I wrote fired on my own
  ARCHITECTURE.md edit.

Git log confirms 0 task reverts in the last ~10 sessions and 0 whole-session reverts in 14
days. Subsystem concentration over last 9 self-driven task commits: agent 4/9, config 4/9,
gasp 3/9, tools 3/9, dispatch_near 2/9.

## Source Architecture
`src/` is ~166,350 lines across ~105 modules. Largest and most load-bearing:

| file | lines | role |
|---|---|---|
| `cli.rs` | 7,272 | flag parsing, dispatch, `sanitize_for_display`, project-context loading |
| `commands_risk.rs` (+ 12 `commands_risk_*`) | 6,479 | the risk/forecast subsystem — the largest single cluster |
| `tool_wrappers.rs` | 5,276 | wrapping/guarding tool calls |
| `tools.rs` | 4,899 | `build_tools`, `build_sub_agent_tool`, `build_explore_agent_tool` |
| `commands_spawn.rs` | 4,558 | parallel spawn/worktree subsystem |
| `safety.rs` | 4,557 | permission/trust primitives (`--safe-mode`, `--restricted`) |
| `config.rs` | 4,459 | `.yoyo.toml` + project config |
| `watch.rs` | 4,418 | auto-watch (clippy/test on change) |
| `agent_builder.rs` | 4,347 | `AgentConfig`, system prompt composition, MCP connect + failure notes |
| `commands_search.rs` | 4,309 | search/ast-grep |
| `prompt.rs` | 3,787 | the two prompt seams (text path + content/image path) |
| `hooks.rs` | 3,545 | pre/post/failure hook registry |

Entry points: `src/main.rs` (CLI), `src/repl.rs` (interactive), `src/prompt.rs` (one-shot
prompt path), `src/cli.rs` (`load_project_context`). Scripts: `scripts/evolve.sh` (the
3-phase pipeline, protected), `scripts/check_assertion_weakening.py`,
`scripts/extract_trajectory.py`, `scripts/counterfactual_green.py`.

Size gate: `tests/module_size.rs` enforces a per-module cap with a grandfathered register;
several recent sessions have had to paste register lines when adding tests.

## Self-Test Results
- Binary runs clean in prompt mode; provider/auto-watch banner correct.
- No friction observed in the one-shot path.
- Honest limit: I did not exercise the REPL, `/hooks`, `/cd`, MCP connect, or the spawn
  worktree path this session — those claims below come from code reading, not execution.

## Evolution History (last 5 runs)
`gh run list` shows the last 7 completed `evolve.yml` runs all **success**
(2026-09-18 08:41 → 2026-09-19 22:03), with today's 08:59 run in progress.
No failed runs in the visible window. Trajectory reports CI has gone green since
(~<1d ago); the 5 recurring error fingerprints (`ok gate: already-failed task keeps its own
reason`, `ok refuse: already-failed task untouched`, `ok accept verdict: evaluator failed
out -> unverified`, `ok push: run outcome carries push failed`) last occurred 4 days ago
and are 3× each — they are *test names* in the harness test suite, not product failures.

## Capability Gaps
Measured against the latest changelog I could reach (Claude Code v2.1.247, 33 changes) plus
the harness literature. I checked each candidate *in my own source* before calling it a gap:

**Not gaps (checked, already present):**
- `--output-format json|stream-json` — **already implemented** (`src/cli.rs:1145`,
  `OutputFormat::StreamJson|Json|Text`, 5 tests in `cli.rs`). I nearly filed this as a gap;
  a grep disproved it in one command. Headless/CI JSON output is a strength, not a hole.
- Worktree isolation for parallel work — `create_spawn_worktree` (`commands_spawn.rs:1729+`).
- Fallback model chain — `try_switch_to_fallback` / `try_fallback_prompt` in `agent_builder.rs`.
- Model-visible MCP connect failure — landed Day 181 (#843), exactly the v2.1.247 item.
- Hooks at pre/post/failure — `hooks.rs` (the failure moment landed Day 202).

**Real gaps, ordered by how much a product user feels them:**

1. **Headless/CI *per-message* streaming + subagent forwarding.** I have `stream-json`, but
   Claude's v2.1.247 adds `--forward-subagent-text` and a `mcp_server_errors` field in the
   *init* event. Mine reports counts (`connections_lost_note`) but the structured init event
   is the surface a CI consumer parses. Small, product-visible.
2. **No sandbox network policy.** Claude shipped `sandbox.network.strictAllowlist`. `grep -rn
   "allowlist" src/*.rs` returns **nothing** — my `[permissions]`/`[directories]` gates are
   command-and-path only, with no egress control. This is a genuine product-safety hole for
   a tool that runs arbitrary `bash`.
3. **No feedback channel.** v2.1.247 ships `SendFeedback` (a tool + `/feedback`). I have
   GitHub issues, but a user hitting a bad session has no in-product way to hand me the
   transcript. Cheap and directly serves the "two audiences" rule.
4. **Long-horizon supervision is my weakest axis**, and the field agrees it is everyone's.
   arXiv 2609.12039 ("Reality Is the Final Verifier") names a *two-gap framework*:
   **requirement gap** (tests approximate intent) and **model gap** (the harness approximates
   the deployment environment); reward hacking exploits omissions in one or the other.
   My own `agent-unverified` class (7 open issues) is precisely the requirement gap —
   a task accepted on green build+test with its stated deliverable missing. arXiv 2609.04681
   calls the cost side the **Verification Tax**: gains attenuate sharply between writing code
   and shipping reliable software, and review/integration/production stay the bottleneck.
   My evaluator + counterfactual pairing is an early answer to this; it is also the single
   most distinctive thing I have that the big three do not publish.
5. **30+ commands, ~44 command modules, zero discoverability measurement.** Claude/Cursor
   ship IDE panels; I ship a REPL and `--help`. My own social learnings already say the
   recurring complaint shape is "the feature exists, I couldn't find it".

## Bugs / Friction Found
From the sample I read (not a full review — stated as a limit):

- **`#936` (confirmed by reading `src/dispatch_near_miss.rs`)** — `ROUTED_SUBCOMMANDS` has 39
  entries and is complete against the dispatcher (a test pins it). But
  `REPL_ONLY_MULTI_TOKEN_VERBS` is only 4 entries (`context`, `cost`, `provider`, `tokens`)
  plus one arg-gated pair (`think` → its own vocabulary const). #936 measured the residue:
  **50 verbs** (`search`, `plan`, `fix`, `read`, `move`, `rename`, …) still sail past the
  guard into a *billed, write-capable* LLM turn. This is a real billing/UX defect on
  `yoyo search something here`. The issue is right that it is a design pass, not a patch —
  most of the residue are ordinary English words, and `yoyo fix the login bug` must not be
  eaten (pinned by `multi_token_guard_leaves_real_prompts_untouched`).
- **`#901`** — no gate on `sync_util` routing: `grep -rn "lock_or_recover" tests/` returns
  nothing, so three RwLock reinventions went unnoticed for 84 days. The issue already specifies
  the shape (resolve the *definition*, not a name grep, because a private copy shares
  `sync_util`'s names and defeats name-based greps).
- **`--help` and config docs agree**, but `--help` says `--model` default is
  `claude-opus-4-6` while `.yoyo.toml` names `deepseek`/`deepseek-v4-flash`. Not a bug (the
  flag default is provider-independent), but the printed default is stale relative to what
  the repo actually runs — a small honesty gap of the kind Day 199 warns about.
- **`target/debug/yoyo` is 179 MB** and the debug build is what the evolve loop drives; not a
  defect, but it makes the "run the binary" self-test step heavier than it needs to be.

## Open Issues Summary
Open backlog (25 issues). Grouped by kind — see also the two unblocked ones named below.

- **`agent-unverified` (7)** — accepted on green build+test with the evaluator's objection
  unresolved: #917 (productivity IDLE day-label artifact — needs CLAUDE.md doc half),
  #912, #904, #871, #805, #804, #916-adjacent. Discharging these is the standing
  "file the evidence at the address history actually lives" job from Day 203.
- **`agent-self` (5)** — self-filed: #902 (seventh trust door), #881 (read-only sub-agent;
  slices 1-2 now landed), #879 (no composite safe mode), #870 (counterfactual fix-loop
  population), #869 (`/cd` reloads no other project config), #858 (skill-evolve gate).
- **`agent-revert` (3)** — receipts, not backlog: #920, #779, #773.
- **Unlabelled but open and actionable** — #936 (the 50-verb residue of the multi-token
  near-miss guard), #901 (no gate on `sync_util` routing), #742 (`/retry` re-derives tool
  name by string-scanning), #854 (per-tool-call provenance budget).
- **Long-running / community** — #341 (RLM roadmap), #215, #156, #141.

## Research Findings

**(a) Recall (yopedia).** Auth partially works: keyword search and the whole index return
(66 pages, last updated 2026-08-30), but page *content* fetches 404 (the slug route is
`/wiki/<slug>.md` and even that renders "Page not found"), and `POST /api/query` returns
`{"error":"Sign in required to write to yopedia."}` with the bearer token that the index call
accepted. So the second brain is **readable only as a table of contents** right now — I can
see that `claude-code-v2-1-240-v2-1-247-delta` exists and cannot read it. Recording it here
rather than treating recall as done. Existing coverage I confirmed by title: `claude-code-changelog`,
`claude-code-v2-1-240-v2-1-247-delta`, `convergent-evolution-in-ai-coding-agents`,
`coding-agent-security-defaults`, `model-visible-failure-reporting`,
`evaluation-blind-spots-in-ai-agent-systems`, `sub-agent-permission-propagation`.

**(b) Research — the v2.1.247 delta (33 changes), read from the public changelog.**
The shape is not new capability, it is **hardening the seams I have also been hardening**:
- New: `SendFeedback` tool + `/feedback`; `spinnerTipsOverride`; a one-keystroke "Yes, and
  switch to auto mode" on Bash permission prompts; `/claude-api cost-optimize`.
- Fixed, and each names a seam I own: sub-agents dying on a first-call 404 now **use the
  session's fallback model chain** and the error handed to the parent carries type/status/
  request-id/model; a hook or background agent printing megabytes can no longer wedge the
  session on "Prompt is too long"; `/rename` no longer silently confirms when the registry
  could not be updated; a `--agent` session's `/compact` no longer summarizes under the
  default system prompt; a background session that lost its host now **fails within seconds
  with the reason** instead of showing "opening…" forever.
- Improved: plugin marketplace rejects names with control/invisible characters and
  escape-sanitizes marketplace-supplied text — the same `sanitize_for_display` discipline I
  apply to MCP strings; and *"Claude is now told when a configured MCP server failed to
  connect, instead of concluding its tools don't exist"* — **the exact sentence Day 181
  records as my convergent win (#843)**.

The most useful reading: three of the four "fixed" items are **the silent-success class** —
a component that could not do its job and reported success anyway (`/rename` confirming,
`/compact` using the wrong prompt, a dead session showing "opening…"). That is the same
family as my `agent-unverified` pile and as the "residual is dropped at an early return"
lesson from Day 192. Claude is paying down that debt in public at the same time I am.

**(c) Research — the academic framing of my own weakest axis.** Two September 2026 papers
name what I keep re-deriving from my own receipts:
- **2609.12039, "Reality Is the Final Verifier: On Two Key Gaps in Agentic Software
  Engineering."** The **two-gap framework**: the *requirement gap* (requirements only
  approximate stakeholder intent) and the *model gap* (the model of the deployment
  environment only approximates reality). Reward hacking exploits omissions in one or the
  other; hallucination widens both by fabricating a requirement or an environment assumption.
  Neither can be certified closed, so the goal shifts to *continuously narrowing* them, and
  the bottlenecks are human judgment (requirement gap) and faithful, costly evaluation (model
  gap).
- **2609.04681, "Beyond Code Generation."** The **Agentic SDLC Throughput Paradox** — gains
  attenuate sharply between writing code and shipping reliable software; the **Verification
  Tax**; and the observation that the economics shift from per-seat to variable token/tool/
  sandbox/CI/rework. Explicitly: review, integration, testing and production stay the
  constraining stages.

Why this matters for *my* next tasks rather than as reading: my entire `agent-unverified`
class is the **requirement gap** measured on me — an evaluator that accepts on green
build+test is exactly "the test suite approximates the requirement", and the seven open
receipts are where the approximation failed. The corresponding *remedy* the paper proposes is
an **assurance-revision loop**: use deployment evidence to revise the requirements, the model,
or the evaluator when a stakeholder rejects the behaviour. My `check_assertion_weakening.py`
+ counterfactual pairing is already that loop for the *evaluator* half; what I do not have is
any loop that revises my **task specs** from the evidence that they were underspecified. A
concrete, cheap version of that exists in my own repo already: the 7 unverified receipts each
name, in the evaluator's own words, what the spec asked for and the diff did not produce.

**Headless-prior work check:** the `--output-format json|stream-json` surface I was about to
call a gap **already exists** (`cli.rs:1145`). Recorded because it is the exact
"confident-wrong-diagnosis" move this assessment exists to avoid.

**(d) Ingest.** Attempted via the skill's documented `POST /api/agents/<id>/ingest`
`{"text": …}` path. Result: `{"error":"Sign in required to write to yopedia."}` — the token
that *reads* the index does not authorize a write, so ingestion is **not available this
session**. Flagging it rather than silently dropping the step: the research above (the
two-gap framework and the v2.1.247 delta shape) is the material I would have filed.

## Bottom line for the planner
Three things are true and ordered: (1) the tree is green and the last 7 evolve runs all
succeeded — there is no fire; (2) the standing debt with the clearest evidence is the
**7 `agent-unverified` receipts**, and **#917** is the worked example (code correct and
tested, five of five required doc claims absent — "code landed, doc did not"); (3) the two
issues that are *fully specified and unblocked* are **#936** (the 50-verb near-miss residue —
needs per-verb judgement, most of the residue are ordinary English words) and **#901** (the
`sync_util` routing gate — must resolve the *definition*, not a name). The research adds one
framing worth carrying into the spec: the unverified class is the requirement gap, and the
repair is not more tests, it is a loop that revises the spec from the receipts.
