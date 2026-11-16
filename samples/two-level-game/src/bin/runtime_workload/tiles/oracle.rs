//! Independent finite witnesses for this fixture, not a Scene parser or general rasterizer.
//!
//! Source pixels were inspected independently in both original 16x16 PNGs. Expected colors
//! never come from the captured texture or GPU readback. Fixed source and decoded-pixel digests
//! make a changed source fail closed. Ground witnesses cover selected canvas pixel centers;
//! they do not establish every rasterized pixel or the original swapchain's contents.

use nara::{
    fs::ContentDigest,
    image::{ImageColorSpace, ImageFormat},
    material::{AlphaMode2d, SamplerDescriptor},
    prelude::{Color, Vec2},
    render::{RenderFrameTarget, ViewLoadOp, ViewportRect},
    render_wgpu::WgpuStockPolicy,
    scene::{SourceDocumentKind, SourceDocumentOrigin},
};
use serde::Serialize;
use two_level_game::{GamePhase, levels::LevelId};

use super::{CANVAS_EXTENT, PresentedTileFrame, WINDOW_EXTENT, require};

pub(super) const PNG_DIGEST: ContentDigest = ContentDigest::from_parts(
    154,
    [
        0x3c, 0xf5, 0xc7, 0xd4, 0xe9, 0xb2, 0xa2, 0x53, 0x24, 0xdd, 0x86, 0x20, 0x3b, 0x3c, 0xfb,
        0xea, 0x9f, 0xd8, 0x7e, 0x6b, 0xe2, 0x41, 0xec, 0xe5, 0xf7, 0xa5, 0x84, 0x01, 0x8c, 0x0c,
        0x20, 0x2a,
    ],
);
const PIXEL_DIGEST: ContentDigest = ContentDigest::from_parts(
    1024,
    [
        0x3d, 0x78, 0xbf, 0xb2, 0xe5, 0xa2, 0x34, 0x4e, 0x1e, 0x8b, 0x68, 0x5b, 0x01, 0xd8, 0x82,
        0x5a, 0x52, 0xef, 0xb4, 0x54, 0x49, 0x76, 0xb6, 0x3b, 0x8f, 0xc8, 0x3b, 0x5f, 0x58, 0x07,
        0x36, 0x4b,
    ],
);
const RGBA_BYTES: usize = 960 * 540 * 4;

pub(super) fn tile_side(population: usize) -> Result<usize, String> {
    match population {
        64 => Ok(8),
        4096 => Ok(64),
        16_384 => Ok(128),
        _ => Err("TilemapTravel oracle supports exactly 64, 4096 or 16384 cells".into()),
    }
}

pub(super) fn assert_fixed_assets() -> Result<(), String> {
    require(
        ContentDigest::of_bytes(include_bytes!("../../../../assets/textures/meadow.png"))
            == PNG_DIGEST
            && ContentDigest::of_bytes(include_bytes!("../../../../assets/textures/cavern.png"))
                == PNG_DIGEST,
        "the original PNGs changed; independently reinspect source texels before updating the oracle",
    )
}

