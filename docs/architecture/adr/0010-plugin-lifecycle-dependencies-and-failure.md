# ADR 0010: Plugin Lifecycle, Dependencies, and Failure Containment

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-07-15
**Refined By**: ADR 0040: Render Resource Lifetime and Submitter Ownership; ADR 0046:
Plugin Metadata and Default Plugin Groups

## Context

nara owns its application and plugin lifecycle. Plugins are trusted Rust engine modules that install
systems, resources, codecs, services, and backend adapters. They are not sandboxed project
extensions. A plugin hook may fail or panic after mutating the world or schedules, so treating every
failure as retryable can run a partially configured application. The previous `plugins_finished`
boolean also allowed a failed `finish` pass to discard shutdown hooks and report success on the next
call.

The lifecycle must preserve the first poison-causing attempt/lifecycle failure, prevent schedule
execution after that failure, and retain enough ownership to shut down every committed plugin exactly
once. Expected setup conflicts must remain ordinary structured errors rather than panics.

## Decision

`App` uses the following explicit lifecycle:

```text
Configuring -> Finishing -> Ready -> ShuttingDown -> ShutdownComplete
      |            |                        ^
      +------------+----> Poisoned ---------+
                           |         ^
                           +---------+
                    shutdown returns to Poisoned
```

`Poisoned` is terminal for build, finish, mutation, and run entry points. Read-only inspection and
explicit shutdown remain available. Shutdown of a poisoned app returns to `Poisoned`, with
`shutdown_complete = true`; it never makes the app runnable again.

### Hook Classification

- `Plugin::declaration()` is a static, type-owned declaration available without constructing an
  instance. Replayable definitions capture it before factory invocation; direct callers may have
  already constructed their one-shot instance. Data-only plugin-group expansion, slot/dependency
  resolution, and repeatable-factory preparation where applicable execute before App mutation or
  native authority acquisition. Their typed errors are plan or preparation failures, not plugin
  lifecycle failures, and do not poison an App.
- `preflight(&PluginPreflightContext)` executes immediately before its plugin is committed. Its
  narrow context exposes immutable installed-plan/structural snapshots, not `World`, arbitrary
  resources, runner mutation, or native authority. A typed `Err` is retryable only when this pure
  contract is obeyed and the current top-level plan commit has not already committed an entry. If an
  earlier entry has built, the enclosing attempt is already partially committed and the App is
  poisoned even though the rejecting plugin never became a shutdown owner. A preflight unwind always
  poisons a caller-owned App or marks the retained product-construction owner failed/unpublishable
  regardless of position. Before a sealed runtime candidate exists, that owner is still the start
  attempt holding the partial App, prepared owners, and obligation ledger; it remains retained
  through terminal shutdown.
- `build(&mut App)` and `finish(&mut App)` are committed on entry. Their `Err` or unwind panic
  poisons a caller-owned App or marks the retained product-construction owner failed/unpublishable,
  records the first failure, and triggers shutdown after the outermost active hook returns. The
  start attempt owns a partial App until a later layer successfully seals and admits it as a runtime
  candidate; whichever owner exists remains retained until shutdown reaches an observable terminal
  result.
- The product path may use a private one-attempt prepared plugin transfer after all repeatable
  factories succeed. The transfer preserves each complete admitted definition key; typed factory
  erasure fixes the concrete plugin type but does not claim to prove that trusted code used every
  config field. It is not a public generic `install_into(App)` API, owns no App/native authority,
  and is not permission to run mutable hooks. Arbitrary mutable preparation callbacks remain
  invalid.
- Hook panics are isolated with `catch_unwind` and converted to stable `PluginError` values. Every
  hook unwind poisons the caller-owned App or marks the retained product-construction owner failed.
  This containment applies only to `panic = "unwind"`; aborting builds cannot run Rust shutdown
  code and make no in-process recovery guarantee.

The first poison-causing attempt/lifecycle failure is immutable. Later shutdown errors and shutdown
panics are appended to `PluginFailureReport::shutdown_failures`; they never replace the primary
error and never stop later shutdown hooks.

### Installation and Dependency Rules

- Top-level `App::add_plugins` accepts direct plugin instances, data-only groups, edited groups, and
  tuples through one sealed collection Interface. Collection, complete closure validation,
  deterministic ordering, and repeatable-factory preparation finish before lifecycle commit.
