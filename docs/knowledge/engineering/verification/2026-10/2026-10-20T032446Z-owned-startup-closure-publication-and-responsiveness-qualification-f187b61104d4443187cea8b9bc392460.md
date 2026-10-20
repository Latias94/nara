---
type: "Verification Evidence"
title: "Owned startup closure publication and responsiveness qualification"
description: "Bounded background startup preparation preserves linear Open ownership, owner session publication, cancellation and retained project replacement."
timestamp: 2026-10-20T03:24:46Z
record_id: "f187b61104d4443187cea8b9bc392460"
producer_id: "codex-root"
run_id: "editor-owned-startup-20261020"
related_plan: "docs/plans/2026-10-20-owned-editor-startup-closure-plan.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "eb1ee98"
---

# Verification

Windows 11, Rust 1.95.0. Shared target and CARGO_BUILD_JOBS=1, no concurrent Cargo.
Starting source: `c369099`, following Host lifetime correction `fe3ad96`.

## Result

The existing linear Open attempt moves capability-bound startup reads, decoding, dependency
traversal and pure schema classification to a lazily created, reusable TaskPools owner. Each pool
kind has one worker and four pending slots. Five physical preparation-capture permits prevent
logical cancellation from admitting unlimited unretired work. Caller-owned completed candidates
retain their existing per-candidate budgets; this is not a global completed-value quota.

Only an actual Completed result may advance the live attempt to receipt admission. The task never
contains an Editor session. Manifest/schema admission, Adapter selection, native Plugin/registry
construction, session/runtime creation and Workspace publication remain owner work. Attempts can
still be transported between Open Hosts: each carries its original task/result, not a foreign
completion mailbox. Attempt Drop requests cancellation; standalone task reaping retains running
captures without waiting for non-preemptible reads.

The private application reports actual phase changes. Repeated StartupClosure Pending polls do not
request presentation; existing native 16 ms Opening maintenance still observes completion.
Replacement retains the original project and keeps persistence-before-preparation and
Save-before-refresh ordering. Public Open consumers now wait for a bounded terminal result rather
than assuming a fixed number of phase advances.

## Evidence

- Final engine/public cohort: **131 passed**, 81 excluded;
  `26fc26cb-98ea-4212-bec6-b7a14c2298b3`. Includes lower persistence, root Editor runtime,
  private Open/background persistence tests, `editor_persistence`, `workspace_play_runtime`.
- Editor/application/panel cohort: **195 passed**, 20 native tests excluded;
  `4ea30c37-cde9-4daf-8d62-5ec368c09db8`. Library plus `application_contract`, `authoring_loop`,
  `first_game_workflow`, `generic_component_composition`, `generic_open_recovery`,
  `registered_panel_interactions`.
- Selected actual Windows/DX12 native cohort: **4 passed**;
  `cf742252-9a74-4639-93a1-0725eb409594`: project Manager recovery/cancel/replacement/dirty close,
  ordinary Scene/Prefab Save/Reopen, Canvas pick/move/resize/Save/Reopen, paused-resume redraw.
- Independent Scene module consumer: **3 passed**;
  `a7dcd3c0-bce2-4eed-b157-fd1b2ca1fadf`, `component_inventory`.
- Independent Dear ImGui panel consumer: **2 passed**;
  `0e55e658-6483-4a67-9514-6e8a02b0ba3a`, `imgui_contribution`.
- Reference-game generic Open and supervised Play: **4 passed**;
  `5fced34a-b33f-451b-8ddf-57c5ac7bb45d`. The actual Play/Restart/Stop/Exit test passes in
  386.505 seconds, including a real child build; no concurrent verification Cargo was started.
- Runtime-UI-off public consumer: **1 passed**;
  `b03d3e8b-55a1-4e89-b021-9f114d6c71f7`, using features
  `project-workflow,tooling,presentation-2d`. An earlier invocation accidentally enabled runtime-ui,
  selected zero tests and was corrected; it is not passing behavior evidence.
- Workspace locked all-features/all-targets check: passed in 70 seconds. Formatting and diff checks
  pass. Initial compile attempts found an in-progress missing test file and a missing test-only
  Duration qualification; both were corrected before the final passing cohorts.

Six deterministic private worker tests cover held preparation without blocking owner polling,
cross-Host attempt transport, deferred owner receipt admission, attempt/Host Drop before capture
retirement, running plus queued logical cancellation retaining all five physical permits, queued
cancellation without execution, closed-pool rejection, and safe worker-panic diagnostics. The
Drop test proves capture/task retention, not OS-call interruption or a directly measured handle
lease. RAII gates release during failure unwinding.

Application regressions cover quiet first launch, initial Cancel, exact retained-frame/generation
preservation after replacement Cancel, and Exit during initial/replacement preparation. Existing
unknown-record preservation, limits, source binding, dependency closure and Save/Reopen owner
regressions remain in the passing cohorts. An independent review of implementation and tests
finds no current reachable blocking defect.

## Measurement

The [v4 report](../../../../benchmarks/2026-10-20-owned-editor-startup-closure.md) owns optimized
first/max owner-call, completion-observation and prepared-receipt disposal measurements. Three
optimized processes pass with 864 raw rows each and 162 summary groups. Independent recomputation
matches all medians, nearest-rank P95/max values and 21 report cells, including the historical v1
comparison. At 4,096 empty entities, startup maximum owner-call P95 is 0.066 / 0.081 / 0.077 ms;
completion observation remains about 11 ms and full Open about 20-22 ms. The observation includes
polling and measurement output, not pure worker CPU or native UI response. Existing v1/v2/v3 data
stays frozen. Final implementation source is `eb1ee98`.

## Limits And Follow-up

These selected development results do not establish cross-platform release support or a native
frame-time SLA. Counts overlap. The executor still runs blocking closures; no generic Future
executor, async Plugin/codec trait, stronger cross-file snapshot or CAS is introduced. Failed or
cancelled work can retain captures while an OS read runs. Untaken completed-candidate destruction
may execute on the caller; receipt-disposal timing has its own observation point and does not
certify arbitrary task-result destructors.

After the selected startup trigger, Browser admission, bound Play receipts, compiled content
refresh, task capture disposal and high-batch GPU retention require separate consumer measurements.
Multiple same-kind documents, code freshness and Tile tooling remain separate capabilities.

A final same-owner sweep confirmed a separate Image metadata cleanup gap: terminal failed
publication retains its FilePublicationHost only in the session's image-import projection. That
authority is not yet registered with runtime custody and can be lost on session Drop, including
after the runtime has stopped during explicit close. The
[Image cleanup custody plan](../../../../plans/2026-10-20-editor-image-publication-custody-plan.md)
prioritizes that correctness repair next; startup preparation does not repair or qualify it.

## Citations

- [Plan](../../../../plans/2026-10-20-owned-editor-startup-closure-plan.md)
- [Worker and deterministic tests](../../../../../src/project_host/runtime/editor/open/preparation.rs)
- [Measurement consumer](../../../../../tests/support/editor_responsiveness_measurement.rs)
- [Host lifetime qualification](2026-10-20T030524Z-editor-host-lifetime-cleanup-custody-qualification-12c6b025d6c74ed1bf6900c8b0413877.md)
