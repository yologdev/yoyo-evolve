# Piped Mode

When stdin is not a terminal (i.e., input is piped), yoyo reads all of stdin as a single prompt, processes it, and exits. This works like single-prompt mode but takes input from a pipe instead of a flag.

## Usage

```bash
echo "explain this code" | yoyo
cat prompt.txt | yoyo
git diff | yoyo
```

## When to use it

Piped mode is useful for:

- **Passing file contents** as part of the prompt
- **Chaining with other commands** in a pipeline
- **Feeding structured input** from scripts

## Examples

**Review a git diff:**
```bash
git diff HEAD~1 | yoyo --system "Review this diff for bugs."
```

**Analyze a file:**
```bash
cat src/main.rs | yoyo --system "Find all potential panics in this Rust code."
```

**Process command output:**
```bash
cargo test 2>&1 | yoyo --system "Explain these test failures and suggest fixes."
```

## Detection

yoyo detects piped mode automatically by checking if stdin is a terminal. If it is not, piped mode activates. If stdin is a terminal, interactive REPL mode starts instead.

If piped input is empty, yoyo exits with an error: `No input on stdin.`

## Structured output (`--output-format`)

Piped and single-prompt modes accept `--output-format <fmt>`:

- `text` (default) — just the response text.
- `json` — a single JSON object with the final result.

In `json` mode and with `--print`, the streamed answer text is not echoed to
stdout: the final payload is written there exactly once, so `| jq` parses it.
Under `--print` and `--output-format json`, progress output (tool `▶` lines, ✓/✗ results, diffs, turn boundaries) goes to stderr, and stdout carries only the answer.

**When a turn dies mid-stream** (the provider errors after text has started arriving, and every retry dies the same way), `--print` still writes the text the last attempt produced to stdout, and yoyo exits **non-zero** with the error on stderr. So stdout carries what was produced and the exit code carries that it failed — check the exit code before trusting the answer as complete. `--output-format json` is not covered by this guarantee: what its envelope carries on a mid-stream failure is not pinned by a test.

