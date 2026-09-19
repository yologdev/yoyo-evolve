Title: #881 slice 2 — a read-only `explore_agent` sub-agent: compose `read_only_child_disallowed` with `sub_agent` instead of trusting the caller
Kind: product
Files: src/tools.rs, src/agent_builder.rs, ARCHITECTURE.md
Issue: #881

## Why this, today

`#881` is a *composition* gap, which is why it outranks adding another primitive: I own
`READ_ONLY_CHILD_REMOVED_TOOLS` / `read_only_child_disallowed` (`src/tools.rs:1445-1471`, #881
slice 1, landed and tested) and I own `build_sub_agent_tool` (`src/tools.rs:1511`) — and the only
thing that joins them is a **process-global flag** (`--read-only-subagents`,
`src/cli.rs:2602` → `crate::cli::is_read_only_subagents()` read at `src/tools.rs:1550`). So today
the answer to "explore this, and you may not write" is *every sub-agent in the session is
read-only*, not *this dispatch is*. Slice 1 shipped the pool; nothing lets a caller pick from it.

## The upstream fact that decides the shape — **already measured, do not re-derive**

`SubAgentTool::parameters_schema()` is `{"task": string}` and `execute()` reads only
`params["task"]` (`~/.cargo/registry/src/*/yoagent-0.13.0/src/sub_agent.rs:296-320`). There is no
per-call parameter seam, so **design option 3 of the issue ("decorator that inspects the task
text") is rejected and option 2 (a second tool) is the shape.** First action, ~10 seconds:
confirm the same is true at the version `Cargo.toml:26` actually pins (`yoagent = "0.18"`,
0.18.1 per `CLAUDE.md`'s marker — read
`~/.cargo/registry/src/*/yoagent-0.18.1/src/sub_agent.rs`). If 0.18.1 *did* gain a per-call seam,
stop and say so in the turn's write-up rather than building the second tool.

**No new CLI flag.** A second tool is a capability, not a convenience: gating it behind an opt-in
flag is exactly the discoverability debt (#745/#767/#769) this repo has already paid three times,
and #448's opt-in rule governs *evolve-loop conveniences reaching product defaults*, which this is
not. It is removable the ordinary way, by name, through `disallowed_tools`.

## Step 1 — build it (`src/tools.rs`)

Add `build_explore_agent_tool(config: &AgentConfig) -> Box<dyn AgentTool>` (plus, if the parent
needs it, a `&SharedState` parameter) beside `build_sub_agent_tool`, and reuse — never re-spell —
the existing pieces:

- child tool set: `sub_agent_child_tools(&config.dir_restrictions, &read_only_child_disallowed(&config.disallowed_tools))`
  (a **fold**, not a second literal list: #881 slice 1 exists precisely so there is one statement
  of the read-only child set);
- the tool itself: `sub_agent_tool_for(...)` with the tool **name `explore_agent`** and a
  description that (a) says the sub-agent is read-only by construction and (b) says what that means
  — no `write_file`/`edit_file`/`rename_symbol` — so the *parent model* does not read an absent
  capability as a nonexistent one (the Day-181 third-door defect, `CLAUDE.md`);
- the **same** `SharedState` store the parent's `sub_agent` uses, so an exploration artifact is
  stored once and read by reference (the RLM invariant) rather than pasted into the parent context;
- nesting: give the explore tool the same depth-bounded `sub_agent` child path only if it is cheap
  to do through the existing builder; if it forces a copy of `build_sub_agent_tool_at_depth`, leave
  it out and **say so in the description and in the write-up** (a silent absence is the defect, an
  announced one is a limit).

## Step 2 — register it, and make the registration honest (`src/agent_builder.rs`)

1. Push the new tool in `build_agent` at the site that already pushes the sub-agent tool
   (`:1181`), which sits **after** the `disallowed_tools` retain — so, exactly like `sub_agent`,
   it cannot be filtered there. **Do not quietly claim confinement you did not build:** either
   (a) route it through `effective_disallowed_tools` / the retain so
   `disallowed_tools = ["explore_agent"]` genuinely removes it, or (b) if the only workable site is
   post-retain, write that sentence at the push site and pin it with a test — the same disclosure
   `--restricted`'s own comment makes about `sub_agent` (`src/cli.rs:1733-1751`).
2. Add `"explore_agent"` to `BUILTIN_TOOL_NAMES` **in this same diff**: the superset guard requires
   every name `build_tools` registers to be listed, and the near-miss twin requires the
   not-registered list to stay exact. Check which of the two lists it belongs on
   (`ask_user`/`sub_agent`/`shared_state` are the current not-registered three) and update it —
   both lists live in `src/agent_builder.rs` and both must move together.
3. Do **not** touch `src/cli.rs` (7,272 / 7,272 registered, and any line there is register drift).

## Step 3 — the regression surface is the *unchanged* path, so test that

Required, in this order (the anti-vacuous assertion first — the #250 `think`-gate shape):

- **near-miss / byte-identical:** a default config's *existing* tool set is unchanged and the
  `explore_agent` child disallow list differs from `sub_agent`'s **only** by
  `READ_ONLY_CHILD_REMOVED_TOOLS`; assert with whole-vector equality and against the const, never a
  re-typed literal. This is every user who never calls the new tool — the entire regression surface.
- **presence, not absence:** assert the explore child tool names *exclude* each name in
  `READ_ONLY_CHILD_REMOVED_TOOLS` (loop the const, assert non-empty first) — a list that silently
  became empty must redden, not pass.
- **the refusal is a refusal, not a filter:** the child keeps `bash` via
  `with_read_guard_bash_arc` (`src/tools.rs:1490`), so a write attempt returns the
  `REFUSAL_STEM_MODE_ACTIVE`-shaped error the parent can see — assert on that string at the
  emission point rather than on the tool simply being missing.
- **source-level guard:** one test slicing `build_explore_agent_tool`'s body (assembled at runtime,
  the `#842` discipline) proving its disallow list comes from `read_only_child_disallowed` rather
  than a second literal. State in its doc comment that it proves *presence*, never that it fires.

## Docs (required, same diff — three receipts died on exactly this omission)

- **ARCHITECTURE.md**: the `src/tools.rs` and `src/agent_builder.rs` entries are where per-file
  history goes now (`CLAUDE.md`'s own rule). Record: what landed, the yoagent version pinned, the
  post-retain disclosure decision from step 2(1), and the residue (a caller still cannot ask for
  read-only *per dispatch* of the existing `sub_agent`; `--read-only-subagents` remains the global
  switch). Do not append any of this to `CLAUDE.md`.
- If `tests/module_size.rs` prints a register line for either touched file, **paste it verbatim**
  (`tools.rs`/`agent_builder.rs` are grandfathered; a few added lines stay inside the 100-line
  grace band and warn rather than fail, so paste the number the gate prints and nothing else).

## Verify

`cargo build && cargo test` then, explicitly, `cargo clippy --all-targets -- -D warnings` and
`cargo fmt -- --check`. `Kind: product` — read-only exploration sub-agents are a user-facing
capability. Bounds from the issue, honoured: default unchanged and opt-in-free-but-removable;
never composed with `/read` mode's process-global state (this is a property of the tool, not the
session).
