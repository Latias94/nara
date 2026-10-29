# ADR 0121: Composable Render Programs And Host Execution

**Status**: Proposed
**Date**: 2026-10-23
**Revised**: 2026-10-29 after source/design review; still Proposed
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
storage and copy access, distinguishing supported read-write storage from illegal feedback.
Writes produce distinct live outputs; initialization and physical aliasing obligations are explicit.
Initial image support may use bounded full 2D subresources and conservative whole-resource access;
reject unsupported use. A small viewport does not initialize an entire attachment, and `Load` reads
prior contents. Keep adapter-private buffers private unless they cross
the shared resource boundary. No general buffer/compute composition is implied by color images.
Define how imported/exported resources, storage/buffer accesses and temporal/history lifetimes
extend this same dependency model before freezing it. Their implementation needs matching concrete
consumers; an image tracer does not qualify them. This is not a second RHI or a requirement to
implement every scheduler/allocator optimization before the graph becomes useful.

History imports a previous producer/View/Feature-instance/history-slot version and publishes a
successor; it does not create a cycle in the current graph. Define initialization, advancement at
producer-output publication and invalidation on resize, camera/policy change and device loss.
Repeated display of one capture never advances history. Per-capture dependency graphs and Host
composition preserve independent stream progress rather than requiring every World to provide a
fresh frame in lockstep. Retained physical versions remain tied to submission/completion custody.

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
from captures, GPU-generated/history content resets, and external content reacquires authority.

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
   the others. Inject device loss, reject old realizations/results and finish Ready or Unavailable.
4. Reject uninitialized reads, cycles, incompatible resources and unauthorized target actions.
   Retain exact captured content while a newer domain projection is published.
5. Match native/platform verification to the actual changed seam and record unqualified targets.
   No unmeasured latency, throughput or memory improvement is part of this decision.
6. Read back transparent composition/alpha-zero controls; replay captures with the Host clock
   advanced. Exercise a Nara-owned clipped drawing tracer with Scene-image import and a popup,
   resize/DPI, stale mapping and presentation failures. This does not qualify the full Editor UI.
7. Add compute luminance statistics into a storage buffer and consume that result in a custom-drawn
   histogram. Retain a previous preview for comparison. Challenge resource kinds, cross-view
   consumers, history scoping and order-independent declaration before freezing the core; a future
   buffer enum case alone is not extension evidence.

Shader/material artifacts and shared View envelope details are separate owner designs in
[reassessment D15/D16](../foundation-architecture-reassessment.md#d15-shader-material-and-pipeline-resources).
They should not be smuggled into this proposal as universal descriptors or claimed implemented.
Their minimum target/color/time facts co-develop with this seam. The
[source/design review](../../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
records the compared upstream implementations, physical-resource and retirement gaps, phased
deletion and remaining experiments. This revision does not promote the proposal to Accepted.
