---
type: "Verification Evidence"
title: "RGF U11 safe surface retirement verification"
description: "Safe owning wgpu surfaces, owner-scoped Winit retirement, device-loss invalidation, and truthful failure aggregation verified."
timestamp: 2026-07-13T21:53:23Z
record_id: "7873dc371c574fa3a0330be2b39ca589"
tags: ["rgf-u11", "render", "winit", "wgpu", "lifecycle", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4821ce8"
verified_by: "codex-root"
---

# Verification

RGF-U11 replaces raw-handle snapshots and unsafe static surface construction with a registered
provider, one atomically issued non-cloneable native surface owner plus control lease, and an owner-
scoped retirement driver. The native owner acknowledges release from `Drop`; callers cannot create
an untracked provider clone or fabricate a surface-drop acknowledgement. Every acquisition carries
a non-reused target generation, so a stale lease cannot retire or acknowledge its replacement.
Native device-loss handling remains at the admitted detection-and-invalidation slice: an unavailable
backend cannot silently re-enter device initialization without reconstruction.

# Result

Passed the focused lifecycle gates on Windows. The implementation establishes
`Active -> RetireRequested -> SurfaceRetired -> ProviderReleased -> NativeDestroyed` for controlled
shutdown, preserves `ExternallyDestroyed` as a distinct sticky fault, and retains both a primary
runner error and a native teardown error through `AppRunError::RunnerTeardown`.

# Evidence

## Automated gates

- `cargo nextest run --locked -p nara_app -p nara_window -p nara_winit -p nara_render_wgpu
  --test-threads=1`: 87 passed.
- `cargo nextest run --locked -p nara --no-default-features --features
  desktop-winit,render-wgpu --test window_surface_retirement --test-threads=1`: 11 passed.
- `cargo nextest run --workspace --locked --test-threads=1`: 643 passed, 3 explicitly skipped.
  U11's added normal
  dependencies required regenerating the two checked-in root-consumer fixture lockfiles; both
  nested locked-Cargo contract tests passed after that synchronization.
- `cargo clippy --locked -p nara_app -p nara_window -p nara_winit -p nara_render_wgpu
  --all-targets --no-deps -- -D warnings -A clippy::too_many_arguments`: passed.
- `cargo check --workspace --locked`: passed.
- `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1`: 5 passed.
- The `windowed_clear`, `windowed_sprites`, and `runtime_ui_panel` examples passed their exact
  no-default-feature capability checks.
- The smoke example's two unit tests passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed; Git reported only existing line-ending
  conversion warnings.
- Dependency searches kept every `winit` import in `nara_winit` and every `wgpu` import in
  `nara_render_wgpu`; the root facade contains only optional adapter wiring/re-exports.
- `cargo run --locked -p nara --features desktop-winit,render-wgpu --example
  window_surface_retirement_smoke`: presented a frame, reconfigured from `320x180` to `400x240`,
  and reached `NativeDestroyed`.
- The same smoke with `-- --drop-backend-before-exit` proved that direct removal of
  `WgpuRenderBackend` uses the Drop fallback before provider/native release. A distinct atomic flag
  is written only when `remove_resource` returns the backend, so this mode cannot pass by silently
  falling back to the normal cleanup path.

An exploratory all-workspace/all-features `-D warnings` Clippy run is not a U11 gate and remains
blocked by pre-existing `nara_asset` lints plus an unchanged all-features UI test construction error.
The affected-crate strict gate, workspace check, and workspace test suite all pass.

## Partial-initialization ownership audit

| Boundary | Owner before publication | Failure behavior | Evidence |
|---|---|---|---|
| Instance and adapter request | Stack-local `Instance`; adapter result is local | No backend-native object is published | `WgpuRenderBackend::ensure_device` source audit |
| Device and queue request | Stack-local instance and adapter | Locals drop on request failure; backend fields remain empty | `ensure_device` source audit |
| Surface binding acquisition | Registry retains the provider and atomically issues one local non-cloneable handle source plus generation-bound lease | Duplicate/retiring target rejection publishes no second owner; dropping an unpublished binding clears `surface_active`; stale leases cannot affect a replacement | unique/stale-binding integration tests |
| Surface creation | Safe `Instance::create_surface(handle_source)` consumes the tracked owner | On failure wgpu drops the consumed handle source, which acknowledges owner release while the registered provider remains live and can issue a new binding | dynamic safe-create failure test and source audit |
| Binding-to-state handoff | Local safe surface already owns the handle source; the paired lease remains local | Infallible `WgpuSurfaceState` construction and map insertion transfer surface and lease together | source audit plus lease-only backend tests |
| Surface configure | Published `WgpuSurfaceState` owns the safe surface and control lease | Configuration errors flow through the same render-system `Result -> mark_error -> clear_gpu_resources` path; actual owner Drop precedes lease confirmation | production wiring source audit plus shared invalidation tests; direct `SurfaceUnsupported` injection is not claimed |
| Unexpected device loss | Published backend owns all current surfaces and device-dependent state | One serialized render-system invalidation clears surfaces/caches and enters `Unavailable`; later frames do not auto-initialize | `Schedule -> render_wgpu_surfaces` device-loss test and unavailable-state tests |
| Runner/event-loop failure | Winit owns only its registered target IDs and platform windows | Finalization requests retirement through the registered scoped driver, releases only acknowledged providers, then drops platform owners; global plugin cleanup remains in `App::run` and no unobserved `NativeDestroyed` is fabricated | owned/foreign driver tests and runner failure aggregation tests |
| Unsolicited native destruction | Winit mapping plus shared target authority still identify the owned target | Records sticky `ExternallyDestroyed`, publishes one `Closed`, retires only Winit-owned surfaces/providers, preserves the primary failure, and treats a repeated event as a no-op | event-loop-independent production transition test |

Adapter/device/surface candidates are deliberately not exposed through a production fault-injection
factory. They remain unpublished RAII locals until an atomic handoff. Surface configuration remains
fallible after publication, but it converges on the dynamically tested shared backend invalidation
path. A dedicated configuration failpoint becomes necessary if that path diverges or configuration
begins yielding across frames.

# Follow-up

- Full device epoch correlation, bounded recovery, and explicit retry remain outside U11 and require
  the ADR 0078 admission evidence.
- Linux/macOS destruction timing and a real unsolicited OS-destroy callback remain unclaimed; the
  current platform smoke evidence is Windows-only.
- The in-process smoke deadline cannot interrupt synchronous GPU initialization; CI should retain an
  external process watchdog around both smoke modes.
- U13 owns single-target frame admission and one acquire/submit/present transaction; this unit does
  not claim that desktop transaction is complete.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`, RGF-U11.
- `crates/nara_window/src/backend.rs`.
- `crates/nara_winit/src/lib.rs`.
- `crates/nara_render_wgpu/src/backend.rs`, `crates/nara_render_wgpu/src/lib.rs`, and
  `crates/nara_render_wgpu/src/surface.rs`.
- `tests/window_surface_retirement.rs`.
- `examples/window_surface_retirement_smoke.rs`.
- ADRs 0032, 0040, 0042, and 0078.
