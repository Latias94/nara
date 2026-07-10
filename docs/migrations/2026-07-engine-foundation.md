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
| U5-1 | U5 | `e77da64` | `rust-api` | Task pool configuration, submission, terminal results, test execution, and shutdown | Configure bounded threaded pools, handle explicit spawn/terminal outcomes, and use the test-only inline driver where deterministic execution is required. |
| U5-2 | U5 | `e77da64` | `persistent-shape` | `nara.toml` runtime plugin-plan spelling and task pool schema | Rewrite flat task fields into per-pool/shutdown tables and use only the canonical `runtime-2d` value. |
| U18-1 | U18 | `6a70847` | `rust-api/behavior` | Diagnostic construction, reports, runtime observations, pressure snapshots, and diagnostic plugin composition | Migrate to validated/classified bounded observations and reduce any `diagnostics.runtime_capacity` above 4,096. |

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

## Persistent Format Matrix

U9 and later format-owning units populate this table before writing a new shape. Rows describe only formats intentionally supported after the refactor; deleted draft formats do not remain as pseudo-legacy rows.

| Kind | Canonical written version | Readable versions | Retained migration chain | Engine minimum | Source action | Cache action |
|---|---:|---|---|---|---|---|
| _Added by the format owner_ | 1 | 1 | none | _set by owner_ | _set by owner_ | _set by owner_ |
