# ADR 0033: Asset Import and Render Resource Preparation Seam

**Status**: Accepted
**Date**: 2026-07-08

## Context

nara now has stable scene/prefab documents, path-backed `AssetRef`, a backend-neutral sprite/tilemap
render data path, and a wgpu adapter that can draw colored batches.

The next painful seam is between authored assets and backend render resources. If texture upload,
atlases, materials, UI images, and future 3D meshes each resolve paths directly into backend-native
objects, nara will later need to rewrite scene data, sprite/tilemap authoring data, hot reload,
editor asset views, and render backend caches at once.

ADR 0007 already accepts UUID-ready asset identity. ADR 0012, ADR 0017, and ADR 0032 already accept
backend isolation and render-graph-ready phases. This ADR connects those decisions into the concrete
asset-import-to-render-resource seam.

## Decision

nara will build the **asset import and render resource preparation seam** before expanding textured
rendering, atlases, materials, runtime UI images, or 3D mesh/material rendering.

Rules:

- `nara_asset` owns source asset identity, `.meta` records, importer registry metadata, import
  settings hashes, imported artifact records, asset dependency graph data, load states, and reload
  events.
- `nara_asset` must not own GPU resources or depend on render backend crates.
- Source asset `.meta` files live beside source assets first, for example
  `assets/textures/player.png.meta`. The `.meta` file is the stable identity authority for the
  source asset.
- Generated imported artifacts live under `.nara/import-cache/` and are never hand-authored source
  data.
- Import artifact identity is content-addressed by stable asset ID, source content hash, importer
  ID/version, import settings hash, and target/import profile when relevant.
- Importers produce backend-neutral runtime assets or descriptors, such as image pixels, texture
  descriptors, sprite atlas metadata, mesh data, material descriptors, or font atlases. They do not
  create backend-native handles.
- `nara_render` owns the backend-neutral render resource preparation interface and frame phase
  vocabulary: asset versions, render resource descriptors, prepare invalidation, and prepare/queue
  ordering.
- `nara_render_wgpu` owns the wgpu GPU resource cache. It consumes backend-neutral imported assets
  and render resource descriptors, then creates textures, buffers, samplers, bind groups, and
  pipelines.
- Gameplay-facing components store typed handles to domain assets or backend-neutral descriptors,
  not GPU handles.
- Hot reload updates asset versions and dependency graph state behind stable handles. Render
  backends invalidate and rebuild their GPU resource cache from the prepared asset/resource data.
- Phase 1 can keep file IO and importing synchronous internally, but public load states and render
  prepare invalidation must leave room for async task-pool integration.

The first implementation slice after scene/prefab serialization should therefore combine:

1. source-side `.meta` identity and stable `AssetRef::StableId` resolution;
2. an importer registry and imported artifact cache model;
3. image import and texture descriptor assets;
4. render resource preparation state/events in `nara_render`;
5. wgpu texture/sampler/cache creation in `nara_render_wgpu`;
6. sprite/tilemap texture usage through typed handles and prepared render resources.

## Alternatives Considered

### Option A: Upload textures directly from sprite paths

**Pros**: Fastest route to textured sprites.

**Cons**: Hardcodes path identity into rendering, bypasses `.meta`, prevents robust rename/move
workflows, and forces future UI/material/3D texture code to invent separate upload paths.

**Decision**: Rejected.

### Option B: Store backend handles in authoring components

**Pros**: Simple draw code and fewer lookup tables.

**Cons**: Leaks backend lifetime into ECS gameplay data, blocks serialization, complicates hot
reload, and breaks backend replaceability.

**Decision**: Rejected.

### Option C: Build a full RenderGraph before asset import

**Pros**: More complete long-term renderer infrastructure.

**Cons**: The immediate risk is asset identity/resource lifetime, not arbitrary pass composition.
ADR 0017 already requires a second concrete pass/resource use case before full graph work.

**Decision**: Rejected for the next slice.

### Option D: Asset import plus render preparation seam first (Chosen)

**Pros**: Solves texture upload through the same path future materials, UI, text, and 3D assets will
use; preserves backend isolation; gives hot reload and editor tooling a durable identity model.

**Cons**: Larger next slice than a direct texture-upload patch.

**Decision**: Chosen.

## Consequences

- `AssetRef::StableId` is a real resolution path through `.meta` and `ProjectAssetDatabase`;
  `AssetRef::Path` remains useful for hand-authored and AI-generated files.
- Texture upload should not be implemented as a sprite-only feature. It should be the first consumer
  of the generic import/prepare/cache path.
