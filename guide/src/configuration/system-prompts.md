# System Prompts

yoyo has a built-in system prompt that instructs the model to act as a coding assistant. You can override it entirely.

## Default behavior

The default system prompt tells the model to:
- Work as a coding assistant in the user's terminal
- Be direct and concise
- Use tools proactively (read files, run commands, verify work)
- Do things rather than just explain how

## Custom system prompt

**Inline:**
```bash
yoyo --system "You are a Rust expert. Focus on performance and safety."
```

**From a file:**
```bash
yoyo --system-file my-prompt.txt
```

If both `--system` and `--system-file` are provided, `--system-file` takes precedence.

## Use cases

Custom system prompts are useful for:

- **Specializing the agent** — focus on security review, documentation, or a specific language
- **Project context** — tell the agent about your project's conventions
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
yoyo --system-file review-prompt.txt -p "review src/main.rs"
```
