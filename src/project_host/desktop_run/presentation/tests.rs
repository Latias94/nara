use std::{cell::Cell, rc::Rc};

use nara_app::{
    App, RuntimeAdmissionReservation, RuntimeCandidateRetirementState, RuntimeClosePolicy,
    RuntimeObligationLedger,
};

use super::*;

fn live_runtime() -> RuntimeInstance {
    RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            App::new().seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote()
}

fn retire(runtime: RuntimeInstance) {
    let mut retirement = runtime.begin_retirement();
    for _ in 0..8 {
        if retirement.retirement_state() == RuntimeCandidateRetirementState::Retired {
            return;
        }
        retirement.drive_retirement();
    }
    assert_eq!(
        retirement.retirement_state(),
        RuntimeCandidateRetirementState::Retired
    );
}

fn sealed_reports() -> DesktopPresentationReports {
    // Native sealing is upstream of this CPU control test. Inject its immutable final report,
    // then exercise the actual observer invocation and exit reconciliation used by maintenance.
    DesktopPresentationReports(Rc::new(RefCell::new(DesktopPresentationReportState {
        telemetry_seal: Some(DesktopTelemetrySealReport {
            outcome: DesktopTelemetrySealOutcome::Complete,
            observation: WgpuRenderObservation::default(),
            completion: WgpuFrameCompletionStats::default(),
            resources: WgpuGpuResourceStats::default(),
            samples: Vec::new(),
            instance_id: 1,
            device_epoch: 1,
            adapter: None,
        }),
        exit: None,
    })))
}

#[test]
fn final_observer_evidence_failure_vetoes_seal_success_and_is_delivered_once() {
    let runtime = live_runtime();
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let options = DesktopPresentationOptions {
        telemetry: None,
        observer: Some(Box::new(move |observation| {
            assert_eq!(
                observation.telemetry_seal().unwrap().outcome(),
                DesktopTelemetrySealOutcome::Complete
            );
            observed_deliveries.set(observed_deliveries.get() + 1);
            let publish_evidence: Result<(), &str> = Err("test.final-evidence-write-rejected");
            match publish_evidence {
                Ok(()) => DesktopPresentationControl::Continue,
                Err(_) => DesktopPresentationControl::Exit(AppExit::Requested),
            }
        })),
    };
    let reports = sealed_reports();
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        options,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        None,
        reports.clone(),
    );
    let final_progress = driver.observe_consumer(
        &runtime,
        None,
        Instant::now(),
        &[],
        __WinitPresentationProgress {
            exit: Some(AppExit::Success),
            next_wake: None,
        },
    );
    assert_eq!(final_progress.exit, Some(AppExit::Requested));
    assert_eq!(reports.reconcile_exit(AppExit::Success), AppExit::Requested);
    let later_progress = driver.observe_consumer(
        &runtime,
        None,
        Instant::now(),
        &[],
        __WinitPresentationProgress {
            exit: None,
            next_wake: None,
        },
    );
    assert_eq!(later_progress.exit, None);
    assert_eq!(deliveries.get(), 1);
    drop(driver);
    retire(runtime);
}

#[test]
fn final_observer_success_cannot_override_a_required_exit() {
    let runtime = live_runtime();
    let options = DesktopPresentationOptions {
        telemetry: None,
        observer: Some(Box::new(|observation| {
            assert!(observation.telemetry_seal().is_some());
            DesktopPresentationControl::Exit(AppExit::Success)
        })),
    };
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        options,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        None,
        sealed_reports(),
    );
    let progress = driver.observe_consumer(
        &runtime,
        None,
        Instant::now(),
        &[],
        __WinitPresentationProgress {
            exit: Some(AppExit::Requested),
            next_wake: None,
        },
    );
    assert_eq!(progress.exit, Some(AppExit::Requested));
    drop(driver);
    retire(runtime);
}

