//! Independent source-field geometry, fixed/presented checks, and exact pixel-center coverage.

use motion_physics::authored::{CollisionDefinition, DebugSnapshot};
use nara::{
    core::{Mat3, Vec2},
    transform::{SpatialSnapshotPhase, Transform2d},
};

use super::model::{
    Authored, BODY_COLOR, CAMERA_CENTER, CAMERA_HEIGHT, CLEAR_COLOR, HEIGHT, PLATFORM_COLOR,
    Sample, WIDTH,
};

pub type Point = [f64; 2];
pub type Corners = [Point; 4];

const EDGE_DISTANCE_PIXELS: f64 = 0.001;
const MAX_AMBIGUOUS_PIXELS: usize = 8;
const BODY_BIT: u8 = 1;
const PLATFORM_BIT: u8 = 2;
const CLEAR_BIT: u8 = 4;

#[derive(Debug, Clone)]
pub struct ExpectedImage {
    pixels: Vec<[u8; 4]>,
    allowed: Vec<u8>,
    pub ambiguous_pixels: usize,
}

impl ExpectedImage {
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageAgreement {
    pub ambiguous_pixels: usize,
    pub alternate_pixels: usize,
}

/// Applies source TRS fields in a root-to-leaf chain, without constructing engine matrices.
pub fn point(mut local: Point, chain: &[Transform2d]) -> Point {
    for transform in chain.iter().rev() {
        let x = local[0] * f64::from(transform.scale.x);
        let y = local[1] * f64::from(transform.scale.y);
        let angle = f64::from(transform.rotation);
        local = [
            f64::from(transform.translation.x) + angle.cos() * x - angle.sin() * y,
            f64::from(transform.translation.y) + angle.sin() * x + angle.cos() * y,
        ];
    }
    assert!(local.into_iter().all(f64::is_finite));
    local
}

/// Keeps source corner correspondence even when a reflected ancestor reverses winding.
pub fn corners(definition: CollisionDefinition, chain: &[Transform2d]) -> Corners {
    let center = [
        f64::from(definition.center.x),
        f64::from(definition.center.y),
    ];
    let width = f64::from(definition.half_width);
    let height = f64::from(definition.half_height);
    assert!(width.is_finite() && width > 0.0);
    assert!(height.is_finite() && height > 0.0);
    [
        [-width, -height],
        [width, -height],
        [width, height],
        [-width, height],
    ]
    .map(|offset| point([center[0] + offset[0], center[1] + offset[1]], chain))
}

/// Returns the root entity's resting Y, accounting for its collider-local center offset.
pub fn resting_height(authored: &Authored) -> f64 {
    assert_rigid_root(authored.body);
    let platform = corners(
        authored.platform_box,
        &[authored.ancestor, authored.platform],
    );
    let horizontal = [
        platform[1][0] - platform[0][0],
        platform[1][1] - platform[0][1],
    ];
    assert!(horizontal[0].abs() > 0.0);
    assert!(
        horizontal[1].abs() <= 0.000_001,
        "rest-height fixture requires a horizontal top"
    );
    let highest = platform
        .into_iter()
        .map(|corner| corner[1])
        .fold(f64::NEG_INFINITY, f64::max);
    highest + f64::from(authored.body_box.half_height) - f64::from(authored.body_box.center.y)
}

/// Uses native root observations only as pose inputs; all geometry comes from authored fields.
pub fn expected_image(sample: &Sample) -> ExpectedImage {
    let previous = sample
        .fixed
        .as_ref()
        .filter(|fixed| fixed.body_entity == sample.body_entity)
        .map(|fixed| {
            assert_eq!(
                fixed.body_after, sample.body,
                "fixed writeback must be the current root pose"
            );
            fixed.body_before
        });
    image_from_poses(
        &sample.authored,
        previous,
        sample.body,
        f64::from(sample.render_time.interpolation_alpha),
        sample.paused,
    )
}

/// Pure CPU entry point: no render input, extracted geometry, or GPU resource is needed.
pub fn image_from_poses(
    authored: &Authored,
    previous: Option<Transform2d>,
    current: Transform2d,
    alpha: f64,
    paused: bool,
) -> ExpectedImage {
    let translation = presented_translation(previous, current, alpha, paused);
    let platform = corners(
        authored.platform_box,
        &[authored.ancestor, authored.platform],
    )
    .map(project);
    let body = corners(authored.body_box, &[])
        .map(|local| project([local[0] + translation[0], local[1] + translation[1]]));
    let mut image = ExpectedImage {
        pixels: Vec::with_capacity((WIDTH * HEIGHT) as usize),
        allowed: Vec::with_capacity((WIDTH * HEIGHT) as usize),
        ambiguous_pixels: 0,
    };
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let center = [f64::from(x) + 0.5, f64::from(y) + 0.5];
            let (in_body, body_edge) = coverage(center, &body);
            let (in_platform, platform_edge) = coverage(center, &platform);
            let nominal = color(in_body, in_platform);
            let mut allowed = palette_bit(nominal);
            for body_inside in [false, true] {
                for platform_inside in [false, true] {
                    if (body_inside == in_body || body_edge)
                        && (platform_inside == in_platform || platform_edge)
                    {
                        allowed |= palette_bit(color(body_inside, platform_inside));
                    }
                }
            }
            image.pixels.push(nominal);
            image.allowed.push(allowed);
            image.ambiguous_pixels += usize::from(allowed.count_ones() > 1);
        }
    }
    assert!(
        image.ambiguous_pixels <= MAX_AMBIGUOUS_PIXELS,
        "{} geometric edge pixels exceed the {MAX_AMBIGUOUS_PIXELS}-pixel ambiguity cap",
        image.ambiguous_pixels,
    );
    image
}

