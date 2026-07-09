---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-07-09T12:37:27+08:00
status: "active"
---

# Current State

- Goal: Build nara Phase 1 runtime foundation as a Rust-native, ECS-first game engine.
- Snapshot timestamp: 2026-07-09T12:37:27+08:00
- Last verified: Foundation hardening passed `cargo fmt --all`, workspace checks with and without serde, `cargo nextest run --workspace` with 206 tests, `winit,wgpu` backend example checks, serde scene/prefab/patch/schema examples, backend boundary searches, runtime identity leak searches, engineering memory validation, and diff hygiene.
- Next action: Choose the next foundation slice, likely async/task execution and hot-reload scheduling, the first Apply Changes patchable subset, runtime UI data model, or the first render-graph forcing use case.

# Active Registrations

- None.

# Integrated Summary

- Done:
  - Committed the initial runtime foundation as `906afd2 feat(runtime): establish nara foundation`.
  - `nara_ecs` now uses `bevy_ecs` as the ECS substrate instead of the placeholder custom ECS.
  - `nara_app` owns nara-specific `App`, `Plugin`, startup/core stages, and Bevy ECS schedules.
  - `nara_transform` owns `Transform2d` and `GlobalTransform2d` as ECS components.
  - `nara_reflect` owns stable `ComponentTypeId`, schema versioning, and a Bevy-reflect-backed `ComponentRegistry`.
  - `nara_diagnostic` owns structured diagnostic reports and severities.
  - `examples/hello_world.rs` uses `Commands` and `Query` systems through the nara facade.
  - `nara_app` owns a fallible runner contract, `run_once(Duration)`, `First`, `FixedUpdate`, and fixed-step time resources.
  - `nara_window` owns backend-independent window IDs, primary window data, normalized events, and raw-handle provider storage.
  - `nara_winit` owns all `winit` imports, desktop event-loop integration, native window creation, raw handle registration, and keyboard/mouse translation.
  - `nara_render` now exposes graph-ready render-domain data: `RenderTarget`, `ViewportRect`, `ExtractedView`, `ExtractedViews`, `RenderPhaseLabel`, and `RenderFrame`.
  - `nara_render_wgpu` owns all `wgpu` imports and a clear-pass backend skeleton with surface status policy tests.
  - `nara_sprite` owns sprite authoring data, while `nara_tilemap` owns tilemap authoring data with dirty chunk revisions.
  - `nara_sprite_render` lowers sprite/tilemap authoring data into backend-neutral colored quad batches through split `types`, `extract`, and `queue` modules.
  - `nara_render_wgpu` now draws colored quad batches from `SpriteBatches` through split surface lifecycle and sprite pipeline modules.
  - `nara_asset` now separates persistent `AssetRef`/`AssetPath` from runtime `Handle<T>`.
  - `nara_reflect` owns `ComponentValue` and preflight/apply component codecs.
  - `nara_scene` owns `SceneDocument`, `PrefabDocument`, stable `SceneEntityId`, validation, spawn/export, and `SceneEntitySource` provenance.
  - Built-in scene, transform, render, sprite, and tilemap codecs register through their owning crate plugins.
  - `nara_asset` now owns stable asset IDs, `.meta` records, project asset database validation, importer descriptors, artifact cache records under `.nara/import-cache/`, asset versions, reload state, dependency edges, and typed runtime handle allocation.
  - `nara_image` now owns backend-neutral PNG image import, image metadata, sampler intent, prepared image resource snapshots, and render prepare systems.
  - `nara_sprite` and `nara_tilemap` now author reusable `Handle<ImageAsset>` texture references and serialize semantic `AssetRef::Path` or `AssetRef::StableId` values through codec context.
  - `nara_sprite_render` now queues colored and textured sprite/tilemap batches with explicit render resource keys and UVs.
  - `nara_render_wgpu` now samples prepared image resources through backend-private texture, view, sampler, bind-group, and pipeline caches.
  - Scene/prefab spawn now supports asset-aware preflight through `ProjectAssetDatabase` and scratch `AssetServer` state before mutating the target world.
  - Review hardening now enforces one-to-one runtime asset identity binding, source-kind aware sprite/tileset preflight, path-ref database validation, prepared image removal cleanup, invalid atlas tile skips, and split wgpu sprite texture responsibilities.
  - Created the next implementation-ready plan for scene patch transactions, patch-based prefab overrides, component schema export, migrations, and nested prefab source resolution.
  - `SceneAuthoringSession` now exposes `SceneAuthoringRevision`, an opaque source revision stamp with source identity plus generation.
  - `nara_tooling` is split into `snapshot`, `inspector`, and `play` modules behind a small public facade.
  - `SceneEditorState` owns the first UI-agnostic Edit/Play/Paused model and starts isolated Play worlds through plain, prefab-resolved, asset-aware, and combined spawn paths.
  - Stop Play drops runtime state; mode-aware inspector commands reject persistent scene patches in Play or Paused.
  - `SceneApplyChangesReport` is guard-only: it reports unsupported apply-back or source revision mismatch and returns no patches.
  - `nara_tooling_egui` provides the first concrete egui debug/editor adapter while keeping `nara_tooling` UI-toolkit agnostic.
  - Foundation hardening now requires explicit serializable component schemas, rejects duplicate runtime type registrations, checks schema default kinds, and keeps scene/prefab spawn preflight-first.
  - `nara_reflect` is split into focused value, schema, path, codec, migration, registry, and test modules behind the same public facade.
  - `nara_app` plugin installation is fallible through `PluginError`; duplicate unique plugins are rejected, and plugin groups use `add_plugin_if_missing`.
  - `nara_diagnostic` no longer logs from `DiagnosticReport::push`; logging is an explicit `emit_to_tracing` bridge.
  - `nara_render` exposes `RenderBackendStatus` / `RenderBackendState` / skipped-frame reasons, and `nara_render_wgpu` reports backend state through that resource.
  - The unused public `RenderBackend` trait was removed; backend extension remains plugin/resource/system based until another backend proves the shared contract.
