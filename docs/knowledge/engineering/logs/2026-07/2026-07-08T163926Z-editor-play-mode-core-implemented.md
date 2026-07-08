---
type: "Implementation"
title: "Editor Play Mode core implemented"
tags: ["play-mode", "tooling", "scene-authoring", "editor"]
timestamp: 2026-07-08T16:39:26Z
status: "accepted"
---

# Editor Play Mode Core Implemented

## Summary

- Split `nara_tooling` into `snapshot`, `inspector`, and `play` modules while preserving the public facade.
- Added `SceneAuthoringRevision` with source identity plus generation tracking in `nara_scene`.
- Added `SceneEditorState`, `SceneEditorMode`, `ScenePlaySession`, `SceneEditorModel`, and transition/apply reports in `nara_tooling`.
- Start Play now spawns a fresh isolated runtime `World` through `SceneSpawner` for plain, prefab-resolved, asset-aware, and combined scene paths.
- Stop Play drops the Play world. Runtime mutations do not write back to `SceneDocument` or the edit preview world by default.
- Mode-aware inspector commands preserve edit-mode patch behavior but reject persistent patch commands in Play or Paused. Selection remains a safe UI state change.
- Apply Changes is intentionally guard-only in this slice. It reports unsupported write-back or source revision mismatch and does not return `ScenePatchDocument`.

## Durable Boundary Knowledge

- Do not compare raw `Entity` values across different `World` instances to prove Play isolation. `Entity` is world-local and two fresh worlds may allocate equal raw values.
- Use `SceneEntityId` as the durable bridge, `SceneEntityMap` as the per-world projection map, and component-state assertions to prove world/storage isolation.
- A Play session records the exact `SceneAuthoringRevision` it was spawned from. Undoing back to equivalent document content still leaves a later revision and should conservatively mismatch apply-back.
- Future Apply Changes work should plug into `SceneApplyChangesReport` only after a concrete patchable subset is designed.

## Verification Snapshot

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo nextest run -p nara_scene -p nara_tooling -p nara`
- `cargo nextest run --workspace`
- `cargo check --examples`
- `cargo check --features serde --examples`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- Boundary searches for `winit` / `wgpu` leaks and persistent runtime identity leaks.
