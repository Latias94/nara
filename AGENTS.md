# AGENTS.md

This file provides repo-local guidance for agents working on nara.

## Project Direction

- nara is a Rust-native, code-first, data-driven game engine.
- The runtime is ECS-first. Components are data; systems own behavior.
- `nara_ecs` uses `bevy_ecs` as the ECS substrate. Do not reintroduce a custom ECS unless an ADR explicitly replaces ADR 0002.
- `nara_app` owns nara's product-facing `App`, `Plugin`, stage, runner, and lifecycle boundary. Do not adopt `bevy_app`.
- Keep backend crates behind adapters. Core gameplay-facing crates must not directly depend on `wgpu`, `winit`, egui, or dear-imgui.
- `nara_winit` owns all `winit` imports and desktop event-loop integration.
- `nara_render_wgpu` owns all `wgpu` imports and GPU surface/device lifecycle.
- `nara_sprite_render` owns backend-neutral 2D extraction, queueing, sorting, and batching. GPU backends should consume `SpriteBatches`, not gameplay `Sprite` or `Tilemap` components.
- `nara_scene` owns persistent `SceneDocument` / `PrefabDocument`, stable `SceneEntityId`, validation, and world spawn/export. Scene/prefab documents must not store runtime `Entity`, runtime `AssetId`, or backend handles.
- Scene/prefab authoring edits should use `ScenePatchDocument` transactions. Patch operations serialize as `op + args`, validate against schema-aware `ComponentFieldPath`, and return inverse patches for undo.
- Prefab overrides are `ScenePatchDocument` values applied relative to source prefab IDs before expansion. Do not reintroduce whole-component prefab override maps.
- Nested prefab source resolution goes through `PrefabSourceResolver`; expanded prefab IDs use the `anchor/source_entity` namespace rule.
- `nara_scene` must keep scene/prefab spawn two-phase: preflight first, then mutate the target `World`. Asset-aware spawn uses a scratch `AssetServer` and only writes it back after the full preflight succeeds.
- `nara_reflect` owns `ComponentValue`, component preflight/apply codecs, `ComponentDecodeContext`, and `ComponentEncodeContext`. Domain crates register their own built-in component codecs through their plugins.
- `nara_asset` persistent references use semantic `AssetRef::Path` or `AssetRef::StableId`; `Handle<T>` and `AssetId` are runtime-only and must not serialize as project data.
- `nara_asset` owns source asset identity, `.meta` records, importer registry metadata, imported artifact records, dependency graph data, load states, and reload events. It must not own GPU resources or depend on render backend crates.
- Texture upload, atlases, materials, UI images, and future 3D assets must flow through the asset import + render resource preparation seam in ADR 0033 instead of direct path-to-wgpu shortcuts.
- `nara_render_wgpu` owns backend GPU resource caches. Gameplay/domain crates store typed handles or backend-neutral descriptors, never `wgpu` handles.
- Keep render modules split by responsibility: `nara_sprite_render::{types,extract,queue}` and `nara_render_wgpu::{surface,sprite}` should stay narrow instead of growing monolithic backend or render-bridge files.
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
