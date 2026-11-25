use nara_app::{FixedTime, RealTime, RenderTime, VirtualTime};

use super::*;

fn time_and_frame(
    runtime: &RuntimeInstance,
) -> (RealTime, VirtualTime, FixedTime, RenderTime, RenderFrame) {
    let world = runtime.world();
    (
        *world.resource::<RealTime>(),
        *world.resource::<VirtualTime>(),
        *world.resource::<FixedTime>(),
        *world.resource::<RenderTime>(),
        *world.resource::<RenderFrame>(),
    )
}

#[test]
fn idle_root_maintenance_adds_no_wake_with_or_without_a_continuing_observer() {
    for with_observer in [false, true] {
        let mut runtime = render_runtime();
        let generation = runtime.generation();
        let state = runtime.state();
        let before = time_and_frame(&runtime);
        let deliveries = Rc::new(Cell::new(0));
        let observed_deliveries = Rc::clone(&deliveries);
        let mut options = DesktopPresentationOptions::default();
        if with_observer {
            options.observer = Some(Box::new(move |observation| {
                assert_eq!(observation.runtime_generation(), generation);
                assert!(observation.world().is_some());
                assert!(observation.presented_frame().is_none());
                assert!(observation.completion_samples().is_empty());
                assert!(observation.telemetry_seal().is_none());
                observed_deliveries.set(observed_deliveries.get() + 1);
                DesktopPresentationControl::Continue
            }));
        }
        let mut driver = DesktopPresentationDriver::new(
            &runtime,
            options,
            #[cfg(feature = "project-source")]
            None,
            DesktopPresentationReports::default(),
        );
        let handles = BackendWindowHandles::default();
        let now = Instant::now();
        for elapsed in [
            Duration::ZERO,
            Duration::from_secs(1),
            Duration::from_secs(60),
        ] {
            let progress = driver
                .maintain(&mut runtime, &handles, now + elapsed)
                .unwrap();
            assert_eq!(progress.exit, None);
            assert_eq!(progress.next_wake, None);
            assert_eq!(runtime.generation(), generation);
            assert_eq!(runtime.state(), state);
            assert!(runtime.fault().is_none());
            assert_eq!(time_and_frame(&runtime), before);
            assert!(driver.producer.is_none());
            assert!(driver.pending_result.is_none());
            assert_eq!(driver.host.runtime_facts().device_epoch, 0);
        }
        assert_eq!(deliveries.get(), if with_observer { 3 } else { 0 });
        drop(driver);
        retire(runtime);
    }
}

#[test]
fn observer_absolute_wake_merges_with_earlier_and_later_original_owner_progress() {
    let runtime = live_runtime();
    let now = Instant::now();
    let deadline = now + Duration::from_millis(100);
    let earlier_owner = now + Duration::from_millis(10);
    let later_owner = now + Duration::from_secs(1);
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        DesktopPresentationOptions {
            telemetry: None,
            observer: Some(Box::new(move |observation| {
                assert!(observation.world().is_some());
                observed_deliveries.set(observed_deliveries.get() + 1);
                DesktopPresentationControl::WakeAt(deadline)
            })),
        },
        #[cfg(feature = "project-source")]
        None,
        DesktopPresentationReports::default(),
    );
    for callback_now in [now, now + Duration::from_millis(5)] {
        for (owner_wake, expected_wake) in [
            (None, deadline),
            (Some(earlier_owner), earlier_owner),
            (Some(later_owner), deadline),
        ] {
            let progress = driver.observe_consumer(
                &runtime,
                None,
                callback_now,
                &[],
                WinitPresentationProgress {
                    exit: None,
                    next_wake: owner_wake,
                },
            );
            assert_eq!(progress.exit, None);
            assert_eq!(progress.next_wake, Some(expected_wake));
            assert!(driver.seal.is_none());
        }
    }
    assert_eq!(deliveries.get(), 6);
    drop(driver);
    retire(runtime);
}

#[test]
fn observer_continue_withdraws_only_its_previous_deadline_from_fresh_owner_progress() {
    let runtime = live_runtime();
    let now = Instant::now();
    let deadline = now + Duration::from_millis(100);
    let earlier_owner = now + Duration::from_millis(10);
    let later_owner = now + Duration::from_secs(1);
    let request = Rc::new(Cell::new(DesktopPresentationControl::Continue));
    let observed_request = Rc::clone(&request);
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        DesktopPresentationOptions {
            telemetry: None,
            observer: Some(Box::new(move |_| {
                observed_deliveries.set(observed_deliveries.get() + 1);
                observed_request.get()
            })),
        },
        #[cfg(feature = "project-source")]
        None,
        DesktopPresentationReports::default(),
    );
    for (owner_wake, expected_wake) in [
        (None, deadline),
        (Some(earlier_owner), earlier_owner),
        (Some(later_owner), deadline),
    ] {
        request.set(DesktopPresentationControl::WakeAt(deadline));
        let requested = driver.observe_consumer(
            &runtime,
            None,
            now,
            &[],
            WinitPresentationProgress {
                exit: None,
                next_wake: owner_wake,
            },
        );
        assert_eq!(requested.exit, None);
        assert_eq!(requested.next_wake, Some(expected_wake));
        request.set(DesktopPresentationControl::Continue);
        let withdrawn = driver.observe_consumer(
            &runtime,
            None,
            now + Duration::from_millis(5),
            &[],
            WinitPresentationProgress {
                exit: None,
                next_wake: owner_wake,
            },
        );
        assert_eq!(withdrawn.exit, None);
        assert_eq!(withdrawn.next_wake, owner_wake);
    }
    assert_eq!(deliveries.get(), 6);
    drop(driver);
    retire(runtime);
}

