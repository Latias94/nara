# ADR 0032: Render Backend Integration Boundary

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-07-16
**Refines**: ADR 0012
**Refined By**: ADR 0040: Render Resource Lifetime and Submitter Ownership; ADR 0044: Root Facade
and Prelude Layering Policy; ADR 0053: Visibility, Culling, and Tilemap Render Cache (Superseded); ADR 0077:
Render Pipeline Recipes, Graph Compilation, and Backend Encoding (Superseded); ADR 0078: Render
Host Affinity, WebGPU Initialization, and Device Recovery; ADR 0094: Minimal Render Execution
Boundary and Evidence-Gated Extensions; ADR 0096: Evidence-Gated Render Scaling and Upload Policy

## ADR 0096 Refinement

Current extraction names should describe current ownership and data, not mirror a hypothetical
future render world. Arbitrary phase/pass construction is not an extension promise until an external
feature can participate through capture, ordering, encoding, target lifetime, and diagnostics using
only public APIs.

## Context

nara now has a Bevy ECS substrate, nara-owned app lifecycle, and render-facing authoring data.
The next foundation slice introduces real desktop window and wgpu surface lifetimes, so the engine must decide where window handles, extracted render data, backend initialization, and facade feature gates live before code hardens the wrong boundary.

## Decision

Phase 1 uses explicit main-world extraction data and backend adapter crates rather than a full render world or full RenderGraph.

Rules:

- `nara_app` owns the fallible runner contract: `App::run` consumes `App`, while the installed runner
  borrows `&mut App` and returns `Result<AppExit, AppRunError>` before once-only plugin cleanup.
- `nara_window` owns normalized window data, safe owning handle providers, and the backend target
  lifecycle keyed by `WindowId`. It never stores an unowned raw-handle snapshot or grants provider
  release before renderer acknowledgement. Atomic acquisition issues only one non-cloneable native
  surface owner plus its control lease. The native owner acknowledges actual release from `Drop`;
  the lease may request retirement and verify that acknowledgement but cannot fabricate it. This
  exclusivity is enforced per shared target authority and `WindowId`; the executable/platform host
  must not register one native target in independent authorities.
- Backend-neutral app, window, input, and render crates do not depend on `winit`. `nara_winit` is the
  first-party desktop integration and current direct-dependency gateway: it owns live platform
  windows, registers owning handle providers, and releases provider/native ownership only after
  surface retirement. An external Platform/Runner may use another platform stack or a future public
  exact-version `nara_winit` bridge. The first docking/multi-viewport or external-winit tracer decides
  whether an exact direct dependency is also required; external runner reachability is not a
  first-party reservation. Each runner calls the backend-neutral retirement driver and waits only
  for targets it successfully registered. It does not invoke global plugin cleanup to drive a local
  target transition.
- `nara_render` owns graph-ready render targets, viewport rectangles, extracted views, render phase labels, and frame lifecycle data, but no `wgpu` types.
- `nara_render` owns the backend-neutral `RenderBackendStatus` resource for backend name,
  readiness state, last error, and skipped-frame reason. This status resource is the current
  backend observation seam, not a speculative render backend trait.
- Extracted render data is frame-local, rebuilt or cleared during `Extract`, not serialized, and not exported through the gameplay prelude initially.
- Backend-neutral engine, gameplay, domain-render, frame-transfer, and tooling crates do not depend
  on `wgpu`. `nara_render_wgpu` is the first-party wgpu Host Adapter and the only stock crate that
  owns exact wgpu device/queue/target execution. A future exact-wgpu integration or replacement
  authority requires its own admitted trust, version, ordering, epoch, target, and teardown
  contract under ADR 0094; dependency isolation alone does not promise such a public API.
- The stock `nara_render_wgpu` Adapter consumes `nara_window::backend` non-cloneable surface handle
  sources by value through wgpu's safe owning surface path, registers the scoped retirement driver,
  and writes wgpu skipped-frame/backend-error state into `RenderBackendStatus`. The actual handle
  owner acknowledges from `Drop`, including when the backend resource is removed or replaced; the
  paired lease then verifies the transition. The current native surface system and retirement
  driver are main-thread operations; broader host placement remains governed by ADR 0078.
