---
type: "Verification Evidence"
title: "Native composition physical pump handoff and retained stock input qualification"
description: "Production native-window ownership, sequential Source/Processed runs and independent numeric composition qualified with honest platform and performance limits."
timestamp: 2026-10-31T22:55:17Z
record_id: "f0cd93c17a3c47fa905993cee07e6c5c"
producer_id: "codex-foundation-main"
run_id: "window-composition-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "2d9f251"
---

# Verification

Implementation: `2d9f251740e5a7ed6ca2a9b7e0a0df33af3a0565`, based on `b34741c` on
`refactor/foundation`. Tests executed the equivalent uncommitted Rust source before that commit;
the final independent output suite also executed after it. Subsequent documentation closure
does not change the tested code. Windows11, Rust1.95.0, locked dependencies, root `target`,
`CARGO_BUILD_JOBS=1`, serial Cargo and test threads. Native selections use `WGPU_BACKEND=dx12`,
`NARA_WGPU_FORCE_FALLBACK=0` and the actual desktop; the Dear adapter gate additionally requires
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.

The new independent lock has no shared dependency version outside the root lock. No remote
publication, compatibility reader/alias or additional migration guide was introduced.

# Result

The selected native-composition unit is qualified in the scopes below. Public native window and
image-use scopes retain one Host's submission/presentation authority, bounded original custody
and privately issued receipts. The read-only sealed-input reader retains replayable CPU meaning
without taking the original transport or UI sidecar. Producer policy slots live in fallible heap
tables, preserving stable indices and charging their full retained capacities.

One move-only, thread-bound physical Winit loop now outlives a logical Runtime. Only full native
retirement returns it; incomplete close retains the original owner. An unbound initial failure
closes its Runtime/driver without pumping an unused supplied loop. Normal logical retirement
keeps the physical loop alive, handoff drains old queued events without a Runtime/window owner,
and a private nominal session-wake ticket rejects late old accessibility wakes. The first app
pump is nonblocking before a successor can adopt configured Wait behavior.

This does not complete full R8/R9a, promote Proposed0121 or finish the foundation Goal. The same
external Scene effect in ordinary Runtime, Editor Scene View and offscreen, with crisp HUD and
exact final UI binding, still needs the public stock-composition successor.

# Evidence

| Executed scope | Actual result | Run identity |
| --- | --- | --- |
| Independent complete content/window/drawing workspace, all ordinary and explicitly ignored native cases | 6 passed, 0 skipped; 11.349s | `b38d9f75-1f3a-4e1f-b784-daa8855a9c59` |
| Winit all-features library, ordinary plus all ignored native cases | 60 passed, 0 skipped; 14.425s | `8a20e35f-a1d8-499e-a69e-2850dd0a44bb` |
| Root desktop close tests and public thread/runner/desktop-owner contract | 9 passed, 1188 skipped; 1.076s | `72e75485-68a9-4ea7-aa84-21fee6355ea2` |
| Broad render/selected Editor group | 424 passed; one release-only measurement incorrectly selected in debug refused its prerequisite; 296 skipped | `5971e89a-deba-4948-84da-18f78df4b9ed` |
| Explicit final window and affected Editor gate | 15 passed, 706 skipped; 28.264s | `2194b10f-89c6-4019-9bf0-404f69a33164` |
| Independent public image-output suite, after implementation commit | 3 passed, 0 skipped; 2.745s | `b16a824d-a776-426e-8788-ab7602d4c411` |
| Root/Winit/render/Editor all-features library correctness Clippy | Passed, 55.98s, advisory warnings retained | `cargo clippy ... -- -D clippy::correctness` |
| Root without default features, only desktop-winit/render-wgpu library | Passed, 5.44s | `cargo check -p nara --no-default-features --features desktop-winit,render-wgpu --lib --locked` |
| Renderer without default features on wasm32-unknown-unknown | Passed, compile only, 2.93s | `cargo check -p nara_render_wgpu --no-default-features --target wasm32-unknown-unknown --locked` |
| Root and both affected independent workspace formatting | Passed | `cargo fmt --all -- --check`, corresponding manifest checks |

Counts overlap and are not additive. The broad command's exit status is failure because
`backend::runtime_measurement::tilemap_and_changing_hud_costs` explicitly requires `--release`
and a distinct measurement output. Its guard was not changed, no optimized measurement was run,
and no performance result is inferred. The final green gate explicitly selects correctness/native
trajectories rather than that measurement.

