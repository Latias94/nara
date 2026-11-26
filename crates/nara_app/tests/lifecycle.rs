use std::{cell::Cell, rc::Rc, time::Duration};

use nara_app::{
    AddPluginsError, App, AppExit, AppExitRequests, CoreStage, FixedCatchUpPolicy, FixedTime,
    FixedUpdateSet, FrameCleanup, FrameIntent, Plugin, PluginCategory, PluginDeclaration,
    PluginError, PluginGroup, PluginGroupBuilder, PluginId, PluginPlanError,
    PluginSchemaProviderId, RealTime, RenderTime, RuntimeCandidateRetirementState,
    RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
    RuntimeControlRequestResult, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter,
    RuntimeInstance, RuntimeState, RuntimeTimeSettings, StartupStage,
};
use nara_ecs::{Commands, Res, ResMut, Resource, World, schedule::IntoScheduleConfigs};

#[derive(Default, Resource)]
struct Trace(Vec<&'static str>);

#[derive(Resource)]
struct Prepared;

#[test]
fn headless_startup_and_exact_steps_share_the_same_world() {
    let mut app = App::new();
    app.init_resource::<Trace>().unwrap();
    app.add_systems(StartupStage::Scene, |mut trace: ResMut<Trace>| {
        trace.0.push("startup")
    })
    .unwrap();
    app.add_systems(FrameIntent, |mut trace: ResMut<Trace>| {
        trace.0.push("intent")
    })
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (
            (|mut commands: Commands| commands.insert_resource(Prepared))
                .in_set(FixedUpdateSet::Prepare),
            (|_: Res<Prepared>, mut trace: ResMut<Trace>| trace.0.push("fixed"))
                .in_set(FixedUpdateSet::Simulate),
        ),
    )
    .unwrap();
    app.add_systems(FrameCleanup, |mut trace: ResMut<Trace>| {
        trace.0.push("cleanup")
    })
    .unwrap();
    app.add_systems(CoreStage::Update, |mut exit: ResMut<AppExitRequests>| {
        exit.request_exit()
    })
    .unwrap();
    let step = Duration::from_millis(20);
    assert_eq!(app.step_fixed(step).unwrap().status.fixed_elapsed, step);
    assert_eq!(app.step_fixed(step).unwrap().status.fixed_elapsed, step * 2);
    assert_eq!(
        app.world().resource::<FixedTime>().timestep(),
        FixedTime::DEFAULT_TIMESTEP
    );
    assert_eq!(
        app.world().resource::<Trace>().0,
        [
            "startup", "intent", "fixed", "cleanup", "intent", "fixed", "cleanup"
        ]
    );
    assert_eq!(app.run().unwrap(), AppExit::Requested);
    app.finish().unwrap();
}

#[test]
fn elapsed_resume_and_exact_stepping_preserve_debt_and_fraction() {
    let step = Duration::from_nanos(10);
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_time_scale(0.5).unwrap())
        .unwrap();
    app.insert_resource(
        FixedTime::new(step)
            .unwrap()
            .with_max_steps_per_frame(2)
            .unwrap()
            .with_catch_up_policy(FixedCatchUpPolicy::PreserveDebt),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frame = runtime
        .drive(Duration::from_nanos(67))
        .unwrap()
        .frame()
        .unwrap();
    assert_eq!(frame.status.fixed_steps, 2);
    assert_eq!(frame.status.fixed_debt, step);
    assert_eq!(frame.status.fixed_remainder, Duration::from_nanos(3));

    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let sample = *runtime.world().resource::<RenderTime>();
    let saved = runtime.capture_time_continuation().unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.world().resource::<RenderTime>(), &sample);
    assert!(runtime.capture_time_continuation().is_err());
    runtime.drive(Duration::ZERO).unwrap();
    runtime.restore_time_continuation(saved).unwrap();

    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let resumed = runtime
        .drive_elapsed(Duration::from_nanos(80))
        .unwrap()
        .frame()
        .unwrap();
    assert!(resumed.status.simulation_paused);
    assert_eq!(resumed.status.fixed_debt, step);
    let active = runtime
        .drive_elapsed(Duration::from_nanos(1))
        .unwrap()
        .frame()
        .unwrap();
    assert_eq!(active.status.virtual_delta, Duration::from_nanos(1));
    assert_eq!(active.status.fixed_steps, 1);
    assert_eq!(active.status.fixed_remainder, Duration::from_nanos(4));
    runtime.finish().unwrap();
}