pub(super) fn assert_packet(frame: &PresentedTileFrame) -> Result<(), String> {
    let game = &frame.game;
    let snapshot = &game.snapshot;
    let packet = frame.input.packet();
    let level = snapshot
        .level
        .ok_or("initialized tile capture lacks its game level")?
        .level();
    let path = match level {
        LevelId::Meadow => "scenes/meadow.scene.json",
        LevelId::Cavern => "scenes/cavern.scene.json",
    };
    require(
        snapshot.initialized
            && snapshot.phase == GamePhase::Running
            && !snapshot.restoring
            && snapshot.error.is_none()
            && snapshot.player.is_finite()
            && snapshot.current_instance == Some(game.instance.instance_id())
            && snapshot.scene_members == 6
            && game.instance.entity_ids().len() == 6
            && packet.source_generation().runtime_generation() == Some(frame.presentation.runtime)
            && packet.frame_index() == frame.presentation.render_frame
            && game.source.kind() == SourceDocumentKind::Scene
            && matches!(game.source.origin(), SourceDocumentOrigin::ProjectLocation { path: source, .. }
                if source.as_str() == path),
        "original tile input, running snapshot and admitted CurrentScene receipt disagree",
    )?;
    let RenderFrameTarget::Window(window) = packet.target() else {
        return Err("the original tile input is not a native-window transaction".into());
    };
    require(
        window == frame.presentation.window
            && packet.target().extent() == WINDOW_EXTENT
            && window.resolution.scale_factor.is_finite()
            && window.resolution.scale_factor > 0.0,
        "original tile window or physical extent differs from the retained presentation",
    )?;
    let canvas = packet
        .world_canvas()
        .ok_or("original tile input lacks its world canvas")?;
    let mapping = canvas.mapping();
    let physical_min = mapping
        .content_rect()
        .min
        .map(|value| value * mapping.pixels_per_point());
    let physical_max = mapping
        .content_rect()
        .max
        .map(|value| value * mapping.pixels_per_point());
    require(
        canvas.extent() == CANVAS_EXTENT
            && mapping.target() == CANVAS_EXTENT
            && mapping.pixels_per_point() == window.resolution.scale_factor as f32
            && physical_min.iter().all(|value| value.abs() < 0.001)
            && (physical_max[0] - 960.0).abs() < 0.001
            && (physical_max[1] - 540.0).abs() < 0.001,
        "original 320x180 tile canvas does not map to its exact 960x540 physical window",
    )?;
    let [view] = packet.views() else {
        return Err("tile oracle requires one original View".into());
    };
    require(
        view.target.window_id(Some(window.id)) == Some(window.id)
            && Some(view.viewport) == ViewportRect::from_extent(CANVAS_EXTENT)
            && view
                .world_to_viewport(Vec2::ZERO)
                .abs_diff_eq(Vec2::new(160.0, 90.0), 0.0001)
            && view
                .world_to_viewport(Vec2::new(320.0, 180.0))
                .abs_diff_eq(Vec2::new(320.0, 0.0), 0.0001)
            && view.load == ViewLoadOp::Clear(background(level)),
        "original tile View/projection/level clear differs from the fixed consumer",
    )
}

/// Validate the finite original stock footprint without looking up a newer World or source.
pub(super) fn assert_ground(frame: &PresentedTileFrame, population: usize) -> Result<(), String> {
    assert_fixed_assets()?;
    assert_packet(frame)?;
    let side = tile_side(population)?;
    require(
        frame.game.population == population
            && frame.game.maps == if population == 16_384 { 2 } else { 1 },
        "retained original World population differs from this tile workload",
    )?;
    let view = &frame.input.packet().views()[0];
    let stock = frame
        .input
        .captured::<WgpuStockPolicy>()
        .ok_or("original tile input is not the installed stock policy")?;
    let batches = stock
        .sprite_batches()
        .ok_or("original tile input lacks captured sprite/tile batches")?;
    let tile_size = 256.0 / side as f32;
    let x_axis = view
        .mapping
        .world_vector_to_clip(Vec2::new(tile_size * 0.5, 0.0));
    let y_axis = view
        .mapping
        .world_vector_to_clip(Vec2::new(0.0, tile_size * 0.5));
    let mut seen = vec![false; population];
    let mut count = 0;
    let mut objects_started = false;
    for batch in batches.as_slice() {
        if batch.layer != -10 {
            objects_started = true;
            require(
                batch.layer == 0,
                "captured tile fixture contains an unexpected render layer",
            )?;
            for instance in &batch.instances {
                let center = view.mapping.clip_to_world(instance.center);
                let radius_y = (view
                    .mapping
                    .clip_to_world(instance.center + instance.x_axis)
                    .y
                    - center.y)
                    .abs()
                    + (view
                        .mapping
                        .clip_to_world(instance.center + instance.y_axis)
                        .y
                        - center.y)
                        .abs();
                require(
                    center.is_finite() && radius_y.is_finite() && center.y.abs() + radius_y < 88.0,
                    "an original game object overlaps the oracle's ground-only Y region",
                )?;
            }
            continue;
        }
        require(
            !objects_started
                && batch.view_index == 0
                && batch.view_order == view.order
                && batch.target == view.target
                && batch.sort_key == 0
                && batch.material_extension.is_none()
                && batch.material.alpha_mode == AlphaMode2d::Blend,
            "original ground batch routing/order/material differs from the fixture",
        )?;
        require(
            batch.material.sampler == SamplerDescriptor::NEAREST_CLAMP,
            "tile oracle requires the fixture TileSet to explicitly use NEAREST_CLAMP; LINEAR_CLAMP blends the center and quarter texels",
        )?;
        let image = batch
            .image
            .as_ref()
            .ok_or("original ground has no retained PNG texture")?
            .resource();
        require(
            image.extent().width == 16
                && image.extent().height == 16
                && image.format() == ImageFormat::Rgba8
                && image.color_space() == ImageColorSpace::Srgb
                && image.has_retention_owner()
                && ContentDigest::of_bytes(image.pixels()) == PIXEL_DIGEST,
            "original ground does not retain the independently inspected sRGB diamond and allocation owner",
        )?;
        for instance in &batch.instances {
            let center = view.mapping.clip_to_world(instance.center);
            require(center.is_finite(), "original ground center is not finite")?;
            let column = ((center.x + 120.0) / tile_size - 0.5).round();
            let row = ((center.y + 128.0) / tile_size - 0.5).round();
            require(
                (0.0..side as f32).contains(&column) && (0.0..side as f32).contains(&row),
                "original ground lies outside world [-120,136] x [-128,128]",
            )?;
            let expected = Vec2::new(
                -120.0 + (column + 0.5) * tile_size,
                -128.0 + (row + 0.5) * tile_size,
            );
            require(
                center.abs_diff_eq(expected, 0.0001)
                    && instance.x_axis.abs_diff_eq(x_axis, 0.00001)
                    && instance.y_axis.abs_diff_eq(y_axis, 0.00001)
                    && instance.color == Color::WHITE
                    && instance.uv.min == Vec2::ZERO
                    && instance.uv.size == Vec2::ONE,
                "original ground geometry, white cell tint or full tile-0 atlas UV differs",
            )?;
            let index = row as usize * side + column as usize;
            require(!seen[index], "original ground contains a duplicate cell")?;
            seen[index] = true;
            count += 1;
        }
    }
    require(
        count == population && seen.into_iter().all(|present| present),
        "original stock input does not contain the complete declared ground footprint",
    )
}

