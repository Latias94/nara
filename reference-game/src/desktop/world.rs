use std::time::Duration;

use nara::{
    asset::add_asset,
    ecs::{
        Local, Single,
        bevy_ecs::{error::Result as SystemResult, query::With},
    },
    prelude::*,
    render::{
        animation::{Clip, Frame, PlaybackMode},
        material::SamplerDescriptor,
    },
    scene::components::SpriteSource,
    window::{PrimaryWindow, Window},
};

use crate::{
    Round, RoundStatus,
    arena::{self, Actor, Arena, SceneActor},
    simulation::{ArenaInstance, sync_actors},
};

const BACKGROUND: Color = Color::rgb(0.009, 0.015, 0.026);
const GATE_POWERED: Color = Color::rgb(0.12, 0.90, 0.60);
const ARENA_HEIGHT: f32 = 484.0;

#[derive(Component)]
pub(super) struct AnimatedPilot;

#[derive(Resource)]
pub(super) struct PilotAnimation {
    clip: Clip,
    image: Handle<ImageAsset>,
}

pub(super) fn setup(world: &mut World) -> SystemResult {
    world.spawn((
        Name::new("Arena camera"),
        Transform2d::default(),
        Camera2d {
            viewport_height: ARENA_HEIGHT,
            clear_color: Some(BACKGROUND),
            ..Default::default()
        },
    ));
    let size = Vec2::new(0.5, 1.0);
    let clip = Clip::new(
        PlaybackMode::Loop,
        [
            Frame::new(
                TextureRegion::new(Vec2::ZERO, size),
                Duration::from_millis(110),
            )?,
            Frame::new(
                TextureRegion::new(Vec2::new(0.5, 0.0), size),
                Duration::from_millis(110),
            )?,
        ],
    )?;
    let image = add_asset(world, make_pilot_texture()?)?;
    world.insert_resource(PilotAnimation { clip, image });
    Ok(())
}

pub(super) fn dress_arena(
    world: &mut World,
    mut dressed: Local<Option<SceneInstanceId>>,
) -> SystemResult {
    let owner = SceneMember(world.resource::<ArenaInstance>().0);
    if *dressed == Some(owner.0) {
        return Ok(());
    }
    let animation = world.resource::<PilotAnimation>();
    let pilot_image = animation.image;
    let initial_region = animation.clip.sample(Duration::ZERO).region;
    let powered = world.resource::<Round>().can_extract();
    let sources = world
        .query::<(
            Entity,
            &SceneMember,
            &SpriteSource,
            &Transform2d,
            Option<&SceneActor>,
        )>()
        .iter(world)
        .filter(|(_, member, _, _, _)| **member == owner)
        .map(|(entity, _, source, pose, actor)| {
            (entity, source.clone(), *pose, actor.map(|actor| actor.role))
        })
        .collect::<Vec<_>>();
    for (entity, source, pose, role) in sources {
        if source.image.is_none() && role == Some(Actor::Pilot) {
            let sprite = Sprite::from_texture(pilot_image, source.size)
                .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
                .with_texture_region(initial_region)
                .with_tint(source.tint())
                .with_layer(source.layer);
            world.entity_mut(entity).insert((AnimatedPilot, sprite));
        } else if source.image.is_some() || powered && role == Some(Actor::Gate) {
            let tint = if powered && role == Some(Actor::Gate) {
                GATE_POWERED
            } else {
                source.tint()
            };
            world.entity_mut(entity).insert(
                Sprite::from_color(source.size, tint)
                    .with_layer(source.layer)
                    .with_sampler(SamplerDescriptor::NEAREST_CLAMP),
            );
        }
        if let Some(role) = role {
            decorate_actor(world, owner, entity, &source, pose, role);
        }
    }
    world.run_system_cached(sync_actors)?;
    *dressed = Some(owner.0);
    Ok(())
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

pub(super) fn fit_camera(
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<&mut Camera2d>,
) {
    let extent = &window.resolution;
    if extent.physical_width == 0 || extent.physical_height == 0 {
        return;
    }
    let aspect = extent.physical_width as f32 / extent.physical_height as f32;
    let arena_width = arena::WIDTH + 56.0;
    let height = ARENA_HEIGHT.max(arena_width / aspect);
    if camera.viewport_height != height {
        camera.viewport_height = height;
    }
}

pub(super) fn update_appearance(
    round: Res<Round>,
    animation: Res<PilotAnimation>,
    mut actors: Query<(
        &SceneActor,
        &SpriteSource,
        Option<&AnimatedPilot>,
        &mut Sprite,
    )>,
) {
    for (actor, source, animated, mut sprite) in &mut actors {
        match actor.role {
            Actor::Pilot if animated.is_some() => {
                let playback = if round.moving && round.status() == RoundStatus::Running {
                    round.elapsed
                } else {
                    Duration::ZERO
                };
                let region = Some(animation.clip.sample(playback).region);
                if sprite.texture_region != region {
                    sprite.texture_region = region;
                }
            }
            Actor::Gate => {
                let tint = if round.can_extract() {
                    GATE_POWERED
                } else {
                    source.tint()
                };
                if sprite.material.tint != tint {
                    sprite.material.tint = tint;
                }
            }
            _ => {}
        }
    }
}

fn make_pilot_texture() -> Result<ImageAsset, nara::render::image::ImageAssetCreateError> {
    const GLYPH: [&[u8; 12]; 12] = [
        b".....aa.....",
        b"....abba....",
        b"...abwwba...",
        b"...abccba...",
        b"..abccccba..",
        b".abccccccba.",
        b"abbbcwwcbbba",
        b"abbdcwwcdbba",
        b"aaaaddddaaaa",
        b"....aeea....",
        b".....ee.....",
        b".....ff.....",
    ];
    let mut pixels = Vec::with_capacity(24 * 12 * 4);
    for row in GLYPH {
        for frame in 0..2 {
            for &symbol in row {
                let rgba = match symbol {
                    b'a' => [26, 80, 105, 255],
                    b'b' => [171, 234, 250, 255],
                    b'c' => [44, 175, 202, 255],
                    b'd' => [35, 103, 140, 255],
                    b'e' if frame == 1 => [192, 245, 155, 255],
                    b'e' => [52, 139, 160, 255],
                    b'f' if frame == 1 => [89, 237, 198, 210],
                    b'w' => [233, 254, 255, 255],
                    _ => [0; 4],
                };
                pixels.extend_from_slice(&rgba);
            }
        }
    }
    ImageAsset::new(
        ImageExtent::new(24, 12),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        pixels,
    )
}
