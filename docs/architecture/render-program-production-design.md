# Production Render Program And Typed Feature Integration

Date: 2026-10-31. Status: selected implementation design for step4 of the
[render ownership transition](render-host-ownership-design.md). Public names below are sketches.
The [finite prerequisite in Accepted ADR0094](adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md#finite-captured-wgpu-programs)
admits the bounded native program; broader policy/history/stock integration still follows below.
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
request explicit Host reconstruction. A captured source/processed input retains the original
definition and exact reconstruction inputs; building it cannot consult a current policy or assets.
Installation identity and producer-local instance identity remain independent of Rust type and list
position. Before exposing withdrawal, distinguish closing new capture admission from the retained
execution admission already issued to an accepted capture. Existing Feature-instance Drop closes
execution admission, so dropping an old binding at route replacement cannot implement draining.
Producer close still revokes execution, even while old inputs retain reconstruction/code custody.

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

## Implementation Order And Deletion

1. Port the finite resource admission/compiler into the production backend with hostile owner,
   semantic, initialization, ordering and budget cases. Keep it internal while pairing typed
   captured operations with exact native adapters and actual Host import/output custody.
2. Integrate texture/buffer execution and publication through the existing Host. An independent
   package runs blur, a chained effect, raw/normalized statistics and vertex histogram with
   actual readback. It imports no private fixture executor or core package branch.
3. Add exact View/instance/history ownership and coherent groups, then move stock Sprite/UI and
   Runtime/Editor/image-only consumers through the same contribution boundary. Delete central
   payload fields, package lowering matches, quad-only prepared containers and combined submit
   facades after their consumers use the replacement.
4. Qualify alternate policy, source/processed capture interleaving, independent streams, retained
   old outputs, native failures/loss/close and clipped Nara-drawn composition. Preserve exact UI,
   input, READY and native Editor evidence. Update accepted contracts and retire displaced private
   experiment implementations only with the complete result.

The [parent unit](../plans/2026-10-31-render-composition-and-host-execution-plan.md) owns the complete
acceptance and measurement limits. A production compiler, public blur or static registration
alone does not close R8/R9, promote the full proposal or finish the foundation Goal.
