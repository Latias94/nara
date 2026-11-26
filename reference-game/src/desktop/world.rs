use std::{f32::consts::FRAC_PI_4, time::Duration};

use nara::{
    asset::add_asset,
    ecs::{
        Single,
        bevy_ecs::{error::Result as SystemResult, query::With},
    },
    prelude::*,
    render::{
        animation::{Clip, Frame, PlaybackMode},
        material::SamplerDescriptor,
    },
    window::{PrimaryWindow, Window},
};

use crate::{Round, RoundStatus, arena};

const BACKGROUND: Color = Color::rgb(0.009, 0.015, 0.026);
const CELL_COLOR: Color = Color::rgb(0.99, 0.57, 0.10);
const DRONE_COLOR: Color = Color::rgb(0.82, 0.11, 0.19);
const GATE_LOCKED: Color = Color::rgb(0.20, 0.30, 0.42);
const GATE_POWERED: Color = Color::rgb(0.12, 0.90, 0.60);
const ARENA_HEIGHT: f32 = 484.0;

#[derive(Component, Clone, Copy)]
pub(super) enum Actor {
    Pilot,
    Cell(usize),
    Drone(usize),
    Gate,
}

#[derive(Resource)]
pub(super) struct PilotAnimation(Clip);

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
    spawn_board(world);
    spawn_markers(world);
    let texture = add_asset(world, make_pilot_texture()?)?;
    let size = Vec2::new(0.5, 1.0);
    let animation = Clip::new(
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
    world.spawn((
        Actor::Pilot,
        Transform2d::from_translation(arena::START),
        Visibility::Inherited,
        Sprite::from_texture(texture, Vec2::splat(32.0))
            .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
            .with_texture_region(animation.sample(Duration::ZERO).region)
            .with_layer(5),
    ));
    world.insert_resource(PilotAnimation(animation));
    Ok(())
}

fn spawn_board(world: &mut World) {
    let board_size = Vec2::new(arena::WIDTH as f32, arena::HEIGHT as f32) * arena::TILE;
    quad(
        world,
        Vec2::ZERO,
        board_size + Vec2::splat(7.0),
        Color::rgb(0.052, 0.24, 0.26),
        -11,
    );
    let mut tiles = Tilemap::new(Vec2::splat(arena::TILE)).with_layer(-10);
    for (column, row, wall) in arena::tiles() {
        let alternate = (column + row) % 2 == 0;
        let position = arena::at(column, row);
        let color = match (wall, alternate) {
            (true, _) => Color::rgb(0.042, 0.096, 0.125),
            (false, true) => Color::rgb(0.020, 0.039, 0.061),
            (false, false) => Color::rgb(0.024, 0.047, 0.070),
        };
        tiles.set_cell(
            TileCoord::new(
                column as i32 - (arena::WIDTH / 2) as i32,
                (arena::HEIGHT / 2) as i32 - row as i32,
            ),
            TileCell::new(TileIndex::new(1)).with_color(color),
        );
        if wall {
            quad(
                world,
                position,
                Vec2::splat(28.0),
                Color::rgb(0.087, 0.156, 0.19),
                -9,
            );
            quad(
                world,
                position + Vec2::Y * 10.0,
                Vec2::new(23.0, 2.0),
                Color::rgb(0.15, 0.29, 0.31),
                -8,
            );
        } else if column % 3 == 0 && row % 2 == 0 {
            quad(
                world,
                position + Vec2::new(10.0, -10.0),
                Vec2::splat(2.5),
                Color::rgb(0.07, 0.15, 0.18),
                -8,
            );
        }
    }
    world.spawn((
        tiles,
        Transform2d::from_translation(Vec2::splat(-arena::TILE * 0.5)),
    ));
}

