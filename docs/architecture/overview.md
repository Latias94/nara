# Nara v2 architecture

Nara builds a Rust-first, 2D desktop engine with a standalone game runtime and a separate editor.
This overview describes the target architecture. Follow the [master plan](../plans/v2-refactor-master-plan.md)
for implementation order and phase gates. The `legacy/v0-imgui-reference` tag preserves v0 history.

## Products

- **Game** runs on Winit with `nara_render` drawing directly to the swapchain. The scene clears the
  target. The renderer loads that target for the HUD pass using layout and paint data from
  `nara_ui`. Shipping games link neither editor tooling nor Open-GPUI.
- **Editor** uses Open-GPUI Mode A for desktop windows, events, IME, AccessKit, and floating
  `dockspace` panes. A single WGPU device and queue serve the UI and Nara viewport passes. The
  editor imports pooled viewport textures without copying them. The standalone game launched by Play
  runs in a supervised process with its own runtime.

## Workspace ownership

| Layer | Owners |
| --- | --- |
| Headless core | `nara_core` owns math, linear color, durable identity values, VFS, and diagnostic data. `nara_ecs` exposes Bevy ECS and world entity identity. `nara_app` owns plugins, schedules, fixed steps, task pools, and runtime diagnostics. `nara_reflect` and its derive crate own Schema and field access. `nara_asset` owns async asset loading and hot reload. `nara_scene` owns scene and prefab data and 2D transforms. |
| Game runtime | `nara_input` owns physical input and action mapping. `nara_render` and its material derive crate own WGPU passes, textures, materials, sprites, animation, tilemaps, and HUD presentation. `nara_ui` owns game layout, text, widgets, and HUD paint data. `nara_audio` owns the game audio API. `nara_physics2d` owns Rapier integration. `nara_window` owns the standalone Winit runner. |
| Game facade | Root `nara` exposes `DefaultPlugins` and `prelude` with a target of fewer than 1,000 lines. |
| Workflow and editor | `nara_project` owns headless project tooling, the `nara` CLI, Schema discovery, Cook, Export, and supervised Play. `nara_editor_core` owns synchronous editing, selection, gestures, Undo, drafts, and registry. `nara_editor` owns the Open-GPUI shell and integrates `dockspace`, Dialogue Graph, and Design Kit v0.4. |

Dependencies point inward toward the headless core. The [workspace ADR](adr/ADR-001-workspace-and-dependency-law.md)
governs package counts and dependency checks. The [Open-GPUI roadmap](../plans/v2-refactor-master-plan.md#open-gpui-target)
lists its seven target packages.

## Accepted decisions

| ADR | Contract |
| --- | --- |
| [ADR-001](adr/ADR-001-workspace-and-dependency-law.md) | Workspace, dependencies, and budgets |
| [ADR-002](adr/ADR-002-ecs-app-spatial-and-audio.md) | Bevy ECS and tasks, fixed updates, transforms, input, and audio |
| [ADR-003](adr/ADR-003-shared-gpu-rendering-and-color.md) | Shared device, rendering passes, texture ownership, and color |
| [ADR-004](adr/ADR-004-schema-scene-prefab-and-cook.md) | Schema, RON scenes and prefabs, and cooked formats |
| [ADR-005](adr/ADR-005-asset-identity-and-hot-reload.md) | UUID sidecars, asset sources, and hot reload |
| [ADR-006](adr/ADR-006-project-workflow-and-supervised-play.md) | CLI, external project Schema discovery, Play, and Export |
| [ADR-007](adr/ADR-007-editor-architecture-and-ui-seam.md) | Editor ownership and UI integration boundaries |
| [ADR-008](adr/ADR-008-game-ui-and-future-convergence.md) | Game UI and criteria for adopting it in the editor |

## Extension seams

1. A dimension-independent core allows `Transform3d` and `nara_physics3d` alongside the 2D modules.
2. `RenderPass` attachment declarations allow a dependency-ordered render graph when 3D passes need it.
3. Canonical `FieldPath` access allows keyframe animation and tweens to address Schema properties.
4. UUID asset identity and target-aware cooking allow additional storage and compression profiles.
5. Headless fixed steps allow dedicated servers, replay, and multiplayer simulation.
6. Schema and `EditorRegistry` provide typed seams for future scripting and editor extensions.
7. The editor core, pure Rust interaction models, world-space overlays, and game UI let the editor
   shell evolve independently of the domain logic.

The [Design Kit v0.4](../design/editor-reference/README.md) is the visual and interaction reference.
Its links to historical Nara documents resolve in the v0 tag.
