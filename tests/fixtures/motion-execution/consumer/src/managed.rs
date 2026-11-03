use std::{
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use motion_animation::{ANIMATION_PLUGIN_ID, AnimationPlugin, MotionIntent};
use motion_contract::{Actors, MotionInput, Phase, Trace};
use motion_physics::{PHYSICS_PLUGIN_ID, PhysicsPlugin, PhysicsSet, Solver, SolverMode};
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration,
    PluginError, PluginId, RuntimeControl, RuntimeControlRequestResult, RuntimeFaultKind,
    RuntimeInstance, StartupStage,
};
use nara_core::Vec2;
use nara_ecs::{
    Commands, NonSend, Query, Res, ResMut, Resource, bevy_ecs::error::BevyError,
    schedule::IntoScheduleConfigs,
};
use nara_gameplay::{
    GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
    GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandSet, GameplayCommandSourceSequence,
    GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
    HierarchyWriter,
};
use nara_reflect::ComponentRegistryPlugin;
use nara_transform::{
    FixedSpatialSnapshot2d, FixedTransformInterpolation, PresentedSpatialSnapshot2d, Transform2d,
    TransformPlugin, add_fixed_transform_projection_systems,
    add_presented_transform_projection_systems,
};

struct Composition;

impl Plugin for Composition {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("fixture.motion.product"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[ANIMATION_PLUGIN_ID, PHYSICS_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.configure_sets(
            CoreStage::FixedUpdate,
            MotionIntent.before(PhysicsSet::Target),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            sync_input
                .before(MotionIntent)
                .in_set(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

fn sync_input(
    fixed: Res<FixedTime>,
    actors: Res<Actors>,
    mut input: ResMut<MotionInput>,
    mut locals: Query<&mut Transform2d>,
) {
    input.tick = fixed.tick();
    input.parent_translation = Vec2::new(if fixed.tick() < 3 { 10.0 } else { 20.0 }, 0.0);
    locals.get_mut(actors.parent).unwrap().translation = input.parent_translation;
}

struct DriverThread(std::thread::ThreadId, Rc<()>);

fn observe_non_send(owner: NonSend<DriverThread>, fixed: Res<FixedTime>, trace: Res<Trace>) {
    assert_eq!(owner.0, std::thread::current().id());
    assert_eq!(Rc::strong_count(&owner.1), 1);
    trace.record(fixed.tick(), Phase::NonSend);
}

#[derive(Debug, Default, Resource)]
struct Projections {
    fixed: Vec<(u64, Vec2, Vec2)>,
    presented: Vec<Vec2>,
}

fn observe_fixed(
    spatial: FixedSpatialSnapshot2d,
    actors: Res<Actors>,
    fixed: Res<FixedTime>,
    solver: Res<Solver>,
    mut projections: ResMut<Projections>,
    trace: Res<Trace>,
) {
    assert_eq!(spatial.stamp().unwrap().tick, fixed.tick());
    let kinematic = spatial.global(actors.kinematic).unwrap().translation();
    let dynamic = spatial.global(actors.dynamic).unwrap().translation();
    assert!((kinematic - solver.sample().kinematic).length() < 1e-5);
    assert!((dynamic - solver.sample().dynamic).length() < 1e-5);
    projections.fixed.push((fixed.tick(), kinematic, dynamic));
    trace.record(fixed.tick(), Phase::Published);
}

fn observe_presented(
    spatial: PresentedSpatialSnapshot2d,
    actors: Res<Actors>,
    mut projections: ResMut<Projections>,
) {
    projections
        .presented
        .push(spatial.global(actors.kinematic).unwrap().translation());
}

fn capture(fixed: Res<FixedTime>, batch: Res<GameplayCommandBatch>, trace: Res<Trace>) {
    assert_eq!(
        batch.active_tick().map(|tick| tick.get()),
        Some(fixed.tick())
    );
    trace.record(fixed.tick(), Phase::Capture);
}

fn configured(mode: SolverMode) -> App {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        HierarchyPlugin,
        TransformPlugin,
        GameplayCommandPlugin::default(),
    ))
    .unwrap();
    let world = app.world_mut().unwrap();
    let parent = world.spawn(Transform2d::default()).id();
    let kinematic = world.spawn(Transform2d::default()).id();
    let dynamic = world.spawn(Transform2d::default()).id();
    HierarchyWriter::new(world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: parent,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: kinematic,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            },
            HierarchyOperation::Place {
                entity: dynamic,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            },
        ])
        .unwrap();
    world.insert_resource(Actors {
        parent,
        kinematic,
        dynamic,
    });
    world.insert_resource(MotionInput {
        tick: 0,
        delta: Vec2::new(0.25, 0.0),
        parent_translation: Vec2::ZERO,
    });
    world.init_resource::<Trace>();
    world.init_resource::<Projections>();
    world.insert_resource(FixedTransformInterpolation);
    world.insert_non_send(DriverThread(std::thread::current().id(), Rc::new(())));
    app.add_plugins((AnimationPlugin, PhysicsPlugin { mode }, Composition))
        .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        observe_non_send
            .after(PhysicsSet::Callback)
            .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    add_fixed_transform_projection_systems(&mut app, observe_fixed).unwrap();
    add_presented_transform_projection_systems(&mut app, observe_presented).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        capture.in_set(GameplayCommandSet::Capture),
    )
    .unwrap();
    app
}

