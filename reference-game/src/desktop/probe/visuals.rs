use nara::{
    app::FrameExecutionStart,
    physics2d::TilemapColliders2d,
    prelude::*,
    render::{
        ExtractedViews,
        camera::CameraShake2d,
        material::AlphaMode2d,
        sprite_render::batching::{QueuedSpriteItems, SpriteBatchKind},
    },
    render_wgpu::{PostProcess2d, desktop::DesktopPresentation},
    scene::TilemapAsset,
};

use crate::desktop::{is_game_frame, world::ArenaEffects};

const CUES: [&str; 8] = [
    "courier thrust frame",
    "cell breathing",
    "gate tween",
    "courier tail",
    "pickup sparks",
    "camera follow",
    "camera shake",
    "sector fade",
];
const MIN_QUIET_FRAMES: u32 = 8;

struct VisualSample {
    cues: [bool; CUES.len()],
    bloom_with_particles: bool,
    merged_tiles: Option<(usize, usize)>,
}

#[derive(Resource, Default)]
pub(super) struct Evidence {
    pending: Option<(u64, VisualSample)>,
    seen: [bool; CUES.len()],
    pub(super) merged_tiles: Option<(usize, usize)>,
    pub(super) quiet_frames: u32,
}

impl Evidence {
    pub(super) fn complete(&self) -> bool {
        self.seen.iter().all(|&observed| observed)
            && self.merged_tiles.is_some()
            && self.quiet_frames >= MIN_QUIET_FRAMES
    }

    pub(super) fn unconfirmed(&self) -> Vec<&'static str> {
        let mut missing = CUES
            .into_iter()
            .zip(self.seen)
            .filter_map(|(cue, observed)| (!observed).then_some(cue))
            .collect::<Vec<_>>();
        if self.merged_tiles.is_none() {
            missing.push("rendered tilemap with merged collision");
        }
        if self.quiet_frames < MIN_QUIET_FRAMES {
            missing.push("steady frames with bloom, particles and zero WGPU resource creation");
        }
        missing
    }
}

pub(super) fn observe(
    (presentation, time, effects): (
        Res<DesktopPresentation>,
        Res<FrameExecutionStart>,
        Res<ArenaEffects>,
    ),
    (views, sprites, post): (
        Res<ExtractedViews>,
        Res<QueuedSpriteItems>,
        Res<PostProcess2d>,
    ),
    (animations, tweens, shakes): (
        Query<(&AnimationPlayer, &Transform2d)>,
        Query<&Tween2d>,
        Query<&CameraShake2d>,
    ),
    (tilesets, maps, compounds): (
        Res<Assets<TileSetAsset>>,
        Query<(&TilemapSource, &TilemapAsset, &TilemapColliders2d)>,
        Query<&CompoundCollider2d>,
    ),
    mut evidence: ResMut<Evidence>,
) {
    // Desktop presentation of an extracted frame is reported after Last has run.
    if let Some((frame_index, sample)) = evidence.pending.take()
        && let Some(frame) = presentation.latest().filter(is_game_frame)
        && frame.frame_index == frame_index
    {
        for (seen, observed) in evidence.seen.iter_mut().zip(sample.cues) {
            *seen |= observed;
        }
        evidence.merged_tiles = evidence.merged_tiles.or(sample.merged_tiles);
        if sample.bloom_with_particles
            && frame.work.created_texture_calls == 0
            && frame.work.created_buffer_calls == 0
        {
            evidence.quiet_frames += 1;
        }
    }

    let view = views
        .as_slice()
        .iter()
        .find(|view| Some(view.camera_entity) == effects.camera);
    let camera_follow = view.is_some_and(|view| view.mapping.clip_to_world(Vec2::ZERO).x > 30.0);
    let camera_shake = view.is_some_and(|view| {
        shakes
            .get(view.camera_entity)
            .is_ok_and(|shake| shake.trauma > 0.0)
            && view.mapping.clip_to_world_matrix().x_axis.y.abs() > 0.1
    });
    let mut courier = false;
    let mut breathing = false;
    let mut gate = false;
    let mut tail = false;
    let mut sparks = false;
    let mut tilemap = None;
    for sprite in sprites.as_slice() {
        if Some(sprite.entity) == effects.pilot {
            courier |= sprite.image.is_some() && sprite.instance.uv.min.x == 0.5;
        }
        if animations
            .get(sprite.entity)
            .is_ok_and(|(animation, pose)| {
                animation.is_playing() && animation.error().is_none() && pose.scale.x > 1.06
            })
        {
            breathing |= sprite.image.is_some();
        }
        if Some(sprite.entity) == effects.gate {
            gate |= sprite.instance.color.g > 0.8
                && tweens
                    .get(sprite.entity)
                    .is_ok_and(|tween| tween.is_finished() && tween.error().is_none());
        }
        if sprite.material.alpha_mode == AlphaMode2d::Additive {
            tail |= Some(sprite.entity) == effects.tail;
            sparks |= effects.cells.contains(&Some(sprite.entity));
        }
        if sprite.image.is_some() && sprite.kind == SpriteBatchKind::Tilemap {
            tilemap = Some(sprite.entity);
        }
    }
    let merged_tiles = evidence
        .merged_tiles
        .is_none()
        .then(|| {
            let (source, asset, colliders) = maps.get(tilemap?).ok()?;
            let (tileset, _) = asset.ready()?;
            let tileset = tilesets.get(tileset)?;
            let solid = source
                .cells
                .0
                .iter()
                .filter(|&&cell| {
                    cell.checked_sub(1)
                        .and_then(|index| tileset.tiles.get(usize::from(index)))
                        .is_some_and(|tile| tile.solid)
                })
                .count();
            let rectangles = colliders
                .entities()
                .iter()
                .filter_map(|&entity| compounds.get(entity).ok())
                .map(|shape| shape.boxes().len())
                .sum::<usize>();
            (rectangles > 0 && rectangles * 4 <= solid).then_some((rectangles, solid))
        })
        .flatten();
    let fade = post.screen_fade.is_some_and(|fade| fade.alpha > 0.0);
    evidence.pending = Some((
        time.frame(),
        VisualSample {
            cues: [
                courier,
                breathing,
                gate,
                tail,
                sparks,
                camera_follow,
                camera_shake,
                fade,
            ],
            bloom_with_particles: (tail || sparks)
                && post.bloom.is_some_and(|bloom| bloom.intensity > 0.0),
            merged_tiles,
        },
    ));
}
