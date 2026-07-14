# ADR 0010: Plugin Lifecycle, Dependencies, and Failure Containment

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-07-10
**Refined By**: ADR 0040: Render Resource Lifetime and Submitter Ownership; ADR 0046:
Plugin Metadata and Default Plugin Groups

## Context

nara owns its application and plugin lifecycle. Plugins are trusted Rust engine modules that install
systems, resources, codecs, services, and backend adapters. They are not sandboxed project
extensions. A plugin hook may fail or panic after mutating the world or schedules, so treating every
failure as retryable can run a partially configured application. The previous `plugins_finished`
boolean also allowed a failed `finish` pass to discard cleanup hooks and report success on the next
call.

The lifecycle must preserve the first committed failure, prevent schedule execution after that
failure, and retain enough ownership to clean every committed plugin exactly once. Expected setup
conflicts must remain ordinary structured errors rather than panics.

## Decision

`App` uses the following explicit lifecycle:

```text
Configuring -> Finishing -> Ready -> Cleaning -> Cleaned
      |            |                    ^
      +------------+----> Poisoned -----+
                           |     ^
                           +-----+
                    cleanup returns to Poisoned
```

`Poisoned` is terminal for build, finish, mutation, and run entry points. Read-only inspection and
explicit cleanup remain available. Cleanup of a poisoned app returns to `Poisoned`, with
`cleanup_complete = true`; it never makes the app runnable again.

### Hook Classification

- `metadata` and `preflight(&App)` execute before a plugin is committed. `preflight` only receives a
  shared app reference. Its ordinary `Err`, or an isolated panic in either pre-mutation hook, is
  retryable and does not create a failure report.
- `build(&mut App)` and `finish(&mut App)` are committed on entry. Their `Err` or unwind panic
  poisons the app, records the first failure, and triggers cleanup after the outermost active hook
  returns.
- nara does not currently expose a prepared-but-uncommitted token. A future preparation API may be
  retryable only if its type owns a registered teardown action and proves that no world, schedule,
  service, or external mutation was committed. Adding such a token requires an ADR revision and a
  real consumer. Arbitrary mutable hooks never receive that classification.
- Hook panics are isolated with `catch_unwind` and converted to stable `PluginError` values. This
  containment applies only to `panic = "unwind"`; aborting builds cannot run Rust cleanup code and
  make no in-process recovery guarantee.

The first committed failure is immutable. Later cleanup errors and cleanup panics are appended to
`PluginFailureReport::cleanup_failures`; they never replace the primary error and never stop later
cleanup hooks.

### Installation and Dependency Rules

- A plugin is retained for cleanup before its `build` hook is entered. Successful nested dependency
  builds are ordered before the dependent plugin for finish, and cleanup uses the reverse order.
- Stable plugin and plugin-group installation stacks reject recursive dependency cycles with the
  complete stable-ID chain instead of recursing indefinitely.
- Unique plugin IDs and group IDs are rejected before mutation. Declared prerequisites,
  capabilities, and conflicts are checked before custom preflight; a conflict declared by either
  the new or an installed plugin rejects the pair independent of installation order.
- `PluginGroup::build` receives `PluginGroupBuilder`, not `&mut App`. A group may only compose
  plugins or other groups; it cannot create unowned world or schedule mutations. Group build is
  committed on entry because members may already have installed when a later member fails.
- Built-in component plugins inspect an existing `ComponentRegistry` during preflight with the same
  stable-ID and Rust-type uniqueness checks used at commit. Registration remains fallible and maps
  `ComponentRegistryError` to a plugin/component-contextual `PluginError`; no recoverable
  registration path uses `expect`.

### Mutation and Cleanup Ownership

- Public mutable app entry points return `Result` and return the preserved primary plugin error when
  the app is poisoned. `App::update` is the only zero-delta convenience update and is fallible;
  the panic-based update path and `try_update` split are removed.
