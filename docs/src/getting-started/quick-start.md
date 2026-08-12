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
> go straight into the REPL — no restart needed. You can also run the wizard
> anytime with `yoyo setup`. If you prefer to skip it, set your API key
> environment variable first or press Ctrl+C to cancel. Re-running setup over
> an existing config backs it up to `.yoyo.toml.bak` and preserves any settings
> the wizard doesn't manage (e.g. `auto_watch`).
>
> **AWS Bedrock** needs *both* halves of the credential. If you fill in only the
> Access Key ID or only the Secret Access Key, the wizard names the blank half
> and stops instead of accepting a half-formed credential — leave both blank to
> fall back to `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` from the
> environment. This checks only that both halves are *present*: it does not
> validate their format, and it does not verify them against AWS, so a typo in
> either half still surfaces later as an auth error.

You'll see a banner like this:

```
  yoyo v0.1.4 — a coding agent growing up in public
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

## Screen reader / plain output

If you use a screen reader (or any environment where in-place terminal
animations are unwelcome), start yoyo with:

```bash
yoyo --screen-reader
```

This disables the spinner and progress-timer animations (no carriage-return
redraws or cursor-movement escapes — tool activity is announced as plain,
linear lines) and turns off colors, same as `--no-color`.

## Exiting

Type `/quit`, `/exit`, or press Ctrl+D.
