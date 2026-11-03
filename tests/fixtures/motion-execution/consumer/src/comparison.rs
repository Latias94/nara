use std::{rc::Rc, thread::ThreadId};

use motion_animation::{MotionIntent, emit_intent};
use motion_contract::{Actors, MotionInput, Phase, Trace};
use motion_physics::{PhysicsSet, Solver, SolverMode};
use nara_core::Vec2;
use nara_ecs::{
    NonSend, Query, Res, ResMut, Resource, World,
    bevy_ecs::schedule::SingleThreadedExecutor,
    schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel, SystemSet},
};
use nara_transform::Transform2d;

#[derive(Debug, Clone, Copy)]
enum Executor {
    Serial,
    #[cfg(feature = "unified-multi-threaded")]
    Parallel,
}

fn executors() -> Vec<Executor> {
    vec![
        Executor::Serial,
        #[cfg(feature = "unified-multi-threaded")]
        Executor::Parallel,
    ]
}

impl Executor {
    fn apply(self, schedule: &mut Schedule) {
        match self {
            Self::Serial => {
                schedule.set_executor(SingleThreadedExecutor::new());
            }
            #[cfg(feature = "unified-multi-threaded")]
            Self::Parallel => {
                bevy_tasks::ComputeTaskPool::get_or_init(|| {
                    bevy_tasks::TaskPoolBuilder::new().num_threads(2).build()
                });
                schedule.set_executor(nara_ecs::bevy_ecs::schedule::MultiThreadedExecutor::new());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct Tick;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
struct Publication;

struct DriverThread(ThreadId, Rc<()>);

#[derive(Debug, Default, Resource)]
struct Published(Vec<(u64, Vec2, Vec2)>);

fn observe_non_send(owner: NonSend<DriverThread>, input: Res<MotionInput>, trace: Res<Trace>) {
    assert_eq!(owner.0, std::thread::current().id());
    assert_eq!(Rc::strong_count(&owner.1), 1);
    trace.record(input.tick, Phase::NonSend);
}

fn publish(
    actors: Res<Actors>,
    input: Res<MotionInput>,
    locals: Query<&Transform2d>,
    mut published: ResMut<Published>,
    trace: Res<Trace>,
) {
    let parent = locals.get(actors.parent).unwrap().translation;
    let kinematic = locals.get(actors.kinematic).unwrap().translation + parent;
    let dynamic = locals.get(actors.dynamic).unwrap().translation + parent;
    published.0.push((input.tick, kinematic, dynamic));
    trace.record(input.tick, Phase::Published);
}

fn initialize(world: &mut World, mode: SolverMode) {
    let parent = world.spawn(Transform2d::default()).id();
    let kinematic = world.spawn(Transform2d::default()).id();
    let dynamic = world.spawn(Transform2d::default()).id();
    world.insert_resource(Actors {
        parent,
        kinematic,
        dynamic,
    });
    world.insert_resource(MotionInput {
        tick: 0,
        delta: Vec2::ZERO,
        parent_translation: Vec2::ZERO,
    });
    world.insert_resource(Solver::new(mode));
    world.init_resource::<Trace>();
    world.init_resource::<Published>();
    world.insert_non_send(DriverThread(std::thread::current().id(), Rc::new(())));
}

fn input(world: &mut World, tick: u64) {
    let translation = Vec2::new(tick as f32 * 2.0, 0.0);
    *world.resource_mut::<MotionInput>() = MotionInput {
        tick,
        delta: Vec2::new(0.25, 0.0),
        parent_translation: translation,
    };
    let parent = world.resource::<Actors>().parent;
    world.get_mut::<Transform2d>(parent).unwrap().translation = translation;
}

fn public_schedule(executor: Executor) -> Schedule {
    let mut schedule = Schedule::new(Tick);
    executor.apply(&mut schedule);
    schedule.configure_sets((MotionIntent, PhysicsSet::Target).chain());
    schedule.configure_sets(
        (
            PhysicsSet::Target,
            PhysicsSet::Solve,
            PhysicsSet::Commit,
            PhysicsSet::Callback,
            Publication,
        )
            .chain(),
    );
    schedule.add_systems(emit_intent.in_set(MotionIntent));
    schedule.add_systems(motion_physics::systems());
    schedule.add_systems((observe_non_send, publish).chain().in_set(Publication));
    schedule
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
enum Segment {
    Intent,
    Physics,
    Publication,
}

#[derive(Debug, PartialEq, Eq)]
enum PlanError {
    Duplicate(Segment),
    Missing(Segment),
}

#[derive(Default)]
struct FinitePlan {
    segments: Vec<(Segment, Schedule)>,
}

impl FinitePlan {
    fn add(&mut self, label: Segment, schedule: Schedule) -> Result<(), PlanError> {
        if self.segments.iter().any(|(present, _)| *present == label) {
            return Err(PlanError::Duplicate(label));
        }
        self.segments.push((label, schedule));
        Ok(())
    }

    fn seal(mut self, world: &mut World) -> Result<Vec<Schedule>, PlanError> {
        let mut ordered = Vec::new();
        for required in [Segment::Intent, Segment::Physics, Segment::Publication] {
            let Some(index) = self
                .segments
                .iter()
                .position(|(label, _)| *label == required)
            else {
                return Err(PlanError::Missing(required));
            };
            let (_, schedule) = self.segments.remove(index);
            ordered.push(schedule);
        }
        for schedule in &mut ordered {
            schedule.initialize(world).unwrap();
        }
        Ok(ordered)
    }
}

fn finite_plan(world: &mut World, executor: Executor) -> Vec<Schedule> {
    let mut intent = Schedule::new(Segment::Intent);
    let mut physics = Schedule::new(Segment::Physics);
    let mut publication = Schedule::new(Segment::Publication);
    for schedule in [&mut intent, &mut physics, &mut publication] {
        executor.apply(schedule);
    }
    intent.add_systems(emit_intent);
    physics.configure_sets(
        (
            PhysicsSet::Target,
            PhysicsSet::Solve,
            PhysicsSet::Commit,
            PhysicsSet::Callback,
        )
            .chain(),
    );
    physics.add_systems(motion_physics::systems());
    publication.add_systems((observe_non_send, publish).chain());
    let mut plan = FinitePlan::default();
    plan.add(Segment::Publication, publication).unwrap();
    plan.add(Segment::Physics, physics).unwrap();
    plan.add(Segment::Intent, intent).unwrap();
    plan.seal(world).unwrap()
}

#[derive(Debug, Clone, Copy)]
enum Candidate {
    PublicSets,
    FinitePlan,
    BevyEmpty,
}

fn run(
    candidate: Candidate,
    executor: Executor,
    mode: SolverMode,
) -> (Vec<(u64, Vec2, Vec2)>, Trace) {
    let mut app = bevy_app::App::empty();
    initialize(app.world_mut(), mode);
    let mut schedules = match candidate {
        Candidate::PublicSets => vec![public_schedule(executor)],
        Candidate::FinitePlan => finite_plan(app.world_mut(), executor),
        Candidate::BevyEmpty => {
            let mut schedule = public_schedule(executor);
            schedule.initialize(app.world_mut()).unwrap();
            app.add_schedule(schedule);
            app.main_mut().update_schedule = Some(Tick.intern());
            Vec::new()
        }
    };
    for schedule in &mut schedules {
        schedule.initialize(app.world_mut()).unwrap();
    }
    for tick in 1..=8 {
        input(app.world_mut(), tick);
        if matches!(candidate, Candidate::BevyEmpty) {
            app.update();
        } else {
            for schedule in &mut schedules {
                schedule.run(app.world_mut());
            }
            app.world_mut().clear_trackers();
        }
    }
    let trace = app.world().resource::<Trace>().clone();
    let published = &app.world().resource::<Published>().0;
    if !matches!(mode, SolverMode::StandIn) {
        assert!(app.world().resource::<Solver>().sample().contact);
    }
    for tick in 1..=8 {
        let phases = trace
            .observations()
            .into_iter()
            .filter(|observation| observation.tick == tick)
            .map(|observation| observation.phase)
            .collect::<Vec<_>>();
        assert_eq!(
            phases,
            [
                Phase::Intent,
                Phase::Target,
                Phase::WorkerStarted,
                Phase::WorkerCompleted,
                Phase::Joined,
                Phase::PoseQueued,
                Phase::Callback,
                Phase::NonSend,
                Phase::Published,
            ],
            "{candidate:?}, {executor:?}, {mode:?}, tick {tick}"
        );
    }
    (published.clone(), trace)
}

#[test]
fn all_three_candidates_run_the_same_standin_and_real_borrowed_motion() {
    for executor in executors() {
        for mode in [
            SolverMode::StandIn,
            SolverMode::Direct,
            SolverMode::Borrowed,
        ] {
            let (expected, _) = run(Candidate::PublicSets, executor, mode);
            for candidate in [Candidate::FinitePlan, Candidate::BevyEmpty] {
                let (actual, _) = run(candidate, executor, mode);
                assert_eq!(actual, expected, "{candidate:?}, {executor:?}, {mode:?}");
            }
        }
    }
}

#[test]
fn direct_and_borrowed_real_solver_have_equal_pose_results() {
    for executor in executors() {
        for candidate in [
            Candidate::PublicSets,
            Candidate::FinitePlan,
            Candidate::BevyEmpty,
        ] {
            let (direct, _) = run(candidate, executor, SolverMode::Direct);
            let (borrowed, _) = run(candidate, executor, SolverMode::Borrowed);
            assert_eq!(borrowed, direct);
            assert!(direct[7].1.x > direct[0].1.x);
        }
    }
}

#[test]
fn serial_is_explicit_and_parallel_observes_actual_worker_placement_with_non_send_on_driver() {
    let owner = std::thread::current().id();
    for candidate in [
        Candidate::PublicSets,
        Candidate::FinitePlan,
        Candidate::BevyEmpty,
    ] {
        let (_, serial) = run(candidate, Executor::Serial, SolverMode::Direct);
        assert!(
            serial
                .observations()
                .iter()
                .all(|observation| observation.thread == owner)
        );
        #[cfg(feature = "unified-multi-threaded")]
        {
            let (_, parallel) = run(candidate, Executor::Parallel, SolverMode::Direct);
            assert!(
                parallel
                    .observations()
                    .iter()
                    .any(|observation| observation.thread != owner)
            );
            assert!(
                parallel
                    .observations()
                    .iter()
                    .filter(|observation| observation.phase == Phase::NonSend)
                    .all(|observation| observation.thread == owner)
            );
        }
    }
}

#[test]
fn missing_set_reference_is_not_a_provider_but_finite_plan_checks_its_closed_inventory() {
    let mut world = World::new();
    let mut schedule = Schedule::new(Tick);
    schedule.add_systems((|| {}).after(MotionIntent));
    assert!(schedule.initialize(&mut world).is_ok());
    let mut plan = FinitePlan::default();
    plan.add(Segment::Intent, Schedule::new(Segment::Intent))
        .unwrap();
    assert_eq!(
        plan.add(Segment::Intent, Schedule::new(Segment::Intent)),
        Err(PlanError::Duplicate(Segment::Intent))
    );
    assert!(matches!(
        plan.seal(&mut world),
        Err(PlanError::Missing(Segment::Physics))
    ));
}

#[test]
fn public_set_cycles_are_fallible_before_any_system_runs() {
    let mut world = World::new();
    let mut schedule = public_schedule(Executor::Serial);
    schedule.configure_sets(MotionIntent.after(Publication));
    assert!(schedule.initialize(&mut world).is_err());
}

#[test]
fn failed_borrow_does_not_queue_pose_or_run_a_success_callback() {
    let mut world = World::new();
    initialize(&mut world, SolverMode::Borrowed);
    input(&mut world, 1);
    world.insert_resource(
        Solver::new(SolverMode::Borrowed)
            .with_gate(motion_physics::WorkerGate::new(std::time::Duration::ZERO)),
    );
    world.insert_resource(nara_ecs::bevy_ecs::error::FallbackErrorHandler(|_, _| {}));
    let mut schedule = Schedule::new(Tick);
    Executor::Serial.apply(&mut schedule);
    schedule.configure_sets(MotionIntent.before(PhysicsSet::Target));
    schedule.add_systems(emit_intent.in_set(MotionIntent));
    schedule.add_systems(motion_physics::systems());
    schedule.run(&mut world);
    assert_eq!(
        world
            .resource::<Trace>()
            .observations()
            .iter()
            .map(|entry| entry.phase)
            .collect::<Vec<_>>(),
        [
            Phase::Intent,
            Phase::Target,
            Phase::WorkerStarted,
            Phase::Failed
        ]
    );
    assert!(!world.contains_resource::<motion_physics::Sample>());
    assert_eq!(world.resource::<Solver>().sample().tick, 0);
}
