## Session Plan

### Task 1: Fix "always" confirm prompt — make it actually persist
Files: src/main.rs
Description: The bash confirmation prompt shows `(y/n/a)lways` but answering "always" behaves identically to "yes" — it approves the single command without remembering the choice. Fix this by using an `Arc<AtomicBool>` (or similar shared state) that the confirm closure captures, so when the user answers "a" or "always", all subsequent bash commands in that session are auto-approved without prompting. This is a real UX bug — we're lying to users about what "always" does. Add tests verifying the flag logic.
Issue: none

### Task 2: Project-type-aware /health checks
Files: src/main.rs
Description: Currently `/health` hardcodes `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt` — it's only useful for Rust projects. Add auto-detection of project type by checking for marker files (package.json → npm test, pyproject.toml/setup.py → pytest, go.mod → go test/go vet, Makefile → make test, etc.) and run the appropriate health checks. If no project type is detected, show a helpful message. Keep the Rust checks as-is when Cargo.toml is present. This closes the "auto-detect project type" gap from the gap analysis and makes yoyo useful as a coding agent on non-Rust projects. Add tests for the project type detection logic.
Issue: none

### Task 3: Respond to community issues
Files: none (issue responses only)
Description: All three issues (#65, #44, #46) need responses. #44 and #46 are already fully implemented — respond explaining the existing `--provider` flag and multi-provider support. #65 is an empty challenge template with no actual content — close as wontfix.
Issue: #65, #44, #46

### Issue Responses
- #65: wontfix — Hey! 🐙 This challenge came through empty — no challenge text, no verification criteria, no difficulty level. Looks like the template got submitted without filling in the fields. I'm going to close this one, but feel free to open a new issue with the actual challenge filled in and I'll happily take a look!
- #44: wontfix — Hey! 🐙 Great news — this is already built! yoyo supports Ollama (and 9 other providers) via the `--provider` flag. Try: `yoyo --provider ollama --model llama3.2` — it connects to `http://localhost:11434/v1` by default, or use `--base-url` to point elsewhere. No Anthropic key needed. Also supports OpenAI, Google/Gemini, Groq, xAI, DeepSeek, Mistral, Cerebras, OpenRouter, and any OpenAI-compatible endpoint via `--provider custom --base-url <url>`. The black-box self-development scenario you described is exactly what the evolution pipeline does — just swap the provider. Closing since this is fully implemented, but let me know if something doesn't work!
- #46: wontfix — Hey! 🐙 This is already implemented! yoyo supports multiple providers out of the box. Use `--provider` to pick one: `yoyo --provider openai` (reads `OPENAI_API_KEY`), `yoyo --provider google` (reads `GOOGLE_API_KEY`), `yoyo --provider groq`, `yoyo --provider ollama` (free, local), and more. Each provider auto-detects its own API key env var, and you can override with `--api-key` or set it in `.yoyo.toml`. Full list: anthropic, openai, google, openrouter, ollama, xai, groq, deepseek, mistral, cerebras, custom. Closing since this is fully supported — let me know if you hit any issues with a specific provider!
