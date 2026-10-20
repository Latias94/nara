# Fixed World Canvas With Native UI

Status: implementation and behavioral qualification complete; delivery closure in progress.

## Outcome

An ordinary desktop game can select a fixed internal world resolution. The world is rendered
to that canvas and composited into the real window using the existing IntegerFit policy;
runtime UI remains at the real window resolution. Window dimensions, logical input and
fractional simulation coordinates remain truthful. Tile authoring tools remain deferred.

## Contract And Acceptance

- One optional configuration flows through Render and project composition. Native window size
  is independent of canvas extent. No public render graph or Editor execution dependency.
- A runtime frame is admitted once. World raster, window composite, native UI and present are
  one closed transaction. Nearest final sampling and centered bars use DisplayMapping2d.
- World pointer conversion uses the successfully presented camera and canvas mapping. Bars,
  nonfinite points and maximum edges are excluded. Resize, scale, suspension and runtime/device
  replacement invalidate obsolete presentation authority.
- Window-only resize reuses the fixed canvas allocation; extent/configuration changes and
  runtime retirement release the old allocation. Failed frames publish no new input authority.
- Verify actual checkerboard/bar pixels, world picking, native UI, fractional scale, odd and
  undersized dimensions, zero-size restoration, and an ordinary external Check/Play consumer.

## Parallel Lane Map

Mode: execute_direct, starting with bounded read-only interface discovery.
Repository/base: shared main checkout at d9af441. Preserve vendor and concurrent user changes.
Root owns all Cargo/fmt/Git, contracts, documentation, integration and final verification.
Reuse target with CARGO_BUILD_JOBS=1; never run Cargo concurrently.

| Lane | Responsibility | Initial writable files |
| --- | --- | --- |
| Root | Contract, integration, public/native acceptance and documentation | This plan and owning architecture documents |
| A | Render configuration, packet and presented world-coordinate API | None until interface agreement |
| B | WGPU world canvas, composite and native UI execution | None until interface agreement |
| C | Manifest/profile lowering and ordinary game ergonomics | None until interface agreement |

Implementation ownership after source agreement:

- A owns `crates/nara_render/`: optional canvas configuration, immutable packet facts, extraction
  and the renderer-neutral world-coordinate contract.
- B owns `crates/nara_render_wgpu/`: shared preparation/encoding, allocation, composition,
  presentation publication and focused actual GPU readback.
- C owns `crates/nara_project/`, `src/product/profile.rs` and relevant additions in
  `tests/plugin_composition.rs`: manifest/profile lowering and composition regressions. A later
  disjoint assignment owns the native button fixture in `game_input_tests/canvas.rs`.
- Root owns remaining product/facade/embedded Game View integration, ordinary external/native
  acceptance, all documentation, verification and delivery.

Workers are not alone in the checkout and must preserve other changes. No worker runs Cargo,
formats, stages or commits. Independent review uses a different lane from each implementation owner.

Integration refinements found by implementation and independent review:

- Canvas configuration travels atomically with ExtractedViews; capture does not reread a resource
  changed after camera extraction.
- Centering uses physical coordinates before conversion to logical points, avoiding fractional-DPI
  round trips at odd dimensions.
- Presentation state moves Prepared to Presented or permanently Revoked. Window event batches,
  observed geometry/configuration mismatch, target retirement and device-loss callbacks revoke it.
- The embedded Game View forwards only an invalidate-only handle to its existing GPU view/device;
  activation remains the Runtime bridge's exact-composite completion responsibility.
- Actual readback found viewport coverage outside the fitted image and interpolated-UV midpoint
  drift in an undersized window. Explicit half-open pixel-center scissor and a per-prepared-draw
  immutable physical-rectangle uniform make shader sampling use the same inversion as input.
- Zero-sized windows skip before canvas mapping and viewport bounds, including explicit camera
  viewports. Restoring geometry cannot revive a revoked frame without a fresh presentation.

## Qualification

Project tests: 45 passed. Render/WGPU all-feature libraries: 175 passed, three ignored native cases
excluded. Product composition, private world retirement and existing font consumers: 68 passed.
The no-submitter WGPU backend also passes real clear-canvas readback.

Three actual Windows/DX12 native Editor trajectories pass, including the fixed canvas's real
Runtime UI button activation before/after Game resize, ordinary no-canvas input, and view/device
retirement. The generated independent product passes strict Check, saved-source Export, native
UI physical-size and presented-world picking assertions, and two fresh desktop launches.
Workspace/all-feature/all-target, runtime-core-only and separate sprite/UI backend checks pass;
formatting and diff checks pass. Root retains exact commands/run IDs for the committed evidence.

No complete GPU residency, frame-tail/iteration benchmark, native Linux/macOS, low-resolution HUD
or Tile-tool qualification is implied. Future measurement must include the documented exclusions
for Editor canvas targets and composite/material uniforms.

## Source Basis

The preceding [ordinary authoring plan](2026-10-20-ordinary-authoring-workflow-plan.md) records
the initial source study and the user's selected native-UI policy. Reuse the existing presentation,
frame and first-party backend owners. Pinned Bevy pixel_grid_snap and Godot Window are local
comparison evidence, not authority to expand this stage into a general rendering framework.
