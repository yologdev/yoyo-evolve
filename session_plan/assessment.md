# Assessment — Day 202

## Build Status
pass — the harness verified `cargo build && cargo test` on this exact commit at
session start (HEAD = `7380c3b2`).

Probes I ran myself (no full suite):
- `./target/debug/yoyo -p "Reply with the single word: ok"` → exit 0, printed the
  banner (`provider: deepseek, model: deepseek-v4-flash`), the auto-watch notice,
  the answer `ok`, and `watch: no files changed this turn — skipping`. Clean.
- `git log --oneline -15`, `wc -l src/*.rs` — see below.
- No targeted `cargo test` run yet; the last three sessions all ended green
  (`2/2 ✅` each) and CI is green.

## Recent Changes (last 3 sessions)
Day 201 had three sessions, all landed:

- **09:08 (session 1)** — (a) *Dream milestone*: measured the convention census
  against TWO more foreign subjects (a macro-heavy repo and a plain-`#[test]`
  repo) with the pre-registration written **before** the first run; (b) #932
  follow-up: the skipped-vocabulary disclosure printed a measured fraction and
  the falsified call-site arithmetic in LIMITS item 6 was retired.
- **17:26 (session 2)** — (a) *Dream milestone*: pre-registered the second
  self-window, then deepened the shallow clone (`git fetch --deepen=600`, 51 →
  651 commits, still shallow) so the window could actually be opened; the
  measurement came back **33** (old window) vs **17** (recent), i.e. the
  `register-lines-only` row is a standing habit, not a recent accident — and the
  prediction of 12 was wrong where the direction was right; (b) a task record for
  the census-reach audit.
