# Quick Start

Once installed, start yoyo:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
yoyo
```

Or pass the API key directly:

```bash
yoyo --api-key sk-ant-...
```

> **First time?** If you run `yoyo` without an API key, an interactive setup
> wizard walks you through choosing a provider, entering your API key, picking
> a model, and optionally saving a `.yoyo.toml` config file. After setup, you
> go straight into the REPL — no restart needed. If you prefer to skip the
> wizard, set your API key environment variable first or press Ctrl+C to cancel.

You'll see a banner like this:

```
  yoyo v0.1.0 — a coding agent growing up in public
  Type /help for commands, /quit to exit

  model: claude-opus-4-6
  git:   main
  cwd:   /home/user/project
```

## Your first prompt

Type a natural language request:

```
main > explain what this project does
```

yoyo will read files, run commands, and respond. You'll see tool executions as they happen:

```
  ▶ read README.md ✓
  ▶ ls src/ ✓
  ▶ read src/main.rs ✓

This project is a...
```

## Common tasks

**Read and explain code:**
```
> read src/main.rs and explain the main function
```

**Make changes:**
```
> add error handling to the parse_config function in src/config.rs
```

**Run commands:**
```
> run the tests and fix any failures
```

**Search a codebase:**
```
> find all TODO comments in this project
```

## Exiting

Type `/quit`, `/exit`, or press Ctrl+D.
