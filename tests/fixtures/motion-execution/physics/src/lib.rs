//! Translation-only solver fixture; no production backend selection or execution policy.

#![forbid(unsafe_code)]

use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use motion_contract::{Actors, MotionInput, Phase, RootMotionIntent, Trace};
use nara_app::{
    App, CoreStage, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
    PluginId,
};
use nara_core::Vec2;
use nara_ecs::{
    Commands, Query, Res, ResMut, Resource, SystemSet,
    bevy_ecs::{error::BevyError, schedule::ScheduleConfigs, system::ScheduleSystem},
    schedule::IntoScheduleConfigs,
};
use nara_transform::Transform2d;
use rapier2d::{math::Vector, prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverMode {
    StandIn,
    Direct,
    Borrowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum PhysicsSet {
    Target,
    Solve,
    Commit,
    Callback,
}

#[derive(Debug, Clone, Copy, PartialEq, Resource)]
pub struct Sample {
    pub tick: u64,
    pub kinematic: Vec2,
    pub dynamic: Vec2,
    pub contact: bool,
}

/// A one-shot worker hold. Both observer and worker waits have the supplied timeout.
#[derive(Debug, Clone)]
pub struct WorkerGate {
    state: Arc<(Mutex<(bool, bool)>, Condvar)>,
    timeout: Duration,
}

impl WorkerGate {
    pub fn new(timeout: Duration) -> Self {
        Self {
            state: Arc::new((Mutex::new((false, false)), Condvar::new())),
            timeout,
        }
    }

    pub fn wait_started(&self) -> bool {
        let (state, changed) = &*self.state;
        let (state, _) = changed
            .wait_timeout_while(
                state.lock().expect("worker gate poisoned"),
                self.timeout,
                |state| !state.0,
            )
            .expect("worker gate poisoned");
        state.0
    }

    pub fn release(&self) {
        let (state, changed) = &*self.state;
        state.lock().expect("worker gate poisoned").1 = true;
        changed.notify_all();
    }

    fn hold_worker(&self) -> Result<(), &'static str> {
        let (state, changed) = &*self.state;
        let mut state = state.lock().map_err(|_| "worker gate poisoned")?;
        state.0 = true;
        changed.notify_all();
        let (state, _) = changed
            .wait_timeout_while(state, self.timeout, |state| !state.1)
            .map_err(|_| "worker gate poisoned")?;
        if state.1 {
            Ok(())
        } else {
            Err("worker gate release timed out")
        }
    }
}

#[derive(Resource)]
pub struct Solver {
    mode: SolverMode,
    gate: Option<WorkerGate>,
    sample: Sample,
    joined_tick: Option<u64>,
    target: Vec2,
    pipeline: PhysicsPipeline,
    integration: IntegrationParameters,
    islands: IslandManager,
    broad: BroadPhaseBvh,
    narrow: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse: ImpulseJointSet,
    multibody: MultibodyJointSet,
    ccd: CCDSolver,
    kinematic: RigidBodyHandle,
    dynamic: RigidBodyHandle,
    floor_collider: ColliderHandle,
    dynamic_collider: ColliderHandle,
}

impl Solver {
    /// The floor top is y=0; the dynamic box starts resting at (3, 0.5).
    pub fn new(mode: SolverMode) -> Self {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let kinematic = bodies.insert(
            RigidBodyBuilder::kinematic_position_based().translation(Vector::new(0.0, 2.0)),
        );
        let dynamic = bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(3.0, 0.5))
                .lock_rotations()
                .can_sleep(false),
        );
        colliders.insert_with_parent(ColliderBuilder::cuboid(0.25, 0.25), kinematic, &mut bodies);
        let dynamic_collider = colliders.insert_with_parent(
            ColliderBuilder::cuboid(0.5, 0.5).restitution(0.0),
            dynamic,
            &mut bodies,
        );
        let floor_collider = colliders
            .insert(ColliderBuilder::cuboid(100.0, 0.5).translation(Vector::new(0.0, -0.5)));
        let sample = Sample {
            tick: 0,
            kinematic: Vec2::new(0.0, 2.0),
            dynamic: Vec2::new(3.0, 0.5),
            contact: false,
        };
        Self {
            mode,
            gate: None,
            sample,
            joined_tick: None,
            target: sample.kinematic,
            pipeline: PhysicsPipeline::new(),
            integration: IntegrationParameters::default(),
            islands: IslandManager::new(),
            broad: BroadPhaseBvh::new(),
            narrow: NarrowPhase::new(),
            bodies,
            colliders,
            impulse: ImpulseJointSet::new(),
            multibody: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            kinematic,
            dynamic,
            floor_collider,
            dynamic_collider,
        }
    }

    pub fn with_gate(mut self, gate: WorkerGate) -> Self {
        self.gate = Some(gate);
        self
    }

    pub fn sample(&self) -> Sample {
        self.sample
    }

    pub fn correct_kinematic(&mut self, position: Vec2) {
        let translation = Vector::new(position.x, position.y);
        self.bodies[self.kinematic].set_translation(translation, true);
        self.bodies[self.kinematic].set_next_kinematic_translation(translation);
        self.sample.kinematic = position;
        self.target = position;
        self.joined_tick = None;
    }

    fn step(&mut self, tick: u64) {
        if self.mode == SolverMode::StandIn {
            self.sample = Sample {
                tick,
                kinematic: self.target,
                contact: true,
                ..self.sample
            };
            return;
        }
        self.bodies[self.kinematic]
            .set_next_kinematic_translation(Vector::new(self.target.x, self.target.y));
        self.pipeline.step(
            Vector::new(0.0, -9.81),
            &self.integration,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse,
            &mut self.multibody,
            &mut self.ccd,
            &(),
            &(),
        );
        let kinematic = self.bodies[self.kinematic].translation();
        let dynamic = self.bodies[self.dynamic].translation();
        self.sample = Sample {
            tick,
            kinematic: Vec2::new(kinematic.x, kinematic.y),
            dynamic: Vec2::new(dynamic.x, dynamic.y),
            contact: self
                .narrow
                .contact_pair(self.floor_collider, self.dynamic_collider)
                .is_some_and(|pair| pair.has_any_active_contact()),
        };
    }
}