#[derive(Default, Debug, Serialize)]
pub(super) struct PixelChecks {
    pub ground_tile_witness_count: usize,
    pub physical_ground_pixels: usize,
    pub white_patch_pixels: usize,
    pub grey_patch_pixels: usize,
    pub center_texel_pixels: usize,
    pub background_pixels: usize,
}

#[derive(Clone, Copy)]
enum Patch {
    White,
    Grey,
    Center,
}

struct Witness {
    x: usize,
    y: usize,
    patch: Patch,
    quadrant: usize,
}

fn witnesses(population: usize) -> Result<Vec<Witness>, String> {
    let tile_size = 256.0 / tile_side(population)? as f32;
    let mut buckets = [0; 12];
    let mut result = Vec::with_capacity(24);
    for y in 0..180 {
        for x in 0..320 {
            let world_x = (x as f32 + 0.5 - 160.0) * 2.0;
            let world_y = (90.0 - y as f32 - 0.5) * 2.0;
            if !(-118.0..134.0).contains(&world_x) || !(88.0..126.0).contains(&world_y.abs()) {
                continue;
            }
            let u = (world_x + 120.0).rem_euclid(tile_size) / tile_size;
            let v = 1.0 - (world_y + 128.0).rem_euclid(tile_size) / tile_size;
            let patch = if (0.40..0.49).contains(&u) && (0.28..0.38).contains(&v) {
                Patch::White
            } else if (0.51..0.60).contains(&u) && (0.62..0.72).contains(&v) {
                Patch::Grey
            } else if population == 4096 && u == 0.25 && v == 0.25 {
                Patch::White
            } else if population == 16_384 && u == 0.5 && v == 0.5 {
                Patch::Center
            } else {
                continue;
            };
            let quadrant = usize::from(world_x >= 8.0) + 2 * usize::from(world_y < 0.0);
            let kind = match patch {
                Patch::White => 0,
                Patch::Grey => 1,
                Patch::Center => 2,
            };
            let bucket = kind * 4 + quadrant;
            if buckets[bucket] == 2 {
                continue;
            }
            buckets[bucket] += 1;
            result.push(Witness {
                x,
                y,
                patch,
                quadrant,
            });
        }
    }
    let mut quadrants = [false; 4];
    for witness in &result {
        quadrants[witness.quadrant] = true;
    }
    require(
        !result.is_empty() && quadrants.into_iter().all(|present| present),
        "tile oracle found no independent unoccluded ground centers in every terrain quadrant",
    )?;
    Ok(result)
}

