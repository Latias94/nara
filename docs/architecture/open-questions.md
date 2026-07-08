# nara Architecture Open Questions

**Status**: Living Draft
**Created**: 2026-07-08

This document tracks decisions that should be discussed before the next implementation pass. It is
not an ADR. Once a topic is decided, move the decision into `docs/architecture/adr/`.

## 2D-First vs Unified 2D/3D Runtime

Accepted direction: dimension-aware runtime with 2D-first authoring. See ADR [0005-dimension-aware-runtime-with-2d-first-authoring.md](adr/0005-dimension-aware-runtime-with-2d-first-authoring.md).

The tension:

- 2D-first gives the best first-hour user experience: `Sprite`, `Transform2d`, `Camera2d`,
  tilemaps, layers, sorting, simple orthographic defaults.
- A unified 2D/3D runtime can avoid future 3D retrofit pain, but it introduces earlier complexity:
  general transforms, projections, meshes, materials, visibility, render phases, culling, and
  render graph decisions.

Follow-up details still to settle:

1. What exact data lives in `View` versus `Camera2d`?
2. Should transform propagation be one generic internal traversal or two systems sharing utilities?
3. How should tilemap chunks choose between generated mesh buffers and instanced quads?
4. Which render phases live in `nara_render` versus `nara_render_wgpu`?

Design principle from discussion:

- Do not avoid mature engine mechanisms merely because they add code. Prefer correct, extensible
  deep modules over short-term scaffolding when the decision affects renderer, scene, asset,
  reflection, or plugin foundations.

## Plugin Dependency and Error Policy

Accepted direction: nara owns plugins and app lifecycle, with staged plugin lifecycle and diagnostics-aware failure. See ADR [0010-plugin-lifecycle-dependencies-and-failure.md](adr/0010-plugin-lifecycle-dependencies-and-failure.md).

Terminology note: `Plugin` means a Bevy-style Rust engine module/capability package, not a Zed-style WASM extension. WASM scripting is separate; see ADR [0021-scripting-and-wasm-boundary.md](adr/0021-scripting-and-wasm-boundary.md).

Still open:

1. Do plugins declare dependencies by type/label?
2. Are duplicate plugins ignored, rejected, or allowed?
3. Does `Plugin::build` stay infallible, or do backend plugins need a fallible init phase?
4. Does runner initialization belong in plugins, `App::run`, or a platform adapter?

## Component Metadata Details

Accepted direction: Bevy-reflect-backed `ComponentRegistry` with stable schema IDs and migrations. See ADR [0004-use-bevy-reflect-backed-component-metadata.md](adr/0004-use-bevy-reflect-backed-component-metadata.md) and ADR [0011-component-schema-ids-and-migrations.md](adr/0011-component-schema-ids-and-migrations.md).

Still open:

1. What derive should a data-facing component need?
2. How does nara define stable schema IDs?
3. How are component migrations represented?
4. Does the registry emit JSON Schema, a custom compact schema, or both?
5. Which components are inspectable but not serializable?

## Scene and Prefab Semantics

Accepted direction: scene and prefab files are dimension-neutral ECS data documents with stable scene-local entity IDs. See ADR [0006-scene-and-prefab-data-model.md](adr/0006-scene-and-prefab-data-model.md).

Follow-up details still to settle:

1. Primary hand-authored format: RON or JSON?
2. Stable scene entity IDs: UUID, integer local IDs, or path-like IDs?
3. Are prefab overrides whole-component first, or field-level from day one?
4. How are nested prefab overrides addressed?
5. How does scene loading validate AI-generated data before spawning ECS entities?

## Asset Identity

Accepted direction: typed handles with UUID-ready asset identity. See ADR [0007-asset-identity-and-import-pipeline.md](adr/0007-asset-identity-and-import-pipeline.md).

Follow-up details still to settle:

1. What exact serialized shape should `AssetRef` use?
2. Where do `.meta` files live, if any?
3. Are imported artifacts content-addressed?
4. Does Phase 1 expose async asset states now or only reserve them in types?

## Runtime Concurrency

Accepted direction: engine-owned task pools with explicit main-thread integration. See ADR [0008-runtime-concurrency-and-task-pools.md](adr/0008-runtime-concurrency-and-task-pools.md).

Follow-up details still to settle:

1. Should task pools live in `nara_tasks` or inside `nara_app` initially?
2. What exact stages tick IO/async results?
3. How are task cancellation and asset unload handled?
4. Do plugins access task pools through resources or app methods?
5. Should networking/scripting use a separate runtime model later?

## Diagnostics and Logging

Accepted direction: diagnostics are first-class structured data and logging uses `tracing`. See ADR [0009-diagnostics-errors-and-logging.md](adr/0009-diagnostics-errors-and-logging.md).

Follow-up details still to settle:

1. What exact diagnostic code namespace does nara use?
2. How are source spans represented for JSON/RON scene files?
3. Which diagnostics are warnings versus hard errors during hot reload?

## Render Crate Boundaries

Accepted direction: split render domain, backend, sprite, tilemap, and sprite-render responsibilities. See ADR [0012-render-crate-boundaries.md](adr/0012-render-crate-boundaries.md). Render graph policy is phase-based first and graph-ready later; see ADR [0017-render-graph-policy.md](adr/0017-render-graph-policy.md).
The next implementation slice uses main-world explicit extraction data and backend handle providers rather than a separate render world; see ADR [0032-render-backend-integration-boundary.md](adr/0032-render-backend-integration-boundary.md).

Follow-up details still to settle:

1. Which crates are created immediately versus temporarily collapsed?
2. What trait or data shape represents queued render items once sprite/tilemap rendering starts?
3. What concrete second use case should trigger full `RenderGraph` implementation?

