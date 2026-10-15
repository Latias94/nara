---
type: "Subagent Finding"
title: "Quiet supervised Play needs effects only exit observation and current phase deadlines"
description: "Focused U12 review identified missing silent exit observation and two repeated redraw causes."
timestamp: 2026-10-15T16:03:16Z
record_id: "db7102914b24438b9767824c3cf7ebe4"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "u12_responsiveness_review"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
---

# Finding

Focused read-only U12 review found three concrete responsiveness defects. It is not the full-plan
ce-code-review receipt and ran no Cargo or formatting.

# Evidence

1. Windows observes child exit only through nonblocking WaitForSingleObject(process, 0); Linux
   observes it only through UI-owned waitid(EXITED | NOHANG | NOWAIT). Output notifications do not
   prove exit. A child can close both output streams before exiting, or exit while a descendant
   retains pipe writers. Without a deadline or exit watcher, quiet Running can remain forever.
2. Process next_deadline returns the minimum of all historical cleanup deadlines. After moving
   into ForceKilling/Draining, an expired terminate deadline remains the minimum. A retained
   CleanupIncomplete owner can therefore expose an expired deadline indefinitely.
3. Session/replacement close-wait paths report actions_observed=true on every call, even when
   waiting for an explicit retry. Native timeout handling also unconditionally requests redraw.

# Recommendation

Reuse the existing nonblocking exit observation with one stable low-frequency process deadline.
Only expose deadlines active in the current cleanup phase; suppress automatic retries while
cleanup is explicitly incomplete. Keep repaint deadlines separate from owner effect deadlines,
combining them only for event-loop scheduling. An effects-only wake advances the retained owner
and requests redraw only after a visible model change. No platform exit thread, generic scheduler,
second reaper, or job framework is required.

# Disposition

Accepted for the current U12 unit. Root inspected the cited process/supervisor/application/native
paths and added focused process deadline regressions; implementation and tests are still pending
at capture time. Earlier Host duplicate-observation and locked-close-UI regressions already
demonstrated their intended failures. The new generation preparation worker remains unverified.

# Citations

- editor/src/supervised_play/process/mod.rs
- editor/src/supervised_play/process/windows.rs
- editor/src/supervised_play/process/unix.rs
- editor/src/supervised_play/supervisor.rs
- editor/src/app.rs
- editor/src/platform.rs
- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U12
