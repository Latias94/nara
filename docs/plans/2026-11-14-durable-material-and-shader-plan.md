# Durable Materials And Tracked Shader Artifacts

Status: active. This is the complete R9b successor of the
[foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md#r9-material-artifacts-views-color-and-domain-render-locality),
starting from `4bd02b9`. Editor/component and Dialogue UI development remain paused. Native
qualification shares the original serial Cargo owner with Runtime work.
The declaration/Shader/native prerequisite is implemented at `899d64b`. The selected persistent
source/processed Material product workflow is implemented and qualified below. Live Editor
Material/Shader hydration and representative production costs remain active successors. The
graph-shared texture workflow is implemented and qualified below; complete R9b and the
foundation Goal are not finished.
The persistent implementation/migration is `2535b46`; independent/public and real executable
delivery consumers are `d091151`. Both are local commits on the original foundation branch.

## Product Outcome

The Palette material is authored with stable typed parameters and resource references, saved
and reopened through the existing Scene/session owner, checked, exported and run independently.
A second materially different uniform layout uses the same declaration, compiler and native
preparation path. Shader edits publish only a compatible complete generation. Invalid edits
retain the same-epoch last-good, and retained old captures use their original values, resources
and Shader artifacts even when first realized after a newer generation.

This unit replaces the startup-only static-text, one-vec4/one-texture extension. Merely adding
serde to `Material2dInstance` or keeping an old pipeline cache entry does not complete it.
No visual Shader Graph, new renderer, universal publication manager or parallel asset registry
is selected. R9a View/history, R9c measured geometry locality and broader 3D retain their scopes.

## Graph-Shared Material Texture Successor

Status: implemented at `13a0927`, with independent consumers at `e9d8755` and scoped
qualification below. Both follow persistent delivery `a67134f` on the original local branch.
In the ordinary public workload, a Feature generates a palette texture and the original stock
Material shader samples that exact program version. A second operation may branch from the
same texture; dependency order, complete group refusal, retained old outputs and original native
cleanup must remain observable. Preserve
all ordinary asset-Image material inputs and immutable exact generations.

The selected seam is the stock policy's world operation: an explicit binding associates one
captured `Material2dBinding` and declared semantic texture ID with a typed program texture. The
mapping belongs to that immutable operation, not a mutable material/current registry. Matching
uses the original generation, exact uniform bytes and complete original Image-handle map.
Reject duplicate, unmatched, foreign-candidate or invalid source bindings; do not allocate fake
Image handles or reconstruct the capture from the current World. Automatically declare the exact
sampled reads on the stock writer, and resolve those same reads when preparing the native bind
group. Missing writers, feedback and cycles keep the existing graph compiler's refusal.

Graph textures remain owned by the program and original submission. The adapter may borrow/clone
their admitted views while building the existing stock bind group, but may not put them into
the Image upload/residency cache. Only unreplaced Image inputs contribute upload demand. Native
allocation/custody accounting must include actual bind groups, uniforms and view storage without
charging graph textures twice. Every operation's capture/access preflight completes before native
work. Late native failure retains the existing original prefix; no second Queue, renderer or
submission owner is introduced.

Scope remains the existing whole single-sample, single-mip 2D baseline float/filterable texture
contract. Extent may differ from the world/output. Transfer follows the declared native format;
alpha interpretation remains explicit and is never silently converted. The material Shader owns
its sampling math and straight-alpha output, and the existing stock sampler remains selected by
the Sprite capture. No automatic framebuffer fetch, implicit mip generation or aliasing is
selected. Imported native outputs retain their original Host/epoch checks.

Pinned comparison: Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`,
`crates/bevy_sprite_render/src/mesh2d/material.rs`,
`crates/bevy_render/macros/src/as_bind_group.rs`, and
`examples/shader/compute_shader_game_of_life.rs` separate texture binding from explicit producer
ordering. That revision's RenderGraph is an ECS schedule label, not its historical texture-slot
API. Godot `7a37606a433c0e7735525037cdf774f043146a21`,
`servers/rendering/renderer_rd/renderer_canvas_render_rd.cpp` and
`servers/rendering/renderer_viewport.cpp` separately own backbuffer snapshot timing and viewport
draw order. These are source constraints, not evidence that Nara already implements the selected
version/access or lifetime contract.

Acceptance requires independent public CPU refusal controls plus actual WGPU readback for a
generated palette, a changed successor and retained original output. Include an imported prior
output, nondefault source extent, sampler/color interpretation, missing writer, incompatible
format, duplicated/unmatched mapping, graph/Host mismatch and original physical cleanup. Run
affected stock/material native controls serially. Runtime source dependency updating remains the
next owning comparison; live Editor hydration and full R9b stay open.

Public CPU admission uses `WgpuRenderHost::validate_input` through the same original policy
builder. It checks the graph, final target and Feature ownership without acquiring a device,
reserving a candidate or publishing bindings/history. The original policy callbacks execute and
may reserve CPU installation/history tables; arbitrary callback effects are not rolled back.
Native import validity, Adapter capabilities and native Feature preflight remain separate
preparation/submission checks. Validation does not return a reusable prepared value. Error
atomicity belongs to the stock world builder: both ordinary and graph-texture world calls restore
only their own appended unwritten resources, modeled bytes, capture metadata and draw facts on
refusal. Verify both unmatched-selection and later operation-admission refusal followed by a
corrected retry on the same builder/key at the exact sufficient resource/metadata limits.

The independent source fixture must consume each original sealed CPU batch through
`take_captured_policy_batch` in the selected Publish system before driving its successor.
Promote that existing finite batch entry from hidden exports to the ordinary public interface,
migrate the first-party Editor Game View import and retain its original body/authority. A pure
CPU collector does not install native-window presentation or publish into its separate Runtime
exchange. Observing the same unconsumed input repeatedly is not evidence of successive source
frames. Taking a batch carries no GPU success facts; retained replayable inputs execute later
through the original image Host.

### Graph Input Qualification

Windows 11 / Rust 1.95.0 qualification reuses the original target directory, one Cargo job and
explicit native selection. The final owning/material/graph/policy/Runtime selection passes
275/275, with 338 outside the selection, run `5bc40439-8964-4a7f-9e42-ed7ea681464b`
(37.143 seconds after an 80-second release build). This includes exact sampled-access native
preflight, ordinary asset defaults, prior cache/failure controls, the new history-GC control and
the imported Material source's original storage lifetime. The production Host identifies the
NVIDIA RTX 4090 / Vulkan adapter. The targeted lifetime control observes
`complete_before_drop=false`, `pending_after_drop=true`, `source_alive_after_drop=true`; its
weak original storage releases after the real callback. This proves source custody in that
submitted trajectory, not GPU hardware occupancy, scanout or a numerical latency budget.

Independent review first finds inaccessible unwritten resources after a rejected mapping, then
default-feature compilation and expired history-table accumulation, incomplete mapping metadata,
weak color sensitivity and masked aggregate-budget lifetime assertions. The owner corrections
are local error rollback, original Host maintenance and pre-admitted fixed-capacity vector
indexing with exact-identity collision checks and source-read deduplication. The final bounded
reviews find no remaining P1/P2 in those contracts. A separate exact source-storage observation
replaces the aggregate-byte assertion as lifetime evidence.

The independent fixture's first metadata check finds an oversized plugin tuple. Its initial
behavioral run `7e1eff0b-f111-4984-b90c-cc152a4f0d57` passes only the oracle sensitivity control
and fails eight fixtures because the unconsumed original pending frame is observed repeatedly.
Assertions before Runtime close trigger the existing fail-stop Drop path. The corrected fixture
retires before inspecting its snapshots. The first correction incorrectly targets the separate
desktop Runtime exchange, which the pure CPU fixture has not installed; run
`62249869-a1be-4cdd-b22f-231051ed8e83` passes eight original controls plus oracle sensitivity
and rejects twelve new fixtures before GPU work. The final correction takes the original sealed
batch in Publish; neither source frame indices nor native success are manufactured. The corrected CPU cohort
passes 9/9, four ignored native cases outside that selection, run
`6e107baf-5f85-4ef4-b8ec-e25711d96e1c` (1.019 seconds).

The final independent source/stock/material selection passes 21/21 with no skips, run
`9dc4cc7d-0e17-404f-b5ce-af704c1f6230` (10.879 seconds). Actual Vulkan readback covers
linear/sRGB source formats, nearest/linear sampling, generated A/B, Grade branching, imported
prior output, delayed old replay, original Image defaults and physical retirement. The source
release case observes an unfinished original callback after external leases are dropped.
RGB-channel loss, wrong sampler and encoded-sRGB-as-linear models differ beyond the fixed
three-byte tolerance before GPU execution. An intermediate 20/21 run
`6c28d1b6-f4bf-4e23-a263-f614eee2d905` identifies a fixture's incorrect expectation that a
rejected foreign import has no retirement prefix. The correction waits for the original failed
attempt's prefix while preserving zero publication/native-work assertions. DX12 independently
passes the four Material native trajectories in `069b1603-8d48-478c-8283-bd09ae3e5108`
(6.213 seconds, nine CPU cases outside the ignored-only selection).

The final owning selection includes the migrated public CPU batch entry, original frame/plugin
controls and all seven public UI sidecar integration cases: 324/324 pass, 296 outside this
selection, run `96cd0d92-dfb1-4dcf-b8f6-112f088a35b2` (42.327 seconds after a 111-second
release build). It repeats the exact source-storage pending/callback observation successfully.
The workspace check first detects four remaining hidden-path test calls; they are migrated to
the public batch entry without aliases or changed assertions. The original desktop Runtime
exchange stays unchanged.

Release library checks pass with no stock features, stock-sprite only and stock-ui only. The
final workspace release all-target check passes after the hidden test-call migration. The
owning all-target/all-feature correctness Clippy passes with existing warnings, and independent
workspace all-target/all-feature Clippy passes with `-D warnings`. Root and independent format
checks, affected whitespace, manifest parsing and added document-link/heading checks pass.
Ordinary CI already discovers the new CPU consumer cases through its existing workspace loop;
the native cases remain explicitly selected local qualification, not a remote CI result.
All three scoped review/regression agents from this continuation are closed, all owned Cargo
sequences are terminal, and paused GPUI/Dialogue changes are excluded from the local commits.

These results do not qualify automatic source updates, live Editor Material hydration, same-Host
device reconstruction, physical loss, foreign-native execution, a numeric performance budget or
the full R9b/Foundation Goal. No remote publication is part of this unit.

### Runtime Source Update Ownership Findings

At `a67134f`, actual source-backed runtime reload already has an explicit complete path:
request the same declared Scene through `ProjectScenes`, inspect its prepared candidate and
commit through `commit_prepared_project_scene` or managed native travel. Invalid Material WGSL
rejects preparation while the incumbent Scene/products remain usable. This replaces the selected
Scene instance and uses the caller's original overlay for preserved gameplay data; it is not
an in-place asset-only or automatic event-driven update.

The missing automatic bridge has three concrete obligations. `project_content/domains.rs`
records raw dependency reads in content receipts but does not retain failed-attempt recovery
paths or live source intent. Changing `a.wgsl` to missing `b.wgsl`, then creating `b.wgsl`,
must recover. The original proposal to reuse `AssetDependencyGraph` Published/Retry sets is
superseded by the concrete source-route selection below: ordinary Scene roots need no asset UUID,
and the per-asset request resolver can invalidate Scene-owned publication baselines.
Second, source intent must be accepted by the original Scene/residency owner. Registering a
parallel Material reloader would advance `AssetStates` independently and invalidate its exact
slot/state baseline. `SourceAssetBuildPlugin` publishes `BuiltSourceAsset<Product>` and does not
update the actual Scene-owned `Assets<Material2dAsset>` or `ProjectDomainAssets`. Third, events
during Preparing/Ready need bounded successor intent and original cancellation/physical-return
handling. A frozen Ready candidate must not be silently reinterpreted from newer disk contents.

Qualify that later bridge with actual semantic source-change notifications, compatible/invalid/
repaired WGSL, missing-file recovery, default Image content/selection edits, removed dependency
events, events during preparation, explicit terminal acknowledgement and Stop. Preserve the
original two-distinct-generation budget, last-good capture, managed native retirement and
processed-base isolation. A test manually requesting Scene reload after every edit only proves
the explicit workflow; it cannot qualify automatic dependency-directed updates. No watcher,
second current map or additional scheduler follows from this source study alone.

## Scene-Owned Source Updates

Status: implemented at `c03b940`, with independent consumers at `55b0ae4`.
Canonical semantic source notifications automatically prepare a
successor of the current hosted source Scene through its original request/residency owner.
Game code still inspects Ready data, supplies its gameplay overlay, explicitly commits through
the ordinary or managed-native API, and acknowledges the exact terminal request. Do not replace
the current Scene, discard game state or infer native success from a file notification.

`ProjectScenes::notify_source_change` accepts existing `AssetSourceChange` data without I/O,
asset-state mutation or another event queue. Match raw principal and metadata paths against the
current successful closure and latest executed automatic failure's bounded attempted-read cut. A matching
notice sets one pending intent; `request_source_rescan` records incomplete observation as a full
rebuild intent. Starting an automatic request consumes that intent, while newer notices remain
pending until the original request physically returns and its terminal result is acknowledged.
An automatic Preparing/Ready/Managed request conservatively retains even currently unknown paths, closing
the window before its newly discovered dependencies become observable. Its frozen candidate is
never reinterpreted from disk. Publish replaces successful paths and clears obsolete retry paths;
valid failed preparation preserves last-good and records requested missing inputs before open.
Cancellation, stale completion and Stop cannot publish a late retry cut or candidate. Source ingress
also checks the original Runtime generation and current Scene domain/instance identity, rather than
retargeting old successful interest after an unrelated current-instance replacement.
Pure status observation still exposes a retained automatic ID after that basis becomes stale, so
physical return and exact terminal acknowledgement remain possible. Publishing an explicit Scene
switch preserves accepted pending intent; its successor targets the newly published current Scene.
Failure of an explicit travel request keeps its own diagnostics without subscribing the current
Scene to a different unpublished target's failed inputs.

The current successful closure receipt retains its original source-fact allowance. Failed-read
observations use the original content budget, finite paths/bytes and no decoded generation claim.
The existing two-distinct-generation bound, task return custody, one request/result slot, exact
current-instance predicates and native travel remain unchanged. Processed Hosts refuse source
intent without reading files or changing their immutable base. Admission failures are observable;
they must not silently consume pending intent or manufacture an admitted request ID.

Scene source routing is selected by composition. Do not broadcast these same publications into
the generic asset reload resolver, which has independent per-asset UUID/generation/type/state
authority. Unrelated generic asset compositions retain their existing graph and consumers. A
mixed publication route requires its own real-target arbitration; a Scene match is not an
exclusive event-consumption claim. The existing `AssetWatcher`, bounded `AssetWatchEventQueue`
and `AssetWatchTranslator` can feed this concrete ingress. The stock `AssetWatchPlugin` drain is
hardwired to the generic route and is not silently redirected. Queue overflow, broad scope or
translation failure requires the Scene owner's full rescan entry. This unit first qualifies
actual semantic notifications, not a new native watcher implementation or its physical close.

Acceptance includes compatible/invalid/repaired WGSL, a missing new dependency followed by its
creation, Image content/default-reference edits, removed dependency notifications, preparation
and Ready interleavings, explicit cancellation/acknowledgement, unaffected last-good/native input
and Stop. The independent consumer must trigger updates with semantic notices rather than call
`request` after every edit. Existing explicit source/processed and native travel tests remain
controls. No Editor UI work or automatic global Input/settings replacement is selected.

Successful optional fallback availability remains a separate source-interest decision. A generic
importer may handle a raw dependency error and successfully return; its complete attempt observations
then include the absent path, while its successful content closure contains only actual captured
members. Ordinary startup and later successful publication do not retain that absent path as live
interest in this unit. The Material importer propagates all required Shader/Image failures and has
no such fallback branch. Preserve that compatibility, use explicit rescan for optional availability,
and qualify an original closure/read-interest extension before claiming general fallback watching.
Catalogue membership and unresolved Product availability have the same explicit rescan boundary:
an unknown new metadata file can introduce a missing UUID or conflict with an existing identity.
Neither successful file receipts nor an executed file-path cut subscribes to directory absence.
The later physical watcher composition must classify these structural observations as full rescans;
the finite raw-path recovery in this unit does not establish a general catalogue watcher.

### Source Update Qualification

The first focused Windows/Rust 1.95.0 release run,
`c3687b31-907b-42d9-bf01-ef7811cb057e`, passes 31/61 after a 16-minute build. The source
observation controls pass, including complete failed reads, exact/one-short accounting, allocation
refusal and frozen Input reuse. The remaining failures expose two integration corrections. The
minimal Scene fixture omitted the Material provider selected by its current default recipe; it
now installs the real plugin instead of relaxing schema agreement. Actual Host admission has
already allocated its Runtime generation, but its exact Scene owner still resides in the original
`StartupSceneActivationInput` until Startup consumption. Source interest now borrows that pending
instance receipt during admission; it neither publishes early nor lazily binds to a later Scene.
The corrected runtime/public-consumer results follow.

Independent review also corrects source-interest custody: requested raw paths are observed before
dependency/work admission, canonical strings reserve exact capacity and charge their actual retained
capacity, and list-allocation refusal restores the previous retained prefix and budget. A sticky
observation refusal rejects the complete attempt even if an importer handles the reader error.
Ordinary handled I/O errors retain their existing semantics when the attempted path was completely
observed. The Scene owner preserves pending rescan across explicit root changes and exposes stale
automatic IDs for exact acknowledgement. These corrections use the original budget, task, startup
activation and request owners. The final bounded source/startup reviews find no remaining P1/P2.

The independently locked public consumer passes 2/2 in
`10ce4b29-8252-4a04-9f33-f7b161b9c997` (0.231 seconds after a 4-minute-3-second release build).
Its actual game-owned HeadlessRun completes eleven automatic requests: eight explicit commits,
two content refusals and one cancellation. It observes the real Scene receipts/content stamps,
AssetStates revisions and versions, Material binding, decoded Image pixels/hashes/allocations and
gameplay overlay after each transition. The source edits include compatible/invalid/repaired WGSL,
missing `b.wgsl` followed by creation, obsolete Shader/Image notices, default Image selection/content
and Ready/pending/terminal interleavings. The startup Scene has no UUID sidecar and remains byte
identical. The consumer never calls manual `ProjectScenes::request` to trigger these updates.
The corrected owning/affected selection passes 145/145 in
`ba5bb798-cfd1-4936-a313-c57f92bbd684` (24.100 seconds after an 8-minute release rebuild;
1648 tests outside the selection). It includes all source-content controls, eleven source-update
lifecycle tests, original explicit source/Product/processed requests, residency, processed Input,
Startup/current-Scene and actual Host Stop while an automatic preparation remains physically held.
The fixture's plugin list uses the existing nested group interface after reaching its eight-element
tuple bound; the production plugin API is unchanged.

The complete independently locked Material package passes 10/10 in
`da08a271-6156-4ddb-93bf-1abdd81fdd77` (1.194 seconds after a 56.17-second build), including the
new consumer plus original source editing/Undo/Save/Reopen/Check, retained captures, processed
source-free residency, codec/target refusals and explicit carrier selection.

The independently locked managed-domain controls pass 16/16 in
`ebdb3fa5-15cf-4c77-92cb-130a3ad95eb6` (0.902 seconds after a 10-minute-16-second release
build for their feature combination). Both independent native-effect plugins retain their
original retirement, one-shot configuration, retry, lifecycle-hook, failure and shutdown checks.
The recovery trajectory additionally observes source interest before publication and after a
reconciliation failure: the unpublished target's notice is unrelated; once that exact target
publishes, its notice queues pending intent and the old Scene notice is unrelated. Pending intent
survives successful native retry while the original unacknowledged terminal slot remains occupied.
This is real managed travel through the public Host/domain protocol, not physical GPU or OS-window
execution.

The adjacent Input consumer migration at `a51ad6e` repairs assumptions exposed by the final
independent all-target check. Its exhaustive origin match omitted the existing `Restored` variant,
and its old exact-step assertion contradicted
the current fixed Prepare input contract. The consumer now explicitly refuses restored input in
this non-restoration workflow, observes outcomes in fixed Finalize and ordinary Update with
frame-identity deduplication, and requires exactly one input/command/gameplay step without Resume
replay. Original source/context/mapping/capacity assertions remain. No production Input behavior
changes. Its complete bounded Input selection passes 3/3 in
`4af6fb23-401f-4de1-a317-e873ba42d9d4` (0.178 seconds after an 8.52-second build), following
the initial outdated-contract failure in `62a19e60-6eba-4773-852b-0d03032a33a5`.

Final Windows 11/Rust 1.95.0 locked/offline closure passes: workspace release all-target check,
owning all-feature/all-target correctness Clippy, independently locked Scene/Material all-feature/
all-target correctness Clippy, root and independent workspace format checks, added local document
targets/fragments and whitespace checks. Existing unrelated warnings remain. All Cargo work is
serial with one build job and the original root target.
Physical watcher event delivery/routing/close, structural catalogue availability, successful
optional fallback availability, Editor hydration, GPU/OS-window execution of this notification
workflow, foreign-native execution and numerical performance budgets remain unqualified. The
complete R9b/R12/Foundation Goal stays active.

## Baseline Source Findings

- `nara_material` stores static Shader text in `Material2dDefinition`; `Material2dInstance`
  explicitly excludes durable Scene codecs and stores process-local Image handles.
- `nara_sprite_render` owns extraction and `PreparedMaterial2d`. Its immutable captures retain
  exact Image snapshots. Both Sprite and Tilemap already share this path.
- `nara_render_wgpu::quad` fixes the material group to a 16-byte uniform and one D2 texture.
  `StockQuadResources` caches Ready and Rejected pipelines by the full static definition.
  Value/Image-content updates do not enter that key, but no current material publication or
  dependency-directed Shader update exists.
- The original Host, native preparation, allocation preflight and submission prefix already
  retain resources independently from cache eviction. Reuse them. The same-producer successful
  frame watermark rejects old frames; delayed A must use an admissible independent producer
  in the same Host/epoch rather than bypassing frame admission.
- Scene schema/codecs, capability-bound source import, Check/source Export, cooked Runtime
  codecs and physical content residency already exist. Imported products' typed dependencies
  still need a complete closure: copying a raw dependency file alone does not import/publish
  the Image product referenced only inside a Material.

## Selected Boundaries

One material declaration owns stable semantic field IDs/version, uniform field types and
texture references. Its binding projection owns actual uniform size/offsets and group bindings;
its Shader artifact owns actual compiled source/interface and tracked source observations.
Rust parameters produce this declaration and encoder from one field declaration. The same
validated declaration constructor is the future Shader-authored admission seam: Rust `TypeId`
is neither persistent identity nor a substitute for layout compatibility.

Use actual uniform-layout/encoding and Shader compiler libraries, with the repository's
existing pinned WGPU/Naga source. Do not implement WGSL parsing, a name resolver or guessed
alignment rules in a script. WGSL qualifies first; a future WESL frontend must report its real
resolved and failed dependency closure through the same artifact boundary.

The pinned Bevy reference is `03ed5a27c92715c8fa2188f31f04e454ec39942f`, especially
`crates/bevy_render/macros/src/as_bind_group.rs`, `crates/bevy_encase_derive/src/lib.rs` and
`crates/bevy_shader/src/shader_cache.rs`. Reuse its separation of field declaration, real
uniform encoder and actual compiler dependencies. Do not copy its invalidation policy as proof
of Nara's retained-capture/current-publication contract. Encase 0.12.1's upstream source is
`561121cd17298355de74c07a7206eb8329fd8347`; `encase_derive_impl::derive_shader_type` accepts the
generated runtime support path, while merely reexporting its ordinary derive would still require
a consumer dependency named `encase`. Glam 0.33.12 supplies the matching `encase-012` feature.
Naga remains at WGPU's `cddb2f5d80dc0009b2ef6767635140cacf90a007`.

The small `nara_material_derive` crate is justified by the two actual parameter layouts, not
another runtime layer: one field traversal supplies stable IDs, typed values and Encase's
generated layout/encoder. Arrays are not WGSL vectors. Runtime Shader/parameter code remains
inside `nara_material` behind an explicit feature so sampler-only users need no Shader compiler.
The existing Reflect type is an optional generated tooling projection, not a separately
maintained material field table. General reflection/App dependency restructuring is not part
of this material workflow.

Semantic fields, binding ABI and Shader interface have distinct fingerprints, joined in one
immutable admitted generation. A prepared edit retains its expected current generation and
cannot replace a newer publication. Invalid source/interface leaves current last-good intact.
Captured instances retain the exact complete generation, encoded values and referenced resource
versions. A late old capture can realize its own pipeline but cannot publish itself as current.
Value-only and ordinary Image-content-only edits change bindings/resources without multiplying
pipeline variants. Unrelated material declarations remain usable.

Persistent Scene values contain semantic parameters and durable resource references, never GPU
handles, process-local Image handles or serialized Rust type identity. Reuse component schema,
reference traversal and codec ownership. Resource import/Runtime codecs and source/processed
residency use existing project content owners; current replacement must not resolve candidate
values against the incumbent's old domain collection.

Native layout, Shader and uniform resource preparation stays in the original stock quad
adapter. Extend preflight, exact layout/pipeline keys and submission-held resources together.
Cross-Feature texture inputs must join existing graph resource versions and declared reads;
an opaque bind group alone does not satisfy that contract. No second Queue, submit or epoch
owner is introduced.

## Implementation Order

1. Build the concrete two-layout typed declaration, parameter encoder and compiler interface
   with stable semantic identity, bounded input and inspectable validation errors.
2. Replace static material captures and fixed group construction through Sprite/Tilemap,
   original native preflight/cache and actual same-epoch GPU acceptance.
3. Add complete persistent schema/reference/source/Runtime content plumbing and extend the
   independent Palette consumer through Save/Reopen/Check/Export/run.
4. Qualify compatible B, rejected C and delayed A; exact dependency-directed updates; value/
   texture controls; actual pixel and original retirement evidence. Remove superseded APIs.
5. Reconcile ADR0033/0094 and relevant guides/ledger, review independently and commit precise
   changes. Full R9b remains open until its product and interleaving acceptance pass.

## Verification

- Owning CPU tests cover unique stable IDs, type/value/ABI disagreement, finite value and
  bounded input rejection, dynamic/typed encoding equality and current-generation revalidation.
- Two actual layouts include mixed scalar/vector alignment and a materially different resource
  set, rather than another name for the original fixed vec4 contract.
- Shader validation uses actual Naga module/layout results. Malformed source, missing/wrong
  resources and incompatible interfaces retain the compatible current generation.
- An independently locked consumer authors, edits, saves, reopens and checks the Palette
  material, then runs exported/source-free content with a texture referenced only by Material.
  Missing/wrong Product and format/target inputs reject before gameplay publication.
- One valid native Host/device epoch retains A, publishes compatible B, refuses C, then first
  realizes A through an independently admitted producer. Readback proves A/B pixels; variant
  counters separate Shader changes from value/texture-content controls. Existing submitted
  resources remain held to their original completion prefix.
- Record actual cold compile/edit-to-ready/variant observations without claiming a performance
  win from uncontrolled build time or test duration.
- Serial locked/offline Cargo reuses root `target`, jobs one and explicit Rust 1.95.0. Match
  tests to changed owners, preserve paused UI work, and record failures and qualification limits.

## Implementation And Qualification

### Persistent Product Successor

The November 14 successor joins the complete source/processed product path. `Material2dAsset`
is an imported product containing an exact validated generation, default semantic values and
durable texture references. The authored `Material2d` component contains its typed asset handle,
declaration identity/version, complete semantic values and explicit texture overrides. Extraction
captures one existing immutable `Material2dBinding`; old prepared frames keep that capture.
Code-first exact bindings remain valid. The source product is not another mutable current slot:
original `Assets<Material2dAsset>`/`AssetStates` and Scene residency publish current products.

The carrier schema/codec preserves durable references and generates material-specific field
projections from the admitted declaration. Candidate content validates complete values against
its own loaded products before startup or Scene replacement; target-World incumbent domain
collections cannot provide that validation. The generic component decoder gains no arbitrary
World or product lookup interface.

`SourceAssetImporter::visit_source_dependencies` reports borrowed actual references, expected
source kind and demand. The existing loader discovers and imports the finite typed closure,
including cross-domain revisits and Scene edges, with its original catalogue, budgets and
publication. Auxiliary raw WGSL reads remain separate source members. A material-only texture
must be genuinely imported and resident, not merely copied to Export. Runtime codecs enumerate
their complete Product edges through the existing processed-content contract.

Acceptance remains the complete independent Palette Edit/Undo/Redo/Save/Reopen/Check/source
Export and actual source-free Player workflow, plus original retained-generation native
controls. The product successor is in progress; interface edits and passing dependency tests
alone do not complete R9b.

The successor implementation adds the concrete versioned source and `runtime_material2d`
format, the single persistent carrier, candidate-local source/processed validation and the
original Scene residency publication tail. The material-only Image closure now uses the same
typed source discovery as Scene references, including same-domain cycles, cross-provider
revisits and deferred/strict policy. Actual default Image identity and publication slots are
checked before authored render extraction; wrong-Product publication and raw writes cannot
silently retarget the authored reference. Explicit code-first binding captures keep their
existing independent semantics.

Independent review identifies and corrects three ownership gaps before qualification: the
incoming temporary product buffer overlaps the grown collection during incremental append;
bare Shader/Input clones initially escaped the generation's acquisition lease; and raw public
source/Runtime collections initially exposed undecorated values before residency. Source views
now retain only original byte/generation leases. Runtime views retain the original decode lease,
their separately reserved metadata and immutable base, without a product/acquisition back-edge.
Owned Shader/Input clones preserve those owners. Canonicalized defaults and decorated views
have separate pre-allocation accounting rather than querying an opaque owner's internal cost.

The original native pipeline cache also cannot retain those source-generation owners after
operation captures retire: its 64-entry bound is larger than the source Host's distinct-generation
allowance. Ready/Rejected entries now use only format/alpha and exact Shader/ABI digests plus
native pipeline or existing diagnostic. Actual prepared/submitted draw references still retain
their current input's exact generation. This preserves native pipeline reuse without letting
acceleration state reserve obsolete source generations.

The first new material owner run executes 73 tests: 71 pass and two projection tests still
expect the former prefixed IDs or a long-ID rejection. The projection uses a namespace and full
semantic-ID digest while keeping the complete literal path; the corrected tests cover the full
128-byte allowance and parameter/texture namespaces. Subsequent integrated builds first meet
an in-flight old dependency API during the lifetime correction, then two new test fixtures that
mistakenly call private Image sharing, and a moved Scene source key in the new Runtime fixture.
The fixtures now use public Image constructors and preserve the original Scene source identity.
A metadata-only owning library/test-target check then finds one missing qualified field-ID name;
its corrected repeat passes in 7.47 seconds. These compile failures are not behavioral passes.

Twenty-five owned Windows metadata graphs resolve offline with every original package version
and source retained; no package is added by this successor. The paused GPUI consumer is excluded.
The independent Palette test check then finds a new test's guessed two-argument loader constructor;
it is corrected to the actual public one-argument interface. Owning behavior, independent
Save/Reopen/Check, actual source Export, cooked running and native cache results remain pending
until their serial commands complete. No cost or full R9b completion claim follows from this
implementation checkpoint.

The frozen owning CPU selection passes **168/168** in
`c258618b-062f-44d4-aa8d-13b783b23171` (10.900 seconds after a 10m19s release build), with
1,652 cases outside the explicit selection. It includes all material/public generation and
Sprite/Tilemap tests, the new typed source-reference closure, material residency, public Runtime
snapshot custody, source inventory and read-only Product lookup controls. The four material
residency cases and five public Runtime custody cases execute against actual stores/base/leases;
the extra Host view charge has a one-byte boundary control. This is owning CPU evidence, not
independent exported products or native GPU execution.

```text
cargo +1.95.0 nextest run --locked --offline --release -p nara -p nara_material -p nara_asset -p nara_sprite_render --no-default-features --features nara/tooling,nara/project-workflow,nara/presentation-2d,nara_material/persistent --lib --test generation_workflow -E 'package(nara_material) | package(nara_sprite_render) | test(project_content::tests::reference_demand) | test(project_host::runtime::content_residency::materials::) | test(runtime_content::materials::) | test(import::tests::) | test(server::tests::)' --test-threads=1 --no-fail-fast --no-tests fail
```

The independently locked Palette library/test-target check passes in 5.45 seconds after the
loader-fixture correction. Its first behavioral run, `ac3e2e60-1bba-4923-840d-e128f77445f2`,
passes 3/7 in 3.153 seconds after an 11m32s release build. The complete Editor trajectory
refuses its first field edit because the new carrier has no first-party native Adapter in the
authoring lane; the failure then aborts during unwinding. The three processed trajectories
reject an incorrectly constructed Schema contribution for a runtime-only inspection plugin.
The passing negative admission, source-budget escape and original exact-generation controls
do not establish the failed Editor or processed workflows.

The owning correction makes the single first-party Material carrier a known compiled provider,
an optional default Runtime2d plugin when persistent data is selected, and part of the existing
Editor preview and generated starter metadata. The Editor still does not execute arbitrary
recipe Schema plugins. Material families and source/processed codecs remain explicit recipe
selections; this does not turn material declarations into native component registrations.
The independent runtime-only inspection plugin must use a Runtime contribution.

The next independent run, `59730f76-c2e8-4fc9-af5d-3d790b0b7308`, again passes 3/7 in
1.260 seconds after a 4m00s build. The native carrier is now Available, but its parameter field
was declared as an opaque Map and the unchanged preservation-safe editor guard correctly
refuses editing it. The three processed fixtures also expose the newly introduced duplicate
between a default plugin slot and their explicit recipe contribution. Existing composition
deliberately rejects that overlap; this is not a reason to weaken duplicate/authority checks.

The correction migrates these new default-based fixtures to the actual default carrier. Custom
plugin groups can still select its ordinary explicit contribution without Runtime2d defaults.
The uncommitted persistent wire is refactored to six complete typed maps under
`parameters.<kind>.<semantic-id>`, with no compatibility reader for the earlier draft. It reuses
the existing Record/Map/Scalar/List shapes instead of adding a general union or opening opaque
values to authoring. Native decoding rejects missing/unknown kind maps, repeated IDs across
kinds, invalid numeric values and vector lengths; material admission still validates against
the selected declaration. Corrected independent execution, actual Export/cooked Player and
native cache qualification remain pending.

The first typed-map codec selection passes 10/10 in
`dc4633b5-645e-45d9-91f9-a13502df8a76` (0.534 seconds after a 55.34-second release build).
Independent review then finds that declaration-specific projections omit the required empty
maps for unused kinds. `binding_schema` now declares those as exact empty Records without Edit
capability, while `binding_fields` continues to return actual semantic fields only. The extended
F32-only/128-byte-ID case validates the complete canonical record and refuses an unexpected i32
member. Independent rereview closes that P2 and the deliberate default-recipe migration P1;
final owning execution of this projection correction remains pending. No shared shape or plugin
resolver rule is relaxed.

The next complete independent run, `24c30463-92fd-4075-a233-6cb8f34d6752`, passes 6/7 in
0.876 seconds after a 4m31s build. The full public Edit/Undo/Redo/Save/Reopen/Check/source
Runtime trajectory and all three source-free processed-content cases pass. The remaining
negative fixture still writes its wrong-type example using the replaced draft wire; it fails
the structural decoder before reaching its intended declaration mismatch assertion. Moving the
same semantic ID from the f32 map into u32 restores that counterexample without removing the
unchanged refusal and incumbent-state assertions. Final independent qualification remains pending.

The first-party carrier's authoring availability is distinct from automatic Material hydration in
the Editor Scene preview. Its current source-preview owner hydrates Images/TileSets; this unit's
actual Material import/residency proof is the independent Runtime/Export path. A complete live
material preview must join that existing source work, exact candidate publication and retained
rendering rather than add a second asset current map. Dependency-directed Editor Shader updates,
graph-shared texture inputs and representative numeric costs remain full R9b successors even
after the selected persistent/source-free delivery qualification passes.

The corrected complete independent Palette cohort passes **7/7**, zero skips, in
`8b6960d8-93d9-4426-ac25-7bbbe321fa81` (0.896 seconds after a 4m30s release build).
It executes the actual prepared field edit and inverse, Undo/Redo, exact Save bytes, Reopen
with empty history, complete Check inventory and source Runtime materialization. The processed
trajectory imports and cooks real PNG products, assembles the real Scene/Material dependency
closure, removes its source/cache and runs from the immutable base. Missing palette, wrong
kind/format/version/host and mismatched declaration/parameter controls reject before gameplay
or preserve the original incumbent. Owned Shader/Input clones retain source budgets until the
last clone drops. Actual separately compiled source Export/cooked Player and native cache
qualification remain pending; this in-process consumer does not establish them.

The actual compiled cooked Player passes **1/1**, zero skips, in
`aa341ef5-71a2-4d17-9b01-cfe0dd6346af` (300.627 seconds, including the real nested builds,
after an 8m33s outer release build). The independently generated project imports and cooks
actual PNGs, imports/canonicalizes Material defaults, compiles a real Scene and publishes the
original generated-bootstrap/captured-executable package. The final installation contains only
Player, `content.pack` and `package.json`. After relocating it and deleting the generated project,
source cache and captured cooker, an exact-executable sealed launch from an empty unrelated
working directory runs two headless frames. It checks the original Scene entity/instance,
four complete Products, exact semantic/uniform values, decoded `[24,120,220,255]` pixels,
no SourceDatabase/Importer and no inherited tool paths. Original native process cleanup
completes. This is actual Windows x64 headless delivery, not GPU scanout, desktop delivery,
foreign execution or a latency distribution. Source Export and native cache remain pending.

The first actual source Export run, `4e45dbda-a8f0-4695-aa7a-5d2a411cb53e`, fails after
1,056.792 seconds (12.21-second outer build). Both product observations and whole-directory
publication/relocation complete, and the relocated headless product exits successfully. Its
Material verification marker is absent: that new fixture installed the check in variable
`Update`, while the original one-tick starter stops after its first completed fixed tick.
The check moves to `FixedUpdate`; the full resource assertions and required marker remain.
This is a fixture scheduling correction, not a missing-source bypass or a proven engine
resource-loading failure. The repeated complete source Export remains pending.

Adding the first-party default also requires updating the six checked-in current authoring
bundles used by the games and runtime/Editor fixtures. Runtime agreement must continue refusing
a missing selected owner. Generate the Material metadata from its real trusted provider,
preserve every existing owner/current/predecessor snapshot exactly, and qualify the starter
and real game composition rather than weakening agreement or editing fingerprints by hand.

The corrected complete source Export passes **1/1**, zero skips, in
`868062fd-b075-4f56-9f0a-ef5a48026fc0` (55.200 seconds after a 10.58-second outer build).
It keeps both actual product observations, exact eleven-member source publication, directory
relocation and source deletion. The relocated sealed headless run now executes the full Material
residency assertions during its actual fixed tick and emits both required markers. Original
native process cleanup succeeds. `WGPU_BACKEND=vulkan` is selected for the desktop operation;
this does not turn CPU Image assertions into native pixel readback or establish source-free
desktop execution. The cold failure/repeat durations are different build states, not a speedup.

A temporary public-Rust consumer generates the single Material owner into the six current
bundles. Its first compile catches a mistaken equality assumption for nominal snapshots before
writing any file; the correction compares complete current/predecessor catalogs. The successful
run retains all 7/6/6/2/7/6 original owner records respectively, with a byte-decoded JSON control
confirming exactly one added owner per bundle. No historical component-schema generation is
rewritten. Starter creation's expected provider set and the reference-game provider assertion
are updated to the same actual contract; their behavior qualification remains pending.

The final expanded independent cohort passes **8/8**, zero skips, in
`1185e35a-2828-4bb9-a8d7-ec00482d1ca2` (1.008 seconds). The additional actual public App
consumer proves one default Material owner/store, one explicit owner/store in a custom Minimal
group, no owner/store after explicit disable, and rejection of duplicate default/recipe selection.
Its first compile incorrectly treats the registry snapshot as an ECS Resource; using the original
public `component_registry` accessor corrects that test without adding storage or weakening
authority. The final owning library/integration-target check passes in 19.75 seconds; final owning,
native cache and wider current-game checks remain pending.

The final expanded owning cohort passes **241/241** in
`75e72673-2dcf-45d7-8a22-e8bcdeb2c2f1` (14.521 seconds after a 12m34s release build),
with 1,652 cases outside the selection. This reexecutes the material/projection, public generation,
Sprite/Tilemap, typed source-reference closure, Material residency/acquisition and read-only
Product lookup cases after the typed-map/default-owner corrections. It also executes the complete
selected plugin-composition, data-component, starter-creation and project-Check integrations.
The final F32-only projection and generated starter's new provider contract therefore have
behavior evidence. The bundle generator/diff controls retain their separate old-metadata
preservation evidence; wider checked-in game execution and the native cache still retain their
pending commands. Counts overlap the earlier focused runs.

The affected native material cohort passes **15/15** in
`f8cf8622-7eb0-41bc-b11d-105f7febbe8e` (11.830 seconds after a 4m53s release build),
with 593 cases outside the selection. It retains all five original production Host trajectories
and material preflight/capacity/cache controls, and executes both new Ready/Rejected cache
source-retention controls. Equal native content reuses the pipeline while each actual draw
reference retains its incoming generation; after the draw holders retire, the cache no longer
holds the old source lease. Rejected cache history likewise releases those leases without uploads.
The private two-slot counter/Weak owners model source custody and do not claim an actual source
loader was used in that GPU control; actual source/acquisition custody has separate CPU evidence.

The production Host reports RTX 4090/Vulkan, NVIDIA driver 621.14, fallback disabled. Private
ReadbackGpu cases retain their original adapter selection and are not all relabeled Vulkan.
One fixture reports 635.8 microseconds initial CPU material compile, 279 microseconds compatible
edit and 21.1031 milliseconds for its first Host submission interval. These remain individual
observations, not a distribution, scanout/GPU duration or a backend performance comparison.

The first checked-in reference-game composition run,
`712c5f41-fd47-4846-a5de-f13d6275391e`, passes 3/4 after a 9m31s release build. Its strict
bundle-byte control finds exactly one missing final LF from the temporary migration generator:
all 195,695 content bytes match and the expected file appends that LF. The existing reference
`authoring_bundle --write` tool restores the required file spelling; the two other originally
newline-terminated Editor fixtures keep their original convention as well. No owner, shape or
fingerprint changes follow. The unchanged reference tests then pass **4/4**, zero skips, in
`6a04ce95-e613-4d6e-9f1e-7d43cacfdd7b` (0.237 seconds after a 5.79-second rebuild).
This qualifies the actual reference recipe/default provider, shared registry identity and
checked-in bundle agreement, not its complete gameplay or desktop suite. Current sample Check
and final lint/format qualification remain pending.

The actual checked-in Two-Level Game's normal headless project-tool Check also succeeds with
the migrated bundle: `success: true`, eight content members, after its independently locked
8m33s release build. This checks the complete declared source closure and selected native
Schema agreement through the public game entry; it is not a new full gameplay/reconstruction
or desktop qualification. The Collect Game Check remains in progress.

The actual checked-in Collect Game's normal headless project-tool Check succeeds as well:
`success: true`, six content members, after a 6m57s independently locked release build.
Together with the reference composition and Two-Level Game Check, this verifies the selected
default-owner/bundle migration against the existing project consumers. It does not replay their
whole gameplay, native Editor or continuous reconstruction suites. Final lint/format checks and
the same affected production Host cohort on DX12 remain in progress.

The final root/asset/material/Sprite all-target correctness Clippy passes in 1m47s, and the
native four-owner all-feature/all-target counterpart passes in 44.77 seconds. The independent
Palette `-D warnings` gate initially finds its new helper returning the large concrete cooker
error on the stack. Boxing only that test helper's error preserves its complete typed negative
match and avoids changing the production public error. The corrected strict Clippy passes in
4.83 seconds; the final actual independent cohort again passes **8/8** in
`cdc577e3-638e-40bc-912a-5af6cce82272` (1.524 seconds after a 28.99-second rebuild).
The actual source Export/cooked Player test targets pass correctness Clippy in 12.94 seconds.
Existing dependency/style/dead-code warnings remain outside these correctness results.

All five production Material Host trajectories also pass on **DX12: 5/5** in
`9badbda0-1909-45bd-9e01-0bb765ed2d5a` (4.674 seconds after a 2.74-second test build),
with 603 cases outside the explicit ignored selection. The Host reports RTX 4090/DX12,
driver 32.0.16.2114. One fixture observes 619.2 microseconds initial CPU material compile,
276.2 microseconds compatible edit and 29.7365 milliseconds for its first Host submission
interval. These have the same single-observation limits as Vulkan and do not establish a
backend performance winner. Final workspace/format/document checks remain pending.

### Persistent Product Qualification Closure

The final workspace release all-target check passes in 3m56s, including the normal Editor and
all compiled module targets. The no-default-feature material library check passes in 0.97
seconds; sampler-only consumption remains Shader-compiler/ECS-free. Root, module-consumer,
cargo-build-consumer and reference-game formatting checks pass after the final owned formatter
run. Vendor sources retain no new changes. All fifteen actually modified owned lockfiles retain
the exact original package/version/source sets; earlier metadata checks cover the twenty-five
affected Windows graphs. Added local document links/fragments, JSON/TOML parsing and ordinary
working/staged whitespace checks pass.

Independent reviews close the typed source transfer accounting, escaped snapshot custody,
default/plugin overlap and empty-type projection findings. The late integration review confirms
the six owner-preserving bundles, actual fixed-tick source verification and public composition
controls. All twenty-five scoped agents across this persistent successor are closed, including
the previously pending cooked-delivery implementation. No new current-asset, native process,
render submission or Editor lifecycle owner is introduced. No remote publication or main/branch
movement belongs to this work.

The selected persistent product workflow now has owning 241, final independent 8, actual source
Export 1, actual cooked Player 1, reference composition 4 and both current sample Check results.
Its native material/cache evidence is the focused 15-case cohort plus five production Host cases
on DX12; those counts overlap rather than add into one native aggregate. The original source
Export scheduling failure and bundle-newline failure remain recorded above with their actual
corrected repeats. Build durations are not production runtime performance measurements.

Continue the full rebaseline through Runtime material dependency updates and graph-declared
shared texture inputs using the existing source/render authorities. Automatic Editor preview
hydration waits for that product's selected work; Editor/component and Dialogue development
remain paused. Physical mixed-DPI needs its own available multi-monitor setup. Numerical
large-game budgets, foreign execution, remaining View/history work and full R9b/R12/foundation
completion remain open.

The first implementation replaces static definition/instance APIs with `Material2dBinding`,
exact immutable generations and two-layout parameter/Shader admission. The library's current
and failed values delegate to original `Assets<Material2dGeneration>`/`AssetStates` and captured
`AssetMutationBaseline`; the first draft's separate `Entry.current` was removed after independent
ownership review, before consumer integration. Source/processed residency must use these same
actual stores rather than keep a synchronized second current. Source dependency observations
are explicit requested/read inputs, including missing reads, not invented WGSL import results.

Actual Encase 0.12.1 derives the layout/encoder and Naga's original pinned compiler validates
the assembled stock/material interface. Pure sampler users retain a compiler-free feature path.
Both declaration digests are computed once on admission. Native pipeline identity excludes
publication sequence, semantic IDs and parameter/texture content; its exact source/ABI remains
retained by the old capture. The original stock WGSL is moved to the material artifact owner and
shared by ordinary/native paths instead of maintaining a validation-only copy.

The owning all-feature production library check passes on Windows/Rust 1.95.0. The first CPU
test build fails in a new test: storing Encase's move-only `Metadata` in a local and calling its
consuming `offset` method repeatedly. Reading the associated constant separately corrects the
test without changing the ABI contract. The second run passes **82/82**, zero skips, in
`00b75977-87fd-4439-b216-cd788d0ec396` (4.551 seconds after a 1m13s release build). It includes
declaration/parameter, actual compiler-interface, derive, five public generation workflows and
the complete Sprite/Tilemap consumer suite. The no-default-feature material library check also
passes (17.71 seconds). Independent consumption and native qualification are still in progress.

These results do not complete the persistent schema/source/Runtime product plumbing, full
R9b, graph-shared material inputs or the foundation Goal. No numerical performance improvement
is inferred from build/test durations.

The independent correctness review finds two concrete derive defects outside the first green
inputs: a supported field named `reader` is shadowed by Encase's unrelated generated reader,
and a scalar alias named `ArrayLength` triggers its syntactic runtime-array special case. Actual
public compile counterexamples reproduce both. The adapter now retains official ShaderType,
ShaderSize, WriteInto and constraint generation, omits unused buffer reader implementations,
and wraps the cloned field type in a transparent syntax group so Rust resolves its actual type.
Semantic reconstruction continues through the original material values. The extended owning
suite passes **83/83**, zero skips, in `a06bc9e8-2e35-411c-9768-ecea181bc3f3` (4.535 seconds
after a 1m12s build). Independent rereview closes both P2 findings. Correctness Clippy for the
material, derive and Sprite/Tilemap owners passes; existing dependency/style warnings remain.

The independently locked material consumer compiles and passes **1/1** in
`683c981a-bf7b-4431-ab68-faed097edfbd` (0.042 seconds after 22.42 seconds). Its first run
`9efbb0a6-d55f-4b0b-8642-01bc398a0863` fails a new test's guessed variable-name assertion from
another Palette fixture; comparing the actual complete input and assembled source corrects that
oracle. Old typed values, signed-zero bytes, handles, Shader artifacts and failed-edit last-good
are retained through library retirement. The no-default normal dependency tree contains only
`nara_material`; sampler policy therefore needs neither ECS nor a compiler. The production
all-feature native library check passes (1m09s), while actual native tests remain pending.

One further ownership review identifies a new sustained-edit retention defect: the former static
pipeline cache appends every distinct Shader artifact until device-epoch replacement. After
ordinary old captures, outputs and original submissions retire, cached Ready/Rejected keys still
retain those old generations, source bytes and native pipelines. Single-candidate budgets do not
bound that cross-call history. The selected repair is a private 64-entry LRU cache, trimmed after
the entire preparation call on both success and failure, with orphaned binding-layout lookup
entries removed. Prepared/submitted draws keep their actual handles independently. One call may
prepare more than 64 variants; the bound governs retained acceleration state after return, not
product admission or a complete native-memory budget. Qualify both multi-call edit churn and a
single legal call exceeding the retained cache, including late original capture realization.

The native adversarial review closes that cache-retention P2 after the common success/failure
exit trims Ready/Rejected LRU entries and orphaned binding layouts. It confirms that trimming
occurs after complete draw resource preparation; old native pipelines/bindings remain held by
prepared work and original submission custody. The active test history is retained:

- The first native test compile finds one old helper invocation missing the newly explicit
  Image handle; the retained TileSet's own Image handle corrects that fixture.
- The second compile catches a new negative-test closure returning the success shape expected
  by neither the original `with_preparation` nor its asserted rejection; returning the actual
  ContentRejected error corrects that fixture.
- The first complete ordinary native-owner run `2f0b6650-acf0-41ba-898f-6dd9ca50e7fb` executes
  415 cases: 414 pass and one new rejection-cache assertion fails, with 65 explicit skips.
  The third private statistic is layout count, not Ready count. The correction keeps one shared
  layout, bounds entries and separately checks that each cached entry is Rejected.
- The first Vulkan focused run passes 12/13. The new churn trajectory incorrectly allocates a
  new source generation at every call and is truthfully refused by the unchanged exact source
  gate. Reusing one source generation for that trajectory corrects the fixture without widening
  admission. Its original independent producer, increasing frame index and old-output controls
  remain intact.

The corrected focused run with Vulkan selected for the production Host **passes 13/13** in
`a07d9528-64f4-4eef-b502-013a1226c35f` (11.031 seconds after a 2m26s release test build),
with 467 cases outside the explicit selection. It executes all five ignored native material
Host cases plus material capacity, budget, identity and cache controls. The production Host
reports NVIDIA GeForce RTX 4090, Vulkan, NVIDIA driver 621.14, with forced fallback disabled.
The separate private `ReadbackGpu` controls retain their original default adapter/fallback
selection; the environment setting does not establish their individual backend or hardware.
Evidence
includes nonzero mixed scalar/vector parameters with pixel tolerance one, integer-mode control,
both texture contributions, compatible B/current, interface-incompatible C refusal, first
realization of retained A, same original Host/device epoch, unchanged pipeline variants for
value/texture-content updates and exact producer/source admission.

The same focused run proves shared Image deduplication across multiple captures while retaining
per-occurrence buffers/custody; allocation demand one byte short rejects before uploads and exact
demand succeeds. Partial texture OOM retains prior work through the original prefix, cache clear
does not refund live holders, frame watermark remains unchanged and the same frame retries.
Cache controls execute a single 66-variant working set, further edits, late evicted-A realization,
old output readback and 66 rejected native Shader variants. Retained cache entries remain at
64 and the shared binding layout remains one; original output/resources retire normally.

One sample records CPU initial compile 645.7 microseconds, compatible edit compile 327
microseconds and first Host submission 3.4823 milliseconds. These are one fixture observation
per interval, not P50/P95, pure GPU compile duration, scanout or an optimization claim; the Host
interval includes its actual device/pipeline preparation and submission, excluding later readback.
No general large-game material cost or foreign native qualification follows.

The full workspace release all-target check passes after the owned formatter runs (5m42s).
Seventeen owned lockfiles retain every original package version/source and add only the required
material encoder/compiler closure. Sixteen independent Windows metadata graphs resolve locked;
the paused GPUI consumer is explicitly excluded. An initial unfiltered offline metadata attempt
reaches an unrelated uncached `cstr` foreign dependency; the target-filtered Windows resolution
passes without downloading or changing that package. No runtime execution is inferred from
metadata. The ordinary stable formatter preserves the already-qualified Winit source; its
nightly-option warnings do not indicate changed vendor behavior.

Representative owning commands, with the original absolute target and jobs one, are:

```text
cargo +1.95.0 check --locked --offline --release --workspace --all-targets
cargo +1.95.0 nextest run --locked --offline --release -p nara_material -p nara_material_derive -p nara_sprite_render --all-features --lib --test generation_workflow --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --offline --release -p nara-material-module-consumer --test workflow --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --locked --offline --release -p nara_render_wgpu --all-features --lib --run-ignored all -E 'test(stock_quad::material_tests::) | test(host::program_tests::material::)' --test-threads=1 --no-fail-fast --no-tests fail --success-output immediate
```

The focused native invocation sets `WGPU_BACKEND=vulkan` and `NARA_WGPU_FORCE_FALLBACK=0`.
The [material guide](../guides/materials.md) describes current explicit source admission and
retained bindings without claiming the pending persistent authoring/import workflow.

### Final Owning And Native Results

After formatting and retaining the original lockfile package selections, the combined owning
run passes **498/498**, with 65 explicit skips, in
`85b905ec-b338-4d58-a132-cd6f54c9fb09` (140.491 seconds after a 13m32s release build). This
includes all 83 material/derive/Sprite controls and all 415 ordinary native-owner cases, closing
the corrected rejection-cache assertion from the earlier aggregate. The ignored native material
Host cases retain their separately executed selection rather than being counted as ordinary
passes. Counts from focused/aggregate runs overlap.

The same five production Host trajectories also pass on **DX12: 5/5**, in
`a1899417-37b1-4bde-bd94-eec08e91e551` (4.924 seconds, 558 outside the selection). The Host
reports RTX 4090, discrete GPU, DX12, driver 32.0.16.2114. The sample records CPU initial
compile 652.1 microseconds, compatible edit compile 278.7 microseconds and first Host submit
call 28.5046 milliseconds. These timestamps bracket `publish_material` and `submit_program`
calls, after `begin_frame`; they do not attribute earlier device setup, readback or hardware
execution. Neither these samples nor the Vulkan sample establish a backend performance winner.

The first DX12 command selected only the native package, creating a different feature union and
unnecessarily rebuilding dependencies. Its exact owned compiler/launcher descendants were
identified, cancelled and confirmed terminal before another Cargo invocation. No test result
is claimed for that cancelled build, and no other session's process was touched. Reusing the
same four-package/all-feature/target selection as the owning run completes the DX12 test build
in 2.50 seconds. Final all-target correctness Clippy for those four owners passes (1m13s),
with existing dependency/style warnings. All verification remains serial and uses the original
target; no clean or parallel Cargo is introduced.

The independently locked renamed-root material test passes **1/1** in
`69e7125d-f728-4a71-b6df-6a6b8fc4eb21` (0.041 seconds after an 8m07s release build).
Its manifest still declares only `engine`; the macro locates the facade's runtime support
without a direct encoder dependency. The same actual test combines `reader`, the scalar
`ArrayLength` alias, a vector's aligned offset, exact bytes and semantic reconstruction.
The direct independent Palette package also passes all-target strict Clippy with `-D warnings`
(34.71 seconds). Neither result is inferred from token parsing or a typed but unexecuted closure.

Final root, module-consumer and renamed-root `cargo fmt --all -- --check` pass. Added local
document links/fragments, affected TOML/lock parsing and normal staged/working whitespace checks
pass. The imported Winit nightly-option warnings retain their known scope; vendor and physical
DPI source have no new changes. The pre-1.0 static definition/input/instance APIs are removed
after migrating all real Rust consumers, with a concrete guide and Conventional Commit breaking
change note rather than compatibility aliases.

All eleven scoped research/implementation/review agents are closed, including the two reused
native-worker/reviewer threads. The source findings, two derive P2s and sustained-cache P2
have been reconciled and independently rereviewed. No remote publication or main/branch
movement is part of this unit. Complete persistent material delivery, shared graph texture
inputs, numerical production budgets, foreign native execution and full R9b/R12/foundation
completion remain open.
