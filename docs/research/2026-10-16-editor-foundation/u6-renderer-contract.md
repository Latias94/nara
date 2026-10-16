# U6 direct Winit/WGPU renderer contract

Date: 2026-10-16. Status: source research and a bounded implementation proposal, not native qualification.

## Decision

Keep `WinitViewportRoute` as the secondary surface/renderer owner. Extend its backend-constructed
frame report with immutable, exact presented records and add one opt-in private Nara bridge that
accepts that backend type. Capture the actual `TextureView` handles used by successfully emitted
draws, rather than asking Nara to infer sampling from widget calls or reimplement Dear ImGui's
draw-command validation. Do not submit those secondary command buffers again.

This is smaller than transferring submission/presentation into a host hook, and much smaller than
implementing custom renderer callbacks. The unchanged v0.20.0 report cannot certify an exact Nara
image in a secondary surface: it exposes only two sets of ImGui viewport IDs. A typed detailed
report is a concrete observability repair at the existing owner, not a general renderer interface.
[S1, S2, S3]

The bridge must preserve the current receipt's meaning. A primary window receipt cannot stand in
for a detached window. A native window number or ImGui viewport ID alone cannot stand in for a
surface lifetime. A matching texture view alone cannot identify which of several writes to the
same allocation was displayed. These are finite join conditions described below, not grounds for
a second window registry, render abstraction, or source-code proof tool. [N1, N2, S8]

## Exact baseline and evidence limit

The reference tag `v0.20.0` resolves to
`af2942c78651b280d18af621de55b5462bb1e10a`; the checkout initially had HEAD
`391171d178beefe76abb5e5921c029ad5001420e`. Both the committed and working-tree diffs against
the tag were empty for `dear-imgui/`, the WGPU/Winit backends, `docs/CUSTOM_BACKENDS.md`, and the
WGPU native smoke/support files read here. All upstream source locations below refer to the exact
tag, not capabilities inferred from a later release. Nara source references describe committed
U5 `bb6314efb34b4bf0665812306694367bed8e0599`; root-owned U6 changes may follow this note.

No Cargo command or native run was performed for this research. Existing tests were read as
source; their presence is not a claim that this worktree executed them. The earlier
[release research](dear-imgui-020.md) owns compatibility and platform qualification context.

## Existing preparation and primary presentation

`WinitViewportRoute::prepare(&self, &ActiveEventLoop, FrameToken)` performs one complete
transaction before the application acquires its primary surface. It validates the Context,
finalizes the UI using the renderer's synchronous consumer, reconciles managed textures, and calls
`ReconciledFrame::update_and_render_platform_windows_default()`. That combined call updates native
platform windows, then dispatches secondary rendering and swapping. Route preparation aggregates
renderer and platform callback faults. [S1, S2, S4]

Within that transaction, WGPU creates secondary surfaces after the Winit windows exist, acquires
each drawable surface, records its draw data, calls the renderer's queue `submit`, retains that
exact acquired frame, and later calls `Queue::present` for WGPU 30. Only then are the corresponding
render/present viewport IDs recorded. Minimized, occluded, timed-out, or otherwise skipped surfaces
do not acquire the corresponding success fact. The report means the WGPU present API was called;
it does not claim monitor scanout or GPU completion. [S3, S5]

`WgpuPreparedViewportFrame` owns a private `ReconciledFrame`, the secondary report, and an exact
runtime identity. It exposes only `context_id()` and `secondary_report()`; it does not expose
primary/secondary draw data or texture usage. It is move-only and is not `Send`/`Sync`. The report
has only `render_submitted_viewport_ids()` and `present_submitted_viewport_ids()`, both sorted and
deduplicated. It has no sampled images, native WindowId, surface generation, acquired frame identity,
submission index, or device/queue handle. [S2, S6]

Nara's existing primary seam already accepts the prepared capability:

1. Submit the admitted Scene/Game images to Nara's existing queue.
2. Build the UI with registrations for those exact active views, then call `route.prepare` outside
   `WgpuEditorExecution::present_window`.
3. Inspect/accept secondary evidence before calling `present_window`; retain it independently of
   primary success, since a skipped primary acquisition does not undo secondary presents.
4. Move the same prepared value into the primary paint closure. Inside
   `WgpuEditorWindowFrame::encode`, create a render pass over Nara's target and call
   `route.render_main(prepared, &mut pass, actual_attachment_extent)` once.
