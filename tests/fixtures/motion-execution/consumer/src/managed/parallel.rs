use std::{
    sync::{Condvar, Mutex},
    thread::{self, ThreadId},
};

use bevy_tasks::{ComputeTaskPool, TaskPoolBuilder};
use motion_audio::{AudioPlugin, PcmAudio};
use motion_contract::RootMotionIntent;
use motion_physics::{Sample, WorkerGate};
use nara_app::{PluginShutdownContext, RuntimeState};
use nara_ecs::{Entity, World, schedule::ScheduleLabel};

use super::*;

fn initialize_host_compute_pool() -> &'static ComputeTaskPool {
    let pool = ComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
    assert_eq!(
        pool.thread_num(),
        2,
        "the fixture Host uses two ECS workers"
    );
    pool
}

#[derive(Default, Resource)]
struct CommittedSamples(Vec<Sample>);

fn observe_committed_sample(
    actors: Res<Actors>,
    input: Res<MotionInput>,
    sample: Res<Sample>,
    intents: Query<&RootMotionIntent>,
    locals: Query<&Transform2d>,
    mut samples: ResMut<CommittedSamples>,
) {
    let intent = intents.get(actors.kinematic).unwrap();
    assert_eq!(intent.tick, input.tick);
    assert_eq!(intent.delta, input.delta);
    assert_eq!(sample.tick, input.tick);
    assert_eq!(
        locals.get(actors.parent).unwrap().translation,
        input.parent_translation
    );
    for (entity, expected) in [
        (actors.kinematic, sample.kinematic),
        (actors.dynamic, sample.dynamic),
    ] {
        assert_pose_close(
            locals.get(entity).unwrap().translation + input.parent_translation,
            expected,
        );
    }
    samples.0.push(*sample);
}

fn configured_motion(mode: SolverMode, profile: ExecutionProfile) -> App {
    let mut app = configured_with_profile(mode, profile);
    app.init_resource::<CommittedSamples>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        observe_committed_sample
            .after(PhysicsSet::Callback)
            .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    app
}

fn assert_pose_close(actual: Vec2, expected: Vec2) {
    assert!(actual.is_finite() && expected.is_finite());
    assert!(
        (actual - expected).length() < 1e-5,
        "motion pose differs: {actual:?}, expected {expected:?}"
    );
}

#[derive(Resource)]
struct Reparent {
    original: Entity,
    replacement: Entity,
    applied: bool,
}

fn reparent_before_third_tick(world: &mut World) {
    if world.resource::<FixedTime>().tick() != 3 {
        return;
    }
    let actors = *world.resource::<Actors>();
    let reparent = world.resource::<Reparent>();
    assert!(!reparent.applied);
    assert_eq!(actors.parent, reparent.original);
    let replacement = reparent.replacement;
    let entities = [actors.kinematic, actors.dynamic];
    let locals = entities.map(|entity| *world.get::<Transform2d>(entity).unwrap());
    for entity in entities {
        let operation = HierarchyOperation::Move {
            entity,
            from: HierarchyPlacement::current(world, entity).unwrap(),
            to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                parent: replacement,
            }),
        };
        HierarchyWriter::new(world)
            .apply_batch(&[operation])
            .unwrap();
    }
    for (entity, local) in entities.into_iter().zip(locals) {
        assert_eq!(*world.get::<Transform2d>(entity).unwrap(), local);
        assert_eq!(
            HierarchyPlacement::current(world, entity).unwrap().owner,
            HierarchyPlacementOwner::Parent {
                parent: replacement,
            }
        );
    }
    world.resource_mut::<Actors>().parent = replacement;
    world.resource_mut::<Reparent>().applied = true;
}

struct Trajectory {
    samples: Vec<Sample>,
    fixed: Vec<(u64, Vec2, Vec2)>,
    presented: Vec<Vec2>,
}

