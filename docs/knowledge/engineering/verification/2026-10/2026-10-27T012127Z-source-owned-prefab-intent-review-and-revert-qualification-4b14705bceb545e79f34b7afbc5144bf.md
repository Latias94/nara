---
type: "Verification Evidence"
title: "Source-owned Prefab intent review and revert qualification"
description: "Qualifies bounded declaration observation, reviewed exact Revert, rejected composition fencing and explicit property value limits with independent and selected Windows evidence."
timestamp: 2026-10-27T01:21:27Z
record_id: "4b14705bceb545e79f34b7afbc5144bf"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
git_commit: "3636b9a8d8550c5cccac9fd1e8d43b4e6d4d9c3d"
supersedes: "97d2113cb1104680bf471d7d3b7f036f"
---

# Verification

This record qualifies the source-owned Prefab declaration observation/Revert and Inspector
successor in the working tree after `3636b9a`. The commit carrying this record contains that
successor; the frontmatter commit identifies its predecessor, not an already committed test
subject. The complete foundation Goal remains active. This is not conversion, cross-source Apply,
all-platform Editor acceptance or completion of the foundation plan.

# Result

- Scene and Prefab sessions expose bounded complete source declarations and nested snapshot paths
  without resolving inherited sources. Workspace, Session, Application and panel adapters check
  the exact document incarnation, revision and Schema basis before observation or preparation.
- `prepare_prefab_revert` removes exactly one selected stored intent or the complete overlay through
  the existing `SetPrefabInstance` candidate, inverse and history owner. Unknown siblings survive;
  removing the final property preserves the empty overlay/base pin; removing the overlay preserves
  its source link. There is no implicit reference/layout cascade or conversion.
- A private prepared-edit origin admits this narrow repair for UI Prefabs. An otherwise identical
  generic patch cannot acquire it. Rejected preparation/commit leaves source and history unchanged.
- `PrefabExpansionFailureKind` distinguishes semantic rejection, source admission and work limits.
  Established declaration sources, paths and intent slots refer to the actual failing owner.
  Resource/authority errors have no invented repair anchor. The generic diagnostic is
  `scene.prefab-compilation-failed`, with `failure-kind`.
- Authoring capture retains rejected composition separately from native readiness. Exact Schema,
  participating working-copy revisions and file receipts fence its observation. A failed capture
  supplies no native content stamp. Missing dependencies require complete periodic recapture, so
  a newly appearing source can recover without a root edit.
- The Inspector's selection-independent `Prefab Intent` tab pages direct source anchors, exposes
  nested declarations and stored intent, and retains one move-only prepared candidate for review,
  cancel and commit. Exact submission/panel incarnation routing supplies its result. Same-document
  tab navigation preserves that candidate; stale basis or read-only state revokes it.
- Reflect property resolution/owned replacement accepts explicit owner value limits through the
  existing selector and storage walker. A valid large component no longer inherits an unrelated
  fixed 16,384-node ceiling. Actual node/byte/depth refusals map to `WorkLimit`; semantic selector
  and value errors remain distinguishable.

# Evidence

All Cargo work was serial, using Rust 1.95.0, `CARGO_BUILD_JOBS=1` and the checkout's existing
`target`. The following runs include the final property-budget correction. Cohorts overlap and
must not be added into a unique-test count.

