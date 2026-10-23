---
status: active
date: 2026-07-10
audience: engine contributors and early project authors
supported_baseline: unreleased source tree at the start of the engine-foundation refactor
---

# July 2026 Engine Foundation Migration Guide

This guide records deliberate breaking replacements made by the engine-foundation completion plan. nara is unreleased: the goal is one correct canonical contract, not compatibility with prototype APIs or draft file shapes.

## Policy

- Remove obsolete code, exports, fixtures, and documentation in the same implementation unit.
- Do not add deprecated aliases, compatibility wrappers, parallel `V1`/`V2` Rust types, or a second loading path.
- Give the corrected Rust API the canonical unsuffixed name.
- A superseded pre-release persistent shape is deleted and replaced by the correct canonical `format_version = 1` after every in-repository source/fixture is updated.
- Runtime readers never silently rewrite project source files. A source rewrite is explicit and performed before or outside runtime load.
- Generated caches may be rebuilt, quarantined, or deleted; they are not compatibility authorities.
- Preserve an old reader or migration chain only when an ADR names the compatibility window, owner, removal trigger, and fixtures.
- Back up external experimental projects before applying a source rewrite.

## Migration Summary

Every implementation unit that changes a public API, persistent shape, cache contract, or observable behavior adds a row. A unit with no external migration records `No external migration` in its verification evidence rather than inventing an entry.

| Migration ID | Unit | Commit | Kind | Affected contract | Required action |
|---|---|---|---|---|---|
| ABFE-1 | ADR boundary refinement | `05531a0` | `rust-api` | Frozen component codec execution | Obtain one `ComponentRegistrySnapshot` after freeze; execute preflight/encode on that snapshot. Handle execution errors separately from actual entity/component absence. |
| ABFE-2 | ADR boundary refinement | `05531a0` | `rust-api` | ECS authoring, upstream and internal imports | Keep ordinary types and the six adapted derives on the ECS root/prelude. Move advanced Bevy API imports to `ecs::upstream`; internal transaction helpers no longer belong to the ordinary root. |
| LLC-U4 | Low-level U4 | `89fdf6f` | `rust-api/behavior` | World-space spatial consumption | Replace Global Component queries and completion-token joins with Historical, Fixed or Presented snapshot readers. Resolve uses the fixed helper; Gameplay Capture reads Fixed directly in Finalize. A non-spatial direct parent ends inheritance. |
| LLC-U5 | Low-level U5 | `fb6af18` | `rust-api/behavior` | Managed asset mutation and watcher reconciliation | Capture `Assets<T>::mutation_baseline` before delayed effects; consume it for success, failure or removal. Retain desired settings independently of last-good values. Complete an owned watcher rescan only after Host reconciliation; draining a rescan error does not acknowledge coverage. |
| LLC-U10 | Low-level U10 | `cfde813` | `behavior` | Gameplay Capture and acknowledgement | Keep command consumers in Consume and success-dependent snapshots in Capture. Managed consumer/Simulate/deferred failure skips Capture and Ack; Capture failure skips Ack. Observe the fault and close/reconstruct explicitly; do not assume rollback, schedule abort, or automatic replay. |
| LLC-U3 | Low-level U3 | `b57008d` | `rust-api/behavior` | File lock and reader lifetimes | Move the file into `try_lock`, retain its `LockedFileCapability`, and recover the file or locked owner from rejected acquisition/release. Readers borrow the owner; keep source files alive through copying. Remove FileLock guards and forgotten-lock workarounds. |
| LLC-U6 | Low-level U6 | `6ca126f` | `rust-api` | Manual persistent component codecs | Return `PreparedComponentCandidate<T>` for the registered T. Encode `&T` to `ComponentValue`; context encoders additionally receive read-only World/context. Remove manual missing-component selection and runtime type-mismatch handling. |
| LLC-U8 | Low-level U8 | `6ca126f` | `rust-api/behavior` | Installed identity operations | Use `adopt_identity_entity`, `register_scene_instance`, `register_persistent_identity` and `retire_identity_entity[_with_disposition]` with the World. Do not extract/reinsert the installed domain to satisfy borrowing. First marker insertion and retirement reject conflicting lifecycle observers before effects. |
| LLC-U7 | Low-level U7 | `8d3515c` | `rust-api` | Bundle/SystemParam facade paths | Derive through Nara root, ECS or prelude paths without a Bevy alias. Keep the ECS runtime and extracted macro helpers at the same exact released version. |
| LLC-U2 | Low-level U2 | `feb273d` | `rust-api/behavior` | Task completion and rejection observation | Use the Tasks-owned return observer when slot reuse requires executor-owned disposal. Terminal publication alone does not prove return. Handle `CleanupFailed` and `TaskRejection::return_state`; retain separate domain cleanup ownership. |
| U2-1 | U2 | `2867235` | `rust-api` | `App` mutation/update, `Plugin`, cleanup, group, and runner signatures | Propagate setup/update errors, use narrow cleanup/group contexts, and let runners borrow the app. |
| U2-2 | U2 | `2867235` | `rust-api` | Built-in `register_*_components` helpers | Handle `ComponentRegistryError`; plugin installation now reports contextual registration failure. |
| RGD-U2-1 | RGD-U2 refresh | `b4d105c` | `rust-api/behavior` | Executable `ComponentRegistry` ownership and direct-App runtime fault reporting | Read the runtime registry through `component_registry`, register schema providers through `register_schema_provider_for_plugin`, and handle `AppRunError::DirectRuntime` or `AppScheduleRunError::Runtime`. |
| U3-1 | U3 | `a4167e4` | `rust-api/behavior` | Runtime/fixed/render time construction, mutation, and frame semantics | Use validated constructors/setters, handle time-frame errors, and replace bulk fixed-step observations with per-tick clock state. |
| U4-1 | U4 | `8ba9384` | `rust-api/behavior` | Gameplay command construction, ingress, fixed-tick observation, retirement, and action mapping | Build bounded drafts/submissions, handle typed rejection, consume the current batch in declared sets, and propagate fallible action bindings. |
| U4-2 | U4 | `8ba9384` | `persistent-shape` | Prototype serialized gameplay command submissions | Rewrite to the canonical tick/source/source-sequence/command shape; enforce ADR 0049 outer parse budgets before serde. |
| U5-1 | U5 | `e77da64` | `rust-api` | Task pool configuration, submission, terminal results, test execution, and shutdown | Configure bounded threaded pools, handle explicit spawn/terminal outcomes, and use the test-only inline driver where deterministic execution is required. |
| U5-2 | U5 | `e77da64` | `persistent-shape` | `nara.toml` runtime plugin-plan spelling and task pool schema | Rewrite flat task fields into per-pool/shutdown tables and use only the canonical `runtime-2d` value. |
| U8-1 | U8 | `9263d8c` | `rust-api/behavior/persistent-shape` | Runtime entity identity, scene instance handles, gameplay entity targets, reflected references, export remaps, and tooling snapshots | Use `nara_identity` references/locators, keep scene instance context explicit, remap before fork/replay/export, and replace raw `Entity` observations. |
| U18-1 | U18 | `6a70847` | `rust-api/behavior` | Diagnostic construction, reports, runtime observations, pressure snapshots, and diagnostic plugin composition | Migrate to validated/classified bounded observations and reduce any `diagnostics.runtime_capacity` above 4,096. |
| RGF-U1-1 | RGF-U1 | `RGF-U1` | `rust-api/persistent-shape` | Component/field identity, registry lifecycle, durable field patches, and canonical scene/prefab/patch/catalog files | Assign permanent field IDs, build/freeze the registry before use, rewrite experimental files to the canonical envelopes, and load file bytes through candidates. |
| RGF-U3-1 | RGF-U3 | `RGF-U3` | `cargo-feature/rust-api/persistent-shape` | Root product capabilities, plugin bundles/preludes, and `nara.toml` product selection | Select the new coarse features, import advanced/tooling/backend names explicitly, replace plugin-plan data with runtime preset plus requested capabilities, and open manifests through a host-issued file capability. |
| RGF-U11-1 | RGF-U11 | `RGF-U11` | `rust-api/behavior` | Native window handle providers and surface/window shutdown | Replace raw-handle providers with an owning source, handle fallible registration/retirement, and let the platform runner complete renderer-acknowledged teardown. |
| RGF-U10-1 | RGF-U10 | `RGF-U10` | `rust-api/behavior/safety/cache` | Image importer input, bounded file reads, PNG decode/publication, reload failure semantics, and image artifact identity | Supply owned bytes or an opened capability, use the audited bounded PNG importer, and publish only through the reservation-bearing candidate; rebuild image import caches. |
| RGF-U4-1 | RGF-U4 | `RGF-U4` | `rust-api/behavior` | Plugin declarations, groups, slots, product planning, schema-provider input, App sealing/shutdown, and custom schedules | Replace imperative metadata/group installation and lifecycle aliases with static declarations, typed repeatable definitions, pure plans, `App::seal`, `App::shutdown_plugins`, and the unified typed schedule API. |
| RGF-U5-1 | RGF-U5 | `RGF-U5` | `rust-api/behavior` | Code-first runtime ownership, exact stepping, typed faults, task close ownership, and Winit driving | Admit a sealed App through `RuntimeCandidate`, drive `RuntimeInstance`, use observable controls and retryable close, call `WinitRunner::run(&mut runtime)`, and rename standalone task shutdown to `shutdown_blocking`. |
| RGF-U29-1 | RGF-U29 | `RGF-U29` | `rust-api/behavior` | Persistent codec output, registry binding, explicit component composition, and target-World apply eligibility | Return `PreparedComponentCandidate`, freeze the registry before runtime preflight, remove implicit required components/hooks from persistent types, and handle fail-closed target-World rejection. |
| RGF-U13-1 | RGF-U13 | `198a680` | `rust-api/behavior` | Managed runtime driver access, physical button transitions, App-exit propagation, and desktop frame/shutdown semantics | Replace generic driver World/resource access with typed resource-local ports, propagate fallible button-edge admission, and handle the one-target desktop result/close contract. |
| RGF-U8-1 | RGF-U8 | `60292e7` | `rust-api/behavior` | Task-update set ownership, ordered task result polling, and filesystem watcher admission/observability | Import asset phases from `nara_asset`, capture a task-pool completion cutoff before consuming ordered results, and send watcher batches through the bounded non-blocking sender while handling sticky `RescanRequired`. |
| RGD-U8-2 | RGD-U8 refresh | `be9b264` | `rust-api/behavior` | `FsError` forward compatibility and strict Unix traversal rejection classification | Add a wildcard arm to external `FsError` matches. Treat exact platform rejection variants as diagnostic detail: all strict Unix adapters reject a symlink leaf, while Linux currently reports `SymbolicLinkTraversal`. |
| CDPF-U1-1 | CDPF-U1 | `e345687` | `rust-api/behavior` | Filesystem publication source binding, explicit file write authority, and same-parent directory staging | Rename `ReplaceSourceBinding` to `PublicationSourceBinding`; create writable files only through a read-write directory capability, borrow writes through `CapabilityWriter`, and retain directory staging in a move-only `DirectoryPublicationTransaction`. |
| CDPF-U2-1 | CDPF-U2 | `e3a5e7b` | `rust-api/behavior` | Exact directory publication and truthful begin/publish terminality | Handle `DirectoryPublicationBeginError` separately from `FsError`; consume staging through `publish` or `abort`; and preserve `Published`, `RejectedClean`, or `Orphaned` evidence without assuming every failed begin was pre-mutation. |
| RPR-U3-1 | RPR-U3 | `676d030` | `rust-api` | Ordinary product composition and file-backed run setup | Replace normal caller use of definition IDs, slot edits, and parallel provider lists with `ProductRecipe` and `SchemaContribution`; keep raw one-shot plugins on direct `App` composition. |
| PCC-U1-1 | PCC-U1 | `742c1e8` | `cargo-feature/rust-api/behavior` | WGPU base and concrete Sprite/UI submission ownership | Keep `WgpuRenderPlugin` base-only; direct Rust products explicitly add `WgpuSpriteSubmitterPlugin` or `WgpuUiSubmitterPlugin`, while file-backed products request the corresponding presentation capability intersection. |
| PCC-U2-1 | PCC-U2 | `18ec878` | `cargo-feature/rust-api/behavior` | Native Winit accessibility activation | Compile support independently from activation; direct Rust products explicitly add `WinitUiAccessibilityContributionPlugin`, while file-backed desktop UI products receive it from the `desktop-winit + presentation-ui` request intersection. |
| PCC-U3-1 | PCC-U3 | `864c151` | `rust-api/behavior` | Ordinary file-backed Host composition authority | Remove raw configure/disable/order/provider methods from ordinary Headless, Desktop, and Editor intents; put replayable plugins and schema contributions in one `ProductRecipe`, and use one `project_host::advanced::RuntimeProbe` only for non-replayable Host probes. |
| RGS-U2-1 | RGS-U2 | `51b3fe4` | `rust-api/behavior` | Runtime hierarchy ownership, scene component composition, and parented spatial content admission | Import runtime relationships from `nara_hierarchy`, install `HierarchyPlugin` and `SceneComponentsPlugin` separately, and keep parented transform/visibility content fail-closed until RGS-U3. |
| RGS-U3-1 | RGS-U3, superseded by LLC-U4 | `fb4fdc7` | `rust-api/behavior` | Completed global 2D projection and world-space camera/sprite/tilemap extraction | Keep explicit `Transform2d` participation and no local/identity fallback; LLC-U4 replaces the earlier Component/token representation with phase snapshots. |
| SRT-U2-1 | SRT-U2 | `0b5beba` | `rust-api/behavior/safety` | Image construction, render prepare snapshot identity, and prepare invalidation | Handle fallible `ImageAsset` construction, capture `AssetSlotRevision` in each prepare snapshot, borrow non-`Copy` snapshots, and remove the unconsumed invalidation event log. |
| SRT-U4-1 | SRT-U4 | `25e7f6c` | `rust-api/behavior` | Lifecycle-free insertion preparation and provisional product replacement retirement | Prepare lifecycle-free insertions before commit and authorize extra retirement with `WorldEntityToken` rather than bare `Entity`; complete hierarchy validation remains read-only. |
| NUI-U1-1 | NUI-U1 | `NUI-U1` | `architecture-authority` | Ordered placement, renderer-neutral UI modules, concrete import execution, Host-owned Editor state, closed rendering, and content/presentation composition | Treat ADRs 0101 through 0106 and the ordered UI kernel correction plan as the accepted target. No source/API migration occurs until its owning NUI unit lands and adds a concrete entry below. |
| NUI-U2-1 | NUI-U2 | `92ee7f9` | `rust-api/cargo-feature/project-manifest` | Runtime tooling plugin installation and manifest-requestable Editor capabilities | Compile Editor models with the `tooling`/`tooling-egui` Cargo features, let the process Host own `EditorWorkspace`, and remove tooling capabilities from `nara.toml`. |
| NUI-U3-1 | NUI-U3 | `3b20856` | `rust-api/behavior` | Generic importer registration, selection, and execution APIs | Call concrete domain importers directly and preserve shared artifact identity without an unconsumed executor registry. |
| NUI-U4-1 | NUI-U4 | `e1a4595` | `rust-api/behavior` | Public render topology and technical sprite/UI pipeline stages | Use the closed first-party render plan and product-level authoring/presentation entry points; remove custom phase and direct pipeline-stage calls. |
| NUI-U5A-1 | NUI-U5A | `2f0bd69` | `cargo-feature/rust-api/project-manifest/behavior` | Implicit 2D presentation through `runtime-2d` and `Runtime2dPlugins` | Keep headless content on `runtime-2d`; add `presentation-2d` plus `Presentation2dPlugins` to presented products and explicitly request both capabilities. |
| NUI-U6-U7-1 | NUI-U6/U7 | `7c2c5ba`, `53d6678` | `serialized-format/rust-api/behavior` | Record-owned `parent`, implicit root order, placement-free add/remove, and bare `Reparent` | Rewrite Scene/Prefab documents to ordered `roots` plus parent-owned `children`; express Patch mutations with `ScenePlacement`, `MoveEntity { from, to }`, and exact subtree `RestoreEntity`. |
| NUI-U8-1 | NUI-U8 | `4b2146a`, `fc3a0b8` | `rust-api/behavior` | Construction-only hierarchy attachment and runtime order inferred from entity allocation | Register every structural root and child with `HierarchyWriter` using exact ordered placements; consume the completed `HierarchyOrder` for structural layout, hit testing, and drawing. |
| UIF-U1-1 | UIF-U1 | `c2806a5` | `rust-api/behavior` | Dispatch batches that mixed routed events with rejection and invalidation releases that entered activation | Match the closed `UiDispatchOutcome`; reduce widgets only for `Consumed`, and treat lifecycle cleanup as a consumed reconciliation. |
| EIR-U1-1 | EIR-U1/U2 | `EIR-U1/U2` | `persistent-shape/rust-api/cache` | Flat image `asset_meta`, caller-authored image recipe hashes, and global image color-space configuration | Rewrite image sidecars to nested identity plus typed settings, pass `ImageImportSettings` to concrete requests, and rebuild image artifacts. |

## Entry Contract

Each migration entry contains:

1. **Removed contract**: the exact public symbol, serialized shape, cache identity, or behavior removed.
2. **Canonical replacement or deletion rationale**: the unsuffixed replacement and why no compatibility path remains.
3. **Before/after**: required for Rust API changes; use short compilable examples.
4. **Affected examples and fixtures**: every in-repository caller/source updated by the unit.
5. **User action**: source edit, configuration edit, no action, or explicit regeneration command.
6. **Source action**: `none`, `manual-rewrite`, or an explicitly named offline tool. Runtime auto-rewrite is forbidden.
7. **Cache action**: `keep`, `rebuild`, `quarantine`, or `delete`.
8. **Compatibility window**: normally `none (unreleased canonical replacement)`; otherwise link the authorizing ADR.
9. **Rollback**: how to recover source data or revert the code commit without relying on a compatibility shim.
10. **Verification anchors**: tests, fixtures, examples, and stale-symbol searches proving the replacement is complete.

## ABFE-1: Frozen Component Codec Execution

**Removed contract**: `ComponentRegistry::{preflight_component, preflight_component_with_context,
encode_component, encode_component_with_context}` and their outer `Option` result. That result
conflated registry readiness, unknown identity and unavailable execution.

**Canonical replacement**: register and freeze through `ComponentRegistry`, then retain the existing
`ComponentRegistrySnapshot` for the complete codec operation or batch. No second registry type or
compatibility wrapper is introduced.

Before:

```rust
registry.freeze()?;
let prepared = registry.preflight_component(&id, &value).unwrap()?;
let encoded = registry.encode_component(&id, &world, entity).unwrap()?;
```

After:

```rust
registry.freeze()?;
let execution = registry.snapshot()?;
let prepared = execution.preflight_component(&id, &value)?;
let encoded = execution.encode_component(&id, &world, entity)?;
```

Preflight returns `Result<PreparedComponent, ComponentCodecError>`; encoding returns
`Result<Option<ComponentValue>, ComponentCodecError>`. Only actual entity/component absence is
`Ok(None)`. Unknown schema, missing native binding and codec failures are explicit errors. A missing
native binding prevents successful freeze and snapshot publication. Context-aware variants share
the caller's existing decode/encode context. Snapshot retention does not replace current target-World,
lifecycle or asset-baseline admission during apply.

**Affected consumers**: Scene preflight/export, Tooling Apply Changes, Editor runtime editing,
component registration tests, independent manual codec/inventory fixtures and reference-game
authoring tests. The [component guide](../guides/persistent-components.md) shows the complete manual
codec registration and execution path.

**User action**: move codec calls to a retained snapshot and update result matches. Source action:
`none`; cache action: `keep`; compatibility window: `none (unreleased canonical replacement)`.
Rollback restores the code revision; no project-source or cache rewrite is required.

**Verification anchors**: snapshot execution and target-World rejection tests in `nara_reflect`,
building-registry compile-fail documentation, Scene/Tooling/root tests, independent manual codecs
and the [qualification record](../knowledge/engineering/verification/2026-10/2026-10-23T022246Z-adr-boundary-and-frozen-codec-execution-verification-b210d1cf09c94203b04bffcfa1dbe236.md).

## ABFE-2: Explicit ECS Imports

**Removed contract**: wholesale Bevy prelude/module re-exports from the ECS root, plus ordinary root
exports of lifecycle-free transaction primitives.

**Canonical replacement**: `nara_ecs` and `nara::ecs` maintain selected authoring imports and the six
adapted derives (`Component`, `Resource`, `Bundle`, `SystemParam`, `ScheduleLabel`, `SystemSet`).
Advanced APIs use the complete version-bound `upstream` path. Runtime types remain original Bevy
types; this does not create a replacement ECS.

Before:

```rust
use nara::ecs::{component::ComponentId, world::DeferredWorld};
```

After:

```rust
use nara::ecs::upstream::{component::ComponentId, world::DeferredWorld};
```

The ordinary `bundle`, `system` and `schedule` modules contain selected authoring names. Hidden
`__private` support is for generated code and internal cross-crate composition; it is not a public
extension API. Upstream macros outside the six adapted derives retain Bevy's own dependency/path
requirements. For example, `nara_app` keeps a direct test-only Bevy dependency for its `Event` derive.

**Affected consumers**: App/runtime, built-in domain modules, root Hosts, public game samples and
independent renamed ECS/root/schedule fixtures. **User action**: update advanced imports and stop
depending on internal transaction helpers as ordinary authoring APIs. Source action: `none`; cache
action: `keep`; compatibility window: `none (unreleased canonical replacement)`. Rollback restores
the code revision without rewriting project source data. The authoring-surface test, renamed
consumers and qualification record linked above cover these paths.

## NUI-U2-1: Host-Owned Editor Workspace

**Removed contract**:

- `nara_tooling::ToolingPlugin`, `TOOLING_PLUGIN_ID`, and the root `ToolingPlugins` group/slot.
- The `Resource` implementation for `EditorWorkspace` and automatic workspace insertion into a
  runtime `World`.
- `ProductCapability::Tooling`, `ProductCapability::ToolingEgui`, the corresponding compiled
  runtime capability claims, Editor-profile implication, and manifest spellings `tooling` plus
  `tooling-egui`.

**Canonical replacement or deletion rationale**: Editor workspace lifetime, persistence, Play,
Retry, and recovery belong to the process Host. Use `EditorProjectSession` for the complete hosted
workflow or construct `EditorWorkspace` as ordinary Host-owned domain state. The existing `tooling`
and `tooling-egui` Cargo features remain compile-time boundaries for models and adapters; they do
not install runtime plugins or become project-requestable capabilities.

Before:

```rust
app.add_plugin(nara_tooling::ToolingPlugin)?;
let workspace = app.world().resource::<nara_tooling::EditorWorkspace>();
```

After:

```rust
let mut workspace = nara_tooling::EditorWorkspace::new();
// The process Host owns this value and applies adapter-returned commands.
```

**Affected examples and fixtures**: root product-capability and plugin-composition fixtures,
`nara_project` manifest/profile tests, tooling unit tests, egui adapter tests, Editor persistence and
Play/Retry integration tests, and the reference-game Host-parity workflow.

**User action**: remove `tooling` and `tooling-egui` from `[capabilities].requested`; keep the Cargo
features when compiling an Editor Host. Remove direct `ToolingPlugin` installation and route
workspace commands through the owning Host session.

**Source action**: `manual-rewrite` for experimental `nara.toml` files that requested either removed
capability. Runtime loading intentionally rejects the old spellings.

**Cache action**: rebuild Rust artifacts; persistent Scene/Prefab data and asset caches are unchanged.

**Compatibility window**: none (pre-1.0 fearless replacement). No plugin alias, capability alias,
or duplicate ECS workspace is retained.

**Rollback**: revert the tooling crate, root product composition, project capability vocabulary,
Host tests, and manifest sources together. Do not restore only the plugin wrapper around a second
workspace authority.

**Verification anchors**: `crates/nara_tooling/src/lib.rs`,
`crates/nara_tooling/src/workspace.rs#EditorWorkspace`, `crates/nara_project/src/capability.rs`,
`src/project_host/runtime/editor.rs#EditorProjectSession`, `tests/plugin_composition.rs`,
`tests/editor_persistence.rs`, `tests/ui/editor_persistence/workspace_not_resource.rs`,
`tests/workspace_play_runtime.rs`,
`crates/nara_tooling_egui/src/lib.rs`, and `reference-game/tests/host_parity.rs`.

## U2-1: Terminal Plugin Lifecycle and Fallible App Mutation

**Removed contract**:

- `App::try_update()` plus panic-based `App::update() -> AppFrameOutcome`.
- Infallible mutable entry points such as `world_mut`, `insert_resource`, `init_resource`,
  `add_systems`, `configure_sets`, custom schedule inspection, and `set_runner`.
- `Plugin::cleanup(&mut App) -> ()` and unrestricted `PluginGroup::build(&mut App)`.
- `RunnerFn = FnOnce(App)`, which hid cleanup failures inside runner-owned `Drop`.
- The implicit `plugins_finished: bool` behavior that allowed committed failures to be retried.

**Canonical replacement or deletion rationale**: `App` exposes one explicit terminal plugin state
machine. Mutable entry points and `update` return `Result`; `Plugin::preflight` is the only
pre-mutation retry boundary; terminal teardown uses `PluginShutdownContext` and
`PluginShutdownError`; groups produce a data-only `PluginGroupBuilder`; runners borrow `&mut App`;
failure details use `PluginFailureReport`. `App::seal` closes configuration, while `App::run` seals
and then performs observable shutdown. The old paths are deleted because they could run a partially
initialized app or lose shutdown ownership.

**Before**:

```rust
app.insert_resource(GameState::default())
    .add_systems(CoreStage::Update, update_game);
app.update();

fn cleanup(&self, app: &mut App) {
    app.world_mut().remove_resource::<Backend>();
}
```

**After**:

```rust
app.insert_resource(GameState::default())?;
app.add_systems(CoreStage::Update, update_game)?;
app.update()?;

fn shutdown(
    &self,
    context: &mut PluginShutdownContext<'_>,
) -> Result<(), PluginError> {
    context.world_mut().remove_resource::<Backend>();
    Ok(())
}
```

**Affected examples and fixtures**: root facade plugin groups, all repository examples, plugin crate
tests, window/wgpu examples, asset import examples, and headless/server examples now use only the
fallible canonical API.

**User action**: update downstream plugin/app code to propagate or explicitly handle every returned
error; replace group access to `App` with a data-only `PluginGroupBuilder`; rename terminal hooks and
error handling to `shutdown`; change runners to borrow `&mut App`.

**Source action**: `none`.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the U2 commit and its callers together. Do not restore the boolean lifecycle or
add deprecated wrappers.

**Verification anchors**: `nara_app` lifecycle tests; focused plugin-crate nextest runs; all-target
facade check; searches for `try_update`, ignored update results, old cleanup/group signatures, and
unhandled mutable app calls.