- Render backends need resource-cache invalidation tests, not only draw-path tests.
- Scene and prefab validation can check unresolved asset IDs before world mutation. Spawning uses a
  scratch `AssetServer` during component preflight and writes it back to the target `World` only when
  the full scene/prefab preflight succeeds.
- Editor asset browsers and future hot reload can reuse the same importer/dependency graph data.
- The next plan should be allowed to add meaningful code and crate structure; avoiding the seam now
  would create more expensive pre-1.0 rewrites.

## Implementation Notes

- Component codecs that decode persistent data should use `ComponentDecodeContext` when they need
  to resolve `AssetRef` values. This keeps asset validation inside the component owner while letting
  `nara_scene` preserve two-phase validation and spawn.
- Component codecs that encode persistent data can use `ComponentEncodeContext` and
  `AssetRefExportPolicy` to choose path output or stable-ID output without serializing runtime
  `AssetId` values.
- Sprite and tilemap authoring components store typed handles to `ImageAsset`/`TileSet`; backend
  texture objects remain private to `nara_render_wgpu`.
- `nara_image::ImagePlugin` is the first domain implementation of the async import seam. It registers
  `ImageImporter`, spawns image reload jobs from `AssetReloadRequest` values in
  `TaskUpdateSet::SpawnAssetJobs`, applies typed `ImageAsset` results in
  `TaskUpdateSet::ApplyAssetResults`, and then updates backend-neutral `PreparedImageResources` in
  `CoreStage::Prepare`.
- `ImagePlugin` composes `ImagePreparePlugin` rather than registering a parallel prepare path. This
  keeps prepare stats and render-resource invalidation single-pass even when sprite rendering also
  depends on image preparation.
- `ImageImporter` receives owned `ImportJobInput` and returns `ImportedAsset<ImageAsset>`. It does
  not read global ECS state and does not create GPU resources.
- Image reload preserves stable handles while changing `AssetVersion`, `LoadState`, asset events,
  prepared-resource invalidation, and source dependency edges behind those handles.
- Removed source assets clear `Assets<ImageAsset>` state and prepared image resources. Failed first
  loads and failed reloads update asset state without inventing a replacement backend resource.
- `nara_asset_watch` is the optional desktop filesystem adapter. It owns `notify` and converts raw
  watcher events into `AssetSourceChange` values; asset/import code remains watcher-agnostic.
- `AssetWatchPlugin` must use the same root as `AssetSourceRoot`. Cross-root rename events preserve
  the in-root side instead of dropping the whole event, and `.meta` removal maps to source removal
  rather than ordinary metadata modification.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Stable asset identity | `AssetRef::StableId` resolves through `.meta`/project asset records | Scene/prefab validation and unit tests |
| Backend isolation | `nara_asset`, `nara_sprite`, `nara_tilemap`, and `nara_render` do not import `wgpu` | Dependency search |
| Source/artifact split | Source `.meta` and generated `.nara/import-cache` records are separate | Fixture and docs review |
| Hot reload readiness | Asset version changes invalidate prepared render resources without changing handles | Unit tests |
| Texture path reuse | Sprite textures use the same import/prepare/cache path that UI/materials can later use | Example review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| `.meta` lifecycle slows Phase 1 | Medium | Medium | Start with deterministic sidecar generation and explicit validation; defer editor UX polish |
| Import cache overfits native desktop | Medium | Medium | Keep imported artifacts backend-neutral; put backend-specific GPU objects in backend caches |
| Render prepare interface becomes too abstract | High | Medium | Pressure it with image texture upload, sprite/tilemap use, and cache invalidation tests first |
| Asset graph duplicates scene/prefab dependency logic | Medium | Medium | Make scene/prefab validation consume asset graph diagnostics rather than owning asset scanning |

## Follow-Up Questions

- Which import profile fields belong in artifact cache keys for desktop-only Phase 1?
- What material/sampler authoring layer should sit above `ImageAsset` once sprites, UI, and text need
  per-material controls?

## Citations

- Asset identity decision: [0007-asset-identity-and-import-pipeline.md](0007-asset-identity-and-import-pipeline.md)
- Render crate boundaries: [0012-render-crate-boundaries.md](0012-render-crate-boundaries.md)
- Render graph policy: [0017-render-graph-policy.md](0017-render-graph-policy.md)
- Render backend integration boundary: [0032-render-backend-integration-boundary.md](0032-render-backend-integration-boundary.md)
- Project layout and package format: [0020-project-layout-and-package-format.md](0020-project-layout-and-package-format.md)
