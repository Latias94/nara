# Render Host Ownership And Production Transition

Date: 2026-10-31. Status: selected implementation direction for the
[R8/R9a unit](../plans/2026-10-31-render-composition-and-host-execution-plan.md), subject to its
complete consumer qualification. Public API names below are sketches. This design does not
promote Proposed [ADR0121](adr/0121-composable-render-programs-and-host-execution.md) or claim
that the production renderer has moved out of the World.

## Problem And Owners

The device domain, producer capture clock, generated pixels, interaction completion and platform
window have different lifetimes. The incumbent stock backend combines their state. Moving only
Device and Queue into a new object would leave the singleton capture clock, writable viewport
texture and World-based retirement proof in place. Keeping a second execution path for offscreen
work would preserve those contradictions.

Use one concrete WGPU Host, owned outside the gameplay World by the runner, Editor or offscreen
consumer. Keep serial native execution initially. The World owns CPU capture and a finite typed
result endpoint; it does not own the GPU service. The same stock draw preparation/encoding serves
all three placements before the external Feature seam replaces its central domain fields.

| Owner | Retained facts | Explicitly separate facts |
| --- | --- | --- |
| Device domain | Non-reused owner identity, monotonic epoch, actual Instance/Adapter/Device/Queue, loss signal and enabled capabilities | Surface configuration, producer frame clocks, CPU content generations |
| Producer | Non-reused incarnation, capture/admission state, bounded pending work, actual submitted prefix and presentation revocation | Host lifetime and other producers' progress |
| Immutable output version | Issued output identity, exact native texture/view, format/extent/usage, initialized range and transfer/alpha facts | Target configuration generation, latest-output policy, CPU reconstruction input, interaction permission |
| Reconstruction input | Exact immutable CPU geometry/content/material/glyph/time facts or same-generation decode custody | Device epoch, submitted GPU lifetime and move-only UI completion |
| Display occurrence | Exact selected output, placement/clip/DPI/mapping and interactive/passive role | Producer advancement, output publication and native resource retirement |
| Submission custody | Original epoch, native accepted prefix, physical uses and actual completion observation | Telemetry, logical output publication, scan-out and window destruction |
| Platform target | Owning handle source/control lease, configuration and acquired texture | World resource membership or an output lease's lifetime |

These are responsibility boundaries, not a requirement for one public wrapper type per row.
Prefer private concrete aggregates and reuse existing nominal receipt owners.

## Device Construction

First extract the existing device fields into one private device-domain aggregate used by both
stock Runtime and Editor code. Ready owns a complete Adapter/Device/Queue/loss-signal set. An
Instance may exist before Ready to create an initial compatible surface. Independent optional
device and queue fields must not describe impossible partial Ready states. Cleanup retires that
aggregate, while the owner identity and monotonically consumed epoch remain with its lifetime.
Native handle equality cannot establish cross-context identity: the device-isolation test actually
observed equal WGPU device IDs after recreating the Instance. Test the issued owner/epoch and an
old-device loss followed by healthy successor execution instead of assuming unequal native handles.

The internal acquisition operation accepts an optional compatible surface. A surface-less request
uses the same fallback policy, request validation, loss callback, epoch assignment and native
error path as a surface-compatible request. A later window is checked against the selected
adapter before configuration. Incompatibility reports a target-local refusal or explicit Host
reconstruction requirement; it does not silently replace a healthy device used by other streams.

This extraction is an internal prerequisite. A test calling it does not qualify a public offscreen
Host, enabled-capability protocol, device recovery or runner-local storage. Those require the
normal initialization/execute/close consumer below. Retain ADR0078's async browser contract; a
synchronous native implementation does not grant permission to block a browser event loop.

## Captures, Native Execution And Results

The installed stock contribution systems produce complete immutable CPU captures. Split their
replayable drawing data from one-shot UI completion obligations before transfer. The endpoint
has one bounded original candidate identity and retains every refused or untaken obligation.
Capture identity, realization attempt and published output identity are separate. A failed
realization may retry its pinned capture; it must not advance a successful-output watermark.

The platform runner services the endpoint after managed frame work. It supplies the concrete
local presentation driver, which owns the Host; Winit remains independent of WGPU. This is a
narrow frame/target-retirement integration, not a service locator or a trait mirroring the GPU.
Keep the current guarded owned driver ports for taking captures and returning exact results.
The result operation coordinates the domain's existing frame/status/UI owners without granting
an arbitrary caller a mutable World callback.

Current desktop first-present verification runs in `CoreStage::Last` and reads the backend ECS
resource. After Host relocation it must observe the exact returned presentation result at the
post-Host boundary. Do not drive another simulation frame or rerun arbitrary Last systems merely
to publish READY. UI acceptance similarly joins returned original submission facts before its
existing one-shot commit; an absent GPU resource cannot suppress required facts. Root observers
consume a read-only result projection, while native custody remains local to the Host.

