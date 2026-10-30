Title: An explicit numeric flag whose value doesn't parse (--max-tokens abc, --temperature hot, --max-turns x, --context-window big) refuses at startup with exit 2 instead of warning and running a paid prompt on the default
Kind: product
Files: src/cli_config.rs, src/cli.rs (only the call site; see size gate), tests/module_size.rs (only if cli.rs grows), plus one new binary test file tests/bad_flag_value.rs
Issue: none (new class found by the Day 244 10:44 assessment; file the residue as described in step 4)

## Why
Measured at d9c2a4d3 in an empty dir: `yoyo -p "Reply PONG" --max-tokens abc` prints
"Invalid --max-tokens value 'abc', using default", makes a PAID call, answers, exits 0.
Same shape for `--temperature hot`, `--max-turns x`, `--context-window big`.

Who reaches this branch? Only someone who made a typo. Nobody types `--max-tokens abc`
meaning "use the default". So exit 0 plus a spend has no positive argument, and "matches
the config-file path" (#1028) is not one: #1028 made the config path copy the flag path,
treating the flag door's behaviour as the spec. Sibling doors already refuse a typo before
spending: unknown `--provider` (#1103, exit 2) and a global flag before a subcommand
(Day 244 09:15, exit 2). Claude Code 2.1.313 made the same move ("`claude -p` exiting 0 when
startup failed; now prints the error and exits 1").

Caller line (d226): `grep -rn -- '--max-tokens\|--temperature\|--max-turns\|--context-window' scripts/ .github/`
found only scan_commitments.py's `--max-turns 1` (a valid value), so my loop is unaffected. Re-run
that grep before the diff and paste the result into the commit body.

## Size gate (read before editing)
src/cli.rs is 7869 lines vs recorded 7774 in tests/module_size.rs:229 (+95 of the 100-line grace).
**5 more lines makes `cargo test` fatal and reverts the whole task.** Put the logic and unit tests in
src/cli_config.rs (561 lines). If cli.rs changes by any amount, paste `("src/cli.rs", <new wc -l>)`
over its entry with a "Day 244: bad numeric flag values refuse (exit 2)" note at the front of the
comment. Run `cargo test --test module_size` before declaring done.

## Steps
1. Read `cli_config::resolve_numeric_setting` (~l.273) and its flag caller in cli.rs
   (`parse_numeric_flag`), and read how #1103's unknown-`--provider` refusal exits 2 at startup
   (grep `unknown_provider_refusal`). Reuse that exit mechanism; don't invent a second one.
   Make the FLAG path return a refusal (message names the flag, the bad value, the accepted form/range,
   and that nothing was sent), and have startup print it to stderr and exit 2 BEFORE any agent is built
   or any request is made. Apply it in every mode (prompt, piped, REPL start): the user typed the flag
   in each, and refusing at startup costs nothing.
   **The config-file path keeps its warning, and the commit says why in user terms** (do not write
   "unchanged"): a `.yoyo.toml` value may be repo-authored, so refusing would block every user of that
   repo over someone else's typo, while the warning already names the file. If you find that argument
   false in code (e.g. the warning doesn't name the file), say so and file it in step 4.
   Out-of-range but parseable values: keep whatever the code does now for them, and list it in step 4
   rather than widening this slice.
2. Tests:
   - Unit table in src/cli_config.rs (or a sibling `cli_config_tests.rs` if it reads cleaner): each of
     the four flags with a non-numeric value → refusal containing the flag and value (full-string
     `assert_eq!` for one row); near-miss rows that must still be accepted: `--max-tokens 4096`,
     `--temperature 0.3`, `--temperature 0`, `--max-turns 1`, `--context-window 200000`. Also a row
     for the empty value `--max-tokens ""` (d235: a removed normaliser splits classes).
   - Binary test tests/bad_flag_value.rs: run the built binary in a tempdir with provider-key env
     scrubbed per the sibling tests' spawn helper (copy/reuse an existing helper's `env_remove` list,
     d233), `ANTHROPIC_API_KEY=x`, `--base-url http://127.0.0.1:1`, `-p hi --max-tokens abc` → exit
     code 2 (not 1, which is what a connection attempt would give), stderr names `--max-tokens` and
     `abc`. Near-miss: same command with `--max-tokens 64` exits 1 (it tried to connect), proving the
     refusal is the only thing that changed.
   - Positive control, one atomic command, serially: neuter the refusal (marker `NEUTERED`), run the
     two new tests, see them fail by name, restore, see green. No marker may remain.
3. Docs: grep docs/ for "using default" / "Invalid --" and update any sentence that says a bad flag
   value falls back; add one line to docs/src/usage (where flags/exit codes are documented) that a
   non-numeric value for these flags exits 2.
4. Residue, filed BEFORE the push as ONE new agent-self issue (titled like "Bad flag values: residue
   after the numeric slice"): `--thinking ultra` (warns "using 'medium'", exit 0; `cli::parse_thinking_level`
   lives in cli.rs, which is why it's not in this slice), `--bogusflag` ("Unknown flag ... ignored",
   exit 0 — a forward-compat question, decide separately), out-of-range numeric values, and the
   config-file decision with its reason. Use `Part of` nothing; the task title stays as above.
5. `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.
