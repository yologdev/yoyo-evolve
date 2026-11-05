# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A self-evolving coding agent CLI built on [yoagent](https://github.com/yologdev/yoagent). The agent spans multiple Rust source files under `src/`. A GitHub Actions cron job (`scripts/evolve.sh`) runs the agent on a schedule using a 3-phase pipeline (plan → implement → respond), which reads its own source, picks improvements, implements them, and commits — if tests pass. **The cadence and the model are parameters, not facts about this project, and they are deliberately not written down here.** The schedule lives in `.github/workflows/evolve.yml` (`schedule.cron`) and the provider, model and their knobs in `.yoyo.toml` only — every script and workflow reads the model from there (a `MODEL` env var overrides one run; there is no `MODEL` secret, and `tests/harness_logic.sh` fails if a model id is hardcoded anywhere else, because provider in one place and model in another is how Days 210-211 sent a DeepSeek id to Anthropic). Read those rather than this file, because a value copied into prose goes stale silently while the config keeps working. Two things about that config are *not* derivable from reading it, so they are recorded here instead:

- **The provider-key line in every workflow is load-bearing, not tidiness.** yoyo's key lookup falls back to `ANTHROPIC_API_KEY` and then `API_KEY` when the configured provider's own variable is unset, so a workflow that forgets to export the provider's key does not fail loudly — it sends the wrong provider's token and gets a 401. Every workflow that runs yoyo must export the key for whichever provider `.yoyo.toml` names.
- **`thinking = "high"` is still the top rung yoyo sends, but now by yoyo's choice, not yoagent's limit.** Superseded claim (yoagent 0.24.0 upgrade, Day 218, #987), recorded rather than erased: this bullet said yoagent 0.18.1 had nothing above `High`. Since 0.19 `ThinkingLevel` has `XHigh` and `Max` (`#[non_exhaustive]`), and on OpenAI-compatible providers `Max` can go out as `max`. yoyo itself still folds `max` to `High` in `src/cli.rs`/`src/config.rs`; sending a higher tier is a priced decision that has not been made <!-- yoagent-version-claim: 0.26 -->.

### Changing the model

The loop's provider and model are the two top-level keys `provider` and `model` in `.yoyo.toml`, and nowhere else: every script reads `model` from there, the workflows set no `MODEL`, and there is no `MODEL` secret. A change is one reviewed commit:

1. **Edit `provider` and `model` together in `.yoyo.toml`**, as top-level keys above any `[section]` (the scripts stop reading at the first section). Never change one without checking the other; a model id sent to the wrong provider fails on every call (Days 210-211: 404s).
2. **Check the knobs in the same file against the new model's documented limits.** `max_tokens` must not exceed the model's output maximum, or every call is a 400 (131072 against a 128000 cap, 2026-09-27). Remember that an OpenAI-compatible arm without its own preset gets `max_tokens = 4096` and a 128K window from the base config, which silently caps reasoning plus answer. `context_window` must fit the model's window with margin (see the comment on that key). `thinking` must be a level the model accepts.
3. **If the provider changes, export its API key** in every workflow under `.github/workflows/` that runs yoyo (today: evolve, social, dream, skill-evolve and synthesize). See the provider-key bullet above for why a missing key fails as a 401 rather than loudly.
4. **Know what may lag a brand-new id:** yoyo's known-model list and `/cost` pricing (a warning, and possibly wrong cost figures, not a failure). Nothing else needs touching: even the commitment scanner makes its call through yoyo, so it follows the same provider, model and credentials.
5. **Verify before relying on the cron:** `bash tests/harness_logic.sh` (fails on any hardcoded model id and checks both keys are present), `cargo test` (some tests read the repo's own `.yoyo.toml`), and `yoyo config get model`. Then push and run `gh workflow run evolve.yml`. The log prints `model: <id>` in plain text. A bad id or bad knob stops the session at assessment with `API error in assessment agent`, and a green run that commits nothing is the signal to read the log.

To try a model for one run without committing, set `MODEL` in the environment of a local run; nothing in CI sets it.

Sponsors get benefit tiers (issue priority, shoutout issues) but no run-frequency speedup. Every sponsor, any amount, is listed in README.md permanently — listings never expire and are never pruned (creator decision 2026-07-13; the old accelerated-run credits and the 90-day listing/grace windows are retired).

**Sponsor benefit tiers:**

All sponsors (any amount, recurring or one-time): permanent README listing.

Monthly recurring:
- $5/mo: Issue priority (💖)
- $10/mo: Priority + shoutout issue

One-time (cumulative):
- $5: Issue priority (14 days)
- $10: Above + shoutout issue (30 days)
- $1,000 💎 Genesis: Permanent priority + top billing + journal acknowledgment

## Build & Test Commands

```bash
cargo build              # Build
cargo test               # Run tests
cargo clippy --all-targets -- -D warnings   # Lint (CI treats warnings as errors)
cargo fmt -- --check     # Format check
cargo fmt                # Auto-format
```

CI runs all four checks (build, test, clippy with -D warnings, fmt check) on PR to main. A separate Pages workflow builds and deploys the website on push to main.

**Pre-push hook** (`.githooks/pre-push`, tracked, opt-in per clone — `git config core.hooksPath .githooks`): lints the **tip commit of each pushed ref** with `scripts/lint_evolve_heredocs.py` and refuses the push if it fails. The rule: no apostrophe inside a `${VAR:+...}` / `${VAR:-...}` block in `scripts/evolve.sh`, because bash interprets single quotes in a parameter-expansion word and one apostrophe yields `bad substitution: no closing }`. `evolve.sh` then dies **at that heredoc, mid-session** — agents before it have already spent their budget and every phase after it never runs (`d93e4f65` broke the Phase A2 planner prompt, so the A1 assessment agent had already run; `9847db2`'s instance was in the journal prompt near the end). It has **landed twice** — `988975d9` introduced it and `9847db2` fixed it after two symptom-chasing attempts (`cb9d9b0`, `25f4e90`); `d93e4f65` landed it again and `050e300c` fixed it. `scripts/lint_evolve_heredocs.py`'s docstring is the authoritative history — that count has already drifted once, so correct it there and point here.

Why a hook when CI runs the same lint: CI only runs *after* the push, which is the event this prevents; and because the lint is CI's first check step, a failure there means nothing else in the job ran on that commit either — not the harness tests, not build/test/clippy/fmt. (On a branch that is neither `main` nor a PR to `main`, CI does not run at all.) `bash -n` does **not** catch it — the expansion only fails at runtime. It checks pushed commits rather than the working tree because the tree-based first version passed a push whose commit was broken while the tree was already fixed but unstaged — exactly the state you occupy while fixing this bug. Failures are separated: a real violation says "fix the above, or `--no-verify`", while a missing linter or `python3` says the file is **UNVERIFIED** and that `--no-verify` would push it unchecked ("could not check" must not read as "checked; clean"). **Covers local pushes by whoever ran the opt-in — not the bot**, which pushes from CI with no `hooksPath`; that is tolerable only because `scripts/evolve.sh` is a protected file the agent cannot modify, with CI as the backstop. Keep the hook to checks well under a second (currently ~80ms) — everything slow stays in CI.

**The eleven invariant gates and the per-file notes now live in [ARCHITECTURE.md](ARCHITECTURE.md), not here.** They were moved out on 2026-09-15 for a measured reason: this file is appended to the system prompt of **every** invocation of every loop (`load_project_context`, `src/cli.rs`), and it had reached **1.4 MB — about 352K tokens — of which 97.6% was accumulated per-file history**. That crowded the prompt of every session, and it is the measured cause of the Day-199 DeepSeek session that compacted on its first turn and lost its own task (no assessment, 0 planned tasks, an evaluator that never wrote its verdict file — #925). Nothing was deleted: the move was contiguous and byte-preserving.

**Read ARCHITECTURE.md's entry for any file you are about to change.** It carries the gate rules and remedies, the superseded-claim record, and the per-file history — and it is the only place that has them. A gate you trip without knowing it existed is exactly what that file prevents, so opening it is part of touching a file, not optional.

**New history goes in ARCHITECTURE.md, never here.** Keep CLAUDE.md to what every session needs *before* it knows which file it is touching: what this project is, how to build and test it, the safety rules, the two audiences, and the MCP/yoagent/RLM notes. If you find yourself appending a measurement, a positive control, a census or a superseded claim to this file, it belongs in ARCHITECTURE.md under the file it is about. This file growing back past ~40 KB is itself the defect.

## RLM substrate

yoyo has shared-state recursive sub-agent dispatch — the [Recursive Language Model](https://alexzhang13.github.io/blog/2025/rlm/) pattern, scaled down to one yoagent primitive plus skill-level conventions. The substrate is in place; specific skills opt into it.

**What's available:**
- `build_sub_agent_tool` in `src/tools.rs` returns `(SubAgentTool, SharedState)`. Parent agents get a handle to pre-populate; sub-agents automatically receive a `shared_state` tool that reads/writes the same yoagent::SharedState key-value store. (Skills opt into this by adding `sub_agent` and `shared_state` to their `tools:` frontmatter.)
- Artifacts are stored once and read by reference rather than re-pasted into every sub-agent prompt. Namespace convention: `<skill>.<key>` (e.g., `trajectory.run-12345`, `research.topic.source-3`).
- `shared_state` is in `BUILTIN_TOOL_NAMES` (MCP collision guard).
- Canonical example: `skills/analyze-trajectory/SKILL.md` — see its "Handle large artifacts" section for chunking, "Dispatch a sub-agent" section for the JSON contract, and "Recurse" section for the depth cap.

**When to reach for RLM:**
- The artifact is too large for one prompt (>5KB triggers sub-agent dispatch; chunk if >30KB).
- The work is decomposable — different focused questions over the same artifact, each independently answerable.
- Fidelity loss is acceptable — sub-agents return summaries, not raw text. (Use direct read when exact diffs matter.)
- Cross-piece reasoning is light — each sub-question can be answered locally.

**When NOT to reach for RLM:**
- The artifact is small (≤5KB; if exactly 5KB, prefer direct read) — sub-agent overhead exceeds the savings.
- The task needs *precise* control (writing code, surgical edits) — fidelity-loss in sub-agent summaries is fatal here.
- The work is sequential with strong mutual context — refactoring needs to see all pieces at once.
- You're already inside a sub-agent and depth=3 is reached — stop, return what you have, do not dispatch further.

**Established pattern in yoyo:**
1. Parent fetches the artifact via `bash`, then stores it under `<skill>.<key>` via the `shared_state` tool's `set` op. The parent gets `shared_state` exactly when it gets `sub_agent` — both are pushed together in `agent_builder.rs` and are absent together under `--no-tools` (#715; before Day 163 only sub-agents had the tool, so this step silently did nothing at the top level).
2. Parent calls the `sub_agent` tool with a *focused question* and a *reference* to the shared-state key — never the artifact itself in the prompt.
3. Sub-agent reads via `shared_state.get`, returns a JSON-shaped summary (see `analyze-trajectory`'s "Dispatch a sub-agent" section for the schema).
4. Parent recurses on `deeper_question` if confidence is low. Hard depth cap = 3 (counts each sub_agent dispatch toward the budget).
5. On sub-agent failure / non-JSON response, fall back to direct read of a slice and produce a low-confidence diagnosis.

For the broader capability roadmap (codebase archaeology, semantic git bisect, multi-source research synthesis, large-scale refactor coordination, etc.), see issue #341.

## MCP gotchas

**Tool-name collisions (Day 39):** If an MCP server exposes a tool whose name matches one of yoyo's builtins (`bash`, `read_file`, `write_file`, `edit_file`, `list_files`, `search`, `rename_symbol`, `ask_user`, `todo`, `web_search`, `sub_agent`, `shared_state`), the Anthropic API will reject the first turn with `"Tool names must be unique"` and the session dies. The flagship reference server `@modelcontextprotocol/server-filesystem` collides on `read_file` AND `write_file`, so the common case was broken until the guard landed.

yoyo now runs a pre-flight tool listing (via a short-lived `yoagent::mcp::McpClient`) before every stdio connect. If any MCP tool name appears in `BUILTIN_TOOL_NAMES` (defined in `src/agent_builder.rs`) or was already registered by an earlier server, the server is **connected under a prefix** (Day 231, #1041, `src/mcp_prefix.rs`): every tool of that server reaches the model as `<prefix>__<tool>`, and calls still use the server's own names. Prefix = sanitized `[mcp_servers.<name>]`, or `mcp<N>` for the N-th `--mcp` flag. Non-colliding servers keep their own names. **Day 240:** every server, prefixed or not, connects through `mcp_prefix::connect_mcp` as a yoagent `McpToolSource`, which withdraws a dead server's tools at the next run and never consumes the agent (superseded: `with_mcp_server_stdio` / `from_client_with_prefix`; history in ARCHITECTURE.md). Only if the *prefixed* names still collide is the server skipped. **Superseded claim, recorded rather than erased:** until Day 231 this sentence read "the whole server is skipped", which left the filesystem server contributing zero tools. Stdio only (yoyo has no HTTP MCP path); the double spawn remains.

**Current state, in brief (history moved to ARCHITECTURE.md, Day 250).** If the pre-flight listing fails it is retried once (`MCP_PREFLIGHT_ATTEMPTS = 2`, in `fetch_mcp_tool_names_retrying`); after the last failure the server falls through to the real connect so the user sees yoagent's own diagnostic, and `collision_guard_skipped_message` says the guard did not run, names the server, and warns of `Tool names must be unique`. An MCP connect failure prints a red `✗ mcp:` line and keeps the agent (`connect_mcp` never consumes it); an OpenAPI failure still rebuilds the agent, dropping every earlier connection, and `connections_lost_note` says how many. Either failure also reaches the model once per session: `take_external_failure_note` drains a store and prepends `external_tool_failure_note` to the first turn on both prompt seams, so the model reads "configured but failed to connect" and not "this capability does not exist". A collision-guard skip is not recorded as a failure, and nothing reconnects a failed server. **Read ARCHITECTURE.md's entry "`src/agent_builder.rs` + `src/mcp_prefix.rs` — MCP history" before changing any of this**: it holds the superseded claims, the weak source guards and their positive controls.

Keep `BUILTIN_TOOL_NAMES` in sync with `tools::build_tools` and the sub-agent's `SharedStateTool` whenever a new builtin is added — the pure helper `detect_mcp_collisions` is unit-tested in `src/agent_builder.rs` against the filesystem server's known tool set as a regression guard. Its consumers: the MCP collision guard, `--lite` (`compute_lite_disallowed_tools`), the `--no-tools` disallow list in `src/cli.rs`, and `src/hooks.rs`'s `unknown_hook_tool_warning`; a dropped name would also break those. The const is the single authority: **every consumer reads it, none copies it.**

## yoagent: Don't Reinvent the Wheel

yoyo is built on [yoagent](https://github.com/yologdev/yoagent). Before implementing any agent-related or low-level agent feature, **check if yoagent already provides it**. Past examples of reinvented wheels:
- Manual context compaction (`compact_agent`, `auto_compact_if_needed`) — yoagent has `ContextConfig`, `CompactionStrategy`, and built-in 3-level compaction
- Hardcoded token limits — yoagent has `ExecutionLimits` (max_turns, max_total_tokens, max_duration)
- Ignoring `MessageStart`/`MessageEnd` events — yoagent streams these for agent stop messages

**The `yoagent` skill is reference, not a source of tasks.** It tells you what yoagent already offers: which feature fits which job, the pitfalls, and what changed per version. Read it when you are about to implement something agent-related, so you use what yoagent provides instead of building it again. It ships inside the yoagent crate, and the evolve harness loads it from the exact version in Cargo.lock (it loads any dependency that ships `skills/`, naming none).

**Before building agent infrastructure in src/:**
1. Read the `yoagent` skill, then search yoagent's source (`~/.cargo/registry/src/*/yoagent-*/src/`) for existing features
2. Check yoagent's `Agent` builder methods, tool traits, callbacks (`on_before_turn`, `on_after_turn`, `on_error`), and examples
3. If yoagent has it → use it. If yoagent almost has it → file it here with `--label upstream` (the evolve skill's "Filing Issues" says how; the creator moves it to yoagent). If yoagent doesn't have it → build it in yoyo.

Key yoagent features available: `SubAgentTool`, `SharedState`, `SharedStateTool`, `ContextConfig`, `ExecutionLimits`, `CompactionStrategy`, `AgentEvent` stream, `default_tools()`, `SkillSet`, `with_sub_agent()`. For `SharedState` / sub-agent recursion details and decision trees, see the **RLM substrate** section above.

**yoagent 0.7.x prompt lifecycle gotcha (Issue #258):** `agent.prompt()` / `agent.prompt_messages()` spawns the agent loop into a tokio task and returns the event receiver immediately. The agent's internal `self.messages` is NOT updated until `agent.finish().await` is called. If you read `agent.messages()` (or `total_tokens(agent.messages())`) right after draining the event stream WITHOUT calling `finish()` first, you will see the stale pre-prompt state — which silently breaks anything that depends on message count (e.g., the context-window usage bar). Always call `agent.finish().await` between event drain and message read.

## Two Audiences: product vs evolve

yoyo serves two different customers, and every task must know which one it's for:

- **product** — people who install yoyo and use it on *their* projects: any
  language, any setup, local models, no CI. Product surface (defaults, CLI
  flags, setup wizard, startup behavior, docs) must be safe for all of them.
- **evolve** — yoyo's own evolution loop: always this Rust repo, fast tests,
  CI. Conveniences built for this loop are fine — but they must be **opt-in**
  the moment they touch anything a product user sees.

The rule: **defaults must be product-safe; evolution-loop conveniences are
opt-in.** Issue #448 is the canonical failure — auto-watch was built for the
evolve loop and shipped as a product default, breaking non-Rust users. Every
planned task declares `Kind: product` or `Kind: evolve` in its task file; the
evaluator rejects evolve-kind changes to product surface that aren't opt-in.

## Safety Rules

These are enforced by the `evolve` skill and `evolve.sh`:
- Never modify `IDENTITY.md`, `PERSONALITY.md`, `ECONOMICS.md`, `scripts/evolve.sh`, `scripts/format_issues.py`, `scripts/build_site.py`, or `.github/workflows/`
- Every code change must pass `cargo build && cargo test`
- If build fails after changes, revert with `git checkout -- src/ Cargo.toml Cargo.lock`
- Never delete existing tests
- Multiple tasks per evolution session, each verified independently
- Write tests before adding features
- **Every deliberate sabotage of a guard, function or classifier must carry a marker, and the tree cannot be green until it is removed.** Running a positive control has two halves — break the guard and watch it fail *by name*, then **restore it and watch it pass**. The second half is the one with no natural owner, and skipping it once cost ~22 hours of red `main` plus four reverted sessions (`src/cli.rs`'s `sanitize_for_display` shipped with `return s.to_string(); // NEUTERED POSITIVE CONTROL` above its real body). So when you neuter anything — for a positive control or any other reason — put `NEUTERED`, `DO NOT COMMIT`, `SABOTAGE` or `TEMPORARILY DISABLED` on the line, and `tests/neutered_guards.rs` will refuse to pass until you take it out or name it in `REGISTERED_MARKERS`. Best practice is stronger than the marker: issue the mutate→run→restore as **one atomic command** so the restore cannot be forgotten, and run file-mutating controls **serially** (two in one parallel block raced once and one falsely *passed*). Three limits, so this is not over-trusted: it does **not** cover the `session wrap-up` sweep (`scripts/evolve.sh` commits with `git add -A` and no build/test check, and that file is protected); it is **redundant with `unreachable_code`** for a `return` placed above a live body, which clippy already makes fatal; and the genuinely new coverage is the **silent** case — a neutered branch no test covers, and a neutered helper in `scripts/*.py`, where `cargo test` gives zero coverage.
- **Never use byte indexing on strings.** `s[..n]`, `s.truncate(n)`, and `s.split_at(n)` panic if `n` falls inside a multi-byte UTF-8 character. Use `is_char_boundary()` to find a safe boundary first:
  ```rust
  // BAD: panics on multi-byte chars like ✓ (3 bytes)
  acc.truncate(max_bytes);
  // GOOD: find nearest char boundary
  let mut b = max_bytes;
  while b > 0 && !acc.is_char_boundary(b) { b -= 1; }
  acc.truncate(b);
  ```
  This caused planning agent crashes in production (#250).
- **`run_git()` has a `#[cfg(test)]` destructive-command guard.** During `cargo test`, calling `run_git()` with a destructive subcommand (commit, revert, reset, push, checkout, etc.) from the project root panics. Tests that need destructive git operations must use a temp directory. This prevents tests from accidentally mutating the real repo (which caused a 6-session deadlock across Days 42-44).
- **Never add a definition without its consumer in the same edit.** A new `fn`, `let`, or enum variant that nothing reads breaks the build, and the gate reverts the *whole* task — including the correct work sitting beside it. Dead functions and unread variables fail clippy under `-D warnings` (`-D dead-code`, `-D unused-variables`, `-D unused-assignments`); a new enum variant fails `cargo build` outright with `E0004 non-exhaustive patterns`. So: add a helper → add its call site. Add a variable → add the code that reads it. Add an enum variant → update every `match` on that enum. If you genuinely need the definition before the consumer, don't leave it dangling for a later step — inline the logic at the call site instead, or finish both halves before ending the turn. Three reverts in fourteen days from this one shape: #618 (functions), #653 (functions), #658 (variable), plus sibling #647 (enum variant).
- **Before declaring any task complete, run `cargo clippy --all-targets -- -D warnings` explicitly** rather than relying on auto-watch. The last edit of a task is the one most likely to be unlinted: watch runs mid-turn, skips when nothing changed, and the task can end before anything re-lints.