- `Plugin::cleanup` is fallible and receives a narrow `PluginCleanupContext` rather than `&mut App`.
  Cleanup can access the world to release owned resources but cannot add plugins, schedules, or a
  runner.
- Cleanup is reverse-order, best-effort, panic-isolated, and once-only. A hook is marked attempted
  before invocation, so a cleanup panic cannot cause a second call during `Drop`.
- Lifecycle-control reentry from committed build or finish hooks is rejected. Nested plugin/group
  installation remains supported and is the only intentional committed-hook nesting.
- `Drop` performs best-effort cleanup without unwinding, including when another panic is already
  unwinding. Callers that need cleanup evidence use `App::cleanup_plugins` or `App::run`.

### Runner Ownership

`RunnerFn` borrows `&mut App`; it does not consume the app. `App::run` therefore regains control when
the runner exits and performs explicit cleanup. If running and cleanup both fail,
`AppRunError::Shutdown` preserves the prior run error and the separate plugin failure report. A
runner that has its own fallible teardown uses `AppRunError::RunnerTeardown` to preserve the prior
run error and the distinct teardown error without pretending either is a plugin failure. If plugin
cleanup also fails, `AppRunError::Shutdown` remains the outer error and retains that combined runner
error as its prior cause. A finish failure is returned with its inspectable report before any runner
executes.

## Alternatives Considered

### Single fallible `build(&mut App)` with no terminal state

Rejected. An error cannot prove that arbitrary earlier mutations were rolled back, and retrying or
running would expose a partially configured app.

### Roll back arbitrary world and schedule mutation

Rejected. Native resources and external services are not generally cloneable or transactional.
Explicit ownership and reverse cleanup are honest; implicit snapshot rollback is not.

### Keep full `&mut App` access during group build and cleanup

Rejected. It permits mutations with no retained cleanup owner. Narrow builders/contexts make the
ownership rule enforceable by the Rust API.

### Fully asynchronous plugin lifecycle

Deferred. Device, window, and importer startup do not justify forcing every plugin hook to be async.
Long-running services remain behind runners, task pools, and explicit backend state machines.

## Implementation Notes

- Canonical types are `PluginLifecycleState`, `PluginHook`, `PluginFailure`,
  `PluginFailureReport`, `PluginCleanupContext`, and `PluginGroupBuilder`.
- Plugin and group metadata remain stable and inspectable after successful installation. A failed
  group is never published as installed; already committed member metadata remains inspectable on
  the terminal app.
- Cleanup-only failures use a report with no primary setup failure. `cleanup_complete` means every
  retained hook was attempted, not that every hook succeeded.
- Runtime diagnostics may later bridge failure reports, but logs are not lifecycle authority.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Preflight retry | Rejection leaves `Configuring` app unmodified | `nara_app` lifecycle tests |
| Committed failure | Build/finish error or panic permanently prevents schedules | `nara_app` lifecycle tests |
| Cleanup ownership | Every committed hook runs once in reverse order | cleanup order and unwind tests |
| Failure fidelity | Primary error survives cleanup errors/panics | failure-report tests |
| Dependency safety | Plugin/group cycles terminate with stable chains | cycle tests |
| Runner shutdown | Prior runner, runner teardown, and plugin cleanup failures remain separately observable | runner cleanup and teardown aggregation tests |
| Built-in registration | Duplicate component registration is a contextual error, not panic | component plugin tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---:|---:|---|
| Plugin ignores a nested installation error | High | Medium | Nested failure poisons the app independently; outer success cannot publish it |
| Cleanup itself fails | High | Medium | Mark attempted first, aggregate separately, continue reverse cleanup |
| Panic abort configuration skips cleanup | High | Low | Document unwind-only containment; process supervision handles abort builds |
| Compatibility wrappers preserve unsafe entry points | High | Medium | Pre-1.0 breaking migration removes old signatures and stale callers |
