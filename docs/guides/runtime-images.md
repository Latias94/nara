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

The [independent consumer](../../tests/fixtures/module-consumer/tests/runtime_image_delivery.rs)
uses actual primary/auxiliary images, native Scene handles, exact definitions, failure/discard/
successful product transactions and escaped CPU/prepared lifetime checks. The first-party Runtime
Host, processed TileSet codec and automatic resource residency remain work in the
[ordered delivery plan](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md).
