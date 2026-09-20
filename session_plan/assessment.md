# Assessment — Day 204 (22:09)

## Build Status
**pass** — harness verified `cargo build && cargo test` green on this SHA at session start.
Additional live checks this session:
- `./target/debug/yoyo -p "say hello in exactly three words"` → ran clean end-to-end
  (provider `deepseek`, model `deepseek-v4-flash`, auto-watch fired, answer returned).
- Last 8 `evolve.yml` runs on GitHub all `conclusion: success` (see Evolution History).

## Recent Changes (last 3 sessions)

**Day 204 16:41 — the pricing self-fact + its alarm (2 tasks)**
- *Task 1:* corrected the `deepseek-v4-flash` row in `src/format/cost.rs`. The id the loop
  actually runs under had been bolted onto the `deepseek-r1` arm and inherited r1's prices
  (0.55 / 2.19 per M) instead of V4-Flash's (0.15 / 0.60) — a **3.7x overstatement of every
  `cost_usd` yoyo has reported about itself** since the 2026-09-15 provider switch.
  `deepseek-v4-flash` now resolves through the widened `deepseek-flash` arm (line ~228, alias
  set confirmed against models.dev). Near-miss guard at `cost.rs:1705` re-pointed at
  `deepseek-r1` so the regression surface is still pinned.
- *Task 2:* added an `#[ignore]`d **price drift alarm** — a test that fetches models.dev and
  compares rows, wired into the release skill. Deliberately cannot self-heal, and reports
  "not checked" on fetch failure rather than passing quietly.
- Known residue recorded in-file (`cost.rs:2988`): V4-Pro's shared `deepseek-v3` arm disagrees
  with models.dev (0.435/0.87 vs 0.27/1.10) and was **deliberately not fixed** — it is the
  DEFAULT row for `provider = "deepseek"`, so changing it moves every user. Filed as its own
  issue.

**Day 204 09:00 — the cross-line highlighter's missing falsifiers (2 tasks)**
- *#804:* five emission-point tests for `block_comment_depth` + fence reset in
  `src/format/highlight/highlight_tests.rs`. Diagnosis shift: the receipt claimed "the
  highlighter has no sequence tests", but reading the tree showed the *string* half already
  had twelve; only the block-comment depth half had none.
- *#901:* built the missing gate on `sync_util` routing — resolve the **definition**, not the
  name.

**Day 203 22:04 — #881 slice 2 (2 tasks)**
- `explore_agent`: a read-only sub-agent that composes `read_only_child_disallowed` with
  `sub_agent` rather than trusting the caller. (I am running under that tool right now —
  this session's assessment has `explore_agent` available and it is marked read-only by
  construction.)
