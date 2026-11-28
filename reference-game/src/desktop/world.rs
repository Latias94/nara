use std::{f32::consts::FRAC_PI_2, time::Duration};

use nara::{
    ecs::bevy_ecs::error::Result as SystemResult,
    prelude::*,
    render::{
        camera::{CameraBounds2d, CameraRig2d, CameraShake2d},
        material::{AlphaMode2d, SamplerDescriptor},
        particles::{ParticleEmitter2d, ParticleEmitterConfig},
    },
    render_wgpu::{Bloom2d, PostProcess2d, ScreenFade2d},
    scene::components::{Camera2dSource, SpriteSource},
};

use crate::{
    Round, RoundStatus,
    arena::{Actor, Arena, CELL_COUNT, SceneActor},
    campaign::{Campaign, Sector},
    simulation::{ArenaInstance, sync_actors},
};

const BACKGROUND: Color = Color::rgb(0.009, 0.015, 0.026);
const GATE_POWERED: Color = Color::rgb(0.12, 0.90, 0.60);
const POWERED_SCALE: f32 = 1.16;
const FADE_SECONDS: f32 = 0.46;

#[derive(Resource)]
pub(super) struct ArenaEffects {
    owner: SceneMember,
    sector: Sector,
    pub(super) pilot: Option<Entity>,
    pub(super) camera: Option<Entity>,
    pub(super) tail: Option<Entity>,
    pub(super) cells: [Option<Entity>; CELL_COUNT],
    pub(super) gate: Option<Entity>,
    shields: u32,
    collected: [bool; CELL_COUNT],
    powered: bool,
}

pub(super) fn dress_arena(world: &mut World) -> SystemResult {
    let owner = SceneMember(world.resource::<ArenaInstance>().0);
    let previous = world.get_resource::<ArenaEffects>();
    if previous.is_some_and(|effects| effects.owner == owner) {
        return Ok(());
    }
    let sector = world.resource::<Campaign>().sector;
    let switching_sectors = previous.is_some_and(|effects| effects.sector != sector);
    let round = world.resource::<Round>();
    let mut effects = ArenaEffects {
        owner,
        sector,
        pilot: None,
        camera: None,
        tail: None,
        cells: [None; CELL_COUNT],
        gate: None,
        shields: round.shields(),
        collected: round.collected_cells(),
        powered: round.can_extract(),
    };
    let sources = world
        .query::<(
            Entity,
            &SceneMember,
            &SceneActor,
            &SpriteSource,
            &Transform2d,
        )>()
        .iter(world)
        .filter(|(_, member, _, _, _)| **member == owner)
        .map(|(entity, _, actor, source, pose)| (entity, actor.role, source.clone(), *pose))
        .collect::<Vec<_>>();
    for (entity, role, source, pose) in sources {
        if source.image.is_some()
            && let Some(mut sprite) = world.get_mut::<Sprite>(entity)
        {
            sprite.material.sampler = SamplerDescriptor::NEAREST_CLAMP;
        }
        decorate_actor(world, owner, entity, &source, pose, role);
        match role {
            Actor::Pilot => {
                effects.pilot = Some(entity);
                effects.tail = Some(spawn_tail(world, owner, entity)?);
            }
            Actor::Cell(index) => {
                effects.cells[index] = Some(spawn_cell_sparks(world, owner, pose, index)?);
            }
            Actor::Gate => {
                effects.gate = Some(entity);
                if effects.powered {
                    if let Some(mut transform) = world.get_mut::<Transform2d>(entity) {
                        transform.scale = pose.scale * POWERED_SCALE;
                    }
                    if let Some(mut sprite) = world.get_mut::<Sprite>(entity) {
                        sprite.material.tint = GATE_POWERED;
                    }
                }
            }
            Actor::Drone(_) => {}
        }
    }

    effects.camera = world
        .query::<(Entity, &SceneMember, &Camera2dSource)>()
        .iter(world)
        .filter(|(_, member, camera)| **member == owner && camera.active)
        .min_by_key(|(entity, _, camera)| (camera.order, entity.to_bits()))
        .map(|(entity, _, _)| entity);
    if let Some(camera) = effects.camera {
        let rig = CameraRig2d {
            target: effects.pilot,
            deadzone: Vec2::new(110.0, 68.0),
            smooth_time: 0.18,
            bounds: Some(CameraBounds2d::new(
                Vec2::new(-354.0, -212.0),
                Vec2::new(354.0, 212.0),
            )),
            ..Default::default()
        };
        world
            .entity_mut(camera)
            .insert((rig, CameraShake2d::default()));
    }
    if switching_sectors {
        world.resource_mut::<PostProcess2d>().screen_fade = Some(ScreenFade2d {
            color: BACKGROUND,
            alpha: 1.0,
        });
    }
    world.insert_resource(effects);
    world.run_system_cached(sync_actors)?;
    Ok(())
}

