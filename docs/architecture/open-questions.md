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
Plugin metadata and default plugin groups are now settled at the policy level. Plugins expose
stable IDs, declared capabilities, optional requirements/conflicts, and inspectable group
membership; groups such as `CorePlugins`, `Runtime2dPlugins`, and desktop backend groups are
explicit product bundles. See ADR
[0046-plugin-metadata-and-default-plugin-groups.md](adr/0046-plugin-metadata-and-default-plugin-groups.md).

Resolved in the foundation hardening slice:

- `Plugin::build` is fallible and returns `PluginError`.
- `App::add_plugin` rejects duplicate unique plugins.
- Plugin groups can use `App::add_plugin_if_missing` when idempotent composition is intended.
- Backend and domain prerequisite failures return structured plugin errors instead of panicking.
- Runner initialization remains owned by app runners and platform adapters, not by ordinary plugin
  build code.

Still open:

1. What exact `PluginId` shape should the first implementation expose: reverse-domain string,
   type-backed label, or both?
2. Should `requires` name plugins, capabilities, resources, schedule sets, or all of them?
3. Which exact group should examples use as the default 2D desktop game bundle?

## Component Metadata Details

Accepted direction: Bevy-reflect-backed `ComponentRegistry` with stable schema IDs and migrations. See ADR [0004-use-bevy-reflect-backed-component-metadata.md](adr/0004-use-bevy-reflect-backed-component-metadata.md) and ADR [0011-component-schema-ids-and-migrations.md](adr/0011-component-schema-ids-and-migrations.md).
Field-level capability metadata is now settled by ADR
[0045-component-schema-capability-metadata.md](adr/0045-component-schema-capability-metadata.md):
component schemas describe domain eligibility for scene/save/inspect/edit/animate/replicate/script
and reference semantics, while each domain still owns behavior policy.

Still open:

1. What derive should a data-facing component need?
2. Does the registry eventually emit JSON Schema in addition to the current compact schema catalog?
3. What exact Rust API should capability registration use: bitflags, enum sets, or builder methods?
4. Which existing built-in fields should be editable versus inspect-only in the first
   implementation?

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
Document shape evolution is separate from component value evolution. Scene, prefab, and patch
documents migrate through explicit document-format migration chains before component migrations and
validation. See ADR
[0043-scene-prefab-and-patch-document-migration-policy.md](adr/0043-scene-prefab-and-patch-document-migration-policy.md).

Follow-up details still to settle:

1. How does hot reload cache and invalidate asset-backed prefab sources once async IO exists?
2. What first document migration registry API should `nara_scene` expose?

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

1. Which import profile fields belong in artifact cache keys for desktop-only Phase 1?
2. How should rename/move operations preserve stable IDs once the editor owns `.meta` lifecycle?
3. What project-level diagnostics should be emitted for repeatedly failing hot reloads?

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

Resolved in the async hot reload foundation:

- `AssetServer` exposes load states through `AssetStates`; domain asset storage such as
  `Assets<ImageAsset>` records successful loads, failed first loads, failed reloads, and removals.
- Source changes are coalesced through `AssetSourceChanges` and `SourceChangeResolver` into
  generation-stamped `AssetReloadRequest` values.
- Same-frame source changes use last-event-wins coalescing per logical path, and dependency
  propagation walks dependent source edges transitively.
- Typed import jobs use owned `ImportJobInput` values and return `ImportedAsset<T>` values through
  domain importers.
- Filesystem watching is optional and isolated in `nara_asset_watch`; raw watcher events are
  translated into semantic `AssetSourceChange` values before entering `nara_asset`.

## Runtime Concurrency

Accepted direction: engine-owned task pools with explicit main-thread integration. See ADR [0008-runtime-concurrency-and-task-pools.md](adr/0008-runtime-concurrency-and-task-pools.md).
Runtime services such as physics, audio, text shaping, scripting, networking, and file watching use
the same boundary: ECS data expresses intent, service/backends own native handles and threads, and
results integrate on the main thread. See ADR
[0042-runtime-service-and-backend-boundary.md](adr/0042-runtime-service-and-backend-boundary.md).

Follow-up details still to settle:

1. Should task pool worker sizing become app-configurable from `nara.toml` or stay explicit
   code-first setup only?
2. Should networking/scripting use a separate runtime model later?
3. What diagnostics should long-running or repeatedly failing tasks emit?

Resolved in the async hot reload foundation:

- Task pools live in `nara_tasks`.
- `nara_app::CoreStage::TaskUpdate` ticks task/result integration before `PreUpdate`.
- `TaskUpdateSet::{Poll, CoalesceAssetChanges, SpawnAssetJobs, ApplyAssetResults}` defines the
  first ordering contract for background work.
