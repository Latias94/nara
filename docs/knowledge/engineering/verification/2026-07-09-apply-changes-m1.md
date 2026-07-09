---
type: "Verification"
title: "Apply Changes M1 Verification"
description: "Verification record for selected-entity explicit-component Apply Changes."
tags: ["engineering-memory", "verification", "tooling", "scene", "apply-changes"]
timestamp: 2026-07-09T15:46:56+08:00
status: "passed"
---

# Apply Changes M1 Verification

## Scope

Implemented the first Play Mode Apply Changes subset:

- Request shape is selected `SceneEntityId` plus explicit registered component IDs.
- Export encodes selected Play world components through `ComponentRegistry` and
  `ComponentEncodeContext`.
- Supported changed components produce `ScenePatchDocument` add/remove/replace component
  operations.
- Apply routes through `SceneAuthoringSession::apply_patch*`, so validation, revision updates,
  inverse patches, and undo history remain on the normal authoring path.
- No-op requests return supported no-op and do not create undo.
- Not-in-play, revision mismatch, missing entity, runtime-only/non-serializable components,
  prefab-expanded entities, duplicate component requests, and failed patch validation reject with
  diagnostics and no document mutation.

## Commands

- `cargo fmt --all --check`
- `cargo nextest run -p nara_tooling -p nara_scene -p nara_reflect`
  - Result: 59 tests passed.
- `cargo check --workspace`

## Boundary Search

- `rg -n "AssetId|Handle<|wgpu::|winit::|bevy_ecs::Entity|nara_ecs::Entity|\bEntity\b" crates/nara_tooling/src/play.rs crates/nara_scene/src/patch.rs crates/nara_scene/src/document.rs crates/nara_reflect/src/codec.rs`
  - Result: runtime `Entity` remains internal to Play session / codec call signatures; no
    `SceneDocument` or `ScenePatchDocument` runtime identity storage was introduced.

## Residuals

- Apply Changes currently uses whole-component add/remove/replace operations for explicitly
  requested components.
- Document component values are canonicalized through registered migrations before comparison, but
  no-op Apply Changes does not rewrite old schema versions by itself.
- Field-level diff minimization, prefab override write-back for expanded entities, whole-scene
  diffing, and edit-while-playing merge UI remain deferred.
