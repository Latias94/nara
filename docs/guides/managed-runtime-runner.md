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

## First-Party Staged Publication

Engine-owned Hosts use hidden `RuntimeAdmissionReservation`, sealed-App and candidate plumbing to
retain an unpublished runtime across Host preparation and publication phases. Those stages preserve
the same runtime owner and close contract. They are not a second execution lifecycle or additional
steps for an ordinary external embedding. See
[ADR 0117](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md) for the
configuration/managed-execution boundary.
