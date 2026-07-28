use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use bevy_ecs::error::{BevyError, ErrorContext, ErrorHandler, FallbackErrorHandler};

use nara::{
    app::{
        __RuntimeDriverPort, App, CoreStage, FixedCatchUpPolicy, FixedTime, FixedUpdateSet, Plugin,
        PluginCategory, PluginDeclaration, PluginError, PluginId, PluginShutdownContext,
        PluginShutdownObligationId, RealTime, RenderTime, RuntimeAdmissionError,
        RuntimeAdmissionFailure, RuntimeAdmissionReservation, RuntimeCandidate,
        RuntimeCandidateRetirementState, RuntimeCloseCause, RuntimeCloseContext,
        RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
        RuntimeCloseParticipantPhase, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
        RuntimeControlFailure, RuntimeControlRejection, RuntimeControlRequestResult,
        RuntimeControlStatus, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter,
        RuntimeInstance, RuntimeObligationLedger, RuntimePublicationError,
        RuntimePublicationFailure, RuntimePublicationSlot, RuntimeScopeError, RuntimeState,
        RuntimeTimeSettings, RuntimeWorldAccessError, SealedApp, StartupStage, VirtualTime,
        drive_runtime_quarantine, runtime_quarantine_status,
    },
    core::{ItemLimit, TimeLimit},
    ecs::{
        Commands, Event, On, Res, ResMut, Resource, World,
        change_detection::DetectChangesMut,
        schedule::{IntoScheduleConfigs, ScheduleLabel},
    },
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandRejection, GameplayCommandSet,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
        GameplayCommandTypeId, submit_gameplay_driver_command,
    },
    input::{
        ActionId, ActionMap, ActionOutcome, ActionOutcomes, ActionPhase, ButtonDriverInput,
        ButtonInput, InputPlugin, KeyCode, MouseButton, apply_keyboard_driver_input,
        apply_mouse_driver_input,
    },
    reflect::{ComponentRegistry, ComponentRegistryPlugin},
    tasks::{
        TaskDomainKey, TaskHandle, TaskKindConfig, TaskPlugin, TaskPoolConfig, TaskPoolKind,
        TaskPools, TaskShutdownPolicy, TaskSpawnRequest, TaskTerminal,
    },
};

#[derive(Debug, Default, Resource)]
struct RuntimeTrace {
    startup_runs: usize,
    fixed_ticks: Vec<u64>,
}

fn record_startup(mut trace: ResMut<RuntimeTrace>) {
    trace.startup_runs += 1;
}

fn record_fixed_tick(fixed_time: nara::ecs::Res<FixedTime>, mut trace: ResMut<RuntimeTrace>) {
    trace.fixed_ticks.push(fixed_time.tick());
}

fn configured_app(fixed_time: FixedTime) -> App {
    let mut app = App::new();
    app.insert_resource(RuntimeTrace::default())
        .unwrap()
        .insert_resource(fixed_time)
        .unwrap()
        .add_systems(StartupStage::Core, record_startup)
        .unwrap()
        .add_systems(CoreStage::FixedUpdate, record_fixed_tick)
        .unwrap();
    app
}

fn start_runtime(app: App) -> RuntimeInstance {
    let sealed = app.seal().unwrap();
    let candidate = admit_candidate(sealed).unwrap();
    match candidate.complete_startup() {
        Ok(ready) => ready.promote(),
        Err(failure) => {
            panic!("candidate startup failed: {:?}", failure.fault())
        }
    }
}

fn admit_candidate(sealed: SealedApp) -> Result<RuntimeCandidate, RuntimeAdmissionFailure> {
    admit_candidate_with(
        sealed,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::default(),
    )
}

fn admit_candidate_with(
    sealed: SealedApp,
    obligations: RuntimeObligationLedger,
    close_policy: RuntimeClosePolicy,
) -> Result<RuntimeCandidate, RuntimeAdmissionFailure> {
    RuntimeAdmissionReservation::try_acquire()
        .expect("the test process has runtime route capacity")
        .admit(sealed, obligations, close_policy)
}

fn accepted_ticket(result: RuntimeControlRequestResult) -> nara::app::RuntimeControlTicket {
    match result {
        RuntimeControlRequestResult::Accepted(ticket) => ticket,
        RuntimeControlRequestResult::Rejected(rejection) => {
            panic!("control request was rejected: {rejection:?}")
        }
    }
}

fn finish_runtime(runtime: RuntimeInstance) {
    let mut retirement = runtime.begin_retirement();
    let deadline = Instant::now() + Duration::from_secs(10);
    while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired
        && Instant::now() < deadline
    {
        retirement.drive_retirement();
        std::thread::yield_now();
    }
    assert_eq!(
        retirement.retirement_state(),
        RuntimeCandidateRetirementState::Retired,
        "runtime retirement did not complete before the test deadline"
    );
}

fn finish_publication_failure(mut failure: RuntimePublicationFailure) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while failure.retirement_state() != RuntimeCandidateRetirementState::Retired
        && Instant::now() < deadline
    {
        failure.drive_retirement();
        std::thread::yield_now();
    }
    assert_eq!(
        failure.retirement_state(),
        RuntimeCandidateRetirementState::Retired,
        "publication failure retirement did not complete before the test deadline"
    );
}

fn wait_for_task<T>(handle: &TaskHandle<T>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(
        handle.is_finished(),
        "task did not finish before the test deadline"
    );
}

fn single_worker_task_config() -> TaskPoolConfig {
    let kind = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::ONE);
    TaskPoolConfig::new(
        kind,
        kind,
        kind,
        TaskShutdownPolicy::new(TimeLimit::MIN, TimeLimit::MIN, TimeLimit::MIN),
    )
    .unwrap()
}

#[derive(Debug, Resource)]
struct TaskFailureIntegration {
    handle: Option<TaskHandle<()>>,
    required: bool,
    observed_failures: usize,
}

fn integrate_task_failure(
    mut integration: ResMut<TaskFailureIntegration>,
    faults: Res<RuntimeFaultReporter>,
) {
    let Some(mut handle) = integration.handle.take() else {
        return;
    };
    let Some(terminal) = handle.try_take() else {
        integration.handle = Some(handle);
        return;
    };
    if matches!(terminal, TaskTerminal::Failed(_)) {
        integration.observed_failures += 1;
        if integration.required {
            faults.report(RuntimeFault::engine(
                RuntimeFaultKind::RequiredTask,
                "nara.test.required-task-integration",
            ));
        }
    }
}

fn runtime_with_failed_task(required: bool) -> RuntimeInstance {
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(TaskPlugin::new(TaskPoolConfig::default()))
        .unwrap();
    let handle = app
        .world()
        .resource::<TaskPools>()
        .spawn(
            TaskPoolKind::Compute,
            TaskSpawnRequest::new(1, TaskDomainKey::new(u64::from(required) + 1)),
            |_| -> () { panic!("injected task integration failure") },
        )
        .into_handle()
        .unwrap();
    wait_for_task(&handle);
    app.insert_resource(TaskFailureIntegration {
        handle: Some(handle),
        required,
        observed_failures: 0,
    })
    .unwrap()
    .add_systems(CoreStage::TaskUpdate, integrate_task_failure)
    .unwrap();
    start_runtime(app)
}

#[derive(Debug, Resource)]
struct ServiceFailureIntegration {
    failed: Arc<AtomicBool>,
    required: bool,
    observed_failures: usize,
}

fn integrate_service_failure(
    mut integration: ResMut<ServiceFailureIntegration>,
    faults: Res<RuntimeFaultReporter>,
) {
    if !integration.failed.swap(false, Ordering::AcqRel) {
        return;
    }
    integration.observed_failures += 1;
    if integration.required {
        faults.report(RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "nara.test.required-service-integration",
        ));
    }
}

#[derive(Debug)]
struct ServiceThreadOwner {
    worker: Option<std::thread::JoinHandle<()>>,
}

impl ServiceThreadOwner {
    fn poll_worker(&mut self) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let Some(worker) = self.worker.as_ref() else {
            return Ok(RuntimeCloseProgress::Complete);
        };
        if !worker.is_finished() {
            return Ok(RuntimeCloseProgress::Pending);
        }
        self.worker
            .take()
            .expect("finished service worker remains owned until join")
            .join()
            .map_err(|_| {
                RuntimeCloseParticipantError::terminal("nara.test.service-thread-panicked")
            })?;
        Ok(RuntimeCloseProgress::Complete)
    }
}

impl RuntimeCloseParticipant for ServiceThreadOwner {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.poll_worker()
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.poll_worker()
    }
}

fn runtime_with_failed_service(required: bool) -> RuntimeInstance {
    let failed = Arc::new(AtomicBool::new(false));
    let worker_failed = failed.clone();
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        worker_failed.store(true, Ordering::Release);
        ready_sender.send(()).unwrap();
    });
    ready_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("service session did not publish its failure before the deadline");

    let mut app = configured_app(FixedTime::default());
    app.insert_resource(ServiceFailureIntegration {
        failed,
        required,
        observed_failures: 0,
    })
    .unwrap()
    .add_systems(CoreStage::TaskUpdate, integrate_service_failure)
    .unwrap();
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.service-thread-owner"),
            ServiceThreadOwner {
                worker: Some(worker),
            },
        )
        .unwrap();
    admit_candidate_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::default(),
    )
    .unwrap()
    .complete_startup()
    .unwrap()
    .promote()
}

#[test]
fn real_required_task_integration_faults_the_runtime_in_the_same_drive() {
    let mut runtime = runtime_with_failed_task(true);

    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::RequiredTask);
    assert_eq!(
        failure.fault().source(),
        "nara.test.required-task-integration"
    );
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.fault(), Some(failure.fault()));
    assert_eq!(
        runtime
            .world()
            .resource::<TaskFailureIntegration>()
            .observed_failures,
        1
    );
    finish_runtime(runtime);
}

#[test]
fn real_optional_task_integration_remains_a_domain_result() {
    let mut runtime = runtime_with_failed_task(false);

    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Running);
    assert_eq!(runtime.fault(), None);
    assert_eq!(
        runtime
            .world()
            .resource::<TaskFailureIntegration>()
            .observed_failures,
        1
    );
    finish_runtime(runtime);
}

#[test]
fn real_required_service_integration_faults_the_runtime_in_the_same_drive() {
    let mut runtime = runtime_with_failed_service(true);

    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::RequiredService);
    assert_eq!(
        failure.fault().source(),
        "nara.test.required-service-integration"
    );
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.fault(), Some(failure.fault()));
    assert_eq!(
        runtime
            .world()
            .resource::<ServiceFailureIntegration>()
            .observed_failures,
        1
    );
    finish_runtime(runtime);
}

#[test]
fn real_optional_service_integration_remains_a_domain_result() {
    let mut runtime = runtime_with_failed_service(false);

    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Running);
    assert_eq!(runtime.fault(), None);
    assert_eq!(
        runtime
            .world()
            .resource::<ServiceFailureIntegration>()
            .observed_failures,
        1
    );
    finish_runtime(runtime);
}

