# Assessment — Day 263

## Build Status
Pass — verified by the harness at session start on cc727eac. CI green on the last 5 pushes; last 5 evolve runs `success` (the 12:36 run is this one). Binary built 12:41 today: `yoyo --version` → `v0.4.0 (cc727eac 2026-11-18)`; `yoyo -p "Reply with exactly: pong"` → `pong` (anthropic / claude-opus-5-5). No full suite re-run (per instructions).

## Recent Changes (last 3 sessions)
- **Day 262 23:24** — `638791d2` Fixes #1196: a bare `echo`/`printf` statement's quoted words are a mention (1,084 of 4,444 corpus commands went refused→allowed, 0 the other way); `$(...)` bodies still checked. Commit body is empty (title only) — evidence lives on #1196 comments. `46cc56c3` Part of #1184: `[permissions]`/`[directories]` read from parsed `toml::Table`, multi-line deny list now builds its fence; line-walker metric 21→14. Filed #1209 (bash -c "kill -9 1" passes) and #1210 (non-print quoted mention still refused). Evolve skill got "grep the diff for #N before Fixes #N".
- **Day 262 12:50** — `bash_parse::unanalyzable_reason`: parse errors, `eval`, hidden command names now ask even under "always"/allow rules (Part of #1202, fail-closed clause). A/B question to @yuanhao on #1202 about `--yes`/`-p`/non-tty (no reply yet — he has no comments on #1202 beyond the body). Bedrock picker: opus-4-7 → opus-5.5 (Fixes #1121).
- **Day 262 00:01** — brace/backslash re-spelling check in bash_parse (Fixes #1205, Part of #1202); per-provider temperature table up to 2.0 (Fixes #1207; residue #1208).
- Since then: skill-evolve NO-OP (11th consecutive saturation), synthesize regenerated memory.
- llm-wiki (journals/llm-wiki.md): paused mid-migration; nothing new.

## Source Architecture
~243k lines across 188 files in src/ (`find src -name '*.rs' | xargs wc -l`). Largest: cli.rs 7,890; commands_risk.rs 6,645; tool_wrappers.rs 5,762; tools.rs 5,301; agent_builder.rs 5,039; safety.rs 4,816; commands_search.rs 4,784; config.rs 4,700; commands_spawn.rs 4,566; watch.rs 4,444; prompt.rs 3,988. Entry: main.rs → cli.rs (parse_args) → agent_builder.rs (build_agent) → prompt.rs (run_prompt*). Safety: safety.rs (33 SAFETY_CHECKS, text) + bash_parse.rs (tree-sitter tree pass, 759 lines). Config: config.rs + config_toml_read.rs (toml crate) + config_value.rs (hand reader, being deleted under #1184).

**Size-gate hazards (from trajectory, confirmed by wc):**
- `src/context.rs` 2,035 lines, over the 2,000 cap and NOT in GRANDFATHERED_OVERSIZED_MODULES (module_size.rs:354 comment says Day 253 pasted it "unlisted at 2036, 14 lines from FATAL"). +15 more lines → `cargo test` fatal → whole-task revert.
- `src/format/mod.rs` 2,874 vs register 2,788 (+86 drift; +14 more → fatal).
Any task touching either file must split or re-paste in the same diff.

## Self-Test Results
- `-p` round trip works; auto-watch line prints after a no-change turn ("skipping"), fine.
- `yoyo config get model` → `claude-opus-5-5 (.yoyo.toml)` ✓.
- `yoyo grep -n x dir` → "unknown flag '-n' (did you mean -s?) … Nothing ran." ✓ (exit 2). `yoyo find --nme` → refused ✓. `yoyo map --bogus` → refused with usage ✓. `yoyo skill show` → usage ✓. `yoyo diff --stat` → "(no uncommitted changes)" ✓.
- No breakage found in the shell-verb surface this pass.

## Evolution History (last 5 runs)
All 5 completed evolve runs `success` (Nov 16 01:32 → Nov 17 23:18). Trajectory: last 10 sessions 0 reverts; one Day-261 task never reached a verdict (model refusal → #1206). 1 provider error, retried. CI failures (2×, 5d ago, clippy bool-literal guard) are green since.
Pattern: commit bodies keep getting cut (638791d2 empty again); the "post evidence on the ticket before the gate" habit is what preserves the record.

## Capability Gaps
- **Status roll-up:** Claude Code 2.1.327 `/status` now lists broken agent/skill/command files, ignored permission settings, and hook matchers that never match; `/config` notes when another setting overrides a row. yoyo has the pieces (unknown_hook_tool_warning, unknown-config-key warnings, shadowed_key_note in `config get`) but scattered across startup stderr, no single place to look.
- **Unparseable-command feedback to the model:** CC improved the refusal for commands its rm check can't parse "so Claude can resend harmless ones in plain syntax instead of giving up". yoyo's Day-262 `unanalyzable_reason` routes to a user prompt (stderr `⚠ reason`); in non-tty/`--yes` it still auto-runs (A/B pending on #1202). Not checked what the model is told when the user says no.
- **New models:** GPT-6.2 Sol (Codex's new default, 500K) / Astra (272K) shipped Nov 17; GPT-6.1 Sol window raised to 500K. yoagent 0.27.9 (= latest on crates.io) has gpt-6.1 rows, none for gpt-6.2; yoyo's openai default is `gpt-6-sol` (providers.rs:323). Upstream-first per #1012/#1147 policy, not a hand row.
- **Worktree cleanup safety:** CC's ExitWorktree now asks before removing a worktree that may hold unsaved work. yoyo /spawn worktrees not checked this session.
- CLAUDE_CODE_GAP.md header verified Day 74, 188 days stale; body never refreshed (a project, not a line edit).

## Bugs / Friction Found
1. **Dead pointer shipped:** `docs/src/troubleshooting/safety.md:110` still says "can still ask (#1196)" — #1196 is closed by 638791d2. #1210 exists precisely to carry it; the retarget was not done. One-line fix (→ #1210). Also `src/safety_git_glued_tests.rs:364` calls "#1102/#1196" class "still open" (#1196 closed, #1102 open).
2. **#1209 real safety miss:** `bash -c "kill -9 1"`, `sh -c "crontab -r"` → analyze_bash_command None (5 of 33 checks can't see a trigger ending at a closing quote; tree pass doesn't parse `-c` bodies). Fix direction on ticket: parse a known shell's `-c` body as commands in the tree pass, like `$(...)` bodies. Graded read+temp-test probe, not re-run today.
3. **Size gate** context.rs / format/mod.rs near fatal (above).
5. **`/export` silently overwrites an existing file** (outside config/safety — commands_session.rs:899-916): `handle_export` does `std::fs::write(path, &markdown)` with no existence check; default path `conversation.md`, so `/export` twice, or `/export README.md`, clobbers the user's file with no prompt. Claude Code fixed this exact defect in 2.1.327 (Nov 17: "Fixed /export overwriting an existing file without asking"). Read from code, not run. Product-kind; good candidate for the self-driven slot given the concentration warning. Census idea: other REPL commands that write a user-named path (grep `fs::write(` in commands_*.rs non-test).
4. **#1206 upstream** yoagent#455 still OPEN; yoagent 0.27.9 is both pinned and latest — nothing to adopt yet.

## Open Issues Summary
Agent-self open (non-parked): #1210, #1209 (safety, filed Day 262), #1208 (temperature keyed on provider name), #1206 (refusal → cache_control 400; yoyo half shipped, waiting yoagent#455), #1202 (owner: tree-sitter bash safety; step 1 fail-closed partly done, A/B to owner unanswered ~23h), #1188 (trust keyed on path, not content), #1186 (fable-5-2 pricing, upstream), #1184 (owner: toml crate; metric 14 line-walker hits left, toml_edit writes, delete hand parser), #1147 (29/78 non-Claude ids without outside authority), #1134 MCP lifecycle, #1102 hard_deny heredoc mention, #1100 MCP orphans on panic, #1094 write_file read guard residue, #1064 child env key scrub, #1012 price audit. Owner agent-input: #1122 (yoagent skill/extensions), #991 (retry_safe_events for non-TTY -p).

**Concentration warning:** config 5/10, bash/safety 4/10 of last self-driven diffs. Trajectory asks the self-driven slot go elsewhere. Candidates outside config/safety: #1188 (trust — touches cli/config though), #991 (yoagent retry_safe_events in prompt path), #1100/#1134 (MCP), #1094 (write_file guard, tools), #1064 (child env scrub), size-gate split of context.rs. Blind-spot files never forecast: src/grep_status.rs (102 lines), src/tools_child_bash_tests.rs.

## Research Findings
Recall (yopedia): prior notes cover Claude Code to 2.1.325 (Nov 13) and Codex 0.167–0.169 security hardening; built on those. New today (saved to yopedia as "agent CLI deltas Nov 17-18 2026 mapped onto yoyo"):
- **Claude Code 2.1.327 (Nov 17):** /export overwrite fix (yoyo has the same defect, above); unparseable-command refusal reworded so the model retries in plain syntax; Bash commands whose variables expand to very long text are denied; replies on unrecognized models cut at 32K now continue at 128K (yoyo's guessed-limits class); `/status` broken-config roll-up; ExitWorktree asks before deleting unsaved work; `claude mcp add` stops instead of overwriting a `.mcp.json` with an unexpected shape; arrow keys on permission prompts stop at No.
- **Codex 0.169.1/0.170.0 (Nov 17-18):** GPT-6.2 Sol/Astra; oversized WebSocket requests fall back to HTTP instead of futile retries (retry-classifier class); same-named MCP servers kept separate across environments; permission updates no longer drop child threads' sandbox restrictions (cf. yoyo child-tool inheritance #977 work).
- **Cursor CLI v2026.10.22:** nested AGENTS.md rules apply when editing a file in a subdirectory even if never read there; one invalid MCP tool schema no longer hides the rest of a server's tools, and a server that fails to list tools reports an error to the agent (cf. yoyo #1134 / external_tool_failure_note).
Biggest gap remains breadth of polish around file-writing commands and a status roll-up, not core capability.

## Suggested priorities for the planner (my read, not binding)
1. Owner tickets #1184 (next slice: toml_edit writes / delete hand reader; metric 14) and #1202 (step 3: parse `-c` bodies = #1209) — but both are config/safety, which trajectory says are over-concentrated; at most one.
2. Self-driven slot outside config/safety: `/export` overwrite (product, small, competitor-validated), or #1094/#1100/#991.
3. Trivial carried debt: retarget `docs/src/troubleshooting/safety.md:110` "(#1196)" → #1210 (dead pointer; #1210's own done-when says to). Watch size gate on context.rs / format/mod.rs.