pub(super) fn post_process() -> PostProcess2d {
    PostProcess2d {
        bloom: Some(Bloom2d {
            threshold: 0.65,
            intensity: 0.23,
            radius: 3.0,
        }),
        ..Default::default()
    }
}

pub(super) fn update_fade(time: Res<VirtualTime>, mut effects: ResMut<PostProcess2d>) {
    if let Some(fade) = &mut effects.screen_fade {
        fade.alpha -= time.delta_seconds() / FADE_SECONDS;
        if fade.alpha <= 0.0 {
            effects.screen_fade = None;
        }
    }
}

fn decorate_actor(
    world: &mut World,
    owner: SceneMember,
    entity: Entity,
    source: &SpriteSource,
    pose: Transform2d,
    role: Actor,
) {
    match role {
        Actor::Pilot => quad(
            world,
            owner,
            pose.translation,
            source.size * 0.81,
            Color::rgb(0.035, 0.18, 0.20),
            -6,
        ),
        Actor::Cell(_) => {
            quad(
                world,
                owner,
                pose.translation,
                source.size * 1.73,
                Color::rgb(0.16, 0.115, 0.035),
                -6,
            );
            if source.image.is_none() {
                detail(
                    world,
                    owner,
                    entity,
                    source.size * 0.4,
                    Color::rgb(1.0, 0.93, 0.65),
                    source.layer.saturating_add(1),
                );
            }
        }
        Actor::Drone(index) => {
            let (start, end) = world.resource::<Arena>().patrols[index].endpoints();
            let track = (end - start).abs() + Vec2::splat(2.0);
            quad(
                world,
                owner,
                start.midpoint(end),
                track,
                Color::rgb(0.22, 0.06, 0.10),
                -5,
            );
            if source.image.is_none() {
                detail(
                    world,
                    owner,
                    entity,
                    source.size * Vec2::new(0.55, 0.22),
                    Color::rgb(1.0, 0.68, 0.52),
                    source.layer.saturating_add(1),
                );
            }
        }
        Actor::Gate if source.image.is_none() => {
            detail(
                world,
                owner,
                entity,
                source.size * 0.71,
                BACKGROUND,
                source.layer.saturating_add(1),
            );
            detail(
                world,
                owner,
                entity,
                source.size * Vec2::new(0.18, 0.54),
                Color::rgb(0.09, 0.47, 0.38),
                source.layer.saturating_add(2),
            );
        }
        Actor::Gate => {}
    }
}

fn quad(
    world: &mut World,
    owner: SceneMember,
    position: Vec2,
    size: Vec2,
    color: Color,
    layer: i32,
) {
    world.spawn((
        owner,
        Transform2d::from_translation(position),
        Visibility::Inherited,
        Sprite::from_color(size, color).with_layer(layer),
    ));
}

fn detail(
    world: &mut World,
    owner: SceneMember,
    parent: Entity,
    size: Vec2,
    color: Color,
    layer: i32,
) {
    world.spawn((
        owner,
        ChildOf(parent),
        Transform2d::default(),
        Visibility::Inherited,
        Sprite::from_color(size, color).with_layer(layer),
    ));
}

