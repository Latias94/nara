use nara_ui_kernel::UiColor;

fn assert_components_close(color: UiColor, expected: [f64; 4]) {
    for (index, (actual, expected)) in color.components().into_iter().zip(expected).enumerate() {
        assert!(
            (f64::from(actual) - expected).abs() <= 2.0e-7,
            "channel {index}: {actual} != {expected}",
        );
    }
}

#[test]
fn rgba_preserves_linear_rgb_and_straight_alpha() {
    const COLOR: UiColor = UiColor::rgba(0.25, 0.5, 0.75, 0.5);
    const COMPONENTS: [f32; 4] = COLOR.components();

    assert_eq!(COMPONENTS, [0.25, 0.5, 0.75, 0.5]);
    assert_eq!(
        UiColor::rgba(0.25, 0.5, 0.75, 0.0).components(),
        [0.25, 0.5, 0.75, 0.0],
    );
}

#[test]
fn srgba8_decodes_endpoints_both_transfer_branches_and_mid_gray() {
    for (encoded, linear) in [
        (0, 0.0),
        (10, 0.003_035_269_835_488_375),
        (11, 0.003_346_535_763_899_158_5),
        (128, 0.215_860_500_113_899_16),
        (255, 1.0),
    ] {
        assert_components_close(
            UiColor::from_srgba8(encoded, encoded, encoded, 255),
            [linear, linear, linear, 1.0],
        );
    }
}

#[test]
fn srgba8_preserves_non_gray_rgb_at_zero_partial_and_full_alpha() {
    for (encoded_alpha, alpha) in [(0, 0.0), (128, 0.501_960_784_313_725_5), (255, 1.0)] {
        assert_components_close(
            UiColor::from_srgba8(32, 128, 224, encoded_alpha),
            [
                0.014_443_843_596_092_54,
                0.215_860_500_113_899_16,
                0.745_404_209_540_387_2,
                alpha,
            ],
        );
    }
}

#[test]
fn srgb8_decodes_non_gray_rgb_with_opaque_alpha() {
    let color = UiColor::from_srgb8(32, 128, 224);

    assert_components_close(
        color,
        [
            0.014_443_843_596_092_54,
            0.215_860_500_113_899_16,
            0.745_404_209_540_387_2,
            1.0,
        ],
    );
    assert_eq!(color, UiColor::from_srgba8(32, 128, 224, 255));
}
