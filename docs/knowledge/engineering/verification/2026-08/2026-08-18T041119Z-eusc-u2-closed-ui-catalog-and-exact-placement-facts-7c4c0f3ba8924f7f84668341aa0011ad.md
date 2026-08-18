---
type: "Verification Evidence"
title: "EUSC-U2 closed UI catalog and exact placement facts"
description: "Verifies closed Container/Text/Image/Button recipes, exact source-local structure preflight, semantic brush schema migration, and generation-bound project image choices."
timestamp: 2026-08-18T04:11:19Z
record_id: "7c4c0f3ba8924f7f84668341aa0011ad"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4a6cd14"
verified_by: "codex-root"
---

# Verification

EUSC-U2 was verified at commit `4a6cd14` with Cargo commands run serially from the
repository root and with `CARGO_BUILD_JOBS=1` for the owner suite.

# Result

Passed. `nara_ui` now owns a closed Container/Text/Image/Button authoring catalog, complete
schema-versioned ID-free recipes, deterministic hierarchy classification and display facts, exact
source-local placement candidates, and typed structure preflight. The projection retains the
complete ordered Prefab source topology before UI filtering, so protected roots, descendant
parents, omitted records, nested roots, and stale or missing anchors fail closed.

Persistent `UiVisual` and `UiText` schemas advance to version 2 under one declared predecessor.
Legacy overloaded brush references migrate canonically into independent role and optional semantic
`AssetRef` fields. Canvas compilation admits each component's actual current schema version, the
Reference Game UI Prefab is stored in the successor form, and an image referenced only by a UI
brush remains discoverable through the ordinary project-content closure.

The root Editor Host derives one bounded image-choice set from the immutable
`ProjectContentSnapshot`, binds it to the exact content generation, replaces it only with an
accepted successor snapshot, and carries the same fact in `EditorUiAuthoringFrame`. `nara_ui`
does not import tooling, egui, filesystem capabilities, Editor IDs, persistence, or GPU ownership.

# Evidence

- `cargo nextest run --locked -p nara_ui --test-threads=1` - 80 passed.
- `cargo nextest run --locked -p nara --lib --features
  'runtime-2d,runtime-ui,tooling,serde' -E
  'test(editor_ui_authoring_frame_and_continuous_gesture_share_one_exact_prefab_revision)'
  --test-threads=1` - 1 passed.
- `cargo nextest run --locked -p nara --test project_content_boot --features
  'runtime-2d,runtime-ui,serde' -E
  'test(ui_visual_asset_reference_alone_enters_the_startup_content_closure)'
  --test-threads=1` - 1 passed.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test
  project_content_boot -E
  'test(committed_project_content_boots_from_random_process_environment)'
  --test-threads=1` - 1 passed and reported no source upgrade requirement.
- The owner tests cover all four exact recipes in one representative loop; one placement case for
  root protection, descendant exclusion, exact anchors, and normalized no-op; one absent/stale/
  current image-choice case; one predecessor migration and semantic reference case; and one
  complete-topology omission/nested-root case.
- `cargo fmt --all -- --check` and `git diff --check` passed before commit; the latter reported only
  the repository's existing LF-to-CRLF notices.

# Follow-up

EUSC-U3 owns the exact document/revision-bound structural mutation, Prefab ID allocation,
failure-atomic document/history/selection/allocator publication, Host revalidation, gesture
cancellation, and Canvas retirement. It must consume the closed `nara_ui` facts without copying
catalog recipes, schema paths, placement policy, or image authority into tooling.

# Citations

- [Editor UI Prefab Structural Composition v1 plan](../../../../plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
