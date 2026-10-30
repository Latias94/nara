# Runtime TileSets

`TileSetDescriptor` describes CPU content without source metadata or receiving native handles.
It carries a complete TileSet Product, its complete required Image Product, exact atlas layout
and sampler/alpha/tint policy. Both Products can be primary or auxiliary. A source-side
`TileSetSource` remains the adapter for authoring files and real Image import provenance.

Source `TileSetContent::request` also receives the actual `&AssetPathScope`. It binds the Image
alias once and retains that canonical dependency with the desired declaration. Dropping the
requesting scope does not stop automatic Image reconciliation; `reconcile()` needs no scope
argument or World-wide default. Binding an equal source under a different scope is a new
declaration when it selects another Image, so old prepared publication claims reject. Direct
`TileSetPublicationInput::from_source` takes the same scope and an explicit bounded canonical
reference allowance; preparation also checks the receiving owner's limit.

The native `TileSetAssetPlugin` reconciles dependencies during ordinary Runtime frames. A Host
using driver-only Image publication must explicitly advance that same `TileSetContent::reconcile`
owner after publication or revocation, including when no simulation frame or visible preview runs.
That maintenance uses the retained declaration and performs no source I/O or alias rebinding.

## Decode The Actual Dependency

With `serde`, `TileSetRuntimeCodec` implements `RuntimeAssetCodec<TileSetDescriptor>` using
`runtime_tileset` version 1 and Core's persistent envelope. Records are strict bounded JSON;
source paths, import hashes, cache fields and native handles are not part of the format. The
descriptor has no owned heap storage; its retained promise and actual modeled cost are both
`size_of::<TileSetDescriptor>()`. Work and encoded bounds are available through `runtime_limits`.

Acquire from one [verified Runtime base](runtime-content-base.md):

```rust,ignore
// The original recipe selected the native TileSet and Image codec facets before base admission.
let descriptor = base.acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(tileset_product)?;
let image = base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(
    descriptor.product().image_product(),
)?;
descriptor.product().validate_image_extent(image.product().extent())?;
```

The codec visits its actual Image Product. Acquisition requires that Product to be present in
this same captured base and declared Required by the TileSet member. A Soft edge cannot satisfy
it. This does not decode the Image or prove its family, extent, native binding or residency;
the concrete consuming owner completes those checks. An Image codec mismatch or wrong extent
must refuse before any live Scene/resource mutation.

## Use The Existing Publication Owner

Stage complete typed bindings with `SceneAssetBindings::reserve_product::<TileSet>` and
`reserve_product::<ImageAsset>`. Preserve the original Image output owner through
`share_with_retention_owner`; the [Image guide](runtime-images.md) explains that custody.
Construct `TileSetPublicationInput::from_product` using the actual descriptor, receiving Image
handle and that retained Image allocation. Call `TileSetAssets::prepare_publications` with
`None` for the source database. No synthetic source record, path or hash is required.

Publication consumes the same `TileSetAssets` owner as source-backed content. It validates
complete Product/type bindings, exact Image identity and extent, mutation baselines, declaration
claims, capacity and the existing atlas rebuild barrier. Publish the exact staged Image before
consuming `PreparedTileSetPublications::commit` in the same exclusive transaction. Its synchronous
commit requires the prevalidated bindings, slots and Image allocation to remain unchanged.

An independent application can prepare its complete product-owned aggregate privately, then move
it through the existing Scene product-resource overlay together with the staged bindings and
processed Scene. The aggregate's concrete `SceneProductResource` baseline protects its installed
incarnation and contents while native retirement is pending. Failure or discard publishes none
of that candidate. This is one Scene/native transaction, not a second publication executor.

Before Scene publication, use `validate_persisted_tilemap_membership` with the actual admitted
descriptor's atlas. The resolver receives the complete Product reference; it must not silently
substitute the enclosing primary selection. Invalid cell indices refuse the candidate. Empty
or explicitly untextured maps retain their ordinary valid behavior.

## Replacement And Lifetime

Keep the returned `TileSetContentReceipt` for later owned replacement or retirement. A copied
receipt does not establish exclusive residency, and a superseded declaration/storage writer
invalidates it. Ordinary Image reconciliation retains the declaration claim and coherent
last-good content. Repeated unchanged observations do not create new publications.

A live declaration cannot change its accepted atlas layout. Explicitly retire its current
content and admit a fresh declaration after affected consumers retire. Removal/revocation alone
does not silently reinterpret existing cell indices. Appearance changes with the same atlas
remain eligible for ordinary replacement.

Escaped TileSet Image snapshots and prepared render captures retain the original immutable pixels,
base and decoded-output charge after native-store retirement. Authorization for new derivation
can be revoked while the old captured pixels remain readable. Source APIs enumerate Source
declarations and cannot take over a Product declaration by inventing an authoring record.

First-party processed runs prepare the actual Required closure, check the declared Image's selected
native family/extent and admit persisted Tilemap cells before constructing a live candidate. Their
shared residency owner publishes to the top-level stores read by stock extraction; later Scene
travel uses the same transaction and native retry/discard custody. An independent resource
aggregate retains its own publication responsibility. See the
[Runtime base entry](runtime-content-base.md#materialization-and-retention). Packaged executable
entry and target build selection remain in the
[ordered delivery plan](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md).
