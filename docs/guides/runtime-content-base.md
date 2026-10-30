# Verified Runtime Content Bases

The root `runtime_content` module provides bounded immutable local content capture and concrete
typed acquisition. It is available with `runtime-2d,serde`. A base contains processed Scene and
resource members plus `catalog.json`; opening it does not read `nara.toml`, source sidecars,
an authoring Schema bundle or an import cache.

The first-party `HeadlessRun` and `DesktopRun` accept an admitted base independently of their
source-backed entries. The Editor retains its source authoring workflow. Base capture does not
cook a target, select Cargo artifacts or qualify a complete packaged Player. First-party native
residency supports the explicitly selected Image and TileSet facets; other families expose typed
CPU collections and keep concrete native publication with their consuming owner.

## Authority And Identity

| Value | Meaning |
| --- | --- |
| `AssetProductRef` | Complete persistent source and primary/auxiliary selection |
| Member path | Portable location inside the local base, never a source lookup fallback |
| Member digest | Expected encoded length and content hash |
| Catalog digest | Exact catalog bytes selected independently by the Host |
| `RuntimeConfigurationPlan` | Original compiled plugin, native/data Schema and Runtime codec selection |
| `RuntimeCodecDefinitionId` | Package author's versioned declaration of one compiled family/codec binding |
| `RuntimeContentBase` | Shared immutable encoded snapshot plus its original configuration plan |
| `RuntimeContentAsset<P>` / `RuntimeContentScene` | Admitted decoded output retaining its base and reservation |
| Native handle and residency | Separately admitted and published state in a receiving Runtime |

The Host supplies `RuntimeContentExpectation` with the expected catalog digest and target/profile.
Obtain those facts from the selected trusted build result. Reading a catalog's digest or target
and treating those same values as the expectation would remove the independent check.

The catalog declares Runtime configuration, plugin/Schema/codec fingerprints, startup and later Scene
roots, explicitly addressable roots, member formats and dependencies. Multiple compound products
can map to one member. Duplicate product mappings, file aliases, invalid roots and missing required
dependencies refuse admission. Soft dependencies may be absent; declaring one does not start a
load or satisfy an actual required Scene reference.

A Required reference may name another Scene, including a finite back-edge. First-party preparation
admits and retains those Scene documents as data; it instantiates only the selected root. A dependent
Scene is not automatically a public travel root. This processed capability does not establish
source-side Scene/sidecar discovery; the authored Door path has its own pending correction in the
ordered delivery plan.

## Capture And Acquisition

Construct the content catalog from a resolved executable configuration and processed member
descriptions. Its canonical encoder orders equivalent definitions independently of insertion
order. Use the resulting catalog bytes' `ContentDigest` as part of the trusted build result.
The Runtime receives a Host-issued read capability for the base directory and the independently
selected expectation:

