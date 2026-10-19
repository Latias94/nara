use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginLifecycleState, PluginShutdownContext, PluginShutdownObligationId, RealTime, RenderTime,
    RuntimeCandidateRetirementState, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeClosePolicy,
    RuntimeCloseProgress, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance,
    RuntimeObligationLedger, RuntimeScheduleError, RuntimeStartError, RuntimeState,
    ScheduleGraphError, StartupStage, VirtualTime,
};
use nara_ecs::{
    Commands, DetectChanges, Res, ResMut, Resource,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, LogLevel, ScheduleLabel, SystemSet},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct DomainSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct MissingSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct UnrequestedSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum DomainSet {
    Prepare,
    Simulate,
}

#[derive(Debug, Default, Resource)]
struct DomainTrace(Vec<&'static str>);

#[derive(Resource)]
struct StartupObserved(Arc<AtomicBool>);

fn observe_startup(startup: Res<StartupObserved>) {
    startup.0.store(true, Ordering::Release);
}

fn add_cycle(app: &mut App, schedule: impl ScheduleLabel) -> Result<(), PluginError> {
    app.configure_sets(
        schedule,
        (
            DomainSet::Prepare.before(DomainSet::Simulate),
            DomainSet::Simulate.before(DomainSet::Prepare),
        ),
    )?;
    Ok(())
}

fn assert_graph_failure(error: &RuntimeStartError, expected: InternedScheduleLabel) {
    let RuntimeStartError::Preparation(PluginError::ScheduleGraph(
        ScheduleGraphError::BuildFailed { schedule, message },
    )) = error
    else {
        panic!("expected a graph admission error, got {error:?}");
    };
    assert_eq!(*schedule, expected);
    assert!(!message.is_empty());
}

#[test]
fn installed_startup_update_and_custom_cycles_reject_before_startup() {
    for schedule in [
        StartupStage::Scene.intern(),
        CoreStage::Update.intern(),
        DomainSchedule.intern(),
    ] {
        let startup = Arc::new(AtomicBool::new(false));
        let mut app = App::new();
        app.insert_resource(StartupObserved(Arc::clone(&startup)))
            .unwrap();
        app.add_systems(StartupStage::Core, observe_startup)
            .unwrap();
        add_cycle(&mut app, schedule).unwrap();

        let failure = RuntimeInstance::start(app).unwrap_err();

        assert_graph_failure(failure.error(), schedule);
        assert!(!startup.load(Ordering::Acquire));
        assert_eq!(
            failure.retirement_state(),
            RuntimeCandidateRetirementState::Retired
        );
    }
}

#[test]
fn installed_custom_graph_honors_its_error_level_for_ambiguities() {
    let startup = Arc::new(AtomicBool::new(false));
    let mut app = App::new();
    app.insert_resource(StartupObserved(Arc::clone(&startup)))
        .unwrap();
    app.init_resource::<DomainTrace>().unwrap();
    app.add_systems(StartupStage::Core, observe_startup)
        .unwrap();
    app.add_systems(DomainSchedule, (record_prepare, record_simulate))
        .unwrap();
    let mut settings = app
        .get_schedule(DomainSchedule)
        .unwrap()
        .get_build_settings();
    settings.ambiguity_detection = LogLevel::Error;
    app.set_schedule_build_settings(DomainSchedule, settings)
        .unwrap();

    let failure = RuntimeInstance::start(app).unwrap_err();

    assert_graph_failure(failure.error(), DomainSchedule.intern());
    assert!(!startup.load(Ordering::Acquire));
}

const GRAPH_OWNER_ID: PluginId = PluginId::new("nara.test.schedule-graph-owner");
const GRAPH_CLOSE: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.schedule-graph-owner.close");
static GRAPH_OWNER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(GRAPH_OWNER_ID, PluginCategory::Runtime)
        .shutdown_obligations(&[GRAPH_CLOSE]);

#[derive(Debug)]
struct FinishCyclePlugin {
    released: Arc<AtomicBool>,
    shutdowns: Arc<AtomicUsize>,
}

impl Plugin for FinishCyclePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &GRAPH_OWNER_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            GRAPH_CLOSE,
            RuntimeCloseParticipantId::new("nara.test.schedule-graph-owner.retirement"),
            PendingGraphOwner(Arc::clone(&self.released)),
        )?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        add_cycle(app, DomainSchedule)
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        self.shutdowns.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