5. Let `present_window` perform its existing queue submission, pre-present callback, and primary
   presentation. Do not call another renderer `render`, platform-window dispatch, queue submission,
   or primary present for that prepared frame. [S1, S2, N1]

`render_main` validates runtime identity and delegates to `render_reconciled`; these methods only
record primary commands. They do not submit or present. If primary acquire skips, dropping the
captured prepared capability is the intended path. Root must derive the primary extent from the
acquired `SurfaceTexture.texture.size()`: U5's `acquire_editor_surface` copied `surface.size` into
the returned extent, while Dear's `FramebufferExtent` contract requires the actual attachment.
[S1, S2, S7, N1, N3]

## Smallest detailed-report extension

The following is a proposal; none of these new fields/accessors exists at the tag.

Keep construction inside `dear-imgui-wgpu`. Add a presented-record collection to the existing
same-frame report, with private fields, no public/default constructor for a successful record, and
no caller-supplied success flags. Each successful secondary record should retain:

- The Context/route lifetime and preparation sequence, for namespace and replay checks.
- A backend-owned native viewport instance identity, the current ImGui viewport ID only as routing
  information, and a stable native window identity. ImGui IDs can change during docking.
- A unique surface lifetime identity and configuration generation. Recreating a surface must change
  its lifetime identity; resizing/reconfiguring must advance its configuration generation.
- The actual acquired attachment extent, obtained from the acquired texture, and its format.
- The exact `Device` and `Queue` handles used for recording/submission, retained by clone.
- Deduplicated `TextureView` handles actually bound for successful standard `Elements` draws.
  Keeping the corresponding `TextureId` is useful for diagnostics but is not needed for Nara's
  resource equality check. [S2, S3, S5, S9]

Collect a sample only after the backend has accepted a nonzero element count, validated index and
vertex ranges, projected a nonempty scissor, and resolved the texture view. The existing preparation
branch already performs these checks. Propagate that identity with its prepared element or collect
it while executing the matching successful draw; do not rescan raw draw data in Nara. Publish a
record only after the entire viewport draw succeeded, its command buffer was submitted, and its
retained acquired frame was passed to the presentation API. [S9]

Store pending frame and pending evidence together, or enforce an equivalent single owner invariant.
`reconfigure_surface` currently drops `pending_frame`, and surface release/recreation drops the
frame before targets and surface. The same paths must discard pending evidence. Otherwise a later
swap callback could attach old samples to a new surface frame. Empty/clipped-only viewports may
still have a surface-present record but an empty sampled-view set. [S3, S5]

The primary path needs an equivalent backend-produced sampled-view result from successful
`render_main`. Merely exposing raw primary draw data would make Nara reproduce scissor/range/texture
validation, and a widget-emission list includes commands the renderer can skip. Returning the
renderer-produced sample result is the smaller exact contract. Nara's existing primary
`record_viewport_sample` then records only images found in that result. [S6, S9, N1]

Raw custom draw callbacks can issue work outside the standard element executor. The U6 adapter
should use the existing standard image, sampler, and reset commands; these records certify only
the backend's emitted element draws. They must not claim to observe arbitrary callback-issued GPU
work. This does not require a callback call graph or a new proof language. [S9, S10]

Current preparation returns an error after `trace.finish()` if later renderer/platform faults are
observed, so successful partial secondary facts can be discarded with that error. A detailed-report
patch must either retain the completed report in the preparation error/attempt or explicitly leave
those Nara views without a presented receipt. It must never recover the missing facts from primary
success or viewport/window existence. [S1, S2]

## Nara bridge and exact join

Add an opt-in private integration in the GPU owner that accepts the concrete backend report. The
runtime and ordinary renderer remain free of a mandatory Dear dependency. The frontend cannot
manufacture a successful backend record and the bridge does not accept a structurally equivalent
plain data assertion. `Device`, `Queue`, and `TextureView` in WGPU 30 implement handle equality;
compare the retained handles with the current Nara namespace and active target view. [W1]

For each accepted record, the bridge should:

1. Validate exact Device/Queue equality, Nara backend instance/device epoch, preparation sequence,
   and the current synchronous host frame. Consume or reject replay of the same report batch.
2. Match sampled views to the existing active per-view targets, then validate the supplied exact
   `WgpuSubmittedImageReceipt` against the active target/source/handoff generations.
