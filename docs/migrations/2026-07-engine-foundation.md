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
| RGF-U4-1 | RGF-U4 | `RGF-U4` | `rust-api/behavior` | Plugin declarations, groups, slots, product planning, schema-provider input, App sealing/shutdown, and custom schedules | Replace imperative metadata/group installation and lifecycle aliases with static declarations, typed repeatable definitions, pure plans, `App::seal`, `App::shutdown_plugins`, and the unified typed schedule API. |
| RGF-U5-1 | RGF-U5 | `RGF-U5` | `rust-api/behavior` | Code-first runtime ownership, exact stepping, typed faults, task close ownership, and Winit driving | Admit a sealed App through `RuntimeCandidate`, drive `RuntimeInstance`, use observable controls and retryable close, call `WinitRunner::run(&mut runtime)`, and rename standalone task shutdown to `shutdown_blocking`. |
| RGF-U29-1 | RGF-U29 | `RGF-U29` | `rust-api/behavior` | Persistent codec output, registry binding, explicit component composition, and target-World apply eligibility | Return `PreparedComponentCandidate`, freeze the registry before runtime preflight, remove implicit required components/hooks from persistent types, and handle fail-closed target-World rejection. |
| RGF-U13-1 | RGF-U13 | `198a680` | `rust-api/behavior` | Managed runtime driver access, physical button transitions, App-exit propagation, and desktop frame/shutdown semantics | Replace generic driver World/resource access with typed resource-local ports, propagate fallible button-edge admission, and handle the one-target desktop result/close contract. |

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

**Canonical replacement or deletion rationale**: the first-playable compatibility inventory is
exactly `CoreStage::FixedUpdate` for typed registration plus joinable
`FixedUpdateSet::Simulate`, `GameplayCommandSet::Consume`, and
`GameplayCommandSet::Capture`. Their Rustdoc owns entry, completion, deferred, skip, error, and
retention semantics. `App::seal` requires automatic deferred insertion, restores final deferred
application, builds the fixed schedule, and returns a structured `ScheduleCompatibilityError`
instead of publishing an invalid graph. `get_schedule_mut` now applies only to custom schedules;
built-in schedules use `add_systems`, `configure_sets`, `set_schedule_build_settings`, and
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
let candidate = RuntimeCandidate::admit(app.seal()?)?;
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
Parented transforms and inherited visibility also reject until the hierarchy contract is
implemented.

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
let intent = HeadlessRunIntent::<GameSnapshot>::new(fixed_ticks)
    .insert_after::<TransformPlugin>(game_plugin_definition());
let mut run = HeadlessRun::new(project_root, intent, commands);

match run.execute_bounded().outcome() {
    HeadlessRunOutcome::Completed(snapshot) => use_snapshot(snapshot),
    HeadlessRunOutcome::CleanupIncomplete => retry_later(),
    HeadlessRunOutcome::Failed => report_failure(),
}
```

**Affected examples and fixtures**: the independent reference-game headless binary and
`runtime_drive` tests now use the concrete product action. The U26 manual raw-App fixture remains
only as the frozen ownership counterfactual and is not a recommended application template.

**User action**: file-backed headless callers should move manifest/content/runtime lifecycle code
behind `HeadlessRun`. Keep game configuration in Rust plugin definitions and schema providers,
submit semantic gameplay commands, retain the action while cleanup is incomplete, and emit only
the returned structured diagnostics. Code-first embedding may continue to use `App`; platform or
Editor Host maintainers may use the advanced runtime module but must not expose its choreography to
ordinary game code.

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
`reference-game/tests/manual_raw_app_baseline.rs` prove the caller boundary, lineage/schema
binding, failure phases, publication linearization, owner retention, and U26 outcome parity.

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
`headless_run_rejects_unbounded_commands` compile-fail fixture prove stable terminal ticks, same-tick
Defeated priority, last-good snapshot retention, privacy-safe CLI sinks, bounded cleanup, public-only
game code, and the owned command boundary.

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
may change before 1.0. Ordinary gameplay code should consume `ActionOutcomes` or submit semantic
gameplay commands rather than driving platform resources.

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