## U2-2: Fallible Built-In Component Registration

**Removed contract**: built-in `register_*_components(&mut ComponentRegistry) -> ()` helpers that
called `expect` when a component ID or Rust type was already registered.

**Canonical replacement or deletion rationale**: registration helpers return
`Result<(), ComponentRegistryError>`. Built-in plugins use read-only
`ComponentRegistry::validate_component_registration` during preflight and map commit-time failures
to `PluginError::ComponentRegistrationFailed` with stable plugin and component context. A duplicate
is project/setup data, not a reason to panic the process.

**Before**:

```rust
register_transform_components(&mut registry);
```

**After**:

```rust
register_transform_components(&mut registry)?;
```

**Affected examples and fixtures**: scene/prefab/schema examples and transform, render, sprite,
hierarchy, tilemap, and runtime UI codec tests.

**User action**: handle or propagate the returned `ComponentRegistryError` when calling registration
helpers directly. Plugin users receive a contextual `PluginError` automatically.

**Source action**: `none`.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the U2 registration commit and callers together; do not restore `expect`.

**Verification anchors**: component registry read-only validation test, built-in duplicate conflict
tests, and a stale search for registration `expect` calls.

## RGD-U2-1: Private Executable Registry Authority

**Removed contract**:

- `ComponentRegistry: Resource` and direct runtime access through
  `World::{resource, resource_mut, insert_resource, remove_resource}`.
- Schema-owning plugins mutating the executable registry through a public ECS resource borrow.
- Direct `App` frames and custom schedules returning success after a sticky runtime or registry
  authority fault had already been reported.
- The hidden managed-only authority-validator installation entry point.

**Canonical replacement or deletion rationale**: `ComponentRegistry` remains a standalone
build/freeze/read value for direct module consumers and isolated authoring code, but an `App` stores
its executable instance behind a private owner resource. Schema-owning plugins call
`register_schema_provider_for_plugin`; runtime and Host readers call `component_registry` and receive
only an immutable view. One hidden validator now covers direct and managed schedule boundaries.
Direct execution reports sticky failures as `AppRunError::DirectRuntime`, while custom schedules
wrap that error in `AppScheduleRunError::Runtime`. The public resource contract is deleted because
temporary removal, replacement, or same-snapshot rewrapping could bypass the claimed frozen
behavior authority.

**Before**:

```rust
let registry = world.resource::<ComponentRegistry>();
let mut registry = app.world_mut()?.resource_mut::<ComponentRegistry>();
provider.register_or_validate_into(&mut registry)?;
app.run_once(delta)?;
```

**After**:

```rust
let registry = nara_reflect::component_registry(world)
    .expect("the component-registry plugin must own the runtime registry");

nara_reflect::register_schema_provider_for_plugin(
    app,
    PLUGIN_ID,
    "example-component",
    &PROVIDER,
)?;

let outcome = match app.run_once(delta) {
    Err(error @ AppRunError::DirectRuntime { .. }) => return Err(error),
    result => result?,
};
```

**Affected examples and fixtures**: all built-in schema-owning plugins, project Host and Editor
runtime readers, reference-game authoring/runtime composition, direct-App gameplay fault tests,
the renamed-root schedule-extension consumer, and the compile-time negative trait assertion.

**User action**: remove `ComponentRegistry` ECS resource reads and mutations. Use
`component_registry` for runtime reads, keep standalone registries outside an `App` where direct
module composition requires them, and use `register_schema_provider_for_plugin` only during plugin
construction. Handle the new direct runtime error variants instead of assuming a completed frame
was healthy.

**Source action**: `none`.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert `b4d105c` and all migrated callers together. Do not restore a public mutable
registry resource or downgrade sticky runtime faults to successful frame results.

**Verification anchors**:
`crates/nara_reflect/src/tests.rs#component_registry_is_not_an_ecs_resource`,
`crates/nara_reflect/src/plugin.rs#tests`, `crates/nara_app/src/lib.rs#tests`,
`tests/runtime_instance.rs`, `tests/plugin_composition.rs`,
`tests/fixtures/schedule-extension/renamed-root/src/lib.rs`, and the RGD-U2 refresh verification
record.

## RGD-U8-2: Forward-Compatible Filesystem Rejection Errors

**Removed contract**:

- Treating the public `FsError` enum as an exhaustively matchable list of every future filesystem
  rejection.
- Treating `FsError::SymbolicLinkTraversal` as the exact rejection returned by every Unix adapter.

**Canonical replacement or deletion rationale**: `FsError` is `#[non_exhaustive]`. Filesystem
capability errors can gain new typed variants as platform adapters prove additional rejection
causes, so downstream callers must retain a fallback arm. Strict Unix adapters share the semantic
contract that a symlink leaf is rejected. Linux currently classifies the `openat2` `ELOOP` result as
`SymbolicLinkTraversal`; Unix adapters without that Linux guarantee may fail closed with another
typed error such as `Unproven`.

**Before**:

```rust
let category = match error {
    FsError::Path(_) => "path",
    FsError::Io { .. } => "io",
    FsError::Unsupported { .. } | FsError::Unproven { .. } => "capability",
    FsError::ReadOnlyCapability { .. }
    | FsError::NotDirectory
    | FsError::NotRegularFile
    | FsError::ReparsePoint { .. }
    | FsError::SymbolicLinkTraversal
    | FsError::CrossVolume
    | FsError::MultipleLinks { .. }
    | FsError::IdentityUnavailable
    | FsError::CapabilitySessionExhausted
    | FsError::IdentityMismatch { .. }
    | FsError::TemporaryParentMismatch
    | FsError::AlreadyExists { .. }
    | FsError::TargetStateMismatch
    | FsError::LockContended
    | FsError::ByteLimitExceeded { .. }
    | FsError::DigestMismatch { .. } => "rejected",
};
```

**After**:

```rust
let category = match error {
    FsError::Path(_) => "path",
    FsError::SymbolicLinkTraversal => "symbolic-link",
    _ => "other-filesystem-rejection",
};
```

**Affected examples and fixtures**: downstream exhaustive matches must add a wildcard arm. The
filesystem contract fixture verifies rejection on all Unix targets and verifies the exact
`SymbolicLinkTraversal` classification only on Linux.

**User action**: add a wildcard arm to every external `FsError` match. Do not use an exact
platform-specific variant as the portable proof that an operation was rejected.

**Source action**: `none`; no persistent project format changes.

**Cache action**: `keep`; rebuild Rust artifacts after updating exhaustive matches.

**Compatibility window**: none (pre-1.0 public-contract correction).

**Rollback**: revert the typed strict-traversal classification and this migration together. Do not
make `FsError` exhaustive again or claim one platform's diagnostic classification as a portable
filesystem guarantee.

**Verification anchors**: `crates/nara_fs/tests/filesystem_contract.rs` and
`crates/nara_fs/src/platform/unix.rs#tests`.

## U3-1: Validated Time Configuration and Per-Tick Fixed Clock

**Removed contract**:

- Public mutable fields on `RuntimeTimeSettings` and infallible `with_time_scale` /
  `with_max_delta` builders that silently normalized invalid values.
- Infallible `FixedTime::new` and `FixedTime::with_max_steps_per_frame` constructors.
- `FixedTime::accumulated` / `FixedTime::overstep` and `RenderTime::overstep`, whose bulk
  frame-level model did not identify the current authoritative tick.
- Bulk deduction of all due fixed steps before `CoreStage::FixedUpdate`, including saturating
  overflow behavior and tracker cleanup that did not define a completed-frame boundary.

**Canonical replacement or deletion rationale**: time configuration is validated before use.
`RuntimeTimeSettings::new`, fallible builders, and setters return `TimeSettingsError`; `FixedTime`
uses fallible construction/setters and exposes catch-up policy, bounded debt, remainder, per-tick
delta/elapsed/tick, and discarded-time observations. `RenderTime::remainder` is the interpolation
remainder. `App::run_once` reports `TimeFrameError` through `AppRunError`, plans all clock changes
before committing them, advances `FixedTime` once immediately before each fixed schedule, and clears
ECS trackers only after the complete frame succeeds. The old model is deleted because it could not
represent zero/one/many-step frames or preserve atomic failure semantics.

**Before**:

```rust
let settings = RuntimeTimeSettings::default().with_time_scale(-1.0);
let fixed = FixedTime::new(Duration::ZERO).with_max_steps_per_frame(0);
let pending = fixed.accumulated();
let alpha_source = render_time.overstep;
```

**After**:

```rust
let settings = RuntimeTimeSettings::default().with_time_scale(0.5)?;
let fixed = FixedTime::new(Duration::from_millis(16))?
    .with_max_steps_per_frame(5)?
    .with_max_debt_steps(120)?
    .with_catch_up_policy(FixedCatchUpPolicy::DiscardExcess);
let authoritative_tick = fixed.tick();
let interpolation_remainder = render_time.remainder;
```

**Affected examples and fixtures**: root prelude exports, project manifest/profile lowering, server
composition, project tests, and headless server setup use the validated canonical values.

**User action**: replace direct time-setting field mutation with getters/setters; propagate
`TimeSettingsError`; replace `accumulated`/`overstep` observations with the specific `remainder`,
`debt`, `tick`, `delta`, or `elapsed` observation required by the caller; handle `AppRunError::Time`.

**Source action**: `none` for code-first users. `nara.toml` runtime values are validated during
profile resolution; invalid zero, non-finite, sub-nanosecond, or overflowing values must be edited.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert `a4167e4` together with its project/facade callers. Do not restore silent
normalization, saturating clocks, or a second bulk-step API.

**Verification anchors**: `nara_app` zero/one/many-step, pause/scale, discard/preserve debt,
overflow atomicity, Startup timing, fixed-set flush, removed-component, and tracker-boundary tests;
`cargo nextest run -p nara_app --locked` passes 52 tests.

## U4-1: Authoritative Fixed-Tick Gameplay Command Lifecycle

**Removed contract**:

- `GameplayCommandTime`, optional/zero authoritative ticks, frame provenance, and
  arrival-assigned/saturating sequence values.
- Public-field `GameplayCommandEnvelope` construction and direct queue `push`, `clear`, and
  `as_slice` access, plus queue `is_empty` whose frame-vector meaning is replaced by lifecycle-aware
  `is_idle`.
- Frame-scoped queue serialization and `GameplayCommandSet::{MapActions, Clear}` with
  `CoreStage::Last` cleanup.
- Public source construction that could impersonate `LocalAction`, including action names embedded
  in the source variant.
- Infallible, unbounded `ActionCommandMap::bind` and `bind_action`.
- Public flat `ActionCommandBinding` fields and its flat serialized
  `command_type`/`target`/`payload` shape.
- Unit-like `GameplayCommandPlugin` construction with no explicit queue settings.

**Canonical replacement or deletion rationale**: command intent is a bounded
`GameplayCommandDraft`. A public producer wraps it in `GameplayCommandSubmission` with a non-zero
authoritative tick, `GameplayCommandIngressSource`, and non-zero producer sequence, then handles the
typed result from `GameplayCommandQueue::submit`. Admission creates immutable
`GameplayCommandEnvelope` values ordered by `(tick, source rank/source ID, source sequence)`.
Fixed Prepare closes the tick into `GameplayCommandBatch`; current-gated Consume observes it.
LLC-U10 requires successful exact generation/tick/reporter completion after Simulate's deferred
boundary before Capture, then rechecks after Capture before engine-owned Ack retires it. Execution
failure retains visible effects and unacknowledged work without rollback or automatic replay.
Pending and active work share one retained budget. Any lifecycle
invariant failure is sticky and terminal, quarantines active work, gates consumers, and requires the
runtime to be rebuilt. The old frame vector was deleted because it lost commands during zero-step
frames and repeated them during catch-up frames.

**Before**:

```rust
let mut command = GameplayCommandEnvelope::new(
    GameplayCommandTypeId::new("move")?,
    GameplayCommandSource::Test,
    GameplayCommandTime {
        frame: 17,
        fixed_tick: None,
    },
);
command.target = Some(GameplayCommandTarget::named("player")?);
queue.push(command);
for command in queue.as_slice() {
    apply(command);
}
queue.clear();
```

**After**:

```rust
let submission = GameplayCommandSubmission::new(
    GameplayCommandTick::new(17).expect("tick is non-zero"),
    GameplayCommandIngressSource::test("driver")?,
    GameplayCommandSourceSequence::new(1).expect("sequence is non-zero"),
    GameplayCommandDraft::new(GameplayCommandTypeId::new("move")?),
);
let accepted_key = queue.submit(submission)?;

// Systems in GameplayCommandSet::Consume read GameplayCommandBatch::commands().
// Systems in Capture record the same immutable batch before engine-owned Ack retires it.
```

Replace `queue.is_empty()` with `queue.is_idle()` only when the caller truly needs to know that no
pending, active, quarantined, or poisoned lifecycle state remains. Most consumers should read only
the current `GameplayCommandBatch` in a declared command set.

`ActionCommandMap::bind` and `bind_action` now return `Result<(), ActionCommandMapError>` and reject
binding 4,097 rather than growing without bound.

Old public binding-field access such as `binding.command_type` becomes
`binding.command().command_type()`. Use `binding.action()`, `context()`, and `phase()` for routing,
and the existing builders to replace target/payload intent atomically.

Replace `app.add_plugin(GameplayCommandPlugin)?` with
`app.add_plugin(GameplayCommandPlugin::default())?`, or pass validated custom limits through
`GameplayCommandPlugin::new(settings)`.

**Affected examples and fixtures**: the root facade prelude and `ServerPlugins` tests,
`examples/headless_server.rs`, and all `nara_gameplay` tests now use the canonical queue/batch and
fallible binding APIs.

**User action**: replace direct envelope/queue mutation with a draft and submission; assign one
stable source sequence per producer stream; register simulation and capture systems in the declared
command sets; handle every submission/binding rejection. Do not read pending buckets or acknowledge
batches from game code.

**Source action**: `none` for Rust-only projects.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert `8ba9384` together with all facade/example callers. Do not restore the frame
vector, public `LocalAction`, consumer-owned clear/ack, or a compatibility wrapper.

**Verification anchors**: 27 `nara_gameplay` serde tests cover zero/one/many tick delivery,
canonical full-envelope ordering, duplicate/late/future rejection atomicity, all count/byte limits,
NaN/infinity, hostile serde values, local-source reservation, and terminal poison/quarantine. Root
tests cover exact `ServerPlugins` batches and absence of raw input; `headless_server` runs without a
desktop backend. Stale searches exclude `GameplayCommandTime`, frame provenance, queue
`push/clear/as_slice`, and `MapActions/Clear` sets.

## U4-2: Canonical Gameplay Command Submission Shape

**Removed contract**: prototype command JSON containing frame/time fields, optional ticks, legacy
source variants, public admitted envelopes, or live queue/batch state; and flat action-command
binding JSON with sibling `command_type`, `target`, and `payload` fields.

**Canonical replacement or deletion rationale**: only `GameplayCommandSubmission` is accepted for
ingress deserialization, and only admitted `GameplayCommandEnvelope` is serialized for capture.
Queue and batch runtime state are not serializable. This is a pre-release canonical reset, so the
correct shape remains version 1 when a replay owner later defines its outer envelope; no `V2` type
or legacy reader is retained.

```json
{
  "tick": 17,
  "source": { "Test": { "driver": "driver" } },
  "source_sequence": 1,
  "command": { "command_type": "move", "target": null, "payload": {} }
}
```

Action bindings now nest command intent so the Rust and persisted contracts share one bounded draft:

```json
{
  "action": "jump",
  "phase": "Started",
  "command": {
    "command_type": "player.jump",
    "target": null,
    "payload": {}
  }
}
```

Move former binding-level `command_type`, `target`, and `payload` fields under `command`; retain
`action`, optional/default gameplay `context`, and `phase` at the binding level.

The exact submission and admitted-envelope serde representations are covered by golden-value tests
in `nara_gameplay`; the action-binding test covers the nested command shape and default gameplay
context. A concrete file, replay, network, or package adapter must first enforce ADR 0049 encoded
byte, nesting-depth, and container-count budgets; semantic serde limits run after that outer
allocation boundary.

**Affected examples and fixtures**: no repository persistent command fixture used the removed
shape. Serde tests now reject obsolete/unknown fields, reserved local ingress, zero identities,
oversized strings/keys/maps, duplicate keys, and non-finite values.

**User action**: manually rewrite experimental command captures to the canonical submission shape,
or regenerate them from current code. Do not deserialize untrusted bytes without an outer bounded
reader/parser.

**Source action**: `manual-rewrite`; runtime auto-rewrite is unsupported.

**Cache action**: `delete` or regenerate prototype replay captures.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: restore an external backup and revert `8ba9384`; do not add a dual reader.

**Verification anchors**: `nara_gameplay` serde rejection, canonical submission/envelope
serialization, and action-binding shape tests, plus ADR 0057's explicit outer parse-budget
requirement.

## U5-1: Bounded Threaded Task Admission and Typed Terminals

**Removed contract**:

- `TaskExecutionMode`, including production/project `Deterministic` and `Threaded` mode switches.
- `TaskResult<T>` and `TaskResultState`.
- `TaskPoolConfig::deterministic`, `TaskPools::deterministic`,
  `TaskPlugin::deterministic`, `TaskPoolConfig::execution_mode`, and
  `TaskPoolConfig::threads_for`.
- Infallible thread-count-only configuration and `TaskPools::spawn(kind, closure) -> TaskHandle<T>`.
- Implicit unbounded submission, caller-thread fallback after closure, and unbounded worker join.

**Canonical replacement or deletion rationale**: production uses one bounded threaded state machine.
`TaskKindConfig` defines non-zero workers and pending capacity per kind; `TaskShutdownPolicy` defines
finite drain/cancel/join limits; `TaskPoolConfig` validates per-kind and aggregate limits.
`TaskPools::spawn` accepts a `TaskSpawnRequest` carrying admission tick, domain key, and overload
policy, then returns `TaskSpawnOutcome::{Accepted, Coalesced, Rejected}`. Handles yield one
`TaskTerminal::{Completed, Cancelled, Failed}` and first terminal wins every cancellation/result
race. `TaskPools::inline_for_tests` plus `run_pending_for_tests` drives the same bounded state machine
only in tests. The removed execution-mode API conflated deterministic application order with
caller-thread execution and was unsafe for real servers.

**Before**:

```rust
let pools = TaskPools::deterministic();
let mut handle = pools.spawn(TaskPoolKind::Compute, |_| 42_u32);
let result: TaskResult<u32> = handle.try_take().unwrap();
assert_eq!(result.into_value(), 42);
```

**After**:

```rust
let workers = ItemLimit::new(1).unwrap();
let config = TaskPoolConfig::threaded(workers, workers, workers)?;
let pools = TaskPools::try_new(config)?;
let request = TaskSpawnRequest::new(17, TaskDomainKey::new(7));
let mut handle = pools
    .spawn(TaskPoolKind::Compute, request, |_| 42_u32)
    .into_handle()
    .expect("the task should be admitted");
if let Some(TaskTerminal::Completed(value)) = handle.try_take() {
    assert_eq!(value, 42);
}
```

**Affected examples and fixtures**: image reload integration, project settings lowering, root
facade/server composition, headless server example, task/image/project tests, and crate preludes now
use only bounded typed outcomes.

**User action**: replace execution-mode branching with a validated `TaskPoolConfig`; supply a stable
`TaskDomainKey` and admission tick on every submission; handle rejection/coalescing and every
terminal variant; use `OrderedTaskResults` when application must be completion-order independent;
call `inline_for_tests` explicitly in deterministic tests.

**Source action**: `none` for Rust code-first configuration.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert `e77da64` together with all domain integration and server/project callers. Do
not restore unbounded channels, caller-thread fallback, or a production inline mode.

**Verification anchors**: `nara_tasks` queue-full/coalesce/panic/drop-panic/cancellation-race/ID-
exhaustion/partial-worker-start/shutdown tests; `nara_image` per-asset ordered-prefix and last-good
tests; root blocked-worker server tick test.

## U5-2: Canonical Version-1 Runtime and Task Project Settings

**Removed contract**:

- `ProjectTaskExecutionMode` and the flat `[tasks]` fields `mode`, `io_threads`,
  `compute_threads`, and `async_compute_threads`.
- The non-canonical `plugin_plan = "runtime2d"` serde alias.
- Effective task settings that could be parsed but did not configure the installed `TaskPlugin`.

**Canonical replacement or deletion rationale**: the unreleased project shape remains schema
version 1 and now expresses actual bounded runtime policy. `[tasks.io]`, `[tasks.compute]`, and
`[tasks.async_compute]` each contain `workers` and `pending_capacity`; `[tasks.shutdown]` contains
finite `drain_timeout_ms`, `cancel_timeout_ms`, and `join_timeout_ms`. Runtime settings add explicit
fixed catch-up/debt policy. `apply_project_settings` installs validated diagnostics, time, and task
configuration before the selected product bundle. The old shape is rejected rather than retained as
a second version-1 reader.

**Before**:

```toml
[runtime]
plugin_plan = "runtime2d"

[tasks]
mode = "deterministic"
io_threads = 0
compute_threads = 0
async_compute_threads = 0
```

**After**:

```toml
[runtime]
plugin_plan = "runtime-2d"
catch_up_policy = "discard-excess"
max_fixed_debt_steps = 120

[tasks.io]
workers = 2
pending_capacity = 64

[tasks.compute]
workers = 4
pending_capacity = 128

[tasks.async_compute]
workers = 2
pending_capacity = 64

[tasks.shutdown]
drain_timeout_ms = 250
cancel_timeout_ms = 250
join_timeout_ms = 250
```

**Affected examples and fixtures**: `examples/headless_server.rs`, root project application tests,
project profile tests, and `crates/nara_project/tests/fixtures/complete_v1.toml` use the canonical
shape.

**User action**: manually rewrite experimental `nara.toml` files to the nested tables above; choose
non-zero values within the documented per-pool/aggregate limits; replace `runtime2d` with
`runtime-2d`. Server plans always use real threaded pools and `PreserveDebt`.

**Source action**: `manual-rewrite`. Runtime auto-rewrite is intentionally unsupported.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: restore the project file backup and revert `e77da64` with the old parser/callers. Do
not add aliases or parallel schema-version types.

**Verification anchors**: the complete version-1 fixture, obsolete flat-schema/alias rejection,
duration/fixed-cap/task-limit/profile merge tests, root configuration equality test, and stale-symbol
searches.

## U8-1: World-Scoped Runtime Identity and Stable Entity References

**Removed contract**:

- Gameplay-owned `SceneStableId` and `PersistentRuntimeId` definitions and their duplicate
  command-target identity ownership. The canonical `PersistentRuntimeId` remains in
  `nara_identity`.
- `nara_scene::SceneInstanceId`, `SceneEntityMap`, `SceneSpawnReport::entity_map`,
  `SceneAuthoringSession::live_entity_map`, and export IDs derived from instance-name string
  concatenation.
- `WorldSnapshot { entities: Vec<Entity> }`, raw live/play `Entity` observations, and tooling APIs
  that treated allocator-local entity bits as stable identity.
- Entity-reference clone/export paths that could publish an incomplete remap or preserve an
  unresolved runtime target.

**Canonical replacement or deletion rationale**: `nara_identity` is the single deep owner of
world-domain, scene-instance, persistent, locator, remap, lookup, and tombstone semantics.
`SceneSpawnReport` returns a `SpawnedSceneInstance`; callers resolve its stable references through
the owning `WorldIdentityDomain`. Gameplay entity targets use `RuntimeEntityReference`. Reflected
component values use durable `EntityReference::{SceneLocal, Persistent}` and bounded, failure-atomic
rewrite helpers. Tooling captures `WorldIdentitySnapshot`, which reports stable locators plus
count-only runtime entities without exposing Bevy `Entity`. The removed APIs could alias across
worlds, collide across spawners, or serialize process-local handles.

**Before**:

```rust
let report = spawn_scene(&mut world, &registry, &document);
let entity = report.entity_map.get(&entity_id).unwrap();
let target = GameplayCommandTarget::Scene(SceneStableId::new("player")?);
let observed_entities = WorldSnapshot::capture(&mut world).entities;
```

**After**:

```rust
let report = spawn_scene(&mut world, &registry, &document);
let instance = report.instance.expect("scene spawn succeeded");
let target = GameplayCommandTarget::Entity(
    instance.runtime_reference(&entity_id).expect("entity is a member"),
);
let Some(EntityLookup::Resolved(entity)) = target.resolve_entity(&world) else {
    panic!("entity target must resolve");
};
let snapshot = WorldIdentitySnapshot::capture(
    &world,
    ItemLimit::new(4_096).expect("limit is non-zero"),
)?;
```

Gameplay scene targets serialize with explicit instance context and no world/domain/process handle:

```json
{
  "Entity": {
    "kind": "scene",
    "instance": 7,
    "entity": "player"
  }
}
```

Reflected durable references use the canonical `entity_ref` leaf shape. Scene-local values do not
carry a runtime scene instance; the owning scene instance supplies that context when resolving:

```json
{
  "type": "entity_ref",
  "value": {
    "kind": "scene_local",
    "entity": "root/player"
  }
}
```

**Affected examples and fixtures**: scene/prefab roundtrip and override examples, authoring,
inspector, Play Mode, sprite/tilemap/UI codec tests, root facade exports, and all scene spawn/export
callers now use the shared identity domain.

**User action**: replace map/raw-entity access with `SpawnedSceneInstance`,
`RuntimeEntityReference`, `WorldEntityLocator`, and typed lookup results. Include scene-instance
context when targeting one of several instances of the same scene. Before replaying into a parallel
fork or exporting a renamed group, build the complete remap and rewrite every declared target; do
not publish a partial candidate. Replace tooling raw-entity snapshots with bounded
`WorldIdentitySnapshot` capture.

**Source action**: `manual-rewrite` for experimental serialized command targets or reflected
component values. Loading obsolete persistent shapes for automatic rewrite is unsupported.

**Cache action**: `delete` or regenerate prototype replay/debug captures containing old gameplay
target tags or raw `Entity` values.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: restore external source/capture backups and revert `9263d8c` together with the U8
identity-core commits. Do not reintroduce duplicate ID owners, a `SceneEntityMap` compatibility
wrapper, or raw-entity observation fields.

**Verification anchors**: `crates/nara_identity/src/tests.rs`,
`crates/nara_reflect/src/tests.rs`, `crates/nara_scene/src/tests.rs`, tooling snapshot/Play tests,
and `tests/stable_runtime_identity.rs` cover world aliasing, duplicate instances, fork/restore
remaps, lookup outcomes, failure-atomic scene replacement/export, canonical serde shapes, and
count-only runtime observations. `cargo nextest run --workspace --all-features` passed 618 tests
with 3 configured skips at `9263d8c`; stale-symbol searches exclude the removed identity/map/raw
snapshot vocabulary from code, tests, and examples.