Advance the post-Host observer at every runner service boundary, including maintenance without
a new capture or successful presentation. Its optional presentation input can be `None`; this
still advances the first-present deadline and an already-started asynchronous READY publication.
Occlusion, pending initialization or a producer pause cannot stall those protocols indefinitely.
Maintenance observes time/results without minting a presentation or advancing simulation.

Old World-based surface retirement currently treats missing `WgpuRenderBackend` as completed
surface release. Delete that inference in the same consumer migration. Surface owner Drop still
acknowledges actual native release, but missing World endpoints or removed namespaces do not prove
submitted work completion. A runner must continue driving retained GPU retirement after Runtime
fault, Stop or endpoint revocation, before releasing its actual registered platform providers.

Each producer connection registers a Runtime close obligation before captures leave the World.
Its logical close stops new capture and settles buffered, borrowed/in-flight and returned/refused
one-shot obligations through the original completion port. Runtime cannot reach `Stopped` while
those obligations still require a state-gated World operation. Physical submitted-prefix retirement
has its own retained owner and can continue after that logical connection has closed. A refused
owned result remains in connection custody; a normal port return accompanied by
`post_operation_fault` is reconciled exactly once before fault policy, never replayed as though the
operation had not consumed it.

The runner's local run session must retain Runtime, presentation Host and the actual platform
dependencies through every incomplete exit. Replace the borrowed-run/bare-error boundary when it
cannot express that custody. An early EventLoop return or close timeout transfers an explicit
move-only incomplete retirement owner to the caller under the original platform affinity, with
actual polling/cleanup and retained diagnostics. Existing Runtime quarantine does not automatically
take ownership of native state moved outside the World. Releasing a failure value must obey the
same existing local retirement/quarantine policy rather than drop pending Host/window dependencies.
The native adapter must qualify this path; ordinary `Result<(), AppRunError>` alone is insufficient
evidence that an external GPU owner is safe to release.

The next production unit joins one owned native run session to one finite stock capture/result
connection. It may retain the current single-Runtime, window-only, serial drawing support while
the subsequent immutable-output and producer-prefix work remains open. The connection carries a
non-reused incarnation, Runtime generation and monotonic capture sequence; a render-frame index
alone cannot authenticate a returned result. Its CPU result carries exact frame/status facts,
world-presentation evidence, original UI outcomes and the read-only WGPU observation, without GPU
handles. Remove the backend's `Resource` and World driver implementations rather than retaining
them behind a second Host facade.

Root assembles the concrete WGPU presentation driver and Winit runner. Winit's integration needs
only local frame maintenance, target retirement and the next necessary wake, without depending on
WGPU or mirroring its GPU operations. Maintenance must both advance `observe(None, now)` and arrange
the corresponding `WaitUntil`; an observation callback with no subsequent wake cannot enforce a
first-present or READY publication deadline. Its close action reaches the runner directly, without
requiring another simulation frame to consume an exit request.

The native event pump belongs to the retained session alongside the actual handle registry,
registered target IDs and platform windows. The locked Winit consuming `run_app` entry cannot
return that pump after an incomplete exit, and `run_app_on_demand` does not establish permission to
carry live windows between separate runs. Use the platform's retained pump entry for this native
workflow and qualify real retirement on the same affinity. A one-time Runtime publication transfer
stays consumed; cleanup drives the session rather than restoring a Runtime into that publication
slot or reducing it prematurely to Runtime-only retirement. A terminal platform failure may remain
incomplete, with its dependencies retained; it must not invent native completion.

For the locked Winit 0.30.13 Windows adapter, `pump_app_events` resets its exit state before
returning `Exit`, so retirement can resume through the same retained pump. Its subsequent
`Init`/`Resumed` callbacks must not recreate a retiring Runtime or window. Keep target mappings
until actual `Destroyed` observation: dropping the last window owner only posts its destruction
message. A retry must dispatch that message rather than immediately exiting again. Qualify early
Exit, budget expiry and actual destruction with this adapter; this source finding does not grant
other-platform or panic-recovery guarantees.

Establish original Device/Queue retirement custody while the device is Ready and before its first
accepted submission. Existing backend error handling can clear the device namespace before
`render_packet` returns; capturing retirement only on that error path is too late. Share the
existing concrete Editor queue-drain mechanism privately. Telemetry, observation capacity and World
resource membership are independent of that custody. Consumer telemetry configuration becomes CPU
initialization input, and bounded samples/statistics return as observations; GPU polling is not
reintroduced as a World operation.

## Outputs, Recovery And Completion

Each published output owns distinct physical storage while a predecessor is retained. Its issued
identity binds Host/epoch, producer incarnation, capture, logical output slot and non-reused output
version. A mutable target generation cannot substitute for that pixel version. The ordinary API
provides Host-validated sampling, without an externally writable target. Exact-WGPU adapters obey
the immutable-import protocol as trusted native Rust; cloned native handles are not a sandbox or
a type-system proof against undeclared writes. A latest-output selection resolves once at
composite admission and retains that exact version even if the producer advances or closes.