- The root `nara` facade keeps `winit` and `wgpu` behind explicit optional features. Default `MinimalPlugins` stays headless and backend-free.

wgpu initialization may use `pollster` for the first native-desktop slice.
The backend should still model `Uninitialized`, `Initializing`, `Ready`, and `Unavailable` states so web/mobile and async task integration remain additive later.

## Alternatives Considered

### Option A: Build a separate render world now

**Pros**: Closer to Bevy's mature render architecture; easier to isolate GPU resources later.

**Cons**: Requires sub-app/render-world scheduling before nara has sprite extraction, asset preparation, or multiple render passes. It would add complexity without a second concrete render use case.

**Decision**: Deferred. Current extracted data names describe current ownership and payloads; they
must not be shaped around a hypothetical render world. A later migration may rename or replace
pre-1.0 render-crate types when a real second-world workflow exists.

### Option B: Let `nara_render_wgpu` depend on `nara_winit`

**Pros**: Simple first surface creation with `Arc<winit::window::Window>`.

**Cons**: Couples renderer backend to one platform adapter and blocks future web/mobile/headless surface providers.

**Decision**: Rejected. `nara_render_wgpu` consumes backend handle providers from `nara_window`, not `winit` windows.

### Option C: Full RenderGraph from day one

**Pros**: Maximum pass/resource flexibility.

**Cons**: Premature for clear-pass and first sprite renderer work; conflicts with ADR 0017.

**Decision**: Rejected for Phase 1.

### Option D: Main-world explicit extraction with backend handle providers (Chosen)

**Pros**: Pressures real `winit`/`wgpu` lifetimes, keeps gameplay APIs clean, and leaves room for a future render world.

**Cons**: Requires discipline to keep extracted data renderer-owned and frame-local.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Backend isolation | Winit dependency/import authority remains isolated to the platform Adapter boundary and exact wgpu dependency/import authority remains isolated to the advanced backend boundary; neither enters backend-neutral/core/domain crates, and any separately admitted external runner/interop/Host capability stays inside its owning boundary | Dependency/import search plus role-specific fixtures when that capability is admitted |
| Default facade cost | Root facade without default features does not include `winit` or `wgpu` | `cargo tree -p nara --no-default-features` |
| Surface safety | Safe surface creation owns a tracked non-cloneable handle source; scoped retirement plus owner-Drop fallback prove surface -> provider -> native target ordering | Lifecycle, replacement, and platform smoke tests |
| Extraction locality | Extracted render data is cleared or rebuilt each frame and stays out of gameplay prelude | Unit tests and API review |
| Backend observability | Skipped frames and backend errors are visible without importing `wgpu` | `RenderBackendStatus` tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Raw handle lifetime is modeled unsafely | High | Medium | Atomically issue a tracked non-cloneable owner to safe `create_surface`; enforce scoped surface -> provider -> native target retirement |
| Main-world extracted data becomes gameplay API | High | Medium | Keep `Extracted*` out of prelude and mark it renderer-domain/frame-local |
| Blocking GPU init freezes platform loop | Medium | Medium | Restrict `pollster` to native desktop and model backend initialization states |
| Feature gates leak backend dependencies | Medium | Medium | Add facade feature, selected-Adapter, and cargo-tree checks; ordinary runtime/render-feature packages must remain backend-neutral |
| Backend status becomes wgpu-shaped | Medium | Medium | Keep public status categories backend-neutral; backend-specific detail remains a string or adapter-owned type |

## Citations

- Render crate boundaries: [0012-render-crate-boundaries.md](0012-render-crate-boundaries.md)
- Platform and runner boundaries: [0013-platform-window-and-runner-boundaries.md](0013-platform-window-and-runner-boundaries.md)
- Render graph policy: [0017-render-graph-policy.md](0017-render-graph-policy.md)
- Determinism and fixed update: [0024-determinism-fixed-update-and-replay-policy.md](0024-determinism-fixed-update-and-replay-policy.md)