## U18-1: Privacy-Safe Bounded Diagnostic Observations

**Removed contract**:

- Runtime-owned `String` values in `DiagnosticCode`, `Diagnostic::message`, `DiagnosticContext`,
  `RuntimeDiagnosticDomain`, `RuntimeDiagnosticContext`, and `RuntimeDiagnosticEntry`.
- `Diagnostic::{error,warning,info}` constructors that accepted arbitrary strings, public context
  fields, and convenience `with_*` methods that could copy raw paths, source text, or credentials.
- Direct `RuntimeDiagnosticEntry` construction and `RuntimeDiagnostics::push`, arbitrary string
  dedupe keys, `RuntimeDiagnosticsSettings { capacity }`, `bounded()`, and
  `MAX_RUNTIME_DIAGNOSTICS_CAPACITY`.
- `RuntimeDiagnostics::{clear,dropped_entries,emit_to_tracing}`. Consumers could erase shared
  history, observe only one undifferentiated loss counter, or replay the complete retained history
  into tracing on every call.
- `Serialize`/`Deserialize` on the live `RuntimeDiagnostics` resource and `Deserialize` on
  diagnostics, settings, identities, contexts, reports, severities, and entries. Diagnostic JSON
  from the prototype is no longer an accepted input format.
- Unbounded `DiagnosticReport` collection semantics: `push`/`extend` returned no admission result,
  `diagnostics()` exposed the internal slice, `into_diagnostics()` erased loss accounting, and
  severity reflected only entries still retained.
- `DiagnosticsPlugin::new(runtime_settings)` and the assumption that runtime diagnostics were the
  only headless observation resource.
- Report-bearing `ProjectProfileError` variants containing `DiagnosticReport` inline.
- Product-plan behavior that treated `DesktopWindowPlugins` and `ToolingPlugins` as complete plans
  even though those public groups install only additive adapters.

**Canonical replacement or deletion rationale**: engine-owned codes, domains, producers, field
keys, and pressure IDs are validated source-authored identities. `SafeSummary` and
`SafeDisplayText` reject unsafe shapes before publication. Context is expressed only through typed
`DiagnosticField` values classified as public, project-relative, sensitive, or secret; sensitive
and secret constructors accept no raw value. `DiagnosticReport` and `RuntimeDiagnostics` enforce
independent entry, byte, field, and text budgets, return typed admission outcomes, use saturating
sticky accounting, and expose explicit retained-entry iteration. Runtime events enter through
`RuntimeDiagnosticDraft` plus `publish(frame)`, with structural dedupe policies and bounded
retention. Numeric pressure lives in `RuntimePressureSnapshots`, not in free-text events.
`DiagnosticsPlugin` installs and retains both resources. Report-bearing project profile errors box
the report and use privacy-safe formatting. The additive window/tooling groups remain narrow;
`add_project_plugin_plan` composes them with `MinimalPlugins` when a complete product plan is
requested. The old APIs are deleted because compatibility wrappers would preserve the leak-prone,
unbounded construction path.

Use `RuntimeDiagnostics::stats()` for distinct published, deduplicated, rejected, evicted, expired,
and truncation observations. History is removed only by configured retention/count/byte policy;
there is no consumer-owned `clear`. For tracing, keep a caller-owned
`RuntimeDiagnostics::tracing_cursor()` and call `emit_new_to_tracing`, or emit one selected entry.
For serialization, call `snapshot()` and serialize the bounded output-only snapshot. Reconstruct
live diagnostic state from current typed producer outcomes instead of deserializing old diagnostic
JSON.

**Before**:

```rust
let mut report = DiagnosticReport::default();
report.push(
    Diagnostic::error("asset.reload", format!("reload failed for {absolute_path}"))
        .with_asset_ref(absolute_path),
);
for diagnostic in report.diagnostics() {
    println!("{}", diagnostic.message);
}

let mut runtime = RuntimeDiagnostics::new(RuntimeDiagnosticsSettings { capacity: 256 });
runtime.push(
    RuntimeDiagnosticEntry::warning("asset", "asset.reload", "reload failed")
        .with_dedupe_key(format!("asset:{absolute_path}")),
);
```

**After**:

```rust
use nara_core::{ByteLimit, ItemLimit};

let code = DiagnosticCode::new("asset.reload")?;
let summary = SafeSummary::new("Asset reload failed")?;
let asset_ref = DiagnosticFieldKey::new("asset_ref")?;
let outcome = report.push(
    Diagnostic::error(code, summary)
        .try_with_field(DiagnosticField::sensitive(asset_ref))?,
);
for diagnostic in report.iter() {
    diagnostic.emit_to_tracing();
}

let settings = RuntimeDiagnosticsSettings::new(
    ItemLimit::new(256).expect("entry limit is non-zero"),
    ByteLimit::new(512 * 1024).expect("byte limit is non-zero"),
)?;
let mut runtime = RuntimeDiagnostics::new(settings);
let draft = RuntimeDiagnosticDraft::new(
    DiagnosticProducer::new("nara.asset")?,
    DiagnosticDomain::new("asset")?,
    code,
    DiagnosticSeverity::Warning,
    summary,
)
.try_with_field(DiagnosticField::sensitive(asset_ref))?
.dedupe_by_code();
let publish = runtime.publish(draft, frame);
```

Whole reports must be merged with `DiagnosticReport::extend` so sticky severity and bounded-loss
statistics survive. Use `iter`, `retained_len`, and `is_retained_empty` for retained entries;
`len` and `is_empty` describe all observed entries. Use `into_retained_diagnostics` only when the
caller intentionally consumes retained entries without merging report accounting.

Construct a configured plugin with both policies:

```rust
app.add_plugin(DiagnosticsPlugin::new(
    runtime_settings,
    RuntimePressureSettings::default(),
))?;
```

Code matching `ProjectProfileError::{InvalidManifest, UnknownProfile}` now receives
`Box<DiagnosticReport>`. Dereference or borrow the box instead of destructuring an inline report.

**Affected examples and fixtures**: `examples/headless_server.rs`,
`examples/scene_prefab_roundtrip.rs`, the root facade/prelude and product-plan tests, project
manifest/profile validation tests, asset reload diagnostics, scene/prefab/patch/Play Mode tests,
and tooling inspector/workspace tests now use only classified fields and bounded reports.

**User action**: migrate downstream Rust producers and consumers to the canonical types above.
Audit each former message/context value and choose the narrowest valid field class. Do not copy a
secret or raw sensitive value into a safe summary, public field, log, or dedupe key. Keep direct
adapter groups additive, or call `add_project_plugin_plan` for full desktop-window/tooling plans.

**Source action**: conditional `manual-rewrite`. The `nara.toml` field name and canonical version-1
shape are unchanged, but its maximum is now 4,096 instead of 65,536. Existing values from 1 through
4,096 need no edit. Manually reduce any experimental `diagnostics.runtime_capacity` in the range
4,097 through 65,536 before loading the project. Runtime auto-rewrite is intentionally unsupported.
Prototype diagnostic JSON is not project source and has no migration reader; regenerate diagnostic
snapshots from a current run.

**Cache action**: `keep`.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert `6a70847` together with migrated project/scene/tooling callers and ADR 0068.
Do not add deprecated aliases, raw-string context builders, an unbounded report, or a second runtime
diagnostic path.

**Verification anchors**: `crates/nara_diagnostic/src/contract_tests.rs` covers safe identities,
secret canaries, byte/field limits, sticky reports, dedupe, retention indexes, tombstone bounds,
pressure snapshots, headless installation, serialization, tracing cursors, and compile-fail
ownership guards. Focused project/scene/tooling/root tests cover caller migration. Sequential
verification passed 50 default diagnostic tests, 51 `serde` diagnostic tests, strict diagnostic
Clippy, two compile-fail doctests, the workspace check, and all five architecture documentation
tests. Stale-symbol searches exclude the removed runtime context/domain, raw dedupe builder,
`diagnostics()` accessor, and owned report extraction path.

## RGF-U1-1: Stable Schema Identity and Canonical Persistence Files

**Removed contract**:

- Persistent schema entries containing `rust_type_path`, native Rust/Bevy identity, or codecs.
- Name-only durable patch targets and persistent `ComponentFieldPath` addresses.
- Registry mutation after runtime publication and access to an unfrozen candidate as runtime truth.
- Direct file deserialization into `SceneDocument`, `PrefabDocument`, or `ScenePatchDocument`.
- Prototype scene, prefab, patch, and schema-catalog shapes without the shared canonical envelope.
- Speculative save, animation, replication, scripting, diagnostic, and runtime-only capability
  values in canonical version 1.

**Canonical replacement or deletion rationale**: stable `ComponentTypeId` and
`ComponentFieldId` values are permanent durable identity; aliases and current value paths may
change. A runtime-independent catalog carries schemas, aliases, defaults, capabilities, and
tombstones, while native bindings and migration functions remain process-local. A registry
publishes one immutable snapshot only after an atomic `Building -> Frozen` validation. Scene,
prefab, standalone patch, and component-schema-catalog files use the same strict version-1 envelope
and produce bounded candidates before semantic publication.

**Before**:

```rust
ScenePatchOperation::SetField {
    entity,
    component,
    path: ComponentFieldPath::from_fields(["health"]),
    value: ComponentValue::I64(9),
}
```

**After**:

```rust
ScenePatchOperation::SetField {
    entity,
    component,
    component_version: ComponentSchemaVersion::ONE,
    field: ComponentFieldId::new("health.current"),
    value: ComponentValue::I64(9),
}
```

**Affected examples and fixtures**: component-schema export, scene/prefab round trips, patch
overrides, authoring/Inspector/Play tests, every built-in persistent component provider, and the
eight JSON/RON fixtures under `tests/fixtures/formats/v1/` use the canonical identities and file
boundary.

**User action**: assign explicit permanent field IDs and aliases to persistent Rust components;
replace name/path patch targets with field IDs and the current component version; rewrite an
old-version field write through the current value semantics instead of relying on its stable ID;
finish component registration before registry freeze; manually rewrite experimental source files
to the canonical envelope. Regenerate experimental catalogs from the current component
declarations.

**Source action**: `manual-rewrite`. Runtime auto-rewrite is unsupported because no released
compatibility window retains the prototype shapes.

**Cache action**: `delete` or regenerate derived schema catalogs and cached prototype documents;
keep source files only after manual canonical rewrite.

**Compatibility window**: none (unreleased canonical replacement). A generation-N catalog is
validated against exactly generation N-1 and its fingerprint. RGF-U1 does not retain an arbitrary
historical catalog chain or any non-v1 file migration.

**Rollback**: restore an external source backup and revert the complete RGF-U1 change. Do not add a
second reader, infer stable IDs from Rust names, or unfreeze a published registry.

**Verification anchors**: `crates/nara_core/tests/format_contract.rs`,
`crates/nara_reflect/tests/{catalog_format,registry_contract}.rs`,
`crates/nara_scene/tests/format_contract.rs`, root scene/Inspector/Play tests, and the canonical
fixtures cover ordered budget gates, post-migration shape/value growth, strict envelopes, lineage,
freeze atomicity, durable field-ID patches, old-version field-write rejection, and candidate
publication. The empty-payload golden files lock the envelope and empty
canonical shape only; construction-based non-empty round trips cover component values, prefab
embedding, patch operations, and catalog records.

## RGF-U3-1: Truthful Product Capabilities and Authorized Manifest Ingest

**Removed contract**:

- Flat mandatory root dependencies behind `default = []` and adapter aliases `winit`, `wgpu`, and
  `egui`.
- `DesktopWgpuPlugins`, `ProjectPluginPlan`, `apply_project_settings`, and ambient
  `ProjectManifest::parse_toml_file*` entry points.
- The broad default prelude as an import path for diagnostic storage, queue lifecycle, tooling,
  render batches, and backend implementation types.
- The unconsumed `nara_audio` placeholder crate and root scaffold binary.

**Canonical replacement or deletion rationale**: root Cargo features are the compiled product
ceiling: `runtime-core`, `runtime-2d`, `runtime-ui`, `presentation-2d`, `tooling`, `asset-watch`,
`desktop-winit`, `render-wgpu`, and `tooling-egui`; `serde` weakly forwards only into selected
domains. Projects use
`[runtime].preset` plus `[capabilities].requested`. The root host reads an already opened
`FileCapability`, applies the 256 KiB sentinel read and bounded TOML shape preflight, then publishes
an immutable `ProjectSettingsCandidate` only when the request fits the compiled ceiling. Plugin
service/conflict/slot closure is recorded by RGF-U4; RGF-U12-1 below records the later authorized
asset/startup-scene closure.

**Before**:

```toml
[dependencies]
nara = { path = "..", features = ["wgpu", "winit", "serde"] }
```

```rust
app.add_plugins(DesktopWgpuPlugins)?;
```

**After**:

```toml
[dependencies]
nara = { path = "..", default-features = false, features = [
    "runtime-2d",
    "presentation-2d",
    "desktop-winit",
    "render-wgpu",
    "serde",
] }
```

```rust
app.add_plugins(Runtime2dPlugins)?;
app.add_plugins(Presentation2dPlugins)?;
app.add_plugins(DesktopWinitPlugins)?;
app.add_plugins(WgpuBackendPlugins)?;
```

File-backed projects declare product intent separately:

```toml
[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d", "presentation-2d", "render-wgpu"]
```

NUI-U5A later corrected the 2D feature meaning recorded by the original RGF-U3 implementation.
Use only `runtime-2d` for headless content. Presented 2D products use the complete current examples
above. Runtime UI still carries its legacy presentation closure until NUI-U5B after NUI-U13.

**Affected examples and fixtures**: all root examples now declare `required-features`; optional
tooling/2D integration tests are target-gated; the independent reference game disables root
defaults, commits `nara.toml`, and consumes its non-default fixed timestep through authorized
manifest ingest.

**User action**: replace removed root features and bundles, move advanced imports to
`advanced_prelude`, `tooling_prelude`, `backend_prelude`, or module paths, rewrite project plugin
selection to runtime preset plus requested capabilities, and let host code open `nara.toml` before
calling `ingest_project_manifest`.

**Source action**: `manual-rewrite`; no released compatibility window exists.

**Cache action**: regenerate Cargo lockfiles and build artifacts after changing features. Project
source remains valid after the explicit `nara.toml` rewrite.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U3 change. Do not restore legacy feature aliases, ambient
file loading, the broad prelude, or the audio placeholder as compatibility shims.

**Verification anchors**: `tests/{product_capabilities,project_composition}.rs`, the locked
`tests/fixtures/product-slices/presentation-2d` consumer, the four `nara_render_wgpu` submitter
feature checks, minimal desktop example checks, and
`reference-game/tests/project_manifest_ingest.rs` prove feature/dependency truth, explicit surface
imports, authorized bounded ingest, product request rejection, and actual settings consumption.

## RGF-U11-1: Owning Native Surface and Window Retirement

**Removed contract**:

- `RawWindowHandleProvider`, its unsafe raw-handle constructor, raw getters, and manual
  `Send`/`Sync` implementations.
- Unconditional `BackendWindowHandles::remove` and infallible provider insertion.
- Freely cloneable `WindowHandleProvider`, `BackendWindowHandles::get`, and the split
  `get`-then-activate sequence that could create untracked native-window owners.
- Winit exit paths that could destroy a native window before a live wgpu surface was retired.
- Public lifecycle mutations that could mark a surface active or dropped without owning the unique
  surface binding.
- Direct public `WinitRunner::run(&mut App)` calls that bypassed managed runtime control, fault, and
  close state.
- The hand-written `WgpuRenderBackend: Resource` marker that omitted Bevy ECS resource-cache hooks
  and left the backend unavailable to render systems at runtime.

**Canonical replacement or deletion rationale**: `WindowHandleProvider::new(Arc<T>)` accepts a
typed source implementing `HasWindowHandle + HasDisplayHandle + Send + Sync + 'static` and is
consumed by registration. `BackendWindowHandles::acquire_surface` atomically issues one non-
cloneable `WindowSurfaceHandleSource` plus one control `WindowSurfaceLease`. The wgpu adapter passes
the source to safe `Instance::create_surface`, which retains it for the surface lifetime; source
`Drop` acknowledges actual owner release and the lease can only request retirement or verify that
release. Each acquisition has a non-reused per-target generation, so stale leases cannot affect a
replacement surface. Exclusivity is enforced within the `BackendWindowHandles` authority shared by
the executable's platform and renderer adapters; custom hosts must register each native target once
instead of creating independent authorities for the same target. `BackendWindowHandles` owns the
target state `Active -> RetireRequested -> SurfaceRetired -> ProviderReleased -> NativeDestroyed`,
while premature native destruction records a sticky `ExternallyDestroyed` fault.
`WgpuSurfaceState` drops its safe surface before lease confirmation in both explicit and resource-
removal paths. Wgpu registers a backend-neutral scoped retirement driver, so Winit retires only
targets it registered without invoking global plugin cleanup. The current native render system and
retirement driver are main-thread operations.

**Before**:

```rust
let provider = unsafe {
    RawWindowHandleProvider::new(raw_window_handle, raw_display_handle, platform_guard)
};
handles.insert(window_id, provider);
handles.remove(window_id);
```

**After**:

```rust
let provider = WindowHandleProvider::new(platform_window);
handles.insert(window_id, provider)?;
let (handle_source, lease) = handles.acquire_surface(window_id)?.into_parts();
let surface = instance.create_surface(handle_source)?;
handles.request_retirement(window_id)?;
drop(surface); // Actual owner drop records the acknowledgement.
lease.confirm_owner_dropped()?;
handles.release_provider(window_id)?;
handles.mark_native_destroyed(window_id)?;
```

Platform adapters should normally let their runner drive this sequence instead of invoking the
individual transitions. Renderer adapters register `WindowSurfaceRetirementDriver`; platform
runners submit only their owned target IDs and must not use global plugin cleanup as a local
retirement operation. Callers handle `WindowTargetError` and may inspect `WindowTargetSnapshot` for
tooling/tests.

**Affected examples and fixtures**: `nara_winit` constructs providers directly from
`Arc<winit::window::Window>`; `nara_render_wgpu` uses only safe owning surface creation and the shared
retirement authority. Root lifecycle tests and window/winit/wgpu unit tests cover controlled exit,
runner cleanup, duplicate and stale lease rejection, repeated transitions, external destruction,
target isolation, surface loss, device-loss invalidation through the production render system,
partial surface/device initialization cleanup, missing-target frame skips, resize reconfiguration,
backend resource removal, main-thread placement, and distinct primary-runner plus native-teardown
failure reporting.

**User action**: custom native platform adapters must replace raw-handle snapshots with an owning
typed source and implement provider/native release through the lifecycle authority. Code that
removed providers directly must request retirement and wait for `SurfaceRetired` first. For the
first-party desktop path, seal an unstarted App, admit and start a `RuntimeCandidate`, promote it to
`RuntimeInstance`, then call `WinitRunner::new(...).run(&mut runtime)`. Winit retires only its own
targets and joins native destruction with registered runtime close. Raw `App::set_runner` /
`App::run` remains a separate embedding path and cannot be admitted into a managed runtime. Custom
runners with a fallible native teardown should return
`AppRunError::runner_teardown(prior, teardown)` when both phases fail. Exhaustive `AppRunError`
matches must include the `RunnerTeardown` arm.

**Source action**: `manual-rewrite`.

**Cache action**: `rebuild`; no persistent project data or imported artifact changes.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U11 change. Do not restore unsafe raw-handle construction or
provider removal that bypasses a live surface lease.

**Verification anchors**: `tests/window_surface_retirement.rs`, `crates/nara_winit/src/tests.rs`,
and `crates/nara_render_wgpu/src/backend.rs#tests` prove owning-source retention, exact controlled order,
sticky external destruction, surface-loss provider retention, runner-scoped cleanup ordering,
device-loss/partial-init invalidation, resize reconfiguration, backend Drop fallback, and native
main-thread execution. `examples/window_surface_retirement_smoke.rs` supplies the presented-window
and resource-removal platform paths.

## RGF-U10-1: Bounded PNG Ingest and Publication

**Removed contract**:

- Image reload jobs that rebuilt ambient filesystem paths from `AssetSourceRoot` and called
  `std::fs::read` without an encoded-byte ceiling.
- Generic `image::load_from_memory` plus `to_rgba8`, which decoded and expanded before Nara could
  reserve the complete versioned modeled peak.
- `ImageImporter::import_job(&ImportJobInput)` and imported candidates that exposed raw
  `into_value`, allowing publication-overlap accounting to end before asset commit.
- Implicit deep cloning of `ImageAsset` pixel storage and cloning/equality of
  `ImageImportedAsset`. Imported candidates now carry a unique RAII reservation and are
  intentionally move-only.
- Unit-style `ImagePlugin` construction and generic/free-text image import errors, which could not
  carry validated limits or privacy-safe failure classification.
- Public imported-candidate construction that allowed callers to forge publication state.
- Importer version 1 image artifacts, whose identity described the removed decode path.

**Canonical replacement or deletion rationale**: `FileCapability::read_to_end_bounded` performs a
checked `limit + 1` sentinel read from an already-authorized handle. `ImageImporter` accepts either
an owned `ImageBytesImportRequest` or an `ImageFileImportRequest` containing that handle. Owned-byte
requests constrain importer/decode work from admission onward; they do not retroactively account
for allocations made before constructing the fixed-length `Box<[u8]>` request. File admission
reserves one encoded ceiling because the bounded `Vec` remains the decoder input. Both paths
privately capture the stable target, expected version, O(1) `AssetStateRevision`, and persistent
`AssetSlotRevision`, validate any prior image against the host overlap ceiling, and charge its
captured RGBA length before PNG scanning or task dispatch.

The lockfile-pinned `png` 0.18.1 path supports only static, non-interlaced PNG. It preflights
signature, IHDR, chunks, dimensions, pixels, RGBA bytes, and decoder-work bytes; rejects Adam7 and
unbounded `eXIf` metadata before decoder construction; rejects APNG during bounded metadata
inspection; and atomically resizes to the versioned modeled encoded/decoder-work/RGBA/publication
peak before pixel decode. `ImageImportBudgetHost` freezes aggregate and publication-overlap
ceilings. Importers may use smaller RGBA limits, but each candidate validates its captured prior
slot against the shared host ceiling and charges only that slot's actual RGBA length. Importers
requiring a larger ceiling are rejected at configuration time. Sharing occurs only by explicitly
injecting the same host; there is no global or static image-budget owner.

`ImageImportedAsset` retains its publication charge and exposes one `commit` operation; its private
constructor, target admission, and raw value extraction prevent callers from pairing a decoded
value with another handle or reload token. Commit revalidates both revisions in O(1),
chooses initial load or reload internally, and releases the modeled charge only after returning.
Initial rejection publishes no value. Reload rejection preserves the existing handle, image,
source hash, and `AssetVersion` while recording only an engine-owned diagnostic code and classified
fields. This is a PNG-specific logical allocation contract, not an arbitrary-codec,
allocator-capacity, fragmentation, heap, or OS/RSS hard limit. The importer version is now 2, so
old image artifacts are not reusable. Direct `ImageAsset::new`, serde construction, and raw image
storage mutation remain advanced in-memory paths whose callers own prior allocation policy; their
state/slot revisions still invalidate any in-flight official candidate.

**Before**:

```rust
let imported = ImageImporter::default().import_job(&ImportJobInput::new(
    record,
    png_bytes,
    dependency_digest,
    settings_hash,
    profile,
))?;
images.commit_loaded(
    handle,
    imported.into_value(),
    &mut states,
    &mut events,
    Some(source_hash),
    Some(artifact_hash),
)?;
```

**After**:

```rust
let importer = ImageImporter::with_limits(image_limits)?;
let handle = asset_server.reserve_record::<ImageAsset>(&record)?;
let expected_version = states.version(handle.id()).unwrap_or(AssetVersion::ZERO);
let imported = importer.import_image(
    ImageBytesImportRequest::new(
        record,
        png_bytes.into_boxed_slice(),
        ImageImportSettings::new(ImageColorSpace::Srgb),
    ),
    handle,
    expected_version,
    &asset_server,
    &images,
    &states,
)?;
imported.commit(&asset_server, &mut images, &mut states, &mut events)?;
```

File-backed hosts instead open the source through `DirectoryCapability`, construct
`ImageFileImportRequest` with the resulting `FileCapability`, and call `admit_file` with the same
handle, expected version, server, values, and states before task dispatch. The admitted job owns
both the file and reservation. Hosts do not reconstruct an ambient `Path` inside `nara_image`.

**Affected examples and fixtures**: `examples/asset_import_texture.rs`,
`examples/runtime_ui_panel.rs`, and `examples/windowed_sprites.rs` use the owned request and
candidate commit APIs. The independent reference game imports
`reference-game/assets/textures/player.png` through an opened capability. Root hostile fixtures
cover exact and limit+1 encoded/dimension cases, Adam7, `eXIf`, invalid CRC, truncation, and a
truncated oversized ancillary declaration. Crate tests additionally construct a complete CRC-valid
8 MiB+1 `eXIf` chunk and cover APNG, pixel/RGBA/work/aggregate limits and every task/publication terminal path.

**User action**: replace ambient image reads and generic `ImportJobInput` calls with an already
bounded owned request or a host-opened file request. Keep the candidate alive until its commit
method succeeds or fails, and explicitly inject one `ImageImportBudgetHost` wherever multiple
importers must observe one host-scoped image budget and publication ceiling. Construct
`ImagePlugin::default()` or
`ImagePlugin::with_limits(...)` and handle the typed image import/reload errors instead of matching
decoder or host error strings. Keep runtime image values behind `Handle<ImageAsset>` or borrow
them from `Assets<ImageAsset>` instead of cloning their pixel buffers. Pass `ImageImportedAsset`
by ownership into `commit`; dropping it cancels publication and releases its reservation.

**Source action**: `manual-rewrite`.

**Cache action**: delete or rebuild `.nara/import-cache` image artifacts. Importer version 1 and 2
artifact identities intentionally differ.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U10 change and rebuild image artifacts. Do not restore an
ambient unbounded loader, a generic decode shortcut, or raw candidate extraction as a compatibility
shim.

**Verification anchors**: `crates/nara_fs/tests/filesystem_contract.rs`,
`crates/nara_asset/src/storage.rs#tests`, `crates/nara_image/src/tests.rs`,
`crates/nara_image/tests/image_import_limits.rs`, `tests/image_import_limits.rs`, and
`reference-game/tests/image_asset_safety.rs` prove bounded reads, O(1) state and persistent slot
revisions, pre-decode rejection, shared-host publication ceilings, exact release, candidate
publication, importer-version cache identity, and last-good reload. The three affected examples
compile through the public path.
Source gates reject `std::fs::read`, ambient path loading, `load_from_memory`, and `to_rgba8` in
`nara_image`.

