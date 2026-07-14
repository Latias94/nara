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
| U2-1 | U2 | `2867235` | `rust-api` | `App` mutation/update, `Plugin`, cleanup, group, and runner signatures | Propagate setup/update errors, use narrow cleanup/group contexts, and let runners borrow the app. |
| U2-2 | U2 | `2867235` | `rust-api` | Built-in `register_*_components` helpers | Handle `ComponentRegistryError`; plugin installation now reports contextual registration failure. |
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

## U2-1: Terminal Plugin Lifecycle and Fallible App Mutation

**Removed contract**:

- `App::try_update()` plus panic-based `App::update() -> AppFrameOutcome`.
- Infallible mutable entry points such as `world_mut`, `insert_resource`, `init_resource`,
  `add_startup_systems`, `add_systems`, `configure_sets`, and `set_runner`.
- `Plugin::cleanup(&mut App) -> ()` and unrestricted `PluginGroup::build(&mut App)`.
- `RunnerFn = FnOnce(App)`, which hid cleanup failures inside runner-owned `Drop`.
- The implicit `plugins_finished: bool` behavior that allowed committed failures to be retried.

**Canonical replacement or deletion rationale**: `App` exposes one explicit terminal plugin state
machine. Mutable entry points and `update` return `Result`; `Plugin::preflight` is the only
pre-mutation retry boundary; cleanup uses `PluginCleanupContext` and `PluginCleanupError`; groups use
`PluginGroupBuilder`; runners borrow `&mut App`; failure details use `PluginFailureReport`. The old
paths are deleted because they could run a partially initialized app or lose cleanup ownership.

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

fn cleanup(
    &self,
    context: &mut PluginCleanupContext<'_>,
) -> Result<(), PluginError> {
    context.world_mut().remove_resource::<Backend>();
    Ok(())
}
```

**Affected examples and fixtures**: root facade plugin groups, all repository examples, plugin crate
tests, window/wgpu examples, asset import examples, and headless/server examples now use only the
fallible canonical API.

**User action**: update downstream plugin/app code to propagate or explicitly handle every returned
error; replace group access to `App` with `PluginGroupBuilder`; change runners to borrow `&mut App`.

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
Fixed Prepare closes the tick into `GameplayCommandBatch`; current-gated Consume and Capture observe
it; engine-owned Ack retires it. Pending and active work share one retained budget. Any lifecycle
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
ceiling: `runtime-core`, `runtime-2d`, `runtime-ui`, `tooling`, `asset-watch`, `desktop-winit`,
`render-wgpu`, and `tooling-egui`; `serde` weakly forwards only into selected domains. Projects use
`[runtime].preset` plus `[capabilities].requested`. The root host reads an already opened
`FileCapability`, applies the 256 KiB sentinel read and bounded TOML shape preflight, then publishes
an immutable `ProjectSettingsCandidate` only when the request fits the compiled ceiling. Plugin
service/conflict/slot closure remains RGF-U4, and assets/startup scene remain RGF-U12.

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
    "desktop-winit",
    "render-wgpu",
    "serde",
] }
```

```rust
app.add_plugins(Runtime2dPlugins)?;
app.add_plugins(DesktopWinitPlugins)?;
app.add_plugins(WgpuBackendPlugins)?;
```

File-backed projects declare product intent separately:

```toml
[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d", "render-wgpu"]
```

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

**Verification anchors**: `tests/{product_capabilities,project_composition}.rs`, the four
`nara_render_wgpu` submitter feature checks, minimal desktop example checks, and
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
- Direct public `WinitRunner::run` calls that bypassed `App::run` plugin finalization and cleanup.
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
runners submit only their owned target IDs and must not use `App::cleanup_plugins` as a local
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
removed providers directly must request retirement and wait for `SurfaceRetired` first. Install a
runner through `WinitPlugin::new(WinitRunner::new(...))` and invoke `App::run`; direct
`WinitRunner::run` is removed so plugin cleanup cannot be bypassed. Custom runners with a fallible
native teardown should return `AppRunError::runner_teardown(prior, teardown)` when both phases fail;
plugin cleanup remains owned and aggregated by `App::run`. Exhaustive `AppRunError` matches must add
the `RunnerTeardown` arm.

**Source action**: `manual-rewrite`.

**Cache action**: `rebuild`; no persistent project data or imported artifact changes.

**Compatibility window**: none (unreleased canonical replacement).

**Rollback**: revert the complete RGF-U11 change. Do not restore unsafe raw-handle construction or
provider removal that bypasses a live surface lease.

**Verification anchors**: `tests/window_surface_retirement.rs`, `crates/nara_winit/src/lib.rs#tests`,
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
        dependency_digest,
        settings_hash,
        profile,
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

## Persistent Format Matrix

Rows describe only formats intentionally supported after the refactor; deleted draft formats do
not remain as pseudo-legacy rows.

| Kind | Canonical written version | Readable versions | Retained migration chain | Engine minimum | Source action | Cache action |
|---|---:|---|---|---|---|---|
| `scene` | 1 | 1 | none | `0.1.0` | `manual-rewrite` | `keep` after rewrite |
| `prefab` | 1 | 1 | none | `0.1.0` | `manual-rewrite` | `keep` after rewrite |
| `scene_patch` | 1 | 1 | none | `0.1.0` | `manual-rewrite` | `keep` after rewrite |
| `component_schema_catalog` | 1 | 1 | direct predecessor validation only | `0.1.0` | regenerate or `manual-rewrite` | `delete`/regenerate derived copies |
