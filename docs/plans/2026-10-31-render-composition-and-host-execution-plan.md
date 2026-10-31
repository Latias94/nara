# Render Composition And Host Execution

Date: 2026-10-31. Status: private CPU/DX12/cost comparison, internal device ownership and original
UI result/logical-close prerequisites evidenced; complete production replacement remains in development.
Baseline: `a0995e7b65288ae67a5786b5312e6166b679d171`.
Parent: [foundation R8/R9a](2026-10-23-foundation-architecture-rebaseline-plan.md#r8-external-render-feature-and-host-local-execution).
Candidate: Proposed [ADR0121](../architecture/adr/0121-composable-render-programs-and-host-execution.md).
Current Accepted authority remains ADRs0017/0032/0040/0078/0094/0105/0111 until explicitly reconciled
for a selected production replacement.

## Complete Outcome

An independently authored typed Feature renders the same separable Scene-color blur in Runtime,
Editor Scene View and real-GPU offscreen execution, keeping Runtime HUD crisp. Retain original
Scene color for before/after composition and chain an independent second effect. The same model
supports compute luminance statistics in a buffer, a preserved successor normalization and a
storage-to-vertex-input histogram. Two Views install two accumulation instances each; independent
producer rates, exact old outputs, history predecessors and output publication remain coherent.
An alternate renderer policy reuses the execution Host. Stock Sprite/UI pass through the same
selected contribution seam, displacing central per-package payload/encoder growth.

This outcome includes the foundation plan's independently paced streams, two windows/image target,
producer-local retirement, loss/recovery, clipped Nara-drawn composition and exact display/input
join. It does not require a complete Editor toolkit, temporal antialiasing, a new RHI, physical
aliasing, pass fusion, parallel GPU execution or a mandatory render thread. Completing a private
graph model or an isolated GPU blur is not completion of this outcome.

## Ownership And Initial Comparison

The source-backed incumbent at this baseline constrains the replacement:

| Existing owner | Reuse and required replacement |
| --- | --- |
| `WgpuRenderBackend::ensure_device(window_id)` | Reuse epoch/loss/caches, separate device initialization from surface configuration. Production adapter selection currently requires a window surface. Test-only `with_probe_device` is not Ready/epoch/loss qualification. |
| `WgpuFramePayload` and `prepare_target_draw_with_canvas_cache` | Retain stock immutable captured inputs and prepare/encode behavior as the oracle. Replace package-specific Sprite/UI payload fields and collection/encoding matches through the selected contribution boundary. |
| `WgpuEditorViewportTarget` | It currently owns one reusable texture and validates only the last submitted receipt. An old TextureView clone does not retain old pixels; exact cross-capture outputs need separately held physical versions. |
| `WgpuEditorGpuRetirement` | Preserve flush-before-close and real physical completion. Whole-queue-empty drain remains the final Host close boundary, not a producer-local completion proof while other streams continue. |
| Root desktop observation and Runtime driver port | Move GPU storage only with its first-present/UI facts/surface retirement/close observations. Resource absence must not impersonate physical completion after ownership moves to a runner. |
| Per-View Editor admission and exact target receipts | Retain their independent source/frame clocks and target/source/Host/epoch identities; they do not yet prove multi-stream output retention or local physical retirement. |

Existing Image cache submission callbacks retain exact imported Image versions; they do not supply
immutable generated-output leases. Existing telemetry completion counters are observations, not
resource-retirement custody. The first production window-free entry must exercise normal epoch,
loss, execution and close ownership without a Winit event loop, provider or dummy surface.

Keep exact immutable domain captures and the existing target/presentation/physical-completion
owners. The comparison workspace lives at `tests/fixtures/render-composition`; its graph and
native harness are private disposable test modules. It does not introduce an engine public API
or bypass current production authority through a hidden extension hook.

Use exact wgpu types at this private native boundary, without mirroring Device/Queue/descriptors.
Texture and buffer versions have candidate-local ownership; cross-candidate use is an explicit
exact-version import. A local output has exactly one producer. Reserve resources independently of
operation declaration order, then compile dependencies with a deterministic serial topological
order. No graph meaning depends on insertion order or a same-shaped resource being latest.
Whole-resource, single-sample full-2D tracking is the initial admitted scope; unsupported ranges
and resolves reject. Access roles include sampled/attachment/storage/copy and uniform/vertex/index/
indirect buffers. Explicit clear/zero or preservation from a compatible exact predecessor
initializes a fresh output. Preserving writes use distinct storage and copies, never mutate a
still-live predecessor. Adapter-private scratch need not become shared graph resources.

The GPU owner retains physical resources and handles submission/finalization. Typed captures pair
with exact native adapters; ordinary feature encoding receives no target acquire/submit/present
authority. Native Rust remains trusted, so this is a protocol, not a sandbox. Candidate output
groups publish only after successful native submission admission in the still-current epoch;
fully encoded/ready candidates are not published output. Initial groups use separate encoders,
validation scopes and submissions. A shared native batch failure rejects every affected output,
even when their logical groups are independent. History imports previous exact
producer/View/Feature-instance/slot versions; publication checks their pinned predecessors with
the complete output group. Empty slots retain reset/revision state and completed captures retain
at-most-once advancement across reset/retry. Displaying the same output again does not advance history.

Keep input-port meaning separate from initialization-copy compatibility. An explicit copy can
initialize a new final-composite version from same-shaped/color-compatible Scene bytes; that
composite still cannot satisfy a Scene-color input. Raw counts and normalized frequency buffer
ports also differ despite equal sizes/usages. Shared readable/exported/history attachments must
Store their final contents; an initial Clear cannot justify later reads after Discard.

The comparison must retain viable controls: named slot with two private passes, explicit ordered
resource plan, and ordinary schedule composition. They share actual kernels, workload, initialization,
retention and presentation obligations with the preferred graph. Quantify required author
choreography, core changes, retained allocations and measured admission/encoding cost. A control
is not disqualified merely because its lifetime proof lives in an explicit owner instead of a graph.

## Workload And Measurement Entry

Start numerical correctness at64-by64 pixels in linear premultiplied RGBA32Float, including
transparent and alpha-zero controls. Use four luminance bins with preserved raw integer counts and
a normalized buffer actually bound as vertex input. Float readback tolerance starts at1e-6 for
clear/copy and is chosen per shader operation against the same CPU oracle before accepting an
effect. Integer membership/statistics and graph order are exact. Texture and buffer extents,
operation/resource counts and modeled CPU/GPU bytes are bounded before allocations.

The first private workload ceiling is128 operations,128 resources,512 accesses and16MiB of encoded
readback; these are test bounds, not production engine limits or performance results. Establish
the viable ordered/slot baseline on the actual adapter before selecting a comparative cost gate:
128 warmups and1000 measured CPU admission/encoding samples, with setup/driver wait and GPU timings
separate. Record hardware/toolchain, p50/p95 and allocator/retention model. Do not claim a speedup,
copy reduction or budget win from source review or worker counts. Use
`delta p95 > max(10 percent of control p95, 2 times the control repeat variation)` as a pause-and-
attribute trigger before interface freeze, not an already measured result or an automatic choice
against the graph. Unexpected overhead pauses
interface freeze for attribution; it does not weaken identity or lifetime invariants.

## Ordered Work And Exit Evidence

The [production ownership design](../architecture/render-host-ownership-design.md) orders the
internal device-domain extraction, owned capture/result and runner transition, immutable output/
producer-prefix custody, and graph/Feature migration. It keeps actual production consumers
together rather than introducing a second offscreen backend. Its internal first step can proceed
while comparative costs are measured; public graph/Feature interfaces do not freeze before the
measurement and complete consumer gates below.

The first [optimized CPU-stage comparison](../benchmarks/2026-10-31-render-composition.md)
completed three processes with 36,000 samples, equal fresh outputs and reused ECS topology. Large
repeated P95 variation prevents a stable speed or tail-cost winner. It does not block the private
device-owner extraction or qualify the production/public boundary below.

The internal [coherent device-owner prerequisite](../knowledge/engineering/verification/2026-10/2026-10-31T064658Z-coherent-wgpu-device-ownership-and-render-cost-comparison-qualification-18760d4fdea7435b90fb9bd2bee64510.md)
is implemented at `48cf9ce` through existing stock Runtime/Editor execution. Its complete Ready
aggregate, private surface-less acquisition, loss isolation and cleanup/epoch behavior have owner
and selected native evidence. The [original-result/logical-close successor](../knowledge/engineering/verification/2026-10/2026-10-31T082117Z-original-ui-result-custody-and-logical-close-qualification-6e1965c6428841ec9138088bef20f90c.md)
is implemented at `8e9d840`: original nominal batch storage, retained receivers, Root UI/Input
settlement and CPU WGPU observations have final owner/neutral and selected native evidence.
Continue with the complete owned native session, capture/result execution and runner migration;
GPU ECS storage and public offscreen/Feature/output workflows are still open.

1. Implement the private graph/access model and hostile cases together with a real window-free
   GPU readback harness. Compare actual pinned Bevy/Godot mechanisms and challenge the proposed
   identity/initialization/history contracts. Record unresolved design questions before public APIs.
2. Execute the complete shared/branching image, compute/buffer, accumulation and two-View/two-instance
   workload against graph and viable controls. Keep exact old resource versions alive while
   successors execute. Reject foreign handles, duplicate producers, cycles, uninitialized inputs,
   wrong semantic wiring and unsupported MSAA combinations. Measure the selected workload.
3. Integrate exact output/history publication groups, source/processed captured-input interleaving,
   independently paced streams and Nara-drawn clipped composition. Prove designated interactive
   display completion versus passive mirrors with existing UI/input owners; passive repaint cannot
   acquire producer interaction or settle an old permit. A failed candidate affects its dependent
   output group while independently admitted candidates continue.
4. Select and reconcile the consumed Accepted contracts, then implement responsibility-owned
   production APIs and Host-local execution. Move stock Sprite/UI through that seam and remove
   displaced central fields/matches, closed-topology bans and obsolete experiment controls.
   Preserve exact target, Ready, supervised Play and physical-close authority.
5. Run independent/public Runtime/Scene View/offscreen and policy consumers, real two-window/image
   and producer-close/loss/recovery traces, relevant native Editor qualification and focused serial
   tests/Clippy/formatting. Record honest support limits, update current guides/ledger, commit and
   locally fast-forward main. Keep the complete foundation Goal active.

Production readiness requires the complete above consumers, not test-only private graph success.
R9b durable shader/material declarations and R9c measured geometry retention remain separately
ordered work but must fit the selected resource/binding/access contract. Reuse exact CPU snapshots
or same-generation processed decode leases for recovery; never reconstruct an old capture through
current source/assets. Do not weaken this boundary to make a smaller tracer pass.

## Private Comparison Evidence

The [first executable comparison](../knowledge/engineering/verification/2026-10/2026-10-31T060722Z-private-render-graph-buffer-history-and-viable-controls-comparison-on-dx12-d6aca1996b2342659bd07933b33e0e64.md)
records 61 unique CPU tests, seven actual DX12 tests and one subsequent actual submission-failure
test. Image/buffer kernels, independent numeric controls, logical history groups and accepted-
prefix custody are exercised within the fixture's bounded scope. These results close neither the
measurement entry nor the production steps above. They select no cost winner, publish no engine
API and do not promote Proposed ADR0121.

## References

- [Render source/design review](../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
- [Research reconciliation](../research/2026-10-30-engine-direction-reconciliation.md)
- Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
  `81ab961f91066a63bcbf3ec340962bb1a1ada558` under read-only `repo-ref`.
- [Native Editor qualification](../verification/native-editor-qualification.md)
