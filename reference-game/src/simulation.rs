use std::{f32::consts::FRAC_PI_2, time::Duration};

use nara::{
    ecs::{Single, bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    prelude::*,
    scene::authoring::SceneDocument,
};

use crate::{
    RunRules,
    arena::{self, Actor, Arena, CELL_COUNT, SceneActor},
    audio::Sounds,
};

const HIT_COOLDOWN: Duration = Duration::from_millis(950);
const PLUGIN: PluginId = PluginId::new("night-relay.game");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN, PluginCategory::Runtime);

/// Installs the shared arena and fixed simulation after Nara's `DefaultPlugins`.
#[derive(Clone, Debug)]
pub struct GamePlugin {
    scene: SceneDocument,
    rules_override: Option<RunRules>,
}

impl GamePlugin {
    /// Uses a source scene whose prefab instances have already been expanded.
    pub fn from_scene(scene: SceneDocument) -> Self {
        Self {
            scene,
            rules_override: None,
        }
    }

    pub fn with_rules(mut self, rules: RunRules) -> Self {
        self.rules_override = Some(rules);
        self
    }
}

impl Plugin for GamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let (arena, rules) = arena::spawn_scene(app.world_mut()?, &self.scene, self.rules_override)
            .map_err(setup_error)?;
        let round = Round::new(rules, arena.pilot.pose);
        app.init_resource::<PilotInput>()?
            .insert_resource(round)?
            .insert_resource(arena)?
            .add_systems(Startup, setup_round)?
            .add_systems(
                FixedUpdate,
                (advance_round, sync_actors)
                    .chain()
                    .in_set(FixedUpdateSet::Simulate)
                    .after(Physics2dSet::Step),
            )?;
        Ok(())
    }
}

fn setup_error(message: String) -> PluginError {
    PluginError::SetupFailed {
        plugin: PLUGIN,
        message,
    }
}

/// Held movement and a restart request consumed by the next fixed tick.
#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct PilotInput {
    pub movement: Vec2,
    pub restart: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundStatus {
    Running,
    Escaped,
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    TimeExpired,
    ShieldDepleted,
}

#[derive(Clone, Debug, PartialEq, Resource)]
pub struct Round {
    status: RoundStatus,
    position: Vec2,
    velocity_limit: f32,
    remaining: Duration,
    shields: u32,
    wall_contacts: u32,
    pub(crate) elapsed: Duration,
    pub(crate) cooldown: Duration,
    pub(crate) heading: f32,
    pub(crate) moving: bool,
    cells: [bool; CELL_COUNT],
}

impl Round {
    fn new(rules: RunRules, starting_pose: Transform2d) -> Self {
        Self {
            status: RoundStatus::Running,
            position: starting_pose.translation,
            velocity_limit: rules.move_speed,
            remaining: Duration::from_secs(u64::from(rules.time_limit_seconds)),
            shields: rules.shield_charges,
            wall_contacts: 0,
            elapsed: Duration::ZERO,
            cooldown: Duration::ZERO,
            heading: starting_pose.rotation,
            moving: false,
            cells: [false; CELL_COUNT],
        }
    }

    pub fn status(&self) -> RoundStatus {
        self.status
    }

    pub fn position(&self) -> Vec2 {
        self.position
    }

    pub fn remaining(&self) -> Duration {
        self.remaining
    }

    pub fn shields(&self) -> u32 {
        self.shields
    }

    pub fn wall_contacts(&self) -> u32 {
        self.wall_contacts
    }

    pub fn collected(&self) -> usize {
        self.cells.iter().filter(|&&collected| collected).count()
    }

    pub(crate) fn can_extract(&self) -> bool {
        self.cells.iter().all(|&collected| collected)
    }
}

#[derive(Resource)]
struct PilotCharacter(TopDownCharacter);

fn setup_round(world: &mut World) -> SystemResult {
    let pilot_radius = world.resource::<Arena>().pilot.radius;
    world.insert_resource(PilotCharacter(TopDownCharacter::new(
        Collider2d::Circle {
            radius: pilot_radius,
        },
        0.1,
    )?));
    let sounds = Sounds::register(world)?;
    world.insert_resource(sounds);
    Ok(())
}

