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

## Generic Scene and Prefab composition

`SceneCompositionCommand` component operations now take a stable `ComponentTypeId` instead of `SceneCompositionComponentKind`.
Ordinary Prefab commands support the same admitted component addition and removal.
Creation choices come from the exact retained catalog, including for an empty document; a missing template remains a visible unavailable capability.
Generic authoring creates empty entities and adds or removes current data-editable direct component records through the existing document transaction and Undo/Redo owners.
Entity deletion/reparenting and unavailable-record removal remain outside that finite generic vocabulary.

Scene and ordinary Prefab sources share the tree and Inspector projection, whose document kind remains explicit.
Native Prefab field edits and source navigation now use the ordinary Prefab persistence lane; they no longer require a UI root.
This shared data projection does not turn a Prefab into a Scene preview producer.
The Host retires an old Scene presentation when the active document changes to a non-Scene source.

The canonical unavailable-record manifest now includes source-local semantic placement and nested patch placement in addition to complete identities, values and references.
An insertion that changes an unavailable record's placement is rejected atomically; appending an unrelated entity and its inverse preserve the manifest.
Specialized UI structure admission remains enforced for existing and newly introduced UI records, including when `runtime-ui` is disabled.

The collection-game custom scalar owner now publishes generation-2 creation templates and retains its frozen generation-1 catalog.
Its component wire versions remain 2; refresh the bundle with the compiled project recipe before strict Check or Play.

## Trusted panel contributions and user layouts

The application owns `EditorPanelWorkspace`: a finite trusted descriptor registry, stable panel instances, explicit document targets and user layout preferences.
Each frontend owns its painting companions and per-instance widgets; callbacks receive read-only semantic models and emit existing `EditorAction` values.
The independently compiled `module-consumer/editor-panel-consumer` installs through the same registration and callback path as the built-ins.
Ordinary project opening does not load that contribution from project data or execute project Rust.

Use the Panels menu to open, hide or reset instances and explicitly save the workspace layout.
The native product stores preferences in the user's configuration directory, outside Scene and Prefab source; headless consumers and canaries opt in separately.
Source-targeted instances retain stable source kind/locator rather than persisting process-local document IDs.
Missing contributions and incompatible toolkit data restore a usable layout with a reason.
Closing an instance leaves document history intact and retires its local drafts; an accepted continuous gesture is cancelled through the existing Host action.
The originating instance is confirmed from the exact accepted batch outcome, so a rejected Begin cannot steal another instance's gesture.

These changes establish semantic layout ownership and same-window floating panels.
Actual independent render views, native detached windows and monitor restoration require the subsequent Host/backend units.
