# File-Backed TileSets

Tilemap is scene data: one layer per entity, cells addressed by signed TileCoord, and an optional
TileSet handle. TilemapPlugin owns its persistent schema and codec. TileSetAssetPlugin owns
decoded file-backed sources and Image-dependent publication; it does not require TilemapPlugin.

## Composition

Enable `serde` for TileSetSource and file-backed execution. Runtime2dPlugins includes the optional
TileSetAssetPlugin slot in that profile. Direct App consumers install AssetPlugin and
TileSetAssetPlugin together with their ordinary Task/Image composition. Managed project Hosts
admit filesystem content; the asset plugin does not scan directories or read files.

A TileSetSource names an Image by project AssetRef and describes a uniform TileAtlasLayout.
Its canonical source envelope has bounded encoded bytes. Metadata must declare the `tileset`
source kind and an identity matching its logical source path. The decoded atlas must match the
Image extent exactly, including margin and spacing.

Use the domain encoder to create the versioned RON PersistentFileEnvelope with kind `tileset`:

```rust
use nara::{
    asset::AssetRef,
    core::Vec2,
    tilemap::{TileAtlasLayout, TileSetSource},
};

let source = TileSetSource::new(
    AssetRef::path("textures/terrain.png")?,
    TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 8, 4),
);
let source_text = source.encode_ron_bytes()?;
```

Save it under the asset root as `tilesets/terrain.tileset.ron` with canonical JSON metadata at
`tilesets/terrain.tileset.ron.meta`. Generate AssetMeta<()> through AssetMeta::to_json_string;
its identity contains the exact logical path, a unique StableAssetId and
AssetSourceKind::Other("tileset".to_owned()), with settings `null`. The Image has its own source
and AssetMeta<ImageImportSettings>. All paths are relative to the asset root. The default TileSet
decoder limits encoded input to 64 KiB and bounds nesting, collections and strings. Dimensions
are positive/finite, counts nonzero, margin/spacing nonnegative and tint finite.

Persist project paths or stable IDs, then resolve typed handles in each runtime through AssetServer
or AssetRef. A principal source cannot claim two incompatible Rust asset types. A process-local
handle is not a serialized identity or a residency lease.

Strict Check, startup and Export validate every persisted cell against the admitted atlas.
TileIndex is currently a row-major atlas-local index. An out-of-range value reports the entity,
source entity, TileSet, signed coordinate and index. Empty maps and maps explicitly lacking a
TileSet remain valid. The renderer defensively rejects invalid runtime regions.

## Publication And Recovery

TileSetAssets keeps the latest desired source separately from its published Image content.
A published TileSet owns the exact immutable pixels and atlas it validated. Sprite consumers can
advance to a newer Image while a TileSet whose successor fails retains its coherent old picture.
Image/source identity revocation removes the publication; retaining a handle does not retain
authority. Removing and recreating an Image cannot revive an old revoked publication.

First requests and dependency changes may use an authorized published Image even while its next
reload is pending or has failed. Only a cold Loading dependency without published content waits.
Identity, publication and exact storage-slot checks still reject revoked or directly replaced data.

Both Image and TileSet managed publication use `Assets<T>::mutation_baseline(handle, &states)`.
It admits the actual typed store/state association, including a fresh empty initial slot, before
capturing an owned `AssetMutationBaseline<T>`. A delayed owner consumes that same baseline through `commit_loaded`,
`record_failure` or `remove`; all three reject a superseded state or slot before publishing their
outcome. Capturing a new baseline cannot legitimize a mismatched resource pair or raw slot write.
Source identity, dependency authority and load generation remain the concrete domain's checks.
Use the domain's normal publication path for file-backed content; raw storage remains the advanced
code-first path described below.

Image reload also separates latest desired settings, the in-flight attempt and last-good usable
pixels. An observed metadata change remains unresolved until its current recipe publishes, even
if a queue rejects it or an attempt fails or is superseded. A later source-only edit uses the
retained desired recipe, or re-admits unresolved metadata; it cannot silently restore the settings
of old pixels. Eligible old pixels may remain usable while recovery is pending. Source removal or
metadata identity mismatch revokes the corresponding authority.

The managed Editor refreshes known TileSet sources through its source catalogue and processes
at most two source files per advance. Missing Image metadata retains a semantic dependency demand;
after a source-index refresh admits that metadata and source, existing serial Image hydration can
recover the latest desired dependency. This progresses without a visible Scene panel. Bad source
content suspends pending desired publication until corrected, while eligible last-good remains.
Refresh does not create Image metadata automatically.

Entry count, distinct retained Image allocations and retained pixel bytes have explicit limits.
A rejected successor preserves eligible last-good. After capacity is released, an explicit same
source request/refresh or TileSetAssets::retry permits another attempt. Unchanged frame observations
do not continually retry failed decoding or re-emit failures. Prepared packets retain the matching
pixel charge until their final holder retires; WGPU applies its separate exact-version budget.

## Layout And Code-First Content

The first successful publication fixes that entry's atlas layout. A different live layout returns
AtlasLayoutChangeRequiresRebuild, including after automatic source or Image removal/recovery.
Rebuild consumers and explicitly remove/re-admit the TileSet, or reopen the project through strict
admission, before changing layout. No tile-ID remapping or brush/terrain workflow is provided yet.

TileSet fields are read through tile_size(), material() and atlas(). Appearance builders preserve
the captured Image because tint, sampler and alpha do not change atlas membership. with_image()
changes Image/layout and clears the captured snapshot. A manually constructed code-first TileSet
uses its caller-owned live handle and defensive renderer checks; it does not acquire file-backed
publication/recovery guarantees by construction alone.

The owning decisions are in [ADR 0117](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md).
Tile painting, durable TileId migration, Tiled import and map streaming remain separate workflows.