struct PendingGraphOwner(Arc<AtomicBool>);

impl RuntimeCloseParticipant for PendingGraphOwner {
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
fn graph_admission_follows_plugin_finish_and_retains_incomplete_close_owner() {
    let startup = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.insert_resource(StartupObserved(Arc::clone(&startup)))
        .unwrap();
    app.add_systems(StartupStage::Core, observe_startup)
        .unwrap();
    app.add_plugin(FinishCyclePlugin {
        released: Arc::clone(&released),
        shutdowns: Arc::clone(&shutdowns),
    })
    .unwrap();

    let mut failure = RuntimeInstance::start_with_options(
        app,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap_err();
    let incomplete = failure.retirement_state();
    let started = startup.load(Ordering::Acquire);
    let shutdowns_before_release = shutdowns.load(Ordering::Relaxed);
    released.store(true, Ordering::Release);
    let retired = failure.drive_retirement();

    assert_graph_failure(failure.error(), DomainSchedule.intern());
    assert!(!started);
    assert_eq!(
        incomplete,
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(retired, RuntimeCandidateRetirementState::Retired);
    assert_eq!(shutdowns_before_release, 1);
    assert_eq!(shutdowns.load(Ordering::Relaxed), 1);
}

#[test]
fn custom_typed_schedule_is_inspectable_and_inert_during_managed_frames() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.init_schedule(DomainSchedule).unwrap();
    app.configure_sets(
        DomainSchedule,
        (DomainSet::Prepare, DomainSet::Simulate).chain(),
    )
    .unwrap();
    app.add_systems(
        DomainSchedule,
        (
            record_prepare.in_set(DomainSet::Prepare),
            record_simulate.in_set(DomainSet::Simulate),
        ),
    )
    .unwrap();
    let mut settings = app
        .get_schedule(DomainSchedule)
        .unwrap()
        .get_build_settings();
    settings.auto_insert_apply_deferred = false;
    settings.ambiguity_detection = LogLevel::Error;
    app.set_schedule_build_settings(DomainSchedule, settings)
        .unwrap();

    assert!(app.get_schedule(DomainSchedule).is_some());
    assert!(matches!(app.get_schedule_mut(DomainSchedule), Ok(Some(_))));
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<DomainTrace>().0.is_empty());
}

#[test]
fn explicit_custom_schedule_executes_once_without_advancing_clocks_or_frame_order() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.add_systems(StartupStage::Core, |mut trace: ResMut<DomainTrace>| {
        trace.0.push("startup");
    })
    .unwrap();
    app.add_systems(CoreStage::Update, |mut trace: ResMut<DomainTrace>| {
        trace.0.push("update");
    })
    .unwrap();
    app.configure_sets(
        DomainSchedule,
        (DomainSet::Prepare, DomainSet::Simulate).chain(),
    )
    .unwrap();
    app.add_systems(
        DomainSchedule,
        (
            record_prepare.in_set(DomainSet::Prepare),
            record_simulate.in_set(DomainSet::Simulate),
        ),
    )
    .unwrap();
    app.add_systems(UnrequestedSchedule, |mut trace: ResMut<DomainTrace>| {
        trace.0.push("unrequested");
    })
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let real = *runtime.world().resource::<RealTime>();
    let virtual_time = *runtime.world().resource::<VirtualTime>();
    let render = *runtime.world().resource::<RenderTime>();
    let fixed = runtime.world().resource::<FixedTime>();
    let fixed_snapshot = (fixed.tick(), fixed.elapsed(), fixed.steps_this_frame());

    runtime.run_schedule(DomainSchedule).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Running);
    assert_eq!(
        runtime.world().resource::<DomainTrace>().0,
        ["startup", "update", "prepare", "simulate"]
    );
    assert_eq!(*runtime.world().resource::<RealTime>(), real);
    assert_eq!(*runtime.world().resource::<VirtualTime>(), virtual_time);
    assert_eq!(*runtime.world().resource::<RenderTime>(), render);
    let fixed = runtime.world().resource::<FixedTime>();
    assert_eq!(
        (fixed.tick(), fixed.elapsed(), fixed.steps_this_frame()),
        fixed_snapshot
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<DomainTrace>().0,
        ["startup", "update", "prepare", "simulate", "update"]
    );
}

