---
type: "Verification Evidence"
title: "Workflow refactor recovery and review corrections"
description: "Host cleanup regression verified; independent review corrections and remaining acceptance."
timestamp: 2026-10-22T11:45:55Z
record_id: "43dfe5955a684108a8a65dab7aee5ed3"
producer_id: "codex-root"
run_id: "engine-workflow-2026-10-22"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "3b7b8617462e42a2dade9cff4a5cdb54bf8975d4"
---

# Verification

Continuation of the U1-U9 workflow refactor on the dirty tree based on `3b7b861`.
The historical recovery source is thread `01a15d8d-ceda-7543-8053-2aca71290dfd`;
the receiving thread `01a1629b-6c28-7203-87c5-85328214df4a` owns later continuity.
Do not rerun historical recovery after ordinary compaction.

# Result

The ordinary desktop zero-budget cleanup subprocess regression initially failed because the
process exited before the retained startup-failure owner had been driven once. Both ordinary
desktop and headless loops now drive that owner before checking the cleanup deadline. The
91-test Host cohort passed after this correction. Later review corrections still need verification.

# Evidence

| Scope | Result | Nextest run ID |
| --- | --- | --- |
| External managed Scene travel and EntityRef workflow before nested additions | 14 passed | `9aca4d54-a7e0-4b9c-9baa-398299737299` |
| Zero-budget cleanup subprocess before correction | 1 failed | `3472ef9b-7e48-4b66-8942-b22fb1cb478c` |
| Root content/browser/launch/Scene request and Editor persistence/workspace/runtime boot | 91 passed, 210 excluded | `ca81a535-7ec8-41e4-b065-98c840fa47f5` |
| External dialogue documents/panels and source import, including source namespace corrections | 16 passed | `3b6f9a54-3de4-4480-be96-b01d99c2b4c2` |
| Hidden native source-local EntityRef and independent dialogue document workflows | 2 passed, 198 excluded | `bb7db5ad-f00e-4460-b2c1-2875add55d00` |

The root cohort used `cargo nextest run -p nara --features
tooling,project-workflow,desktop-winit,render-wgpu --locked --lib --test editor_persistence
--test scene_file_workspace --test project_content_boundary --test project_content_delivery
--test project_runtime_boot` with the filter selecting `project_content`, `browser_preparation`,
`product_launch`, `scene_requests`, and the three complete persistence/workspace/runtime binaries.
Cargo commands are serialized and use `CARGO_BUILD_JOBS=1` and the existing root `target`.
These results precede the latest U3 pending-participant, U4 privacy, and U7 source-namespace fixes.
The two later rows qualify their own updated scopes. Native execution used Windows 11, Rust
1.99.0, `WGPU_BACKEND=dx12` and an actual adapter. Hidden windows, Context-owned input and real
submit/present establish only those two affected workflows, not OS input delivery, monitor
scanout, the complete Rust 1.95 release gate, or another platform. The selected tests were
`native_source_local_entity_reference_widgets_save_and_reopen` and
`native_extension_document_widgets_save_reopen_and_retire_panels`.

Independent review confirmed the following corrections, rather than establishing runtime acceptance:

- U4: unknown source kind text and semver prerelease/build text must not become public diagnostic
  identifiers. RON lexical classification and ordinary Open legacy-placement migration facts also
  needed repairs. Source and regressions have been added; execution remains pending.
- U7: Scene/Prefab source locators and project-relative extension locators have different scopes.
  Workspace lookup now compares within the document namespace; Host checks actual project paths
  in both extension and ordinary Open directions. External regressions passed in the later row.
- U3/U8: Winit must consume the same final-failure query as other Hosts. Current pending
  participants must be projected independently of historical errors to identify error-free stalled
  cleanup. The ledger projection is implemented; Host lowering and pending/terminal subprocess
  regressions have been added and need execution.

# Follow-up

Complete the active U5 reviewer/repair lane; run final domain travel and nested/optional EntityRef
external consumers. Run App/Winit, Scene/tooling, affected root cohort, Editor ordinary and the
two new hidden-window native trajectories if later behavior changes invalidate their scope.
The source-built dialogue native test compiled and executed successfully. Update ADR implementation evidence and create precise local
commits only after the relevant current source is verified. The bounded final architecture/API
review and its actionable remaining opportunities still need a final artifact.

Known resumed handles at capture: `01a16311-e22b-7b22-a89d-2c9cd55624e6` (U5 review/repair),
`01a16364-d54e-7121-b46f-112c35b76798` (nested/optional external tests),
`01a16368-3e9d-7653-a488-7b6a4add3689` (pending close projection and Winit).
Disk status is historical evidence; reconcile live tool state before dispatching replacements.

# Citations

- [Workflow plan](../../../../plans/2026-10-22-1100-refactor-engine-workflow-boundaries-plan.md)
- [Catalogue measurement](../../../../benchmarks/2026-10-22-stable-id-delivery-catalogue.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