## RGF-U4-1: Pure Plugin Composition, Sealed App, and Typed Schedules

**Removed contract**:

- Instance-owned `Plugin::metadata` / `plugin_id` and parallel `PluginGroupMetadata` member arrays.
- Imperative group expansion through a builder borrowing `&mut App`, build-time dependency
  installation, and public `add_plugin_if_missing` prerequisite policy.
- Public `App::finish_plugins`, `App::cleanup_plugins`, `Plugin::cleanup`, cleanup error/context
  names, and the separate `add_startup_systems` spelling.
- Hook-time plugin/group installation or runner selection, including calls whose returned errors
  were ignored.
- Root product composition that inferred requirements from a plugin-ID switchboard or applied
  settings before plugin/service/provider closure was known.

**Canonical replacement or deletion rationale**: each plugin type owns one static
`PluginDeclaration`. Repeatable configuration is represented by typed helpers returning
`PluginDefinition`; data-only `PluginGroupBuilder` values lower stable slots and edits into a pure
`PluginPlan`. Product composition keeps `CompositionError`, `PluginPlanError`,
`PluginPrepareError`, and `PluginError` as distinct phases, binds the request to its opaque project
lineage, and freezes selected schema providers before publishing `RuntimePlan`. A direct code-first
App uses the same resolver, closes configuration with `App::seal`, and tears down through
`App::shutdown_plugins` or `App::run`. Different plugins cannot claim one slot, and first-party
close owners must register their declared `PluginShutdownObligationId` before sealing.

All schedules now live in one typed registry. `add_systems`, `configure_sets`, `init_schedule`, and
inspection accept any `ScheduleLabel`. `run_schedule` is the custom-schedule entry point: it rejects
built-in startup/core stages, validates that the custom schedule exists without sealing on a
missing label, seals the App, and then runs the schedule. Custom schedules remain inert unless an
owner explicitly drives them; the built-in frame order is unchanged.

**Before**:

```rust
impl Plugin for GamePlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(GAME_PLUGIN_ID, PluginCategory::Runtime)
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(DependencyPlugin)?;
        Ok(())
    }

    fn cleanup(&self, context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
        context.world_mut().remove_resource::<GameOwner>();
        Ok(())
    }
}

app.add_plugin(GamePlugin)?;
app.add_startup_systems(StartupStage::Core, initialize)?;
app.finish_plugins()?;
app.cleanup_plugins()?;
```

**After**:

```rust
const GAME_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(GAME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[DEPENDENCY_PLUGIN_ID]);

impl Plugin for GamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &GAME_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::Update, update_game)?;
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        context.world_mut().remove_resource::<GameOwner>();
        Ok(())
    }
}

let mut app = App::new();
app.add_plugins((MinimalPlugins, DependencyPlugin, GamePlugin))?;
app.add_systems(StartupStage::Core, initialize)?;
app.init_schedule(GameMaintenance)?;
app.add_systems(GameMaintenance, maintain_game)?;
app.run_schedule(GameMaintenance)?;
let sealed = app.seal()?;
```

The integrated project path should start from `project_runtime_plugins(&candidate)` or a typed
game helper wrapping it, apply type-directed `disable` / `configure` / relative-order edits, and
call `resolve_runtime_plan` with the compiled provider catalog. Do not construct definition IDs,
configuration fingerprints, slot constants, erased factories, or live plugin instances merely to
configure ordinary first-party groups.

**Affected examples and fixtures**: all first-party plugin declarations and default groups, root
examples, the independent reference game, task/window/render/UI adapters, public-prelude fixtures,
plugin composition tests, and custom schedule tests use the canonical APIs.

**User action**: move invariant metadata to `Plugin::declaration`; return `PluginDefinition` from
typed configuration helpers; make `PluginGroup::build(self)` return a data-only builder; declare
dependencies/services/conflicts/providers instead of installing them in hooks; replace startup
registration with `add_systems`; replace manual finish/cleanup calls with `seal`, `run`, or explicit
`shutdown_plugins` as ownership requires. Configurable project code should edit a lineage-bound
request and resolve it before acquiring an App or native service.

**Source action**: `none`.

**Cache action**: `keep`; plugin-plan and schema fingerprints are runtime validation identities,
not a compatibility promise for pre-U4 generated caches.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U4 commit and its callers together. Do not restore metadata,
finish/cleanup aliases, imperative group mutation, or hidden hook installation as compatibility
wrappers.

**Verification anchors**: `crates/nara_app/tests/plugin_composition.rs`,
`crates/nara_app/tests/schedule_registry.rs`, `tests/plugin_composition.rs`,
`tests/product_capabilities.rs`, `reference-game/tests/plugin_composition.rs`, and
`reference-game/tests/authoring.rs`; stale-symbol searches reject the removed lifecycle/group API.

## RGF-U28-1: Public Semantic Schedule Anchors and Seal Validation

**Removed contract**:

- Treating every public schedule/set variant, concrete first-party system, or registration order as
  an extension compatibility promise.
- Deferring invalid `CoreStage::FixedUpdate` graphs until the first runtime execution panic.
- Allowing a fixed schedule with disabled automatic deferred insertion or disabled final deferred
  application to publish as a compatible sealed App.
- Exposing raw mutable built-in schedules that could replace the fixed graph, executor, or build
  passes after Nara installed its semantic anchors.
- Requiring a root-only or renamed-root extension to add a direct `bevy_ecs` dependency merely to
  derive its own `Resource`, `ScheduleLabel`, or `SystemSet`.

**Canonical replacement or deletion rationale at the U28 closure point**: the first-playable
compatibility inventory was exactly `CoreStage::FixedUpdate` for typed registration plus joinable
`FixedUpdateSet::Simulate`, `GameplayCommandSet::Consume`, and `GameplayCommandSet::Capture`.
Their Rustdoc owned entry, completion, deferred, skip, error, and retention semantics.
`App::seal` required automatic deferred insertion, restored final deferred application, built the
fixed schedule, and returned a structured `ScheduleCompatibilityError` instead of publishing an
invalid graph. `get_schedule_mut` now applies only to custom schedules; built-in schedules use
`add_systems`, `configure_sets`, `set_schedule_build_settings`, and
`set_schedule_apply_final_deferred`. Custom schedules remain owner-defined and inert.

An explicit `before_ignore_deferred` / `after_ignore_deferred` relation is trusted advanced code:
it may seal, but it opts out of the public deferred-visibility contract. Ordering against an absent
set or a set populated only in another schedule remains ineffective rather than becoming a hidden
cross-schedule edge. Unordered peers retain no relative-order promise.

External packages that depend only on `nara`, including a renamed dependency, can derive the ECS
types needed for extension sets through the facade-safe root exports:

```rust
use engine::ecs::{Resource, ScheduleLabel, SystemSet};
use engine::ecs::schedule::IntoScheduleConfigs;
```

Do not import those derive macros from `engine::ecs::schedule`; that module remains the direct Bevy
schedule surface. Do not add a direct `bevy_ecs` dependency solely to make a Nara root-package
extension compile.

**Affected examples and fixtures**: the renamed-root schedule-extension fixture exercises all four
anchors, deferred visibility, skip, cleanup, ignore-deferred opt-out, registration permutation, and
absent/cross-schedule negative cases. The reference game records that its current scheduling
dependencies stop at `CoreStage::FixedUpdate` and `FixedUpdateSet::Simulate`.

**User action**: order extensions only against the documented inventory. If another engine phase is
needed as a compatibility dependency, extend ADR 0003, the external conformance fixture, and owner
Rustdoc before consuming it. Handle `PluginError::ScheduleCompatibility` when accepting arbitrary
schedule configuration before sealing. Replace raw built-in `get_schedule_mut` calls with the
controlled App methods; raw custom-schedule mutation remains available before sealing.

**Source action**: `none`; no persistent project format changes.

**Cache action**: `keep`; rebuild Rust artifacts after the derive and seal-contract changes.

**Compatibility window**: none (pre-1.0 public-contract correction).

**Rollback**: revert the U28 seal validation, derive adapters, owner documentation, and external
fixture together. Do not preserve a facade that compiles only through transitive Bevy visibility or
restore runtime-panic graph validation as a compatibility layer.

**Verification anchors**: `crates/nara_app/tests/schedule_compatibility.rs`,
`crates/nara_gameplay/src/lib.rs#tests`, `tests/schedule_extension_contract.rs`,
`tests/fixtures/schedule-extension/renamed-root/`, and
`reference-game/tests/public_surface.rs`.

## RGD-U11-1: Frame-End Cleanup Anchor Correction

The first U28 inventory was intentionally narrow, but the desktop first-playable measurement later
needed to inspect the submitted frame in the same frame, after the render pipeline had completed.
Moving that observation to the next fixed tick would change the frozen startup boundary. The
correction therefore adds `CoreStage::Cleanup` as a schedule-label anchor rather than exposing
`FixedUpdateSet::Finalize` or any other internal phase.

`Cleanup` runs after `Render` and before `Last`, including while paused. It is an observation and
retirement point, not proof that a present occurred. `App::seal` applies the same automatic
deferred-insertion and final-flush validation to `FixedUpdate` and `Cleanup`. The renamed-root
fixture now covers both schedule labels; the reference-game probe registers its parity publication
after the documented `GameplayCommandSet::Capture` anchor and does not depend on
`FixedUpdateSet::Finalize`.

This is a pre-1.0 compatibility correction. The remaining `CoreStage` variants and
`FixedUpdateSet::{Prepare,Finalize}` remain callable implementation API but are not public
ordering promises.

## RGF-U5-1: Thin Code-First Runtime and Truthful Close

**Removed contract**:

- `WinitRunner::install(&mut App)` and the first-party platform path that retained raw App authority
  or called `App::run_once` directly.
- Treating plugin shutdown completion as proof that waitable task/service owners had also closed.
- Treating a once-only plugin shutdown hook error as retryable unfinished ownership, or treating a
  `Stopped` ownership state as proof that teardown succeeded.
- Leaking an unfinished `App`, `World`, and close ledger with `mem::forget` after abnormal Drop.
- Task shutdown that discarded unfinished worker `JoinHandle` values at a deadline and reported the
  resulting detached work as a terminal outcome.
- The ambiguous standalone `TaskPools::shutdown` name; it synchronously drives configured close
  deadlines and is not the managed runtime's nonblocking close path.
- Silent loss of engine-owned local gameplay intent and ignored Admit/Acknowledge lifecycle errors.

**Canonical replacement or deletion rationale**: `RuntimeCandidate` admits one sealed, unstarted
App with no raw runner and moves every explicitly registered close obligation into one unpublished
owner. Successful startup produces `ReadyRuntimeCandidate`; its consuming `promote` operation is
infallible and yields a generation-scoped `RuntimeInstance`. The App remains the only World,
schedule, plugin, time, and tracker authority. Runtime controls apply at App safe points, exact step
runs one complete fixed/gameplay transaction, the first typed fault is sticky, and `Stopped` means
only that every registered runtime-owned close participant completed. `CloseIncomplete` retains the
same owner for `RetryClose`; arbitrary resources remain caller-owned unless explicitly transferred.
`RuntimeCandidate::scope_world_mut` and `RuntimeInstance::with_driver_scope` now return
`Result<_, RuntimeScopeError>`. Both bind the canonical runtime reporter while the short-lived World
scope runs and verify reporter/handler authority around healthy execution. An unhandled fallible
system or observer that reaches the canonical fallback records a sticky fault and returns
`RuntimeScopeError::Faulted`; an explicit system- or observer-specific error handler remains a
caller-owned handling boundary. Candidate access rejects an existing fault, while driver access
remains available on a faulted runtime for retirement work until `Stopped`.
An attempted plugin shutdown hook may fail after all waitable owners complete: the runtime then
reaches `Stopped` as an ownership state, while `RuntimeCloseEvidence::plugin_shutdown_failed`, the
`Failed(CloseFailed)` control result, and the Winit teardown error preserve the failed outcome.

Abnormal Drop of an admission failure, startup failure, or published runtime first performs one
bounded close pass. If work remains, the complete `App`, `World`, and obligation ledger move into a
bounded owner-thread-affine quarantine instead of becoming unreachable. Hosts can inspect
`runtime_quarantine_status` and call `drive_runtime_quarantine` from that owner thread; exhausting a
per-thread/process ceiling or exiting the owner thread with retained state fails closed rather than
silently detaching ownership.

`TaskPlugin` now registers the move-only worker owner separately from the World-facing `TaskPools`
facade. A deadline records timeout history without dropping a worker handle. Standalone callers may
use the explicitly named `shutdown_blocking` and retry it on the same pools. Abnormal owner Drop is
nonblocking and moves unfinished handles plus pending destruction into process-owned retained
quarantine. A bounded internal lane set keeps one blocking destructor from monopolizing the owner
coordinator; each receipt remains pending until that owner's pending queue, in-flight destruction,
and worker handles are empty. This fallback never counts as managed `Stopped` evidence.

**Before**:

```rust
let mut app = App::new();
app.add_plugins((MinimalPlugins, WindowPlugin::default(), WgpuBackendPlugins))?;
WinitRunner::default().install(&mut app)?;
app.run()?;

let report = pools.shutdown();
```

**After**:

```rust
let mut app = App::new();
app.add_plugins((MinimalPlugins, WindowPlugin::default(), WgpuBackendPlugins))?;
let reservation = RuntimeAdmissionReservation::try_acquire()?;
let candidate = reservation.admit(
    app.seal()?,
    RuntimeObligationLedger::new(),
    RuntimeClosePolicy::default(),
)?;
let mut runtime = candidate.complete_startup()?.promote();
WinitRunner::default().run(&mut runtime)?;

let report = pools.shutdown_blocking();
```

Manifest-free and headless code may drive the same runtime explicitly:

```rust
let pause = runtime.request_control(RuntimeControl::Pause);
runtime.drive(Duration::ZERO)?;
let step = runtime.request_control(RuntimeControl::StepFixedTick);
runtime.drive(Duration::ZERO)?;
let stop = runtime.request_control(RuntimeControl::Stop);
while !matches!(runtime.state(), RuntimeState::Stopped | RuntimeState::CloseIncomplete) {
    runtime.drive(Duration::ZERO)?;
}
```

Control requests return Accepted/Rejected immediately and their generation-scoped tickets expose
Pending/Applied/Failed results separately. A `CloseIncomplete` runtime accepts `RetryClose`, not a
second `Stop`; `CloseFailed` distinguishes terminal teardown failure from incomplete ownership. Raw
`App::set_runner` / `App::run` remains supported for low-level embedding, but a
sealed App carrying that runner intentionally fails managed candidate admission.

**Affected examples and fixtures**: `windowed_clear`, `windowed_sprites`, `runtime_ui_panel`, and
`window_surface_retirement_smoke` now start and pass `RuntimeInstance` to Winit. The independent
reference game exposes a manifest-free managed runtime helper and drives every early-return path
through bounded Stop. Image tests use the renamed standalone task shutdown API.

**User action**: first-party-style platform integrations must drive `RuntimeInstance`, use only a
short-lived `with_driver_scope` to project normalized events, and join their native teardown with
runtime close. Handle `RuntimeScopeError` instead of assuming World projection is infallible. Code
that needs retryable close must retain the runtime and handle
`CloseIncomplete`; code that observes `Stopped` must still inspect its control/runner result for
terminal teardown failure. Do not rely on destructor completion. Keep arbitrary external resources outside
`Stopped` claims unless their owner is explicitly registered. Rename direct task-pool shutdown calls
to `shutdown_blocking` and retain the same `TaskPools` value when retrying an incomplete report.

`RuntimeCandidate::admit` and `RuntimeCandidate::admit_with` were removed before 1.0. Acquire
`RuntimeAdmissionReservation` before sealing or transferring the App and obligation ledger. A
capacity error leaves those inputs with the caller. Once `reservation.admit(...)` begins, any later
admission failure owns the App, obligations, and reserved fault route; drive its explicit retirement
path instead of attempting to recover the transferred inputs.

`RuntimeFaultKind::ScheduleAuthority` was also removed. Independent managed runtimes may execute
overlapping Bevy schedules; fallible systems, conditions, commands, and observers now report only
through the route reserved for their owning runtime.

`App::add_observer` now records configuration that becomes active at the first raw execution or
managed-runtime admission boundary. Events triggered while the App is still being configured are
not replayed to those observers. Once any raw schedule has begun execution, the App cannot later be
admitted as a managed runtime; construct a fresh App so every observer and fallback handler is bound
to one execution authority from the start.

**Source action**: `none`; no persistent project file changes.

**Cache action**: `keep`; rebuild Rust artifacts after the API change.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U5 commit and all managed-runner callers together. Do not
restore a raw first-party Winit path, false `Stopped`, or detached-worker success reporting as a
compatibility layer.

**Verification anchors**: `tests/runtime_instance.rs`, `tests/runtime_driver_boundary.rs`,
`reference-game/tests/runtime_core.rs`, `crates/nara_app/src/lib.rs#tests`,
`crates/nara_tasks/src/tests/close.rs`, `crates/nara_tasks/src/tests/execution.rs`, and
`crates/nara_winit/src/tests.rs` prove admission,
generation isolation, exact step, qualified fault propagation, caller-owned resources, once-only
shutdown, retryable close, retained worker ownership, and runtime/native teardown joining.
The same matrix covers abnormal admission/start/runtime Drop quarantine, fault-resource replacement,
candidate/driver/close observer fallback capture, ready-to-publish fault races, pending-Stop surface
retirement, initial incomplete helper retry, normal panic unwind, and required/optional task and
service integration.

## RGF-U12-1: Authorized Immutable Startup Content

**Removed contract**:

- Treating `nara.toml` ingest as a complete file-backed boot path while startup scene, prefab, and
  image content were still manually imported or seeded by Rust startup systems.
- Reopening project content through ambient `std::fs`/`Path` authority, canonicalize-and-reopen, or
  an implicit whole-root scan after the manifest handle was authorized.
- Unbounded scene/prefab decode, ad hoc `.meta` JSON, and public `ImageAsset: Clone` payload escape
  outside a retained content budget.
- Publishing partial asset/document state while a dependency, schema, importer, or aggregate budget
  failure was still possible.

**Canonical replacement or deletion rationale**: `ProjectContentLoader` owns one host-issued
project `DirectoryCapability` and requires a `ProjectSettingsCandidate` plus `RuntimePlan` with the
same opaque lineage, root identity, and frozen schema input. It follows only the startup scene's
path-addressed prefab and reflected image references. Scene, prefab, canonical `asset_meta`, and PNG
boundaries retain their own strict decoders beneath one aggregate budget host. Successful loading
publishes one immutable `ProjectContentSnapshot`; failures publish nothing.

The snapshot carries lineage, schema fingerprint/generation, original and expanded scene
documents, prefab/image values, content revision/digest, and one retained residency lease. Cloning
the snapshot shares documents and pixel payloads rather than duplicating them. `ImageAsset` itself
is no longer `Clone`, so a public value clone cannot outlive the snapshot charge. The snapshot
contains no source capability, runtime plan, App, service, native binding, backend value, runtime
handle, or target World.

**Before**:

```rust
let scene_bytes = std::fs::read("scenes/startup.scene.json")?;
let scene = SceneFileCandidate::decode_json_bytes(&scene_bytes)?;
// Prefabs and images were opened and imported separately by caller-specific code.
```

**After**:

```rust
let loader = ProjectContentLoader::new(project_root)?;
let snapshot = loader.load(&settings_candidate, &runtime_plan)?;

let scene = snapshot.expanded_startup_scene();
let images = snapshot.images();
```

**Affected examples and fixtures**: the independent reference game now commits
`scenes/startup.scene.json`, `prefabs/enemy.prefab.json`,
`assets/textures/player.png.meta`, and the PNG source. Root and reference-game boot tests randomize
current/home directories and consume only public project APIs. Boundary tests prohibit ambient
filesystem access, hidden module/include/macro bypasses, whole-root indexing, authority-bearing
snapshot fields, and payload cloning.

**User action**: use `AssetRef::Path` for the first file-backed startup closure, commit canonical
version-1 asset metadata, and pass the same authorized settings candidate and resolved runtime plan
to `ProjectContentLoader`. Keep the returned snapshot alive for every consumer of its documents or
imported values. Stable-ID-only lookup requires a future admitted index and currently rejects.
Parented transforms and inherited visibility remain rejected until their completed runtime
projections are implemented.

**Source action**: `manual-rewrite` for experimental `.meta` files that do not use the canonical
`asset_meta` envelope. Existing canonical scene/prefab files remain unchanged.

**Cache action**: `delete` or regenerate prototype metadata-derived/imported artifacts after the
source metadata rewrite; keep canonical source content.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U12 implementation and its reference-game content together.
Do not restore ambient project opens, whole-root scans, cloneable leased payloads, or partial
publication as compatibility paths.

**Verification anchors**: `tests/project_content_boot.rs`,
`tests/project_content_boundary.rs`, `tests/project_content_limits.rs`,
`crates/nara_asset/tests/meta_format.rs`, `crates/nara_scene/tests/format_contract.rs`,
`crates/nara_fs/tests/path_contract.rs`, `reference-game/tests/project_content_boot.rs`, and
`reference-game/tests/prefab_startup.rs` prove authorized randomized-directory boot, strict
formats/references, aggregate `limit + 1`, high-water/release accounting, immutable shared
residency, source stability, and the World-independent boundary.

## RGF-U29-1: Explicit Persistent Composition and Guarded Apply

**Removed contract**:

- Public `PreparedComponent::new` / `PreparedComponent::insert`, which allowed a codec to construct
  an applicable value without proving that its Rust type matched the registry binding.
- Treating a codec result from a building registry as runtime-ready state.
- Allowing Bevy `#[require]` metadata or intrinsic component hooks to add persistent composition
  that was absent from the Scene/Prefab record.
- Applying persistent values to a target `World` without rechecking late required-component
  metadata, lifecycle hooks, and matching lifecycle observers.

**Canonical replacement or deletion rationale**: codecs now return the non-applicable
`PreparedComponentCandidate`. `insert` owns a complete value, `deferred` proves through its closure
signature that delayed work cannot access target-World resources, and `with_asset_server` declares
possible apply-time asset resolution. Only a frozen `ComponentRegistry` can bind a candidate's Rust
`TypeId`, stable component ID, registration function, and target-World validator into
`PreparedComponent`.

Persistent Scene/Prefab records are the complete persistent component set. Provider validation
rejects intrinsic requirements and hooks. Every public persistent apply flushes deferred
registration, validates the applicable target topology, and rejects late requirements, hooks, or
matching Add/Insert/Discard/Remove/Despawn observers before its first persistent mutation. This is
a guarded apply contract, not rollback for arbitrary hook side effects. Runtime-only ECS behavior
remains unchanged.

**Before**:

```rust
Ok(PreparedComponent::new(move |context| {
    let image = context.resolve_asset_ref::<ImageAsset>(&image_ref)?;
    Ok(Sprite::from_image(image))
}))
```

**After**:

```rust
Ok(PreparedComponentCandidate::with_asset_server(move |context| {
    let image = context.resolve_asset_ref::<ImageAsset>(&image_ref)?;
    Ok(Sprite::from_image(image))
}))
```

For an already decoded value, return `PreparedComponentCandidate::insert(value)`. For delayed work
that cannot resolve assets, use `PreparedComponentCandidate::deferred(|| ...)`.

**Affected examples and fixtures**: built-in Sprite, Tilemap, and runtime UI codecs now declare
asset access only when their decoded value contains a deferred asset reference. Scene, Prefab,
authoring, Play Mode, direct apply, derive, and independent reference-game fixtures all use the
registry-bound path.

**User action**: update hand-written codec preflight closures to return
`PreparedComponentCandidate`. Remove `#[require]` and intrinsic component hooks from persistent
types; model every stored component explicitly in Scene/Prefab data and perform derived/runtime
projection after persistent publication. Freeze the registry before calling runtime preflight and
handle `ComponentCodecError::PersistentApplyRejected` or
`ComponentCodecError::PersistentApplySupportRejected` without retrying in place.

**Source action**: `none`; the canonical Scene/Prefab component records already express the stored
set explicitly.

**Cache action**: `keep`; rebuild Rust artifacts after the API change.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U29 code and callers together. Do not restore public
applicable-value construction or infer missing persistent components through Bevy lifecycle
metadata.

**Verification anchors**: `crates/nara_ecs/src/__private.rs#tests`,
`crates/nara_reflect/src/persistent_apply.rs#persistent_apply_contract_tests`,
`crates/nara_reflect/tests/registry_contract.rs`, `crates/nara_scene/src/tests.rs`,
`tests/scene_component_composition.rs`, `tests/scene_sprite_serialization.rs`, and
`reference-game/tests/authoring.rs` cover static provider rejection, frozen binding, every
lifecycle event and observer scope, late required metadata, binding authority, asset-access
classification, exact persistent composition, and external construction denial.

## RGF-U24-1: Concrete Headless Product Action and Atomic Runtime Publication

**Removed contract**:

- Ordinary file-backed callers manually joining manifest ingest, product/plugin/schema planning,
  startup-content loading, App construction, candidate admission, publication, exact stepping, and
  retirement.
- Treating `ReadyRuntimeCandidate::promote` as the product publication boundary even though a
  separate Host-visible slot could still reject or race with a sticky fault.
- Letting partial threaded task-pool construction synchronously wait or transfer earlier owners to
  process fallback before a product runtime ledger could retain them.

**Canonical replacement or deletion rationale**: use the root `HeadlessRunIntent<O>` and
`HeadlessRun<O>` action for file-backed headless products. The action owns one authorized project
root, one run intent, semantic command submissions, and one output resource type. It returns
`HeadlessRunOutcome<O>` plus structured diagnostics. Calling `execute_bounded` after
`CleanupIncomplete` drives only the retained cleanup owner; it never reopens source, reconstructs
the runtime, or resubmits commands.

The private root Host owns plan/content lineage checks, App/ledger construction, candidate
admission, U29 guarded scene materialization, startup, and retirement. Advanced Host integrations
that already own a ready candidate publish through the single-use `RuntimePublicationSlot`; the
reporter lock linearizes the final fault check and ownership/visibility transition. Direct
code-first `App` and `ReadyRuntimeCandidate::promote` remain advanced paths without the file-backed
product action's Host publication guarantee.

**Before**:

```rust
let candidate = ingest_project_manifest(&manifest, profile)?;
let plan = resolve_runtime_plan(&candidate, plugins, providers)?;
let snapshot = ProjectContentLoader::new(project_root)?.load(&candidate, &plan)?;

// Caller then prepared/committed App, admitted and started a RuntimeCandidate,
// materialized the scene, promoted the runtime, drove ticks, and joined retirement.
```

**After**:

