---
name: blindspot
description: "Systematically find blind spots in code, architecture, APIs, and deployment — structured critique that catches what familiarity hides"
tools: [bash, read_file, list_files, search, sub_agent]
core: false
origin: yoyo
status: active
score: 0.59
uses: 1
wins: 1
last_used: "2026-08-19T23:06:39Z"
last_evolved: "2026-08-21"
parent_pattern_key: null
keywords: ["skills/blindspot", "roast level", "structured critique", "Analysis Dimensions"]
---

# Blindspot

You are performing a **structured critique** of code, architecture, or systems. Your job is to find what the author can't see — the gaps that familiarity hides, the risks that daily use normalizes, the debt that accumulates silently.

This is not a linter. Linters catch syntax and style. You catch design decisions that will hurt later — the unwrap() that will panic in production, the O(n²) that's fine with 10 items but fatal with 10,000, the API that can't evolve without breaking clients.

## When to use

- During evolution self-assessment (proactive self-critique)
- On-demand via `/skill run blindspot` with a target
- When reviewing unfamiliar code before modifying it
- After a feature is "done" — the last-mile audit
- When a community issue reports a class of problem (search for siblings)

## When NOT to use

- For style/formatting issues (use clippy/rustfmt)
- When you need to understand code before critiquing it (use `explore-codebase` first)
- For single-line fixes you can see directly (just fix them)

## Parameters

### Target
What to analyze. One of:
- A file path: `src/tools.rs`
- A directory: `src/format/`
- A module or concept: "error handling in the REPL"
- `self` — yoyo's own codebase (defaults to `src/`)

### Roast level
Controls the threshold for reporting:
- **gentle** — Critical and Warning only. For when you want actionable items without noise.
- **standard** (default) — Critical + Warning + Smell. The balanced default.
- **brutal** — Everything, including nitpicks. For when you want the full picture.

## Analysis Dimensions

Examine the target through these lenses:

### 1. Error handling gaps
- `unwrap()` / `expect()` on fallible operations in non-test code
- Functions that return `Ok(())` but swallow errors silently
- Missing error context (bare `?` without `.context()` or `.map_err()`)
- Panic paths reachable from user input
- Error types that lose information (stringly-typed errors)

### 2. Security blind spots
- Hardcoded secrets, tokens, or credentials
- User input passed to shell commands without sanitization
- Path traversal vulnerabilities (unchecked `../`)
- Unsafe blocks without safety comments
- Dependencies with known vulnerabilities

### 3. Architecture debt
- God objects (structs/modules doing too many things)
- Circular dependencies between modules
- Leaky abstractions (implementation details in public interfaces)
- Dead code that's maintained but never called
- Tight coupling that prevents independent testing

### 4. Scalability risks
- O(n²) or worse in paths that handle user data
- Unbounded collections (Vec/HashMap that grow without limit)
- Missing pagination on queries or listings
- Blocking operations in async contexts
- Single-threaded bottlenecks in concurrent code

### 5. Testing gaps
- Public functions without any test coverage
- Tests that only check the happy path
- Tests that mirror implementation rather than behavior
- Missing edge cases: empty input, Unicode, very large input, concurrent access
- Brittle tests that break on unrelated changes

