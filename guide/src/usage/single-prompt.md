# Single-Prompt Mode

Use `--prompt` or `-p` to run a single prompt without entering the REPL. yoyo will process the prompt, print the response, and exit.

## Usage

```bash
yoyo --prompt "explain this codebase"
yoyo -p "find all TODO comments"
```

## When to use it

Single-prompt mode is useful for:

- **Scripting** — run yoyo as part of a larger workflow
- **Quick questions** — get an answer without starting a session
- **CI/CD pipelines** — automate code review or analysis

## Example

```bash
$ yoyo -p "count the lines of Rust code in this project"
  ▶ $ find . -name '*.rs' | xargs wc -l ✓ (0.1s)

There are 1,475 lines of Rust code across 1 file (src/main.rs).
```

## Combining with other flags

You can combine `-p` with other flags:

```bash
yoyo -p "review this diff" --model claude-sonnet-4-20250514
yoyo -p "explain the architecture" --thinking high
yoyo -p "analyze the code" --system "You are a security auditor."
```
