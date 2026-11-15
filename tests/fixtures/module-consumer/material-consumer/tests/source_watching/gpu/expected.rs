//! Independent CPU oracle for the source-watching palette fixture's linear RGBA8 output.
//!
//! Both one-texel source Images use the default sRGB import settings. The palette width is one,
//! offset is zero, strength is one, and the Sprite tint is white. The decoded base RGB therefore
//! cannot affect the selected texel or final RGB; its linear alpha still affects coverage.
//! The Material returns straight alpha, and stock source-over blending over transparent black
//! associates RGB once. The offscreen `Rgba8Unorm` target does not encode RGB back to sRGB.

pub(super) const WIDTH: u32 = 32;
pub(super) const HEIGHT: u32 = 24;

/// Fixed before GPU execution, matching the stock-composition consumer's three-byte allowance
/// for hardware sRGB conversion, UNORM rounding, and floating-point arithmetic.
pub(super) const TOLERANCE: u8 = 3;

/// Computes the center from the gain in the captured Material source and the saved, decoded
/// source pixels for the base indices Image and the Material's selected palette Image.
///
/// This models only the one-texel, full-strength fixture, with gain in `[0, 1]`. RGB is sRGB
/// decoded before gain and blending; alpha is normalized directly and never gamma decoded.
pub(super) fn expected_center(gain: f32, indices: [u8; 4], palette: [u8; 4]) -> [u8; 4] {
    assert!(
        (0.0..=1.0).contains(&gain),
        "the source-watching fixture requires a finite gain in [0, 1], got {gain}"
    );
    let alpha = (f32::from(indices[3]) / 255.0) * (f32::from(palette[3]) / 255.0);
    [
        unorm(srgb_decode(palette[0]) * gain * alpha),
        unorm(srgb_decode(palette[1]) * gain * alpha),
        unorm(srgb_decode(palette[2]) * gain * alpha),
        unorm(alpha),
    ]
}

/// Checks the covered center and exact transparent corners of the fixed 32 by 24 viewport.
/// The centered eight-unit Sprite must cover the center under the fixture's Camera mapping.
pub(super) fn assert_image(actual: &[[u8; 4]], expected: [u8; 4], label: &str) {
    assert_eq!(
        actual.len(),
        (WIDTH * HEIGHT) as usize,
        "{label}: the readback must contain exactly {WIDTH} by {HEIGHT} RGBA pixels"
    );
    assert!(
        expected[3] > TOLERANCE && expected[..3].iter().any(|channel| *channel > TOLERANCE),
        "{label}: the expected center must distinguish the Sprite from a clear: {expected:?}"
    );
    let center = actual[((HEIGHT / 2) * WIDTH + WIDTH / 2) as usize];
    for channel in 0..4 {
        assert!(
            center[channel].abs_diff(expected[channel]) <= TOLERANCE,
            "{label}: center channel {channel}: actual={center:?}, expected={expected:?}, tolerance={TOLERANCE}"
        );
    }
    for (x, y) in [
        (0, 0),
        (WIDTH - 1, 0),
        (0, HEIGHT - 1),
        (WIDTH - 1, HEIGHT - 1),
    ] {
        assert_eq!(
            actual[(y * WIDTH + x) as usize],
            [0; 4],
            "{label}: corner ({x}, {y}) must remain exact transparent black"
        );
    }
}

fn srgb_decode(encoded: u8) -> f32 {
    let value = f32::from(encoded) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn unorm(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::{HEIGHT, TOLERANCE, WIDTH, assert_image, expected_center, unorm};

    const INDICES: [u8; 4] = [24, 120, 220, 255];
    const UPDATED_PALETTE: [u8; 4] = [17, 61, 139, 255];
    const GAINS: [f32; 5] = [1.0, 0.75, 0.5, 0.25, 0.125];

    fn rgb_difference(left: [u8; 4], right: [u8; 4]) -> u8 {
        (0..3)
            .map(|channel| left[channel].abs_diff(right[channel]))
            .max()
            .unwrap()
    }

    #[test]
    fn source_pixels_distinguish_every_gain_and_incorrect_linear_sampling() {
        let original = [
            [2, 48, 183, 255],
            [2, 36, 137, 255],
            [1, 24, 91, 255],
            [1, 12, 46, 255],
            [0, 6, 23, 255],
        ];
        let updated = [
            [1, 12, 66, 255],
            [1, 9, 49, 255],
            [1, 6, 33, 255],
            [0, 3, 16, 255],
            [0, 1, 8, 255],
        ];
        for (palette, centers) in [(INDICES, original), (UPDATED_PALETTE, updated)] {
            for (index, gain) in GAINS.into_iter().enumerate() {
                let expected = expected_center(gain, INDICES, palette);
                assert_eq!(expected, centers[index]);
                let linear = [
                    unorm(f32::from(palette[0]) / 255.0 * gain),
                    unorm(f32::from(palette[1]) / 255.0 * gain),
                    unorm(f32::from(palette[2]) / 255.0 * gain),
                    255,
                ];
                for incorrect in centers[..index]
                    .iter()
                    .copied()
                    .chain([linear, [0, 0, 0, 255]])
                {
                    assert!(
                        rgb_difference(expected, incorrect) > 2 * TOLERANCE,
                        "palette={palette:?}, gain={gain}: tolerance bands must be disjoint for expected={expected:?}, incorrect={incorrect:?}"
                    );
                }
                assert!(
                    rgb_difference(original[index], updated[index]) > 2 * TOLERANCE,
                    "gain={gain}: stale palette selection must be observable"
                );
            }
        }
    }

    #[test]
    fn both_source_alphas_are_linear_and_rgb_is_associated_once() {
        assert_eq!(
            expected_center(0.5, [24, 120, 220, 128], [17, 61, 139, 128]),
            [0, 1, 8, 64]
        );
        assert_eq!(
            expected_center(0.5, [24, 120, 220, 0], UPDATED_PALETTE),
            [0; 4]
        );
        assert_eq!(expected_center(0.5, INDICES, [17, 61, 139, 0]), [0; 4]);
        assert_eq!(
            expected_center(0.5, [255, 0, 0, 255], UPDATED_PALETTE),
            expected_center(0.5, INDICES, UPDATED_PALETTE)
        );
    }

    #[test]
    fn image_assertion_rejects_stale_gain_clear_and_corner_leaks() {
        let expected = expected_center(0.125, INDICES, UPDATED_PALETTE);
        let center = ((HEIGHT / 2) * WIDTH + WIDTH / 2) as usize;
        let mut actual = vec![[0; 4]; (WIDTH * HEIGHT) as usize];
        actual[center] = expected;
        assert_image(&actual, expected, "correct probes");

        for incorrect in [
            [0; 4],
            expected_center(1.0, INDICES, UPDATED_PALETTE),
            expected_center(0.25, INDICES, UPDATED_PALETTE),
            [2, 8, 17, 255],
        ] {
            actual[center] = incorrect;
            assert!(
                std::panic::catch_unwind(|| assert_image(&actual, expected, "incorrect center"))
                    .is_err()
            );
        }
        actual[center] = expected;
        actual[0] = [1, 0, 0, 0];
        assert!(
            std::panic::catch_unwind(|| assert_image(&actual, expected, "corner leak")).is_err()
        );
    }
}