#[test]
fn code_first_runtime_controls_one_sealed_app_without_project_state() {
    let mut runtime = start_runtime(configured_app(FixedTime::default()));

    assert_eq!(runtime.state(), RuntimeState::Running);
    assert_eq!(runtime.world().resource::<RuntimeTrace>().startup_runs, 1);

    let pause = accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    assert_eq!(
        runtime.control_status(pause),
        Some(RuntimeControlStatus::Pending)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(
        runtime.control_status(pause),
        Some(RuntimeControlStatus::Applied)
    );

    let resume = accepted_ticket(runtime.request_control(RuntimeControl::Resume));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Running);
    assert_eq!(
        runtime.control_status(resume),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(runtime.world().resource::<RuntimeTrace>().startup_runs, 1);
}

#[test]
fn exact_step_preserves_fixed_debt_and_remainder_and_rotates_one_tick() {
    let fixed_time = FixedTime::default()
        .with_max_steps_per_frame(1)
        .unwrap()
        .with_catch_up_policy(FixedCatchUpPolicy::PreserveDebt);
    let step = fixed_time.timestep();
    let mut runtime = start_runtime(configured_app(fixed_time));

    runtime.drive(step * 3 + step / 2).unwrap();
    let before = *runtime.world().resource::<FixedTime>();
    assert_eq!(before.tick(), 1);
    assert_eq!(before.debt(), step * 2);
    assert_eq!(before.remainder(), step / 2);

    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    let paused = *runtime.world().resource::<FixedTime>();
    let real_before_step = *runtime.world().resource::<RealTime>();
    let virtual_before_step = *runtime.world().resource::<VirtualTime>();
    let render_before_step = *runtime.world().resource::<RenderTime>();
    let settings_before_step = *runtime.world().resource::<RuntimeTimeSettings>();
    assert_eq!(paused.debt(), before.debt());
    assert_eq!(paused.remainder(), before.remainder());

    let exact_step = accepted_ticket(runtime.request_control(RuntimeControl::StepFixedTick));
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Rejected(RuntimeControlRejection::Busy)
    ));
    let step_real_delta = Duration::from_millis(7);
    runtime.drive(step_real_delta).unwrap();

    let after = *runtime.world().resource::<FixedTime>();
    let real_after_step = *runtime.world().resource::<RealTime>();
    let virtual_after_step = *runtime.world().resource::<VirtualTime>();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(
        runtime.control_status(exact_step),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(after.tick(), before.tick() + 1);
    assert_eq!(after.elapsed(), before.elapsed() + step);
    assert_eq!(after.debt(), before.debt());
    assert_eq!(after.remainder(), before.remainder());
    assert_eq!(real_after_step.delta, step_real_delta);
    assert_eq!(
        real_after_step.elapsed,
        real_before_step.elapsed + step_real_delta
    );
    assert_eq!(real_after_step.frame, real_before_step.frame + 1);
    assert_eq!(virtual_after_step.delta, step);
    assert_eq!(
        virtual_after_step.elapsed,
        virtual_before_step.elapsed + step
    );
    assert_eq!(virtual_after_step.frame, virtual_before_step.frame + 1);
    assert_eq!(
        *runtime.world().resource::<RenderTime>(),
        render_before_step
    );
    assert_eq!(
        *runtime.world().resource::<RuntimeTimeSettings>(),
        settings_before_step
    );
    assert_eq!(
        runtime.world().resource::<RuntimeTrace>().fixed_ticks,
        [1, 2]
    );
}

#[derive(Debug, Default, Resource)]
struct GameplayStepTrace {
    consume_runs: usize,
    consumed_commands: usize,
}

fn record_gameplay_step(batch: Res<GameplayCommandBatch>, mut trace: ResMut<GameplayStepTrace>) {
    trace.consume_runs += 1;
    trace.consumed_commands += batch.len();
}

#[test]
fn exact_step_runs_one_complete_gameplay_command_transaction() {
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(GameplayCommandPlugin::default())
        .unwrap()
        .insert_resource(GameplayStepTrace::default())
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            record_gameplay_step.in_set(GameplayCommandSet::Consume),
        )
        .unwrap();
    let mut runtime = start_runtime(app);
    runtime
        .with_driver_scope(|scope| {
            submit_gameplay_driver_command(
                scope,
                GameplayCommandSubmission::new(
                    GameplayCommandTick::new(1).unwrap(),
                    GameplayCommandIngressSource::test("runtime-step").unwrap(),
                    GameplayCommandSourceSequence::new(1).unwrap(),
                    GameplayCommandDraft::new(GameplayCommandTypeId::new("runtime.step").unwrap()),
                ),
            )
            .unwrap()
            .unwrap();
        })
        .unwrap();

    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    accepted_ticket(runtime.request_control(RuntimeControl::StepFixedTick));
    runtime.drive(Duration::ZERO).unwrap();

    let trace = runtime.world().resource::<GameplayStepTrace>();
    assert_eq!(trace.consume_runs, 1);
    assert_eq!(trace.consumed_commands, 1);
    assert!(
        runtime
            .world()
            .resource::<GameplayCommandBatch>()
            .is_empty()
    );
    let stats = runtime.world().resource::<GameplayCommandQueue>().stats();
    assert_eq!(stats.admitted, 1);
    assert_eq!(stats.acknowledged, 1);
    assert_eq!(stats.acknowledged_through_tick, 1);
}

#[derive(Debug)]
struct TestCloseParticipant {
    released: Arc<AtomicBool>,
    begins: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
}

#[derive(Debug, Clone, Copy)]
enum PanicClosePhase {
    Begin,
    Poll,
}

#[derive(Debug)]
struct PanickingCloseParticipant {
    phase: PanicClosePhase,
    has_panicked: Arc<AtomicBool>,
    begins: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl Drop for PanickingCloseParticipant {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl RuntimeCloseParticipant for PanickingCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::SeqCst);
        if matches!(self.phase, PanicClosePhase::Begin)
            && !self.has_panicked.swap(true, Ordering::SeqCst)
        {
            panic!("injected begin-close panic");
        }
        Ok(if matches!(self.phase, PanicClosePhase::Begin) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if matches!(self.phase, PanicClosePhase::Poll)
            && !self.has_panicked.swap(true, Ordering::SeqCst)
        {
            panic!("injected poll-close panic");
        }
        Ok(RuntimeCloseProgress::Complete)
    }
}

#[derive(Debug)]
struct CompletingCloseParticipant {
    begins: Arc<AtomicUsize>,
}

impl RuntimeCloseParticipant for CompletingCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Complete)
    }
}

#[derive(Debug)]
struct ReportingCloseParticipant {
    reporter: RuntimeFaultReporter,
    fault: RuntimeFault,
}

impl RuntimeCloseParticipant for ReportingCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.reporter.report(self.fault.clone());
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Complete)
    }
}

#[derive(Debug)]
struct ProtectedFaultBridgeCloseParticipant {
    rejected: Arc<AtomicBool>,
}

impl RuntimeCloseParticipant for ProtectedFaultBridgeCloseParticipant {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.rejected.store(
            matches!(
                context.resource_mut::<FallbackErrorHandler>(),
                Err(RuntimeWorldAccessError::ProtectedType { .. })
            ),
            Ordering::Release,
        );
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Complete)
    }
}

#[derive(Debug)]
struct RetainedCloseParticipant {
    released: Arc<AtomicBool>,
    begins: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct RetryableBeginCloseParticipant {
    begins: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
}

impl RuntimeCloseParticipant for RetryableBeginCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let attempt = self.begins.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            Err(RuntimeCloseParticipantError::retryable(
                "nara.test.retryable-begin",
            ))
        } else {
            Ok(RuntimeCloseProgress::Complete)
        }
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Complete)
    }
}

impl Drop for RetainedCloseParticipant {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl RuntimeCloseParticipant for RetainedCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

#[derive(Debug)]
struct RegistrationOwner {
    begins: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl Drop for RegistrationOwner {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl RuntimeCloseParticipant for RegistrationOwner {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Complete)
    }
}

#[test]
fn duplicate_obligation_registration_returns_the_unconsumed_owner() {
    let duplicate_id = RuntimeCloseParticipantId::new("nara.test.duplicate-owner");
    let begins = Arc::new(AtomicUsize::new(0));
    let first_drops = Arc::new(AtomicUsize::new(0));
    let second_drops = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            duplicate_id,
            RegistrationOwner {
                begins: begins.clone(),
                drops: first_drops.clone(),
            },
        )
        .unwrap();

    let failure = obligations
        .register(
            duplicate_id,
            RegistrationOwner {
                begins: begins.clone(),
                drops: second_drops.clone(),
            },
        )
        .unwrap_err();
    assert_eq!(first_drops.load(Ordering::SeqCst), 0);
    assert_eq!(second_drops.load(Ordering::SeqCst), 0);
    let (error, owner) = failure.into_parts();
    assert_eq!(
        error,
        nara::app::RuntimeObligationLedgerError::Duplicate { id: duplicate_id }
    );
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.returned-owner"),
            owner,
        )
        .unwrap();

    let candidate = admit_candidate_with(
        configured_app(FixedTime::default()).seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    let mut runtime = candidate.complete_startup().unwrap().promote();
    accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(begins.load(Ordering::SeqCst), 2);
    drop(runtime);
    assert_eq!(first_drops.load(Ordering::SeqCst), 1);
    assert_eq!(second_drops.load(Ordering::SeqCst), 1);
}

#[test]
fn stop_is_observable_as_stopping_before_close_work_runs() {
    let begins = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.observable-stopping"),
            RegistrationOwner {
                begins: begins.clone(),
                drops,
            },
        )
        .unwrap();
    let mut runtime = admit_candidate_with(
        configured_app(FixedTime::default()).seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap()
    .complete_startup()
    .unwrap()
    .promote();

    let stop = accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    let stopping = runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(stopping.state(), RuntimeState::Stopping);
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Pending)
    );
    assert_eq!(begins.load(Ordering::SeqCst), 0);

    let stopped = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(stopped.state(), RuntimeState::Stopped);
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(begins.load(Ordering::SeqCst), 1);
}

impl RuntimeCloseParticipant for TestCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

#[test]
fn close_participant_panics_unwind_out_of_runtime_drive() {
    for phase in [PanicClosePhase::Begin, PanicClosePhase::Poll] {
        let quarantine_before = runtime_quarantine_status();
        let has_panicked = Arc::new(AtomicBool::new(false));
        let panicking_begins = Arc::new(AtomicUsize::new(0));
        let panicking_polls = Arc::new(AtomicUsize::new(0));
        let panicking_drops = Arc::new(AtomicUsize::new(0));
        let completing_begins = Arc::new(AtomicUsize::new(0));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.completing-close"),
                CompletingCloseParticipant {
                    begins: completing_begins.clone(),
                },
            )
            .unwrap();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.panicking-close"),
                PanickingCloseParticipant {
                    phase,
                    has_panicked: has_panicked.clone(),
                    begins: panicking_begins.clone(),
                    polls: panicking_polls.clone(),
                    drops: panicking_drops.clone(),
                },
            )
            .unwrap();
        let candidate = admit_candidate_with(
            configured_app(FixedTime::default()).seal().unwrap(),
            obligations,
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();

        accepted_ticket(runtime.request_control(RuntimeControl::Stop));
        assert_eq!(
            runtime.drive(Duration::ZERO).unwrap().state(),
            RuntimeState::Stopping
        );
        let drive = catch_unwind(AssertUnwindSafe(|| runtime.drive(Duration::ZERO)));

        assert!(drive.is_err(), "close participant panic was contained");
        assert_eq!(runtime.state(), RuntimeState::Stopping);
        assert_eq!(panicking_begins.load(Ordering::SeqCst), 1);
        assert_eq!(
            panicking_polls.load(Ordering::SeqCst),
            usize::from(matches!(phase, PanicClosePhase::Poll))
        );
        assert_eq!(completing_begins.load(Ordering::SeqCst), 0);
        assert_eq!(panicking_drops.load(Ordering::SeqCst), 0);
        drop(runtime);
        assert!(has_panicked.load(Ordering::SeqCst));
        assert_eq!(completing_begins.load(Ordering::SeqCst), 1);
        assert_eq!(panicking_drops.load(Ordering::SeqCst), 1);
        assert_eq!(
            runtime_quarantine_status().process_retained(),
            quarantine_before.process_retained()
        );
    }
}