Composite identity also includes a Host-issued non-reused display occurrence, actual destination
presentation and its placement/mapping incarnation. The original Runtime UI completion designates
one exact interactive completing occurrence/attempt. Image equality or image-level sample
deduplication cannot substitute for that join: passive mirror M sampling the same A1 must not
settle designated D's permit. Repainting D does not settle it twice. Hiding/failing D settles or
revokes its own pending completion through connection close rules, independently of a live mirror.
Pixel custody, occurrence presentation and producer interaction retain separate authorities.

Native encoding, Ready preparation, submission admission, logical output/history publication and
physical completion are separate boundaries. Register the original accepted-prefix custody in
the same serialized submission segment. It must retain all actual textures, buffers and imported
Image versions, including conservative custody after a failed submit attempt. Telemetry can be
disabled or observation capacity exhausted without disabling physical retirement. Failure cannot
publish output or advance any of its history group; independent output groups use separate native
validation/encoding/submission boundaries initially.

Producer retirement first stops admission and settles its pending one-shot UI obligations, then
seals its submitted prefix. The producer completes when that prefix physically completes, while
other producers continue submitting. An output held by another composition remains alive and
charged after producer retirement. Whole-queue drain remains the final Host shutdown boundary.
Device loss revokes old-epoch authority and GPU history but does not fabricate completion.

Retain reconstruction input explicitly and independently of the output's GPU lifetime. Recovery
consumes those exact CPU facts and produces a new epoch/output identity. It cannot replay old UI
permissions, reuse GPU history or resolve current source/assets as the meaning of an old capture.
When the caller has retired the input, report recovery unavailable. The first consumer must
release live stores and ordinary frame captures to expose accidental dependence on them.

## Implementation And Deletion Order

1. Extract a coherent private device domain through existing Runtime and Editor consumers. Preserve
   ordinary fallback/loss/close behavior; no second backend, facade alias or public raw-GPU hook.
2. Join the owned stock capture/result endpoint and runner-local presentation driver. Migrate
   desktop first-present/UI facts and retained close together. Delete GPU ECS storage, direct
   scheduled submissions and the missing-resource retirement shortcut after all real consumers
   use the local owner. Reconcile ADRs0032/0040/0078 at this boundary.
3. Separate Editor target configuration from immutable pixel outputs. Scene, Canvas, Game and
   toolkit registration retain exact leases. Add producer-local prefixes and independent
   reconstruction inputs; qualify real window-free execution, retained A1/A2, B progress through
   A close and actual loss/recovery. Reconcile ADRs0111/0112 without weakening composite/input
   authority. Offscreen GPU remains distinct from content-only Headless.
4. Integrate the compared graph/typed Feature adapters, external policy and Nara-drawn composition
   through these owners. Move stock Sprite/UI across the same seam and delete displaced central
   fields/matches. Proposed0121 becomes Accepted only for the selected and qualified contract;
   immutable material declarations follow under R9b.

The first step can be committed as a verified internal refactor. It does not close steps2-4.
Every public owner change travels with a real consumer; source compatibility is unnecessary for
this unreleased engine. Keep complete module/feature builds and the selected native Editor and
ordinary supervised-desktop paths throughout the transition.

## Qualification

The complete unit includes an external public no-window stock consumer using normal Ready,
actual drawing/readback and close, two physical output versions with different pixels, A/B
interleaved submissions and A retirement while B continues, independent original reconstruction
input through loss and explicit unavailability after its release. Include actual late native
validation/submission failures and old-epoch refusal without output or UI success.

Use two real windows plus an image consumer for surface/provider order and retained output
composition. Preserve ordinary first-present/supervised READY, exact UI completion and Scene/
Canvas picking, and run the unit plan's native cadence, input and independent View close/recovery
traces. The private graph fixture supplies numerical and failure counterexamples, not evidence
that any of these production consumers already pass.

Inject early EventLoop return/close timeout with a pending result and native prefix: the complete
local retirement owner remains held and retryable, Runtime does not become Stopped before its
logical completion barrier, and providers cannot release early. Separately reject a result before
port application and fault immediately after a successful application; preserve the former's
original input and consume the latter only once. Exercise first-present timeout with no results,
READY Pending with no later capture, passive M displaying before designated D, and loss both before
submit and after accepted submit but before completion. New Ready must not release old-epoch
physical custody or restore its UI/picking authority.

## Evidence

- [Source-backed rendering review](../research/2026-10-29-render-architecture-and-refactor-plan-review.md)
  compares pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
  `81ab961f91066a63bcbf3ec340962bb1a1ada558`. Schedule composition and product effect slots can
  coexist with explicit resource/lifetime owners; neither removes those obligations.
- [Private CPU/DX12 comparison](../knowledge/engineering/verification/2026-10/2026-10-31T060722Z-private-render-graph-buffer-history-and-viable-controls-comparison-on-dx12-d6aca1996b2342659bd07933b33e0e64.md)
  exercises exact versions, buffer semantics, history groups and accepted-prefix custody.
- [ADR0078](adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md) owns one
  serialized device-domain authority, native/browser placement, epochs and actual native drain.
