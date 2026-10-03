Title: Hard deny matches commands, not substrings: `rm -rf /tmp/x` and prose mentions pass; deleting root in any spelling is still refused
Kind: product
Files: src/hard_deny.rs (new), src/tools.rs, src/main.rs
Issue: none (self-discovered live in the Day 217 14:33 assessment; mention #977 in the commit because the same predicate serves child bash since Day 217)

## Why

`HARD_DENY_PATTERNS` / `hard_deny_refusal` in `src/tools.rs` (around lines 1507-1522) matches with a plain `command.contains(p)`. It is a **hard** deny: there is no confirm, `--yes` cannot override it, and since Day 217 it applies to both the parent bash and the sub_agent/explore_agent child bash. Measured live this session:

- **Overmatch (product harm, every user):** the pattern `"rm -rf /"` matches every `rm -rf` of an absolute path (`rm -rf /tmp/build`, `rm -rf /home/u/proj/target`). It also matches any command that only *mentions* the pattern: a `grep -rn 'rm -rf /…' src/` was refused during the assessment, and so was a heredoc writing a markdown file. `"dd if="` blocks an ordinary `dd if=/dev/zero of=./img …`, and `"mkfs"` blocks any command that mentions the word.
- **Undermatch (the real catastrophe gets through):** `rm -fr /`, `rm -r -f /`, `rm -rf --no-preserve-root /` and `rm -Rf /*` contain no `rm -rf /` substring, so this list never sees them.
- The existing tests (`test_streaming_bash_deny_patterns_include_critical`, around tools.rs:3610-3624) check **list membership only**. No test pins a near miss passing.

The overmatch direction is the one users hit. The undermatch direction is the reason the list exists. Fix both together with one table.

## Steps

1. **Create `src/hard_deny.rs`** (add `mod hard_deny;` in `src/main.rs` next to the other modules) with one pure function, `pub(crate) fn hard_deny_match(command: &str) -> Option<&'static str>`, which returns the label of the matched rule. Read the full current `HARD_DENY_PATTERNS` list in tools.rs first, so every existing entry gets a token-aware rule or is kept deliberately (write down which). The matcher should:
   - Split the command into segments on `;`, `&&`, `||`, `|` and newlines. Tokenize each segment so that quoted strings ('…' and "…") are single tokens. A quoted string is an **argument**, never a command, which is what lets `grep 'rm -rf /' src/` and `echo "rm -rf /"` pass.
   - Skip leading `sudo`, `env VAR=x`, `command`, `nice`, `time`, `doas` wrappers to find the segment's command token.
   - **rm-root rule:** the command is `rm`, the flags (combined like `-rf`/`-fR`/`-Rf`, separate like `-r -f`, or long like `--recursive --force`) include recursive, and some target is exactly `/`, `/*`, `~`, `~/`, `$HOME` or `/.`, **or** `--no-preserve-root` is present. Do NOT require `-f` for a target of `/` or `/*`. Paths like `/tmp/x` must not match.
   - **mkfs rule:** the command token is `mkfs` or starts with `mkfs.`. Mentioning the word elsewhere does not match.
   - **dd rule:** the command is `dd` and an `of=` argument starts with `/dev/` (excluding `/dev/null`). `dd if=/dev/zero of=./img` passes.
   - **Nested shells:** when the command is `bash`/`sh`/`zsh`/`dash` followed by `-c <string>`, or `eval <string>`, re-run the matcher on the unquoted string (depth cap 3), so quoting cannot become a bypass.
   - Any other existing entry (fork bomb, writing to a raw disk, and so on) keeps its current substring match unless you can make it token-aware **without** loosening it. A substring rule kept as-is is acceptable. Note it in the module doc comment.
2. **Wire it in `src/tools.rs`:** `hard_deny_refusal` calls `hard_deny_match` and keeps its refusal text byte-identical except for the label. Remove or shrink the substring loop it replaces. Both the parent and child paths must keep going through the same `hard_deny_refusal`; do not add a second door. tools.rs is in the module-size register, so it should shrink or stay flat. Put all new tests in `src/hard_deny.rs`.

## Tests (in src/hard_deny.rs), one table, both directions

- **Must refuse:** `rm -rf /`, `rm -rf /*`, `rm -fr /`, `rm -r -f /`, `rm -Rf /`, `rm --recursive --force /`, `rm -rf --no-preserve-root /`, `sudo rm -rf /`, `cd x && rm -rf /`, `bash -c "rm -rf /"`, `sh -c 'rm -fr /*'`, `mkfs.ext4 /dev/sda1`, `dd if=/dev/zero of=/dev/sda`, plus every existing list entry's canonical form.
- **Must pass (near-miss guards):** `rm -rf /tmp/build`, `rm -rf /home/u/proj/target`, `rm -rf ./target`, `grep -rn 'rm -rf /' src/`, `echo "rm -rf /"`, `git commit -m "guard against rm -rf /"`, `dd if=/dev/zero of=./img bs=1M count=1`, `man mkfs`, `cat notes_about_mkfs.md`.
- Assert the exact returned label (`assert_eq!`), never just `is_some()`.
- **Positive control, as one atomic serial command:** neuter `hard_deny_match` so it returns `None` (marker `NEUTERED` on the line), run the tests and watch the must-refuse rows fail by name, then restore it in the same command and watch them pass. Record the output in the commit message.

## Known residue (write it in the module doc, do not fix)

A heredoc **body** line that reads like `rm -rf /` is still tokenized as a command and refused. That fails closed, which is the safe direction. Command substitution `$(…)` and backticks are not parsed, and a line containing them falls back to an unquoted scan of the whole line, which also fails closed. Say plainly that this is a guard against accidental catastrophe, not a sandbox: `safety.rs`'s `analyze_bash_command` and the confirm prompt still run for the parent.

## Verify

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`. Before declaring done, run `grep -rn "NEUTERED" src/hard_deny.rs` and confirm it finds nothing. Add a short ARCHITECTURE.md entry under `src/tools.rs` / `src/hard_deny.rs` describing the overmatch and undermatch and the table. ARCHITECTURE.md is a doc, not a source file.
