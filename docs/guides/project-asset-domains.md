# Compiled Project Asset Domains

Use a source domain when one referenced source produces immutable typed CPU data for project
Check and Play. Select the package in the same compiled `ProductRecipe` used by the game and
project tools:

```rust,ignore
fn recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    game_schema_recipe()?.add_asset_domain::<text_package::TextDomain>()
}
```

Processed delivery selects its concrete decoder separately with
`add_runtime_asset_codec::<D, C>(RuntimeCodecDefinitionId::new("studio.package.runtime", 1))`.
Selecting source import never enables Runtime decode. The original Runtime configuration plan
freezes that typed binding and its limits, and the catalog must match its dedicated codec
fingerprint. The [Runtime base guide](runtime-content-base.md) describes processed startup and
later Scene requests. Collection reads are shared: `get` checks full Product membership, `iter`
visits stored values once and `source_records` exposes only actual source metadata.

Immediate/Deferred source preparation and processed dependency validation are being integrated
under the [CP3 plan](../plans/2026-10-30-reference-validity-and-load-demand-plan.md). Complete
consumer qualification remains in progress; this guide supplies no new native/platform or
performance qualification for that change.

Implement the pure source/import facet with `nara_asset` and its `serde` feature. Its settings,
input, limits and error types require no root facade, Scene, 2D or Editor dependency. Product
selection, typed collections and project workflows remain in the root Host and require
`runtime-2d` and `serde`; `project-workflow` enables both. The
[independent text package](../../tests/fixtures/module-consumer/asset-domain-consumer/src/lib.rs)
and its [public workflow tests](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/workflow.rs)
are complete examples outside root workspace inheritance.

## Domain Contract

Implement `nara_asset::SourceAssetImporter` with a stable `SOURCE_KIND`, associated `Settings`
and generic `prepare<E>`. Implement `AssetDomain` for the typed `Product` shared by operation
facets and supply conservative `AssetImportLimits`. Persist
`AssetSourceKind::Other(SOURCE_KIND)` in the source's
canonical `.meta` identity. The Host routes declared persistent `AssetRef` fields by that identity.
It does not infer a handler from file extensions or crate presence. Two selected owners of the
same kind reject; a referenced kind without an owner fails Check/Play. Unreferenced files need no
handler and are not rewritten.

The identity sidecar's `AssetPath` is local to the actual configured source directory. The Host
binds it through an explicit `AssetPathScope` to the catalogue's `AssetSourcePath`; directory role
and source kind never choose that location. For example, a sidecar path `dialogue.txt` under
`content/text` becomes the canonical record location `content/text/dialogue.txt`. Collection
`get(&AssetRef::Path(..))` still takes the authored local alias. `source_records()` exposes real
canonical locations; do not prepend Assets again or save them as local aliases.

`AssetImportSource::metadata()` returns an identity-first candidate. Admit the package's settings
with `admit::<Self::Settings>()` and validate only domain invariants. Source-only settings need
deserialization; serialization and editable defaults belong to the optional Editor facet.
`bytes()` provides the bounded
primary source. `read_dependency(path, limit)` reads a raw auxiliary source under the same Assets
capability and records its exact digest. Paths are Assets-relative; no ambient filesystem or
project-root handle is exposed. Auxiliary files have no implied metadata or decoder. This is not
yet a cross-domain prepared-product dependency API.

All required source reads must use this context. Repeated auxiliary reads share one immutable
observation per source-domain load while each declared edge counts against the global limit.
Strict Check visits declared Scene roots and their complete mandatory delivery closure, then
unions their exact members. Export reopens the receipt members and verifies their digests instead
of rediscovering dependencies.

