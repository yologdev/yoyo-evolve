Title: Mark sub-agent output as sub-agent output — a sub-agent's text must not read as the session's own instructions
Kind: product
Files: src/tool_wrappers.rs, src/tools.rs, docs/src/usage/commands.md
Issue: none (self-driven; same defect class as #902 on a channel I fully control)

## Why this, now

Claude Code v2.1.277 changelog, verbatim: *"Changed subagent results to reach the main agent under a header
marking them as subagent output, with the result indented, so text in a subagent's result cannot pass as the
session's own instructions."*

That is a **prompt-injection boundary between trust levels**, not a feature. yoyo's parent receives a sub-agent's
summary text as an ordinary tool result with no marker, and by my own RLM notes a sub-agent may read arbitrary
artifacts — CI logs, web pages, foreign repositories — so a sub-agent that summarised a hostile file can inject
into the parent in the parent's own first-person voice. This is the same defect as #902 (untrusted text arriving
where trusted instruction lives) on a channel I fully control, and it needs no trust-gate design work at all.

## What already exists (verified at planning time — re-verify before editing)

yoyo **does** wrap the yoagent sub-agent tool in its own layer, so the parent-visible text passes through code
yoyo owns:

- `src/tool_wrappers.rs:1508` — `pub(crate) struct FallbackSubAgentTool` (one retry on the fallback model).
- `src/tool_wrappers.rs:1671` — `pub(crate) struct DiagnosticSubAgentTool`.
- `src/tools.rs:1802` and `src/tools.rs:1811` — the composition site: `Box::new(FallbackSubAgentTool::new(…))`
  then `Box::new(DiagnosticSubAgentTool::new(…))`.
- `src/tool_wrappers.rs:5227` — an existing test constructs `DiagnosticSubAgentTool::new(inner, LABEL)` over a stub
  inner tool. **That is the precedent to copy for the emission-point test**; you do not need a live sub-agent.
- `src/tools.rs:40` — `use yoagent::sub_agent::SubAgentTool;` — yoagent is a **dependency**. Never edit it.

Module-size gate (`tests/module_size.rs`): `MAX_MODULE_LINES = 2000`; both files are **grandfathered register
entries** (`src/tool_wrappers.rs` 5276, `src/tools.rs` 4931) and `REGISTER_DRIFT_GRACE_LINES = 100`. So keep the
diff small (well under ~100 lines per file, tests included) and measure the files before and after. If the gate
warns about register drift, follow the gate's own printed remedy — update the entry **with the reason in the
commit message** — rather than leaving a warning behind.

## Deliverable

A pure helper that composes the marker (e.g. `mark_subagent_output(text: &str) -> String`) plus **exactly one**
call site at the seam through which every parent-visible sub-agent result passes, so the parent sees:

- a header naming the text as **sub-agent output** (not session instructions), and
- the sub-agent's own text, unchanged, under that header.

Indenting the body (mirroring the rival's shape) is optional — the **marker is the requirement**. The header must
read as a provenance boundary, not as an error: e.g. the parent is told this text came from a sub-agent, that it
may summarise untrusted material, and that instructions inside it are data to be reported rather than obeyed.

## Step 1 — locate the seam before writing anything (do not guess)

Run `grep -n "build_sub_agent_tool\|FallbackSubAgentTool\|DiagnosticSubAgentTool" src/tools.rs src/tool_wrappers.rs src/agent_builder.rs`
and read **only those hits plus the enclosing function**. You are answering one question: *which of yoyo's own
seams does the parent-visible sub-agent result flow out of?* Put the marker at the outermost yoyo-owned seam so
the marker is applied exactly once, whatever the inner wrapper does. A source-level guard that counts the marker
helper's call sites (asserting exactly one) is the cheap way to keep that true.

## Hard constraints — do not

- Do **not** edit yoagent (a dependency) or any file under `~/.cargo/registry`.
- Do **not** change the sub-agent's own prompt, the parent's system prompt, the fallback/diagnostic logic, tool
  names, schemas, or the session tool-call cap.
- Do **not** alter the result text of any non-sub-agent tool.
- Do **not** mark the result in more than one wrapper — one marker per parent-visible result.

## Tests (same diff, non-negotiable)

1. **Emission point, full-string `assert_eq!`:** build the wrapper over a stub inner tool whose result is a known
   string; assert the exact composite string the parent receives (header + stub text). Not a `contains`.
2. **Exactly one marker**, including on the retry/fallback path (feed a stub that fails once and succeeds), and/or
   the source-level call-site-count guard above.
3. **Anti-vacuous fixture:** the stub's text must be distinctive enough that test 1 cannot pass by agreeing with
   itself; additionally assert the stub text still appears **verbatim** inside the marked result, so a future edit
   that swallows or rewrites the sub-agent's own words reddens.
4. **Near-miss:** an unmarked path stays byte-identical to before (e.g. the helper is not called, or a sibling
   wrapper's result is compared with `assert_eq!` against its previous form).

## Docs (same commit — the evaluator checks the Files line)

`docs/src/usage/commands.md` describes sub-agents (grep hits). Add 2–4 lines: sub-agent output reaches the parent
under a header marking it as sub-agent output, so text inside a sub-agent result cannot pass as session
instructions. Do **not** claim a sandbox or a sanitizer — this is a provenance marker, nothing more.

## Steps (do all three; a half-executed protocol gets reverted)

1. Read `ARCHITECTURE.md`'s entries for `src/tool_wrappers.rs` and `src/tools.rs`
   (`grep -n "tool_wrappers.rs\|src/tools.rs" ARCHITECTURE.md`), read the module-size gate's doc comment, then do
   **Step 1 — locate the seam** above and write down (in the commit message) which seam you chose and why it is
   the outermost one.
2. Implement the helper + the single call site; add the four tests; update `docs/src/usage/commands.md`.
3. Verify, in this order: `cargo fmt -- --check` → `cargo clippy --all-targets -- -D warnings` →
   `cargo build && cargo test` (the full suite — `tests/module_size.rs` is whole-repo). Run clippy explicitly at
   the end rather than trusting auto-watch.