#[test]
fn expired_observer_deadline_returns_its_original_time_without_an_app_frame() {
    let mut runtime = render_runtime();
    let generation = runtime.generation();
    let state = runtime.state();
    let before = time_and_frame(&runtime);
    let deadline = Instant::now();
    let now = deadline + Duration::from_secs(60);
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        DesktopPresentationOptions {
            telemetry: None,
            observer: Some(Box::new(move |observation| {
                assert!(observation.now() >= now);
                assert_eq!(observation.runtime_generation(), generation);
                assert!(observation.presented_frame().is_none());
                observed_deliveries.set(observed_deliveries.get() + 1);
                DesktopPresentationControl::WakeAt(deadline)
            })),
        },
        #[cfg(feature = "project-source")]
        None,
        DesktopPresentationReports::default(),
    );
    let handles = BackendWindowHandles::default();
    for callback_now in [now, now + Duration::from_secs(1)] {
        let progress = driver
            .maintain(&mut runtime, &handles, callback_now)
            .unwrap();
        assert_eq!(progress.exit, None);
        assert_eq!(progress.next_wake, Some(deadline));
        assert!(progress.next_wake.unwrap() < callback_now);
        assert_eq!(runtime.generation(), generation);
        assert_eq!(runtime.state(), state);
        assert!(runtime.fault().is_none());
        assert_eq!(time_and_frame(&runtime), before);
        assert!(driver.producer.is_none());
        assert!(driver.pending_result.is_none());
        assert_eq!(driver.host.runtime_facts().device_epoch, 0);
    }
    assert_eq!(deliveries.get(), 2);
    drop(driver);
    retire(runtime);
}

#[test]
fn final_observer_exit_veto_survives_its_previous_wake_request() {
    let runtime = live_runtime();
    let now = Instant::now();
    let deadline = now + Duration::from_secs(1);
    let reports = DesktopPresentationReports::default();
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        DesktopPresentationOptions {
            telemetry: None,
            observer: Some(Box::new(move |observation| {
                observed_deliveries.set(observed_deliveries.get() + 1);
                if let Some(report) = observation.telemetry_seal() {
                    assert_eq!(report.outcome(), DesktopTelemetrySealOutcome::Complete);
                    DesktopPresentationControl::Exit(AppExit::Requested)
                } else {
                    DesktopPresentationControl::WakeAt(deadline)
                }
            })),
        },
        #[cfg(feature = "project-source")]
        None,
        reports.clone(),
    );
    let pending = driver.observe_consumer(
        &runtime,
        None,
        now,
        &[],
        WinitPresentationProgress {
            exit: None,
            next_wake: None,
        },
    );
    assert_eq!(pending.exit, None);
    assert_eq!(pending.next_wake, Some(deadline));
    reports.0.borrow_mut().telemetry_seal = sealed_reports().telemetry_seal();
    let final_progress = driver.observe_consumer(
        &runtime,
        None,
        now + Duration::from_millis(10),
        &[],
        WinitPresentationProgress {
            exit: Some(AppExit::Success),
            next_wake: None,
        },
    );
    assert_eq!(final_progress.exit, Some(AppExit::Requested));
    assert_eq!(final_progress.next_wake, None);
    assert_eq!(reports.reconcile_exit(AppExit::Success), AppExit::Requested);
    assert!(driver.seal.is_none());
    let later = driver.observe_consumer(
        &runtime,
        None,
        deadline,
        &[],
        WinitPresentationProgress {
            exit: None,
            next_wake: None,
        },
    );
    assert_eq!(later.exit, None);
    assert_eq!(later.next_wake, None);
    assert_eq!(deliveries.get(), 2);
    drop(driver);
    retire(runtime);
}

#[test]
fn original_seal_deadline_survives_observer_wake_withdrawal_and_repeated_sealing() {
    let runtime = live_runtime();
    let now = Instant::now();
    let original_deadline = now + Duration::from_micros(500);
    let later_deadline = now + Duration::from_secs(30);
    let request = Rc::new(Cell::new(DesktopPresentationControl::WakeAt(
        later_deadline,
    )));
    let observed_request = Rc::clone(&request);
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        DesktopPresentationOptions {
            telemetry: None,
            observer: Some(Box::new(move |observation| {
                assert!(observation.telemetry_seal().is_none());
                observed_deliveries.set(observed_deliveries.get() + 1);
                observed_request.get()
            })),
        },
        #[cfg(feature = "project-source")]
        None,
        DesktopPresentationReports::default(),
    );
    driver.begin_seal(original_deadline, Some(AppExit::Success));
    for (callback_now, control) in [
        (now, DesktopPresentationControl::WakeAt(later_deadline)),
        (
            now + Duration::from_micros(100),
            DesktopPresentationControl::Continue,
        ),
        (
            original_deadline,
            DesktopPresentationControl::SealTelemetryAndExit {
                exit: AppExit::Success,
                deadline: later_deadline,
            },
        ),
    ] {
        request.set(control);
        let progress = driver.observe_consumer(
            &runtime,
            None,
            callback_now,
            &[],
            WinitPresentationProgress {
                exit: Some(AppExit::Requested),
                next_wake: None,
            },
        );
        assert_eq!(progress.exit, None);
        assert_eq!(progress.next_wake, Some(original_deadline));
        assert_eq!(driver.seal.as_ref().unwrap().deadline, original_deadline);
        assert_eq!(driver.seal.as_ref().unwrap().exit, Some(AppExit::Requested));
        assert!(driver.reports.telemetry_seal().is_none());
    }
    assert_eq!(deliveries.get(), 3);
    drop(driver);
    retire(runtime);
}