```rust,ignore
let executable_recipe = game_recipe()?
    .add_runtime_asset_codec::<TextBundleDomain, TextBundleDomain>(
        RuntimeCodecDefinitionId::new("studio.text-bundle.runtime", 1),
    )?
    .add_runtime_image_codec::<ImageRuntimeCodec>(
        RuntimeCodecDefinitionId::new("nara.image.runtime", 1),
    )?
    .add_runtime_tileset_codec::<TileSetRuntimeCodec>(
        RuntimeCodecDefinitionId::new("nara.tileset.runtime", 1),
    )?;
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
actual executable recipe and compares plugin/Schema/codec fingerprints. Each Asset member must
have a uniquely selected format/version before capture. It then bounds and captures
every declared member, checking each exact encoded digest. Only a complete base is returned.
The returned base retains no filesystem capability. Replacing or deleting files afterwards cannot
change its acquisitions. Reopening another base creates another snapshot; it does not redirect
work already using the first one.

Future container/package providers must preserve this immutable content-set boundary. A provider
or overlay contributes a separately admitted set or composed successor; it cannot mutate a live
base, change an authored Product's meaning or redirect existing acquisition/recovery leases.
Original base/plan identity remains distinct from equality of catalog bytes. This is an evolution
constraint, not an implemented provider, overlay precedence or package-resolution API.

Scene acquisition uses the expected containing identity in the member description and the
original plan's exact definitions. It also traverses actual Schema asset leaves and requires each
complete Product to be present and declared as a required dependency of that Scene member. It
does not treat arbitrary strings as references or trust the dependency table in place of payload
inspection.

Resource acquisition requires the concrete Rust family and codec already selected by that base's
original plan. Kind/version alone cannot authorize a same-format replacement implementation.
The stable definition revision declares executable meaning independently of the persistent format;
frozen work/output limits and concrete CPU/Image/TileSet role also participate in agreement.
Different formats or versions may serve one family; ambiguous selectors refuse composition.
The codec's complete-identity query and the family's membership query validate every
Product mapped to the member. The compound text consumer checks the enclosing source as well as
each auxiliary ID; valid bytes for a different source are rejected. The required
`visit_required_runtime_products` query enumerates references from the decoded payload. The Host
checks each against this base's membership and this member's Required declarations before
returning the asset. Implement an explicit empty inventory when the format has no required
references. The query borrows existing values, stops on the visitor's error and grants no I/O or
dependency loading. Decoder/query panics return a rejected acquisition without publishing a product.
This validates declarations, not the required asset's Rust family, decoded descriptor or residency;
the concrete consuming owner still admits those facts.

The concrete [Runtime Image guide](runtime-images.md) describes `ImageDomain` and
`ImageRuntimeCodec`, source-independent pixels and the original decode reservation retained by
escaped CPU/native/prepared captures. Image membership is one complete Product per member.

## Materialization And Retention

First-party runs prepare the complete Required resource closure through the original immutable
base, then use the existing candidate startup, Scene request and residency lifecycle:

```rust,ignore
let mut run = HeadlessRun::<GameOutcome>::from_runtime_content(
    base,
    HeadlessRunOptions::frames(frame_limit, Duration::from_millis(16))
        .stop_when(|outcome| outcome.finished()),
    Vec::new(),
);
let report = run.execute_bounded();
assert!(report.startup_content_closure().is_none()); // This receipt describes source Check only.
```

Desktop uses `DesktopRun::from_runtime_content(base, DesktopRunOptions::new())`. Execution options
select frame/loop and cleanup policy; they cannot substitute a new recipe or source profile for
the base's original plan. The source-supervised desktop attestation remains a source operation.

Game code requests a declared processed Scene with `ProjectScenes::request(scene_product)`.
`PreparedProjectSceneView::selected_scene()` distinguishes a source path from a complete Product;
the convenience path/product accessors return only the matching selection. Ready typed data is
available through `domain_assets()`. Inspect it, then explicitly commit or begin managed travel.
Live CPU collections, actual Image/TileSet stores and current-Scene ownership change only through
the successful shared publication tail. Failed preparation or native retirement retains the same
last-good and retry/discard owners as the source path. Cancellation waits for physical worker return.

`ProjectAssetCollection::get` requires an exact complete Product for processed auxiliary values;
it never substitutes a primary product. `iter()` visits each stored decoded value once, even when
several complete Products map to it. `source_records()` is empty for processed collections.
One preparation decodes each member once for its selected family/codec, with several codecs of
one family aggregated into one collection. Original acquisition/base reservations remain attached
to typed collections and escaped native pixels; collection metadata is admitted before allocation.

Default native/presentation plugins install no source database, queues, importer or directory
capability. Queued source reload is explicitly selected through `AssetSourcePlugin` and
`ImageSourcePlugin`; processed Host startup rejects those source-execution selections and watchers
before constructing the Runtime. No empty database substitutes for a processed content proof.

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

A `SceneProductResource` supplies a concrete owned baseline through `capture_baseline` and
`matches_baseline`. Preparation captures the currently installed resource; final publication checks
that incarnation and its relevant content again. Use actual owner generations or precise value/state
facts, including equal data in a newly installed owner and same-tick ABA. Temporarily removing and
returning the same unchanged Rust owner does not create a new logical incarnation. Presence,
collection length or an ECS change tick cannot authorize overwriting a newer resource. Stale
refusal preserves the current Scene, bindings and resource values in the existing transaction.

For a mutable game-owned resource, keep replacement-relevant fields private and advance a checked
write generation in its mutation methods. Pair that counter with a stable owned incarnation; normal
writes need no new identity allocation. Constructing another resource establishes a new incarnation,
while data equality can continue to compare gameplay values alone. An immutable complete generation
can instead use its retained owner identity. This is a concrete owner contract, not a World-wide
resource writer policy or a requirement to wrap ordinary ECS access in document transactions.

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
Native GPU/device objects, additional concrete family residency, input-map representation, the
complete packaged Player/build entry, target stripping/build selection, CAS, patch/DLC and remote authenticity
remain separate work in the [ordered delivery plan](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md).
The current local base neither adds a universal decoder registry nor implements a second asset
transaction, package resolver or Rust dependency resolver.
