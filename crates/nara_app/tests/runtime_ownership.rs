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
    PluginPlan, PluginPlanFingerprint, RuntimeAdmissionReservation,
    RuntimeCandidateRetirementState, RuntimeCloseContext, RuntimeCloseEvidence,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl, RuntimeFaultReporter,
    RuntimeGeneration, RuntimeInstance, RuntimeObligationLedger, drive_runtime_quarantine,
    runtime_quarantine_status,
};

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