```rust
let recipe = ProductRecipe::new().add_plugin::<GamePlugin>()?;
let intent = HeadlessRunIntent::<GameSnapshot>::new(fixed_ticks).with_recipe(recipe);
let mut run = HeadlessRun::new(project_root, intent, commands);

match run.execute_bounded().outcome() {
    HeadlessRunOutcome::Completed(snapshot) => use_snapshot(snapshot),
    HeadlessRunOutcome::CleanupIncomplete => retry_later(),
    HeadlessRunOutcome::Failed => report_failure(),
}
```

**Affected examples and fixtures**: the independent reference-game headless binary and
`runtime_drive` tests now use the concrete product action. The U26 manual raw-App fixture remained
through the original Host comparison, then RPR-U4 removed its parallel lifecycle implementation.
Its frozen counterfactual remains in Git history and
`docs/benchmarks/runtime-ownership-baseline.md`, not as a current application template.

**User action**: file-backed headless callers should move manifest/content/runtime lifecycle code
behind `HeadlessRun`. Put replayable runtime plugins in one `ProductRecipe`; bind every
schema-owning plugin and its providers through one `SchemaContribution`. Submit semantic gameplay
commands, retain the action while cleanup is incomplete, and emit only the returned structured
diagnostics. Code-first embedding may continue to use `App`; non-replayable test or fault probes
use the narrow `project_host::advanced::RuntimeProbe` path and must not become ordinary game
configuration.

**Source action**: `none`; this migration changes Rust ownership/API usage, not project files.

**Cache action**: `keep`; rebuild Rust artifacts after the API change.

**Compatibility window**: none (pre-1.0 fearless replacement for the product path). The advanced
code-first runtime path remains intentionally supported rather than shimmed through `HeadlessRun`.

**Rollback**: revert the complete U24 product action and its callers together. Do not restore a
second public lifecycle builder, publish an already-started raw `App`, flatten structured failure
phases, or drop an incomplete retirement owner to simulate success.

**Verification anchors**: `tests/project_runtime_boot.rs`, `tests/project_host_boundary.rs`,
`tests/runtime_instance.rs`, `crates/nara_app/tests/plugin_composition.rs`,
`crates/nara_tasks/src/tests/{execution,close}.rs`, `reference-game/tests/runtime_drive.rs`, and
the historical manual baseline at revision `f90e1b9b3235a8c5a8544eca9ba442bb7b81fd9f`
prove the caller boundary, lineage/schema binding, failure phases, publication linearization,
owner retention, and U26 outcome parity without retaining a second current startup path.

## RGF-U6-1: Bounded Authoritative Headless Product Contract

**Removed contract**:

- `HeadlessRun::new` accepting an arbitrary `IntoIterator` and collecting it during product action
  construction, which left the ordinary boundary unable to prove finite caller work.
- Treating a fixed number of ticks and an arbitrary outcome resource as successful even when the
  product had not reached a semantic terminal state.
- The reference-game binary as a first-tick tracer without a stable stdout schema, explicit
  tick-limit failure, or one bounded cleanup deadline.

**Canonical replacement or deletion rationale**: `HeadlessRun::new` now requires an owned
`Vec<GameplayCommandSubmission>`. Producers must finish collection and their own untrusted-input
budgeting before handing commands to the Host; runtime admission still validates every submission.
`HeadlessRunIntent::stop_when` captures the typed outcome only after a complete matching fixed tick,
while its `NonZeroU32` tick count remains a hard upper bound. Reaching that bound without a match is
`project.run.tick-limit`, not success.

The reference game uses this action with bundled typed commands and a `WaveSnapshot` terminal
predicate. Completed and Defeated emit one
`nara-reference-game.wave-summary-v1` JSON record on stdout and exit zero. Project, content,
command, runtime, tick-limit, stdout-write, and cleanup-deadline failures emit only bounded static
diagnostics on stderr and exit nonzero. External scenario paths and serialized command files remain
unsupported until a separately budgeted Adapter is admitted.

```rust
let commands: Vec<GameplayCommandSubmission> = build_bounded_commands()?;
let intent = HeadlessRunIntent::<WaveSnapshot>::new(maximum_ticks)
    .stop_when(WaveSnapshot::is_terminal);
let mut run = HeadlessRun::new(project_root, intent, commands);
```

**Affected examples and fixtures**: the independent reference-game headless binary, first-wave and
snapshot tests, root project Host tests, and external compile fixture now use the owned command
buffer and terminal predicate contract.

**User action**: collect command producers into a finite `Vec` before constructing `HeadlessRun`.
If commands originate from an untrusted file, network, replay, or package source, enforce encoded
byte/depth/count budgets in that Adapter before collection. Use `stop_when` for semantic product
completion and treat `tick-limit` as failure.

**Source action**: `none`; no persistent project format changes are required.

**Cache action**: `keep`; rebuild Rust artifacts after the signature change.

**Compatibility window**: none (pre-1.0 fearless replacement). No iterator compatibility shim or
external scenario loader is retained.

**Rollback**: revert the complete U6 product proof and callers together. Do not restore an
unbounded iterator boundary, return a failed-frame snapshot as success, or bypass the Host cleanup
owner.

**Verification anchors**: `reference-game/tests/{first_wave,headless_cli,headless_snapshot}.rs`,
`tests/{reference_game_contract,project_runtime_boot,project_host_boundary}.rs`, and the
`headless_run_constructor_requires_an_owned_command_buffer` type and syntax contract prove stable
terminal ticks, same-tick Defeated priority, last-good snapshot retention, privacy-safe CLI sinks,
bounded cleanup, public-only game code, and the owned command boundary.

## RGF-U13-1: Typed Desktop Driver and Ordered Button Transitions

**Removed contract**:

- `RuntimeDriverScope::{world, get_resource_mut, resource_mut, get_non_send_resource_mut,
  non_send_resource_mut}`, which let a platform adapter inspect or mutate arbitrary live runtime
  storage during managed execution.
- Infallible `ButtonInput::press` and `ButtonInput::release` plus opposite-edge-cancelling
  `just_pressed`/`just_released` sets. Those sets could not preserve press then release in one
  platform frame and could not reject input storms atomically.
- Headless product driving that consumed `AppExitRequests` without returning the exit result to the
  product Host.

**Canonical replacement or deletion rationale**: `RuntimeDriverScope` now applies only a typed
resource-local `__RuntimeDriverPort` operation whose resource type owns the complete input/output
surface and declares accepted runtime states. The carrier is intentionally doc-hidden pre-1.0
plumbing, not a frozen public Platform Adapter Interface; it prevents ambient `World` access while
OQ-038 waits for a second production adapter.

`ButtonInput` keeps retained pressed state plus a bounded monotonic `ButtonTransition` list.
`press`, `release`, and `release_all` return `Result`; capacity and sequence exhaustion leave both
retained state and transitions unchanged. `ActionMap` consumes edges in sequence order, and the
engine clears only the transient list at the declared frame boundary. `HeadlessRun` and
`DesktopRun` both propagate a completed-frame App exit request as a product result.

**Before**:

```rust
fn update_input(input: &mut ButtonInput<KeyCode>) {
    input.press(KeyCode::Character('w'));
    input.release(KeyCode::Character('w'));
}
```

**After**:

```rust
fn update_input(input: &mut ButtonInput<KeyCode>) -> Result<(), ButtonInputError> {
    input.press(KeyCode::Character('w'))?;
    input.release(KeyCode::Character('w'))?;
    Ok(())
}
```

Advanced Adapter code replaces each generic scope read/write with a resource-owned
`__RuntimeDriverPort` operation and calls `RuntimeDriverScope::__apply_port`. That hidden carrier
may change before 1.0. Ordinary gameplay code should consume `SemanticActions` or submit semantic
gameplay commands rather than driving platform resources. `ActionOutcomes` remains available for
low-level per-binding observations; it does not own aggregate held state or context cancellation.

**Held-action correction (2026-10-19)**: `GameplayCommandPlugin` now maps `SemanticActions` into
commands. Multiple physical bindings for one action emit one aggregate Started/Released pair.
Context exit ends the action once; restoring the context while its key is held requires release
and a fresh press. Mapping replacement also expires held actions and deferred old presses.
Consumers that need held gameplay behavior should replace raw `ActionOutcomes` edge counting with
`SemanticActions::pressed` or its frame outcomes. Physical binding diagnostics keep the raw API.
This changes Rust runtime behavior, not the persisted `ActionMap` format or replay command format.

**Affected examples and fixtures**: `nara_winit`, root runtime/Host tests, the native surface smoke,
and the independent reference-game headless/desktop paths now use the typed port and fallible
ordered input contract.

**User action**: propagate or explicitly handle `ButtonInputError` from `press`, `release`, and
`release_all`. Remove platform code that expects generic managed-World/resource access; keep custom
Adapter integration behind its own module while the shared Adapter shape remains unfrozen.

**Source action**: `none`; the desktop profile added by the reference game uses existing
`nara.toml` profile syntax and changes no persistent format version.

**Cache action**: `keep`; rebuild Rust artifacts after the API change.

**Compatibility window**: none (pre-1.0 fearless replacement). No generic driver-access shim or
opposite-edge-cancelling input alias is retained.

**Rollback**: revert commit `198a680` and all desktop callers together. Do not restore ambient
managed-World mutation, collapse same-frame edges, or hide App exit from a product Host.

**Verification anchors**: `crates/nara_input/src/lib.rs#tests`,
`crates/nara_winit/src/tests.rs`, `tests/{project_runtime_boot,runtime_driver_boundary}.rs`,
`reference-game/tests/{desktop_flow,desktop_parity}.rs`, and the U13 verification record prove
ordered/fallible input, focus release, typed driver state gates, headless/desktop exit parity, and
truthful desktop shutdown.

## RGF-U8-1: Domain-Owned Task Integration and Bounded Watcher Admission

**Removed contract**:

- `nara_app::TaskUpdateSet::{Poll, CoalesceAssetChanges, SpawnAssetJobs,
  ApplyAssetResults}` and `TaskPlugin` configuration of that asset-specific chain.
- `OrderedTaskResults::drain_ready_prefix`, whose call time could include a terminal published
  after a domain Poll system began.
- The watcher queue's unbounded `push`/`push_error` path and best-effort callback failures, which
  could lose events without a durable rescan state.

**Canonical replacement or deletion rationale**: `nara_app` retains only
`CoreStage::TaskUpdate`. `nara_asset::AssetTaskUpdateSet::{Poll, ResolveSourceChanges, SpawnJobs,
ApplyResults}` owns the asset-domain chain, while `nara_tasks` remains execution-only.
`TaskPools::capture_completion_cutoff` creates an instance-bound linearization point and
`OrderedTaskResults::capture_ready_prefix` moves only the ordered terminal prefix published at or
before it into an owned snapshot.

`AssetWatchEventQueue` now has explicit event and retained-byte limits. Callbacks use
`AssetWatchEventSender::try_send` or `try_send_batch`; Poll drains one published prefix. Any
overflow, producer contention, disconnect, backend/translation failure, or unavailable queue state
is counted and moves the runtime to sticky `AssetWatchRuntimeState::RescanRequired`. Recovery is a
Host-authorized full scan followed by watcher reconstruction, not an in-place flag clear.

**Before**:

```rust
app.add_systems(
    CoreStage::TaskUpdate,
    poll_results.in_set(TaskUpdateSet::Poll),
)?;
let ready = ordered_results.drain_ready_prefix();

let queue = AssetWatchEventQueue::new();
queue.push(AssetWatchEvent::modified(path))?;
```

**After**:

```rust
app.add_systems(
    CoreStage::TaskUpdate,
    poll_results.in_set(AssetTaskUpdateSet::Poll),
)?;
let cutoff = task_pools.capture_completion_cutoff();
let ready = ordered_results
    .capture_ready_prefix(&cutoff)?
    .into_terminals()?;

let mut queue = AssetWatchEventQueue::with_limits(limits);
let sender = queue.sender();
sender.try_send(AssetWatchEvent::modified(path))?;
let captured = queue.drain();
```

**Affected examples and fixtures**: `nara_asset`, `nara_asset_watch`, `nara_image`, root facade
exports, schedule-contract tests, and the independent reference-game asset task flow now use the
domain-owned set and bounded entry-snapshot contracts.

**User action**: advanced asset-domain plugins must import `AssetTaskUpdateSet` from `nara_asset`.
Consumers of ordered task streams must capture a cutoff from the same `TaskPools` instance and
handle pool mismatch/snapshot errors. Watcher integrations must retain the receiver queue, send via
its `AssetWatchEventSender`, observe `AssetWatchRuntimeStatus`, and rebuild from an authorized full
rescan after `RescanRequired`.

**Source action**: `none`; no persistent project format changes are required.

**Cache action**: `keep`; rebuild Rust artifacts after the API change. A watcher Host that observes
`RescanRequired` must rebuild its in-memory source index from an authorized full scan.

**Compatibility window**: none (pre-1.0 fearless replacement). No `TaskUpdateSet` alias,
unbounded watcher queue, or in-place rescan reset is retained.

**Rollback**: revert commit `60292e7` and every migrated caller together. Do not restore
asset-specific vocabulary to `nara_app`/`nara_tasks`, let Poll consume post-entry task completions,
or continue incremental watching after an observed event loss.

**Verification anchors**: `crates/nara_tasks/src/tests/execution.rs`,
`crates/nara_asset_watch/src/tests.rs`, `crates/nara_image/src/tests.rs`,
`tests/task_update_integration.rs`, `tests/schedule_extension_contract.rs`, and
`reference-game/tests/asset_task_flow.rs` prove pool-bound entry cutoffs, same/next-frame behavior,
stale retirement, bounded non-blocking watcher admission, observable loss, ownership, and public
last-good reload behavior.

## Receipt-Backed Editor Save and Host-Owned Play

The public `MarkSaved` authoring command and tooling-owned `ScenePlaySession` bare-World path are
removed. File-backed known-schema editor workflows now use root `EditorProjectSession`: Save returns
retained typed persistence results, Reopen is the explicit reconciliation path, and Play lifecycle
commands drive a concrete scheduled runtime owned outside `nara_tooling`.

**User action**: UI adapters must render `EditorProjectView`, submit `EditorPersistenceCommand`,
`EditorPlayCommand`, runtime-edit, Apply Changes, and workspace-intent requests, then acknowledge
retained terminal results. They must not clear dirty state directly or retain a runtime owner.

**Source action**: `none`; canonical scene bytes and schema versions are unchanged. A successful
Save advances only the captured authoring revision. `PersistenceUncertain` requires explicit
Reopen/Reconcile before another Save.

**Cache action**: `keep`; runtime generations are reconstructed from the current validated editor
document and authorized project content rather than restored from cache.

**Compatibility window**: none (pre-1.0 fearless replacement). No `MarkSaved` alias or bare-World
Play owner remains.

**Verification anchors**: `tests/editor_persistence.rs`, `tests/workspace_play_runtime.rs`,
`tests/scene_play_mode.rs`, and `crates/nara_tooling_egui/src/lib.rs#tests` prove the receipt,
dirty-close/reopen, scheduled lifecycle, safe-point edit/export, and adapter command/view contracts.

## Specialized Plugin Ownership and Render Scaling Decision Reset

ADR 0095 supersedes the unimplemented engine-owned physics/audio provider abstractions in ADR 0019
and ADR 0030. Specialized plugins now own their semantic APIs and persistent schemas; changing a
plugin may require an explicit source/configuration/cross-schema migration. `nara.toml` does not
select an arbitrary provider. ADR 0096 supersedes ADR 0053's unimplemented fixed tile chunk-cache
topology and makes allocation/cache mechanisms measurement-gated while preserving GPU ownership,
finite admission, observability, and epoch invalidation.

**User action**: `none` for the current repository. No physics/audio domain, portable provider API,
tile chunk-cache format, or public upload planner was implemented. Future users replacing one
specialized plugin with another must follow that plugin pair's explicit migration guide rather than
changing a provider setting.

**Source action**: `none`; existing scene/prefab/component records are unchanged. A future cross-
plugin conversion must use new stable IDs and a validated reversible patch rather than rebinding an
existing component ID.

**Cache action**: `keep`; no current import or GPU cache format changes result from the decision
correction.

**Compatibility window**: none (pre-1.0). The abandoned portable physics/audio and fixed render-
cache contracts had no implementation to preserve.

## RPR-U3: Ordinary Rust Product Recipes

`ProductRecipe` is the current ordinary Rust composition path for a product. It holds only
replayable plugin factories and typed canonical configuration. A schema-owning extension exposes
one `SchemaContribution`, which binds its plugin and the exact schema-provider definitions at the
extension boundary. The direct `App` path remains available for raw one-shot plugin values.

**User action**: move normal file-backed headless, desktop, and editor setup to one recipe. Use
`add_plugin` or `add_configured_plugin` for runtime-only entries, and let a schema-owning crate
return `SchemaContribution` rather than accepting a separate provider list. Use
`HeadlessRun::from_recipe`, `DesktopRun::from_recipe`, or
`EditorProjectIntent::new().with_recipe(...)` for product Hosts. Keep `PluginDefinition`, raw slot
edits, candidates, and lifecycle control to explicit advanced embedding code.

**Source action**: no project, scene, prefab, asset, or save format changes. Recipe configuration
is compiled Rust data, not a new `nara.toml` extension or package wire format.

**Cache action**: keep; rebuild Rust artifacts after changing recipe code.

**Compatibility window**: none for the ordinary API. Raw project-plan inspection and profile-owned
slot edits remain available only under `project_host::advanced`; they are not accepted by ordinary
Headless, Desktop, or Editor intents.

**Verification anchors**: `tests/plugin_composition.rs`, `tests/project_runtime_boot.rs`,
`tests/workspace_play_runtime.rs`, and the public-prelude fixture prove pure replacement,
direct/file-backed schema parity, headless and Editor execution, and the ordinary public compile
surface.

## PCC-U3: ProductRecipe-Only Ordinary Host Composition

Ordinary `HeadlessRunIntent`, `DesktopRunIntent`, and `EditorProjectIntent` no longer expose
`configure`, `disable`, `insert_before`, `insert_after`, or `with_schema_provider`. Those methods
formed a second composition authority beside `ProductRecipe` and allowed plugin selection to drift
from schema authority.

**Canonical replacement or deletion rationale**:

- Add replayable runtime-only plugins with `ProductRecipe::add_plugin` or
  `ProductRecipe::add_configured_plugin`.
- Add schema-owning plugins only through `SchemaContribution` and
  `ProductRecipe::add_contribution`; there is no parallel provider vector.
- Keep Image, Tilemap, window, WGPU, and other profile-owned built-ins in project/profile
  configuration. A recipe cannot disable, reorder, or replace entries it does not own.
- Use `project_host::advanced::{RuntimeProbe, headless_run_with_probe,
  desktop_run_with_probe, open_editor_with_probe}` only for one non-replayable probe definition.
  The probe cannot attach schema providers, and schema-owning definitions reject before an `App`
  exists.
- Keep raw one-shot plugins on direct `App::add_plugins`; the direct-App path does not acquire the
  file-backed Host lifecycle or publication guarantees.

**Before**:

```rust
let intent = HeadlessRunIntent::<GameSnapshot>::new(fixed_ticks)
    .configure(game_plugin_definition())
    .insert_after::<GameplayCommandPlugin>(fault_probe_definition())
    .with_schema_provider(game_schema_provider());
```

**After, ordinary replayable product**:

```rust
let contribution = SchemaContribution::<GamePlugin>::for_default([game_schema_provider()])?;
let recipe = ProductRecipe::new().add_contribution(contribution)?;
let mut run = HeadlessRun::from_recipe(project_root, recipe, fixed_ticks, commands);
```

**After, explicit non-replayable Host probe**:

```rust
let intent = HeadlessRunIntent::<GameSnapshot>::new(fixed_ticks).with_recipe(recipe);
let probe = RuntimeProbe::new(fault_probe_definition());
let mut run = headless_run_with_probe(project_root, intent, probe, commands);
```

**Source action**: none. This is a Rust composition API correction; project, Scene, Prefab, asset,
and save formats are unchanged.

**Cache action**: keep content caches; rebuild Rust artifacts and plugin-plan fingerprints.

**Compatibility window**: none (pre-1.0 fearless replacement). No deprecated ordinary intent
methods or provider-list wrappers remain.

**Verification anchors**: `tests/project_host_boundary.rs`, `tests/plugin_composition.rs`,
`tests/project_runtime_boot.rs`, `tests/workspace_play_runtime.rs`,
`tests/runtime_runner_contract.rs`, `reference-game/tests/plugin_composition.rs`, and
`reference-game/tests/host_parity.rs`.

## RGS-U2-1: Runtime Hierarchy Ownership and Scene Component Separation

**Removed contract**:

- `nara_scene::{Parent, Children, spawn_child, sync_children}` as the runtime hierarchy authority.
- `nara_scene::{HierarchyPlugin, HIERARCHY_PLUGIN_ID, HIERARCHY_PLUGIN_DECLARATION}` as the plugin
  that combined runtime topology with persistent scene components.
- Direct mutation of the derived `Children` collection through Nara's ordinary API.
- Inferring persistent scene parent facts from runtime `Parent` components during `export_scene`.
- Prepared Scene identity transaction ports through the ordinary `nara::identity` facade.
- Startup publication of parented `Transform2d` or inherited `Visibility` before their runtime
  projections are implemented.

**Canonical replacement or deletion rationale**: `nara_hierarchy` now owns immutable non-linked
parent/ordered-children components, bounded whole-topology validation, and ordered failure-atomic
mutation. Runtime
structure does not own entity lifetime, persistence, transform math, visibility, UI layout, or
prefab provenance. `nara_scene::SceneComponentsPlugin` separately owns the persistent `Name` and
`Visibility` schemas. The split removes the dependency cycle that prevented `nara_transform` from
consuming hierarchy while retaining one authoritative relationship projection. Exact Scene
identity preparation remains a workspace-internal composition port between `nara_scene` and
`nara_identity`; ordinary callers use `SceneSpawner` and `SceneAuthoringSession` rather than
committing detached identity transactions.

**Before**:

```rust
use nara_scene::{HierarchyPlugin, Parent, spawn_child};

app.add_plugin(HierarchyPlugin)?;
spawn_child(app.world_mut()?, child, parent);
let parent = world.get::<Parent>(child);
```

**After**:

```rust
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
    HierarchyWriter, Parent,
};
use nara_scene::SceneComponentsPlugin;

app.add_plugin(HierarchyPlugin)?;
app.add_plugin(SceneComponentsPlugin)?;
HierarchyWriter::new(app.world_mut()?).apply_batch(&[
    HierarchyOperation::Place {
        entity: parent,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    },
    HierarchyOperation::Place {
        entity: child,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
    },
])?;
app.world_mut()?.flush();
let parent = app.world()?.get::<Parent>(child);
```

The former plugin ID `nara.scene.hierarchy` is removed. Runtime hierarchy uses `nara.hierarchy`;
the persistent scene-component plugin uses `nara.scene.components`. The existing schema provider,
owner, and native binding identities under `nara.scene.hierarchy.components*` intentionally remain
stable because the persistent `Name` and `Visibility` records did not change identity or format.

**Affected examples and fixtures**: root product bundles, scene spawn/replace/unload, UI hierarchy
queries, runtime UI examples, the reference game, plugin-composition tests, and public-prelude
compile fixtures now use the new ownership boundary.

**User action**: add `nara_hierarchy` where runtime structure is required, replace removed scene
hierarchy imports with `nara_hierarchy` imports, and install both plugins when persistent scene
components and runtime hierarchy are needed. Do not schedule ordinary systems against the internal
hierarchy completion set; its public placement is not yet a compatibility promise. Replace any use
of root-facade prepared identity internals with the corresponding Scene spawn, replacement, or
authoring operation. Keep persistent parent facts in `SceneDocument` or `ScenePatchDocument`;
`export_scene` now rejects runtime hierarchy instead of guessing document ownership from it.

**Source action**: `none` for canonical scene/prefab files; stable scene entity IDs, `parent` fields,
component type IDs, schema versions, and canonical bytes are unchanged. Runtime-only export callers
that previously inferred parents must move those facts into their owning document or patch. RGS-U2
initially rejected parented `Transform2d` and inherited `Visibility`; RGS-U3 admits the former after
completing global projection, while inherited visibility remains unavailable.

**Cache action**: `keep`; rebuild Rust artifacts and runtime plugin-plan fingerprints after the
plugin identity split.

**Compatibility window**: none (pre-1.0 fearless replacement). No aliases for the removed
`nara_scene` hierarchy symbols or old plugin ID remain.

**Rollback**: revert the RGS-U2 implementation and all migrated consumers together. Do not restore
the frame-wide `Children` rebuild or accept parented spatial content without complete propagation.

**Verification anchors**: `crates/nara_hierarchy/src/tests.rs`, `crates/nara_scene/src/tests.rs`,
`tests/plugin_composition.rs`, `tests/product_capabilities.rs`,
`tests/project_content_boundary.rs`, and the public-prelude compile fixtures prove topology
validation, atomic scene replacement/unload, plugin-plan composition, fail-closed spatial
admission, removed ordinary mutation paths, and the absence of prepared identity transactions from
the root facade.

## RGS-U3-1: Completed Global 2D Projection and Render Consumption

**Superseded representation (LLC-U4, 2026-10-23)**: the earlier derived ECS Component and
completion-token representation is removed. The example here describes the canonical replacement.
`GlobalTransform2d` is now a copyable opaque affine value returned by a borrowed phase snapshot.

**Removed contract**:

- Public tuple construction and field access through `GlobalTransform2d(pub Mat3)`.
- Arbitrary `Default` construction of `GlobalTransform2d` by ordinary callers.
- Sprite and tilemap extraction from `Option<&Transform2d>` with implicit identity fallback.
- Camera extraction from an optional local transform with implicit origin fallback.
- The temporary rejection of parented `Transform2d` content after Scene/Prefab preflight.

**Canonical replacement or deletion rationale**: `Transform2d` remains authored mutable local
state. `GlobalTransform2d` is an immutable, opaque affine value returned from the privately owned
phase cut after hierarchy/transform completion. World-space camera, sprite, and
tilemap consumers require both explicit local participation and the completed global projection;
they do not walk parent chains or invent local/identity fallbacks. This creates one freshness and
failure boundary across Direct App, managed runtimes, Project Host, paused Editor Play, and render
extraction.

**Before**:

```rust
let global = GlobalTransform2d::default();
let matrix = global.0;
let extracted = extract_sprite(entity, &sprite, Some(&local), 0);
```

**After**:

```rust
let spatial = nara::transform::presented_spatial_snapshot(&world)?;
let global = spatial.global(entity)?;
let matrix = global.matrix();
let extracted = extract_sprite(entity, &sprite, matrix, 0);
```

