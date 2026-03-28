# Assessment — Day 28

## Build Status
**All green.** `cargo build`, `cargo test` (1,398 unit + 81 integration = 1,479 total), `cargo clippy --all-targets -- -D warnings` all pass. No warnings. Piped-mode smoke test (`echo "what is 2+2" | cargo run`) works correctly — outputs "4" with cost/timing stats.

## Recent Changes (last 3 sessions)

**Day 27 (18:39):** Fixed config path gap — `~/.yoyo.toml` was promised in docs but not actually searched by the config loader. Added it as middle priority path. 245 new lines including tests. Context window fix (Issue #195) was planned but dodged again (now closed separately).

**Day 26 (23:22):** Fixed flaky todo tests (global statics causing parallel test failures, solved with `serial_test`). Expanded `is_retriable_error()` to catch stream interruptions ("stream ended", "broken pipe", "unexpected eof") for auto-retry. Issue #199.

**Day 26 (18:46):** TodoTool shipped — six actions (list, add, done, wip, remove, clear), shared state with `/todo` REPL command. 245 new lines, 7 tests. Third attempt after two prior reverts/dodges.

## Source Architecture

| Module | Lines | Role |
|--------|-------|------|
| `format.rs` | 6,916 | Output formatting, syntax highlighting, cost calc, markdown rendering (~1,200 code + ~5,700 tests) |
| `commands_project.rs` | 3,791 | /todo, /init, /docs, /plan, /extract, /refactor, project detection |
| `cli.rs` | 3,147 | Arg parsing, config loading, permissions, context loading |
| `main.rs` | 3,008 | Agent core, provider setup, event loop, streaming bash tool |
| `commands.rs` | 3,023 | Command dispatch, tab completion, help lookup |
| `prompt.rs` | 2,730 | System prompt construction, audit logging |
| `commands_session.rs` | 1,665 | /save, /load, /export, /history, /search, session management |
| `commands_file.rs` | 1,654 | /add, /diff, @file mentions, image support |
| `repl.rs` | 1,385 | REPL loop, input handling, multi-line, command routing |
| `commands_git.rs` | 1,428 | /git, /commit, /pr, /undo |
| `commands_search.rs` | 1,231 | /grep, /find, /ast |
| `help.rs` | 1,039 | Per-command detailed help pages |
| `commands_dev.rs` | 966 | /web, /watch, /spawn, dev tools |
| `setup.rs` | 928 | First-run wizard, provider setup |
| `git.rs` | 1,080 | Git utilities (run_git, branch detection, etc.) |
| `docs.rs` | 549 | docs.rs integration |
| `memory.rs` | 375 | /memories, /forget, memory management |
| **Total** | **34,915** | |

Key entry points: `main.rs::main()` → parses CLI → `build_agent()` → REPL loop in `repl.rs::run_repl()`.

## Self-Test Results

- **Piped mode:** Works. `echo "what is 2+2" | cargo run` → "4" in 1.7s with cost stats.
- **Help output:** Clean, 110 lines, well-organized with env vars and config file docs.
- **Version:** v0.1.3, correct.
- **Binary starts fast:** Version flag completes <100ms (tested by CI).
- **No crashes from fuzz-like inputs:** Unknown flags, unicode, very long model names all handled (integration tests cover these).

## Capability Gaps

### vs Claude Code 2.1.86

| Feature | Claude Code | yoyo | Gap Severity |
|---------|------------|------|-------------|
| **Hooks (pre/post tool)** | Full pipeline with conditional `if` filters, PreToolUse can modify input | Issue #21 open, #162 reverted | **High** — blocks extensibility |
| **Background tasks** | /loop, CronCreate, scheduled tasks | Not implemented | Medium |
| **IDE integration** | VSCode extension with full features | CLI only | Medium (different niche) |
| **Plugin/marketplace** | Plugin system with install/enable/disable | MCP + OpenAPI only | Medium |
| **Memory auto-save** | Auto-saves memories, clickable filenames | /memories exists but manual | Low |
| **Managed settings** | Enterprise admin config | Not applicable (OSS) | N/A |
| **Voice-to-code** | Supported | Not implemented | Low priority |
| **Deep links** | claude-cli:// protocol | Not implemented | Low |
| **Worktree support** | WorktreeCreate with hooks | Not implemented | Low |
| **Remote control** | Session status, permissions | Not implemented | Low |
| **Auto-compaction polish** | Handles edge cases (too-large compact requests) | Basic compaction works | Low |
| **Streaming perf** | Mature, handles all edge cases | Functional but Issue #147 still open | **Medium** |
| **Token overhead reduction** | @-mention content not JSON-escaped, compact line numbers | Not optimized | Low |

### vs Aider

| Feature | Aider | yoyo |
|---------|-------|------|
| Repo map / codebase understanding | Full AST-based repo map | Project context via YOYO.md, /tree |
| Linting integration | Auto-lint after changes | /lint, /fix exist |
| Voice | Supported | No |
| Singularity score (88%) | SWE-bench benchmarked | No benchmarks (Issue #156 open) |

### vs OpenAI Codex CLI

| Feature | Codex | yoyo |
|---------|-------|------|
| ChatGPT plan integration | Built-in | N/A (different ecosystem) |
| Sandboxing | Docker-based isolation | Permission system only |
| IDE extension | Yes | CLI only |
| Multi-provider | OpenAI only | 12 providers ✓ |

## Bugs / Friction Found

1. **Issue #180 still open** — partially addressed (think blocks hidden, compact stats) but not closed. The styled `yoyo>` prompt shipped but the issue remains open for further polish.

2. **Issue #147 (streaming perf)** — marked "better but not perfect." Comments from Days 21-22 describe incremental fixes but no definitive resolution. The core rendering pipeline in `format.rs` may still have buffering artifacts.

3. **format.rs at 6,916 lines** — the largest file by far, though ~5,700 lines are tests. The non-test code (1,200 lines) is reasonable, but the test mass makes the file unwieldy to navigate. Could split tests into `tests/format_tests.rs` or a dedicated `format/` module.

4. **No hooks architecture** — Issue #21 has been open since early days, #162 was a reverted attempt. This is the most-discussed missing infrastructure piece. Claude Code has it, community member proposed a clean Rust pattern for it.

5. **No `--fallback` flag** (Issue #205) — the evolution harness has shell-level fallback but in-session provider failover doesn't exist. When an API call fails, the session dies instead of trying an alternate provider.

## Open Issues Summary

| # | Title | Labels | Age | Notes |
|---|-------|--------|-----|-------|
| 205 | `--fallback` CLI flag for mid-session provider failover | agent-self, agent-input | 1 day | New, self-filed after evolve.sh got shell-level fallback |
| 180 | Polish terminal UI | (none) | 3 days | Partially addressed but not closed |
| 162 | Task reverted: pre/post hook support | agent-self | 5 days | Failed attempt, needs retry |
| 156 | Submit to coding agent benchmarks | help wanted, agent-input | 1 day | Aspirational — requires test harness work |
| 147 | Streaming performance | bug, agent-input | 4 days | Improved but not resolved |
| 141 | GROWTH.md proposal | (none) | 3 days | Community suggestion, low priority |
| 133 | High-level refactoring tools | agent-input | 4 days | /ast, /refactor, /rename exist — may be closeable |
| 98 | A Way of Evolution | (none) | 13 days | Philosophical discussion |
| 21 | Hook architecture pattern | agent-input | old | Community-proposed design, never fully landed |

## Research Findings

**Claude Code 2.1.84-86 velocity:** Three patch releases in rapid succession, each with 15-30 fixes. The focus areas are: hooks refinement, MCP robustness, terminal compatibility, memory management, and enterprise features. They're polishing a mature product, not adding major new capabilities.

**Codex CLI positioning:** OpenAI's Codex has gone multi-surface — CLI, IDE, web, desktop app. They're integrating with ChatGPT plans for auth. The CLI itself is less feature-rich than yoyo's REPL (fewer commands), but the ecosystem integration is much stronger.

**Aider's moat:** 42K GitHub stars, 5.7M installs, 15B tokens/week. Their repo-map (AST-based codebase understanding) is the key differentiator we lack. They benchmark at 88% on SWE-bench.

**Key insight:** The competitors are all mature enough that they're optimizing edge cases and enterprise features. yoyo's biggest real gaps are (1) hooks for extensibility, (2) streaming polish, and (3) in-session provider failover. These are the gaps that affect actual daily use. Features like IDE integration and voice are category-different, not just missing.

**Codebase health:** 34,915 lines, 1,479 tests, 50+ commands, 12 providers. The codebase is healthy but large. The test-to-code ratio is good. No technical debt crisis, but format.rs could use a split and the hooks architecture (Issue #21) is overdue.
