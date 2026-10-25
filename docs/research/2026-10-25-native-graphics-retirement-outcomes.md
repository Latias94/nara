# Native Graphics Retirement Outcome Inventory

Date: 2026-10-25. Source research for the native follow-up to the
[owner-local close design](../architecture/native-close-ownership-design.md). No native failure
injection or GPU qualification follows from this inventory.

Sources: Nara at `2c13c67` plus the local Rust-close working changes; locked Dear ImGui `0.21.0`,
WGPU `30.0.1` and wgpu-core `30.0.0`. SDK paths below are relative to their downloaded crate
source, not the untracked repository `vendor/`. The root manifest has no vendor patch. Research
agent `01a17355-a7db-77c0-8156-04bec88829f1` inspected the downstream implementations read-only.

## Why A Plain Error Cannot Drive The State Machine

The concrete teardown APIs can fail before mutation, retain partial shutdown custody, or report
an error after reaching physical release. Error severity alone does not determine whether a
provider may be destroyed or whether a method can safely be called again.

| Operation | Observed source behavior | Consequence for a retained owner |
| --- | --- | --- |
| `ImguiTarget::retire` (`editor/src/platform/imgui.rs`) | Takes the texture token before fallible unregister. On failure the view/metadata are still present. | Preserve the token until completion is established, or use final bulk renderer shutdown and explicitly retire adapter references afterward. |
| WGPU renderer external-texture unregister | Backend/contract checks can reject before removing a mapping. Missing texture means no mapping exists. Native route has a post-call `finish_entry` that can return an error after the mapping changed. | Do not infer token disposition from `Err` alone. Current Nara adapter removes its binding only on `Ok`. |
| Docked renderer shutdown (`renderer/lifecycle.rs`) | Context validation precedes taking the renderer consumer. Failed reset preparation puts the consumer back; success clears textures/backend/bindings. | Failed preflight retains retryable custody; record success and avoid re-running a detached Docked renderer. |
| Native renderer shutdown (`renderer/multi_viewport_runtime/runtime.rs`) | Preflight can retain everything; viewport cleanup can retain `ShuttingDown`; callback release or an old queued fault can be reported after resource drop/detach. | The idempotent native path can be driven again, retaining all errors exposed to the Host. SDK internal first-error retention can already lose later errors in one call. Do not classify completion by error name. |
| Nara renderer adapter shutdown (`crates/nara_render_wgpu/src/editor_execution/imgui.rs`) | Delegates shutdown but does not clear its `bindings` TextureView clones. | Downstream shutdown is not proof that all adapter references were released. |
| Platform disable/shutdown (`multi_viewport/runtime/lifecycle.rs`, `platform/ownership.rs`) | Active renderer attachment rejects release. Retryable queued faults can reject before teardown. Other errors can be returned after `finish_shutdown`/base detach. Detached shutdown can return `Ok` on retry. | Retain the platform until its actual runtime/base state permits release; renderer completion precedes platform release. |
| GPU idle wait (`crates/nara_render_wgpu/src/backend.rs`) | Checks device loss, submits empty work to flush uploads, then waits at most five seconds. Timeout is not completion; the loss path rejects again for the same epoch. | Do not retain a lost epoch forever waiting for an idle API that cannot succeed. Device-loss disposal needs a separate proof. |
| Surface retirement (`crates/nara_render_wgpu/src/surface.rs`) | Takes the surface owner, requests retirement, drops it and attempts to verify owner release even when request/verification reports an error. Actual authority acknowledgment comes from `WindowSurfaceHandleSource::drop`. | A returned error does not preserve that same surface owner for retry. Owner destruction, its acknowledgment and subsequent verification are distinct facts. |
| Provider release (`crates/nara_window/src/backend.rs`) | Validates phase before taking the provider. Secondary work separately holds surface/window chain leases. | Early-release rejection retains provider custody. Platform detach alone does not prove final OS window destruction. |

The possible native unregister postflight failure is an interface path, not a demonstrated normal
Nara execution failure. WGPU device loss also does not establish a resource-safety defect: final
queue Drop has its own loss/idle handling. The missing fact is a qualified, observable Host close
protocol, not proof that every existing destructor is unsafe.

The context attachment boundary also distinguishes preflight/begin failure, before native destroy
starts, from postflight/end failure, after the destroy call ends. The latter alone does not prove
all Rust work leases or OS windows have been released (`dear-imgui-rs/src/context/attachment.rs`).

## Host-Owned Successor To Design

`NativeEditorHost` should distinguish `Absent`, `Live` and `Retiring`. Retiring owns the full local
graphics aggregate, reason, phase, accumulated errors and the means to continue progress. It
stops new render admission, keeps the primary window and maintenance/event pumping available,
and refuses conflicting reinstallation until the predecessor physically retires.

Both current entry points need that drive: recovery's `retire_native` takes the complete aggregate
before fallible shutdown and then drops it; ordinary `run_application` calls shutdown after its
event loop returns. The successor must preserve required progress capability for ordinary exit as
well as recovery/reinstallation.

The candidate fixed sequence is drain, renderer bulk shutdown, release adapter bindings and
Scene/Game views, platform shutdown, then primary surface/provider retirement. Preserve completed
phases rather than repeating non-idempotent operations. Prefer typed owner/lease completion over
runtime IDs or a universal graph. This sequence is a candidate pending actual backend outcome
validation; device-loss disposal is a distinct branch.

Partial construction must retain cleanup custody too. Inspect both `NativeImgui::new` and the
renderer adapter constructor: a failure that encounters another cleanup failure currently falls
into Drop rather than returning a retained retiring aggregate. Context Drop can abort when SDK
shutdown phases remain incomplete; it is not a retry protocol.

The Editor Host owns this aggregate. It is not moved into an unrelated Play Runtime merely to use
the new local participant boundary.

## Discriminating Verification

- Inject one drain timeout, then allow completion; provider and aggregate remain retained.
- Exercise real outstanding-renderer epochs, viewport cleanup failure and foreign sidecar
  preflight; block reinstallation and complete after releasing the named blocker.
- Distinguish a queued pre-teardown fault from a completed callback-drift error while retaining
  both diagnostics and true completion state.
- Cover unregister refusal, missing mapping and downstream-complete/adapter-binding-retained
  states with explicit token/reference observations.
- Observe surface-owner Drop separately from failed retirement/confirmation and provider release.
- Extend the actual Windows/DX12 detach/minimize/reopen/recover trajectory for exit before first
  presentation, partial initialization, delayed retirement, explicit recovery and secondary work
  present/discard/drop.
- Prove device-loss disposal separately from successful idle and state exact native/OS limits.

These checks are future work. The local Rust comparison cannot supply their SDK or native evidence.