#[test]
fn close_participant_panic_during_drop_retains_the_owner_for_retry() {
    for phase in [PanicClosePhase::Begin, PanicClosePhase::Poll] {
        let quarantine_before = runtime_quarantine_status();
        let has_panicked = Arc::new(AtomicBool::new(false));
        let drops = Arc::new(AtomicUsize::new(0));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.drop-panic-close"),
                PanickingCloseParticipant {
                    phase,
                    has_panicked: has_panicked.clone(),
                    begins: Arc::new(AtomicUsize::new(0)),
                    polls: Arc::new(AtomicUsize::new(0)),
                    drops: drops.clone(),
                },
            )
            .unwrap();
        let runtime = admit_candidate_with(
            configured_app(FixedTime::default()).seal().unwrap(),
            obligations,
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();

        let dropped = catch_unwind(AssertUnwindSafe(|| drop(runtime)));

        assert!(dropped.is_err(), "close participant panic was contained");
        assert!(has_panicked.load(Ordering::SeqCst));
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime_quarantine_status().current_thread_retained(),
            quarantine_before.current_thread_retained() + 1
        );

        let reaped = drive_runtime_quarantine();
        assert_eq!(
            reaped.current_thread_retained(),
            quarantine_before.current_thread_retained()
        );
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn consuming_retirement_retries_only_unfinished_owners_and_preserves_evidence() {
    let released = Arc::new(AtomicBool::new(false));
    let retained_begins = Arc::new(AtomicUsize::new(0));
    let retained_polls = Arc::new(AtomicUsize::new(0));
    let retained_drops = Arc::new(AtomicUsize::new(0));
    let completed_begins = Arc::new(AtomicUsize::new(0));
    let retry_begins = Arc::new(AtomicUsize::new(0));
    let retry_polls = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.completed-retirement"),
            CompletingCloseParticipant {
                begins: completed_begins.clone(),
            },
        )
        .unwrap();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.retained-retirement"),
            RetainedCloseParticipant {
                released: released.clone(),
                begins: retained_begins.clone(),
                polls: retained_polls.clone(),
                drops: retained_drops.clone(),
            },
        )
        .unwrap();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.retryable-retirement"),
            RetryableBeginCloseParticipant {
                begins: retry_begins.clone(),
                polls: retry_polls.clone(),
            },
        )
        .unwrap();
    let candidate = admit_candidate_with(
        configured_app(FixedTime::default()).seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    let runtime = candidate.complete_startup().unwrap().promote();
    let mut retirement = runtime.begin_retirement();

    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(completed_begins.load(Ordering::SeqCst), 1);
    assert_eq!(retained_begins.load(Ordering::SeqCst), 1);
    assert_eq!(retained_polls.load(Ordering::SeqCst), 1);
    assert_eq!(retry_begins.load(Ordering::SeqCst), 1);
    assert_eq!(retry_polls.load(Ordering::SeqCst), 0);
    assert!(
        retirement
            .close_evidence()
            .causes()
            .contains(&RuntimeCloseCause::ParticipantError {
                participant: RuntimeCloseParticipantId::new("nara.test.retryable-retirement"),
                phase: RuntimeCloseParticipantPhase::Begin,
                code: "nara.test.retryable-begin",
                disposition: nara::app::RuntimeCloseErrorDisposition::Retryable,
            })
    );

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(completed_begins.load(Ordering::SeqCst), 1);
    assert_eq!(retained_begins.load(Ordering::SeqCst), 1);
    assert_eq!(retained_polls.load(Ordering::SeqCst), 2);
    assert_eq!(retry_begins.load(Ordering::SeqCst), 2);
    assert_eq!(retry_polls.load(Ordering::SeqCst), 0);
    assert_eq!(retained_drops.load(Ordering::SeqCst), 0);
    drop(retirement);
    assert_eq!(retained_drops.load(Ordering::SeqCst), 1);
}

#[test]
fn dropping_an_incomplete_runtime_does_not_destroy_its_close_owner() {
    let quarantine_before = runtime_quarantine_status();
    let released = Arc::new(AtomicBool::new(false));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.drop-retained-owner"),
            RetainedCloseParticipant {
                released: released.clone(),
                begins: Arc::new(AtomicUsize::new(0)),
                polls: Arc::new(AtomicUsize::new(0)),
                drops: drops.clone(),
            },
        )
        .unwrap();
    let runtime = admit_candidate_with(
        configured_app(FixedTime::default()).seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap()
    .complete_startup()
    .unwrap()
    .promote();

    drop(runtime);

    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let retained = runtime_quarantine_status();
    assert_eq!(
        retained.current_thread_retained(),
        quarantine_before.current_thread_retained() + 1
    );
    assert_eq!(
        retained.process_retained(),
        quarantine_before.process_retained() + 1
    );

    released.store(true, Ordering::SeqCst);
    let reaped = drive_runtime_quarantine();
    assert_eq!(
        reaped.current_thread_retained(),
        quarantine_before.current_thread_retained()
    );
    assert_eq!(
        reaped.process_retained(),
        quarantine_before.process_retained()
    );
    assert_eq!(reaped.total_reaped(), quarantine_before.total_reaped() + 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn quarantine_is_process_observable_and_driven_on_the_owner_thread() {
    let process_before = runtime_quarantine_status();
    let released = Arc::new(AtomicBool::new(false));
    let drops = Arc::new(AtomicUsize::new(0));
    let (status_sender, status_receiver) = mpsc::sync_channel(2);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let worker_released = released.clone();
    let worker_drops = drops.clone();

    let worker = std::thread::spawn(move || {
        let thread_before = runtime_quarantine_status();
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.thread-affine-quarantine"),
                RetainedCloseParticipant {
                    released: worker_released.clone(),
                    begins: Arc::new(AtomicUsize::new(0)),
                    polls: Arc::new(AtomicUsize::new(0)),
                    drops: worker_drops,
                },
            )
            .unwrap();
        let runtime = admit_candidate_with(
            configured_app(FixedTime::default()).seal().unwrap(),
            obligations,
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();

        drop(runtime);
        status_sender
            .send((thread_before, runtime_quarantine_status()))
            .unwrap();
        release_receiver.recv().unwrap();
        worker_released.store(true, Ordering::SeqCst);
        status_sender
            .send((thread_before, drive_runtime_quarantine()))
            .unwrap();
    });

    let (thread_before, retained) = status_receiver.recv().unwrap();
    assert_eq!(
        retained.current_thread_retained(),
        thread_before.current_thread_retained() + 1
    );
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        process_before.process_retained() + 1
    );
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        process_before.current_thread_retained()
    );

    release_sender.send(()).unwrap();
    let (thread_before, reaped) = status_receiver.recv().unwrap();
    worker.join().unwrap();
    assert_eq!(
        reaped.current_thread_retained(),
        thread_before.current_thread_retained()
    );
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        process_before.process_retained()
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn dropping_multiple_runtimes_with_blocked_task_owners_is_bounded_and_reapable() {
    const RUNTIME_COUNT: usize = 3;

    let quarantine_before = runtime_quarantine_status();
    let mut releases = Vec::with_capacity(RUNTIME_COUNT);
    let mut exits = Vec::with_capacity(RUNTIME_COUNT);

    for index in 0..RUNTIME_COUNT {
        let mut app = configured_app(FixedTime::default());
        app.add_plugin(TaskPlugin::new(single_worker_task_config()))
            .unwrap();
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::sync_channel::<()>(1);
        let (exit_sender, exit_receiver) = mpsc::sync_channel(1);
        let handle = app
            .world()
            .resource::<TaskPools>()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(1, TaskDomainKey::new(100 + index as u64)),
                move |_| {
                    started_sender.send(()).unwrap();
                    let _ = release_receiver.recv();
                    exit_sender.send(()).unwrap();
                },
            )
            .into_handle()
            .unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("blocked task did not start before the deadline");
        drop(handle);

        let runtime = admit_candidate_with(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();
        drop(runtime);

        releases.push(release_sender);
        exits.push(exit_receiver);
    }

    let retained = runtime_quarantine_status();
    assert_eq!(
        retained.current_thread_retained(),
        quarantine_before.current_thread_retained() + RUNTIME_COUNT
    );
    assert!(retained.current_thread_retained() <= retained.current_thread_capacity());
    assert!(retained.process_retained() <= retained.process_capacity());

    drop(releases);
    for exit in exits {
        exit.recv_timeout(Duration::from_secs(10))
            .expect("blocked task did not exit before the deadline");
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut reaped = drive_runtime_quarantine();
    while reaped.current_thread_retained() != quarantine_before.current_thread_retained()
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(1));
        reaped = drive_runtime_quarantine();
    }

    assert_eq!(
        reaped.current_thread_retained(),
        quarantine_before.current_thread_retained()
    );
    assert!(reaped.total_reaped() >= quarantine_before.total_reaped() + RUNTIME_COUNT as u64);
}

#[derive(Debug, Resource)]
struct CallerOwnedResource(Arc<AtomicUsize>);

impl Drop for CallerOwnedResource {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct ShutdownCountPlugin {
    shutdowns: Arc<AtomicUsize>,
}

const SHUTDOWN_COUNT_PLUGIN_ID: PluginId = PluginId::new("nara.test.shutdown-count");
const SHUTDOWN_COUNT_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SHUTDOWN_COUNT_PLUGIN_ID, PluginCategory::Runtime);

impl Plugin for ShutdownCountPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SHUTDOWN_COUNT_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Debug)]
struct FailingRuntimeShutdownPlugin;

const FAILING_RUNTIME_SHUTDOWN_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.runtime-shutdown-failure");
const FAILING_RUNTIME_SHUTDOWN_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FAILING_RUNTIME_SHUTDOWN_PLUGIN_ID, PluginCategory::Runtime);

impl Plugin for FailingRuntimeShutdownPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FAILING_RUNTIME_SHUTDOWN_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: FAILING_RUNTIME_SHUTDOWN_PLUGIN_ID,
            message: "injected runtime shutdown failure".to_owned(),
        })
    }
}

#[test]
fn plugin_shutdown_failure_is_terminal_and_visible_without_claiming_close_success() {
    let quarantine_before = runtime_quarantine_status();
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(FailingRuntimeShutdownPlugin).unwrap();
    let mut runtime = start_runtime(app);

    let stop = accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert!(runtime.close_evidence().plugin_shutdown_failed());
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::CloseFailed
        ))
    );
    drop(runtime);
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        quarantine_before.process_retained()
    );
}

#[test]
fn close_time_fault_remains_visible_after_runtime_stops() {
    let app = configured_app(FixedTime::default());
    let reporter = app.world().resource::<RuntimeFaultReporter>().clone();
    let fault = RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.close-time-service",
    );
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.reporting-close"),
            ReportingCloseParticipant {
                reporter,
                fault: fault.clone(),
            },
        )
        .unwrap();
    let candidate = admit_candidate_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::default(),
    )
    .unwrap();
    let mut runtime = candidate.complete_startup().unwrap().promote();

    accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(runtime.fault(), Some(&fault));
}

#[test]
fn close_scope_protects_the_runtime_error_handler() {
    let rejected = Arc::new(AtomicBool::new(false));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.protected-handler-close"),
            ProtectedFaultBridgeCloseParticipant {
                rejected: rejected.clone(),
            },
        )
        .unwrap();
    let mut runtime = admit_candidate_with(
        configured_app(FixedTime::default()).seal().unwrap(),
        obligations,
        RuntimeClosePolicy::default(),
    )
    .unwrap()
    .complete_startup()
    .unwrap()
    .promote();

    accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert!(rejected.load(Ordering::Acquire));
    assert_eq!(runtime.fault(), None);
}

