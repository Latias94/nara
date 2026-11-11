# Retained WGPU Image Outputs

`nara_render_wgpu::WgpuRenderHost` owns the stock native device outside the gameplay World.
An image-only consumer can initialize that same owner with `WgpuRenderHost::offscreen()`;
it needs no Runtime, Winit, window handle or hidden Surface. This is GPU execution, distinct
from content-only headless operation. The current path is native and serialized; it does not
make synchronous initialization suitable for a browser event loop.

The independent [render-output consumer](../../tests/fixtures/render-output-consumer/README.md)
contains a complete example using public crate-root interfaces with no default features.

## Shared WGPU Source

The maintained native renderer currently uses official WGPU Git revision
`cddb2f5d80dc0009b2ef6767635140cacf90a007`. Its package version is `30.0.0`, but it includes
surface-drop and wasm-timeout fixes newer than registry `30.0.1`. A later corrected registry
release can replace this source after affected-consumer qualification.

An application directly sharing Device/Queue/Texture types must select that same Cargo source
and use the actual original device. If the application also builds the reference Dear renderer,
patch its compatible `wgpu = "30.0"` registry edge at the application workspace root:

```toml
[patch.crates-io]
wgpu = { git = "https://github.com/gfx-rs/wgpu", rev = "cddb2f5d80dc0009b2ef6767635140cacf90a007" }
```

The patch is application-owned and does not propagate through path dependencies. Direct exact
`=30.0.1` requirements must be reconciled as well; that version requirement cannot select a Git
package declaring `30.0.0`. Each independent workspace retains its own lockfile.

## Producers And Configuration

Create an owned producer with `host.create_producer()`. Its copyable `producer.id()` is an
observation fact; it cannot admit work. Target preparation, submission and retirement borrow
the producer owner. Retirement is irreversible, and dropping the owner stops its admission.
The Host reaps its original pending native responsibility during maintenance.

Allocate a logical target with `RenderImage2d::allocate_handle()` and `RenderImagePacket::new`.
Use `RenderFrameSourceGeneration::try_acquire()` for an independent capture stream, then
`build_image_render_frame_packet` with its extracted views. A managed Runtime retains its
actual Runtime generation; do not construct a Runtime to obtain an offscreen identity.

Begin the Host transaction, reserve the target with `reserve_image_target`, and activate the
reservation with `activate_image_target(&producer, reservation)`. A coordinated consumer
can instead hold `prepare_image_target_activation` through its other final publication and
consume `activation.commit()`. The guard exclusively borrows both Host and producer, so a
delayed commit cannot overwrite intervening configuration. A reservation admits configuration
and its first allocation, not a future toolkit registration or successful output.

## Pixels, Display And Reconstruction

Create a `WgpuPolicyDefinition<P>` and keep its `WgpuPolicyInstallation<P>` with the capture
owner. For stock clears, use `WgpuStockPolicy::new()` and `WgpuStockCapture::new()`. Calling
`installation.capture(packet, Arc::new(capture))` produces a `WgpuRenderInput` that retains its
original definition and exact captured CPU facts. Call `host.prepare_policy(&producer, &definition)`
before beginning a frame or reserving a target when the policy needs nondefault device features
or limits. A late request must fit the already enabled device or require explicit reconstruction.

`host.submit(&producer, &input)`
borrows those facts and returns `WgpuImageOutput` only after successful native admission and
device-health checks. A refused attempt preserves the successful producer watermark; the same
input can be retried where the error permits it. Inputs contain no Runtime UI completion permit.

Each output owns distinct immutable texture storage and a non-reused output version. Retaining
A1 while submitting A2 preserves A1's pixels through producer resize or close. Clone the output
to retain that allocation, rather than looking up the producer's latest target. The default Host
limit is 512MiB of shared logical native payload; `with_resource_byte_limit` changes it.
`retained_resource_bytes` includes inactive output reservations, retained outputs, graph allocations,
and prepared Image, glyph, fallback, instance and material allocations. Each allocation is charged
once until its last responsibility releases it, including cached state, output leases and submitted
uses. Toolkit-owned storage and native pipeline/driver overhead are excluded; the budget does not
measure physical driver residency. A WorldCanvas world image is a declared program resource.

