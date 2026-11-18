# Managed Runtime Runners

An embedded or external Rust host can own a concrete managed-runtime loop through the public
`nara::app` API. This is an advanced code-first integration surface. Ordinary projects should use
the first-party product Hosts unless they have a real embedding requirement.

The host owns its loop and selects its platform/event integration before it builds the runtime. Nara
does not provide a universal `Runner` trait, factory, or service registry for this purpose.

## Construction

Configure an `App`, then transfer it to `RuntimeInstance::start`. The runtime owns preparation,
admission, startup and physical close. The ordinary caller does not seal or promote candidates.

```rust
use nara::app::{App, RuntimeInstance, RuntimeStartFailure};

fn start_runtime() -> Result<RuntimeInstance, RuntimeStartFailure> {
    let app = App::new();
    // Configure the embedding application's plugins, resources and systems here.
    RuntimeInstance::start(app)
}
```

Use `start_with_options(app, obligations, close_policy)` when the embedding Host has its own
close obligations or requires a different finite close policy. Startup failure returns
`RuntimeStartFailure`, which retains the error, close evidence and physical retirement owner.
Keep that value in the Host's failure state and poll `drive_retirement` on its constructing thread
until `retirement_state` is `Retired`. Preserve any terminal plugin-shutdown failure evidence.

These start methods synchronously configure and start the runtime. A failed start also performs
one finite close attempt before returning its owner. They are not nonblocking UI startup APIs.
The first-party product Hosts own responsive project preparation and staged publication.

`RuntimeTimeSettings::with_paused(true)` configures an initially paused Runtime. Startup systems
may also set the final pause value. Successful start preserves that value and exposes matching
`RuntimeState::Paused` or `Running` immediately; promotion does not reset the game clock policy.
An initially paused Runtime accepts `StepFixedTick` and `Resume` through the ordinary control
queue. Paused drives still maintain real-time tasks, rendering and cleanup without accumulating
simulation time. After startup, use Runtime controls to keep lifecycle state and time policy
in agreement rather than treating the settings resource as a second control queue.

Use `drive_elapsed` for wall time already sampled by a native runner. A Resume applied to an
interval that began paused settles that interval without virtual advancement.
`RuntimeFrameStatus::simulation_paused` reports this actual settlement; requested settings can
already say running. Transform presentation uses the settled fact, so an exact Step followed
immediately by Resume shows the current pose without an extra maintenance call. Explicit
`drive` keeps its next-interval semantics. Exact Step advances one simulation tick, reports
`simulation_paused == false`, preserves the remainder and does not run Extract.

If a system removes a resource that App still needs in the frame, the drive returns its existing
missing-resource fault. Stop and physical retirement remain available; work already completed in
that frame is not undone. Removing a resource after its final frame use does not trigger another
clock audit or automatically recreate it.

An exact-tick `HeadlessRun` performs one paused maintenance frame before its requested fixed
transactions, including when Startup already paused the Runtime. This entry frame lets real-time
tasks publish readiness without running variable gameplay or advancing simulation. An exit from
that frame ends the run before any fixed tick. Ordinary frame-mode runs execute only their
requested complete frames; they do not receive an additional exact-mode entry frame.
Readiness is still required before stepping. Use frame mode when initialization needs further
service passes rather than assuming that one maintenance frame completes asynchronous work.

Construct the App on the thread that will drive and retire it. App, sealed candidates, runtime
instances and failure/retirement owners are non-Send/non-Sync because their World may retain
thread-affine or NonSend state. Transfer a replayable recipe or definition to another thread and
construct a fresh App there. Control values and fault reporters remain transportable; they do not
transfer executable ownership. Dropping an incomplete owner transfers its retained obligations
to quarantine rather than proving a clean stop. Drain quarantine on the constructing thread.

## Automatic Execution Policy

`App::new()` explicitly selects serial execution for every engine-driven Startup, Core and
private cleanup schedule, even when dependencies enable Bevy multithreading. Custom schedules
keep their author's explicit executor. NonSend systems and physical Runtime retirement remain
on the managed driver under either automatic profile.

Native code-first Hosts may enable Nara's `parallel-executor` feature and call
`app.set_execution_profile(ExecutionProfile::Parallel)` during configuration. Before starting
or sealing, initialize the existing process-global `bevy_tasks::ComputeTaskPool` with the Host's
selected positive worker count. Use the matching Bevy 0.20 dependency; Nara neither creates nor
resizes that pool. An existing pool keeps the initialization selected by its original Host.
Unavailable build/target/pool requests return `ExecutionProfileError` through the ordinary
pre-Startup failure owner. Feature availability alone never changes a Runtime's selected policy.

The selection freezes before Finish/Startup. All automatic schedules use the original Bevy
executor under Nara's managed error, deferred-command, fixed-time and physical-close contracts.
Parallel peer execution does not order unrelated systems; declare real domain dependencies and
join borrowed solver work before publishing its poses. Runtime close does not destroy the
process-global compute facility. Cross-frame Tasks and solver worker budgets remain separate.

