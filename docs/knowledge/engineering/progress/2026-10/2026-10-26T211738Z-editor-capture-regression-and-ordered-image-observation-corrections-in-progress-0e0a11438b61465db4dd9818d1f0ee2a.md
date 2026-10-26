---
type: "Work Progress"
title: "Editor capture regression and ordered Image observation corrections in progress"
description: "Preview cohort passes 60 cases; complete root and Editor run finds five failures, with source selection and Image evidence corrections still being integrated."
timestamp: 2026-10-26T21:17:38Z
record_id: "0e0a11438b61465db4dd9818d1f0ee2a"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
supersedes: "068027a8af994642921dff01067fe9e6"
---

# Summary

The complete foundation Goal remains active. The declarative Prefab replacement and actual Editor
capture are still uncommitted on `refactor/declarative-prefab-overlays`, based on local main
`47a2000`. No merge, push or PR occurred. Preserve all authorized WIP, untouched `vendor/` and
read-only `repo-ref/`. Cargo is serial, jobs=1, Rust 1.95.0, existing root target.

# Details

## Executed Verification

- Current Editor all-feature compilation passed in 0.74 seconds before subsequent review fixes.
  The previously unknown build had no surviving Cargo process; it was not left running.
- Focused root all-feature preview/capture/source-receipt cohort:
  `cargo +1.95.0 nextest run -p nara --all-features --locked --lib -E
  'test(scene_capture::tests::) | test(preview_content_tests::) | test(source_receipts) |
  test(source_observation) | test(independent_capture::) | test(capture_hydration)'
  --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail`.
  Run `7ff630f0-d992-497f-9cec-94dd46825217`: **60 passed, 475 filtered**;
  build 49.62 seconds, execution 11.088 seconds. Includes actual second-Scene PNG hydration
  against the captured asset database. This predates the new selection assertion and later
  ordered Image corrections.
- `cargo +1.95.0 fmt --all` passed after all previous writers froze. Later changes need formatting
  again after current writers freeze; do not format their active files.
- Full ordinary root/Editor:
  `cargo +1.95.0 nextest run -p nara -p nara-editor --all-features --locked --lib
  --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail`.
  Run `e55fc8e8-0b37-4eee-9340-c5567043918f`: **744 passed, 5 failed, 34 skipped**;
  build 6m47s, execution 209.024 seconds, terminal exit 100. It sampled source before the
  selection correction, old test migration and dead retention-helper removal.
- Failures: two Image cases did not settle (`generic_editor_failed_image_revocation_settles_catalog_and_blocks_metadata_writes`,
  `generic_editor_rejects_a_malformed_tileset_before_a_late_image_can_publish`); one root Image
  test and two Editor persistence tests assumed an immediate initial preview.
- Core-only all-feature Scene/Tooling/UI nextest is currently running, exec session **56619**,
  logging to the local temporary `nara-overlay-current-scene-tooling-ui-tests.log`.
  It does not compile the Image worker's active root files. Check terminal state before any
  subsequent Cargo command.

## Independent Findings And Corrections

Bernoulli found a confirmed source-selection defect: Prefab Sprite hit facts published a materialized
locator to Workspace SelectEntity, which rejects it. Parent now maps typed provenance to the
containing instance root anchor, or the direct source-local ID. The actual second-Scene PNG test
dispatches the resulting action and checks accepted selection; the native binding fixture moves
the Sprite into a real Prefab and checks the actual presentation pick and submitted selection.
These new assertions have not yet compiled. Inherited subentity Inspector/selection remains open.

Kepler found three additional Image defects after the preceding worker's correction:

1. Rechecking only an Image's own unchanged sidecar can permanently ignore a later global
   StableAssetId conflict.
2. An old physically completed Recheck success can consume a newer catalog deletion observation
   while explicit import has paused hydration.
3. Pixel reuse drops a new capture's updated metadata receipt, so an old catalog can revoke the
   still-valid cached Image.

The full regression also confirmed that failed asynchronous revocation requeues forever instead
of latching ADR 0115's terminal runtime-publication failure. The Image fix worker owns these
corrections together. No passing result for that successor is claimed.

Parent removed the unused direct-only production `project_scene` adapter; its omission test now
calls the Scene owner directly. Parent also removed the old Host patch-retention calculator,
its test-only wrapper and tests which were its only consumers, and unused file `open_target`.
Actual overlay retention and the Scene owner's exact snapshot/history accounting remain.
The root Image retirement test now waits for initial capture and permits an explicitly revoked,
unauthorized preview instead of requiring a valid fallback packet after source rejection.

ADR 0047 now adopts actual-demand open working copies for Editor preview, independent file
evidence, async validation, complete content authorization and current instance-anchor selection.
ADR 0115 states ordered catalog reconciliation, conflict-member proof and source-receipt refresh
independent of pixel reuse. These are contracts, not current passing qualification.
The overlay design now marks its removed comparison compiler as historical and distinguishes
integrated Scene capture from the still-direct UI Canvas path.

## Active Write Sets

- **Kepler** `01a17a02-7226-7120-90cf-e50a7978bd8d`: fix worker,
  `src/project_host/runtime/editor/image_import/{hydration,transaction,store,capture_hydration,catalog}.rs`
  and private related tests only. No Cargo/fmt/commit. Must retain bounded conflict-member
  evidence, observation ordering, cached receipt refresh and exact terminal revocation failure.
  Do not overwrite these files until frozen.
- **Bernoulli** `01a179ff-3460-72f2-a905-180828fbdcaa`: narrow read-only selection recheck, then
  test-only writer for `editor/src/app/persistence_tests.rs`. Two legacy initial-preview
  assumptions must wait before the exact once-drive replacement assertions. No Cargo/fmt/commit.
- **Chandrasekhar** `01a179cd-5f3c-78d2-9394-e3736e8c749a`: completed and CLOSED; predecessor
  Image correction is now superseded by Kepler's findings.
- Parent owns all other integration changes, especially `preview.rs`, actual Scene capture tests,
  native `scene.rs`, root `authoring_images.rs`, ADRs and this memory integration.

# Next Action

1. Observe exec 56619 to completion. Receive both worker freezes, inspect corrections and
   format only after all write sets freeze.
2. Run the focused Image/Scene capture/Editor persistence cohort, including actual selection,
   conflict membership, overlapping observations, cached receipt refresh and terminal failures.
3. Re-run full affected root/Editor regression; run selected Windows/DX12 retained Scene,
   Canvas-priority and actual image-pixel trajectories. Current thread has no new native result.
4. Refresh current independent/public consumer, Reference Game and no-presentation evidence
   as appropriate before a coherent local commit. Do not use the earlier 60 or 744 results to
   qualify later fixes.
5. Remaining production Prefab work includes conflict observations/repair, source-owned
   convert-to-local, shared UI Canvas capture and inherited-entity Inspector/selection. Repeated
   Schema descriptor work and the complete foundation waves also remain open.

# Citations

- [Predecessor integration](2026-10-26T201651Z-editor-background-scene-capture-and-native-content-stamp-integration-in-progress-068027a8af994642921dff01067fe9e6.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0115](../../../../architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md)
- [Unity/Godot ownership study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