#[test]
fn explicit_custom_schedule_rejects_builtin_missing_pending_and_paused_entries() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.add_systems(DomainSchedule, record_prepare).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    for schedule in StartupStage::ALL
        .into_iter()
        .map(|schedule| schedule.intern())
        .chain(CoreStage::ALL.into_iter().map(|schedule| schedule.intern()))
        .chain([nara_app::__private::FrameCleanup.intern()])
    {
        assert_eq!(
            runtime.run_schedule(schedule),
            Err(RuntimeScheduleError::BuiltIn { schedule })
        );
    }
    assert_eq!(
        runtime.run_schedule(MissingSchedule),
        Err(RuntimeScheduleError::Missing {
            schedule: MissingSchedule.intern()
        }),
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(
        runtime.run_schedule(DomainSchedule),
        Err(RuntimeScheduleError::ControlPending)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.run_schedule(DomainSchedule),
        Err(RuntimeScheduleError::Unavailable {
            state: RuntimeState::Paused
        })
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.run_schedule(DomainSchedule).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(
        runtime.run_schedule(DomainSchedule),
        Err(RuntimeScheduleError::ControlPending)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.run_schedule(DomainSchedule),
        Err(RuntimeScheduleError::Unavailable {
            state: RuntimeState::Stopping
        })
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.run_schedule(DomainSchedule),
        Err(RuntimeScheduleError::Unavailable {
            state: RuntimeState::Stopped
        })
    );
    assert_eq!(runtime.world().resource::<DomainTrace>().0, ["prepare"]);
}

#[test]
fn custom_schedule_configuration_is_frozen_by_seal() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.init_schedule(DomainSchedule).unwrap();
    app.add_systems(DomainSchedule, record_prepare).unwrap();

    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );

    let sealed = app.seal().unwrap();
    assert!(sealed.world().resource::<DomainTrace>().0.is_empty());
}

#[test]
fn missing_custom_schedule_is_observed_without_sealing_the_app() {
    let app = App::new();

    assert!(app.get_schedule(MissingSchedule).is_none());
    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
}

#[test]
fn managed_startup_runs_once_and_the_frame_uses_engine_stage_order() {
    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.add_systems(StartupStage::Core, record_prepare).unwrap();
    app.add_systems(CoreStage::Update, record_simulate).unwrap();

    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );
    assert!(app.world().resource::<DomainTrace>().0.is_empty());

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<DomainTrace>().0,
        ["prepare", "simulate"]
    );
}

#[test]
fn graph_admission_allows_update_resources_inserted_by_startup_commands() {
    #[derive(Resource)]
    struct StartupValue(&'static str);

    let mut app = App::new();
    app.init_resource::<DomainTrace>().unwrap();
    app.add_systems(StartupStage::Core, |mut commands: Commands| {
        commands.insert_resource(StartupValue("startup value"));
    })
    .unwrap();
    app.add_systems(
        CoreStage::Update,
        |value: Res<StartupValue>, mut trace: ResMut<DomainTrace>| {
            trace.0.push(value.0);
        },
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(
        runtime.world().resource::<DomainTrace>().0,
        ["startup value"]
    );
}

fn record_prepare(mut trace: ResMut<DomainTrace>) {
    trace.0.push("prepare");
}

#[test]
fn custom_schedule_change_detection_advances_on_each_explicit_run() {
    #[derive(Default, Resource)]
    struct Value(u32);
    #[derive(Default, Resource)]
    struct Observations(Vec<bool>);

    let mut app = App::new();
    app.init_resource::<Value>().unwrap();
    app.init_resource::<Observations>().unwrap();
    app.add_systems(CoreStage::Update, |mut value: ResMut<Value>| {
        value.0 += 1;
    })
    .unwrap();
    app.add_systems(
        DomainSchedule,
        |value: Res<Value>, mut observations: ResMut<Observations>| {
            observations.0.push(value.is_changed());
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.run_schedule(DomainSchedule).unwrap();
    runtime.run_schedule(DomainSchedule).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime.run_schedule(DomainSchedule).unwrap();
    runtime.run_schedule(DomainSchedule).unwrap();
    assert_eq!(
        runtime.world().resource::<Observations>().0,
        [true, false, true, false]
    );
    runtime.finish().unwrap();
}

fn record_simulate(mut trace: ResMut<DomainTrace>) {
    trace.0.push("simulate");
}