Source limits bound reads. The Host reserves parser work and retained product allocations before
calling the parser; collection and identity overhead are charged separately. Settings parsing,
temporary strings and any shared source buffers retained by a product must fit the declared
reservations. This is trusted Rust: limits are a domain promise, not an allocator sandbox. Keep
products and their destructors effect-free, and do not panic. Return a bounded `Diagnostic` with
`AssetImportError::domain` for actionable format/settings failures. `read_dependency` returns
`AssetImportError<E>`, so ordinary `?` preserves the caller's concrete read failure without
requiring a root product error type. The project adapter maps only domain diagnostics; source
IO, authority and budget errors retain their original classification. Host-reported parser
panics omit the panic payload from the diagnostic report.

```rust,ignore
impl nara_asset::AssetDomain for TextImporter {
    type Product = TextResource;
}

impl nara_asset::SourceAssetImporter for TextImporter {
    type Settings = TextSettings;
    const SOURCE_KIND: &'static str = "studio.text";

    fn limits() -> nara_asset::AssetImportLimits {
        text_import_limits()
    }

    fn prepare<E>(
        source: &mut nara_asset::AssetImportSource<'_, E>,
    ) -> Result<TextResource, nara_asset::AssetImportError<E>> {
        prepare_text(source)
    }
}
```

The generic parameter belongs to the reader's caller. Importers propagate it and author their
own domain diagnostics; they do not need to inspect filesystem or project Host errors.

