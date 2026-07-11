---
type: "Decision"
title: "Exact fixed-step time ledger and checkpoint boundaries"
description: "Freezes paused exact-step clock accounting and separates completed-tick observation from stable checkpoint eligibility."
timestamp: 2026-07-11T03:49:11Z
record_id: "5cf371cef1324269870794cc615858f0"
producer_id: "codex-root"
run_id: "recovery-019f4918-177d-70b1-9694-619ba1df9f2b"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Decision

Paused fixed-tick stepping uses a dedicated exact-step frame plan. For fixed timestep `H`, Real
time advances from the runner's actual delta, Virtual time advances exactly `H` as an explicit
step override, and Fixed time advances one tick and `H` elapsed time. The plan injects and consumes
the same `H`, preserving all pre-existing fixed pending time, debt, remainder, render interpolation,
pause state, and time scale. Ordinary max-delta, catch-up, debt-limit, and discard policy cannot
change the one-tick result.

`FixedTickCompleted` is a per-tick observation boundary, not automatic checkpoint eligibility. The
initial checkpoint slice waits for a stable paused `AppFrameCompleted` boundary and separately
proves gameplay queue health/acknowledgement plus service quiescence. Bevy change/removal trackers
remain app-frame scoped and cannot be used to infer per-tick diffs.

# Context

Current `App::run_once` derives zero-to-many fixed iterations from `pending + virtual_delta`, then
settles catch-up debt only after all iterations. It has no exact-step API. Temporary resume or
`run_once(H)` can therefore execute zero or multiple ticks, reject on debt pressure, or discard
retained debt. Runtime frame status is published after the whole fixed loop, and trackers are
cleared only after all later Core stages complete.

# Alternatives

- Advancing Fixed time alone was rejected because it permanently separates Virtual and Fixed
  elapsed time and leaves ambiguous whether existing debt was consumed.
- Treating every fixed-schedule return as a recoverable checkpoint was rejected because the app
  frame, catch-up accounting, later Core stages, and native services may still be open.

# Consequences

- U16 needs an exact-step planner rather than a run condition layered onto normal elapsed-time
  planning.
- U16 tests must cover Real/Virtual/Fixed fields, pending/debt/remainder/interpolation preservation,
  gameplay Admit/Consume/Capture/Acknowledge, tracker rotation, and Paused-to-Stepping-to-Paused.
- Future checkpoint formats register canonical semantic time state and execution mode rather than
  persisting frame-internal implementation flags.

# Citations

- `docs/architecture/adr/0039-main-loop-time-pause-and-runtime-state.md`
- `docs/architecture/adr/0076-play-runtime-debug-control-and-observation.md`
- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md` U16
- `crates/nara_app/src/lib.rs#TimeFramePlan`
- `crates/nara_app/src/lib.rs#App::run_once`