- For a caller-owned App configured through several top-level calls, the actual committed order is
  an immutable prefix. A later plan may append entries and depend on that prefix; any edit or order
  edge that would disable, replace, reconfigure, or move a committed entry is rejected before
  mutation.
- New plan/group/provenance snapshots and provenance merged onto the immutable prefix remain staged
  until the attempt's first plugin commit. An earlier typed preflight rejection discards all staged
  inspection changes. Once commit begins, actual committed entries and attempt evidence remain
  inspectable on failure, but an installed-group snapshot publishes only after the complete group
  succeeds. A fully validated batch with no new plugin suffix may atomically publish matching
  group/provenance inspection metadata without entering the plugin lifecycle.
- A plugin is retained for shutdown before its `build` hook is entered. Finish follows resolved
  order, and shutdown uses the strict reverse of actual committed order.
- Stable plugin/group IDs, inactive nested-group expansion, and the ordering graph reject recursive
  dependency cycles with complete bounded stable-ID chains before App mutation.
- A resolved plan contains one record per plugin/group ID. Duplicate plugin occurrences converge
  only under ADR 0046's occurrence/definition-key equality; duplicate groups converge only under
  the same intrinsic group-definition fingerprint. Divergent duplicates reject. Declared
  prerequisites, capabilities, and conflicts are checked before custom preflight; a conflict
  declared by either the new or an installed plugin rejects the pair independent of installation
  order.
- `PluginGroup::build` produces a data-only `PluginGroupBuilder`. The builder records stable slots,
  repeatable definitions, nested groups, and order intent; it has no `App`, `finish(App)`, native
  authority, or public installation callback. Group membership and order derive from the resolved
  entry collection rather than a parallel static plugin-ID list.
- `Plugin::build` and `Plugin::finish` may never add a plugin or group. An attempted nested install
  is a sticky committed contract violation that poisons the App even when the plugin ignores the
  returned error. Dependencies belong in declarations and selected groups; nested groups expand
  only during pure collection.
- Every public plugin/group installation entry checks for an active hook and records this sticky
  violation before lifecycle-state, duplicate/already-installed, declaration, group expansion, or
  skip logic. No apparent no-op or earlier error may bypass the poison rule.
- Built-in component plugins inspect an existing `ComponentRegistry` during preflight with the same
  stable-ID and Rust-type uniqueness checks used at commit. Registration remains fallible and maps
  `ComponentRegistryError` to a plugin/component-contextual `PluginError`; no recoverable
  registration path uses `expect`.

### Mutation and Shutdown Ownership

- Public mutable app entry points return `Result` and return the preserved primary plugin error when
  the app is poisoned. `App::update` is the only zero-delta convenience update and is fallible;
  the panic-based update path and `try_update` split are removed.
- `Plugin::shutdown` is fallible and receives a narrow `PluginShutdownContext` rather than
  `&mut App`. Shutdown can access the world to release owned resources but cannot add plugins,
  schedules, or a runner.
- Shutdown is reverse-order, best-effort, panic-isolated, and once-only. A hook is marked attempted
  before invocation, so a shutdown panic cannot cause a second call during `Drop`.
- Lifecycle-control reentry and plugin/group installation from committed build or finish hooks are
  rejected. No intentional committed-hook nesting remains.
- `App::set_runner` is likewise rejected as a sticky violation from plugin hooks. Top-level
  code-first callers or concrete Host/driver Adapters own runner selection; product plugins do not.
- `Drop` performs best-effort shutdown without unwinding, including when another panic is already
  unwinding. Callers that need shutdown evidence use `App::shutdown_plugins` or `App::run`.

`shutdown` is deliberately not named `cleanup`. In Bevy, `Plugin::cleanup` is a startup hook that
runs after `finish` and before the first update; nara's hook is terminal reverse-order teardown.
Nara also has a per-frame `CoreStage::Cleanup`. Reusing `cleanup` for all three meanings would make
plugin lifecycle code ambiguous to both Bevy users and nara maintainers.

### Runner Ownership

