# Verified Runtime Content Bases

The root `runtime_content` module provides bounded immutable local content capture and concrete
typed acquisition. It is available with `runtime-2d,serde`. A base contains processed Scene and
resource members plus `catalog.json`; opening it does not read `nara.toml`, source sidecars,
an authoring Schema bundle or an import cache.

This API is the content boundary used to build source-independent delivery. The first-party
`HeadlessRun`, desktop launch and Editor source workflows still have their own source-backed
entry paths. The base API alone does not install every asset family, cook a target, select Cargo
artifacts or qualify a complete packaged Player.

## Authority And Identity

| Value | Meaning |
| --- | --- |
| `AssetProductRef` | Complete persistent source and primary/auxiliary selection |
| Member path | Portable location inside the local base, never a source lookup fallback |
| Member digest | Expected encoded length and content hash |
| Catalog digest | Exact catalog bytes selected independently by the Host |
| `RuntimeConfigurationPlan` | Actual compiled plugin and native/data Schema selection |
| `RuntimeContentBase` | Shared immutable encoded snapshot plus its original configuration plan |
| `RuntimeContentAsset<P>` / `RuntimeContentScene` | Admitted decoded output retaining its base and reservation |
| Native handle and residency | Separately admitted and published state in a receiving Runtime |

The Host supplies `RuntimeContentExpectation` with the expected catalog digest and target/profile.
Obtain those facts from the selected trusted build result. Reading a catalog's digest or target
and treating those same values as the expectation would remove the independent check.

The catalog declares Runtime configuration, recipe/Schema fingerprints, startup and later Scene
roots, explicitly addressable roots, member formats and dependencies. Multiple compound products
can map to one member. Duplicate product mappings, file aliases, invalid roots and missing required
dependencies refuse admission. Soft dependencies may be absent; declaring one does not start a
load or satisfy an actual required Scene reference.

## Capture And Acquisition

Construct the content catalog from a resolved executable configuration and processed member
descriptions. Its canonical encoder orders equivalent definitions independently of insertion
order. Use the resulting catalog bytes' `ContentDigest` as part of the trusted build result.
The Runtime receives a Host-issued read capability for the base directory and the independently
selected expectation:

```rust,ignore
let base = RuntimeContentBase::open(
    &base_directory,
    &expected_base,
    executable_recipe,
    RuntimeContentLimits::default(),
)?;
let startup = base.acquire_startup_scene()?;
let later = base.acquire_scene(later_scene_product)?;
let text = base.acquire_asset::<TextBundleDomain, TextBundleDomain>(caption_product)?;
```

`open` verifies the catalog bytes, admits its configuration through `nara_project`, resolves the
actual executable recipe and compares plugin/Schema fingerprints. It then bounds and captures
every declared member, checking each exact encoded digest. Only a complete base is returned.
The returned base retains no filesystem capability. Replacing or deleting files afterwards cannot
change its acquisitions. Reopening another base creates another snapshot; it does not redirect
work already using the first one.

Scene acquisition uses the expected containing identity in the member description and the
original plan's exact definitions. It also traverses actual Schema asset leaves and requires each
complete Product to be present and declared as a required dependency of that Scene member. It
does not treat arbitrary strings as references or trust the dependency table in place of payload
inspection.

Resource acquisition selects a concrete Rust family and codec. Kind/version must match before
decode. The codec's complete-identity query and the family's membership query validate every
Product mapped to the member. The compound text consumer checks the enclosing source as well as
each auxiliary ID; valid bytes for a different source are rejected. Decoder and membership panics
return a rejected acquisition without publishing a product.

The concrete [Runtime Image guide](runtime-images.md) describes `ImageDomain` and
`ImageRuntimeCodec`, source-independent pixels and the original decode reservation retained by
escaped CPU/native/prepared captures. Image membership is one complete Product per member.

## Materialization And Retention

`startup.scene_source()` shares its complete retained owner with the existing startup/current
Scene lifecycle. The private input selection accepts source expansion or processed Runtime data
and calls the corresponding preparation method on the same Scene owner. It preserves receiving
World checks, exact definitions, native retirement claims, product transactions and last-good
replacement. It does not synthesize an expanded source, a project database or source receipts.

`SceneSourceView::with_document` can inspect either retained representation. Representation-specific
views return no value for the other representation. These views are weak borrows: they cannot
keep a retired current Scene or its decoded-output reservation alive.

An admitted CPU asset is not automatically native-resident. A concrete resource owner must still
stage complete Product bindings and actual values through its transaction before a native Scene
codec can resolve them. Use `SceneAssetBindings::reserve_product` and an owned product resource
overlay where appropriate. There is no source import fallback when the selected Runtime codec or
residency owner is missing.

## Bounds And Current Scope

Catalog decoding enforces encoded, structural, count, string/path and modeled work bounds before
publishing a catalog. Base capture separately bounds each member, total encoded bytes and logical
retained storage. Typed acquisition reserves trusted codec work and retained-output promises
before calling the codec. After checking decoded membership, it calls the codec's bounded
`runtime_retained_bytes` query over that actual value. The query includes the inline product and
owned allocations, excludes encoded/base/wrapper storage and allocates no product/query scratch.
Only a bounded domain diagnostic may be constructed on refusal; the query performs no effects.
An underestimated promise or unaccountable value refuses; no post-allocation capacity is borrowed.
Only validated output refunds unused retained capacity and temporary work. The Host adds wrapper
storage once and exposes the complete admitted output charge through
`RuntimeContentAsset::retained_bytes`. Scene decoding uses its existing bounded allocation model
and checked retention cost. Concurrent acquisitions share one base budget; failed, panicked or
dropped outputs release their reservations. Shared clones retain the same reservation, while
separate decodes of one Product remain independently charged.

These are modeled content-owner costs, not allocator capacity or RSS. Compiled plugin/Schema
construction and allocations inside trusted native code retain their existing owner contracts.
Native GPU/device objects, automatic family residency, input-map representation, the first-party
packaged Player entry, target stripping/build selection, CAS, patch/DLC and remote authenticity
remain separate work in the [ordered delivery plan](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md).
The current local base neither adds a universal decoder registry nor implements a second asset
transaction, package resolver or Rust dependency resolver.