#[test]
fn close_is_retryable_and_proves_only_explicitly_transferred_owners() {
    let released = Arc::new(AtomicBool::new(false));
    let begins = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let caller_resource_drops = Arc::new(AtomicUsize::new(0));
    let plugin_shutdowns = Arc::new(AtomicUsize::new(0));

    let mut app = configured_app(FixedTime::default());
    app.add_plugin(ShutdownCountPlugin {
        shutdowns: plugin_shutdowns.clone(),
    })
    .unwrap();
    app.insert_resource(CallerOwnedResource(caller_resource_drops.clone()))
        .unwrap();
    let sealed = app.seal().unwrap();
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.waitable-close"),
            TestCloseParticipant {
                released: released.clone(),
                begins: begins.clone(),
                polls: polls.clone(),
            },
        )
        .unwrap();
    let candidate =
        admit_candidate_with(sealed, obligations, RuntimeClosePolicy::new(Duration::ZERO)).unwrap();
    let mut runtime = match candidate.complete_startup() {
        Ok(ready) => ready.promote(),
        Err(failure) => {
            panic!("candidate startup failed: {:?}", failure.fault())
        }
    };

    accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(plugin_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(caller_resource_drops.load(Ordering::SeqCst), 0);
    assert!(runtime.world().contains_resource::<CallerOwnedResource>());
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Rejected(RuntimeControlRejection::InvalidState {
            state: RuntimeState::CloseIncomplete,
            control: RuntimeControl::Stop,
        })
    ));

    let incomplete_retry = accepted_ticket(runtime.request_control(RuntimeControl::RetryClose));
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Rejected(RuntimeControlRejection::InvalidState {
            state: RuntimeState::CloseIncomplete,
            control: RuntimeControl::Stop,
        })
    ));
    assert_eq!(
        runtime.control_status(incomplete_retry),
        Some(RuntimeControlStatus::Pending)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(
        runtime.control_status(incomplete_retry),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::CloseIncomplete
        ))
    );

    released.store(true, Ordering::SeqCst);
    let close_fault =
        RuntimeFault::engine(RuntimeFaultKind::RequiredService, "nara.test.close-service");
    runtime.fault_reporter().report(close_fault.clone());
    let completed_retry = accepted_ticket(runtime.request_control(RuntimeControl::RetryClose));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    assert_eq!(plugin_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(
        runtime.control_status(completed_retry),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(runtime.fault(), Some(&close_fault));
    assert_eq!(caller_resource_drops.load(Ordering::SeqCst), 0);
    assert!(runtime.world().contains_resource::<CallerOwnedResource>());
}

#[test]
fn dropping_a_live_runtime_begins_best_effort_close_once() {
    let quarantine_before = runtime_quarantine_status();
    let baseline_reservations = reserve_remaining_runtime_routes();
    let baseline_available = baseline_reservations.len();
    drop(baseline_reservations);
    let released = Arc::new(AtomicBool::new(false));
    let begins = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let plugin_shutdowns = Arc::new(AtomicUsize::new(0));
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(ShutdownCountPlugin {
        shutdowns: plugin_shutdowns.clone(),
    })
    .unwrap();
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.drop-close"),
            TestCloseParticipant {
                released: released.clone(),
                begins: begins.clone(),
                polls: polls.clone(),
            },
        )
        .unwrap();
    let candidate = admit_candidate_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    let runtime = candidate.complete_startup().unwrap().promote();

    drop(runtime);

    assert_eq!(plugin_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        quarantine_before.process_retained() + 1
    );
    let reservations = reserve_remaining_runtime_routes();
    assert_eq!(
        reservations.len() + 1,
        baseline_available,
        "a quarantined managed owner must retain its route"
    );

    released.store(true, Ordering::SeqCst);
    let reaped = drive_runtime_quarantine();
    assert_eq!(
        reaped.process_retained(),
        quarantine_before.process_retained()
    );
    let released_route = RuntimeAdmissionReservation::try_acquire()
        .expect("reaping the quarantined owner releases its route");
    drop(released_route);
    drop(reservations);
}

#[test]
fn admission_failure_retires_plugin_then_host_and_retries_only_pending_owner() {
    const PLUGIN_ID: PluginId = PluginId::new("nara.test.admission.plugin-owner");
    const OBLIGATION_ID: PluginShutdownObligationId =
        PluginShutdownObligationId::new("nara.test.admission.plugin-owner");
    const PLUGIN_PARTICIPANT_ID: RuntimeCloseParticipantId =
        RuntimeCloseParticipantId::new("nara.test.admission.plugin-owner");
    const HOST_PARTICIPANT_ID: RuntimeCloseParticipantId =
        RuntimeCloseParticipantId::new("nara.test.admission.host-owner");
    const DECLARATION: PluginDeclaration =
        PluginDeclaration::new(PLUGIN_ID, PluginCategory::Runtime)
            .shutdown_obligations(&[OBLIGATION_ID]);

    #[derive(Debug)]
    struct AdmissionOwnerPlugin {
        log: Arc<Mutex<Vec<&'static str>>>,
        released: Arc<AtomicBool>,
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    impl Plugin for AdmissionOwnerPlugin {
        fn declaration() -> &'static PluginDeclaration {
            &DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.register_plugin_runtime_close_participant(
                OBLIGATION_ID,
                PLUGIN_PARTICIPANT_ID,
                RetainedAdmissionOwner {
                    log: Arc::clone(&self.log),
                    released: Arc::clone(&self.released),
                    begins: Arc::clone(&self.begins),
                    polls: Arc::clone(&self.polls),
                },
            )?;
            Ok(())
        }
    }

    struct RetainedAdmissionOwner {
        log: Arc<Mutex<Vec<&'static str>>>,
        released: Arc<AtomicBool>,
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    impl RuntimeCloseParticipant for RetainedAdmissionOwner {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.begins.fetch_add(1, Ordering::SeqCst);
            self.log
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push("plugin");
            Ok(RuntimeCloseProgress::Pending)
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(if self.released.load(Ordering::SeqCst) {
                RuntimeCloseProgress::Complete
            } else {
                RuntimeCloseProgress::Pending
            })
        }
    }

    struct ImmediateAdmissionOwner {
        log: Arc<Mutex<Vec<&'static str>>>,
        begins: Arc<AtomicUsize>,
    }

    impl RuntimeCloseParticipant for ImmediateAdmissionOwner {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.begins.fetch_add(1, Ordering::SeqCst);
            self.log
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push("host");
            Ok(RuntimeCloseProgress::Complete)
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            panic!("an immediately complete owner must not be polled")
        }
    }

    let baseline_reservations = reserve_remaining_runtime_routes();
    let route_capacity = baseline_reservations.len();
    drop(baseline_reservations);
    let log = Arc::new(Mutex::new(Vec::new()));
    let released = Arc::new(AtomicBool::new(false));
    let plugin_begins = Arc::new(AtomicUsize::new(0));
    let plugin_polls = Arc::new(AtomicUsize::new(0));
    let host_begins = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            HOST_PARTICIPANT_ID,
            ImmediateAdmissionOwner {
                log: Arc::clone(&log),
                begins: Arc::clone(&host_begins),
            },
        )
        .unwrap();
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(AdmissionOwnerPlugin {
        log: Arc::clone(&log),
        released: Arc::clone(&released),
        begins: Arc::clone(&plugin_begins),
        polls: Arc::clone(&plugin_polls),
    })
    .unwrap();
    app.set_runner(|_| Ok(nara::app::AppExit::Success)).unwrap();

    let failure = admit_candidate_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap_err();

    assert_eq!(failure.error(), RuntimeAdmissionError::RawRunnerInstalled);
    let retained_route_reservations = reserve_remaining_runtime_routes();
    assert_eq!(retained_route_reservations.len() + 1, route_capacity);
    assert_eq!(plugin_begins.load(Ordering::SeqCst), 0);
    assert_eq!(host_begins.load(Ordering::SeqCst), 0);
    let mut retirement = failure.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(plugin_begins.load(Ordering::SeqCst), 1);
    assert_eq!(plugin_polls.load(Ordering::SeqCst), 1);
    assert_eq!(host_begins.load(Ordering::SeqCst), 1);
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["plugin", "host"]
    );

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(plugin_begins.load(Ordering::SeqCst), 1);
    assert_eq!(plugin_polls.load(Ordering::SeqCst), 2);
    assert_eq!(host_begins.load(Ordering::SeqCst), 1);
    assert_eq!(
        *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        ["plugin", "host"]
    );
    let released_route = RuntimeAdmissionReservation::try_acquire()
        .expect("truthful admission-failure retirement releases its route");
    drop(released_route);
    drop(retained_route_reservations);
}

#[test]
fn reservation_saturation_precedes_app_and_obligation_transfer() {
    let mut reservations = reserve_remaining_runtime_routes();
    let capacity = reservations.len();
    assert!(capacity >= 131);
    assert_eq!(reservations.len(), capacity);

    let sealed = configured_app(FixedTime::default()).seal().unwrap();
    let obligations = RuntimeObligationLedger::new();
    drop(
        reservations
            .pop()
            .expect("saturated capacity contains one releasable reservation"),
    );
    let candidate = RuntimeAdmissionReservation::try_acquire()
        .expect("releasing one reservation restores capacity")
        .admit(sealed, obligations, RuntimeClosePolicy::default())
        .expect("inputs remain owned by the caller until explicit admission");
    let runtime = candidate.complete_startup().unwrap().promote();
    finish_runtime(runtime);
    drop(reservations);
}

fn reserve_remaining_runtime_routes() -> Vec<RuntimeAdmissionReservation> {
    let mut reservations = Vec::new();
    while let Ok(reservation) = RuntimeAdmissionReservation::try_acquire() {
        reservations.push(reservation);
    }
    reservations
}