`output.descriptor()` observes that exact resource's admitted shape, transfer and alpha meaning.
External UI/image adapters use those facts rather than infer opacity from the native format or
look up the producer's latest policy. The returned descriptor is detached metadata: editing it
does not alter the resource or its nominal semantic. Typed program imports still require their
original marker, and readable metadata after retirement does not permit native sampling.

Stock Scene and Canvas outputs contain premultiplied accumulated RGB. Their source Images and
authored tints remain straight alpha: the stock shader/blend path associates each source once,
and stock camera/Canvas clears convert to the target's associated representation. Opaque drawing
preserves shaded RGB and writes alpha one. Arbitrary finite-program clears remain raw values under
that program's declared target meaning. For an sRGB output, association happens in decoded linear
RGB before storage encoding; alpha never receives the sRGB transfer function. See
[ADR0092](../architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md).

An external UI adapter must also select its renderer's working domain. Sampling an sRGB texture
decodes its RGB; writing a linear result to an sRGB attachment encodes it. Neither operation fixes
a renderer that blends numeric sRGB UI colors into the engine's linear target. Preserve imported
transfer/alpha facts and choose the correct composition mode rather than relabeling resources.
The [GPUI engine consumer](../../tests/fixtures/gpui-engine-consumer/README.md) demonstrates this
boundary using the original engine device, encoder and window submission.

