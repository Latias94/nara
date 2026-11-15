#![forbid(unsafe_code)]

use std::panic::catch_unwind;

use motion_physics::authored::CollisionDefinition;
use nara::{core::Vec2, transform::Transform2d};

#[path = "support/authored_collision/model.rs"]
mod model;
#[path = "support/authored_collision/oracle.rs"]
mod oracle;

use model::{Authored, BODY_COLOR, CLEAR_COLOR, HEIGHT, PLATFORM_COLOR, Variant, WIDTH, authored};

fn transform(translation: [f32; 2], rotation: f32, scale: [f32; 2]) -> Transform2d {
    Transform2d {
        translation: Vec2::from_array(translation),
        rotation,
        scale: Vec2::from_array(scale),
    }
}

fn axis_aligned() -> Authored {
    Authored {
        variant: Variant::Initial,
        ancestor: transform([0.0, 0.0], 0.0, [1.0, 1.0]),
        platform: transform([3.0, -0.5], 0.0, [1.0, 1.0]),
        platform_box: CollisionDefinition {
            center: Vec2::new(-0.5, 0.25),
            half_width: 2.0,
            half_height: 0.5,
            dynamic: false,
        },
        body: transform([3.0, 1.5], 0.0, [1.0, 1.0]),
        body_box: CollisionDefinition {
            center: Vec2::new(0.25, 0.75),
            half_width: 0.5,
            half_height: 0.25,
            dynamic: true,
        },
    }
}

fn pixel(image: &oracle::ExpectedImage, x: usize, y: usize) -> [u8; 4] {
    image.pixels()[y * WIDTH as usize + x]
}

fn count(image: &oracle::ExpectedImage, color: [u8; 4]) -> usize {
    image
        .pixels()
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

#[test]
fn source_chain_retains_shear_all_four_mirrors_and_offset_corners() {
    let definition = CollisionDefinition {
        center: Vec2::new(0.25, -0.5),
        half_width: 1.0,
        half_height: 0.5,
        dynamic: false,
    };
    let offsets = [
        [1.353_553_390_593_273_7, -1.487_436_867_076_458_2],
        [4.181_980_515_339_465, -0.073_223_304_703_363_13],
        [2.767_766_952_966_369, 0.633_883_476_483_184_3],
        [-0.060_660_171_779_821_415, -0.780_330_085_889_910_6],
    ];
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let root = transform([3.0, -2.0], 0.0, [2.0 * sx, sy]);
        let local = transform([0.5, -0.25], std::f32::consts::FRAC_PI_4, [1.0, 1.0]);
        let actual = oracle::corners(definition, &[root, local]);
        for (corner, offset) in actual.into_iter().zip(offsets) {
            let expected = [
                3.0 + f64::from(sx) * offset[0],
                -2.0 + f64::from(sy) * offset[1],
            ];
            for (actual, expected) in corner.into_iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < 0.000_001,
                    "{actual} differs from {expected}"
                );
            }
        }
        let a = [actual[1][0] - actual[0][0], actual[1][1] - actual[0][1]];
        let b = [actual[3][0] - actual[0][0], actual[3][1] - actual[0][1]];
        assert!(
            (a[0] * b[0] + a[1] * b[1]).abs() > 2.9,
            "genuine shear survives reflections"
        );
    }
}

