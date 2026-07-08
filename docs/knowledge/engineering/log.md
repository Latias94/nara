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
