---
type: "Subagent Finding"
title: "Scene Serialization Read-only Review Findings"
tags: ["nara", "scene", "prefab", "serialization", "subagent-review"]
timestamp: 2026-07-08T09:15:00Z
status: "resolved"
git_branch: "feat/scene-prefab-serialization"
---

# Summary

A read-only review of the scene/prefab serialization foundation found several issues after the
initial implementation and verification pass. The blocking issues were resolved before final
verification.

# Findings Resolved

- `PrefabDocument` and `PrefabInstance` were too close to data shells. Resolution: direct
  `PrefabDocument` spawn APIs and whole-component overrides were added; external
  `PrefabInstance.source` resolution now emits an explicit unsupported diagnostic instead of being
  silently ignored.
- Asset-bearing codecs accepted `AssetRef::StableId` during preflight and failed only during apply.
  Resolution: sprite and tilemap codecs now reject stable IDs during preflight with field and asset
  context.
- `SceneEntityId` and `AssetPath` serde deserialization bypassed constructor validation.
  Resolution: both now use custom string serde and validate through `new()`.
- `export_scene` could emit a parent reference to an entity skipped from the exported document.
  Resolution: export performs a final exported-parent check, emits a warning, and clears invalid
  parent references.
- `format_version` was not validated and `PrefabDocument::default()` produced version `0`.
  Resolution: scene preflight validates format version, and prefab default uses the current format.
- Several codecs narrowed numbers with `as`.
  Resolution: transform, render, sprite, and tilemap codecs now use checked f32/i32/u32 conversion.

# Verification

Final verification is recorded in
`../verification/2026-07-08T091921Z-scene-prefab-serialization-foundation-final.md`.