#[test]
fn close_incomplete_retains_its_fault_route_until_truthful_retirement() {
    let baseline_reservations = reserve_remaining_runtime_routes();
    let capacity = baseline_reservations.len();
    drop(baseline_reservations);
    let released = Arc::new(AtomicBool::new(false));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.route-retention"),
            TestCloseParticipant {
                released: Arc::clone(&released),
                begins: Arc::new(AtomicUsize::new(0)),
                polls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .unwrap();
    let runtime = admit_candidate_with(
        configured_app(FixedTime::default()).seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap()
    .complete_startup()
    .unwrap()
    .promote();
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );

    let reservations = reserve_remaining_runtime_routes();
    assert_eq!(reservations.len() + 1, capacity);

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    let released_route = RuntimeAdmissionReservation::try_acquire()
        .expect("truthful retirement releases exactly one route");
    drop(released_route);
    drop(reservations);
}

#[test]
fn admission_panic_retains_its_owner_and_route_in_process_quarantine() {
    let quarantine_before = runtime_quarantine_status();
    let baseline_reservations = reserve_remaining_runtime_routes();
    let route_capacity = baseline_reservations.len();
    drop(baseline_reservations);

    let mut app = configured_app(FixedTime::default());
    app.world_mut().unwrap().add_observer(
        |_: On<nara::ecs::lifecycle::Add, nara::ecs::observer::Observer>| {
            panic!("injected observer materialization panic");
        },
    );
    app.add_observer(count_raw_observer).unwrap();
    let sealed = app.seal().unwrap();

    let admission = catch_unwind(AssertUnwindSafe(|| {
        RuntimeAdmissionReservation::try_acquire()
            .expect("the panic test has one runtime route")
            .admit(
                sealed,
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
    }));

    assert!(admission.is_err());
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        quarantine_before.process_retained() + 1
    );
    let retained_route_reservations = reserve_remaining_runtime_routes();
    assert_eq!(retained_route_reservations.len() + 1, route_capacity);

    let reaped = drive_runtime_quarantine();
    assert_eq!(
        reaped.process_retained(),
        quarantine_before.process_retained()
    );
    let released_route = RuntimeAdmissionReservation::try_acquire()
        .expect("reaping the admission panic owner releases its route");
    drop(released_route);
    drop(retained_route_reservations);
}

#[test]
fn dropping_an_admission_failure_transfers_its_owner_to_quarantine() {
    let quarantine_before = runtime_quarantine_status();
    let released = Arc::new(AtomicBool::new(false));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.dropped-admission-owner"),
            RetainedCloseParticipant {
                released: released.clone(),
                begins: Arc::new(AtomicUsize::new(0)),
                polls: Arc::new(AtomicUsize::new(0)),
                drops: drops.clone(),
            },
        )
        .unwrap();
    let mut app = configured_app(FixedTime::default());
    app.set_runner(|_| Ok(nara::app::AppExit::Success)).unwrap();
    let failure = admit_candidate_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap_err();

    drop(failure);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        quarantine_before.process_retained() + 1
    );

    released.store(true, Ordering::SeqCst);
    let reaped = drive_runtime_quarantine();
    assert_eq!(
        reaped.process_retained(),
        quarantine_before.process_retained()
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn already_started_app_admission_returns_an_owned_retirement_path() {
    let mut app = configured_app(FixedTime::default());
    app.run_once(Duration::ZERO).unwrap();
    let failure = admit_candidate(app.seal().unwrap()).unwrap_err();

    assert_eq!(failure.error(), RuntimeAdmissionError::AppStarted);
    let mut retirement = failure.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn startup_failure_retains_pending_owners_until_explicit_retry() {
    let baseline_reservations = reserve_remaining_runtime_routes();
    let route_capacity = baseline_reservations.len();
    drop(baseline_reservations);
    let released = Arc::new(AtomicBool::new(false));
    let begins = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.startup-owner"),
            TestCloseParticipant {
                released: released.clone(),
                begins: begins.clone(),
                polls: polls.clone(),
            },
        )
        .unwrap();
    let mut app = configured_app(FixedTime::default());
    app.add_systems(StartupStage::Core, fail_system).unwrap();
    let candidate = admit_candidate_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();

    let Err(mut failure) = candidate.complete_startup() else {
        panic!("failing startup unexpectedly became ready");
    };
    assert_eq!(
        failure.retirement_state(),
        RuntimeCandidateRetirementState::Retiring
    );
    let retained_route_reservations = reserve_remaining_runtime_routes();
    assert_eq!(retained_route_reservations.len() + 1, route_capacity);
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 1);

    released.store(true, Ordering::SeqCst);
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert_eq!(begins.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    let released_route = RuntimeAdmissionReservation::try_acquire()
        .expect("truthful startup-failure retirement releases its route");
    drop(released_route);
    drop(retained_route_reservations);
}

#[test]
fn dropping_a_startup_failure_transfers_its_owner_to_quarantine() {
    let quarantine_before = runtime_quarantine_status();
    let released = Arc::new(AtomicBool::new(false));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.dropped-startup-owner"),
            RetainedCloseParticipant {
                released: released.clone(),
                begins: Arc::new(AtomicUsize::new(0)),
                polls: Arc::new(AtomicUsize::new(0)),
                drops: drops.clone(),
            },
        )
        .unwrap();
    let mut app = configured_app(FixedTime::default());
    app.add_systems(StartupStage::Core, fail_system).unwrap();
    let candidate = admit_candidate_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    let failure = candidate.complete_startup().unwrap_err();

    drop(failure);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        quarantine_before.process_retained() + 1
    );

    released.store(true, Ordering::SeqCst);
    let reaped = drive_runtime_quarantine();
    assert_eq!(
        reaped.process_retained(),
        quarantine_before.process_retained()
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn the_first_runtime_fault_is_sticky_and_generations_do_not_repeat() {
    let mut first = start_runtime(configured_app(FixedTime::default()));
    let second = start_runtime(configured_app(FixedTime::default()));
    assert_ne!(first.generation(), second.generation());

    let reporter = first.fault_reporter();
    let first_fault =
        RuntimeFault::engine(RuntimeFaultKind::RequiredTask, "nara.test.required-task");
    let later_fault = RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.required-service",
    );
    assert!(reporter.report(first_fault.clone()));
    assert!(!reporter.report(later_fault));

    let error = first.drive(Duration::ZERO).unwrap_err();
    assert_eq!(error.fault(), &first_fault);
    assert_eq!(first.state(), RuntimeState::Faulted);
    assert_eq!(first.fault(), Some(&first_fault));
}

#[test]
fn the_build_time_fault_reporter_remains_bound_to_the_runtime() {
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    let build_time_reporter = app.world().resource::<RuntimeFaultReporter>().clone();
    let mut runtime = start_runtime(app);
    let fault = RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.build-time-service",
    );

    assert!(build_time_reporter.report(fault.clone()));
    let error = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(error.fault(), &fault);
    assert_eq!(runtime.fault(), Some(&fault));
}

#[test]
fn candidate_rejects_shared_world_reporter_authority_before_startup() {
    let shared = RuntimeFaultReporter::new();
    let mut first_app = configured_app(FixedTime::default());
    first_app.insert_resource(shared.clone()).unwrap();
    let mut second_app = configured_app(FixedTime::default());
    second_app.insert_resource(shared).unwrap();
    for app in [first_app, second_app] {
        let candidate = admit_candidate(app.seal().unwrap()).unwrap();
        let mut failure = candidate.complete_startup().unwrap_err();

        assert_eq!(
            failure.fault().kind(),
            RuntimeFaultKind::FaultReporterAuthority
        );
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
    }
}

#[test]
fn missing_or_replaced_candidate_fault_reporter_prevents_startup() {
    for replace in [false, true] {
        let mut app = configured_app(FixedTime::default());
        let world = app.world_mut().unwrap();
        world.remove_resource::<RuntimeFaultReporter>();
        if replace {
            world.insert_resource(RuntimeFaultReporter::new());
        }
        let candidate = admit_candidate(app.seal().unwrap()).unwrap();

        let mut failure = candidate.complete_startup().unwrap_err();

        assert_eq!(
            failure.fault().kind(),
            RuntimeFaultKind::FaultReporterAuthority
        );
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
    }
}

#[test]
fn admission_rebinds_missing_or_replaced_error_handler_before_startup() {
    for replace in [false, true] {
        let mut app = app_with_runtime_scope_observer();
        let world = app.world_mut().unwrap();
        world.remove_resource::<FallbackErrorHandler>();
        if replace {
            world.insert_resource(FallbackErrorHandler(ignore_bevy_error));
        }
        app.add_systems(StartupStage::Runtime, trigger_runtime_scope_error)
            .unwrap();
        let candidate = admit_candidate(app.seal().unwrap()).unwrap();

        let mut failure = candidate.complete_startup().unwrap_err();

        assert_eq!(failure.fault().kind(), RuntimeFaultKind::Observer);
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
    }
}

#[derive(Debug, Resource)]
struct CandidateAdmissionProbe(u64);

#[test]
fn unpublished_candidate_scope_materializes_host_state_before_startup() {
    let app = configured_app(FixedTime::default());
    let mut candidate = admit_candidate(app.seal().unwrap()).unwrap();

    candidate
        .with_admission_scope(|scope| {
            scope.apply_command(|world: &mut World| {
                world.insert_resource(CandidateAdmissionProbe(7));
            });
        })
        .unwrap();

    let runtime = candidate.complete_startup().unwrap().promote();
    assert_eq!(runtime.world().resource::<CandidateAdmissionProbe>().0, 7);
    finish_runtime(runtime);
}

#[test]
fn unpublished_candidate_scope_rejects_replaced_fault_authority() {
    let app = configured_app(FixedTime::default());
    let mut candidate = admit_candidate(app.seal().unwrap()).unwrap();

    let error = candidate
        .with_admission_scope(|scope| {
            scope.apply_command(|world: &mut World| {
                world.insert_resource(FallbackErrorHandler(ignore_bevy_error));
            });
        })
        .unwrap_err();

    assert!(matches!(
        error,
        RuntimeScopeError::Faulted { ref fault }
            if fault.kind() == RuntimeFaultKind::FaultReporterAuthority
    ));
    let mut failure = candidate.complete_startup().unwrap_err();
    assert_eq!(
        failure.fault().kind(),
        RuntimeFaultKind::FaultReporterAuthority
    );
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn control_tickets_are_scoped_to_their_runtime_generation() {
    let mut first = start_runtime(configured_app(FixedTime::default()));
    let mut second = start_runtime(configured_app(FixedTime::default()));

    let first_pause = accepted_ticket(first.request_control(RuntimeControl::Pause));
    let second_pause = accepted_ticket(second.request_control(RuntimeControl::Pause));

    assert_ne!(first_pause, second_pause);
    assert_eq!(first.control_status(second_pause), None);
    assert_eq!(second.control_status(first_pause), None);
}

#[derive(Debug, Resource)]
struct FirstRuntimeOnly;

fn submit_runtime_isolation_command(
    runtime: &mut RuntimeInstance,
    tick: u64,
    source: &str,
    sequence: u64,
) {
    runtime
        .with_driver_scope(|scope| {
            submit_gameplay_driver_command(
                scope,
                GameplayCommandSubmission::new(
                    GameplayCommandTick::new(tick).unwrap(),
                    GameplayCommandIngressSource::test(source).unwrap(),
                    GameplayCommandSourceSequence::new(sequence).unwrap(),
                    GameplayCommandDraft::new(
                        GameplayCommandTypeId::new("runtime.isolation").unwrap(),
                    ),
                ),
            )
            .unwrap()
            .unwrap();
        })
        .unwrap();
}

#[test]
fn alternating_runtime_drives_do_not_share_world_time_or_gameplay_queue_state() {
    let mut first_app = configured_app(FixedTime::default());
    first_app
        .add_plugin(GameplayCommandPlugin::default())
        .unwrap();
    first_app.insert_resource(FirstRuntimeOnly).unwrap();
    let mut second_app = configured_app(FixedTime::default());
    second_app
        .add_plugin(GameplayCommandPlugin::default())
        .unwrap();
    let mut first = start_runtime(first_app);
    let mut second = start_runtime(second_app);

    submit_runtime_isolation_command(&mut first, 1, "first-runtime", 1);
    first.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    submit_runtime_isolation_command(&mut second, 1, "second-runtime", 1);
    second.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    submit_runtime_isolation_command(&mut first, 2, "first-runtime", 2);
    first.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();

    assert_eq!(first.world().resource::<FixedTime>().tick(), 2);
    assert_eq!(second.world().resource::<FixedTime>().tick(), 1);
    assert!(first.world().contains_resource::<FirstRuntimeOnly>());
    assert!(!second.world().contains_resource::<FirstRuntimeOnly>());
    assert_eq!(
        first
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        2
    );
    assert_eq!(
        second
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        1
    );
}

#[test]
fn a_reported_fault_prevents_a_pending_exact_step() {
    let mut runtime = start_runtime(configured_app(FixedTime::default()));
    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    let tick_before = runtime.world().resource::<FixedTime>().tick();
    let step = accepted_ticket(runtime.request_control(RuntimeControl::StepFixedTick));
    runtime.fault_reporter().report(RuntimeFault::engine(
        RuntimeFaultKind::RequiredTask,
        "nara.test.pre-step-task",
    ));

    runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), tick_before);
    assert_eq!(
        runtime.control_status(step),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::RuntimeFaulted
        ))
    );
}

#[test]
fn stopped_is_terminal_even_when_the_sticky_fault_remains_reported() {
    let mut runtime = start_runtime(configured_app(FixedTime::default()));
    let fault = RuntimeFault::engine(RuntimeFaultKind::RequiredTask, "nara.test.stop-after-fault");
    runtime.fault_reporter().report(fault.clone());
    runtime.drive(Duration::ZERO).unwrap_err();
    accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);

    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Stopped);
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(runtime.fault(), Some(&fault));
}

