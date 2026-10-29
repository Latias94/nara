# Image Content, Import Provenance And Runtime Delivery

**Date:** 2026-10-29. **Nara baseline:** `3ac32a4`.
**Scope:** CPU Image identity, prepared content and independently decoded Runtime pixels.
This source study supports the owning replacement; execution evidence belongs in qualification.

## Pinned Sources

| Reference | Pin |
| --- | --- |
| Bevy | `85503941e488152711a4695bbe248e9ed9f453fc` |
| Godot | `81ab961f91066a63bcbf3ec340962bb1a1ada558` |

The checkouts in `repo-ref` were inspected read-only. Their APIs and latest upstream behavior are
not inferred from the pinned observations.

## Observations

Bevy's `crates/bevy_image/src/image.rs` stores data, layout, texture descriptor and sampler in its
Image. `Image::new` accepts actual pixels without import-artifact records. The separate
`image_loader.rs` settings are loading inputs projected into that value. For RGBA8,
`image_texture_conversion.rs` selects sRGB or linear GPU format from actual image interpretation.
`crates/bevy_render/src/render_asset.rs` indexes prepared assets by AssetId and uses modified/unused
events for extraction and retirement. `texture/gpu_image.rs` updates or recreates the GPU resource
from actual descriptors. These roles are not an import-recipe hash.

Godot's `core/io/image.h` and `image.cpp` own format, dimensions, mipmaps and bytes;
`Image::create_from_data` does not require an Editor import record. Import options belong in
`editor/import/resource_importer_texture.cpp`. `scene/resources/image_texture.cpp` consumes Image
content to create/update a RID and validates compatible layout for an update.
`scene/resources/compressed_texture.cpp` reads its own processed descriptor/payload rather than
reimporting the authoring image. Godot's color-view selection in
`servers/rendering/renderer_rd/storage_rd/texture_storage.cpp` depends on usage, so its field model
is not copied as a universal Nara color policy.

Neither reference proves Nara's captured-CPU lifetime requirements automatically. Bevy can move
data into a GPU-only extraction; Godot can request an Image from RenderingServer. Nara's existing
retained render packets and recovery path require the actual immutable CPU allocation and its
reservation to remain owned independently of later source/cache availability.

## Nara's Owning Change

At the baseline, ImageAsset could only be constructed with ImageSourceMetadata. Color came from
import settings. ImageContentSnapshot checked a primary source ID and path. PreparedImageResource
required source/artifact hashes, and `image_descriptor_hash` mixed those facts into a value that
did not read the actual pixels. Exact slot revisions already distinguished raw pixel replacement;
this study does not claim an observed GPU cache collision. The problem was the data contract's
inability to express independently delivered content.

The replacement separates explicit color/extent/format and immutable pixels from an optional
complete Product association and optional actual import provenance. The Image owner hashes the
complete descriptor and pixels once; neither Product nor provenance participates in content
equality. The render hash has its own type and meaning while key/version/slot remain part of the
complete prepared snapshot.

An independent cross-consumer review challenged the first replacement's mandatory Product field.
`AssetServer::reserve_anonymous` and code-first prepared capture already admit anonymous native
values, while `examples/support/procedural_image.rs` encoded existing RGBA as PNG and supplied
synthetic path/source facts just to import it. That is present interface pressure, not evidence of
an observed GPU defect. Ordinary construction now accepts anonymous pixels; `from_product` names
delivery content explicitly, and `from_import` retains actual provenance. Managed capture requires
a present matching Product, never equality of two absent associations. Runtime encoding refuses
anonymous values and decoding always supplies the payload's complete Product. The independent
glyph-page snapshots already use runtime incarnation/generation and need no migration here.

Source import, Editor metadata and source TileSet publication still need actual source evidence.
They explicitly refuse a processed value where that evidence is required. Managed Image content
captures validate a present complete Product binding and existing publication/slot guards;
code-first preparation uses the actual typed store and usable version/slot. Readable old pixels
do not imply current authorization after revocation.

The concrete Runtime Image format is a bounded metadata envelope plus raw RGBA. Full Product,
actual descriptor and exact byte length are admitted before pixel copying. A Host reserves decode
and output, then composes that retained owner with native pixel shares. The same Scene transaction
stages bindings and actual resource values; no fake import recipe, source database or additional
publication mechanism is needed.

This unit is limited to the currently supported RGBA8 contract. Future mips, compressed formats,
layout and usage must be represented explicitly in the Image owner and included in its content
encoding. Material alpha/sampler policy remains above Image; a source-independent image does not
implement the proposed render graph or every native residency owner.

## Runtime Output Cost

The next consumer exposed a separate scaling defect: generic base acquisition retained the
codec's whole-format maximum for each decoded output. A four-byte image therefore occupied the
same logical output budget as the maximum supported image. Raising the aggregate cap would hide
the mismatch and weaken concurrent decode admission.

Keep the complete maximum promise before invoking a trusted decoder, then query its actual
returned product through required `RuntimeAssetCodec::runtime_retained_bytes`. This belongs to
the concrete codec because its decoded representation determines the allocations; the generic
Host owns wrapper/base overhead, checked admission, refund and shared lifetime. A default maximum
query would silently preserve the defect when an implementation forgets its contract. A public
generic allocation walker or residency executor is unnecessary for the concrete Image/compound
consumers. The bounded query reports inline product plus owned heap storage, has no effects or
product/query-scratch allocation, and may refuse through the existing domain diagnostic.

Image output has exact fixed-size pixels and known Arc storage. Source provenance and opaque
attached owners refuse this query because this decoder returns neither and cannot account for
arbitrary external custody. The Host checks the original maximum, retains actual product/wrapper
cost and releases temporary work. Independent acquisitions remain independently charged; clones
and escaped pixel owners retain their original acquisition charge. A new decode still needs its
full upfront headroom. This is logical ownership accounting, not allocator/RSS measurement.

The pinned Bevy `bevy_render/src/render_asset.rs` independently asks concrete render assets for
`byte_len` to throttle GPU upload. Godot `core/io/image.cpp::get_data_size` reports actual pixel
data length. These support querying concrete content rather than a source recipe, but neither
is Nara's complete decoded-output/Host-wrapper model: upload bytes, pixel bytes, retained CPU
ownership and physical GPU residency remain different costs. Nara does not copy Bevy's optional
upload query or Godot's pixel-only size as a complete Runtime retention guarantee.

## Owning Sources

- [Import/render seam](../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Envelope policy](../architecture/adr/0051-persistent-file-envelope-migration-and-golden-fixtures.md)
- [Ordered delivery plan](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Image owner](../../crates/nara_image/src/asset.rs)
- [Runtime codec](../../crates/nara_image/src/runtime.rs)
- [Independent consumer](../../tests/fixtures/module-consumer/tests/runtime_image_delivery.rs)
