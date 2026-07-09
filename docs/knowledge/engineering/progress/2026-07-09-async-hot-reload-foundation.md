---
type: Work Progress
title: async hot reload foundation
tags: nara,async,tasks,asset,hot-reload,image,watcher
timestamp: 2026-07-09T14:31:38+08:00
status: verified
related_plan: docs/plans/2026-07-09-003-feat-async-hot-reload-foundation-plan.md
---

# Summary

The async hot reload foundation plan has landed the core task, asset reload, image import, and
optional watcher adapter work. Final workspace verification passed after review hardening.

# Implemented Contracts

- `nara_tasks` owns engine task pools, typed `TaskHandle<T>` results, cooperative cancellation,
  deterministic inline execution, threaded std worker pools, and task stats.
- `nara_app::CoreStage::TaskUpdate` now contains ordered task integration sets:
  `Poll`, `CoalesceAssetChanges`, `SpawnAssetJobs`, and `ApplyAssetResults`.
- `nara_asset::AssetPlugin` installs task and asset reload resources, source-change coalescing,
  reload generations, typed importer contracts, and dependency-aware reload request scheduling.
- `nara_image::ImagePlugin` registers `ImageImporter`, spawns owned image import jobs, applies typed
  results on the main thread, preserves stable handles, records load/reload failures, and prepares
  backend-neutral image render resources.
- `nara_asset_watch` is optional behind the root `asset-watch` feature. It owns `notify`, validates
  watcher roots against `AssetSourceRoot`, preserves in-root rename sides, maps `.meta` events to
  source changes, and never mutates asset storage directly.
- Review hardening added expected-version guards for first-load success/failure application,
  last-event-wins source-change coalescing for atomic-save sequences, transitive dependency reload
  propagation, single-pass image prepare plugin composition, and app-level watcher queue tests.

# Commit Trail

- `e56c498 docs(plan): add async hot reload foundation plan`
- `4a5a7ff feat(tasks)!: add engine task pools and task update stage`
- `0ae173a feat(asset)!: add typed async image reload foundation`
- `14386a9 feat(asset): add optional asset watcher adapter`

# Focused Verification

- `cargo check -p nara_app -p nara_tasks -p nara`
- `cargo nextest run -p nara_app -p nara_tasks`
- `cargo check -p nara_asset -p nara_image -p nara`
- `cargo nextest run -p nara_asset -p nara_image`
- `cargo run -q --example asset_import_texture`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo nextest run -p nara_asset_watch -p nara_asset -p nara_image -p nara_sprite_render`
- `cargo check -p nara --features asset-watch`

# Citations

- [Plan](../../../plans/2026-07-09-003-feat-async-hot-reload-foundation-plan.md)
- [Foundation architecture](../../../architecture/nara-foundation.md)
- [Runtime concurrency ADR](../../../architecture/adr/0008-runtime-concurrency-and-task-pools.md)
- [Asset identity ADR](../../../architecture/adr/0007-asset-identity-and-import-pipeline.md)
- [Asset/render seam ADR](../../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Verification evidence](../verification/2026-07-09-async-hot-reload-foundation.md)
