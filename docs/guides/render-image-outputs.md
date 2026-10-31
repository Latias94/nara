# Retained WGPU Image Outputs

`nara_render_wgpu::WgpuRenderHost` owns the stock native device outside the gameplay World.
An image-only consumer can initialize that same owner with `WgpuRenderHost::offscreen()`;
it needs no Runtime, Winit, window handle or hidden Surface. This is GPU execution, distinct
from content-only headless operation. The current path is native and serialized; it does not
make synchronous initialization suitable for a browser event loop.

The independent [render-output consumer](../../tests/fixtures/render-output-consumer/README.md)
contains a complete example using public crate-root interfaces with no default features.

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

`WgpuImageInput` owns exact captured CPU drawing facts. `host.submit(&producer, &input)`
borrows those facts and returns `WgpuImageOutput` only after successful native admission and
device-health checks. A refused attempt preserves the successful producer watermark; the same
input can be retried where the error permits it. Inputs contain no Runtime UI completion permit.

Each output owns distinct immutable texture storage and a non-reused output version. Retaining
A1 while submitting A2 preserves A1's pixels through producer resize or close. Clone the output
to retain that allocation, rather than looking up the producer's latest target. The default Host
limit is 512MiB of logical RGBA8 output allocations; `with_output_byte_limit` changes it.
`retained_output_bytes` includes inactive reservations, output leases and actual submitted uses.
It does not report physical driver residency or all other render caches.

`display_image(output)` selects one exact output for a new display occurrence. Clone that display
for repaint; create a separate display for a mirror. Toolkit bindings retain the exact display.
Image equality, a receipt or a passive mirror cannot settle another occurrence's Runtime UI
permission. Editor adapters additionally preserve document/revision, geometry, mapping and
existing Runtime input authority. A retained pixel allocation alone grants no interaction.

Keep the original `WgpuImageInput` separately when reconstruction is required. A new Host can
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

After all consumers stop, drive `poll_retirement` until complete. This final whole-queue boundary
is separate from producer-local close. Direct native Host Drop attempts the same original-owner
retirement for a finite drive budget and aborts if dependencies cannot be safely retired, matching
the native Session policy. The drive budget is not a strict wall-clock bound on WGPU callbacks
or native destructors. Explicit retirement retains errors and allows the caller to retry.

The production resource graph, external Feature/policy interface and Nara-drawn editable Editor
remain separately ordered work in the [render plan](../plans/2026-10-31-render-composition-and-host-execution-plan.md).
This image-output interface does not imply that those capabilities are implemented.