`app.execution_profile()` and `runtime.execution_profile()` observe the same retained selection.
`app.configuration_fingerprint()` now returns `AppConfigurationFingerprint`, which includes
that profile and the exact plugin plan. Use `app.plugin_plan_fingerprint()` when comparing the
original `PluginPlanFingerprint`; the two identity types have separate meaning. Arbitrary Rust
systems, resources, binary provenance and physical worker counts are not attested by either
digest. Schedule policies configured outside a plugin's encoded definition also need their own
compatibility inventory. The new App digest intentionally differs from the former plugin-plan-only
digest even for Serial; persisted compatibility records must be recaptured. A reconstruction workflow must
retain its own complete compatibility inventory.

The [joint motion unit](../plans/2026-11-03-joint-motion-execution-plan.md#explicit-production-executor-successor)
owns actual profile qualification. Building the feature does not establish repeatability,
solver-internal parallelism, foreign-platform support or a performance improvement. This is a
code-first App selection; it adds no persisted project setting or product-recipe default change.

Product preparation reports an unavailable framework profile as
`project.run.execution-profile-unavailable`, with static `execution-profile` and
`execution-profile-reason` fields. A plugin that returns this typed error retains its actual
invocation's plugin/hook and lifecycle diagnostic context. A policy refusal does not attribute
the framework's admission check to an unrelated plugin.

## Fixed Spatial Input

For a domain that needs same-tick spatial input before simulation, register readers through
`nara::transform::add_fixed_transform_input_systems`. The opt-in
`FixedInputSpatialSnapshot2d` SystemParam and `fixed_input_spatial_snapshot(&World)` expose
the original Transform owner's completed Prepare input. They include captured local values,
structural parent identity, effective parent frame, global affine values and the original stamp.
Declare the domain's additional ordering against its own target/solve sets. Do not walk parents
or substitute a live local/identity when input is unavailable.
Opt-in worlds retain a separate input payload/cache and perform the original validated source
scan at this extra boundary. Unchanged matrix payloads can be reused, but registration is not
a zero-cost promise; numeric frame/memory budgets need separate measurement.

This role has its own World/Runtime/tick/frame validation. It does not replace
`HistoricalSpatialSnapshot2d`, the after-simulation `FixedSpatialSnapshot2d`, or presentation.
An input is a frozen cut; mutations after it belong to later completion. A native solver must
validate its supported pose/shape domain and publish its own writeback before the ordinary
fixed-result barrier. It must also revalidate its live mutation targets; an earlier captured
root cannot authorize writing a root pose after live reparenting. Presented `local()` describes
the captured source local, while presented `global()` may contain the owner's interpolated
affine result; do not infer an interpolated local pose by treating them as the same value.
A visual affine is not automatic permission for dynamic-body scale or
parented writeback. The
[transformed collision workflow](../plans/2026-11-04-authored-collision-derivation-plan.md#fixed-spatial-inputs-and-transformed-collision-successor)
records the concrete consumer and qualification limits.

## Control And Shutdown

Use `request_control` and `drive` for pause, resume, exact fixed-tick stepping, Stop, and retryable
close. A request is accepted separately from the later operation result; inspect its ticket with
`control_status` after a drive. Stop is finite only when the runtime reaches `RuntimeState::Stopped`.

Keep the ticket for the exact operation being observed. The original Stop result remains available
for that Runtime's lifetime, independently of bounded ordinary control history. Reissuing Stop
where admitted returns the same ticket. Each `RetryClose` has its own ticket: a later successful
retry does not replace an earlier Stop's `Failed(CloseIncomplete)` result. Observe the retry and
the original close evidence to distinguish that operation from eventual physical completion.
Completed ordinary results may expire; `None` also rejects a ticket from another Runtime generation.
An absent observation proves neither Pending nor success and grants no cleanup authority.

Use `drive(delta)` when the Host supplies the next interval explicitly, including finite headless
or controlled simulation. Native wall-clock runners use `drive_elapsed(delta)` for an interval
that already elapsed under the previous state. If it entered paused and Resume applies, that
single drive completes the original paused App transaction: RealTime includes the wait, virtual
time and fixed pending phase do not advance, and the returned state is Running. The next native
interval runs normally. No extra App frame or sampling reset is introduced; Stop still produces
no frame, and an exact step retains its original one-tick policy. A control's Applied result
does not erase a fault in the ensuing App transaction.

Native cadence is separate from Runtime pause. A live `WinitSession` or `DesktopRun` can select
`set_control_flow(WinitControlFlow::Wait)` for a known workload that needs frames only for
native window/input demand or explicit operations. Existing pending demand and presentation
maintenance deadlines remain active. Select `Poll` for continuous App stages and rendering;
returning from Wait requests original frame work without resetting the elapsed-time baseline.
Repeated selections leave demand unchanged, and faulted or retiring sessions refuse changes.

`WinitSession::maintenance_wake()` and `DesktopRun::maintenance_wake()` return a cloneable,
thread-safe hint for that original live native session. Call `notify()` after recording work
in the consumer's own bounded state. It wakes native maintenance without requesting an App
frame, resetting elapsed time or changing paused capture eligibility. A successful return
means queued or coalesced, not completed work. Read and validate the original work at the
consumer boundary; the handle carries no payload or Runtime/window authority.

Shutdown, faults, Drop or native delivery refusal revoke every clone. The physical event loop
does not reuse session wake identities, so a stale handle cannot notify a successor session.
Keep a reconciliation policy where the external source can lose notifications. The reference
game's private control-directory watcher uses a 250 ms ceiling and joins before Closed;
see its [development Host guide](../../samples/two-level-game/README.md#native-request-notifications).

Wait does not discover arbitrary World mutations, automatically wake for background task completion, or
animate shaders using real time. Task results still need a later complete App frame for
integration. The caller must keep Poll, explicitly request paused maintenance while its work
is pending, or accept deferring that work until a native event or explicit operation. Pause
does not automatically choose Wait, and Wait while Running still uses the original elapsed
time caps and fixed catch-up policy on the next frame.

A presentation observer's `WakeAt(deadline)` requests maintenance without an App frame.
`Continue` supplies no observer timer. Original submitted GPU work, first-present/READY and
telemetry sealing retain their own wake requirements independently; see the
[desktop observer guide](rust-product-recipes.md#native-desktop-execution).

`capture_time_continuation()` requires a healthy full paused frame and no pending control.
Its portable value records actual timestep, caps, catch-up policy, scale, maximum delta and
pending virtual time. It contains no prior Runtime/frame/tick identity. Construct a value from
validated settings with `RuntimeTimeContinuation::new`, then apply it through
`restore_time_continuation(value)` at the receiving paused boundary. Policy must match exactly.
The owner preserves local time history and derives interpolation from pending time. Complete
another full paused frame before capture; exact stepping does not replace that maintenance.

An ordinary rejection changes no time. `RuntimeTimeContinuationError::Faulted` retains whether
initialization already applied and any refused output separately from its original sticky fault.
The game still owns full state eligibility, service classification and its logical clock.

For a domain that must wait for admitted background work, use `TaskPools::return_state()`.
`Pending` includes cancelled queued captures and executor-owned disposal after the terminal
has been consumed or the handle dropped. Once those original returns complete, `CleanupFailed`
still distinguishes caught cleanup failure from `Returned`. Diagnostic zero queued/running
counts cannot supply this evidence. A root facade observes its whole physical facility;
an activity facade observes its own admitted work and excludes sibling task failure.

This snapshot drives no work or close and freezes no future admission. It does not join
workers, retire published outputs or classify a game checkpoint. Preserve existing domain
eligibility checks and native/service owners. The reference game's capture refuses pending
returns and cleanup failure; its finite child waits within the original operation deadline,
and original Close still records failure separately from physical completion.

For a finite headless or one-shot loop, `finish` consumes the runtime and performs one finite close
attempt. Success returns `RuntimeCloseEvidence`. An incomplete attempt or terminal shutdown failure
returns `RuntimeRetirement`; retain it and poll `drive_retirement`, inspecting both its state and
close evidence. `Retired` describes physical completion and does not erase a reported shutdown error.
An interactive Host should drive Stop/RetryClose through its existing event loop instead of
blocking that loop in `finish`.

App is configuration-only. Platform adapters translate their events and elapsed time into the
concrete runtime's public control and drive operations. Hidden driver ports and mutable runtime
World access are first-party plumbing, not external runner integration APIs.

## Sequential Native Runs

Winit permits one physical event loop per native process. Keep that physical owner when a
process needs sequential logical runs. `WinitRunner::run_on(event_loop, runtime)` consumes an
explicit `WinitEventLoop` for a platform-only Runtime. `WinitRetiredSession::event_loop` returns
it only after the session's Runtime and native dependencies physically retire. Ordinary
presentation products use `DesktopRun` instead of the platform-only runner.

For desktop products, recover the loop with `DesktopRun::take_event_loop()` after a terminal
report, then pass it to the next action through `DesktopRunOptions::with_event_loop(loop)`.
Incomplete cleanup returns no loop and must continue through the same action. A loop supplied
before a failed source admission remains recoverable after that action reaches its terminal
failure. Keep every operation on the loop's constructing thread.

Each action creates its own Runtime generation, content admission, windows and presentation Host.
Loop reuse transfers none of those authorities and does not allow live Source/Processed content
to bypass Schema agreement or residency publication. The independent
[composition consumer](../../tests/fixtures/render-composition-consumer/README.md) exercises
the normal Source then Processed actions and separately replays their retained CPU inputs.

## First-Party Staged Publication

Engine-owned Hosts use hidden `RuntimeAdmissionReservation`, sealed-App and candidate plumbing to
retain an unpublished runtime across Host preparation and publication phases. Those stages preserve
the same runtime owner and close contract. They are not a second execution lifecycle or additional
steps for an ordinary external embedding. See
[ADR 0117](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md) for the
configuration/managed-execution boundary.
