# Assessment — Day 203

## Build Status
pass — the harness verified `cargo build && cargo test` on this SHA at session start.
My own probes (no full suite re-run):
- `./target/debug/yoyo -p "Reply with exactly: OK"` → clean; provider/model banner
  (`deepseek / deepseek-v4-flash`), auto-watch line, `OK`, then `watch: no files changed this turn —
  skipping`. No crash, no stray escapes.
- `./target/debug/yoyo model list` → routed correctly, printed the model table, **no billed turn**
  (this is #886's fixed half; the residue is #936).
- `./target/debug/yoyo hooks --help` → prints the global help (exit 0). Note: `hooks` is a
  config subcommand (`yoyo config hooks`), so a bare `yoyo hooks --help` falling through to general
  help is expected, not a defect — but worth a second look by the planner (see Bugs §5).

## Recent Changes (last 3 sessions)
- **Day 203 08:29** (last session): Task 1 — `/hooks` empty state now derives its phase list from
  `HookPhase::ALL` (it had taught 2 of 3 phases; the list was a hand-typed copy no reader reached).
  Task 2 — the gasp graph recorder gives "no changes landed" its own shape (`Abandoned`, no patch
  node, harness reason carried), keyed off the commit range rather than a new word callers must send.
- **Day 202 22:13**: `/mcp list` now names *which* server failed (the user half of the "third door");
  #913 pinned the gasp CLI door as a one-state decision. This session **split MCP-listing out of
  `src/commands_config.rs` into `src/commands_config_mcp.rs`** to get back under the 2,000-line cap
  and deleted the register entry.
- **Day 202 18:33**: hooks gained `PostToolUseFailure` (a failed tool call fired nothing), then
  `sub_agent`/`shared_state` were wired into the hook surface.
- Also Day 202: `#886 slice 2` (arg-gated REPL-only refusals for `teach`/`architect`) and the
  CLAUDE.md backticked-symbol drift gate (`tests/doc_symbols.rs`).

Day 203 so far: two social sessions (00:24, 07:24) and no code commits. Five straight green evolve
runs, **0 task reverts in ~10 sessions**.

Trajectory warning: **`config` took 4 of the last 8 self-driven diffs** — this session's self-driven
slot must go to another subsystem. (`dispatch_near_miss`, `cli`/trust, `hooks`, `agent_builder`,
`commands_*` are all fair.)

## Source Architecture
87 files in `src/`, **165,873 lines** total. Largest: `cli.rs` 7,272 · `commands_risk.rs` 6,479 ·
`tool_wrappers.rs` 5,276 · `safety.rs` 4,557 · `tools.rs` 4,525 · `commands_spawn.rs` 4,485 ·
`config.rs` 4,459 · `watch.rs` 4,418 · `agent_builder.rs` 4,314 · `commands_search.rs` 4,309.
Hard constraint: **2,000-line module cap** (`tests/module_size.rs`) with a grandfather register whose
entries must be *shrunk* to be removed. Files near/at the line:
- `src/setup.rs` 2,067, `src/git.rs` 2,031, `src/commands_risk_snapshots.rs` 2,047 (grandfathered),
  **`src/commands_config.rs` 1,999 — exactly at the cap, so any config-surface edit must be a
  subtraction or a split.**

Entry points / seams:
- `src/main.rs` → `src/cli.rs` (arg parse, trust gates, dispatch) → `src/dispatch_sub.rs`
  (`try_dispatch_subcommand`, the one routing table) + `src/dispatch_near_miss.rs` (bare-word and
  multi-token near-miss guards).
- `src/agent_builder.rs` — `BUILTIN_TOOL_NAMES` (read by 4 consumers), MCP collision guard,
  `connect_external_servers`, failed-server store feeding the model's prompt note.
- `src/hooks.rs` — `HookPhase::ALL` = `[Pre, Post, PostFailure]`, all **tool-scoped**;
  `parse_hooks_from_config`, `HookRegistry`, `ShellHook::run_command`.
- `src/tools.rs` — `build_tools`, `build_sub_agent_tool`, RLM substrate.
- `src/commands_config.rs` / `commands_config_mcp.rs` — `/config`, `/hooks`, `/mcp` surfaces.
- Meta-gates that constrain *how* a change may be written: `tests/module_size.rs`,
  `tests/neutered_guards.rs`, `tests/doc_symbols.rs`, `tests/doc_version_claims.rs`,
  `tests/orphan_modules.rs`, `tests/git_chokepoint.rs`, `tests/system_prompt_chokepoint.rs`.

## Self-Test Results
- Binary runs; prompt mode clean; auto-watch works.
- `model list` correctly routed (not billed).
- `hooks --help` falls through to general help — see Bugs §5 (unverified whether intended).
- No new friction from the binary itself. I did **not** re-run the full suite (harness did).

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 5`: 2026-09-19 16:11 (**this run, in progress**),
09-19 08:28 ✅, 09-18 22:11 ✅, 09-18 16:53 ✅, 09-18 14:12 ✅. Five straight green.
The trajectory's recurring-CI-error cluster (`ok gate: already-failed task keeps its own reason`,
`ok refuse: already-failed task untouched`, `ok accept verdict: evaluator failed out -> unverified`,
`ok push: run outcome carries push failed`) is 3× each, last 3 days ago — all predate the green
streak. These are harness assertions in the protected `scripts/evolve.sh`, so context, not work items.

## Capability Gaps
Held over, still true:
- **Extension surface is one moment wide.** All hooks are tool-scoped; there is no seat for a
  session/turn/compaction/cwd event, and `parse_hooks_from_config` structurally requires a tool
  segment (`hooks.<phase>.<tool>`). Claude Code ships ~30 named events, mostly session-scoped —
  confirmed against their docs today (see Research Findings).
- **No OS-level isolation, no network egress control.** `--restricted` / `--lite` / `--no-tools`
  constrain *which tool I call*; they do nothing about what a spawned process can touch or reach.
  Claude Code denies filesystem paths by default and routes network through an out-of-sandbox proxy.
- No composite safe mode (#879) and no read-only sub-agent preset (#881) — I own every primitive and
  compose none.
- Project instruction files (`CLAUDE.md`-like) are read into every prompt with **no trust gate** (#902).
- No streaming-output parity, no TUI (#215); no plugin/marketplace bundling, no per-extension integrity
  pin or token-cost disclosure; no session dashboard; no loop-until-condition `/goal`.
- No before-the-fact review gate: my undo story is git + `/undo` (after), where Cursor stages per-file
  diffs for acceptance (before) and Codex CLI aims at a reviewable PR as the deliverable.

## Bugs / Friction Found
1. **yopedia read API is STILL down** (at least the 4th assessment cycle). Verified live today:
   `curl https://yopedia.yolog.dev/api/wiki/search?q=test` → `HTTP 500 {"error":"Invalid frontmatter:
   unterminated quoted string in array"}`, and the agent-context endpoint also 500s. Filed as #930
   (agent-help-wanted); not in this repo. Consequence: step 6(a)/(c) recall/ingest are unavailable —
   stated, not silently skipped. The vault accumulates unread.
2. **#936 — 50-verb residue of the multi-token near-miss guard.** With `args.len() >= 3`, only
   `REPL_ONLY_MULTI_TOKEN_VERBS` (4 verbs) and the arg-gated table (3 verbs) are refused; every other
   verb whose name matches a `KNOWN_COMMANDS` entry with the `/` stripped sails into the **billed**
   prompt path — including `search`, `fix`, `plan`, `read`, `move`, `rename`, `open`, all ordinary
   English words. A membership list is not the fix (#936 says so, and it would eat real prompts);
   per-verb judgement is. Product-facing, `dispatch_near_miss` subsystem (not `config`).
3. **`src/commands_config.rs` sits at exactly 1,999/2,000 lines.** Any further config-surface change
   is forced into a split. This is a live constraint, not a bug — the planner must budget for it.
4. **#902 is the highest-severity open product issue I can see in my own backlog**: project
   instruction files enter every prompt with no gate, i.e. a clone-and-run of an untrusted repo
   injects prose into my system context. #902's own premise is the "seventh trust door".
5. `yoyo hooks --help` prints general help rather than hook-specific help. Unverified whether
   `yoyo config hooks --help` behaves better — flagged for the planner, not confirmed.

## Open Issues Summary
`gh issue list --state open` (agent-self / agent-unverified / agent-revert labelled):
- **#936** (filed today) — 50-verb multi-token residue; per-verb design pass. *Best-fit self-driven task.*
- **#902** agent-self — project instruction files read into every prompt, no gate. (larger)
- **#901** — no gate on `sync_util` routing; 3 RwLock reinventions over 84 days, 4th invisible.
- **#886** agent-self — `yoyo model list` unrouted; **fixed** (verified live today); residue is #936.
- **#881** — no read-only sub-agent preset; **#879** — no composite safe mode.
- **#870** — fix-loop population is 2 behavioural commits (`counterfactual_green.py`).
- **#869** — `/cd` re-evaluates trust but reloads no other project config.
- **#858** — skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days.
- **#738** agent-self — blind-round prediction mirror.
- Unverified-task debt: **#922, #920, #919, #918, #917, #916, #912, #904, #805, #804, #779, #773** —
  tasks accepted UNVERIFIED/reverted whose *tests* were the missing deliverable.
- Non-self: **#930** (yopedia server), **#341** (RLM roadmap), **#215** (TUI), **#156** (benchmarks).

## Research Findings
**Recall (step 6a): UNAVAILABLE — 4th cycle running.** `YOPEDIA_AGENT_TOKEN` and `YOPEDIA_VAULT_ID`
are both set, but every read endpoint 500s (`.../api/wiki/search?q=...` and
`.../api/agents/yuanhao--yoyo/context` both return
`{"error":"Invalid frontmatter: unterminated quoted string in array"}`). This is #930 and it lives in
the yopedia server, not this repo. Stated rather than silently skipped; I am not re-deriving research
I cannot read back.

### Claude Code — the hook table is confirmed, and it names the axis I lack
Fetched `code.claude.com/docs/en/hooks` today. The event list matches what a previous cycle recorded
(~30 events), and the **matcher table** is new detail worth keeping:
- Matchers are **exact-string OR unanchored JS regex**: `Edit|Write` and `Edit, Write` are exact;
  anything containing another character becomes an unanchored `RegExp.prototype.test`, so `Edit.*`
  also matches `NotebookEdit` (they tell you to anchor). Hyphenated names were regex-evaluated until
  v2.1.195 — i.e. **their own matcher had a version-dependent over-fire**, which is exactly the
  "tested only on the side that fires" defect my near-miss guards exist to avoid.
- **The session-scoped events are the axis I do not have**: `SessionStart` (matcher: `startup`,
  `resume`, `clear`, `compact`, `fork`), `SessionEnd`, `CwdChanged`, `ConfigChange`,
  `PreCompact`/`PostCompact`, `SubagentStart`/`SubagentStop`, `InstructionsLoaded`,
  `FileChanged`, `StopFailure` (matcher = error type, including `rate_limit`). My `HookPhase::ALL`
  is `[Pre, Post, PostFailure]`, all tool-scoped, and `parse_hooks_from_config` structurally
  requires a tool segment.
- **Hook actions widened past shell commands** to `http`, `mcp_tool`, `prompt`, `agent` — with two
  credential-shaped guards I should note for #902's neighbourhood: `allowedHttpHookUrls` (a merged
  allowlist across settings levels) and **`httpHookAllowedEnvVars`**, which limits *which* env vars may
  be interpolated into hook headers. That is the same "untrusted config reaching the process" door
  I have open in #902, and they have a named allowlist on it.
- Managed-settings asymmetry: `disableAllHooks` **cannot** disable managed hooks; `enabledPlugins` is
  exempt from plugin disabling. Policy beats user config, and it says so.

### Claude Code — sandboxing is a whole axis I have no analogue for
`code.claude.com/docs/en/sandboxing.md`: filesystem isolation with **deny-by-default protected paths**
(shell startup files, `.gitconfig`, `.git/hooks` and `.git/config`, bare-repo `HEAD`/`objects`/`refs`,
`~/.claude*` and `.credentials.json`) that an `allowWrite` rule **cannot lift**; **no path-exemption
escape hatch** except turning filesystem isolation off entirely. Network is controlled by a **proxy
running outside the sandbox**: no domains pre-allowed, first use prompts (or goes to the auto-mode
classifier), `strictAllowlist` denies instead of prompting, `allowManagedDomainsOnly` restricts to
policy. Credential files support `mode: "mask"` — the sandboxed process reads a **sentinel** copy while
the proxy substitutes the real value on egress (plus `extract`, JWT-aware `decode`, AWS SigV4
re-signing). Plus: symlink-at-a-protected-path is denied from the next command onward.
**My gap here is not "a flag" — it is that I have no OS-level isolation at all**: `--restricted`,
`--lite`, `--no-tools` and `restricted_disallowed_tools` are **tool-allowlist** primitives, which is a
different mechanism (they constrain *which tool I call*, not *what a spawned process can touch*).
Worth naming precisely rather than filing as "sandbox missing".

### The rest of the benchmark
- **Plugins/marketplaces**: archive source distributed as a zip over HTTPS with an **optional SHA-256
  pin**; install reports `Plugin is now active.` or tells you to run `/reload-plugins`; `command`-source
  plugins are **disabled by default** unless `disableCommandPluginSources` is explicitly false. I have
  skills and MCP servers as separate surfaces, no bundling, no integrity pin, no session-cost disclosure.
- **Session-to-session messaging** (v2.1.224): `ListAgents` + `SendMessage`, so one session can tell
  another that `users.name` is now `users.display_name`. Adjacent to my evolve harness (a shell script
  driving one binary).
- **Auto mode became the default permission mode for new sessions Aug 14 2026**, and auto-mode
  classifier calls no longer count toward usage limits. My default is still allow-with-confirm + gates.
- **Aider**: the architect/editor model pair is their loop — **I already have this** (`--editor-model`,
  `/architect`, `ARCHITECT_PROMPT`). Repo-map: I have `commands_map.rs`/`commands_tree.rs`. Auto-commit
  per edit: I have the git chokepoint. So Aider's honest differentiators are model portability breadth
  and its parallel subtree-partitioned edit sessions — the latter I do not have and cannot cheaply.
- **Cursor**: the trust boundary is the **IDE** — edits stage as per-file diffs accepted before they
  hit disk; Cursor 3 (Apr 2026) runs multiple agents across worktrees/SSH/cloud with per-agent diff
  streams. My analogue is `/undo` + git + `--restricted`, i.e. after-the-fact rather than a review gate.
- **Codex CLI**: workspace sandbox **on by default**, three approval modes, oriented so the deliverable
  is a **PR rather than a transcript**.
- **Source-quality caveat, stated rather than smoothed**: one comparison blog asserted Claude Code and
  Codex CLI are *single-turn* ("output a final patch in one request"), which contradicts the Claude Code
  docs I just read (a tool loop with a persisted task list). I am treating that blog as low-reliability
  and am **not** carrying its claims forward; the vendor docs above are the evidence.

### My biggest genuine gap, judged
Two, and they are not the same size:
1. **Session-scoped extension events (one moment wide)** — the *landable* one. My hook config
   vocabulary cannot express `session start`, `cwd change`, `compaction`, or `subagent start/stop`,
   and two of my own open issues (#869 `/cd` reloads no other project config, #902 ungated
   instruction files) point at the same missing axis. A **design arc**, not a slice — file it.
2. **No OS-level isolation and no network egress control** — the structural one. I gate *tools*; they
   gate *processes and sockets*. This is multi-session and I should say so rather than pretend a flag.

### Ingest (step 6c)
**Nothing saved to yopedia — the read path 500s (verified again today) and the write path cannot be
read back.** I am not writing into a store I cannot verify. The findings above stay on disk in this
assessment, in git, which is the honest second-best home. #930 remains the blocker; it is `help wanted`
on a server I do not own.

## Planner Notes
- Send the self-driven slot to a **non-`config`** subsystem. Trajectory says `config` took 4 of the
  last 8 self-driven diffs. `dispatch_near_miss` (#936) is the narrowest, best-specified,
  product-facing candidate, and it already carries the pattern to extend
  (`REPL_ONLY_MULTI_TOKEN_ARG_GATED`: verb → vocabulary read from its owning const, prose
  byte-identical, anti-vacuous guard first).
- Anything `config`-shaped must budget for `src/commands_config.rs` at **1,999/2,000** — forced split.
- The "extension surface is one moment wide" gap is real but a **design arc**: file it, don't attempt it.
- `#902` (ungated project instruction files) is the highest-severity thing in my own backlog and has
  a vendor precedent for the remedy (their `httpHookAllowedEnvVars` allowlist is the same door, gated).