fn advance_round(
    mut commands: Commands,
    mut round: ResMut<Round>,
    mut input: ResMut<PilotInput>,
    rules: Single<&RunRules>,
    time: Res<FixedTime>,
    (physics, pilot): (Res<PhysicsWorld2d>, Res<PilotCharacter>),
    arena: Res<Arena>,
    sounds: Res<Sounds>,
) -> SystemResult {
    if input.restart {
        input.restart = false;
        *round = Round::new(rules.checked()?, arena.pilot.pose);
        return Ok(());
    }
    if round.status != RoundStatus::Running {
        return Ok(());
    }
    let delta = time.delta().min(round.remaining);
    round.elapsed += delta;
    round.remaining -= delta;
    if round.remaining.is_zero() {
        round.status = RoundStatus::Failed(Failure::TimeExpired);
        commands.spawn(AudioPlayer::one_shot(sounds.expired));
        return Ok(());
    }

    let movement = input.movement.clamp_length_max(1.0);
    let displacement = movement * round.velocity_limit * delta.as_secs_f32();
    round.moving = false;
    if displacement != Vec2::ZERO {
        let motion = physics.move_top_down_character(
            &pilot.0,
            PhysicsPose2d::new(round.position, 0.0),
            displacement,
            delta,
            None,
        )?;
        round.position += motion.translation;
        round.moving = motion.translation.length_squared() > 0.0001;
        round.heading = movement.y.atan2(movement.x) - FRAC_PI_2;
        round.wall_contacts = round
            .wall_contacts
            .saturating_add(u32::from(motion.collision_count > 0));
    }

    round.cooldown = round.cooldown.saturating_sub(delta);
    let position = round.position;
    let elapsed = round.elapsed.as_secs_f32();
    if round.cooldown.is_zero()
        && arena.patrols.iter().any(|patrol| {
            position.distance_squared(patrol.position(elapsed))
                <= (patrol.radius() + arena.pilot.radius).powi(2)
        })
    {
        round.shields -= 1;
        round.cooldown = HIT_COOLDOWN;
        commands.spawn(AudioPlayer::one_shot(sounds.impact));
        if round.shields == 0 {
            round.status = RoundStatus::Failed(Failure::ShieldDepleted);
            return Ok(());
        }
    }

    for (collected, cell) in round.cells.iter_mut().zip(arena.cells) {
        if !*collected && position.distance_squared(cell.pose.translation) < cell.radius.powi(2) {
            *collected = true;
            commands.spawn(AudioPlayer::one_shot(sounds.pickup));
        }
    }
    if round.can_extract()
        && position.distance_squared(arena.gate.pose.translation) < arena.gate.radius.powi(2)
    {
        round.status = RoundStatus::Escaped;
        commands.spawn(AudioPlayer::one_shot(sounds.escaped));
    }
    Ok(())
}

fn sync_actors(
    round: Res<Round>,
    arena: Res<Arena>,
    mut actors: Query<(&SceneActor, &mut Transform2d, &mut Visibility)>,
) {
    let age = round.elapsed.as_secs_f32();
    for (actor, mut transform, mut visibility) in &mut actors {
        let mut pose = *transform;
        let hidden = match actor.role {
            Actor::Pilot => {
                pose.translation = round.position();
                pose.rotation = round.heading;
                round.status() == RoundStatus::Failed(Failure::ShieldDepleted)
                    || !round.cooldown.is_zero() && (age * 14.0) as u32 % 2 == 0
            }
            Actor::Cell(index) => {
                pose.rotation = actor.rotation + 0.15 * (age * 4.0 + index as f32).sin();
                round.cells[index]
            }
            Actor::Drone(index) => {
                pose.translation = arena.patrols[index].position(age);
                false
            }
            Actor::Gate => false,
        };
        let shown = if hidden {
            Visibility::Hidden
        } else {
            actor.visibility
        };
        if *transform != pose {
            *transform = pose;
        }
        if *visibility != shown {
            *visibility = shown;
        }
    }
}
