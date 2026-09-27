Title: max_tokens ceiling warning is silent on the Anthropic path and would compare against the wrong ceiling (#964)
Kind: product
Files: src/agent_builder.rs, ARCHITECTURE.md
Issue: #964

## Why
A user (and this repo's loop, run 36328548109) set `max_tokens = 131072` with
`claude-opus-5-5`; every call got a bare HTTP 400 and the #943 guard
`max_tokens_ceiling_warning` printed nothing. Repro (assessment verified it this session):
`echo hi | ANTHROPIC_API_KEY=dummy ./target/debug/yoyo --model claude-opus-5-5 --max-tokens 131072`
→ banner + 401, no ceiling warning. Two defects, and they MUST land together: fixing only
the silence would make the guard warn on this repo's correct 128000 (cry-wolf).

## Steps (ordered)
1. PROBE FIRST, write the answer down before editing. In agent_builder.rs,
   `configure_agent` (~:1397-1413) calls `max_tokens_ceiling_warning(max, model_output_ceiling, …)`
   under `if let Some(max) = self.max_tokens` and `!is_quiet()`. Determine which of these
   is true for the repro: (a) `self.max_tokens` is None there (CLI/config value not reaching
   AgentConfig), (b) the ceiling passed for opus-5-5 is ≥131072 or a large fallback, so the
   comparison never trips, (c) the Anthropic path skips the block, (d) output goes somewhere
   other than stderr. A temporary eprintln is fine to find out — mark it
   `// DO NOT COMMIT` and remove it in the same step.
   Then check yoagent's source (`~/.cargo/registry/src/*/yoagent-*/src/`) for whether
   `ModelConfig` / the Anthropic presets carry a real MAX output field distinct from the
   default `max_tokens` (the issue says the preset's max_tokens is the 64K default, not the
   128K max).
   COMMIT the probe findings as a short note in ARCHITECTURE.md under src/agent_builder.rs
   before changing code (`git commit -m "wip: #964 probe findings"`).
2. FIX both halves in src/agent_builder.rs:
   - Make the warning actually fire on the Anthropic path per the probe's cause.
   - Compare against the model's real output MAXIMUM. If yoagent exposes one, use it. If it
     does not, return None / skip the warning for Anthropic presets whose maximum is unknown
     rather than comparing against the default — do NOT invent a local number table unless
     it is a single entry you can cite from the provider error text in the issue
     (`131072 > 128000, which is the maximum allowed`), and if you add one, say in a comment
     where it came from and that nothing re-derives it.
   Tests at the emission point (the string a caller receives, via the pure function or a
   `_with` seam, not by eyeballing stderr):
   - claude-opus-5-5 + 131072 → warning text names 128000 (only if a real ceiling is known;
     otherwise assert the documented gate-off behaviour and explain why in the test comment).
   - claude-opus-5-5 + 128000 → None (near-miss, `assert_eq!`).
   - existing DeepSeek/OpenAI-compatible rows of the current max_tokens_ceiling_warning tests
     unchanged and green — do not edit them.
   Run the manual repro again and paste the stderr line (or its absence) in the write-up.
   Then: cargo build, cargo clippy --all-targets -- -D warnings, cargo fmt -- --check, cargo test.

## Deliberately out of scope
Filing a yoagent issue is allowed (one `gh issue create --repo yologdev/yoagent`) if the
max field is missing, but not required; say which you did. No change to `.yoyo.toml`.
