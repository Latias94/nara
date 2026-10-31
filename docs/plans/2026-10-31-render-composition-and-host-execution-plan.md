# Render Composition And Host Execution

Date: 2026-10-31. Last reviewed: 2026-11-01. Status: private CPU/DX12/cost comparison, native
ownership, immutable outputs, finite typed programs, original policy capture and captured
View/history transactions qualified in their recorded scopes. Complete Source/Processed capture
interleaving, independent native streams, clipped numeric drawing and physical-pump handoff now
have their own scoped qualification. Public stock-stage Scene effects and the editable
Nara-drawn Inspector remain open.
Baseline: `a0995e7b65288ae67a5786b5312e6166b679d171`.
Parent: [foundation R8/R9a](2026-10-23-foundation-architecture-rebaseline-plan.md#r8-external-render-feature-and-host-local-execution).
Candidate: Proposed [ADR0121](../architecture/adr/0121-composable-render-programs-and-host-execution.md).
Current Accepted authority remains ADRs0017/0032/0040/0078/0094/0105/0111 until explicitly reconciled
for a selected production replacement.

ADR0094 now explicitly admits the finite captured texture/buffer program prerequisite through the
existing Host, with exact typed definition/instance/capture pairing and original submission custody.
The [production integration design](../architecture/render-program-production-design.md) orders its
stock migration, complete Image preflight barrier, history/policy and custom composition. The
[public guide](../guides/render-programs.md) describes the implemented prerequisite only; it does
not promote Proposed0121 or replace the complete outcome below.

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
| Concrete GPU retirement (now `WgpuGpuRetirement`) | Preserve flush-before-close and real physical completion. Whole-queue-empty drain remains the final Host close boundary, not a producer-local completion proof while other streams continue. |
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
Its [owned native desktop successor](../knowledge/engineering/verification/2026-10/2026-10-31T105632Z-owned-native-desktop-execution-and-post-host-presentation-qualification-4db921de03c94598b2ee20d7e82508c5.md)
is implemented at `37ca8bb`: one retained Runtime/Host/window/pump session, authenticated CPU
capture/results, actual post-Host UI/READY/startup, finite telemetry seal and maintenance without
simulation. GPU ECS storage, scheduled submit and the obsolete World retirement operation are
removed. Minimal code-first and Source/Processed products share the owner. Continue with exact
immutable outputs, original reconstruction inputs and producer-prefix custody; public window-free,
external Feature, alternate policy and complete graph/output workflows remain open.

The [production image-output replacement](../knowledge/engineering/verification/2026-10/2026-10-31T121248Z-immutable-render-outputs-owning-producers-and-original-native-prefix-qualification-1b4de7f4305740698abba87b589f9459.md)
is qualified under the selected
[immutable-output contracts](../architecture/render-host-ownership-design.md#selected-immutable-output-replacement).
It replaces mutable viewport storage with exact retained outputs, makes producer admission an
owned irreversible lifetime, and holds an exclusive Host/producer borrow through final target
activation. Ordinary native output use gains Host-owned submission custody; raw interoperation
does not become the normal read-only consumer protocol. Actual qualification covers paint/upload failure,
OOM/Internal classification, callback-abandonment recovery, retained byte limits and same-capture
retry alongside A1/A2, designated D/passive M and producer-local close. Ordinary478, Root445,
selected native27 and independent public3 pass in the linked scope. Continue the production
graph/Feature seam; the complete resource/history/policy workload remains open.

The [shared concrete drain](../knowledge/engineering/verification/2026-10/2026-10-31T084653Z-shared-concrete-gpu-queue-retirement-qualification-ab68d8adf25f406d9222df6b3b2fcc70.md)
is extracted at `96bd0bf` with its actual Editor consumer and adapter/native retirement evidence.
The Runtime Host must still capture that owner before first use; extracting the shared mechanism
does not establish the native session or producer-local retirement.

The [late-result admission successor](../knowledge/engineering/verification/2026-10/2026-10-31T091550Z-late-presentation-retirement-admission-and-exact-step-outcomes-817901432b684d17b69c2f87f4427408.md)
at `525cd6f` separates consumed retirement from presentation and closes the actual first-Stop and
unavailable-close-port windows, including a real pending-Step outcome. This strengthens the same
prerequisite; actual native run/session relocation remains the next complete production unit.

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

The [shared stock preparation prerequisite](../knowledge/engineering/verification/2026-10/2026-10-31T144610Z-shared-stock-native-preparation-and-whole-candidate-preflight-qualification-01b79b28279b48618dd19ec377ad2cc8.md)
adds actual complete preflight, typed Sprite/UI adapters and one native allocation/upload owner.
Current Runtime/Editor drawing uses its shared guards and original prefixes, and production-program
tests execute both stock adapters. Their ordinary capture/policy consumers still require migration
before central payloads, combined image facades and quad-only prepared containers can be deleted.
Continue that actual replacement next; tested adapter presence does not close the complete unit.

The [ordinary stock image successor](../knowledge/engineering/verification/2026-10/2026-10-31T153613Z-stock-image-consumers-and-declared-worldcanvas-through-shared-programs-qualification-f6cfe4185aad459587db367d807aeb60.md)
now routes actual Scene, Canvas, embedded Game and independent image consumers through that program
transaction. Mapped fixed-world Game uses graph-owned world/resolved/final versions; initial target
storage has exact shared custody and survives pure preflight refusal. Continue ordinary Runtime/
window and typed policy capture installation next, removing the central payload and combined
facades with their remaining consumers before qualifying general policy/history replacement.

The [ordinary Runtime/window successor](../knowledge/engineering/verification/2026-10/2026-10-31T165610Z-runtime-window-programs-and-synchronous-producer-world-permission-qualification-c05b8d29194543bc9fb11fbbc79414de.md)
now uses that same staged program and final surface-consumer submission. Original capture/result,
UI/Input, READY, telemetry seal and native close remain with Root's owned run session. World
presentation is producer-local, synchronously revoked at Drop, and displaced only when another
successful presentation replaces its physical window. The concrete Backend and old mutable
window/Canvas executor are absent from the public/production path. Continue typed policy capture
installation and actual alternate-policy consumers before deleting the central capture payload,
combined image facades and remaining private numerical oracles. View/history groups, complete
independent streams and custom-drawn composition retain their full outcome above.

The current typed-policy integration replaces that central payload and facade family. Its
independent consumer must select a non-stock scalar renderer through normal ECS capture and
ordinary Runtime window presentation, then reconstruct the original input on an image Host.
Review also requires a concrete public UI-sidecar capture route for that selected policy, a
final-image UI binding verified against actual encode reports before publication, recoverable
policy-content rejection, and complete fixed engine metadata accounting. These are acceptance
refinements inside steps4-5 above, not a new render roadmap. The
[typed policy qualification](../knowledge/engineering/verification/2026-10/2026-10-31T192756Z-original-typed-render-policy-capture-and-complete-consumer-qualification-df7f9377f51142b78060b53617c800a1.md)
records their implemented result, scoped ordinary/native evidence and final external consumers.

The selected production slice implements the design's
[View history transaction](../architecture/render-program-production-design.md#next-production-unit-view-history-transactions):
two Views with two independently replaceable accumulation instances each, pinned history bases
and explicit slot/output bindings, published together through the existing Host transaction.
Retry retains its original predecessor basis. Reorder, one-instance replacement, stale/reset
siblings, public budget refusal and independently progressing producer B have an external numeric
oracle. Candidate custody uses one opaque caller-owned value with original bindings and budget
guards; the existing Host retains final publication and native-prefix authority without a
candidate registry or another Runtime in-flight barrier. This refines the remaining history work
rather than reopening the graph decision. Its
[production qualification](../knowledge/engineering/verification/2026-10/2026-10-31T210034Z-captured-view-histories-and-original-submission-cleanup-production-qualification-5c061376803446c0bff1a95495ca55c7.md)
separates the actual four-chain numeric/Runtime consumers, owner corrections and native evidence
from the private comparison. Its native-composition successor below supplies the selected
source/processed interleaving and independent streams; the complete Scene-effect workflow and
Nara-drawn editable Inspector remain open.

Include cleanup after both synchronous publication and later original-prefix completion. The
candidate/input may already be gone when native polling releases the last Capture or Bindings.
Prove reverse flat release, remaining-resource cleanup, explicit fault observation and preserved
successful results without misclassifying destructor failure as callback abandonment. Host
maintenance transfers stable observations and removes a completed prefix in one decision, including
when a public clone completes on another polling thread. Qualify producer-wide active-owner capacity
separately from each candidate's binding/metadata allowance; neither test may loosen the other bound.

The complete Source/Processed capture consumer also exposes a native lifetime mismatch: the
incumbent desktop action creates a physical Winit event loop per logical Runtime, while Winit
admits only one such loop per native process. Replace that incidental coupling with an explicit
move-only physical pump returned only after complete native-session retirement, under ADRs0078/
0084/0110. Reuse it for sequential ordinary Source and Processed desktop actions, retain their
four original stock captures, then interleave those captures on the same offscreen Host after
live stores and owned backing retire. Preserve each mode's existing witness and residency rules;
neither a private transport drain nor a synthetic direct-App substitute qualifies this workflow.
Exercise incomplete-close refusal and failed startup recovery as well as the successful handoff.

The [native-composition qualification](../knowledge/engineering/verification/2026-10/2026-10-31T225517Z-native-composition-physical-pump-handoff-and-retained-stock-input-qualification-f0cd93c17a3c47fa905993cee07e6c5c.md)
records that production replacement at `2d9f251`: public window/read-use scopes, bounded receipt
preparation, heap-owned stable policy slots, a read-only original-input reader and explicit
physical-loop transfer through ordinary desktop actions. The independent consumer completes real
Source/Processed publication and retained-input replay, two-window/image progression and clipped
premultiplied drawing with numeric readback. Windows/DX12, owner and selected Editor gates pass
within their recorded scopes. A release-only measurement was incorrectly selected by a broad
debug command and refused its prerequisite; no performance result or fully green broad run is
claimed. This checkpoint does not close the complete outcome or promote Proposed0121.

The next [public stock-composition unit](../architecture/render-program-production-design.md#next-production-unit-public-stock-composition)
must close the remaining actual API gap: stock world/Canvas/UI lowering is currently fixed inside
one build, ordinary collectors require its concrete installation, and Scene/Canvas construction
hardcodes that definition. Compare shared typed stages with full replacement using the same
external blur/second-effect packages across Runtime, Scene View and offscreen, with crisp HUD and
exact final UI binding. Expose explicit world-raster versus resolved-target effect inputs rather
than freezing one implicit pre-UI pixel space. This follows the selected graph/Host model and
does not add another executor, string-slot registry or full0121 qualification claim.

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
