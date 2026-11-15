# Durable Materials And Tracked Shader Artifacts

Status: active. This is the complete R9b successor of the
[foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md#r9-material-artifacts-views-color-and-domain-render-locality),
starting from `4bd02b9`. Editor/component and Dialogue UI development remain paused. Native
qualification shares the original serial Cargo owner with Runtime work.
The declaration/Shader/native prerequisite is implemented at `899d64b`. The selected persistent
source/processed Material product workflow is implemented and qualified below. Native Scene
watching, live external GPU execution and ordinary windowed source updates are also qualified
below, the latter at `177684e`. The captured View/time successor below is now implemented and
qualified for its ordinary Material consumer. The joint Material/history and bounded depth/unit
successors below are also qualified for their named consumers; broader View/history policy,
3D authoring and numerical production costs remain open. Live Editor Material/Shader hydration
remains paused with the Editor work. The graph-shared texture workflow is implemented and
qualified below; complete R9b and the foundation Goal are not finished.
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

## Native Scene Source Watching

Status: implemented at `f948da9`, with shared native custody at `bf8bda1` and independent
consumers at `159da63`. Qualified on Windows; Linux cross-compilation only. Compose an optional
`ProjectSceneWatchPlugin` through the existing product
recipe. It starts native coverage only during actual source Host admission, after matching an
explicit absolute native root to the loader's original directory capability. Processed Hosts
reject the plugin before native acquisition. An unbound ordinary Runtime refuses startup rather
than claiming active source coverage. The stock generic asset watcher is a conflicting route;
Scene observations never enter its independent per-asset publication resolver.

Use one original notify worker for the project root nonrecursively and the configured source
directories recursively, plus uncovered schema/Input parents and intermediate ancestors watched
nonrecursively. The latter detect nested parent replacement. Deduplicate coverage rather than
recursively watching Cargo output, with a finite 128-path admission bound. Establish the watcher
before requesting a mandatory initial
source rescan, closing the capture-to-watch gap while retaining the original usable Scene.
TaskUpdate translates a bounded queue prefix into the existing Scene source ingress before its
request poll. Metadata/catalogue structural observations request full revalidation. Overflow,
backend failure and uncertain translation replace the sticky queue and native watcher before
requesting a full rescan; replacement failure is observable and explicitly retryable. Ready
content, game overlays, exact commit/acknowledgement and the two-generation bound stay unchanged.

The published notify 8.2.0 Windows/Linux Drop only requests worker shutdown. Maintain a bounded
source patch retaining the original worker handle, polling completion and joining that worker;
callback destruction alone cannot prove join. `AssetWatchRuntime` registers with the original
Runtime close owner, retains every acquired native session before watch setup, and budgets active
and retiring sessions together. Failed setup, facade/receipt Drop, rescan replacement and Stop
cannot lose unfinished custody. Runtime close fences acquisition and remains Pending until all
original workers are joined. Persistent joined failures remain errors. A read-only observer may
outlive the stopped World without keeping a native worker alive. Managed join initially admits
Windows/Linux only; native execution on another platform needs its own coherent backend contract.

Acceptance must include actual filesystem edits through an independently locked game: initial
reconciliation, valid/invalid/repaired/missing Shader input, Material/Image state, catalogue
membership, frozen Ready interleavings and explicit game commits. Deterministic queue-loss
controls remain separately labeled. Preserve original generic watcher/rescan controls and prove
partial-setup cleanup, finite-capacity reuse, held receipt/facade shutdown, deadline/RetryClose and
post-Stop started/joined equality through the read-only observer. Test changed integration and
native ownership, not unrelated UI/GPU mechanisms. No automatic game-state replacement, general
watcher framework, second task scheduler or numeric performance claim is selected.

### Native Watching Qualification

The initial owning library check passes on Windows/Rust 1.95.0. Its complete ordinary suite passes
47/47 in `b58b122d-9cd6-41a4-b688-49726383a6d3` (2.342 seconds after a 40.38-second release
build). It includes original generic routing/rescan controls, actual callback blocking through a
zero-deadline CloseIncomplete and RetryClose, partial multi-path setup failure, finite-capacity
reuse and retained facade/receipt/observer retirement. The maintained Windows backend's direct
original-worker controls pass 2/2 in `86a73815-97aa-4c7e-9be0-d8bc04759773` (0.074 seconds;
seven upstream cases outside the selection). Since notify is an excluded path dependency, its
private tests use the imported package's own manifest and separately resolved development closure;
the owning library run uses Nara's original locked production dependency closure. An initial
workspace package test refusal and missing offline `trash` development dependency produce no
execution evidence. The package-local retry downloads the missing development dependencies
through the configured proxy without changing Nara's package versions.

Independent review identifies three concrete P2 corrections. Inotify now distinguishes a queued
Shutdown from a successful wake: failed wake is reported and retried without another Shutdown;
losing the worker's waker does not certify its original thread was joined. Two deterministic
controls exercise wake failure and disconnect-before-return, with Linux execution still separate.
Scene coverage includes uncovered intermediate directory ancestors nonrecursively, so replacing
a nested configured parent rebuilds the leaf watches. Lifecycle scope refusal revokes coverage,
records an observable failure and returns the original classified Runtime fault rather than
silently leaving `Watching`. Successful full-rescan admission clears its previous ingress error;
it does not claim preparation or commit success. Bounded independent rereviews close all three
findings without a second watcher, queue manager or Runtime authority.

The first independently locked physical game run, `b3527007-cd31-4391-9fb0-57772d5ac47c`,
builds in 3m40s and exposes a fixture expectation at initial reconciliation: Startup includes its
own executable/schema attestation plan, while a later Scene request uses the already admitted
native schema. Initial equivalence now compares schema, manifest and runtime-content digests;
full Ready-candidate equality and the committed candidate's complete stamp remain strict. The
next run, `497d6f7c-08a1-42b9-a7fc-32e50f5f9254`, completes its content transitions but fails a
final per-queue counter assumption. Native recovery correctly creates a fresh zero-counter queue.
The fixture now retains actual nonzero queue observations across frames instead of assuming the
final queue accumulated every event. Neither failed run is a passing physical qualification.

The published Windows notify backend emits undifferentiated `Any` create/modify/remove kinds.
The original conservative adapter treats these as incomplete observation; the physical workflow
must observe actual coverage recovery as well as exact accepted events. Its final activity check
therefore accepts recorded incremental events or a real recovery, with an explicit Windows
recovery assertion. This preserves the existing uncertain-scope contract rather than weakening
it to infer file scope from a native path. Windows full-rescan cost and backend classification
remain explicit future optimization work; native dependency-directed filtering is not claimed.

The first combined owning/affected run passes 105/112 in
`c9b8eaf0-a330-4494-b8ad-4782478b3bae` (13.026 seconds after an 8m41s release build;
1745 cases outside the selection). All 47 watcher-library controls and the original selected
Scene/Startup controls pass. Seven new hosted watcher controls fail before watcher assertions
because their fixture independently reopens the settings and loader directory capabilities.
The fixture now shares one original authority for both, preserving the production
`ProjectRootMismatch` guard instead of weakening source identity.

The corrected-capability run reaches 110/112 in
`bc7ad5cd-f5bd-451b-9be2-391df79c9409` (13.258 seconds after an 8m20s release build).
The remaining two failures are Windows `PermissionDenied` from moving ancestors of live watched
directories. A standalone Win32 control reproduces that denial with share-delete descendant
handles, including a `FileRenameInfoEx` POSIX request; moving with only the original root handle
retained succeeds. Windows controls now first require the real denied move and unchanged state,
then retire and join the exact original watches before moving directories through the coverage
gap and requesting explicit recovery. Original root-mismatch, retry deduplication, sender
disconnection, fresh coverage and Scene preservation assertions remain. Linux retains its active
worker movement branch. The fixture validates every move against its own canonical workspace.
Independent bounded review finds no P1/P2 in this correction. This is not a claim that Windows
permits moving those ancestors while their descendant watches remain open.

Both Linux cross-compilation checks pass with the installed Rust 1.95.0 target: the maintained
notify package's production library and tests (12.52 seconds), and Nara's locked
`nara_asset_watch` all-target surface (34.88 seconds). The latter retains the original production
dependency versions. The former uses its separate package-local development lock; its first
offline attempt lacks `errno` and then downloads the missing locked dependencies through the
configured proxy. These checks type-check the Linux original-worker and wake-retry paths;
they do not execute Linux native watching, faults or filesystem behavior.

The final independently locked Material package passes 11/11 in
`e30abfbf-4111-43b6-9d9a-c777bc3b4712` (1.987 seconds after a 4m42s release build).
The physical filesystem trajectory completes eight explicit commits and three expected content
refusals without manual semantic notices or rescan calls. It retains a Ready candidate across a
later real edit, preserves last-good through invalid/missing WGSL, recovers after file creation,
updates actual decoded PNG pixels and Material texture selection, and discovers a previously
unknown Image through metadata/catalogue reconciliation. Every commit preserves owned gameplay
progress through the explicit overlay and acknowledges the exact request. Duplicate native
observations do not require a fixed OS event count. Initial schema/content equivalence, frozen
complete candidates and committed exact stamps remain checked.

An independent game close participant waits for started/joined equality and zero active/retiring
native sessions while the original World still retains its lifetime marker. The retained
read-only observer repeats those checks after Headless Host cleanup and World destruction. This
is actual Windows filesystem and CPU-content evidence, not destination GPU presentation. The
package also retains all ten original public Material, semantic source-update, source editing and
source-free processed controls. The CI adds this opt-in trajectory to its existing Windows/Linux
module-consumer job; only local Windows execution is claimed here.

The final owning/affected selection passes 112/112 in
`cd789eef-abc3-435a-b9a4-f7b183027a98` (13.342 seconds after an 8m23s release build;
1745 cases outside the selection). All 17 new Source watcher controls pass, including exact
configuration bytes, finite coverage admission, optional/unbound and processed refusal, route
conflicts, initial reconciliation, all three loss recoveries, lifecycle faults and platform-aware
directory controls. All 47 watcher-library controls and the 48 selected original Scene/Startup
controls also pass. Counts across the focused, preliminary and final runs overlap.

The root workspace release all-target check passes (1m53s). The root production Scene adapter
also passes a Linux cross-compilation check with only `runtime-2d,serde,asset-watch` enabled
(1m03s), supplementing the two library checks above without claiming Linux execution. Owning
root/watcher all-target/all-feature correctness Clippy passes (2m17s); independent opt-in Material
all-target correctness Clippy passes (20.63 seconds), with non-correctness warnings retained.
Root and independent formatting checks pass. Dependency review confirms only twenty new package
identities needed by the independently enabled watcher closure, with no existing package
version/source/checksum change. The Nara root lock only replaces notify's registry provenance
with the maintained path. Imported package-byte comparison, affected TOML/lock/YAML parsing,
added local document links/fragments and whitespace checks pass.

Whitespace checking excludes the two byte-identical imported notify license/README files, whose
published trailing spaces include Markdown hard line breaks; maintained source and documentation
checks pass without editing those upstream artifacts.

The remaining scope is explicit: Windows undifferentiated events take full reconciliation;
numerical costs, optimized native classification, source-notification-to-GPU presentation,
successful optional-fallback interest and foreign native execution remain open. Managed watcher
registration outside Windows/Linux refuses before acquisition. No UI/component work is resumed,
and this qualified unit does not complete R9b, R12 or the full foundation Goal.

## Source Update Qualification

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

## Captured View And Shader Time Successor

Status: implemented at `ac6828a`/`26af57b`, with the independent consumer at `9212074`.
R9a extends the existing immutable frame packet through an ordinary
independent Material consumer. At the original Extract boundary, capture the existing App
real, virtual, fixed and render clocks, time settings and validated presented-spatial stamp
together with the actual spatial interpolation decision. Publish these facts atomically with
the extracted views and preserve them through target adaptation and delayed execution. Exact
Step without Extract retains the preceding captured view/time sample. No Host wall-clock
lookup or additional clock, scheduler, history or spatial owner is introduced.

The stock material shader receives a separate engine-owned View uniform, outside the authored
material parameter layout. It exposes the original projection/viewport, explicitly available
real/virtual/fixed time facts and pause/interpolation state. CPU elapsed time remains Duration;
GPU elapsed values use an explicit 3,600-second period with integer period/counter words.
Manually authored views without a clock sample advertise unavailable time rather than claim a
current Runtime sample. Ordinary time changes must not create material pipeline variants.

Qualify zero/multiple fixed ticks, Pause, exact Step and subsequent Extract, scaled virtual
time, actual interpolation, real-versus-virtual shader output and delayed replay using original
public capture and Host execution. Read back ordinary Material pixels, preserve the original
capture after later frames and check unchanged pipeline counts. Retain existing motion/history
discontinuity controls; camera/material compatibility classifications and broader View effects
remain separately open. Match owning API, feature-disabled and independent checks to the changed
seams, and record real execution, failures and limits before claiming qualification.

Primary-source comparison pins Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`
(`crates/bevy_render/src/globals.rs`, `crates/bevy_time/src/{time,virt,fixed}.rs`) and Godot
`7a37606a433c0e7735525037cdf774f043146a21` (`main/main.cpp`,
`servers/rendering/renderer_rd/{renderer_compositor_rd,renderer_canvas_render_rd}.cpp`,
`scene/main/scene_tree.cpp`). Bevy copies the existing virtual clock during Extract and wraps
GPU elapsed time at 3,600 seconds; ordinary pause freezes it. Godot advances renderer TIME using
the supplied scaled frame step and the same default rollover, while ordinary SceneTree Pause
does not freeze that Shader value. Neither source establishes Nara's required immutable delayed
replay or a high/low precision guarantee. Nara therefore retains its existing App semantics and
exposes both captured real and virtual facts rather than copying an ambiguous shared TIME policy.

### Implementation And Review

The CPU prerequisite is implemented at `ac6828a`. Owning Material/Render/Transform controls
pass 179/179 in `fa56fe66-85fc-4be6-88e0-a32d85f96d29` (11.009 seconds after a 1m47s
release build), and neutral Render/Material library checks pass. Independent core review finds
no P1/P2 in original sample custody, Step, failed publication or manual-view invalidation.

The native realization uses the existing private allocation/submission owners. Each distinct
View allocation contains a 224-byte uniform and one bind group. A temporary table compares
complete bytes, including signed zero, and indexes at most 16 entries per preparation;
additional distinct values still render through separately budgeted allocations. Preflight
charges the conservative per-batch upper bound, including metadata and original custody.
There is no persistent mutable View cache or time-dependent pipeline key.

Review first exposes an unnecessary local-name ban that cannot be inferred completely from
lowered Naga IR. The correction removes that extra policy and leaves ordinary WGSL scope to
Naga, retaining exact global resource/layout validation and global-redeclaration refusal.
An actual two-case red run, `bb709756-7b6a-452d-b7ae-a916be992a1a`, also reproduces the
new View allocation-stage cache assertion: the original primary texture binding is already
cached before that failure. The corrected control explicitly requires zero or one binding at
the appropriate stage, preserving retry, upload and retirement assertions. No backend cache
flush or additional source parser is introduced to satisfy the test.

Independent consumer review strengthens the pixel oracle with nonzero asymmetric forward and
inverse matrix probes, then a real Camera transform/projection change between captures A/B.
The old View identity continues while its mapping changes. Late first preparation and replay of
A must retain old time, mapping and motion coverage while retained B remains unchanged. Final
source rereviews close all four reported P2s; actual native results are recorded separately.

### Owning Qualification

The corrected Material/Sprite/native-owner selection passes **558/558**, with 67 explicit
skips, in `c423765c-ff30-4d49-8a99-cdc742456eb6` (157.534 seconds after a 3m58s release
build). This uses Windows 11, Rust 1.95.0 and explicit DX12 selection. It includes the actual
allocation-failure recovery, strict-byte View sharing/custody tests, all Naga/native member
offsets and span, legal local-scope controls, stock/UI/native Host regressions and retained
Source-generation controls. The 179-case prerequisite and this aggregate overlap in Material
coverage; their counts are not additive. The initial two-case red run builds in 6m47s and
fails both intended controls before the correction. Build times are not runtime cost evidence.

The ordinary Material consumer passes on Vulkan/RTX 4090 in
`05d558a5-e4d3-485c-a0ce-44cf0c0a5cb3` (1/1, 0.959 seconds), DX12/RTX 4090 in
`6ff5a302-73b7-435c-9c28-b85ed6ba2e99` (1/1, 1.292 seconds), and explicitly requested
DX12 Microsoft Basic Render Driver in `bb70dc1d-8ac6-425b-86a6-11584d4fe038` (1/1,
1.072 seconds). Each run captures and publishes seven frames, observes one material pipeline
and physically releases the original World. Owning all-target correctness Clippy, neutral
feature checks, independently locked all-target compilation, formatting and document/config
checks pass. The first independent compile refuses its stale lock, then exposes an unsupported
nine-plugin tuple; the fixture selects the ordinary dependency closure and splits installation
using the existing public API. Lock resolution adds 20 packages and retains all existing package
versions/sources. These are Windows/Rust 1.95.0 offscreen facts, not native-window scanout,
foreign execution or numerical production performance. The previous material/history scopes
remain independently qualified; counts from overlapping owning selections are not additive.

## Joint Material And History Discontinuity Successor

Status: implemented and locally qualified at `693149b`. The independent motion/history consumer
uses ordinary stock Material pixels as the current input of its finite accumulation effects.
It uses real Camera extraction, actual captured View/time and the existing Animation/Rapier
Runtime, preserving the manual procedural history oracle as a separate control. No production
clock, scheduler, history registry, public discontinuity taxonomy or GPU owner is introduced.

The concrete accumulator selects camera cut, solver correction/teleport, adopted material
parameter changes and same-ABI Shader body edits as incompatible with View 0 history. The
domain serializes each change with its original lineage `discontinue` and exact selected-slot
reset. Keep View/effect/lineage identities and the successful watermark; View 1 observes a
separate control Sprite and continues its actual predecessors. Failed Shader edits retain
last-good Material and continuous history. Normal time progression, Pause and exact Step
retain their existing owners and captured-sample rules. Material pipeline compatibility remains
separate: value/camera/time changes do not add variants, while an adopted Shader body may.

For each selected cut, hold a raw and admitted A plus an already prepared A, adapt its execution
target before the cut, then capture post-cut B. Late raw admission and first preparation of A
must refuse `StaleHistory`; submitting the prepared A must return the exact nonretryable original
candidate. B starts affected slots from Reset and preserves the other View's predecessor.
Verify four float histories and final pixels against an independent recurrence over actual
Material/Camera/motion pixels, retain old output reads and original submission prefixes, and
check ordinary duplicate/reversed-frame refusal. An independent non-temporal producer may still
first render old stock A with its exact original time, mapping and Material; it cannot replace B.
Include zero/multiple fixed ticks, scaled time, Pause and Step without Extract followed by the
next Extract. Close the original Runtime and GPU owners on success and assertion unwind.

This selects a concrete effect's compatibility policy, not automatic renderer classification,
general TAA/reprojection or cross-device temporal replay. The same-lineage cut rejects the whole
old group; selected reset limits predecessor invalidation only. Cross-thread cuts still require
domain synchronization around the complete Host transaction. The qualification below records
actual execution, corrections and limits for this consumer.

### Joint Qualification And Corrections

The first actual DX12 attempt, `a5dc436b-76c3-4c0d-a514-e4a78ca0941f`, refuses the first
capture before drawing: the old procedural consumer requires black clear while the ordinary
stock source explicitly selects transparent clear. Stock captures now accept their original
explicit Clear operation through shared stock validation; the manual path keeps its strict
black-clear oracle. The second attempt, `843f6903-6083-4de1-98a4-feb4239c6d93`, reaches float
history readback and exposes a one-byte source UNORM half-grid conversion difference. The
fixture now chooses integer UNORM byte centers before dividing by 255; it does not widen the
`3e-6` history tolerance or derive expected pixels from GPU results. The subsequent trajectory
passes 1/1 in `6c78fb04-3c2c-4411-aff8-363da57e2602`.

The affected DX12 cohort then passes **9/9** in
`6f2bdc3e-d534-476d-a165-8a05d6e93054` (15.033 seconds after a 48.51-second build).
It preserves all seven original history controls, including numeric reorder, effect replacement,
motion discontinuity, exact stale candidate/refusal, 512-plus View reclamation, independent
producer progress and Host reconstruction. The existing ordinary Material time consumer also
passes. No old test or pixel assertion is removed. Shared stock construction retains the final
image's premultiplied interpretation; its compositor still writes opaque alpha, preserving
all original numeric image results. Graph limits remain unchanged.

Independent source review finds no P1/P2 in graph/resource custody, exact capture pairing or
retirement. Testing review identifies two coverage gaps: ordinary Step could change pixels
even if correction were lost, and a failed Shader edit's pipeline increase could be hidden by
the next local baseline. The final consumer independently requires fixed tick 4 at `(1.25, 2)`
and exact next-Extract poses, records the native pipeline baseline after the first publication,
compares ordinary time/scale/Pause and invalid edits directly, and permits only the adopted
Shader body's single increase. Bounded rereview closes both findings. All six research,
implementation and review agents are closed.

The strengthened final trajectory passes on DX12 RTX 4090 in
`0c2ff457-c8f4-4415-8d55-157cd1032bc5` (1/1, 1.645 seconds) and Vulkan RTX 4090 in
`af0eb0fc-8e4b-4a4d-96ab-c44180bd64ee` (1/1, 0.875 seconds). After explicit software
selection is added for CI, the combined Material/time and joint-history selection passes
**2/2** on DX12 Microsoft Basic Render Driver in
`ca16c955-82f3-410c-94e1-1d9323c39fde` (6.060 seconds); the final hardware control also passes
1/1 in `fe49a674-6c10-4251-92c0-ec3b43ccc6a2` (2.684 seconds). Every joint run reports four
cuts, fourteen temporal publications, four retained old stock outputs and live native pipeline
count `7 -> 8`. This live count is observed before producer retirement; later cache/state
reclamation is a separate observation. Outputs are reread after original World destruction and
both drawing-producer retirements; final Host bytes, candidate metadata and cleanup faults
are zero. Counts from overlapping selections are not additive and timings are fixture facts,
not a production frame budget.

The updated ordinary main-thread `history_window` example separately passes 1/1 on DX12 RTX
4090 in `712a1854-d814-43be-945e-4551ccc7f35b` (4.670 seconds). It presents three actual
temporal outputs with predecessors `Uninitialized`, frame 1 and frame 2, validates original
captured clocks/spatial publication and retires the original Runtime/window/GPU owners.
This adds window execution evidence, not window-pixel readback or scanout. Both final native
paths use Windows 11 and Rust 1.95.0 with locked offline dependencies and one Cargo/build owner.
All-target correctness Clippy and independent-workspace formatting pass. Manifest, lock and CI
configuration parse checks and changed-document link/whitespace checks also pass. No production
engine Rust, dependency version or lockfile changes are needed for this joint consumer.
CI configuration is local and has not been run remotely. Foreign native execution, generic
compatibility classification, TAA/reprojection, cross-device temporal replay, numeric budgets
and full R9/R12/Foundation remain open; Editor/Dialogue work stays paused.

## Depth And Spatial Unit Successor

Status: implemented at `199a39d`, with the independent consumer at `d7e7c54` and local
qualification below. Starting source `2c665b3` explicitly refuses all depth formats and only
admits color clears for whole-texture initialization. This successor extends the original
finite program, Feature, output, producer and submission owners to complete R9's bounded
depth/unit consumer. It does not hide depth inside a Feature or describe it as linear
premultiplied color.

Replace the shared descriptor's unconditional transfer/alpha fields with an explicit
`TextureInterpretation`: Color retains both original facts; Depth has neither. Migrate existing
color consumers directly, without aliases or duplicate depth handle/output families. Admit only
whole single-mip, single-sample `Depth32Float` with exact original interpretation, usages and
budget accounting. Depth clear must be finite and within `[0, 1]`; color/depth clear mismatches,
unsupported formats, storage usage, incompatible copies and undeclared access refuse before
native work. Clear the whole attachment independently of the Feature viewport and Store it.
Preserve typed immutable imports, distinct predecessor storage and original physical custody.
Color-only final images, UI, Canvas and Material inputs must refuse depth explicitly.

An independently locked ordinary Feature consumer draws overlapping 3D geometry through the
actual native depth attachment, then consumes the declared depth version in another operation.
Its immutable capture states the camera projection and spatial-unit conversion. An independent
CPU oracle derives expected projection, occlusion and depth from that authored geometry rather
than copying GPU results or the uploaded matrix. Equivalent meter/centimeter inputs must agree;
changed camera/geometry must produce a distinguishable successor while old color/depth outputs
remain exact. Exercise imported depth, compatible preservation, a real refusal, original-prefix
observations and retained output reads after producer retirement. A 2D packet target does not
pretend to contain an engine Camera3d or a new spatial owner.

The consumer explicitly selects column-vector projection with WGSL column-major matrix upload,
Y-up and camera-forward +Z, a finite perspective near/far interval, WGPU clip depth `[0, 1]`,
clear depth 1 and `Less` comparison. Culling is disabled so draw-order reversal tests depth
without adding a winding policy. Authored coordinates use a finite positive meters-per-unit
conversion before the captured meter-space camera projection. These are local tracer facts,
not the engine's future global 3D handedness, import-unit or reverse-Z contract. The oracle uses
independent ray/plane equations at physical pixel centers and checks untouched clear regions.

Pinned source comparison keeps attachment ownership separate from projection policy. Bevy
`03ed5a27c92715c8fa2188f31f04e454ec39942f`,
`crates/bevy_core_pipeline/src/core_3d/mod.rs`, selects `Depth32Float` and constructs the View
depth texture from the Camera's explicit clear/load choice. Its
`crates/bevy_camera/src/components.rs` defaults that clear to 0 for reverse-Z. Godot
`7a37606a433c0e7735525037cdf774f043146a21`, `core/math/projection.cpp`, separately builds
perspective and depth correction, including clip-range remapping and reverse-Z. Nara's shared
texture declaration and finite clear do not select a comparison function or projection; those
stay with the concrete captured Feature. This tracer's ordinary finite projection therefore
does not freeze future engine-wide reverse-Z, import units or Camera3d design.

Set correctness bounds at entry: finite depth/color samples within `1e-6` of the independent
reference, exact clear/retained-copy bytes where applicable, and unambiguous pixel-center
coverage. Run CPU graph/refusal controls, the native consumer on explicit DX12 and Vulkan
hardware plus a separately reported DX12 software adapter, then affected color/program/history
regressions and appropriate checks serially. Missing requested adapters fail. Report actual
execution and corrections; these fixture facts do not select production performance budgets.
No general 3D renderer, Camera3d authoring, stencil, MSAA, reverse-Z policy, universal spatial-unit
type, second Queue or native owner is introduced. Editor/Dialogue remains paused and full
R8/R9/R12/Foundation stays open.

### Depth Implementation And Review

The original graph, texture port, immutable output/import and native initialization owners now
carry explicit Color/Depth interpretation. All existing color descriptors migrate directly;
there is no legacy field alias, second depth handle family or new GPU owner. The original WGPU
pin remains `cddb2f5d80dc0009b2ef6767635140cacf90a007`. Its `Depth32Float` uses four bytes
per texel and permits whole texture-to-texture copies with the default All aspect, including
`COPY_DST`; buffer upload restrictions do not prohibit that preservation path. Sampling uses
`TextureSampleType::Depth` and `texture_depth_2d`, without an optional filtering feature.
The consumer resolves sampled depth to float color for readback, so it does not claim the
separate downlevel depth-to-buffer capability.

Production all-feature library compilation passes in 12.90 seconds. The owning CPU selection
passes **131/131**, with 378 outside the selection, run
`bc031670-6818-4119-8dee-05600d60e1ad` (15.132 seconds after a 1m51s build). It includes
depth interpretation/format/usage refusal, finite clear admission with error atomicity, exact
16,384-byte depth budget, typed predecessor/read matching, original allocation rollback and
existing policy/UI/Canvas controls. Independent production review finds no P1/P2; actual
native depth execution remains separate from those source and CPU results.

The first standalone consumer compilation exposes the pinned WGPU's optional
`depth_write_enabled` and `depth_compare` fields; the Feature now supplies `Some(true)` and
`Some(Less)`. Independent testing review also identifies two meaningful gaps before native
qualification. The second raster pass originally redraws equal-depth geometry and changes no
pixels, so omitted encoding could pass. It now writes a closer green plane inside the smaller
scissor. Separate independent references require precisely 64 changed color/depth pixels,
unchanged original outputs, exact bytes outside that scissor and matching original/imported
preserved depth resolves. A further capacity boundary allows the first raster preparation and
its queued uploads before the second allocation refuses. Review of the actual native owner
corrects the initial zero-work assumption: `upload_buffer` calls the original Queue immediately,
and the Host binds even failed preparation to its original submitted prefix. The consumer now
checks each refusal's exact upload/encoded-copy increments, observes that producer's original
prefix without closing it, settles physical custody and checks released bytes before retry.
Successful work is checked as an increment over those retained failure facts. The Host's new
read-only `producer_submission_prefix` observation grants neither publication success nor a
new submit/retirement owner. The CPU reference uses the existing fixed tolerance for ordinary
f64 rounding at the closer plane; it does not widen GPU pixel tolerances.
The following standalone and native results qualify this selected successor.

The stable standalone CPU selection passes **11/11**, with one native case ignored, run
`8f2d2054-cce5-4205-bbc0-9196e14aad44` (0.998 seconds after a 29.03-second build).
It includes all five original candidate controls and six new public depth/independent-reference
controls. An intermediate compilation reads the library before the new prefix observation was
added and reports a missing method; the stable rerun recompiles both sides. The earlier owner
selection and this independently locked selection are separate evidence, not an aggregate.
Bounded production and testing rereviews close the concrete findings without widening the
fixed `1e-6` tolerance or changing the original Queue/submission owner.

The complete native trajectory passes on Windows 11 / Rust 1.95.0 with DX12 RTX 4090 in
`f561260f-fcaf-49c5-b2fc-a9928734e2d2` (1/1, 1.155 seconds after a 9.76-second build)
and Vulkan RTX 4090 in `fff1d92d-ca36-4711-b4b6-1224f71d0b27` (1/1, 0.926 seconds).
The explicitly requested DX12 Microsoft Basic Render Driver is observed as a CPU adapter;
the new depth trajectory and all three original color/buffer native program controls pass
**4/4** in `a99d8e94-1035-4159-944d-aca26038831b` (3.687 seconds after a 6.86-second build).
Each depth run completes six draw groups, eight imported readbacks, two post-upload capacity
refusals and exact 172,032-byte graph payloads. Eighteen prefix observations include retirement
clones rather than eighteen distinct submissions. Retained bytes and cleanup faults finish at
zero. Hardware/software logs retain actual adapter facts, and missing requested adapters fail.
These are full-pixel/depth and lifecycle facts for this bounded tracer, not production costs,
window scanout, general 3D authoring or qualification of foreign native platforms.

The final all-feature owning default selection passes **442/442**, with 67 ignored cases,
run `54f12db0-d13f-4bbc-af02-77eb7cc8c9cc` (153.127 seconds after a 13m21s build).
This selection includes actual existing GPU glyph/image/View controls, so it is not reported
as pure CPU coverage. The subsequent native program/Material selection initially passes
20/21 in `30bff8ea-4f29-44d2-a51f-8f3b4d18de47`. Its failure occurs in the fixture's
premise before drawing: 20,000 batches no longer exceed the fixed 64 MiB modeled metadata
envelope. The test now derives a crossing input from actual per-batch metadata cost and keeps
the original engine ceiling and refusal-before-native-budget assertions. The qualified rerun
passes **21/21** in `146f83e2-5e0d-4720-99b7-dc0a7e9e6e5b` (18.029 seconds after a
1m28s build). Its 21,789 batches model 67,110,480 bytes against the unchanged 67,108,864-byte
limit; fallback texture, instance charges and publication watermark remain untouched on refusal.
No production metadata limit or allocation policy is relaxed. These selections overlap the
earlier owner checks and are not additive. Independent History/caller/check closure follows.

The affected independently locked History, joint Material/motion and ordinary Material View/time
selection passes **9/9** on DX12 RTX 4090 in `e08e9fd9-7f7b-495a-92e5-409faf3ebf1c`
(12.201 seconds after an 8m36s build). It preserves the seven original History cases and both
ordinary Material trajectories. Final standalone CPU checks, including absent/foreign producer
prefix observations, pass **11/11** in `dcf3f2bf-d1aa-4140-a423-378840dc8eff`
(1.007 seconds after a 22.07-second build). Owning all-target/all-feature correctness Clippy
and standalone all-target correctness Clippy pass. The six direct caller closures also pass
locked/offline release all-target compilation: render-policy, render-composition, stock-composition,
Material source watching with its GPU feature, native UI Host upload and the desktop Two-Level
Game. The independent typed-port compile-fail doctests pass 2/2. Root/standalone final format,
other caller formatting, TOML/lock and CI YAML parsing, document links/headings and whitespace
checks pass. No dependency, lockfile, vendor or paused GPUI/Dialogue changes are included.
Eight bounded research, implementation and review tasks have ended; concrete findings and
the native fixture premise correction are closed. CI configuration is local only and was not
run remotely. This closes the bounded depth/unit successor while full R8/R9/R12/Foundation,
general 3D authoring, foreign native execution and numerical production budgets remain open.

## Presented Drawing And Picking Successor

Status: implemented at `cf22a5b`, with the independent native consumer at `a7a5fc8` and local
qualification below. Starting from `6304a7e`, complete the named R9 drawing/picking inverse,
mirrored-camera and DPI consumer through the ordinary Runtime and original DesktopRun. Source
inspection identifies an owning-contract gap: arbitrary custom policies currently receive the
original camera picking mapping after presentation even when their output changes coordinates.
Keep Editor/Dialogue product work paused; the shared Game presentation bridge must follow the
same corrected capture contract without new Editor functionality.

The render policy explicitly declares optional `WorldInputMapping2d` with its immutable CPU
capture. The validated finite invertible affine maps final-target physical pixels to original
packet-target physical pixels. Identity explicitly preserves coordinates. No declaration means
World picking is unavailable, while drawing can still succeed. Freeze the result once in the
original `WgpuRenderInput`; cloning and equal-extent image adaptation preserve it. Do not derive
it from the currently installed policy, parse Shader source or guess from successful rendering.
The ordinary stock policy declares identity; custom policies opt in only to a mapping they own.
Arbitrary nonlinear or channel-dependent effects remain unpickable without a qualified inverse.

After genuine presentation, the original guard joins that declared transform to captured Views,
Window resolution and WorldCanvas mapping. Convert native logical points to final physical
pixels once, apply the captured inverse, then use the original display/camera mapping. Reject
nonfinite, singular or projective declarations, target/source out-of-bounds, bars and half-open
View maximum edges. A successful unmapped successor revokes previous picking; a prepared,
unpresented capture cannot activate its mapping. Existing resize/DPI/close/producer revocation,
other-window isolation and immutable image output remain authoritative. The Editor Game bridge
uses the same frozen input fact rather than granting identity before native preparation.

The independent ordinary consumer uses real Camera/Sprite extraction with an asymmetric color
pattern, parented nonuniform/mirrored camera transforms, camera changes and a concrete horizontal
post-effect. Query public `WorldPointer` through a `SystemState` created on the original App
World and used in the genuine post-Host observer. Match the exact original capture to its
present receipt. Qualify identity, tint, declared mirror and undeclared mirror; the latter must
render successfully and refuse picking. Hold old captures across changes and native retirement,
then replay their exact CPU inputs offscreen for complete pixel comparison. An independent f64
oracle derives camera/world positions and color coverage, not the uploaded matrix or GPU bytes.
Use fixed `1e-5` world-unit and `1e-3` physical-pixel bounds for the bounded affine controls,
one UNORM byte for covered RGB and exact alpha/clear/retained bytes. Pixel-center inputs avoid
ambiguous coverage boundaries. These are correctness gates, not production performance budgets.

CPU controls cover scales 1, 1.5 and 2, offset Viewports, all mirror-axis combinations, a parented
sheared basis and unavailable/invalid declaration cases. Native Windows qualification separately
moves the original process/window across two actually active displays with different DPI and
joins native geometry, event-driven Window facts, genuine presentations and picking. A bounded
Win32 automation helper may locate only its own exact PID/title HWND and reposition that borrowed
window; it does not create/destroy a window, modify desktop configuration, replace Winit or
publish a presented guard. This is OS automation evidence, not a new engine window-control API.
Missing requested display hardware is a failed prerequisite, not a passing skip. A single-display
native path retains its separate scope. Keep original native retirement and output cleanup on
assertion unwind; window scanout and physical mouse delivery remain separate from callback
queries unless actually exercised.

Pinned source constraints: Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`,
`crates/bevy_camera/src/camera.rs`, preserves the full global affine inverse;
`crates/bevy_sprite_render/src/mesh2d/mesh.rs` disables culling for 2D meshes.
Godot `7a37606a433c0e7735525037cdf774f043146a21`, `scene/main/viewport.cpp`, localizes
input through the final viewport transform, while `core/math/rect2.h` excludes maximum edges.
Its Camera2D rebuilds position/rotation/zoom and does not prove full inherited shear. Neither
engine automatically provides a unique inverse for arbitrary post-processing Shader sampling.
Keep these source constraints distinct from Nara's executable qualification recorded below.

### Presented Mapping Implementation And Initial Qualification

The original policy admission now freezes the optional mapping once alongside its original
history lineage. Public observation, cloning, equal-extent target adaptation and repeated
construction read that frozen value. The stock policy explicitly chooses identity. The original
WorldPointer path validates a finite invertible affine, converts logical coordinates to final
physical pixels, applies the captured inverse and then checks original target, Canvas bars and
View bounds before the captured camera inverse. Runtime and embedded Game presentation use the
same input fact; a displayed unmapped successor drops the old guard, while pending work grants
no authority. No new input registry, Shader parser or native window owner is introduced.

The initial locked root check exposes the new standalone consumer's Windows automation dependency
edge in the root development closure. Offline resolution adds only that package's existing
`windows-sys 0.61.2` edge, with no version/source changes. The production all-feature library
check then passes. After independent source review, the embedded Game ownership test explicitly
declares identity rather than inheriting the new custom-policy default, and the mapped recovery
test is correctly selected as native because device-loss registration needs an initialized
original namespace. It initializes that real namespace rather than fabricating a CPU device.
The existing Game test additionally keeps old picking while an unmapped candidate waits, revokes
it on actual completion and restores it only after a later mapped completion.

The focused owner selection passes **77/77**, with 506 outside the selection, run
`c7cfaca1-1263-45b9-9757-53586c34d21c` (5.446 seconds after a 4m36s release build).
It includes the full render owner CPU suite, twelve new affine/DPI/guard controls, four frozen
policy mapping tests and the existing CPU Host retirement/isolation controls. This is not
evidence of actual window presentation, physical DPI events or GPU pixels; the independently
locked native consumer and the new original-device recovery control retain those gates below.

### Independent Native Drawing And Picking Qualification

The independently locked stock-composition consumer now provides `picking_window`, its ordinary
main-thread `DesktopRun`, and separate pure-math/native test targets. The original App's public
`WorldPointer` is queried before the Host and in the genuine post-presentation observer. The
observer joins the exact retained input to the original presentation receipt. Nine stages cover
identity, tint, mapped/unmapped horizontal effects, changed camera pose, all camera mirror-axis
combinations and source/destination/source movement. An unpresented changed capture retains the
preceding displayed inverse; an actually displayed unmapped result refuses picking.

The original HWND moves between the active 3840x2160 display at `[0, 0, 3840, 2160]`, DPI 144
(150%), and the 2560x1440 display at `[-2560, 533, 0, 1973]`, DPI 120 (125%). Genuine captures
settle at `384x256 -> 320x213 -> 384x256` physical client pixels, with matching native DPI,
Window resolution and independent numeric world positions. The software control stays on the
150% display. These geometry changes preserve the original logical window size within native
integer rounding; no desktop configuration or second native window owner is introduced.

After the original Runtime, World, HWND and physical pump retire, all nine retained CPU inputs
are realized through a separate original offscreen Host. An independent f64 camera/coverage
oracle checks complete pixels, exact alpha and known clear/bar colors, with at most one byte
for covered RGB. Later captures cannot mutate retained typed outputs. Every original prefix
settles, and final retained resource/candidate bytes and cleanup faults are zero. These are
captured-input pixel replays and genuine native presentation facts, not screen/front-buffer
readback, physical scanout or GPU-duration measurements.

| Selection | Actual result | Run |
|---|---|---|
| Independent camera reference and deliberate one-byte alpha/clear corruption controls | 2/2, 0.121 s; three native cases remain explicitly ignored | `a98d3e4f-5c57-464c-b351-0534490d5c1d` |
| First single-display DX12 hardware trajectory | 1/1, 5.063 s | `4df017f8-ab15-4259-8fdb-ef56592c7366` |
| DX12 RTX 4090: complete native target, mixed DPI, single display and observer failure | 3/3, zero skips, 12.439 s | `fe16d9e7-c244-4a60-b801-e7024cf4a4e7` |
| Vulkan RTX 4090: actual mixed-DPI trajectory | 1/1, 4.630 s; two cases outside selection | `f156bace-6979-4914-9419-a4d57f0610f4` |
| DX12 Microsoft Basic Render Driver: explicit software, single display | 1/1, 4.784 s; two cases outside selection | `ad8f7be4-fff9-418f-b477-7728345a5827` |
| Original Winit geometry and OS title-bar mixed-DPI controls | 2/2, 10.397 s; 81 cases outside selection | `7fb92328-c2e3-4c16-9c23-0c178993cf98` |
| Full all-feature render/WGPU library owners | 515/515, 182.505 s; 68 explicitly ignored cases | `6f90db80-432d-4cc1-a8f3-55842fa77da4` |
| Original native Runtime guard registration/revocation/recovery | 4/4, 4.387 s; 510 cases outside selection | `6d233bd4-bf46-48e2-8d84-bf540178779a` |
| Original Game capture/input/completion/Stop bridge, with desktop feature closure | 20/20, 2.257 s; 1,642 cases outside selection | `5ffbb723-0d6e-4b17-938d-e63a9e6fe8b7` |
| DX12 software: injected original observer failure and native retirement | 1/1, 0.845 s; two cases outside selection | `0850b217-1145-4ca7-ba32-3c54ccd9b0a8` |

The first ordinary release build passes in 15m40s; after formatting the final example rebuild
passes in 17.36 s. These are local build observations, not latency distributions or speedups.
The only root/independent lock changes add the existing `windows-sys 0.61.2` dependency edge
for this consumer's target-Windows automation. No package version or source changes.

The all-feature owner run includes existing native allocation/pixel controls as well as CPU
graph/capture/budget tests; it is not labeled a pure CPU run. The four separately selected
original-device controls check presentation-permission registration, own/other-window revocation,
unmapped success, mapped recovery and injected loss. Their private owner calls do not substitute
for the ordinary window's genuine presentations. Counts overlap the focused 77-case prerequisite
and other selections and must not be added into a framework-wide total.

The first root Game-bridge release test build is interrupted by the user before a final result;
its original handle is subsequently absent and no owned compiler remains. It contributes no
passing test evidence. Inspection also confirms that the complete `game_view_outcomes` module
requires `desktop-winit`; the successor command explicitly enables it alongside tooling,
project workflow, 2D/UI presentation and WGPU. This is a qualification-command correction,
not a production feature-gate change or a reason to repeat the completed physical GPU runs.
The corrected command completes its release build in 16m08s and passes all twenty selected
bridge controls. It includes the new mapped/pending/unmapped/recovered sequence, original
capture selection, logical input routing, exact completion, fault and Stop retirement. This
remains a bounded bridge/owner run, separate from native window-presentation qualification.

Independent review corrects the fixture's initial stage name to the actual Render stage with
Seal/Publish ordering, and rejects a callback panic that could bypass original native custody.
The observer now retains the first panic, requests ordinary exit, verifies original World/HWND/
pump retirement, drops `DesktopRun`, then resumes that same payload. The deliberate-failure
child exits 101 with its original panic and exact cleanup marker; the wrapper accepts no success
marker for that path. Review also strengthens alpha/clear comparison, requires the pre-Host fact
and distinguishable mapping transitions, and moves native negative probes clearly outside the
View/window. Exact f64 boundary construction followed by f32 quantization could otherwise fall
inside at 150% DPI. Exact half-open maximum-edge controls remain in the owning CPU tests.

The unchanged original OS title-bar controls also pass on the same live display pair, after a
3m25s owning release build. Both directions preserve the default `360x240` logical client size;
explicit `InnerSizeWriter` legs settle at `424x272` on DISPLAY1 and `360x240` on DISPLAY2.
Native/Winit/Runtime geometry, original HWND destruction and mouse-button/cursor restoration
remain asserted. No additional Winit repair or display setting is needed. This is OS-injected
drag evidence, separate from the programmatic GPU/picking trajectory above; GPU presentation
during that injected gesture and human-hand feel are not inferred. The
[native qualification guide](../verification/native-editor-qualification.md#mixed-dpi-window-controls)
retains earlier failures and patch provenance.

The Windows CI definition prebuilds the ordinary example, then selects the single-display and
observer-failure cases with explicit DX12 software rendering. Actual mixed-DPI hardware remains
a separately invoked local gate. YAML parsing passes; this is local CI configuration and does
not claim a hosted run or remote publication.

The independent source review has no remaining P1/P2; all eight research, implementation and
review tasks are closed. General nonlinear inverses, UI hit retargeting, foreign native execution,
physical device loss, maximized/snap/keyboard movement, human-hand observation and numerical
performance budgets remain outside this bounded consumer. Editor/Dialogue product work stays
paused, and full R8/R9/R12/Foundation remains active.

### Final Static And Delivery Checks

Release correctness Clippy passes for both render owners with all features/targets (15.69 s),
the root production library with the same Game bridge feature closure (49.38 s), and the
independently locked stock consumer's default-feature all-target selection (30.64 s). This last
selection compiles the ordinary examples and tests; the optional native Editor consumer is
outside it. These gates deny `clippy::correctness`, not every existing warning. Root and
independent `cargo fmt --all -- --check` pass. Added relative Markdown links/heading fragments,
both manifests/locks, CI YAML and ordinary whitespace checks pass. No unrelated native Editor
matrix, foreign execution or remote CI is inferred or repeated.

With Rust 1.95.0, jobs one, offline resolution and the absolute root `target`, the exact corrected
bridge and affected owner selections are:

```text
cargo +1.95.0 nextest run --locked --offline --release -p nara --no-default-features --features tooling,project-workflow,presentation-2d,presentation-ui,desktop-winit,render-wgpu --lib -E 'test(game_view::) | test(game_view_outcomes::)' --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --locked --offline --release -p nara_render -p nara_render_wgpu --all-features --lib --test-threads=1 --no-fail-fast --no-tests fail
cargo +1.95.0 nextest run --locked --offline --release -p nara_render_wgpu --all-features --lib --run-ignored only -E 'test(host::runtime::tests::)' --test-threads=1 --no-fail-fast --no-tests fail
```

Native commands additionally select `WGPU_BACKEND` and `NARA_WGPU_FORCE_FALLBACK` explicitly;
the independent consumer's README records prebuilding and the separate physical/software
selections. Implementation `cf22a5b` and consumer/CI `a7a5fc8` are precise local commits. This
closes the named drawing/picking successor while the complete Foundation Goal stays active.

## Live Native Source Updates Through External GPU Execution

Status: implemented and locally qualified at `0602b5d`. Extend the independently
locked Material consumer's existing physical-file workflow without replacing its CPU-only
trajectory. Drive the ordinary public `HeadlessSession` one frame at a time. A selected Publish
system takes the original sealed render-only batch once and retains immutable CPU input and
game observations. The caller executes that input between live Runtime frames through one
external `WgpuRenderHost`; no native GPU owner or second Scene publisher enters the World.

Use a logical 32 by 24 target, a game-owned Camera and an eight-unit white Sprite. Resolve any
ordinary product-composition gap at its original owner rather than using Editor-only placement
or reconstructing stock packets. The optional GPU test retains the complete existing filesystem,
Ready, explicit commit, rejection, gameplay-overlay, exact acknowledgement and watcher-close
assertions. An independent fixed-tolerance oracle checks actual center and transparent corner
pixels after every distinct committed Shader/Image/Material state. New captures submitted while
invalid or missing source is rejected must still produce last-good pixels; rereading an earlier
output does not establish this behavior.

Hold the original startup input without native execution until the first Shader successor is
published and rendered live. First-submit it through another producer in the same Host, verify
its original pixels, then release it before preparing another Source generation. Repeat with
one unsubmitted palette input across the next Image publication. Original immutable captures
retain their original Source content budget; preserving the two-generation bound requires this
explicit release before a third generation, rather than increasing the limit or detaching its
accounting. This also respects each producer's original monotonic frame authority. Retain
published outputs across all later publication and Runtime Stop, verify their
pixels again, retire original producer prefixes, release CPU captures and outputs, then require
final Host retirement with zero retained native bytes and cleanup faults. Drive original watcher
join and World cleanup even on a failed assertion. Qualify the exact ignored trajectory separately
on local Vulkan and DX12, with a missing adapter or timeout reported as a failure. No OS-window
scanout, device reconstruction, foreign native execution or numeric performance budget follows
from this offscreen workflow. Full R9b/R12 and foundation completion remain open.

### Live Source-To-GPU Qualification

The independently locked ordinary Material package passes 14/14 in
`69ae5455-06f0-4391-805e-7fbb0b2d1115` (2.265 seconds after a 13m05s release build).
This includes the ten original Material/source/processed controls, the actual native watcher
workflow and three independent oracle controls. The oracle fixes a three-byte channel tolerance
before native execution, distinguishes every selected gain, stale palette and incorrect linear
sampling, and keeps source alpha linear with one association over transparent black.

The first native attempt, `5565b649-f67d-49ca-94c9-9e503ea0068c`, faults in the first Runtime
frame before pixel comparison: the CPU fixture's Sprite has no spatial Transform. The game-owned
View setup now inserts a missing `Transform2d` before extraction, without replacing an existing
Transform. The next attempt, `856ab678-73bb-492c-9664-8535d8255d8f`, reaches native submission but
finds a fixture assertion confusing Host maintenance sequence with the original capture sequence.
The image receipt and typed final output retain the original packet frame index; Host maintenance
keeps its independent increasing clock. Neither failed run qualifies the full trajectory.

The following Vulkan run, `339d8448-e440-4480-9d55-8fac1d6ea564`, reads correct initial and
successor Shader pixels, then the original Source owner refuses a third retained content revision.
Holding the startup CPU capture indefinitely is incompatible with the established two-generation
budget. A bounded ownership study confirms that native material pipelines retain compiled native
state and digest keys, Image caches retain native textures and resource identities, and published
outputs retain native allocation budgets. These do not retain the original Source generation.
CPU Material/Shader/input snapshots and unfinished submission custody do. The corrected trajectory
replays and releases each old CPU input across one publication boundary, with actual copy-prefix
completion before preparing another generation. It retains 14 representative published outputs
throughout, covering the distinct commit/refusal states and the two late submissions;
no limit increase, detached accounting or cache-policy replacement is introduced.

The complete trajectory then passes Vulkan 1/1 in
`3f66fcc3-00c3-483a-846e-17758f7c1403` (1.382 seconds after a 20.92-second fixture build), and
DX12 1/1 in `ebade381-d85b-4155-9364-0c35535c5c6b` (1.682 seconds after a 3.46-second build).
Both use Windows 11, Rust 1.95.0 and an actual adapter with fallback disabled. They perform eight
explicit Scene commits, three content refusals and two late first submissions of original CPU
captures on another producer in the same Host. The original startup frame still reads
`[2,48,182,255]` after the 0.75 Shader successor reads `[2,36,137,255]`. The old palette capture
still reads `[0,6,23,255]` after the edited PNG reads `[0,1,8,255]`. Invalid/missing/unknown
source receives new captured and submitted frames with last-good pixels. Transparent corners
remain exact zero; typed output source generation, producer and frame identity remain exact.

Vulkan performs 120 image submissions and DX12 117; these are fixture observations, not fixed
native-event or performance requirements. Each retains 14 exact published outputs, rereads them
after Runtime Stop and producer retirement, and joins all 11 original native watcher workers.
Both finish with zero active/retiring workers, zero retained logical native/candidate bytes and
zero GPU cleanup faults. Pixel comparison covers the center and four corners, not a complete
image-equality oracle. This proves retained GPU outputs after Stop, not first submission of a
CPU input after Stop. Gameplay score/inventory and unchanged authored Scene bytes remain
checked. The external Host stays outside the World throughout actual execution.

Independent review corrects one P2 in failure cleanup: a physically terminal Runtime `Failed`
observation cannot skip producer-prefix retirement or final Host drain. Each stage is protected
separately, remaining cleanup always runs, and the original trajectory panic is preserved with
specific secondary diagnostics. Bounded rereview closes that finding. The CI adds the actual
ignored workflow to its existing Windows DX12 consumer lane and compiles/runs the ordinary
oracle controls in the module lane; local execution does not attest remote CI.

The final bounded independent review finds no remaining P1/P2 in the two replay windows,
original capture/Source budget custody, post-Stop output reads, failure cleanup or requested
desktop composition. All seven scoped research/implementation/review agents are closed.

The final ordinary independent package repeats 14/14 in
`8d499386-3dc1-40ea-a4e1-6f0c545321e2` (2.750 seconds after a 2.17-second build), with the
actual GPU case separately qualified above. The complete affected root plugin/configuration
composition targets pass 73/73 in `cc472f8c-93bb-4c07-9e9b-05c6d80178ca` (3.388 seconds
after a 4m04s release build). They retain the positive requested Desktop/WGPU composition,
unrequested native-consumer absence, shared policy/contribution deduplication, executable
identity and direct/file-backed parity. Root owning-target and independent all-target
correctness Clippy pass in 17.74 and 13.93 seconds respectively, with existing warnings.
Root and independent `cargo fmt --all -- --check`, affected TOML/lock/workflow parsing,
added local document links/fragments and normal whitespace checks pass. No dependency version
or source changes; the independent lock adds only the explicit existing WGPU edge. Verification
uses serial Cargo, one build job and the original target directory without cleaning.

Windows `Any` notifications still recover through full rescan. This unit selects no native
filtering optimization, numerical latency/memory budget, OS-window/scanout proof, device
reconstruction, Linux/macOS execution or full R9b/R12/Foundation completion. Editor/component
and Dialogue work remain paused. No remote publication or branch/main movement is performed.

## Live Source Updates Through Ordinary Desktop Presentation

Status: implemented and locally qualified at `177684e`. Reuse the
independently locked physical-file workflow from `0602b5d`
through an ordinary main-thread `DesktopRun` source product requesting desktop, 2D presentation
and WGPU. Keep the same eight explicit Scene commits, three content refusals, frozen Ready
interleaving, gameplay overlay, exact acknowledgement and original native watcher shutdown.
Use the original stock capture, window Publish consumer, runner-local GPU Host and native loop;
do not add a second renderer, surface lease, GPU owner or Editor integration.

Inspect the original sealed input through `WgpuPolicyFrameReader` after Seal and before Publish,
without taking its batch or retaining Source generations for a detached audit trail. Retain
bounded copied frame/content facts only. A CPU-only game gate waits until the preceding content
state has actually presented before advancing the next scripted edit/commit. The presentation
observer must join the original runtime, App frame, render capture, window, surface incarnation,
backend and device epoch with the original stock Material/Shader/Image facts. New frames after
rejection must keep last-good bindings and still have genuine submission/present receipts.
Compiled desktop support alone remains separate from a requested desktop product.

After the final Done state actually presents, stop new submissions and seal the original bounded
completion telemetry before exit. Correlate representative successful states with their exact
completion samples; do not relabel callback latency as edit-to-result or GPU-only duration.
Retain the original target registry and read-only watcher observer through close, require actual
native target/provider/surface retirement, worker joins before World destruction, and recovery
of the same physical event loop only after complete retirement. Cleanup must still drive the
original owner on an assertion, observer or execution failure. Bound startup/live/cleanup waits,
report cooperative deadlines honestly, and fail missing adapters instead of silently skipping.

Run the ordinary native example serially on local Vulkan and DX12, then retain the affected
ordinary Source/Material controls and prior offscreen GPU control. The new evidence establishes
actual surface submission/present calls joined to content; the previous fixed-tolerance
offscreen oracle owns pixel comparisons. No screen capture, physical scanout, mixed-DPI movement,
device reconstruction, numerical production budget, foreign native execution or paused Editor/
Dialogue development is implied. Full R9b/R12/Foundation remains active.

### Ordinary Desktop Presentation Qualification

The independent main-thread example's first locked release check passes on Windows/Rust 1.95.0
in 27.11 seconds. The original public reader and presentation observer suffice: no new native
report API or render owner is needed. `captured.rs` shares inspection of the original stock
Shader digest, exact uniform bytes, semantic Image binding, one Sprite instance and original
decoded Image pixels between offscreen and windowed consumers. The desktop collector copies
only frame/content facts, retaining neither the transport batch nor a detached Source-generation
audit trail. The original native Publish consumer still takes and executes each batch.

Independent review finds one P1 and one P2 before native execution. A scripted `Wait` consumer
can stop advancing App frames once external events drain; maintenance wakeups deliberately do
not run gameplay. The fixture now uses the original `Poll` mode and keeps its actual-presentation
gate. The existing engine policy is unchanged. A panic leaving a callback can unwind a local
native session before an outer handler runs, invoking its intentional fail-stop Drop. Fixture
Update, configuration, capture and observer callbacks now retain the first panic inside their
own callback boundary, stop subsequent fixture work and request normal original-owner close.
Only after physical retirement is checked does the example resume that original panic. This is
fixture assertion containment, not a new production panic policy or arbitrary rollback promise.
The corrected check passes in 5.47 seconds; the first actual executable builds in 5m14s.

The initial three complete native runs pass on Vulkan/RTX 4090, DX12/RTX 4090 and DX12/Microsoft
Basic Render Driver. They present/complete 44/44, 44/44 and 40/40 frames respectively. Three
injected callback controls also return the expected original panic only after explicit physical
cleanup. Final source cleanup removes two new Clippy style warnings without changing those
contracts. The final executable rebuild takes 34.95 seconds. Its independently bounded process
sequence completes on November 15, 09:00:28--09:00:53 +08:00:

| Native mode | Observed adapter | Presented/completed frames | Represented states | Final result |
|---|---|---:|---:|---|
| Vulkan, fallback disabled | NVIDIA GeForce RTX 4090 | 60/60 | 12 | Complete seal and clean retirement |
| DX12, fallback disabled | NVIDIA GeForce RTX 4090 | 96/96 | 12 | Complete seal and clean retirement |
| DX12, fallback requested | Microsoft Basic Render Driver, CPU | 64/64 | 12 | Complete seal and clean retirement |

Every run performs eight explicit commits and three refusals through actual files and the
original Scene owner. Each representative has an unforgeable surface submission/present receipt
joined to its exact Runtime, App frame, original render capture, window, surface generation,
Host and epoch, followed by its matching completion sample. Rejected source retains the original
Scene incarnation and last-good Material/Shader/Image facts in a newly presented frame. The final
Done capture replaces the earlier same-count `(8,3)` representative before its completion is
matched. One-time observer sample batches are consumed once; the overlapping seal report is not
recounted. All runs finish with tracking enabled, zero pending/buffered/lost samples, zero pending/
lost resource retirements, the original target `NativeDestroyed`, no provider/active surface,
all 11 original watcher workers joined, and the physical event loop transferred exactly once.

Presentation and native-notification counts are fixture observations, not fixed event counts or
production budgets. The logged durations are original App-frame-start to GPU-completion facts,
including queueing and other work. The differing cold/warm runs do not establish GPU-only time,
source-edit latency, frame-rate performance or a comparative speedup. Pixel comparison and late
first submission of old CPU inputs remain the separate qualified offscreen workflow; this
CPU-only native observer does not inspect window pixels or establish scanout/user visibility.

The final Update, capture and observer failure processes each run with explicit DX12 software
selection, reach an earlier actual native presentation and exit 101 after verifying the original
physical pump recovered, native target destroyed and watcher joins before World destruction.
Their `native source fault cleanup` marker is required alongside that exit; abort/nonzero/timeout
alone cannot qualify retirement. Each parent invocation bounds and reaps only its original
example process at 180 seconds. No timeout, abort, abandoned process or replacement owner is used
as a passing result. Unfinished cleanup retains the original owner/project until process exit
and remains a failure.

The full independent Material package passes 14/14 with Desktop support compiled in
`e14cdab1-0ae8-49d2-b49c-bcf856f942a0` (2.271 seconds after a 1m34s release build). This also
preserves headless capture when Desktop is compiled but not requested. The affected original
offscreen trajectory passes Vulkan 1/1 in `a85df528-3e79-4baa-8e87-5e18135c94c0` (1.249 seconds)
and DX12 1/1 in `76718376-3230-4bf3-b7e9-bdcd57ec39c1` (1.777 seconds), keeping its fixed
pixel tolerance, old capture replays, two-generation Source limit and retained-output cleanup.
Final independent all-target correctness Clippy passes in 6.19 seconds with only existing root
warnings. The locked Linux `x86_64-unknown-linux-gnu` ordinary-example cross-check passes in
2m43s; it does not execute Linux native watching, surface presentation or failure cleanup.

Four scoped API/implementation/review agents are closed. Bounded rereviews close the P1/P2
corrections and find no remaining P1/P2 in exact frame/Scene/surface joins, representative/Done
handling, completion/seal accounting or failure retirement. The existing Windows CI lane adds
the ordinary DX12 software example and three original callback failure controls; the module lane
type-checks the example independently. Workflow/Python syntax and local execution are separate
from remote CI, which is not run or published in this unit. Editor/Dialogue work stays paused;
full R9b/R12/Foundation and numerical production costs remain open.
Root and independent format checks, affected TOML/lock/workflow and embedded Python syntax,
added local document links/fragments and staged/working whitespace checks pass. The independent
lock and dependency versions/sources are unchanged. Native runs, cross-checks and Cargo use the
original target serially with one build job; no clean, remote publication or branch/main movement
is performed.

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
