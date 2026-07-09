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

Resolved in the foundation hardening slice:

- `Plugin::build` is fallible and returns `PluginError`.
- `App::add_plugin` rejects duplicate unique plugins.
- Plugin groups can use `App::add_plugin_if_missing` when idempotent composition is intended.
- Backend and domain prerequisite failures return structured plugin errors instead of panicking.
- Runner initialization remains owned by app runners and platform adapters, not by ordinary plugin
  build code.

Still open:

1. Do plugins declare dependencies by type/label before they run?
2. What plugin metadata should groups expose for diagnostics, editor tooling, and generated docs?

## Component Metadata Details

Accepted direction: Bevy-reflect-backed `ComponentRegistry` with stable schema IDs and migrations. See ADR [0004-use-bevy-reflect-backed-component-metadata.md](adr/0004-use-bevy-reflect-backed-component-metadata.md) and ADR [0011-component-schema-ids-and-migrations.md](adr/0011-component-schema-ids-and-migrations.md).

Still open:

1. What derive should a data-facing component need?
2. Does the registry eventually emit JSON Schema in addition to the current compact schema catalog?
3. Which components are inspectable but not serializable?

Resolved in the scene/prefab serialization foundation:

- `ComponentRegistry` now owns `ComponentValue` and preflight/apply component codecs.
- Data-facing components can be serializable through explicit codecs without requiring runtime
  components that contain handles to derive direct serde or Bevy Reflect.
- Built-in scene, transform, render, sprite, and tilemap codecs use stable reverse-domain
  `ComponentTypeId` strings such as `nara.transform.Transform2d`.
- `ComponentSchemaCatalog` is the current compact schema export format.
- Component migrations are registered as one-step `ComponentValue` transforms and composed by
  `ComponentRegistry` before scene/prefab preflight.
- Serializable registrations require explicit field schemas.
- Duplicate Rust `TypeId` registrations are rejected instead of silently replacing codecs.
- Schema defaults are checked against their declared field value kinds during registration.
- The reflection crate is split into focused value, schema, path, codec, migration, and registry
  modules.
- Field paths are structured `ComponentFieldPath` values with `Field` and `Index` segments.

## Scene and Prefab Semantics

Accepted direction: scene and prefab files are dimension-neutral ECS data documents with stable scene-local entity IDs. See ADR [0006-scene-and-prefab-data-model.md](adr/0006-scene-and-prefab-data-model.md).

Follow-up details still to settle:

1. How does hot reload cache and invalidate asset-backed prefab sources once async IO exists?

Resolved in the scene/prefab serialization foundation:

- JSON is the AI/tooling format and RON is the Rust-native hand-authored format; both share
  `SceneDocument` / `PrefabDocument`.
- Stable scene entity IDs are validated path-like strings stored as `SceneEntityId`, not runtime
  `Entity` values.
- Scene loading preflights IDs, parent graph, component registrations, component versions, payloads,
  and asset refs before mutating the target world.
- `ScenePatchDocument` is the first editor/AI authoring mutation format. It serializes as `op +
  args`, validates atomically on a scratch document, and returns inverse patches.
- `SceneAuthoringSession` integrates patch transactions with live `World` projection through
  document-as-truth apply, undo/redo inverse stacks, dirty tracking, and rebuild-style sync that
  preserves unrelated runtime entities.
- Prefab overrides are field-level patch transactions relative to source prefab IDs. They apply
  before expanded IDs are namespaced.
- Nested prefab expansion uses `PrefabSourceResolver`. The first adapter is
  `InMemoryPrefabSourceResolver`; missing sources, source cycles, and excessive depth emit
  diagnostics before spawn.
- Expanded prefab IDs use the deterministic `anchor/source_entity` rule. Repeated prefab instances
  get collision-free source namespaces.
- The old whole-component prefab override API was removed before 1.0.

## Asset Identity

Accepted direction: typed handles with UUID-ready asset identity. See ADR [0007-asset-identity-and-import-pipeline.md](adr/0007-asset-identity-and-import-pipeline.md).
The next concrete implementation direction connects asset import to render resource preparation; see ADR [0033-asset-import-and-render-resource-preparation-seam.md](adr/0033-asset-import-and-render-resource-preparation-seam.md).

