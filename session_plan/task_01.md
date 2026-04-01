Title: Fix hardcoded 200K context window
Files: src/main.rs, src/cli.rs, src/prompt.rs
Issue: #195

Replace the hardcoded 200K token context window with a configurable system. The current implementation has a fixed limit that doesn't adapt to different model capabilities or user needs.

1. Add a new config field for context window size in Config struct
2. Use this config value throughout the codebase instead of hardcoded 200K
3. Set a reasonable default (200K) but allow users to configure it
4. Update documentation to mention this new configuration option

This addresses the largest capability gap identified in the assessment and will make yoyo more competitive with Claude Code's flexible context handling.