### 6. API design issues
- Inconsistent naming conventions within the same module
- Functions that accept `String` when `&str` would suffice
- Missing input validation at API boundaries
- Breaking change risks (public types that can't evolve)
- Boolean parameters that should be enums

### 7. Dependency risks
- Outdated dependencies with available updates
- Heavy dependencies used for trivial functionality
- Dependencies with single maintainers or low activity
- Vendored or forked code that's drifted from upstream
- Missing `Cargo.lock` entries or version pinning

## Procedure

### 1. Scope the target

Determine what you're analyzing and how large it is.

```bash
# For a file
wc -l <target_file>

# For a directory
find <target_dir> -name '*.rs' | xargs wc -l | sort -rn | head -20

# For "self"
find src/ -name '*.rs' | xargs wc -l | sort -rn | head -20
```

### 2. Decide: direct analysis or sub-agent dispatch?

- **≤5KB total** (roughly ≤150 lines): Read directly, analyze in main context.
- **>5KB but ≤30KB**: Read key files directly, focus analysis on highest-risk areas.
- **>30KB**: Use sub_agent dispatch. Give each sub-agent one analysis dimension and a subset of files. Synthesize their reports.

For sub-agent dispatch, store the file contents in SharedState and dispatch focused questions:
- "Analyze error handling patterns in these files. Find unwrap() calls, swallowed errors, and missing error context."
- "Find scalability risks: O(n²) algorithms, unbounded collections, missing pagination."

### 3. Analyze each dimension

For each of the 7 dimensions, scan the target with appropriate tools:

```bash
# Error handling: find unwrap/expect in non-test code
grep -rn '\.unwrap()' <target> | grep -v '#\[cfg(test)\]' | grep -v 'tests/'
grep -rn '\.expect(' <target> | grep -v '#\[cfg(test)\]' | grep -v 'tests/'

# Security: find potential secrets
grep -rn 'password\|secret\|token\|api_key' <target> --include='*.rs'

# Architecture: find large files (potential god objects)
find <target> -name '*.rs' -exec wc -l {} \; | sort -rn | head -10

# Scalability: find nested loops
grep -rn 'for.*{' <target> --include='*.rs' -A 5 | grep -B 1 'for.*{'

# Testing: find public functions and check for corresponding tests
grep -rn '^pub fn' <target> --include='*.rs'
```

These are starting points. Use judgment — not every grep hit is a real finding. Read context around hits before reporting.

### 4. Classify findings

Assign each finding a severity:

- **Critical** — Will cause failures in production, security vulnerability, or data loss risk. Fix now.
- **Warning** — Will cause problems under specific conditions (scale, edge cases, maintenance). Fix soon.
- **Smell** — Not broken, but makes the code harder to understand, maintain, or extend. Consider fixing.
- **Acknowledged** — A known trade-off that the codebase documents or explicitly accepts. Note but don't nag.

### 5. Produce the report

Format output as:

```
🔍 BLINDSPOT REPORT — [target]
   Roast level: [gentle|standard|brutal]
   Analyzed: [N files, M lines]

## Critical (fix now)
- **[category]** `file:line` — [description of the issue and why it matters]

## Warning (fix soon)
- **[category]** `file:line` — [description]

## Smell (consider)
- **[category]** `file:line` — [description]

## Acknowledged (known, accepted)
- [items explicitly documented as accepted trade-offs in the codebase]

---
Summary: N critical, M warnings, K smells
```

### 6. Distinguish real findings from noise

Before including a finding, ask:
- Is this actually reachable? (Dead code unwrap() isn't critical)
- Is there context I'm missing? (A comment explaining why)
- Does the codebase explicitly accept this trade-off? (Move to Acknowledged)
- Would fixing this actually improve the code? (If not, skip it)

The goal is signal, not volume. Ten precise findings beat fifty grep hits.

## RLM dispatch pattern (for large targets)

When the target exceeds 30KB:

1. **Partition** — split files into groups by module or concern
2. **Store** — `shared_state.set("blindspot.group-1", <file contents>)`
3. **Dispatch** — one sub-agent per analysis dimension per file group:
   ```
   "You are analyzing [files] for [dimension]. Report findings as JSON:
   {findings: [{file, line, severity, category, description}]}"
   ```
4. **Synthesize** — collect sub-agent results, deduplicate, rank by severity
5. **Report** — format the final structured report

Hard depth cap: 3. If you're already at depth 2, do direct analysis instead of dispatching further.

## Principles

- **Familiarity is the enemy.** The code's author has read it a hundred times. You're the fresh eyes.
- **Severity over volume.** One critical finding is worth more than twenty smells.
- **Specificity is respect.** "Error handling could be improved" is useless. "`src/tools.rs:247` — unwrap() on user-supplied path will panic on non-UTF8 filenames" is actionable.
- **Acknowledge good decisions.** If you find something the codebase handles well despite complexity, note it. Context for what's working helps prioritize what isn't.
- **Don't nag about accepted trade-offs.** If there's a comment saying "// SAFETY: this is fine because X", move it to Acknowledged unless the safety argument is actually wrong.
