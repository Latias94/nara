# File-Backed TileSets

The `runtime-2d` and `serde` features support file-backed TileSet sources during strict project
checking, cold startup, and trusted compiled Editor startup. Enable `presentation-2d` to render
their cells. Generic Editor TileSet import/hydration and in-place TileSet hot reload are not yet
supported.

## Source And Metadata

A TileSet source is a versioned RON `PersistentFileEnvelope` with kind `tileset`. Use the domain
encoder to produce the header and payload:

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

Save the returned text beneath the project's asset root, for example as
`tilesets/terrain.tileset.ron`. Store canonical JSON metadata alongside it as
`tilesets/terrain.tileset.ron.meta`. Its `AssetMeta<()>` identity must contain the exact asset path,
a unique `StableAssetId`, and `AssetSourceKind::Other("tileset".to_owned())`; settings serialize as
`null`. Generate metadata with `AssetMeta::to_json_string` rather than writing a bare JSON record.

The Image reference can be a path or stable ID. The Image requires its own source and typed
`AssetMeta<ImageImportSettings>` metadata. All paths are relative to the project's asset root.

Atlas dimensions must be positive and finite, row/column counts nonzero, and margin/spacing
nonnegative. The decoded Image extent must exactly equal the atlas's computed full extent,
including margin and spacing; fractional or otherwise unrepresentable extents are rejected rather
than truncated. Tint channels must be finite; finite HDR values remain supported. The default
source decoder limits encoded data to 64 KiB and also bounds nesting, collection sizes, and
strings.

## Scene References

A persisted `nara.tilemap.Tilemap` component references the TileSet through its `tileset` AssetRef.
The codec rejects duplicate cell coordinates and dimensions that become nonpositive when narrowed
to `f32`. Deliberate in-memory cell replacement through `Tilemap::set_cell` remains supported.

Project content admission resolves the TileSet metadata/source and dependent Image metadata/source
into one immutable startup closure. Every Image is decoded before the TileSet extents are checked
and TileSet identities are admitted; an invalid dependency rejects the whole successor snapshot.
Runtime and trusted Editor startup share publication: Image values publish before TileSets are
constructed with their resolved Image handles. TileSet asset state retains the source hash. The
source contains no runtime handles or GPU resources. Code-first `TileSet::from_image` construction
keeps image compatibility under the caller's authority and does not imply this file-backed
admission proof.

Runtime TileSet and Image handles come from typed `AssetServer::reserve_record` or `AssetRef`
resolution. A catalogue rejects assigning a second Rust asset type to one principal source.
`AssetServer::claim` converts a known runtime ID after checking catalogue membership and its type;
`get_handle` only retrieves an existing matching claim. Persist paths or stable IDs, then resolve
them again in a fresh runtime. Raw runtime ID/handle constructors are private.

Reloading the project content snapshot creates a new immutable revision; failed dependency loading
does not mutate an earlier snapshot. This whole-snapshot refresh does not provide concurrent typed
TileSet hot reload. That later workflow must define dependency generations, last-good publication,
source removal, and stale completion rejection.

The [foundation slice plan](../plans/2026-10-18-tilemap-file-backed-foundation-plan.md) records
verification and the remaining consumer gates.
