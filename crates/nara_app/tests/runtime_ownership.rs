use std::{
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, ThreadId},
    time::Duration,
};

use nara_app::{
    App, PluginConfigurationFingerprint, PluginDeclaration, PluginDefinition, PluginDefinitionKey,
    PluginId, PluginPlan, PluginPlanFingerprint, RuntimeAdmissionReservation,
    RuntimeCandidateRetirementState, RuntimeCloseContext, RuntimeCloseEvidence,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl, RuntimeFault, RuntimeFaultKind,
    RuntimeFaultReporter, RuntimeGeneration, RuntimeInstance, RuntimeObligationLedger,
    RuntimeScheduleError, RuntimeState, drive_runtime_quarantine, runtime_quarantine_status,
};
use nara_ecs::{Res, ResMut, Resource, ScheduleLabel, error::BevyError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct CustomSchedule;

#[derive(Debug, Default, Resource)]
struct CustomRuns(usize);

#[derive(Resource)]
struct CustomAuthority(Arc<AtomicBool>);

fn fail_custom_schedule(mut runs: ResMut<CustomRuns>) -> Result<(), BevyError> {
    runs.0 += 1;
    Err(BevyError::error(std::io::Error::other(
        "injected custom schedule failure",
    )))
}

#[test]
fn definitions_and_world_free_runtime_handles_remain_transportable() {
    fn assert_send_sync<Value: Send + Sync>() {}
    fn assert_send<Value: Send>() {}

    assert_send_sync::<PluginDeclaration>();
    assert_send_sync::<PluginDefinition>();
    assert_send_sync::<PluginDefinitionKey>();
    assert_send_sync::<PluginConfigurationFingerprint>();
    assert_send_sync::<PluginPlan>();
    assert_send_sync::<PluginPlanFingerprint>();
    assert_send_sync::<RuntimeAdmissionReservation>();
    assert_send_sync::<RuntimeCloseEvidence>();
    assert_send_sync::<RuntimeControl>();
    assert_send_sync::<RuntimeFaultReporter>();
    assert_send_sync::<RuntimeGeneration>();
    assert_send::<RuntimeObligationLedger>();
}

struct NonSendDropProbe {
    drops: Arc<Mutex<Vec<ThreadId>>>,
    _thread_local: Rc<()>,
}

impl Drop for NonSendDropProbe {
    fn drop(&mut self) {
        self.drops.lock().unwrap().push(thread::current().id());
    }
}

#[test]
fn independent_threads_construct_and_close_their_own_non_send_worlds() {
    let owners = (0..2)
        .map(|_| {
            thread::spawn(|| {
                let owner_thread = thread::current().id();
                let drops = Arc::new(Mutex::new(Vec::new()));
                let mut configuration = App::new();
                configuration
                    .world_mut()
                    .unwrap()
                    .insert_non_send(NonSendDropProbe {
                        drops: Arc::clone(&drops),
                        _thread_local: Rc::new(()),
                    });
                drop(configuration);
                assert_eq!(*drops.lock().unwrap(), [owner_thread]);

                let mut app = App::new();
                app.world_mut().unwrap().insert_non_send(NonSendDropProbe {
                    drops: Arc::clone(&drops),
                    _thread_local: Rc::new(()),
                });
                let mut runtime = RuntimeAdmissionReservation::try_acquire()
                    .unwrap()
                    .admit(
                        app.seal().unwrap(),
                        RuntimeObligationLedger::new(),
                        RuntimeClosePolicy::default(),
                    )
                    .unwrap()
                    .complete_startup()
                    .unwrap()
                    .promote();
                runtime.drive(Duration::ZERO).unwrap();
                runtime.finish().unwrap();

                assert_eq!(*drops.lock().unwrap(), [owner_thread, owner_thread]);
                assert_eq!(runtime_quarantine_status().current_thread_retained(), 0);
                owner_thread
            })
        })
        .collect::<Vec<_>>();
    let owner_threads = owners
        .into_iter()
        .map(|owner| owner.join().unwrap())
        .collect::<Vec<_>>();

    assert_ne!(owner_threads[0], owner_threads[1]);
}

struct WaitForRelease(Arc<AtomicBool>);

impl RuntimeCloseParticipant for WaitForRelease {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.poll_close(context)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(if self.0.load(Ordering::Acquire) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

#[test]
fn quarantine_retires_non_send_world_on_its_constructing_thread() {
    thread::spawn(|| {
        let owner_thread = thread::current().id();
        let drops = Arc::new(Mutex::new(Vec::new()));
        let released = Arc::new(AtomicBool::new(false));
        let mut app = App::new();
        app.world_mut().unwrap().insert_non_send(NonSendDropProbe {
            drops: Arc::clone(&drops),
            _thread_local: Rc::new(()),
        });
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.non-send-owner-retirement"),
                WaitForRelease(Arc::clone(&released)),
            )
            .unwrap();
        let runtime = RuntimeInstance::start_with_options(
            app,
            obligations,
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap();
        let mut retirement = runtime.begin_retirement();
        let progress = retirement.drive_retirement();
        drop(retirement);
        let retained = runtime_quarantine_status().current_thread_retained();
        let drops_before_release = drops.lock().unwrap().clone();

        released.store(true, Ordering::Release);
        let drained = drive_runtime_quarantine();

        assert_eq!(
            progress,
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        assert_eq!(retained, 1);
        assert!(drops_before_release.is_empty());
        assert_eq!(drained.current_thread_retained(), 0);
        assert_eq!(*drops.lock().unwrap(), [owner_thread]);
    })
    .join()
    .unwrap();
}

#[test]
fn faulted_custom_schedule_preserves_first_fault_and_retryable_close_ownership() {
    let released = Arc::new(AtomicBool::new(false));
    let mut app = App::new();
    app.init_resource::<CustomRuns>().unwrap();
    app.add_systems(CustomSchedule, fail_custom_schedule)
        .unwrap();
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.custom-schedule.retirement"),
            WaitForRelease(Arc::clone(&released)),
        )
        .unwrap();
    let mut runtime = RuntimeInstance::start_with_options(
        app,
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();

    let result = runtime.run_schedule(CustomSchedule);
    let faulted_state = runtime.state();
    let repeated = runtime.run_schedule(CustomSchedule);
    let drive = runtime.drive(Duration::ZERO);
    let first_fault = runtime.fault().cloned();
    runtime.request_control(RuntimeControl::Stop);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let incomplete = runtime.state();
    let rejected_while_incomplete = runtime.run_schedule(CustomSchedule);
    released.store(true, Ordering::Release);
    runtime.request_control(RuntimeControl::RetryClose);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let error = result.unwrap_err();
    let fault = error
        .fault()
        .expect("custom execution must return its fault");
    assert_eq!(fault.kind(), RuntimeFaultKind::System);
    assert_eq!(first_fault.as_ref(), Some(fault));
    assert_eq!(drive.unwrap_err().fault(), fault);
    assert_eq!(runtime.fault(), Some(fault));
    assert_eq!(faulted_state, RuntimeState::Faulted);
    assert_eq!(
        repeated,
        Err(RuntimeScheduleError::Unavailable {
            state: RuntimeState::Faulted
        })
    );
    assert_eq!(incomplete, RuntimeState::CloseIncomplete);
    assert_eq!(
        rejected_while_incomplete,
        Err(RuntimeScheduleError::Unavailable {
            state: RuntimeState::CloseIncomplete
        })
    );
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(runtime.world().resource::<CustomRuns>().0, 1);
}

#[test]
fn previously_reported_fault_prevents_custom_schedule_execution() {
    let mut app = App::new();
    app.init_resource::<CustomRuns>().unwrap();
    app.add_systems(CustomSchedule, |mut runs: ResMut<CustomRuns>| runs.0 += 1)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let first = RuntimeFault::engine(RuntimeFaultKind::RequiredTask, "nara.test.background-fault");
    runtime.fault_reporter().report(first.clone());

    let result = runtime.run_schedule(CustomSchedule);
    let runs = runtime.world().resource::<CustomRuns>().0;
    let state = runtime.state();
    let retained_fault = runtime.fault().cloned();
    runtime.finish().unwrap();

    assert_eq!(
        result,
        Err(RuntimeScheduleError::Unavailable {
            state: RuntimeState::Faulted
        })
    );
    assert_eq!(retained_fault, Some(first));
    assert_eq!(state, RuntimeState::Faulted);
    assert_eq!(
        runs, 0,
        "a previously reported fault must stop the next schedule"
    );
}

#[test]
fn explicit_custom_schedule_checks_authority_before_and_after_its_systems() {
    for break_during_schedule in [false, true] {
        let authority = Arc::new(AtomicBool::new(true));
        let authority_check = Arc::clone(&authority);
        let mut app = App::new();
        app.init_resource::<CustomRuns>().unwrap();
        app.insert_resource(CustomAuthority(Arc::clone(&authority)))
            .unwrap();
        app.__install_runtime_authority_validator(
            PluginId::new("nara.test.custom-schedule.authority"),
            Arc::new(move |_, _| {
                if authority_check.load(Ordering::Acquire) {
                    Ok(())
                } else {
                    Err(RuntimeFault::engine(
                        RuntimeFaultKind::RuntimeAuthority,
                        "nara.test.custom-schedule.authority",
                    ))
                }
            }),
        )
        .unwrap();
        app.add_systems(
            CustomSchedule,
            |authority: Res<CustomAuthority>, mut runs: ResMut<CustomRuns>| {
                runs.0 += 1;
                authority.0.store(false, Ordering::Release);
            },
        )
        .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        if !break_during_schedule {
            authority.store(false, Ordering::Release);
        }

        let error = runtime.run_schedule(CustomSchedule).unwrap_err();

        assert_eq!(
            error.fault().unwrap().kind(),
            RuntimeFaultKind::RuntimeAuthority
        );
        assert_eq!(runtime.state(), RuntimeState::Faulted);
        assert_eq!(
            runtime.world().resource::<CustomRuns>().0,
            usize::from(break_during_schedule)
        );
        runtime.finish().unwrap();
    }
}