- `TaskCancellationToken` provides cooperative cancellation, and asset reload generations reject
  stale results before world/asset state mutation.
- Plugins access task infrastructure through ECS resources installed by `TaskPlugin`.

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
Render resource lifetime and submitter ownership are now settled at the policy level. Backend GPU
caches own native resources, avoid one-frame eager pruning as the product contract, recover from
generation invalidation/device loss, and keep sprite/UI/text submitters separate from device/surface
setup. See ADR
[0040-render-resource-lifetime-and-submitter-ownership.md](adr/0040-render-resource-lifetime-and-submitter-ownership.md).

Follow-up details still to settle:

Implemented in the 2D render foundation slice:

- `nara_sprite`, `nara_tilemap`, and `nara_sprite_render` are real crates, while `nara_render`
  keeps cameras, views, targets, frame lifecycle, and phase labels.
- The first queued render shape is concrete data, not a trait: `ExtractedSprites`,
  `QueuedSpriteItems`, and `SpriteBatches`. Backends consume batches rather than gameplay
  authoring components.
- `nara_render_wgpu` draws sprite/tilemap and runtime UI quad instance batches and remains the only
  crate that imports `wgpu`.
- `nara_render` exposes `RenderBackendStatus`, `RenderBackendState`, and skipped-frame reasons as
  backend observation resources.
- `nara_render_wgpu` updates render backend status for uninitialized, missing-window, rendering,
  and backend-error states.
- The unused public `RenderBackend` trait was removed; the current backend seam is
  plugin-installed resources, systems, and status observations until a second backend or test
  adapter creates real abstraction pressure.

Resolved in the runtime UI / pass-plan slice:

- `nara_ui_render` adds a second backend-neutral batch stream, `UiBatches`, instead of forcing UI
  through sprite authoring data.
- `nara_render::RenderPassPlan` is the first general pass-order contract for clear, world 2D, UI,
  and gizmo phases. Backends consume the plan rather than hardcoding UI/world order in private draw
  loops.

Still open:

1. What concrete second pass/resource use case should trigger full `RenderGraph` implementation?
2. What reusable material asset and shader-specialization model should sit above inline 2D material
   descriptors once projects need shared material files?
3. What exact cache eviction defaults should `nara_render_wgpu` use: grace frames, memory budget, or
   both?

Resolved by ADR 0033:

- Texture upload, atlases, materials, UI images, and future 3D assets attach through asset import,
  backend-neutral render resource preparation, and backend-owned GPU resource caches.
- The first backend-neutral texture resource is `ImageAsset` prepared into
  `PreparedImageResource`; images own content/import identity only.
- `nara_material` owns the first sampler/material authoring layer through `SamplerDescriptor`,
  `AlphaMode2d`, `Material2dDescriptor`, and material keys.
- Sprites and tilemaps carry material-first wrappers around typed image handles and UVs.
- `nara_sprite_render` batches by `SpriteMaterialKey`, not by image-resource-only keys.
- `nara_render_wgpu` owns textures, samplers, bind groups, buffers, and pipeline cache details, with
  image texture upload cached separately from material/sampler bind-group identity.
- Gameplay/domain crates store typed handles or backend-neutral descriptors, not backend handles.

Resolved in the async hot reload foundation:

- `nara_image::ImagePlugin` keeps stable image handles across reloads while updating asset versions
  and prepared-resource invalidation state.
- Image result application checks both reload generation and expected asset version before writing
  runtime asset data or failure state.
- Removed image sources clear both runtime image data and prepared image resources.

## Platform and Runner

Accepted direction: `nara_app` owns runner traits, `nara_window` owns normalized window data, and `nara_winit` is the adapter. See ADR [0013-platform-window-and-runner-boundaries.md](adr/0013-platform-window-and-runner-boundaries.md).
The next implementation slice uses a fallible owned-app runner, backend-only raw handle providers, and optional facade features for `winit`/`wgpu`; see ADR [0032-render-backend-integration-boundary.md](adr/0032-render-backend-integration-boundary.md).
Main-loop semantics are now settled at the policy level. Runners pass real elapsed time; nara lowers
that into real, virtual, fixed, and render-interpolation domains with explicit pause, time scale,
max-delta, fixed catch-up, state transition, background, and redraw policy. See ADR
[0039-main-loop-time-pause-and-runtime-state.md](adr/0039-main-loop-time-pause-and-runtime-state.md).

Follow-up details still to settle:

1. What exact public type names should nara expose for real, virtual, fixed, and interpolation
   time?
