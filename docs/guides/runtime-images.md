# Runtime Images

`nara_image` separates image content from persistent delivery and source importing. An `ImageAsset`
owns extent, format, explicit color space and immutable fixed-size pixels. `ImageAsset::new`
constructs anonymous content directly from those facts. `from_product` associates content with a
complete `AssetProductRef`; `from_import` derives the source's primary Product and retains real
import provenance. Its `product()` and `source()` return their respective optional associations.

Procedural textures use anonymous handles and the same native value/state stores. They need no
source UUID, fake path, import settings or PNG encode/decode. `PreparedImageSnapshot::capture`
reads their actual store, usable version and exact slot. Managed `ImageContentSnapshot::capture`
requires a present complete Product matching the receiving catalog; two absent identities never
establish that authority.

`ImageContentHash` identifies the actual extent, format, color interpretation and pixels. It is
computed once during validated construction. Equal content can belong to different Products or
import recipes. `PreparedImageResource::content_hash` exposes that content fact;
`RenderResourceSnapshot` additionally retains key, usable asset version and exact slot revision.
The neutral `RenderResourceContentHash` is not an import artifact or Runtime member digest.

## Encode And Acquire

With `serde`, `ImageRuntimeCodec` implements `RuntimeAssetCodec<ImageAsset>`. It writes canonical
version 1 metadata of kind `runtime_image`, preceded by `NRI\0` and the metadata's little-endian
`u32` length, followed by raw RGBA. The metadata contains complete Product, descriptor and exact
pixel byte length. It carries no source path, import settings, artifact identity or trusted cached
hash. The codec recomputes content identity after validating dimensions and exact payload size.

The fixed format bounds are 16 KiB metadata, 16,384 per dimension, 16,777,216 pixels and 64 MiB RGBA.
The codec's work and retained promises are available through `runtime_limits`; the Host reserves
them before invoking decode. `ImageDomain` names the CPU family independently of a source importer.
Each decoded value contains exactly its actual PRIMARY or auxiliary Product, not both implicitly.
Encoding an anonymous image refuses with `image.runtime-product`; choose its persistent Product
explicitly when producing delivery content.

Base acquisition reserves the codec's conservative maximum before decoding, then checks its
borrowed `runtime_retained_bytes` result and retains the actual modeled output plus Host wrapper.
The Image codec accounts for returned pixel storage and its inline/shared-allocation overhead;
the original acquisition carries that charge through escaped captures. This does not select
automatic residency or a native publication policy. A new decode still requires enough free
capacity for its upfront maximum while previous outputs remain charged.

Use the existing [verified Runtime base](runtime-content-base.md) and the concrete family/codec:

```rust,ignore
// The original recipe selected this concrete native facet before catalog/base admission:
// .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new("nara.image.runtime", 1))?
let admitted = base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product)?;
assert!(admitted.product().source().is_none());
```

Catalog membership and CPU decode do not publish a native handle or pixel store. The concrete
content owner stages a complete binding and actual `Assets<ImageAsset>` value through its existing
transaction. A wrong Product, absent member or unavailable codec must refuse without source/cache
fallback.

## Retain The Original Output Owner

If native pixels or prepared captures can outlive acquisition, retain the original output wrapper
with them. Do not clone a decoded value and drop the reservation that accounted for its pixels:

```rust,ignore
let owner = Arc::new(admitted);
let image = owner.product().share_with_retention_owner(Arc::clone(&owner));
```

This shares the same immutable allocation and composes existing custody. It grants no publication
authority or exemption from overlapping-generation accounting. The actual native resource owner
still publishes values and states with the corresponding Product binding. ImageContentSnapshot
and prepared projections carry the attached lifetime owner; after native-store removal, the last
escaped capture still retains pixels and their reservation. Releasing the last capture permits
later decode admission again.

`ImageContentSnapshot::capture` checks complete Product/type binding and current published state.
Its authorization can be revoked while its old immutable pixels remain readable. Source import,
Editor metadata and source-backed TileSet publication separately require real source metadata and
refuse source-free values where that contract needs provenance.

Real import provenance stores the catalogue's canonical `AssetSourcePath`. A source reader binds
its narrow directory and an explicit `AssetPathScope` in `ImageSourceDirectory::new`; it validates
that location and derives a local read path before opening source or metadata. The sidecar keeps
its local `AssetPath`. A caller that reads the path from `ImageSourceMetadata` must not prepend
the Assets directory again. Standalone readers explicitly select `AssetPathScope::ROOT`; a
project reader uses its actual configured prefix. Processed and anonymous Images need neither.

## Use Images In UI

The Scene/ECS UI adapter binds an Image explicitly with
`nara_ui::UiVisual::image_asset(handle, tint)`. It uses the actual `Handle<ImageAsset>` for
projection and reverse export under the selected AssetRef policy. Source-backed handles can
export local aliases or stable identities; source-free primary and auxiliary Products keep their
complete Product reference. Anonymous Images remain usable at runtime but need a persistent
association before asset-reference export can succeed.

`UiVisual::image(id, tint)` instead accepts an opaque manual `UiImageId`. A manual provider joins
that identity to a snapshot through `UiPreparedImages::insert`; its spelling has no asset meaning.
The kernel also accepts provider-scoped `UiImageId::external::<T>(token)` without depending on
the Asset owner. These process-local keys are never persistent references. Replacing a visual's
background clears its previous typed asset binding, even when the new opaque ID compares equal.
The rendering packet retains the exact prepared snapshot independently of the live catalogue.

The [independent consumer](../../tests/fixtures/module-consumer/tests/runtime_image_delivery.rs)
uses actual primary/auxiliary images, native Scene handles, exact definitions, failure/discard/
successful product transactions and escaped CPU/prepared lifetime checks. First-party processed
runs use the original selected native facet and shared residency transaction to install actual
top-level Image values for stock preparation and extraction. See the
[Runtime base entry](runtime-content-base.md#materialization-and-retention). Packaged executable
and target-build qualification remain in the
[ordered delivery plan](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md).
The [Runtime TileSet guide](runtime-tilesets.md) describes source-independent descriptors and
their shared concrete publication owner, including actual Image dependencies and cell admission.
