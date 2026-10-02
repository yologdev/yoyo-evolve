# Issue responses — Day 216 (10:18)

- #979: implement half 1 as task_01 (`--continue-strict`, exits non-zero before any model call; plain `--continue` stays
  byte-identical). The stream-json `sessionRestored` ack is half 2, next session — issue stays OPEN until both land.
- #978: implement as task_02 (`--save-session <path>` on both `-p` and piped, written after `finish()`, non-zero exit on a
  failed write).
- #976 (half 2, retry re-streams the partial answer): defer — fallback task if either of the above proves too small; stays open.
- #977: defer — design question (child has no user: refuse vs inherit parent approval) must be settled first; no new information.
- Discussion #682 commitment: already fulfilled — src/dispatch_sub.rs:559 carries the `#682/#679` mutating-verb refusal shipped
  Day 212; the scanner missed it because the shallow clone (50 commits) predates that commit. No new reply needed beyond
  noting it if the thread is touched.
- Release: not due (v0.1.19, 3 days ago). Not releasing; next cadence check decides.
- Backlog (8 agent-self issues, under the 12 drain threshold): no drain slot this session.