The independent Source/Processed example uses both ordinary content Hosts. Source actually
presents red, requests/commits the declared later Scene, seals green, empties live Image/prepared
entries and actually presents that retained green input. After full Source retirement, a failed
source-manifest admission returns the same unused physical loop; the Processed Host then repeats
the real publication/capture flow on that loop. Original Runtime/source/frame, native Host/epoch,
Scene occurrence and provenance facts are checked against actual post-Host receipts. Both
Runtimes and owned backing retire before four original inputs execute in interleaved order on
one offscreen Host. A replacement Host repeats another order. Every retained red/green output is
read numerically after all four publish; foreign producer and candidate-owner requests refuse.
Window receipts supply actual presentation/identity facts; this does not read original window
framebuffers or prove monitor scanout.

The two-window/image consumer executes deterministic30/60/144 schedules across720 logical ticks.
They are not measured frame rates. Paused/closed A retains exact A1 pixels while B and image
composition advance. Distinct mirrors retain immutable placement snapshots and reject stale joins.
Acquired paint refusal, paint panic and pre-present panic preserve original-prefix responsibility
and later recovery. Native minimize/resize requests remain separate manager observations.

The clipped drawing consumer uses explicit fractional DPI, half-open clipping, letterbox geometry,
translucent/alpha-zero controls and a front popup. A separate f64 pixel oracle checks RGBA32Float
readback at1e-6 tolerance. Its readback is a declared Feature importing exact typed output; the
Host retains that import and submission. A real second-effect byte refusal leaves old outputs
readable, B progressing and the same refused capture eligible for retry.

The final Editor gate includes all three critical trajectories from the native procedure, the
fixed-WorldCanvas input/resize/Step flow, delegated-window exact receipt checks and selected
Canvas/image owners. It does not qualify every Editor feature or the future custom-drawn toolkit.

## Corrections And Review

- Actual initial two-window execution aborted with `0xc00000fd`; a256KiB CPU stack regression
  reproduced it. Large inline64/1024 policy tables amplified producer BTreeMap nodes and callback
  stack use. Heap tables corrected the owning representation; the final consumer and stack
  regression pass. No larger example stack or discounted retained capacity was used.
- Independent review corrected old-prefix maintenance before new metadata admission and isolated
  an unpublished native result's destructor while preserving its original error and cleanup count.
- Physical-loop review found the Wait-before-first-callback cycle on running X11/Wayland pumps.
  The first zero-time app pump closes that source-level cycle. No Linux/macOS execution evidence
  is supplied by this Windows run.
- Source fixtures were corrected to include real task capacities and use production Runtime Scene
  cooking, not hand-built incomplete wire. Cross-World instance checks compare the full domain/
  occurrence identity; local `SceneInstanceId` sequences may legitimately repeat.
- The large stock-custody fixture now respects the fixed64MiB metadata ceiling, rather than
  assuming `usize::MAX` can bypass it. Independent ordinary tests still check complete custody
  counts. Exact two-output admission is proved before separately admitting readback metadata;
  no product cap was loosened to obtain a passing observation.
- Final independent native-window/content and physical-loop/Root reviews report no remaining
  P1/P2 in their selected scopes. A bounded App/Tasks/Asset/Reflect/UI storage audit found no
  additional analogous inline-table candidate; this is not a general platform stack guarantee.

# Follow-up

Keep the complete foundation Goal active. The next public stock-composition unit must replace
fixed world/Canvas/UI lowering, concrete stock-only collectors and hardcoded Scene/Canvas policy
construction through actual external blur/second-effect consumers. Preserve exact captured
definitions, backpressure, original UI sidecars and explicit world-raster/resolved-target pixels.
Materials, authored Schema/Prefab evolution, joint execution/motion and the editable Nara-drawn
Inspector retain their existing independent acceptance. Native Linux/macOS, browser execution,
actual OS IME/accessibility and general device-loss/platform coverage remain outside this record.

# Citations

- [Render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Production integration and next stock seam](../../../../architecture/render-program-production-design.md)
- [Physical native ownership](../../../../architecture/render-host-ownership-design.md)
- [Native Editor procedure](../../../../verification/native-editor-qualification.md)
- [Independent consumer](../../../../../tests/fixtures/render-composition-consumer/README.md)
- [Input research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
