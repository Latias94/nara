# Production Render Program And Typed Feature Integration

Date: 2026-10-31. Last reviewed: 2026-11-01. Status: selected production program, history and
native-window integration, followed by the public stock-composition design, within step4 of the
[render ownership transition](render-host-ownership-design.md).
The [finite prerequisite in Accepted ADR0094](adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs)
admits the bounded native program and original typed policy capture. Their implementation evidence
is tracked in the [ledger](adr/implementation-status.md). Captured View/history prerequisites
have scoped production qualification; the public stock-composition successor is in production
replacement and has not completed its independent/native qualification.
Proposed [ADR0121](adr/0121-composable-render-programs-and-host-execution.md) remains Proposed
until its complete production consumers qualify the selected contract.

## Existing Owners

Keep the qualified device domain, immutable output/display leases, producer admission, original
submission-prefix custody and one-shot Runtime completion owners. A graph is a declaration and
admission mechanism inside those owners. It does not create a second device, submission loop,
output publisher or retirement protocol. The current private comparison remains an independent
oracle during migration and is deleted or reduced to controls after the production path covers it.

The graph and exact native adapters live in `nara_render_wgpu`. The backend-neutral `nara_render`
continues to own captured Views, logical targets and coordinate mappings. Native descriptors use
WGPU types directly; a parallel descriptor family or new RHI is unnecessary.

## Finite Resource Model

Each candidate owns its resource handles. Texture and buffer versions retain an unforgeable
candidate identity and a local index, checked before indexing. Native resource descriptions keep
exact WGPU descriptors plus explicit color/alpha interpretation and a code-owned semantic type.
The declared usage flags limit graph access; physical storage must contain those usages and may
support additional Host operations. That superset grants no additional graph access. Extent,
format, subresources, semantic identity and logical descriptor agreement remain exact.
The first implementation tracks whole single-sample, single-mip 2D textures and whole aligned,
unmapped buffers. Other ranges, multisampling and resolves reject at admission.

Resource declarations precede or follow operation declarations without changing meaning. Every
local version has exactly one writer. Reads and preservation copies create dependency edges;
stable operation keys select the serial order of simultaneously ready operations. Foreign handles,
duplicate writers, missing writers, cycles, feedback and incompatible accesses reject before any
native work. Resource/count/metadata limits are admitted before graph and derived scratch allocation.

Typed semantic ports distinguish pre-UI Scene color, effect results and final composite, and raw
integer statistics from normalized frequencies. Physical copy compatibility does not relabel a
semantic input. A compatible explicit copy may initialize a new semantic output. Every write has
an explicit full clear, zero or exact predecessor-copy initialization; shared outputs are stored.
A viewport/scissor does not narrow that initialization proof. Preserving writes allocate distinct
storage while a predecessor is retained.

Imports are actual Host-admitted immutable leases or exact captured content, not caller assertions
that an arbitrary native handle is initialized. The graph model has no public initialized boolean.
The executor validates original Host/epoch, descriptor, usages and interpretation and retains the
actual import through candidate and submitted use. Source and processed content retain their
original snapshot/reconstruction owner and never resolve current assets during realization.

## Typed Contribution And Native State

Install an immutable definition that pairs one captured Rust type with its exact native adapter
factory. Definition, producer/View instance and Host/epoch-native state have separate identities.
The pair is erased only inside the Host. An admitted operation retains the exact definition and
capture, so it cannot resolve a replacement implementation through a current registry. Native
state belongs to that instance and epoch; it is not an ECS Resource or mutable captured payload.

An effect package provides a small build function over typed input/output handles. It can declare
two blur passes or a compute/draw chain using its installed adapter. It need not inherit a central
quad type or add a core package enum case. Per-pass captures may share the original immutable
domain capture. Required native features are known before device acquisition; requirements are
checked against the actual enabled device, not merely an adapter's possible capabilities.

Adapter preparation may create resources and enqueue uploads under the original Host reservation.
Encoding receives the exact declared resources and command encoder, with no target acquisition,
Queue submission or presentation operation. Raw native Rust remains trusted code. Native prepared
work and code/capture owners stay retained through their actual submission, including failure.
Typed reports describe encoded work; they do not independently authorize output or UI completion.

