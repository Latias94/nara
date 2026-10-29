# Asset Import Ownership And Processed Runtime Delivery

**Date:** 2026-10-29. **Status:** Source/import, compound identity, declared products, immutable local base and concrete Image/TileSet ownership qualified; first-party processed delivery open.
**Baseline:** `13a2744`.
**Parent:** [Foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md), R6/R7.

## Problem

At the baseline, the compiled source-domain parser imports the root facade to name its input,
error and reservation types. Expressing a pure text importer therefore requires Scene and 2D
features. Registered project domains also rely on source preparation as their only Runtime input.
Source-carrying Export
correctly runs without the development directory, but still reads package-local source and
authoring metadata during startup and later Scene acquisition.

The [delivery audit](../knowledge/engineering/subagents/2026-10/2026-10-28T225432Z-source-carrying-export-still-lacks-an-independent-verified-runtime-content-entry-b9156a4b7e2043f09f3ab41b78cf3bf8.md)
identifies these distinct gaps. Build-machine Cargo and an explicit source-carrying mode remain
valid. Preserve actual filesystem unlink and reviewed artifact reclamation; imported project
copies remain outside installation disposal authority.

## Ordered Development

1. Move the executing source/import contract into `nara_asset` under its existing serde feature.
   Keep typed settings/product, immutable primary bytes, bounded auxiliary reads and domain
   diagnostics. Preserve the caller's concrete read errors. Migrate Check, source-backed Play,
   Editor metadata validation, source Export and independent consumers, then remove the root-owned
   parser protocol without aliases. Root retains source receipts, aggregate budgets, private
   heterogeneous storage, task custody and Scene publication.
2. Extend the independent text package with a compound text bundle and a concrete processed
   Runtime codec. Exercise two persistent subproducts and an auxiliary input. Separate product
   continuity from display name, ordering, content digest and runtime handle. If the source lacks
   intrinsic stable keys, require author-persisted continuity with explicit ambiguity handling;
   generated cache never supplies identity authority. Do not freeze a universal product executor
   or extend every asset reference before the complete consumer establishes its requirements.
3. Add one verified immutable local base catalog containing declared Runtime configuration,
   startup Scene, later Scene and processed resources. Use the existing Scene owner for a declared
   pass-through format or a specifically admitted Runtime representation. Startup and subsequent
   Scene requests capture the same base authority. Missing Runtime support or catalog failure
   refuses without source/import-cache fallback. Preserve existing transactional Scene commit and
   last-good products.
4. Separate the starter's fixed Cargo bins/profile/features and local acceptance runner from
   generic executable selection. Bind target artifacts and content compatibility explicitly.
   Foreign-target build-only evidence stays distinct from actual target execution.

Each step is a reviewable dependency unit, not completion of all R6/R7. Local CAS/group publication,
remote services, patch/DLC and universal all-platform qualification remain later work in the
foundation plan.

Step 1 is implemented as `c352614`, with independently consumed parsing, Check/Play/Scene and
Editor metadata workflows. The [qualification](../knowledge/engineering/verification/2026-10/2026-10-28T233934Z-source-import-contracts-below-project-hosts-with-typed-read-errors-e617bfdd9a104fb69bdf3a20936f7d57.md)
records actual tests, the corrected held-parser fixture, dependency proof and limits.

Step 2 has its [bounded qualification](../knowledge/engineering/verification/2026-10/2026-10-29T004054Z-compound-product-continuity-and-independent-runtime-format-qualification-a2d6d4837fd44218b7cb87f67cb38063.md)
under [ADR 0122](../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md).
The reviewed implementation is `9ba8624`.
The product family marker no longer implies a source importer. A separately selected Runtime
codec can serve that family's product. The compound consumer binds an explicit row assignment to
exact primary source bytes; its Editor facet also validates previous-to-candidate settings through
the actual metadata publication worker. A pure helper alone cannot establish that transition.
Startup/later Scene source-free catalog admission remains step 3.
That predecessor consumer used an ordinary declared bundle `AssetRef` plus a concrete text
selector. Its codec round trip alone did not qualify Schema traversal or required-target validation.

