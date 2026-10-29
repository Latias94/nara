# ADR 0033: Asset Import and Render Resource Preparation Seam

**Status**: Accepted
**Date**: 2026-07-08
**Amended**: 2026-10-23 for Image-owned immutable prepared captures; 2026-10-29 for
source-independent Image/TileSet content and neutral prepared-content hashes
**Refined By**: ADR 0037: Runtime Asset Acquisition, Reload, and Lifetime Policy; ADR 0040:
Render Resource Lifetime and Submitter Ownership; ADR 0054: GPU Upload Budget and Buffer
Allocation Policy; ADR 0080: Domain-Owned TaskUpdate Integration Sets;
[ADR 0103](0103-concrete-domain-owned-asset-import-execution.md): Concrete Domain-Owned Asset
Import Execution; [ADR 0115](0115-editor-image-import-metadata-and-authoring-publication.md):
Editor Image Import Metadata and Authoring Publication

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

- `nara_asset` owns source asset identity, strict identity-first `.meta` admission, typed
  `AssetMeta<TSettings>` composition, imported artifact records, asset dependency graph data, load
  states, and reload events. Concrete asset domains own settings semantics and hashing plus import
  execution; `nara_asset` has no generic decoder registry.
- `nara_asset` must not own GPU resources or depend on render backend crates.
- Source asset `.meta` files live beside source assets first, for example
  `assets/textures/player.png.meta`. The `.meta` file is the stable identity authority for the
  source asset.
- When a durable imported-artifact product is separately admitted, generated artifacts live under
  `.nara/import-cache/` and are never hand-authored source data. ADR 0115's Editor image v1 keeps
  accepted decoded pixels session-local and does not publish that cache.
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
- Image content owns explicit extent/format/color interpretation and immutable pixels. Persistent
  delivery and managed derivation require an explicit complete Product association; procedural
  content can remain anonymous. Real source-import provenance is optional and cannot supply
  mandatory Runtime state. Sampler, alpha, tint and material policy live above images in
  `nara_material`.
- `nara_render` owns backend-neutral render resource identity and frame phase vocabulary: loader
  versions, exact asset-slot revisions, content hashes, and prepare/queue ordering. A neutral
  identity alone does not prove association with any domain content.
- `nara_image` owns `PreparedImageSnapshot`, an opaque association of exact immutable pixels and
  their complete render identity, and the `PreparedImages` collection of those captures. A managed
  `ImageContentSnapshot` projects its admitted content; code-first capture reads the actual typed
  `Assets<ImageAsset>` slot and applicable usable `AssetStates` without requiring a source catalog.
  Consumers may inspect or clone a capture, but cannot supply identity and pixels independently.
  Collection insertion accepts only a complete capture. No generic writable identity/content
  pairing store or speculative preparation framework is retained for hypothetical asset domains.
- `nara_render_wgpu` owns the wgpu GPU resource cache. It consumes backend-neutral imported assets
  and render resource descriptors, then creates textures, buffers, samplers, bind groups, and
  pipelines.
- Gameplay-facing components store typed handles to domain assets or backend-neutral descriptors,
  not GPU handles.
- Hot reload updates asset versions and dependency graph state behind stable handles. Render
  backends invalidate and rebuild their GPU resource cache from the prepared asset/resource data.
- Managed mutation admission binds the actual typed value store and state store before issuing an
  owned baseline. Success, failure, and removal revalidate that same baseline; unchanged but
  mismatched resources do not establish publication authority. Raw code-first writes revoke prior
  managed derivation authority while remaining eligible for their explicit rendering path.
- Phase 1 can keep file IO and importing synchronous internally, but public load states and exact
  prepare snapshot identity must leave room for async task-pool integration.

### Source-Independent Image Content

`ImageAsset::new` accepts extent, format, color interpretation and pixel bytes without persistent
identity. `from_product` explicitly associates content with a complete Product. `from_import`
derives the PRIMARY Product and color from actual admitted import provenance.
Imported metadata and the explicit descriptor must agree; general serialization recomputes
content identity through construction and rejects contradictions. A processed image does not
manufacture a source path, import settings or artifact record to satisfy a runtime constructor.
An anonymous image does not invent a source UUID or pass through PNG import to become renderable.

The Image owner computes a domain-separated `ImageContentHash` once from the complete actual
extent/format/color/pixels. Equal content from different Products or import recipes has equal
content hashes. Logical Product identity, native binding, AssetVersion, exact slot mutation and
GPU Host/epoch retain their separate meanings. Render preparation uses the distinct
`RenderResourceContentHash` in the existing complete snapshot. It never converts a source,
import-artifact or Runtime member digest into prepared-content authority. The original key,
version and slot remain required; equal content does not authorize a stale or foreign capture.

`ImageContentSnapshot` requires a present complete Product and validates its type/binding plus the
existing usable state, publication and slot evidence. Two absent Product associations cannot
establish managed authority. Code-first `PreparedImageSnapshot` instead captures actual anonymous
content through its typed store, usable version and exact slot. Path aliases are not Runtime
membership proof. A managed snapshot's pixels and prepared projection remain readable after
revocation while `is_authorized` refuses further
derivation. Source-oriented import/reload, Editor metadata and TileSet-source publication require
real source provenance explicitly; a processed image in a native store cannot impersonate it.

The concrete `ImageRuntimeCodec` reads/writes `runtime_image` version 1: a small bounded JSON
envelope preceded by `NRI\0` and its little-endian metadata length, followed by exact raw RGBA.
Metadata carries only complete Product and actual descriptor/byte length. Validate header,
version/engine, identity, dimensions and exact total length before copying pixel bytes. No
source/import/cache fields, trusted cached content hash or alternate legacy reader is accepted.
The independent family supports one actual PRIMARY or auxiliary Product per decoded value.
Runtime encoding refuses anonymous content; decoding always produces the explicitly named Product.

