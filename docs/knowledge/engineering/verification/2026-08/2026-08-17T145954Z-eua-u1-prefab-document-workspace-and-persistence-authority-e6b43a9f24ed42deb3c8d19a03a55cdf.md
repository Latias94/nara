---
type: "Verification Evidence"
title: "EUA-U1 Prefab document, workspace, and persistence authority"
description: "Verifies true path-bound UI Prefab document ownership, exact anchor navigation, kind/source-bound persistence, and Scene regression coverage."
timestamp: 2026-08-17T14:59:54Z
record_id: "e6b43a9f24ed42deb3c8d19a03a55cdf"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "dc7ea07"
verified_by: "codex-root"
---

# Verification

EUA-U1 was verified against commits `d156415`, `2cc334d`, `7974c54`, and `dc7ea07` with Cargo
commands run serially from the repository root.

# Result

Passed. A path-bound UI Prefab is now a nominal `PrefabAuthoringSession` and closed Editor document
kind with its own revision, history, selection, dirty checkpoint, source binding, Save/Reopen lane,
and canonical Prefab writer. Scene-only mutation and Play paths remain kind-bound.

Host source navigation requires the exact Scene document/revision and currently selected Prefab
anchor. The path must belong to the admitted project-content closure, stable IDs remain rejected,
and a schema-valid non-UI Prefab is rejected without publishing a document. Persistence receipts
cannot cross Scene/Prefab kinds or Prefab source bindings.

# Evidence

- `cargo nextest run -p nara_tooling` — 28 passed.
- `cargo nextest run -p nara_tooling_egui` — 13 passed as a regression check only; it is not the
  proof of document authority.
- `cargo nextest run -p nara --lib --features runtime-2d,runtime-ui,serde,tooling,presentation-2d editor_prefab_lane_saves_and_reopens_the_exact_source_document`
  — passed the selected-anchor, non-UI rejection, edit, Save, Reopen, close, reopen-source, and
  multi-document Exit trajectory.
- `cargo nextest run -p nara --lib --features runtime-2d,serde,tooling,presentation-2d prefab_lane_saves_and_reopens_the_prefab_source_without_scene_projection`
  — passed kind/source-bound persistence and canonical roundtrip checks.
- `cargo nextest run -p nara --test editor_persistence --features runtime-2d,serde,tooling,presentation-2d persistence_authority_surface_is_opaque_and_linear`
  — passed the compile-fail authority boundary.
- `cargo check -p nara --tests --features runtime-2d,serde,tooling,presentation-2d`,
  `cargo check -p nara-editor --tests`, and
  `cargo check --manifest-path reference-game/Cargo.toml --features editor --test editor_journey`
  — passed affected consumer compilation.
- `cargo fmt --all -- --check` and `git diff --check` — passed before commit.

# Follow-up

EUA-U2 owns the immutable UI authoring projection, typed field descriptors, layout ownership,
source map, and bounded Begin/Preview/Commit/Cancel gesture transaction. It must not move egui or
WGPU identities into the shared authoring seam.

# Citations

- [Editor UI Prefab Authoring and Canvas v1 plan](../../../../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
