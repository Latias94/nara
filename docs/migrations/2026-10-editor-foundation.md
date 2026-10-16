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

`EditorApplication` owns project opening, replacement, recovery, document actions and Play without toolkit state.
Move egui drawing calls from `EditorApplication::show` to a separately owned `EguiEditorFrontend::show(ui, &mut application)`.
Interactive egui consumers submit their returned batch through `EguiEditorFrontend::submit_and_drive`; it observes that exact dispatch result before advancing the application.
Native integration likewise delivers the returned dispatch feedback immediately.
The neutral `EditorApplication::submit_and_drive` remains available for consumers without widget feedback.
Recreating a frontend resets only local widgets and drafts; it does not replace the project session, accepted document changes, history or Play owner.

## Component creation metadata

`ComponentSchema` can publish an optional complete `authoring_initial_value` through `with_authoring_initial_value`.
Catalog admission checks it against every declared field, including optional fields that ordinary stored records may omit.
This value is copied for an explicit creation operation; it does not change field `default_value` or make required stored fields optional.
Schemas without a template retain their previous serialization and fingerprint and do not gain generic creation capability.

Scalar derives opt in with `#[nara(authoring_initial)]` and specify `#[nara(initial = ...)]` on every field.
Only scalar literals and negative numeric literals are admitted.
Rust checks the actual field type, including aliases; floating-point finite checks happen at compile time before the existing sealed codec constructs the data.
Arbitrary expressions and native `Default` callbacks are unsupported.

The built-in Transform2d, Sprite, Name and Visibility schemas now publish explicit complete templates.
Their transform, sprite and Scene component owners advance to catalog generation 2, retaining generation 1 as their predecessor; component wire versions remain 1.
Refresh a compiled project's declared schema bundle explicitly with its own recipe using `nara schema <project-directory> --write`, then reopen the Editor.
Strict Check/Play still rejects a bundle that differs from the selected native definitions.
Intentionally historical or degraded data-only fixtures need not be upgraded to claim capabilities they do not have.
