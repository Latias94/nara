use nara_stock_composition_consumer::oracle::Image;

use super::{
    effect::Mode,
    scene::{CameraMirror, RetainedCapture, Revision, SceneState},
};

pub const CANVAS_PROBES: [[f64; 2]; 3] = [[12.5, 10.5], [32.5, 24.5], [53.5, 37.5]];

pub fn assert_pixels(expected: &Image, actual: &Image) {
    assert_eq!(
        (actual.width, actual.height, actual.format),
        (expected.width, expected.height, expected.format)
    );
    assert_eq!(actual.pixels.len(), expected.pixels.len());
    for (index, (expected, actual)) in expected.pixels.iter().zip(&actual.pixels).enumerate() {
        assert_eq!(actual[3], expected[3], "exact alpha at pixel {index}");
        if matches!(
            *expected,
            [0, 0, 0, 255] | [255, 255, 0, 255] | [0, 255, 255, 255]
        ) {
            assert_eq!(actual, expected, "exact bars/clear at pixel {index}");
        } else {
            for channel in 0..3 {
                assert!(
                    actual[channel].abs_diff(expected[channel]) <= 1,
                    "RGB pixel {index} channel {channel}: {actual:?} != {expected:?}"
                );
            }
        }
    }
}

pub fn expected(record: &RetainedCapture, format: wgpu::TextureFormat) -> Image {
    let resolution = record.window.resolution;
    Image::from_linear_pixels(
        resolution.physical_width,
        resolution.physical_height,
        format,
        expected_pixels(record)
            .into_iter()
            .map(|pixel| pixel.map(|channel| channel as f32)),
    )
}

pub fn expected_pixels(record: &RetainedCapture) -> Vec<[f64; 4]> {
    let resolution = record.window.resolution;
    let width = resolution.physical_width;
    let height = resolution.physical_height;
    assert!(width > 0 && height > 0 && width <= 2048 && height <= 2048);
    let (origin, scale) = canvas_fit(record);
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let final_x = f64::from(x) + 0.5;
            let source = [
                match record.mode {
                    Mode::MirrorMapped | Mode::MirrorUnmapped => f64::from(width) - final_x,
                    Mode::Identity | Mode::Tint => final_x,
                },
                f64::from(y) + 0.5,
            ];
            let canvas = [
                (source[0] - origin[0]) / scale,
                (source[1] - origin[1]) / scale,
            ];
            let mut color = if in_rect(canvas, [0.0, 0.0], [64.0, 48.0]) {
                canvas_pixel(
                    record.state,
                    [canvas[0].floor() + 0.5, canvas[1].floor() + 0.5],
                )
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            if record.mode == Mode::Tint {
                color.swap(0, 2);
            }
            pixels.push(color);
        }
    }
    pixels
}

pub fn canvas_to_world(state: SceneState, canvas: [f64; 2]) -> [f64; 2] {
    let (basis, translation) = camera_pose(state);
    let camera_x = (canvas[0] - 32.0) * 0.25;
    let camera_y = (24.0 - canvas[1]) * 0.25;
    [
        translation[0] + basis[0][0] * camera_x + basis[0][1] * camera_y,
        translation[1] + basis[1][0] * camera_x + basis[1][1] * camera_y,
    ]
}

pub fn world_to_canvas(state: SceneState, world: [f64; 2]) -> [f64; 2] {
    let (basis, translation) = camera_pose(state);
    let x = world[0] - translation[0];
    let y = world[1] - translation[1];
    let determinant = basis[0][0] * basis[1][1] - basis[0][1] * basis[1][0];
    let camera_x = (basis[1][1] * x - basis[0][1] * y) / determinant;
    let camera_y = (basis[0][0] * y - basis[1][0] * x) / determinant;
    [32.0 + camera_x * 4.0, 24.0 - camera_y * 4.0]
}

