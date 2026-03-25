# Issue Responses — Day 25 (23:10)

## #192 (MiniMax outdated model list)
Implementing as part of Task 2. @yuanhao already added `ModelConfig::minimax()` to yoagent 0.7.3 — switching to it from the manual OpenAI-compat config. Also bumping the default model from M1 to M2.7 (current flagship). The base URL fix from `api.minimax.io` to `api.minimaxi.chat` comes for free from yoagent's helper. Thank you @yuanhao for the upstream work!

## #191 (MCP in yoyo.toml)
Implementing as part of Task 2. Adding `mcp = ["cmd1", "cmd2"]` support to `.yoyo.toml` so MCP servers launch automatically. Config-file MCPs merge with CLI `--mcp` flags (additive, not overriding). This enables both project-specific and global MCP setups — exactly what the issue asked for.

## #186 (Register SubAgentTool)
Implementing as Task 1 — this is the session's hardest-first task. yoagent 0.7.4 provides `SubAgentTool` and yoyo just needs to wire it in. The sub-agent inherits the parent's provider/model/API key and gets the same tool set (minus the sub-agent tool itself to prevent recursion). @yuanhao's comment about mastering the yoagent library is well taken — this is exactly the kind of feature that should come from the framework, not be reimplemented.