#[test]
fn independent_plugins_use_real_projection_for_zero_multiple_paused_and_exact_ticks() {
    for mode in [SolverMode::Direct, SolverMode::Borrowed] {
        let mut runtime = RuntimeInstance::start(configured(mode)).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().resource::<Projections>().fixed.is_empty());
        runtime
            .drive(FixedTime::DEFAULT_TIMESTEP * 2 + FixedTime::DEFAULT_TIMESTEP / 2)
            .unwrap();
        let projections = runtime.world().resource::<Projections>();
        assert_eq!(
            projections
                .fixed
                .iter()
                .map(|sample| sample.0)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        let midpoint = (projections.fixed[0].1 + projections.fixed[1].1) * 0.5;
        assert!((*projections.presented.last().unwrap() - midpoint).length() < 1e-5);
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        let projections = runtime.world().resource::<Projections>();
        assert_eq!(
            projections.presented.last().copied(),
            Some(projections.fixed[1].1)
        );
        assert!(matches!(
            runtime.request_control(RuntimeControl::StepFixedTick),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.world().resource::<Projections>().fixed.len(), 3);
        runtime.drive(Duration::ZERO).unwrap();
        let projections = runtime.world().resource::<Projections>();
        assert_eq!(
            projections.presented.last().copied(),
            Some(projections.fixed[2].1)
        );
        assert_eq!(
            runtime
                .world()
                .get::<Transform2d>(runtime.world().resource::<Actors>().parent)
                .unwrap()
                .translation,
            Vec2::new(20.0, 0.0)
        );
        runtime.finish().unwrap();
    }
}

#[derive(Resource)]
struct StartupRan(Arc<AtomicBool>);

#[test]
fn product_missing_domain_and_invalid_motion_topology_refuse_before_startup() {
    let mut missing = App::new();
    assert!(missing.add_plugin(Composition).is_err());
    let startup = Arc::new(AtomicBool::new(false));
    let mut app = configured(SolverMode::Direct);
    app.insert_resource(StartupRan(Arc::clone(&startup)))
        .unwrap();
    app.add_systems(StartupStage::Core, |ran: Res<StartupRan>| {
        ran.0.store(true, Ordering::Release)
    })
    .unwrap();
    app.configure_sets(
        CoreStage::FixedUpdate,
        MotionIntent.after(PhysicsSet::Commit),
    )
    .unwrap();
    assert!(RuntimeInstance::start(app).is_err());
    assert!(!startup.load(Ordering::Acquire));
}

#[derive(Resource)]
struct PartialEffect;

#[derive(Default, Resource)]
struct ConsumedCommands(usize);

fn consume(batch: Res<GameplayCommandBatch>, mut consumed: ResMut<ConsumedCommands>) {
    consumed.0 += batch.len();
}

fn fail_after_capture(
    input: Res<MotionInput>,
    trace: Res<Trace>,
    mut commands: Commands,
) -> Result<(), BevyError> {
    commands.insert_resource(PartialEffect);
    trace.record(input.tick, Phase::Failed);
    Err(BevyError::error(std::io::Error::other(
        "motion fixture late authoritative failure",
    )))
}

#[test]
fn late_authoritative_failure_preserves_narrow_ack_but_not_tick_success_or_rollback() {
    let mut app = configured(SolverMode::Borrowed);
    app.init_resource::<ConsumedCommands>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        consume.in_set(GameplayCommandSet::Consume),
    )
    .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<GameplayCommandQueue>()
        .submit(GameplayCommandSubmission::new(
            GameplayCommandTick::new(1).unwrap(),
            GameplayCommandIngressSource::test("joint-motion").unwrap(),
            GameplayCommandSourceSequence::new(1).unwrap(),
            GameplayCommandDraft::new(
                GameplayCommandTypeId::new("fixture.motion.command").unwrap(),
            ),
        ))
        .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        fail_after_capture.after(FixedUpdateSet::Finalize),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let fault = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap_err();
    assert_eq!(fault.fault().kind(), RuntimeFaultKind::System);
    let world = runtime.world();
    assert_eq!(world.resource::<ConsumedCommands>().0, 1);
    assert_eq!(
        world
            .resource::<GameplayCommandQueue>()
            .stats()
            .acknowledged,
        1
    );
    assert_eq!(
        world
            .resource::<GameplayCommandQueue>()
            .stats()
            .acknowledged_through_tick,
        1
    );
    assert_eq!(world.resource::<Projections>().fixed.len(), 1);
    assert!(world.contains_resource::<PartialEffect>());
    let phases = world.resource::<Trace>().observations();
    assert!(
        phases
            .iter()
            .position(|entry| entry.phase == Phase::Capture)
            .unwrap()
            < phases
                .iter()
                .position(|entry| entry.phase == Phase::Failed)
                .unwrap()
    );
    assert_eq!(runtime.drive(Duration::ZERO).unwrap_err(), fault);
    runtime.finish().unwrap();
}

