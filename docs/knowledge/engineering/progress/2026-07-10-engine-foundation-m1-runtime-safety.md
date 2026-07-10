---
type: "Implementation Progress"
title: "Engine foundation M1 runtime safety"
description: "Milestone evidence for ADR governance, lifecycle, time, tasks, diagnostics, and filesystem capabilities."
timestamp: 2026-07-10T05:14:35Z
resource: "nara engine foundation"
tags: ["m1", "lifecycle", "time", "tasks", "diagnostics", "filesystem"]
status: "active"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Summary

M1 is active. U1 ADR governance and U2 plugin failure containment are implemented and committed. Read-only implementation audits for U3, U5, U18, and U25 confirmed the planned failures in code and sharpened their canonical replacement contracts.

# Details

## U1 ADR Governance

- Added an ADR catalogue that separates decision acceptance from implementation evidence.
- Added a ledger for the 23 existing ADRs directly touched by the plan; untouched historical backfill remains U20 work.
- Added the pre-1.0 replacement rule: delete obsolete APIs and draft formats, use canonical unsuffixed Rust names, and reset corrected unreleased persistent shapes to canonical version 1.
- Amended ADRs 0043 and 0051 so canonical-version policy is an Accepted decision rather than a README override; migration chains now require a real ADR-backed compatibility window.
- Replaced the mixed history/open-question document with stable `OQ-*` entries carrying owner, trigger, related ADRs, and one unresolved question.
- Added a migration-guide contract and std-only architecture documentation tests.
- Observed verification: `cargo nextest run --test architecture_docs` passed 5/5 on 2026-07-10.

## U2 Plugin Failure Containment

- Replaced `plugins_finished: bool` with explicit configuring, finishing, ready, poisoned, cleaning,
  and cleaned states. A committed build/finish error or unwind panic preserves the first error,
  blocks every mutation/run entry point, and never executes a schedule.
- Added read-only plugin preflight, stable plugin/group dependency-cycle rejection, order-independent
  conflict checks, and a composition-only `PluginGroupBuilder`.
- Retain cleanup ownership before build entry. Cleanup is fallible, panic-isolated, reverse-order,
  once-only, and exposed through a boxed `PluginFailureReport`; cleanup errors never replace setup
  or runner errors.
- Changed runners to borrow `&mut App`, allowing `App::run` to perform explicit cleanup and return a
  prior runner error together with cleanup failures.
- Made mutable App APIs and zero-delta `update` fallible and deleted `try_update` plus the panic-based
  update path. All repository callers and examples use the canonical breaking API.
- Replaced built-in component registration `expect` calls with read-only registry validation,
  fallible commit, and contextual plugin/component errors for transform, render, sprite, hierarchy,
  tilemap, and runtime UI components.
- Revised ADR 0010 and added U2 migration entries. Implementation commit: `2867235`.

### U2 Verification Evidence

- Red baseline reproduced committed build failure still running a frame and finish failure losing all
  cleanup hooks before production changes.
- `cargo nextest run --workspace`: 361/361 passed after final review fixes.
- `cargo check --workspace` and `cargo check -p nara --all-features --all-targets`: passed.
- `windowed_clear`, `windowed_sprites`, and `runtime_ui_panel` winit/wgpu example checks: passed.
- `cargo clippy -p nara_app --all-targets -- -D warnings`: passed after boxing cold-path reports.
- `cargo fmt --all -- --check`, `git diff --check`, architecture documentation tests, and stale-symbol
  searches: passed.
- `wiki_memory.py validate --root docs/knowledge/engineering`: passed; existing legacy records emit
  compatibility warnings but no validation error.
- Two independent read-only reviews found no remaining P0/P1/P2 issue. Their only actionable finding,
  order-dependent conflict validation, was fixed and re-reviewed.

## Confirmed M1 Load-Bearing Findings

- U2 resolved in `2867235`: failed committed hooks now terminally poison, retain the original error,
  and complete reverse once-only cleanup.
- U3: capped fixed work can produce interpolation above one, fixed time is not advanced per tick, paused debt still runs, and completed frames do not clear Bevy trackers.
- U5: task queues are unbounded, closed channels run work on the submitter, panic kills a worker, cancellation can rewrite completed success, and shutdown can join forever.
- U18: runtime diagnostic messages/context/dedupe keys accept unrestricted strings and pressure metrics are not a separate bounded resource.
- U25: canonicalize/strip-prefix/path-then-open checks cannot provide handle-bound containment; a shared leaf `nara_fs` capability adapter is required.

The detailed subagent evidence remains in this session and will be distilled into the owning ADR before each code unit.

# Next Action

Implement the U25 capability-filesystem platform spike, the remaining Wave B unit. U2 verification
also opens U3 fixed time and U5 bounded tasks; those may proceed in parallel only where files and
public contracts do not overlap. The M1 gate remains open until U3, U5, U18, and U25 are verified.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/architecture/adr/README.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `tests/architecture_docs.rs`
- `crates/nara_app/src/lib.rs`
- `crates/nara_reflect/src/registry.rs`
- `crates/nara_winit/src/lib.rs`
- `docs/architecture/adr/0010-plugin-lifecycle-dependencies-and-failure.md`
- `docs/migrations/2026-07-engine-foundation.md`