2. Should runtime state support stacks, independent domains, or only single typed states first?
3. How much raw platform event access should advanced users get?

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
The editor workspace layer is now settled by ADR
[0047-editor-workspace-and-scene-document-state.md](adr/0047-editor-workspace-and-scene-document-state.md):
`nara_tooling` should own UI-agnostic open-document slots, active document state, selection sets,
document revisions, dirty/saved state, external reload conflicts, and workspace commands.

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
- `SceneApplyChangesRequest` names one selected `SceneEntityId` plus explicit component IDs.
  Registered serializable components can export candidate `ScenePatchDocument` values from the
  isolated Play world and apply them through `SceneAuthoringSession`.
- Apply Changes records normal undo entries on success, reports supported no-op without undo, and
  rejects stale revisions, runtime-only components, missing scene entities, prefab-expanded
  entities, duplicate component requests, and patch validation failures with diagnostics.

Resolved in the first debug UI adapter slice:

- Early debug/editor UI uses egui first through `nara_tooling_egui`.
- `nara_tooling_egui` renders UI-agnostic `SceneEditorModel` and `SceneInspectorModel` values and
  returns explicit editor actions plus `SceneInspectorCommand` values. It does not own scene
  mutation, ECS storage, windowing, or GPU resources.
- dear-imgui-rs remains an acceptable later adapter, and runtime UI remains nara-owned ECS UI
  rather than egui/dear-imgui.

Resolved in the first runtime UI slice:

- `nara_ui` provides ECS UI authoring components, computed layout resources, and pointer
  hover/press/focus state.
- `nara_ui_render` turns computed UI panels into color/image batches using the same image prepare
  and material-key path as sprites.
- The first dogfooding threshold is now concrete: panels, images, clipping, pass ordering, and
  pointer hit testing exist, but text, widgets, richer layout, and editor viewport integration are
  still missing.

Resolved by ADR 0041:

- Input is layered as normalized platform events, retained device state, routing decisions, action
  maps, text/IME streams, UI focus/pointer capture, and accessibility semantics.
- UI/editor/gameplay input conflicts should be resolved through one routing/action model rather
  than private editor shortcut paths.
- Text input and IME composition are separate from key/button actions.

Follow-up details still to settle:

1. Which accepted patch operations need specialized incremental `WorldCommand` paths before
   rebuild-style projection is too expensive?
2. When should Apply Changes emit field-level patch operations instead of whole-component
   replacements?
3. How should prefab-expanded entity write-back produce source-prefab override patches?
4. What editor dogfooding milestone should switch a real panel from egui to nara UI?
5. What is the first minimal `EditorWorkspace` API: scenes only, or scenes plus prefab documents?
6. What external reload conflict workflow should exist before a full visual editor?

## Backend and Domain Extension Seams

Accepted direction: stable ECS data plus plugin/backend adapter seams. See ADR [0016-extension-seams-for-backends-and-domain-modules.md](adr/0016-extension-seams-for-backends-and-domain-modules.md).
The shared service/backend boundary is now codified by ADR
[0042-runtime-service-and-backend-boundary.md](adr/0042-runtime-service-and-backend-boundary.md):
components/resources express stable intent, services own native handles and queues, and background
work integrates through declared main-thread stages.

Follow-up details still to settle:

1. What is the first stable 2D physics component set?
2. Should physics stepping be fixed-timestep only?
3. Which backend adapter should be spiked first: Box2D, Rapier, or Avian?
4. How are fake/test backends registered?

## Coordinate, Units, and Time

Accepted direction: world units, 2D Y-up, radians, fixed timestep simulation, render interpolation. 3D uses right-handed Y-up with default forward `-Z`. See ADR [0018-coordinate-units-and-time.md](adr/0018-coordinate-units-and-time.md) and ADR [0022-3d-coordinate-system.md](adr/0022-3d-coordinate-system.md).
Runtime time semantics are refined by ADR
[0039-main-loop-time-pause-and-runtime-state.md](adr/0039-main-loop-time-pause-and-runtime-state.md):
real time, virtual/game time, fixed time, and render interpolation are separate domains.

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
Settings authority is now settled by ADR [0035-project-manifest-and-runtime-settings-authority.md](adr/0035-project-manifest-and-runtime-settings-authority.md): `nara.toml` is the file-backed project authority, with code-first resource/plugin overrides for embedded apps.

Follow-up details still to settle:

1. What exact TOML field names should the first manifest parser expose?
2. Should project stable identity be required immediately or optional until package/export exists?
3. What template should `nara new` generate later?

## Event, Command, Determinism, and Replay