The Runtime catalog step first replaces that opaque selector with a complete declared product
reference in the existing asset-reference grammar. Reflect uses its existing asset leaf traversal
and rewrite, while the project Host validates the selected product against the actual prepared
domain value. Native primary-only stores and Prefab-source loading refuse auxiliary selections
instead of silently loading their enclosing primary. Copying a source remaps its enclosing source
identity and preserves its internal product-ID pattern. This reference integration is a
prerequisite of the named catalog consumer, not completion of source-free delivery.
This prerequisite is implemented as `c3c6d33`, with complete nested/default traversal and rewrite,
strict JSON/RON records, primary-only refusal, actual prepared-domain target checks, source-copy
scope and cached compiled Editor Play refusal/recovery. The
[qualification](../knowledge/engineering/verification/2026-10/2026-10-29T023522Z-declared-product-references-and-prepared-content-target-admission-qualification-37c1b3f6eeca448f928a6961f1c7ab6b.md)
records independent consumers, review corrections, actual verification and limits.

### Runtime Base Implementation Order

After the declared-product prerequisite, establish these owners in dependency order:

- `nara_project` admits a profile-resolved Runtime configuration without authoring paths,
  source identity, schema-bundle paths or import settings. Reuse its existing section validation,
  lowering and product invariants; do not deserialize an effective value and bypass admission.
  Projection refuses a remaining source-relative `input.action_map` until a concrete bounded
  input resource has its own Runtime representation; explicit profile clearing remains valid.
  Do not silently omit a requested input map.
- Composition consumes the same admitted Runtime settings for source and catalog entry. Keep
  source manifest/root lineage in the source candidate. Do not manufacture a `ProjectManifest`
  or source lineage for processed delivery.
- Bind the immutable base to a trusted expected catalog digest, target and this executable's
  selected recipe/Schema before constructing the Runtime. A catalog cannot authorize itself by
  supplying both its actual and expected digest. Source agreement and Runtime-base agreement
  retain distinct proofs.
- Prepare startup and later Scene requests from that exact admitted base and original plan.
  Select the Scene representation through the Scene owner, preserve complete product identities,
  and validate decoded domain products against their catalog identity. Reuse the existing Scene
  transaction and physical cleanup owners. No authoring source, metadata or cache fallback is
  permitted after selecting the processed base.
- Bind complete native resource products in the receiving `AssetServer` after concrete product
  admission, without constructing a source database. Its Product index owns runtime identity;
  source paths are primary aliases. Runtime native reference resolution looks up an existing
  Product/type binding and never allocates an unknown selection. Actual membership, codec and
  resource values remain the content owner's proof. Qualify primary/auxiliary native codecs,
  reverse export and missing/type/stale binding refusal with the source database dropped.

This ordering records current design constraints. The Scene representation and weak Product-binding
unit is qualified below. The catalog and complete runtime-input/resource delivery still need owner
implementation and qualification; these bullets do not establish a complete delivery path.

The configuration unit `a5563ca` replaces `RuntimePlan`'s complete source-project settings with
the admitted Runtime projection under [ADR 0035](../architecture/adr/0035-project-manifest-and-runtime-settings-authority.md).
The existing source candidate still owns real source lineage. It also repairs the observed
parse-only diagnostic-capacity setting through the configured Diagnostics definition `552fbb0`.
Derived time values use exact integer seconds/nanoseconds rather than reverse-converting the
source's floating-point seconds. Initial clocks precede plugin build/finish and participate in
resolved identity; preset invariants follow the complete profile overlay. Its
[qualification](../knowledge/engineering/verification/2026-10/2026-10-29T041356Z-admitted-exact-runtime-configuration-and-owner-initialization-qualification-106a7693310949e980e8cda1e52c5548.md)
records exact round trips, source-input refusal, actual startup values, Reference Game consumers
and all three selected Windows/DX12 critical trajectories. No catalog or source-free startup is
implied by configuration alone. Continue Scene cook/admission before catalog wiring.