The compiled-project tool preserves retained domain code/severity/safe-summary records through
`ProjectToolInvocationError::Project`; it does not infer them from `Error::to_string()`.
Inspect the bounded diagnostics and loss facts described in the
[tool failure guide](rust-product-recipes.md#inspect-compiled-tool-failures). General diagnostic
fields are deliberately not transported. This does not change the domain's preparation contract
or execute runtime plugin hooks during Check.

## Current Demand And Strict Source Validation

The recursive Schema AssetRef leaf owns `AssetReferenceDemand::{Immediate, Deferred}`. Ordinary
Rust AssetRef fields default to Immediate; use
[`#[nara(asset_demand = deferred)]`](persistent-components.md#asset-reference-demand) for a
future-use field. A present Deferred reference still requires a readable source with admitted
identity, source kind and actual canonical binding. Optional/null presence does not permit a
present reference to name missing content.

Ordinary `ProjectContentLoader` preparation decodes the Immediate closure. For Deferred targets,
it retains resolved target and metadata/receipt facts without expanding a target Scene or running
the target's importer and resource preparation. A resolved target is weaker than a validated
decoded Product: generic metadata cannot prove an arbitrary compound auxiliary Product exists.
A later actual request checks its selected importer and membership before publication.

Strict saved-source Check/Export follows both demands, including dependency-only Scenes. It uses
the production Scene compiler and selected source importers, then calls
`AssetDomain::contains_product` on actual prepared values to establish compound membership.
Malformed Deferred content or a missing auxiliary selection refuses strict validation even when
ordinary preparation of the referring Scene only resolved the target. Validation-only products
do not enter that Scene's current typed/native collections or become travel roots.

`ProjectContentLoader::load_delivery` returns the separate `ProjectContentDeliveryReceipt` for
that saved-source closure. Its `members()` includes mandatory Deferred targets; ordinary startup
attestation continues to describe the startup Immediate preparation. Export consumes the strict
delivery result. Editor preview uses the admitted dirty working copy for an Immediate Scene
dependency; Deferred targets retain binding observations without adding their document body,
revision or resources to the current preview. Saved-source Check does not certify a dirty copy.

## Runtime Access

Borrow the installed resource from an ordinary system:

```rust,ignore
fn show_text(assets: nara::ecs::Res<nara::project_host::ProjectDomainAssets>) {
    if let Some(texts) = assets.collection::<text_package::TextDomain>() {
        for text in texts.iter() {
            // Consume each stored typed CPU value once.
        }
    }
}
```

`collection::<D: AssetDomain>()` distinguishes families even when they use the same Rust product type.
`get(&AssetRef)` accepts a complete admitted Product, including auxiliary selections. Source
collections first locate the real source record and use the domain's pure membership query.
Processed collections use exact Product mappings retained from their decoded members. A source
path aliases only a real primary source record; processed collections have no paths. Stable IDs
always select the exact primary Product. Use the concrete product's own lookup API to select a
value inside a returned compound bundle. `source_records()` visits actual source records only;
`iter()` visits stored values independently of aliases. No mutable collection is exposed.
Clone `ProjectDomainAssets` when retaining data beyond a borrow; this keeps its generation and
budget alive. A domain whose products share allocations must retain this owner alongside them.

The Host installs products before gameplay startup. Ready Scene requests expose target
`domain_assets()` for preparation, while the current runtime keeps its old collection. An explicit
successful Scene commit changes Scene and collection together. Failed preparation or rejected
commit preserves the current values. A Scene with no domain sources installs an empty collection.
Normal runtime shutdown releases the runtime owner; escaped collection clones retain their budget.

## Scope

This source preparation runs during Check and during source-backed Play. Ordinary runtime plugin
hooks still do not run during Check. An Editor importer alone does not install this capability.
Image and TileSet retain their concrete typed stores and GPU/native publication rules. Editor
native/GPU import adapters, cross-domain products, cooking, source-free export, watchers and dynamic plugin
loading need their own concrete consumers. The metadata-only Editor facet below is a narrower
source-backed workflow. See [ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md).

## Processed Runtime Facet

`RuntimeAssetCodec<P>` defines a concrete format independently of `SourceAssetImporter`. A compiled
Host binds a selected family and codec with `C: RuntimeAssetCodec<D::Product>`. Different codecs
can encode the same family without creating another logical product family. `RUNTIME_KIND` and
`RUNTIME_VERSION` name the exact format; `AssetRuntimeLimits` promises encoded, work and retained
budgets. The caller reserves these before the operation and separately accounts for identity,
collection and overlapping generations. The codec validates its own bounded format and semantic
invariants and returns `AssetRuntimeError` with an actionable diagnostic.

Implement `contains_runtime_product(&product, reference)` as a pure complete-identity query.
Unlike the source import family's file-local membership query, this check includes both the
decoded product's enclosing source and its selected product ID. The Runtime Host checks every
catalog mapping for a shared member. Accepting any source merely because a subproduct ID matches
would let valid bytes for another resource inherit the catalog's identity.

Implement `visit_runtime_dependencies(product, visitor)` over the actual decoded value. Invoke
`visitor(target_product, demand)` for every present reference, using `AssetReferenceDemand` for
that actual leaf. Existing unconditional dependencies use Immediate; the TileSet codec reports
its actual Image leaf as Immediate. A format without references explicitly returns `Ok(())`
without invoking the visitor. Forwarding codecs pass both arguments through unchanged. Stop on
the visitor's concrete error; the query allocates no scratch, loads nothing and performs no
external effects.

The Host normalizes duplicate complete Products with Immediate precedence. This actual inventory
must equal the member's mandatory catalog declarations: Immediate corresponds to Required and
Deferred to Reference. Both roles require captured membership and executable format agreement.
Missing, extra mandatory and wrong-role edges reject with `DependencyMismatch` before outgoing
edges can trigger work. Soft metadata is missing-allowed, excluded from that equality and unable
to satisfy a present payload reference. Ordinary preparation follows only admitted Immediate
edges; [`base.validate_deep()`](runtime-content-base.md#strict-payload-validation) follows both
mandatory roles through the selected production decoders without publishing native stores.

`runtime_retained_bytes` reports the actual modeled inline/owned storage after decode, bounded
by the pre-reserved maximum. Query failure or panic returns no usable candidate and releases
the original reservation. These checks do not establish native residency or select another codec.

Neither format conversion method reads authoring source, sidecars or generated cache. Target
cooking, member integrity, Runtime catalog admission, IO and publication remain separate owners.
The compound text package uses this facet with `AssetProductRef` and `ImportedProductId` to keep
two text entries stable across explicitly authored rebinds. The whole bundle owns the immutable
entry map. A persistent `AssetRef::Product(AssetProductRef)` uses the existing Schema asset leaf,
collector and rewrite, including nested/default references and their exact leaf demand. The
project Host checks currently prepared Immediate targets and all targets in strict delivery with
`AssetDomain::contains_product` on the actual prepared value. Its default accepts only `PRIMARY`;
compound domains provide their bounded, pure membership query. Ordinary Deferred source
resolution alone does not prove Product existence. Source-database Image/TileSet resolution and
Prefab sources refuse auxiliary selection instead of silently loading the primary. Already
admitted complete Runtime Products can resolve through their typed bindings without a source
database. This does not install global subresource loading.
The [scoped identity decision](../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md)
defines those limits. An encoded round trip alone is not source-free project delivery.

The root `runtime_content` module captures a Host-expected immutable local base and admits typed
Scene/resource acquisitions from those verified bytes with the original executable composition.
Its catalog contains processed members, declared dependencies and roots, rather than authoring
metadata. See the [Runtime base guide](runtime-content-base.md) for the capture and decode boundary
and the integration work still needed before a complete packaged Player claim.

For processed Runtime resources, a concrete content owner can reserve a complete identity with
`AssetServer::reserve_product::<T>(reference)` and prepare the actual `Assets<T>` value through
its own transaction. The server's Product index is authoritative for runtime handles; a primary
source path is an optional authoring alias, not required Runtime data. `AssetRef::resolve` for a
Product uses an existing binding and rejects unknown/wrong-typed selections. It never opens a
source or creates a missing product. Reverse conversion preserves auxiliary and pathless primary Product
references. These APIs prove weak identity/type agreement only; the owner must still admit the
actual format, membership and residency. Source database operations retain their primary-only
contract. The [native processed consumer](../../tests/fixtures/module-consumer/tests/runtime_scene_assets.rs)
uses actual primary/auxiliary values with native Scene codecs and no decode-time source database.

The same consumer stages a new later-Scene Product through `SceneAssetBindings::reserve_product`
and supplies its values through the existing product resource overlay. A final native failure or
explicit discard leaves both the old catalogue and actual values intact. Successful replacement
publishes all three together without changing the live server during preparation. Product-owned
resource overlays capture the installed resource's concrete incarnation/content baseline and
recheck it before commit; mere resource presence cannot authorize overwriting newer values.
The [Runtime TileSet guide](runtime-tilesets.md) applies this same transaction to actual
Image/TileSet values, required Image dependencies and persisted cell membership.

## Editor Metadata Import

A trusted compiled Editor package can implement `EditorSourceAssetDomain` with `type Domain`
naming its source domain. The adapter can live in a separate crate without forwarding the
domain's parser, limits or identity. The domain's one `Settings` type owns typed admission;
the Editor facet additionally requires serialization, and `default_settings` seeds a panel-local
draft. The facet reuses `SourceAssetImporter::prepare` to validate primary and
auxiliary sources. The Host discards that temporary CPU value and publishes canonical `.meta`
only when validation succeeds. It does not create a cooked artifact or install a preview resource.

The [external text adapter and panel](../../tests/fixtures/module-consumer/editor-panel-consumer/src/text_import.rs)
show the complete typed contribution. Select the contributor in the Editor executable:

```rust,ignore
let mut composition = nara_editor::EditorNativeComposition::new();
composition.register_source_asset_domain::<text_editor::TextImportDomain>()?;
text_editor::install_panel(&mut composition)?;
composition.run(options)?;
```

For an embedded shell, consume `composition.build(options)` to obtain the fully configured
`(application, frontend)` pair. This is the same construction used by `run`; it transfers both
Host facets and panels together. The explicit advanced `build_trusted_compiled` and native
trusted runner preserve the same selection for project replacement. Their initial compiled
execution lane has no generic source index, so metadata Import remains unavailable there;
replacing it with an ordinary authoring session enables the selected facet.
`install_panels` attaches only frontend contributions to an existing shell and rejects a
composition with selected Host facets before changing its workspace.
The compiled selection persists across project replacement; operation
state belongs to each fresh session. Ordinary Open remains data-only and never loads game Rust.
The game independently selects its source domain through `ProductRecipe` for Check and Play.

An authoring facet with continuity or other predecessor-sensitive settings overrides
`validate_settings_successor(previous, candidate)`. The Host supplies settings decoded from the
freshly admitted existing metadata leaf and its candidate, before source preparation and inside
the worker's budget/panic boundary. Default behavior permits ordinary stateless settings changes.
The compound bundle facet uses its domain-owned `candidate.validate_successor(previous)` to reject
lost retirements or reuse of known-retired product IDs. A standalone valid candidate is not proof
of an admissible transition.

The original file publisher still fences the previous object and digest before writes. Candidate,
predecessor and encoded transport remain charged while queued/running/disposed; cancellation
does not free their reservation before actual worker return. This proves the transition against
the current admitted leaf, not unlimited history after manual sidecar edits or branch rollback.

Panels read `EditorFrameModel::source_asset_import`, retain their typed draft and submit
`EditorAction::SourceAssetImport`. Use the facet's `import_action(path, &settings)` to derive the
exact domain kind and encode that domain's settings without repeating transport details.
Encoding failure remains local feedback; the Host still admits the resulting dynamic action.
The action uses its current frame and indexed source authority; extensions and registration order
do not choose a format. Dispatch `Started` is not terminal success. Later frames report preparation,
`MetadataPublished`, actionable failure or a reconciliation requirement. The view follows the
Browser selection, or the latest operation target when nothing is selected.

Import creates a stable identity. Reimport preserves it and changes only the requested metadata
settings after validating the same bounded source closure. Invalid/conflicting existing metadata
is never overwritten implicitly. Failure preserves its exact prior bytes. No metadata is written
on Open, selecting a source or editing a draft. The Host reuses the existing metadata identity scan,
task runtime, file-publication receipts and cleanup custody; the panel receives no directory writer.
After publication, the previous metadata detail is unavailable until its successor catalog is
ready. A newer frame containing a retained Browser snapshot cannot start another operation against
the old sidecar binding. The existing metadata-refresh owner supplies native maintenance and
settles failed discovery without a second source-import refresh loop.
The Shell observes import-view and pending-state changes for presentation; unchanged idle polls
do not request another redraw.

This validate-before-publish policy differs from Image's separate desired settings, in-flight
attempt and last-good pixel publication. Image retains unresolved metadata observations across
rejection, failure and supersession until the current recipe publishes; old pixels do not choose
the settings for a later source-only edit. See
[publication and recovery](file-backed-tilesets.md#publication-and-recovery) for Image/TileSet
admission and stale-result handling. This source-domain facet still validates before changing
metadata bytes.
Editor native/GPU publication, arbitrary import-only products, automatic reimport, durable caches
and dynamic project plugin loading remain separate contracts.

## Finite Local Artifact Builds

Source-backed Check/Play remains usable without a durable build cache. A domain that also
supports generated encoded output implements the optional `nara_asset::SourceAssetBuild` facet:
select its `RuntimeAssetCodec`, name the build ID/revision, normalize effective settings and
enumerate every actual primary/auxiliary Product. Only this facet requires serializable settings.
Its default `build` executes the unchanged source importer. A custom build may additionally use
`AssetBuildSource::read_optional_dependency`; `None` records actual absence, while authority,
budget and I/O failures preserve the caller's error.

With the root `project-workflow` feature, a trusted compiled Host opens an `ArtifactBuildStore`
from an existing read/write directory capability and explicit `ArtifactBuildLimits`. It supplies
an `ArtifactSourceRequest` with the actual source directory, logical scope, local alias and
selected versioned definition. The definition's implementation digest names independently
observed implementation bytes; a revision string or hash of an unused label is insufficient.
Generated cache metadata records that selection and cannot choose executable code.

```rust,ignore
use nara::asset_build::{ArtifactSourceRequest, build_source};

let request = ArtifactSourceRequest::new(
    source_directory,
    source_scope,
    local_source_path,
    selected_build_definition,
);
let result = build_source::<MyDomain>(&store, &request, |point| {
    revalidate_selected_request(point)
})?;
if let Some(receipt) = result.publication_receipt() {
    observe_publication(receipt.index_receipt(), receipt.directory_sync_tier());
}
let group = result.into_group();
let runtime_bytes = group.member_bytes("runtime").expect("source build Runtime member");
```

The Host admits/normalizes current metadata before probing reuse. It records the exact normalized
values passed to execution and replays the prior raw/presence observations against current inputs.
Raw read limits belong to the source domain, independently of encoded output member size. A miss
replaces the observation set with the current execution's actual reads; discarded dependencies
do not remain in a successor recipe. The callback runs on reuse and publication boundaries, must
fence the current request, and must not re-enter the same store while its commit lock is held.
The source executor revalidates actual file bindings and bytes after the callback.

`build_image_import` imports an actual PNG into source-free Runtime Image bytes. The explicit
`cook_image_group` adapter consumes a verified group's captured member and runs the real CPU
Image cook with the selected settings, target and limits. The caller fences that immutable
upstream group's current selection; immutability alone does not establish that it is still desired.
Processed observations include the complete upstream group, member, format and Product, so raw
PNG equality does not hide changed color-space settings. The adapter does not resize an atlas
implicitly or invoke GPU work.

A result is current `Hit` or `Built`, with a reason and complete verified encoded group. A build
retains its actual successful publication receipt; reuse performs no index replacement and has
no new receipt. Publication writes blobs and manifest before the single final index boundary.
An error or panic after that boundary returns `ArtifactBuildError::Uncertain` with the actual
receipt and cause. Missing/corrupt generated data is an explicit invalid-cache result; it never
becomes a current hit or silently usable last-good output. Lock/directory-sync guarantee tiers
are observations, not a promise of portable crash atomicity.

Groups retain exact captured bytes and their original allowance across store drop or cache
deletion. Feed those bytes into the existing Runtime catalogue/pack delivery path; Runtime
acquisition never invokes this builder. Product identity remains authored, independently of
recipe/content hashes and generated names. See the independent
[compound execution consumer](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/durable_build.rs)
and [Image cook/delivery consumer](../../tests/fixtures/module-consumer/tests/durable_image_build.rs)
for complete capability-backed examples. Target executable selection and a packaged Player
remain separate from these local build operations.

## Live Typed Source Refresh

An authoring or development Host can select `SourceAssetBuildPlugin<D>` to connect this build
facet to the existing Asset source-change and Tasks phases. It requires `AssetPlugin`,
`AssetSourcePlugin` and `TaskPlugin`; it does not install a watcher. The Host supplies the same
actual read capability, canonical scope, compiled definition and shared store used for finite
builds. The plugin ID is the domain's build ID, and a source kind has one typed consumer.
Different kinds may use the same product type; their registrations and request authorities
remain separate, while actual handles are type-checked per asset ID.

```rust,ignore
use std::sync::Arc;
use nara::asset::{
    AssetPlugin, AssetSourcePlugin, AssetServer, ProjectAssetDatabase,
    AssetSourceChange, AssetSourceChangeKind, AssetSourceChanges,
};
use nara::asset_build::{BuiltSourceAsset, SourceAssetBuildPlugin};
use nara::tasks::TaskPlugin;

app.add_plugins((
    AssetPlugin,
    AssetSourcePlugin,
    TaskPlugin::new(task_configuration),
    SourceAssetBuildPlugin::<MyDomain>::new(
        Arc::clone(&store), source_directory, source_scope, selected_build_definition,
    ),
))?;
app.world_mut()?.resource_mut::<ProjectAssetDatabase>().insert(record.clone())?;
let handle = app.world_mut()?.resource_mut::<AssetServer>()
    .reserve_record::<BuiltSourceAsset<MyProduct>>(&record)?;
app.world_mut()?.resource_mut::<AssetSourceChanges>().push(
    AssetSourceChange::new(record.path().clone(), AssetSourceChangeKind::Modified),
)?;
```

Advance the managed App through its ordinary runtime. Systems read the original
`Assets<BuiltSourceAsset<MyProduct>>` and `AssetStates` to distinguish Loading, Failed and usable
content. `value()` borrows the typed product; clone the complete wrapper when retaining it across
updates. The wrapper retains its original encoded group and decoded reservation together, even
after replacement. `group()` exposes the closed build recipe; it is distinct from a source
`ImportRecipeDigest` and is not copied into that state field.

After the initial request, producers only submit semantic `AssetSourceChanges`. The adapter
derives complete canonical raw-path subscriptions from actual successful reads, including
optional absence and primary/metadata selection. Auxiliary files need no `.meta` or source UUID.
Current value adoption replaces the successful set and clears failed-attempt retry inputs;
successful A-to-B changes remove A. A current source-execution failure preserves last-good and
records bounded retry inputs. Warm reuse replays current inputs and restores subscriptions without
executing the domain. Superseded, cancelled or stale completions cannot adopt a value or edges.
Currentness includes the original issuing generation resource, not only its numeric counter.

Metadata notifications retain their actual raw origin while keeping the existing principal
invalidation semantics. Explicit producers use `MetaModified` for sidecar changes and
`AssetSourceChange::metadata_removed` for a sidecar deletion; ordinary `Removed` denotes a source
file deletion. Coalescing preserves both actual input flags, so a sidecar-only change does not
refresh a consumer that reads only the source bytes. The native watch translator supplies those
facts automatically.

Configure the adapter's `with_pending_limit`, original Task/store limits and dependency graph's
`SourceSubscriptionLimits` for the workload. Admission refusal is diagnosed and retains existing
recovery inputs; it does not imply an unbounded retry loop. Principal removal revokes usable
publication even if the request queue refuses cleanup. Logical pending counters in
`SourceAssetBuildStats<D>` are not evidence of physical worker retirement: obsolete work retains
its original inputs and allowances until Tasks retires it.

The independent [live refresh consumer](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/dependent_resource_refresh.rs)
contains complete source editing, rebind, failure/recovery, warm reuse and retained Runtime
workflows. This adapter updates its explicitly selected typed asset store. Existing immutable
Source/Processed Scene collections and escaped Runtime bases keep their original publication
owners; a processed Runtime does not acquire source execution by using a codec.

## Executable Delivery

Declare the domain package in the game's own Cargo dependencies and register it in the product
recipe shared by the tooling, headless and desktop targets. A recipe supplied only by a test or
development Host does not install the domain in a separately compiled product. Export carries
the admitted primary source, metadata and auxiliary members; domain code is compiled into the
product executables through Cargo.

The [compiled CLI trajectory](../../tests/project_workflow_cli.rs) adds the independent text package
to a generated product, references its source from a persistent Caption, and observes the parsed
text in gameplay. It follows the ordinary Check/Play/Repair/Export path, then runs the exported
headless executable from an unrelated directory after detaching the development project path.
Its missing-auxiliary-source check requires rejection before gameplay. The separate
[source Export tests](../../src/project_workflow/export/source_tests.rs) retain their synthetic
build observations for focused source-change and publication checks. The
[Windows qualification record](../verification/2026-10-21-compiled-asset-domain-export.md) captures
the actual execution and its platform limits.
