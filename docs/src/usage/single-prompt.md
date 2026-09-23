# Single-Prompt Mode

Run a single prompt without entering the REPL. yoyo will process the prompt, print the response, and exit.

## Usage

The simplest way is a bare positional prompt — just put your prompt in quotes:

```bash
yoyo "explain this codebase"
yoyo "find all TODO comments"
```

You can also use `--prompt` or `-p` explicitly:

```bash
yoyo --prompt "explain this codebase"
yoyo -p "find all TODO comments"
```

Both forms are equivalent. The bare prompt form is shorter and matches how most CLI tools work.

## When to use it

Single-prompt mode is useful for:

- **Quick questions** — get an answer without starting a session
- **Scripting** — run yoyo as part of a larger workflow
- **CI/CD pipelines** — automate code review or analysis

## Example

```bash
$ yoyo "count the lines of Rust code in this project"
  ▶ $ find . -name '*.rs' | xargs wc -l ✓ (0.1s)

There are 1,475 lines of Rust code across 1 file (src/main.rs).
```

## Slash commands are not expanded here

A prompt whose first non-space character is `/` is treated as a REPL command, not as
text. Single-prompt mode cannot dispatch REPL commands, so it refuses: the message goes
to **stderr**, the exit code is **2**, and **no API call is made** — you are not charged
for a turn that could only have puzzled over the literal string. Piped mode
(`echo "/risk" | yoyo`) refuses the same inputs the same way and with the same exit code,
so a wrapper can rely on one contract across both doors.

The check only looks at the first character, so a prompt that merely *begins* with `/`
is refused even when you meant the text literally (`yoyo -p "/tmp is full"`). Rephrase it
— for example, drop the leading slash or start with a word — and it is sent through as an
ordinary prompt.

## Combining with other flags

You can combine prompts with other flags:

```bash
yoyo "review this diff" --model claude-sonnet-4-20250514
yoyo "explain the architecture" --thinking high
yoyo -p "analyze the code" --system "You are a security auditor."
```