The initial Host can seal its installed definitions at construction. Any withdrawal/update entry
must explicitly fence new admission, retain old definitions through cleanup and refuse replacement
lookup for old work. A successor requiring different enabled capabilities requests Host
reconstruction. Static registration alone does not qualify runtime replacement or code unload.

## Stock Policy And Shared Preparation

Keep clear/load semantics in captured `ExtractedView`; do not invent a second clear DTO. The stock
policy orders world Views, optional world-canvas composition, selected Scene effects and one
full-resolution UI overlay. UI is not repeated per View and does not enter a low-resolution world
canvas. An alternate policy uses the same graph and execution owner.

Sprite and UI keep their existing immutable domain captures. Their adapters may share private quad
pipeline/texture helpers, but the core prepared operation container accepts arbitrary native
Features. Preserve complete working-set preflight and exact Image/material/glyph identity across
all Views and UI before uploads. Splitting adapters must not multiply shared budgets or move
rejection after partial upload merely for implementation convenience.

The current per-operation prepare/encode loop cannot by itself establish this complete working-set
barrier. The preparation prerequisite adds one preparation-free `preflight` hook and a candidate-owned private
stock Image collector before any local native allocation or state creation. Its public context
exposes enabled limits, with no Device, Queue, state or upload entry; stock helpers privately
register the exact already-lowered immutable batch slices. After visiting every capture, the
shared stock resource owner calls existing `preflight_image_residency` across all those slices.
This preserves main and material-extension snapshot deduplication, complete-set rejection and
live-old-allocation deferral. It does not expand that method into a claim of complete glyph,
fallback or per-frame upload preflight. Collectors are rebuilt for each attempt rather than
encoded as a persistent "checked" flag in a capture.

The same borrowed stock cost calculation covers all native automatic custody entries and engine
records for captured batches/material extensions, before any local resource allocation or upload.
Program admission joins that cost to its remaining metadata allowance; current stock Runtime/Editor
consumers use the same calculation during policy migration. Native preparation reserves the complete
known custody table before work. External Features' arbitrary extra retention stays a separate
fallible dynamic request. Prefix observation also reserves its slot before uploads/submission,
including the window backend, so successful native work cannot precede its observer allocation.

Shared quad pipeline/Image/glyph resources remain a private Host/epoch-owned realization helper.
Actual mutable Feature state remains per instance; an adapter with no such state uses `State = ()`.
Uploads through the common helper retain destination/guard before enqueue. Candidate instance
reservations refund on preparation/encoding failure and commit together immediately before the
native submission attempt, not at the end of an individual Feature's encoding. Preserve the
structured Backend error at the program-to-Host boundary so stock residency deferral and device
failure do not collapse into a generic Feature content rejection.

One private `NativePreparation` carries the exact native pair, retained-byte budget and original
reservation through both paths. Image, glyph, fallback, instance and material payload allocations
reserve before creation and keep the shared guard with actual native storage. Cache hits return an
exact guarded binding/view and retain that use; per-Image completion callbacks are removed. The
public Host resource-byte names describe this complete logical payload accounting. Pipeline/driver
overhead remains excluded, and no residency/RSS improvement follows from the accounting change.

Stock cache age uses the Host maintenance frame, never an independently paced producer capture
counter. Candidate failure discards only native instance states it actually touched. A pure
preflight refusal or failed local allocation cannot evict unrelated original instance caches.

UI drawing packets remain separate from move-only backend submission permits. Replayed packets
and new permits are not a zip relation. The UI adapter/bridge owns exact packet/target facts and
the original completion rendezvous. Generic prepared work has no mandatory UI field. Scene,
Canvas, embedded Game and ordinary desktop all consume the same installed adapter definitions;
Game retains its logical window routing when its physical execution target is an image.

### Selected Stock Migration Shape

One stock Sprite operation owns the admitted ordered world Views and one Scene-color writer.
Its immutable capture flattens exact batches once and retains each View's viewport, load operation
and prepared-buffer range. Preparation uses one instance reservation and shared native resources;
encoding preserves the ordered attachment transaction. A later View Clear still clears the entire
attachment. Serial Views do not require a full-target preservation copy per View, and the generic
graph executor does not acquire a quad-only sequence container. The existing finite View limit
also bounds this capture's engine metadata, including a clear-only world.

