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

## Viewing the assembled prompt

To see the full system prompt (including project context, repo map, skills, and any overrides), use:

```bash
yoyo --print-system-prompt
```

This prints the complete prompt to stdout and exits — useful for debugging or understanding exactly what context the model receives. It works with other flags:

```bash
# See what the prompt looks like with a custom system prompt
yoyo --system "You are a Rust expert" --print-system-prompt

# See the prompt without project context
yoyo --no-project-context --print-system-prompt
```

## Automatic project context

In addition to the system prompt, yoyo automatically injects project context when available:

- **Project instructions** — from `YOYO.md` (primary), `CLAUDE.md` (compatibility alias), or `.yoyo/instructions.md`
- **Project file listing** — from `git ls-files` (up to 200 files)
- **Recently changed files** — from `git log` (up to 20 files)
- **Git status** — current branch, count of uncommitted and staged changes
- **Project memories** — from `memory/` files if present

Use `/context` to see which project context files are loaded.

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
