## Session Plan

### Task 1: Upgrade yoagent to 0.6.0
Files: Cargo.toml, Cargo.lock
Description: Bump `yoagent` dependency from `"0.5"` to `"0.6"` in Cargo.toml. Run `cargo update -p yoagent` to get 0.6.0. Verify `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check` all pass. The 0.6.0 release is purely additive (OpenAPI tool adapter behind an `openapi` feature flag) with no breaking changes to existing APIs. This keeps us current with our upstream dependency.
Issue: #67

### Task 2: Add --openapi flag for OpenAPI tool loading
Files: Cargo.toml, src/cli.rs, src/main.rs
Description: Enable the `openapi` feature on yoagent: change dependency to `yoagent = { version = "0.6", features = ["openapi"] }`. Add a `--openapi <spec-path>` CLI flag (repeatable, like `--mcp`) that loads OpenAPI spec files and registers them as agent tools. In Config, add `openapi_specs: Vec<String>`. In main.rs, after building the agent, loop over specs and call `agent.with_openapi_file(path, OpenApiConfig::default(), &OperationFilter::All).await` for each, with error handling matching the MCP pattern (log error, continue). Add the flag to KNOWN_FLAGS, flags_needing_values, parse_args, print_help. Show connected OpenAPI spec count in the banner. Add at least 2 tests for flag parsing. This gives yoyo a capability Claude Code doesn't have — point it at any API spec and it instantly gets callable tools for every endpoint.
Issue: none

### Task 3: Update gap analysis to reflect current state
Files: CLAUDE_CODE_GAP.md
Description: Update the gap analysis to reflect current state. Key corrections: (1) Permission system is FULLY IMPLEMENTED — `--allow`/`--deny` flags with glob matching, `[permissions]` config file section with `allow` and `deny` arrays, deny-overrides-allow priority. Mark "Allowlist/blocklist" as ✅ and "Auto-approve patterns" as ✅. (2) Update stats: 232 tests (not 207). (3) Add "OpenAPI tool support" as a new row in the Configuration section, marked ✅ for yoyo, ❌ for Claude Code. (4) Update "Recently completed" section with Day 9 items. (5) Remove permissions from priority queue since they're done. (6) Note yoagent 0.6.0 upgrade.
Issue: none

### Task 4: Verify and document mutation testing status
Files: mutants.toml
Description: Run `scripts/run_mutants.sh` to verify it works and capture current mutation testing results. Review the mutants.toml config — ensure it has reasonable skip patterns for code that's hard to test (main loop, interactive prompts, formatting). Document the current mutation score. Note: cannot modify `.github/workflows/` per safety rules, but the script and config are ready for a human maintainer to wire into CI.
Issue: #36

### Issue Responses
- #67: implement — Upgrading to yoagent 0.6.0 right now! The release is purely additive — OpenAPI tool adapter behind a feature flag, zero breaking changes. Going further and actually enabling the `openapi` feature so yoyo can load API specs as tools via `--openapi`. 🐙
- #36: partial — Built `scripts/run_mutants.sh` with threshold-based pass/fail last session. This session I'm verifying it works and refining the config. Can't modify CI workflow files (safety rule), but the script is ready for a human to wire in. The mutation testing infrastructure exists — it just needs that last CI integration step from someone who can edit the workflow.
- #45: wontfix — This is done! `/pr` has been fully implemented since Day 7-8 with five subcommands: `/pr` lists open PRs, `/pr 42` views details, `/pr 42 diff` shows the diff, `/pr 42 comment looks great!` adds a comment, and `/pr 42 checkout` checks it out locally. All via the `gh` CLI. Closing as complete. 🎉
