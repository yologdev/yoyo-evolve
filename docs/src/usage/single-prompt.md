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
`-p` takes exactly one word, so quote a prompt with spaces: `yoyo -p hi there` is refused
(exit 2, nothing sent) and names `there`, instead of silently sending only `hi`. Quote it
(`yoyo -p "hi there"`) or put the rest after `--` (`yoyo -p hi -- there`).

## Tool approval

A single-prompt run approves every tool call without asking, with or without `-y`/`--yes`:
there is no one to answer a `[y/N]` prompt, so every bash command and file write the model asks
for runs. This holds for `-p`, `--prompt`, a bare `yoyo "..."` and `--print`. What still applies
is your deny list (`--deny`, or `[permissions] deny` in `.yoyo.toml`), the built-in refusals no
approval can override (such as `rm -rf /`), and `--allow-dir`/`--deny-dir` on the file tools.
Before running it on a repository you don't trust, set a deny list or run it in a sandbox. See
[Permissions & Safety](../configuration/permissions.md).

`--` ends option parsing; everything after it is prompt text, sent verbatim even when it starts
with a dash: `yoyo -- --explain this` and `yoyo -p -- --explain this` both send `--explain this`,
and `yoyo -p hi -- --x` sends `hi --x`. Unknown flags *before* `--` are still refused.
No flag after `--` is applied either, including the ones read at startup: `yoyo -- --no-color hi`
sends `--no-color hi` and leaves colour, bell, notifications, `--quiet` and `--print` as they were.
The same holds for the word right after a flag that takes a value: `yoyo -p --help` sends
`--help` (with a warning that the value looks like a flag) instead of printing the help page.
On/off flags are not applied from that slot either: `yoyo -p --print-system-prompt` sends the
text, and `yoyo --system --yes hi` uses `--yes` as the system prompt without turning on
auto-approve. Flags that take a value are still read from it (`yoyo --system --model x` picks
model `x`); that is tracked in #1114.

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

## Capturing the output in a script

Plain `-p` writes the response to stdout as the model streams it. Leading blank lines
(the model's own, plus yoyo's framing newline) are dropped, so stdout starts with the
first real line of the answer, with its indentation kept. Trailing blank lines are
dropped too: the answer ends with exactly one newline, so `yoyo -p "Reply with exactly: PONG"`
prints `PONG\n` (blank lines *inside* the answer are kept). Tool progress may also
appear on stdout, because `-p` is the interactive-looking path.

If you are capturing the answer (`$(...)`, a pipe, a file), use `--print`: its stdout
carries only the answer text, with no spinners, tool output or color, and with leading
blank lines dropped. For structured output use `--output-format json` instead.

```bash
answer=$(yoyo --print -p "Reply with exactly: PONG")
yoyo -p "summarize this repo" --output-format json | jq -r .response
```

If the run is interrupted with Ctrl+C (SIGINT), yoyo exits with code **130**, writes
`(interrupted)` to stderr, and leaves whatever text was already streamed on stdout
unchanged. A script can tell a cut-off answer from a complete one, which exits 0.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | The answer is complete. Also a reply cut off at a `max_tokens` **you** set (`--max-tokens` or `max_tokens` in `.yoyo.toml`): stderr carries `warning: reply stopped at the output limit (max_tokens = N)`, and you asked for that limit. |
| 1 | No complete answer: the run failed (an API error, a dead turn, a failed `--output` write), **or** the reply was cut off at an output limit you never set (a model preset or the request default; stderr names "the model's default max_tokens"), **or** the model refused the request (stderr says `refused this request`). Whatever text was streamed stays on stdout. |
| 2 | A `--context-strategy checkpoint` stop: the run asks to be resumed. A refused slash-command prompt (below) also exits 2. |
| 130 / 143 | Interrupted by SIGINT / SIGTERM. |

A script that reads only stdout cannot see where a reply ended, which is why a cut at a
limit nobody chose is a failure: the partial text would otherwise pass for the whole answer.
The same codes apply to piped mode and to `--output-format json`; `--output-format stream-json`
exits 0 or 1 by the same rules (it has no checkpoint code).

With `--output-format json`, the result object says how the final reply ended in an
`incomplete` field, so `jq .response` is not the only thing a script can read:

| `incomplete` | Meaning | `is_error` | Exit |
|--------------|---------|------------|------|
| `null` | The reply ended normally (a failed run is reported by `is_error`). | as the run | as the run |
| `{"reason": "output_limit", "limit_set_by": "default"}` | Cut at a limit you never set. | `true` | 1 |
| `{"reason": "output_limit", "limit_set_by": "user"}` | Cut at your own `max_tokens`. | `false` | 0 |
| `{"reason": "refusal"}` | The model refused. | `true` | 1 |

`is_error` always agrees with the exit code for these endings. With
`--output-format stream-json`, the same stderr line the text modes print (the output-limit
warning, or the refusal notice) is printed to **stderr** when the final reply was cut or
refused; stdout stays pure JSON lines, and the final `agentEnd` message still carries the
raw `stopReason`.

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
yoyo "review this diff" --model claude-sonnet-5-5
yoyo "explain the architecture" --thinking high
yoyo -p "analyze the code" --system "You are a security auditor."
```
