//! Expectations for the two shipped Scenes; this is not a general rasterizer or schema reader.

use nara::{
    asset::{AssetRecord, AssetSourceKind, AssetSourcePath, StableAssetId},
    image::{
        ImageAsset, ImageBytesImportRequest, ImageColorSpace, ImageFormat, ImageImportSettings,
        ImageImporter,
    },
    material::{AlphaMode2d, SamplerDescriptor},
    prelude::{Color, Transform2d, Vec2},
    render::{RenderFrameTarget, ViewLoadOp, ViewportRect},
    render_wgpu::WgpuStockPolicy,
    scene::{SourceDocumentKind, SourceDocumentOrigin},
};
use two_level_game::{
    GamePhase, GameSnapshot,
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId},
    resume::ResumeState,
};

use super::{CANVAS_EXTENT, Result, SealedGameFrame, WINDOW_EXTENT, require};

const PLAYER_FILL: [[u8; 4]; 4] = [
    [64, 160, 240, 255],
    [32, 216, 232, 255],
    [240, 152, 48, 255],
    [112, 224, 80, 255],
];
const PLAYER_BORDER: [u8; 4] = [16, 32, 48, 255];

pub(super) fn assert_snapshot(snapshot: &GameSnapshot, expected: &ResumeState) -> Result<()> {
    expected.validate()?;
    let key = expected.progress.location.level;
    let mut markers: Vec<_> = expected
        .progress
        .markers
        .iter()
        .filter(|marker| marker.level == key)
        .map(|marker| marker.id)
        .collect();
    markers.sort();
    let collected_here = expected
        .progress
        .collected
        .iter()
        .filter(|pickup| pickup.level == key)
        .count();
    require(
        collected_here <= 2,
        "the shipped Scene has exactly two pickups",
    )?;
    require(
        snapshot.initialized
            && snapshot.phase == GamePhase::Running
            && !snapshot.restoring
            && snapshot.error.is_none()
            && snapshot.level == Some(key)
            && snapshot.player.to_array() == expected.player.translation
            && snapshot.score == expected.progress.score
            && snapshot.collected == expected.progress.collected.len()
            && snapshot.visible_pickups == 2 - collected_here
            && snapshot.markers == markers
            && snapshot.held == expected.held
            && snapshot.logical_tick == expected.clock.ticks
            && snapshot.logical_elapsed_nanos == expected.clock.elapsed_nanos
            && snapshot.travel_count == expected.counters.travel_count
            && snapshot.restarts == expected.counters.restarts
            && snapshot.saves == expected.counters.saves
            && snapshot.restores == expected.counters.restores
            && snapshot.scene_members == AUTHORED_LEVEL_MEMBERS
            && snapshot.current_instance.is_some(),
        format!("presented GameSnapshot differs from expected resume semantics: {snapshot:?}"),
    )
}

