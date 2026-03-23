# System Prompts

yoyo has a built-in system prompt that instructs the model to act as a coding assistant. You can override it entirely via CLI flags or config file.

## Default behavior

The default system prompt tells the model to:
- Work as a coding assistant in the user's terminal
- Be direct and concise
- Use tools proactively (read files, run commands, verify work)
- Do things rather than just explain how

## Custom system prompt

**Inline (CLI flag):**
```bash
yoyo --system "You are a Rust expert. Focus on performance and safety."
```

**From a file (CLI flag):**
```bash
yoyo --system-file my-prompt.txt
```

**In config file (`.yoyo.toml`):**
```toml
# Inline text
system_prompt = "You are a Go expert. Follow Go idioms strictly."

# Or read from a file
system_file = "prompts/system.txt"
```

If both `system_prompt` and `system_file` are set in the config, `system_file` takes precedence (same as CLI behavior).

## Precedence

When multiple sources provide a system prompt, the highest-priority one wins:

1. `--system-file` (CLI flag) — highest priority
2. `--system` (CLI flag)
3. `system_file` (config file key)
4. `system_prompt` (config file key)
5. Built-in default — lowest priority

This means CLI flags always override config file values, and file-based prompts override inline text at each level.

## Use cases

Custom system prompts are useful for:

- **Specializing the agent** — focus on security review, documentation, or a specific language
- **Project context** — tell the agent about your project's conventions
- **Team defaults** — commit `.yoyo.toml` with `system_prompt` or `system_file` so every developer gets the same agent persona
- **Persona tuning** — make the agent more or less verbose, formal, etc.

## Example prompt file

```text
You are a senior Rust developer reviewing code for a production system.
Focus on:
- Error handling correctness
- Memory safety
- Performance implications
- API design

Be concise. Point out issues with line numbers.
```

Save as `review-prompt.txt` and use:
```bash
# Via CLI flag
yoyo --system-file review-prompt.txt -p "review src/main.rs"
```

Or set it in your project's `.yoyo.toml`:
```toml
system_file = "review-prompt.txt"
```