Fixed WorldCanvas rendering declares its world texture in the graph. A typed composite Feature
samples that exact input into the initialized physical-resolution output, sharing the incumbent
shader and mapping/scissor implementation. It does not allocate a second hidden world texture.
UI writes a distinct FinalComposite version through an explicit initialization copy and runs once
at physical target resolution. A policy without UI uses ordinary initialization/output declarations;
compiling the UI package does not activate it.

The next policy seam pairs a code-provided immutable Capture with its original build definition.
Stock fields belong to that policy, rather than the central frame, connection or Host. Concrete
capture systems read already-extracted domain data through normal ECS parameters; the policy
interface does not receive a World. Producer-local CPU instances hold typed Feature instances and
stable View/effect identity. Candidate ports are created only during Host-local construction, so
capturing a frame does not require native resources or preexisting graph handles. Erasure stays
inside the connection and Host. This is a selected integration shape, not a qualified public
policy interface or a new generic section registry.

Policy requirements close over its original Feature definitions before device acquisition. Instance
creation cannot expand that declared request. Late installation must fit actual enabled facts or
request explicit Host reconstruction. Check original device health before policy or Feature
capability admission: actual device loss remains `DeviceLost`, distinct from a healthy device's
refusal to broaden its enabled request. A captured source/processed input retains the original
definition and exact reconstruction inputs; building it cannot consult a current policy or assets.
Installation identity and producer-local instance identity remain independent of Rust type and list
position. Before exposing withdrawal, distinguish closing new capture admission from the retained
execution admission already issued to an accepted capture. Existing Feature-instance Drop closes
execution admission, so dropping an old binding at route replacement cannot implement draining.
Producer close still revokes execution, even while old inputs retain reconstruction/code custody.

### Selected Policy Capture Replacement

`WgpuRenderPolicy` pairs an immutable `Capture` with candidate-local immutable CPU `Bindings`, a closed
device request, capture validation and program construction. `WgpuPolicyDefinition<P>` retains
the exact implementation and request. `WgpuPolicyInstallation<P>` owns admission of new captures;
its non-reused installation identity is separate from definition identity and Rust type. Closing
or dropping it refuses new captures while accepted `WgpuRenderInput` values retain the original
definition and execution lease. Producer close remains irreversible. No registry lookup can
retarget an accepted input to the currently selected policy.

Capture systems use normal ECS parameters and one finite capture stage. The generic frame has
no Sprite, UI drawing or future domain sections. `WgpuStockCapture` owns those concrete inputs;
explicit stock contributors populate it. The UI completion sidecar continues to own one-shot
permissions separately from replayable capture data. Native Runtime and embedded Game transport
the same selected input; neither reconstructs a stock policy from optional fields at presentation.
Scene and authoring Canvas bind their definitions when their original candidates are captured.

`WgpuPolicyBindingContext` resolves original logical Feature declarations into actual owners only
within the policy's original closed request. It grants no Device, Queue, World or target authority.
The Host resolves CPU instances by producer and original logical declaration, with a finite bound
and reclamation after declarations and accepted candidates release their leases. Each `bind`
constructs a new immutable membership snapshot; only complete publication updates the latest
snapshot. The private heap slot tables retain stable indices and their full admitted capacity
after entry reclamation. Account for that retained capacity and exact original-submission payload increment,
then admit predictable custody wrappers before their allocation. Native Feature state remains separately epoch-owned.
An original submitted reservation retains its exact policy capture, definition and CPU instances
through actual GPU completion, including failure. Closing capture admission cannot prematurely
Drop Feature instances and impersonate draining.

`WgpuPolicyBuildContext` supplies the captured packet and actual destination format. A policy
constructs candidate-local ports and returns `WgpuPolicyProgram` with an exact published final
texture and explicit observations. The Host validates the original source/frame, native format,
extent, interpretation and Feature ownership before execution. An image adaptation preserves
source, frame, logical Views and fixed-canvas facts, changing only an equal-sized execution target.
Accepted CPU input remains usable for explicit reconstruction on a new Host; it contains no old
native instances.

Runtime UI completion uses a concrete `WgpuUiProgramBinding` to the original UiFeature operation,
instance and capture. In the finite selected contract, that UI operation writes the exact selected
final texture. An independent offscreen UI branch cannot authorize completion for another image.
Validate this relationship before native work. After encoding, match the actual original UiReport
and its retained capture-facts identity before output publication consumes its watermark. Prepare
completion capacity before native work, then match permits by packet identity plus logical target.
This is a presentation obligation, not a generic render-domain
section registry. A different renderer can omit UI; it cannot settle pending UI permits without
their exact binding. Compiling a stock domain does not install its contribution.

