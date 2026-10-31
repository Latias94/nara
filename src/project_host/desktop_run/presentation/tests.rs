use std::{cell::Cell, rc::Rc};

use nara_app::{
    __RuntimeDriverPort, App, RuntimeAdmissionReservation, RuntimeCandidateRetirementState,
    RuntimeClosePolicy, RuntimeObligationLedger,
};
use nara_ecs::Resource;
use nara_render::{FrameStats, RenderFrame, RenderFrameState};

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

#[derive(Resource)]
struct FrameStatsPort;

impl __RuntimeDriverPort for FrameStatsPort {
    type Input = Option<FrameStats>;
    type Output = Option<FrameStats>;

    fn apply_driver_input(&mut self, _: Self::Input) -> Self::Output {
        unreachable!("the test uses its registered frame-stats operation")
    }
}

fn render_runtime() -> RuntimeInstance {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_render_wgpu::WgpuRenderPlugin,
    ))
    .unwrap();
    app.insert_resource(FrameStatsPort).unwrap();
    app.__register_driver_port::<FrameStatsPort>(|world, replacement| {
        let original = world.remove_resource::<FrameStats>();
        if let Some(stats) = replacement {
            world.insert_resource(stats);
        }
        Ok(original)
    })
    .unwrap();
    RuntimeInstance::start(app).unwrap()
}

fn swap_frame_stats(
    runtime: &mut RuntimeInstance,
    stats: Option<FrameStats>,
) -> Option<FrameStats> {
    let outcome = runtime
        .__with_owned_driver_port::<FrameStatsPort>(stats)
        .unwrap_or_else(|(error, _)| panic!("the test frame-stats scope was refused: {error}"));
    assert!(outcome.post_operation_fault.is_none());
    outcome
        .output
        .unwrap_or_else(|(error, _)| panic!("the test frame-stats operation was refused: {error}"))
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
fn lazy_producer_failure_reconciles_or_retains_the_original_capture_result() {
    for refuse_completion in [false, true] {
        let mut runtime = render_runtime();
        let handles = BackendWindowHandles::default();
        let mut driver = DesktopPresentationDriver::new(
            &runtime,
            DesktopPresentationOptions::default(),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            None,
            DesktopPresentationReports::default(),
        );
        driver
            .maintain(&mut runtime, &handles, Instant::now())
            .unwrap();
        assert!(driver.producer.is_none());
        assert_eq!(driver.host.runtime_facts().device_epoch, 0);

        // An already closed CPU-only Host makes the first actual producer admission fail.
        assert!(driver.host.poll_runtime_retirement(&[]).unwrap());
        runtime.drive(Duration::ZERO).unwrap();
        let original_stats =
            refuse_completion.then(|| swap_frame_stats(&mut runtime, None).unwrap());
        let error = driver
            .maintain(&mut runtime, &handles, Instant::now())
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("WGPU Runtime producer creation failed")
        );
        assert!(driver.producer.is_none());
        assert_eq!(driver.last_observation.instance_id(), 0);
        if let Some(original_stats) = original_stats {
            assert!(matches!(error, AppRunError::RunnerTeardown { .. }));
            let identity = driver.pending_result.as_ref().unwrap().identity();
            assert_eq!(identity.runtime_generation(), runtime.generation());
            assert_eq!(identity.sequence(), 1);
            assert!(driver.complete_pending(&mut runtime).is_err());
            assert_eq!(driver.pending_result.as_ref().unwrap().identity(), identity);
            assert!(swap_frame_stats(&mut runtime, Some(original_stats)).is_none());
            assert!(driver.complete_pending(&mut runtime).unwrap().is_none());
        }
        assert!(driver.pending_result.is_none());
        assert_eq!(
            runtime.world().resource::<RenderFrame>().state,
            RenderFrameState::Skipped
        );
        assert_eq!(
            runtime
                .world()
                .resource::<WgpuRenderObservation>()
                .instance_id(),
            0
        );

        // Publishing the next capture proves that the original in-flight barrier was released.
        runtime.drive(Duration::ZERO).unwrap();
        let capture = take_runtime_frame(&mut runtime).unwrap().output.unwrap();
        assert_eq!(capture.identity().sequence(), 2);
        let completion =
            complete_runtime_frame(&mut runtime, cancel_runtime_frame(capture)).unwrap();
        assert!(completion.post_operation_fault.is_none());
        drop(driver);
        retire(runtime);
    }
}

#[test]
fn retirement_retains_original_result_and_seals_host_facts_before_native_close() {
    let mut runtime = render_runtime();
    let handles = BackendWindowHandles::default();
    let reports = DesktopPresentationReports::default();
    let mut driver = DesktopPresentationDriver::new(
        &runtime,
        DesktopPresentationOptions {
            telemetry: Some(WgpuBackendTelemetryConfig::default()),
            observer: None,
        },
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        None,
        reports.clone(),
    );
    driver.producer = Some(driver.host.create_producer().unwrap());
    let facts = driver.host.runtime_facts();
    assert_ne!(facts.backend_instance_id, 0);
    assert_eq!(facts.device_epoch, 0);
    let deliveries = Rc::new(Cell::new(0));
    let observed_deliveries = Rc::clone(&deliveries);
    driver.observer = Some(Box::new(move |observation| {
        let report = observation.telemetry_seal().unwrap();
        assert_eq!(report.backend_instance_id(), facts.backend_instance_id);
        assert_eq!(report.device_epoch(), facts.device_epoch);
        assert_eq!(
            report.outcome(),
            DesktopTelemetrySealOutcome::TrackingUnavailable
        );
        assert_eq!(
            report.frame_completion_stats(),
            observation.frame_completion_stats()
        );
        assert_eq!(
            report.gpu_resource_stats(),
            observation.gpu_resource_stats()
        );
        observed_deliveries.set(observed_deliveries.get() + 1);
        DesktopPresentationControl::Exit(AppExit::Requested)
    }));
    runtime.drive(Duration::ZERO).unwrap();
    let capture = take_runtime_frame(&mut runtime).unwrap().output.unwrap();
    let identity = capture.identity();
    driver.pending_result = Some(cancel_runtime_frame(capture));
    let original_stats = swap_frame_stats(&mut runtime, None).unwrap();
    assert!(
        driver
            .retire(&mut runtime, &handles, &[], Instant::now())
            .is_err()
    );
    assert_eq!(driver.pending_result.as_ref().unwrap().identity(), identity);
    assert_eq!(deliveries.get(), 1);
    assert_eq!(reports.reconcile_exit(AppExit::Success), AppExit::Requested);
    // Refused original results still own admission; whole-Host close has not started.
    let probe = driver.host.create_producer().unwrap();
    drop(probe);
    assert!(swap_frame_stats(&mut runtime, Some(original_stats)).is_none());
    assert!(
        driver
            .retire(&mut runtime, &handles, &[], Instant::now())
            .unwrap()
    );
    assert!(driver.pending_result.is_none());
    assert!(driver.host.create_producer().is_err());
    assert_eq!(deliveries.get(), 1);
    assert_eq!(
        reports.telemetry_seal().unwrap().backend_instance_id(),
        facts.backend_instance_id
    );
    drop(driver);
    retire(runtime);
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
