---
type: "Current State"
title: "Current Engineering State"
description: "Short durable summary of the active engineering state."
tags: ["engineering-memory"]
timestamp: 2026-07-08T09:19:21Z
status: "active"
---

# Current State

- Goal: Build nara Phase 1 runtime foundation as a Rust-native, ECS-first game engine.
- Snapshot timestamp: 2026-07-08T09:19:21Z
- Last verified: Scene/prefab serialization foundation passed fmt, workspace checks with and without serde, examples check, winit/wgpu backend example checks, `scene_prefab_roundtrip`, serde ID/path regression tests, `cargo nextest run --workspace` with 77 tests, backend boundary searches, and runtime serialization leak searches.
- Next action: After the feature branch is merged to local `main`, continue with texture upload/image import, scene patch transactions, component schema export/migrations, or asset `.meta` identity.

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
- Pending follow-ups:
  - Texture upload, atlas batching, `.meta` asset identity/import cache, scene patch transactions, field-level prefab overrides, nested prefab source resolution, component schema export, and migration chains remain follow-up work.
- Blocked:
  - None.

# Citations

- [Scene/prefab serialization progress](progress/2026-07-08T084957Z-scene-prefab-serialization-foundation.md)
- [Scene/prefab serialization final verification](verification/2026-07-08T091921Z-scene-prefab-serialization-foundation-final.md)
