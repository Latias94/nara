---
type: "Verification Evidence"
title: "Fixed world canvas and native UI qualification"
description: "Pixel readback, presented world input, native button resize and independent exported desktop qualification."
timestamp: 2026-10-20T07:07:52Z
record_id: "e7c7f1ee29e4459688d51f1ca20282d8"
producer_id: "codex-root"
run_id: "fixed-world-canvas-20261020"
related_plan: "docs/plans/2026-10-20-fixed-world-canvas-plan.md"
git_branch: "main"
git_commit: "472329658829ec5cb8523717e232e844e02b8b94"
---

# Verification

Source: `472329658829ec5cb8523717e232e844e02b8b94`, based on main `d9af441`.
Windows 11, Rust 1.95.0, DX12 for GPU/native acceptance, shared target and one Cargo job.
Root runs Cargo/fmt/Git serially; three implementation lanes have disjoint ownership and review
other owners. Untracked vendor content is preserved.

## Outcome And Regressions

- Optional `[render.world_canvas]` lowers into the existing Render slot through the same
  `WorldCanvas` configuration as Rust consumers. Positive extent validation, profile replacement/
  clear conflicts and unchanged Window/UI settings are covered. Headless retains inert data
  without requesting presentation. All contributing Render slots share one canonical definition.
- World views raster at the configured extent, the completed image is nearest-sampled into the
  actual destination, and native-resolution UI follows. Runtime windows and embedded Game View
  use the same private preparation/encoder; one source frame is admitted once.
- Configuration is captured atomically with ExtractedViews, closing a real Extract/capture race.
  Pointer conversion retains the exact presented camera, rejects bars/nonfinite/max edges, and
  checks current runtime/configuration/window geometry. Window event batches and observed
  mismatches revoke permanently, including zero-size followed by restored geometry.
- Actual odd/fractional/undersized readback initially exposed physical centering round-trip,
  missing content scissor and interpolated-UV midpoint drift. Physical centering, half-open
  pixel-center scissor, and a per-prepared-draw immutable 16B physical rectangle now align shader
  sampling with input inversion. World/bar colors remain exact; only the existing translucent
  native UI sRGB conversion has a one-channel-value tolerance.
- Independent review found that Editor bridge guards were not initially reached by device-loss
  callbacks. Invalidate-only handles now join existing native Runtime/Editor view device owners.
  Prepared-to-Presented CAS cannot reactivate a revoked guard after a callback. Final view/source/
  device retirement revokes; destination-only resize retains the fixed raster allocation.
- Every resource is checked before handle use or initial mapping. Texture/pipeline and late
  mapping/binding failures preserve the prior complete cache allocation and refund preparation.
  Zero-sized windows skip before mapping/bounds, including explicit camera viewports.
- The first native button fixture omitted UiButton's required UiVisual. Its rejected projection
  made the observer panic across the native callback (`c98be635-51e4-4337-ac64-bb3e523f8038`).
  The fixture now supplies the required public component and returns diagnostics through normal
  workflow cleanup. Production projection checks were not relaxed. This failure is not evidence
  of a stack or allocator defect.

## Passing Cohorts

| Scope | Result | Nextest run ID |
| --- | --- | --- |
| `nara_project --all-features --lib` | 45 passed | `5fcc0e07-1601-4acc-bb41-0a2edee8929d` |
| Render/WGPU all-feature libraries, serial actual GPU | 175 passed; 3 ignored native cases excluded | `0fd51415-02f3-4a16-bcd1-79e709ee4f90` |
| Root/Editor selected composition, private world retirement and font consumers | 68 passed; 458 excluded | `b758d567-0d9f-4117-8ca4-dffdcc2c00a4` |
| WGPU no-default-features clear-canvas readback | 1 passed; 58 excluded | `a6233911-a280-4101-ac1f-61397440e411` |
| Final actual Windows/DX12 Editor trajectories | 3 passed; 185 excluded | `8a75fc08-fe77-4add-9cee-57839893b98d` |
| Generated independent Check/Export/native restart | 1 passed; 15 excluded | `28a17c68-a18a-4997-97b3-52607885d0cc` |