/// Checks every RGBA byte, reporting how many bounded edge alternatives were actually used.
pub fn assert_matches(expected: &ExpectedImage, actual: &[[u8; 4]]) -> ImageAgreement {
    assert_eq!(actual.len(), (WIDTH * HEIGHT) as usize, "full image extent");
    assert_eq!(expected.pixels.len(), actual.len());
    assert!(expected.ambiguous_pixels <= MAX_AMBIGUOUS_PIXELS);
    let mut alternate_pixels = 0;
    for (index, (observed, nominal)) in actual.iter().zip(&expected.pixels).enumerate() {
        if observed == nominal {
            continue;
        }
        let bit = palette_bit(*observed);
        assert!(
            bit != 0 && expected.allowed[index] & bit != 0,
            "pixel ({}, {}): actual {observed:?}, expected {nominal:?}, edge palette mask {}",
            index % WIDTH as usize,
            index / WIDTH as usize,
            expected.allowed[index],
        );
        alternate_pixels += 1;
    }
    assert!(alternate_pixels <= expected.ambiguous_pixels);
    ImageAgreement {
        ambiguous_pixels: expected.ambiguous_pixels,
        alternate_pixels,
    }
}

pub fn disjoint_pixels(left: &ExpectedImage, right: &ExpectedImage) -> usize {
    assert_eq!(left.allowed.len(), right.allowed.len());
    left.allowed
        .iter()
        .zip(&right.allowed)
        .filter(|(left, right)| **left & **right == 0)
        .count()
}

