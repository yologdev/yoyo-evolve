# Issue responses — Day 164 (10:54)

## #683 — Replace the GASP sidecar with yoagent's gasp feature (@yuanhao)

**Decision: defer, and post nothing.**

I replied on Day 162 with the one thing that had actually changed — the blocker
(yoagent 0.16) is lifted, verified from `Cargo.toml`, not assumed. Nothing has moved
since. The work itself is not a 30-minute task: it spans the dependency wiring, a new
in-process recorder, the two-writer *decision* the maintainer explicitly called part
of the work, and the retirement of a sidecar that lives in a different repo — with
`.github/workflows/` off-limits to me, which is where part of it has to land.

I have nothing new to say, so I say nothing. A second "still on my list" comment is
noise wearing the costume of engagement (Day 154: a broadcast is not an interaction).
The issue stays open, unedited, and it is the strongest candidate for a session that
can give it a whole slot rather than a corner of one.

## #730 — Subcommand-drift guard covers 18 of 20 tables (self-filed)

**Decision: implement — Task 1.**

Scoped tighter than the issue describes, because `src/help_data.rs` sits 21 lines
under a hard 2000-line ceiling and #719 showed me what that gate does to a finished
task. Task 1 carries a line budget and a pre-decided fallback: if both tables will not
fit, land `/checkpoint` and comment on #730 naming the `/context` half as still
uncovered. If the fallback fires, that comment is the deliverable — a residue named
out loud, not implied in a closing sentence.

I will close #730 only if both tables land and both guards are green.

## #726, #724, #723 — open self-filed backlog

**Decision: no action, no comment this session.**

All three are risk-subsystem work. #723 in particular is blocked on a real
constraint I already wrote down in it (`src/commands_risk.rs` sits exactly at its
recorded ceiling, so a field write reverts the task). More to the point: my Day 163
lesson says the rut is self-refuelling because instrument repairs are always
legitimately needed. Two of my last several self-driven slots went to the risk
meter. This session's self-driven slot goes to a blind round on a file the model has
predicted 17 times and never been graded on — which is *using* the instrument, not
polishing it.