3. Require the receipt to be the latest successful submission selected for this host frame. A view
   can be written repeatedly without changing its `TextureView`; resource equality alone cannot
   distinguish those contents. Store the latest receipt in the existing per-view owner if needed,
   and prevent another write/replacement between prepare and completion of this synchronous join.
4. Construct secondary window/composite receipts using that record's surface lifetime/configuration
   and its own window identity. Do not borrow primary `surface_generation`, primary `WindowId`, or
   primary presentation outcome. The bridge does not submit/present anything. [N1, N2]

The existing `WgpuEditorWindowPresentedReceipt` stores a Nara `WindowId` plus one surface generation.
Neither ImGui viewport IDs nor a Winit numeric ID can be silently cast into this independent Nara
ID namespace. The implementation must make the delegated identity explicit at this receipt seam:
reuse an existing admitted Editor window identity if one exists, or extend the private presentation
target to distinguish a delegated `(route lifetime, native viewport instance, surface lifetime)`
from an engine window. This is a local receipt/sidecar change; do not create a second OS-window
registry or counterfeit a `RenderWindowPacket`. A typed detailed record solves missing facts; it
does not by itself solve an incompatible receipt target type. [N2, N4, S8]

## Winit window mapping and lifetime

There is no public safe `viewport ID -> Arc<Window>` lookup in the tag. The existing
`multi_viewport::window_for_viewport` is `pub(crate)` and the authoritative registry lookup is
`pub(super)`; internally they clone the platform-owned `Arc<Window>` after validating ownership.
The renderer's Winit adapter instead dereferences the viewport's `platform_handle` as `Window`
and creates an unsafe static surface, relying on renderer-resource teardown before platform-window
teardown. The stored pointer comes from `Arc::as_ptr`; downstream code must not reconstruct an
owning Arc with `Arc::from_raw`. [S11, S12]

The detailed report does not require exposing `Arc<Window>` or a new public registry. The backend
can obtain a stable native identity while its existing validated callback owns the window borrow,
then retain only the immutable identity in the record. If an actual retained window handle is
needed by a host-owned surface path, the smallest safe upstream addition is a lookup on the
exact-generation `WinitViewportRendererAdapter` that validates Context/viewport ownership and clones
the existing registry Arc. A raw `Viewport::platform_handle()` accessor is not a safe retained
ownership capability. [S11, S12]

## Alternatives and cost

| Option | Ownership retained or moved | Required upstream boundary | Assessment |
| --- | --- | --- | --- |
| Immutable detailed records | Existing route keeps surfaces, encode, submit, present and teardown | Add real sampled views, exact surface identity/generation, actual extent, GPU handle identity and primary sample result at existing operations | Recommended: smallest repair, no second submission or WSI failure protocol |
| Scoped host submit/present hook | Route keeps window/surface creation and rendering; host consumes one command buffer and acquired frame | A synchronous hook must move the exact `SurfaceTexture` and command buffer, retain exact surface metadata, and return a move-only completion; callback/reconfigure paths must preserve or discard that whole obligation consistently | Viable if Nara must own the actual queue call; larger because it transfers presentation responsibility and introduces host failure/abandonment paths |
| Own five public renderer callbacks | Nara takes surface creation/configuration/acquire, encode dispatch, submission/present, per-window resources, fault aggregation and teardown | No custom viewport renderer trait exists; unsafe typed/raw callback setters are public, but WGPU's reusable `&DrawData` secondary draw method is private | Not a narrow adapter at v0.20.0; either add a supported renderer draw capability upstream or implement/maintain a full custom renderer |

For a scoped hook, do not pass only a viewport ID plus an assertion that the host presented it. Pass
the moved acquired frame, its one finished command buffer, actual extent, exact sampled views and
stable surface/window identities; consume the presentation obligation exactly once. The default
path can preserve current behavior. However, a hook does not eliminate the need for exact sample
capture or lifetime/generation metadata, and retaining the current route's submit/present operation
already supplies authoritative facts. Therefore the hook adds ownership transfer without solving
an additional U6 requirement. [S3, S5]

The tag has public unsafe `PlatformIo::set_renderer_create_window`, destroy, set-size, render, and
swap setters, with matching raw variants. The set-size callback must use the wrapper's aggregate
ABI shim. There is no `RendererViewportBackend`/`PlatformViewportBackend` trait at the exact tag;
the similarly named renderer consumer capability is sealed and unrelated. A custom route can
borrow the Winit event loop through `with_event_loop`, but must own stable callback state,
Context attachment, complete callback lease, deferred faults, and renderer-before-platform
teardown. [S10, S13, S14]