The Scene unit `89d9bc8` implements a distinct bounded `runtime_scene` format, deterministic logical
projection IDs, explicit effective values, canonical Product leaves and exact definition witness.
It reuses production source compilation and Reflect reference interpretation, then the same private
native/data preparation and publication path. Complete Product indices replace primary-only Runtime
indices; concrete owners can stage new bindings and actual values through the existing transaction
without source metadata or live World mutation. The
[qualification](../knowledge/engineering/verification/2026-10/2026-10-29T060803Z-processed-runtime-scene-and-complete-product-binding-qualification-cd202f5ff4514802a3c5f57a32b43b7a.md)
records actual source removal, native/data values, managed/exact definitions, reference/provenance
round trips, last-good and native-claim retry, and new-resource failure/discard/publication. Final
Asset134, domain1334, independent21, root70, Reference Game57 and all three selected native DX12
cases pass, with explicit skips/platform limits. This closes the Scene/binding prerequisite;
same-base catalog/startup/later acquisition, actual Runtime codecs for other resources and build
selection remain open. Proposed ADR 0088 is not accepted wholesale by this result.

The local-base unit `a3b235f` implements a strict version-one catalog, independent Host expectations,
verified immutable member capture and typed acquisition through the original executable plan.
`RuntimeConfigurationPlan` removes source lineage from the shared composition algorithm while
the source wrapper keeps its real lineage. The current-Scene owner accepts actual processed
candidates through a private closed input selection. Compound codecs prove complete enclosing
source/product identity; actual Scene references must match present Products and Required edges.
Its [qualification](../knowledge/engineering/verification/2026-10/2026-10-29T072753Z-immutable-local-runtime-base-and-shared-processed-scene-lifecycle-qualification-140b1b89e14c4130a72ac1c6d8f3de89.md)
records final owner45, independent48, domains918, Reference Game57, all3 native DX12 critical
cases, minimal features and correctness Clippy. The earlier broad root1084 and public-boundary
corrections retain their exact scope. This qualifies local capture/acquisition and direct managed
Scene use, not yet a complete first-party packaged Player.

Keep the remaining step-3 ownership work explicit rather than disguising processed content as a
source snapshot:

- Add a closed source/processed selection to the first-party Runtime Host and request context;
  startup and later requests must capture the original base, plan and native cleanup custody.
  Include start inputs, prepared request views, later travel and game-facing typed CPU reads in
  that selection. Select concrete Runtime codecs/publication facets through the original compiled
  recipe/plan independently of source importers; complete Product lookup must not require fake
  source records or paths after startup.
- Add concrete Runtime-native resource codecs and residency owners. CPU decode, catalog membership
  and weak Product binding do not establish installed `Assets<T>` values or physical cleanup.
  The Image ownership replacement is qualified below: optional explicit Product, actual color/descriptor,
  immutable pixel hash, optional real import provenance, neutral prepared-content identity and
  a compact bounded `runtime_image` codec replace the mandatory-source data model. Include
  `PreparedImageResource`, `ImageContentSnapshot`, import/Editor guards and actual escaped native
  pixel custody in the same qualification. A Runtime member digest is not an import-artifact digest.
  Anonymous procedural content remains directly constructible and preparable without source UUIDs
  or PNG import. Managed capture requires a present matching Product and Runtime encoding refuses
  anonymous values. TileSet Runtime dependencies likewise name complete Image Products,
  while receiving handles and source-path aliases remain their respective owners' concerns.
  Native publication and escaped pixel/render captures must retain their original decoded-output
  reservation and base after the acquisition wrapper retires. Reusing a source import lease or
  copying an unretained CPU value does not prove that lifetime; qualify cancellation, overlapping
  generations and the last shared pixel owner through the concrete publication transaction.
  The decoded-output successor replaces whole-format maximum retained charges with a required
  concrete codec query bounded by the original predecode promise. It retains upfront worst-case
  admission, refunds only modeled unused output after successful decode, and retains the actual
  charge through escaped owners. The linked qualification covers 32 small independent Images,
  a held 4 MiB decoded output, overlapping work/output promises and typed cost-query failures.
  It uses the existing lease's finish-decode contract; caps, publication and persistent formats
  are unchanged. Stock residency remains a separate concrete owner workflow.