pub(super) fn check_pixels(
    rgba8: &[u8],
    population: usize,
    level: LevelId,
) -> Result<PixelChecks, String> {
    require(
        rgba8.len() == RGBA_BYTES,
        "tile replay has an unexpected RGBA8 extent",
    )?;
    let mut checks = PixelChecks::default();
    for witness in witnesses(population)? {
        let color = match witness.patch {
            Patch::White => [255; 4],
            Patch::Grey | Patch::Center => [linear_grey(), linear_grey(), linear_grey(), 255],
        };
        check_block(rgba8, witness.x, witness.y, color, "independent ground")?;
        checks.ground_tile_witness_count += 1;
        checks.physical_ground_pixels += 9;
        match witness.patch {
            Patch::White => checks.white_patch_pixels += 9,
            Patch::Grey => checks.grey_patch_pixels += 9,
            Patch::Center => checks.center_texel_pixels += 9,
        }
    }
    let clear = background(level);
    let color = [clear.r, clear.g, clear.b, clear.a].map(|value| (value * 255.0).round() as u8);
    for (x, y) in [(4, 4), (315, 4), (4, 175), (315, 175)] {
        check_block(rgba8, x, y, color, "original level clear")?;
        checks.background_pixels += 9;
    }
    require(
        checks.ground_tile_witness_count > 0,
        "tile replay has no ground witness",
    )?;
    Ok(checks)
}

fn linear_grey() -> u8 {
    (((185.0_f32 / 255.0 + 0.055) / 1.055).powf(2.4) * 255.0).round() as u8
}

fn background(level: LevelId) -> Color {
    match level {
        LevelId::Meadow => Color::rgb(0.04, 0.13, 0.09),
        LevelId::Cavern => Color::rgb(0.07, 0.05, 0.15),
    }
}

fn check_block(
    rgba8: &[u8],
    x: usize,
    y: usize,
    expected: [u8; 4],
    label: &str,
) -> Result<(), String> {
    for py in y * 3..y * 3 + 3 {
        for px in x * 3..x * 3 + 3 {
            let offset = (py * 960 + px) * 4;
            let actual = &rgba8[offset..offset + 4];
            require(
                actual[..3]
                    .iter()
                    .zip(&expected[..3])
                    .all(|(a, b)| a.abs_diff(*b) <= 3)
                    && actual[3] == expected[3],
                format!(
                    "tile replay {label} ({px},{py}): expected RGB {expected:?} +/-3 and exact alpha, got {actual:?}"
                ),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_sources_and_ground_centers_cover_every_population() {
        assert_fixed_assets().unwrap();
        assert_eq!(linear_grey(), 124);
        for population in [64, 4096, 16_384] {
            let samples = witnesses(population).unwrap();
            assert!(!samples.is_empty() && samples.len() <= 24);
            assert!(
                (0..4).all(|quadrant| samples.iter().any(|sample| sample.quadrant == quadrant))
            );
        }
        assert!(tile_side(65).is_err());
    }

    #[test]
    fn opaque_clear_or_missing_tiles_cannot_qualify_as_ground() {
        let mut pixels = vec![0; RGBA_BYTES];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[10, 33, 23, 255]);
        }
        for population in [64, 4096, 16_384] {
            assert!(check_pixels(&pixels, population, LevelId::Meadow).is_err());
        }
    }

    #[test]
    fn wrong_transfer_or_alpha_and_a_changed_physical_subpixel_fail() {
        let mut pixels = vec![255; RGBA_BYTES];
        assert!(check_block(&pixels, 1, 1, [255; 4], "control").is_ok());
        let offset = (4 * 960 + 4) * 4;
        pixels[offset + 3] = 254;
        assert!(check_block(&pixels, 1, 1, [255; 4], "alpha control").is_err());
        pixels[offset + 3] = 255;
        pixels[offset] = 251;
        assert!(check_block(&pixels, 1, 1, [255; 4], "RGB control").is_err());
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[185, 185, 185, 255]);
        }
        assert!(check_block(&pixels, 1, 1, [124, 124, 124, 255], "transfer control").is_err());
    }
}