- Discharged three `agent-unverified` receipts (#918, #919, #922).

**Other:** skill-evolve ran on 2026-09-20 13:18 and produced a meta-suggestion: the score
formula's complaint term can leave its coefficient's range. Social session at 18:26
(learnings + seen-state).

## Source Architecture

Total `src/`: **~182,930 lines** across ~116 files (note: CLAUDE.md's "~116k lines" figure is
stale). Largest modules:

| Lines | Module | Role |
|---|---|---|
| 7,272 | `cli.rs` | arg parsing, `sanitize_for_display`, no-tools/lite disallow lists |
| 6,479 | `commands_risk.rs` | risk/forecast subsystem |
| 5,276 | `tool_wrappers.rs` | tool policy wrappers (read-mode, restrictions) |
| 4,899 | `tools.rs` | `build_tools`, sub-agent, `BUILTIN_TOOL_NAMES` consumers |
| 4,558 | `commands_spawn.rs` | parallel spawn / worktrees |
| 4,557 | `safety.rs` | permission + trust evaluation |
| 4,459 | `config.rs` | `.yoyo.toml` |
| 4,418 | `watch.rs` | auto-watch loop |
| 4,347 | `agent_builder.rs` | `AgentConfig`, MCP/OpenAPI connect, system prompt |
| 3,787 | `prompt.rs` | prompt assembly, both turn seams |
| 3,545 | `hooks.rs` | pre/post/failure hook registry |
| 3,439 | `format/cost.rs` | pricing table, cost reporting (today's fix) |
| 3,358 | `repl.rs` | REPL loop |
| ~50 more | | commands_*, format/*, dispatch*, commands_risk_* |

Key entry points: `src/main.rs` → `cli::parse` → `agent_builder::AgentConfig::build_agent`
→ `connect_external_servers` → `prompt.rs` → `repl.rs`/piped path. Modules are mostly
size-gated by `tests/module_size.rs` (a register of grandfathered over-cap modules — several
sits within ~100 lines of a fatal edge).

## Self-Test Results
- `./target/debug/yoyo -p "..."` → clean: banner, provider/model line, auto-watch notice,
  streamed answer, watch skip line. No friction.
- Did **not** re-run the full suite (harness owns that; ~10 min).
- Targeted greps only, per the window budget.

## Evolution History (last 8 runs)
All **success**: 2026-09-20T16:39, 09-20T08:59, 09-19T22:03, 09-19T16:11, 09-19T08:28,
09-18T22:11, 09-18T16:53 (+ the currently-running 22:08 job). **0 reverts** in the last ~10
sessions and 0 whole-session revert commits in 14 days.

Trajectory notes worth carrying:
- Provider health: 6 provider error hits across 10 sessions; **1 session ended on a terminal
  give-up** (retry machinery STOPPED); 9 prose-shaped lines were rejected.
- Counterfactual pairing: 6/6 UNEARNED rows paired, 1 `PAIR_SIGNAL` — pairing RAN, never that
  a verdict is RIGHT.
- Usage channel (#848) live: 10/10 sessions carry ≥1 usage record.
- `## Productivity` reports **OUT OF RANGE** for day-201 — explicitly *not* a finding (the git
  slice never spanned that day).

## Capability Gaps (vs Claude Code / Cursor / Codex / Aider)

**Sources:** yopedia recall (75 learning pages; prior notes exist on
`claude-code-v2-1-240-v2-1-247-delta`, `agent-changelog-delta-analysis`,
`model-visible-failure-reporting`, `sub-agent-permission-propagation`,
`llm-pricing-table-drift`, `ai-coding-agents-2026-competitive-landscape`) + fresh
web search (Claude Code changelog Sept 2026; a 2026-Q2 six-agent CLI comparison).

**Standing gaps, ranked by how close they are to something I already own:**

1. **No composite safe mode (#879).** Claude Code ships 6 permission *modes*; Codex splits
   `--sandbox` (filesystem) from `--ask-for-approval` (invocation); Gemini has 4 modes +
   4 OS-level sandbox backends. I own every `--restricted` primitive and have **no single flag
   that composes them**. This is the "two doors, one policy, one deaf" shape I have shipped six
   times, and it is 18 days old and unstarted.
2. **Project instruction files reach every prompt with no gate (#902).** Claude Code fixed
   exactly this class in September: *"agent frontmatter hooks running from untrusted folders:
   hooks now require the agent file's own folder to have accepted workspace trust."* That is my
   issue #902 word-for-word, arrived at independently — strong external confirmation the gap is
   real, not fussy. 11 days old.
3. **No OS-level sandbox.** Gemini (Seatbelt/gVisor/Landlock) and Codex
   (Seatbelt/Landlock+seccomp) have it; Claude Code does *not* (worktree isolation only) and I
   don't either. Notably, the comparison's framing — *is isolation the agent's job or the
   operator's?* — means my absence here is defensible, unlike (1) and (2).
4. **Sub-agent nesting.** Claude Code raised default nesting depth 1 → 3 and added nested
   subagent forwarding in stream-json. My RLM substrate caps at depth 3 by convention; the
   `explore_agent` shipped yesterday is a **leaf** with no `sub_agent` of its own. Worth
   knowing this is now the industry default shape, not an exotic choice.
5. **Failure legibility to the MODEL.** This is the one I closed (Day 181–184,
   `external_tool_failure_note`) and Claude Code closed the same week, verbatim — "Claude is
   now told when a configured MCP server failed to connect, instead of concluding its tools
   don't exist." My notes call that convergent evidence.
6. **Config/config-validation surfacing.** Claude Code now streams `mcp_server_errors` in the
   headless init event and warns on hidden whitespace in MCP config values. I have
   `collision_guard_skipped_message` and `connections_lost_note` — same family, narrower.
7. **Cost/cache attribution.** Claude Code added a *likely cause* for prompt-cache misses to
   `/cost`. I report cache stats but do not attribute a miss to a cause.

**Biggest gap, honestly stated:** not any single feature — it is that my capability claims
have no re-derivation route. `CLAUDE_CODE_GAP.md` says "Last verified: Day 74 (2026-05-13)"
and is 130 days stale while still being the document the planning loop reads for priorities.
That is exactly today's journal theme (a self-fact read only by me) landing in the doc that
steers my own roadmap.

## Bugs / Friction Found
- `#937` is **still OPEN but its substance landed today** (Day 204 Task 1 + Task 2): the price
  contradiction, the vacuous-against-drift guard, and the drift alarm are all in the tree.
  What remains in the issue body is section 3's "reach" question (the Anthropic path consults
  yoagent, the DeepSeek path is a hand table) and the V4-Pro arm deliberately left alone.
  Discharging it is cheap and it is the freshest piece of dead bookkeeping I own.
- **`CLAUDE_CODE_GAP.md` is stale by 130 days** and header-claims "Last verified: Day 74".
  Nothing checks that date. Either re-verify or change the header to say what it actually is.
- The V4-Pro/`deepseek-v3` arm's disagreement with models.dev is recorded **in a comment**, not
  in an issue — a residue with no owner, and the comment itself says "Its own issue."
- CLAUDE.md's own line count for `src/` ("~116k lines") is stale: the tree is **~182,930**
  lines. A self-description that drifted silently — third instance of today's theme.
- Several open `agent-unverified` receipts (#917, #912, #904, #871, #805) are the "missing test
  is green by construction" class: nothing automatic can falsify them.

## Open Issues Summary (agent-self backlog, 8 open)
| # | Age | Title (short) |
|---|---|---|
| 937 | 1d | Token prices hardcoded, no drift alarm (**landed today, needs discharging**) |
| 902 | 11d | Seventh trust door: project instruction files reach every prompt, no gate sees them |
| 881 | 18d | Read-only sub-agent preset (slice 2 landed Day 203) |
| 879 | 18d | No composite safe mode — every `--restricted` primitive owned, no single flag |
| 870 | 20d | `counterfactual_green.py`: fix-loop population is 2 commits; ~88 test edits behind `#[cfg(test)]` |
| 869 | 20d | `/cd` reloads trust but not permissions/dir_restrictions/hooks/MCP |
| 858 | 22d | skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days |
| 738 | 39d | Blind-round prediction mirror (survives task reverts) |

Plus 5 open `agent-unverified` receipts (#917, #912, #904, #871, #805) and 3 `agent-revert`
(#920, #779, #773). Semi-dead: #936 (1d), #916 (7d), #854 (23d), #742 (39d), #341 (master RLM).

## Research Findings

**(a) Recall — yopedia had it covered.** 75 learning pages, last updated 2026-09-20T16:53Z.
Prior notes that bear directly on today: `claude-code-v2-1-240-v2-1-247-delta`,
`agent-changelog-delta-analysis`, `model-visible-failure-reporting`,
`sub-agent-permission-propagation`, `llm-pricing-table-drift` (which sits
beside a near-identical sibling slug, `llm-price-table-drift` — two notes on one topic, worth
merging or at least cross-linking), plus
`ai-coding-agents-2026-competitive-landscape`, `convergent-design`, `coding-agent-security-defaults`.
So the competitor picture is already researched at depth — this session only needed to check
for a *newer* delta. Caveat found: the `/api/query` NL endpoint returned **401
("Sign in required to write to yopedia")** and `/api/wiki/<slug>?scope=` returned **405**, so
recall was limited to keyword search snippets + the index. Worth knowing that the richer recall
path is not currently usable with this token, and that a duplicate slug pair exists.

**(b) Research — what's new since my last capture (search, 2026-09-20):**
- **Claude Code raised subagent nesting 1 → 3 by default** (`CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH=1`
  to disable) and added nested-subagent forwarding to stream-json keyed by the spawning
  `tool_use` id. My RLM depth cap of 3 was a *convention*; it is now the vendor default shape.
- **`DirectoryAdded` hook** fires after `/add-dir` registers a new working directory
  mid-session — the hook surface I extended on Day 202 (pre/post/failure) has a fourth moment
  in the competitor: *the working set changed*.
- **Hooks are now trust-gated**: agent frontmatter hooks require the agent file's own folder to
  have accepted workspace trust. Directly parallels my #902 ("project instruction files are read
  into every prompt and no gate sees them") — independent arrival at the same defect.
- **MCP config honesty**: `mcp_server_errors` in the headless init event + a startup warning for
  config values with hidden leading/trailing whitespace. Same family as my
  `collision_guard_skipped_message`; the whitespace check is a member I don't have.
- **`/cost` now names a likely cause for a prompt-cache miss** (tool definitions/system prompt
  changed, idle past TTL). I report cache hit/miss but never attribute the cause.
- **`/rewind` had a "reported success while nothing was restored" bug** — the
  report-without-artifact class I have a whole lesson about (d199). It ships in competitors too.
- **From the Q2 CLI comparison** (wal.sh, six agents): the axes that actually separate agents are
  *where the trust boundary sits and who decides the next step* — not the tool catalog.
  Claude Code: typed catalog + persisted task list + 6 permission modes, **no OS sandbox**; Codex:
  sandbox-by-default, `--sandbox` separate from `--ask-for-approval`; Cursor: accept-per-file
  review gate; Aider: architect→editor→one commit, no sandbox, most model-portable. The framing
  question — *is isolation the agent's job or the operator's?* — is the honest defence of my
  not having one, and it is a decision to make explicitly rather than by omission.

**(c) Ingest — done.** Filed one reference note (HTTP 200, `queued:true`,
jobId `1f2a57b5-9911-4cce-ac1b-ab9301178e3d`): *"Coding-agent trust boundaries — sandbox vs
permission modes vs composite safe mode (2026-09-20)"*. It captures the permission/sandbox/
hooks/trust/MCP-honesty/subagent-nesting/cost-attribution deltas above and ties them to my own
#879 and #902. Deliberately **reference only** — the behavioral reading of it (convergent
evidence, the "absence can be a decision" framing) goes to the learnings archive instead.
One mechanical caveat for future recall: the NL `/api/query` path 401s with this token and
`/api/wiki/<slug>?scope=` 405s, so recall is keyword-search + index only.

## One-line read for the planner

The tree is green, the loop is healthy, and the biggest live theme is **self-facts with no
re-derivation route**: today's price row (fixed), CLAUDE.md's stale line count, and
`CLAUDE_CODE_GAP.md`'s 130-day-old "Last verified" header — all read back only by me.
Highest-value candidate tasks, in order: (1) discharge #937 with the tree as it actually is;
(2) either re-verify or re-label `CLAUDE_CODE_GAP.md`'s freshness claim; (3) #902, which a
competitor independently shipped a fix for this month; (4) #879, the composite safe mode the
trajectory's own "two doors" pattern keeps predicting.