fn spawn_markers(world: &mut World) {
    quad(
        world,
        arena::START,
        Vec2::splat(26.0),
        Color::rgb(0.035, 0.18, 0.20),
        -6,
    );
    for (index, position) in arena::CELLS.into_iter().enumerate() {
        quad(
            world,
            position,
            Vec2::splat(26.0),
            Color::rgb(0.16, 0.115, 0.035),
            -6,
        );
        let cell = quad(world, position, Vec2::splat(15.0), CELL_COLOR, 2);
        world.entity_mut(cell).insert(Actor::Cell(index));
        detail(
            world,
            cell,
            Vec2::splat(6.0),
            Color::rgb(1.0, 0.93, 0.65),
            3,
        );
    }
    for (index, patrol) in arena::PATROLS.into_iter().enumerate() {
        let track = (patrol.end - patrol.start).abs() + Vec2::splat(2.0);
        quad(
            world,
            patrol.start.midpoint(patrol.end),
            track,
            Color::rgb(0.22, 0.06, 0.10),
            -5,
        );
        let drone = quad(
            world,
            patrol.position(0.0),
            Vec2::splat(arena::DRONE_RADIUS * 1.65),
            DRONE_COLOR,
            2,
        );
        world.entity_mut(drone).insert(Actor::Drone(index));
        detail(
            world,
            drone,
            Vec2::new(10.0, 4.0),
            Color::rgb(1.0, 0.68, 0.52),
            3,
        );
    }
    let gate = quad(world, arena::GATE, Vec2::splat(28.0), GATE_LOCKED, 1);
    world.entity_mut(gate).insert(Actor::Gate);
    detail(world, gate, Vec2::splat(20.0), BACKGROUND, 2);
    detail(
        world,
        gate,
        Vec2::new(5.0, 15.0),
        Color::rgb(0.09, 0.47, 0.38),
        3,
    );
}

fn quad(world: &mut World, position: Vec2, size: Vec2, color: Color, layer: i32) -> Entity {
    world
        .spawn((
            Transform2d::from_translation(position),
            Visibility::Inherited,
            Sprite::from_color(size, color).with_layer(layer),
        ))
        .id()
}

fn detail(world: &mut World, parent: Entity, size: Vec2, color: Color, layer: i32) {
    world.spawn((
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
    let arena_width = arena::WIDTH as f32 * arena::TILE + 56.0;
    let height = ARENA_HEIGHT.max(arena_width / aspect);
    if camera.viewport_height != height {
        camera.viewport_height = height;
    }
}

pub(super) fn update(
    round: Res<Round>,
    animation: Res<PilotAnimation>,
    mut actors: Query<(&Actor, &mut Transform2d, &mut Visibility, &mut Sprite)>,
) {
    let age = round.elapsed.as_secs_f32();
    for (actor, mut transform, mut visibility, mut sprite) in &mut actors {
        let mut pose = *transform;
        let mut shown = Visibility::Inherited;
        match *actor {
            Actor::Pilot => {
                pose.translation = round.position();
                pose.rotation = round.heading;
                let playback = if round.moving && round.status() == RoundStatus::Running {
                    round.elapsed
                } else {
                    Duration::ZERO
                };
                let region = Some(animation.0.sample(playback).region);
                if sprite.texture_region != region {
                    sprite.texture_region = region;
                }
                if round.status() == RoundStatus::Failed(crate::Failure::ShieldDepleted)
                    || !round.cooldown.is_zero() && (age * 14.0) as u32 % 2 == 0
                {
                    shown = Visibility::Hidden;
                }
            }
            Actor::Cell(index) => {
                pose.rotation = FRAC_PI_4 + 0.15 * (age * 4.0 + index as f32).sin();
                if round.cells[index] {
                    shown = Visibility::Hidden;
                }
            }
            Actor::Drone(index) => pose.translation = arena::PATROLS[index].position(age),
            Actor::Gate => {
                let tint = if round.can_extract() {
                    GATE_POWERED
                } else {
                    GATE_LOCKED
                };
                if sprite.material.tint != tint {
                    sprite.material.tint = tint;
                }
            }
        }
        if *transform != pose {
            *transform = pose;
        }
        if *visibility != shown {
            *visibility = shown;
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
