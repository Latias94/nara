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

The reference-demand and deep-validation changes are being integrated under the
[CP3 plan](../plans/2026-10-30-reference-validity-and-load-demand-plan.md). Complete consumer
qualification remains in progress; this API description supplies no new native/platform or
performance qualification.

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
| `RuntimeContentValidationReport` | Catalog digest and counts from strict mandatory payload validation |
| `RuntimeInputContent` | Global startup definition retaining its actual typed acquisition independently of Scene collections |
| Native handle and residency | Separately admitted and published state in a receiving Runtime |

The Host supplies `RuntimeContentExpectation` with the expected catalog digest and target/profile.
Obtain those facts from the selected trusted build result. Reading a catalog's digest or target
and treating those same values as the expectation would remove the independent check.

The catalog declares Runtime configuration, plugin/Schema/codec fingerprints, startup and later Scene
roots, explicitly addressable roots, member formats and dependencies. Multiple compound products
can map to one member. Duplicate product mappings, file aliases, invalid roots and absent mandatory
targets refuse admission. A dependency's role distinguishes target validity from current demand:

| Catalog role | Actual payload demand | Ordinary preparation |
| --- | --- | --- |
| `Required` | `AssetReferenceDemand::Immediate` | Prepare the target and follow its admitted Immediate dependencies |
| `Reference` | `AssetReferenceDemand::Deferred` | Require catalog membership and executable format agreement; defer target decoding |
| `Soft` | Optional missing-allowed metadata, outside the payload inventory | Trigger no preparation or deep mandatory traversal |

Both Required and Reference are mandatory delivery declarations. Soft cannot satisfy a present
Schema or codec reference. Optional/null field values describe presence separately from demand.

