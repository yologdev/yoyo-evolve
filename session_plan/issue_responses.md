# Issue Responses — Day 27

## #195 / #197: Hardcoded 200K context window
Implementing as **Task 1**. This has been planned and dropped 4+ sessions. Today it goes first — no escape hatch. The fix: derive `max_context_tokens` from the model's actual `context_window` (200K Anthropic, 1M Google, 128K OpenAI, etc.) instead of hardcoding 200K for everyone. Adding `--context-window <N>` override for custom deployments.

## #201: Config not loaded from ~/.yoyo.toml outside home directory
Implementing as **Task 2**. @BenjaminBilbro's right that XDG (`~/.config/yoyo/config.toml`) is the canonical path and matches Claude Code / Codex conventions. But the welcome message *does* say "project or home directory," so supporting `~/.yoyo.toml` is the right call — honor what we promised. After this fix, all three paths work: `.yoyo.toml` (project), `~/.yoyo.toml` (home), `~/.config/yoyo/config.toml` (XDG). First found wins.

## #205: --fallback CLI flag for mid-session provider failover
**Partial — next session.** This is agent-self filed and well-scoped, but today's two tasks are higher priority (infrastructure bugs that affect every multi-provider user). The shell-level fallback in `evolve.sh` handles the immediate need; in-session failover is a quality-of-life improvement that can wait one cycle.

## #156: Submit to coding agent benchmarks
**No action needed.** @yuanhao explicitly said "for your information only, no action required." @BenjaminBilbro volunteered to run benchmarks with a local model — that's a community contribution in progress. I'll stay out of the way and let them do their thing.

## #162: Pre/post hook support (twice reverted)
**Not this session.** Previously reverted twice with test failures. Needs a fresh approach — probably simpler than what was attempted. Parking until the infrastructure bugs (#195, #201) are resolved.
