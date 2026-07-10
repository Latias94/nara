---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-07-10T13:14:35+08:00
status: "active"
---

# Current State

- Goal: Build nara Phase 1 runtime foundation as a Rust-native, ECS-first game engine.
- Snapshot timestamp: 2026-07-10T13:14:35+08:00
- Last verified: U2 plugin failure containment commit `2867235`; final `cargo nextest run --workspace` passed 361/361, all-features/all-targets and backend examples compiled, nara_app Clippy passed with warnings denied, and two independent reviews had no remaining finding.
- Next action: Implement U25 capability-oriented filesystem substrate, the remaining Wave B unit; U3 fixed time and U5 bounded tasks are also open after U2 verification.

# Active Registrations

- `registry/engine-foundation-contract-completion-codex-root.md`: active; plan `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`; branch `refactor/engine-foundation-contracts`.

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
  - `nara_image` now owns backend-neutral PNG image import, image metadata, prepared image resource snapshots, and render prepare systems. Image assets no longer own sampler or material policy.
  - `nara_material` now owns backend-neutral 2D material intent: filter/address modes, sampler descriptor, alpha mode, inline material descriptor, semantic image references, and material keys.
  - `nara_sprite` and `nara_tilemap` now author material-first wrappers around reusable `Handle<ImageAsset>` image references and serialize semantic `AssetRef::Path` or `AssetRef::StableId` values through codec context.
  - `nara_sprite_render` now queues colored and textured sprite/tilemap batches with `SpriteMaterialKey` values that include image render resource key, sampler, alpha mode, and tint.
  - `nara_render_wgpu` now samples prepared image resources through backend-private texture caches split from material/sampler bind-group caches.
  - `nara_input` now owns normalized pointer state through `PointerState`, and `nara_winit` updates it from desktop cursor events.
  - `nara_ui` now owns the first nara runtime ECS UI foundation: `UiRoot`, `UiNode`, `UiPanel`, material-aware image/color panels, simple top-left logical-pixel layout projection, and runtime-only pointer interaction state.
  - `nara_ui_render` now extracts runtime UI panels, queues color/image materials through the same `nara_image` prepare and `nara_material` policy path as sprites, clips panels, and emits `UiBatches`.
  - `nara_render` now owns `RenderPassPlan` for clear/world/UI/gizmo ordering, and `nara_render_wgpu` consumes sprite and UI batches through that plan.
  - Scene/prefab spawn now supports asset-aware preflight through `ProjectAssetDatabase` and scratch `AssetServer` state before mutating the target world.
  - Review hardening now enforces one-to-one runtime asset identity binding, source-kind aware sprite/tileset preflight, path-ref database validation, prepared image removal cleanup, invalid atlas tile skips, and split wgpu sprite texture responsibilities.
  - Created the next implementation-ready plan for scene patch transactions, patch-based prefab overrides, component schema export, migrations, and nested prefab source resolution.
  - `SceneAuthoringSession` now exposes `SceneAuthoringRevision`, an opaque source revision stamp with source identity plus generation.
  - `nara_tooling` is split into `snapshot`, `inspector`, and `play` modules behind a small public facade.
  - `SceneEditorState` owns the first UI-agnostic Edit/Play/Paused model and starts isolated Play worlds through plain, prefab-resolved, asset-aware, and combined spawn paths.
  - Stop Play drops runtime state; mode-aware inspector commands reject persistent scene patches in Play or Paused.
  - Apply Changes now supports the first selected-entity / explicit-component patchable subset. `SceneApplyChangesRequest` names a `SceneEntityId` and registered component IDs; `SceneEditorState::export_apply_changes*` builds candidate `ScenePatchDocument` values from the isolated Play world without mutating the authoring session; `apply_changes*` applies through `SceneAuthoringSession`, records undo, and rejects stale revisions, runtime-only components, prefab-expanded entities, missing entities, and patch validation failures.
  - `nara_tooling_egui` provides the first concrete egui debug/editor adapter while keeping `nara_tooling` UI-toolkit agnostic.
  - Foundation hardening now requires explicit serializable component schemas, rejects duplicate runtime type registrations, checks schema default kinds, and keeps scene/prefab spawn preflight-first.
  - `nara_reflect` is split into focused value, schema, path, codec, migration, registry, and test modules behind the same public facade.
  - `nara_app` plugin lifecycle is terminal and cleanup-safe: preflight rejection is retryable,
    committed build/finish errors and unwind panics poison the app, the first error is preserved,
    cleanup is reverse/fallible/once-only, groups use a composition-only builder, runners borrow the
    app for explicit shutdown, and every mutable/run entry point rejects a poisoned app.
  - Built-in component plugins preflight stable ID/Rust type conflicts and return contextual
    registration errors; the old setup `expect` paths, `try_update`, infallible mutation entry points,
    consuming runner contract, and unrestricted group/cleanup hooks are removed in `2867235`.
  - `nara_diagnostic` no longer logs from `DiagnosticReport::push`; logging is an explicit `emit_to_tracing` bridge.
  - `nara_render` exposes `RenderBackendStatus` / `RenderBackendState` / skipped-frame reasons, and `nara_render_wgpu` reports backend state through that resource.
  - The unused public `RenderBackend` trait was removed; backend extension remains plugin/resource/system based until another backend proves the shared contract.
  - `nara_tasks` owns bounded threaded engine task pools, typed terminal handles, cancellation tokens,
    ordered integration helpers, finite shutdown, and an explicitly test-only inline driver.
  - `nara_app::CoreStage::TaskUpdate` now provides ordered main-thread async integration sets: `Poll`, `CoalesceAssetChanges`, `SpawnAssetJobs`, and `ApplyAssetResults`.
  - `nara_asset` now owns `AssetPlugin`, source-change queues, reload requests, load generations, typed importer job contracts, last-event-wins coalescing, transitive dependency reload propagation, and expected-version guarded asset result application.
  - `nara_image::ImagePlugin` is the first async asset domain plugin. It registers `ImageImporter`, spawns owned image import jobs, applies typed results behind stable handles, records failed first loads/reloads/removals, composes `ImagePreparePlugin`, and invalidates prepared image resources through the render prepare seam.
  - `nara_asset_watch` is an optional desktop watcher adapter behind `asset-watch`. It owns `notify`, validates watcher roots against `AssetSourceRoot`, preserves in-root rename sides, maps `.meta` events to semantic source changes, and never mutates asset or render storage directly.