Ordinary systems use `HistoricalSpatialSnapshot2d` during mutation, `FixedSpatialSnapshot2d`
after fixed propagation, and `PresentedSpatialSnapshot2d` after presentation. Matching shared-World
functions serve exclusive readers. Fixed Resolve uses `add_fixed_transform_projection_systems`;
Finalize/Capture reads Fixed directly without that helper. Extract readers use
`add_presented_transform_projection_systems`. Readers validate the owner, role and time; copied
stamps do not grant authority. Later edits or retirement leave the borrowed cut unchanged until
the next publication. A non-spatial direct parent ends spatial inheritance.

**Affected examples and fixtures**: camera setup, runtime UI fixtures, image/render tests, root
Project Content and Editor Play tests, the reference game's desktop projection, and all sprite/
tilemap extraction callers now install the hierarchy/transform plugins and provide explicit local
transforms.

**User action**: remove direct construction, mutation, defaulting, ECS insertion/query, and
completion-token joins for `GlobalTransform2d`. Keep authored state in `Transform2d`, read the
completed affine value from the appropriate phase snapshot,
and call `.matrix()` or `.translation()` for read access. Add an explicit identity `Transform2d` to
world-space cameras, sprites, and tilemaps that previously relied on absence. Update direct calls to
`extract_sprite` and `extract_tile_cell` to pass the captured matrix.

**Source action**: `none`; canonical Scene/Prefab parent and Transform records retain their format
and stable identities. Parented `Transform2d` now loads; inherited `Visibility` remains rejected
until its own product slice.

**Cache action**: `keep` for content/import caches; rebuild Rust artifacts and product-plan
fingerprints after the plugin dependency and public API change.

**Compatibility window**: none (pre-1.0 fearless replacement). No public constructor, mutable
field, or local/identity extraction fallback remains.

**Rollback**: revert `fb4fdc7` and its migrated consumers together, then restore the parented-
Transform admission rejection. Do not retain parented content while reverting propagation or any
world-space consumer.

**Verification anchors**: `crates/nara_transform/src/lib.rs`,
`crates/nara_transform/src/propagation.rs`, `crates/nara_render/src/lib.rs`,
`crates/nara_sprite_render/src/tests.rs`, `tests/project_content_boundary.rs`,
`tests/workspace_play_runtime.rs`, `reference-game/tests/desktop_render.rs`, and
`docs/knowledge/engineering/verification/2026-08/2026-08-02T084559Z-rgs-u3-completed-2d-global-transform-projection-1fa10428b5e240acbade0440cc25ae75.md`.
The historical record qualifies its original Component representation; LLC-U4's fresh evidence
qualifies its replacement, including same-change-tick edits and World-bound publication association.

## SRT-U2-1: Exact Image Revision and Snapshot-Owned Invalidation

**Removed contract**:

- Infallible `ImageAsset::new` construction that accepted zero extents or mismatched RGBA bytes.
- `RenderResourceSnapshot::new(key, version, descriptor_hash)` and the snapshot's `Copy`-by-value
  accessors.
- `PreparedRenderResources::invalidate_if_snapshot_changed`, removal with an invalidation reason,
  the unused asynchronous `RenderPrepareApplyResult` arbitration, and the public
  `RenderPrepareInvalidation`, `RenderPrepareInvalidationReason`, and
  `RenderPrepareInvalidations` event-log types.
- Exhaustive matches that assumed the previous closed sets of `RenderFrameSkipReason` or
  `WgpuRenderError` variants.

**Canonical replacement or deletion rationale**: `ImageAsset::new` now validates one exact,
non-zero RGBA allocation and returns `Result`. Each backend-neutral prepare snapshot includes the
existing opaque `AssetSlotRevision`, so direct `Assets::insert` or mutable access invalidates cached
content even when loader-owned version and metadata stay unchanged. Snapshot comparison, prepared
record replacement, and backend frame-age eviction already own cache invalidation; retaining a
second unbounded event log created duplicate authority and had no production consumer. A slot
change between Prepare and WGPU submission skips that frame as `ResourceChanged` without poisoning
the backend; the next Prepare observes the new revision.

**Before**:

```rust
let image = ImageAsset::new(source, extent, format, color_space, pixels);
let snapshot = RenderResourceSnapshot::new(key, asset_version, descriptor_hash);
prepared.remove(key, &mut invalidations, RenderPrepareInvalidationReason::AssetRemoved);
```

**After**:

```rust
let image = ImageAsset::new(source, extent, format, color_space, pixels)?;
let snapshot = RenderResourceSnapshot::new(
    key,
    asset_version,
    images.slot_revision(handle),
    descriptor_hash,
);
let removed = prepared.remove(key);
```

Callers that inspect snapshots now borrow them through `record.snapshot()` and pass
`&RenderResourceSnapshot` to `needs_prepare`. There is no replacement invalidation event stream;
observe the current prepared record or backend cache statistics when product telemetry is required.
Exhaustive enum matches must handle `RenderFrameSkipReason::ResourceChanged` and
`WgpuRenderError::ResourceChangedAfterPrepare`; both identify a retryable frame-local asset race,
not a device or backend failure.

**Affected examples and fixtures**: image import and serde tests, sprite/UI test images, prepared
render-resource tests, WGPU texture-cache tests, and the Project Host retained-image publication
fixture use the fallible constructor and exact slot revision.

**User action**: propagate or explicitly handle `ImageAssetCreateError`, obtain the slot revision
from the same `Assets<ImageAsset>` store whose value is being prepared, update snapshot access to
borrowed values, and delete invalidation-event consumers. Do not replace the removed log with a
parallel queue or hash complete pixels every frame.

**Source action**: `none`; serialized image bytes and project source formats are unchanged, while
serde input now rejects values that were never valid RGBA images.

**Cache action**: `keep`; rebuild Rust artifacts and restart runtimes so ephemeral prepared/GPU
caches are reconstructed with slot revisions.

**Compatibility window**: none (pre-1.0 fearless replacement). No deprecated constructor,
snapshot overload, or invalidation aliases remain.

**Rollback**: revert the image, prepare, WGPU, and migrated caller changes together. Do not restore
direct asset replacement without an exact content revision, and do not restore the invalidation log
without a bounded production consumer.

**Verification anchors**: `crates/nara_asset/src/storage.rs#tests`,
`crates/nara_render/src/prepare.rs#tests`, `crates/nara_image/src/tests.rs`,
`crates/nara_image/tests/image_import_limits.rs`, `crates/nara_render_wgpu/src/texture.rs#tests`,
`crates/nara_sprite_render/src/tests.rs`, `crates/nara_ui_render/src/tests.rs`, and
`src/project_host/runtime/tests.rs`.

## SRT-U3-1: Bounded Startup Activation and Classified Runtime Faults

**Changed contract**:

- File-backed Project Profiles now install the provisional startup-scene activation plugin. Direct
  managed-App integrations must install `StartupSceneActivationPlugin` explicitly before sealing.
- Project Host materialization consumes one move-only `StartupSceneSource`, spawns that exact
  document, and creates the matching `SpawnedSceneInstance` internally. The resulting
  `StartupSceneActivation` is a read-only advanced `SystemParam`; an unnameable root resource owns
  the source, receipt, and retention guard for the candidate and runtime lifetime.
- The materializer derives the frozen component registry from the candidate `World`; callers can no
  longer provide a separate registry. Startup finalization permanently closes the materialization
  window, so a published Direct App cannot inject a late startup scene.
- `AppRunError::DirectRuntime` and `AppRunError::ManagedRuntime` now include an optional validated
  `detail` field. Exhaustive matches must use `..` or match that field.
- Engine-owned fallible systems may return `RuntimeFaultDetail::into_bevy_error()`; the private
  carrier forces non-panicking Bevy severity, while unclassified third-party errors continue to
  expose only generic fault kind and source.

**Canonical usage**:

```rust
use nara::advanced_prelude::{
    StartupSceneActivation, StartupSceneActivationPlugin, StartupSceneActivationSet,
};

// Install the plugin during direct managed-App composition. Product initialization systems run in
// StartupSceneActivationSet and borrow StartupSceneActivation read-only.
```

Project-backed callers do not construct a source or activation. Bundled content shares the exact
snapshot `Arc` and lease; Editor Play receives a separately charged retained source. Direct
managed-App callers use `StartupSceneSource::direct(document, retained_byte_limit)` and pass it to
`materialize_startup_scene`; source/receipt pairs cannot be assembled independently. The direct
limit uses Nara's deterministic logical retained-byte model, not allocator capacity or RSS. Direct
materialization runs only inside the sealed candidate's admission scope, after the component
registry authority exists and before `complete_startup`.

**User action**: update exhaustive `AppRunError` patterns, add the activation plugin only to custom
direct managed-App compositions that call the advanced materializer, and place product Startup
initialization in `StartupSceneActivationSet`. Do not move startup materialization into
`ProductRecipe`, reconstruct the source from the live `World`, or retain arbitrary third-party
error text.

**Source action**: none. Scene, Prefab, AssetMeta, and project manifest formats are unchanged.

**Cache action**: keep content/import caches; rebuild Rust artifacts and product-plan fingerprints
because the Project Profile plugin composition changed.

**Compatibility window**: none (pre-1.0 fearless replacement). Startup activation remains a
provisional advanced contract and is not re-exported from the ordinary prelude.

**Verification anchors**: `src/startup_scene.rs`, `src/project_content.rs`,
`src/project_host/runtime.rs`, `crates/nara_app/src/runtime/fault_route.rs`,
`src/project_host/runtime/tests.rs`, `tests/startup_scene_public_surface.rs`, and
`tests/workspace_play_runtime.rs`.

## SRT-U4-1: Prepared Atomic Product Scene Replacement

**Changed contract**:

- `LifecycleFreeInsertionPlan::commit(&mut World)` is replaced by
  `LifecycleFreeInsertionPlan::prepare(&mut World)?.commit()`. Preparation flushes the existing
  deferred baseline, resolves only already registered component IDs, and rejects required
  components, hooks, observers, missing targets, duplicates, and existing components before any
  insertion.
- `nara_hierarchy::validate_hierarchy` remains an immutable `&World` validator. Its filtered query
  state is created with `QueryState::try_new`, so validation neither visits unrelated entities nor
  registers missing hierarchy component types.
- The provisional advanced `replace_scene_with_product` path accepts exact additional retirement
  authority as `&[WorldEntityToken]`. Bare `Entity` values cannot authorize product-owned
  retirement across World identity domains.
- `SceneProductTransactionLimits::MAX_OVERLAY_WRITES` and
  `MAX_ADDITIONAL_RETIREMENTS` are the documented engine ceilings above caller-supplied limits.

**Canonical replacement or deletion rationale**: the former insertion call combined validation
and mutation behind a fallible commit. Scene replacement needs every recoverable check complete
before hierarchy, identity, resource, or membership authority changes. The prepared guard owns the
exclusive World borrow through insertion commit. The focused product path similarly accepts only
stable scene IDs for candidate overlay writes and identity-bound tokens for exact extra retirement;
it does not expose a candidate World, runtime entity map, provider registry, or general scene
session.

**Before**:

```rust
insertion_plan.commit(&mut world)?;
```

**After**:

```rust
let world = insertion_plan.prepare(&mut world)?.commit();
```

Advanced product replacement callers obtain `WorldEntityToken` values from the active
`WorldIdentityDomain`, supply explicit overlay and retirement limits, and target candidate
components only by `SceneEntityId`. Do not reconstruct token authority from `Entity` bits.

**User action**: split lifecycle-free insertion into prepare and commit, and replace any provisional
bare-entity extra retirement list with identity-domain tokens. Continue calling complete hierarchy
validation through immutable World access. Handle limit and preparation rejection before treating
the old scene generation as changed.

**Source action**: none. Scene, Prefab, component, and project manifest formats are unchanged.

**Cache action**: keep; rebuild Rust artifacts because public Rust signatures changed.

**Compatibility window**: none (pre-1.0 fearless replacement). No fallible insertion-commit shim
or bare-entity retirement overload remains.

**Rollback**: revert the ECS insertion guard, identity-token preflight, hierarchy query validation,
and scene product transaction together. Do not restore a recoverable branch after the first scene
authority mutation.

**Verification anchors**: `crates/nara_ecs/src/transaction.rs`,
`crates/nara_identity/src/domain.rs#validate_additional_retirement_identity_axes`,
`crates/nara_hierarchy/src/validation.rs#validate_hierarchy`,
`crates/nara_reflect/src/persistent_apply.rs#PreparedComponentApplyBatch`,
`crates/nara_scene/src/product_transaction.rs`,
`crates/nara_scene/src/spawn.rs#replace_scene_with_product`, and
`crates/nara_scene/src/product_transaction_tests.rs`.

## SRT-U5: Canonical Reference-Game Runtime Facts And Retry Ownership

**Changed contract**:

- Empty persistent marker components are valid scene schemas and encode as an empty
  `ComponentValue::Map`. `ComponentRegistryError::MissingSceneComponentFields` is removed; callers
  with exhaustive matches must remove that branch.
- The reference game replaces the persistent `Player`, `Enemy`, and `Projectile` aggregate
  components with authored `PlayerRole`, `EnemyRole`, `InitialHealth`, `InitialVelocity2d`,
  `WaveSpawn`, and `Weapon` facts. Mutable health, velocity, cooldown, projectile damage, and
  projectile lifetime are runtime-only components. `Transform2d` is the only authored position.
- Reference-game schema generation 4 tombstones the three removed aggregate component type IDs.
  `Weapon` advances from version 1 to version 2 and removes the persisted `remaining-ticks` field;
  the retained migration drops that field before decoding the authored configuration.
- `StartupSceneActivation::source_view` returns a cloneable `StartupSceneSourceView` with a weak
  document reference. The private root activation owner remains the sole document and Project
  Content lease owner. Product-local Retry can borrow the admitted document while the runtime is
  alive but cannot extend its retention after retirement.
- `STARTUP_SCENE_ACTIVATION_PLUGIN_ID` is available from the advanced surface so a product plugin
  can declare its mandatory startup dependency during pure plugin planning.

**Canonical replacement or deletion rationale**: persisted aggregate gameplay objects duplicated
spatial and mutable runtime authority, made Retry reconstruct state by hand, and allowed unrelated
same-shaped entities to enter gameplay queries. The replacement keeps authored configuration in
the scene, initializes mutable run state once, owns projectiles by exact world identity tokens, and
accepts only entities in the current scene receipt's exact stable-ID-to-entity mapping. Retry uses
the same retained document and prepared scene product transaction as Startup. A weak source view is
necessary because a public clone carrying the retention lease would let product code keep Project
Content budget charged after runtime retirement.

**User action**: regenerate reference-game content against schema generation 4, remove exhaustive
matches for `MissingSceneComponentFields`, and update advanced product plugins that consume startup
activation to declare `STARTUP_SCENE_ACTIVATION_PLUGIN_ID`. Persistent marker types may now derive
`PersistentComponent` without inventing a placeholder field. Do not persist mutable run state or
retain a startup document outside `StartupSceneSourceView::with_document`.

**Source action**: reference-game scene and prefab sources must use the generation-4 component IDs.
The removed aggregate component records are intentionally rejected by the current executable
registry rather than silently reinterpreted. Existing generation-1 `Weapon` values migrate through
the retained version-1-to-version-2 migration.

**Cache action**: regenerate derived schema catalogs and rebuild Rust artifacts. Project Content and
image import caches may be retained after the source files are updated.

**Compatibility window**: none (pre-1.0 fearless replacement). The removed aggregate Rust types,
old scene records, and removed error variant have no compatibility shims. Startup source access
remains provisional and advanced-only.

**Verification anchors**: `crates/nara_reflect/src/registry.rs`,
`crates/nara_reflect_derive/src/lib.rs`, `crates/nara_scene/src/spawn.rs`,
`src/startup_scene.rs`, `reference-game/schema/component-schema-v4.json`,
`reference-game/src/components.rs`, `reference-game/src/systems.rs`,
`reference-game/tests/authoring.rs`, `reference-game/tests/first_wave.rs`, and
`reference-game/tests/desktop_render.rs`.

## NUI-U3: Concrete Domain-Owned Asset Import Execution

**Removed contract**:

- `Importer`, `TypedImporter<T>`, `ImporterRegistry`, `ImporterDescriptor`, `ImportRequest`,
  `ImportJobInput`, generic `ImportedAsset<T>`, their selection/registration errors, and
  `SourceExtension` plus `ImportLabelKind::SourceExtension`.
- The `ImporterRegistry` ECS resource and Image plugin registration step.
- `ImageImporter`'s generic trait implementation and descriptor-shaped construction failure.
- `ImageImportError::Selection`; a missing suffix is now the concrete
  `MissingSourceExtension` error. Its stable diagnostic code changes from
  `image.import-selection-failed` to `image.import-source-extension-missing`, and its safe summary
  changes from `Image importer selection failed` to `Image source extension is missing`.

**Canonical replacement or deletion rationale**: no production Host executed the registry. Image
jobs always consumed `ImageImporter` directly and already owned source authority, budgets, decode,
task ordering, generation/version checks, and typed publication. `nara_asset` therefore retains the
shared stable source/artifact identity and reload primitives, while `nara_image` privately creates
the exact `ImportArtifactKey` for its decoded product. No provider, registry, or compatibility
facade replaces the deleted surface. A shared executor may be reconsidered only after a second
complete asset domain demonstrates concrete duplicated policy.

**Before**:

```rust
let mut registry = ImporterRegistry::new();
registry.register(ImageImporter::default())?;
let artifact = registry.import(ImportRequest::new(
    &record,
    png_bytes,
    dependency_digest,
    settings_hash,
    profile,
))?;
```

**After**:

```rust
let imported = ImageImporter::default().import_image(
    ImageBytesImportRequest::new(
        record,
        png_bytes,
        ImageImportSettings::new(ImageColorSpace::Srgb),
    ),
    handle,
    expected_version,
    &asset_server,
    &images,
    &states,
)?;
```

**User action**: call the concrete Image byte/file APIs and carry `ImageImportedAsset` through its
typed commit path. Remove generic importer registration, dispatch, descriptor inspection, and
exhaustive matches for the deleted errors. Match `ImageImportError::MissingSourceExtension` or
`UnsupportedFormat { extension }` for Image admission diagnostics, remove exhaustive
`ImportLabelKind::SourceExtension` arms, and update telemetry/alert classification to the new
missing-extension diagnostic code.

**Source action**: none. Asset metadata, Scene/Prefab records, and image sources are unchanged.

**Cache action**: keep. The Image importer ID, importer version 2, output type, artifact format
version, profile, settings hash, dependency digest, source hash, and artifact label are unchanged,
so existing version-2 Image artifact keys remain exact.

**Compatibility window**: none (pre-1.0 fearless deletion). No deprecated aliases, empty registry,
or adapter traits remain.

**Rollback**: revert the complete NUI-U3 boundary change. Do not restore registration metadata
without a Host that owns execution, authority, budgets, errors, tasks, and publication end to end.

**Verification anchors**: `crates/nara_asset/src/artifact.rs`,
`crates/nara_image/src/import.rs`, `crates/nara_image/src/reload.rs`,
`crates/nara_image/src/tests.rs`, `crates/nara_image/tests/image_import_limits.rs`,
`tests/image_import_limits.rs`, `reference-game/tests/image_asset_safety.rs`, and the root Image
examples prove concrete execution, exact artifact identity, bounded failure, stale-result
rejection, and last-good publication.

## NUI-U4: Closed First-Party Render And UI Internals

**Removed contract**:

- `RenderPhaseLabel`, `RenderPhaseInput`, `RenderPassNodeId`, `RenderPassStepLabel`,
  `RenderPassStep`, `RenderPassPlan`, `RenderPassDependency`, `RenderPassDependencyError`,
  `build_render_pass_plan`, and `render_phase_order`.
- The phase-input argument to `build_render_frame_packet` and
  `RenderFramePacket::pass_plan`.
- Root and advanced-prelude exports for render topology, sprite extraction/queue/batch DTOs, and
  UI extraction/queue/batch DTOs.
- Public `nara_ui_render` access to extract, queue, compare, sort, batch construction,
  material-key construction, mutable intermediate resources, and render statistics.
- The direct `bevy_ecs` dependencies of `nara_render`, `nara_sprite_render`, `nara_ui_render`, and
  `nara_render_wgpu`; these crates use the `nara_ecs` ownership boundary.

**Changed contract**:

- `ExtractedSprite::phase` is removed; `ExtractedSpriteKind` remains the source-domain fact.
  `QueuedSpriteItem::phase` and `SpriteBatch::phase` are replaced by the closed owner-local
  `SpriteBatchKind::{Tilemap, Sprite}` ordering fact. It cannot introduce a new renderer-wide
  phase.
- WGPU performs one fixed first-party draw sequence: clear through the render-pass load operation,
  then ordered sprite content (`Tilemap`, `Sprite`), then UI content, then presentation. It no
  longer interprets a runtime pass plan or scans buffers once per phase.
- `nara_ui_render` publicly exposes `UiRenderPlugin` as its product entry. Its doc-hidden
  `__private` module gives WGPU a temporary read-only `UiBatchReader -> UiBatchSnapshot` bridge
  until NUI-U14 replaces the incomplete live-resource handoff with the owned resource-complete UI
  packet required by ADR 0105. The internal `UiBatches` ECS resource is not exported, and the
  snapshot is neither constructible nor a `Resource`; this bridge is not a supported external
  extension surface.

**Canonical replacement or deletion rationale**: no production consumer created a distinct phase,
dependency graph, or UI pipeline schedule. The old pass plan deduplicated labels and sorted them,
but its dependencies did not control execution, its clear step was ignored, and frame packet
admission already rejected the multi-view shape used by its topology tests. The real backend order
is static and is now represented directly by owner-local batch order plus one WGPU draw traversal.
Technical UI stages remain owned by `UiRenderPlugin`; external products author UI and select a
presentation recipe instead of mutating half-built render state.

**User action**: remove custom phase labels, pass dependencies, pass-plan inspection, direct UI
stage calls, and imports of sprite/UI render DTOs from root preludes. Author cameras, sprites,
tilemaps, and UI through their product modules, install the applicable product recipe, and treat
render submission as engine-owned. Code that needs a real intermediate resource, history lifetime,
or cross-target dependency must supply the evidence required by OQ-001 before a general execution
model can be proposed.

**Source action**: none. Scene, Prefab, project, image, sprite, tilemap, and UI source formats are
unchanged by this unit.

**Cache action**: keep. Rebuild Rust artifacts because public Rust signatures and feature dependency
edges changed; imported assets and prepared image identities are unchanged.

**Compatibility window**: none (pre-1.0 fearless deletion). No deprecated phase label, pass-plan
adapter, empty dependency graph, public UI stage wrapper, or alternate prelude export remains.

**Rollback**: revert the topology deletion, sprite owner-local kind, UI surface closure, WGPU fixed
ordering, and facade cleanup together. Do not restore a public graph whose dependency edges do not
control execution.

**Verification anchors**: `crates/nara_render/src/lib.rs`,
`crates/nara_sprite_render/src/{types.rs,queue.rs}`, `crates/nara_ui_render/src/lib.rs`,
`crates/nara_render_wgpu/src/{lib.rs,quad.rs}`, `tests/product_capabilities.rs`, and the independently
locked `tests/fixtures/public-prelude` consumer prove topology removal, product authoring access,
fixed first-party ordering, private UI stages, and exact manifest ownership.

## NUI-U5A: Explicit 2D Content And Presentation Composition

**Removed contract**:

- `runtime-2d` compiling `nara_render` and `nara_sprite_render`, enabling image preparation, and
  weakly selecting the WGPU sprite submitter.
- `Runtime2dPlugins` installing `RenderPlugin`, `ImagePreparePlugin`, and `SpriteRenderPlugin`.
- `ImagePlugin` depending on `ImagePreparePlugin` for both plugin closure and initialization of
  `Assets<ImageAsset>`.
- Presented examples and desktop project profiles relying on `runtime-2d` to imply packet
  production.

**Canonical replacement or deletion rationale**: 2D content and simulation must remain usable by
the same headless product that owns Scene, gameplay, sprite, tilemap, image, and Retry facts.
`runtime-2d` now owns only that content closure. The explicit `presentation-2d` Cargo capability,
project capability, and `Presentation2dPlugins` group add backend-neutral render coordination,
image preparation, and sprite packet production. Windowing and WGPU remain separate adapters.
Manifest ingest rejects `presentation-2d` without an explicit `runtime-2d` request using
`project.product.incomplete-capability-closure`. Existing `ProductCapability` discriminants 0
through 5 remain stable; `Presentation2d` uses the new value 6.

Before:

```toml
[dependencies]
nara = { path = "..", default-features = false, features = ["runtime-2d"] }
```

```rust
app.add_plugins(Runtime2dPlugins)?;
```

After, for headless content:

```toml
[dependencies]
nara = { path = "..", default-features = false, features = ["runtime-2d"] }
```

```rust
app.add_plugins(Runtime2dPlugins)?;
```

After, for presented 2D content:

```toml
[dependencies]
nara = { path = "..", default-features = false, features = [
    "presentation-2d",
    "desktop-winit",
    "render-wgpu",
] }
```

```rust
app.add_plugins((
    Runtime2dPlugins,
    Presentation2dPlugins,
    DesktopWinitPlugins,
    WgpuBackendPlugins,
))?;
```

Presented project profiles request the complete product closure:

```toml
[capabilities]
requested = [
    "runtime-2d",
    "presentation-2d",
    "desktop-winit",
    "render-wgpu",
]
```

**Affected examples and fixtures**: `windowed_sprites`, `asset_import_texture`,
`sprite_tilemap_scene`, `scene_prefab_roundtrip`, and `component_schema_export` select
`presentation-2d`; `hello_world` remains a presentation-free `runtime-2d` consumer. The reference
game base dependency and headless profile remain content-only, while its desktop feature/profile
selects 2D presentation and adapters explicitly. The independently locked
`tests/fixtures/product-slices/presentation-2d` workspace proves the public group and physical
dependency boundary.

**User action**: presented 2D products add the `presentation-2d` Cargo feature, import
`Presentation2dPlugins`, install it alongside `Runtime2dPlugins`, and add `presentation-2d` to each
presented `nara.toml` profile. Headless products keep `runtime-2d` only. Code using image import
without presentation no longer installs `RenderPlugin` or `ImagePreparePlugin` merely to satisfy
`ImagePlugin`.

**Source action**: `manual-rewrite` only for presented `nara.toml` capability lists. Scene, Prefab,
Patch, image, sprite, tilemap, and UI source formats are unchanged.

**Cache action**: rebuild Rust artifacts and regenerate affected Cargo lockfiles. Imported assets
and prepared image identities are unchanged.

**Compatibility window**: none (pre-1.0 fearless replacement). No alias preserves the old
implicit meaning of `runtime-2d` or `Runtime2dPlugins`.

**Rollback**: revert `2f0bd69` together with its root, reference-game, public-prelude,
runtime-runner, and presentation fixture lockfiles. Do not add an alias or silently reattach
presentation to the content feature.