Most decisively, WGPU's only `&DrawData` rendering entry is
`pub(super) render_read_only_draw_data_with_fb_size`. The public `render_reconciled` consumes the
single reconciled frame and draws its primary data; it is not an arbitrary secondary-draw entry.
Thus public callbacks alone cannot reuse the existing WGPU renderer for secondary frames. Do not
fork that renderer merely to observe presentation. [S7]

## Registration, retirement and recovery

The route exposes `register_external_texture(&TextureView) -> ExternalTextureId`,
`update_external_texture(ExternalTextureId, &TextureView)`, and
`unregister_external_texture(ExternalTextureId)`. Registration clones the view. Updating keeps the
external ID, replaces the view mapping, and invalidates the image bind-group cache. Unregistering
removes the mapping immediately and does not wait for GPU completion or destroy the application
texture. There is no deferred external-registration retirement API. Stale, removed, or foreign
renderer handles fail lookup. [S1, S15, S16]

Make updates/unregisters at synchronous frame boundaries after all previously referring UI frames
have been recorded or abandoned. Do not update an ID between secondary prepare and primary
recording of the same prepared frame. Keep old resources alive for recorded/submitted users and
do not explicitly destroy them until those users finish. The bridge's retained sample view is
evidence and a handle reference, not an authorization to release resources early. The upstream
integration tests exercise external-view replacement and reuse across managed texture changes;
they do not provide an automatic external-retirement queue. [S15, S16]

For same-live-device object invalidation, `route.invalidate_device_objects(&mut Context)` resets
renderer-owned objects and managed texture bindings while preserving external texture handles.
The next preparation recreates renderer objects. This is not device-loss recovery. On actual
device replacement, shut down the old route, recreate all GPU resources/registrations for the new
Nara namespace, and construct a new renderer using cloned handles from Nara's `with_device` scope.
[S1, S17, N1]

If detached windows already exist, a fresh route cannot simply attach after renderer shutdown:
attachment rejects `PlatformWindowsAlreadyCreated`, while renderer shutdown deliberately leaves
native windows to Winit. Use `route.shutdown -> platform.disable_viewports -> platform.enable_viewports
-> new renderer/route.attach`, before the next preparation creates secondary windows. Preserve
toolkit layout/user document state separately. Final shutdown is `route.shutdown`, then
`platform.disable_viewports`, then platform shutdown/detach before primary-window destruction.
Explicit shutdown is retryable when outstanding renderer epochs prevent texture reset; dropping
the route defers cleanup to Context teardown and does not make the renderer slot reusable.
[S1, S3, S17, S18]

## Focused verification for the selected repair

The implementation needs a finite test set: clipped/zero-count images produce no sample; two
active images yield only the actual sampled views; target replacement cannot match an old view;
same-view repeated writes reject stale receipts; Device/Queue mismatch and replay reject; a
secondary present remains accepted when primary acquisition skips; reconfigure/discard never
publishes stale pending evidence; recreated surfaces have a new lifetime; shutdown releases the
renderer before windows. Native qualification must then detach a real Nara image, resize, redock,
close, and shut down on Windows. No source parser or generalized proof framework is required.

Upstream source tests already cover trace normalization, foreign prepared-runtime rejection,
renderer-before-platform destruction, explicit surface recovery actions, and external texture
replacement/invalidation. They support the owner contract but do not establish Nara qualification.
[S16, S19]

## Source map

Upstream links are pinned to `af2942c78651b280d18af621de55b5462bb1e10a`. Line ranges are one-based.