The complete consumer for this replacement includes normal ECS capture, ordinary Runtime windows,
Scene/Canvas/Game, a source/processed product, independent image reconstruction and an independently
authored alternate policy through the same capture/final-target selection. Remove the old central
payload, domain collection stages, submitter names and combined image facade family with those
consumers. Installation withdrawal proves accepted-work draining; it does not qualify resident
native code unload or a persistent renderer-recipe format.

The pinned Bevy/Godot source comparison supports separating extracted CPU input, policy wiring,
per-instance state and device-local realization. It does not supply this accepted-capture drain
guarantee: Bevy's pipeline-cache requeue follows current shader handles, while Godot's effect
lists/callbacks are live lookups and delayed RID destruction establishes physical lifetime rather
than old-capture execution admission. Both close device features/limits before device creation.
Use their ownership concepts without substituting a current cache lookup for the original Nara
definition. Reference pins remain Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`; this source comparison is not performance evidence.
The inspected owners are Bevy's `crates/bevy_render/src/render_resource/pipeline_cache.rs`
(`set_shader`/`remove_shader`), and Godot's
`servers/rendering/storage/compositor_storage.cpp` plus
`servers/rendering/renderer_rd/pipeline_cache_rd.cpp` (`setup`/`update_shader`).

Use one private staged program transaction for ordinary Runtime/window, Editor Scene/Canvas/Game
and independent image output. It performs one producer admission, complete preflight, preparation,
native validation and original submission-prefix binding. An Editor target's already-reserved
texture may realize a final local version only through a Host-private binding: it remains an
uninitialized local resource with one writer, rather than a public initialized import. Program
outputs and the exact image receipt retain the same actual allocation/byte guard. Native handle
clones alone cannot create a second charge or discharge the original responsibility.

The acquired surface is the Host's final consumer. Its final draw and the program's commands join
one original reservation and native submission attempt before present. This avoids requiring
surface copy usages or treating an acquired surface as initialized imported content. Keep output
publication, surface presentation, exact UI settlement and physical completion separate. Do not
implement stock migration by chaining public submit_program and the incumbent image/window path;
that would retain multiple admission/submission authorities.

Runtime world-presentation guards belong to the actual producer, not a backend-wide last-frame
slot. Target loss, incompatible window/canvas facts, producer close and device loss revoke their
original permissions. Another producer may keep presenting and retiring independently. Bind the
Host's native window authority only after it actually admits a surface; a missing first target
cannot reserve a foreign registry. Construct exchange identity from the managed Runtime owner,
and keep the associated request opaque so a caller cannot substitute a replaceable World resource
or forge an owner generation through the driver-port input type.

The concrete UI bridge joins reports to permits by packet identity and original logical target,
validating uniqueness and result capacity before native submission. Replayed packets with no new
permit still draw; a fully clipped packet still has exact facts. The bridge never zips packets
with permits or puts a permit in a replayable capture. Runtime Presented still requires actual
surface presentation; embedded Game requires its designated exact display occurrence. Authoring
Canvas has no Runtime permit. Preserve the Root post-Host UI/Input join, first-present/READY,
telemetry seal, accepted target/picking facts and original close/result custody together.

Migrate the actual desktop driver, Scene and Canvas presenters, embedded Game candidate/presenter,
and independent image consumer before deleting their displaced fields and facades. The first
alternate policy must cross the same capture selection and final-target adaptation; another
direct submit_program blur alone does not qualify renderer-policy replacement.

## Publication, History And Close

One candidate owns one complete output publication group. Initially encode and submit independent
groups separately, each with native scopes and original-prefix custody. A failed batch cannot
publish a subset just because outputs have different logical group labels. Validate native errors
and epoch before atomically publishing the group's outputs and all associated history slots.

History is keyed by producer, View, Feature instance/incarnation and a code-owned slot. Tickets
retain exact predecessors, revision/reset facts and capture watermark, including empty slots.
Final publication revalidates those facts; sibling/outdated tickets refuse without rebinding.
Reset and regroup do not erase completed-capture deduplication. Output slot identity, not vector
position, joins a submitted resource to its history destination. Retirement reclaims inactive
metadata after its outstanding users retire; experimental lifetime registration caps are not a
production reclamation policy.

Submission, output/history publication, display occurrence presentation, UI settlement and physical
completion remain distinct. Graph execution uses the existing original reservation before upload,
failed-attempt flush and callback-abandonment recovery. Producer close freezes its actual prefix
while other producers progress; final Host close drains the whole original queue. Graph output
retention never restores a retired interaction permission.

Submission retention is flat and bounded with iterative reverse release. Graph budgets include
its actual value/slot records; automatic entry counts refuse before program allocation. Scoped
uploads admit destination custody first, and a refused retained value leaves the custody lock
before its destructor runs. A valid large candidate cannot create a recursive Arc cleanup chain
merely by repeating exact imports.

### Next Production Unit: View History Transactions

This selected owner slice follows typed policy migration; its
[production evidence](../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md)
records the bounded implementation and remaining full-unit scope. It extends the existing Host
transaction and exact resource imports. No separately mutable history manager sits beside
`submit_program`, and no caller-side commit follows already-published GPU results.

The first consumer has two captured Views with two accumulation instances per View. Each instance
keeps its own exact predecessor and code-owned slot. Captured View identity includes its source
and runtime incarnation; camera ordering, operation order, target extent and vector position are
not identity. Camera identity alone must not preclude separately identified subviews of one
source camera. The Host's current `views` table is producer target state, not a per-camera history
table. Keep the capture's logical effect incarnation separate from its Host-specific Feature
binding; reuse the existing non-reused instance identity for the latter. Add bounded
producer-local instance reconciliation within the policy's original closed device request, so
retiring or replacing one effect preserves the other three histories without reinstalling the
entire policy. Reconciliation must distinguish live membership, capture admission and accepted
candidate execution; dropping a latest-map entry cannot revoke a still-accepted candidate's
original instances. The immutable CPU capture fixes its original topology, definitions,
incarnations and parameters even before Host preparation; preparation resolves those facts into
an immutable binding snapshot retaining the actual Feature instance owners. Keeping only a shared
map or numeric instance IDs does not retain the instance's execution admission. Preserve explicit
Feature/producer close semantics rather than changing them as a hidden reconciliation detail.
A previously accepted input first prepared after a topology replacement or installation withdrawal
still resolves its original declaration; it cannot consult the current topology. Original policy
requirements close before capture admission, and selected Feature definitions must fit that
request. Preparation cannot discover new capabilities and silently enlarge it.
A temporarily absent or minimized View is not implicitly retired. Removing and readding the
camera component is a real lifecycle change even if its Entity survives.
Temporal topology retains `WgpuHistoryView` alongside its captured `RenderViewId`; the original
topology/input/candidate holds that concrete lifetime owner. Slot reclamation follows both the
View owner's and effect declaration's Weak leases. A shared static effect does not turn destroyed
Views into lifetime slot registrations. Reusing a copyable ID with another live temporal owner
cannot silently adopt its predecessor. This owner has no global registry or retirement mailbox.

Reconciliation prepares candidate membership without changing published membership. New instance,
capability or budget failure leaves the original group, last-good slots and older candidates
unchanged. Candidate-driven membership changes commit with publication. Explicit reset or
execution revocation may fence older candidates immediately, under their separate owner operation.

A Host-issued candidate pins the complete history basis before program construction: exact
predecessors, empty-slot revisions, reset incarnations, group membership and capture watermarks.
Explicit slot-to-resource bindings select its new history outputs independently of output order.
Retry executes that same pinned candidate; it cannot silently rebuild against current history.
An explicit new preparation may acquire another basis. Retained CPU input remains the separate
reconstruction authority on a new Host; retired GPU history cannot cross epochs as a valid import.
An accumulation capture alone cannot reproduce its old temporal output after losing its GPU
predecessor. This unit selects an explicit effect-declared temporal reset on a new Host/epoch,
with an empty-history reason and declared initialization. The resulting pixels belong to a new
lineage, not an exact reconstruction of the old accumulated output. Complete original CPU
history replay/checkpoints are separate unimplemented capability. Preserve non-temporal
original-input reconstruction without extending its promise to unavailable predecessors.

Validate the basis before native work and again at the existing final publication boundary.
Publish the complete output group, every history pointer and successful-capture watermark
together after native submission admission and epoch checks. Admit ticket/binding/result storage
and old-value retirement capacity before native work. Successful-capture deduplication belongs to
the source/history lineage, independently of current group, target and reclaimable slot records.
Reset, regroup and instance replacement do not clear it. A source handoff cannot restore an old
capture's execution eligibility: retain its lineage watermark or permanently retire that lineage.
A stale sibling, reset basis or explicitly revoked instance rejects the whole group.
A later surface-presentation failure does not undo successful GPU output/history
publication. Publication consumes the candidate, including when its exact output has not yet
presented; transport retries return the original result rather than accumulating again. Display,
UI settlement, READY and physical completion keep their existing separate owners.

Success cleanup also includes the original submitted custody's eventual final references. A
Capture or Bindings destructor can run during a later native poll after the consumed candidate
and input have gone. Isolate each flat retained entry at that release boundary, preserve reverse
dependency order and the actual completion fact, and retain an explicit cleanup observation.
Never convert that destructor fault into callback abandonment or lose another already-published
result because polling released an older capture. Genuine callback abandonment remains an
unsettled responsibility requiring its existing original-device recovery.

Select an opaque, caller-owned, move-only candidate with no public constructor, `Clone` or retained
`&mut Host` borrow. It owns the original input, complete program, actual binding owners, target
contract, history basis and RAII count/metadata reservations. Its Host/epoch/producer facts fence
execution without a Host candidate table, cancellation mailbox or second admission barrier. The
existing Runtime/Game in-flight owner retains it; Scene/Canvas use their existing presenter. It
never holds an acquired surface across calls. A retryable failure returns the same candidate;
success consumes it, and terminal rejection or Drop releases its uncommitted references and
reservations. Object lifetime does not promise that reset or close leaves it executable.

Do not let an unbound `WgpuSubmissionReservation` escape with the candidate. Create and bind that
original native responsibility inside one serial Host attempt, including its failure path. A
candidate may retain ordinary budget guards across retries; touched resources independently remain
with their original submitted prefix. This preserves conservative failed-attempt cleanup without
requiring Drop to route a cancellation back into a Host table. No second renderer API or
caller-side history commit is introduced.

If a transport retains a retry across calls, it retains the original capture and UI
sidecar too; settling that sidecar and later recreating its responsibility from cached input is
invalid. One-shot paths explicitly cancel instead. Policy `Bindings` and native `State` may
cache implementation data but do not advance authoritative history during build or preparation.

Initially one producer/capture owns one publication group; independent producer rates do not
require a multi-group scheduler. Metadata reclamation follows outstanding candidates, tickets,
outputs and native custody rather than a lifetime registration counter. The independent consumer
checks four distinguishable accumulation chains against a linear-float CPU oracle, changes View
and operation order, replaces one instance, rejects stale/reset siblings, injects public budget
pressure and keeps producer B progressing after A pauses or closes. Retained A outputs remain
readable. Owning native tests separately cover submission failure and device loss.

Pinned Bevy's `RetainedViewEntity` supports an explicit retained View identity; its TAA frame
parity is not a predecessor transaction for independently paced producers. Godot's named
`RenderSceneBuffersRD` storage and TAA initialization support local lifetime reasoning; copying
over history does not preserve Nara's independently retained exact versions. These are mechanism
comparisons at the pins above, not upstream defect or performance claims. Reconcile the selected
finite history contract in ADRs0040/0094 and neutral View facts in ADR0117 before implementing it;
full0121 remains Proposed.

## Next Production Unit: Public Stock Composition

The native-window/content/drawing consumer does not complete the ordinary external Scene-effect
workflow. At the unit's `3d7eb7b` baseline, `WgpuStockPolicy::build` owns the whole world/Canvas/UI graph. Its intermediate
markers and binding fields are private; Sprite lowering is crate-private. Runtime collectors
require the concrete stock installation, and Scene/Canvas constructors install that definition
directly. Public Features and an alternate scalar policy therefore do not yet let a normal
package add a Scene blur to those stock consumers without reproducing lowering or using private
imports. This is an explicit remaining capability, not an assertion that current supported stock
drawing violates its contract.

Compare a public typed stock-stage helper, used by the existing `WgpuRenderPolicy`, with full
policy replacement on the same complete consumer. Prefer shared stage lowering when it preserves
the existing complete preflight, ordered Views, statistics and final UI binding. The default stock
policy becomes the ordinary composition of those same stages; do not retain duplicate fixed
lowering. Stage definitions close their exact Feature requirements before device creation and
each original capture retains the selected definitions and parameters. A helper name or generic
shape is a design candidate until the real consumer qualifies it.

Expose actual world raster, optional Canvas resolution and final UI composition as independently
owned graph operations. World-raster color, resolved Scene color and final composite remain
distinct semantic ports with actual extents, transfer and premultiplied-alpha facts. An effect's
radius/coordinates explicitly select world-raster or resolved-target pixels. The illustrative
Canvas-then-effects order above is one wiring choice, not a universal fixed hook: a foreseeable
world-pixel effect and a target-pixel effect must not require another graph-model replacement.
Only the selected pre-UI Scene result feeds the final full-resolution HUD. Preserve the original
Scene as an exact separately exported version for before/after composition; never obtain it by
relabeling the final image or consulting a latest slot.

Complete the CPU and construction boundary with the stage helper. A public stock-capture handoff
must provide the collected immutable stock drawing and original optional UI sidecar under actual
backpressure. The composing collector captures its effect parameters and submits through existing
`capture`/`capture_with_ui`, retaining any refused original sidecar. It need not understand private
`SpriteBatches`. Runtime recipe and Scene/Canvas construction must select the same original typed
policy and capture adapter. Picking/provenance remains with its original Editor owner, independent
of a mandatory downcast to `WgpuStockPolicy`. Reconcile ADRs0094/0105/0106/0111/0117 and the affected
0017/0032 stage wording before replacing those owners; full0121 remains Proposed.

The next independent consumer has separately authored blur and second-effect packages, normal
Sprite Scene content, actual Runtime and Editor Scene View, and image-only replay. Include optional
WorldCanvas, unchanged full-resolution HUD, original Scene before/after branching and an actual
final-UI-binding rejection. Reuse the qualified original-input, producer, candidate and native
submission owners. The existing comparison blur writes fixed `rgba32float` storage, so it cannot
be silently reused as a general stock-format shader. Use actual supported attachment formats,
explicit sampling/color semantics and a format-appropriate numeric oracle. A graph-only blur or
another manually assembled policy is not complete consumer evidence for this seam.

### Selected Construction And Capture Replacement

Accepted ADR0094's shared-construction refinement selects `WgpuStockStages` through the existing
policy interface. Its original declarations bind once through the candidate context. One
`WgpuStockProgramBuilder` owns the ordinary finite graph and accumulated stock observations:
`world` writes ordered Views, `resolve_canvas` explicitly consumes world-raster color when a
Canvas exists, and `finish` composes full-resolution UI over the caller-selected target-sized
Scene. Caller-authored typed operations use that same graph between these steps. The default
policy invokes these helpers directly, replacing its duplicated fixed graph body.

`WgpuWorldColor` and `WgpuSceneColor` distinguish the world raster from Canvas-resolved pixels.
Without a Canvas the caller finishes from its world version, avoiding an unnecessary physical
copy merely to rename a semantic port. An effect preserves the meaning of its chosen pixel space
while producing a distinct version. Untouched Scene versions may join the same output group for
before/after use. Stage bindings must belong to the original declaration; foreign bindings,
missing Canvas, target-shape/color mismatch and ordinary graph/output limits reject.

`WgpuStockCapturePlugin` separates stock collection from the default policy installation.
`WgpuStockFrameReader` moves the complete snapshot and optional original UI sidecar only for the
exact selected available capture route. Incomplete composition retains its original draft until
logical Close rather than silently replacing permissions. Ordinary composers submit through
`WgpuPolicyFrameWriter`; refusal returns the original sidecar. Scene/Canvas construction can use
a move-owned `WgpuStockCaptureAdapter` whose erasure covers one complete typed capture and grants
no World/native/presentation access. Existing product definitions select and fingerprint compiled
configuration. These owners and their independent effect packages now have
[complete stock-composition qualification](../knowledge/engineering/verification/2026-11/2026-11-01T013746Z-shared-stock-composition-original-captures-and-native-scene-effects-qualification-2f250ecb33b24b5091f28a141cd6b244.md)
through actual Runtime, native Scene View and offscreen consumers. The record retains failure
corrections and support limits; it does not promote full0121 or qualify the editable Nara-drawn
Inspector.

The construction comparison selects shared stages for stock-content effects: a full alternate
policy already fits the execution interface, but at this baseline it must either reproduce the
private Sprite lowering or omit normal stock content. Adding public extraction/quad internals
solely to make that control reusable would freeze a second drawing interface. The independent
consumer therefore replaces whole policy wiring while sharing stock lowering; the already
qualified non-stock scalar policy remains the complete-replacement execution control. This is an
ownership/authoring choice, not a measured speed claim or proof that every renderer should use
stock stages. All three real entry paths execute the same independent packages in the linked
qualification; no performance winner is inferred.

The concrete consumer also requires the exact post-UI output as a public typed input. The stock
marker is `WgpuCompositeColor`; the existing published-frame typed getter retains its original
meaning. `WgpuImageOutput` now keeps that selected program-resource owner rather than duplicating
its native handles, epoch and allocation guard. Its typed texture lease checks the same semantic
identity and imports through the existing graph path. This supports actual Editor image numeric
observation and Nara-drawn composition without a readback-only service, arbitrary port relabeling
or retention of the entire publication and capture.

## Implementation Order And Deletion

### Native Window Composition Consumer

After the selected View/history transaction, qualify two independently paced producers, two
actual native windows and an image-only composition under the original Host. The native frontend
owns platform windows and their safe providers; public exact-wgpu window scopes admit their
initialization and painting without exposing another acquire/submit/present owner. Remove the
former toolkit-specific hidden window scope/outcome vocabulary as its actual Dear consumers
migrate. Public window and image-composite receipts retain original Host/epoch, native target,
display clock and exact image occurrence; their constructors remain private. A composite receipt
proves sampling/presentation facts, not a widget mapping, interaction permit or monitor scanout.

Admit bounded image-display, unique-sample, extra-command and complete-receipt storage before
native work. Repeated sampling of one occurrence does not grow metadata. Prepare all receipt
joins before present and publish them only after successful native validation/presentation;
allocation or generation failure cannot turn an already-presented window into a missing result.
Painting failure or unwind still binds its original queued writes to submitted custody. Window
failure remains local to that attempt and does not roll back another window or producer output.
Existing one-shot UI completion continues to name its designated display occurrence.

Bounded producer policy/Feature tables keep their stable slot space in fallibly admitted heap
storage, outside inline producer-map nodes and native callback stacks. Pruning active entries
does not discount retained table capacity. Candidate slot indices remain stable, and final
publication still performs no table allocation or arbitrary old-value destruction.

The independent consumer lives at `tests/fixtures/render-composition-consumer`. It uses real
source/processed Image owners to retain original inputs across successor publication and live-store
retirement, checking actual old/new pixels through Host-owned readback. Its native window example
uses actual windows, exact retained display occurrences and independent capture/display schedules.
An image composition keeps an older paused/closed producer output while another advances. A clipped
Nara-drawn Scene/control/popup tracer checks ordering and linear-premultiplied pixels before adding
Editor widget/text behavior. Actual device-loss injection remains in owning native tests unless
the independent public consumer has an admitted loss route. Full editable Inspector, material ABI,
automatic shader inverses and unmeasured performance remain separate work.

### Ordered Replacement

1. Port the finite resource admission/compiler into the production backend with hostile owner,
   semantic, initialization, ordering and budget cases. Keep it internal while pairing typed
   captured operations with exact native adapters and actual Host import/output custody.
2. Integrate texture/buffer execution and publication through the existing Host. An independent
   package runs blur, a chained effect, raw/normalized statistics and vertex histogram with
   actual readback. It imports no private fixture executor or core package branch.
3. Move stock Sprite/UI and Runtime/Editor/image-only consumers through the same contribution
   boundary, then qualify original typed policy capture and a real alternate policy. Delete
   displaced central payload fields, package lowering matches and combined submit facades with
   their consumers. Test-only numerical controls remain scoped comparison evidence until their
   replacement oracle is complete.
4. Add exact View/instance/history ownership and coherent groups through the existing Host
   publication transaction, following the next-unit boundary above. Qualify source/processed
   capture interleaving, independent streams, retained old outputs, native failures/loss/close and
   clipped Nara-drawn composition. Preserve exact UI,
   input, READY and native Editor evidence. Update accepted contracts and retire displaced private
   experiment implementations only with the complete result.

The [parent unit](../plans/2026-10-31-render-composition-and-host-execution-plan.md) owns the complete
acceptance and measurement limits. A production compiler, public blur or static registration
alone does not close R8/R9, promote the full proposal or finish the foundation Goal.
