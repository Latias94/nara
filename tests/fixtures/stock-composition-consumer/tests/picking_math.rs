#![forbid(unsafe_code)]

#[path = "../examples/picking/effect.rs"]
mod effect;
#[path = "../examples/picking/oracle.rs"]
mod oracle;
#[path = "../examples/picking/scene.rs"]
mod scene;

#[test]
fn independent_parented_camera_reference_preserves_mirror_and_shear() {
    let state = scene::SceneState::default();
    let point = oracle::canvas_to_world(state, [44.0, 8.0]);
    assert!((point[0] + 11.0).abs() <= 1.0e-5);
    assert!((point[1] + 6.0).abs() <= 1.0e-5);
    let restored = oracle::world_to_canvas(state, [-11.0, -6.0]);
    assert!((restored[0] - 44.0).abs() <= 1.0e-3);
    assert!((restored[1] - 8.0).abs() <= 1.0e-3);
    for mirror in [
        scene::CameraMirror::None,
        scene::CameraMirror::X,
        scene::CameraMirror::Y,
        scene::CameraMirror::Both,
    ] {
        let state = scene::SceneState {
            mirror,
            revision: scene::Revision::A,
        };
        for point in oracle::CANVAS_PROBES {
            let restored = oracle::world_to_canvas(state, oracle::canvas_to_world(state, point));
            assert!((restored[0] - point[0]).abs() <= 1.0e-3);
            assert!((restored[1] - point[1]).abs() <= 1.0e-3);
        }
    }
}

#[test]
fn pixel_reference_rejects_one_byte_alpha_and_clear_corruption() {
    use nara_stock_composition_consumer::oracle::Image;
    let expected = Image {
        width: 2,
        height: 1,
        format: wgpu::TextureFormat::Rgba8Unorm,
        pixels: vec![[255, 0, 0, 255], [0, 0, 0, 255]],
    };
    oracle::assert_pixels(&expected, &expected);
    for (pixel, channel) in [(0, 3), (1, 0)] {
        let mut damaged = expected.clone();
        damaged.pixels[pixel][channel] ^= 1;
        assert!(std::panic::catch_unwind(|| oracle::assert_pixels(&expected, &damaged)).is_err());
    }
}