#[derive(Clone, Resource)]
struct AutomaticThreads(Arc<std::sync::Mutex<Vec<(String, std::thread::ThreadId)>>>);

#[test]
fn managed_automatic_schedules_have_an_explicit_serial_baseline_under_feature_unification() {
    let driver = std::thread::current().id();
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut app = App::new();
    app.insert_resource(AutomaticThreads(Arc::clone(&observed)))
        .unwrap();
    for stage in StartupStage::ALL {
        for _ in 0..2 {
            app.add_systems(stage, move |threads: Res<AutomaticThreads>| {
                threads
                    .0
                    .lock()
                    .unwrap()
                    .push((format!("{stage:?}"), std::thread::current().id()));
            })
            .unwrap();
        }
    }
    for stage in CoreStage::ALL {
        for _ in 0..2 {
            app.add_systems(stage, move |threads: Res<AutomaticThreads>| {
                threads
                    .0
                    .lock()
                    .unwrap()
                    .push((format!("{stage:?}"), std::thread::current().id()));
            })
            .unwrap();
        }
    }
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    runtime.finish().unwrap();
    let threads = observed.lock().unwrap();
    assert_eq!(
        threads.len(),
        (StartupStage::ALL.len() + CoreStage::ALL.len()) * 2
    );
    assert!(
        threads.iter().all(|(_, thread)| *thread == driver),
        "automatic thread placement changed with dependency features: {threads:?}"
    );
}

#[test]
fn held_real_solver_borrow_cannot_publish_or_return_before_explicit_join() {
    let gate = motion_physics::WorkerGate::new(Duration::from_secs(10));
    let mut app = configured(SolverMode::Borrowed);
    let trace = app.world().resource::<Trace>().clone();
    app.insert_resource(Solver::new(SolverMode::Borrowed).with_gate(gate.clone()))
        .unwrap();
    let observer = std::thread::spawn(move || {
        let started = gate.wait_started();
        let before_release = trace.observations();
        gate.release();
        assert!(started, "borrowed solver never entered its worker");
        assert_eq!(
            before_release
                .iter()
                .map(|entry| entry.phase)
                .collect::<Vec<_>>(),
            [Phase::Intent, Phase::Target, Phase::WorkerStarted]
        );
        before_release.last().unwrap().thread
    });
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let worker = observer.join().unwrap();
    assert_ne!(worker, std::thread::current().id());
    let trace = runtime.world().resource::<Trace>().observations();
    let completed = trace
        .iter()
        .position(|entry| entry.phase == Phase::WorkerCompleted)
        .unwrap();
    let joined = trace
        .iter()
        .position(|entry| entry.phase == Phase::Joined)
        .unwrap();
    let committed = trace
        .iter()
        .position(|entry| entry.phase == Phase::PoseQueued)
        .unwrap();
    let published = trace
        .iter()
        .position(|entry| entry.phase == Phase::Published)
        .unwrap();
    assert!(completed < joined && joined < committed && committed < published);
    assert_eq!(runtime.world().resource::<Projections>().fixed.len(), 1);
    runtime.finish().unwrap();
}

#[cfg(feature = "unified-multi-threaded")]
#[test]
fn explicitly_driven_custom_schedule_keeps_its_owner_selected_parallel_executor() {
    use nara_ecs::schedule::ScheduleLabel;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct Custom;

    bevy_tasks::ComputeTaskPool::get_or_init(|| {
        bevy_tasks::TaskPoolBuilder::new().num_threads(2).build()
    });
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut app = App::new();
    app.insert_resource(AutomaticThreads(Arc::clone(&observed)))
        .unwrap();
    app.add_systems(Custom, |threads: Res<AutomaticThreads>| {
        threads
            .0
            .lock()
            .unwrap()
            .push(("custom".to_owned(), std::thread::current().id()));
    })
    .unwrap();
    app.get_schedule_mut(Custom)
        .unwrap()
        .unwrap()
        .set_executor(nara_ecs::bevy_ecs::schedule::MultiThreadedExecutor::new());
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(observed.lock().unwrap().is_empty());
    runtime.run_schedule(Custom).unwrap();
    runtime.finish().unwrap();
    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_ne!(observed[0].1, std::thread::current().id());
}
