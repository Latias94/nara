# ADR 0032: Render Backend Integration Boundary

**Status**: Accepted
**Date**: 2026-07-08

## Context

nara now has a Bevy ECS substrate, nara-owned app lifecycle, and render-facing authoring data.
The next foundation slice introduces real desktop window and wgpu surface lifetimes, so the engine must decide where window handles, extracted render data, backend initialization, and facade feature gates live before code hardens the wrong boundary.

## Decision

Phase 1 uses explicit main-world extraction data and backend adapter crates rather than a full render world or full RenderGraph.

Rules:

- `nara_app` owns the fallible runner contract: runners consume `App` and return `Result<AppExit, AppRunError>`.
- `nara_window` owns normalized window data and backend-only raw-window-handle provider types keyed by `WindowId`.
- `nara_winit` is the only crate that depends on `winit`; it owns live platform windows and registers backend handle providers.
- `nara_render` owns graph-ready render targets, viewport rectangles, extracted views, render phase labels, and frame lifecycle data, but no `wgpu` types.
- Extracted render data is frame-local, rebuilt or cleared during `Extract`, not serialized, and not exported through the gameplay prelude initially.
- `nara_render_wgpu` is the only crate that depends on `wgpu`; it consumes `nara_window::backend` providers and guarantees surfaces are dropped before the provider/window guard.
- The root `nara` facade keeps `winit` and `wgpu` behind explicit optional features. Default `MinimalPlugins` stays headless and backend-free.

wgpu initialization may use `pollster` for the first native-desktop slice.
The backend should still model `Uninitialized`, `Initializing`, `Ready`, and `Unavailable` states so web/mobile and async task integration remain additive later.

## Alternatives Considered

### Option A: Build a separate render world now

**Pros**: Closer to Bevy's mature render architecture; easier to isolate GPU resources later.

**Cons**: Requires sub-app/render-world scheduling before nara has sprite extraction, asset preparation, or multiple render passes. It would add complexity without a second concrete render use case.

**Decision**: Deferred. Extracted data names should mirror future render-world concepts so the later migration is internal to render crates.

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
| Backend isolation | `winit` appears only in `nara_winit`; `wgpu` appears only in `nara_render_wgpu` | Dependency and import search |
| Default facade cost | Root facade without default features does not include `winit` or `wgpu` | `cargo tree -p nara --no-default-features` |
| Surface safety | Surface creation consumes backend handle providers whose guard outlives the surface | Code review and tests |
| Extraction locality | Extracted render data is cleared or rebuilt each frame and stays out of gameplay prelude | Unit tests and API review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Raw handle lifetime is modeled unsafely | High | Medium | Store a provider guard and document teardown order: surface before provider/window |
| Main-world extracted data becomes gameplay API | High | Medium | Keep `Extracted*` out of prelude and mark it renderer-domain/frame-local |
| Blocking GPU init freezes platform loop | Medium | Medium | Restrict `pollster` to native desktop and model backend initialization states |
| Feature gates leak backend dependencies | Medium | Medium | Add facade feature and cargo-tree checks |

## Citations

- Render crate boundaries: [0012-render-crate-boundaries.md](0012-render-crate-boundaries.md)
- Platform and runner boundaries: [0013-platform-window-and-runner-boundaries.md](0013-platform-window-and-runner-boundaries.md)
- Render graph policy: [0017-render-graph-policy.md](0017-render-graph-policy.md)
- Determinism and fixed update: [0024-determinism-fixed-update-and-replay-policy.md](0024-determinism-fixed-update-and-replay-policy.md)