Native publication must keep the original decoded-output reservation and immutable base alive
through escaped pixel/content/render owners. `share_with_retention_owner` composes a trusted
owner with existing pixel custody without granting publication or exempting overlap accounting.
The resulting immutable share can enter an existing product-owned native resource transaction;
failed/cancelled Scene publication leaves last-good bindings and values intact. This boundary
does not introduce a second native asset transaction, generic residency executor, render graph
implementation or complete first-party packaged Player.

### Shared Source And Processed TileSet Publication

`TileSetDescriptor` is source-independent CPU content: its own complete Product, one complete
required Image Product, atlas layout and sampler/alpha/tint values. Both Product selections may
be primary or auxiliary. The descriptor contains no native handles, source paths, import hashes
or residency claims. Its concrete `runtime_tileset` version-one codec owns bounded strict format
admission and visits the actual required Image Product through ADR 0007's inventory contract.

The existing TileSet content owner retains one closed Source or Product declaration for each
destination. A Source declaration requires actual admitted database records and Image import
provenance. A Product declaration requires complete typed bindings and an actual Image with the
exact declared Product. It does not reconstruct a source declaration or manufacture an
`AssetRecord`, `SourceHash` or `ProjectAssetDatabase`. Source-only primary-selection restrictions
remain in that adapter. Native Runtime decode can look up already bound auxiliary TileSets;
persisted cell membership consumes the concrete owner's complete-reference-to-atlas proof.

Both declarations use the same publication claims, value/state baselines, actual Image snapshot,
exact extent arithmetic, atlas rebuild barrier, retained pixels and prepare/commit/retire path.
Changing a declaration cannot bypass a live atlas barrier. A copied receipt does not establish
that other residency holders retired. Reconciliation preserves last-good and does not churn an
unchanged publication. Actual Image captures keep their original Runtime acquisition and
decoded-output reservation through the last escaped TileSet or prepared-render owner.

Content acquisition proves the actual Image dependency is present and Required in the captured
base. The concrete publication owner additionally proves typed binding, actual Image identity and
extent. The Scene/content adapter checks cell indices against that admitted atlas before live
mutation. Neither a catalog edge nor descriptor decoding proves all these facts. Source and
processed callers stage resources with the existing Scene/native transaction; independent
product-owned resource aggregates do not imply that stock render extraction reads their fields.

The first implementation slice after scene/prefab serialization should therefore combine:

1. source-side `.meta` identity and stable `AssetRef::StableId` resolution;
2. stable imported-artifact identity plus concrete domain-owned import execution;
3. image import and texture descriptor assets;
4. neutral render identity in `nara_render` and coherent Image-owned prepared state;
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
- Texture upload should not be implemented as a sprite-only feature. It consumes the shared
  artifact identity and prepare/cache primitives after the Image domain completes import.
- Render backends need resource-cache invalidation tests, not only draw-path tests.
- Scene and prefab validation can check unresolved asset IDs before world mutation. Spawning uses a
  scratch `AssetServer` during component preflight and writes it back to the target `World` only when
  the full scene/prefab preflight succeeds.
- Editor catalogs may reuse stable identity and source-index facts, but a catalog entry does not
  prove source freshness or authorize a generic importer/dependency-graph executor.
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
- `nara_image::ImagePlugin` is the first domain implementation of the async import seam. It installs
  `ImageImporter` as the concrete execution resource, spawns image reload jobs from
  `AssetReloadRequest` values in
  `AssetTaskUpdateSet::SpawnJobs`, applies typed `ImageAsset` results in
  `AssetTaskUpdateSet::ApplyResults`, and then updates backend-neutral `PreparedImages` in
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
- ADR 0115 adds explicit Editor Import/Reimport and referenced-image hydration as two Host consumers
  of this same Image-domain recipe. The Host owns capability-bound metadata publication and its
  concrete session image store; neither consumer is a second asset domain or a trigger for a
  generic executor.
- Image reload preserves stable handles while changing `AssetVersion`, `LoadState`, asset events,
  exact `AssetSlotRevision`, prepared-resource snapshots, and source dependency edges behind those
  handles. Direct slot mutation is observable through the same exact revision even when loader
  metadata is unchanged.
- An Image capture derives its slot, usable version, kind and descriptor inside the Image owner.
  The shared pixel allocation and its retention owner live as long as any captured packet clone.
  A successor publication or direct slot replacement cannot mutate an earlier capture. Equal
  captures reuse preparation and GPU cache entries without a per-frame full-pixel hash.
- Removed source assets clear `Assets<ImageAsset>` state and prepared image resources. Failed first
  loads and failed reloads update asset state without inventing a replacement backend resource.
- `nara_asset_watch` is the optional desktop filesystem adapter. It owns `notify` and converts raw
  watcher events into `AssetSourceChange` values; asset/import code remains watcher-agnostic.
- `AssetWatchPlugin` must use the same root as `AssetSourceRoot`. A semantic rename whose file
  scope is already proven preserves its in-root side across roots. Native notify rename or
  ambiguous subtree observations retain a full rescan obligation under ADR 0037 instead of
  claiming complete incremental coverage. `.meta` removal maps to source removal rather than
  ordinary metadata modification.
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
| Source/artifact split | Editor source `.meta` and session-local pixels are separate; durable `.nara/import-cache` publication requires another admitted product | Fixture and docs review |
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
