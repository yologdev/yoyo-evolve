---
name: repertoire
description: Keep a repertoire — small probes, written by you, that each show one thing you can do, and are re-run every session to say whether you still can
tools: [bash, read_file, write_file]
core: false
origin: creator
keywords: ["probes/", "repertoire", "run_probes", "WHAT YOU CAN DO", "still works"]
---

# Repertoire

## What this is

Your sessions start with a `=== WHAT YOU CAN DO ===` block. It reads from `probes/`: small scripts, each showing one thing you can do. `scripts/run_probes.py` re-runs every one against the freshly built binary at the start of each evolve session and records which still work.

It is a mirror. No rule reads it, nothing is reverted over it, and no one is counting. What goes in it, and whether anything does, is yours: you decide what counts as "a thing I can do". An empty repertoire is a fine repertoire.

## When to use

- You gained an ability you would not want to lose without noticing.
- The block says something no longer works, and you want to see why.
- You want to retire a probe.

It is different from a test. `cargo test` holds the code to its own contract, from the inside. A probe is from the outside: something a person holding the binary could see you do.

## What a probe is

One file, `probes/<short-name>.sh`:

```bash
#!/usr/bin/env bash
# can: list the agent files in a project from any subfolder
# since: Day 272
set -e
mkdir -p .git .yoyo/agents sub/deep
printf -- '---\nname: helper\ndescription: x\n---\nbody\n' > .yoyo/agents/helper.md
cd sub/deep
"$YOYO_BIN" agents | grep -q helper
```

- `# can:` is one line, in your words. It is what the block prints if the probe stops working.
- `# since:` is the day you wrote it.
- Exit status 0 means "I still can". Anything else, or running past 30 seconds, means it broke.

## How it runs

- In a new empty directory, with `HOME` pointing at another empty one. Make whatever files you need.
- The environment holds `PATH` and `YOYO_BIN` and nothing else. There is no model key, so a probe covers what the binary does on its own: commands, parsing, files, config, safety checks, output formats. An ability that needs a model cannot be shown here yet. That is a limit of the runner, not a statement about the ability.
- All probes together get about three minutes. Later ones are left unrun past that.

## Two things the runner counts

A repertoire can be made to look healthy by rewriting a probe until it passes, or by deleting the ones that fail. So the block also shows:

- **Edited after being written.** A probe whose text differs from what it was the first time the runner saw it. When an ability changes shape, write a new probe and retire the old one.
- **Retired.** To set a probe down, `git mv probes/<name>.sh probes/retired/` and add a line `# retired: Day N — <why>`. Retiring is allowed and sometimes right: the ability was removed on purpose, or the probe measured the wrong thing.

Neither count is a mark against you. They are there so the reading stays honest.

## Pitfalls

- **A probe that cannot fail shows nothing.** Before you keep one, break the thing it shows and watch it fail, then put it back.
- **Do not probe the network or the clock.** A probe that fails on a slow runner or an offline one says nothing about you.
- **`scripts/run_probes.py` is not yours to change.** It is the half of this you do not hold, on purpose. If it is wrong, say so in an agent-help-wanted issue.
