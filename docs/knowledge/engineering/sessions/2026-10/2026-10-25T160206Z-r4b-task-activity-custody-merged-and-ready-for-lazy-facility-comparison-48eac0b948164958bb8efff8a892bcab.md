---
type: "Session Handoff"
title: "R4b task activity custody merged and ready for lazy facility comparison"
description: "Pins the qualified optional custody substrate to local main while preserving the full foundation Goal and remaining workload/product selection."
timestamp: 2026-10-25T16:02:06Z
record_id: "48eac0b948164958bb8efff8a892bcab"
producer_id: "codex-root"
run_id: "foundation-task-scopes-20261025"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "main"
git_commit: "375c80b"
---

# Summary

The full foundation Goal remains active, without a budget or scope reduction. The first optional
R4b activity-custody unit is committed as `375c80b refactor(tasks)!: separate activity custody from
physical facilities` and fast-forwarded to local `main`. The source commit is
`375c80b1228e62ed73af5c87b303c7d35bfb65ce`. No remote publication. Only pre-existing untracked
`vendor/` remains; it and read-only `repo-ref/` were untouched. The completed local branch
`refactor/task-activity-scopes` still points at the same source commit.

# Verified State

- Final Tasks/Image all-features: 189 passed, nextest
  `23a53269-aed2-4b6d-b564-3c8662b1055c`; includes seventeen scope tests and two actual PNG reload
  consumers with identical AssetId/coalesce keys across two managed Runtime owners.
- Independent renamed-root/direct Tasks consumer: 1 passed, nextest
  `133d9389-42bd-42e8-b005-10fc26762bbd`.
- Final workspace all-features/all-targets locked check passed; focused all-feature/all-target
  correctness/suspicious Clippy passed; root and independent fixture format checks, changed
  Markdown links, whitespace and memory validation passed. Existing warnings remain.
- Cargo was strictly serial, Rust `+1.95.0`, jobs1, existing checkout `target`. No Cargo/rustc/nextest
  process remained at handoff. The previously confirmed C: PDB cleanup stays complete; no D:
  directory belonging to another project was removed. All new build products stay on F:.
- Independent design review found one P1 reaper-retry gap, fixed and rechecked. A separate
  correctness reviewer found no other confirmed defect and rechecked requested test additions
  plus the actual Image consumers. All three agents completed and were closed.

# Open Threads

- `TaskFacilities` explicitly owns physical pools. `TaskScope` produces scope-bound `TaskPools`
  facades without worker-close authority and itself implements `RuntimeCloseParticipant`.
- Scope admission takes its gate before the executor queue, registers return before publication,
  and drops locks before user disposal. Coalescing includes exact scope identity; completion
  cutoffs have per-scope tokens while IDs remain facility-local.
- `TaskReturnObservation` tracks constant-space aggregate pending/cleanup failure without owning
  task output. Successful physical-return CAS decrements once after actual disposal. Close borrows
  queue controls in place, preserves sibling queue order and sends own pending captures to the
  existing retained disposal lanes. It does not wait behind unrelated blocked workers.
- Reaper startup failure retains custody; scope poll now retries the supervisor. Isolated test
  override proves fallback recovery by polling alone and capture destruction exactly once.
- Managed scope close publishes `TaskScopeCloseReport`, not `TaskShutdownReport`. The latter
  remains physical worker/disposal evidence. Sticky failure is independent of physical completion.
- Shared plugin instances retain the explicit strong facilities Arc. The final plugin/Runtime
  can trigger physical Drop fallback if the outer Host released its owner early. Manual scopes
  still require a retained facilities owner. Existing arbitrary completed-result destructor
  latency is not made bounded. Published outputs/escaped independent work keep separate owners.
- The actual Image consumer keeps one of two I/O workers runnable: it proves independent close
  and real result application, not total-saturation fairness, queue latency or performance gains.
- `TaskHandle::admission_to_start` measures accepted queue admission through execution
  authorization, not spawn-call wall time. `TaskPoolStats::live_workers` is physical aggregate
  observation. No benchmark or default product policy was selected.

# Next Action

1. Recheck worktree/processes, retain the same target and continue R4b. Compare a genuine lazy
   per-Runtime facility candidate with Host facilities/scopes on the same consumer, including
   first-use activation and admission-to-start/interactive completion/worker multiplicity. Do
   not call the former eager allocation a lazy control or treat new instrumentation as evidence.
2. Integrate the selected policy through the trusted compiled Editor authoring/preview and
   same-process Play consumer. Existing ordinary supervised Play is a separate process and is
   not shared-facility proof. Source selection: `src/project_host/runtime/tests/editor_scene_travel.rs`.
3. Before wiring product construction, reconcile the bridge: `PluginPlan::instantiate_runtime_candidate`
   currently creates App itself before plugin preparation; pre-inserting `TaskPools` skips default
   managed ownership and is not a valid integration. Keep physical policy Host-owned without
   making a process-global service registry or hiding runtime-local admission in a fingerprint.
4. Keep Save/Reopen file custody, Image completion cutoffs and Scene-request physical return.
   `SceneRequest` close cancellation currently relies on the managed Tasks owner for actual return;
   a shared deployment must retain that scope evidence. Later Runtime starts must not silently
   reconfigure facilities selected by an earlier Host.
5. R4c, R3, broader R1/R2/R5 and assets/delivery/render waves remain open under the complete plan.
   Do not mark the Goal complete or give a global percentage based on these test counts.

# Citations

- [R4b verification](../../verification/2026-10/2026-10-25T155728Z-independent-task-activity-custody-and-shared-image-runtime-qualification-13caef5f8c39471aa58529789ae9c530.md)
- [Activity design](../../../../architecture/task-activity-scope-design.md)
- [Master plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