fn trajectory(mode: SolverMode, profile: ExecutionProfile) -> Trajectory {
    let driver = thread::current().id();
    let mut app = configured_motion(mode, profile);
    let world = app.world_mut().unwrap();
    let original = world.resource::<Actors>().parent;
    let replacement = world
        .spawn(Transform2d::from_translation(Vec2::new(-3.0, 7.0)))
        .id();
    HierarchyWriter::new(world)
        .apply_batch(&[HierarchyOperation::Place {
            entity: replacement,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }])
        .unwrap();
    world.insert_resource(Reparent {
        original,
        replacement,
        applied: false,
    });
    app.add_systems(
        CoreStage::FixedUpdate,
        reparent_before_third_tick.in_set(FixedUpdateSet::Prepare),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    assert_eq!(runtime.execution_profile(), profile);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<CommittedSamples>().0.is_empty());
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP * 2 + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    let projections = runtime.world().resource::<Projections>();
    assert_eq!(projections.fixed.len(), 2);
    assert_pose_close(
        *projections.presented.last().unwrap(),
        (projections.fixed[0].1 + projections.fixed[1].1) * 0.5,
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(runtime.world().resource::<CommittedSamples>().0.len(), 2);
    let projections = runtime.world().resource::<Projections>();
    assert_pose_close(
        *projections.presented.last().unwrap(),
        projections.fixed[1].1,
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(runtime.world().resource::<CommittedSamples>().0.len(), 3);
    assert!(runtime.world().resource::<Reparent>().applied);
    let world = runtime.world();
    assert_eq!(world.resource::<Actors>().parent, replacement);
    assert_eq!(
        world.get::<Transform2d>(original).unwrap().translation,
        Vec2::new(10.0, 0.0)
    );
    assert_eq!(
        world.get::<Transform2d>(replacement).unwrap().translation,
        Vec2::new(20.0, 0.0)
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.world().resource::<CommittedSamples>().0.len(), 3);
    let projections = runtime.world().resource::<Projections>();
    assert_pose_close(
        *projections.presented.last().unwrap(),
        projections.fixed[2].1,
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    for _ in 0..5 {
        runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    }
    let world = runtime.world();
    let samples = &world.resource::<CommittedSamples>().0;
    let projections = world.resource::<Projections>();
    assert_eq!(samples.len(), 8);
    assert_eq!(projections.fixed.len(), samples.len());
    assert!(samples.last().unwrap().contact);
    assert!(samples[7].kinematic.x > samples[0].kinematic.x);
    for (sample, (tick, kinematic, dynamic)) in samples.iter().zip(&projections.fixed) {
        assert_eq!(sample.tick, *tick);
        assert_pose_close(*kinematic, sample.kinematic);
        assert_pose_close(*dynamic, sample.dynamic);
    }
    let observations = world.resource::<Trace>().observations();
    assert_eq!(
        observations
            .iter()
            .filter(|entry| entry.phase == Phase::NonSend)
            .count(),
        8
    );
    assert!(
        observations
            .iter()
            .filter(|entry| entry.phase == Phase::NonSend)
            .all(|entry| entry.thread == driver)
    );
    if profile == ExecutionProfile::Parallel {
        assert!(
            observations
                .iter()
                .any(|entry| entry.phase == Phase::Intent && entry.thread != driver)
        );
    }
    for tick in 1..=8 {
        let entries = observations
            .iter()
            .filter(|entry| entry.tick == tick)
            .collect::<Vec<_>>();
        let at = |phase| {
            entries
                .iter()
                .position(|entry| entry.phase == phase)
                .unwrap()
        };
        assert!(at(Phase::WorkerCompleted) < at(Phase::Joined));
        assert!(at(Phase::Joined) < at(Phase::PoseQueued));
        assert!(at(Phase::PoseQueued) < at(Phase::Callback));
        assert!(at(Phase::Callback) < at(Phase::Published));
        let worker = entries[at(Phase::WorkerStarted)].thread;
        assert_eq!(worker, entries[at(Phase::WorkerCompleted)].thread);
        if mode == SolverMode::Borrowed {
            assert_ne!(worker, driver);
            assert!(
                observations
                    .iter()
                    .filter(|entry| !matches!(
                        entry.phase,
                        Phase::WorkerStarted | Phase::WorkerCompleted
                    ))
                    .all(|entry| entry.thread != worker)
            );
        } else {
            assert_eq!(worker, entries[at(Phase::Joined)].thread);
        }
    }
    let result = Trajectory {
        samples: samples.clone(),
        fixed: projections.fixed.clone(),
        presented: projections.presented.clone(),
    };
    runtime.finish().unwrap();
    result
}

#[test]
fn production_parallel_matches_real_serial_and_borrowed_motion_through_pause_step_and_reparent() {
    initialize_host_compute_pool();
    let expected = trajectory(SolverMode::Direct, ExecutionProfile::Serial);
    for (mode, profile) in [
        (SolverMode::Borrowed, ExecutionProfile::Serial),
        (SolverMode::Direct, ExecutionProfile::Parallel),
        (SolverMode::Borrowed, ExecutionProfile::Parallel),
    ] {
        let actual = trajectory(mode, profile);
        assert_eq!(actual.samples.len(), expected.samples.len());
        for (actual, expected) in actual.samples.iter().zip(&expected.samples) {
            assert_eq!(
                (actual.tick, actual.contact),
                (expected.tick, expected.contact)
            );
            assert_pose_close(actual.kinematic, expected.kinematic);
            assert_pose_close(actual.dynamic, expected.dynamic);
        }
        assert_eq!(actual.fixed.len(), expected.fixed.len());
        for (actual, expected) in actual.fixed.iter().zip(&expected.fixed) {
            assert_eq!(actual.0, expected.0);
            assert_pose_close(actual.1, expected.1);
            assert_pose_close(actual.2, expected.2);
        }
        assert_eq!(actual.presented.len(), expected.presented.len());
        for (actual, expected) in actual.presented.iter().zip(&expected.presented) {
            assert_pose_close(*actual, *expected);
        }
    }
}

#[derive(Default, Resource)]
struct EcsPeers {
    threads: Mutex<Vec<ThreadId>>,
    changed: Condvar,
}

impl EcsPeers {
    fn meet(&self) {
        let mut threads = self.threads.lock().unwrap();
        threads.push(thread::current().id());
        self.changed.notify_all();
        let (threads, _) = self
            .changed
            .wait_timeout_while(threads, Duration::from_secs(5), |threads| threads.len() < 2)
            .unwrap();
        assert_eq!(threads.len(), 2, "independent ECS peers did not overlap");
    }
}

fn first_ecs_peer(peers: Res<EcsPeers>) {
    peers.meet();
}

fn second_ecs_peer(peers: Res<EcsPeers>) {
    peers.meet();
}

#[test]
fn production_parallel_overlaps_actual_ecs_peers_with_real_motion_and_non_send_on_driver() {
    initialize_host_compute_pool();
    let driver = thread::current().id();
    let mut app = configured_motion(SolverMode::Direct, ExecutionProfile::Parallel);
    app.init_resource::<EcsPeers>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (first_ecs_peer, second_ecs_peer)
            .after(PhysicsSet::Callback)
            .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let peers = runtime
        .world()
        .resource::<EcsPeers>()
        .threads
        .lock()
        .unwrap();
    assert_eq!(peers.len(), 2);
    assert_ne!(peers[0], peers[1]);
    assert!(peers.iter().all(|thread| *thread != driver));
    drop(peers);
    assert_eq!(runtime.world().resource::<CommittedSamples>().0.len(), 1);
    let observations = runtime.world().resource::<Trace>().observations();
    assert!(
        observations
            .iter()
            .any(|entry| entry.phase == Phase::WorkerStarted && entry.thread != driver)
    );
    assert!(
        observations
            .iter()
            .any(|entry| entry.phase == Phase::NonSend && entry.thread == driver)
    );
    runtime.finish().unwrap();
}

#[test]
fn production_parallel_held_borrowed_solver_blocks_real_pose_commit_and_spatial_publication() {
    initialize_host_compute_pool();
    let gate = WorkerGate::new(Duration::from_secs(10));
    let mut app = configured_motion(SolverMode::Borrowed, ExecutionProfile::Parallel);
    let trace = app.world().resource::<Trace>().clone();
    let returned = Arc::new(AtomicBool::new(false));
    app.insert_resource(Solver::new(SolverMode::Borrowed).with_gate(gate.clone()))
        .unwrap();
    let completed = Arc::clone(&returned);
    let observer = thread::spawn(move || {
        let started = gate.wait_started();
        let before_release = trace.observations();
        let returned = completed.load(Ordering::Acquire);
        gate.release();
        assert!(started, "borrowed solver never entered its worker");
        assert!(
            !returned,
            "managed drive returned while the solver was borrowed"
        );
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
    let drive = runtime.drive(FixedTime::DEFAULT_TIMESTEP);
    returned.store(true, Ordering::Release);
    let worker = observer.join().unwrap();
    drive.unwrap();
    let samples = &runtime.world().resource::<CommittedSamples>().0;
    assert_eq!(samples.len(), 1);
    let projections = runtime.world().resource::<Projections>();
    assert_eq!(projections.fixed.len(), 1);
    assert_pose_close(projections.fixed[0].1, samples[0].kinematic);
    assert_pose_close(projections.fixed[0].2, samples[0].dynamic);
    let observations = runtime.world().resource::<Trace>().observations();
    let at = |phase| {
        observations
            .iter()
            .position(|entry| entry.phase == phase)
            .unwrap()
    };
    assert!(at(Phase::WorkerCompleted) < at(Phase::Joined));
    assert!(at(Phase::Joined) < at(Phase::PoseQueued));
    assert!(at(Phase::PoseQueued) < at(Phase::Callback));
    assert!(at(Phase::Callback) < at(Phase::Published));
    assert_ne!(worker, observations[at(Phase::Joined)].thread);
    runtime.finish().unwrap();
}

struct CleanupProbe(Arc<Mutex<Vec<(ThreadId, Sample)>>>);

impl Plugin for CleanupProbe {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("fixture.motion.parallel-cleanup"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        let driver = context.world().non_send::<DriverThread>();
        assert_eq!(driver.0, thread::current().id());
        assert_eq!(Rc::strong_count(&driver.1), 1);
        self.0
            .lock()
            .unwrap()
            .push((driver.0, context.world().resource::<Solver>().sample()));
        Ok(())
    }
}

#[test]
fn production_parallel_late_fault_keeps_narrow_ack_and_original_independent_cleanup() {
    initialize_host_compute_pool();
    let driver = thread::current().id();
    let failed_audio = Arc::new(Mutex::new(PcmAudio::new(48_000).unwrap()));
    let other_audio = Arc::new(Mutex::new(PcmAudio::new(48_000).unwrap()));
    let failed_cleanup = Arc::new(Mutex::new(Vec::new()));
    let other_cleanup = Arc::new(Mutex::new(Vec::new()));
    let mut app = configured_motion(SolverMode::Borrowed, ExecutionProfile::Parallel);
    app.add_plugins((
        AudioPlugin {
            owner: Arc::clone(&failed_audio),
        },
        CleanupProbe(Arc::clone(&failed_cleanup)),
    ))
    .unwrap();
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
            GameplayCommandIngressSource::test("parallel-motion").unwrap(),
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
    let mut other_app = configured_motion(SolverMode::Direct, ExecutionProfile::Parallel);
    other_app
        .add_plugins((
            AudioPlugin {
                owner: Arc::clone(&other_audio),
            },
            CleanupProbe(Arc::clone(&other_cleanup)),
        ))
        .unwrap();
    let mut failed = RuntimeInstance::start(app).unwrap();
    let mut other = RuntimeInstance::start(other_app).unwrap();
    assert_ne!(failed.generation(), other.generation());
    let fault = failed.drive(FixedTime::DEFAULT_TIMESTEP).unwrap_err();
    assert_eq!(fault.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(failed.state(), RuntimeState::Faulted);
    let world = failed.world();
    assert_eq!(world.resource::<ConsumedCommands>().0, 1);
    let stats = world.resource::<GameplayCommandQueue>().stats();
    assert_eq!(stats.acknowledged, 1);
    assert_eq!(stats.acknowledged_through_tick, 1);
    assert!(world.contains_resource::<PartialEffect>());
    assert_eq!(world.resource::<CommittedSamples>().0.len(), 1);
    assert_eq!(world.resource::<Projections>().fixed.len(), 1);
    let original_sample = world.resource::<Solver>().sample();
    assert_eq!(original_sample.tick, 1);
    let observations = world.resource::<Trace>().observations();
    let at = |phase| {
        observations
            .iter()
            .position(|entry| entry.phase == phase)
            .unwrap()
    };
    assert!(at(Phase::Capture) < at(Phase::Failed));
    assert!(at(Phase::Published) < at(Phase::Failed));
    assert_eq!(failed.drive(Duration::ZERO).unwrap_err(), fault);
    assert_eq!(failed.world().resource::<CommittedSamples>().0.len(), 1);
    assert!(other.fault().is_none());
    other.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert!(other.fault().is_none());
    let other_sample = other.world().resource::<Solver>().sample();
    assert_pose_close(other_sample.kinematic, original_sample.kinematic);
    assert_pose_close(other_sample.dynamic, original_sample.dynamic);
    assert!(failed_cleanup.lock().unwrap().is_empty());
    assert!(other_cleanup.lock().unwrap().is_empty());
    failed.finish().unwrap();
    assert!(failed_audio.lock().unwrap().is_closed());
    assert_eq!(*failed_cleanup.lock().unwrap(), [(driver, original_sample)]);
    assert!(!other_audio.lock().unwrap().is_closed());
    assert!(other_cleanup.lock().unwrap().is_empty());
    other.finish().unwrap();
    assert!(other_audio.lock().unwrap().is_closed());
    assert_eq!(*other_cleanup.lock().unwrap(), [(driver, other_sample)]);
}

#[test]
fn production_parallel_preserves_explicitly_driven_custom_serial_schedule_policy() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    struct CustomSerial;

    initialize_host_compute_pool();
    let driver = thread::current().id();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut app = configured_motion(SolverMode::Direct, ExecutionProfile::Parallel);
    app.insert_resource(AutomaticThreads(Arc::clone(&observed)))
        .unwrap();
    app.add_systems(CustomSerial, |threads: Res<AutomaticThreads>| {
        threads
            .0
            .lock()
            .unwrap()
            .push(("custom-serial".to_owned(), thread::current().id()));
    })
    .unwrap();
    app.get_schedule_mut(CustomSerial)
        .unwrap()
        .unwrap()
        .set_executor(nara_ecs::bevy_ecs::schedule::SingleThreadedExecutor::new());
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert!(observed.lock().unwrap().is_empty());
    runtime.run_schedule(CustomSerial).unwrap();
    assert_eq!(runtime.execution_profile(), ExecutionProfile::Parallel);
    assert_eq!(
        *observed.lock().unwrap(),
        [("custom-serial".to_owned(), driver)]
    );
    assert_eq!(runtime.world().resource::<CommittedSamples>().0.len(), 1);
    runtime.finish().unwrap();
}
