# Bevy ECS And Rust Hot-Patch Boundaries

Date: 2026-10-21. Nara basis: `6bb1a355c2e09d60979b063569b1de2237e25163`.
Locked ECS: registry `bevy_ecs 0.20.0`. Read-only comparison source:
Bevy `85503941e488152711a4695bbe248e9ed9f453fc` (`0.20.0-dev`).
This is a source/documentation investigation, not a benchmark or an adopted decision.

## Recommendation

Retain the selected ECS substrate and the change-class separation in
[ADR 0093](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md).
Nara's dependency on Bevy ECS is intentional and reaches public authoring semantics, runtime
scheduling, and persistence admission. Replacing it would be a substantial product/API migration.
No evidence gathered here establishes that replacement improves Rust iteration. Conversely,
Bevy's optional system patching does not supply Nara's required structural-change classification,
state migration, runtime-wide quiescence, or known-good fallback.

Develop a complete rebuild-and-resume baseline first, as proposed by the
[mature-engine comparison](2026-10-21-mature-engine-iteration-boundaries.md). An optional patch
experiment should then use a deliberately small supported boundary, identify the actually patched
crate, and compare complete useful-result latency. This study authorizes no dependency or runtime
change.

## What Nara Actually Owns And Inherits

[ADR 0002](../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md) explicitly selects a thin
product-facing layer over Bevy ECS. It assigns lifecycle/plugins to `nara_app`, serialization to
`nara_scene`, and product metadata to Nara rather than claiming an interchangeable ECS implementation.
The current sources implement several concrete coupling points:

| Boundary | Current evidence | Consequence for replacement or patching |
| --- | --- | --- |
| Public ECS authoring | `nara_ecs` reexports Bevy's prelude and modules including queries, systems, World, schedules, lifecycle, relationships, messages and observers | A wrapper crate does not make these types or semantics substrate-neutral |
| Derives | Nara `Component` uses `bevy_ecs_macro_logic`; `Resource` builds a sparse-set component with required `IsResource` support; schedule/set derives use Bevy label machinery | Replacement includes macro expansion and resource-registration behavior, beyond storage |
| App configuration | `App::add_systems` accepts `IntoScheduleConfigs<ScheduleSystem, M>`; `configure_sets` and schedule policy APIs expose Bevy-shaped configuration | Public game/plugin consumers depend on the schedule model |
| Managed runtime | Runtime owner holds `World` and `Schedules`; built-in stages invoke `Schedule::run` with Nara authority/fault validation around them | Nara owns frame/lifecycle policy while Bevy executes systems; both contracts matter |
| Persistence admission | Private ECS helpers examine required components, intrinsic hooks, observers and resource-entity support; exclusive insertion/despawn guards flush baseline work and retain the mutable World borrow through commit | Lifecycle semantics are part of Nara's persistence safety, not incidental implementation details |
| Reflection | `nara_reflect` directly owns a Bevy `TypeRegistry`/`GetTypeRegistration` integration and publicly reexports Bevy reflection | Removing the ECS reflection bridge would not remove Nara's direct reflection dependency |

Sources: [ECS exports](../../crates/nara_ecs/src/lib.rs),
[derive implementation](../../crates/nara_ecs_derive/src/lib.rs),
[App configuration and managed frames](../../crates/nara_app/src/lib.rs),
[runtime owner](../../crates/nara_app/src/runtime.rs),
[private lifecycle admission](../../crates/nara_ecs/src/__private.rs),
[exclusive transaction guards](../../crates/nara_ecs/src/transaction.rs),
[reflection exports](../../crates/nara_reflect/src/lib.rs), and
[reflection registry](../../crates/nara_reflect/src/registry.rs).

**Inference:** an honest ECS replacement evaluation must preserve or intentionally revise these
consumer contracts. A generic ECS facade would not eliminate the work and could obscure it.
The accepted revisit trigger is a concrete unmet requirement with unacceptable public API cost,
not a dependency count or the existence of another scheduler.
[ADR 0002](../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md).

## Task Dependency And Feature Context

The current [manifest](../../Cargo.toml) and [lockfile](../../Cargo.lock) select registry Bevy ECS
0.20.0. Nara's ECS manifest retains dependency defaults. The preceding
[execution study](2026-10-21-task-execution-baseline.md) traced four product feature unions,
found no enabled `multi_threaded`, and measured the actual Nara worker allocation separately.
It also rejected an `async_executor` feature removal after its qualified experiment showed no
binary-size benefit. That report owns its graphs, measurements and limits; this study did not
rerun them.

The current lockfile has no `bevy_app`, `subsecond`, or `dioxus-devtools` package. Therefore the
Bevy hotpatch implementation discussed below is an available upstream mechanism, not an installed
Nara iteration route. Depending on ECS neither installs full Bevy App policy nor enables optional
hotpatching. Sources: [lockfile](../../Cargo.lock), [ECS manifest](../../crates/nara_ecs/Cargo.toml),
and [upstream ECS features](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/Cargo.toml).

## What Bevy Hotpatching Does

The pinned Bevy feature route connects full Bevy's `hotpatching` feature to ECS and App support.
ECS optionally depends on Subsecond; App adds `dioxus-devtools`. This is a distinct composition
from Nara's App. The pinned manifest names Subsecond `0.7.0-rc.0`, but that declaration alone
does not identify a resolved toolchain: the reference checkout has no Cargo.lock.
[Root features](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/Cargo.toml),
[App features](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/Cargo.toml),
[ECS features](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/Cargo.toml).