/// Validates observed matrices and native corners against independently reconstructed geometry.
pub fn assert_spatial(sample: &Sample) {
    let time = sample
        .input
        .packet()
        .captured_time()
        .expect("the original captured Runtime time");
    assert_eq!(time.real().frame, sample.frame);
    assert_eq!(time.fixed().tick(), sample.tick);
    let previous = sample
        .fixed
        .as_ref()
        .filter(|fixed| fixed.body_entity == sample.body_entity)
        .map(|fixed| fixed.body_before);
    let translation = presented_translation(
        previous,
        sample.body,
        f64::from(sample.render_time.interpolation_alpha),
        sample.paused,
    );
    assert_frame(
        sample.platform_presented,
        frame(&[sample.authored.ancestor, sample.authored.platform]),
        "platform Presented",
    );
    assert_frame(
        sample.body_presented,
        translated_frame(translation),
        "body Presented",
    );
    assert_eq!(
        sample.native_counts,
        (2, 2),
        "exact native body/collider inventory"
    );
    let Some(fixed) = sample
        .fixed
        .as_ref()
        .filter(|fixed| fixed.body_entity == sample.body_entity)
    else {
        assert!(
            sample
                .debug
                .shapes
                .iter()
                .all(|shape| shape.entity != sample.body_entity),
            "a current native body requires its original fixed observation",
        );
        return;
    };
    assert_eq!(
        sample.body, fixed.body_after,
        "native writeback reaches the current local pose"
    );
    assert_eq!(fixed.input_stamp.phase, SpatialSnapshotPhase::FixedInput);
    assert_eq!(fixed.result_stamp.phase, SpatialSnapshotPhase::Fixed);
    assert_eq!(fixed.input_stamp.world, fixed.result_stamp.world);
    assert_eq!(fixed.result_stamp.world, time.spatial_stamp().world);
    assert_eq!(
        fixed.result_stamp.runtime_generation,
        time.spatial_stamp().runtime_generation
    );
    assert_eq!(
        fixed.input_stamp.runtime_generation,
        fixed.result_stamp.runtime_generation
    );
    assert_eq!(fixed.input_stamp.tick, fixed.result_stamp.tick);
    assert_eq!(fixed.input_stamp.frame, fixed.result_stamp.frame);
    assert_eq!(fixed.result_stamp.tick, sample.tick);
    assert!(fixed.input_stamp.publication < fixed.result_stamp.publication);
    assert_frame(
        fixed.body_global,
        frame(&[fixed.body_after]),
        "body completed Fixed",
    );
    assert_eq!(fixed.debug.input_stamp, Some(fixed.input_stamp));
    assert!(
        fixed.debug.solved,
        "native corners must describe a solved fixed input"
    );
    assert_eq!(
        sample.debug.as_ref(),
        fixed.debug.as_ref(),
        "current original debug capture"
    );
    assert_debug(sample, &fixed.debug);
}

/// Requires an actual solver-reported contact with the intended platform as well as rest height.
pub fn assert_resting(sample: &Sample) {
    assert!(
        sample.contact,
        "the intended platform/body pair must have a native contact"
    );
    let fixed = sample
        .fixed
        .as_ref()
        .expect("resting requires a solved fixed observation");
    assert_eq!(
        fixed.body_entity, sample.body_entity,
        "contact belongs to this source incarnation"
    );
    assert_eq!(fixed.body_after, sample.body);
    let expected = resting_height(&sample.authored);
    let actual = f64::from(sample.body.translation.y);
    assert!(
        (actual - expected).abs() < 0.025,
        "root rest height {actual} differs from authored expectation {expected}",
    );
}

fn assert_rigid_root(pose: Transform2d) {
    assert_eq!(pose.rotation, 0.0, "this drop fixture locks body rotation");
    assert_eq!(pose.scale, Vec2::ONE, "dynamic root has unit scale");
    assert!(pose.translation.is_finite());
}

fn presented_translation(
    previous: Option<Transform2d>,
    current: Transform2d,
    alpha: f64,
    paused: bool,
) -> Point {
    assert!(alpha.is_finite() && (0.0..=1.0).contains(&alpha));
    assert_rigid_root(current);
    let current = [
        f64::from(current.translation.x),
        f64::from(current.translation.y),
    ];
    let Some(previous) = previous.filter(|_| !paused) else {
        return current;
    };
    assert_rigid_root(previous);
    let previous = [
        f64::from(previous.translation.x),
        f64::from(previous.translation.y),
    ];
    [
        previous[0] + (current[0] - previous[0]) * alpha,
        previous[1] + (current[1] - previous[1]) * alpha,
    ]
}

fn frame(chain: &[Transform2d]) -> [Point; 3] {
    [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]].map(|local| point(local, chain))
}

fn translated_frame(translation: Point) -> [Point; 3] {
    [
        translation,
        [translation[0] + 1.0, translation[1]],
        [translation[0], translation[1] + 1.0],
    ]
}

