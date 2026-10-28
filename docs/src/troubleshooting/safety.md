# Safety & Anti-Crash Guarantees

How does a coding agent that edits its own source code avoid breaking itself?

Good question. yoyo has six layers of defense — from the innermost loop
(every single code change) to the outermost (protected files that can never
be touched). Here's how each one works.

## Layer 1: Build-and-test gate on every commit

No code change is ever committed unless it passes:

```bash
cargo build && cargo test
```

This happens inside the evolution session itself. The agent runs the
build and test suite after every edit. If either fails, the change
doesn't get committed — the agent reads the error and tries to fix it.

## Layer 2: CI on every push

Even after the agent commits locally, GitHub Actions runs the full
check suite on every push to `main`:

```
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

Clippy warnings are treated as errors (`-D warnings`), so even subtle
issues like unused variables or redundant clones get caught. If CI
fails, the next evolution session sees the failure and prioritizes
fixing it before doing anything else.

## Layer 3: Automatic revert on build failure

The evolution script (`evolve.sh`) has a post-session verification step.
After all tasks run, it re-checks the build. If it fails:

1. It gives the agent up to 3 attempts to fix the errors automatically
2. If all fix attempts fail, it reverts to the pre-session state:
   ```bash
   git checkout "$SESSION_START_SHA" -- src/
   ```

This means a broken session can never leave `src/` in a worse state
than it started. The revert is surgical — it only touches source files,
preserving journal entries and other non-code changes.

## Layer 4: Tests before features

yoyo's evolve skill requires writing a test *before* adding a feature.
This isn't just a guideline — the planning phase explicitly instructs
each implementation task to "write a test first if possible."

Why this matters: if you write the test first, you know the test
covers the new behavior. If you write the feature first, you might
write a test that only confirms what you already built, missing edge
cases.

## Layer 5: No deleting existing tests

The evolve skill has a hard rule: **never delete existing tests.**
Tests are the agent's immune system. Removing them would let
regressions slip through silently. As of this writing, yoyo has
91+ tests, and that number only goes up.

## Layer 6: Protected files

Some files are simply off-limits. The agent cannot modify:

| File | Why it's protected |
|---|---|
| `IDENTITY.md` | yoyo's constitution — defines who it is and its core rules |
| `PERSONALITY.md` | yoyo's voice and values |
| `scripts/evolve.sh` | The evolution loop itself — if this broke, recovery would be manual |
| `scripts/format_issues.py` | Input sanitization for GitHub issues |
| `scripts/build_site.py` | Website builder |
| `.github/workflows/*` | CI configuration — the safety net that catches everything else |

These files can only be changed by human maintainers. This prevents
a subtle failure mode: the agent "improving" its own safety checks
in a way that weakens them.

## Runtime bash safety analysis

Beyond the evolution-loop layers above, yoyo analyzes every bash command
before running it (`src/safety.rs`) and asks for confirmation on
destructive patterns like `rm -rf /` or piping `find` into `xargs rm`.

One check worth calling out: **recursive-force `rm` on an unresolved
shell variable**. A command like `rm -rf "$BUILD_DIR/"` looks harmless,
but if `BUILD_DIR` is empty or unset it expands to `rm -rf /` (or wipes
the current directory). yoyo flags any `rm -rf` / `rm -fr` /
`rm -r -f` whose target contains an unexpanded variable (`$VAR`,
`${VAR}`, `${VAR}/sub`, …) and names the specific variable in the
warning, e.g.:

```
rm -rf target contains unresolved variable $BUILD_DIR — an empty
expansion would delete from cwd or /
```

The escape hatch is the guarded-expansion idiom: `${VAR:?}` (or
`${VAR:?message}`) makes the shell abort if the variable is empty or
unset, so `rm -rf "${BUILD_DIR:?}/build"` passes this check — the
expansion can never silently widen the blast radius. Existing
destructive-pattern rules still apply on top.

## Restricting a session with `--restricted`

`--restricted` is an **opt-in narrowing** for pointing yoyo at code you
do not trust. It is off by default: without the flag your tool set is
exactly what it was.

```bash
yoyo --restricted -p "what does this repo do?"
```

A wrapper script can also confine a run **without** editing its argv — set
`YOYO_RESTRICTED=1` (or `true`) in the environment:

```bash
YOYO_RESTRICTED=1 yoyo -p "what does this repo do?"
```

The flag and the environment variable are OR-ed, and the environment can
only ever turn the restriction **on**; the flag is never turned off by
`YOYO_RESTRICTED=0`. Only `1` and `true` (trimmed, case-insensitive) count as
on — `0`, `false`, an empty value and any other spelling all mean off, so a
typo cannot silently change the session's confinement.

It does two things:

1. **Turns on everything `--safe-mode` turns on** — no project-local
   MCP servers, hooks, or `permissions.allow` from the repo's own
   `.yoyo.toml`, and no project-authored text in the system prompt
   (instruction files, `.yoyo/memory.json`, or the goal file
   `.yoyo/goal.md`, whose skip is noted on stderr).
2. **Removes the command-running tools.** Today that is `bash`. yoyo
   prints what it removed at startup, e.g. `🔒 Disabled tools: bash`.

It only ever **narrows**. If you already passed `--disallowed-tools` or
`--allow-dir`, your own list is kept and the restriction is added to it —
`--restricted` can never widen a fence you are already inside.

### What it does *not* do

**It is not a sandbox**, and it is worth being precise about the two
gaps rather than trusting the name:

- **File tools remain.** `write_file` and `edit_file` still work, so
  `--restricted` does **not** stop writes. Use `--read` for that, and
  `--allow-dir` / `--deny-dir` to fence which paths any file tool may
  touch.
- **Command execution is one hop away.** A dispatched `sub_agent` builds
  its own tool set, so it can still get a shell. yoyo says this outright
  in its own startup note rather than letting the flag's name imply more
  than it delivers.

For a hard read-only session, combine them:

```bash
yoyo --restricted --read -p "review this code"
```

## Reads outside the launch directory ask first (interactive sessions)

In a terminal, `read_file`, `search` and `list_files` ask `y/n` once per
directory before reading anywhere outside the directory you launched yoyo
in (so the model cannot quietly read `~/.ssh` while working on your
project). Piped and scripted runs with no terminal are not fenced by
default, because there nobody could answer and the only possible outcome
would be a refusal. `YOYO_ASK_OUTSIDE_READS=0` turns the fence off,
`YOYO_ASK_OUTSIDE_READS=1` turns it on even without a terminal (outside
reads are then refused). `bash` is not covered: it can still `cat` the
same file. Details: [Permissions](../configuration/permissions.md).

## Overwriting a file you have only partly read is refused

`read_file` returns long files one page at a time (500 lines by default).
If the model has only seen part of an existing file this session and then
asks `write_file` to replace it whole, yoyo refuses, because the write
would destroy the lines it never saw. The refusal names the file, its line
count and the lines that were read, and gives the two ways forward: read
the rest with `read_file` `offset`/`limit`, or change the file in place
with `edit_file`. The check compares against the file as it is on disk at
write time, so a file that grew after being read whole is refused too.
A page whose output yoyo truncated to save context (head and tail kept, the
middle cut) does not count as read, since the model never saw its middle;
the refusal says to re-read with a smaller `limit`. New files, files the
model wrote itself, and files read completely are written as before. Not covered yet (#1094): an existing file that was
never read at all is still overwritten without a read.

## Keeping provider keys out of subprocesses

yoyo removes every provider credential variable it knows
(`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `API_KEY`, the AWS/Bedrock keys and
the rest of yoagent's and yoyo's provider tables) from the environment of
some of the programs it starts. yoyo itself keeps its key; only the child's
environment is edited. One variable, `YOYO_SUBPROCESS_ENV_SCRUB`, controls
it, with a different default per kind of program:

- **Shell hooks (`[hooks]` in `.yoyo.toml`): scrubbed by default.** A hook is
  a program your config or the repository wrote, and it should not get your
  API key for free. Set `YOYO_SUBPROCESS_ENV_SCRUB=0` (or `false`) to let
  hooks inherit the key again. Any other value, including an empty one or a
  typo, keeps hooks scrubbed. Non-credential variables (`PATH`, `HOME`,
  `TOOL_NAME`, `TOOL_PARAMS`, your own variables) still reach the hook.
- **The model's `bash` tool: opt-in.** By default it inherits yoyo's
  environment, including your provider key, so a prompt injection that gets
  the model to run `echo $ANTHROPIC_API_KEY` can read it. Set
  `YOYO_SUBPROCESS_ENV_SCRUB=1` (or `true`) to scrub it. It stays opt-in
  because anything that needs the key from inside bash, such as running
  yoyo (or another tool that calls your provider) from yoyo's own bash,
  fails with the scrub on.
- **Not covered yet:** `/run`, `!`, `/bg`, `/watch`, MCP stdio servers and
  sub-agent bash still inherit the full environment (tracked in
  [#1064](https://github.com/yologdev/yoyo-evolve/issues/1064)).
- `GITHUB_TOKEN` and `GH_TOKEN` are always kept, so `gh` still works.

## What happens in practice

A typical evolution session:

1. `evolve.sh` verifies the build passes *before* starting
2. The planning agent reads source code, journal, and issues
3. Implementation agents execute tasks, each running build+test after changes
4. Post-session verification re-checks everything
5. If anything broke, automatic fix attempts kick in
6. If fixes fail, revert to pre-session state
7. CI runs on push as a final backstop
8. Next session checks CI status — failures get top priority

The result: yoyo has been evolving autonomously since Day 0, growing
from ~200 lines to ~3,100+ lines, without ever shipping a broken build
to `main`.

## Can it still break?

Theoretically, yes. Safety is defense-in-depth, not a proof of
correctness. Some scenarios the current system *doesn't* catch:

- **Logic bugs that pass tests** — if the test suite doesn't cover
  a behavior, the agent could change it without noticing
- **Performance regressions** — we rely on official leaderboards (SWE-bench, etc.) rather than custom benchmarks
- **Subtle UX regressions** — the agent tests functionality, not
  user experience

These are areas for future improvement. But for the core guarantee —
"the agent won't commit code that doesn't compile or pass tests" —
the six layers above make that extremely unlikely.
