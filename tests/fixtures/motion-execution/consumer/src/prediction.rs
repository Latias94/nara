use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use motion_animation::{MotionIntent, emit_intent};
use motion_audio::{AudioPlugin, Cue, PcmAudio, SemanticEffectId};
use motion_contract::{Actors, MotionInput, RootMotionIntent, Trace};
use motion_physics::{PhysicsSet, Sample, Solver, SolverMode, SolverSnapshot};
use nara_app::{App, RuntimeInstance};
use nara_core::Vec2;
use nara_ecs::{
    Commands, Component, Entity, Query, Res, ResMut, Resource, World,
    bevy_ecs::schedule::SingleThreadedExecutor,
    schedule::{IntoScheduleConfigs, ScheduleLabel},
};
use nara_transform::Transform2d;

mod tests;

const MAX_TICK: u64 = 32;

type UncoveredSystems = (
    Unsupported,
    nara_ecs::bevy_ecs::schedule::ScheduleConfigs<nara_ecs::bevy_ecs::system::ScheduleSystem>,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Attempt {
    generation: nara_app::RuntimeGeneration,
    tick: u64,
    ordinal: u64,
}

#[derive(Debug, Clone, Copy)]
struct Input {
    tick: u64,
    event: u64,
    delta: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Outcome {
    attempt: Attempt,
    key: u8,
    value: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unsupported {
    SystemLocal,
    NativeOperation,
    ExternalIo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Refusal {
    Unsupported(Unsupported),
    InvalidInput,
    StaleAttempt,
    DuplicateOutcome,
    UnclassifiedOutcome,
    MissingOutcome,
    NotCompleted,
    ConfirmedHistory,
    ForeignCheckpoint,
    Solver(String),
    Audio(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
struct Particle {
    logical: u64,
    life: u8,
    value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingCommand {
    Spawn(Particle),
    Despawn(u64),
}

#[derive(Clone, Resource)]
struct Model {
    tick: u64,
    rng: u64,
    phase: u64,
    pending: Vec<PendingCommand>,
    admitted: Vec<(u64, u8, i32)>,
    effects: Vec<Cue>,
}

struct Step {
    input: Input,
    outcomes: Vec<Outcome>,
}

enum Invocation {
    Step(Step),
    Restore(Box<Checkpoint>),
}

#[derive(Clone, Resource)]
struct Inbox(Arc<Mutex<Option<Invocation>>>);

#[derive(Clone)]
struct Checkpoint {
    generation: nara_app::RuntimeGeneration,
    history: Vec<[u8; 32]>,
    model: Model,
    input: MotionInput,
    locals: [Transform2d; 3],
    intent: Option<RootMotionIntent>,
    sample: Sample,
    particles: Vec<Particle>,
    solver: SolverSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
enum PredictionSchedule {
    Attempt,
    Restore,
}

struct Session {
    runtime: RuntimeInstance,
    inbox: Arc<Mutex<Option<Invocation>>>,
    audio: Arc<Mutex<PcmAudio>>,
    current: Option<(Attempt, Input)>,
    outcomes: BTreeMap<u8, Outcome>,
    ordinal: u64,
    completed: bool,
    history: Vec<[u8; 32]>,
    confirmed: Option<(u64, [u8; 32])>,
}

impl Session {
    fn new(mode: SolverMode, unsupported: Option<UncoveredSystems>) -> Result<Self, Refusal> {
        if let Some((unsupported, _systems)) = unsupported {
            return Err(Refusal::Unsupported(unsupported));
        }
        let mut app = App::new();
        let inbox = Arc::new(Mutex::new(None));
        let audio = Arc::new(Mutex::new(
            PcmAudio::new(48_000).map_err(|error| Refusal::Audio(error.to_string()))?,
        ));
        app.add_plugin(AudioPlugin {
            owner: Arc::clone(&audio),
        })
        .unwrap();
        let solver = Solver::new(mode);
        let sample = solver.sample();
        let world = app.world_mut().unwrap();
        let parent = world.spawn(Transform2d::default()).id();
        let kinematic = world
            .spawn(Transform2d::from_translation(sample.kinematic))
            .id();
        let dynamic = world
            .spawn(Transform2d::from_translation(sample.dynamic))
            .id();
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
        world.insert_resource(sample);
        world.insert_resource(solver);
        world.insert_resource(Inbox(Arc::clone(&inbox)));
        world.init_resource::<Trace>();
        world.insert_resource(Model {
            tick: 0,
            rng: 0x7e57_a11c_19b3_8d25,
            phase: 0,
            pending: vec![PendingCommand::Spawn(Particle {
                logical: 100,
                life: 2,
                value: 17,
            })],
            admitted: Vec::new(),
            effects: Vec::new(),
        });
        app.configure_sets(
            PredictionSchedule::Attempt,
            MotionIntent.before(PhysicsSet::Target),
        )
        .unwrap();
        app.add_systems(
            PredictionSchedule::Attempt,
            (prepare, advance_particles).chain().before(MotionIntent),
        )
        .unwrap();
        app.add_systems(
            PredictionSchedule::Attempt,
            emit_intent.in_set(MotionIntent),
        )
        .unwrap();
        app.add_systems(PredictionSchedule::Attempt, motion_physics::systems())
            .unwrap();
        app.add_systems(
            PredictionSchedule::Attempt,
            finish_attempt.after(PhysicsSet::Callback),
        )
        .unwrap();
        app.add_systems(PredictionSchedule::Restore, restore)
            .unwrap();
        for schedule in [PredictionSchedule::Attempt, PredictionSchedule::Restore] {
            app.get_schedule_mut(schedule)
                .unwrap()
                .unwrap()
                .set_executor(SingleThreadedExecutor::new());
        }
        let mut session = Self {
            runtime: RuntimeInstance::start(app).unwrap(),
            inbox,
            audio,
            current: None,
            outcomes: BTreeMap::new(),
            ordinal: 0,
            completed: false,
            history: Vec::new(),
            confirmed: None,
        };
        session.history.push(session.checkpoint()?.digest());
        Ok(session)
    }

    fn begin(&mut self, input: Input) -> Result<Attempt, Refusal> {
        let tick = self.runtime.world().resource::<Model>().tick;
        if input.tick != tick + 1
            || input.tick > MAX_TICK
            || input.event == 0
            || !input.delta.is_finite()
            || self.current.is_some() && !self.completed
        {
            return Err(Refusal::InvalidInput);
        }
        self.ordinal = self.ordinal.checked_add(1).ok_or(Refusal::InvalidInput)?;
        let attempt = Attempt {
            generation: self.runtime.generation(),
            tick: input.tick,
            ordinal: self.ordinal,
        };
        self.current = Some((attempt, input));
        self.outcomes.clear();
        self.completed = false;
        Ok(attempt)
    }

    fn admit(&mut self, outcome: Outcome) -> Result<(), Refusal> {
        if self.current.map(|(attempt, _)| attempt) != Some(outcome.attempt) || self.completed {
            return Err(Refusal::StaleAttempt);
        }
        if !matches!(outcome.key, 0 | 1) {
            return Err(Refusal::UnclassifiedOutcome);
        }
        if self.outcomes.contains_key(&outcome.key) {
            return Err(Refusal::DuplicateOutcome);
        }
        self.outcomes.insert(outcome.key, outcome);
        Ok(())
    }

    fn abandon(&mut self, attempt: Attempt) -> Result<(), Refusal> {
        if self.current.map(|(active, _)| active) != Some(attempt) || self.completed {
            return Err(Refusal::StaleAttempt);
        }
        self.current = None;
        self.outcomes.clear();
        Ok(())
    }

    fn simulate(&mut self, attempt: Attempt) -> Result<(), Refusal> {
        let Some((active, input)) = self.current else {
            return Err(Refusal::StaleAttempt);
        };
        if active != attempt || self.completed {
            return Err(Refusal::StaleAttempt);
        }
        if self.outcomes.len() != 2 {
            return Err(Refusal::MissingOutcome);
        }
        *self.inbox.lock().unwrap() = Some(Invocation::Step(Step {
            input,
            outcomes: self.outcomes.values().copied().collect(),
        }));
        self.runtime
            .run_schedule(PredictionSchedule::Attempt)
            .unwrap();
        self.completed = true;
        self.outcomes.clear();
        self.history.push(self.checkpoint()?.digest());
        Ok(())
    }

    fn checkpoint(&self) -> Result<Checkpoint, Refusal> {
        if self.current.is_some() && !self.completed {
            return Err(Refusal::NotCompleted);
        }
        let world = self.runtime.world();
        let actors = world.resource::<Actors>();
        let mut particles = world
            .iter_entities()
            .filter_map(|entity| entity.get::<Particle>().copied())
            .collect::<Vec<_>>();
        particles.sort_by_key(|particle| particle.logical);
        Ok(Checkpoint {
            generation: self.runtime.generation(),
            history: self.history.clone(),
            model: world.resource::<Model>().clone(),
            input: *world.resource::<MotionInput>(),
            locals: [actors.parent, actors.kinematic, actors.dynamic]
                .map(|entity| *world.get::<Transform2d>(entity).unwrap()),
            intent: world.get::<RootMotionIntent>(actors.kinematic).copied(),
            sample: *world.resource::<Sample>(),
            particles,
            solver: world
                .resource::<Solver>()
                .checkpoint()
                .map_err(|error| Refusal::Solver(error.to_string()))?,
        })
    }

    fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), Refusal> {
        if checkpoint.generation != self.runtime.generation() {
            return Err(Refusal::ForeignCheckpoint);
        }
        if let Some((tick, digest)) = self.confirmed
            && checkpoint.history.get(tick as usize) != Some(&digest)
        {
            return Err(Refusal::ConfirmedHistory);
        }
        *self.inbox.lock().unwrap() = Some(Invocation::Restore(Box::new(checkpoint.clone())));
        self.runtime
            .run_schedule(PredictionSchedule::Restore)
            .unwrap();
        self.current = None;
        self.outcomes.clear();
        self.completed = false;
        self.history.clone_from(&checkpoint.history);
        Ok(())
    }

    fn confirm(&mut self, attempt: Attempt) -> Result<usize, Refusal> {
        if self.current.map(|(active, _)| active) != Some(attempt) {
            return Err(Refusal::StaleAttempt);
        }
        if !self.completed {
            return Err(Refusal::NotCompleted);
        }
        let count = self
            .audio
            .lock()
            .unwrap()
            .publish(&self.runtime.world().resource::<Model>().effects)
            .map_err(|error| Refusal::Audio(error.to_string()))?;
        self.confirmed = Some((attempt.tick, self.history[attempt.tick as usize]));
        Ok(count)
    }

    fn finish(self) {
        self.runtime.finish().unwrap();
    }
}

fn prepare(
    inbox: Res<Inbox>,
    mut model: ResMut<Model>,
    mut input: ResMut<MotionInput>,
    particles: Query<(Entity, &Particle)>,
    mut commands: Commands,
) {
    let Some(Invocation::Step(step)) = inbox.0.lock().unwrap().take() else {
        panic!("one exact prediction request");
    };
    for pending in model.pending.drain(..) {
        match pending {
            PendingCommand::Spawn(particle) => {
                commands.spawn(particle);
            }
            PendingCommand::Despawn(logical) => {
                let (entity, _) = particles
                    .iter()
                    .find(|(_, particle)| particle.logical == logical)
                    .unwrap();
                commands.entity(entity).despawn();
            }
        }
    }
    model.tick = step.input.tick;
    model.phase += 1;
    model.rng ^= model.rng << 13;
    model.rng ^= model.rng >> 7;
    model.rng ^= model.rng << 17;
    let correction = step
        .outcomes
        .iter()
        .map(|outcome| outcome.value as f32)
        .sum::<f32>()
        / 128.0;
    input.tick = step.input.tick;
    input.delta = step.input.delta + Vec2::new(correction, (model.phase % 2) as f32 / 64.0);
    model.admitted.extend(
        step.outcomes
            .into_iter()
            .map(|outcome| (outcome.attempt.tick, outcome.key, outcome.value)),
    );
    let frequency_hz = 220 + (model.rng % 5) as u32 * 55;
    model.effects.push(Cue {
        id: SemanticEffectId {
            input_event: step.input.event,
            occurrence: 1,
            cue: 1,
            sequence: 0,
        },
        at_sample: step.input.tick * 800,
        sample_count: 128,
        frequency_hz,
    });
}

fn advance_particles(mut particles: Query<&mut Particle>, mut model: ResMut<Model>) {
    for mut particle in &mut particles {
        particle.life -= 1;
        particle.value = particle.value.wrapping_add(model.rng);
        if particle.life == 0 {
            model
                .pending
                .push(PendingCommand::Despawn(particle.logical));
        }
    }
}

fn finish_attempt(mut model: ResMut<Model>, sample: Res<Sample>) {
    assert_eq!(sample.tick, model.tick);
    let particle = Particle {
        logical: 100 + model.tick,
        life: 2,
        value: model.rng,
    };
    model.pending.push(PendingCommand::Spawn(particle));
}

fn restore(world: &mut World) {
    let Some(Invocation::Restore(checkpoint)) = world.resource::<Inbox>().0.lock().unwrap().take()
    else {
        panic!("one exact checkpoint request");
    };
    world
        .resource_mut::<Solver>()
        .restore(&checkpoint.solver)
        .unwrap();
    let particles = world
        .iter_entities()
        .filter(|entity| entity.contains::<Particle>())
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    for entity in particles {
        world.despawn(entity);
    }
    for particle in &checkpoint.particles {
        world.spawn(*particle);
    }
    let actors = *world.resource::<Actors>();
    for (entity, transform) in [actors.parent, actors.kinematic, actors.dynamic]
        .into_iter()
        .zip(checkpoint.locals)
    {
        *world.get_mut::<Transform2d>(entity).unwrap() = transform;
    }
    if let Some(intent) = checkpoint.intent {
        world.entity_mut(actors.kinematic).insert(intent);
    } else {
        world
            .entity_mut(actors.kinematic)
            .remove::<RootMotionIntent>();
    }
    world.insert_resource(checkpoint.model);
    world.insert_resource(checkpoint.input);
    world.insert_resource(checkpoint.sample);
}

impl Checkpoint {
    fn digest(&self) -> [u8; 32] {
        let mut bytes = Vec::new();
        for value in [
            self.model.tick,
            self.model.rng,
            self.model.phase,
            self.input.tick,
        ] {
            word(&mut bytes, value);
        }
        vector(&mut bytes, self.input.delta);
        vector(&mut bytes, self.input.parent_translation);
        for local in self.locals {
            vector(&mut bytes, local.translation);
            word(&mut bytes, u64::from(local.rotation.to_bits()));
            vector(&mut bytes, local.scale);
        }
        match self.intent {
            Some(intent) => {
                word(&mut bytes, 1);
                word(&mut bytes, intent.tick);
                vector(&mut bytes, intent.delta);
            }
            None => word(&mut bytes, 0),
        }
        word(&mut bytes, self.sample.tick);
        vector(&mut bytes, self.sample.kinematic);
        vector(&mut bytes, self.sample.dynamic);
        word(&mut bytes, u64::from(self.sample.contact));
        word(&mut bytes, self.model.pending.len() as u64);
        for command in &self.model.pending {
            match command {
                PendingCommand::Spawn(particle) => {
                    word(&mut bytes, 0);
                    particle_bytes(&mut bytes, particle);
                }
                PendingCommand::Despawn(logical) => {
                    word(&mut bytes, 1);
                    word(&mut bytes, *logical);
                }
            }
        }
        word(&mut bytes, self.model.admitted.len() as u64);
        for (tick, key, value) in &self.model.admitted {
            word(&mut bytes, *tick);
            word(&mut bytes, u64::from(*key));
            bytes.extend(value.to_le_bytes());
        }
        word(&mut bytes, self.model.effects.len() as u64);
        for effect in &self.model.effects {
            for value in [
                effect.id.input_event,
                effect.id.occurrence,
                u64::from(effect.id.cue),
                u64::from(effect.id.sequence),
                effect.at_sample,
                u64::from(effect.sample_count),
                u64::from(effect.frequency_hz),
            ] {
                word(&mut bytes, value);
            }
        }
        word(&mut bytes, self.particles.len() as u64);
        for particle in &self.particles {
            particle_bytes(&mut bytes, particle);
        }
        let solver = self.solver.encoded().unwrap();
        word(&mut bytes, solver.len() as u64);
        bytes.extend(solver);
        *blake3::hash(&bytes).as_bytes()
    }
}

fn word(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend(value.to_le_bytes());
}

fn vector(bytes: &mut Vec<u8>, value: Vec2) {
    word(bytes, u64::from(value.x.to_bits()));
    word(bytes, u64::from(value.y.to_bits()));
}

fn particle_bytes(bytes: &mut Vec<u8>, particle: &Particle) {
    word(bytes, particle.logical);
    word(bytes, u64::from(particle.life));
    word(bytes, particle.value);
}
