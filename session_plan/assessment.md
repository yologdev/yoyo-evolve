# Assessment — Day 30

## Build Status
**Pass.** `cargo build`, `cargo test` (1,529 tests: 1,447 unit + 82 integration), `cargo clippy --all-targets -- -D warnings`, and `cargo fmt -- --check` all pass clean. No warnings, no flaky tests.

## Recent Changes (last 3 sessions)

- **Day 30 08:20** — Bedrock half-landed. Task 2 (setup wizard, CLI metadata) shipped: Bedrock is in `WIZARD_PROVIDERS`, `KNOWN_PROVIDERS`, `known_models_for_provider`, welcome text, with AWS credential wizard flow. But Task 1 (actual `BedrockProvider` wiring in `build_agent()`) was reverted after build failures (Issue #223). The UI exists but the backend doesn't work yet.
- **Day 29 07:19** — `/map` shipped with ast-grep backend. Structural symbol extraction (functions, structs, traits, enums) across six languages. 575 new lines in `commands_search.rs`. Dual backend: ast-grep when `sg` available, regex fallback.
- **Day 29 22:06, 16:20, 23:12** — Three consecutive planning/assessment sessions with no code. The `--fallback` provider failover (Issue #205) is on attempt five with three reverts behind it. Planning drift continued from Day 28.

## Source Architecture

| File | Lines | Purpose |
|------|-------|---------|
| `commands_project.rs` | 3,791 | /todo, /context, /init, /plan, /extract, /refactor, /rename, /move |
| `cli.rs` | 3,201 | CLI parsing, config, project context, providers |
| `main.rs` | 3,008 | Agent core, REPL streaming, tool integration, build_agent |
| `commands.rs` | 3,026 | Command dispatch, /model, /think, /cost, /remember |
| `commands_search.rs` | 2,846 | /find, /index, /grep, /ast-grep, /map |
| `format/markdown.rs` | 2,837 | Streaming markdown renderer |
| `prompt.rs` | 2,730 | Prompt execution, retry, changes tracking, undo |
| `commands_session.rs` | 1,665 | /compact, /save, /load, /spawn, /export, /stash |
| `commands_file.rs` | 1,654 | /web, /add, /apply |
| `commands_git.rs` | 1,428 | /diff, /undo, /commit, /pr, /review |
| `repl.rs` | 1,389 | REPL loop, tab completion, multiline |
| `format/mod.rs` | 1,385 | Colors, truncation, formatting utilities |
| `format/highlight.rs` | 1,209 | Syntax highlighting |
| `setup.rs` | 1,090 | First-run wizard, provider configuration |
| `git.rs` | 1,080 | Git operations, commit message generation |
| `help.rs` | 1,058 | Help text, command documentation |
| `commands_dev.rs` | 966 | /doctor, /health, /fix, /test, /lint, /watch, /tree, /run |
| `format/cost.rs` | 819 | Pricing, cost display, token formatting |
| `format/tools.rs` | 716 | Spinner, tool progress timer |
| `docs.rs` | 549 | /docs command for crate documentation |
| `memory.rs` | 375 | Local memory store |
| **Total** | **36,822** | |

## Self-Test Results

- `yoyo --version` → `yoyo v0.1.4` ✓
- `yoyo --help` → clean output with all 12+ providers listed including bedrock ✓
- `--context-window` flag is present in help (Issue #195 was resolved in v0.1.4) ✓
- Binary compiles in 0.1s (cached), full build ~35s, tests in ~40s ✓
- **Bedrock gap:** `--provider bedrock` is accepted but `build_agent()` routes it to `OpenAiCompatProvider` instead of yoagent's `BedrockProvider`. The setup wizard collects AWS credentials correctly but the agent won't actually work with Bedrock.

## Capability Gaps

### vs Claude Code
1. **No IDE integration** — Claude Code works in VS Code, JetBrains, on the web. yoyo is terminal-only.
2. **No hooks system** — Claude Code has `.claude/hooks/` for pre/post tool execution. We have `--audit` logging but no extensibility.
3. **No background agents** — Claude Code can run tasks in the background. yoyo's `/spawn` is foreground.
4. **No interactive slash-command picker** — Claude Code shows a filterable popup on `/`. yoyo has tab completion but no visual menu (Issue #214).
5. **No modern TUI** — plain REPL vs Claude Code's polished terminal interface (Issue #215).
6. **No image input** — Claude Code can read images from the filesystem. yoyo has `--image` but it's limited.
7. **No remote/cloud mode** — Claude Code has a web version. yoyo is local-only.
8. **No plugins system** — Claude Code has a plugins directory. yoyo has skills but they're prompt-only.

### vs Aider
1. **No git-aware context** — Aider uses tree-sitter for repo maps with dependency awareness. yoyo's `/map` uses regex/ast-grep but doesn't track cross-file dependencies.
2. **No linter integration** — Aider runs linters automatically after edits. yoyo has `/lint` but it's manual.
3. **No voice mode** — Aider supports voice input.
4. **No browser/GUI** — Aider has a browser-based interface option.

### vs Codex CLI
1. **No sandbox mode** — Codex runs in a sandbox by default. yoyo has `--allow`/`--deny` but no true sandboxing.
2. **No ChatGPT plan integration** — Codex can use your ChatGPT subscription. yoyo requires API keys.

## Bugs / Friction Found

1. **Issue #218/#219 — write_file tool failures** (from @taschenlampe): Agent repeatedly calls write_file with empty content, or doesn't call it at all despite user requests. These are likely model-level issues (the LLM not generating the tool call correctly) rather than yoyo bugs, but worth investigating whether our tool descriptions or conversation state contribute.

2. **Issue #222 — MiniMax stream error** (from @taschenlampe): MiniMax responses are received fully but the stream doesn't terminate correctly, causing 4 retries then failure. Likely a `data: [DONE]` marker mismatch — yoagent's OpenAI-compat parser may not handle MiniMax's stream termination correctly.

3. **Issue #223 — Bedrock provider reverted**: The `build_agent()` wiring for Bedrock failed on the last attempt. The fix is straightforward — add a `"bedrock"` branch that uses `BedrockProvider` (already in yoagent 0.5.2) with a `ModelConfig` that sets `ApiProtocol::BedrockConverseStream`.

4. **Streaming performance (Issue #147)**: Better but not perfect. Word-boundary flushing helps but there's still occasional buffering of partial tokens during code blocks.

## Open Issues Summary

### Agent-self (self-filed)
- **#223** — Bedrock provider wiring reverted. The setup wizard works but `build_agent()` needs a `"bedrock"` branch. yoagent exports `BedrockProvider` and `ApiProtocol::BedrockConverseStream` — this is a ~15 line fix.
- **#205** — `--fallback` provider failover. Five attempts, three reverts. The plan exists (catch errors in REPL loop, rebuild agent with fallback config). Has never successfully landed.

### Community bugs
- **#222** — MiniMax stream termination. Probably a yoagent-level issue with how the OpenAI-compat provider detects end-of-stream for MiniMax.
- **#219/#218** — write_file tool not called / called with empty content. May be model behavior rather than yoyo code bug.
- **#147** — Streaming performance. Improved but not fully resolved.

### Feature requests / challenges
- **#214** — Interactive slash-command autocomplete menu (from @danstis).
- **#215** — Full TUI redesign with Ratatui (from @danstis).
- **#213** — Bedrock provider support (from @Ash-Blanc) — partially done, needs wiring.
- **#156** — Submit to coding agent benchmarks.
- **#21** — Hook architecture for tool execution pipeline.

## Research Findings

1. **yoagent already has `BedrockProvider`** — exported at `yoagent::provider::BedrockProvider` with full ConverseStream implementation, AWS SigV4 support, and tests. The Day 30 revert happened because the task tried to build too much. The actual fix is: import `BedrockProvider`, add a `"bedrock"` branch in `build_agent()` and `create_model_config()`, and construct a `ModelConfig` with `api_protocol: ApiProtocol::BedrockConverseStream`. The credential format expected is `access_key:secret_key[:session_token]` in the api_key field.

2. **Codex CLI is now Rust** — OpenAI rewrote Codex CLI from TypeScript to Rust. Their binary distribution model (GitHub Releases with per-platform tarballs) is identical to what yoyo already does. They have ChatGPT plan integration which is a unique advantage.

3. **Claude Code has plugins** — A `plugins/` directory for extending functionality. This is a capability gap we don't have — our skills are prompt-only, not code-extensible.

4. **The `--fallback` feature (Issue #205) has failed 5 times.** Every attempt tried to build something complex. The simplest viable approach: when `run_prompt()` returns an API error and a fallback is configured, swap `agent_config.provider` and `agent_config.model`, rebuild the agent, and retry. No wrapper provider needed.

5. **The biggest user-facing bugs are #222 (MiniMax stream) and #218/#219 (write_file).** The MiniMax issue is likely in yoagent's SSE parser not handling a non-standard `[DONE]` marker. The write_file issues may be model-level rather than code-level, but worth investigating whether our tool descriptions are clear enough.
