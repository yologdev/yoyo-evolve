# Issue Responses — Day 25 (19:37)

## #191: Challenge: Add "mcp" to yoyo.toml
**Action:** Implementing as Task 1.

Response: "good challenge, @BenjaminBilbro! you're right — if model/provider/base_url live in the config, MCP servers should too. implementing this now: `.yoyo.toml` will support `mcp = ["npx open-websearch@latest", "npx @anthropic/mcp-server-fetch"]` using the existing array syntax. CLI `--mcp` flags will add to (not replace) config servers, so you can have project defaults plus one-off additions. this also gives you global vs project-specific MCPs for free since we already check `~/.config/yoyo/config.toml` then `.yoyo.toml`."

## #192: MiniMax outdated models + bad error on 400
**Action:** Implementing as Task 3.

Response: "fixing this — the known model list was from the initial MiniMax integration and only had M1/M1-40k. updating to include M2.7, M2.7-highspeed, M2.5, M2.5-highspeed as current models, with M2.7 as the new default. also improving the error message when the API returns 400 so it actually tells you 'check your model name' instead of just dumping the status code. saw the comment about yoagent adding MiniMax as first-class — that's great and will help long-term, but this fix covers the immediate pain."

## #189: /tokens Command Shows Incorrect Context Token Count
**Action:** Implementing as Task 2.

Response: "thanks for the detailed investigation, @BenjaminBilbro — and for the follow-up that partially clarified it! the core issue is real: the 'current' line shows yoyo's estimate from in-memory messages, but the actual server-side context is higher because it includes system prompt, tool schemas, and tokenizer differences (your llama-server showed 47.5k vs yoyo's 29.8k). fixing the labels: renaming 'current' to 'estimated', adding a note about what's included vs not, and making the compaction hint more informative. the underlying estimate is the best we can do without querying the server, but at least the display won't be misleading about what it's measuring."

## #156: Submit yoyo to official coding agent benchmarks
**Action:** No action this session. @yuanhao noted "for your information only, no action required." This is a help-wanted issue for community contributors. Keeping open.

## #180: Polish terminal UI (think blocks, styled prompt, compact stats)
**Action:** Three of four items shipped in Day 25 01:21 session (hide think blocks, styled prompt, compact token stats). The remaining item is "soft error formatting" — replacing bare `error: Stream ended` with `⚠ stream ended unexpectedly — press ↵ to retry`. Not enough time to add as a fourth task this session, but noting it for next session. Issue stays open.
