//! Native effect controls with independent numeric oracles and distinct retained versions.

#![forbid(unsafe_code)]

#[path = "support/controls.rs"]
mod controls;
#[path = "support/effects.rs"]
mod effects;
#[path = "support/gpu.rs"]
mod gpu;
#[path = "support/oracle.rs"]
mod oracle;

#[test]
#[ignore = "real native GPU numeric comparison; explicitly select and run serially"]
fn ordered_slot_and_schedule_controls_preserve_original_statistics_and_crisp_hud() {
    let gpu = gpu::Gpu::new();
    let effects = effects::Effects::new(&gpu.device);
    let pixels = oracle::scene(controls::WIDTH, controls::HEIGHT, 0);
    let horizontal = oracle::blur(&pixels, controls::WIDTH, controls::HEIGHT, true);
    let blurred = oracle::blur(&horizontal, controls::WIDTH, controls::HEIGHT, false);
    let complement = oracle::complement(&blurred);
    let composite = oracle::hud(&complement, controls::WIDTH);
    let counts = oracle::luminance_counts(&pixels);
    let normalized = oracle::normalized(counts);
    let mut retained = Vec::new();
    for control in [
        controls::Control::Ordered,
        controls::Control::Slots,
        controls::Control::Schedule,
    ] {
        let output = controls::execute(&gpu, &effects, &pixels, control);
        for (texture, expected) in [
            (&output.original, &pixels),
            (&output.horizontal, &horizontal),
            (&output.blurred, &blurred),
            (&output.complement, &complement),
            (&output.composite, &composite),
        ] {
            oracle::assert_pixels(
                &gpu.read_rgba32(texture, controls::WIDTH, controls::HEIGHT),
                expected,
                1.0e-6,
            );
        }
        let raw = gpu.read_buffer(&output.raw);
        let actual_counts: [u32; 4] = std::array::from_fn(|index| {
            u32::from_le_bytes(raw[index * 4..index * 4 + 4].try_into().unwrap())
        });
        assert_eq!(actual_counts, counts, "control {control:?}");
        let normalized_bytes = gpu.read_buffer(&output.normalized);
        for (actual, expected) in normalized_bytes.chunks_exact(4).zip(normalized) {
            assert!((f32::from_le_bytes(actual.try_into().unwrap()) - expected).abs() <= 1.0e-6);
        }
        let histogram = gpu.read_rgba32(&output.histogram, controls::WIDTH, controls::HEIGHT);
        for y in 0..controls::HEIGHT {
            for x in 0..controls::WIDTH {
                let bin = (x * 4 / controls::WIDTH) as usize;
                let covered = y as f32 + 0.5 >= controls::HEIGHT as f32 * (1.0 - normalized[bin]);
                let expected = if covered { [1.0; 4] } else { [0.0; 4] };
                assert_eq!(
                    histogram[(y * controls::WIDTH + x) as usize],
                    expected,
                    "control {control:?} histogram ({x}, {y})"
                );
            }
        }
        retained.push(output);
    }
    let newer = oracle::scene(controls::WIDTH, controls::HEIGHT, 1);
    let _successor = controls::execute(&gpu, &effects, &newer, controls::Control::Ordered);
    // Newer execution uses distinct storage; every delayed consumer still observes original A.
    for output in retained {
        oracle::assert_pixels(
            &gpu.read_rgba32(&output.composite, controls::WIDTH, controls::HEIGHT),
            &composite,
            1.0e-6,
        );
        oracle::assert_pixels(
            &gpu.read_rgba32(&output.original, controls::WIDTH, controls::HEIGHT),
            &pixels,
            0.0,
        );
    }
}