pub(super) fn assert_frame(frame: &SealedGameFrame, expected: &ResumeState) -> Result<()> {
    assert_snapshot(&frame.snapshot, expected)?;
    let packet = frame.input.packet();
    let key = expected.progress.location.level;
    let source_path = match key.level() {
        LevelId::Meadow => "scenes/meadow.scene.json",
        LevelId::Cavern => "scenes/cavern.scene.json",
    };
    require(
        frame.source.kind() == SourceDocumentKind::Scene
            && matches!(frame.source.origin(), SourceDocumentOrigin::ProjectLocation { path, .. }
                if path.as_str() == source_path),
        "captured CurrentScene is not the expected game's admitted level source",
    )?;
    let stamp = expected
        .compatibility
        .levels
        .iter()
        .find(|stamp| stamp.level == key.level())
        .ok_or_else(|| std::io::Error::other("expected resume lacks its active level stamp"))?;
    require(
        frame.stamp.schema_fingerprint().as_bytes() == expected.compatibility.schema
            && *frame.stamp.manifest_digest().as_bytes() == stamp.manifest
            && *frame.stamp.content_digest().as_bytes() == stamp.content,
        "original CurrentScene Schema/manifest/runtime-content stamp differs from expected resume",
    )?;
    require(
        packet.source_generation().runtime_generation() == Some(frame.runtime)
            && frame.snapshot.current_instance == Some(frame.instance.instance_id())
            && frame.timestep_nanos == expected.clock.timestep_nanos
            && frame.runtime_time.to_continuation()? == expected.runtime_time.to_continuation()?
            && frame.instance.entity_ids().len() == AUTHORED_LEVEL_MEMBERS
            && packet.target().extent() == WINDOW_EXTENT
            && packet.views().len() == 1,
        "original source generation, Scene membership, window extent or View count differs",
    )?;
    let RenderFrameTarget::Window(window) = packet.target() else {
        return Err(
            std::io::Error::other("original game input is not a window transaction").into(),
        );
    };
    require(
        window.resolution.scale_factor.is_finite() && window.resolution.scale_factor > 0.0,
        "captured native scale factor must be finite and positive",
    )?;
    let canvas = packet.world_canvas().ok_or_else(|| {
        std::io::Error::other("the shipped desktop profile requires a world canvas")
    })?;
    let mapping = canvas.mapping();
    let physical_min = mapping
        .content_rect()
        .min
        .map(|x| x * mapping.pixels_per_point());
    let physical_max = mapping
        .content_rect()
        .max
        .map(|x| x * mapping.pixels_per_point());
    require(
        canvas.extent() == CANVAS_EXTENT
            && mapping.target() == CANVAS_EXTENT
            && mapping.pixels_per_point() == window.resolution.scale_factor as f32
            && physical_min.iter().all(|value| value.abs() < 0.001)
            && (physical_max[0] - 960.0).abs() < 0.001
            && (physical_max[1] - 540.0).abs() < 0.001,
        "captured 320x180 canvas must map to the actual 960x540 window at its observed DPI",
    )?;
    let view = &packet.views()[0];
    require(
        view.target.window_id(Some(window.id)) == Some(window.id)
            && view.viewport == ViewportRect::from_extent(CANVAS_EXTENT).unwrap()
            && view
                .world_to_viewport(Vec2::ZERO)
                .abs_diff_eq(Vec2::new(160.0, 90.0), 0.0001)
            && view
                .world_to_viewport(Vec2::new(320.0, 180.0))
                .abs_diff_eq(Vec2::new(320.0, 0.0), 0.0001)
            && view.load == ViewLoadOp::Clear(background(key.level())),
        "captured View differs from the shipped centered camera, projection or level clear",
    )?;
    let stock = frame
        .input
        .captured::<WgpuStockPolicy>()
        .ok_or_else(|| std::io::Error::other("original capture is not the stock policy"))?;
    let batches = stock
        .sprite_batches()
        .ok_or_else(|| std::io::Error::other("original stock input lacks Sprite extraction"))?;
    let objects = objects(expected)?;
    let player_atlas = player_fixture()?;
    let marker_count = expected
        .progress
        .markers
        .iter()
        .filter(|marker| marker.level == key)
        .count();
    require(
        batches.total_instances() == objects.len()
            && objects.len() == AUTHORED_LEVEL_MEMBERS - 1 + marker_count * 2,
        "stock capture has missing or extra game objects",
    )?;
    let mut originals = batches.as_slice().iter().flat_map(|batch| {
        batch
            .instances
            .iter()
            .map(move |instance| (batch, instance))
    });
    for object in &objects {
        let (batch, instance) = originals.next().unwrap();
        let (sampler, uv_min, uv_size) = object.image.sprite_sampling();
        let matrix = object.transform.matrix();
        let center = view.mapping.world_to_clip(object.transform.translation);
        let x = view
            .mapping
            .world_vector_to_clip(matrix.transform_vector2(Vec2::new(object.size.x * 0.5, 0.0)));
        let y = view
            .mapping
            .world_vector_to_clip(matrix.transform_vector2(Vec2::new(0.0, object.size.y * 0.5)));
        require(
            batch.view_index == 0
                && batch.view_order == view.order
                && batch.target == view.target
                && batch.layer == 0
                && batch.sort_key == 0
                && batch.material_extension.is_none()
                && batch.material.alpha_mode == AlphaMode2d::Blend
                && batch.material.sampler == sampler
                && instance.center.abs_diff_eq(center, 0.00001)
                && instance.x_axis.abs_diff_eq(x, 0.00001)
                && instance.y_axis.abs_diff_eq(y, 0.00001)
                && instance.color == object.color
                && instance.uv.min == uv_min
                && instance.uv.size == uv_size
                && batch.image.is_some() == (object.image != GameImage::Solid),
            format!(
                "original ordered stock Sprite differs from expected {}",
                object.name
            ),
        )?;
        if let Some(image) = &batch.image {
            let image = image.resource();
            let (width, height) = match object.image {
                GameImage::PlayerAtlas(_) => (64, 16),
                GameImage::Pickup => (16, 16),
                GameImage::MarkerBadge => (2, 2),
                GameImage::Solid => unreachable!(),
            };
            require(
                image.extent().width == width
                    && image.extent().height == height
                    && image.format() == ImageFormat::Rgba8
                    && image.color_space() == ImageColorSpace::Srgb
                    && image.pixels().len() == (width * height * 4) as usize
                    && image.has_retention_owner(),
                format!(
                    "captured {} must retain its original RGBA8 sRGB Image and allocation owner",
                    object.name
                ),
            )?;
            match object.image {
                GameImage::PlayerAtlas(_) => require(
                    image.pixels() == player_atlas.pixels(),
                    "captured player atlas differs from the independently decoded original fixture",
                )?,
                GameImage::Pickup => require(
                    image.pixels()[(3 * 16 + 7) * 4..(3 * 16 + 7) * 4 + 4] == [255; 4]
                        && image.pixels()[(11 * 16 + 9) * 4..(11 * 16 + 9) * 4 + 4]
                            == [185, 185, 185, 255],
                    "captured pickup must retain the real sRGB diamond fixture pixels",
                )?,
                GameImage::MarkerBadge => require(
                    image.pixels() == MARKER_BADGE_TEXEL.repeat(4),
                    "captured marker must retain its authored sRGB texture pixels",
                )?,
                GameImage::Solid => unreachable!(),
            }
        }
    }
    Ok(())
}

