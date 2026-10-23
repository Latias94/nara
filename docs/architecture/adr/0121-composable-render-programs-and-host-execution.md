# ADR 0121: Composable Render Programs And Host Execution

**Status**: Proposed
**Date**: 2026-10-23
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

Compose a code-provided Render Program with typed domain Features. Features capture owned inputs
and declare versioned logical color/depth reads/writes and final consumers. Validate initialized
inputs, conflicts and dependency cycles before execution. Begin with a finite resource plan;
physical aliasing, pass fusion, multi-queue scheduling and a universal GPU allocator are not
initial requirements.

Simple named effect slots may lower into this plan; they must not become a second submission
mechanism. An open render ECS/schedule remains a comparison alternative, not an assumed complete
resource-lifetime proof. Do not revive ADR 0077's entire family/profile/recipe taxonomy.

An exact-WGPU Feature adapter can realize and encode against the declared WGPU version. It does
not receive target acquire/submit/present authority. Do not duplicate all WGPU descriptors as a
new Nara RHI. First-party Sprite/UI adapters use the same Feature boundary; adding a domain must
not require a central package-specific payload field or match arm.

### Execution Ownership

A runner/Editor-local Host owns GPU initialization, device/queue, targets, submission and
recovery. Runtime Worlds produce captured stream data. An explicit stream may close while other
streams and the Editor keep the Host alive. Offscreen GPU is an explicit profile distinct from
content-only Headless. A dedicated render thread is optional deployment work, not a prerequisite;
native/browser-local affinity remains explicit rather than blanket Send+Sync.

Renderer-policy replacement reuses this Host; whole-Host replacement is a different privileged
integration. Device requirements close before initialization, with supported/requested/enabled
capabilities distinguished. A Feature/package is not permission to create a second submitter.

### Failure And Retirement

Publish a validated plan plus required realizations as a complete candidate. Keep last-good only
within a valid compatible epoch. Host identity and device epoch bind every physical object and
asynchronous result; loss revokes old presentation authority. Uploaded content can reconstruct
from captures, GPU-generated/history content resets, and external content reacquires authority.

Submission, image publication, window presentation and physical GPU completion are separate.
Targets have independent lifetime and finalization; presenting one window cannot be rolled back
because another fails. Exact native Rust adapters remain fully trusted code, not a sandbox.

## Alternatives

- **Permanent stock plan:** smallest implementation but centralizes each new rendering domain.
- **Named slots only:** ergonomic for one effect, fragile for multiple intermediate dependencies.
- **Typed Features with a bounded logical resource plan:** leading candidate; explicit ownership
  and combination costs, without building a general GPU compiler.
- **Fully open render ECS/schedules:** broad Rust control, but more sequencing/resource burden on
  every author. Compare with the same external effect before final Interface selection.

## Admission Experiments

1. An independent package contributes two-pass Outline/Blur with an intermediate texture to
   Runtime, Editor Scene View and real-GPU offscreen rendering, without core package branches.
2. A second code-provided renderer replaces Stock2d policy using the same execution Host.
3. Two producer streams feed two windows and one image target; retiring one stream preserves
   the others. Inject device loss, reject old realizations/results and finish Ready or Unavailable.
4. Reject uninitialized reads, cycles, incompatible resources and unauthorized target actions.
   Retain exact captured content while a newer domain projection is published.
5. Match native/platform verification to the actual changed seam and record unqualified targets.
   No unmeasured latency, throughput or memory improvement is part of this decision.

Shader/material artifacts and shared View envelope details are separate owner designs in
[reassessment D15/D16](../foundation-architecture-reassessment.md#d15-shader-material-and-pipeline-resources).
They should not be smuggled into this proposal as universal descriptors or claimed implemented.
