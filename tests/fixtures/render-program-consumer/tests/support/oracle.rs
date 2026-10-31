//! CPU equations, independent of native encoding and graph execution.

use super::model::{HEIGHT, WIDTH};

pub fn scene(capture: u32) -> Vec<[f32; 4]> {
    (0..HEIGHT)
        .flat_map(|y| {
            (0..WIDTH).map(move |x| {
                let alpha = if x == 0 {
                    0.0
                } else if y < HEIGHT / 4 {
                    0.5
                } else {
                    1.0
                };
                let grey = [0.1, 0.35, 0.6, 0.85][((x / 8 + y / 8 + capture) % 4) as usize] * alpha;
                [grey, grey, grey, alpha]
            })
        })
        .collect()
}

fn blur(input: &[[f32; 4]], horizontal: bool) -> Vec<[f32; 4]> {
    let mut result = Vec::with_capacity(input.len());
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut sum = [0.0; 4];
            for offset in -1..=1 {
                let sx = if horizontal {
                    (x as i32 + offset).clamp(0, WIDTH as i32 - 1) as u32
                } else {
                    x
                };
                let sy = if horizontal {
                    y
                } else {
                    (y as i32 + offset).clamp(0, HEIGHT as i32 - 1) as u32
                };
                for (total, value) in sum.iter_mut().zip(input[(sy * WIDTH + sx) as usize]) {
                    *total += value;
                }
            }
            result.push(sum.map(|value| value / 3.0));
        }
    }
    result
}

pub fn blurred(input: &[[f32; 4]]) -> Vec<[f32; 4]> {
    blur(&blur(input, true), false)
}

pub fn complement(input: &[[f32; 4]]) -> Vec<[f32; 4]> {
    input
        .iter()
        .map(|p| [p[3] - p[0], p[3] - p[1], p[3] - p[2], p[3]])
        .collect()
}

pub fn counts(input: &[[f32; 4]]) -> [u32; 4] {
    let mut counts = [0; 4];
    for p in input {
        let luminance = (p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722).clamp(0.0, 1.0);
        counts[((luminance * 4.0) as usize).min(3)] += 1;
    }
    counts
}

pub fn frequencies(counts: [u32; 4]) -> [f32; 4] {
    let total = counts.into_iter().sum::<u32>().max(1) as f32;
    counts.map(|value| value as f32 / total)
}

pub fn histogram(background: &[[f32; 4]], frequencies: [f32; 4]) -> Vec<[f32; 4]> {
    assert!(
        frequencies
            .iter()
            .all(|value| ((value * HEIGHT as f32).fract() - 0.5).abs() > 1.0e-6),
        "choose scene captures whose histogram edges do not coincide with pixel centers"
    );
    background
        .iter()
        .enumerate()
        .map(|(index, pixel)| {
            let x = index as u32 % WIDTH;
            let y = index as u32 / WIDTH;
            let bin = (x * 4 / WIDTH) as usize;
            // Rasterization samples pixel centers; no chosen scene places an edge at a center.
            let from_bottom = HEIGHT as f32 - y as f32 - 0.5;
            if from_bottom < frequencies[bin] * HEIGHT as f32 {
                [1.0; 4]
            } else {
                *pixel
            }
        })
        .collect()
}

pub fn assert_pixels(actual: &[[f32; 4]], expected: &[[f32; 4]]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for (channel, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                actual.is_finite() && (*actual - *expected).abs() <= 1.0e-6,
                "pixel {index}, channel {channel}: actual={actual}, expected={expected}"
            );
        }
    }
}