- Replace TileSet's source-only admission before attaching its Runtime codec. A processed
  descriptor names its own complete Product, a complete Image Product and actual atlas/material
  values. Source adapters supply real path/provenance; processed adapters supply same-base
  catalog/codec facts. Both share actual Image snapshot, extent, atlas barrier and publication
  baseline validation through the existing Scene/native transaction. Do not fabricate an
  `AssetRecord`, `SourceHash` or source database for a processed value. At the content Host, check
  the payload's actual Image dependency against present typed Products and Required declarations;
  catalog edges alone do not prove it. Separate source-only auxiliary refusal from already bound
  Runtime Products in native decode and persisted Tilemap admission. Before live mutation, reject
  omitted/wrong-role dependencies, wrong Image family/extent and out-of-atlas Scene cells while
  preserving the current Scene/resources. A codec round trip alone does not qualify this owner.
  Product-owned resources staged with Scene publication capture concrete incarnation/content
  baselines; recheck them after native waits and before the publication tail. A resource that still
  exists but changed must reject the candidate, including same-tick ABA or a newly installed owner
  with equal data. Temporary removal/return of the unchanged Rust owner keeps its logical identity.
  Reuse that transaction's existing failure/discard/retry path instead of adding a second executor.
- Resolve the stock `AssetPlugin`'s source-change dependency on an empty `ProjectAssetDatabase`
  before claiming an entirely source-facet-free composition. Current base acquisition constructs
  no database and performs no source I/O; a default plugin's empty source table is not a content
  agreement or source metadata. Do not fabricate source records to satisfy processed loading.
- Deliver a concrete processed input map, keep target build selection independent, then qualify
  a packaged executable with source/metadata/cache and Cargo unavailable. Local eager capture
  does not freeze a universal container, streaming or residency implementation.

TileSet content admission with actual dependencies/cells is qualified below. The next concrete
unit is first-party Host selection and native residency through its existing lifecycle.
Keep startup, later Scene acquisition and game-facing typed reads in that same Host qualification;
adding a processed branch only at startup does not establish source-independent delivery.

### First-Party Content Integration Boundary

After concrete TileSet admission, extend the existing Runtime Host, Scene request/travel owner and
`content_residency` in that order. Use a private closed source/processed preparation result and
request context. Source retains its candidate, plan, loader, lineage and actual aliases; processed
retains `RuntimeContentScene`, the exact original `RuntimeContentBase`/configuration plan and
decoded acquisitions. Source paths and complete Runtime Products are distinct request selections.
Same catalog bytes do not establish the same base or executable-definition instance. Do not reopen
the directory, reconstruct a recipe, manufacture a source Check receipt or use a dummy Scene path.
Existing managed travel, physical cancellation return, native claims, discard/degraded retry and
candidate cleanup remain the lifecycle owners.

Freeze concrete `<D, C>` Runtime codec contributions into the selected executable recipe/plan
independently of source importer selection. Catalog kind/version checks that selection and cannot
select executable Rust code. Include admitted facet definition/configuration identity in the resolved
executable/content agreement; equal format names alone do not prove the same compiled selection.
Reuse the product's definition/fingerprint owners rather than persisting a Rust TypeId or function
address. A missing or conflicting facet refuses before publication without source fallback.
Shared game-facing CPU collections index complete Products and retain
actual `RuntimeContentAsset<P>` owners; real source records supply aliases only on the source path.
Prepared request views borrow the selected preparation, and successful Scene publication alone
replaces the live CPU collection. Compound products must not silently select their enclosing primary.
Within one prepared generation, products sharing one member and selected typed family/codec can
share one admitted decoded owner; retain each complete Product mapping and its membership proof.
Do not decode an entire compound member once per selected auxiliary product by accident. Explicit
independent acquisitions retain their existing separate charges, and collection/index metadata
remains budgeted by its concrete preparation owner.