The final native selection executes:

1. `native_fixed_world_canvas_keeps_ui_input_resize_and_step_coherent`: real submitted/composited
   Game images, actual Runtime UiButton pointer press/release through the existing native route,
   exactly one journal activation before/after grip resize, Pause/Resume/Step, held-input release,
   Game close/reopen and Stop. The probe requires real UI surface extent, not the world canvas size.
2. `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`: original
   no-canvas behavior remains qualified after the shared test fixture changes.
3. `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`: independent views,
   native lifecycle and recovery retain exact image/composite receipts and physical cleanup.

The external test creates an independent generated Cargo project, configures canvas and selected
font through ordinary source/profile APIs, checks without starting gameplay, exports saved source,
then runs headless and two fresh desktop processes. Each desktop proves selected glyph submission,
native physical UI extent, fixed extracted world extent, and WorldPointer through an actual present.
Its 386.081 seconds includes nested Cargo builds and is one acceptance run, not an iteration or
frame-performance benchmark. Artifact: `target/nara-profile-font-78700-1/font-candidate`.

## Commands And Source Boundaries

All commands use `CARGO_BUILD_JOBS=1`. GPU/native commands additionally use `WGPU_BACKEND=dx12`;
the external/native tests set `CARGO_TARGET_DIR` to this checkout's existing `target` directory.

```text
cargo +1.95.0 nextest run -p nara_project --all-features --lib --locked
cargo +1.95.0 nextest run -p nara_render -p nara_render_wgpu --all-features --lib --locked --test-threads=1 --no-fail-fast
cargo +1.95.0 nextest run -p nara -p nara-editor --all-features --lib --test plugin_composition --test project_runtime_boot --locked -E 'test(world_canvas) | test(world_presentation_tests) | test(profile_font) | binary(plugin_composition)' --test-threads=1
cargo +1.95.0 nextest run -p nara_render_wgpu --no-default-features --lib --locked -E 'test(canvas::tests::) | test(device_loss_revokes_presented_world)' --test-threads=1
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_fixed_world_canvas_keeps_ui_input_resize_and_step_coherent) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts)' --test-threads=1 --no-fail-fast
cargo +1.95.0 nextest run -p nara --all-features --test project_runtime_boot --locked --run-ignored only -E 'test(fixed_world_canvas_and_font_recipe_survive_check_export_and_native_restart)' --test-threads=1
cargo +1.95.0 check --workspace --all-features --all-targets --locked
cargo +1.95.0 check -p nara --no-default-features --features runtime-core --all-targets --locked
cargo +1.95.0 check -p nara_render_wgpu --no-default-features --features sprite-submitter --all-targets --locked
cargo +1.95.0 check -p nara_render_wgpu --no-default-features --features ui-submitter --all-targets --locked
cargo +1.95.0 fmt --all -- --check
git diff --check
```

All listed build/format/diff checks pass. Existing feature-dependent unused/dead-code warnings remain;
this is not a warning-free build claim. Final native test-only fixture extraction was compiled and
executed after the workspace check; it preserves the already passing product behavior.
Raw logs remain under `target/fixed-world-canvas-*.log` and are not committed.

## Remaining Scope

This qualifies a fixed world with native UI on Windows/DX12. It does not qualify a low-resolution
HUD, Linux/macOS release, physical OOM, monitor scanout, a render graph, complete GPU residency or
P99/Rust iteration performance. Telemetry excludes Editor view-owned canvases/image targets,
composite/material uniforms and driver allocations; future measurement must account for them.
The two desktop launches prove process restart, not arbitrary in-process device reconstruction.
Tile tools remain deferred. No claim that future engine refactoring is unnecessary follows.

[Plan](../../../../plans/2026-10-20-fixed-world-canvas-plan.md),
[ADR 0117](../../../../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md),
[Native procedure](../../../../verification/native-editor-qualification.md).