Follow-up details still to settle:

1. What exact `.meta` schema fields are required in the first slice?
2. Should `AssetServer` expose `LoadState` immediately, or should load state live in a separate project asset database resource first?
3. Which import profile fields belong in artifact cache keys for desktop-only Phase 1?

Resolved in the scene/prefab serialization and asset/render seam foundations:

- `AssetRef::Path` and `AssetRef::StableId` are both semantic persistent reference shapes. Paths
  remain useful for hand-authored files; stable IDs resolve through `ProjectAssetDatabase`.
- Asset paths are project-asset-root-relative logical paths using `/`, rejecting empty, absolute,
  drive-prefixed, backslash, `.` and `..` traversal forms.
- `Handle<T>` no longer serializes as runtime `AssetId`; persistent scene data uses `AssetRef`.
- Scene/prefab spawn keeps asset resolution two-phase: component codecs use `ComponentDecodeContext`,
  unknown stable IDs report entity/component/field/asset diagnostics, and failed preflight does not
  allocate scene entities or commit scratch `AssetServer` state.

Resolved by ADR 0033:

- Source asset `.meta` files live beside source assets first, for example
  `assets/textures/player.png.meta`.
- Generated imported artifacts live under `.nara/import-cache/` and are not hand-authored source
  data.
- Import artifact identity is content-addressed by stable asset ID, source content hash, importer
  ID/version, import settings hash, and target/import profile when relevant.
- Backend GPU objects are not imported artifacts; they live in backend resource caches.

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

Resolved in the foundation hardening slice:

- `DiagnosticReport::push` only collects structured diagnostics.
- Logging is an explicit bridge through `Diagnostic::emit_to_tracing` or
  `DiagnosticReport::emit_to_tracing`.
- Runtime code that needs inspection should pass diagnostic reports/resources instead of relying on
  implicit log side effects.

## Render Crate Boundaries

Accepted direction: split render domain, backend, sprite, tilemap, and sprite-render responsibilities. See ADR [0012-render-crate-boundaries.md](adr/0012-render-crate-boundaries.md). Render graph policy is phase-based first and graph-ready later; see ADR [0017-render-graph-policy.md](adr/0017-render-graph-policy.md).
The next implementation slice uses main-world explicit extraction data and backend handle providers rather than a separate render world; see ADR [0032-render-backend-integration-boundary.md](adr/0032-render-backend-integration-boundary.md).
Texture upload and material/resource growth should use the asset import + render resource preparation seam; see ADR [0033-asset-import-and-render-resource-preparation-seam.md](adr/0033-asset-import-and-render-resource-preparation-seam.md).

Follow-up details still to settle:

Implemented in the 2D render foundation slice:

- `nara_sprite`, `nara_tilemap`, and `nara_sprite_render` are real crates, while `nara_render`
  keeps cameras, views, targets, frame lifecycle, and phase labels.
- The first queued render shape is concrete data, not a trait: `ExtractedSprites`,
  `QueuedSpriteItems`, and `SpriteBatches`. Backends consume batches rather than gameplay
  authoring components.
- `nara_render_wgpu` draws colored and textured quad instance batches and remains the only crate
  that imports `wgpu`.
- `nara_render` exposes `RenderBackendStatus`, `RenderBackendState`, and skipped-frame reasons as
  backend observation resources.
- `nara_render_wgpu` updates render backend status for uninitialized, missing-window, rendering,
  and backend-error states.
- The unused public `RenderBackend` trait was removed; the current backend seam is
  plugin-installed resources, systems, and status observations until a second backend or test
  adapter creates real abstraction pressure.

Still open:

1. What abstraction generalizes `SpriteBatches` once runtime UI, gizmos, text, or 3D submit their
   own phase items?
2. What concrete second pass/resource use case should trigger full `RenderGraph` implementation?
3. What material input shape should sit above image textures once sprites need sampler/material
   overrides?

Resolved by ADR 0033:

- Texture upload, atlases, materials, UI images, and future 3D assets attach through asset import,
  backend-neutral render resource preparation, and backend-owned GPU resource caches.
- The first backend-neutral texture resource is `ImageAsset` prepared into
  `PreparedImageResource`; sprites and tilemaps carry typed handles and UVs.
- `nara_render_wgpu` owns textures, samplers, bind groups, buffers, and pipeline cache details.
- Gameplay/domain crates store typed handles or backend-neutral descriptors, not backend handles.

## Platform and Runner

Accepted direction: `nara_app` owns runner traits, `nara_window` owns normalized window data, and `nara_winit` is the adapter. See ADR [0013-platform-window-and-runner-boundaries.md](adr/0013-platform-window-and-runner-boundaries.md).
The next implementation slice uses a fallible owned-app runner, backend-only raw handle providers, and optional facade features for `winit`/`wgpu`; see ADR [0032-render-backend-integration-boundary.md](adr/0032-render-backend-integration-boundary.md).

Follow-up details still to settle:

1. How does fixed timestep interact with winit redraw requests and control-flow mode?
2. How much raw platform event access should advanced users get?

## Editor and Tooling

Accepted direction: the editor is a client of runtime APIs and should dogfood nara runtime/rendering concepts; UI toolkit dogfooding is phased. See ADR [0015-editor-tooling-and-dogfooding-boundary.md](adr/0015-editor-tooling-and-dogfooding-boundary.md).

Runtime UI is nara-owned ECS UI; egui/dear-imgui-rs are allowed for early debug/editor tooling only. See ADR [0025-runtime-ui-system.md](adr/0025-runtime-ui-system.md).

Editor/AI authoring changes use validated patch transactions with undo/redo support. The first
live sync boundary is `SceneAuthoringSession`, which projects a document into a managed live
`World` slice by rebuild. See ADR
[0026-editor-command-patch-and-undo-model.md](adr/0026-editor-command-patch-and-undo-model.md).

The first UI-agnostic inspector model is `SceneInspectorState` in `nara_tooling`. It consumes
`SceneAuthoringSession`, `ComponentRegistry`, and optional `WorldSnapshot`, then emits
`SceneInspectorCommand` values that apply scene patches.

Play Mode uses an isolated runtime `World` spawned from a validated edit document snapshot. Stop
Play discards runtime changes by default; Apply Changes is explicit and patch-based. See ADR
[0034-editor-play-mode-world-boundary.md](adr/0034-editor-play-mode-world-boundary.md).

Resolved in the Play Mode core slice:

- `SceneAuthoringSession` exposes an opaque source revision stamp that advances on successful
  authoring document mutations and not on sync, failed patches, empty undo/redo, or live-world
  cleanup.
- `nara_tooling::SceneEditorState` owns the first UI-agnostic `Edit` / `Play` / `Paused` model.
  It starts isolated Play sessions through the same plain, prefab-resolved, asset-aware, and
  combined `SceneSpawner` paths as scene loading.
- Mode-aware inspector commands keep direct edit-mode patch behavior but reject persistent patch
  commands in Play or Paused. Selection remains a safe UI state change.
- `SceneApplyChangesReport` is currently a diagnostic guard only. It reports unsupported
  apply-back when the source revision still matches and revision mismatch when the authoring
  document changed after Play started.

Resolved in the first debug UI adapter slice:

- Early debug/editor UI uses egui first through `nara_tooling_egui`.
- `nara_tooling_egui` renders UI-agnostic `SceneEditorModel` and `SceneInspectorModel` values and
  returns explicit editor actions plus `SceneInspectorCommand` values. It does not own scene
  mutation, ECS storage, windowing, or GPU resources.
- dear-imgui-rs remains an acceptable later adapter, and runtime UI remains nara-owned ECS UI
  rather than egui/dear-imgui.

Follow-up details still to settle:

1. What minimum runtime UI is required before editor dogfooding?
2. Which accepted patch operations need specialized incremental `WorldCommand` paths before
   rebuild-style projection is too expensive?
3. What is the first supported Apply Changes subset for Play Mode: selected entity fields,
   selected component, or whole scene diff?

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
