# ADR 0033: Asset Import and Render Resource Preparation Seam

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0037: Runtime Asset Acquisition, Reload, and Lifetime Policy; ADR 0040:
Render Resource Lifetime and Submitter Ownership; ADR 0054: GPU Upload Budget and Buffer
Allocation Policy; ADR 0080: Domain-Owned TaskUpdate Integration Sets

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
- An import recipe key includes stable asset ID, source content digest, importer ID/version and
  implementation digest, canonical import settings, every tracked import-input dependency,
  output schema/format, and target/import profile when relevant. An artifact content digest
  identifies immutable output bytes and is not the recipe key or durable product identity.
- Importers produce backend-neutral runtime assets or descriptors, such as image pixels, sprite
  atlas metadata, mesh data, material descriptors, or font atlases. They do not create
  backend-native handles.
- Importers must not observe untracked ambient files, environment state, clocks, or mutable global
  ECS state. Dependency discovery and multi-product publication require a bounded tracked import
  context and an atomically published artifact group.
- Image assets describe image content and import identity only. Sampler, alpha, tint, and material
  policy live above images in `nara_material`.
- `nara_render` owns the backend-neutral render resource preparation interface and frame phase
  vocabulary: loader versions, exact asset-slot revisions, render resource descriptors, snapshot
  cache identity, and prepare/queue ordering.
- `nara_render_wgpu` owns the wgpu GPU resource cache. It consumes backend-neutral imported assets
  and render resource descriptors, then creates textures, buffers, samplers, bind groups, and
  pipelines.
- Gameplay-facing components store typed handles to domain assets or backend-neutral descriptors,
  not GPU handles.
- Hot reload updates asset versions and dependency graph state behind stable handles. Render
  backends invalidate and rebuild their GPU resource cache from the prepared asset/resource data.
- Phase 1 can keep file IO and importing synchronous internally, but public load states and exact
  prepare snapshot identity must leave room for async task-pool integration.

The first implementation slice after scene/prefab serialization should therefore combine:

1. source-side `.meta` identity and stable `AssetRef::StableId` resolution;
2. an importer registry and imported artifact cache model;
3. image import and texture descriptor assets;
4. render resource preparation state in `nara_render`;
5. material-aware sprite/tilemap usage through typed handles and prepared render resources;
6. wgpu image texture caches plus sampler/material bind-group caches in `nara_render_wgpu`.

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
- Cache reuse is valid only when the complete import recipe matches. Equal output bytes may share a
  content digest without collapsing stable source/product identity or dependency provenance.
- The next plan should be allowed to add meaningful code and crate structure; avoiding the seam now
  would create more expensive pre-1.0 rewrites.

## Implementation Notes

- Component codecs that decode persistent data should use `ComponentDecodeContext` when they need
  to resolve `AssetRef` values. This keeps asset validation inside the component owner while letting
  `nara_scene` preserve two-phase validation and spawn.
- Component codecs that encode persistent data can use `ComponentEncodeContext` and
  `AssetRefExportPolicy` to choose path output or stable-ID output without serializing runtime
  `AssetId` values.
- `nara_material` owns `FilterMode`, `AddressMode`, `SamplerDescriptor`, `AlphaMode2d`,
  `Material2dDescriptor`, semantic image references, and material hashing/keying.
- `nara_image::ImageAsset` and `PreparedImageResource` intentionally do not store sampler data.
  Changing sampler policy is a material change, not an image import or prepare change.
- Sprite and tilemap authoring components store typed handles to `ImageAsset`/`TileSet` inside
  narrow material wrappers; backend texture objects remain private to `nara_render_wgpu`.
- `nara_image::ImagePlugin` is the first domain implementation of the async import seam. It registers
  `ImageImporter`, spawns image reload jobs from `AssetReloadRequest` values in
  `AssetTaskUpdateSet::SpawnJobs`, applies typed `ImageAsset` results in
  `AssetTaskUpdateSet::ApplyResults`, and then updates backend-neutral `PreparedImageResources` in
  `CoreStage::Prepare`.
- `ImagePlugin` composes `ImagePreparePlugin` rather than registering a parallel prepare path. This
  keeps prepare stats and render-resource snapshot replacement single-pass even when sprite
  rendering also depends on image preparation.
