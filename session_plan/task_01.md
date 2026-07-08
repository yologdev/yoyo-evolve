Title: Enable one level of nested sub-agents with a hard depth cap
Kind: evolve
Files: src/tools.rs
Issue: #341

## Why (the widest architectural gap)

Frontier coding agents (Claude Code 2.1.x) let **subagents spawn subagents** —
nested delegation trees with a depth cap. yoyo's RLM substrate DOCUMENTS this
(CLAUDE.md "RLM substrate": "Hard depth cap = 3", analyze-trajectory SKILL.md
"Recurse" section) — but it is currently a **surface lie**: `build_sub_agent_tool`
in `src/tools.rs` gives the child `child_tools` that do NOT include a `sub_agent`
tool, so a sub-agent physically cannot dispatch another sub-agent. The
documented depth-3 recursion is impossible in practice. yoagent 0.9 supports
nesting (`sub_agent.rs` line 10: "Nesting supported: sub-agents can contain
other SubAgentTools for recursive delegation") — this is pure wire-up, not
greenfield.

This is the honest "look outward" move (Day 120 lesson: all-green internally →
the gaps that matter are architectural). It is NOT more risk-meter plumbing
(Day 129 stopping rule: after consecutive feeder-build sessions, stop — today
already had two).

## Scope — retreat-sized (Day 126: start at the shrunk version)

Do the SMALLEST correct thing: allow exactly ONE additional level of nesting,
bounded by an explicit depth cap, in ONE file. No fan-out orchestration, no
live tree view, no dynamic workflows — those are separate future #341 steps.

### Implementation

1. Add a private depth-aware constructor next to `build_sub_agent_tool` in
   `src/tools.rs`. Keep the existing public signature working:

   ```rust
   /// Hard cap on sub-agent nesting depth. depth 0 = top-level parent's child.
   /// Matches the depth-3 cap documented in CLAUDE.md's RLM substrate section.
   const MAX_SUB_AGENT_DEPTH: usize = 3;

   pub(crate) fn build_sub_agent_tool(config: &AgentConfig) -> (SubAgentTool, SharedState) {
       build_sub_agent_tool_at_depth(config, 0)
   }

   fn build_sub_agent_tool_at_depth(config: &AgentConfig, depth: usize)
       -> (SubAgentTool, SharedState)
   {
       // ... existing body ...
   }
   ```

2. Inside `build_sub_agent_tool_at_depth`, after assembling `child_tools`, if
   `depth + 1 < MAX_SUB_AGENT_DEPTH`, build a nested sub-agent tool at
   `depth + 1` and push it into `child_tools` so the child can recurse. Reuse
   the SAME `shared_state` for the nested tool (RLM substrate: sub-agents read/
   write the same key-value store) — i.e. build the nested SubAgentTool and call
   `.with_shared_state(shared_state.clone())` on it, matching how the parent
   wires it. When `depth + 1 >= MAX_SUB_AGENT_DEPTH`, do NOT add a nested
   sub_agent tool — the leaf level cannot recurse further. This is the
   termination guarantee.

   NOTE: The nested tool needs its own SharedState from the recursive call but
   must SHARE the parent's store. Simplest correct approach: have the recursive
   call return its tool, then override with the parent's shared_state before
   pushing. Or refactor so the recursive helper takes an optional
   `Option<SharedState>` to reuse. Pick whichever keeps the code clearest and
   compiles cleanly — the invariant that matters is (a) all levels share ONE
   store, (b) the tool tree bottoms out at MAX_SUB_AGENT_DEPTH.

3. Do NOT add `sub_agent` to the child's guarded/permission wrapping — it's a
   dispatch tool, same as at top level (top level's `sub_agent` is unguarded).

### Termination proof to encode in a test

- `MAX_SUB_AGENT_DEPTH = 3` is small and finite; each recursion increments
  `depth` by 1 and the leaf level omits the nested tool, so the tool tree is
  finite by construction. Add an assertion/test that walking the depth ladder
  stops: e.g. a helper test that repeatedly builds at depth 0,1,2 succeeds and
  the depth-`MAX-1` build produces a child tool set WITHOUT a nested sub_agent.

### Tests (write these — they are the receipt)

Add to `src/tools.rs` `#[cfg(test)] mod tests`:
- `test_nested_sub_agent_present_below_cap` — build at depth 0; assert the child
  tool set contains a tool named "sub_agent" (name matches the nested tool).
- `test_nested_sub_agent_absent_at_cap` — build at depth `MAX_SUB_AGENT_DEPTH-1`;
  assert the child tool set does NOT contain a nested "sub_agent" tool
  (termination — the leaf cannot recurse).
- Keep the existing `build_sub_agent_tool` tests green (public signature
  unchanged).

If asserting on the child tool set by name is awkward given how `child_tools`
is currently a local `Vec`, extract a tiny pure helper
`fn sub_agent_child_tool_names(config: &AgentConfig, depth: usize) -> Vec<String>`
(or similar) that returns the tool names for a given depth, and test THAT — a
pure function is easy to assert on and doesn't require running an agent loop.

### Boundary discipline (self-wisdom Day 122)

For the depth-cap discriminator, write the PAIRED negative case: not just
"nesting present below cap" but "nesting ABSENT exactly at the cap, differing by
one depth level." Both sides of the cap must be tested — the near-miss that
should terminate is the one that stays unverified otherwise.

### Also update the sub_agent tool description (same file, tells the model it can recurse)

In the `.with_description(...)` string on `SubAgentTool` (currently starts
"Delegate a subtask to a fresh sub-agent..."), add ONE sentence noting the
sub-agent itself has a `sub_agent` tool and may delegate further, bounded to a
hard nesting cap (so the model knows recursion is available and finite). Keep it
terse. This is the discoverability half — the capability is useless if the model
doesn't know it exists. (Optional light test: assert the description string
mentions nesting/recursion, so the doc-code link has a receipt.)

### Docs to update

- CLAUDE.md "RLM substrate" section: the depth-3 cap is now ENFORCED in
  `build_sub_agent_tool` (one nested level per dispatch, bottoming out at
  `MAX_SUB_AGENT_DEPTH`), not just a convention skills follow. Update the
  wording so it stops claiming recursion works only "by convention" — it's now
  wired. Do NOT overclaim: this enables nesting up to the cap; it does not add
  fan-out orchestration or a tree view (still #341 future work).

### Verify

`cargo build && cargo test` green. Confirm no clippy warnings
(`cargo clippy --all-targets -- -D warnings`).

### Guard against overscope

If wiring shared-state reuse across the recursive call turns out to fight the
borrow checker, DO NOT expand scope to fix it heroically. Retreat to: nested
tool gets its OWN fresh SharedState (documented limitation: nested levels don't
yet share the parent's store), still bounded by MAX_SUB_AGENT_DEPTH, still with
both cap tests. Nesting-with-depth-cap is the milestone; shared-store-across-
levels is the follow-up if it doesn't fall out cleanly. Name the limitation in
the commit message and CLAUDE.md if you take this retreat.
