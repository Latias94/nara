# AGENTS.md

This file provides repo-local guidance for agents working on nara.

## Project Direction

- nara is a Rust-native, code-first, data-driven game engine.
- The runtime is ECS-first. Components are data; systems own behavior.
- `nara_ecs` uses `bevy_ecs` as the ECS substrate. Do not reintroduce a custom ECS unless an ADR explicitly replaces ADR 0002.
- `nara_app` owns nara's product-facing `App`, fallible `Plugin`, stage, runner, and lifecycle boundary. Do not adopt `bevy_app`.
- Plugin setup and prerequisite failures must return `PluginError`; do not reintroduce panic-based plugin prerequisite helpers.
- `nara_tasks` owns engine task pools, task handles, cancellation tokens, deterministic inline execution, and the threaded std worker backend. Do not expose Tokio or async-std as nara's gameplay-facing async contract.
- `nara_app::CoreStage::TaskUpdate` is the explicit main-thread integration point for background results. Keep `TaskUpdateSet::{Poll, CoalesceAssetChanges, SpawnAssetJobs, ApplyAssetResults}` ordering stable unless an ADR replaces the task/update contract.
- Keep backend crates behind adapters. Core gameplay-facing crates must not directly depend on `wgpu`, `winit`, egui, or dear-imgui.
- `nara_winit` owns all `winit` imports and desktop event-loop integration.
- `nara_render` owns backend-neutral render concepts, frame lifecycle, phases, `RenderBackendStatus`, `RenderBackendState`, and skipped-frame reasons. Do not reintroduce a public `RenderBackend` trait until a second backend or test adapter creates real abstraction pressure.
- `nara_render_wgpu` owns all `wgpu` imports and GPU surface/device lifecycle.
- `nara_render_wgpu` reports backend initialization, skipped frames, and backend errors through `RenderBackendStatus`.
- `nara_sprite_render` owns backend-neutral 2D extraction, queueing, sorting, and batching. GPU backends should consume `SpriteBatches`, not gameplay `Sprite` or `Tilemap` components.
- `nara_scene` owns persistent `SceneDocument` / `PrefabDocument`, stable `SceneEntityId`, validation, and world spawn/export. Scene/prefab documents must not store runtime `Entity`, runtime `AssetId`, or backend handles.
- Scene/prefab authoring edits should use `ScenePatchDocument` transactions. Patch operations serialize as `op + args`, validate against schema-aware `ComponentFieldPath`, and return inverse patches for undo.
- `SceneAuthoringSession` is the first authoring/live sync boundary. It treats `SceneDocument` as truth, stores undo/redo as inverse patches, and rebuilds its managed live `World` projection instead of mutating arbitrary ECS storage directly.
- `nara_tooling` owns UI-agnostic editor/debug models such as `WorldSnapshot` and `SceneInspectorState`. UI adapters should render tooling models and send tooling commands instead of inventing editor-only mutation paths.
- `nara_tooling_egui` owns all `egui` imports and early egui debug/editor panels. Core runtime crates and `nara_tooling` must remain UI-toolkit agnostic.
- Play Mode must use an isolated runtime `World` fork spawned from a validated edit document snapshot. Stop Play discards runtime changes by default; persistent write-back must be an explicit Apply Changes flow that produces `ScenePatchDocument` and goes through normal validation/undo. The first supported Apply Changes subset is selected `SceneEntityId` plus explicitly requested registered serializable component IDs; whole-scene diffing, prefab-expanded write-back, and edit-while-playing merge are still unsupported.
- Prefab overrides are `ScenePatchDocument` values applied relative to source prefab IDs before expansion. Do not reintroduce whole-component prefab override maps.
- Nested prefab source resolution goes through `PrefabSourceResolver`; expanded prefab IDs use the `anchor/source_entity` namespace rule.
- `nara_scene` must keep scene/prefab spawn two-phase: preflight first, then mutate the target `World`. Asset-aware spawn uses a scratch `AssetServer` and only writes it back after the full preflight succeeds.
- `nara_reflect` owns `ComponentValue`, schema metadata, `ComponentFieldPath`, component preflight/apply codecs, migrations, `ComponentDecodeContext`, and `ComponentEncodeContext`. Keep its value, schema, path, codec, migration, and registry modules focused. Domain crates register their own built-in component codecs through their plugins.
- `nara_asset` persistent references use semantic `AssetRef::Path` or `AssetRef::StableId`; `Handle<T>` and `AssetId` are runtime-only and must not serialize as project data.
- `nara_asset` owns source asset identity, `.meta` records, importer registry metadata, typed import job contracts, imported artifact records, dependency graph data, load states, reload generations, source change coalescing, and reload request scheduling. It must not own GPU resources, file watchers, or depend on render backend crates.
- `SourceChangeResolver` must keep reload scheduling generation-stamped, expected-version guarded, and dependency-aware. Same-frame source changes coalesce by logical path with the last semantic event winning; do not make `Removed` unconditionally dominate atomic-save modify sequences.
- `nara_asset_watch` owns all `notify` imports. Filesystem watcher events must be translated into semantic `AssetSourceChange` values before asset reload logic sees them. Keep this crate optional behind the root `asset-watch` feature.
- `nara_image::ImagePlugin` owns typed image importer registration, async image reload jobs, runtime `Assets<ImageAsset>`, and image render-resource preparation. `ImagePreparePlugin` is prepare-only and must not become a second asset loading path.
- `nara_image::ImageAsset` and `PreparedImageResource` describe image content/import identity only: source metadata, extent, format, color space, hashes, and pixels. Do not reintroduce image-owned sampler or material policy.
- `nara_material` owns backend-neutral 2D material intent: `FilterMode`, `AddressMode`, `SamplerDescriptor`, `AlphaMode2d`, `Material2dDescriptor`, semantic image references, and material keys. Sprites, tilemaps, UI images, and future 2D material users should route sampler/alpha/tint through this domain.
- Texture upload, atlases, materials, UI images, and future 3D assets must flow through the asset import + render resource preparation seam in ADR 0033 instead of direct path-to-wgpu shortcuts.
- `nara_render_wgpu` owns backend GPU resource caches. Gameplay/domain crates store typed handles or backend-neutral descriptors, never `wgpu` handles.
- Sprite/tilemap render batches are material-aware. `nara_sprite_render` resolves runtime image handles into `SpriteMaterialKey` values containing image resource key, sampler, alpha mode, and tint; backend caches consume those keys instead of texture-only batch keys.
- `nara_render_wgpu` must keep image texture upload cached separately from sampler/material bind-group identity so sampler changes do not require image reimport or texture reupload.
- Keep render modules split by responsibility: `nara_sprite_render::{types,extract,queue}` and `nara_render_wgpu::{surface,sprite}` should stay narrow instead of growing monolithic backend or render-bridge files.
- `DiagnosticReport::push` only collects diagnostics. Use `Diagnostic::emit_to_tracing` or `DiagnosticReport::emit_to_tracing` explicitly when logs are desired.
- The root `nara` facade must keep `winit` and `wgpu` optional; default features stay backend-free.
- `repo-ref/` contains reference source trees. Treat it as read-only reference material and keep it out of git.