## Platform and Runner

Accepted direction: `nara_app` owns runner traits, `nara_window` owns normalized window data, and `nara_winit` is the adapter. See ADR [0013-platform-window-and-runner-boundaries.md](adr/0013-platform-window-and-runner-boundaries.md).
The next implementation slice uses a fallible owned-app runner, backend-only raw handle providers, and optional facade features for `winit`/`wgpu`; see ADR [0032-render-backend-integration-boundary.md](adr/0032-render-backend-integration-boundary.md).

Follow-up details still to settle:

1. How does fixed timestep interact with winit redraw requests and control-flow mode?
2. How much raw platform event access should advanced users get?

## Editor and Tooling

Accepted direction: the editor is a client of runtime APIs and should dogfood nara runtime/rendering concepts; UI toolkit dogfooding is phased. See ADR [0015-editor-tooling-and-dogfooding-boundary.md](adr/0015-editor-tooling-and-dogfooding-boundary.md).

Runtime UI is nara-owned ECS UI; egui/dear-imgui-rs are allowed for early debug/editor tooling only. See ADR [0025-runtime-ui-system.md](adr/0025-runtime-ui-system.md).

Editor/AI authoring changes use validated patch transactions with undo/redo support. See ADR [0026-editor-command-patch-and-undo-model.md](adr/0026-editor-command-patch-and-undo-model.md).

Follow-up details still to settle:

1. What is the first editor-facing command/patch format?
2. Does debug UI use egui first or dear-imgui-rs first?
3. What is the minimum `WorldSnapshot` needed for inspector work?
4. What minimum runtime UI is required before editor dogfooding?
5. What typed value representation should patch payloads use?

## Backend and Domain Extension Seams

Accepted direction: stable ECS data plus plugin/backend adapter seams. See ADR [0016-extension-seams-for-backends-and-domain-modules.md](adr/0016-extension-seams-for-backends-and-domain-modules.md).

Follow-up details still to settle:

1. What is the first stable 2D physics component set?
2. Should physics stepping be fixed-timestep only?
3. Which backend adapter should be spiked first: Box2D, Rapier, or Avian?
4. How are fake/test backends registered?

## Coordinate, Units, and Time

Accepted direction: world units, 2D Y-up, radians, fixed timestep simulation, render interpolation. 3D uses right-handed Y-up with default forward `-Z`. See ADR [0018-coordinate-units-and-time.md](adr/0018-coordinate-units-and-time.md) and ADR [0022-3d-coordinate-system.md](adr/0022-3d-coordinate-system.md).

Follow-up details still to settle:

1. What default pixels-per-unit should `Camera2d` examples use?
2. Which helpers make pixel-perfect 2D easy?
3. How should UI/screen coordinates be separated from world coordinates?
4. What coordinate presets should asset importers support first?

## Physics

Accepted direction: high-level nara physics components plus replaceable backend adapters; physics runs in fixed timestep. See ADR [0019-physics-strategy.md](adr/0019-physics-strategy.md).

Follow-up details still to settle:

1. Which 2D backend should be spiked first?
2. What exact collision event model should nara expose?
3. How does physics interpolation integrate with render interpolation?

## Project Layout

Accepted direction: `nara.toml` plus conventional `assets/`, `scenes/`, `prefabs/`, `scripts/`, and `.nara/` generated cache directories. See ADR [0020-project-layout-and-package-format.md](adr/0020-project-layout-and-package-format.md).

Follow-up details still to settle:

1. What fields are required in `nara.toml`?
2. Which directories are configurable?
3. What template should `nara new` generate later?

## Event, Command, Determinism, and Replay

Accepted direction: ECS-native messages/events and deferred commands, plus deterministic-friendly fixed-step simulation. See ADR [0023-event-message-and-command-model.md](adr/0023-event-message-and-command-model.md) and ADR [0024-determinism-fixed-update-and-replay-policy.md](adr/0024-determinism-fixed-update-and-replay-policy.md).
The next implementation slice sets the first fixed timestep default to 1/60 second with bounded catch-up, exposed through `FixedTime` and testable `run_once(Duration)`.

Follow-up details still to settle:

1. What exact event retention policy should nara use?
2. Do fixed-update and frame-update events use separate channels?
3. What data is required for a future replay capture?

## Runtime UI

Accepted direction: nara builds its own runtime ECS UI. See ADR [0025-runtime-ui-system.md](adr/0025-runtime-ui-system.md).

Follow-up details still to settle:

1. Flexbox-like layout, grid, or custom retained layout?
2. How does UI relate to scene hierarchy and cameras?
3. What text shaping/rendering libraries are acceptable?

## Save, Networking, Animation, Audio, Text

Accepted directions:

- Save games are separate runtime persistence, not scene files. See ADR [0027-save-game-and-runtime-persistence.md](adr/0027-save-game-and-runtime-persistence.md).
- Networking is a Phase 1 non-goal but the engine stays replication-ready. See ADR [0028-networking-and-replication-scope.md](adr/0028-networking-and-replication-scope.md).
- Animation is asset-driven and component-targeted. See ADR [0029-animation-strategy.md](adr/0029-animation-strategy.md).
- Audio uses stable authoring components/commands with backend adapters. See ADR [0030-audio-strategy.md](adr/0030-audio-strategy.md).
- Text/font is a dedicated engine domain. See ADR [0031-text-and-font-strategy.md](adr/0031-text-and-font-strategy.md).

Follow-up details still to settle:

1. What exact persistent entity ID model should save games use?
2. What replication metadata should components eventually expose?
3. What is the first sprite animation asset shape?
4. Which audio backend should be spiked first?
5. Which text shaping/font stack should nara use?
