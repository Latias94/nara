---
type: "Implementation Progress"
title: "Engine foundation M1 runtime safety"
description: "Milestone evidence for ADR governance, lifecycle, time, tasks, diagnostics, and filesystem capabilities."
timestamp: 2026-07-10T02:58:15Z
resource: "nara engine foundation"
tags: ["m1", "lifecycle", "time", "tasks", "diagnostics", "filesystem"]
status: "active"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Summary

M1 is active. U1 ADR governance is implemented and its focused test passes. Read-only implementation audits for U2, U3, U5, U18, and U25 confirmed the planned failures in code and sharpened their canonical replacement contracts.

# Details

## U1 ADR Governance

- Added an ADR catalogue that separates decision acceptance from implementation evidence.
- Added a ledger for the 23 existing ADRs directly touched by the plan; untouched historical backfill remains U20 work.
- Added the pre-1.0 replacement rule: delete obsolete APIs and draft formats, use canonical unsuffixed Rust names, and reset corrected unreleased persistent shapes to canonical version 1.
- Amended ADRs 0043 and 0051 so canonical-version policy is an Accepted decision rather than a README override; migration chains now require a real ADR-backed compatibility window.
- Replaced the mixed history/open-question document with stable `OQ-*` entries carrying owner, trigger, related ADRs, and one unresolved question.
- Added a migration-guide contract and std-only architecture documentation tests.
- Observed verification: `cargo nextest run --test architecture_docs` passed 5/5 on 2026-07-10.

## Confirmed M1 Load-Bearing Findings

- U2: failed `finish_plugins` can lose the plugin vector and later allow schedule execution; cleanup is forward, infallible, and not panic-isolated.
- U3: capped fixed work can produce interpolation above one, fixed time is not advanced per tick, paused debt still runs, and completed frames do not clear Bevy trackers.
- U5: task queues are unbounded, closed channels run work on the submitter, panic kills a worker, cancellation can rewrite completed success, and shutdown can join forever.
- U18: runtime diagnostic messages/context/dedupe keys accept unrestricted strings and pressure metrics are not a separate bounded resource.
- U25: canonicalize/strip-prefix/path-then-open checks cannot provide handle-bound containment; a shared leaf `nara_fs` capability adapter is required.

The detailed subagent evidence remains in this session and will be distilled into the owning ADR before each code unit.

# Next Action

Commit U1 after diff review, then revise ADR 0010 and implement U2 plugin failure containment test-first.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/architecture/adr/README.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `tests/architecture_docs.rs`