`RunnerFn` borrows `&mut App`; it does not consume the app. `App::run` therefore regains control when
the runner exits and performs explicit shutdown. If running and shutdown both fail,
`AppRunError::Shutdown` preserves the prior run error and the separate plugin failure report. A
runner that has its own fallible teardown uses `AppRunError::RunnerTeardown` to preserve the prior
run error and the distinct teardown error without pretending either is a plugin failure. If plugin
shutdown also fails, `AppRunError::Shutdown` remains the outer error and retains that combined runner
error as its prior cause. A finish failure is returned with its inspectable report before any runner
executes.

## Alternatives Considered

### Single fallible `build(&mut App)` with no terminal state

Rejected. An error cannot prove that arbitrary earlier mutations were rolled back, and retrying or
running would expose a partially configured app.

### Roll back arbitrary world and schedule mutation

Rejected. Native resources and external services are not generally cloneable or transactional.
Explicit ownership and reverse shutdown are honest; implicit snapshot rollback is not.

### Keep full `&mut App` access during group build and shutdown

Rejected. It permits mutations with no retained shutdown owner. Narrow builders/contexts make the
ownership rule enforceable by the Rust API.

### Permit nested installation only for direct code-first Apps

Rejected. The same plugin would have different dependency and failure semantics when used directly
versus through a package or product plan. Groups, tuples, and static declarations preserve concise
code-first composition without retaining an open committed hook.

### Fully asynchronous plugin lifecycle

Deferred. Device, window, and importer startup do not justify forcing every plugin hook to be async.
Long-running services remain behind runners, task pools, and explicit backend state machines.

## Implementation Notes

- Canonical types are `PluginLifecycleState`, `PluginHook`, `PluginFailure`,
  `PluginFailureReport`, `PluginShutdownContext`, `PluginGroupBuilder`, `PluginPlanError`,
  `PluginPrepareError`, and the private `PluginCommitBatch` transfer.
- Static declaration and group-definition failures are plan/preparation failures, not lifecycle
  hooks. They do not use `PluginHook::Metadata` or `PluginFailureSubject::Group`; those transitional
  variants are removed when U4 replaces the imperative group path.
- Plugin declarations and resolved group provenance remain stable and inspectable after successful
  installation. Group definition/plan failure publishes no installed group. If lifecycle commit
  later fails, actual committed entries remain inspectable on the terminal App.
- Shutdown-only failures use a report with no primary setup failure. `shutdown_complete` means every
  retained hook was attempted, not that every hook succeeded.
- Runtime diagnostics may later bridge failure reports, but logs are not lifecycle authority.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Preflight boundary | Pure typed rejection before the attempt's first commit is retryable; later rejection or any unwind poisons | `nara_app` plan-commit lifecycle tests |
| Committed failure | Build/finish error or panic permanently prevents schedules | `nara_app` lifecycle tests |
| Shutdown ownership | Every committed hook runs once in reverse order | shutdown order and unwind tests |
| Failure fidelity | Primary error survives shutdown errors/panics | failure-report tests |
| Dependency safety | Plugin/group cycles terminate with stable chains | cycle tests |
| Closed commit | Hook-time nested installation always poisons and never installs a hidden entry | ignored-error contract tests |
| Group purity | Group/slot/dependency rejection leaves App fingerprints unchanged | plan fault tests |
| Incremental direct order | Later top-level additions append or reject before mutation; committed prefix never reorders | direct App order tests |
| Early preflight staging | Retryable rejection leaves installed entry/group/provenance snapshots byte-for-byte unchanged | snapshot fault tests |
| Metadata-only batch | Matching empty-suffix group/provenance changes publish atomically or leave inspection unchanged | direct App snapshot tests |
| Driver authority | Plugin hooks cannot set/replace the runner | ignored-error and static boundary tests |
| Runner shutdown | Prior runner, runner teardown, and plugin shutdown failures remain separately observable | runner shutdown and teardown aggregation tests |
| Built-in registration | Duplicate component registration is a contextual error, not panic | component plugin tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---:|---:|---|
| Plugin ignores a forbidden nested installation error | High | Medium | The attempt records a sticky violation before returning; outer success cannot clear it |
| Shutdown itself fails | High | Medium | Mark attempted first, aggregate separately, continue reverse shutdown |
| Panic abort configuration skips shutdown | High | Low | Document unwind-only containment; process supervision handles abort builds |
| Compatibility wrappers preserve unsafe entry points | High | Medium | Pre-1.0 breaking migration removes old signatures and stale callers |