#[derive(Debug)]
struct TestSystemError;

#[derive(Event)]
struct RuntimeScopeFaultEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct RawAdmissionSchedule;

#[derive(Default, Resource)]
struct RawObserverCount(usize);

#[derive(Resource)]
struct RuntimeOverlapGate {
    entered: mpsc::SyncSender<()>,
    release: Arc<Barrier>,
}

impl std::fmt::Display for TestSystemError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("injected system failure")
    }
}

impl std::error::Error for TestSystemError {}

fn overlap_gate(world: &mut World) {
    let gate = world.resource::<RuntimeOverlapGate>();
    gate.entered.send(()).unwrap();
    gate.release.wait();
}

fn overlap_fail_system() -> Result<(), BevyError> {
    Err(BevyError::error(TestSystemError))
}

fn overlap_fail_condition() -> Result<bool, BevyError> {
    Err(BevyError::error(TestSystemError))
}

fn overlap_condition_target() {}

fn overlap_healthy_system() {}

fn ignore_bevy_error(_error: BevyError, _context: ErrorContext) {}

fn temporarily_ignore_runtime_scope_error(world: &mut nara::ecs::World) {
    let canonical = *world.resource::<FallbackErrorHandler>();
    world.insert_resource(FallbackErrorHandler(ignore_bevy_error));
    world.trigger(RuntimeScopeFaultEvent);
    world.insert_resource(canonical);
}

fn temporarily_bypass_runtime_scope_error_handler(world: &mut nara::ecs::World) {
    let canonical = world.resource::<FallbackErrorHandler>().0;
    world
        .resource_mut::<FallbackErrorHandler>()
        .bypass_change_detection()
        .0 = ignore_bevy_error;
    world.trigger(RuntimeScopeFaultEvent);
    world
        .resource_mut::<FallbackErrorHandler>()
        .bypass_change_detection()
        .0 = canonical;
}

#[derive(Debug, Default, Resource)]
struct CrossStageFaultRouteProbe {
    canonical: Option<ErrorHandler>,
    generation: Option<nara::app::RuntimeGeneration>,
    update_runs: usize,
    last_runs: usize,
}

fn hide_fallback_before_next_stage(world: &mut nara::ecs::World) {
    let canonical = world.resource::<FallbackErrorHandler>().0;
    let generation = world
        .remove_resource::<nara::app::RuntimeGeneration>()
        .expect("a managed runtime publishes its generation resource");
    {
        let mut probe = world.resource_mut::<CrossStageFaultRouteProbe>();
        probe.canonical = Some(canonical);
        probe.generation = Some(generation);
    }
    world
        .resource_mut::<FallbackErrorHandler>()
        .bypass_change_detection()
        .0 = ignore_bevy_error;
}

fn fail_after_fallback_was_hidden(
    mut probe: ResMut<CrossStageFaultRouteProbe>,
) -> Result<(), BevyError> {
    probe.update_runs += 1;
    Err(BevyError::error(TestSystemError))
}

fn restore_fallback_after_failure(world: &mut nara::ecs::World) {
    let (canonical, generation) = {
        let mut probe = world.resource_mut::<CrossStageFaultRouteProbe>();
        probe.last_runs += 1;
        (
            probe
                .canonical
                .take()
                .expect("the first stage captured the canonical handler"),
            probe
                .generation
                .take()
                .expect("the first stage captured the runtime generation"),
        )
    };
    world.insert_resource(generation);
    let mut handler = world.resource_mut::<FallbackErrorHandler>();
    handler.bypass_change_detection().0 = canonical;
}

fn queue_failing_default_command(mut commands: Commands) {
    commands.queue(|_: &mut nara::ecs::World| -> Result<(), BevyError> {
        Err(BevyError::error(TestSystemError))
    });
}

static EXPLICIT_COMMAND_ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

fn count_explicit_command_error(_error: BevyError, _context: ErrorContext) {
    EXPLICIT_COMMAND_ERROR_COUNT.fetch_add(1, Ordering::SeqCst);
}

fn queue_failing_handled_command(mut commands: Commands) {
    commands.queue_handled(
        |_: &mut nara::ecs::World| -> Result<(), BevyError> {
            Err(BevyError::error(TestSystemError))
        },
        count_explicit_command_error,
    );
}

fn fail_system() -> Result<(), BevyError> {
    Err(BevyError::error(TestSystemError))
}

fn fail_system_with_panic_severity() -> Result<(), BevyError> {
    Err(BevyError::panic(TestSystemError))
}

fn fail_runtime_scope_observer(_: On<RuntimeScopeFaultEvent>) -> Result<(), BevyError> {
    Err(BevyError::error(TestSystemError))
}

fn count_raw_observer(_: On<RuntimeScopeFaultEvent>, mut count: ResMut<RawObserverCount>) {
    count.0 += 1;
}

fn trigger_runtime_scope_error(world: &mut nara::ecs::World) {
    world.trigger(RuntimeScopeFaultEvent);
}

fn app_with_runtime_scope_observer() -> App {
    let mut app = configured_app(FixedTime::default());
    app.add_observer(fail_runtime_scope_observer).unwrap();
    app
}

#[test]
fn app_observers_begin_at_the_first_execution_boundary() {
    let mut app = App::new();
    app.insert_resource(RawObserverCount::default())
        .unwrap()
        .add_observer(count_raw_observer)
        .unwrap()
        .add_systems(StartupStage::Core, trigger_runtime_scope_error)
        .unwrap();

    app.world_mut().unwrap().trigger(RuntimeScopeFaultEvent);
    assert_eq!(app.world().resource::<RawObserverCount>().0, 0);

    app.run_once(Duration::ZERO).unwrap();

    assert_eq!(app.world().resource::<RawObserverCount>().0, 1);
}

#[test]
fn raw_schedule_execution_prevents_later_managed_admission() {
    let mut app = App::new();
    app.insert_resource(RawObserverCount::default())
        .unwrap()
        .add_observer(count_raw_observer)
        .unwrap()
        .init_schedule(RawAdmissionSchedule)
        .unwrap();

    app.run_schedule(RawAdmissionSchedule).unwrap();
    let failure = admit_candidate(app.seal().unwrap()).unwrap_err();

    assert_eq!(failure.error(), RuntimeAdmissionError::AppStarted);
    let mut retirement = failure.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

fn panic_system() {
    panic!("injected Rust system panic");
}

thread_local! {
    static NESTED_RUNTIME: RefCell<Option<RuntimeInstance>> = const { RefCell::new(None) };
}

#[derive(Debug, Default, Resource)]
struct NestedRuntimeDriveObservation {
    fault: Option<RuntimeFaultKind>,
}

fn drive_nested_runtime(mut observation: ResMut<NestedRuntimeDriveObservation>) {
    observation.fault = NESTED_RUNTIME.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .expect("nested runtime remains present until the test reclaims it")
            .drive(Duration::ZERO)
            .err()
            .map(|error| error.fault().kind())
    });
}

