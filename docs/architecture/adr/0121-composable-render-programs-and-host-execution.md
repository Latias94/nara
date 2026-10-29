# ADR 0121: Composable Render Programs And Host Execution

**Status**: Proposed
**Date**: 2026-10-23
**Revised**: 2026-10-30 after bounded contract review; still Proposed
**Owner**: Render semantics, concrete domain features, WGPU execution and native target Hosts
**Would Replace In Part**: ADR 0017, ADR 0094, ADR 0105
**Would Refine**: ADR 0012, ADR 0032, ADR 0040, ADR 0054, ADR 0078, ADR 0096, ADR 0106

## Context

Current immutable captured packets, one submitter and exact target authority are useful
invariants. They do not imply that only stock Sprite/UI work may be composed. Current central
payload and encoder know those domains; an independent two-pass effect cannot supply complete
work through an admitted public path. The requested integrated/extensible engine needs a real
third-party rendering seam, not only larger material fragments or private first-party labels.

## Proposed Direction

Compose a code-provided Render Program with typed domain Features over an explicit finite
resource-dependency graph. Features capture owned inputs and declare logical resource versions,
reads/writes and final consumers. Validate initialized inputs, conflicts and dependency cycles
before execution. This is the preferred full-workflow design, still subject to the experiments;
physical aliasing, pass fusion, multi-queue scheduling and a universal GPU allocator are not
initial requirements.

Design shared/branching and cross-view dependencies before freezing the interface. Compare viable
ordered-plan and slot controls on those workflows to expose costs and falsify the graph's premises;
minimum initial code is not the primary selection criterion. Phase implementation capability while
keeping identity, dependency and ownership semantics coherent for foreseeable engine evolution.
Use pinned Bevy and Godot as primary architecture references rather than treating a historical
RenderGraph API or the absence of one as the desired result. Dependency declarations do not derive
meaning from registration/insertion order; initial execution can use deterministic serial
topological order. The core design covers texture and buffer resources with sampled, attachment,
storage, copy, uniform, vertex/index and indirect access. Distinguish supported read-write storage
from illegal feedback rather than treating every buffer use as storage.
Each logical write declares either discard initialization or preservation from an exact predecessor
version. A fresh allocation does not inherit predecessor contents. Initial preserving writes use a
distinct allocation plus explicit initialization/copy; physical reuse requires proof that no other
live read, export or history needs the predecessor. In-place mutation is not an implicit write mode.
This also applies to attachment `Load` and storage read-modify-write. Writes produce distinct live
outputs; initialization and physical aliasing obligations are explicit.
Initial image support may use bounded full 2D subresources and conservative whole-resource access;
reject unsupported use. A small viewport does not initialize an entire attachment, and `Load` reads
prior contents. Keep adapter-private buffers private unless they cross
the shared resource boundary. No general buffer/compute composition is implied by color images.
Define how imported/exported resources, storage/buffer accesses and temporal/history lifetimes
extend this same dependency model before freezing it. Their implementation needs matching concrete
consumers; an image tracer does not qualify them. This is not a second RHI or a requirement to
implement every scheduler/allocator optimization before the graph becomes useful.

Same-execution cross-View dependencies are graph edges. Cross-capture composition instead imports
an exact immutable output lease published by the Host. Selecting a latest-output policy resolves
once at composition admission; it cannot retarget an admitted consumer as the producer advances.
The first unavailable output yields an explicit unavailable result or caller-selected initialized
fallback. Paused producers keep their last admitted output. Closing a producer stops new work and
input authority; already retained outputs remain physically held through their actual consumers
and submissions. Device loss revokes old-epoch presentation authority. A cloned native TextureView
alone is not a pixel-version lease and cannot protect an output from later overwrites.

History imports a previous producer/View/Feature-instance/history-slot version and publishes a
successor; it does not create a cycle in the current graph. Admission pins each history input and
its expected predecessor revision. All slots advanced by one output validate and publish with that
output; an outdated successor cannot replace a newer history chain. Initial execution may serialize
each slot. Retry uses the pinned inputs or explicitly prepares a new candidate, never silently
switches to latest history. Failure or supersession before output publication does not advance
history; a later window-present failure does not roll back an already published producer output.
Repeated display of one capture never advances history. The effect defines initialization and
compatibility, including resize, policy changes and camera cuts versus compatible camera motion;
device loss invalidates GPU history. Per-capture graphs and exact output leases preserve independent
stream progress rather than requiring every World to provide a fresh frame in lockstep. Retained
physical versions remain tied to submission/completion custody.