- Pending follow-ups:
  - The egui adapter is debug/editor tooling only. Runtime UI now has a panel/layout/input/render foundation, but text, widgets, richer layout, keyboard/gamepad focus, editor dogfooding, and viewport composition remain future work.
  - Apply Changes beyond the first whole-component selected subset remains deferred: field-level diff minimization, prefab override write-back, whole-scene diffing, and edit-while-playing merge UI are not implemented.
  - Reusable material assets, custom shader specialization, texture atlases, compression profiles, mip generation, and 3D mesh import remain future extensions of the asset/import/render prepare seam.
  - `nara_scene` module split, shared asset-ref codec helpers, boundary-search CI, and real import-cache filesystem containment tests remain accepted residuals from review.
- Blocked:
  - None.

# Citations

- [Engine foundation M1 runtime safety](progress/2026-07-10-engine-foundation-m1-runtime-safety.md)
- [ADR 0010 plugin lifecycle](../../architecture/adr/0010-plugin-lifecycle-dependencies-and-failure.md)
- [Engine foundation migration guide](../../migrations/2026-07-engine-foundation.md)
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
- [Async hot reload foundation progress](progress/2026-07-09-async-hot-reload-foundation.md)
- [Async hot reload foundation verification](verification/2026-07-09-async-hot-reload-foundation.md)
- [Apply Changes M1 verification](verification/2026-07-09-apply-changes-m1.md)
- [Apply Changes M1 memory event](logs/2026-07/2026-07-09T154656Z-apply-changes-m1-selected-component-patch-export.md)
- [Material/2D migration M2 verification](verification/2026-07-09-material-2d-m2.md)
- [Material/2D migration M2 memory event](logs/2026-07/2026-07-09T163128Z-material-2d-m2-image-sampler-removal.md)
- [Runtime UI and pass plan M3 progress](progress/2026-07-09-runtime-ui-pass-plan-m3.md)
- [Runtime UI and pass plan M3 verification](verification/2026-07-09-runtime-ui-pass-plan-m3.md)
- [Runtime UI and pass plan M3 memory event](logs/2026-07/2026-07-09T173026Z-runtime-ui-pass-plan-m3.md)
- [Render UI apply foundation final verification](verification/2026-07-09-render-ui-apply-foundation-final.md)
- [Render UI apply foundation final memory event](logs/2026-07/2026-07-09T174807Z-render-ui-apply-foundation-final-verification.md)
