use std::{f32::consts::FRAC_PI_2, time::Duration};

use nara::{
    ecs::{Single, bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    prelude::*,
};

use crate::{RunRules, arena, audio::Sounds};

const PILOT_RADIUS: f32 = 10.0;
const PICKUP_RADIUS: f32 = 15.0;
const GATE_RADIUS: f32 = 17.0;
const HIT_COOLDOWN: Duration = Duration::from_millis(950);
const PLUGIN: PluginId = PluginId::new("night-relay.game");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN, PluginCategory::Runtime);

/// Installs the shared arena and fixed simulation after Nara's `DefaultPlugins`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GamePlugin(pub RunRules);

impl Plugin for GamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let rules = checked_rules(self.0).map_err(|message| PluginError::SetupFailed {
            plugin: PLUGIN,
            message: message.to_owned(),
        })?;
        app.init_resource::<PilotInput>()?
            .insert_resource(Round::new(rules))?
            .add_systems(Startup, setup_arena)?
            .add_systems(
                FixedUpdate,
                advance_round
                    .in_set(FixedUpdateSet::Simulate)
                    .after(Physics2dSet::Step),
            )?;
        app.world_mut()?
            .spawn((Name::new("Round rules"), ActiveRules, rules));
        Ok(())
    }
}

fn checked_rules(rules: RunRules) -> Result<RunRules, &'static str> {
    if !(1..=180).contains(&rules.time_limit_seconds)
        || !(40.0..=300.0).contains(&rules.move_speed)
        || !(1..=5).contains(&rules.shield_charges)
    {
        return Err("round rules require 1–180 seconds, a speed of 40–300, and 1–5 shields");
    }
    Ok(rules)
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
    pub(crate) cells: [bool; arena::CELLS.len()],
}

impl Round {
    fn new(rules: RunRules) -> Self {
        Self {
            status: RoundStatus::Running,
            position: arena::START,
            velocity_limit: rules.move_speed,
            remaining: Duration::from_secs(u64::from(rules.time_limit_seconds)),
            shields: rules.shield_charges,
            wall_contacts: 0,
            elapsed: Duration::ZERO,
            cooldown: Duration::ZERO,
            heading: 0.0,
            moving: false,
            cells: [false; arena::CELLS.len()],
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

#[derive(Component)]
struct ActiveRules;

fn setup_arena(world: &mut World) -> SystemResult {
    world.insert_resource(PilotCharacter(TopDownCharacter::new(
        Collider2d::Circle {
            radius: PILOT_RADIUS,
        },
        0.1,
    )?));
    let sounds = Sounds::register(world)?;
    world.insert_resource(sounds);
    for (column, row, solid) in arena::tiles() {
        if solid {
            world.spawn((
                Transform2d::from_translation(arena::at(column, row)),
                Collider2d::Box {
                    half_extents: Vec2::splat(arena::TILE * 0.5),
                },
            ));
        }
    }
    Ok(())
}

fn advance_round(
    mut commands: Commands,
    mut round: ResMut<Round>,
    mut input: ResMut<PilotInput>,
    rules: Single<&RunRules, nara::ecs::With<ActiveRules>>,
    time: Res<FixedTime>,
    physics: Res<PhysicsWorld2d>,
    pilot: Res<PilotCharacter>,
    sounds: Res<Sounds>,
) -> SystemResult {
    if input.restart {
        input.restart = false;
        *round = Round::new(checked_rules(**rules)?);
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
        && arena::PATROLS.iter().any(|patrol| {
            position.distance_squared(patrol.position(elapsed))
                <= (arena::DRONE_RADIUS + PILOT_RADIUS).powi(2)
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

    for (collected, cell) in round.cells.iter_mut().zip(arena::CELLS) {
        if !*collected && position.distance_squared(cell) < PICKUP_RADIUS.powi(2) {
            *collected = true;
            commands.spawn(AudioPlayer::one_shot(sounds.pickup));
        }
    }
    if round.can_extract() && position.distance_squared(arena::GATE) < GATE_RADIUS.powi(2) {
        round.status = RoundStatus::Escaped;
        commands.spawn(AudioPlayer::one_shot(sounds.escaped));
    }
    Ok(())
}