**Verification anchors**: `Cargo.toml`, `src/{lib.rs,product.rs}`,
`crates/nara_image/{Cargo.toml,src/lib.rs,src/reload.rs}`,
`crates/nara_project/src/{capability.rs,tests.rs}`, `tests/{product_capabilities,
project_composition,runtime_runner_contract,reference_game_contract}.rs`,
`tests/fixtures/product-slices/presentation-2d`, `reference-game/tests/{plugin_composition,
project_manifest_ingest,asset_task_flow}.rs`, and `.github/workflows/ci.yml` prove the content-only
tree, explicit presentation closure, exact desktop composition, stable capability bits, and
independent locked consumer.

This is the first dependency-ordered NUI-U5 checkpoint, not final UI closure. `runtime-ui` still
contains render and UI-render owners. NUI-U5B runs after NUI-U13 and must split UI presentation and
make Host adapter selection fully semantic before ADR 0106 can be implemented.

## NUI-U6-U7-1: Canonical Ordered Documents And Exact Placement Patches

**Removed contract**: Scene and Prefab records no longer serialize a record-owned `parent` field or
derive root/sibling order from record order. `ScenePatchOperation::AddEntity { entity }`,
`RemoveEntity { entity }`, and `Reparent { entity, parent }` are removed. The old version-1
Scene/Prefab object shape is rejected rather than interpreted as an unordered approximation.

**Canonical replacement or deletion rationale**: `SceneDocument` and `PrefabDocument` own an
explicit ordered `roots` sequence, while each parent record owns its ordered `children` sequence.
Entity records remain canonically sorted by stable ID independently of placement. Patch add,
remove, restore, and move operations use one `ScenePlacement` vocabulary with `Root` or `Parent`
ownership and `First`, `Last`, `Before`, or `After` anchors. `MoveEntity { from, to }` makes stale
source context an explicit failure; subtree deletion returns one `RestoreEntity` inverse carrying
the exact records and prior placement. Nara is pre-1.0, so retaining either ambiguous shape would
create two structural authorities without a migration contract.

**Before**:

```rust
let patch = ScenePatchDocument::new([ScenePatchOperation::Reparent {
    entity: child,
    parent: Some(parent),
}]);
```

**After**:

```rust
let from = ScenePlacement::current(&scene, &child).expect("validated placement");
let patch = ScenePatchDocument::new([ScenePatchOperation::MoveEntity {
    entity: child,
    from,
    to: ScenePlacement::last(ScenePlacementOwner::Parent { parent }),
}]);
```

Serialized Scene and Prefab sources must move hierarchy ownership out of each child's `parent`
field and into the document's ordered placement closure:

```json
{
  "format_version": 1,
  "roots": ["ui", "world"],
  "entities": [
    { "id": "ui", "children": ["ui/title", "ui/retry"], "components": {} },
    { "id": "ui/retry", "children": [], "components": {} },
    { "id": "ui/title", "children": [], "components": {} },
    { "id": "world", "children": [], "components": {} }
  ]
}
```

**Affected examples and fixtures**: all first-party Scene, Prefab, Patch, project-content, Host,
workspace, persistence, module-consumer, and reference-game sources were rewritten together.
`examples/scene_patch_roundtrip.rs` demonstrates the canonical Patch API; Scene/Prefab JSON and RON
fixtures now carry explicit placement while retaining stable-ID record sorting.

**User action**: manually rewrite every authored Scene, Prefab, and Scene Patch source. Preserve the
intended root and sibling sequence explicitly; do not infer it from entity-record order. Rewrite
custom authoring commands to capture the current `from` placement and provide a stable target
anchor.

**Source action**: `manual-rewrite`. There is no runtime or offline auto-migrator for the removed
draft shape.

**Cache action**: `keep` after the source rewrite. Rebuild Rust artifacts for the API change;
asset identities and imported payloads do not change.

**Compatibility window**: none (pre-1.0 fearless replacement). No serde alias, deprecated
operation, or unordered fallback preserves the removed contract.

**Rollback**: revert `53d6678` and `7c2c5ba` together, then restore the matching pre-U6 authored
sources. Do not revert only Patch or document ownership, and do not introduce a compatibility
reader that guesses order.

**Verification anchors**: `crates/nara_scene/src/tests.rs`,
`crates/nara_scene/tests/format_contract.rs`, `tests/scene_patch_transactions.rs`,
`tests/{scene_authoring_session,scene_inspector,editor_persistence,workspace_play_runtime}.rs`,
`src/project_host/runtime/tests.rs`, `module-consumer`, and `reference-game/tests/{authoring,
prefab_startup}.rs` prove canonical formats, bounded legacy rejection, candidate-relative anchors,
failure atomicity, exact undo/redo, Prefab override order, and save/reopen parity. Source searches
find no Rust use of `ScenePatchOperation::Reparent` or record-owned `parent`.

## NUI-U8-1: Ordered Runtime Hierarchy Projection

**Removed contract**: `HierarchyConstructionWriter`, `HierarchyConstructionEdge`, attachment-only
command helpers, and the public `Parent: Relationship` / `Children: RelationshipTarget` escape
hatch are removed. Parentless ECS entities are no longer implicit hierarchy roots, and callers
cannot derive root or sibling order from query iteration or entity allocation. Raw despawn is not
a structural mutation API.

**Canonical replacement or deletion rationale**: `HierarchyRoots` is the ordered read-only root
projection, `Children` remains the immutable ordered child projection, and `HierarchyOrder`
publishes one validated depth-first index per completed generation. `HierarchyWriter`
accepts `Place`, exact `Move { from, to }`, and exact `Remove { from }` operations using the same
root/parent plus first/last/before/after placement vocabulary as persistent documents. A batch
resolves sequentially against one candidate, validates the complete closure, and publishes only
after every operation succeeds. Scene spawn, Prefab expansion, export, replacement, Retry, exact
instance retirement, and individual Scene-entity retirement all use this owner.

**Before**:

```rust
HierarchyConstructionWriter::new(world).attach(child, parent)?;
```

**After**:

```rust
HierarchyWriter::new(world).apply_batch(&[
    HierarchyOperation::Place {
        entity: parent,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    },
    HierarchyOperation::Place {
        entity: child,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
    },
])?;
```

Every participating entity must have exactly one placement. Move or remove callers must capture
the current placement and provide it as `from`; stale context rejects the complete batch. When an
owning Scene subtree retires while external descendants survive, the hierarchy owner promotes the
surviving frontier to the retired root position in original depth-first structural order before
postorder retirement. Product replacement reuses the first retired root position for the first
new root and keeps remaining new roots adjacent in persistent order.

**Affected examples and fixtures**: transform, render, sprite-render, runtime UI panel, Scene
spawn/export/product transaction tests, and the reference-game HUD now register explicit ordered
roots and children. Scene orchestration delegates hierarchy lowering and retirement to
`crates/nara_scene/src/runtime_hierarchy.rs`.

**User action**: replace attachment-only calls with one ordered batch that registers both roots and
children. Use `HierarchyPlacement::current` before exact move/remove operations. Use Scene-owned
retirement APIs for Scene entities; do not mutate `Parent`, `Children`, or `HierarchyRoots`
directly and do not despawn structural participants before removing or promoting their placement.

**Source action**: `manual-rewrite`. No compatibility writer or entity-order fallback remains.

**Cache action**: `keep`. Rebuild Rust artifacts; persistent Scene/Prefab bytes and asset identity
do not change beyond the NUI-U6/U7 ordered document migration.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert `fc3a0b8` and `4b2146a` together with this migration entry. Do not restore only
the attachment writer or infer roots from missing parents, because either choice recreates a
second runtime ordering authority.

**Verification anchors**: `crates/nara_hierarchy/src/tests.rs`,
`crates/nara_identity/src/tests.rs`, `crates/nara_scene/src/{tests,product_transaction_tests}.rs`,
`crates/nara_transform/src/lib.rs`, `crates/nara_render/src/lib.rs`,
`crates/nara_sprite_render/src/tests.rs`, `examples/runtime_ui_panel.rs`, and
`reference-game/src/ui.rs` prove exact ordered placement, unrelated allocation independence,
deep/wide bounds, atomic rollback, export equality, deterministic replacement/retirement, and
explicit runtime-created HUD roots.

## NUI-U13-1: Renderer-Neutral Runtime UI Authoring Adapter

**Removed contract**: `UiPanel`, `UiPanelMaterial`, `ComputedUiLayouts`, `UiPointerRoute`, ECS-owned
hit/focus state, renderer-bearing `UiRoot` fields, `UiNode::with_sibling_order`, and the legacy
extract/queue/batch authoring path are removed. Old component maps that omit required version-1
fields or encode the retired root/panel shapes are rejected.

**Canonical replacement or deletion rationale**: `nara_ui` now owns only the ECS/Scene authoring
adapter. Persistent content uses `UiRoot`, `UiNode`, `UiVisual`, `UiText`, `UiButton`,
`UiSemantics`, and `UiTheme`; runtime-created stateful nodes add an explicit `UiRuntimeKey`.
Ordered `HierarchyRoots`/`Children` supply structure once, structured Scene/Prefab provenance
supplies persistent identity, and the adapter compiles immutable `UiFrame` values per explicit
`UiSurfaceDescriptor`. Presented-frame dispatch publishes bounded typed `UiActionEvent` values;
callers do not query computed ECS layout or pointer-route resources.

**Before**:

```rust
commands.spawn((
    UiRoot::window(0),
    UiPanel::new(UiPanelMaterial::solid(color)),
    UiNode::new(style).with_sibling_order(10),
));
let layouts = world.resource::<ComputedUiLayouts>();
```

**After**:

```rust
commands.spawn((
    UiRoot::new(UiLayer::Base),
    UiNode::new(style),
    UiVisual::solid(color),
    UiRuntimeKey::new("hud", "root")?,
));
let descriptor = UiSurfaceDescriptor::new(
    surface,
    InputWindowId::new(1),
    InputSeatId::PRIMARY,
    960.0,
    540.0,
    1.0,
    1,
    1,
    1,
)?;
install_ui_surface(world, descriptor)?;
let frame = latest_ui_frame(world, surface)?;
```

Scene/Prefab data must carry the same canonical version-1 components. Runtime code supplies only
typed product facts and consumes typed actions; it must not rebuild the persistent UI tree in Rust.

**User action**: replace panel/material pairs with `UiVisual`, replace renderer-specific root data
with a semantic root layer plus Host-installed surface descriptor, and replace all computed-layout
or pointer-route reads with immutable frame/semantic/action APIs. Add `UiRuntimeKey` only for
runtime-created roots or stateful nodes; persistent nodes derive identity from structured Scene
provenance.

**Source action**: `manual-rewrite`. There is no legacy codec, deprecated alias, defaulted shape,
or entity/query-order fallback.

**Cache action**: `delete` any derived UI frame/layout/batch snapshots and rebuild Rust artifacts.
Authored Scene/Prefab sources must be rewritten if they contain the removed component shapes.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert the complete NUI-U13 adapter and its Scene/Prefab source migration together.
Do not restore only `UiPanel`, a sibling rank, or a computed-layout resource because each would
reintroduce a second authoring, ordering, or frame authority.

**Verification anchors**: `crates/nara_ui/src/{authoring,codec,runtime,adapter_tests}.rs`,
`crates/nara_ui_kernel/tests`, `crates/nara_ui_widgets/tests`, `examples/runtime_ui_panel.rs`,
`reference-game/prefabs/reference-ui.prefab.json`, `reference-game/src/runtime_ui.rs`, and
`reference-game/tests/{first_wave,desktop_flow,desktop_render,prefab_startup}.rs` prove canonical
codecs, stable projected identity/order, headless frame/action behavior, data-authored product UI,
Retry generation replacement, and removal of the old public execution tree.

## NUI-U5B-1: Runtime UI Content And Presentation Composition

**Removed contract**: the `runtime-ui` feature no longer installs render ownership, image
preparation, UI packet compilation, or a WGPU-facing presentation bridge. A project capability
cannot imply both UI authoring and UI presentation through one bit.

**Canonical replacement or deletion rationale**: `runtime-ui` installs only the renderer-neutral
authoring adapter. The new `presentation-ui` feature and `PresentationUiPlugins` group depend on
`runtime-ui` and explicitly add render/image preparation, `nara_ui_render`, and the root-owned
surface-to-packet bridge. `ProductCapability::RuntimeUi` and
`ProductCapability::PresentationUi` are distinct; headless/server plans reject the latter.

**Before**:

```toml
[features]
desktop = ["nara/runtime-ui", "nara/render-wgpu"]
```

**After**:

```toml
[features]
headless = ["nara/runtime-ui"]
desktop = ["nara/presentation-ui", "nara/render-wgpu"]
```

```rust
app.add_plugins(RuntimeUiPlugins)?; // authoring/headless
app.add_plugins(PresentationUiPlugins)?; // explicit presentation product
```

**User action**: keep `runtime-ui` for content, headless simulation, and immutable frame/action
projection. Add `presentation-ui` only to desktop or other presentation products and select the
platform/backend adapter explicitly in the owning Host recipe.

**Source action**: `manual-rewrite`. The old feature meaning has no compatibility alias.

**Cache action**: `keep` content assets; regenerate Cargo lockfiles and product/plugin-plan
snapshots whose feature or capability closure changed.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert the feature, capability, product recipe, independent fixture, and Host-plan
changes together. Do not make `runtime-ui` presentation-bearing again.

**Verification anchors**: `Cargo.toml`, `src/product.rs`, `src/project_host/composition.rs`,
`crates/nara_project/src/{capability,tests}.rs`, `tests/{product_capabilities,
plugin_composition}.rs`, `tests/fixtures/product-slices/presentation-ui`, and
`reference-game/{Cargo.toml,nara.toml,src/lib.rs}` prove the physical dependency split, exact
capability closure, presentation-free headless recipe, and explicit desktop presentation.

## NUI-U14-1: Resource-Complete UI And Sprite Presentation Packets

**Removed contract**: the old UI extract/queue/types DTO surface, submit-time image lookup through
live `Assets<ImageAsset>`, public Sprite material image keys, and ambiguous `None` clipping are
removed. A backend may not repair a stale or incomplete packet by reading newer World or asset
state, and a fully clipped primitive may not degrade to an unclipped draw.

**Canonical replacement or deletion rationale**: `nara_ui_render` compiles one immutable
`UiPaintPacket` that owns exact frame identity, prepared image snapshots, bounded glyph-atlas page
snapshots, and ordered primitives. Sprite batches retain the prepared image snapshot as their only
image identity authority; `SpriteMaterialKey` contains renderer state only. WGPU resolves private
GPU residency from the captured snapshot and returns one closed outcome: `Presented`,
`ContentRejected`, `Deferred`, or `BackendFault`. A rejected candidate preserves the prior
last-good packet until a replacement is admitted or the Host retires its route.

**Before**:

```rust
let key = SpriteMaterialKey {
    image: Some(image_key),
    sampler,
    alpha_mode,
    tint,
};
// The backend could recover image bytes from live asset state.
```

**After**:

```rust
let key = SpriteMaterialKey {
    sampler,
    alpha_mode,
    tint,
};
let snapshot = batch.image.as_ref();
// The packet-owned snapshot is the only image identity and byte authority.
```

**User action**: remove `SpriteMaterialKey::image` reads and construction. Consumers that need
resource identity must read it from the retained `PreparedImageSnapshot`. Replace old UI
extract/queue/batch imports with the explicit advanced paint-packet integration API only when
implementing a Host or backend adapter; ordinary gameplay code should import `UiRenderPlugin`
only.

**Source action**: `manual-rewrite`. No compatibility alias preserves the old technical DTOs or
the duplicate Sprite image key.

**Cache action**: `delete` derived UI/Sprite packets and backend texture caches; keep source and
prepared content, then rebuild exact snapshots for the next admitted frame.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert the packet compiler, image preparation, Sprite batching, WGPU adapter, and
product composition changes together. Do not restore live asset reads, duplicate image identity,
or ambiguous clipping independently.

**Verification anchors**: `crates/nara_ui_render/src/{packet,atlas,compiler,integration,
packet_tests}.rs`, `crates/nara_sprite_render/src/{queue,types,tests}.rs`,
`crates/nara_render_wgpu/src/{backend,sprite,texture,ui}.rs`,
`reference-game/tests/desktop_render.rs`, and
`docs/knowledge/engineering/verification/2026-08/2026-08-06T052759Z-nui-u14-owned-ui-and-image-packet-through-wgpu-059e76de3be74d5894fe41135b3c7670.md`
prove exact packet ownership, last-good retention, closed outcomes, fail-closed clipping, and real
image/glyph submission.

## NUI-U17-1: Data-Owned Interaction Visuals And Surface Mapping

**Removed contract**: runtime UI no longer relies on a Rust-only `Accent`/`Positive`/
`Focus` button palette, assumes platform-logical pointer coordinates already equal authored design
coordinates, or treats source-only desktop submission as proof of packaged glyph residency.

**Canonical replacement or deletion rationale**: persistent UI authoring adds the version-1
`nara.ui.UiButtonVisuals` component. It stores theme roles for normal, hover, pressed, focused, and
disabled text/background/border states while `UiVisual` remains the authored normal background and
shape. The adapter composes focus borders with hover/pressed backgrounds instead of selecting one
mutually exclusive visual. Host-owned `UiSurfaceDescriptor::with_surface_mapping` records render
scale and the platform-logical-to-authored input scale; the presentation bridge derives both from
the physical window and OS scale factor. Gamepad and accessibility drivers enter the same bounded
input timeline as pointer and keyboard events. Presentation probes require a real resident glyph
texture and binding before reporting a submitted frame. This entry records the earlier public API
break; NUI-U17-2 closes the exact presented-session and pause-safe action-delivery contracts.

**Before**:

```rust
let visual = UiVisual::role(UiColorRole::Accent);
// Hover, press, focus, and coordinate scaling were selected outside authored product data.
```

**After**:

```rust
let visuals = UiButtonVisuals::new()
    .with_normal_text(UiColorRole::PrimaryText)
    .with_hovered(UiColorRole::Accent, UiColorRole::Surface)
    .with_pressed(UiColorRole::Positive, UiColorRole::Surface)
    .with_focused(UiColorRole::Focus, UiColorRole::Focus, 2.0)?
    .with_disabled(UiColorRole::Disabled, UiColorRole::SecondaryText);
let descriptor = descriptor.with_surface_mapping(render_scale, input_scale)?;
```

The same `UiButtonVisuals` fields are persisted in Scene/Prefab component data. Product adapters
change typed facts and enabled state only; they do not reconstruct the state palette.

**User action**: add `UiButtonVisuals` to authored Button entities and choose roles from the
`UiTheme` palette. Do not add state colors to the Rust product adapter. Treat physical resize and
OS scale events as Host mapping updates; pointer drivers may continue to submit platform-logical
coordinates because the Host now records their conversion explicitly.

**Source action**: `manual-rewrite`. Existing Button records without `UiButtonVisuals` remain valid
and use the bounded default mapping; a product that requires data-authored state roles must add the
component. A present `UiButtonVisuals` record is an exact canonical shape with no missing/defaulted
fields or compatibility aliases.

**Cache action**: `delete` derived UI frames and render packets after changing the authored visual
component or surface mapping; keep source image/font assets and regenerate packet/GPU caches.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert the complete product UI authoring, input-driver, surface-mapping, and desktop
probe changes together. Do not restore a Rust-only palette or divide coordinates by a fixed scale,
because either choice makes the product proof diverge from authored data and Host ownership.

**Verification anchors**: `crates/nara_ui/src/{authoring,codec,interaction,runtime,
adapter_tests}.rs`, `src/ui_presentation.rs`, `reference-game/prefabs/reference-ui.prefab.json`,
`reference-game/tests/{desktop_flow,plugin_composition,desktop_render}.rs`, and the
[NUI-U17 verification shard](../knowledge/engineering/verification/2026-08/2026-08-06T181000Z-nui-u17-deep-runtime-ui-ownership-3eb9a95fdf4a43caa78aea9fad953e13.md)
prove canonical state-role round trips, composable visual variants, exact logical/physical
mapping, one coalesced input path, and presentation-free headless composition.

## NUI-U17-2: Exact Presented UI Sessions And Pause-Safe Action Delivery

**Removed contract**: `UiActionEvents` is no longer a frame-local buffer, and ordinary callers may
not invoke a public technical `dispatch_presented_ui_input` function or read mutable interaction
state independently of the presented frame. A candidate that has not been presented cannot change
the behavior of the frame the user is operating. A stale, retired, mismatched, or blocked route is
closed rather than falling through to gameplay.

**Canonical replacement or deletion rationale**: `nara_ui` now owns private projection, candidate,
presented-history, transaction, timeline, and journal modules. A `UiPresentedSession` couples the
exact `UiFrame`, presentation receipt, surface descriptor, and immutable behavior snapshot used by
both rendering and interaction. `UiActionJournal` provides bounded monotonic sequences with
explicit consumer registration, acknowledgement, safe-prefix reclamation, unregister, and typed
backpressure. Route outcomes are `PassThrough`, `Consumed`, or `RejectedClosed`; only an explicit
non-UI route may pass to gameplay. The Reference Game consumes actions with a generation-bound
cursor and unregisters it during plugin shutdown.

**Before**:

```rust
let events = world.resource::<UiActionEvents>();
let action = events.as_slice().iter().find(|event| event.action() == "retry");
```

**After**:

```rust
let cursor = world
    .resource_mut::<UiActionJournal>()
    .register_consumer(UiActionConsumerId::new("gameplay-ui")?)?;
let pending = world.resource::<UiActionJournal>().pending(&cursor)?;
// Acknowledge only after the product command has been admitted.
```

**User action**: migrate each product or adapter consumer to one registered cursor. Retain the
cursor until its shutdown obligation has successfully unregistered it. Do not cache a behavior
snapshot separately from the exact presented session, and do not acknowledge an action before the
downstream command queue accepts it.

**Source action**: `manual-rewrite`. There is no compatibility event buffer or public dispatch
wrapper.

**Cache action**: `delete` stale interaction/frame-derived state after changing a surface
descriptor, route endpoint, or authored UI revision; packet and journal state are rebuilt or
reclaimed through their owning lifecycle.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert the complete runtime UI ownership and consumer migration commits together.
Do not restore frame-local events, global button behavior, or public technical schedule phases.

**Verification anchors**: the private modules under `crates/nara_ui/src/runtime/` and
`crates/nara_ui/src/interaction/`,
`crates/nara_ui/src/adapter_tests.rs`, `crates/nara_ui_kernel/src/interaction/session.rs`,
`reference-game/src/runtime_ui.rs`, and the NUI-U17 verification shard prove exact old-frame
authority, cross-surface isolation, fail-closed routing, pause/resume exactly-once delivery, route
ownership, cursor reclamation, and independent schedule composition.

## UIF-U1-1: Atomic Presented-Session Interaction And Invalidation

**Removed contract**: `UiInteractionSession::dispatch` no longer returns a batch that can contain
both routed events and an optional `UiNonActivation`. That shape allowed a rejected input to carry
candidate focus, hover or capture changes into the runtime adapter. Input-timeline synthetic mouse
releases produced during resize, scale-factor changes, focus loss, pointer leave or retirement also
must not enter the ordinary release/activation path.

**Canonical replacement**: the kernel now returns the closed
`UiDispatchOutcome::Consumed(UiDispatchBatch) | UiDispatchOutcome::Rejected(UiNonActivation)`.
Only `Consumed` carries routed events and a commit-capable candidate. `Rejected` drops the cloned
session before the adapter can replace session, widget reducers or the action journal. When a new
presented frame requires capture/hover cleanup, that cleanup is a successful consumed
reconciliation; a later input against the retired or disabled target is then rejected without a
second state mutation. Semantic snapshots project only committed focus and never synthesize a
first-focus fallback.

**Before**:

```rust
let dispatch = session.dispatch(&journal, route, input)?;
if let Some(reason) = dispatch.non_activation() {
    record_rejection(reason);
}
reduce_widgets(&dispatch);
```

**After**:

```rust
match session.dispatch(&journal, route, input)? {
    UiDispatchOutcome::Consumed(batch) => reduce_widgets(&batch),
    UiDispatchOutcome::Rejected(reason) => record_rejection(&reason),
}
```

**User action**: update direct kernel consumers and neutral examples to match on the closed
outcome. Do not reintroduce a compatibility alias or manually clear widget state after a rejected
route. Product adapters should continue to consume typed journal entries only after downstream
command admission.

**Source action**: `manual-rewrite`; this is an intentional pre-1.0 Rust API and observable input
semantics break.

**Cache action**: `delete` interaction/session state after changing the dispatch outcome or
geometry invalidation contract; presented frame and action journal history remain owned by their
existing lifecycle and are not reconstructed from a rejected candidate.

**Compatibility window**: none (fearless refactor before 1.0).

**Rollback**: revert the complete UIF-U1 kernel outcome, runtime transaction, timeline projection,
widget test and adapter migration together. Do not restore batch-level rejection or ordinary
release handling for invalidated input.

**Verification anchors**: `crates/nara_ui_kernel/src/interaction/{session,types,state,pointer,
focus}.rs`, `crates/nara_ui/src/interaction/{transaction,timeline}.rs`,
`crates/nara_ui_kernel/tests/interaction.rs`, `crates/nara_ui/src/adapter_tests.rs`,
`crates/nara_ui_widgets/tests/button.rs`, `crates/nara_winit/tests/accessibility.rs`, and the
`interaction_lab` example prove rejection atomicity, pointer-capacity preservation, fresh
geometry coordinates, committed semantic focus, disabled-focus navigation, retirement cleanup,
and resize cancellation without activation.

## UIF-U1-2: Presented Reconciliation, Candidate Staging, And Pointer Ownership

**Removed contract**: publishing or dispatching a UI frame no longer permits interaction state to
silently advance on a different generation. A candidate compile that requires pointer, hover,
focus, or reducer reconciliation is not itself presented and cannot mutate the live interaction
session. A physical pointer release is no longer allowed to leave pressed state behind solely
because the bounded action journal cannot admit the resulting activation.

**Canonical replacement**: `UiInteractionSession::stage_candidate_presentation` returns an opaque,
documentation-hidden `UiCandidateReconciliation` containing a cloned session and a reconciliation
batch. The runtime adapter reduces that batch against the candidate behavior, reprojects visual facts,
and adopts the staged tuple only after the behavior and interaction facts are identical. Any input
committed after candidate compilation invalidates the candidate and requires a fresh compile. During
presentation reconciliation, equivalent routing facts rebind pointer capture and position to the
new epoch; changed geometry, modal scope, removal, or action/enabled behavior emits consumed
cleanup and retires the old gesture. The baseline admits at most one active pointer owner and one
active seat owner per surface; distinct surfaces remain the multi-endpoint boundary.

**Before**:

```rust
let cleanup = session.reconcile_candidate_presentation(
    surface,
    previous_frame,
    candidate_frame,
    invalidated_keys,
)?;
// `session` was already changed before dependent widget reducers could fail.
```

**After**:

```rust
let staged = session.stage_candidate_presentation(
    surface,
    previous_frame,
    candidate_frame,
    invalidated_keys,
)?;
let (candidate_session, cleanup) = staged.unwrap().into_parts();
// Adopt `candidate_session` only together with successfully reduced widget state.
```

**User action**: direct kernel consumers must use the staged result for unpresented candidates and
must not retain pointer capture across a changed presentation generation. Treat a consumed cleanup
batch as the complete transaction; do not replay its release or cancellation event as a new user
input. When action-journal admission rejects a pointer release, commit the physical release and
record the typed rejection, then require a fresh press for a later activation.

**Source action**: `manual-rewrite`; this is an intentional pre-1.0 API and observable input
semantics break.

**Cache action**: `delete` candidate/session/reducer state produced before an input commit or failed
reprojection. Preserve only the last presented history through its owning journal; a failed candidate
must not replace it.

**Compatibility window**: none (fearless refactor before 1.0).

**Rollback**: revert commit `d43de14` and its evidence/registration successor together. Do not
restore an in-place candidate reconciliation method, cross-generation pointer activation, or silent
release loss.

**Verification anchors**: `crates/nara_ui_kernel/src/interaction/{session,state,pointer}.rs`,
`crates/nara_ui/src/runtime/{candidate,state,presentation}.rs`,
`crates/nara_ui/src/interaction/transaction.rs`,
`crates/nara_ui_kernel/tests/interaction.rs`, `crates/nara_ui/src/adapter_tests.rs`,
`crates/nara_ui_widgets/tests/button.rs`, and `crates/nara_ui/examples/interaction_lab.rs`.

## UIF-U2: Root Envelopes, Relative Fallback, And Candidate Retirement

**Removed contract**: `UiAuthoringSnapshotBuilder::build` no longer rejects the complete snapshot
for one root-local style, depth, text or node-key failure. `UiAuthoringError::DepthLimitExceeded`
is removed; depth failure is now a root outcome. Cached compiled roots no longer retain their old
global layer or root sequence. A failed current source attempt may no longer leave an older
unpresented candidate available through `latest_ui_frame` or `commit_ui_presentation`.

**Canonical replacement**: snapshot-global root identity, order and capacity failures remain
`UiAuthoringError`/`UiCompileError` and publish no candidate. Every known root otherwise owns an
ordered envelope whose body is either validated or carries a typed `UiRootDiagnostic` such as
`DepthLimitExceeded`, `KeyRejected` or `InvalidStyle`. `UiAuthoringSnapshot::roots()` returns only
validated root bodies; the compiled frame's ordered `root_outcomes()` reports every envelope. A
compatible last-good cache stores relative layout/paint facts only, rebinds the current layer and
ordinal, and contributes no hit, focus, semantics or action. A retained modal remains an inert
barrier; a modal without a compatible fallback rejects the candidate. Any whole-candidate failure
discards unpresented work while preserving an already committed presented session.

**Before**:

```rust
let snapshot = UiAuthoringSnapshotBuilder::new(revision)
    .with_root(invalid_root)
    .with_root(valid_root)
    .build()?; // One invalid root aborted both roots.
```

**After**:

```rust
let snapshot = UiAuthoringSnapshotBuilder::new(revision)
    .with_root(invalid_root)
    .with_root(valid_root)
    .build()?;
let frame = compiler.compile(&snapshot, environment, theme)?;
for outcome in frame.root_outcomes() {
    // Published, RetainedVisual, or Absent in current envelope order.
}
```

**User action**: direct kernel consumers must inspect root outcomes instead of expecting
root-local validation to fail `build`. Do not infer total envelope count from `roots().len()`, do
not cache frame-global order outside the compiler, and do not retain a candidate `Arc<UiFrame>`
after a later source attempt is rejected. Presenters must treat `FrameNotLatest` as terminal for
that candidate.

**Source action**: `manual-rewrite`; no compatibility error variant, stale-order cache adapter or
candidate alias is retained.

**Cache action**: `delete` root caches produced before the relative-order contract and discard every
unpresented frame after a whole-candidate failure. Preserve only the presented history through its
own journal/session lifecycle.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert commit `2fde103` and its evidence/registration successor together. Do not
restore whole-snapshot root-local rejection, cached global order, modal pass-through or stale
candidate commit authority.

**Verification anchors**: `crates/nara_ui_kernel/src/authoring.rs`,
`crates/nara_ui_kernel/src/compiler/{cache,order,root}.rs`,
`crates/nara_ui/src/runtime/{projection,state}.rs`,
`crates/nara_ui_kernel/tests/{validation,compile_order,failure_atomic}.rs`,
`crates/nara_ui/src/adapter_tests.rs`, and `crates/nara_ui_kernel/examples/layout_gallery.rs`.

## UIF-U2C: Snapshot Profiles And Candidate Text Work

**Removed contract**: a `UiAuthoringSnapshot` is no longer interpreted solely by the caller's
current `UiLimits`. The builder records an exact authoring profile (root count, source-node count,
depth and encoded-key limits), and a compiler with a different profile rejects before advancing
frame revision or cache/LRU state. Compile-only paint/diagnostic/cache settings and session limits
remain independent.

**Canonical replacement**: reserve input text work for each unique layout request before shaping;
cache deterministic failed requests within the candidate; and count successful shaped-glyph output
separately from source bytes. Unicode scalar count is not used as a proxy for glyph output. Repeated
failed layout requests therefore do
not repeatedly shape or charge, while distinct failures eventually exhaust the candidate-wide work
budget. No compatibility alias is retained.

## UIF-U3C: Prepared Submission And Infallible Receipt Commit

**Removed contract**: a backend may no longer submit pixels and then call a public, fallible
frame-plus-epoch helper that independently publishes the UI receipt and input route.

**Canonical replacement**: stage the exact presented history, interaction/reducer state and input
route reservations before backend submit. The backend receives a non-cloneable prepared attempt and
returns a commit bound to packet, target, Surface Slot, environment, capability and GPU generations.
Consuming that commit performs an infallible authority swap. Deferred attempts are superseded by a
newer admitted attempt and cannot revive; target faults and device faults retire only their owning
domain.

**User action**: first-party presenters must call the runtime preparation Interface before
submitting a packet, pass the resulting exact backend permit through the backend, and return the
consumed completion to the Host. Do not construct a `UiPresentationReceipt`, submit directly through
`UiFrameJournal`, or publish a raw backend outcome. Treat a missing, duplicate, stale or superseded
permit as fail-closed and abort its prepared attempt.

**Source action**: `manual-rewrite`; migrate direct frame-plus-epoch commit callers to
`prepare_ui_presentation` plus exact backend completion. The old test-only receipt compatibility
helper and raw `issue_presented`/`defer_submission`/`discard_submitted` re-exports are removed.

**Cache action**: `delete` outstanding presenter/backend retry state created before this contract.
Retained immutable packets remain readable, but they are not submission authority and cannot revive
an aborted or superseded attempt.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert commit `0f5154f` and its evidence/registration successor together. Do not
restore a fallible post-submit route publication path or allow consumers to mint receipts.

**Verification anchors**: `crates/nara_ui/src/runtime.rs`,
`crates/nara_ui/src/runtime/{candidate,presentation,state}.rs`,
`crates/nara_ui_kernel/src/journal.rs`, `crates/nara_input/src/timeline.rs`,
`crates/nara_ui_render/src/{integration,packet/outcome,planner/mod}.rs`,
`crates/nara_render_wgpu/src/{backend,error,lib}.rs`, `src/ui_presentation.rs`,
`src/ui_presentation/attempts.rs`, `src/ui_presentation/tests.rs`, and
`examples/ui_presentation_gallery.rs`.

## UIF-U4: Host-Owned Surface Slots And Receipt-Stamped Accessibility

**Removed contract**: Runtime UI no longer installs a copyable `UiSurfaceDescriptor`, updates
logical size/input endpoint/render scale through independent helpers, or publishes an opaque numeric
accessibility action through `nara_input`. The old `update_ui_surface_mapping`,
`update_ui_surface_logical_size`, `update_ui_surface_input_endpoint`, and independent
`install_ui_presentation_route`/`retire_ui_presentation_route` paths are deleted. A platform route
cannot be replaced by a late snapshot with older slot, route, geometry, or same-slot presentation
generations.

**Canonical replacement**: Host code installs one immutable `UiSurfaceSlot` composed from a stable
surface identity, endpoint, and `UiSurfaceContentTransform`. Endpoint and transform changes use one
atomic `update_ui_surface_slot` operation; retirement uses one atomic `retire_ui_surface`
transaction and propagates its exact `UiSurfaceRetirement` record to downstream platform and
presentation owners. The input timeline stores the exact `InputRouteStamp` returned when a
presented receipt is published. Logical slot/transform invalidation uses
`unbind_presented_endpoint`, which preserves held physical edges while making their later release
follow the consumer that received the original press even if a successor presentation is installed.
True platform teardown uses
`retire_presented_endpoint`, which emits bounded balanced releases and clears all endpoint state.
The coordinated retirement path admits its complete lifecycle batch before removing the route; a
capacity or sequence failure therefore leaves the UI owner and the physical endpoint unchanged.
Platform-observed window loss remains fail-closed because the native endpoint is already gone. Both
operations reject a replacement owner.

**Before**:

```rust
let descriptor = UiSurfaceDescriptor::new(surface, window, seat, 960.0, 540.0, 1.0)?;
install_ui_surface(world, descriptor)?;
update_ui_surface_mapping(world, surface, render_scale, input_scale)?;
```

**After**:

```rust
let transform = UiSurfaceContentTransform::new(
    UiSize::new(960.0, 540.0)?,
    physical_size,
    safe_area,
    platform_scale,
)?;
let endpoint = UiSurfaceEndpoint::new(window, seat, pointer);
let slot = UiSurfaceSlot::new(surface, endpoint, transform)?;
install_ui_surface(world, slot)?;
update_ui_surface_slot(world, surface, endpoint, next_transform)?;
```

Accessibility adapters now call `WinitAccessibilityPublisher::publish_update`. The publisher
performs all fallible validation and candidate construction, records the exact AccessKit tree and
mapping in Host-owned state, and atomically swaps the committed mapping. There is no caller-supplied
callback or publishable prepared token. `WinitAccessibilityReceipt` proves exact publisher-record
commit only; it does not claim OS/client observation. `map_action` returns a typed
`UiAccessibilityInput` containing the committed route and seat. Focus and semantic actions use the
same UI transaction as pointer and keyboard input.

**User action**: migrate callers to the slot/transform constructor and the single successor update.
Keep surface IDs non-reusable. Do not construct `UiRouteStamp` from an epoch alone; derive it from
the exact `UiPresentationReceipt` or the platform route returned by the timeline. Treat
`PresentedRouteRegression`, `PresentedRouteMismatch`, stale snapshots, and retired identities as
fail-closed outcomes.

**Source action**: `manual-rewrite`; this is an intentional pre-1.0 ownership and coordinate
contract break. No compatibility alias is retained.

**Cache action**: delete cached transforms, input routes, semantic trees, callback mappings, and
render-target route mirrors after an endpoint/transform replacement or retirement. Retained packet
and glyph caches may survive only when their owning planner/backend contract explicitly permits it.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert implementation commits `f7228ad` and `d088630`, assertion commit `c1c5554`,
and this migration/ledger/evidence successor together. Do not restore independent surface update
helpers, an opaque accessibility ingress, or a mutable callback mapping that advances before its
TreeUpdate is published.

**Verification anchors**: `crates/nara_ui/src/runtime/{surface,state}.rs`,
`crates/nara_ui/src/runtime.rs`, `crates/nara_input/src/timeline.rs`,
`crates/nara_ui_kernel/src/{geometry,journal}.rs`,
`crates/nara_ui_kernel/tests/surface_transform.rs`,
`crates/nara_ui/src/adapter_tests.rs`, `crates/nara_winit/src/accessibility.rs`,
`crates/nara_winit/tests/accessibility.rs`, `src/ui_presentation.rs`, and the UIF-U4 focused
nextest gate prove transform round-trips, black-bar handling, endpoint isolation, route regression,
cross-slot epoch restart, logical-unbind versus physical-retirement behavior, exclusive
accessibility publication, and exact receipt/action mapping.

## UIF-U4C: Explicit Surface Availability And Opaque Accessibility Publishers

**Removed contract**: an installed Surface Slot no longer remains presentation-authoritative while
its native target has zero extent, is missing, or has an invalid scale. A caller can no longer
construct or choose `WinitAccessibilityPublisherId`, pass that ID into
`WinitAccessibilityPublisher::new`, map an unbound AccessKit `ActionRequest`, republish semantics at
the suspended slot generation, or assume every different suspension generation is a conflict.

**Canonical replacement**: the Runtime UI Host owns a unique window-target-to-Surface-Slot index
and exposes each installed lifetime as `UiSurfaceAvailability::Active` or
`UiSurfaceAvailability::Suspended(reason)`. Use `suspend_ui_surface` for a temporarily unavailable
target, `restore_ui_surface` for a fresh successor endpoint/transform generation, and
`retire_ui_surface` for permanent teardown. Suspension keeps the target reservation and only the
physical held-edge facts required for balanced releases; direct retirement releases the
reservation even when no restored frame was presented.

`WinitAccessibilityPublisher::new(surface_id)` now allocates its own opaque process-unique
publisher identity and returns `Result`. Save the exact `WinitAccessibilityReceipt` returned by
`publish_update`, bind an incoming AccessKit request with `receipt.bind_action(request)`, then pass
that receipt-bound request to `map_action`. A surface suspension must be forwarded to the publisher
with its successor slot generation. The suspended generation itself is not publishable; only a
strict successor semantic route can restore action mapping. A later successor suspension replaces
the earlier one, while stale suspensions and stale retirement tokens fail closed.

**Before**:

```rust
let publisher_id = WinitAccessibilityPublisherId::new(7).unwrap();
let mut publisher = WinitAccessibilityPublisher::new(publisher_id, surface);
let receipt = publisher.publish_update(&semantics)?;
let input = publisher.map_action(&WinitAccessibilityActionRequest::new(receipt, request))?;
```

**After**:

```rust
let mut publisher = WinitAccessibilityPublisher::new(surface)?;
let receipt = publisher.publish_update(&semantics)?;
let input = publisher.map_action(&receipt.bind_action(request))?;

let suspension = suspend_ui_surface(world, surface, UiSurfaceSuspensionReason::ZeroExtent)?;
if let Some(suspension) = suspension {
    publisher.suspend(WinitAccessibilitySuspension::new(
        suspension.surface_id(),
        UiRevision::new(suspension.suspended_slot_generation()).unwrap(),
    ))?;
}
```

**User action**: rewrite publisher construction and every platform action callback. Keep the exact
publication receipt alongside the platform tree lifetime; do not reconstruct it from a surface,
node ID, generation, or integer publisher ID. Replace “skip the frame while minimized” branches
with an explicit suspend/restore transition, and forward permanent teardown through the exact
retirement token.

**Source action**: `manual-rewrite`. The caller-supplied identity, raw action ingress, implicit
unavailable-target behavior, and equal-generation restore semantics are intentionally removed.

**Cache action**: delete cached platform trees, node mappings, input transforms, prepared backend
attempts and presentation routes when a surface suspends. Restore may retain source authoring and
resource caches, but it must compile and present a fresh slot generation before semantics or input
resume.

**Compatibility window**: none (pre-1.0 fearless replacement).

**Rollback**: revert implementation commit `d28efcf` and this migration/ledger/evidence successor
together. Do not restore caller-selected publisher IDs, raw AccessKit action mapping, skipped
unavailable-target branches, or stale backend acceptance after suspension.

**Verification anchors**: `crates/nara_ui/src/runtime/{state,surface,targets}.rs`,
`crates/nara_ui/src/adapter_tests.rs`, `crates/nara_winit/src/accessibility.rs`,
`crates/nara_winit/tests/accessibility.rs`, `src/ui_presentation/{attempts,surfaces,tests}.rs`, and
the UIF-U4C verification record prove target-conflict atomicity, direct suspended retirement,
outstanding-attempt cancellation, strict accessibility generations, stale/foreign action rejection,
missing-window recovery and non-16:9 letterbox mapping.

## CDPF-U1-1: Explicit Directory Staging And File Write Authority

**Removed contract**: `ReplaceSourceBinding` no longer names the source-binding proof shared by
file replacement and directory publication. Existing `FileCapability` values no longer imply
writable authority merely because their underlying host handle might be writable. No caller may
construct a same-parent directory candidate through native paths and later present it as a
capability-owned publication transaction.

**Canonical replacement**: use `PublicationSourceBinding` for the exact handle-bound,
cooperative name-bound or unsupported source proof. A read-write `DirectoryCapability` may create
one direct child directory or regular file without overwrite. Newly created files expose their
write authority only through `FileCapability::writer`, which returns a borrowed
`CapabilityWriter`; files opened from existing project content remain read-only. Use
`DirectoryCapability::begin_directory_publication` to retain an exclusive staging directory,
destination component, parent lineage and exact source-binding proof in a move-only
`DirectoryPublicationTransaction`. CDPF-U1 intentionally exposes no publish, rename, removal or
recursive product operation.

**Before**:

```rust
use nara_fs::ReplaceSourceBinding;

let source_binding = matrix.replace_source_binding();
assert_eq!(source_binding, ReplaceSourceBinding::HandleBound);
```

**After**:

```rust
use std::io::Write;

use nara_fs::{PublicationSourceBinding, RelativeComponent};

let transaction = parent.begin_directory_publication(
    &RelativeComponent::new("project").unwrap(),
    &RelativeComponent::new(".project-staging-7f4a").unwrap(),
)?;
assert!(matches!(
    transaction.source_binding(),
    PublicationSourceBinding::HandleBound | PublicationSourceBinding::NameBound
));

let staging = transaction.staging_directory();
let mut manifest = staging.create_child_file(
    &RelativeComponent::new("nara.toml").unwrap(),
)?;
manifest.writer()?.write_all(b"format_version = 1\n")?;
manifest.sync()?;
```

**User action**: rename imports and matches to `PublicationSourceBinding`. Treat
`FileCapability::rights` as the write-authority fact and request a `CapabilityWriter` before
writing. Product code that stages a finite directory tree must keep the transaction alive and use
only the returned child capabilities; CDPF-U2 will add the consuming terminal operations.

**Source action**: `none`; this migration changes Rust authority and ownership, not project source
or persistent formats.

**Cache action**: `keep`; no cache identity or serialized artifact changes in CDPF-U1.

**Compatibility window**: none (unreleased canonical replacement). No deprecated
`ReplaceSourceBinding` alias is retained.

**Rollback**: revert the complete CDPF-U1 implementation, migration, ledger and evidence together.
Do not restore implicit writable file handles or native-path directory staging independently.

**Verification anchors**: `crates/nara_fs/src/{capability,directory_publication,file,proof}.rs`,
`crates/nara_fs/src/platform/{mod,windows,unix}.rs`,
`crates/nara_fs/tests/filesystem_contract.rs`, owner-local move-only and failure-injection tests,
and the CDPF-U1 verification record prove exclusive child creation, capability lineage, explicit
write authority, supported-tier admission and fail-closed rejection before staging.

## CDPF-U2-1: Exact Directory Publication And Truthful Begin Terminality

**Removed contract**: `DirectoryCapability::begin_directory_publication` no longer returns a plain
`FsError` whose failure could be interpreted as proof that no staging object was created. A live
transaction is no longer an indefinitely retained staging holder without a consuming terminal
operation. Callers may not infer that every rejection removed the candidate or recover staging
authority from a diagnostic name.

**Canonical replacement**: `begin_directory_publication` returns
`DirectoryPublicationBeginError`. `BeforeStaging` alone proves that no staging name was created;
post-create validation failures return either `RejectedClean` with exact empty-directory cleanup
evidence or `Orphaned` with every authority and identity fact that the adapter could truthfully
retain. A successful move-only transaction is consumed through `publish` or `abort`, returning
`DirectoryPublicationOutcome::{Published, RejectedClean, Orphaned}`. A publication receipt keeps
parent, candidate, published identity, source binding, topology, conflict protection, atomicity and
durability as independent facts.

**Before**:

```rust
let transaction = parent.begin_directory_publication(&destination, &staging)?;
// The prototype exposed no consuming publication outcome, and every error looked pre-mutation.
```

**After**:

```rust
let transaction = match parent.begin_directory_publication(&destination, &staging) {
    Ok(transaction) => transaction,
    Err(error) => {
        if let Some(orphan) = error.orphan() {
            report_recoverable_staging(orphan.staging_name());
        }
        return Err(error.into());
    }
};

match transaction.publish() {
    DirectoryPublicationOutcome::Published(receipt) => accept(receipt),
    DirectoryPublicationOutcome::RejectedClean(rejection) => reject_cleanly(rejection),
    DirectoryPublicationOutcome::Orphaned(orphan) => report_orphan(orphan),
}
```

**User action**: update error handling at every begin call. Only `BeforeStaging` permits the caller
to assert that the namespace was unchanged. Quiesce staging writers before `publish`; an outstanding
shared staging capability intentionally prevents exact cleanup and produces `Orphaned`. Consume the
transaction exactly once and persist or surface orphan diagnostics instead of attempting recursive
name-based deletion.

**Source action**: `none`; this migration changes Rust authority, failure and receipt semantics,
not project source or persistent formats.

**Cache action**: `keep`; no serialized cache format changes. Callers must discard any prototype
in-memory assumption that a failed begin or publish implies an absent staging name.

**Compatibility window**: none (unreleased canonical replacement). No `FsError` wrapper, implicit
drop cleanup, overwrite fallback or name-based reclamation compatibility path is retained.

**Rollback**: revert implementation commits `eede23a` and `e3a5e7b` together with their migration,
ledger and verification successor. Do not restore a plain begin error independently of the exact
post-create terminal outcomes.

**Verification anchors**: `crates/nara_fs/src/directory_publication.rs`,
`crates/nara_fs/src/platform/{windows,unix}.rs`, `crates/nara_fs/tests/filesystem_contract.rs`, and
verification record `b27082ccee9b484bb298d4d00901400c` prove no-replace publication, collision,
identity/topology drift, exact cleanup, created-but-unopened staging, orphan authority and the
absence of a destructive fallback.

## EIR-U1-1: Typed Image Metadata And Canonical Import Recipes

**Removed contract**:

- The flat `AssetMeta { stable_id, path, source_kind }` Rust value and matching flat
  `asset_meta` payload.
- `MissingMetaPolicy` and `ProjectAssetDatabase::insert_source_with_meta_policy`, which could
  install an asset identity without a persisted sidecar.
- Image byte/file request parameters for caller-provided dependency digests, settings hashes, and
  import profiles, plus importer/plugin-wide color-space setters.
- Image source metadata that duplicated stable ID and source hash outside its artifact key.

**Canonical replacement or deletion rationale**: use `AssetMetaIdentity` for the persisted identity
core and `AssetMeta<ImageImportSettings>` for the only canonical image sidecar. The image domain
owns the closed sRGB/Linear settings value, canonical settings bytes, settings hash, fixed PNG
recipe facts, and importer version 3. Strict ProjectContent and runtime reload consume that same
typed recipe. No compatibility reader remains because the superseded shape was never released.

Before:

```rust
let meta = AssetMeta::new(stable_id, path, AssetSourceKind::Image);
let request = ImageBytesImportRequest::new(
    record,
    png_bytes,
    dependency_digest,
    settings_hash,
    profile,
);
```

After:

```rust
let settings = ImageImportSettings::new(ImageColorSpace::Linear);
let meta = AssetMeta::new(
    AssetMetaIdentity::new(stable_id, path, AssetSourceKind::Image),
    settings,
);
let request = ImageBytesImportRequest::new(record, png_bytes, settings);
```

The corresponding payload is now nested:

```json
{
  "asset": {
    "stable_id": "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f",
    "path": "textures/player.png",
    "source_kind": "image"
  },
  "settings": {
    "color_space": "linear"
  }
}
```

**Affected examples and fixtures**: root image-import and windowed-sprite examples, the UI
presentation gallery, Sprite/UI/WGPU image fixtures, ProjectContent fixtures, Reference Game image
sidecars and safety tests, and every image import/reload test now use the typed recipe.

**User action**: rewrite every experimental image `.meta` payload into the nested `asset` plus
`settings` shape before opening the project. Preserve the existing stable ID and assets-root-relative
path. Choose `srgb` or `linear` explicitly. Update direct import requests to pass
`ImageImportSettings`; remove independent hash/profile arguments and global color-space setters.

**Source action**: `manual-rewrite`. Runtime and Generic Authoring do not rewrite the superseded flat
shape. Repository-owned sidecars were rewritten atomically with the strict reader.

**Cache action**: `delete` or rebuild affected image artifacts. Importer version 3 and the typed
settings hash intentionally supersede the NUI-U3 version-2 cache-keep statement.

**Compatibility window**: none (unreleased canonical replacement). There is no flat-reader alias,
implicit on-open migration, default settings hash, or global color-space fallback.

**Rollback**: restore sidecars and code together from the pre-EIR revision. Do not restore a flat
reader, unpersisted identity generation, or caller-authored recipe hashes independently.

**Verification anchors**: `crates/nara_asset/tests/meta_format.rs`,
`crates/nara_image/tests/{image_settings,image_import_limits}.rs`,
`crates/nara_image/src/tests.rs`, `tests/{project_content_boot,project_content_boundary}.rs`,
`src/project_host/authoring/mod.rs`, `reference-game/tests/image_asset_safety.rs`, and the canonical
Reference Game sidecars prove strict nested admission, distinct settings hashes, typed strict load,
recipe-preserving reload, Generic Authoring retention, and fail-before-publication rejection.

## Persistent Format Matrix

Rows describe only formats intentionally supported after the refactor; deleted draft formats do
not remain as pseudo-legacy rows.

| Kind | Canonical written version | Readable versions | Retained migration chain | Engine minimum | Source action | Cache action |
|---|---:|---|---|---|---|---|
| `scene` | 1 | 1 | none | `0.1.0` | `manual-rewrite` | `keep` after rewrite |
| `prefab` | 1 | 1 | none | `0.1.0` | `manual-rewrite` | `keep` after rewrite |
| `scene_patch` | 1 | 1 | none | `0.1.0` | `manual-rewrite` | `keep` after rewrite |
| `component_schema_catalog` | 1 | 1 | direct predecessor validation only | `0.1.0` | regenerate or `manual-rewrite` | `delete`/regenerate derived copies |
| `asset_meta` | 1 | 1 | none | `0.1.0` | `manual-rewrite` | `delete`/regenerate derived copies |
