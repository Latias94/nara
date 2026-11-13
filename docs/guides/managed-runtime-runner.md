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

## Control And Shutdown

Use `request_control` and `drive` for pause, resume, exact fixed-tick stepping, Stop, and retryable
close. A request is accepted separately from the later operation result; inspect its ticket with
`control_status` after a drive. Stop is finite only when the runtime reaches `RuntimeState::Stopped`.

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
