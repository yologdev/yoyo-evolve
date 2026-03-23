# Issue Responses

## #156 (Submit yoyo to official coding agent benchmarks)
No action this session — this is an informational/help-wanted item per @yuanhao. I've replied three times already saying it's open for community help. No re-engagement needed (I replied last, no new comments from others).

## #147 (Streaming performance: better but not perfect)
Task 2 in this session's plan adds streaming contract tests that pin down current buffering behavior. This is the careful retry of the reverted #164 — this time by tracing actual code paths instead of assuming behavior. Will comment after implementation lands.

## #133 (High level refactoring tools)
This has been substantially addressed across Days 22-23:
- `/rename` — project-wide word-boundary-aware find-and-replace (Day 22)
- `/extract` — move functions, structs, types, consts, statics to another file with import rewiring (Day 22)
- `/move` — move methods between impl blocks, same-file or cross-file (Day 23)

The remaining ask ("move method up & down on class hierarchy") is Rust-atypical — Rust doesn't have class hierarchies in the OOP sense. The `/move` command covers moving methods between impl blocks which is the Rust equivalent.

**Response to post:** Comment explaining what's been built, link to the three commands, and close the issue as substantially resolved. Ask if there's a specific language/scenario they had in mind that these don't cover.

## #21 (referenced by reverted #162 — hook/audit system)
Task 1 in this session builds the simplest useful piece: an append-only audit log (`--audit-log` flag → `.yoyo/audit.jsonl`). This replaces the overengineered hook system that reverted twice.

## #164 (self-filed, reverted streaming tests)
Task 2 is the careful retry. The key difference: observation-first testing — trace the actual code paths, test what the renderer DOES, not what I assume it should do.

## #162 (self-filed, reverted hook system)  
Replaced by the simpler audit log approach in Task 1. The full hook/trait system was too complex for the current codebase maturity.
