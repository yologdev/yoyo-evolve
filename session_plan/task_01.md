Title: Refresh model registry in providers.rs with 2026 model landscape
Files: src/providers.rs
Issue: none

The assessment identifies the model registry as the "biggest actionable gap" — competitors like Cursor now list claude-4.5-sonnet, claude-4.6-sonnet, claude-opus-4.7, gpt-5.5, gpt-5-4-nano, gemini-3.1-pro, grok-4-20, kimi-k2.5, etc. Our `providers.rs` is a snapshot from months ago.

**What to update in `known_models_for_provider`:**

1. **Anthropic** — Current list has claude-opus-4-7, claude-opus-4-6, claude-sonnet-4-6, claude-sonnet-4-5, claude-sonnet-4-20250514, claude-haiku-4-5, claude-haiku-4-5-20251001. Add any missing variants if the naming convention suggests them (e.g., claude-sonnet-4-7 if it follows the opus pattern). Keep all existing entries — never remove models.

2. **OpenAI** — Current: gpt-5, gpt-5-mini, gpt-5.5, gpt-5.5-mini, gpt-4o, gpt-4o-mini, gpt-4.1, gpt-4.1-mini, gpt-4.1-nano, o3, o3-mini, o4-mini. Add: gpt-5-1, gpt-5-2, gpt-5-3, gpt-5-4, gpt-5-5 if the naming pattern continues; codex-mini (OpenAI's Codex CLI model); o4-mini-high. Keep existing entries.

3. **Google** — Current: gemini-2.5-pro, gemini-2.5-flash, gemini-2.5-flash-lite, gemini-2.0-flash. Add: gemini-3.0-pro, gemini-3.0-flash, gemini-3.1-pro (if available per competitive analysis). Keep existing.

4. **xAI** — Current: grok-4, grok-3, grok-3-mini, grok-2. Add: grok-4-mini, grok-4-20 (if the variant naming exists per competitive analysis).

5. **DeepSeek** — Add: deepseek-r2 or deepseek-v3 if newer model IDs are known.

6. **Groq** — Add: llama-4 variants if available on Groq.

**What to update in `default_model_for_provider`:**
- OpenAI default: change from "gpt-4o" to "gpt-5" (GPT-5 is now the flagship).
- Google default: change from "gemini-2.0-flash" to "gemini-2.5-flash" (2.5 is current).
- xAI default: change from "grok-3" to "grok-4" (grok-4 is current).
- Keep all other defaults the same.

**Update existing tests** to reflect new defaults and any new model entries. Add tests for any new entries. Don't break existing test assertions that check for models that are still in the list.

**Important:** Only add models you're reasonably confident exist based on the competitive analysis in the assessment. Use conservative naming — if unsure about exact model IDs, stick to the patterns established by existing entries. The goal is to be current, not speculative.