#[test]
fn authored_variants_have_distinct_numeric_platforms_and_offset_rest_heights() {
    for (variant, expected_corner, expected_rest) in [
        (
            Variant::Initial,
            [0.069_928_804_145_443_42, -0.884_158_666_612_368_3],
            -0.570_167_705_924_331_6,
        ),
        (
            Variant::Edited,
            [4.387_710_390_950_173, -0.114_600_451_875_894_52],
            0.761_011_113_563_637_9,
        ),
    ] {
        let source = authored(variant);
        let actual = oracle::corners(source.platform_box, &[source.ancestor, source.platform]);
        for (actual, expected) in actual[0].into_iter().zip(expected_corner) {
            assert!((actual - expected).abs() < 0.000_000_001);
        }
        assert!((oracle::resting_height(&source) - expected_rest).abs() < 0.000_000_001);
        let shape_bottom = expected_rest + f64::from(source.body_box.center.y)
            - f64::from(source.body_box.half_height);
        let platform_top = actual
            .into_iter()
            .map(|corner| corner[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((shape_bottom - platform_top).abs() < 0.000_000_001);
        assert!(
            (expected_rest - platform_top).abs() > 0.39,
            "root and collider center must stay distinct"
        );
    }
}

#[test]
fn pixel_centers_cover_full_orthographic_image_with_nonzero_anchor_offsets() {
    let source = axis_aligned();
    let expected = oracle::image_from_poses(&source, None, source.body, 0.0, false);
    assert_eq!(expected.pixels().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(expected.ambiguous_pixels, 0);
    assert_eq!(count(&expected, BODY_COLOR), 32);
    assert_eq!(count(&expected, PLATFORM_COLOR), 256);
    assert_eq!(count(&expected, CLEAR_COLOR), 9_952);
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let color = if (62..70).contains(&x) && (32..36).contains(&y) {
                BODY_COLOR
            } else if (44..76).contains(&x) && (50..58).contains(&y) {
                PLATFORM_COLOR
            } else {
                CLEAR_COLOR
            };
            assert_eq!(
                pixel(&expected, x, y),
                color,
                "independent integer rectangle ({x}, {y})"
            );
        }
    }
    let agreement = oracle::assert_matches(&expected, expected.pixels());
    assert_eq!(
        agreement,
        oracle::ImageAgreement {
            ambiguous_pixels: 0,
            alternate_pixels: 0
        }
    );
}

#[test]
fn opaque_body_wins_platform_overlap_without_blended_or_transparent_pixels() {
    let mut source = axis_aligned();
    source.body.translation.y = -0.5;
    let expected = oracle::image_from_poses(&source, None, source.body, 0.0, false);
    assert_eq!(expected.ambiguous_pixels, 0);
    assert_eq!(count(&expected, BODY_COLOR), 32);
    assert_eq!(count(&expected, PLATFORM_COLOR), 240);
    assert_eq!(count(&expected, CLEAR_COLOR), 9_968);
    assert_eq!(pixel(&expected, 65, 50), BODY_COLOR);
    assert_eq!(pixel(&expected, 61, 50), PLATFORM_COLOR);
    assert!(expected.pixels().iter().all(|pixel| pixel[3] == 255));
}

#[test]
fn presentation_interpolates_native_endpoints_while_pause_and_fresh_sources_use_current() {
    let source = axis_aligned();
    let previous = source.body;
    let mut current = previous;
    current.translation.y -= 1.0;
    for (alpha, first_row) in [(0.0, 32), (0.25, 34), (0.5, 36), (1.0, 40)] {
        let expected = oracle::image_from_poses(&source, Some(previous), current, alpha, false);
        assert_eq!(count(&expected, BODY_COLOR), 32);
        assert_eq!(pixel(&expected, 64, first_row), BODY_COLOR);
        assert_eq!(pixel(&expected, 64, first_row - 1), CLEAR_COLOR);
        assert_eq!(pixel(&expected, 64, first_row + 4), CLEAR_COLOR);
    }
    let paused = oracle::image_from_poses(&source, Some(previous), current, 0.25, true);
    let fresh = oracle::image_from_poses(&source, None, current, 0.25, false);
    let current_only = oracle::image_from_poses(&source, Some(previous), current, 1.0, false);
    assert_eq!(paused.pixels(), current_only.pixels());
    assert_eq!(fresh.pixels(), current_only.pixels());
    let moving = oracle::image_from_poses(&source, Some(previous), current, 0.25, false);
    assert_ne!(moving.pixels(), current_only.pixels());
}

#[test]
fn byte_corruption_and_one_pixel_geometry_shift_are_rejected() {
    let source = axis_aligned();
    let expected = oracle::image_from_poses(&source, None, source.body, 0.0, false);
    for (index, channel) in [(33 * WIDTH as usize + 65, 3), (0, 0), (0, 3)] {
        let mut corrupted = expected.pixels().to_vec();
        corrupted[index][channel] ^= 1;
        assert!(catch_unwind(|| oracle::assert_matches(&expected, &corrupted)).is_err());
    }
    let mut shifted_pose = source.body;
    shifted_pose.translation.x += 0.125;
    let shifted = oracle::image_from_poses(&source, None, shifted_pose, 0.0, false);
    assert!(catch_unwind(|| oracle::assert_matches(&expected, shifted.pixels())).is_err());
}

#[test]
fn edge_allowance_accepts_only_exact_competing_colors_and_counts_actual_alternates() {
    let mut source = axis_aligned();
    source.body_box.half_width = 0.125;
    source.body_box.half_height = 0.125;
    source.body.translation.x += 0.0625;
    let expected = oracle::image_from_poses(&source, None, source.body, 0.0, false);
    assert_eq!(expected.ambiguous_pixels, 4);
    let index = 33 * WIDTH as usize + 65;
    let mut alternative = expected.pixels().to_vec();
    alternative[index] = if alternative[index] == BODY_COLOR {
        CLEAR_COLOR
    } else {
        BODY_COLOR
    };
    let agreement = oracle::assert_matches(&expected, &alternative);
    assert_eq!(
        agreement,
        oracle::ImageAgreement {
            ambiguous_pixels: 4,
            alternate_pixels: 1
        }
    );
    alternative[index] = PLATFORM_COLOR;
    assert!(catch_unwind(|| oracle::assert_matches(&expected, &alternative)).is_err());
    alternative[index] = [255, 0, 0, 254];
    assert!(catch_unwind(|| oracle::assert_matches(&expected, &alternative)).is_err());
    alternative[index] = [254, 0, 0, 255];
    assert!(catch_unwind(|| oracle::assert_matches(&expected, &alternative)).is_err());
    let mut outside_edge = expected.pixels().to_vec();
    outside_edge[index - 1] = BODY_COLOR;
    assert!(catch_unwind(|| oracle::assert_matches(&expected, &outside_edge)).is_err());
    let mut near_pose = source.body;
    near_pose.translation.x += 0.000_062_5;
    let near = oracle::image_from_poses(&source, None, near_pose, 0.0, false);
    assert_eq!(
        near.ambiguous_pixels, 4,
        "0.0005-pixel edge distance is admitted"
    );
    let mut distant_pose = source.body;
    distant_pose.translation.x += 0.000_25;
    let distant = oracle::image_from_poses(&source, None, distant_pose, 0.0, false);
    assert_eq!(
        distant.ambiguous_pixels, 0,
        "0.002-pixel edge distance is refused"
    );
}

#[test]
fn edge_ambiguity_cap_refuses_a_broad_raster_escape_hatch() {
    let mut source = axis_aligned();
    source.body.translation.x += 0.0625;
    source.body_box.half_height = 0.375;
    assert!(
        catch_unwind(|| oracle::image_from_poses(&source, None, source.body, 0.0, false)).is_err()
    );
}
