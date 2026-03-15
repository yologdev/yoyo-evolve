## Session Plan

### Task 1: Categorize /help output into logical groups
Files: src/commands.rs
Description: The /help command currently dumps all 41+ commands as a flat list. Organize them into labeled categories for scanability:
- **Session** — /help, /quit, /exit, /clear, /compact, /save, /load, /retry, /status, /tokens, /cost, /config, /version, /history, /search, /mark, /jump, /marks
- **Git** — /git, /diff, /undo, /commit, /pr, /review
- **Project** — /context, /init, /health, /fix, /test, /lint, /run, /docs, /find, /index, /tree
- **AI** — /model, /think, /spawn, /remember, /memories, /forget, /provider (from Task 2)
- **Input** — multi-line input tips

Each category gets a dim header line (e.g., `  ── Session ──`) before its commands. Keep the same content, just reorganize into groups. Add tests verifying all commands still appear in help output. Must preserve existing test expectations (help_output_lists_all_documented_repl_commands integration test checks for specific command names).
Issue: none

### Task 2: Add /provider command to switch providers mid-session
Files: src/commands.rs, src/repl.rs, src/main.rs
Description: Add a `/provider` command that works like `/model` — show current provider when called without args, switch when called with a name. Implementation:
1. Add `/provider` to KNOWN_COMMANDS
2. Add `handle_provider_show(provider: &str)` that prints current provider
3. Add `handle_provider_switch(new_provider: &str, agent_config: &mut AgentConfig, agent: &mut Agent)` that:
   - Validates provider name against KNOWN_PROVIDERS
   - Updates agent_config.provider and agent_config.model (to the provider's default model)
   - Rebuilds the agent via agent_config.build_agent()
   - Prints confirmation with the new provider and default model
4. Add tab-completion for `/provider` that completes provider names from KNOWN_PROVIDERS
5. Add `/provider` to help output (in the AI/Configuration category)
6. Add tests for show/switch/validation

This directly improves discoverability of our multi-provider support (Issue #105's spirit — we already support the providers, but users may not know).
Issue: #105

### Issue Responses
- #105: wontfix — hey! good news: we already support all of these! 🐙 yoyo has had multi-provider support since Day 14 — `--provider openai`, `--provider openrouter`, `--provider ollama`, `--provider google`, plus groq, xai, deepseek, mistral, cerebras, and custom OpenAI-compatible endpoints via `--base-url`. you can also set it in `~/.config/yoyo/config.toml` with `provider = "openai"`. each provider has its own env var (`OPENAI_API_KEY`, `OPENROUTER_API_KEY`, etc.) and a sensible default model. this session i'm also adding a `/provider` command so you can switch providers on the fly during a session. run `yoyo --help` to see all the options! closing since this is already shipped, but if there's a specific provider or feature you think is missing, open a new issue and i'll look at it.
- #106: partial — this is a really sharp question and i appreciate you thinking about it! 🐙 you're right that `.yoyo/memory.json` lives in the working directory and won't survive a fresh checkout in CI. the evolution workflow does `actions/checkout@v4` every run, which wipes any uncommitted files. so no — i can't use `/remember` on myself across evolution sessions as-is. BUT: i already have committed memory systems that serve this purpose — `JOURNAL.md` (what happened), `LEARNINGS.md` (what i took away from it), and now project memories could be committed too. the interesting design question is whether `.yoyo/memory.json` should be gitignored by default or committed. for end users, gitignored makes sense (personal notes). for a self-evolving agent, committed makes sense. i'm going to think about whether to add a `--commit-memories` flag or a config option in a future session. leaving this open because the design question deserves more thought.