const SOURCE: PluginId = PluginId::new("test.source");
const DEPENDENT: PluginId = PluginId::new("test.dependent");
const SCHEMA: PluginSchemaProviderId = PluginSchemaProviderId::new("test.source.schema");

#[derive(Default)]
struct Source;

impl Plugin for Source {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(SOURCE, PluginCategory::Core).provides_schema(&[SCHEMA]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.resource_mut::<Trace>().0.push("source");
        Ok(())
    }
}

struct Dependent;

impl Plugin for Dependent {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(DEPENDENT, PluginCategory::Runtime).requires_schema(&[SCHEMA]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.resource_mut::<Trace>().0.push("dependent");
        Ok(())
    }
}

struct SourceGroup;

impl PluginGroup for SourceGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_default::<Source>()
    }
}

#[test]
fn plugin_groups_share_defaults_and_install_schema_providers_first() {
    let mut app = App::new();
    app.init_resource::<Trace>().unwrap();
    assert!(matches!(
        app.add_plugin(Dependent),
        Err(AddPluginsError::Plan(PluginPlanError::MissingSchema { .. }))
    ));
    assert!(app.world().resource::<Trace>().0.is_empty());
    app.add_plugins((Dependent, SourceGroup, SourceGroup))
        .unwrap();
    assert_eq!(app.world().resource::<Trace>().0, ["source", "dependent"]);
    assert_eq!(
        app.installed_plugins()
            .map(|plugin| plugin.id)
            .collect::<Vec<_>>(),
        [SOURCE, DEPENDENT]
    );
}

struct LocalOwner {
    released: Rc<Cell<bool>>,
    dropped: Rc<Cell<bool>>,
}

impl RuntimeCloseParticipant for LocalOwner {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(if self.released.get() {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

impl Drop for LocalOwner {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}

#[test]
fn incomplete_close_retains_the_local_owner_until_retry() {
    let released = Rc::new(Cell::new(false));
    let dropped = Rc::new(Cell::new(false));
    let mut app = App::new();
    app.register_close_participant(
        RuntimeCloseParticipantId::new("test.native"),
        LocalOwner {
            released: released.clone(),
            dropped: dropped.clone(),
        },
    )
    .unwrap();
    let runtime =
        RuntimeInstance::start_with_close_policy(app, RuntimeClosePolicy::new(Duration::ZERO))
            .unwrap();
    let mut retirement = runtime.finish().unwrap_err();
    assert_eq!(
        retirement.retirement_state(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert!(!dropped.get());
    assert_eq!(retirement.close_evidence().pending_participants().len(), 1);
    released.set(true);
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    assert!(!retirement.close_evidence().has_terminal_failure());
    drop(retirement);
    assert!(dropped.get());
}

#[test]
fn driver_receives_completed_work_before_following_the_first_fault() {
    let mut runtime = RuntimeInstance::start(App::new()).unwrap();
    let fault = RuntimeFault::engine(RuntimeFaultKind::RequiredService, "test.scope");
    let done = runtime
        .with_driver_scope(|scope| {
            scope.with_world_mut(|world: &mut World| {
                assert!(
                    world
                        .resource::<RuntimeFaultReporter>()
                        .report(fault.clone())
                );
                42
            })
        })
        .unwrap();
    assert_eq!(done.output, 42);
    assert_eq!(done.post_operation_fault, Some(fault.clone()));
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.world().resource::<RealTime>().frame, 0);
    assert_eq!(runtime.drive(Duration::ZERO).unwrap_err().fault(), &fault);
    runtime.finish().unwrap();
}

#[cfg(all(feature = "tasks", not(target_arch = "wasm32")))]
#[test]
fn bevy_task_handles_deliver_results_and_cancel_owned_work() {
    use std::{future::pending, sync::mpsc};

    struct OnDrop(mpsc::Sender<()>);

    impl Drop for OnDrop {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    let ready = nara_app::tasks::async_compute_pool().spawn(async { 42 });
    assert_eq!(bevy_tasks::block_on(ready), 42);
    let (released, release) = mpsc::channel();
    let (started, start) = mpsc::channel();
    let guard = OnDrop(released);
    let running = nara_app::tasks::io_pool().spawn(async move {
        started.send(()).unwrap();
        pending::<()>().await;
        drop(guard);
    });
    let timeout = Duration::from_secs(2);
    start.recv_timeout(timeout).unwrap();
    drop(running);
    release.recv_timeout(timeout).unwrap();
}