Without `--print` (plain `-p`, or piped stdin), text streams to stdout. When stdout is **not a terminal**, each assistant message is written once it has finished, not token by token: yoyo holds every attempt until it succeeds, so when the provider library retries a rate limit or a dropped connection mid-answer, the failed attempt's text never reaches the pipe and the retried answer is printed once, exit 0. Ctrl+C (SIGINT), SIGTERM (what `timeout` sends), SIGHUP (a dropped ssh session or closed terminal) and SIGQUIT (Ctrl+\ from the terminal) all write the text produced so far, then exit 130, 143, 129 and 131 respectively. While output is held this way, SIGQUIT does not dump core. A panic inside yoyo while text is held writes that text before the usual panic exit (101); a build with `panic = "abort"` does not. SIGKILL still ends the process at once, and the held text is lost. If the agent's own task dies mid-run (the event stream closes before the run's end event, for example a panic inside the agent library), the text produced so far is written once, stderr says `(reply cut off: the agent stopped before finishing the turn)`, and yoyo exits **1** (`--output-format json` reports `is_error: true`). This holds on a terminal too. A terminal is unchanged (live tokens). If a turn's last attempt also fails after producing text, that text is written once and yoyo's own retry follows the rule below. A turn that dies **after** it has started printing is **not retried**: bytes already written to a pipe cannot be taken back, so a retry would print the partial answer a second time and the reader could not tell the copies apart. yoyo exits non-zero with the error on stderr, plus a line saying the retry was skipped. Rerun the command. A turn that dies before printing anything (the usual overloaded-at-start case) is still retried, and a terminal still retries as before.

**Opt-in: `retry_after_partial = true`** in `.yoyo.toml` (default `false`). With it on, yoyo's own retry loop retries such a turn anyway, so one transient error (e.g. `Overloaded`) after streamed text no longer kills the run. The cost: the partial text already in the pipe stays there, so stdout carries it, a blank-line separator, then the retried answer (`PARTIAL…` `\n\n` `answer`). If every attempt fails, yoyo still stops at the usual retry cap and exits non-zero, with one partial per attempt on stdout. Turn it on when you'd rather have a log with a duplicate fragment than lose the turn. It is off by default because a consumer parsing stdout can't tell the copies apart. `--print` and `--output-format json` aren't affected, since they hold stdout back and already retry safely. Retries performed inside the provider library (rate limits, and dropped connections mid-answer) don't need the opt-in when stdout is a pipe: their failed attempts are never printed. They are capped separately, so with the opt-in on and every attempt failing, the run can take several times as many requests before it gives up (measured: 24 requests, about 70s, with one partial per yoyo attempt on stdout, 6 in all).

**When a reply is cut off at the output-token limit** (the model hit `max_tokens`), the text it produced is written to stdout as usual and yoyo exits **0**, but stderr gets one line: `warning: reply stopped at the output limit (max_tokens = N); raise --max-tokens or max_tokens in .yoyo.toml`. Exit 0 is deliberate, so a run that sets a low `max_tokens` on purpose doesn't fail every time; check stderr for that line if you need to know the answer is complete. The same line appears in the REPL. `--output-format json`/`stream-json` don't print it.
- `stream-json` — newline-delimited JSON (NDJSON): one yoagent `AgentEvent`
  per line, emitted in real time as the agent works.

The `stream-json` stream is the raw yoagent `AgentEvent` serialization —
internally tagged on a `"type"` field, `camelCase` keys, full fidelity (no
lossy translation). The first line is always `{"type":"agentStart"}` and the
last yoagent event is `{"type":"agentEnd", ...}` (see below for the one line
that can follow it). Tool activity appears as
`toolExecutionStart` / `toolExecutionEnd`, and assistant text arrives as
`messageStart` / `messageUpdate` / `messageEnd`. Turn boundaries are
`turnStart` / `turnEnd`.

```bash
echo "list the files" | yoyo --output-format stream-json
```

```jsonl
{"type":"agentStart"}
{"type":"turnStart", ...}
{"type":"messageStart", ...}
{"type":"toolExecutionStart","toolCallId":"...","toolName":"bash","args":{...}}
{"type":"toolExecutionEnd","toolCallId":"...", ...}
{"type":"messageEnd", ...}
{"type":"agentEnd", ...}
```

**Continued runs: one `sessionRestored` line.** When `--continue` or
`--continue-strict` actually restored a saved session, the line right after
the first `agentStart` reports how many messages were restored:

```jsonl
{"type":"agentStart"}
{"type":"sessionRestored","messages":4}
```

It appears once per process. It is **absent** when no session was restored:
with no `--continue` flag, or with a lenient `--continue` whose restore failed.
A failed restore never shows up as `messages: 0`. `--continue-strict` exits 1
in that case instead. A run without `--continue` streams exactly what it did
before this line existed.

**Degraded runs: one `externalServers` line.** If an MCP server or OpenAPI
spec you configured failed to connect this session, one extra line follows
the last yoagent event (so it comes after `agentEnd`, and the first line is
still `agentStart`):

```jsonl
{"type":"externalServers","mcp_connected":1,"mcp_failed":["npx some-server"],"openapi_connected":0,"openapi_failed":[]}
```

Its fields are exactly the `external_servers` object that `--output-format json`
puts in its envelope, plus `"type"`: `*_connected` counts servers that did
connect, and `*_failed` lists the server command (MCP) or spec path/URL
(OpenAPI) of each one that did not. The line is **absent** when every
configured server connected, or when none are configured, so a healthy run's
stream is unchanged. The run itself is not marked as an error — a script that
cares must look for this line.

The exact set of fields on each event is defined by yoagent — see its
[messages & events reference](https://github.com/yologdev/yoagent/blob/main/docs/concepts/messages-events.md)
for the authoritative contract.

## Quiet mode

When both stdin and stdout are piped (fully scripted usage), yoyo automatically enables quiet mode, suppressing informational `config:` and `context:` loading messages on stderr. You can also enable this explicitly with `--quiet` or `-q`:

```bash
echo "fix the test" | yoyo -q > result.md  # explicit quiet
echo "fix the test" | yoyo > result.md     # auto-quiet (both pipes detected)
```

The `YOYO_QUIET=1` environment variable also enables quiet mode.

## Slash commands aren't dispatched in piped mode

Slash commands (`/doctor`, `/status`, `/help`, etc.) belong to the interactive REPL — they depend on REPL state that piped mode doesn't have. If you pipe a slash command into yoyo, it won't run it; it would only get sent to the model as a literal string and waste a turn of tokens.

Instead, yoyo detects this case, prints a one-line warning to stderr, and exits with status code `2`. Use one of these alternatives:

```bash
yoyo doctor                       # run the subcommand directly
yoyo --prompt "/doctor"           # send the literal text to the agent
yoyo                              # interactive REPL
```


## Saving the session

Add `--save-session <path>` to write the run's session to `<path>` when it ends (even after a failed turn), in the format `/save` writes: `echo "fix the test" | yoyo --save-session run.json`. A missing parent directory or unwritable path exits non-zero with `error: --save-session: could not write <path>: <reason>`.