- Pending follow-ups:
  - The egui adapter is debug/editor tooling only; long-term runtime UI remains a nara-owned engine UI design problem.
  - The first Apply Changes patchable subset remains undecided; runtime-to-`ScenePatchDocument` diffing is not implemented.
  - Engine-owned IO/task-pool execution, file watching, async import jobs, and reload scheduling remain deferred behind the current synchronous import/reload-ready contracts.
  - Runtime UI is expected to be nara-owned; the next UI slice should reuse `ImageAsset` and render prepare contracts rather than introducing editor/debug UI dependencies into runtime UI.
  - Material/sampler authoring, texture atlases, compression profiles, mip generation, and 3D mesh import remain future extensions of the asset/import/render prepare seam.
  - Importer typed payload APIs, `nara_scene` module split, shared asset-ref codec helpers, boundary-search CI, and real import-cache filesystem containment tests remain accepted residuals from review.
- Blocked:
  - None.

# Citations

- [Scene/prefab serialization progress](progress/2026-07-08T084957Z-scene-prefab-serialization-foundation.md)
- [Scene/prefab serialization final verification](verification/2026-07-08T091921Z-scene-prefab-serialization-foundation-final.md)
- [Next architecture priority](decisions/2026-07-08T093608Z-next-priority-asset-import-render-resource-seam.md)
- [ADR 0033](../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Asset/render seam final verification](verification/2026-07-08T120928Z-asset-render-resource-seam-final.md)
- [Asset/render seam final memory event](logs/2026-07/2026-07-08T120928Z-verification-u9-asset-render-resource-seam-final-examples-docs-boundary.md)
- [Asset/render seam review hardening verification](verification/2026-07-08T125216Z-asset-render-resource-seam-review-hardening.md)
- [Asset/render seam review hardening memory event](logs/2026-07/2026-07-08T125216Z-review-hardening-asset-render-resource-seam.md)
- [Scene patch prefab schema foundation plan](../../plans/2026-07-08-005-feat-scene-patch-prefab-schema-foundation-plan.md)
- [Scene patch prefab schema planning memory event](logs/2026-07/2026-07-08T133500Z-planning-scene-patch-prefab-schema-foundation.md)
- [Editor Play Mode core implementation memory event](logs/2026-07/2026-07-08T163926Z-editor-play-mode-core-implemented.md)
- [Foundation hardening progress](progress/2026-07-09-foundation-hardening.md)
- [Foundation hardening verification](verification/2026-07-09-foundation-hardening.md)