fn spawn_tail(world: &mut World, owner: SceneMember, pilot: Entity) -> SystemResult<Entity> {
    let emitter = ParticleEmitter2d::new(ParticleEmitterConfig {
        max_particles: 24,
        lifetime_range: 0.18..=0.36,
        speed_range: 34.0..=66.0,
        angle_range: (-FRAC_PI_2 - 0.30)..=(-FRAC_PI_2 + 0.30),
        size_over_life: [5.0, 1.0],
        color_over_life: [
            Color::rgba(0.24, 0.90, 0.80, 0.83),
            Color::rgba(0.03, 0.45, 0.72, 0.0),
        ],
        alpha_mode: AlphaMode2d::Additive,
        seed: 971,
        layer: 4,
        ..Default::default()
    })?;
    Ok(world
        .spawn((
            owner,
            ChildOf(pilot),
            Transform2d::from_xy(0.0, -13.0),
            emitter,
        ))
        .id())
}

fn spawn_cell_sparks(
    world: &mut World,
    owner: SceneMember,
    pose: Transform2d,
    index: usize,
) -> SystemResult<Entity> {
    let emitter = ParticleEmitter2d::new(ParticleEmitterConfig {
        max_particles: 16,
        lifetime_range: 0.25..=0.54,
        speed_range: 34.0..=91.0,
        size_over_life: [6.0, 1.0],
        color_over_life: [
            Color::rgba(1.0, 0.76, 0.27, 0.92),
            Color::rgba(0.96, 0.26, 0.07, 0.0),
        ],
        alpha_mode: AlphaMode2d::Additive,
        seed: 4831 + index as u64,
        layer: 3,
        ..Default::default()
    })?;
    Ok(world
        .spawn((
            owner,
            Transform2d::from_translation(pose.translation),
            emitter,
        ))
        .id())
}

pub(super) fn update_appearance(
    mut commands: Commands,
    round: Res<Round>,
    arena: Res<Arena>,
    effects: Option<ResMut<ArenaEffects>>,
    mut animators: Query<&mut SpriteAnimator>,
    mut emitters: Query<&mut ParticleEmitter2d>,
    mut shakes: Query<&mut CameraShake2d>,
    mut gates: Query<(&SpriteSource, &mut Sprite, &mut Transform2d)>,
) -> SystemResult {
    let Some(mut effects) = effects else {
        return Ok(());
    };
    let thrust = round.moving && round.status() == RoundStatus::Running;
    let clip = if thrust { "thrust" } else { "idle" };
    if let Some(pilot) = effects.pilot
        && let Ok(mut animator) = animators.get_mut(pilot)
        && animator.clip() != clip
    {
        animator.play(clip);
    }
    let rate = if thrust { 48.0 } else { 0.0 };
    if let Some(tail) = effects.tail
        && let Ok(mut emitter) = emitters.get_mut(tail)
        && emitter.rate() != rate
    {
        emitter.set_rate(rate)?;
    }
    let shields = round.shields();
    if shields < effects.shields
        && let Some(camera) = effects.camera
        && let Ok(mut shake) = shakes.get_mut(camera)
    {
        shake.add_trauma(0.86);
    }
    let collected = round.collected_cells();
    for (index, &picked_up) in collected.iter().enumerate() {
        if picked_up
            && !effects.collected[index]
            && let Some(cell) = effects.cells[index]
            && let Ok(mut emitter) = emitters.get_mut(cell)
        {
            emitter.emit(16);
        }
    }
    let powered = round.can_extract();
    if powered != effects.powered
        && let Some(gate) = effects.gate
        && let Ok((source, mut sprite, mut pose)) = gates.get_mut(gate)
    {
        let rest = arena.gate.pose.scale;
        if powered {
            commands.entity(gate).insert(
                Tween2d::new(Duration::from_millis(300), Easing::CubicOut)
                    .with_scale(rest, rest * POWERED_SCALE)
                    .with_tint(source.tint(), GATE_POWERED),
            );
        } else {
            commands.entity(gate).remove::<Tween2d>();
            pose.scale = rest;
            sprite.material.tint = source.tint();
        }
    }
    effects.shields = shields;
    effects.collected = collected;
    effects.powered = powered;
    Ok(())
}
