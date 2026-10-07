Title: `.yoyo/memory.json` must honour --deny-dir/--allow-dir at its resolved target before its content reaches the system prompt (#1002 part 3)
Kind: product
Files: src/context.rs, src/context_fence_tests.rs, docs/src/configuration/permissions.md
Issue: #1002

## Why

The assessment measured this at HEAD with fixture `/tmp/p221s`. `proj/.yoyo/memory.json` is a symlink to a file in `secret/` that holds a unique token. Then
`cd proj && yoyo --deny-dir /tmp/p221s/secret --print-system-prompt | grep -c <token>` → **1**, and no warning is printed. The same fixture gives 0 for CLAUDE.md, AGENTS.md and goal, because cf8ad734 and a39737e0 fixed those today. Memory is the last measured pre-turn reader that still leaks into the provider sink. (`--safe-mode` gives 0 only because it drops the whole project context.)

The prompt-side read is `src/context.rs:410`, inside `load_project_context_from`:
`crate::memory::load_memories_from(&dir.join(crate::memory::memory_file_path()))`. Since cf8ad734, that same function already applies the fence to the instruction files. So the fence value and the warning helper should already be in scope there. **Read `git show cf8ad734 -- src/context.rs` first and reuse its exact mechanism**: the same canonicalize-then-`check_path` and the same ⚠ warning shape. Do not write a second fence helper (the "two doors, one policy" class).

## Steps

1. **Reproduce first with the recorded invocation, byte-for-byte.** Rebuild the fixture: `secret/mem.json` holds a valid memory JSON (copy the shape from an existing memory test) with a token like `MEMTOK_1002_ZQ`, and `proj/.yoyo/memory.json` is a symlink to it. Run the exact `--deny-dir <abs>/secret --print-system-prompt | grep -c` command and record **1**. Then make the fix in `load_project_context_from`: resolve the memory path's real target, and if the fence refuses it, skip the memories and emit the same ⚠ warning the instruction-file fence emits. A missing memory file stays silent, exactly as today. Re-run the same command and record **0** plus the warning line. Also run `--allow-dir <abs>/proj` and record 0 plus a warning, because the target is outside the allow-list.
2. **Tests in `src/context_fence_tests.rs`, at the emission point** (the context string `load_project_context_from` returns, not a helper below it), using tempdirs only:
   (a) symlinked memory.json into a denied dir → token absent and the warning produced.
   (b) **Near-miss:** an ordinary, non-symlinked memory.json with no fence → the token is present, and the memory section is byte-identical to before (`assert_eq!` on the section, not `contains`).
   (c) The same symlink with a fence that does NOT cover the target → token present.
   (d) Anti-vacuous: assert the fixture file really contains the token.
   Then run a **positive control as one atomic command**: neuter the new check, run the tests, see (a) fail by name, restore, see it pass. Put a `NEUTERED` marker on the neutered line while it is out. Run the control serially.
3. Add one line for memory.json to `docs/src/configuration/permissions.md`, next to the lines cf8ad734/a39737e0 added. Run `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`.

## Out of scope, named so it isn't mistaken for done
- The REPL `/memories` display and the `/remember` write path (`memory.rs:187`). Those go to the user's terminal or to disk, not to the provider. They are separate doors.
- Skills dirs (`--print-system-prompt` can't show them; this needs a request-level sink) and `commands_spawn.rs`'s direct `load_project_context()` calls. Those calls inherit this fix if they go through `load_project_context_from`: check with one grep and say which in the commit.

The commit message ends with `Part of #1002` (NOT `Fixes`): skills are still unprobed, so the issue must stay open.
