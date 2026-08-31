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

> ⚠️ **Use with caution.** This gives yoyo unrestricted access to your shell and filesystem.

## Command Filtering: `--allow` and `--deny`

For finer control over which bash commands run automatically, use glob patterns:

```bash
yoyo --allow "git *" --allow "cargo *" --deny "rm -rf *"
```

### How it works

1. **Deny is checked first.** If a command matches any `--deny` pattern, it's rejected immediately — the agent sees an error message and must try something else.
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

**Trailing wildcards are unchanged.** `cargo *`, `npm run *` and `git commit -m *` are honest
"anything goes" patterns: nothing follows the `*`, so this rule does not apply and
`cargo test --lib` auto-approves exactly as before.

**`deny` is unchanged too**, and deliberately so: narrowing an allow pattern removes
privilege, but narrowing a deny pattern would make a fence stop matching. `deny = ["git * main"]`
still blocks `git -c x=y push main`.

This closes the wildcard-swallows-options case. It is not a guarantee that an allow pattern
is safe against every possible command shape — prefer patterns that are as literal as you can
stand.

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

This affects `read_file`, `write_file`, `edit_file`, `list_files`, and `search`.

### Rules

- If **`--allow-dir`** is set, *only* paths under allowed directories are accessible. Everything else is blocked.
- If **`--deny-dir`** is set, paths under denied directories are blocked.
- **Deny overrides allow** — if a path is under both an allowed and a denied directory, it's blocked.
- Paths are resolved to absolute paths before checking, so `../` traversal escapes are caught.
- Symlinks are resolved via `canonicalize` when the path exists.

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
