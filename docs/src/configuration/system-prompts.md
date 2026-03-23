# System Prompts

yoyo has a built-in system prompt that instructs the model to act as a coding assistant. You can override it entirely.

## Default behavior

The default system prompt tells the model to:
- Work as a coding assistant in the user's terminal
- Be direct and concise
- Use tools proactively (read files, run commands, verify work)
- Do things rather than just explain how

## Custom system prompt

### CLI flags

**Inline:**
```bash
yoyo --system "You are a Rust expert. Focus on performance and safety."
```

**From a file:**
```bash
yoyo --system-file my-prompt.txt
```

### Config file

You can set per-project system prompts in `.yoyo.toml` so your team doesn't need CLI flags:

**Inline prompt:**
```toml
system_prompt = "You are a Rust expert. Focus on performance and safety."
```

**From a file (path relative to the config file's directory):**
```toml
system_file = "prompts/agent.txt"
```

### Priority order

When multiple sources provide a system prompt, yoyo uses the highest-priority one:

1. `--system-file` CLI flag (highest)
2. `--system` CLI flag
3. `system_file` in config file
4. `system_prompt` in config file
5. Built-in default (lowest)

The `system_file` path in config is resolved relative to the config file's directory. For `.yoyo.toml` in your project root, that means paths are relative to the project root. For `~/.config/yoyo/config.toml`, paths are relative to `~/.config/yoyo/`.

## Use cases

Custom system prompts are useful for:

- **Specializing the agent** — focus on security review, documentation, or a specific language
- **Project context** — tell the agent about your project's conventions
- **Team consistency** — commit a `.yoyo.toml` with `system_prompt` or `system_file` so every team member gets the same agent behavior
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

Save as `prompts/review.txt` and use via CLI:
```bash
yoyo --system-file prompts/review.txt -p "review src/main.rs"
```

Or set it in `.yoyo.toml` for the whole project:
```toml
system_file = "prompts/review.txt"
```