fn assert_frame(observed: Mat3, expected: [Point; 3], label: &str) {
    let [origin, x, y] = expected;
    let expected = [
        x[0] - origin[0],
        x[1] - origin[1],
        0.0,
        y[0] - origin[0],
        y[1] - origin[1],
        0.0,
        origin[0],
        origin[1],
        1.0,
    ];
    let observed = observed.to_cols_array();
    for (index, (actual, expected)) in observed.into_iter().zip(expected).enumerate() {
        assert!(
            near(f64::from(actual), expected),
            "{label} matrix element {index}: actual {actual}, expected {expected}",
        );
    }
    assert_eq!(
        [observed[2], observed[5], observed[8]],
        [0.0, 0.0, 1.0],
        "{label} affine row"
    );
}

fn assert_debug(sample: &Sample, debug: &DebugSnapshot) {
    assert_eq!(debug.shapes.len(), 2, "exact native debug membership");
    for (entity, expected) in [
        (
            sample.platform_entity,
            corners(
                sample.authored.platform_box,
                &[sample.authored.ancestor, sample.authored.platform],
            ),
        ),
        (
            sample.body_entity,
            corners(sample.authored.body_box, &[sample.body]),
        ),
    ] {
        let mut shapes = debug.shapes.iter().filter(|shape| shape.entity == entity);
        let shape = shapes
            .next()
            .expect("native debug must contain the original entity");
        assert!(
            shapes.next().is_none(),
            "one native shape per original entity"
        );
        assert_eq!(shape.origin.revision, sample.revision);
        let mut consumed = [false; 4];
        for expected in expected {
            let index = shape
                .vertices
                .iter()
                .enumerate()
                .position(|(index, actual)| {
                    !consumed[index]
                        && near(f64::from(actual.x), expected[0])
                        && near(f64::from(actual.y), expected[1])
                })
                .unwrap_or_else(|| {
                    panic!(
                        "native {entity:?} corners {:?} omit {expected:?}",
                        shape.vertices
                    )
                });
            consumed[index] = true;
        }
        assert!(consumed.into_iter().all(|used| used));
    }
}

fn near(actual: f64, expected: f64) -> bool {
    actual.is_finite()
        && expected.is_finite()
        && (actual - expected).abs() <= 0.000_1 + 0.000_01 * expected.abs()
}

fn project(world: Point) -> Point {
    let pixels_per_unit = f64::from(HEIGHT) / CAMERA_HEIGHT;
    [
        f64::from(WIDTH) * 0.5 + (world[0] - CAMERA_CENTER[0]) * pixels_per_unit,
        f64::from(HEIGHT) * 0.5 - (world[1] - CAMERA_CENTER[1]) * pixels_per_unit,
    ]
}

fn color(body: bool, platform: bool) -> [u8; 4] {
    if body {
        BODY_COLOR
    } else if platform {
        PLATFORM_COLOR
    } else {
        CLEAR_COLOR
    }
}

fn palette_bit(pixel: [u8; 4]) -> u8 {
    match pixel {
        BODY_COLOR => BODY_BIT,
        PLATFORM_COLOR => PLATFORM_BIT,
        CLEAR_COLOR => CLEAR_BIT,
        _ => 0,
    }
}

fn coverage(point: Point, corners: &Corners) -> (bool, bool) {
    let area = (0..4)
        .map(|index| {
            let a = corners[index];
            let b = corners[(index + 1) % 4];
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>();
    assert!(
        area.is_finite() && area.abs() > f64::EPSILON,
        "nondegenerate projected box"
    );
    let mut inside = true;
    let mut edge = false;
    for index in 0..4 {
        let a = corners[index];
        let b = corners[(index + 1) % 4];
        let direction = [b[0] - a[0], b[1] - a[1]];
        let offset = [point[0] - a[0], point[1] - a[1]];
        let side = direction[0] * offset[1] - direction[1] * offset[0];
        inside &= side * area >= 0.0;
        let length_squared = direction[0] * direction[0] + direction[1] * direction[1];
        assert!(length_squared.is_finite() && length_squared > 0.0);
        let along = ((offset[0] * direction[0] + offset[1] * direction[1]) / length_squared)
            .clamp(0.0, 1.0);
        let dx = offset[0] - along * direction[0];
        let dy = offset[1] - along * direction[1];
        edge |= dx * dx + dy * dy <= EDGE_DISTANCE_PIXELS * EDGE_DISTANCE_PIXELS;
    }
    (inside, edge)
}