- S1: [`renderer/multi_viewport.rs:121-258`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport.rs#L121): route ownership, attach, prepare, render, external textures and shutdown.
- S2: [`multi_viewport_runtime/runtime.rs:1058-1079,1235-1298`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport_runtime/runtime.rs#L1058): prepared capability and preparation/render transaction.
- S3: [`multi_viewport_runtime/callbacks.rs:164-172,225-235,499-546,682-780`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport_runtime/callbacks.rs#L499): attach rejection and actual secondary acquire/draw/submit/present points.
- S4: [`render/frame.rs:169-188`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/dear-imgui/src/render/frame.rs#L169): combined platform-window transaction.
- S5: [`multi_viewport_runtime/surface.rs:10-56,227-284,314-329`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport_runtime/surface.rs#L10): pending frame, drop order, configure/recreate and recovery policy.
- S6: [`multi_viewport_runtime/trace.rs:3-30`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport_runtime/trace.rs#L3); `multi_viewport_runtime/tests.rs:39`: exact public report and thread restrictions.
- S7: [`renderer/render.rs:68-164`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/render.rs#L68); `renderer/draw.rs:20-40`: public consumed-frame renderer, private draw-data renderer and actual-extent requirement.
- S8: [`docs/CUSTOM_BACKENDS.md:303-320`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/docs/CUSTOM_BACKENDS.md#L303): native instance identity versus mutable viewport ID.
- S9: [`renderer/draw.rs:105-125,216-330,438-448`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/draw.rs#L216): accepted elements, scissor/texture resolution and actual draw issuance.
- S10: [`platform_io/renderer_callbacks.rs:34-76,108-169,179-214,224-310`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/dear-imgui/src/platform_io/renderer_callbacks.rs#L34); `draw/list/raw.rs:12-28`: unsafe callback contract.
- S11: [`dear-imgui-winit/src/multi_viewport.rs:113-121`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-winit/src/multi_viewport.rs#L113); `multi_viewport/registry.rs:320-355`; `multi_viewport/viewport_data.rs:350-397`: private authoritative Arc lookup and raw pointer source.
- S12: [`renderer/multi_viewport_winit_adapter.rs:6-38`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport_winit_adapter.rs#L6): window borrow and unsafe surface lifetime.
- S13: [`dear-imgui-winit/src/multi_viewport/runtime/lifecycle.rs:451-500`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-winit/src/multi_viewport/runtime/lifecycle.rs#L451): exact-generation renderer adapter, scoped event loop and custom-route fault duty.
- S14: [`docs/CUSTOM_BACKENDS.md:327-376,384-454`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/docs/CUSTOM_BACKENDS.md#L327); `context/attachment.rs:94-148`; `context/core.rs:140`: callback lease, attachment, teardown and aggregate ABI. Exact-tag `git grep` found no public viewport renderer/backend trait; `render/snapshot.rs:133` is the sealed consumer capability.
- S15: [`renderer/external_textures.rs:5-57`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/external_textures.rs#L5); `texture/cache.rs:26-57`: immediate registration replacement/removal and lifetime obligations.
- S16: [`tests/integration_test.rs:505-550,625-710`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/tests/integration_test.rs#L505); `texture/tests.rs:245-262`: texture lifecycle tests.
- S17: [`renderer/lifecycle.rs:181-262`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/lifecycle.rs#L181); WGPU `README.md:136-146,197-227`: same-device invalidation, actual loss and retryable ordered shutdown.
- S18: [`multi_viewport_runtime/runtime.rs:855-914`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport_runtime/runtime.rs#L855); Winit `multi_viewport/runtime/lifecycle.rs:522-530`: renderer-before-platform release.
- S19: [`multi_viewport_runtime/tests.rs:1084-1124,1380-1401,1778-1805,2064-2097,2148-2173`](https://github.com/Latias94/dear-imgui-rs/blob/af2942c78651b280d18af621de55b5462bb1e10a/backends/dear-imgui-wgpu/src/renderer/multi_viewport_runtime/tests.rs#L1380): runtime identity, teardown, trace and surface tests.
- N1: `crates/nara_render_wgpu/src/editor_execution.rs:414-464,585-636,790-841,1068-1165`: same-device scoped handles, actual primary encoding and receipt join.
- N2: `crates/nara_render_wgpu/src/presented.rs:81-117,162-235`; `editor_execution.rs:159-173,486-538`: submitted-image, window and composite identity contracts.
- N3: `crates/nara_render_wgpu/src/backend.rs:717-765` at U5: primary acquire and configured extent.
- N4: `crates/nara_window/src/lib.rs:16-41`: independent Nara window ID namespace.
- W1: locally installed `wgpu-30.0.0/src/api/device.rs:19-26`, `queue.rs:14-21`, `texture_view.rs:15-23`: `impl_eq_ord_hash_proxy!` implements handle equality. See the [exact WGPU 30 API source](https://docs.rs/wgpu/30.0.0/src/wgpu/api/device.rs.html).