struct GameObject {
    name: String,
    kind: GameObjectKind,
    transform: Transform2d,
    size: Vec2,
    color: Color,
    image: GameImage,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GameObjectKind {
    Player,
    Pickup,
    Wall,
    Marker,
    OtherSceneObject,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GameImage {
    Solid,
    Pickup,
    PlayerAtlas(usize),
    MarkerBadge,
}

const MARKER_BADGE_TEXEL: [u8; 4] = [255, 192, 64, 255];

impl GameImage {
    fn sprite_sampling(self) -> (SamplerDescriptor, Vec2, Vec2) {
        match self {
            Self::PlayerAtlas(frame) => (
                SamplerDescriptor::NEAREST_CLAMP,
                Vec2::new(frame as f32 / 4.0, 0.0),
                Vec2::new(0.25, 1.0),
            ),
            Self::MarkerBadge => (SamplerDescriptor::NEAREST_CLAMP, Vec2::ZERO, Vec2::ONE),
            Self::Solid | Self::Pickup => (SamplerDescriptor::LINEAR_CLAMP, Vec2::ZERO, Vec2::ONE),
        }
    }
}

fn player_frame(expected: &ResumeState) -> usize {
    let held = expected.held;
    if held[0] == held[1] && held[2] == held[3] {
        0
    } else {
        1 + ((expected.clock.elapsed_nanos / 100_000_000) % 3) as usize
    }
}

fn player_fixture() -> Result<ImageAsset> {
    let record = AssetRecord::new(
        StableAssetId::parse_str("2eece9cd-6332-4868-b0d2-289879df9de8")?,
        AssetSourcePath::new("textures/explorer.png")?,
        AssetSourceKind::Image,
    );
    let image = ImageImporter::new()?
        .preflight_unpublished_import(ImageBytesImportRequest::new(
            record,
            include_bytes!("../../../assets/textures/explorer.png").as_slice(),
            ImageImportSettings::new(ImageColorSpace::Srgb),
        ))?
        .import()?;
    require(
        image.extent().width == 64
            && image.extent().height == 16
            && image.format() == ImageFormat::Rgba8
            && image.color_space() == ImageColorSpace::Srgb
            && image.pixels().len() == 64 * 16 * 4,
        "original player fixture must be a 64x16 RGBA8 sRGB atlas",
    )?;
    assert_player_fixture_pixels(image.pixels())?;
    Ok(image)
}

fn assert_player_fixture_pixels(pixels: &[u8]) -> Result<()> {
    require(
        pixels.len() == 64 * 16 * 4,
        "wrong player fixture byte length",
    )?;
    for (index, actual) in pixels.chunks_exact(4).enumerate() {
        let x = index % 64;
        let y = index / 64;
        let local_x = x % 16;
        let expected = if local_x == 0 || local_x == 15 || y == 0 || y == 15 {
            PLAYER_BORDER
        } else if (local_x == 5 || local_x == 10) && y == 5 {
            [255; 4]
        } else {
            PLAYER_FILL[x / 16]
        };
        require(
            actual == expected,
            format!(
                "original player fixture texel ({x}, {y}): expected {expected:?}, got {actual:?}"
            ),
        )?;
    }
    Ok(())
}

fn player_texel(image: &ImageAsset, frame: usize, local_uv: Vec2) -> [u8; 4] {
    let x = (((frame as f32 + local_uv.x) / 4.0 * 64.0).floor() as usize).min(63);
    let y = ((local_uv.y * 16.0).floor() as usize).min(15);
    let offset = (y * 64 + x) * 4;
    image.pixels()[offset..offset + 4].try_into().unwrap()
}

fn srgb_channel(byte: u8) -> f32 {
    let encoded = f32::from(byte) / 255.0;
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn texel_color(texel: [u8; 4]) -> Color {
    Color::rgba(
        srgb_channel(texel[0]),
        srgb_channel(texel[1]),
        srgb_channel(texel[2]),
        f32::from(texel[3]) / 255.0,
    )
}

fn objects(expected: &ResumeState) -> Result<Vec<GameObject>> {
    expected.validate()?;
    require(
        expected.progress.location.anchor.as_str() == "checkpoint",
        "unexpected game checkpoint anchor",
    )?;
    let key = expected.progress.location.level;
    for pickup in &expected.progress.collected {
        require(
            matches!(pickup.entity.as_str(), "gem-near" | "gem-far"),
            "unknown game pickup in complete progress",
        )?;
    }
    let object = |name: &str, kind, position, size, color, image| GameObject {
        name: name.into(),
        kind,
        transform: Transform2d::from_translation(position),
        size,
        color,
        image,
    };
    let mut objects = vec![
        GameObject {
            name: "player".into(),
            kind: GameObjectKind::Player,
            transform: Transform2d {
                translation: Vec2::from_array(expected.player.translation),
                rotation: expected.player.rotation,
                scale: Vec2::from_array(expected.player.scale),
            },
            size: Vec2::splat(18.0),
            color: Color::WHITE,
            image: GameImage::PlayerAtlas(player_frame(expected)),
        },
        object(
            "checkpoint",
            GameObjectKind::OtherSceneObject,
            Vec2::new(-120.0, 0.0),
            Vec2::splat(14.0),
            Color::rgb(0.25, 1.0, 0.5),
            GameImage::Solid,
        ),
    ];
    let determinant = objects[0].transform.matrix().determinant();
    require(
        determinant.is_finite() && determinant != 0.0,
        "player transform must be invertible for a pixel witness",
    )?;
    for (name, position) in [
        ("gem-near", Vec2::new(-60.0, 0.0)),
        ("gem-far", Vec2::new(80.0, 40.0)),
    ] {
        let collected = expected
            .progress
            .collected
            .iter()
            .any(|pickup| pickup.level == key && pickup.entity.as_str() == name);
        objects.push(object(
            name,
            GameObjectKind::Pickup,
            position,
            Vec2::splat(18.0),
            Color::rgba(1.0, 0.8, 0.2, if collected { 0.0 } else { 1.0 }),
            GameImage::Pickup,
        ));
    }
    objects.push(object(
        "portal",
        GameObjectKind::OtherSceneObject,
        Vec2::new(200.0, 0.0),
        Vec2::splat(26.0),
        Color::rgb(0.8, 0.3, 1.0),
        GameImage::Solid,
    ));
    let inner_x = match key.level() {
        LevelId::Meadow => 0.0,
        LevelId::Cavern => 20.0,
    };
    for (name, position, size) in [
        ("wall-west", Vec2::new(-294.0, 0.0), Vec2::new(10.0, 328.0)),
        ("wall-east", Vec2::new(294.0, 0.0), Vec2::new(10.0, 328.0)),
        ("wall-north", Vec2::new(0.0, 164.0), Vec2::new(598.0, 10.0)),
        ("wall-south", Vec2::new(0.0, -164.0), Vec2::new(598.0, 10.0)),
        (
            "wall-inner",
            Vec2::new(inner_x, -12.0),
            Vec2::new(12.0, 72.0),
        ),
    ] {
        objects.push(object(
            name,
            GameObjectKind::Wall,
            position,
            size,
            Color::rgb(0.18, 0.22, 0.28),
            GameImage::Solid,
        ));
    }
    let mut markers: Vec<_> = expected
        .progress
        .markers
        .iter()
        .filter(|marker| marker.level == key)
        .collect();
    markers.sort_by_key(|marker| marker.id);
    for marker in markers {
        let position = Vec2::from_array(marker.position);
        require(
            position.x.abs() <= 280.0 && position.y.abs() <= 150.0,
            "marker is outside the game's playable bounds",
        )?;
        objects.push(object(
            &format!("marker {}", marker.id.get()),
            GameObjectKind::Marker,
            position,
            Vec2::splat(10.0),
            Color::rgb(1.0, 0.3, 0.8),
            GameImage::Solid,
        ));
        objects.push(object(
            &format!("marker {} child", marker.id.get()),
            GameObjectKind::Marker,
            position + Vec2::splat(3.0),
            Vec2::splat(4.0),
            Color::WHITE,
            GameImage::MarkerBadge,
        ));
    }
    Ok(objects)
}

fn background(level: LevelId) -> Color {
    match level {
        LevelId::Meadow => Color::rgb(0.04, 0.13, 0.09),
        LevelId::Cavern => Color::rgb(0.07, 0.05, 0.15),
    }
}

/// Counts physical RGBA pixels checked, not queue submissions or display pixels observed.
#[derive(Debug, Default)]
pub struct PixelChecks {
    pub player: usize,
    pub markers: usize,
    pub walls: usize,
    pub visible_pickups: usize,
    pub collected_pickups: usize,
    pub background: usize,
    pub other_scene_objects: usize,
}

impl GameObject {
    fn local(&self, world: Vec2) -> Vec2 {
        self.transform.matrix().inverse().transform_point2(world)
    }

    fn may_cover(&self, world: Vec2) -> bool {
        let local = self.local(world);
        self.color.a > 0.0
            && local.is_finite()
            && local.abs().cmple(self.size * 0.5 + Vec2::splat(2.0)).all()
    }
}

struct PlayerPixelWitness {
    x: usize,
    y: usize,
    texel: [u8; 4],
}

pub(super) fn check_pixels(rgba8: &[u8], expected: &ResumeState) -> Result<PixelChecks> {
    require(
        rgba8.len() == 960 * 540 * 4,
        "unexpected replay readback length",
    )?;
    let objects = objects(expected)?;
    let player_atlas = player_fixture()?;
    let mut checks = PixelChecks::default();
    let clear = background(expected.progress.location.level.level());
    for (index, object) in objects.iter().enumerate() {
        let mut found = 0;
        let mut player_witnesses: Vec<PlayerPixelWitness> = Vec::with_capacity(4);
        let mut player_fill_seen = false;
        for y in 0..180 {
            for x in 0..320 {
                let world = Vec2::new(
                    (x as f32 + 0.5 - 160.0) * 2.0,
                    (90.0 - y as f32 - 0.5) * 2.0,
                );
                let local = object.local(world);
                if !local.is_finite() || local.abs().cmpgt(object.size * 0.5 - Vec2::ONE).any() {
                    continue;
                }
                let occluded = if object.color.a == 0.0 {
                    objects.iter().any(|other| other.may_cover(world))
                } else {
                    objects[index + 1..]
                        .iter()
                        .any(|other| other.may_cover(world))
                };
                if occluded {
                    continue;
                }
                let mut color = object.color;
                let uv = Vec2::new(local.x / object.size.x + 0.5, 0.5 - local.y / object.size.y);
                if let GameImage::PlayerAtlas(frame) = object.image {
                    let texel = player_texel(&player_atlas, frame, uv);
                    let witness = PlayerPixelWitness { x, y, texel };
                    if texel == PLAYER_FILL[frame] {
                        player_fill_seen = true;
                        if player_witnesses.len() < 4 {
                            player_witnesses.push(witness);
                        }
                    } else if !player_witnesses.iter().any(|sample| sample.texel == texel) {
                        if player_witnesses.len() == 4 {
                            player_witnesses.pop();
                        }
                        player_witnesses.insert(0, witness);
                    }
                    continue;
                }
                if object.image == GameImage::Pickup {
                    let white = (0.40..0.49).contains(&uv.x) && (0.28..0.38).contains(&uv.y);
                    let shadow = (0.51..0.60).contains(&uv.x) && (0.62..0.72).contains(&uv.y);
                    if !white && !shadow {
                        continue;
                    }
                    if shadow {
                        let linear = srgb_channel(185);
                        color.r *= linear;
                        color.g *= linear;
                        color.b *= linear;
                    }
                }
                if object.image == GameImage::MarkerBadge {
                    color = texel_color(MARKER_BADGE_TEXEL);
                }
                if color.a == 0.0 {
                    color = clear;
                }
                check_block(rgba8, x, y, color, &object.name)?;
                found += 9;
                if found >= 36 {
                    break;
                }
            }
            if found >= 36 {
                break;
            }
        }
        if let GameImage::PlayerAtlas(frame) = object.image {
            require(
                player_fill_seen,
                "player has no unoccluded frame-specific atlas pixel witness; choose a discriminating game state",
            )?;
            for witness in player_witnesses {
                check_block(
                    rgba8,
                    witness.x,
                    witness.y,
                    texel_color(witness.texel),
                    &format!("player atlas frame {frame}"),
                )?;
                found += 9;
            }
        }
        if object.kind != GameObjectKind::OtherSceneObject {
            require(
                found > 0,
                format!(
                    "{} has no unoccluded interior pixel witness; choose a discriminating game state",
                    object.name
                ),
            )?;
        }
        match object.kind {
            GameObjectKind::Player => checks.player += found,
            GameObjectKind::Pickup if object.color.a == 0.0 => checks.collected_pickups += found,
            GameObjectKind::Pickup => checks.visible_pickups += found,
            GameObjectKind::Wall => checks.walls += found,
            GameObjectKind::Marker => checks.markers += found,
            GameObjectKind::OtherSceneObject => checks.other_scene_objects += found,
        }
    }
    for (x, y) in [(4, 4), (315, 4), (4, 175), (315, 175)] {
        check_block(rgba8, x, y, clear, "level background")?;
        checks.background += 9;
    }
    Ok(checks)
}

fn check_block(rgba8: &[u8], x: usize, y: usize, expected: Color, label: &str) -> Result<()> {
    let channels =
        [expected.r, expected.g, expected.b, expected.a].map(|x| (x * 255.0).round() as u8);
    for py in y * 3..y * 3 + 3 {
        for px in x * 3..x * 3 + 3 {
            let offset = (py * 960 + px) * 4;
            let actual = &rgba8[offset..offset + 4];
            require(
                actual
                    .iter()
                    .zip(channels)
                    .take(3)
                    .all(|(a, b)| a.abs_diff(b) <= 3)
                    && actual[3] == channels[3],
                format!(
                    "replay {label} pixel ({px}, {py}): expected {channels:?} +/-3 RGB and exact alpha, got {actual:?}"
                ),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nara::{asset::ImportedProductId, identity::SceneEntityId};
    use two_level_game::{
        levels::LevelKey,
        resume::{
            GameClock, LevelContentStamp, PlayerState, ResumeCompatibility, RuntimeCatchUpPolicy,
            RuntimeTimeState, SessionCounters,
        },
        save::{Checkpoint, CheckpointLocation, CollectedPickup, MarkerId, MarkerRecord},
    };

    use super::*;

    const LINEAR_FILL: [[u8; 4]; 4] = [
        [13, 90, 222, 255],
        [4, 175, 206, 255],
        [222, 80, 8, 255],
        [41, 190, 20, 255],
    ];
    const PLAYER_FILL_BLOCKS: [(usize, usize); 3] = [(156, 116), (157, 116), (158, 116)];
    const PLAYER_EYE_BLOCK: (usize, usize) = (158, 118);
    const MARKER_ROOT_BLOCKS: [(usize, usize); 4] =
        [(178, 118), (178, 119), (178, 120), (178, 121)];
    const MARKER_CHILD_BLOCK: (usize, usize) = (181, 118);
    const WALL_BLOCKS: [(&str, [(usize, usize); 4]); 5] = [
        ("wall-west", [(11, 12), (12, 12), (13, 12), (14, 12)]),
        ("wall-east", [(305, 12), (306, 12), (307, 12), (308, 12)]),
        ("wall-north", [(11, 6), (12, 6), (13, 6), (14, 6)]),
        ("wall-south", [(11, 170), (12, 170), (13, 170), (14, 170)]),
        ("wall-inner", [(157, 78), (158, 78), (159, 78), (160, 78)]),
    ];

    fn resume(frame: usize) -> ResumeState {
        let level = LevelKey::main(LevelId::Meadow);
        let mut progress = Checkpoint::new(CheckpointLocation {
            level,
            anchor: SceneEntityId::new("checkpoint").unwrap(),
        });
        progress.score = 10;
        progress.collected.push(CollectedPickup {
            level,
            entity: SceneEntityId::new("gem-near").unwrap(),
        });
        progress.markers.push(MarkerRecord {
            id: MarkerId::new(1).unwrap(),
            level,
            position: [40.0, -60.0],
        });
        progress.next_marker_id = MarkerId::new(2).unwrap();
        let ticks = frame.saturating_sub(1) as u64;
        let state = ResumeState {
            progress,
            player: PlayerState {
                translation: [0.0, -60.0],
                rotation: 0.0,
                scale: [1.0; 2],
            },
            held: [false, frame != 0, false, false],
            clock: GameClock {
                ticks,
                elapsed_nanos: ticks * 100_000_000,
                timestep_nanos: 100_000_000,
            },
            runtime_time: RuntimeTimeState {
                timestep_nanos: 100_000_000,
                max_steps_per_frame: 8,
                max_debt_steps: 8,
                catch_up_policy: RuntimeCatchUpPolicy::PreserveDebt,
                time_scale: 1.0,
                max_delta_nanos: 250_000_000,
                pending_nanos: 0,
                scaled_remainder: [0; 3],
            },
            counters: SessionCounters::default(),
            compatibility: ResumeCompatibility {
                schema: [1; 32],
                configuration: [2; 32],
                levels: vec![LevelContentStamp {
                    level: LevelId::Meadow,
                    manifest: [3; 32],
                    content: [4; 32],
                }],
            },
        };
        state.validate().unwrap();
        state
    }

    fn paint_block(pixels: &mut [u8], (x, y): (usize, usize), color: [u8; 4]) {
        for py in y * 3..y * 3 + 3 {
            for px in x * 3..x * 3 + 3 {
                let offset = (py * 960 + px) * 4;
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }

    fn readback(frame: usize) -> Vec<u8> {
        let mut pixels = vec![0; 960 * 540 * 4];
        for block in PLAYER_FILL_BLOCKS {
            paint_block(&mut pixels, block, LINEAR_FILL[frame]);
        }
        paint_block(&mut pixels, PLAYER_EYE_BLOCK, [255; 4]);
        for x in 97..101 {
            paint_block(&mut pixels, (x, 87), [64, 255, 128, 255]);
        }
        for block in [(129, 88), (130, 91)] {
            paint_block(&mut pixels, block, [10, 33, 23, 255]);
        }
        paint_block(&mut pixels, (199, 68), [255, 204, 51, 255]);
        paint_block(&mut pixels, (200, 71), [124, 99, 25, 255]);
        for x in 254..258 {
            paint_block(&mut pixels, (x, 84), [204, 77, 255, 255]);
        }
        for (_, blocks) in WALL_BLOCKS {
            for block in blocks {
                paint_block(&mut pixels, block, [46, 56, 71, 255]);
            }
        }
        for block in MARKER_ROOT_BLOCKS {
            paint_block(&mut pixels, block, [255, 77, 204, 255]);
        }
        paint_block(&mut pixels, MARKER_CHILD_BLOCK, [255, 134, 13, 255]);
        for block in [(4, 4), (315, 4), (4, 175), (315, 175)] {
            paint_block(&mut pixels, block, [10, 33, 23, 255]);
        }
        pixels
    }

    #[test]
    fn original_atlas_has_the_known_primary_product_and_texels_in_every_quarter() {
        let atlas = player_fixture().unwrap();
        let product = atlas.product().unwrap();
        assert_eq!(
            product.source().to_string(),
            "2eece9cd-6332-4868-b0d2-289879df9de8"
        );
        assert_eq!(product.product(), ImportedProductId::PRIMARY);
        for frame in 0..4 {
            for (local, expected) in [
                (Vec2::new(1.5, 1.5), PLAYER_FILL[frame]),
                (Vec2::new(5.5, 5.5), [255; 4]),
                (Vec2::new(10.5, 5.5), [255; 4]),
                (Vec2::new(5.5, 10.5), PLAYER_FILL[frame]),
                (Vec2::new(0.5, 8.5), PLAYER_BORDER),
                (Vec2::new(15.5, 8.5), PLAYER_BORDER),
                (Vec2::new(8.5, 0.5), PLAYER_BORDER),
                (Vec2::new(8.5, 15.5), PLAYER_BORDER),
            ] {
                assert_eq!(player_texel(&atlas, frame, local / 16.0), expected);
            }
        }
    }

    #[test]
    fn source_recipe_rejects_swapped_frames_wrong_rows_and_opacity_corruption() {
        let atlas = player_fixture().unwrap();
        let mut swapped = atlas.pixels().to_vec();
        for y in 0..16 {
            for x in 0..16 {
                for channel in 0..4 {
                    swapped.swap((y * 64 + x) * 4 + channel, (y * 64 + x + 16) * 4 + channel);
                }
            }
        }
        assert!(assert_player_fixture_pixels(&swapped).is_err());
        for (x, y, replacement) in [
            (21, 5, PLAYER_FILL[1]),
            (21, 10, [255; 4]),
            (32, 8, PLAYER_FILL[2]),
            (49, 1, [112, 224, 80, 254]),
        ] {
            let mut corrupted = atlas.pixels().to_vec();
            let offset = (y * 64 + x) * 4;
            corrupted[offset..offset + 4].copy_from_slice(&replacement);
            assert!(assert_player_fixture_pixels(&corrupted).is_err());
        }
    }

    #[test]
    fn four_frames_preserve_the_original_object_categories_and_pixel_counts() {
        for frame in 0..4 {
            let checks = check_pixels(&readback(frame), &resume(frame)).unwrap();
            assert_eq!(checks.player, 36);
            assert_eq!(checks.markers, 45);
            assert_eq!(checks.walls, 180);
            assert_eq!(checks.visible_pickups, 18);
            assert_eq!(checks.collected_pickups, 18);
            assert_eq!(checks.background, 36);
            assert_eq!(checks.other_scene_objects, 72);
        }
    }

    #[test]
    fn pixel_oracle_rejects_each_missing_wall() {
        let state = resume(2);
        let original = readback(2);
        check_pixels(&original, &state).unwrap();
        for (name, blocks) in WALL_BLOCKS {
            let mut missing_wall = original.clone();
            for block in blocks {
                paint_block(&mut missing_wall, block, [10, 33, 23, 255]);
            }
            let error = check_pixels(&missing_wall, &state).unwrap_err().to_string();
            assert!(error.contains(name) && error.contains("pixel"), "{error}");
        }
    }

    #[test]
    fn pixel_oracle_rejects_each_missing_authored_marker_member() {
        let state = resume(2);
        let original = readback(2);
        check_pixels(&original, &state).unwrap();
        let mut missing_root = original.clone();
        for block in MARKER_ROOT_BLOCKS {
            paint_block(&mut missing_root, block, [10, 33, 23, 255]);
        }
        let error = check_pixels(&missing_root, &state).unwrap_err().to_string();
        assert!(
            error.contains("marker 1") && error.contains("pixel"),
            "{error}"
        );
        for replacement in [[255, 77, 204, 255], [10, 33, 23, 255]] {
            let mut missing_child = original.clone();
            paint_block(&mut missing_child, MARKER_CHILD_BLOCK, replacement);
            let error = check_pixels(&missing_child, &state)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("marker 1 child") && error.contains("pixel"),
                "{error}"
            );
        }
    }

    #[test]
    fn pixel_oracle_rejects_wrong_frame_full_atlas_uv_wrong_row_and_encoded_rgb() {
        let state = resume(2);
        let original = readback(2);
        check_pixels(&original, &state).unwrap();
        for wrong_frame in [0, 1, 3] {
            let mut corrupted = original.clone();
            for block in PLAYER_FILL_BLOCKS {
                paint_block(&mut corrupted, block, LINEAR_FILL[wrong_frame]);
            }
            let error = check_pixels(&corrupted, &state).unwrap_err().to_string();
            assert!(error.contains("player atlas frame 2"), "{error}");
        }
        let mut full_atlas = original.clone();
        for (block, frame) in PLAYER_FILL_BLOCKS.into_iter().zip([0, 0, 1]) {
            paint_block(&mut full_atlas, block, LINEAR_FILL[frame]);
        }
        assert!(check_pixels(&full_atlas, &state).is_err());
        let mut wrong_row = original.clone();
        paint_block(&mut wrong_row, PLAYER_EYE_BLOCK, LINEAR_FILL[2]);
        assert!(check_pixels(&wrong_row, &state).is_err());
        let mut encoded_rgb = original;
        for block in PLAYER_FILL_BLOCKS {
            paint_block(&mut encoded_rgb, block, PLAYER_FILL[2]);
        }
        assert!(check_pixels(&encoded_rgb, &state).is_err());
    }

    #[test]
    fn pixel_oracle_keeps_exact_opaque_alpha_and_the_old_non_player_witnesses() {
        let state = resume(2);
        let original = readback(2);
        for (block, label) in [
            (PLAYER_FILL_BLOCKS[0], "player"),
            ((97, 87), "checkpoint"),
            ((129, 88), "gem-near"),
            ((199, 68), "gem-far"),
            ((200, 71), "gem-far"),
            ((254, 84), "portal"),
            (WALL_BLOCKS[0].1[0], "wall-west"),
            (WALL_BLOCKS[1].1[0], "wall-east"),
            (WALL_BLOCKS[2].1[0], "wall-north"),
            (WALL_BLOCKS[3].1[0], "wall-south"),
            (WALL_BLOCKS[4].1[0], "wall-inner"),
            ((178, 118), "marker 1"),
            (MARKER_CHILD_BLOCK, "marker 1 child"),
            ((4, 4), "level background"),
        ] {
            let mut corrupted = original.clone();
            let offset = (block.1 * 3 * 960 + block.0 * 3) * 4;
            corrupted[offset + 3] = 254;
            let error = check_pixels(&corrupted, &state).unwrap_err().to_string();
            assert!(
                error.contains(label) && error.contains("exact alpha"),
                "{error}"
            );
        }
    }
}
