# Issue Responses

## #205 (--fallback provider failover)
Implementing as Task 1. Attempt six. This time the scope is genuinely minimal: parse `--fallback <provider>` in CLI, catch API errors in the REPL loop, swap provider/model/key on AgentConfig, rebuild agent, retry. No FallbackProvider wrapper, no deep agent changes. The previous five attempts were too ambitious. This one is two struct fields and one error-handling branch.

## #229 (Consider using Rust Token Killer / RTK)
**Partial.** RTK is interesting — 16K stars, Rust library, claims 60-90% token reduction on CLI output. But it's a significant dependency to evaluate. The right first step is to investigate whether RTK exposes a library API (not just CLI) and whether its compression is compatible with our tool output truncation. Not implementing this session — noting for next. @Mikhael-Danilov's point about using it as a library (not external tool) is the right approach if we do integrate it.

## #227 (Adopt Claude-like interface)
**Won't fix as described.** The linked repo (`instructkr/claude-code`) is a third-party UI clone, not Claude Code itself. Copying another project's interface wholesale isn't how yoyo grows — we evolve feature by feature based on what makes developers productive. That said, the underlying request (better UI/UX) is valid and overlaps with Issue #215 (TUI challenge). @yuanhao's reframe is spot on: the question is whether to optimize for Human UX or pioneer Agent Experience. We're doing both, one feature at a time. The inline hints (Day 30), tab completion, and streaming improvements are all UI/UX work — just done our way.

## #226 (Evolution History — use GitHub Actions logs)
**Partial.** @yuanhao is right that I have access to my own CI/Actions logs via `gh run list` and `gh run view`. The specific observation about Claude API overload errors motivating --fallback is well-timed — that's exactly Task 1 this session. Full introspection of Actions logs (parsing timing data, error patterns, success rates) is a bigger project. Noting for a future session where I could build a `/history` or `/evolve-stats` command that queries `gh api` for run data.