- `ImageBytesImportRequest` owns a fixed-length `Box<[u8]>`; file requests own an admitted
  host-issued `FileCapability`. `ImageImporter::{import_image, admit_file}` privately capture the
  target stable binding, expected version, O(1) `AssetStateRevision`, and persistent
  `AssetSlotRevision`.
  They validate the captured last-good value against the budget host's publication-overlap ceiling
  and charge its actual RGBA length; an independently constructed importer sets the ceiling from
  `max_rgba_bytes`. The reservation-bearing `ImageImportedAsset` exposes one `commit` operation, which
  revalidates that admission, chooses initial load or reload internally, and releases its modeled
  charge only after commit returns. Accounting is shared only through an explicitly injected
  `ImageImportBudgetHost`; no global or static owner exists. Built-in importer version 2
  intentionally invalidates version-1 image artifacts. The importer does not read ambient paths or
  create GPU resources.
- Image reload preserves stable handles while changing `AssetVersion`, `LoadState`, asset events,
  exact `AssetSlotRevision`, prepared-resource snapshots, and source dependency edges behind those
  handles. Direct slot mutation is observable through the same exact revision even when loader
  metadata is unchanged.
- Removed source assets clear `Assets<ImageAsset>` state and prepared image resources. Failed first
  loads and failed reloads update asset state without inventing a replacement backend resource.
- `nara_asset_watch` is the optional desktop filesystem adapter. It owns `notify` and converts raw
  watcher events into `AssetSourceChange` values; asset/import code remains watcher-agnostic.
- `AssetWatchPlugin` must use the same root as `AssetSourceRoot`. Cross-root rename events preserve
  the in-root side instead of dropping the whole event, and `.meta` removal maps to source removal
  rather than ordinary metadata modification.
- `nara_sprite_render` resolves sprite/tilemap material data into `SpriteMaterialKey` values
  containing image resource key, sampler, alpha mode, and tint. Sorting and batching use these
  material keys rather than image-resource-only keys.
- `nara_ui_render` resolves UI image panel material data into `UiMaterialKey` values using the
  same prepared image resources, sampler descriptors, alpha mode, and tint policy. Runtime UI does
  not introduce a second direct path from asset paths to backend textures.
- `nara_render_wgpu::WgpuSpriteTextureCache` caches GPU image textures by prepared image snapshot
  and caches sampler/bind-group choices by sprite/UI material keys. Sampler-only changes create a
  new bind group without rebuilding the prepared image resource or reuploading the texture.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Stable asset identity | `AssetRef::StableId` resolves through `.meta`/project asset records | Scene/prefab validation and unit tests |
| Backend isolation | `nara_asset`, `nara_sprite`, `nara_tilemap`, and `nara_render` do not import `wgpu` | Dependency search |
| Source/artifact split | Source `.meta` and generated `.nara/import-cache` records are separate | Fixture and docs review |
| Hot reload readiness | Asset version changes invalidate prepared render resources without changing handles | Unit tests |
| Texture path reuse | Sprite/tilemap and runtime UI image materials use the same image import/prepare/cache path that reusable materials can later use | Example review and UI render tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| `.meta` lifecycle slows Phase 1 | Medium | Medium | Start with deterministic sidecar generation and explicit validation; defer editor UX polish |
| Import cache overfits native desktop | Medium | Medium | Keep imported artifacts backend-neutral; put backend-specific GPU objects in backend caches |
| Render prepare interface becomes too abstract | High | Medium | Pressure it with image texture upload, sprite/tilemap use, and cache invalidation tests first |
| Asset graph duplicates scene/prefab dependency logic | Medium | Medium | Make scene/prefab validation consume asset graph diagnostics rather than owning asset scanning |

## Follow-Up Questions

- Which import profile fields belong in artifact cache keys for desktop-only Phase 1?
- What reusable material-asset layer should sit above inline `Material2dDescriptor` once projects
  need shared materials, shader specialization, or editor-authored material files?

## Citations

- Asset identity decision: [0007-asset-identity-and-import-pipeline.md](0007-asset-identity-and-import-pipeline.md)
- Render crate boundaries: [0012-render-crate-boundaries.md](0012-render-crate-boundaries.md)
- Render graph policy: [0017-render-graph-policy.md](0017-render-graph-policy.md)
- Render backend integration boundary: [0032-render-backend-integration-boundary.md](0032-render-backend-integration-boundary.md)
- Project source layout: [0020-project-layout-and-package-format.md](0020-project-layout-and-package-format.md)