Simple named effect slots lower into this graph; they must not become a second submission
mechanism. An open render ECS/schedule remains a comparison alternative, not an assumed complete
resource-lifetime proof. Do not revive ADR 0077's entire family/profile/recipe taxonomy.

An exact-WGPU Feature adapter can realize and encode against the declared WGPU version. It does
not receive target acquire/submit/present authority. Do not duplicate all WGPU descriptors as a
new Nara RHI. First-party Sprite/UI adapters use the same Feature boundary; adding a domain must
not require a central package-specific payload field or match arm.

Pair each immutable captured Rust type with its exact native adapter at registration, erasing the
pair only inside the Host. Registration, frame values and Host/epoch-local mutable state are
different owners. Ordinary callers install/select contributions rather than drive internal phases.
Exact native Device/Queue access can escape a borrow; target authority is a trusted-code protocol,
not a type-system sandbox. Withhold submission/finalization from the ordinary encoding surface
without building a universal GPU wrapper or behavior verifier.

### Identity, Wiring And Candidate Boundaries

The October 30 amendments define the following boundary contracts. Exact output leases,
predecessor-checked history, stream retirement and recovery remain the already specified basis.
These additions refine the proposal and report no implementation or qualification.

**Contribution lifecycle.** Each contribution is admitted under an explicit owner scope with a
registration identity and incarnation. Its descriptor, exact immutable capture/native-adapter pair
and capability requirements form one installed definition. Feature-instance identity additionally
distinguishes the owner's instances of that registration; Rust type and list position are not
instance identities. Initial composition may freeze registrations at Host construction. Before
an update/removal interface freezes, define withdrawal as stopping new admission and fencing the
old incarnation, while retained candidates keep the exact definition needed for realization,
encoding and cleanup. They never look up a replacement through a current registration. Removal
completes after actual retained/code/native uses retire through their owners; route removal alone
is insufficient. A successor is separately admitted against enabled capabilities, or requests
explicit Host reconstruction. This specifies lifetime boundaries, not dynamic loading, a native
ABI, hot unload or a universal mutable contribution registry.

**Instance and graph identity.** Feature instances have stable identities within their declared
owner scope, independent of captured Rust type, registration order and list position. Reordering
preserves identity; removal and recreation or incompatible replacement starts a new incarnation
unless the owner explicitly admits continuity under the effect's compatibility rules. Graph-local
resource handles belong to one candidate; each locally produced version has exactly one producer.
Foreign-candidate handles and duplicate producers reject. Cross-candidate use requires an explicit
exact-version import. These identities do not replace asset, source or product identity, View
incarnation or device epoch.

**Semantic wiring and compatibility.** Render Programs explicitly wire code-owned semantic inputs
and outputs to exact resource versions. Pre-UI Scene color, resolved color and the final UI
composite are not implicitly interchangeable because their physical descriptors match. Image
compatibility includes extent, format, sample count, subresource range, permitted accesses and
color/alpha meaning. Resolves and conversions produce explicit output versions. Missing required
outputs and unsupported resource combinations reject admission. Initial support may remain single
sample and bounded full 2D subresources while rejecting other combinations. Small typed output
structures and explicit wiring suffice; no global output-name resolver or RHI descriptor mirror
is required.

**Candidate publication.** Each candidate declares an output publication group and its complete
required dependency closure. Initially keep producer-output groups and Editor composition in
separate candidates. One Host drive may service several candidates; sharing resources or a
submission does not merge independently declared publication or failure domains. Content and
preparation failures reject affected candidates and their dependent work while independent
candidates continue; device/Host failures retain their domain-wide scope. A selected fallback is
a separately validated candidate with pinned initialized inputs. Failed preparation cannot publish
partial outputs or advance history. Existing presentation and interaction owners still decide
display and input authority; this does not require arbitrary partial-graph publication.

### Imported Content And UI Output Boundaries

**Captured content to graph input.** A domain's capture step may resolve a selected logical asset
through its actual content owner. It retains exact immutable content and any same-generation
reconstruction lease before GPU candidate preparation. Existing `PreparedImageSnapshot` is the
initial Image boundary; no universal content wrapper is required. Persistent Product association
is retained when present, but procedural content need not invent one. GPU realization/import
consumes that exact capture rather than querying current source, Product bindings or mutable World
state to recreate its meaning. Content ownership stays independent of GPU Host identity.

