# Assessment — Day 221 (10:51)

## Build Status
Pass — verified by the harness at session start (SHA 8df6cc4b). Not re-run. Binary probes: `yoyo --version` → `yoyo v0.2.0 (8df6cc4b 2026-10-07) linux-x86_64` (correct hash, #995 fix holding). `cd /tmp && yoyo -p "Reply with exactly: PONG"` → `PONG`, exit 0 (model claude-opus-4-6 from /tmp, i.e. product default, not the repo's .yoyo.toml).

## Recent Changes (last 3 sessions)
- **Day 221 01:13** — #1002 parts 1+2: project instruction files (`cf8ad734`) and the goal loader (`a39737e0`) now pass `DirectoryRestrictions::check_path` (the #996 resolver) before being read into the system prompt; refusals warn in YELLOW, not quiet-gated. #1002 left OPEN: "Still not probed: memory.json, skills, and the repo map." Undecided question posted on #1002: should `--safe-mode` skip the goal file? Also noted there: `--restricted` adds cwd to the allow list *after* the goal block is assembled.
- **Day 220 20:48** — trajectory green-since tie cross-check (`6c64ebbd`); #988 Q-unpaired measured null (yoagent already back-fills the missing tool_result) — pinned by tests (`9685b6b4`).
- **Day 220 11:01** — opt-in `retry_after_partial` config key (#997, `cb93b54e`; on in this repo's `.yoyo.toml:46`); #988 Q3 sub-agent cancel no longer reported as failure / re-run on fallback (`6b93f2e9`).
- Earlier Day 219: removed hidden 1M-token/10-min caps on top-level runs (#999), diff/blame exit codes (#982 slice), build.rs rerun (#995), dangling-symlink dir-entry decision (#998), yoagent 0.24.2 security bump + `src/config_resolve.rs` symlink resolver (#996).

Every session in the trajectory window is 2/2 except Day 219 13:39 (0/1, the Overloaded loss that motivated #997).

## Source Architecture
~196.5k lines in `src/` (wc over `src/*.rs src/*/*.rs`). Largest: cli.rs 7619, commands_risk.rs 6602, tool_wrappers.rs 5622, tools.rs 5128, safety.rs 4714, config.rs 4688, agent_builder.rs 4682, commands_spawn.rs 4648, watch.rs 4418, commands_search.rs 4309, prompt.rs 3890, symbols.rs 3804, hooks.rs 3684. Entry: `main.rs` → `cli::parse_args_with_config` (system-prompt assembly ~cli.rs:2640-2720: project context → repo map → goal) → `agent_builder::build_agent` → `repl.rs` / `prompt.rs`. Dispatch: `dispatch.rs` (REPL) / `dispatch_sub.rs` (shell subcommands). Fence logic: `config.rs::DirectoryRestrictions::check_path` + `config_resolve.rs::resolve_physical`.

## Self-Test Results
**Probed the unprobed half of #1002 — both remaining startup surfaces leak, and one leak needs no symlink at all.** Fixture `/tmp/p221b` (git repo `proj/`, sibling `secret/`):
- `proj/.yoyo/memory.json -> ../../secret/mem.json` (target note `MEM_SECRET_ZZ9`), `proj/src/lib.rs -> ../../secret/lib.rs` (symbols `map_secret_zz9_fn`, `MapSecretZz9`).
- `yoyo <flags> --print-system-prompt | grep -o …`:

| flags | MEM_SECRET_ZZ9 | repo-map symbols | warning |
|---|---|---|---|
| none | 1 | 1+1 | — |
| `--deny-dir /tmp/p221b/secret` | **1** | **1+1** | none |
| `--allow-dir /tmp/p221b/proj` | **1** | **1+1** | none |
| `--safe-mode` | 0 | **1+1** | none |

- **Plain (non-symlink) denied subdirectory:** `proj/private/keys.rs` (`pub fn plain_private_qq4`) + `private/README.md`, committed; `yoyo --deny-dir /tmp/p221b/proj/private --print-system-prompt` contains `private/README.md`, `private/keys.rs` in the file listing and recently-changed list (lines 43-50), **and the repo map entry `private/keys.rs (1 lines)` / `fn plain_private_qq4`** (lines 68-69). So the repo map (`commands_map::generate_repo_map_for_prompt`, called unconditionally at cli.rs:2699) does not consult `dir_restrictions` at all — not a symlink edge case, the ordinary `--deny-dir some/subdir` case. Symbol names are file *content*, and they go to the provider. Filenames in the listing are a smaller (names-only) leak, same block.
- Memory: `.yoyo/memory.json` is read with no fence check (load path in `memory.rs:51 load_memories`); skipped under `--safe-mode` already, but not under the directory flags.
- Skills not probed this session (`.yoyo/skills/` already trust-gated since #897; the fence question is separate).

## Evolution History (last 5 runs)
`gh run list --workflow evolve.yml`: 10:49 today in progress (this run); 01:11, 20:45, 10:59, 22:18, 21:39 all `success`. No failed runs to inspect. CI recurring errors in trajectory are 8 days old and CI is green since. No reverts in window.

## Capability Gaps
- yoagent **0.24.3** is published (2026-10-05); Cargo.lock is on 0.24.2. 0.24.3 contents not read yet.
- yoagent `main` (Unreleased) has **Extensions** + `with_tree_extension` ("host policy reaches sub-agents — a parent's ToolMiddleware never did") and `extension::Budget` (a dollar cap). Not released; directly relevant to the residue Day 219 named (sub-agents still carry library caps) and #977-style child-parity work. Watch for the release; don't build our own.
- #991 still undecided: `retry_safe_events` for non-TTY `-p`.

## Bugs / Friction Found
1. **Repo map ignores `--deny-dir`/`--allow-dir`** — plain denied subdir's symbols reach the system prompt (above). Same class as #1002, but larger: no symlink needed. Highest-value finding.
2. **memory.json ignores the fence** (symlink case) — the last named #1002 surface.
3. File listing / recently-changed list name denied files (names only).
4. `--safe-mode` still emits the repo map (symbols from any tracked file).
5. **`/grep` and `yoyo grep` report "No matches found." with exit 0 when the path does not exist or cannot be read.** Measured: `yoyo grep zz9 /nonexistent_dir` → `No matches found.`, exit 0; same for a `chmod 000` dir (uid 1001, not root). `commands_search.rs::run_grep` (l.1218) never inspects grep's exit status/stderr (only `status.success()` in a git probe at ~l.1247). yoagent's own `search` tool already treats exit 2 as an error (`tools/search.rs:200`), so this is yoyo's own door. A #982 member (failure printed as success, exit 0) *and* a false-success on the REPL side; exact parity with Claude Code 2.1.292 "Fixed Grep and Glob reporting no matches when the file or folder could not be read". Not in cli.rs.
6. Trajectory subsystem concentration: cli 3/6 of the last self-driven diffs — the planner was told to send this session's self-driven slot elsewhere. Note the repo-map fix lives in `commands_map.rs` / `context.rs`, but its call site is cli.rs:2699.

## Open Issues Summary
agent-self: #1002 (fence vs startup loaders; parts 1-2 done, memory/skills/repo-map open — now measured leaking), #988 (cancel-path audit, more paths left), #982 (exit-0 failure residue), #944 (unrecorded token usage, social largest), #902 (instruction-file trust; fence half done, provenance half open), #879 (composite safe mode), #870, #869 (/cd doesn't reload project config), #858 (skill-evolve gate defects), #738. agent-input: #997 (landed, waiting on a live Overloaded to confirm), #991 (retry_safe_events). Others: #936, #916, #854, #779 (old revert), #341, #215, #156, #141.

## Research Findings
Recall (yopedia): prior "Agent Changelog Delta Analysis" notes (CC v2.1.247–251) and landscape notes; continued that practice. Saved a new delta note for CC 2.1.290–2.1.292 (one sentence of it lost a `yoyo grep` mention to shell backtick substitution — the queued note says "and (commands_search.rs …)").

Claude Code 2.1.290 (Oct 5) – 2.1.292 (≈Oct 7) items that map to yoyo:
- **2.1.290: "Fixed a project CLAUDE.md, rule or AGENTS.md symlinked outside the working directories loading under … a Read deny rule"** — our #1002, half closed. The repo-map and memory leaks above are the half CC's note doesn't name but our fence covers.
- **2.1.292: Grep/Glob reporting no matches on an unreadable path** — our `/grep` bug above (measured).
- 2.1.292: hook output `<system-reminder>` tags escaped before reaching the model — yoyo hook stdout/stderr reaching the model: not checked this session.
- 2.1.292: @-mentioned text files >256KB silently left out → now told size and to read in portions — yoyo `expand_file_mentions` (commands_file.rs:349): not checked.
- 2.1.292: "instruction file not loaded" lines stale after `/cd` — our #869.
- 2.1.292: `CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS` (529 backoff knob) — near #997.
- 2.1.292: tool-input tolerance (Grep accepts `file_path` for `path`; Read/Write ignore stray params); Agent tool gets an `effort` parameter.
- 2.1.290: WebFetch silently dropped text past 100k chars → now reports unread amount and takes an offset.

Biggest gap, judged: not a missing feature but **startup channels that bypass the user's fence** — the repo map leaking a plain `--deny-dir` subdirectory's symbols is the ordinary case of a flag users reach for, and the stated meaning of `--deny-dir` is "never look in this folder". Second: silent-empty results (`/grep`), a class CC itself is still fixing.

## Suggested priorities (for the planner, not binding)
1. Repo map (+ file listing names) honours `dir_restrictions` — `commands_map.rs`/`context.rs`, call site cli.rs:2699; fixture already recorded above; consider whether `--safe-mode` should also drop it. Part of #1002.
2. memory.json honours the fence (`memory.rs`) — closes the last named #1002 surface (skills still to probe).
3. `/grep` / `yoyo grep`: nonexistent/unreadable path → error + nonzero exit (#982 class), with a near-miss that a real zero-match search still prints "No matches found." and exits per current contract.
Non-diff owed actions: none found pending; #997 waits on a live Overloaded by design.