#[test]
fn fallible_system_and_startup_failures_are_typed_and_unpublished() {
    let mut frame_app = configured_app(FixedTime::default());
    frame_app
        .add_systems(CoreStage::Update, fail_system)
        .unwrap();
    let mut runtime = start_runtime(frame_app);

    let error = runtime.drive(Duration::ZERO).unwrap_err();
    assert_eq!(error.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(runtime.state(), RuntimeState::Faulted);

    let mut startup_app = App::new();
    startup_app
        .add_systems(StartupStage::Core, fail_system)
        .unwrap();
    let candidate = admit_candidate(startup_app.seal().unwrap()).unwrap();
    let Err(mut failure) = candidate.complete_startup() else {
        panic!("fallible startup unexpectedly published a ready candidate");
    };
    assert_eq!(failure.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn overlapping_runtime_schedules_route_failures_to_their_own_reporters() {
    for _ in 0..8 {
        run_overlapping_runtime_schedule_round();
    }
}

fn run_overlapping_runtime_schedule_round() {
    let (entered_sender, entered_receiver) = mpsc::sync_channel(5);
    let release = Arc::new(Barrier::new(6));
    let system_sender = entered_sender.clone();
    let system_release = Arc::clone(&release);
    let system_thread = std::thread::spawn(move || {
        let mut app = configured_app(FixedTime::default());
        app.insert_resource(RuntimeOverlapGate {
            entered: system_sender,
            release: system_release,
        })
        .unwrap()
        .add_systems(
            CoreStage::Update,
            (overlap_gate, overlap_fail_system).chain(),
        )
        .unwrap();
        let mut runtime = start_runtime(app);
        let fault = runtime.drive(Duration::ZERO).unwrap_err().fault().kind();
        let state = runtime.state();
        finish_runtime(runtime);
        (fault, state)
    });
    let condition_sender = entered_sender.clone();
    let condition_release = Arc::clone(&release);
    let condition_thread = std::thread::spawn(move || {
        let mut app = configured_app(FixedTime::default());
        app.insert_resource(RuntimeOverlapGate {
            entered: condition_sender,
            release: condition_release,
        })
        .unwrap()
        .add_systems(
            CoreStage::Update,
            (
                overlap_gate,
                overlap_condition_target.run_if(overlap_fail_condition),
            )
                .chain(),
        )
        .unwrap();
        let mut runtime = start_runtime(app);
        let fault = runtime.drive(Duration::ZERO).unwrap_err().fault().kind();
        let state = runtime.state();
        finish_runtime(runtime);
        (fault, state)
    });
    let command_sender = entered_sender.clone();
    let command_release = Arc::clone(&release);
    let command_thread = std::thread::spawn(move || {
        let mut app = configured_app(FixedTime::default());
        app.insert_resource(RuntimeOverlapGate {
            entered: command_sender,
            release: command_release,
        })
        .unwrap()
        .add_systems(
            CoreStage::Update,
            (overlap_gate, queue_failing_default_command).chain(),
        )
        .unwrap();
        let mut runtime = start_runtime(app);
        let fault = runtime.drive(Duration::ZERO).unwrap_err().fault().kind();
        let state = runtime.state();
        finish_runtime(runtime);
        (fault, state)
    });
    let observer_sender = entered_sender.clone();
    let observer_release = Arc::clone(&release);
    let observer_thread = std::thread::spawn(move || {
        let mut app = app_with_runtime_scope_observer();
        app.insert_resource(RuntimeOverlapGate {
            entered: observer_sender,
            release: observer_release,
        })
        .unwrap()
        .add_systems(
            CoreStage::Update,
            (overlap_gate, trigger_runtime_scope_error).chain(),
        )
        .unwrap();
        let mut runtime = start_runtime(app);
        let fault = runtime.drive(Duration::ZERO).unwrap_err().fault().kind();
        let state = runtime.state();
        finish_runtime(runtime);
        (fault, state)
    });
    let healthy_release = Arc::clone(&release);
    let healthy_thread = std::thread::spawn(move || {
        let mut app = configured_app(FixedTime::default());
        app.insert_resource(RuntimeOverlapGate {
            entered: entered_sender,
            release: healthy_release,
        })
        .unwrap()
        .add_systems(
            CoreStage::Update,
            (overlap_gate, overlap_healthy_system).chain(),
        )
        .unwrap();
        let mut runtime = start_runtime(app);
        let outcome = runtime.drive(Duration::ZERO).unwrap().state();
        let state = runtime.state();
        finish_runtime(runtime);
        (outcome, state)
    });

    for _ in 0..5 {
        entered_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("every runtime entered its overlapping schedule");
    }
    release.wait();

    let (system_fault, system_state) = system_thread.join().unwrap();
    let (condition_fault, condition_state) = condition_thread.join().unwrap();
    let (command_fault, command_state) = command_thread.join().unwrap();
    let (observer_fault, observer_state) = observer_thread.join().unwrap();
    let (healthy_outcome, healthy_state) = healthy_thread.join().unwrap();
    assert_eq!(system_fault, RuntimeFaultKind::System);
    assert_eq!(condition_fault, RuntimeFaultKind::RunCondition);
    assert_eq!(command_fault, RuntimeFaultKind::Command);
    assert_eq!(observer_fault, RuntimeFaultKind::Observer);
    assert_eq!(system_state, RuntimeState::Faulted);
    assert_eq!(condition_state, RuntimeState::Faulted);
    assert_eq!(command_state, RuntimeState::Faulted);
    assert_eq!(observer_state, RuntimeState::Faulted);
    assert_eq!(healthy_outcome, RuntimeState::Running);
    assert_eq!(healthy_state, RuntimeState::Running);
}

#[test]
fn default_command_failures_enter_the_runtime_fault_channel() {
    let mut app = configured_app(FixedTime::default());
    app.add_systems(CoreStage::Update, queue_failing_default_command)
        .unwrap();
    let mut runtime = start_runtime(app);

    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::Command);
    assert_eq!(runtime.state(), RuntimeState::Faulted);
}

#[test]
fn explicit_command_error_handlers_remain_caller_owned() {
    EXPLICIT_COMMAND_ERROR_COUNT.store(0, Ordering::SeqCst);
    let mut app = configured_app(FixedTime::default());
    app.add_systems(CoreStage::Update, queue_failing_handled_command)
        .unwrap();
    let mut runtime = start_runtime(app);

    let outcome = runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(outcome.state(), RuntimeState::Running);
    assert_eq!(runtime.state(), RuntimeState::Running);
    assert!(runtime.fault().is_none());
    assert_eq!(EXPLICIT_COMMAND_ERROR_COUNT.load(Ordering::SeqCst), 1);
    finish_runtime(runtime);
}

#[test]
fn startup_rejects_a_temporarily_replaced_error_handler() {
    let mut app = app_with_runtime_scope_observer();
    app.add_systems(StartupStage::Core, temporarily_ignore_runtime_scope_error)
        .unwrap();
    let candidate = admit_candidate(app.seal().unwrap()).unwrap();

    let mut failure = candidate.complete_startup().unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::Observer);
    assert_eq!(
        failure.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn frame_captures_observer_errors_when_fallback_changes_are_hidden() {
    let mut app = app_with_runtime_scope_observer();
    app.add_systems(
        CoreStage::Update,
        temporarily_bypass_runtime_scope_error_handler,
    )
    .unwrap();
    let mut runtime = start_runtime(app);

    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::Observer);
    assert_eq!(runtime.state(), RuntimeState::Faulted);
}

#[test]
fn frame_rejects_a_fallback_replacement_before_the_next_schedule() {
    let mut app = configured_app(FixedTime::default());
    app.insert_resource(CrossStageFaultRouteProbe::default())
        .unwrap()
        .add_systems(CoreStage::First, hide_fallback_before_next_stage)
        .unwrap()
        .add_systems(CoreStage::Update, fail_after_fallback_was_hidden)
        .unwrap()
        .add_systems(CoreStage::Last, restore_fallback_after_failure)
        .unwrap();
    let mut runtime = start_runtime(app);

    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(
        failure.fault().kind(),
        RuntimeFaultKind::FaultReporterAuthority
    );
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    let probe = runtime.world().resource::<CrossStageFaultRouteProbe>();
    assert_eq!(probe.update_runs, 0);
    assert_eq!(probe.last_runs, 0);
}

#[test]
fn exact_step_rejects_a_temporarily_replaced_error_handler() {
    let mut app = app_with_runtime_scope_observer();
    app.add_systems(
        CoreStage::FixedUpdate,
        temporarily_ignore_runtime_scope_error.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let mut runtime = start_runtime(app);
    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    accepted_ticket(runtime.request_control(RuntimeControl::StepFixedTick));

    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::Observer);
    assert_eq!(runtime.state(), RuntimeState::Faulted);
}

#[derive(Resource)]
struct RetirementMarker(usize);

impl __RuntimeDriverPort for RetirementMarker {
    type Input = ();
    type Output = ();

    fn accepts_driver_state(state: RuntimeState) -> bool {
        matches!(
            state,
            RuntimeState::Running
                | RuntimeState::Paused
                | RuntimeState::Faulted
                | RuntimeState::Stopping
                | RuntimeState::CloseIncomplete
        )
    }

    fn apply_driver_input(&mut self, (): Self::Input) {
        self.0 += 1;
    }
}

fn rewrap_component_registry(world: &mut World) {
    let snapshot = world
        .resource::<ComponentRegistry>()
        .snapshot()
        .expect("component registry freezes before runtime frame execution");
    world.insert_resource(ComponentRegistry::from_snapshot(snapshot));
}

#[test]
fn faulted_runtime_keeps_typed_retirement_ports_available() {
    let mut app = configured_app(FixedTime::default());
    app.insert_resource(RetirementMarker(0)).unwrap();
    let mut runtime = start_runtime(app);
    runtime.fault_reporter().report(RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.retirement-scope-service",
    ));
    assert_eq!(runtime.state(), RuntimeState::Faulted);

    runtime
        .with_driver_scope(|scope| {
            scope.__apply_port::<RetirementMarker>(()).unwrap();
        })
        .unwrap();

    assert_eq!(runtime.world().resource::<RetirementMarker>().0, 1);
    accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    runtime
        .with_driver_scope(|scope| {
            scope.__apply_port::<RetirementMarker>(()).unwrap();
        })
        .unwrap();
    assert_eq!(runtime.world().resource::<RetirementMarker>().0, 2);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopped
    );
}

#[test]
fn registry_authority_fault_does_not_block_typed_retirement_ports() {
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(ComponentRegistryPlugin)
        .unwrap()
        .insert_resource(RetirementMarker(0))
        .unwrap()
        .add_systems(CoreStage::Last, rewrap_component_registry)
        .unwrap();
    let mut runtime = start_runtime(app);

    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    runtime
        .with_driver_scope(|scope| {
            scope.__apply_port::<RetirementMarker>(()).unwrap();
        })
        .unwrap();
    assert_eq!(runtime.world().resource::<RetirementMarker>().0, 1);
    finish_runtime(runtime);
}

#[test]
fn a_preexisting_candidate_fault_prevents_startup_side_effects() {
    #[derive(Resource)]
    struct StartupSideEffect(Arc<AtomicUsize>);

    fn record_side_effect(effect: Res<StartupSideEffect>) {
        effect.0.fetch_add(1, Ordering::SeqCst);
    }

    let side_effects = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.insert_resource(StartupSideEffect(side_effects.clone()))
        .unwrap()
        .add_systems(StartupStage::Core, record_side_effect)
        .unwrap();
    let candidate = admit_candidate(app.seal().unwrap()).unwrap();
    candidate.fault_reporter().report(RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.pre-start-service",
    ));

    let Err(failure) = candidate.complete_startup() else {
        panic!("faulted candidate unexpectedly became ready");
    };

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::RequiredService);
    assert_eq!(side_effects.load(Ordering::SeqCst), 0);
}

#[test]
fn a_fault_after_readiness_is_not_published_as_running() {
    let candidate = admit_candidate(configured_app(FixedTime::default()).seal().unwrap()).unwrap();
    let reporter = candidate.fault_reporter();
    let ready = candidate.complete_startup().unwrap();
    let fault = RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.ready-publication-service",
    );
    assert!(reporter.report(fault.clone()));

    let runtime = ready.promote();

    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.fault(), Some(&fault));
}

#[test]
fn atomic_publication_rejects_a_fault_that_wins_the_reporter_lock() {
    let candidate = admit_candidate(configured_app(FixedTime::default()).seal().unwrap()).unwrap();
    let reporter = candidate.fault_reporter();
    let ready = candidate.complete_startup().unwrap();
    let fault = RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.publication-report-first",
    );
    let reporter_thread = std::thread::spawn(move || reporter.report(fault));
    assert!(reporter_thread.join().unwrap());

    let mut slot = RuntimePublicationSlot::new();
    let failure = ready.publish_into(&mut slot).unwrap_err();

    assert!(matches!(
        failure.error(),
        RuntimePublicationError::CandidateFault(fault)
            if fault.source() == "nara.test.publication-report-first"
    ));
    assert!(slot.is_vacant());
    finish_publication_failure(failure);
}

#[test]
fn atomic_publication_installs_the_owner_before_a_later_reported_fault() {
    let candidate = admit_candidate(configured_app(FixedTime::default()).seal().unwrap()).unwrap();
    let reporter = candidate.fault_reporter();
    let ready = candidate.complete_startup().unwrap();
    let mut slot = RuntimePublicationSlot::new();

    ready.publish_into(&mut slot).unwrap();
    let reporter_thread = std::thread::spawn(move || {
        reporter.report(RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "nara.test.publication-publish-first",
        ))
    });
    assert!(reporter_thread.join().unwrap());

    let runtime = slot
        .take()
        .expect("successful publication installs one runtime owner");
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(
        runtime.fault().map(RuntimeFault::source),
        Some("nara.test.publication-publish-first")
    );
    finish_runtime(runtime);
}

#[test]
fn atomic_publication_preserves_an_occupied_slot_and_retires_the_rejected_owner() {
    let baseline_reservations = reserve_remaining_runtime_routes();
    let route_capacity = baseline_reservations.len();
    drop(baseline_reservations);
    let first = admit_candidate(configured_app(FixedTime::default()).seal().unwrap())
        .unwrap()
        .complete_startup()
        .unwrap();
    let second = admit_candidate(configured_app(FixedTime::default()).seal().unwrap())
        .unwrap()
        .complete_startup()
        .unwrap();
    let mut slot = RuntimePublicationSlot::new();
    first.publish_into(&mut slot).unwrap();
    let first_generation = slot.runtime().unwrap().generation();

    let failure = second.publish_into(&mut slot).unwrap_err();

    assert_eq!(
        failure.error(),
        &RuntimePublicationError::DestinationOccupied
    );
    assert_eq!(slot.runtime().unwrap().generation(), first_generation);
    let retained_route_reservations = reserve_remaining_runtime_routes();
    assert_eq!(retained_route_reservations.len() + 2, route_capacity);
    finish_publication_failure(failure);
    let rejected_route = RuntimeAdmissionReservation::try_acquire()
        .expect("retiring the rejected publication owner releases one route");
    finish_runtime(slot.take().unwrap());
    let published_route = RuntimeAdmissionReservation::try_acquire()
        .expect("retiring the published runtime releases its distinct route");
    drop(published_route);
    drop(rejected_route);
    drop(retained_route_reservations);
}

#[test]
fn a_consumed_publication_slot_cannot_publish_another_generation() {
    let first = admit_candidate(configured_app(FixedTime::default()).seal().unwrap())
        .unwrap()
        .complete_startup()
        .unwrap();
    let second = admit_candidate(configured_app(FixedTime::default()).seal().unwrap())
        .unwrap()
        .complete_startup()
        .unwrap();
    let mut slot = RuntimePublicationSlot::new();
    first.publish_into(&mut slot).unwrap();
    let runtime = slot.take().unwrap();
    assert!(!slot.is_vacant());

    let failure = second.publish_into(&mut slot).unwrap_err();

    assert_eq!(
        failure.error(),
        &RuntimePublicationError::DestinationOccupied
    );
    assert!(slot.runtime().is_none());
    finish_publication_failure(failure);
    finish_runtime(runtime);
}

#[test]
fn panic_severity_fallible_systems_unwind_out_of_runtime_drive() {
    let mut app = configured_app(FixedTime::default());
    app.add_systems(CoreStage::Update, fail_system_with_panic_severity)
        .unwrap();
    let mut runtime = start_runtime(app);

    let driven = catch_unwind(AssertUnwindSafe(|| runtime.drive(Duration::ZERO)));

    assert!(driven.is_err(), "panic-severity error was contained");
}

