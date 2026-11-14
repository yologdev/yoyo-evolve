# Assessment — Day 259

## Build Status
Pass — verified by the harness at session start (HEAD 3ea4439d; CI on that sha: success). Binary built 02:04 today, after HEAD's commit (23:57) — so probes below ran the current code. `yoyo --version` → v0.4.0 (3ea4439d). `yoyo -p "Reply with exactly: OK"` → `OK`, exit 0, model claude-opus-5-5.

**Size-gate warnings (from trajectory, confirmed with wc -l):**
- `src/context.rs` = 2035 lines, UNLISTED in `tests/module_size.rs` GRANDFATHERED list; trajectory says +15 more lines makes `cargo test` fatal (whole-task revert). Any task touching context.rs must split it or add the entry first.
- `src/format/mod.rs` = 2874 vs recorded 2788 (+86 drift); +14 more is fatal. Paste the new value over its entry if touched.

## Recent Changes (last 3 sessions)
All 10 recent sessions 2/2 tasks, 0 reverts.
- **Day 258 21:51:** Fixes #1192 — `/read` and `/plan` now refuse bash deletes (rm, rmdir, unlink, shred) by adding them to the write set; 17-shape probe went 12 leaking → 0. Fixes #1190 — `--help` no longer claims `--max-tokens` defaults to 8192.
- **Day 258 15:50:** Fixes #1189 — `strip_quoted_regions` no longer blanks `$(...)`/backticks inside double quotes (real-shell differential test). Fixes #1185 — config rungs compared by file identity (home-dir run no longer lists its own file as "not read"). Filed #1191 (GIT_DIR= glued to `$(`).
- **Day 258 11:27:** `exit_process` single door (tests no longer die silently on process::exit; #1187 follow-up). #1181 recording-stub test proving cross-origin 307/302 redirects don't carry the key.
- **Day 257:** Part of #1184 slice 1 — `toml` crate gate at load (invalid TOML → exit 2 with path:line:col). Readers still hand-parse behind it.
- llm-wiki (external journal): still paused mid-migration, per every Day 258 entry.

## Source Architecture
181 files in src/, ~239k lines total (incl. *_tests.rs). Largest: cli.rs 7874, commands_risk.rs 6645, tool_wrappers.rs 5762, tools.rs 5286, agent_builder.rs 5009, config.rs 4895, safety.rs 4789, commands_search.rs 4780, commands_spawn.rs 4566, watch.rs 4444, prompt.rs 3987.
Config subsystem (the #1184 target): config.rs 4895 + config_value.rs 611 + config_unknown_keys.rs 1403 + config_syntax.rs 474. config.rs still has ~21 `parse_*_from_config`/load functions; the only `toml::` use in src/ is `toml_parse_error` (config.rs:2133) — the gate. `toml = "1"` and `serde` (derive) are already direct deps.

## Self-Test Results
- Basic `-p` round-trip: OK.
- **Config probe (tempdir repo, `.yoyo.toml` = `model = ""`, `[mcp_servers]` + inline `fs = { command = "npx", ... }`, `[permissions] deny = ["rm *"] # comment`):**
  - `config get model` → `model =   (.yoyo.toml)` exit 0 — **#1172 still live**: empty value shown as set.
  - Both commands warn `[mcp_servers] is valid TOML, but yoyo does not read [mcp_servers], so its keys (fs) have no effect` — **#1182 still live**: a valid inline-table MCP server is dropped; the warning now openly admits the input is valid TOML, i.e. the defect is my reader, not the user (d255 lesson).
  - Deny with trailing comment was read (`deny = ["rm *"]`) — #1177 fix holds.
- Both live config bugs are exactly what #1184 slice 2 (readers go through the parsed `toml::Table`) would remove.

## Evolution History (last 5 runs)
- 2026-11-14 01:57 — in progress (this run).
- 2026-11-13 21:46 — success. 2026-11-13 15:43 — success. 2026-11-13 11:21 — success.
- 2026-11-13 11:49 — **cancelled** (overlapping run start ~28 min after the 11:21 one; looks like concurrency cancellation, not a failure — not investigated further).
- CI: last runs green. Trajectory's recurring CI errors (clippy `if` guard returning bool literal, exit 101) all predate the latest green.
No reverts, no provider errors in window.

## Capability Gaps
(research section below; prior known gaps) vs Claude Code: no polished TUI (#215 parked), no benchmark submission (#156 parked), MCP is stdio-only with no restart of dead servers (#1134), no hot-reload of project hooks/MCP on /cd (#1040 parked). Product-surface correctness work (config, safety, pricing) dominates recent sessions; capability expansion has been rare.

## Bugs / Friction Found
1. **#1184 slice 2 (owner-filed, @yuanhao)** — readers still hand-parse behind the toml gate. Live specimens confirmed today: #1172 (empty model shown as set, ignored by run) and #1182 (inline table under header refused). Owner's do-list: typed structs via serde, one parse per file; toml_edit for writes; delete hand parser + tomllib differential test after moving cases. This is the highest-leverage open item: one migration closes a class that produced 5+ tickets in 3 days, several security-relevant (deny fences). Needs slicing — 21 readers; a first slice could move the `HashMap<String,String>` / scalar path + `[permissions]`/`[directories]` arrays onto `toml::Table`, and close #1172/#1182 only if their done-whens are met (check each body before "Fixes").
2. **#1191** — `detect_git_redirection_escape` misses `GIT_DIR=` glued to `$(`/`(`/backtick (`echo $(GIT_DIR=/x git log)`). Small, safety, well-specified (cause located in safety.rs: token-anchored over split_whitespace). Census note: per d246/d249, check every other token-anchored env/verb check in safety.rs for the same `$(`-glued shape, not just this one.
3. **#1188** — trust keyed on path, not content: a `git pull` adding `base_url` to an already-trusted `.yoyo.toml` is honoured with no prompt. Security; design question (hash the trust-relevant keys?).
4. **#1186** — claude-fable-5-2 priced as Fable 5 ($10/$50) vs vendor $8/$40; labelled upstream too.
5. Size gate: context.rs and format/mod.rs near fatal (see Build Status).
6. Doc freshness: CLAUDE_CODE_GAP.md header 184 days stale.

## Open Issues Summary
agent-self open (non-parked): #1191, #1188, #1186, #1184 (owner-filed, slice 1 done), #1182, #1173 (Bedrock thinking without maxTokens, unchecked), #1172, #1147 (29/78 non-Claude known ids lack outside authority), #1134 (MCP lifecycle), #1121 (Bedrock inference-profile ids), #1102 (hard_deny heredoc false positive), #1100 (MCP servers outlive panic), #1094 (write_file partial-read residue), #1064 (credential scrub opt-in), #1012 (price audit). agent-input: #1122 (yoagent upgrade/skill reuse), #991 (retry_safe_events for non-TTY -p). Parked: #1070, #1060, #1040, #1018, #1014, #854, #341, #215, #156.
No comments on #1191/#1188/#1186/#1182/#1172 since filing.

## Research Findings
Recall: yopedia has a Day 249 "Agent CLI Changelog Delta Analysis" note; `/api/query` returned "Sign in required" (keyword search worked). Today's scan saved to yopedia ("Agent CLI changelog delta, Nov 3-13 2026").
- **Claude Code 2.1.321 (Nov 9)** publishes claude-fable-5-2: 1M context, $8/$40 per Mtok, $0.20 cache reads. That's an outside source backing **#1186** (yoyo bills it as Fable 5, $10/$50, $1.0 cache read). The fix is cheap and sourced if yoyo owns the row. Check first whether the row comes from yoagent's table (#1186 is labelled upstream).
- **Claude Code 2.1.323 (Nov 12)** adds a startup warning for Bash allow rules with `*` in an env-var value, e.g. `Bash(PORT=* npm run dev)`. This defect class may carry over to yoyo's `[permissions] allow` globs: does `allow = ["PORT=* npm run dev"]` admit `PORT=$(<destructive cmd>) npm run dev` or `PORT=1 GIT_DIR=/x npm run dev`? `PermissionConfig::check_with_suspended` (config.rs:23) has guards for option-swallowing, command chains and cloud metadata. Whether a `*` in an assignment slot is covered is **unprobed**. A unit probe against `check_with_suspended` would settle it in minutes.
- **Claude Code 2.1.319**: adds `permissions.denyInsteadOfPrompting`. Also, "a project's settings files or a plugin can no longer switch [the auto-mode server check] back on once you have switched it off outside the project", which hardens the trust boundary around project config.
- **Codex 0.169 (Nov 13)**: `codex exec` keeps project config and MCP servers **disabled in untrusted directories**, and `--trust` saves trust explicitly. Same family as **#1188** (yoyo trusts by path, so changes to a trusted file's content are honoured silently).
- Codex 0.163–0.167: steering mid-response by sending a follow-up, an agent command center, a built-in Vertex provider, compaction that counts output kept from failed responses, and `codex doctor --network-only`.
- The biggest gaps are still structural (TUI, background/multi-agent UX, MCP lifecycle), not correctness. But this month the rivals worked on trust boundaries and permission-rule footguns, which overlap yoyo's open #1188 and #1191 directly.

## Incidence during this assessment
- 02:2x: hard_deny refused my own `python3 <<EOF` heredoc that wrote this file, because the text *mentioned* a recursive home-directory delete inside an example string. Nothing would have executed it. That's the mention-only false positive class of **#1102**, logged as an incidence there.

## Planner notes (candidates, ranked by my read)
1. **#1184 slice 2** (owner-filed, highest leverage): route the readers through the parsed `toml::Table`. If their done-whens are met, this closes the live #1172 and #1182. Slice carefully: config.rs is 4895 lines (grandfathered), so check its cap entry before adding.
2. **#1191** (small safety fix), plus a census of every token-anchored check in safety.rs for the shape glued to `$(`, `(` or a backtick.
3. **#1186**: pricing row from a vendor number that's now published.
4. Probe the `PORT=*` allow-glob class (Claude Code 2.1.323) against `check_with_suspended`, then file or fix.
Don't touch src/context.rs or src/format/mod.rs without first fixing their size-gate entries.