#[test]
fn final_observer_veto_during_target_retirement_survives_native_success() {
    let mut runtime = live_runtime();
    let reports = sealed_reports();
    let options = DesktopPresentationOptions {
        telemetry: None,
        observer: Some(Box::new(|observation| {
            assert!(observation.telemetry_seal().is_some());
            DesktopPresentationControl::Exit(AppExit::Requested)
        })),
    };
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        options,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        None,
        reports.clone(),
    );
    assert!(
        driver
            .retire(
                &mut runtime,
                &BackendWindowHandles::default(),
                &[],
                Instant::now()
            )
            .unwrap()
    );
    assert_eq!(reports.reconcile_exit(AppExit::Success), AppExit::Requested);
    drop(driver);
    retire(runtime);
}

#[test]
fn repeated_active_seal_request_preserves_veto_without_resetting_deadline() {
    let runtime = live_runtime();
    let now = Instant::now();
    let original_deadline = now + Duration::from_secs(1);
    let repeated_deadline = now + Duration::from_secs(30);
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let options = DesktopPresentationOptions {
        telemetry: None,
        observer: Some(Box::new(move |observation| {
            assert!(observation.telemetry_seal().is_none());
            let first = observed_deliveries.get() == 0;
            observed_deliveries.set(observed_deliveries.get() + 1);
            DesktopPresentationControl::SealTelemetryAndExit {
                exit: if first {
                    AppExit::Success
                } else {
                    AppExit::Requested
                },
                deadline: if first {
                    original_deadline
                } else {
                    repeated_deadline
                },
            }
        })),
    };
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        options,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        None,
        DesktopPresentationReports::default(),
    );
    let first = driver.observe_consumer(
        &runtime,
        None,
        now,
        &[],
        __WinitPresentationProgress {
            exit: None,
            next_wake: None,
        },
    );
    assert_eq!(
        first.exit, None,
        "an active seal drains before returning its exit"
    );
    assert_eq!(driver.seal.as_ref().unwrap().exit, Some(AppExit::Success));
    let repeated = driver.observe_consumer(
        &runtime,
        None,
        now,
        &[],
        __WinitPresentationProgress {
            exit: None,
            next_wake: None,
        },
    );
    assert_eq!(repeated.exit, None);
    assert_eq!(driver.seal.as_ref().unwrap().exit, Some(AppExit::Requested));
    assert_eq!(driver.seal.as_ref().unwrap().deadline, original_deadline);
    assert_eq!(deliveries.get(), 2);
    drop(driver);
    retire(runtime);
}

#[test]
fn final_report_seal_request_vetoes_success_without_restarting_or_redelivery() {
    let runtime = live_runtime();
    let reports = sealed_reports();
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let options = DesktopPresentationOptions {
        telemetry: None,
        observer: Some(Box::new(move |observation| {
            assert_eq!(
                observation.telemetry_seal().unwrap().outcome(),
                DesktopTelemetrySealOutcome::Complete
            );
            observed_deliveries.set(observed_deliveries.get() + 1);
            DesktopPresentationControl::SealTelemetryAndExit {
                exit: AppExit::Requested,
                deadline: observation.now() + Duration::from_secs(30),
            }
        })),
    };
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        options,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        None,
        reports.clone(),
    );
    let final_progress = driver.observe_consumer(
        &runtime,
        None,
        Instant::now(),
        &[],
        __WinitPresentationProgress {
            exit: Some(AppExit::Success),
            next_wake: None,
        },
    );
    assert_eq!(final_progress.exit, Some(AppExit::Requested));
    assert_eq!(reports.reconcile_exit(AppExit::Success), AppExit::Requested);
    assert!(driver.seal.is_none());
    let later = driver.observe_consumer(
        &runtime,
        None,
        Instant::now(),
        &[],
        __WinitPresentationProgress {
            exit: None,
            next_wake: None,
        },
    );
    assert_eq!(later.exit, None);
    assert!(driver.seal.is_none());
    assert_eq!(deliveries.get(), 1);
    drop(driver);
    retire(runtime);
}