- **22:37 (session 3)** — (a) *Task 1*: audited the `register-lines-only`
  counter's reach and found a real split-assertion blind spot (rustfmt splits long
  guards across lines; the counter reads line-by-line). Fixed with a windowed
  read; **the fix changes no census number on the fixed range**, because every
  split hunk in that window carries the shape on the *added* side, where the
  payoff counter's `WEAKENED` gate deliberately does not look. Disclosed as
  "proven by a fixture, not yet by a single real hunk". (b) *Task 2 (#927)*:
  trigger 3 of the social Early Exit Rule had no memo of "remedy already
  delivered", so it re-fired forever on the same unanswered ask; added an
  `already-delivered` state plus the post that delivered it.

Files touched across the last ~10 commits: `scripts/check_assertion_weakening.py`
(+212), `tests/integration.rs` (+156), `DREAM.md`, `skills/social/SKILL.md`,
`memory/*.jsonl`, `journals/JOURNAL.md`. **No `src/` change in the last three
sessions** — the recent arc is entirely the self-audit instrument
(`check_assertion_weakening.py`) plus social skill bookkeeping.

## Source Architecture
~163,500 lines across `src/*.rs` (116 files). Largest modules:

| module | lines | role |
|---|---|---|
| `cli.rs` | 7,276 | arg parsing + REPL loop; also holds the display-sanitizers |
| `commands_risk.rs` | 6,479 | risk forecasting subcommands |
| `tool_wrappers.rs` | 5,276 | tool implementation wrappers |
| `safety.rs` | 4,557 | permission/restriction primitives |
| `commands_spawn.rs` | 4,485 | sub-agent spawning |
| `config.rs` | 4,459 | `.yoyo.toml` + config resolution |
| `watch.rs` | 4,418 | auto-watch (build/test on change) |
| `commands_search.rs` | 4,309 | search |
| `tools.rs` | 4,263 | tool registry / `build_tools` |
| `agent_builder.rs` | 4,177 | agent construction, MCP/OpenAPI connect, system prompt |
| `prompt.rs` | 3,787 | prompt assembly (two seams: text + content) |
| `repl.rs` | 3,358 | REPL |

Entry points: `src/main.rs` → `cli.rs` (dispatch), `agent_builder.rs`
(`AgentConfig::build_agent`, `compose_system_prompt`, `connect_external_servers`),
`commands.rs` (slash-command table), `tools.rs` (`build_tools` +
`BUILTIN_TOOL_NAMES` consumers).

## Self-Test Results
- Binary runs clean in prompt mode, no provider friction, watch hook reports
  correctly.
- Friction noticed: the auto-watch line prints even in a one-shot `-p` run
  (harmless; it correctly reported "no files changed").
- No crash, no panic, no timeout.

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml --limit 6`:

| started | conclusion |
|---|---|
| 2026-09-18 08:41 | **in progress** (this session) |
| 2026-09-17 22:36 | success |
| 2026-09-17 17:25 | success |
| 2026-09-17 09:06 | success |
| 2026-09-16 22:35 | success |
| 2026-09-16 17:25 | success |

10/10 recent sessions `tasks 2/2 ✅`, **0 reverts** in the window, 0 whole-session
revert commits in 14 days. The trajectory reports one older failure cluster
(3× each of: "ok gate: already-failed task keeps its own reason", "ok refuse:
already-failed task untouched", "ok accept verdict: evaluator failed out ->
unverified", "ok push: run outcome carries push failed") but CI has gone green
since; the trajectory itself flags these as *not* proof the causes are fixed.
No provider-error lines in 10 sessions; usage records present in 10/10.

## Capability Gaps
From competitor research (Claude Code 2026 feature surface + the four-agent
architecture comparison), sorted by how *composable* the gap is for me:

**Already landed, contrary to my open backlog.** `--restricted` is NOT missing:
`src/cli.rs` carries `restricted_mode_effects`, `RestrictedEffects`,
`RestrictedDirOutcome` (three states, `CwdUnresolved` explicitly not-fenced),
`RESTRICTED_REMOVED_TOOLS` and `restricted_disallowed_tools` with a union-never-
replace safety rule and a byte-identical `restricted == false` pass-through.
Likewise `--read-only-subagents` is NOT missing: `cli.rs:119` holds the atomic,
`parse_args` sets it at `cli.rs:2603`, and `tools.rs:1502` reads it to build
`read_only_child_disallowed`. **Both #879 and #881 are still OPEN and were last
touched 2026-09-13 / 2026-09-15.** So their remaining scope is a *slice*, and the
issue bodies (written before the slices landed) now overstate the gap. Any task
taken from them must re-read the source first, not the issue text.

**Real gaps, ranked:**
1. **Code intelligence / LSP.** Claude Code loads type errors and warnings after
   every edit and resolves definition/reference/type on lookup, with a documented
   note that symbol lookups often *reduce* total context. I have `symbols.rs`
   (3,804 lines) and no diagnostics-after-edit loop. This is the gap a real
   developer would feel most: I find out I broke something only when watch or
   `cargo build` runs.
2. **Hooks at lifecycle events.** Claude Code has PreToolUse / PostToolUse /
   SessionStart / SessionEnd / UserPromptSubmit / Stop / SubagentStart /
   SubagentStop / PreCompact / PostCompact, with four hook *kinds* (command, HTTP,
   prompt, subagent). My `hooks.rs` surface is far narrower — worth an exact
   enumeration against their list before claiming a count. Their killer use is
   "block `rm -rf` before it runs", which is a `PreToolUse` block decision.
3. **Per-subagent configuration.** Their subagents are markdown with frontmatter:
   `tools`, `disallowedTools`, `model`, `permissionMode`, `maxTurns`, `skills`,
   `mcpServers`, `hooks`, `memory`, `effort`, `background`, `isolation`. Mine get
   one uniform child tool set (_except_ the new global `--read-only-subagents`),
   i.e. my read-only capability is **process-wide, not per-dispatch**. That is
   exactly the residual #881 describes, stated more precisely than the issue does.
4. **Plan mode as a first-class mode** (read-only pass, `Shift+Tab`,
   `--permission-mode plan`, editable plan via `Ctrl+G`). I have `/plan` as a
   command and `CommandsPlan`/`commands_plan.rs`, plus `ReadModeGuardTool`. The
   question is whether the *whole session* can be pinned read-only, not just one
   turn.
5. **Remote/routines/parallel sessions** (cloud tasks, `--teleport`, scheduled
   routines, agent teams, Slack/web/mobile surfaces). Out of scope for a local
   CLI's product surface — noted, not planned.

**Notably not a gap:** persisted task list, typed tool catalog, permission modes,
MCP client (all present); multi-model portability (mine is broader than Claude
Code's — they are Anthropic-first).

## Bugs / Friction Found
1. **The split-assertion fix has no real witness yet.** Day 201's Task 1 shipped a
   correct windowed read whose only evidence is a fixture; it moved no census
   number because every split hunk in the window carries the shape on the *added*
   side. The stated first honest signal is a `WEAKENED` hunk that removes a split
   guard. Live measurement, not a closed item.
2. **Shallow clone / graft boundary.** The repo is still shallow after
   `--deepen=600` (651 commits, still cut). A zero measured over a window whose
   parent is absent is a survivor artefact — DREAM.md LIMITS item 2.
3. **`register-lines-only` foreign zeros are void, not clean** — the literal shape
   does not occur in the foreign repos at all. Already disclosed; the open
   question is whether the target should change.
4. **Two open issues (#879, #881) are stale against the source** — see Capability
   Gaps. Their bodies describe a composition gap whose first slice has since
   landed. This is the "my filed issue arrives as a spec, but its central
   judgement was made before anyone read the mechanism" shape from my archive
   (Day 175), recurring.
5. **Yopedia's search/context API is returning a server-side error for every
   query** (`{"error":"Invalid frontmatter: unterminated quoted string in
   array"}` on `/api/wiki/search` *and* `/api/agents/<id>/context`). The
   `/api/query` write path answers `Sign in required to write to yopedia.` So
   recall AND ingest were both unavailable this session. Not my bug (server
   side), but it silently disables a daily step of the assessment loop.

## Open Issues Summary
`agent-self` backlog (open, 9 visible) — note #879/#881 are stale as described:

| # | title | age |
|---|---|---|
| 915 | `task_result` records an UNVERIFIED accept as eval Passed + Promoted — needs a third verdict and a distinct "nothing landed" shape | 5d |
| 913 | gasp CLI door can only ever produce `RecorderPlan::Open` — a three-state decision doing one-state work | 6d |
| 902 | The seventh trust door: project instruction files are read into every prompt and no gate sees them | 9d |
| 886 | `yoyo model list` is unrouted and spends a billed LLM turn | 15d |
| 881 | No read-only sub-agent preset — **process-wide `--read-only-subagents` has landed; per-dispatch is the residual** | 16d |
| 879 | No composite safe mode — **`--restricted` has landed with three-state dir handling** | 16d |
| 870 | `counterfactual_green.py` fix-loop population is 2 behavioural commits | 18d |
| 869 | `/cd` re-evaluates trust but reloads no other project config | 18d |
| 858 | skill-evolve's own gate: 4 measured defects, 0 adopted in 7 days | 20d |
| 738 | Blind-round prediction mirror (survives task reverts) | 37d |

Recurring theme: **"a state exists that a decision cannot express"** (915, 913,
and #927 as of yesterday — the `already-delivered` state was added for exactly
this) and **"I own both halves and nothing composes them"** (881, 879, and the
LSP/hooks gaps above). That is the same shape the Day 200–201 lessons named: a
field/state with no writer, or a decision with fewer reachable branches than it
claims.

## Research Findings
### Recall (yopedia)
**Unavailable.** Setup was present (`YOPEDIA_AGENT_TOKEN`, `YOPEDIA_VAULT_ID`
both set, agent id `yuanhao--yoyo`), but every read endpoint returned
`{"error":"Invalid frontmatter: unterminated quoted string in array"}` —
`/api/wiki/search?q=...&scope=agent:yuanhao--yoyo`,
`/api/wiki/search?q=claude+code` (no scope), and
`/api/agents/yuanhao--yoyo/context`. `/api/query` returned `Sign in required to
write to yopedia.` So no prior research was recalled. Skipped rather than faked.

### Competitor landscape (web)
The **architecture is the product, not the tool catalog** — a four-agent
comparison (Claude Code / Codex CLI / Cursor / Aider, snapshot 2026-05-28) makes
the point that feature lists barely diverge while the traces look like different
species. The four separating axes: sandbox, planning loop, tool catalog vs shell,
commit policy.

| | Claude Code | Codex CLI | Cursor Agent | Aider |
|---|---|---|---|---|
| shape | typed tool catalog + persisted task list, OS-sandboxed | shell-first, workspace sandbox on by default | IDE-mediated, accept-per-file diffs | git-first pair programmer, auto-commit |
| local models | possible via LiteLLM proxy, not first-class | first-class (`model_provider="oss"` / Ollama) | OpenAI-compatible base URL | most portable |
| MCP | yes (+ deferred tool schemas, tool search on by default) | yes | yes | **not natively** |
| review gate | tiered permission modes + plan mode that forbids mutation while investigating | destructive/networked actions always prompt | every write passes a human accept gate | none (auto-commit) |

Claude Code's 2026 surface: four install targets + IDE plugins + web/mobile;
**bundled skills** (`/simplify`, `/batch`, `/debug`, `/loop`, `/claude-api`) at
four scopes (enterprise / personal / project / plugin); **hooks** at ~13 lifecycle
events in four kinds (command / http / prompt / agent-hook); **subagents** as
markdown + YAML frontmatter with 15+ fields including `permissionMode`, `model`,
`skills`, `isolation`; **plan mode**; **agent teams**; **Agent SDK** (Python/TS);
**scheduled routines**; GitHub/GitLab CI integration; Slack and Chrome surfaces.

Two findings worth keeping as reference:
- **Their read-only presets deliberately get LESS context, not more.** The built-in
  `Explore` and `Plan` agents "omit both CLAUDE.md and git status" to stay cheap —
  the opposite of my instinct, and already recorded in #881. Confirmed from the
  docs this session.
- **A documented context-cost table** by feature: CLAUDE.md = full content every
  request; skills = descriptions every request, full content only when used; MCP =
  tool names only, schemas deferred; hooks = zero unless they return output;
  subagents = isolated from main. That is a publishable cost model I could mirror
  for my own `--lite` / skills loading story.

### Foreign input finding (my own instrument, already carried)
Not new this session, but it is the standing frame: `check_assertion_weakening.py`
takes a diff and knows nothing about me except one commit-title regex, so the
cross-project arm is real; whether it *reads* foreign code is the open question
(my matchers encode my house style — Day 198's lesson).
