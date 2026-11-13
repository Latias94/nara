//! Expectations for the two shipped Scenes; this is not a general rasterizer or schema reader.

use nara::{
    material::{AlphaMode2d, SamplerDescriptor},
    prelude::{Color, Transform2d, Vec2},
    render::{RenderFrameTarget, ViewLoadOp, ViewportRect},
    render_wgpu::WgpuStockPolicy,
    scene::{SourceDocumentKind, SourceDocumentOrigin},
};
use two_level_game::{GamePhase, GameSnapshot, levels::LevelId, resume::ResumeState};

use super::{CANVAS_EXTENT, Result, SealedGameFrame, WINDOW_EXTENT, require};

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
            && snapshot.scene_members == 6
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
            && frame.instance.entity_ids().len() == 6
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
    require(
        batches.total_instances() == objects.len(),
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
        let matrix = object.transform.matrix();
        let center = view.mapping.world_to_clip(object.transform.translation);
        let x = view
            .mapping
            .world_vector_to_clip(matrix.transform_vector2(Vec2::new(object.size * 0.5, 0.0)));
        let y = view
            .mapping
            .world_vector_to_clip(matrix.transform_vector2(Vec2::new(0.0, object.size * 0.5)));
        require(
            batch.view_index == 0
                && batch.view_order == view.order
                && batch.target == view.target
                && batch.layer == 0
                && batch.sort_key == 0
                && batch.material_extension.is_none()
                && batch.material.alpha_mode == AlphaMode2d::Blend
                && batch.material.sampler == SamplerDescriptor::LINEAR_CLAMP
                && instance.center.abs_diff_eq(center, 0.00001)
                && instance.x_axis.abs_diff_eq(x, 0.00001)
                && instance.y_axis.abs_diff_eq(y, 0.00001)
                && instance.color == object.color
                && instance.uv.min == Vec2::ZERO
                && instance.uv.size == Vec2::ONE
                && batch.image.is_some() == object.pickup,
            format!(
                "original ordered stock Sprite differs from expected {}",
                object.name
            ),
        )?;
        if let Some(image) = &batch.image {
            let image = image.resource();
            require(
                image.extent().width == 16
                    && image.extent().height == 16
                    && image.color_space() == nara::image::ImageColorSpace::Srgb
                    && image.pixels().len() == 16 * 16 * 4
                    && image.has_retention_owner()
                    && image.pixels()[(3 * 16 + 7) * 4..(3 * 16 + 7) * 4 + 4] == [255; 4]
                    && image.pixels()[(11 * 16 + 9) * 4..(11 * 16 + 9) * 4 + 4]
                        == [185, 185, 185, 255],
                "captured pickup must retain the real sRGB diamond texture and its allocation owner",
            )?;
        }
    }
    Ok(())
}

struct GameObject {
    name: String,
    transform: Transform2d,
    size: f32,
    color: Color,
    pickup: bool,
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
    let object = |name: &str, position, size, color, pickup| GameObject {
        name: name.into(),
        transform: Transform2d::from_translation(position),
        size,
        color,
        pickup,
    };
    let mut objects = vec![
        GameObject {
            name: "player".into(),
            transform: Transform2d {
                translation: Vec2::from_array(expected.player.translation),
                rotation: expected.player.rotation,
                scale: Vec2::from_array(expected.player.scale),
            },
            size: 18.0,
            color: Color::rgb(0.25, 0.7, 1.0),
            pickup: false,
        },
        object(
            "checkpoint",
            Vec2::new(-120.0, 0.0),
            14.0,
            Color::rgb(0.25, 1.0, 0.5),
            false,
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
            position,
            18.0,
            Color::rgba(1.0, 0.8, 0.2, if collected { 0.0 } else { 1.0 }),
            true,
        ));
    }
    objects.push(object(
        "portal",
        Vec2::new(200.0, 0.0),
        26.0,
        Color::rgb(0.8, 0.3, 1.0),
        false,
    ));
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
            position,
            10.0,
            Color::rgb(1.0, 0.3, 0.8),
            false,
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
        self.color.a > 0.0 && self.local(world).abs().max_element() <= self.size * 0.5 + 2.0
    }
}

pub(super) fn check_pixels(rgba8: &[u8], expected: &ResumeState) -> Result<PixelChecks> {
    require(
        rgba8.len() == 960 * 540 * 4,
        "unexpected replay readback length",
    )?;
    let objects = objects(expected)?;
    let mut checks = PixelChecks::default();
    let clear = background(expected.progress.location.level.level());
    for (index, object) in objects.iter().enumerate() {
        let mut found = 0;
        for y in 0..180 {
            for x in 0..320 {
                let world = Vec2::new(
                    (x as f32 + 0.5 - 160.0) * 2.0,
                    (90.0 - y as f32 - 0.5) * 2.0,
                );
                let local = object.local(world);
                if !local.is_finite() || local.abs().max_element() > object.size * 0.5 - 1.0 {
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
                if object.pickup {
                    let uv = Vec2::new(local.x / 18.0 + 0.5, 0.5 - local.y / 18.0);
                    let white = (0.40..0.49).contains(&uv.x) && (0.28..0.38).contains(&uv.y);
                    let shadow = (0.51..0.60).contains(&uv.x) && (0.62..0.72).contains(&uv.y);
                    if !white && !shadow {
                        continue;
                    }
                    if shadow {
                        let linear = ((185.0_f32 / 255.0 + 0.055) / 1.055).powf(2.4);
                        color.r *= linear;
                        color.g *= linear;
                        color.b *= linear;
                    }
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
        if index == 0 || object.pickup || index >= 5 {
            require(
                found > 0,
                format!(
                    "{} has no unoccluded interior pixel witness; choose a discriminating game state",
                    object.name
                ),
            )?;
        }
        match index {
            0 => checks.player += found,
            2 | 3 if object.color.a == 0.0 => checks.collected_pickups += found,
            2 | 3 => checks.visible_pickups += found,
            5.. => checks.markers += found,
            _ => checks.other_scene_objects += found,
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
                actual.iter().zip(channels).all(|(a, b)| a.abs_diff(b) <= 3),
                format!(
                    "replay {label} pixel ({px}, {py}): expected {channels:?} +/-3, got {actual:?}"
                ),
            )?;
        }
    }
    Ok(())
}
