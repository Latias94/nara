# Managed Runtime Runners

An embedded or external Rust host can own a concrete managed-runtime loop through the public
`nara::app` API. This is an advanced code-first integration surface. Ordinary projects should use
the first-party product Hosts unless they have a real embedding requirement.

The host owns its loop and selects its platform/event integration before it builds the runtime. Nara
does not provide a universal `Runner` trait, factory, or service registry for this purpose.

## Construction

Use a `RuntimeAdmissionReservation` before transferring a sealed `App` and its close obligations.
Then complete startup and promote the candidate into the concrete `RuntimeInstance` owned by the
host.

```rust
use std::{error::Error, time::Duration};

use nara::app::{
    App, RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeObligationLedger,
};

fn start() -> Result<(), Box<dyn Error>> {
    let reservation = RuntimeAdmissionReservation::try_acquire()?;
    let app = App::new();
    let sealed = app.seal()?;
    let candidate = reservation.admit(
        sealed,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::default(),
    )?;
    let mut runtime = candidate.complete_startup()?.promote();

    runtime.drive(Duration::ZERO)?;
    Ok(())
}
```

The reservation exists so capacity failure happens before a caller transfers a sealed App or close
ledger. An admission or startup failure retains a retryable retirement owner; drive that retirement
to its truthful terminal state rather than treating a dropped value as a clean stop.

## Control And Shutdown

Use `request_control` and `drive` for pause, resume, exact fixed-tick stepping, Stop, and retryable
close. A request is accepted separately from the later operation result; inspect its ticket with
`control_status` after a drive. Stop is finite only when the runtime reaches `RuntimeState::Stopped`.

Do not install a raw App runner on an App that will enter managed admission. Do not call
`App::run_once` behind a `RuntimeInstance`, and do not use hidden driver ports or runtime World
mutation as an external integration mechanism. Platform-specific adapters should translate their
own events and elapsed time into the concrete runtime's public control and drive operations.