pub fn world_at_logical(record: &RetainedCapture, logical: [f64; 2]) -> Option<[f64; 2]> {
    if record.mode == Mode::MirrorUnmapped || !logical.into_iter().all(f64::is_finite) {
        return None;
    }
    let resolution = record.window.resolution;
    let density = resolution.scale_factor;
    if !density.is_finite() || density <= 0.0 {
        return None;
    }
    let mut physical = logical.map(|coordinate| coordinate * density);
    let extent = [
        f64::from(resolution.physical_width),
        f64::from(resolution.physical_height),
    ];
    if !in_rect(physical, [0.0, 0.0], extent) {
        return None;
    }
    if record.mode == Mode::MirrorMapped {
        physical[0] = extent[0] - physical[0];
    }
    if !in_rect(physical, [0.0, 0.0], extent) {
        return None;
    }
    let (origin, scale) = canvas_fit(record);
    let canvas = [
        (physical[0] - origin[0]) / scale,
        (physical[1] - origin[1]) / scale,
    ];
    if !in_rect(canvas, [4.0, 4.0], [60.0, 44.0]) {
        return None;
    }
    Some(canvas_to_world(record.state, canvas))
}

pub fn presented_point(record: &RetainedCapture, canvas: [f64; 2]) -> Option<[f64; 2]> {
    if !canvas.into_iter().all(f64::is_finite) || !in_rect(canvas, [0.0, 0.0], [64.0, 48.0]) {
        return None;
    }
    let resolution = record.window.resolution;
    let density = resolution.scale_factor;
    if !density.is_finite() || density <= 0.0 {
        return None;
    }
    let (origin, scale) = canvas_fit(record);
    let mut physical = [origin[0] + canvas[0] * scale, origin[1] + canvas[1] * scale];
    if matches!(record.mode, Mode::MirrorMapped | Mode::MirrorUnmapped) {
        physical[0] = f64::from(resolution.physical_width) - physical[0];
    }
    let extent = [
        f64::from(resolution.physical_width),
        f64::from(resolution.physical_height),
    ];
    in_rect(physical, [0.0, 0.0], extent).then(|| physical.map(|coordinate| coordinate / density))
}

fn canvas_pixel(state: SceneState, canvas: [f64; 2]) -> [f64; 4] {
    if in_rect(canvas, [4.0, 4.0], [60.0, 44.0]) {
        let world = canvas_to_world(state, canvas);
        for (center, size, color) in [
            ([-11.13, -6.27], [5.82, 4.26], [1.0, 0.0, 0.0, 1.0]),
            ([8.31, 1.43], [7.18, 3.62], [0.0, 0.0, 1.0, 1.0]),
            ([-0.73, -11.61], [4.46, 5.18], [0.0, 1.0, 0.0, 1.0]),
        ] {
            let min = [center[0] - size[0] * 0.5, center[1] - size[1] * 0.5];
            let max = [center[0] + size[0] * 0.5, center[1] + size[1] * 0.5];
            if in_rect(world, min, max) {
                return color;
            }
        }
    }
    [1.0, 1.0, 0.0, 1.0]
}

fn camera_pose(state: SceneState) -> ([[f64; 2]; 2], [f64; 2]) {
    let (x_sign, y_sign) = match state.mirror {
        CameraMirror::None => (1.0, 1.0),
        CameraMirror::X => (-1.0, 1.0),
        CameraMirror::Y => (1.0, -1.0),
        CameraMirror::Both => (-1.0, -1.0),
    };
    match state.revision {
        Revision::A => (
            [
                [-1.8 * y_sign, -2.4 * y_sign],
                [1.6 * x_sign, -1.2 * x_sign],
            ],
            [10.0 - 6.0 * y_sign, -4.0 + 2.0 * x_sign],
        ),
        Revision::B => (
            [[2.4 * y_sign, -1.8 * y_sign], [1.2 * x_sign, 1.6 * x_sign]],
            [8.0 - 4.5 * y_sign, -1.0 - x_sign],
        ),
    }
}

fn canvas_fit(record: &RetainedCapture) -> ([f64; 2], f64) {
    let resolution = record.window.resolution;
    let width = f64::from(resolution.physical_width);
    let height = f64::from(resolution.physical_height);
    let fit = (width / 64.0).min(height / 48.0);
    let scale = if fit >= 1.0 { fit.floor() } else { fit };
    assert!(scale.is_finite() && scale > 0.0);
    let mut origin = [(width - 64.0 * scale) * 0.5, (height - 48.0 * scale) * 0.5];
    if fit >= 1.0 {
        origin = origin.map(f64::round);
    }
    (origin, scale)
}

fn in_rect(point: [f64; 2], min: [f64; 2], max: [f64; 2]) -> bool {
    (0..2).all(|axis| point[axis] >= min[axis] && point[axis] < max[axis])
}