The Host admits the physical import into its own identity/device epoch, with exact resource
version, initialized range, extent/format/sample count, color/alpha interpretation and permitted
usages where applicable. A graph import retains those facts and physical custody through its
actual candidate/submission uses. Content publication or a later realization cannot retarget it.
Domain admission/revocation, retained readability, recovery eligibility and physical GPU completion
keep separate meanings. A new import must satisfy its current authority; existing admitted packets
retain their exact input under their original lifetime and epoch rules. Concrete content owners
still publish/retire content, and the Host still realizes device objects. The Material/native-family
publication successor in R7 must join this seam before its public interface freezes. This is not
a source resolver, generic GPU registry or second RHI.

**Graph output to UI composition.** An Editor or Runtime UI adapter imports an exact Host-issued
output lease, retaining producer/View/capture/output version, physical compatibility, color/alpha
meaning and Host/device epoch. Its owned render candidate/composite packet retains that lease;
an opaque UI equality key alone carries no custody. `UiImageId` and the neutral `UiFrame` keep
their existing identity-only image meaning. Nara's custom-drawn Editor is free to use its own
drawing vocabulary without receiving raw target acquire/submit/present authority.

The display occurrence separately owns its destination rectangle, clipping, DPI/coordinate
transform and interaction policy. Composite admission joins these exact facts with the output's
producer mapping/picking facts; two panels can display one output with different placements.
An effect that cannot preserve/provide a valid mapping explicitly disables precise picking.
The presented composite receipt, not the output's texture identity, grants input authority.
Initial realization may use direct sampling or an explicit copied version; record that choice,
copy cost and completion/retirement custody. A producer's next output cannot overwrite a retained
version. On unavailable input, preserve only an authorized compatible last-good presentation or
reject explicitly; device loss cannot preserve old-epoch authority. No lookup of the latest output,
source or asset may reconstruct an older UI capture.

### Execution Ownership

A runner/Editor-local Host owns GPU initialization, device/queue, targets, submission and
recovery. Runtime Worlds produce captured stream data. An explicit stream may close while other
streams and the Editor keep the Host alive. Offscreen GPU is an explicit profile distinct from
content-only Headless. A dedicated render thread is optional deployment work, not a prerequisite;
native/browser-local affinity remains explicit rather than blanket Send+Sync.

Renderer-policy replacement reuses this Host; whole-Host replacement is a different privileged
integration. Device requirements close before initialization, with supported/requested/enabled
capabilities distinguished. A Feature/package is not permission to create a second submitter.

Late contributions must fit enabled capabilities or return an explicit reconstruction requirement
without resetting healthy streams. Check each actual surface; qualify offscreen initialization
without a dummy window. Keep ordinary supervised project-process Play separate from local producer
sharing; this proposal does not introduce cross-process GPU-object transport.

### Failure And Retirement

Publish a validated plan plus required realizations as a complete candidate. Keep last-good only
within a valid compatible epoch. Host identity and device epoch bind every physical object and
asynchronous result; loss revokes old presentation authority. Uploaded content can reconstruct
from retained exact captures or an immutable processed-content lease that can decode that same
generation. Recovery eligibility is a separate lifetime from GPU completion: releasing the last
reconstruction input ends that eligibility. Never reconstruct an old generation by looking up the
current asset or mutable source. Initial policy can retain CPU projections; permanent full CPU
copies are not part of every future resource contract. Source and processed inputs share this
boundary. GPU-generated/history content resets, and external content reacquires authority.

Submission, image publication, window presentation and physical GPU completion are separate.
Targets have independent lifetime and finalization; presenting one window cannot be rolled back
because another fails. Exact native Rust adapters remain fully trusted code, not a sandbox.

Stream retirement stops its admission and terminalizes queued interaction work, then retains its
submitted prefix until physical completion while other streams continue. Whole-Host queue-empty
drain does not supply that stream-local completion contract. Stream/view incarnation, policy,
target generation and device epoch remain separate axes, joined by existing receipt owners.
Bound per-stream pending/in-flight work and cumulative Host reservations explicitly.

The first reusable transparent-color output uses linear premultiplied accumulated color, with
explicit source/shader alpha semantics and captured shader clocks. This is an R9a tracer contract,
not a universal asset or HDR policy. Nara-owned Editor drawing consumes the shared Host lifecycle
without inheriting Runtime UiFrame vocabulary. Image effects preserve the admitted display/picking
mapping or supply a replacement/disable precise picking; a color receipt alone cannot prove an
arbitrary shader's inverse.

## Alternatives

- **Permanent stock plan:** smallest implementation but centralizes each new rendering domain.
- **Named slots:** can completely implement the two-pass effect with private scratch and ordered
  chaining; compare retained original output and cross-package composition before rejecting them.
- **Typed Features with a bounded logical resource plan/graph:** leading family; compare ordered
  and graph forms where actual dependencies distinguish them, with explicit ownership and
  combination costs, without requiring a general GPU compiler.
