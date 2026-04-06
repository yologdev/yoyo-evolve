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

### Precedence

CLI flags override config file values:
- If you pass any `--allow` or `--deny` flag, the entire `[permissions]` section from the config file is ignored.
- If you pass any `--allow-dir` or `--deny-dir` flag, the entire `[directories]` section from the config file is ignored.
- `--yes` / `-y` overrides everything — all tools are auto-approved regardless of permission patterns.

Config file search order (first found wins):
1. `.yoyo.toml` in the current directory
2. `~/.yoyo.toml` in your home directory
3. `~/.config/yoyo/config.toml`

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

## Summary

| Mechanism | Scope | Effect |
|---|---|---|
| Default prompts | All modifying tools | Ask `[y/N]` before each call |
| `--yes` / `-y` | Everything | Auto-approve all tools |
| `--allow <pattern>` | Bash commands | Auto-approve matching commands |
| `--deny <pattern>` | Bash commands | Auto-reject matching commands |
| `--allow-dir <dir>` | File tools | Only allow paths under these dirs |
| `--deny-dir <dir>` | File tools | Block paths under these dirs |
| `[permissions]` in config | Bash commands | Same as `--allow`/`--deny` |
| `[directories]` in config | File tools | Same as `--allow-dir`/`--deny-dir` |

> **Tip:** Use `/permissions` during a session to see the full security posture — auto-approve status, command patterns, and directory restrictions all in one view.