The existing transaction observation also exposes `frame_transaction_stats().native_work()`.
It records work reached through Nara's preparation/program paths, separately from allocation
demand and retained bytes. Retries after failure accumulate until the next transaction; native
callback work such as `submit_image_use` readback, arbitrary Feature commands and third-party
renderers is excluded. See the [program work contract](render-programs.md#admission-outputs-and-close)
before using those issued-write/initialization/encoded-copy counts as evidence.

`display_image(output)` selects one exact output for a new display occurrence. Clone that display
for repaint; create a separate display for a mirror. Toolkit bindings retain the exact display.
Image equality, a receipt or a passive mirror cannot settle another occurrence's Runtime UI
permission. Editor adapters additionally preserve document/revision, geometry, mapping and
existing Runtime input authority. A retained pixel allocation alone grants no interaction.

Keep the original `WgpuRenderInput` separately when reconstruction is required. Closing or dropping
its installation refuses new captures while accepted inputs retain their original implementation.
A new Host can
realize those exact facts with a new output identity after the old device/Host retires. An output
does not retain or recreate its CPU input; after the caller releases that input there is no
reconstruction operation from the output. Do not resolve current assets to impersonate an old
capture. Device loss and Host retirement revoke old-epoch sampling and interaction authority.

## Native Uses And Retirement

Use `submit_image_use(&outputs, operation)` for a trusted native read-only composition or readback.
The Host validates and retains those exact output leases through the submitted use, including
uploads queued by a callback that later refuses. The callback encodes commands and may enqueue
uploads; it must not independently submit, mutate imported outputs, or outlive their leases.
The returned prefix observes resource settlement, not successful pixels or map completion.
Readback must separately wait for its actual mapping callback.

`with_image` is the lower-level trusted WGPU escape seam. Native handle clones are not a sandbox:
an adapter must not mutate/destroy the imported output and must retain its lease for every native
use. Prefer the owning submission operation for ordinary consumption.

`retire_producer(&producer)` stops admission and returns the producer's frozen submitted prefix.
Poll it without waiting for other producers to stop; B can keep submitting after A closes.
The Host retains every outstanding prefix separately because a newer native prefix does not
own abandoned callback CPU custody. Callback abandonment is an explicit failure observation;
original native fence evidence may recover resource settlement without authorizing output or UI.
An SDK prefix without an exposed native index has a conservative original-QueueEmpty recovery
branch. Neither normal polling nor recovery enqueues new work.

## Native Window Composition

A native frontend owns its actual platform windows and registers their owning providers with
one Host. `WgpuRenderHost::new` admits the initial provider; `register_window` adds another real
target to the same device domain. The selected adapter must support each actual surface. Keep
native windows alive through Host retirement and registered-provider release.

Use `begin_window_frame` for the Host drive and `with_device` to initialize frontend-native
pipeline state from `WgpuWindowDeviceScope`. That borrowed scope shares the original device,
queue, enabled requirements and retirement owner. It does not grant independent submission.
The same Host may also initialize window-free and later accept real registered windows.

Before any operation acquires that device, contribute frontend-native requirements with
`host.request_device_requirements(&WgpuDeviceRequirements::new(features, limits))`. They join
the same closed request as render Feature definitions. For example, an adapter using encoder
timestamp commands declares `TIMESTAMP_QUERY | TIMESTAMP_QUERY_INSIDE_ENCODERS` before its first
`with_device` or offscreen submission. Declaring requirements does not acquire or prove support;
actual acquisition checks the selected Adapter and enabled device. A late subset can proceed,
but a new capability cannot silently replace a healthy device or broaden its recovery request.
After retirement the Host rejects declarations, and actual original-device loss stays `DeviceLost`.
Prerequisite refusals use `WgpuRenderHostError::DeviceRequirements`, covering both native
frontend and render Feature declarations. Acquisition/backend failures retain their separate
typed cause. This replaces the narrower pre-1.0 `FeatureRequirements` variant.

Admit `WgpuImageDisplay` with a frontend-owned immutable snapshot of destination, placement,
clip, DPI and role. A changed snapshot gets a fresh occurrence; a passive mirror has its own
occurrence. Pass those exact displays to `present_window`. Its `WgpuWindowFrame` exposes native
encoding and actual acquired extent/format, while the Host acquires, submits and presents.
`record_image_sample` declares an actual non-fully-clipped encoded sample of a supplied display.
The Host does not parse shaders or infer input mappings from a texture.

`WgpuWindowFrameOutcome` returns an optional actual window receipt and exact image-composite
receipts. Join each `WgpuImageCompositeReceipt` to its original immutable placement/binding
snapshot before accepting picking or UI interaction. A repaint cannot mint another one-shot UI
permit, and another window's mirror cannot complete the designated occurrence.

An external frontend retaining UI-thread-local recordings calls
`WgpuWindowFrame::submission_prefix()` before its first upload or encoding operation. Keep that
exact observation and all local recording resources outside the paint callback. The observation
starts unbound and incomplete, then shares the Host's one-time original submission binding even
when paint, encoding, submission or pre-present fails. Release resources only after the original
prefix settles through `poll` or explicit abandoned-callback recovery. A failed attempt has no
presentation authority even if its resources later settle. A producer's earlier render prefix
and a later latest-prefix query cannot replace this window attempt's resource obligation.

The independent [GPUI consumer](../../tests/fixtures/gpui-engine-consumer/README.md)
records a hosted root through this same window encoder. The framework provides UI execution and
an optional Scene recorder; Nara owns the device, target, submit/present and retirement. Its
workspace consumes the sibling maintained framework; ordinary renderer consumers
do not acquire a GPUI dependency.

Each native attempt admits at most256 output/display items and256 extra command buffers.
Repeated sampling of one occurrence consumes one record. Engine tables and original-prefix
metadata admit before clones/native work; receipt storage and generation checks precede present.
Paint/read-use callback refusal or panic still binds its queued uploads and retains the original
prefix. `NativeCallbackPanicked` is attempt-local; actual native execution failure retains its
separate owner-replacement classification. A skipped/minimized target returns no present facts.

Acquiring a window texture marks that surface configuration for recovery until actual native
presentation succeeds. If encoding, validation, submission or pre-present fails, the next Host
turn reconfigures the same surface before acquiring again. This also covers Vulkan backends
whose discarded acquired image does not reset its binary semaphore. The failure never presents
partial drawing or publishes a window/composite receipt. Healthy presentations clear this
recovery requirement; a suboptimal acquire retains the existing reconfiguration policy.

The independent [composition consumer](../../tests/fixtures/render-composition-consumer/README.md)
exercises two real windows and an independent image stream. Its source, processed-content and
clipped drawing cases have separate assertions; their actual qualification belongs to the
render unit's evidence rather than interface availability alone.

After all consumers stop, drive `poll_retirement` until complete. This final whole-queue boundary
is separate from producer-local close. Direct native Host Drop attempts the same original-owner
retirement for a finite drive budget and aborts if dependencies cannot be safely retired, matching
the native Session policy. The drive budget is not a strict wall-clock bound on WGPU callbacks
or native destructors. Explicit retirement retains errors and allows the caller to retry.

The [program guide](render-programs.md) explains finite texture/buffer Features and policy capture.
Complete cross-stream composition and the Nara-drawn editable Editor remain separately ordered work in
the [render plan](../plans/2026-10-31-render-composition-and-host-execution-plan.md). Interface
availability and native qualification retain their explicitly recorded scopes.