A Required reference may name another Scene, including a finite back-edge. First-party preparation
admits and retains those Immediate Scene documents as data; it instantiates only the selected root.
A Deferred Scene target stays outside that preparation's decoded collection. Neither role makes
a dependent Scene a public travel root. The separate
[source-backed workflow](rust-product-recipes.md#strict-tools-use-the-compiled-project) resolves
actual Scene sidecars. Ordinary source preparation retains its Immediate document closure and
resolved Deferred target facts; strict saved-source Check/Export validates both demands. Those
source receipts and declared-root checks remain separate from this immutable processed base.

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
original plan's exact definitions. It traverses actual recursive Schema asset leaves, including
defaults, and preserves each leaf's Immediate/Deferred demand. Arbitrary strings do not become
references. See [persistent components](persistent-components.md#asset-reference-demand) for
`#[nara(asset_demand = deferred)]` and hand-authored mixed-demand shapes.

For each decoded Scene or resource member, the Host aggregates occurrences by complete Product:
any Immediate occurrence wins; otherwise the aggregate is Deferred. This complete actual set must
equal the member's mandatory declarations, with Immediate mapped to Required and Deferred to
Reference. Missing edges, extra mandatory edges and wrong roles reject before any outgoing edge
can trigger work. Soft metadata is excluded from that equality. A mismatch returns
`RuntimeContentError::DependencyMismatch { product, declared, actual }`; an omitted declaration
has `declared: None`, and an extra mandatory declaration has `actual: None`.

Resource acquisition requires the concrete Rust family and codec already selected by that base's
original plan. Kind/version alone cannot authorize a same-format replacement implementation.
The stable definition revision declares executable meaning independently of the persistent format;
frozen work/output limits and concrete CPU/Image/TileSet role also participate in agreement.
Different formats or versions may serve one family; ambiguous selectors refuse composition.
The codec's complete-identity query and the family's membership query validate every
Product mapped to the member. The compound text consumer checks the enclosing source as well as
each auxiliary ID; valid bytes for a different source are rejected. The required
`visit_runtime_dependencies(product, visitor)` query enumerates every actual reference as
`visitor(target_product, demand)`, where demand is `AssetReferenceDemand`. Implement an explicit
empty inventory when the format has no references. The query borrows existing values, allocates
no scratch, stops on the visitor's concrete error and grants no I/O or dependency loading.
Forwarding codecs pass the visitor through without changing demand. Decoder/query panics return
a rejected acquisition without publishing a product. Membership and dependency agreement alone
do not decode a Deferred target or establish its semantic validity or native residency.

The concrete [Runtime Image guide](runtime-images.md) describes `ImageDomain` and
`ImageRuntimeCodec`, source-independent pixels and the original decode reservation retained by
escaped CPU/native/prepared captures. Image membership is one complete Product per member.

## Strict Payload Validation

`open` proves immutable encoded capture and agreement. Use
`RuntimeContentBase::validate_deep()` to validate the actual mandatory delivery payloads:

```rust,ignore
let validation: RuntimeContentValidationReport = base.validate_deep()?;
assert_eq!(validation.catalog_digest(), base.catalog_digest());
println!(
    "Validated {} members and {} Product mappings",
    validation.members(),
    validation.products(),
);
```

Validation starts from declared Scene roots, addressable roots and global Input, then follows both
Required and Reference edges after exact payload dependency admission. Dependency-only Scenes
participate without gaining travel authority. Soft metadata alone never causes a target decode.
Each reachable member uses its original selected production parser/codec and complete Product
membership checks, including the concrete first-party Image/TileSet/Tilemap CPU constraints.

The returned `RuntimeContentValidationReport` identifies the checked catalog through
`catalog_digest()`. `members()` counts unique validated encoded members; `products()` counts their
admitted Product mappings, independently of repeated reference occurrences. These are counts for
the mandatory reachable closure, rather than every unused catalog entry.

Deep validation releases heavy decoded outputs between validation units and retains bounded
cross-member facts. Finite index and native-fact capacity is admitted from catalogue membership
before traversal, including currently unreachable members; this conservative scratch allowance
does not decode those members. It publishes no native stores or receiving World state. The report retains
no decoded resource closure; ordinary preparation and later acquisition still use their original
owners and admission. Capture may succeed while deep validation refuses malformed Deferred
content, invalid auxiliary membership or its own overlapping work budget.

## Global Startup Input

Select an optional complete primary Product in `RuntimeConfiguration::input.action_map` and
register the executable decoder with the recipe:

```rust,ignore
configuration.input.action_map = Some(action_map_product);
let recipe = game_recipe()?.add_runtime_asset_codec::<ActionMapDomain, ActionMapRuntimeCodec>(
    RuntimeCodecDefinitionId::new("studio.input.runtime", 1),
)?;
```

The selected configuration must include the actual Input plugin. Catalog admission requires one
`runtime_action_map` version-one member containing exactly that primary Product. This startup
selection does not grant Scene or addressable-root permissions. Source projection retains its
path obligation separately; it does not silently convert an authored path into this Product.

The first-party runs acquire and install global Input once before Scene materialization and
Startup. `RuntimeContentBase::acquire_startup_input()` exposes the optional retained CPU owner for
inspection. Its bindings preserve the actual `RuntimeContentAsset<ActionMapAsset>`, original
base and decoded-output reservation. Mutable context and mapping state receives a separate
reservation from that base. Explicit code configuration, including an empty map, competes with
the selected Product and refuses before Startup.

Scene travel preserves the global owner and mutable state. An explicit Scene Required reference
may independently acquire the same Product as typed CPU data and pay its own decode/output
charge; it gains no installation authority. Failure or cancellation preserves the original
running World, and an escaped bindings handle retains its acquisition until its last owner drops.
The [Input guide](input-actions.md) describes cooking, contexts and the distinct file kinds.

## Materialization And Retention

First-party runs prepare only the verified Immediate resource/data closure through the original
immutable base, then use the existing candidate startup, Scene request and residency lifecycle.
Reference edges retain delivery obligations without preparing their target payloads for this run:

```rust,ignore
let mut run = HeadlessRun::<GameOutcome>::from_runtime_content(
    base,
    HeadlessRunOptions::frames(frame_limit, Duration::from_millis(16))
        .stop_when(|outcome| outcome.finished()),
    Vec::new(),
);
let report = run.execute_bounded();
assert!(report.startup_content_closure().is_none()); // Source startup attestation is separate.
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

`prepared.scene_dependency(scene_product)` on a `PreparedProjectSceneView` returns a borrowed
`Option<&SceneDocument>` for an Immediate Scene data dependency already prepared for that request.
For example, an encounter/rule Scene can supply CPU data without being instantiated. The selected
root, Deferred-only targets and unrelated members return `None`; the query performs no loading
and grants no travel authority. The borrow is limited to the prepared view and its actual content
owner. An independent declared-root check still governs `ProjectScenes::request`.

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
Native GPU/device objects, additional concrete family residency, broader Input device/replay workflows, the
complete packaged Player/build entry, target stripping/build selection, CAS, patch/DLC and remote authenticity
remain separate work in the [ordered delivery plan](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md).
The current local base neither adds a universal decoder registry nor implements a second asset
transaction, package resolver or Rust dependency resolver.
