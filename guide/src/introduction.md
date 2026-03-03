# yoyo

**yoyo** is a coding agent that runs in your terminal. It can read and edit files, execute shell commands, search codebases, and manage git workflows — all through natural language.

yoyo is open-source, written in Rust, and built on [yoagent](https://github.com/yologdev/yoagent). It started as ~200 lines and evolves itself one commit at a time.

## What yoyo can do

- **Read and edit files** — view file contents, make surgical edits, or write new files
- **Run shell commands** — execute anything you'd type in a terminal
- **Search codebases** — grep across files with regex support
- **Navigate projects** — list directories, understand project structure
- **Track context** — monitor token usage, auto-compact when the context window fills up
- **Persist sessions** — save and resume conversations across sessions
- **Estimate costs** — see per-turn and session-total cost estimates

## Quick example

```bash
export ANTHROPIC_API_KEY=sk-ant-...
cargo install yoyo  # or: cargo run from source

yoyo
```

Then just talk to it:

```
> read src/main.rs and find any unwrap() calls that could panic
> fix the bug in parse_config and run the tests
> explain what this codebase does
```

## What makes yoyo different

yoyo is not a product — it's a process. It evolves itself in public. Every improvement is a git commit. Every session is journaled. You can read its [source code](https://github.com/yologdev/yoyo-evolve/blob/main/src/main.rs), its [journal](https://github.com/yologdev/yoyo-evolve/blob/main/JOURNAL.md), and its [identity](https://github.com/yologdev/yoyo-evolve/blob/main/IDENTITY.md).

Current version: **v0.1.0**
