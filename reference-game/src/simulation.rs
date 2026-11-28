use std::{f32::consts::FRAC_PI_2, time::Duration};

use nara::{
    audio::AudioBusId,
    ecs::{Single, bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    prelude::*,
    scene::{SceneUpdateSet, cooked::PreparedScene},
};

use crate::{
    RunRules,
    arena::{Actor, Arena, ArenaPlan, CELL_COUNT, COURIER, SceneActor, SceneRow, WALLS},
    audio::{ARRIVAL_BUS, Sounds},
    campaign::{Campaign, Sector, resolve_switch},
};

const HIT_COOLDOWN: Duration = Duration::from_millis(950);
const PLUGIN: PluginId = PluginId::new("night-relay.game");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN, PluginCategory::Runtime);

/// Binds the active scene to the shared fixed-step simulation.
#[derive(Clone, Copy, Debug, Default)]
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let world = app.world_mut()?;
        let instance = world
            .get_resource::<SceneManager>()
            .and_then(SceneManager::main_instance)
            .ok_or_else(|| setup_error("Night Relay requires an active scene"))?;
        bind_arena(world, instance, true).map_err(setup_error)?;
        app.add_systems(Startup, setup_sounds)?
            .add_systems(CoreStage::TaskUpdate, resolve_switch.after(SceneUpdateSet))?
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

fn setup_error(message: impl ToString) -> PluginError {
    PluginError::SetupFailed {
        plugin: PLUGIN,
        message: message.to_string(),
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

    #[cfg(feature = "desktop")]
    pub(crate) const fn collected_cells(&self) -> [bool; CELL_COUNT] {
        self.cells
    }

    pub(crate) fn can_extract(&self) -> bool {
        self.cells.iter().all(|&collected| collected)
    }
}

#[derive(Resource)]
pub(crate) struct PilotCharacter(TopDownCharacter);

impl PilotCharacter {
    pub(crate) fn new(radius: f32) -> Result<Self, nara::physics2d::PhysicsError> {
        Ok(Self(TopDownCharacter::new(
            Collider2d::Circle { radius },
            0.1,
        )?))
    }
}

#[derive(Clone, Copy, Resource)]
pub(crate) struct ArenaInstance(pub SceneInstanceId);

pub(crate) fn validate_arena(scene: &PreparedScene<'_>) -> Result<(), String> {
    let plan = ArenaPlan::prepare(scene.rows().map(|row| Ok(SceneRow::from_prepared(row))))?;
    PilotCharacter::new(plan.arena.pilot.radius).map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn bind_arena(
    world: &mut World,
    instance: SceneInstanceId,
    reset_round: bool,
) -> SystemResult {
    let entities = world
        .resource::<SceneManager>()
        .entities(instance)
        .ok_or("the Night Relay scene is not ready")?
        .values()
        .copied()
        .collect::<Vec<_>>();
    let rows = entities.iter().map(|&entity| {
        world
            .get_entity(entity)
            .map(SceneRow::from_entity)
            .map_err(|error| error.to_string())
    });
    let plan = ArenaPlan::prepare(rows)?;
    let character = PilotCharacter::new(plan.arena.pilot.radius)?;
    let (arena, rules) = plan.bind(world, &entities);
    if reset_round {
        world.insert_resource(Round::new(rules, arena.pilot.pose));
        world.insert_resource(PilotInput::default());
    }
    world.insert_resource(arena);
    world.insert_resource(character);
    world.insert_resource(ArenaInstance(instance));
    Ok(())
}

fn setup_sounds(world: &mut World) -> SystemResult {
    world
        .resource_mut::<AudioMixer>()
        .add_bus(AudioBusId::new(ARRIVAL_BUS))?;
    let sounds = Sounds::register(world)?;
    world.insert_resource(sounds);
    Ok(())
}

pub(crate) fn advance_round(
    mut commands: Commands,
    mut round: ResMut<Round>,
    mut input: ResMut<PilotInput>,
    rules: Single<&RunRules>,
    time: Res<FixedTime>,
    (physics, pilot): (Res<PhysicsWorld2d>, Res<PilotCharacter>),
    (arena, instance): (Res<Arena>, Res<ArenaInstance>),
    sounds: Res<Sounds>,
    (campaign, mut scenes): (Option<ResMut<Campaign>>, ResMut<SceneManager>),
) -> SystemResult {
    if input.restart {
        input.restart = false;
        if !campaign
            .as_ref()
            .is_some_and(|campaign| campaign.pending.is_some())
        {
            *round = Round::new(rules.checked()?, arena.pilot.pose);
        }
        return Ok(());
    }
    let mut play = |clip| {
        commands.spawn((SceneMember(instance.0), AudioPlayer::one_shot(clip)));
    };
    if round.status != RoundStatus::Running {
        return Ok(());
    }
    let delta = time.delta().min(round.remaining);
    round.elapsed += delta;
    round.remaining -= delta;
    if round.remaining.is_zero() {
        round.status = RoundStatus::Failed(Failure::TimeExpired);
        play(sounds.expired);
        return Ok(());
    }

    let movement = input.movement.clamp_length_max(1.0);
    let displacement = movement * round.velocity_limit * delta.as_secs_f32();
    round.moving = false;
    if displacement != Vec2::ZERO {
        let motion = physics.move_top_down_character_with_filter(
            &pilot.0,
            PhysicsPose2d::new(round.position, 0.0),
            displacement,
            delta,
            PhysicsQueryFilter2d {
                groups: Some(CollisionGroups2d {
                    memberships: COURIER,
                    filters: WALLS,
                }),
                ..Default::default()
            },
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
        play(sounds.impact);
        if round.shields == 0 {
            round.status = RoundStatus::Failed(Failure::ShieldDepleted);
            return Ok(());
        }
    }

    for (collected, cell) in round.cells.iter_mut().zip(arena.cells) {
        if !*collected && position.distance_squared(cell.pose.translation) < cell.radius.powi(2) {
            *collected = true;
            play(sounds.pickup);
        }
    }
    if round.can_extract()
        && position.distance_squared(arena.gate.pose.translation) < arena.gate.radius.powi(2)
    {
        round.status = RoundStatus::Escaped;
        play(sounds.escaped);
        if let Some(mut campaign) = campaign {
            campaign.clear(round.elapsed);
            if campaign.sector == Sector::One {
                campaign.pending =
                    Some(scenes.switch_scene_validated(Sector::Two.asset()?, validate_arena));
            }
        }
    }
    Ok(())
}

pub(crate) fn sync_actors(
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
