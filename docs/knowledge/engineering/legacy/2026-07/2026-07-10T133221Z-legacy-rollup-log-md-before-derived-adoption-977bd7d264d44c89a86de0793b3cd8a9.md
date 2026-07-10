---
type: "Legacy Rollup Snapshot"
title: "Legacy rollup log.md before derived adoption"
description: "Preserved log.md before derived rollup adoption."
timestamp: 2026-07-10T13:32:21Z
record_id: "977bd7d264d44c89a86de0793b3cd8a9"
producer_id: "codex-root"
source_path: "log.md"
source_sha256: "034226c4065a9fcdfc2a95a2eceb9ca8d45c7d3af7b84db3ad0c67e68e6163f2"
---

# Legacy Rollup Snapshot

- Original path: `log.md`
- Original SHA-256: `034226c4065a9fcdfc2a95a2eceb9ca8d45c7d3af7b84db3ad0c67e68e6163f2`

# Original Content

~~~markdown
# Engineering Memory Update Log

This root log is an optional rollup. Prefer append-only concepts in `logs/` during parallel work.

## 2026-07-08
* **Initialization**: Created engineering wiki memory bundle.
* **Runtime foundation**: Replaced placeholder ECS with `bevy_ecs` substrate, implemented nara-owned App/Plugin stages, added `nara_transform`, `nara_reflect`, and `nara_diagnostic`, and verified the workspace with fmt/check/nextest plus runtime/example smoke runs.
* **Commit**: Created `906afd2 feat(runtime): establish nara foundation`.
* **Plan**: Created and committed `89a0053 docs(plan): add platform window render backend plan`.
* **Architecture decision**: Added ADR 0032 for render backend integration boundary and removed the duplicate generic ADR 0004.
* **Platform/window/render backend slice**: Added fallible app runners and fixed update, backend-independent window data, `nara_winit`, graph-ready render-domain extraction data, and `nara_render_wgpu` clear-pass backend skeleton behind optional facade features.
* **Plan**: Created the 2D sprite/tilemap render foundation plan and registered active implementation context; tilemap chunk identity and dirty revisions are planned in authoring data while chunked GPU caching remains deferred.
* **2D render foundation**: Implemented split sprite/tilemap authoring crates, backend-neutral `nara_sprite_render` extraction/queue/sort/batch data, and a wgpu colored quad path consuming `SpriteBatches`.
* **Render module split**: Split `nara_sprite_render` into types/extract/queue/tests modules, split wgpu surface policy/configuration into `nara_render_wgpu::surface`, preserved source-order sorting for equal sprite keys, and kept pure clear frames from creating a sprite pipeline.
* **Plan**: Created and committed `9b4020f docs(plan): add scene prefab serialization plan`.
* **Scene/prefab serialization foundation**: Added `AssetRef`/`AssetPath`, contextual diagnostics, `ComponentValue` codecs, `SceneDocument`/`PrefabDocument` validation, spawn/export, and built-in codecs for scene, transform, render, sprite, and tilemap components.
* **Read-only review hardening**: Resolved prefab shell semantics, asset stable-id preflight, serde ID/path validation, exported-parent cleanup, format-version validation, and checked numeric narrowing before final verification.
* **Verification**: Scene/prefab serialization foundation passed fmt, workspace checks with and without serde, examples check, winit/wgpu backend example checks, roundtrip example, serde ID/path regression tests, `cargo nextest run --workspace` with 77 tests, backend boundary searches, and runtime serialization leak searches.
* **Architecture decision**: Added ADR 0033 for the asset import and render resource preparation seam; the next recommended implementation slice should solve `.meta` identity, import artifacts, render prepare state, and wgpu resource cache before direct texture upload.
* **Asset scene preflight**: Added `ComponentDecodeContext`/`ComponentEncodeContext`, scene/prefab stable asset ID validation through `ProjectAssetDatabase`, scratch `AssetServer` spawn preflight, and explicit stable-ID export policy.
* **Asset/render resource seam**: Completed U1-U9 for stable asset IDs, project database validation, importer/artifact records, PNG image import, asset versions/reload state, backend-neutral render prepare, textured sprite/tilemap batches, wgpu prepared texture sampling, scene/prefab stable asset preflight, examples, docs, and final verification.
* **Asset/render resource seam review hardening**: Resolved review findings for one-to-one runtime asset identity binding, source-kind aware sprite/tileset preflight, path-ref database validation, prepared image removal cleanup, invalid atlas tile skips, import dependency source-hash coverage, and split wgpu sprite texture responsibilities; final verification passed with 136 tests.
* **Plan**: Created `docs/plans/2026-07-08-005-feat-scene-patch-prefab-schema-foundation-plan.md` for validated scene patch transactions, field-level prefab overrides, component schema export, migrations, and nested prefab source resolution.
* **Editor Play Mode core**: Implemented authoring source revisions, split tooling modules, isolated Play sessions, plain/prefab/asset-aware Start Play flows, and mode-aware inspector rejection in Play/Paused. The first diagnostics-only Apply Changes placeholder was superseded on 2026-07-09 by selected-component patch export/apply.

## 2026-07-09
* **Apply Changes M1**: Implemented selected-entity / explicit-component Apply Changes patch export and authoring-session apply. Supported changes produce `ScenePatchDocument` operations, enter normal undo history, and reject stale revisions, runtime-only components, missing entities, prefab-expanded entities, and patch validation failures with diagnostics.
* **Material/2D migration M2**: Added `nara_material`, removed sampler/material policy from `ImageAsset` and `PreparedImageResource`, migrated sprites and tilemaps to material-first wrappers, changed sprite render queueing/batching to `SpriteMaterialKey`, and split wgpu image texture caches from material/sampler bind-group caches.
* **Runtime UI and pass plan M3**: Added `nara_ui`, `nara_ui_render`, pointer state integration, material-aware UI panel batching, `RenderPassPlan`, and wgpu consumption of clear/world/UI/gizmo ordering through sprite and UI batches.
* **Final verification**: Passed the full render/UI/apply foundation Verification Contract with 267 workspace tests, optional backend examples, asset import/watch gates, dependency boundary searches, stale-contract search, engineering memory validation, and `git diff --check`.
~~~