- **Fully open render ECS/schedules:** broad Rust control, but more sequencing/resource burden on
  every author. Compare with the same external effect before final Interface selection.

## Admission Experiments

1. An independent package contributes the same separable Scene-color blur through the explicit
   resource-graph candidate and viable slot, ordered-plan and open-schedule controls, retaining
   crisp Runtime UI. Run in Runtime, Editor Scene View and window-free real-GPU offscreen rendering
   without core package branches. Retain original color for a before/after view and chain a second
   independent effect. Branching/shared and cross-view consumption must challenge the preferred
   graph's dependency model and ergonomics rather than decide whether to include that candidate.
2. A second code-provided renderer replaces Stock2d policy using the same execution Host.
3. Two producer streams feed two windows and one image target; retiring one stream preserves
   the others. Run them at different rates, pause one and retain an exact older output while the
   other continues. Inject device loss, reject old realizations/results and finish Ready or
   Unavailable. Release current captures while retaining the selected recovery input; reconstruction
   must use that generation or report unavailable when its input was explicitly retired.
4. Reject uninitialized reads, cycles, incompatible resources and unauthorized target actions.
   Retain exact captured content while a newer domain projection is published.
5. Match native/platform verification to the actual changed seam and record unqualified targets.
   No unmeasured latency, throughput or memory improvement is part of this decision.
6. Read back transparent composition/alpha-zero controls; replay captures with the Host clock
   advanced. Exercise a Nara-owned clipped drawing tracer with Scene-image import and a popup,
   resize/DPI, stale mapping and presentation failures. This does not qualify the full Editor UI.
7. Add compute luminance statistics into a storage buffer and consume that result in a custom-drawn
   histogram, including a storage-to-draw-input path. Preserve raw statistics while a successor
   normalizes them, so a comparison consumer detects accidental predecessor overwrite. Challenge
   resource kinds, cross-view consumers and order-independent declaration before freezing the
   core; a future buffer enum case alone is not extension evidence.
8. A bounded accumulation effect pins history inputs through failed realization, dropped candidates,
   retry and out-of-order completion. Verify exact predecessor admission, coherent output/slot
   publication and camera-cut reset without requiring complete TAA or reprojection.
9. Two Views each install two instances of the same accumulation Feature. Reorder Views and
   instances, then remove/recreate one instance. Live histories remain associated with their exact
   instances and late old-incarnation work cannot attach to the replacement. Reject foreign-candidate
   handles and duplicate local producers; cross-candidate reuse requires an exact-version import.
10. Explicitly wire a pre-UI Scene-color consumer while a same-descriptor final composite is also
    available; registration order cannot retarget it. A multisampled attachment cannot satisfy a
    single-sample input without an admitted explicit resolve. Reject an unsupported resolve path at
    admission; this negative case does not qualify MSAA rendering.
11. Keep an Editor popup compositing an exact older Scene output and another producer advancing while
    the Scene effect's second pass is pending or rejected. Its output group and history cannot
    publish partially. Any selected fallback is separately validated; independent candidates keep
    progressing and their presentation/input outcomes remain truthful.
12. Withdraw a contribution with retained captures, a pending realization and submitted work, then
    admit a replacement. New admission refuses the withdrawn incarnation; old work never resolves
    the replacement, actual cleanup keeps its required definition alive, and unrelated streams
    continue. Initial static registration does not qualify runtime replacement or hot unload.
13. Import a captured Image through the same content-to-Host seam from both source and processed
    content, then publish a successor and retire live stores. Old admitted packets retain their
    exact generation; new imports require current authority. Display one exact Scene output in
    two clipped UI placements and reject stale input/composite mappings independently. Exercise
    the selected copy/direct-sampling path and device-epoch refusal without requiring a complete
    custom-drawn Editor toolkit.

Shader/material artifacts and shared View envelope details are separate owner designs in
[reassessment D15/D16](../foundation-architecture-reassessment.md#d15-shader-material-and-pipeline-resources).
They should not be smuggled into this proposal as universal descriptors or claimed implemented.
Their minimum target/color/time facts co-develop with this seam. The additional retained-capture,
interface-rejection and late-realization interleaving belongs to
[R9b acceptance](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r9-material-artifacts-views-color-and-domain-render-locality)
under the existing declaration-generation contract; it follows the selected seam and relevant
immutable shader inputs rather than gating the first R8 effect. The
[source/design review](../../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
records the compared upstream implementations, physical-resource and retirement gaps, phased
deletion and remaining experiments. This revision does not promote the proposal to Accepted.
