---
type: "Verification Evidence"
title: "EUA-U2 toolkit-neutral UI prefab authoring and gesture conformance"
description: "Verifies the immutable UI authoring frame, typed descriptors, complete component validation, source map, and bounded continuous gesture transaction."
timestamp: 2026-08-17T17:14:53Z
record_id: "19920c861d394a4fb43db6f60e4cf2a5"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "16b0d30"
verified_by: "codex-root"
---

# Verification

EUA-U2 was verified at commit `16b0d30` after the U1 Prefab document and persistence baseline.
Cargo commands were run serially from the repository root.

# Result

Passed. The shared authoring seam now publishes one immutable `EditorUiAuthoringFrame` with an
ordered UI hierarchy, source map, typed field descriptors, authored layout/gizmo admission, and a
bounded Canvas outcome. Field lowering is schema/version-bound and complete component codecs are
revalidated before publication. Continuous Begin/Preview/Commit/Cancel edits are Host-owned,
single-active, exact-document/revision transactions; source handoff, stale revisions, reopen, and
invalid previews retire the transient gesture without publishing a document mutation.

The seam does not import egui, WGPU, native window, or renderer identities. Image brush reference
selection remains read-only until the later asset-picker unit; existing image tint edits remain
possible.

# Evidence

- `cargo nextest run -p nara_ui authoring_model` — 7 passed.
- `cargo nextest run -p nara_tooling gesture` — 3 passed.
- `cargo check -p nara-editor --tests` — passed.
- `cargo nextest run -p nara --lib --features tooling,presentation-ui,render-wgpu,project-workflow -E 'test(editor_ui_authoring_frame_and_continuous_gesture_share_one_exact_prefab_revision)'` — 1 passed.
- `cargo fmt --all` and `git diff --check HEAD` — passed.

The broader feature listing exposed an existing unrelated `tests/plugin_composition.rs` feature
matrix assumption for the disabled sprite-submitter feature; it was not expanded in this unit.

# Follow-up

EUA-U3 owns the private Canvas preview and exact presented-generation recovery contract. It must
consume this frame/source-map seam without moving renderer or toolkit ownership into
`nara_tooling` or `nara_ui`.

# Citations

- [Editor UI Prefab Authoring and Canvas v1 plan](../../../../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
- [EUA-U1 verification](2026-08-17T145954Z-eua-u1-prefab-document-workspace-and-persistence-authority-e6b43a9f24ed42deb3c8d19a03a55cdf.md)