| Scope | Actual result |
| --- | --- |
| Focused explicit property-limit regressions | `cdd4d713-7a81-4cbc-af03-be837df8950b`: 7 passed; build 4.84s, execution 0.358s |
| Reflect/Scene/Tooling, all features | `3334f1a1-3f42-4392-bd39-df86627df929`: 912 passed, 4 ignored; build 5.35s, execution 41.975s |
| Root/Editor ordinary libraries, all features | `5fe09645-ba16-4047-b4ca-27b1ecdf267f`: 770 passed, 34 ignored; build 47.75s, execution 189.495s |
| Selected actual Windows/DX12 | `156b70c4-f754-44a6-835d-158fba3b060f`: 6 passed; build 0.84s, execution 10.938s |
| Workspace all-feature/all-target check | Passed; final build 1m18s |
| Independent module consumer | `5a0b61b1-aa3d-466d-8bbe-1f0af26d258f`: 19 passed across four binaries; build 1m03s, execution 1.301s |
| Selected Reference Game | `135e28cf-e6b0-466f-b9b4-a84a7dc509ad`: 13 passed across seven binaries; build 1m30s, execution 1.758s |
| Minimal Scene | `cargo +1.95.0 check -p nara_scene --no-default-features --lib --locked`: passed; final confirmation 0.47s |
| Formatting and whitespace | `cargo +1.95.0 fmt --all -- --check` and `git diff --check`: passed |

The independent consumer executes `prefab_sources`, `property_authoring`, `durable_identity` and
`nested_schema` under `tests/fixtures/module-consumer/Cargo.toml`, sharing the root target. Its
source observation/Revert/Undo/Save/Reopen workflow covers both Scene and Prefab and verifies
preserved unknown intent. The Reference Game selection is `prefab_startup`, `project_content_boot`,
`runtime_core`, `runtime_drive`, `public_surface`, `ui_authoring_roundtrip` and
`vec2_schema_migration`.

The six native cases are:

- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
- `native_ui_canvas_priority_survives_closing_retained_scene`
- `native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes`

These are actual hidden-window DX12 presentation/input trajectories. The new Prefab Intent
Review/Cancel/Commit mouse workflow instead runs through the owned Dear ImGui test host. Neither
that widget evidence nor the six native cases proves the complete nested dropdown, Undo/Redo,
Save/Reopen control chain on a native desktop. Existing unused/dead-code compiler warnings remain;
ignored tests are not counted as passes.

Independent source/Host and frontend reviews drove the typed attribution, missing-source recovery,
tab hitbox and candidate-retention corrections recorded by the preceding progress shards. Final
read-only reviewers Bohr (`01a17af0-3bb9-78a3-bc04-3381103a403b`, explicit property limits and
failure mapping) and Hooke (`01a17af0-7156-73e0-b9d1-a2bd56573087`, Inspector/Host stale-candidate,
result and capture handling) found no further actionable defects in their assigned scopes.
They did not independently rerun tests. Both reviewers are closed. The parent confirmed minimal
Scene, formatting, whitespace and all 981 local Markdown targets in the 15 then-changed document
paths. Memory render/validation passed with existing advisory warnings about historical records
and local absolute paths; no unrelated historical cleanup was attempted.

# Follow-up

- Navigate directly from a canonical foreign-source failure to that source's current session.
- Qualify the complete nested declaration selection/Revert/Undo/Redo/Save/Reopen control trajectory.
- Implement source-owned convert-to-local with complete identity, hierarchy and reference remap;
  qualify cross-source Apply separately.
- Move the direct UI Canvas helper onto shared capture and expose inherited Inspector selection.
- Address aggregate repeated Schema/value work through its actual owner, without a second parser
  or generic budget framework.
- Continue the dependency-ordered foundation waves. No push, PR, remote publication or foundation
  completion is authorized by this evidence.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Prefab design](../../../../architecture/prefab-overlay-design.md)
- [Prior Editor capture qualification](2026-10-26T221436Z-captured-editor-scene-preview-and-ordered-image-publication-qualification-bd36086e52524bb1b227dbd21da81056.md)
- [Source Revert integration](../../progress/2026-10/2026-10-26T234134Z-source-owned-prefab-revert-integration-and-review-corrections-c8fb754f923b49de9812cb196b66a3d7.md)
- [Property-budget correction](../../progress/2026-10/2026-10-27T003831Z-caller-owned-property-value-budget-correction-in-progress-97d2113cb1104680bf471d7d3b7f036f.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
- [Unity/Godot ownership study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
