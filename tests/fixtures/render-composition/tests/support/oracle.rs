//! Independent CPU equations for numeric GPU comparisons, in linear premultiplied color.

pub(super) fn scene(width: u32, height: u32, capture: u32) -> Vec<[f32; 4]> {
    let mut pixels = Vec::with_capacity((width as usize).checked_mul(height as usize).unwrap());
    for y in 0..height {
        for x in 0..width {
            let alpha = if x == 0 {
                0.0
            } else if y < height / 4 {
                0.5
            } else {
                1.0
            };
            let grey = [0.1, 0.35, 0.6, 0.85][((x / 8 + y / 8 + capture) % 4) as usize] * alpha;
            pixels.push([grey, grey, grey, alpha]);
        }
    }
    pixels
}

pub(super) fn blur(input: &[[f32; 4]], width: u32, height: u32, horizontal: bool) -> Vec<[f32; 4]> {
    assert_eq!(input.len(), width as usize * height as usize);
    let mut pixels = Vec::with_capacity(input.len());
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0; 4];
            for offset in -1..=1 {
                let source_x = if horizontal {
                    (i64::from(x) + offset).clamp(0, i64::from(width) - 1) as u32
                } else {
                    x
                };
                let source_y = if horizontal {
                    y
                } else {
                    (i64::from(y) + offset).clamp(0, i64::from(height) - 1) as u32
                };
                let source = input[(source_y * width + source_x) as usize];
                for (sum, component) in sum.iter_mut().zip(source) {
                    *sum += component;
                }
            }
            pixels.push(sum.map(|value| value / 3.0));
        }
    }
    pixels
}

pub(super) fn complement(input: &[[f32; 4]]) -> Vec<[f32; 4]> {
    input
        .iter()
        .map(|pixel| {
            [
                pixel[3] - pixel[0],
                pixel[3] - pixel[1],
                pixel[3] - pixel[2],
                pixel[3],
            ]
        })
        .collect()
}

pub(super) fn hud(input: &[[f32; 4]], width: u32) -> Vec<[f32; 4]> {
    input
        .iter()
        .enumerate()
        .map(|(index, pixel)| {
            let x = index % width as usize;
            let y = index / width as usize;
            if (2..10).contains(&x) && (2..6).contains(&y) {
                [1.0, 1.0, 0.0, 1.0]
            } else {
                *pixel
            }
        })
        .collect()
}

pub(super) fn luminance_counts(input: &[[f32; 4]]) -> [u32; 4] {
    let mut bins = [0; 4];
    for pixel in input {
        let luminance = pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722;
        let bin = (luminance.max(0.0) * 4.0) as usize;
        bins[bin.min(3)] += 1;
    }
    bins
}

pub(super) fn normalized(counts: [u32; 4]) -> [f32; 4] {
    let total = counts.into_iter().sum::<u32>();
    if total == 0 {
        [0.0; 4]
    } else {
        counts.map(|count| count as f32 / total as f32)
    }
}

pub(super) fn assert_pixels(actual: &[[f32; 4]], expected: &[[f32; 4]], tolerance: f32) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for (channel, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                actual.is_finite() && (*actual - *expected).abs() <= tolerance,
                "pixel {index} channel {channel}: actual={actual} expected={expected} tolerance={tolerance}"
            );
        }
    }
}