#[test]
fn rust_system_panics_unwind_from_variable_and_every_fixed_set() {
    for fixed_set in [
        None,
        Some(FixedUpdateSet::Prepare),
        Some(FixedUpdateSet::Simulate),
        Some(FixedUpdateSet::Finalize),
    ] {
        let mut app = configured_app(FixedTime::default());
        if let Some(fixed_set) = fixed_set {
            app.add_systems(CoreStage::FixedUpdate, panic_system.in_set(fixed_set))
                .unwrap();
        } else {
            app.add_systems(CoreStage::Update, panic_system).unwrap();
        }
        let mut runtime = start_runtime(app);
        if fixed_set.is_some() {
            accepted_ticket(runtime.request_control(RuntimeControl::Pause));
            runtime.drive(Duration::ZERO).unwrap();
            accepted_ticket(runtime.request_control(RuntimeControl::StepFixedTick));
        }

        let driven = catch_unwind(AssertUnwindSafe(|| runtime.drive(Duration::ZERO)));

        assert!(driven.is_err(), "Rust system panic was contained");
    }
}

#[test]
fn nested_runtime_drive_keeps_both_runtime_routes_independent() {
    let inner = start_runtime(configured_app(FixedTime::default()));
    NESTED_RUNTIME.with(|slot| {
        assert!(slot.replace(Some(inner)).is_none());
    });
    let mut outer_app = configured_app(FixedTime::default());
    outer_app
        .world_mut()
        .unwrap()
        .insert_resource(NestedRuntimeDriveObservation::default());
    outer_app
        .add_systems(CoreStage::Update, drive_nested_runtime)
        .unwrap();
    let mut outer = start_runtime(outer_app);

    outer.drive(Duration::ZERO).unwrap();

    assert_eq!(
        outer
            .world()
            .resource::<NestedRuntimeDriveObservation>()
            .fault,
        None
    );
    let inner = NESTED_RUNTIME.with(|slot| {
        slot.borrow_mut()
            .take()
            .expect("nested runtime is reclaimed exactly once")
    });
    assert_eq!(inner.state(), RuntimeState::Running);
    assert_eq!(outer.state(), RuntimeState::Running);
    finish_runtime(inner);
    finish_runtime(outer);
}

fn replace_queue_before_acknowledge(mut queue: ResMut<GameplayCommandQueue>) {
    *queue = GameplayCommandQueue::default();
}

#[test]
fn acknowledge_invariant_failure_becomes_a_sticky_runtime_fault() {
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(GameplayCommandPlugin::default())
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            replace_queue_before_acknowledge.in_set(GameplayCommandSet::Capture),
        )
        .unwrap();
    let mut runtime = start_runtime(app);
    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    accepted_ticket(runtime.request_control(RuntimeControl::StepFixedTick));

    let error = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(error.fault().kind(), RuntimeFaultKind::GameplayLifecycle);
    assert_eq!(error.fault().source(), "nara.gameplay.fixed-acknowledge");
    assert_eq!(runtime.state(), RuntimeState::Faulted);
}

#[test]
fn external_submission_rejections_do_not_fault_a_healthy_runtime() {
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    let mut runtime = start_runtime(app);
    let submission = GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).unwrap(),
        GameplayCommandIngressSource::external("external-rejection").unwrap(),
        GameplayCommandSourceSequence::new(1).unwrap(),
        GameplayCommandDraft::new(GameplayCommandTypeId::new("runtime.external").unwrap()),
    );
    runtime
        .with_driver_scope(|scope| {
            submit_gameplay_driver_command(scope, submission.clone())
                .unwrap()
                .unwrap();
            assert_eq!(
                submit_gameplay_driver_command(scope, submission)
                    .unwrap()
                    .unwrap_err(),
                GameplayCommandRejection::Duplicate
            );
        })
        .unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    runtime
        .with_driver_scope(|scope| {
            let rejection = submit_gameplay_driver_command(
                scope,
                GameplayCommandSubmission::new(
                    GameplayCommandTick::new(1).unwrap(),
                    GameplayCommandIngressSource::external("late-rejection").unwrap(),
                    GameplayCommandSourceSequence::new(2).unwrap(),
                    GameplayCommandDraft::new(
                        GameplayCommandTypeId::new("runtime.external").unwrap(),
                    ),
                ),
            )
            .unwrap()
            .unwrap_err();
            assert!(matches!(rejection, GameplayCommandRejection::Late { .. }));
        })
        .unwrap();

    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Running);
    assert_eq!(runtime.fault(), None);
}

#[test]
fn a_failed_control_records_a_terminal_result_before_faulting() {
    let mut runtime = start_runtime(configured_app(FixedTime::default()));
    let pause = accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.fault_reporter().report(RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.test.pending-control-service",
    ));
    let error = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(error.fault().kind(), RuntimeFaultKind::RequiredService);
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(
        runtime.control_status(pause),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::RuntimeFaulted
        ))
    );
}

#[derive(Debug, Default, Resource)]
struct StageCounts {
    task_updates: usize,
    simulation_updates: usize,
    render_updates: usize,
    cleanup_updates: usize,
}

fn count_task_update(mut counts: ResMut<StageCounts>) {
    counts.task_updates += 1;
}

fn count_simulation_update(mut counts: ResMut<StageCounts>) {
    counts.simulation_updates += 1;
}

fn count_render_update(mut counts: ResMut<StageCounts>) {
    counts.render_updates += 1;
}

fn count_cleanup_update(mut counts: ResMut<StageCounts>) {
    counts.cleanup_updates += 1;
}

#[derive(Debug, Default, Resource)]
struct ObservedInputOutcomes(Vec<ActionOutcome>);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct ObservedInputFrameEdges {
    key_presses: usize,
    key_releases: usize,
    mouse_presses: usize,
    mouse_releases: usize,
}

fn observe_input_outcomes(
    outcomes: Res<ActionOutcomes>,
    mut observed: ResMut<ObservedInputOutcomes>,
) {
    observed.0 = outcomes.as_slice().to_vec();
}

fn observe_input_frame_edges(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut observed: ResMut<ObservedInputFrameEdges>,
) {
    observed.key_presses += usize::from(keyboard.just_pressed(KeyCode::Enter));
    observed.key_releases += usize::from(keyboard.just_released(KeyCode::Enter));
    observed.mouse_presses += usize::from(mouse.just_pressed(MouseButton::Left));
    observed.mouse_releases += usize::from(mouse.just_released(MouseButton::Left));
}

#[test]
fn paused_driving_runs_real_time_work_without_variable_simulation() {
    let mut app = configured_app(FixedTime::default());
    app.insert_resource(StageCounts::default())
        .unwrap()
        .add_systems(CoreStage::TaskUpdate, count_task_update)
        .unwrap()
        .add_systems(CoreStage::Update, count_simulation_update)
        .unwrap()
        .add_systems(CoreStage::Render, count_render_update)
        .unwrap()
        .add_systems(CoreStage::Cleanup, count_cleanup_update)
        .unwrap();
    let mut runtime = start_runtime(app);

    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    let paused = runtime.drive(Duration::from_millis(5)).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(paused.frame().unwrap().status.fixed_steps, 0);
    assert_eq!(runtime.world().resource::<StageCounts>().task_updates, 1);
    assert_eq!(
        runtime.world().resource::<StageCounts>().simulation_updates,
        0
    );
    assert_eq!(runtime.world().resource::<StageCounts>().render_updates, 1);
    assert_eq!(runtime.world().resource::<StageCounts>().cleanup_updates, 1);

    accepted_ticket(runtime.request_control(RuntimeControl::Resume));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.world().resource::<StageCounts>().task_updates, 2);
    assert_eq!(
        runtime.world().resource::<StageCounts>().simulation_updates,
        1
    );
    assert_eq!(runtime.world().resource::<StageCounts>().render_updates, 2);
    assert_eq!(runtime.world().resource::<StageCounts>().cleanup_updates, 2);
}

#[test]
fn paused_input_edges_are_retained_until_the_next_resolving_frame() {
    let mut app = configured_app(FixedTime::default());
    app.add_plugin(InputPlugin)
        .unwrap()
        .insert_resource(ObservedInputOutcomes::default())
        .unwrap()
        .insert_resource(ObservedInputFrameEdges::default())
        .unwrap()
        .add_systems(CoreStage::Update, observe_input_outcomes)
        .unwrap()
        .add_systems(CoreStage::Extract, observe_input_frame_edges)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ActionMap>()
        .bind_key(ActionId::new("confirm").unwrap(), KeyCode::Enter);
    app.world_mut()
        .unwrap()
        .resource_mut::<ActionMap>()
        .bind_mouse(ActionId::new("select").unwrap(), MouseButton::Left);
    let mut runtime = start_runtime(app);

    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Enter)).unwrap();
            apply_keyboard_driver_input(scope, ButtonDriverInput::Release(KeyCode::Enter)).unwrap();
            apply_mouse_driver_input(scope, ButtonDriverInput::Press(MouseButton::Left)).unwrap();
            apply_mouse_driver_input(scope, ButtonDriverInput::Release(MouseButton::Left)).unwrap();
        })
        .unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .transitions()
            .is_empty()
    );
    assert_eq!(
        *runtime.world().resource::<ObservedInputFrameEdges>(),
        ObservedInputFrameEdges {
            key_presses: 1,
            key_releases: 1,
            mouse_presses: 1,
            mouse_releases: 1,
        }
    );
    assert!(
        runtime
            .world()
            .resource::<ObservedInputOutcomes>()
            .0
            .is_empty()
    );

    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(
        *runtime.world().resource::<ObservedInputFrameEdges>(),
        ObservedInputFrameEdges {
            key_presses: 1,
            key_releases: 1,
            mouse_presses: 1,
            mouse_releases: 1,
        }
    );

    accepted_ticket(runtime.request_control(RuntimeControl::Resume));
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(
        runtime
            .world()
            .resource::<ObservedInputOutcomes>()
            .0
            .iter()
            .map(|outcome| (outcome.action.as_str(), outcome.phase))
            .collect::<Vec<_>>(),
        [
            ("confirm", ActionPhase::Started),
            ("confirm", ActionPhase::Released),
            ("select", ActionPhase::Started),
            ("select", ActionPhase::Released),
        ]
    );
    assert_eq!(
        *runtime.world().resource::<ObservedInputFrameEdges>(),
        ObservedInputFrameEdges {
            key_presses: 1,
            key_releases: 1,
            mouse_presses: 1,
            mouse_releases: 1,
        }
    );
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .transitions()
            .is_empty()
    );
    finish_runtime(runtime);
}

#[test]
fn paused_runtime_state_remains_authoritative_during_elapsed_drive() {
    let mut runtime = start_runtime(configured_app(FixedTime::default()));
    accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    let before = *runtime.world().resource::<FixedTime>();

    runtime.drive(before.timestep() * 2).unwrap();

    let after = *runtime.world().resource::<FixedTime>();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(after.tick(), before.tick());
    assert_eq!(after.debt(), before.debt());
    assert_eq!(after.remainder(), before.remainder());
}

#[test]
fn stop_supersedes_an_unapplied_control_and_is_idempotent() {
    let mut runtime = start_runtime(configured_app(FixedTime::default()));
    let pause = accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    let stop = accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    assert_eq!(
        runtime.control_status(pause),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::SupersededByStop
        ))
    );
    assert_eq!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(stop)
    );

    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Pending)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Applied)
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(RuntimeControlRejection::InvalidState {
            state: RuntimeState::Stopped,
            control: RuntimeControl::Resume,
        })
    ));
}