Accepted direction: ECS-native messages/events and deferred commands, plus deterministic-friendly fixed-step simulation. See ADR [0023-event-message-and-command-model.md](adr/0023-event-message-and-command-model.md) and ADR [0024-determinism-fixed-update-and-replay-policy.md](adr/0024-determinism-fixed-update-and-replay-policy.md).
Channel lifetimes are now refined by ADR [0036-event-message-and-resource-queue-lifetime.md](adr/0036-event-message-and-resource-queue-lifetime.md): typed resource queues are acceptable when producer, consumer, retention, cleanup stage, and replay/diagnostic role are explicit.
Main-loop and pause semantics are now refined by ADR
[0039-main-loop-time-pause-and-runtime-state.md](adr/0039-main-loop-time-pause-and-runtime-state.md).
The next implementation slice should expose the refined time domains and state transition stage,
not only `FixedTime` and testable `run_once(Duration)`.

Follow-up details still to settle:

1. Should nara provide reusable `Events<T>` / `Requests<T>` wrappers with stage metadata?
2. Which channels belong in future deterministic replay capture first?
3. What data is required for a future replay capture?

## Runtime UI

Accepted direction: nara builds its own runtime ECS UI. See ADR [0025-runtime-ui-system.md](adr/0025-runtime-ui-system.md).
Input routing, action maps, text/IME, focus, pointer capture, and accessibility are now codified by
ADR
[0041-input-routing-actions-text-focus-and-accessibility.md](adr/0041-input-routing-actions-text-focus-and-accessibility.md).

Implemented first slice:

- UI is ordinary ECS data using `UiRoot`, `UiNode`, `UiPanel`, and `Parent` hierarchy components.
- Layout currently resolves simple absolute/percentage style values into runtime-only
  `ComputedUiLayouts` in top-left logical UI pixels.
- Pointer hover/press/focus state is runtime-only and fed by `PointerState`.
- UI rendering has its own extraction/queue/batch path and submits through the UI render phase
  after world 2D phases.

Follow-up details still to settle:

1. Which advanced layout model should come next: flexbox-like layout, grid, or a smaller retained
   layout model?
2. What exact editor dogfooding milestone should switch from egui panels to nara UI panels?
3. What text shaping/rendering libraries are acceptable?
4. How should UI/screen-space cameras, multiple viewports, and editor overlays compose once full
   render graph pressure arrives?
5. What is the smallest Phase 1 action-map schema that still supports rebinding and UI/gameplay
   context priority?

## Save, Networking, Animation, Audio, Text

Accepted directions:

- Save games are separate runtime persistence, not scene files. See ADR [0027-save-game-and-runtime-persistence.md](adr/0027-save-game-and-runtime-persistence.md).
- Networking is a Phase 1 non-goal but the engine stays replication-ready. See ADR [0028-networking-and-replication-scope.md](adr/0028-networking-and-replication-scope.md).
- Animation is asset-driven and component-targeted. See ADR [0029-animation-strategy.md](adr/0029-animation-strategy.md).
- Audio uses stable authoring components/commands with backend adapters. See ADR [0030-audio-strategy.md](adr/0030-audio-strategy.md).
- Text/font is a dedicated engine domain. See ADR [0031-text-and-font-strategy.md](adr/0031-text-and-font-strategy.md).
- Runtime services share a common backend boundary. See ADR
  [0042-runtime-service-and-backend-boundary.md](adr/0042-runtime-service-and-backend-boundary.md).
- Save, networking, animation, scripting, and editor tooling share component field capability
  metadata as the first eligibility gate. See ADR
  [0045-component-schema-capability-metadata.md](adr/0045-component-schema-capability-metadata.md).

Follow-up details still to settle:

1. What exact persistent entity ID model should save games use?
2. What replication metadata should components eventually expose?
3. What is the first sprite animation asset shape?
4. Which audio backend should be spiked first?
5. Which text shaping/font stack should nara use?

## Facade and Prelude

Accepted direction: the root `nara` facade stays small, optional backends remain feature-gated, and
the default prelude is gameplay-first and backend-free. Backend, tooling, debug, render extraction,
queue/batch, and GPU cache internals should live in advanced or module-specific preludes. See ADR
[0044-root-facade-and-prelude-layering-policy.md](adr/0044-root-facade-and-prelude-layering-policy.md).
Default plugin groups are explicit facade products, not silent feature side effects. See ADR
[0046-plugin-metadata-and-default-plugin-groups.md](adr/0046-plugin-metadata-and-default-plugin-groups.md).

Follow-up details still to settle:

1. Which current `nara::prelude` exports should move to `advanced_prelude` or module preludes?
2. Which scheduling/task/diagnostic types are common enough for gameplay prelude?
3. Should `nara::minimal_prelude` exist, or is `nara::prelude` already the minimal gameplay surface?