The traced runtime path is:

1. `HotPatchPlugin` connects to Dioxus on native targets and registers a patch notification handler.
   Its comment describes receipt of the notification after the jump table is updated. A `Last`
   system observes the channel, emits `HotPatched`, and marks `HotPatchChanges` changed. Optional
   automatic reflection registration runs separately.
2. Executors compare that resource's change tick with the system's last run and refresh system
   pointers. Observer and registered-system execution also have refresh paths.
3. `FunctionSystem` retrieves parameters from its existing stored parameter state and calls
   `HotFn::try_call_with_ptr` using its stored pointer. A failed hot call panics with a request
   for a full rebuild. `refresh_hotpatch` updates the pointer; it does not reconstruct the World,
   system parameter state, or captured function value.

Sources: [HotPatchPlugin](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_app/src/hotpatch.rs),
[single-threaded executor](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/executor/single_threaded.rs),
[multi-threaded executor](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/executor/multi_threaded.rs),
[observer runner](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/observer/runner.rs),
[registered systems](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/system/system_registry.rs), and
[FunctionSystem](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/system/function_system.rs).
The locked registry 0.20.0 `FunctionSystem` was separately checked and has the same stored-state,
pointer-refresh and failed-call behavior.

The upstream example demonstrates system-body text/color edits, observer patching, and an explicit
hot call inside a background thread. It demonstrates that such calls can be placed outside systems;
it does not retire that thread before patch application or demonstrate arbitrary state migration.
[Hotpatching example](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/examples/ecs/hotpatching_systems.rs).

**Inference:** these entry-point refresh mechanisms are useful integration evidence. They do not
establish Nara's stronger guarantee that all participating schedules, tasks and callbacks retire
before a patch commits. A notification observed after jump-table update is not an admission barrier.
The multi-threaded executor additionally documents a possible undefined-behavior case if a system
mutates `HotPatchChanges`; its comment restricts the feature to development. This is a specific
upstream caveat, not a claim that ordinary Nara ECS execution is unsound.
[Executor caveat](https://github.com/bevyengine/bevy/blob/85503941e488152711a4695bbe248e9ed9f453fc/crates/bevy_ecs/src/schedule/executor/multi_threaded.rs#L433-L449).

## Subsecond Constraints Relevant To A Prototype

Retrieved Subsecond 0.7.10 documentation describes explicit jump-table call redirection with
external compiler/protocol tooling. It still calls the route experimental. It documents patching
only the entry binary's crate, with a warning for `main.rs` forwarding to its own library. It
does not migrate changed struct layouts; frameworks must discard or reconstruct affected state.
Static initializer edits are not observed, new global destructors do not run, and patched-crate
thread-local behavior has documented crash risks. These restrictions are version-specific
documentation claims, not Nara measurements.
[Subsecond 0.7.10 crate documentation](https://docs.rs/subsecond/0.7.10/subsecond/).

The unsafe `HotFn::try_call_with_ptr` contract separately requires unchanged argument layouts.
`ptr_address` can aid a framework's state-validity tracking, but Subsecond does not maintain that
state history for the framework. The checked 0.7.0-rc.0 documentation exposes the same layout
precondition.
[HotFn 0.7.10](https://docs.rs/subsecond/0.7.10/subsecond/struct.HotFn.html),
[HotFn 0.7.0-rc.0](https://docs.rs/subsecond/0.7.0-rc.0/subsecond/struct.HotFn.html).

The [two-level consumer manifest](../../samples/two-level-game/Cargo.toml) declares a library
and small desktop/headless binaries; game modules live under that library. **Inference:** a
prototype must first prove that the intended gameplay edit belongs to the tool's actual patch
scope. Merely running an existing Nara binary through `dx` would not establish that the library
gameplay or engine crate changes are patched. A disposable tip-crate fixture is a smaller first
experiment than restructuring production packages around an experimental limitation.

## Bounded Next Questions

- Measure the complete rebuild/resume consumer before choosing a faster path. Separate compile,
  link/publication, process activation, state admission and useful presentation.
- If patching is prototyped, pin the toolchain and resolved versions. Begin with a coarse callback,
  compatible body edits, explicit unsupported-edit rejection, and task/callback retirement. Keep
  layout, signature, feature/dependency, static/TLS and unknown changes on fresh-runtime rebuild.
- Test captured state, system parameter state, observers, background callbacks, repeated patches,
  manual restart, failure and memory growth. ECS schema equality alone cannot establish the
  compatibility of arbitrary native state.
- Evaluate optional scripting only against a concrete consumer. Neither ECS replacement nor a
  scripting VM is required by the evidence gathered here.

These are investigation boundaries under
[ADR 0093](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md),
not new acceptance requirements or a new implementation plan.

## Evidence Limits

No Rust code, features, dependencies, ADRs or runtime behavior changed. No Cargo command, test,
live patch or latency/memory benchmark ran for this study. Source inspection cannot qualify a
platform/toolchain or prove arbitrary unsafe patch calls sound. The web search/open tool returned
gateway errors twice; versioned docs.rs pages were then retrieved directly over the configured
local proxy. One console read initially hit a Windows output-encoding error; the relevant crate
page was reread with UTF-8 output. Pinned Bevy development source, locked registry ECS source and
versioned Subsecond documentation are distinguished above rather than treated as one resolved
tested toolchain.
