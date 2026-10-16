# October 2026 Editor Foundation Migration

## Dependency cohort

The editor foundation moves the Bevy ECS/reflection/macro family from 0.19 to 0.20, WGPU from 30.0.0 to 30.0.1, and the retained egui frontend from 0.36.1 to 0.36.2.
Winit remains on the supported stable 0.30.13 backend route.
This does not adopt Bevy App, rendering or asset ownership.
The locked WGPU HAL implementation also moves to 30.0.1: its Vulkan acquire-fence and Metal color-space repairs are not supplied by updating the public WGPU package alone.
All six independently locked consumers containing WGPU carry that HAL patch.

Nara's public `Vec2`, `Vec3` and `Mat3` now use glam 0.33 instead of 0.30.
They share a single resolved math-type family with Bevy reflection.
Consumers that construct these values through a direct glam dependency must migrate to 0.33 or use the Nara re-exports.
Persistent Vec2 values still use the existing explicit x/y scalar codec, so the math crate change does not change component wire schemas.

Lifecycle observer examples now use `On<Add<T>>`, `On<Remove<T>>` and the corresponding typed event patterns.
An unfiltered lifecycle event uses `On<Add<()>>` or its equivalent event kind.
Custom event observers retain their own event types.
Handwritten SystemParam implementations receive `SystemAccess` during access registration.
See the [official Bevy migration guide](https://bevy.org/learn/migration-guides/0-19-to-0-20/) for external integrations.

Refresh each independently locked Cargo consumer after migrating its dependency requirements.
Remove obsolete generated build artifacts through ordinary Cargo rebuilds; no source document or asset cache rewrite is required by this dependency change.
The declared minimum compiler remains Rust 1.95 and must be verified against the resolved dependency graph.

## Editor frontend boundaries

[ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md) and the [implementation plan](../plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md) own subsequent application/frontend, creation-template and window/view changes.
Dependency migration alone does not establish Dear ImGui adoption or multi-window product support.