pub fn target(
    actors: Res<Actors>,
    input: Res<MotionInput>,
    intents: Query<&RootMotionIntent>,
    mut solver: ResMut<Solver>,
    trace: Res<Trace>,
) {
    let intent = intents
        .get(actors.kinematic)
        .expect("kinematic root-motion intent missing");
    assert_eq!(intent.tick, input.tick, "stale root-motion intent");
    solver.target = solver.sample.kinematic + intent.delta;
    trace.record(input.tick, Phase::Target);
}

/// Only the solver is borrowed by the scoped worker; ECS access remains on the driver.
pub fn solve(
    mut solver: ResMut<Solver>,
    input: Res<MotionInput>,
    trace: Res<Trace>,
) -> Result<(), BevyError> {
    let tick = input.tick;
    let solver = &mut *solver;
    solver.joined_tick = None;
    let result = if solver.mode == SolverMode::Borrowed {
        let trace = trace.clone();
        std::thread::scope(|scope| {
            let borrowed_solver = &mut *solver;
            let worker = scope.spawn(move || {
                trace.record(tick, Phase::WorkerStarted);
                if let Some(gate) = &borrowed_solver.gate {
                    gate.hold_worker()?;
                }
                borrowed_solver.step(tick);
                trace.record(tick, Phase::WorkerCompleted);
                Ok::<(), &'static str>(())
            });
            worker.join().unwrap_or(Err("physics worker panicked"))
        })
    } else {
        trace.record(tick, Phase::WorkerStarted);
        solver.step(tick);
        trace.record(tick, Phase::WorkerCompleted);
        Ok(())
    };
    if let Err(message) = result {
        trace.record(tick, Phase::Failed);
        return Err(BevyError::error(std::io::Error::other(message)));
    }
    solver.joined_tick = Some(tick);
    trace.record(tick, Phase::Joined);
    Ok(())
}

pub fn commit(
    actors: Res<Actors>,
    input: Res<MotionInput>,
    solver: Res<Solver>,
    mut commands: Commands,
    trace: Res<Trace>,
) {
    let sample = solver.sample();
    assert_eq!(
        sample.tick, input.tick,
        "physics solve did not complete this tick"
    );
    commands
        .entity(actors.kinematic)
        .insert(Transform2d::from_translation(
            sample.kinematic - input.parent_translation,
        ));
    commands
        .entity(actors.dynamic)
        .insert(Transform2d::from_translation(
            sample.dynamic - input.parent_translation,
        ));
    commands.insert_resource(sample);
    trace.record(input.tick, Phase::PoseQueued);
}

pub fn callback(
    actors: Res<Actors>,
    input: Res<MotionInput>,
    sample: Res<Sample>,
    locals: Query<&Transform2d>,
    trace: Res<Trace>,
) {
    assert_eq!(sample.tick, input.tick, "callback observed stale sample");
    let parent = locals
        .get(actors.parent)
        .expect("parent transform missing")
        .translation;
    assert_eq!(parent, input.parent_translation);
    for (entity, expected) in [
        (actors.kinematic, sample.kinematic),
        (actors.dynamic, sample.dynamic),
    ] {
        let actual = locals
            .get(entity)
            .expect("committed transform missing")
            .translation
            + parent;
        assert!(
            (actual - expected).length() < 1e-5,
            "callback ran before pose commit"
        );
    }
    trace.record(input.tick, Phase::Callback);
}

fn solved_this_tick(solver: Res<Solver>, input: Res<MotionInput>) -> bool {
    solver.joined_tick == Some(input.tick)
}

/// Includes local ordering and the deferred Commit-to-Callback visibility boundary.
pub fn systems() -> ScheduleConfigs<ScheduleSystem> {
    (
        target.in_set(PhysicsSet::Target).before(PhysicsSet::Solve),
        solve.in_set(PhysicsSet::Solve).before(PhysicsSet::Commit),
        commit
            .run_if(solved_this_tick)
            .in_set(PhysicsSet::Commit)
            .before(PhysicsSet::Callback),
        callback
            .run_if(solved_this_tick)
            .in_set(PhysicsSet::Callback),
    )
        .chain()
}

pub const PHYSICS_PLUGIN_ID: PluginId = PluginId::new("fixture.motion.physics");

pub struct PhysicsPlugin {
    pub mode: SolverMode,
}

impl Plugin for PhysicsPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PHYSICS_PLUGIN_ID, PluginCategory::Runtime)
                .requires_plugins(&[nara_transform::TRANSFORM_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(Solver::new(self.mode))?;
        app.configure_sets(
            CoreStage::FixedUpdate,
            (
                PhysicsSet::Target,
                PhysicsSet::Solve,
                PhysicsSet::Commit,
                PhysicsSet::Callback,
            )
                .chain(),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            systems().in_set(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}