Publish actual Images and TileSets to the existing top-level stores read by stock extraction and
Image preparation. Extend existing residency receipts and Image ownership to complete Product keys;
keep `CatalogChange` in the source branch. Processed inputs use staged Product bindings, actual same-base
Image decode and the shared TileSet owner with no database. Revalidate resource/slot/state/claim and
binding baselines after native waits. The existing exclusive tail publishes Scene/bindings, Images,
TileSets, CPU collection and current/residency receipts together. For startup, establish required
native bindings and resources in the unpublished candidate World before materializing its processed
Scene; candidate retirement retains failed-start cleanup. Make the Asset source-change facet optional
before claiming a composition with no source database. An empty default source table proves neither
content agreement nor source-free facet selection.

The [Image qualification](../knowledge/engineering/verification/2026-10/2026-10-29T090832Z-anonymous-and-processed-image-content-with-retained-native-publication-qualification-3e529af056d34a5caa2c12924c0c1014.md)
records anonymous content, strict Runtime format, complete managed Product admission, original
escaped pixel custody and source-owned refusal. Actual domains317, root1087, independent9,
WGPU145, selected native Editor5 and Reference Game61 pass, with exact skipped/platform scope in
the evidence. It also records the reviewed mandatory-Product correction and source-admission
counterexamples. This closes the Image owner prerequisite, not stock residency or the packaged
Player. Its recorded whole-format retained-charge limitation is replaced by the
[actual-output qualification](../knowledge/engineering/verification/2026-10/2026-10-29T094316Z-actual-runtime-decoded-output-charging-and-shared-pixel-retention-qualification-14f023431b4c462dbeb1051f1739662e.md).
That successor passes Asset/Image271, Host30 and independent64 tests, validates both complete
compound data and actual native Image captures, and preserves predecode/last-owner bounds without
raising limits. It does not qualify first-party residency, a packaged Player or later render work.

The [TileSet/resource-baseline qualification](../knowledge/engineering/verification/2026-10/2026-10-29T114236Z-processed-tileset-ownership-and-exact-scene-resource-baselines-qualification-e5b31550eacc4fd1a3d4b6168fb4df52.md)
at `7247273` closes the concrete TileSet prerequisite. It validates actual Required Image references,
source-independent primary/auxiliary descriptors, shared native publication/retirement, exact cell
membership and retained original Image output. Scene resource overlays retain concrete content
baselines through native waits; Reference Game and the two-level consumer use checked owner
generations. The record contains actual owner/root/independent/GPU/native/game results and limits.
Proceed with the first-party integration boundary above; this owner qualification does not install
processed Host startup/travel, game-facing collections or stock residency automatically.

## Owning Decisions

[ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md) already admits
typed operation facets below root orchestration. Its source-domain refinement names the new
executing contract independently of Runtime and cook. Selecting the source facet does not select
Runtime decode or cook. [ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
continues to own explicit source-carrying delivery.

Reconcile the actually consumed identity, local catalog and build selection contracts before
steps 2-4. Proposed ADRs 0083/0086/0087/0088 are design evidence, not wholesale accepted authority.
Adopt narrowly scoped decisions only after their detailed contract and consumer are reviewable.

## Reference Constraints

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` separates source readers, processing and
the processed output loader. Its glTF index-based labels do not prove rename/reorder continuity.
Pinned Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` exports imported resources plus Runtime
remap metadata; that metadata is distinct from authoring import settings. Its import IDs and
name/path fallbacks do not establish automatic continuity after arbitrary source edits.

Use those responsibility boundaries, not their complete watcher, processor, DCC matching or
platform-variant systems. Reference checkouts remain read-only.

## Verification

For step 1, compile the independent parser with `nara_asset` and serde only. Run its actual public
Check/Play/Scene-change tests, Editor metadata publication/cancellation tests and source Export
consumer. Cover concrete input failure propagation, domain diagnostics, reservation-before-call,
panic containment and retained generation release. Check affected feature combinations and
correctness Clippy. Run Cargo serially with the existing root target.

For subsequent delivery, require clean content builds with reproducible content digests, two
Scenes, unavailable authoring source/cache and no Cargo on the run path. Reject wrong target,
Schema, codec, duplicate identity and corrupted/missing members. Failed candidates preserve the
current admitted Scene and products. Qualify actual platform execution separately from static
checks, source-only tests and future custom-drawn Editor UI.