## Architecture Rules

- Record durable architecture decisions under `docs/architecture/adr/`.
- Keep `docs/architecture/nara-foundation.md` aligned with implemented crate boundaries.
- Use `docs/knowledge/engineering/` for session memory, subagent findings, verification, and handoff state.
- Prefer fearless refactoring before compatibility layers. This project is pre-1.0; remove obsolete scaffolding instead of preserving it.
- Keep scene/prefab/save data independent from runtime `bevy_ecs::Entity` values, runtime `AssetId`, and backend-native handles.
- Runtime UI is expected to be nara-owned long term. egui/dear-imgui are acceptable for debug/editor tooling, not as the runtime UI foundation.

## Rust Workflow

- Use Rust 2024 and the workspace MSRV in `Cargo.toml`.
- Format with `cargo fmt --all`.
- Prefer `cargo nextest run --workspace` for tests.
- Run `cargo check --workspace` before considering implementation work complete.
- Check optional backend examples with `cargo check -p nara --features winit,wgpu --example windowed_clear` and `cargo check -p nara --features winit,wgpu --example windowed_sprites` when touching platform or render backend code.
- Use dependency boundary searches when touching backend crates: `rg -n "winit::|winit =" crates src Cargo.toml` and `rg -n "wgpu::|wgpu =" crates src Cargo.toml`.
- Use precise commits with Conventional Commit messages.
- Do not discard or rewrite user changes. Never use `git reset --hard`, `git checkout --`, `git restore`, `git clean`, or stash to remove work unless the user explicitly asks.

## Subagent Guidance

- For architecture research, prefer read-only subagents with an explicit instruction not to spawn nested subagents.
- Subagents may inspect `repo-ref/`, docs, and source files, but the orchestrating agent owns edits, staging, commits, and final verification unless the user says otherwise.
